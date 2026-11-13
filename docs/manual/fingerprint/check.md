---
title: "Check a cut with it"
summary: "Score a cut with your fingerprint before it's posted, check that a fix removed a flag instead of hiding it, judge scripts and stills, find things floating in a 3D shot, and compare a cut with work you admire."
order: 2
sources: ["docs/fingerprint.feature", "apps/fingerprint/src", "systems/fingerprint-use/src", "systems/fingerprint-things/src"]
---

## Score a cut

```sh
raea fingerprint score out/cut.mp4                     # the moments it expects a note on, first
raea fingerprint score out/cut.mp4 --sheet cut.png     # the same on a sheet of frames, with where boxed
raea fingerprint score out/cut.mp4 --timeline          # the piece second by second, in words
raea fingerprint which out/cut.mp4                     # which fingerprint judged it, and why
```

`score` names which fingerprint judged the piece, then lists the moments it expects a note on, each in your words. On
a picture axis that has learned where, a moment also says where in the frame and, on a render with a layer pass, the
layers there ("top right on Floor"). `--json` gives every axis for every window, for a script.

An axis that **knows** can hold a post back until you've looked. One that is **learning** only advises. Your agents
run `score` before they post, fix each moment that holds the cut back, or tell you why they didn't.

## Check a fix

```sh
raea fingerprint score out/v2.mp4 --was out/v1.mp4
```

For each moment that held v1 back, this says whether v2 fixed it, still has it, or looks **disguised**: nearer the
axis's disguised defects than clean work, so the fix hid the defect instead of removing its cause. Only a fixed flag
counts as fixed.

## Scripts, briefs and stills

A fingerprint reads more than video. A script, a brief or another text file gets EmbeddingGemma 2 rows, one per
paragraph, and a still gets the picture columns. Each axis judges only what the file has, so a script is never called
blurry. raea's own axes judge pictures, sound and motion. To judge writing, add an axis that reads text to your
fingerprint.ts and teach it like any other.

## Things in a shot

```sh
raea render shot.tsx --layers -o shot.mp4           # the video and its layers (shot.layers)
raea render shot.tsx --aov -o shot.aov              # its depth, beside it
raea fingerprint things shot.mp4 --json > things.json
raea check shot.mp4 --things things.json
```

`things` finds a piece's shots, the things in each (a render's layers, and anything marked on its notes) with their
depth, and the camera's path. It says where a thing floats, where two pass through each other, where the camera lurches
in the middle of a move, and where nothing is on screen. `raea check --things` turns those into findings beside the
rest. Floating needs real depth: with the render's `--aov` pass beside it (`shot.aov` for `shot.mp4`) it uses the
render's own depth, and without one it estimates depth from the picture, which can't always tell a hanging box from
one on the floor.

## Profiles of work you admire

```sh
raea fingerprint profile golden/*.mp4 --as reels --against others.txt
raea fingerprint score cut.mp4 --profile reels
```

A profile takes the ranges your references span: loudness, bass against treble, cuts a minute, motion, brightness,
contrast, saturation and sharpness. A cut scored against it hears which measures fall outside, where, and which way
to move. The profile checks itself: each reference has to fit the others' ranges more often than unrelated work does,
or raea says it doesn't tell this taste apart.
