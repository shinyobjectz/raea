---
title: "raea run"
summary: "Run a TypeScript program on the engine, headless, and print what it prints."
order: 8
sources: ["engine/cli/src/cli.rs"]
---

`raea run` bundles a TypeScript file and runs it on the engine's JavaScript, with no window and no composition. If
the file's default export is a function, it's called and awaited, and what it returns is printed: a string as it is,
anything else as JSON. The program's console goes to the terminal as node's does: `console.log` and `console.info`
to stdout, `console.warn` and `console.error` to stderr. A throw prints the error's name, message and stack, and ends
the run with exit code 1. It's how checks such as a room's `checkLayout` run, and it reaches the engine's native
calls (solids, ECS worlds) the way a composition does.

```ts
// count.ts: a line on stderr while it works, then its answer as JSON on stdout
export default async function () {
  console.error("counting")
  return { rooms: 3, errors: 0 }
}
```

```sh
raea run checks/layout.ts
raea run bake.ts --timeout 600
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea run SCRIPT.ts   (a TypeScript program on the engine, headless: its default export is called, what it prints is the answer; a check like a room's checkLayout; also: --timeout S)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--timeout` | `VALUE` | Stop the program after this many seconds (120 by default). |

<!-- generated:end -->
