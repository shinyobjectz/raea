---
title: "raea component"
summary: "Copy a ready-made component or set into the project's components/, or the workspace's shared/components/, to use and change as your own."
order: 26
sources: ["engine/cli/src/index.ts", "engine/cli/src/registry.gen.ts"]
---

Components are ready-made pieces of a composition (captions, lower thirds, end cards and the like) and sets of them.
`raea component list` shows them; `raea component add` copies one into `components/NAME/`, where it's yours: raea
never updates or overwrites it unasked. With `--shared` it goes into the workspace's `shared/components/` instead, for
every project to import as `@shared/components/NAME`. `raea part` is the old name and still works.

```sh
raea component list
raea component add captions end-card
raea component add reel --shared
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea component list | raea component add NAME ... [--shared] [--force] | raea part list | raea part add NAME ... [--shared] [--force]   (a ready component or set copied into components/, or the workspace's shared/components/ with --shared, yours to change; raea part is its old name)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--force` | – | Replace a component's files that are already there. Without it, an existing component is left alone and the command stops. |
| `--shared` | – | Copy into the workspace's `shared/components/` instead, where every project in the workspace imports it as `/components/NAME`. |

<!-- generated:end -->
