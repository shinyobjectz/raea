# Fingerprint: the plan

Shane, 2026-11-12: "build a framework within raea called Fingerprint ... a taxonomy data structure and an embedding
model that can run on a personal device, that could be easily trained on device, to best understand if a video (its
audio and video and whatever else we can put together into this) is good or not ... train through comments or notes
or a post-mortem process the agent can run ... and if you have a team, you can distribute those embedding models."

This is the one plan for Fingerprint: what it is, the stack, how it trains, how people and agents use it, the
measurements behind each part, and the build order with a confidence for each step. The rask tasks are in
[fingerprint.feature](fingerprint.feature) (FP0-FP13). The reference design is
[earshot](https://github.com/Leonxlnx/earshot) (MIT), a local sound critic for agents. It is the closest working
system to what Fingerprint does for sound. We take its methods, not its code (see "earshot, the reference").

## What a fingerprint is

A fingerprint is not a fine-tuned embedding model. It is a **taxonomy of axes**: the things that can be good or wrong
about a piece, such as a soft picture, dead motion, noise under the voice or a cut that drags. Each axis has the
**words** people use for it and **one small head** over **frozen, pinned encoders that run on the person's own
device**. On top of the axes sit the person's **weighting** across them, an optional **profile** (ranges taken from
work they admire) and the **evidence ledger** that every head was trained from.

On day one, an axis's words already give it a zero-shot score. Each label the person adds retrains its head in
milliseconds. Whatever can be measured exactly, such as loudness, silence gaps, frozen frames or one thing passing
through another, is a check rather than a learned head. A team shares evidence, never media, and each machine
retrains from it.

The hard part is turning notes into labels, not the model. On Shane's real notes, picture and sound alone can't
predict where he will leave a note. They do learn a named axis from 4 to 32 examples, and some axes from no examples
at all.

## The stack

```
media ─► columns (once, at ingest, cached by content hash)
           picture: SigLIP 2 per frame at 4 fps (pooled vector; 14x14 patch grid for flagged or marked frames)
           change:  frame-to-frame SigLIP distance          look: luminance histogram, saturation, contrast, sharpness
           sound:   CLAP audio, 2 s windows, 1 s hop          YAMNet, 0.48 s frames (event classes and timing)
           other:   EmbeddingGemma 2 for scripts, briefs, boards, stills (the Library's rows)
           things:  EdgeTAM masks, Depth Anything 3 depth and camera (footage only, when an axis needs them)
       ─► checks (no model): loudness, silence gaps, clipping, flash, freeze, sync to cut, floating, passing through
       ─► axis heads: word head (zero-shot) → learned head (logistic / Bradley-Terry) → patch head (heatmap)
       ─► person's weighting over axis scores          ─► profile ranges from reference work
       ─► summary · JSON · sheet PNG · timeline · watch lane
```

| part | model | size on disk | runs at score time | cost for a 90 s render |
|---|---|---|---|---|
| picture columns | SigLIP 2 base p16-224 (image tower) | 95 MB int8, 372 MB fp32 | yes | 17 s on ort's CPU path (47 ms a frame); MLX on the Mac once MX5 lands; never Core ML's GPU |
| sound columns | CLAP htsat-unfused (audio tower) | 34 MB q8, 117 MB fp32 | yes | about 3 s (33 ms a 2 s clip) |
| sound events | YAMNet | 16 MB | yes | under 1 s |
| change and look | none | none | yes | negligible |
| words, scripts, boards, stills | EmbeddingGemma 2 | 841 MB q8, already the Library's | only for non-video items | 2.3 s a window; reads Library rows instead where they exist |
| axis words → vectors | CLAP text tower (127 MB q8), SigLIP 2 text tower | | **no**: only when an axis's words change; the vectors are stored in the fingerprint | |
| heads, weighting, profile | ours | 7 KB a head | yes | milliseconds |
| things (footage only) | EdgeTAM, Depth Anything 3 Small | 37 MB, 51 MB (both already in perception) | only for thing-level axes and checks | about 86 ms a frame for EdgeTAM |

