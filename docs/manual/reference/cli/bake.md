---
title: "raea bake"
summary: "Bake a composition's bounce light into lightmaps on raea's own ray tracer, and measure the bake against Cycles."
order: 104
sources: ["apps/lightbake/src/lib.rs", "apps/lightbake/src/compare.rs", "apps/lightbake/sets.json", "bench/bake/"]
---

`raea bake light` puts the light that bounces around a static set into lightmaps: the sky, emissive surfaces and
every bounce, so a shadow fills with the colour of what's around it, as in a path-traced still. The sun's direct light
stays live, so anything that moves still casts its shadow. The bake runs on raea's own ray tracer, with no Blender
(docs/bake.feature).

`raea bake light EDIT.tsx` bakes a composition's `<LightBake>` ahead of a render. It isn't built yet: it comes with
BK1-BK5, and says so until then. Today the bake is Cycles' through `raea blender run blender/lightbake.py`
(`raea guide bevy/blender`).

`raea bake light --compare DIR` measures bakes against Cycles on the test set, `apps/lightbake/sets.json`: the table
set from `raea guide bevy/realism`, a Cornell box lit only by an emissive panel, and a room lit through one window
(the compositions are in `bench/bake/`). For each set it makes the Cycles references, a still from blender/look.py and
a bake from blender/lightbake.py, both at `--samples`. Then it renders the set in raea, unbaked and with Cycles' maps,
and later with raea's own bake. Each frame is measured against the Cycles still, as RMSE (sRGB 0 to 1, every pixel)
and in stops (the mean and the worst 64-pixel tile, as `raea light check` measures). The report also gives
lightbake.py's time and peak memory, and whether the bar is met: on the table set, raea's bake within 0.045 RMSE of
Cycles, no mesh more than a third of a stop off on average, in under a quarter of lightbake.py's time.

The references are kept in DIR and used again until the set's compositions, its flags or the samples change, or until
`--fresh`. Cycles' maps go into `bench/bake/media/baked/`, which the `-cycles` compositions read and git ignores.
It needs Blender (`raea setup blender`).

```sh
raea bake light --compare out/bake/
raea bake light --compare out/bake/ --set table --samples 256
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea bake light EDIT.tsx [--composition ID] [--samples N] | raea bake light --compare DIR [--sets FILE] [--set NAME] [--samples N] [--fresh]   (bounce light baked into lightmaps on raea's ray tracer; --compare measures the test set against Cycles' bake and still, docs/bake.feature)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--compare` | `DIR` | Measure the test set against Cycles instead of baking: each set's frames, references and report.json in this folder. |
| `--composition` | `ID` | The composition to bake, by its id (the file's first by default). |
| `--fresh` | – | Make the Cycles references again, even where they're kept from an earlier run. |
| `--samples` | `N` | Samples a texel, for raea's bake and for both Cycles references (1024 by default). |
| `--set` | `NAME` | Measure only the set with this name. |
| `--sets` | `FILE` | The test set to measure (apps/lightbake/sets.json by default). |

<!-- generated:end -->
