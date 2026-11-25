# Videoform: the plan

An agency or a creator makes the same kind of video again and again: a listing walkthrough, a recipe short, a podcast
clip, a product unboxing. They give raea five to ten finished videos of that kind, from their own drive or by link, and
a line of context ("real estate walkthroughs for agents' Reels"). raea reads them and writes down what makes them that
kind of video: its **videoform**. From then on raea cuts new raw footage to it, and checks every pass against it,
quickly.

This is the one plan for videoforms: what one is, how it flows into what raea already has (a videoform is a kind of
fingerprint), how videos come in (from the local drive, or streamed from a URL with nothing downloaded), how each is
read once into embeddings and measures that are all a videoform ever needs again, how it's followed and checked, how
it holds up when references disagree or encoders change, how it's judged, and the build order with a confidence for
each step. The rask tasks follow in `videoform.feature` once this plan is agreed.

## Why a videoform is read from approved work, not written from notes

A style written by hand from a person's corrections records only what went wrong once. What was right from the start
is never said, so it's never written down. Pacing is the plainest case: if the first cut's voice was tight, nobody
leaves a note about it, the written style never mentions it, and the next cut made from that style can be slack while
satisfying every line of it. The approved videos already hold the answer, in numbers: how long the pauses are, how
long the shots are, how fast the words come, where the cuts fall. A videoform read from them carries what was approved,
not only what was corrected. Notes still matter. They teach what the numbers can't (DR6-DR8, FP5), on top of it.

## Decisions taken

- **The name is videoform.** The spec is `videoform` (raea/brief), and the command is `raea videoform`.
- **A videoform is a kind of fingerprint,** flowing into what's built. It's a fingerprint (FP14) that `extends` the
  person's own and adds what a kind of video needs: its context, its references, its vocabulary, its structure and its
  rules. Everything Fingerprint already does applies to it: finding the one that applies, inheriting evidence, being
  taught by notes, being shared with a team and keeping its evidence when an encoder upgrades.
- **What ships runs on shipped models, on the person's machine.** Reading, embedding, storing and building new videos
  from a videoform use only the models raea ships, plus at most one or two light additions. There are no paid language
  model APIs in the product.
- **The domain vocabulary comes from the person's own AI.** The agent already in raea (Claude, Kimi, any of them) turns
  the context line into the things to look for. That adds no new bill and no shipped domain packs.
- **What stays the same across the references is the videoform; what varies is the content.** A rule's strength is
  its consistency across the references. Every videoform is checked by building it from all references but one and
  testing it on the one left out.
- **A reference is read once, and never needed again.** Its embeddings and measures are what the videoform keeps, and
  they're enough to follow it and check against it forever after. Following and checking never open a reference, so
  they stay fast, and the references can be deleted, offline or behind a dead link without the videoform noticing.