Only CLAP is a new download. Its q8 audio and text towers come to 161 MB. Every encoder runs in Rust:
- **ONNX Runtime** (ort, through raea-sys-ml) is the path on every platform and the reference. YAMNet already runs on
  it, CLAP's audio and text towers join it, and CLAP's tokenizer is the `tokenizers` crate raea-sys-speech already
  uses.
- **MLX** (raea-sys-mlx) is for SigLIP 2 on the Mac, the one heavy column. On this Mac, ort's CPU path costs 47 ms a
  frame and Core ML's GPU computes SigLIP wrong. SigLIP is already a named candidate for mlx.feature MX5, and it moves
  there once its MLX path matches ONNX fp32.
- **CLAP stays on ort.** At 33 ms a clip on the CPU there's nothing to gain from rewriting its HTSAT network in MLX.

transformers.js (onnxruntime-node underneath, the same .onnx files) was used only to measure. Everything runs inside
the feed host, which stays alive, so the models load once.
earshot's CLI reloads its models on every call (20-40 s) and has to steer agents to its MCP server for that reason.
`raea fingerprint` talks to the running host instead. Columns are cached by content hash and step version, which is
safer than earshot's cache key of path, size, modification time and sampled blocks.

**Used only for labelling and training, never at score time:** the Director reading notes into labels, EdgeTAM
growing a click into a region, the open-vocabulary grounder (gated, FP13b), the damage and red-team generators, the
text towers, and the gated LoRA trainer (FP10).

### Axes and their heads

An axis is declared in TypeScript beside `taste` (`fingerprint({...})` from "raea/brief"). It has:
- a name and the words people use for it;
- a modality: picture, change, look, sound or words;
- a scope: moment, shot or the whole piece;
- a polarity;
- optionally, the check it maps to.

