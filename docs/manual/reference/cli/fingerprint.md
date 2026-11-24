---
title: "raea fingerprint"
summary: "A person's sense of good work, learned on this Mac: axes with small heads over on-device encoders that score a render moment by moment and say what they can't judge yet."
order: 45
sources: ["apps/fingerprint/src/lib.rs", "systems/fingerprint/src/taxonomy.rs", "systems/fingerprint-things/src/things.rs", "apps/double/src/lib.rs"]
---

A fingerprint is one person's sense of good work, kept in `~/.raea/fingerprints/NAME`. It is a list of axes (blur,
flash, clipping, pace, a brand's colours) with a small head for each over encoders that run on this Mac: SigLIP for
pictures, CLAP for sound, YAMNet for sound events and EmbeddingGemma 2 for scripts and other text; each axis judges
only what a file has. A new axis judges from its words until it has been trained;
`bootstrap` trains the mechanical axes by damaging moments of your own renders, and `teach` adds a label at a moment
you point at. A person can keep several, each saying what it's for, and one can build on another with `--extends`.
The app's Fingerprint page shows the same thing: what each one knows, what it's still learning, and what it can't
judge yet. `postmortem` looks back on a finished project with at most ten questions, and `things` says where a
thing floats or passes through another, the camera lurches or the frame is empty, for `raea check --things`.
`score --ask flags` puts each flag to clef again, through `raea ask`, on that moment's window alone. Beside the flag it
says "clef agrees (0.96)" or "clef doesn't see it (0.00)". It costs by the flags, never by the piece, and a render
asked again costs nothing. Clef's answers are kept as its evidence (`eye:clef`), which trains the axis's head but never
counts as your own. See
[docs/fingerprint.md](https://github.com/shinyobjectz/raea/blob/main/docs/fingerprint.md) for the plan.

```sh
raea fingerprint new me
raea fingerprint new client-ads --about "Short ads for one brand" --extends me
raea fingerprint bootstrap renders/*.mp4 --fp client-ads
raea fingerprint score cut.mp4 --timeline
raea fingerprint score cut.mp4 --ask flags
raea fingerprint teach cut.mp4 --axis blur --at 4.2 --bad
raea fingerprint report
raea fingerprint postmortem projects/film
raea fingerprint things shot.mp4 --json > things.json
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea fingerprint new NAME [--who W] [--about TEXT] [--extends FP] [--kinds K,K] [--tags T,T] | raea fingerprint bootstrap FILE... [--from LIST] [--per N] | raea fingerprint teach FILE --axis A --at T [--bad|--good] [--span S] [--box x0,y0,x1,y1] [--words W] [--project P] | raea fingerprint approve FILE... [--project P] | raea fingerprint pair WORSE BETTER --axis A [--at T] [--then T2] | raea fingerprint versions V1 V2... | raea fingerprint profile REFS... --as NAME [--against LIST] | raea fingerprint score FILE [--was OLD] [--ask flags|unsure] [--json] [--sheet out.png] [--timeline] [--profile NAME] | raea fingerprint report [--json] | raea fingerprint which [PATH] [--json] | raea fingerprint use NAME [--in project|workspace] [PATH] | raea fingerprint diff A B [--json] | raea fingerprint share NAME [--in project|workspace] [PATH] | raea fingerprint reembed PLACE... | raea fingerprint read PROJECT [--by kimi|claude] | raea fingerprint readings [PROJECT] [--ask N] [--tally] | raea fingerprint learn-notes [PROJECT] | raea fingerprint move [--fp CHILD] [--to PARENT] [--axis A] [--source S] [--project P] | raea fingerprint postmortem [PROJECT] [--ask N] [--tally] | raea fingerprint things FILE [--project P] [--json] | raea fingerprint stop [--fp NAME]   (a person's sense of good work as axes with small heads over on-device encoders, docs/fingerprint.md: new makes ~/.raea/fingerprints/NAME with raea's base taxonomy in fingerprint.ts (or FP's axes with --extends), what it's for, the kinds of piece and project tags it judges, and every axis scoring from its words; bootstrap puts known defects into windows of real work and trains the mechanical axes, held out by source, and patch heads that say where in the frame a picture defect is; teach adds a label at a moment; approve makes approved work clean on every axis, away from its notes; pair says one moment is better than another on an axis (a pick, a hand edit); versions makes weak pairs of noted moments the next version changed; score prints the moments it expects a note on first (where in the frame, and the layers there, when its patch head points held out), then every axis per window with --json, a sheet PNG with --sheet, or the piece second by second in words with --timeline, and with --was OLD whether each of OLD's gating flags is fixed, still there, or looks disguised (nearer the axis's disguised defects than clean work), and with --ask flags each flag put to clef again on its own window, whether clef agrees said beside it and kept as clef's evidence (unsure: the moments it's unsure of too); profile takes the ranges of work you admire (loudness, tilt, cut rate, motion, brightness, contrast, saturation, sharpness), checked leave-one-out, and score --profile says what falls outside them, where and which way; report says each head's held-out score, labels, cheats, gate and what it can't judge; which says the one that judges a piece (its project's, its workspace's, then yours) and why, use pins one there, diff lists the axes two fingerprints rank the same evidence most differently on, and share copies one's taxonomy and evidence (no heads, no media) into a project or workspace for its team; reembed moves a fingerprint's evidence to this machine's encoders, re-embedding the rows whose media it finds; read has a fresh kimi or claude session read a project's pinned notes into kinds and axes, readings puts them to you to check and tallies your answers, and learn-notes makes the defects and praise evidence once you've agreed with 45 kinds and 40 axes in 50, each project's in the fingerprint that judges it; move takes a child's own labels (by axis, source or project) up into a fingerprint it extends, which never reads them otherwise; postmortem scores every version in a project's feed and asks you at most ten yes/no questions in the app, each with its moment playing, where the fingerprint disagrees with your labels or is least sure, and --tally makes your answers evidence and says how each head's held-out score moved; things finds a piece's shots, its things (a render's layers, and the marks drawn on its notes) with their depth, and the camera's path, and says where a thing floats, two pass through each other, the camera lurches mid-move or nothing is on screen, as JSON for `raea check --things` with --json)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--about` | `TEXT` | What this fingerprint is for, in a sentence, shown beside its name in the app |
| `--against` | `LIST` | With profile: a file listing unrelated work, one path a line, that the references must fit their own ranges better than |
| `--as` | `NAME` | With profile: the name the profile is kept under, for score --profile |
| `--ask` | `flags\|unsure` | With score: put each flag to clef again, on that moment's window alone, and say beside it whether clef agrees (`unsure`: also the moments an axis puts in its plausible band without flagging); the answers are kept as clef's evidence, never yours, and `ask: "flags"` in fingerprint.ts does it on every score. With readings (`--ask N`): put N of the readings to you in the app, one question each, to check before notes train; with postmortem (`--ask N`): ask at most N (10 at most) |
| `--at` | `T` | The moment a label is for, in seconds |
| `--axis` | `A` | The axis a label is for, by name, such as blur or clip |
| `--bad` | – | The moment has the problem the axis names (the default) |
| `--box` | `x0,y0,x1,y1` | Where in the frame the label is, as fractions of the width and height |
| `--by` | `kimi\|claude` | With read: which fresh session reads your notes, kimi (the default) or claude |
| `--extends` | `FP` | Another of your fingerprints to start from: the new one takes its axes and adds its own |
| `--fp` | `CHILD` | Which fingerprint to use, by name; without it, the default |
| `--from` | `LIST` | A file listing the renders to learn from, one path a line, relative to the file; lines starting with # are skipped |
| `--good` | – | The moment is fine on that axis |
| `--in` | `project\|workspace` | With use or share: the level the fingerprint is pinned or copied to, the project's or the workspace's folder |
| `--json` | – | Print the result as JSON |
| `--kinds` | `K,K` | With new: the kinds of piece it judges (film, ad, game, playable, still, sound), comma-separated |
| `--per` | `N` | How many moments to take from each render (10 if not given) |
| `--profile` | `NAME` | With score: also say which measures fall outside this profile's ranges, where, and which way to move |
| `--project` | `P` | The project the label came from, so evidence is held out by project; with move, only that project's labels |
| `--sheet` | `out.png` | Also write a contact sheet PNG of the moments it expects a note on |
| `--source` | `S` | With move: only labels from this source (note, teach, approval, pair, damage) |
| `--span` | `S` | How long the labelled moment lasts, in seconds |
| `--tags` | `T,T` | With new: the project tags it judges, comma-separated; a project picks the one matching its kind and tags best |
| `--tally` | – | With readings: count your answers so far against the readings asked, and say whether notes have earned training; with postmortem: make your answers evidence and say how each head moved |
| `--then` | `T2` | With pair: the moment in BETTER, when it isn't at the same time as --at in WORSE |
| `--timeline` | – | Describe the piece second by second in words |
| `--to` | `PARENT` | With move: the fingerprint the labels move up to, one this one extends (its nearest by default) |
| `--was` | `OLD` | With score: whether each of the earlier cut's gating flags is fixed, still there, or looks disguised (nearer the axis's disguised defects than clean work); exits 1 unless all are fixed |
| `--who` | `W` | Whose fingerprint it is, shown on its card |
| `--words` | `W` | The label's own words, as the note said them |

<!-- generated:end -->
