---
title: "raea next"
summary: "The two or three most useful next steps for a project, read from its brief, compositions, renders and checks."
order: 52
sources: ["apps/workspace/src/next.rs", "core/facts/src/record.rs"]
---

`raea next` reads a project's files (what its brief still needs, the storyboard its modes want, its compositions)
and its record of renders and checks, and prints the two or three steps worth doing next. It's a menu for the agent
or the person to pick from; nothing runs.

The record (`.raea/record/`, which every `raea render` and `raea check` of a file in the project writes) decides the
step after a composition exists. Never rendered: render stills first. Changed since its last render: render again.
Its last render found an error: fix it, naming the error and the render that found it. Otherwise the video is next,
then its `raea check`, then whatever that check found, and with nothing found, showing it to the person.
[`raea findings`](findings.md) lists what each render found.

```sh
raea next
raea next ~/Documents/raea/acme/projects/spring-launch
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea next [DIR]   (the two or three useful next steps for the project, from its brief, compositions, renders and checks)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

<!-- generated:end -->