raea ships a base taxonomy drawn from Shane's notes: glitch and continuity, sound, pace, camera, cut, light and look,
realism and detail, interaction, design and type, story. A person or team adds axes. Creative directions ("could be a
garden door") are not axes. They go to the brief and the agent, never into a head.

Each axis uses the best head it has earned:
1. **A word head** (from day one): the similarity between the window and the axis's words, as a z-score against a
   background vocabulary of about 40 ordinary descriptions. This is earshot's calibration. CLAP serves sound axes and
   SigLIP 2's text tower serves picture axes. A word head is always advice: it never gates.
2. **A learned head**: logistic on the axis's own modality columns, standardised with the encoder's shipped
   statistics, plus Bradley-Terry when the axis has pairs. It takes over from the word head once it beats it on
   held-out evidence.
3. **A patch head** (picture axes): logistic on SigLIP 2's patch tokens, trained from marks. It says *where* in the
   frame.

An axis that a check measures exactly, such as dropout or loudness, reports the check, and its head only adds what
the check can't see.

## How it trains

1. **Day 0.** The base taxonomy's words give every axis an advisory score. `raea fingerprint bootstrap` damages
   windows of the person's own approved work (blur, dark, glow, flash, freeze, noise, dropout, clipping) and trains
   the mechanical axes from those labels by construction (FP0b). No placeholder footage is used.
2. **Every label is an evidence row**: the media's content hash and span, the region or layer, axis, polarity,
   weight, who gave it, its source, and the window's columns (fp16, about 4 KB). The heads retrain after each row,
   in milliseconds.
3. **Sources, strongest first:**
   1. a mark or box on a defect note (FP11);
   2. a pinned defect note, read by the Director into {kind, axis, span, layer};
   3. an approval, a picture lock, a publish or a golden reference: positives over every un-noted span;
   4. a pick between real options, or a hand edit's before and after: pairs;
   5. reference work the person admires, as a profile (FP6e);
   6. version pairs (a noted moment that changed and wasn't noted again): weak pairs;
   7. damage variants (bootstrap);
   8. the post-mortem's answers;
   9. clef's or Kimi's readings, kept as another eye's labels and never counted as the person's.
4. **The gate.** A head is advice until it has 16 labels of each polarity from 3 or more projects, a held-out by
   project AUC of 0.8 or better, and a catch rate of 0.8 or better on its red-team cheats. A gated head reports
   bands, as earshot's verdicts do: strong, plausible or weak, each with the precision measured on that head's own
   held-out evidence.
5. **The person's weighting** is a logistic fit over the axis scores, from their own labels (FP0c).
6. **The post-mortem.** At a project's end, the agent scores every version, finds where the fingerprint is least sure
   or disagrees most with the person's history, and asks at most ten yes/no questions with the moment playing
   (`raea show --question`). Each answer is an evidence row.
7. **Encoder upgrades.** A row whose media is present is re-embedded and its head retrained. A row without media keeps
   the old pin until it's labelled again.
8. **Fine-tuning the encoder** is gated. It is tried only for an axis still under its bar with 300 labels or more after
   its own columns were added (glow is the likely first), as a LoRA adapter on SigLIP 2 trained on the device through
   MLX. It is kept only if it raises held-out AUC by 0.05.

## The CLI

```
raea fingerprint new NAME [--team PROJECT]        a fingerprint from the base taxonomy (~/.raea/fingerprints/NAME or the project's)
raea fingerprint score FILE [--fp NAME]           summary first: the moments it expects a note on, each axis in the person's words
       [--json] [--sheet out.png] [--timeline]    full scores; an annotated sheet; a second-by-second reading
raea fingerprint teach FILE --axis A --at T       a label from the CLI or an agent: --bad/--good, --span, --box x0,y0,x1,y1
raea fingerprint bootstrap [--from DIR]           damage variants of approved work train the mechanical axes
raea fingerprint profile REFS... --as NAME        reference ranges from work the person admires, checked leave-one-out
raea fingerprint report                           each head: labels by source, held-out score, cheats caught, gate state
raea fingerprint postmortem PROJECT               the ten questions, then the report's change
raea fingerprint diff A B                         the axes two fingerprints disagree on most, with the moments
```

The sheet is one PNG an agent or a person reads at a glance. It holds frames with the patch heatmaps over them, the
axis lanes, the spectrogram with flagged sounds boxed, and the checks' findings. In the app, the same scores are a lane
on the watch timeline. Agents get the axis, the moment and the person's words, never a number or a threshold.

## Teams

A fingerprint is a folder with these files:
- `fingerprint.ts` (axes, words, encoder pins);
- `words.f16` (the axis words' vectors);
- `heads.safetensors`;
- `evidence.jsonl` with `evidence.f16`;
- `profiles/`;
- `report.json`.

It travels with the project (Git + LFS, or raea's API) under the project's permissions. Teammates share evidence rows
and each machine retrains: pooling beat averaging the heads (FP0c). Each person's weighting stays their own.
Embeddings partly reveal content, so evidence never leaves the project's permissions, and media never travels at all.

## Agents, and keeping them honest

The feed's agent runs `raea fingerprint score` on a cut before posting it, and either fixes each gating flag or says
why not. A fix is checked against the flag it claims to fix, and the axis's cheats are rerun.

earshot found the risk the hard way. Once a sound-making search optimised against its learned critics, those critics
stopped telling made sounds from real ones, while its model-free measurements still could. Our own cheats agree:
glow at half strength is caught only 0.20. So:
- measured checks are the arbiter wherever a thing can be measured;
- every learned axis carries red-team cheats;
- agents never see scores;
- like earshot's LIMITS.md, `report.json` says what each head can't judge.

## earshot, the reference

earshot is a CPU-only Python toolkit that gives agents ears. It runs CLAP (laion/larger_clap_general, Apache-2.0) and
AST (BSD-3) over a film's sound and returns a timeline, sync checks, a critique against reference films and a
realness score. It's honest about its measured limits. It covers sound only, and its runtime is Python and PyTorch,
which raea doesn't ship. We take six ideas:

| earshot | Fingerprint | spec |
|---|---|---|
| text-audio match calibrated as a z against a 322-term vocabulary, with verdict bands whose precision is measured on held-out recordings | word heads from the axis words, for sound (CLAP) and picture (SigLIP 2); bands on gated heads | FP2e, FP2d |
| `profile` reference films, `critique` against their ranges with timestamped fixes, checked leave-one-out (86% in range against a 71% baseline) | profiles from golden references and approved work: the positives that notes don't give (3 approvals in 205 notes) | FP6e |
| realness: real against code-synthesised sound, a small head on CLAP vectors, grouped CV AUC 0.92 held out by category and engine | a realness axis for sound, with labels by construction from raea's own generators (TokenSynth, SAO, Fish) against recorded foley; matches the taste rule "real material only" | FP2f |
| summary first, `--json`, an annotated `sheet` PNG, a `timeline` transcript | the same outputs from `raea fingerprint score` | FP3a, FP3c |
| a resident MCP server, because loading the models costs 20-40 s | everything in the feed host, models loaded once | FP3a |
| "model critics stop discriminating after search; model-free measurements still do"; a "cannot judge" list | checks as arbiter, red-team cheats per axis, limits in report.json | FP4b, FP4c, FP9a |

We don't take:
- its Python and PyTorch runtime;
- AST: YAMNet already serves event classes, and CLAP's heads match or beat YAMNet's on our defects;
- its larger CLAP checkpoint (larger_clap_general), until htsat-unfused falls short on a measured axis;
- its optional Gemini critique, which breaks the two-API-models rule;
- its cache key.

Its results are not ours to quote as Fingerprint's. Our numbers for the same ideas are FP0g below.

## What was measured (11-12, this Mac; spike in ~/films/fingerprint-spike)

The data:
- **Notes:** Shane's 205 pinned notes, with time and place, on 17 feed renders (Stay assemblies 18-23 and the launch
  film's passes b-l).
- **Videos:** those 17 renders, the five golden references and 11 other renders: 33 videos, 34 minutes.
- **Encoders:** SigLIP 2 base (ONNX fp32), YAMNet, EmbeddingGemma 2 (q8, through apps/app host/library/embed.ts) and
  CLAP htsat-unfused (Xenova's ONNX through the same transformers.js).

**FP0a. Notes alone don't say where the next note goes.** A logistic head scored 197 noted 2 s windows against 263
windows at least 3 s from any note:

| features | AUC held out by video | AUC held out by film |
|---|---|---|
| position in the video | 0.41 | 0.40 |
| cheap columns (brightness, contrast, motion, loudness) | 0.53 | 0.51 |
| SigLIP 2 | 0.64 | 0.40 |
| YAMNet | 0.54 | 0.46 |
| EmbeddingGemma 2, picture and sound | 0.64 | 0.53 |

Within a film, the heads score above chance because the same scenes recur across versions. Across films they're at
or below chance whichever encoder reads the windows, so the encoder isn't what's missing. The notes aren't good/bad
labels:
- about 98 name a defect, about 74 are directions (23 are both), and 3 are approvals;
- there is one note every 3-4 s of film;
- only 21 of 143 noted moments make a version pair.

**FP0b. A named axis is learned from a few examples.** Defects were put into real renders and the heads held out by
source. Each cell is AUC at k = 4 / 16 / 32 labelled windows a class:

| axis | own columns (SigLIP 2, change, YAMNet) | CLAP head | CLAP words only, no labels | EmbeddingGemma 2 |
|---|---|---|---|---|
| blur | 1.00 / 1.00 / 1.00 | | | 0.70 / 0.90 / 0.96 |
| too dark | 0.72 / 0.90 / 0.95 | | | 0.58 / 0.74 / 0.85 |
| glow | 0.55 / 0.69 / 0.80 | | | 0.54 / 0.65 / 0.73 |
| flash frame | 0.97 / 0.97 / 0.97 | | | |
| freeze | 1.00 / 1.00 / 1.00 | | | 0.56 / 0.66 / 0.77 |
| noise under the sound | 0.99 / 1.00 / 1.00 | 0.94 / 1.00 / 1.00 | **0.97** | 0.82 / 0.92 / 0.97 |
| sound drops out | 0.79 / 0.94 / 0.97 | 0.86 / 0.99 / 0.99 | 0.66 | 0.68 / 0.88 / 0.94 |
| clipped sound | 0.91 / 0.97 / 0.98 | 0.94 / 0.99 / 1.00 | **0.98** | 0.80 / 0.92 / 0.96 |

**FP0g. CLAP, measured 11-12.** CLAP ran on 2 s windows, 130 windows from 26 sources.
- **Words alone.** With no labels at all, the words catch noise and clipping at AUC 0.97 and 0.98. At the threshold
  that flags 10% of clean windows, they catch 0.94 and 0.95 of the damaged ones.
- **Dropout fails from words** (0.66): silence isn't a sound a description matches. So dropout is a check (a silence
  gap) plus a head, never a word head.
- **Cheats.** Noise at a third of the level is caught 0.48 from words (0.78 on 5 s windows), so word heads stay
  advice.
- **Learned heads.** CLAP heads match or beat YAMNet on every sound axis, most at k = 4.
- **Window length.** 5 s windows change little: words 0.99 / 0.60 / 0.98; heads at k = 4 0.98 / 0.76 / 0.98.
- **q8 copy.** It holds the numbers (words 0.99 / 0.67 / 0.97), with audio vectors at cosine 0.9975 to fp32.
- **Cost.** 33 ms a clip on the CPU.

**FP0b's lessons on method.**
- Standardise columns. Unstandardised, SigLIP hid behind YAMNet's larger values: blur scored 0.75 at k = 32.
- Keep each axis on its own modality. A head given both modalities was worse at small k: dropout scored 0.68 at k = 4.

**FP0c. Taxonomy first, then the person.** Eight shared axis heads, followed by a person's 9 weights fit from 32 of
their labels, scored 0.84, 0.73 and 0.95 on three made-up tastes. One head fit straight on the embeddings from the
same labels scored 0.73 and 0.64. Two opposed tastes stayed apart (each ranks the other's labels 0.38 and 0.39). Three
teammates with 16 labels each scored 0.66 alone, 0.74 with their heads averaged, and 0.76 with their evidence pooled
and retrained.

**FP0d. Cost and cheats.**
- **Cost.** A head fits in 3.9 ms on 32 windows and 81 ms on 2,430, scores 2,430 windows in 1.6 ms, and takes 7 KB.
- **Cheats**, at the threshold that flags 10% of clean work:
  - a de-rez sharpened afterwards is still caught 0.99;
  - noise at a third of the level 0.85;
  - glow only 0.33, and 0.20 at half strength.

  SigLIP's pooled vector ignores a picture's look, so look axes get their own columns.

**FP0f. Marks teach where; pins don't.** A linear head on SigLIP 2's 14 x 14 patch tokens put local defects in real
frames. Its top patch fell inside the defect box (chance 0.10):
- 0.85-0.96 of the time from 8 frames marked with a click;
- 0.89-0.99 from 8 frames marked with a box;
- 0.23-0.62 with only "this frame is bad";
- local glow 0.44-0.62.

On Shane's 203 pinned notes, a head ranks 35% of a frame's patches above the one he clicked, while his average click
position ranks 16% above it. His pins sit near the middle of the frame (x 0.50, y 0.52). A pin anchors a note where
it's easy to read; it doesn't mark the thing.

**Encoders ruled out.**
- **PE-AV small** (Meta) is a ViT-L/14 at 336 px per frame and 3.4 GB. Shane, 11-12: "if it cant run locally on
  consumer hardware its not usable."
- **V-JEPA 2** carries a research licence.
- **7B grounded vision-language critics** (Q-Insight, VisualQuality-R1) fail both the consumer-hardware rule and the
  two-API-models rule.
- **YOLOE** is AGPL.

The literature agrees with FP0b and FP0c. Personal aesthetics work finds per-user linear heads on frozen
vision-language features better than prompting or LoRA (arXiv 2604.11374). Video aesthetics work keeps the encoder
frozen (VADB, arXiv 2510.25238). Rich Human Feedback (CVPR 2024) learns heatmaps from people's points, as FP12 will.

## Build order, with confidence

| phase | step | what | confidence | why |
|---|---|---|---|---|
| 1, foundation | FP1 | columns at ingest: SigLIP 2, change, look, CLAP, YAMNet; EmbeddingGemma 2 rows read from the Library | high | every model is measured; only the look columns and CLAP's Rust front end are new |
| | FP2 | the folder, the base taxonomy, word heads, learned heads, the trainer and the gate, in Rust | high | FP0b-d and FP0g: heads learn from 4-32 labels in milliseconds, and words already score noise and clipping at 0.97-0.98 |
| | FP3 | `raea fingerprint score` through the feed host: summary, JSON, sheet, timeline, watch lane | high | plumbing over FP1-2; earshot shows the outputs agents can use |
| | FP4 | bootstrap from damage variants; red-team cheats; measured checks for dropout, clipping, loudness, flash, freeze | high, except glow (low until the look columns lift it) | FP0b, FP0d, FP0g |
| 2, learning from people | FP5 | notes into evidence: the Director reads each note, checked against Shane on 50 notes (45 right on kind, 40 on axis) | medium | FP0a: notes don't label themselves |
| | FP6 | approvals, picks, hand edits, version pairs, reference profiles | medium; profiles medium-high | the sources exist; profiles are earshot's measured method and fill the missing positives |
| | FP7 | the person's weighting and the post-mortem | medium | FP0c works on made-up tastes; a real one needs FP5-6's labels |
| 3, where | FP11 | markup in the review player: mark, box, lasso, arrow, strike, span, sound box | high to build, medium for uptake | FP0f: a pin isn't a mark |
| | FP12 | patch heads and heatmaps from marks, grown by EdgeTAM | high for blur, holes, flat surfaces, noise; low for glow | FP0f |
| | FP13 | things per shot (EdgeTAM, DA3); geometry checks; a note's nouns grounded near its pin | high, high, gated | both models are in perception; the grounder ships only if FP13b passes |
| 4, share and use | FP8 | teams: shared evidence, retrain, diff | medium-high | FP0c measured pooling; the transport is the project |
| | FP9 | the agent scores before posting and can't game it | medium | needs FP5-7 and red-team sets |
| gated | FP2f | realness axis for sound | medium, measured first | earshot's 0.92; our labels come free from raea's generators |
| | FP10 | a LoRA adapter on SigLIP 2, or another device-sized encoder | low | only for an axis stuck after its own columns |

**The first proof that matters is FP5c.** A fingerprint built from Shane's evidence on Stay assemblies 18-22 has to
rank the moments he noted on assembly 23 above the rest at AUC 0.70 or better, held out by film. The axis it names
has to match his words. The spike couldn't reach that bar with raw notes, and the plan stands on it. Phase 1 needs no
notes and is worth shipping without it: a person gets word scores, checks and bootstrapped heads the day they install.

## Decisions taken by default (Shane can overrule any)

1. **Patch grid storage.** Every frame stores its pooled vector. The 14 x 14 grid (300 KB a frame, about 108 MB for a
   90 s render) is kept only for flagged or marked frames and recomputed on demand.
2. **The note reader is the Director (Kimi).** It is checked on 50 of Shane's notes before notes train any head.
3. **Markup tools live in apps/app's review player.** The work is coordinated with the player's owner through the
   orchestrator.
4. **Teams share evidence rows, not heads.**
5. **CLAP joins the stack** (Shane, 11-12: "include clap if we can if its not too much work"). It is about 300 lines
   of Rust on ort: the 48 kHz log-mel front end and the audio session (200), and the text tower with its tokenizer
   (100), run only when an axis's words change. Both are checked against the spike's vectors at cosine 0.999.
