---
title: "Fingerprint: teach raea your eye"
summary: "What raea learns about good work from your notes, marks and approvals, on your Mac: the axes it judges, the Fingerprint page, and what it can't judge yet."
order: 0
sources: ["docs/fingerprint.feature", "docs/fingerprint.md", "apps/fingerprint/src", "systems/fingerprint/src", "systems/fingerprint-use/src", "apps/app/src/fingerprint"]
---

A fingerprint is your sense of good work, as raea has learned it on your Mac. It's a list of axes, the things you
notice: blur, a black frame, hiss, clipping, a glow over everything, a frozen picture, and taste axes such as fake,
barren or unreadable text. Each axis has a small model over encoders that run on your Mac (SigLIP 2 for pictures, CLAP
for sound, YAMNet for sound events, EmbeddingGemma 2 for scripts and other text), so nothing leaves the machine unless
you share it.

Your agents use it before they post a cut: `raea fingerprint score` lists the moments your fingerprint expects a note
on, each named in your words, and the agent fixes them or tells you why it didn't. It tells you the axis and the
moment, never a number.

## In this section

- [Train your fingerprint](train.md): learning from your own renders, your notes and marks, a box on the part of a
  sound you mean, and a post-mortem when a project is done.
- [Check a cut with it](check.md): scoring a cut, checking a fix, scripts and stills, things in a shot, and profiles
  of work you admire.
- [Projects and teams](projects.md): one fingerprint for each kind of work, which one judges a project, keeping one
  with a project or workspace, and sharing with a team.

## The Fingerprint page

Open **Fingerprint** in the sidebar. The card at the top is your mark, one column of bits for each axis, grouped by
sense: pictures, sound and motion. Point at a sense to see its axes and how well each one judges. Below it, every axis
says whether it **knows** (it's trusted, and can hold a post back until you've looked), is **learning** (it gives
advice), or is still **to learn** (it only has its words). **Can't judge yet** lists what no axis can see, such as
whether a piece works as a whole.

The name in the page's title is a menu of your fingerprints. **Settings** holds what one is for, what it builds on,
whether it's your default, and **Learn from renders**.

A fingerprint's axes are a file you can read and edit, `~/.raea/fingerprints/NAME/fingerprint.ts`, typed by
`raea/fingerprint` ([the SDK page](../reference/sdk/fingerprint.md)). Add, remove or reword an axis there; its
evidence is raea's to keep. Every command is on [the CLI page](../reference/cli/fingerprint.md).

## What it can't do yet

Axes judge two-second moments, so a fingerprint can't say whether a piece works as a whole, or judge story and pace
across shots. It can say where in the frame only for blur, glitches and flat, barren surfaces; glow it can only
flag. Sound axes judge the whole sound in each moment, so a box's pitch band is kept but not yet learned from. A
stylised look or a held shot can read as a defect. And taste axes with only words never hold a post back.
