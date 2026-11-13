---
title: "Train your fingerprint"
summary: "Teach a fingerprint from your own renders, your notes, marks and approvals, a box on the part of a sound you mean, and a post-mortem when a project is done; and when it trusts an axis."
order: 1
sources: ["docs/fingerprint.feature", "apps/fingerprint/src", "systems/fingerprint/src", "apps/app/src/fingerprint"]
---

A new fingerprint judges from the words for each axis on day one. Everything after that is evidence: your own work,
and what you say about it.

## From your renders

To train the mechanical axes, choose **Learn from renders** in the Fingerprint page's Settings, or run:

```sh
raea fingerprint bootstrap ~/renders/*.mp4
```

raea damages moments of your own finished work on purpose (a blur, a black frame, a gap in the sound) and learns the
difference, held out by source, so it can't learn a shortcut. Each axis also has disguised versions of its defect, such
as a blur that was sharpened afterwards or a freeze hidden under a slow zoom. An axis that misses its disguises stays
advice.

The same run teaches picture axes where a defect sits. It blurs, punches a hole in, greys out or glows one patch of a
frame, and learns to find the patch. An axis that finds it 85 times in 100 on renders it didn't learn from says where:
`score` then gives the moment's place in the frame, such as "top right", and on a render with a layer pass the layers
there, such as "on Floor", and `--sheet` boxes it on the frame.

## From your reviewing

Your own reviewing teaches it the rest:

```sh
raea fingerprint teach cut.mp4 --axis blur --at 4.2 --bad      # a moment you marked
raea fingerprint approve final.mp4                              # approved work is clean, except where you left a note
raea fingerprint pair v1.mp4 v2.mp4 --axis glow                 # v2 is better on glow
raea fingerprint read projects/stay                             # read your pinned notes into labels
raea fingerprint readings --ask 50                              # check 50 of those readings in the app
raea fingerprint learn-notes projects/stay                      # once they read right, your notes become evidence
```

Notes become evidence only after you've checked how they were read: the kind right 45 times in 50 and the axis 40. A
note that asks for a creative change goes to the brief, never into an axis. Each project's notes teach the
fingerprint that judges that project ([Projects and teams](projects.md)).

## Point at what you mean

A note can say exactly what it's about. On the picture, drop a pin and mark, box or lasso the thing you mean.

On the sound, choose **Spectrogram** on the timeline's sound track (or open the sound in Listen) and, with the Comment
tool, drag a box around the part you mean, such as a hiss up high or a hum down low. A sound on its own shows its
spectrogram under the waveform, where any drag draws a box. The note keeps the box's time as its span and its pitch
band, your agent reads both with the note, and `learn-notes` labels that span and keeps the band in the label. Sound
axes still judge the whole sound in each moment, so the band is kept but not yet learned from.

## When an axis is trusted

An axis is trusted once it has 16 labels of each kind from 3 projects, ranks real defects above clean work on
projects it never saw, and catches its disguises. `raea fingerprint report` says where each one stands. An axis that is
still under its bar with 300 labels is **stuck**: more labels won't help it, so `report` names it as one to try on
another encoder.

## Looking back on a project

When a project is done, a post-mortem asks you what the fingerprint is least sure of:

```sh
raea fingerprint postmortem projects/film            # at most ten yes/no questions in the app, each with its moment
raea fingerprint postmortem projects/film --tally    # your answers become evidence; each head's score before and after
```

It scores every version in the project's feed and first asks where a head disagrees with a label you gave on that
moment, then where a head is nearest its flag, one question per axis within four seconds.
