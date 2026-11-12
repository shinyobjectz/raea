@spec
Feature: Fingerprint: a person's or a team's sense of good work, learned on their own device from their own notes
  Shane, 2026-11-12: "build a framework within raea called Fingerprint ... some kind of taxonomy data structure and an
  embedding model that can run on a personal device that could be easily trained on device to best understand if a
  video, both its audio and video and whatever else we can put together into this ... is good or not ... train through
  all these comments or notes or through post mortem process that the agent can run ... and if you have a team, you can
  distribute those embedding models." The plan, its measurements and the reference design (earshot,
  github.com/Leonxlnx/earshot, MIT: its methods, not its code): docs/fingerprint.md.

  The model this file sets:
    1. A fingerprint is a taxonomy of axes, one small head per axis over frozen, pinned on-device encoders, a person's
       weighting over the axes, and the evidence the heads learned from. Encoders are not fine-tuned until an axis is
       measured stuck (FP10).
    2. Everything trains on the device in milliseconds, after every new label. Nothing leaves the device unless the
       person shares the fingerprint, and then it's evidence (embeddings and labels), never media.
    3. A note is not a label until it's read into one: its axis, polarity, span and layer. Creative directions go to
       the brief, never into a head.
    4. Every score is held out by project before it's trusted, and every axis has cheats that must still be caught.
    5. Agents get the axis and the moment, never the number.

  Background:
    Given the spike of 11-12 (~/films/fingerprint-spike: Python in a scratch venv, not repo code): Shane's 205 pinned
      notes on 17 feed renders (Stay assemblies 18-23, the launch film's passes b-l), the five golden references and
      11 other renders, 33 videos and 34 minutes
    And the encoders on disk: SigLIP 2 base p16-224 and YAMNet (~/.local/share/raea-models), EmbeddingGemma 2 q8
      (~/.raea/models, the Library's model through apps/app host/library/embed.ts), CLAP htsat-unfused (Xenova's
      ONNX, q8: audio 34 MB, text 127 MB, through the same transformers.js)
    And every encoder runs locally on consumer hardware or isn't used (Shane, 11-12, dropping PE-AV: "if it cant run
      locally on consumer hardware its not usable"); the smallest PE-AV is a ViT-L/14 at 336 px per frame, 3.4 GB
    And Rust goes through the lanes' build rules, new code in small files and a small crate (systems/fingerprint),
      repo scripts in bash, any heavy maths a Rust subcommand

  Rule: FP0. Measure before building (the spike)
    @done
    Scenario: FP0a. Notes alone don't say where the next note goes
      Then 11-12, on the 9 renders with 8 or more notes (197 noted 2 s windows, 263 windows 3 s or more from any note),
        a logistic head ranks noted above un-noted at AUC 0.64 on SigLIP 2 held out by video, and 0.40 held out by
        film; YAMNet 0.54 / 0.46, all of them together 0.55 / 0.42, cheap columns 0.53 / 0.51, position 0.41 / 0.40;
        EmbeddingGemma 2's picture, sound and both 0.65 / 0.38, 0.66 / 0.47, 0.64 / 0.53 (2.3 s a window on the CPU)
      And the kind of note (10 axes read from its words) is predicted from its window 0.150 of the time, against 0.130
        for the commonest kind
      And the reason is in the notes: of 205, about 98 name a defect, about 74 are directions (23 are both), and 3 are
        approvals; one note every 3-4 s of film; a noted moment is noted again within 1 s in the next version 0.25-0.62
        of the time, against 0.26-0.45 for any moment; 21 of 143 noted moments make a version pair, too few to score

    @done
    Scenario: FP0b. A named axis is learned from 4 to 32 examples
      Then 11-12, defects put into 270 windows of 27 real renders, held out by source, on standardised features of the
        axis's own modality (AUC at 4 / 16 / 32 labelled windows a class):
        | axis     | features | k=4  | k=16 | k=32 |
        | blur     | picture  | 1.00 | 1.00 | 1.00 |
        | dark     | picture  | 0.72 | 0.90 | 0.95 |
        | glow     | picture  | 0.55 | 0.69 | 0.80 |
        | flash    | change   | 0.97 | 0.97 | 0.97 |
        | freeze   | change   | 1.00 | 1.00 | 1.00 |
        | noise    | sound    | 0.99 | 1.00 | 1.00 |
        | dropout  | sound    | 0.79 | 0.94 | 0.97 |
        | clip     | sound    | 0.91 | 0.97 | 0.98 |
      And without standardising, SigLIP hides behind YAMNet's larger values (blur 0.75 at 32); with both modalities
        a head is worse at small k (blur 0.97, dropout 0.68 at 4)

    @done
    Scenario: FP0c. Taxonomy first, then a person's weighting
      Then 11-12, eight shared axis heads and a person's 9 weights over their scores, from 32 of that person's labels,
        give AUC 0.84, 0.73 and 0.95 for three made-up tastes, against 0.73 and 0.64 for one head straight on the
        embeddings from the same labels
      And two opposed tastes stay apart: each head ranks the other person's labels at 0.38 and 0.39
      And three teammates' 16 labels each: one alone 0.66, heads averaged 0.74, evidence pooled and retrained 0.76

    @done
    Scenario: FP0d. Cost and cheats
      Then a head fits in 3.9 ms on 32 windows and 81 ms on 2,430, scores 2,430 in 1.6 ms, and is 7 KB in fp32;
        SigLIP 2 costs 47 ms a frame on the CPU, so a 90 s render at 4 fps takes 17 s
      And at the threshold that flags 10% of clean work, a de-rez sharpened afterwards is still caught 0.99 and noise
        at a third of the level 0.85, but glow only 0.33 (half strength 0.20): SigLIP's pooled vector ignores a
        picture's look, so look axes need their own columns

    @done
    Scenario: FP0e. EmbeddingGemma 2 on the same windows
      Then 11-12, the Library's EmbeddingGemma 2 vectors (q8 on the CPU, 2.3 s a window: 2 frames at 1 fps and 2 s of
        sound) on FP0a's windows: picture 0.65 / 0.38, sound 0.66 / 0.47, both 0.64 / 0.53 (by video / by film), no
        better than SigLIP 2
      And on FP0b's axes (135 windows, held out by source, k = 4 / 16 / 32): blur 0.70 / 0.90 / 0.96, glow 0.54 / 0.65 /
        0.73, dark 0.58 / 0.74 / 0.85, freeze (2 frames) 0.56 / 0.66 / 0.77, noise 0.82 / 0.92 / 0.97, dropout 0.68 /
        0.88 / 0.94, clip 0.80 / 0.92 / 0.96: it learns them, more slowly than SigLIP 2, change and YAMNet
      And so video and sound axes read SigLIP 2, change, look and YAMNet; EmbeddingGemma 2 carries words, scripts,
        boards and stills, and the Library's rows; PE-AV was dropped untested (it can't run on consumer hardware)

    @done
    Scenario: FP0f. Marks teach where; today's pins don't
      Then 11-12, five local defects in a random 64-80 px box of 290 real frames, linear heads on SigLIP 2's 14 x 14
        patch tokens, held out by source: the head's top patch falls in the box (chance 0.10) 0.85-0.96 of the time from
        8 frames marked with a click on the defect, 0.89-0.99 with a box, and 0.23-0.62 with only "this frame is bad",
        for blur, a hole, a flat grey surface and noise; local glow 0.44-0.62
      And on Shane's 203 pinned notes, a head trained on other videos' clicks ranks 35% of a frame's patches above the
        clicked one (44% by film), and his average click position 16%: pins sit near the middle (x 0.50 sd 0.14, y 0.52
        sd 0.18), also on the 89 that name a defect, so a pin anchors a note and isn't a mark of the thing

    @done
    Scenario: FP0g. CLAP: sound axes from words alone, and heads at least as good as YAMNet's
      Then 11-12, CLAP htsat-unfused on FP0b's sound defects (130 windows of 2 s from 26 sources, held out by source):
        an axis prompt's similarity as a z against 40 background descriptions (earshot's calibration), with no labels,
        scores noise 0.97, clipping 0.98 and dropout 0.66 (AUC damaged against clean); at the threshold that flags 10%
        of clean windows it catches 0.94, 0.95 and 0.12
      And a head on CLAP's audio vectors scores, at k = 4 / 16 / 32: noise 0.94 / 1.00 / 1.00, dropout 0.86 / 0.99 /
        0.99, clipping 0.94 / 0.99 / 1.00, at or above YAMNet's (FP0b) on every axis from k = 16
      And noise at a third of the level (the cheat) is caught 0.48 from words alone (0.78 on 5 s windows), so a word
        head is advice; silence matches no description, so dropout is a check and a head, never a word head
      And 5 s windows change little; q8 keeps the numbers (words 0.99 / 0.67 / 0.97) at cosine 0.9975 to fp32; a 2 s
        clip costs 33 ms on the CPU (spike: ~/films/fingerprint-spike/clap, results/fp0g-clap*.txt)

  Rule: FP1. Columns are made once at ingest and read by every fingerprint
    @needs:FP0b
    Scenario: FP1a. Each video gets its fingerprint columns
      Given the plan
        | crate                  | file                               | lines | job                                                                    |
        | raea-sys-columns       | systems/columns/src/lib.rs         | 40    | the crate's surface: a file in, its columns out                        |
        | raea-sys-columns       | systems/columns/src/columns.rs     | 250   | SigLIP 2 (through raea-sys-vision), CLAP, YAMNet, change and look columns for a file, cached by content hash and step version |
        | raea-sys-columns       | systems/columns/src/clap.rs        | 200   | CLAP's audio tower on ort (raea-sys-ml): the 48 kHz log-mel front end and the session, checked against the spike's vectors at cosine 0.999 |
        | raea-sys-columns       | systems/columns/src/clap_text.rs   | 100   | CLAP's text tower on ort with its tokenizer (the tokenizers crate), for axis words only |
        | raea-sys-columns       | systems/columns/src/look.rs        | 120   | luminance histogram, saturation, local contrast and sharpness per frame |
        | raea-sys-columns       | systems/columns/src/window.rs      | 120   | a span's features from the columns (mean, change, look), as the heads read them |
      When a video is ingested or rendered
      Then it gets SigLIP 2 per frame at 4 fps (the pooled vector, fp16), CLAP per 2 s window with a 1 s hop, YAMNet per
        0.48 s, change (frame-to-frame SigLIP distance) and look (luminance histogram, saturation, local contrast,
        sharpness) columns, stored under the file's content hash and each step's version (platform.feature: derived
        files are a cache)
      And SigLIP 2's 14 x 14 patch tokens (300 KB a frame) are kept only for flagged or marked frames, and remade on demand
      And the columns are made inside the feed host, which keeps the models loaded (earshot's CLI pays 20-40 s a call)
      And the Library's EmbeddingGemma 2 rows (v, a, t a segment) are read where they exist, never remade
      And every encoder runs on ort (raea-sys-ml) everywhere; on the Mac SigLIP 2 moves to MLX (raea-sys-mlx) once
        docs/mlx.feature MX5 gives it a path, never Core ML's GPU, and any new runtime path is checked against ONNX fp32
        at cosine 0.999 or better before its columns are kept
      And no encoder runs through transformers.js or Python; the spike's transformers.js vectors are only the reference

    @needs:FP1a
    Scenario: FP1b. Not only video
      Then scripts, briefs, storyboards, stills, pages and sounds get EmbeddingGemma 2 rows the same way, so a
        fingerprint can hold axes about them (a script that reads the buttons aloud, a board with wide empty shots)

  Rule: FP2. The fingerprint is a folder, its taxonomy is TypeScript
    @needs:FP1a
    Scenario: FP2a. What's in it
      Then a fingerprint is ~/.raea/fingerprints/NAME/ (a person's), or PROJECT/.raea/fingerprints/NAME/ (a project's
        or team's), holding fingerprint.ts (`export default fingerprint({...})` from "raea/brief"), heads.safetensors,
        evidence.jsonl with evidence.f16 beside it, and report.json
      And fingerprint.ts names its encoders by id and hash, and a head refuses columns from any other

    @needs:FP2a
    Scenario: FP2b. An axis
      Then an axis has a name, the words people use for it, a modality (picture, change, look, sound, words), a scope
        (moment, shot, whole piece), a polarity, and optionally the taste check it maps to (text-hold, loudness...)
      And raea's base taxonomy is drawn from Shane's notes: glitch and continuity, sound, pace, camera, cut, light and
        look, realism and detail, interaction, design and type, story
      And a person or team adds axes in their fingerprint.ts; directions are never axes

    @needs:FP2b,FP1a
    Scenario: FP2e. Every axis scores from its words on day one
      Given the plan
        | crate                  | file                               | lines | job                                                                    |
        | raea-sys-fingerprint   | systems/fingerprint/src/words.rs   | 120   | word heads: an axis's words against the background vocabulary as a z, per window |
      Then each axis's words are embedded when they change (CLAP's text tower for sound, SigLIP 2's for picture, on ort) and stored in the fingerprint as words.f16, with a background vocabulary of about 40 ordinary
        descriptions; the text towers never run at score time
      And a word head is always advice, and hands over to the learned head once that beats it on held-out evidence
      And on FP0g's defects the base taxonomy's words score noise and clipping at 0.97 or better with no labels

    @needs:FP1a,FP2c
    Scenario: FP2f. Realness, measured before it ships
      Then a realness axis for sound is trained from labels by construction: raea's own generated sound (TokenSynth,
        Stable Audio Open, Fish) against recorded foley and footage, held out by generator and by category, as earshot
        holds its out by engine and category (its grouped CV AUC 0.92)
      And it ships as an axis only at 0.85 or better held out by generator; it's a taste check ("real material only"),
        never a claim that a sound is or isn't AI-made

    @needs:FP2a
    Scenario: FP2c. Heads train in milliseconds, on the device
      Given the plan
        | crate                  | file                               | lines | job                                                                    |
        | raea-sys-fingerprint   | systems/fingerprint/src/lib.rs     | 60    | the crate's surface: columns and evidence in, scores out               |
        | raea-sys-fingerprint   | systems/fingerprint/src/folder.rs  | 200   | the fingerprint folder read and written: heads.safetensors, evidence.jsonl and .f16, report.json |
        | raea-sys-fingerprint   | systems/fingerprint/src/evidence.rs| 180   | an evidence row (hash, span, axis, polarity, weight, who, source, columns) |
        | raea-sys-fingerprint   | systems/fingerprint/src/head.rs    | 220   | logistic and Bradley-Terry heads on standardised columns, their encoder pin |
        | raea-sys-fingerprint   | systems/fingerprint/src/train.rs   | 250   | fitting (L-BFGS), the person's weighting, held-out by project scores, the gate |
        | raea                   | sdk/raea/src/fingerprint.ts        | 60    | `fingerprint({...})` and the Axis type, re-exported from raea/brief beside `taste` |
      Then each axis's head is a logistic fit on its modality's columns, standardised with the encoder's shipped
        statistics; an axis with pairs adds a Bradley-Terry fit on their differences; a person's weighting is a
        logistic fit over the axis scores
      And a retrain runs after every new evidence row and takes under a second for 10,000 rows on the CPU
      And the trainer is a Rust crate (systems/fingerprint), with no Python at run time

    @needs:FP2c
    Scenario: FP2d. A head is advice until it's earned its place
      Then a head gates only once it has 16 labels of each polarity or more, from 3 projects or more, and a held-out
        by project AUC of 0.8 or better; until then `raea fingerprint score` marks it advice
      And a gated head reports strong, plausible or weak, each band with the precision measured on its own held-out
        evidence (earshot's verdict bands)
      And report.json gives each head's held-out score, its labels by source, its red-team results, and what it can't
        judge (earshot's LIMITS.md)

  Rule: FP3. Scoring a piece
    @needs:FP2c
    Scenario: FP3a. raea fingerprint score
      Given the plan
        | crate                  | file                               | lines | job                                                                    |
        | raea-sys-fingerprint   | systems/fingerprint/src/score.rs   | 160   | per-window axis scores and the expected notes, as JSON |
        | raea-cli               | engine/cli/src/fingerprint.ts      | 200   | `raea fingerprint` new, score, teach, bootstrap, profile, report, postmortem and diff, through the feed host |
      When I run `raea fingerprint score FILE [--fp NAME]`
      Then it prints a summary first: the moments the fingerprint expects a note on, each with its axis named in the
        person's words and the checks' findings; every axis's score per window with --json
      And it runs in the feed host with the models already loaded
      And `raea fingerprint teach FILE --axis A --at T [--bad|--good] [--span S] [--box x0,y0,x1,y1]` adds an evidence
        row from the CLI or an agent
      And the watch timeline shows the same as a lane (the player's lane plugins)

    @needs:FP3a
    Scenario: FP3c. A sheet and a timeline an agent can read
      Given the plan
        | crate                  | file                               | lines | job                                                                    |
        | raea-sys-fingerprint   | systems/fingerprint/src/sheet.rs   | 220   | one PNG: frames with heatmaps, axis lanes, the spectrogram with flagged sounds boxed, check findings |
      When I run `raea fingerprint score FILE --sheet out.png` or `--timeline`
      Then the sheet shows the whole piece in one picture (earshot's `sheet`), and the timeline reads it second by
        second in words: what is seen, what is heard, which axes flag (earshot's `timeline`)
      And `raea show` opens the sheet for a person

    @needs:FP3a,FP4a
    Scenario: FP3b. The golden references stay clean, and a known bad cut doesn't
      Then the five golden references' approved.mp4 raise no gating flag
      And the cut Shane called "explicitly a bad one" raises more flags, on the same axes, than the approved reel
        made from the same footage (both in the golden references' private media, never in git)

  Rule: FP4. Damage variants bootstrap the axes
    @needs:FP2c
    Scenario: FP4a. Labels by construction, on real work
      Then for each mechanical axis (blur, dark, glow, flash, freeze, noise, dropout, clip, and the ones added later)
        `raea fingerprint bootstrap` makes damaged windows of the person's own approved work and trains the axis head,
        held out by source, to FP0b's numbers or better
      And no placeholder footage: the sources are real renders and footage

    @needs:FP4a
    Scenario: FP4b. Every axis has cheats that must still be caught
      Then each axis carries a red-team set (sharpened after a de-rez, half-strength glow, quieter noise, a one-frame
        flash, a freeze with a slow zoom on it), and report.json shows each cheat's catch rate at the threshold that
        flags 10% of clean work
      And a cheat caught under 0.8 holds that axis at advice
      And glow gates only once the look columns exist and its cheats pass

    @needs:FP1a
    Scenario: FP4c. What can be measured is a check, and the arbiter
      Then dropout (a silence gap in a sound that had level), clipping (samples at full scale), loudness, a flash frame
        and a freeze are measured from the columns and the sound as `raea check` findings, each with red-team cases
      And where a check and a head disagree, the check wins: earshot's search fooled its learned critics while its
        measurements held, and FP0g shows silence matching no words

  Rule: FP5. Notes become evidence
    @needs:FP2b
    Scenario: FP5a. A pinned note is read into a label
      When a person pins a note on a video (docs/review.feature R1)
      Then the Director reads it into: defect, direction or praise; the axis, from the fingerprint's own axes and their
        words; the span (by default the 1.5 s before the pin to 0.5 s after); and the layer under the pin's x,y
        through the layer pass (review.feature R2), when the video has one
      And a defect or praise becomes an evidence row with the note's words, its id and who wrote it; a direction goes to
        the brief and the agent only

    @needs:FP5a
    Scenario: FP5b. The reading is checked against the person
      Then on 50 of Shane's real notes read by the Director, he agrees with the kind (defect, direction, praise) on 45
        or more and with the axis on 40 or more, before notes train any head

    @needs:FP5b,FP6a
    Scenario: FP5c. The proof that matters
      Then a fingerprint built from Shane's evidence on Stay assemblies 18-22 ranks the moments he noted on assembly 23
        above the rest by AUC 0.70 or better on its gating axes, held out by film as well as by version, and beats the
        previous version's note times as a predictor (0.60 in FP0)

  Rule: FP6. Positives and pairs come from what the person already does
    @needs:FP2c
    Scenario: FP6a. Approvals
      Then an approval, a picture lock, a publish or a golden reference is a positive on every axis over every span with
        no defect note
    @needs:FP6a
    Scenario: FP6b. Picks and hand edits
      Then a pick between real options (docs/craft-system.md F) is a pair on the axes the options differ on, and a hand
        edit's before and after (~/.raea/taste/corrections.jsonl) is a pair on the axis the edit touched
    @needs:FP6a
    Scenario: FP6c. Version pairs
      Then a noted moment whose picture or sound changed in the next version (cosine distance 0.01 or more) and that
        isn't noted again within 1 s is a weak pair (weight 0.5)
    @needs:FP6a
    Scenario: FP6d. Another eye stays another eye
      Then clef's or Kimi's readings are evidence from that eye, with their source, and never count as the person's

    @needs:FP2c,FP4c
    Scenario: FP6e. Profiles: ranges from work the person admires
      Given the plan
        | crate                  | file                               | lines | job                                                                    |
        | raea-sys-fingerprint   | systems/fingerprint/src/profile.rs | 180   | a profile's ranges over measures and axis scores, the leave-one-out check, critique against it |
      When I run `raea fingerprint profile REFS... --as NAME`
      Then each measure (loudness, spectral balance, cut rate, motion, brightness and contrast) and each axis score gets
        the range the references span, and `raea fingerprint score --profile NAME` says which fall outside it, where,
        and which way to move (earshot's `profile` and `critique`)
      And the profile is checked leave-one-out: each reference against a profile of the others must fall in range more
        often than against a profile of unrelated work (earshot's 86% against 71%), or it's reported as not telling
        this taste apart
      And the golden references and approved cuts are the first profiles, and give the positives notes don't (3
        approvals in 205 notes)

  Rule: FP7. The post-mortem
    @needs:FP5a,FP6a
    Scenario: FP7a. The agent asks what it's least sure of
      When a project is closed, or on `raea fingerprint postmortem PROJECT`
      Then the agent scores every version, finds the moments where the fingerprint is least sure or disagrees most with
        the person's history, and asks at most ten yes/no questions on them with `raea show --question`, each with the
        moment playing
      And each answer is an evidence row, and the report says how each head's held-out score moved

  Rule: FP8. Teams
    @needs:FP2d
    Scenario: FP8a. Sharing is evidence, not weights
      Then sharing a fingerprint shares fingerprint.ts and the evidence rows (embeddings, spans, labels, who), through
        the project's repo (Git + LFS) or raea's API, under the project's permissions; never media
      And each teammate's machine retrains the shared heads from the pooled evidence (FP0c: pooled 0.76 against
        averaged heads 0.74 and one person's 0.66)
      And each person's weighting stays their own; a team fingerprint's weighting is fit on everyone's labels

    @needs:FP8a
    Scenario: FP8b. Where tastes differ
      Then `raea fingerprint diff A B` lists the axes where two fingerprints disagree most on the same evidence, with
        the moments that show it

    @needs:FP8a
    Scenario: FP8c. Encoder upgrades keep the evidence
      Then when an encoder is replaced, rows whose media is present are re-embedded and the heads retrained; rows
        without media keep the old pin until re-labelled

  Rule: FP9. Agents use it, and can't game it
    @needs:FP3a,FP4b,FP5c
    Scenario: FP9a. Before posting a cut
      Then the feed's agent runs `raea fingerprint score` on a cut before posting it and fixes each gating flag, or says
        why not
      And the agent is told the axis and the moment in the person's words, never a score or a threshold
      And a fix is checked against the flag it claims to fix, and the axis's cheats are rerun on it

  Rule: FP11. Markup: the player asks for a mark that means "this thing"
    @needs:FP5a
    Scenario: FP11a. Marks beside the pin
      Given the review player (docs/review.feature R1), where a pin places a note
      Then a note can carry marks: mark (a click on the thing), box, lasso, arrow (from a thing to a place), strike (a
        stroke through a thing), span (a drag along the timeline) and, on sound, a box on the spectrogram
      And each mark is stored with the note, in the frame's 0-1 coordinates, with its frame or span and the layer keys
        under it from the layer pass when the video has one
      And a note without a mark keeps its pin, which is never read as a mark (FP0f)

    @needs:FP11a
    Scenario: FP11b. What each mark teaches
      Then a mark or a box on a defect note is a region label for that axis on those frames; an arrow is a direction
        (the layer and its displacement) and goes to the agent as an instruction; a strike is "remove this layer" for
        the agent; a span sets the label's time; a spectrogram box is a time and frequency label on the sound
      And a click is grown into a region by EdgeTAM and tracked through its shot, so one mark labels every frame of it,
        with the frames EdgeTAM loses dropped

  Rule: FP12. Heatmaps: a picture axis says where, not only whether
    @needs:FP11b,FP2c
    Scenario: FP12a. Patch heads
      Then each picture axis also has a patch head on SigLIP 2's patch tokens, trained from region labels (marked
        patches positive, clean frames' patches and a box's outside negative), and `raea fingerprint score` gives a
        heatmap for each flagged moment
      And on raea's renders the heatmap is read through the layer pass into the layers it covers, so a flag names the
        mesh, the text or the component
      And on FP0f's local defects, pointing is 0.85 or better from 8 marked frames for blur, holes, flat surfaces and
        noise, held out by source, and glow gates only once the look columns lift it to 0.8

    @needs:FP12a
    Scenario: FP12b. Pointing on real marks
      Then once Shane has made 50 marks on defect notes, a patch head trained on the other projects' marks puts his mark
        in its top 10% of patches on half of them or more, against his own centre prior, held out by project

  Rule: FP13. Things, not only pixels: depth, masks and words for footage raea didn't make
    @needs:FP11b
    Scenario: FP13a. A shot's things
      Then footage gets, per shot, its things: masks from EdgeTAM (from marks, and from words below), depth and the
        camera's path from Depth Anything 3 Small (both already in perception), and one embedding per thing (SigLIP 2's
        patch tokens pooled in its mask)
      And on raea's renders the same things come from the layer pass, the depth and motion passes and the scene's
        camera, with no model run

    @needs:FP13a,FP5a
    Scenario: FP13b. A note's words find its thing (the test that decides whether a grounder ships)
      Then an open-vocabulary detector that runs on consumer hardware under a commercial licence (Grounding DINO or
        Florence-2 base, measured first; nothing AGPL) finds a defect note's nouns within 20% of the frame of its pin
      And on 30 of Shane's defect notes that he also marks (FP11a), the grown region covers his mark on 24 or more;
        below that, notes stay frame labels and the detector isn't shipped

    @needs:FP13a
    Scenario: FP13c. Geometry axes are checks, not heads
      Then floating (a gap under a thing that should rest), passing through (two things' masks and depths crossing),
        a camera that changes speed mid-move, and a frame with nothing on it are measured from depth, masks and the
        camera's path as `raea check` findings, each with red-team cases, and never trained as heads

  Rule: FP10. A fine-tuned or other device-sized encoder, only on evidence
    @needs:FP4b
    Scenario: FP10a. When an axis stays stuck
      Then an axis under its bar with 300 labels or more, after its own columns were added, may try a LoRA adapter on
        SigLIP 2's last blocks trained on the device through raea-sys-mlx with a heatmap loss as well as the axis's own
        (checked against PyTorch to cosine 0.999), or another encoder that runs locally on consumer hardware
      And either is kept only if it raises that axis's held-out by project AUC by 0.05 or more, at a cost the person's
        device can pay, and the result is recorded here with the date
