---
title: "raea traces"
summary: "List the sessions raea apps left, newest first, with how each ended and its glitches, or tell one in full."
order: 55
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

## What a run measures

A run can also time each of Bevy's render passes, count what the engine does, and turn on raea's own traces. Each
of these is a stat channel, and you choose channels from your composition, so you don't need a rebuild or a variable.
Choose them at the file's top level, so they're on before the engine opens:

```tsx
import { native } from "@raea/app/native";

native("stats.on", { channels: ["bevy_passes", "trace"] });   // or ["all"]
```

`bevy_passes` times every render pass Bevy runs. Every other channel is one of raea's `RAEA_*_TRACE` or `*_STATS`
switches, named without `RAEA_` and in lower case: `bake_trace`, `seedling_trace`, `media_stats`. Setting the variable
itself still works and overrides the composition, and `RAEA_STATS=bevy_passes,trace` chooses channels for a run without
touching the file.

Debug views are channels too, and a view that takes a value takes it after `=`:

```tsx
native("stats.on", { channels: ["pass=depth"] });        // the depth pass instead of the picture (or normal, albedo, sun, sky)
native("stats.on", { channels: ["lod_debug", "fog_debug", "volume_debug"] });
```

While the run goes, `native("stats.read", {})` gives what it has measured so far: each timed pass's runs and its mean
and worst milliseconds, and the counters (`engine/draws`, `engine/updates`; two updates a draw means each frame was
drawn twice). A render works out its frames ahead of the engine, so a frame's code can read the totals from before that
frame was drawn. When a run that chose any channel ends, it prints the table and keeps it as `stats.json` in its session
folder, which is the folder `raea traces show` reads.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea traces [show [N|last|DIR]]   (the sessions raea left, newest first: how each ended, its glitches; show tells one in full)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

<!-- generated:end -->