- **The context is everything said of the videos, never one line (VF8f).** References come in groups (a brand and its
  round), each with the documents given with it (a brief, a brand's guides, notes) and the person's words as they gave
  them. The person's words are from now and outrank every document; documents are from the past, the newest first.
  Embeddings read them (EmbeddingGemma 2, against what each shot says and shows); the person's AI writes only what a
  similarity can't tell (a never, an order, a choice). What two groups or more asked, and how it was made, is the
  videoform's; what one group asked is its brand's. No count cuts a finding off: each says how strongly it holds.
- **Checks start from the composition.** Most of a videoform's rules are measured on what raea knows it put where,
  before any render. Embeddings run only on shots that changed.

## What raea already has, and what this builds on

| Already in raea | What it gives a videoform | Gap this plan fills |
|---|---|---|
| Fingerprint: `fingerprint.ts` with `about`, `extends`, `applies` kinds and tags; `which` finds one by project, workspace and account; a child inherits its parent's evidence, its own counted more; notes teach the one that judged (FP14a-e) | the home: a videoform is one of these | the fields a kind of video needs |
| Fingerprint's frozen encoders (SigLIP 2, CLAP, YAMNet, EmbeddingGemma 2, Depth Anything 3), columns cached by content hash, word and learned heads, evidence in f16 rows (FP1-FP4) | the per-shot vectors and measures, and where they're kept | no shot vocabulary, no structure |
| Fingerprint profiles: ranges from admired work, critique against them, a leave-one-out test (FP6e, `raea fingerprint profile`) | a videoform's ranges, and its validation | few measures (loudness, tilt, cut rate, motion, light, colour, sharpness) |
| Taste lines that carry a check, a check by name or a `.ts` check on raea/check's world (DR4; a standing rule is a taste line, PJ9) | rules that `raea check PROJECT` and every agent hold work to | none written from references |
| Notes become checks, and climb to project, workspace and taste (DR5-DR8, `raea notes keep`) | the path a correction takes to becoming a rule | a videoform's rules start from evidence, not notes |
| `raea check`: findings and the record (DR3); who can fix what, carried findings, one decode (raea-app-check-cause, PR #205) | the check every pass runs | a videoform's rules join it as more rules |
| `NAME.script.ts` (scenes, beats, lines, cut); `brief.ts` (`references`, `look`, `mode`, `deliver`) | a storyboard spec, and a brief that names references | no template made from references; references named, never read |
| `taste.ts`, six checked forms | the person's standing lines | a videoform's mined rules are a taste file of their own, NAME.taste.ts, beside it |
| Footage perception (`systems/footage`: shots, cuts, camera, faces, words on screen, scene labels, performance, facts.json) | what each reference shot is | read for raw footage, not for references |
| Transcription (Parakeet), beats, stems and voice activity (`raea song`, HTDemucs, Silero VAD), OCR | words, pauses, cuts against beats, music under voice, captions | not gathered into a videoform |
| Index and libraries (SigLIP over a person's media; turbopuffer in the cloud) | finding footage by meaning; vector storage for teams | — |
| video2raea sound, img2raea | reverse engineering a piece, with answer keys from our own films | — |
| The video-decompiler skill (raea-skills): one admired film taken apart into a written dossier (`REFERENCE.md`), `vd pin` writing a film's numbers as targets | holding a piece to a reference, and measured targets | one reference, read by hand; product films made from code, not footage cuts |
| raea's URL cache (`io/net`): a direct file URL fetched once into a cache | reading a media link | it saves the file; no platform pages, playlists or channels |

## A videoform is a kind of fingerprint

`fingerprint.ts` gains an optional `videoform` section. A fingerprint with one is a videoform. It's found by `which`
like any other (its `applies` names the kinds and tags of piece it judges), it `extends` the person's fingerprint, so
their general taste carries in, and it's kept and shared where fingerprints are (`.raea/fingerprints/NAME`, a
project's, a workspace's or the person's own).

```
.raea/fingerprints/listing-walkthrough/
  fingerprint.ts      about, extends, applies, axes; and videoform: { context, references, vocabulary, template, rules }
  vocabulary.ts       what to look for, written by the person's AI from the context
  profiles/           the videoform's ranges (FP6e's profile, measures added)
  read/               the references' per-shot rows, in the f16 format Fingerprint's evidence uses; the structure; the report
  evidence.jsonl      as any fingerprint: the person's notes, picks and verdicts on pieces it judged
```

Each part of a videoform flows into the part of raea that already does that job:

| A videoform's... | Flows into | So that |
|---|---|---|
| ranges (pace, pauses, shot lengths, loudness, colour, captions) | its own profile (FP6e) | `raea fingerprint score --profile` critiques a cut against them, and leave-one-out is already built |
| the taste taught later | its own evidence and heads (FP2, FP14b-d) | a note on a piece it judged teaches it, never the person's general fingerprint unless they move it up |
| rules (coverage, order, event rules, limits) | NAME.taste.ts, taste lines with their checks, which `videoform.rules` names | `raea check PROJECT` runs them with the person's taste (VF10) |
| structure (slots, their order and lengths) | a `NAME.script.ts` template, which `videoform.template` names | a project made to the videoform starts from it, and agents plan from it as from any storyboard |
| references | `brief.ts`'s existing `references` shape, `{ target, note }`, with the source kept by content hash | a reference is named the way a brief already names one |

`raea videoform new NAME --context "…" REF...` makes one, as `raea fingerprint new` does. `raea fingerprint which` on a
piece names the videoform that judges it and why, and `raea context` prints it with the brief.

## Bringing videos in, with nothing downloaded

- **From the local drive.** A file or a folder (`raea videoform add NAME ~/Videos/listings/`), or a library's files, is
  read where it is. No copy is made.
- **From a URL, streamed.** `raea videoform add NAME URL...` reads the video over the network in one pass and keeps only
  what was measured:
  - **A direct link to a file** (an `.mp4` on a server, S3, a share link that serves the file) is decoded straight from
    the network.
  - **A platform page** (YouTube, TikTok, Instagram, Vimeo, X), a playlist or a creator's channel with `--last N`
    isn't a video file: it's a page with a player, and the video sits behind it as short-lived, signed stream URLs,
    often in segments. A resolver (yt-dlp, Unlicense, fetched on first use as raea fetches FFmpeg on Windows) turns
    the page into those stream URLs, and raea streams them. Nothing is written to disk.
- **One pass.** A streamed video can't be sought cheaply, so every reader takes its share of the one decode, as raea
  check's picture rules now do. Shot bounds come from the same pass, and the per-shot reads hold a shot's frames until
  it ends.
- **What's kept.** Each video's readings (below), by content hash of the decoded stream, with its source URL, title,
  creator and date. The video itself is not kept. A video already read is recognised by its hash and never read twice.
- **Rights.** Once per account, before the first reference is read from a URL, raea asks the person to confirm they have
  the right to use what they add (their own work, a client's, or licensed), says that a videoform keeps measurements and
  vectors and never the video, and that a platform's terms are theirs to keep. The answer is kept with the account.
- **Limits.** Login-only and private links are refused, with the reason given. Streams are read at 1080p at most;
  reading never needs more.

## The resolver, and keeping it current

A platform page is resolved by **yt-dlp** (Unlicense): the most widely used and most actively kept page resolver, with
extractors for over a thousand sites and releases every week or two. raea uses it only to resolve, never to download:
`yt-dlp -J URL` (a playlist or channel: `--flat-playlist`, `--playlist-end N`) gives each video's formats as JSON, with
their stream URLs and the headers they need, and raea picks the best stream at 1080p or under and decodes it itself.

Sites change their pages often, and an old resolver stops working within weeks, so it's kept current by raea, not by
the person:
- **Fetched, not installed.** The official standalone binary for the platform (`yt-dlp_macos`, `yt-dlp_linux`,
  `yt-dlp.exe`) is fetched on first use into `~/.raea/bin`, as raea fetches FFmpeg on Windows, and checked against the
  release's published SHA-256 sums before it runs.
- **Pinned, then moved forward.** Each raea release pins the yt-dlp version it was tested with. Between raea releases,
  raea checks yt-dlp's latest release at most once a day when a URL is added, fetches it, checks its sums, and keeps the
  one before it.
- **Updated when a site breaks.** When resolving fails with an extractor error, raea fetches the latest release and
  tries once more. If the new one fails where the old one worked, it goes back to the old one and says which site
  failed.
- **Said, not hidden.** `raea doctor` prints the yt-dlp in use, its date, and the last site that failed. A link that
  can't be resolved is refused with the site's reason, and the person can add the file from their drive instead.
- **Its needs.** Some sites' pages (YouTube's) make yt-dlp run JavaScript, for which it uses a JavaScript runtime on the
  machine. raea gives it one: yt-dlp is pointed at a runtime raea ships or fetches, and the bench checks it against
  each site raea names as supported.
- **Tested.** The test bench resolves one public video from each supported site each night, so a broken site is known
  before a person finds it.

## Reading a reference

Each video is read into rows by what raea already runs:
- footage perception: shots, cuts, camera, faces, words on screen, scene labels;
- Fingerprint's columns: SigLIP 2 per shot, CLAP and YAMNet over the sound;
- Parakeet's words, and the voice's pauses;
- beats and stems;
- OCR's caption boxes;
- depth, for shot scale.

All of it is keyed by content hash, as Fingerprint's columns are, so a reference read for one videoform serves every
videoform that uses it.

## What a videoform keeps, and why it's enough

Following and checking need four kinds of thing from the references, and a videoform keeps exactly those:

| Kept | What it is | What uses it |
|---|---|---|
| per-shot rows | each reference shot's SigLIP 2 vector (one pooled vector, a few frames averaged), its CLAP vector over the shot's sound, and its measures (length, scale, camera move, caption boxes, light and colour, words, pauses at its edges, its place in the video) | the shot vocabulary; matching new footage to shot kinds; the embedding checks |
| the vocabulary's vectors | each term's phrasings through the text towers, as word heads store them (FP2e) | naming shot kinds; finding domain things in new footage |
| the profile | the ranges of every measure across the references (FP6e) | the critique, and composition-first range checks |
| the structure and the rules | the slots with their lengths and shot kinds; the event and coverage rules with their strength | the template, and the taste lines `raea check` runs |

Also kept: one small still for each shot kind (a thumbnail, a few kilobytes), so the person can see what a kind is when
confirming rules, and the evidence the person adds over time. The reference videos aren't kept.

**Sizes.** A videoform of ten one-minute references, about 50 shots each, keeps about 500 per-shot rows of about 3 KB
each (SigLIP 2's 768 values and CLAP's 512, in f16, with the measures): about 1.5 MB with its profile, structure, rules
and stills. That's small enough to load at once, keep in git with the project, and share with a team.

**Times, measured and held to.** Reading a reference is once only, at about real time or faster: one decode, every
reader on its share of it. Making the videoform's ranges, shot vocabulary, structure and rules from its rows takes
seconds. Following and checking read only what's kept: the composition-first checks in milliseconds, matching a new
shot to its kind in well under a millisecond against ~500 rows, and one SigLIP frame for each changed shot.

## When it doesn't hold together

- **Too few references.** Under five, every rule and range is reported as weak, and nothing gates on it. Leave one out
  needs at least five to mean anything.
- **References that aren't one kind of video.** If the references fall into two or more clear groups (by structure, by
  shot kinds, by pace), raea says so and offers a videoform for each, rather than averaging them into one that fits
  none.
- **A trait that varies.** A measure whose range across the references is wide, or a rule that holds in few of them, is
  a free choice, not part of the videoform. It's recorded as free, so the agent knows it may decide.
- **An outlier.** One reference far from the others on several measures is named ("number 7 runs twice as fast as the
  rest"), and the person decides whether it belongs.

## Adding and removing references

Adding a reference reads only the new one, then makes the ranges, shot vocabulary, structure and rules again from the
kept rows, in seconds. Removing one drops its rows, and the same. The person's evidence (their notes, picks and
verdicts) stays through both, as a fingerprint's does.

## When an encoder changes

A videoform records the encoders its rows came from (their pins, as `fingerprint.ts` does). A raea that ships a new
encoder keeps using the videoform's rows with the old encoder's pin, for matching and checking against it. The measures
(lengths, pauses, captions, light) don't depend on an encoder and carry over as they are. To move a videoform to the new
encoder, its references are read again (`raea videoform refresh NAME`, given the files or links). Until then it keeps
working as it was. This is the one time a reference is needed again, and it's the person's choice.

## Writing the videoform

1. **The vocabulary.** The person's AI expands the context line into `vocabulary.ts`:
   - places, then the things in each place, each with several phrasings;
   - near-misses (a kitchen against a dining room);
   - OCR patterns for text on screen (prices, addresses);
   - things to ignore.

   These become the videoform's word heads (FP2e), scored by SigLIP's and CLAP's text towers. raea reports what each
   did on the references: never fires, fires everywhere, confused with another, or a recurring shot no term names. The
   AI revises the vocabulary, and two or three rounds is usually enough.
2. **The ranges,** across the references, into the videoform's profile:
   - shot lengths, and cuts a minute;
   - the voice's pauses: inside a line, between lines, between sections;
   - words a minute, and cuts against beats and sentence ends;
   - loudness, and music under voice;
   - caption size, place and words on screen at once;
   - colour and grade;
   - talking head against cutaway.
3. **The shot vocabulary.** Every reference shot is clustered on its vectors and measures. Each cluster gets its terms
   from the vocabulary, its measured ranges (scale, move, length) and how many references have it.
4. **The structure.** The references are aligned by their run of shot kinds and their script's meaning, into slots
   (open, sections, close), each with its length range, its shot kinds and what's on screen. This becomes the script
   template.
5. **The event rules.** How often one thing comes with another across the references ("a punch-in when a number is
   said", "a close-up of the counters in every kitchen"). Those that hold in most references become taste lines,
   each stating how strongly it holds.
6. **What was asked, and how it was made (VF8f).** Each document given with a group of references is read once into
   passages (a PDF's pages by Vision, so a page that's a picture reads too; a Word document's paragraphs; text) and
   kept by content hash with its EmbeddingGemma 2 vectors. Each passage is found where its group's videos carry it
   out: the shot whose words said or shown stand out for it, and it for them. That shot's construction (where it
   falls, what carries it, its layers, framing and sound) is how it was carried out. Passages of different groups
   that are each other's nearest say the same thing (two briefs' calls to action) and are joined into one ask; what
   their making shares, each part with its strength and how common it is in shots at large, is the videoform's
   pattern, which a new job's brief is planned from (`follow --brief`). What only one group asked goes to its
   brand's fingerprint. The person's AI reads the rest from `raea videoform docs NAME` into briefs.ts (a never, an
   order, a choice), and each rule is measured on its group's videos.
7. **The person confirms the strongest rules once,** on a contact sheet ("countertop close-up in every kitchen: 9 of
   10"). Each yes or no is evidence in the videoform, as any label is in a fingerprint.

## Following it

- `raea context` prints the videoform: its template, its rules and its ranges.
- New raw footage is read the same way, and each shot is matched to the videoform's shot kinds.
- Before any cut, the agent is told what the footage lacks: "no close-up of the counters, which every kitchen in the
  references has". That's a shot list for the person.
- The agent plans the cut to the template, slot by slot, and edits to the ranges (cut points, pauses, shot lengths).

## Checking it, fast

1. **On the composition, before any render:** the videoform's rules (NAME.taste.ts: what every video has, how it
   opens and closes, what each section is made of and which comes first, a place's things with it, what follows a
   word said or shown), run by `raea check PROJECT` with the person's taste. The composition is read from its worlds
   as a reference is, each cut's picture from its footage's kept columns, and set beside the references' rows, so each
   rule is tested by the very test that mined it. Only the rules on how it's layered wait for a render (VF10).
2. **On the render manifest and measures, in seconds:** loudness, light and colour against the profile (`raea
   fingerprint score --profile`).
3. **On embeddings, only for shots that changed:** a cut's picture is pooled from its footage's columns, kept by
   content hash, so a shot is embedded once, when its footage is first read, however often it's checked, moved or
   recut (VF11).

The routing from raea-app-check-cause (PR #205) applies unchanged: the footage's own faults are notes, carried findings
say so, and a round goes only to what another render can change.

## Judging

Judging is for testing the build, set by set. The person's verdict is the judge; a second, automatic judge for the
test bench is deferred (see the end).

**The person's judgement (the one that decides).** For each test, raea shows the original and its own edit side by side,
unlabelled, in a random order (a feed artifact, or `raea show --question`). The person:
- picks which they'd post;
- scores pacing, shot choice, structure, graphics, sound and overall feel, 1 to 5;
- writes what's wrong in their own words.

Each verdict is kept in the test bench's record, and is also evidence in the videoform (a pick is a pair, FP6b; a note
is a label, FP5). The judging trains the videoform as well as grading it.

## The test sets

The person provides the sets: 8 to 10 videos each, from one creator or one format, with raw footage for a few where it
exists. Every test reads a set's references once, then runs only on what the videoform kept, so the tests also prove
that a videoform is enough without its references. For each set:
- **Leave one out:** a videoform built from all but one reference must hold the one left out (its rules pass, its
  measures fall in range) more often than other sets' videos do.
- **Tells sets apart:** each videoform must reject other sets' videos. This is profile FP6e's test, extended to rules
  and structure.
- **Edit to it:** where raw footage for a held-out reference exists, the agent edits it to the videoform, and the cut is
  set beside the original.
- **Judged:** by the person, as above.

The sets, their answers and their verdicts are kept as fixtures, so each step of the build is measured on all of them.

## Build order, with confidence

| Phase | Step | What | Confidence | Why |
|---|---|---|---|---|
| 1, in | VF1 | ingest from the local drive, and streamed from direct links, platform pages, playlists and channels; read once, nothing kept but the readings; the rights prompt, once per account | high, medium for platform pages |
| | VF1b | the resolver: yt-dlp fetched and checked against its sums, pinned per release, moved forward daily, updated and retried when a site breaks, rolled back when the new one is worse, a nightly site check | high to build, medium to keep | the sites change; the update loop is what keeps it working | decoding over the network is standard; platform pages depend on the resolver keeping up with the sites |
| | VF2 | `videoform` in `fingerprint.ts`, found, inherited and taught as a fingerprint; `raea videoform new` and `add` | high | FP14's machinery exists |
| | VF3 | reading references in one pass: perception, columns, words, pauses, beats, stems, OCR, depth, into the kept rows | high | every reader exists; one decode is already done for raea check |
| | VF3b | what a videoform keeps, its sizes and times, and the proof that following and checking never open a reference | high | the rows are Fingerprint's format; the test sets prove it |
| 2, read | VF4 | ranges into the videoform's profile, measures added (pauses, words a minute, shot lengths, captions, cuts against beats) | high | FP6e computes ranges and leave-one-out already |
| | VF5 | the vocabulary loop with the person's AI, as word heads | medium-high | word heads already score from text; the report is new |
| | VF6 | the shot vocabulary (clusters named from the vocabulary) | medium | clustering is plain; naming quality varies by domain |
| | VF7 | the structure template | medium | alignment across varied videos is the hard part |
| | VF8 | event rules into taste lines, with strength, confirmed by the person | medium | co-occurrence is plain; telling habit from style needs the person |
| | VF8b | when it doesn't hold together: too few, mixed kinds (split into videoforms), traits that vary, outliers | medium-high | groups and spreads come from the kept rows |
| | VF8c | adding and removing references from kept rows; encoder pins and `refresh` | high | Fingerprint keeps evidence across upgrades the same way (FP8c) |
| | VF8f | what was asked and how it was made: documents and the person's words with each group, found where they're carried out, joined across groups, briefs.ts's rules measured, a new brief planned from them | medium | finding a passage by what's said and shown is sound; one by what's seen only is weak (pooled pictures, VF5); two groups is a thin base for a pattern |
| 3, use | VF9 | following: context, coverage gaps, planning to the template | medium-high | context and scripts exist |
| | VF10 | composition-first checks for the videoform's rules | high | rules as `.ts` checks over the world exist (DR4) |
| | VF11 | embedding checks on changed shots | medium | per-shot SigLIP is cheap; thresholds need the test sets |
| 4, judge | VF12 | the test bench: the sets as fixtures, leave-one-out and telling sets apart, run after every step | high | the measures are defined above |
| | VF13 | the person's blind judging, kept as evidence | high to build | an artifact and the evidence rows exist |

**Where it stands (11-25):** VF1 to VF11 are built; VF12 and VF13, the bench and the blind judging, are next. What
ships without its models (stems, voice activity) and the pause between sections are deferred, below.

**The first proof that matters is VF4 with VF12.** On at least five of the person's sets, each videoform, built leave
one out, holds its left-out video's pacing and shot lengths, and rejects other sets' videos, more often than chance by
a clear margin. If the numbers alone can't tell videoforms apart, nothing built above them will.

## Deferred

- **An automatic judge for the test bench:** a video-capable language model, paid for by the person, in the bench only
  (never shipped), answering fidelity, pairs and gaps (what the references show that the videoform doesn't hold),
  calibrated against the person's verdicts.
- **The order of the test sets.**
- **Stems (HTDemucs) and voice activity (Silero) in a reference's reading,** once their models ship in a release.
- **The pause between sections,** measured where the structure cuts each reference.
