---
title: "Projects and teams"
summary: "One fingerprint for each kind of work, which one judges a project and why, keeping one with a project or its workspace from the app, and sharing one with a team."
order: 3
sources: ["docs/fingerprint.feature", "apps/fingerprint/src", "apps/app/src/fingerprint", "apps/app/host/fingerprint.ts"]
---

## One for each kind of work

You can keep several fingerprints, each saying what it's for. One can build on another: a client's ads can start from
your own and add what that client cares about. It inherits your axes and evidence, counts its own labels twice, and
never changes yours.

```sh
raea fingerprint new client-ads --about "Short ads for one brand: pace, type on screen" --extends me
```

## Which one judges a project

Which one judges a piece is decided by where it lives: the project's fingerprints first, then the workspace's, then
your own. At each level, the one whose `applies` matches the piece's kind wins, else that level's default.

In the app, a project's feed names it beside the project's name at the top: the fingerprint chip. Open it to see which
fingerprint judges the project and why. When it's one of yours, **Open** takes you to its Fingerprint page. The chip
shows once you have a fingerprint.

From a terminal:

```sh
raea fingerprint which projects/brand/out/cut.mp4          # which one judges this, and why
```

## Keep one with a project

The chip's menu has **Keep with this project** and **Keep with the workspace**. Pick one of your fingerprints under
either and raea copies it into that folder and commits it, so it travels with the folder's repository: everyone on
the team judges the project with the same taste, and the chip names it from then on.

The same from a terminal:

```sh
raea fingerprint share client-ads --in project projects/brand   # copy it into the project
raea fingerprint use client-ads --in project projects/brand     # and make it the one that judges there
```

Use `--in workspace` for every project in the workspace.

## Sharing with a team

Sharing copies the axes and the evidence (embeddings, spans and labels) into the workspace or project, to travel with
its repository. It never shares media or trained weights: each teammate's Mac trains its own from everyone's
evidence, and each person's evidence sits in its own folder so two people labelling at once never edit the same
file. When raea's encoders change, `raea fingerprint reembed FOLDER` moves the evidence to the new ones wherever it
finds the media.

```sh
raea fingerprint diff me ana                                # where two tastes differ, and the moments that show it
```

## Notes teach the one that judged

Your notes and marks on a piece teach the fingerprint that judged it, so a client's notes stay with the client's
fingerprint. When one of its lessons is really yours, move it up:

```sh
raea fingerprint move --fp client-ads --axis glow            # its glow labels become yours (me)
```
