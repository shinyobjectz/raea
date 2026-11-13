---
title: "raea docs"
summary: "What raea is, and a map of every page in these docs, so an agent reading only this page knows where everything is."
order: 0
sources: ["docs/manual"]
---

raea is the Render Anything Engine. You describe a video, a motion graphic, a 3D scene, a game or an app in
TypeScript and React, and raea draws it on the GPU with its own engine, built on the Bevy game engine. The same file
renders to an MP4, plays live as a game, or renders single frames for you to look at. It runs on your own computer
and needs no account.

These docs are written for coding agents first and for developers second. Every page is plain Markdown: add `.md` to
any page's address to get its source, or read [llms.txt](https://raea.io/llms.txt) for the list of pages. Offline,
`raea skill` and `raea guide` print the skill and the craft guides that match the raea you have installed.

```sh
curl -fsSL https://raea.io/install | sh     # macOS (Apple Silicon) or Linux (x86_64)
raea --version                              # the version these docs should match
```

## If you are an agent

Read [How an agent should use raea](start/agents.md) first. It gives the order of work: read the project, write the
composition, check it in a second, render stills and look at them, render the export and check it, then hand it over.
The raea skill (`raea skill` prints it) is the short version of the same loop; these docs are the long version.

## Start

- [Install raea](start/install.md): install, update, pick a version, and what goes where.
- [Your first video](start/first-video.md): a composition from an empty folder to an MP4, in five minutes.
- [Your first game](start/first-game.md): a game that plays live and renders the same every time.
- [How an agent should use raea](start/agents.md): the loop, what runs headless, and what waits on a person.

## Concepts

- [Compositions and frames](concepts/compositions.md): a composition is a function of the frame, and why that matters.
- [Rendering and checking](concepts/rendering.md): stills, exports, quality, and the checks that catch defects.
- [Workspaces and projects](concepts/workspaces.md): where work lives, and the spec files an agent reads first.

## Guides

- [Captions and cues on words](guides/words.md): cue anything on the moment a phrase is said, and build captions.
- [3D scenes](guides/3d.md): cameras, lights, meshes and materials in TSX, drawn by Bevy.
- [Apps on the web target or native](guides/apps.md): a React file that runs as a desktop app.

## Fingerprint

- [Fingerprint: teach raea your eye](fingerprint/index.md): what raea learns about good work from your notes, on your Mac.
- [Train your fingerprint](fingerprint/train.md): your renders, notes and marks, and a box on the part of a sound you mean.
- [Check a cut with it](fingerprint/check.md): score a cut before it's posted, and check that a fix really fixed it.
- [Projects and teams](fingerprint/projects.md): which fingerprint judges a project, and keeping one with it for the team.

## Reference

- [The CLI](reference/cli.md): a page for each of the 43 `raea` verbs, with its forms, where it runs and what each flag does.
- [The SDK](reference/sdk.md): a page for each `raea` import path, with every export's signature, props and examples.
- [Files and specs](reference/files.md): a workspace's and a project's files, and each spec's fields.
- [Settings](reference/settings.md): every setting raea reads from the environment or `~/.raea/env`.

## What's new

- [What's new](whats-new/index.md): every release, newest first, with what changed and what is still missing.
