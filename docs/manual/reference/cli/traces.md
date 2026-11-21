---
title: "raea traces"
summary: "List the sessions raea apps left, newest first, with how each ended and its glitches, or tell one in full."
order: 54
sources: ["engine/cli/src/cli.rs", "io/trace/src"]
---

Every raea app records its session: how it ended, its glitches and errors, its slowest frames and its memory.
`raea traces` lists the last 40, newest first. `raea traces show` tells one in full, such as why it crashed. Attach
it to a bug report.

```sh
raea traces
raea traces show            # the newest
raea traces show 3          # the fourth in the list
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea traces [show [N|last|DIR]]   (the sessions raea left, newest first: how each ended, its glitches; show tells one in full)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

<!-- generated:end -->
