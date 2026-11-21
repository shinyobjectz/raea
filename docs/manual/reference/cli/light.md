---
title: "raea light"
summary: "Measure a scene's light on each light path against its path-traced reference: distance in stops, the worst tile, flicker and cost."
order: 48
sources: ["engine/light/src/lib.rs", "engine/light/src/metric.rs", "engine/render/raytrace/src/force.rs"]
---

`raea light check` is the bar raea's lighting is held to (docs/realism.feature LT1). Each scene in a list is
rendered on each light path (raster as the composition has it; `traced-ao`, raster with traced occlusion in place of
SSAO on a camera with `<AmbientOcclusion>`; Solari; and `gi`, raster with raea GI's probes traced, its GPU
milliseconds a frame reported beside the frame's) and against its reference: the same frames path-traced, each
drawn `--spp` times so it converges. For each path it reports:

- how far the stills are from the reference, in stops: the mean, the bias (brighter is positive) and the worst
  64-pixel tile. It compares each 8x8-pixel square's mean luminance, so it measures the light, not antialiasing that
  differs by path or noise left in the reference;
- how much a short moving shot flickers against the same shot path-traced: each pixel's error in stops, and that
  error's second difference in time. Motion is in the reference too, so only light that changes when the truth
  doesn't counts. The frames' own second difference, in 8-bit steps, is beside it as `raw`;
- what a still costs to render, and what a frame costs live: the scene played headless and unthrottled for six
  seconds (`raea play --offscreen --no-vsync --3d`), as milliseconds a frame. The 3D draws at its composition's size
  whatever the stage's, so there's no half-size measure yet.

A reference is kept and used again while it's the same frames of the same composition at the same samples, until
`--fresh`.

The scenes are a JSON file: `{ "scenes": [{ "name", "dir", "file", "composition", "stills": [frames], "shot":
[first, last], "spp": N, "reference": false, "why": "...", "live": true, "size": [w, h] }] }`. `dir` is the scene's
project (renders run there), `file` its composition in it. A few frames make a shot, since each is path-traced.
`"spp"` sets the scene's reference samples in place of `--spp` (an interior lit by small lamps needs thousands). A
scene whose reference can't be made yet says so with `"reference": false` and why. `"live": true` plays it live as
well; `raea play` runs a file's first composition, so only a scene that's first in its file can be (`"live_why"` says
why another isn't). `"size"` is the live stage, 1920x1080 by default.

Any render takes the same switches: `RAEA_LIGHT=raster|solari|pathtracer` puts every 3D camera on that path,
whatever the composition asks, and `RAEA_LIGHT_SPP=N` draws each frame N times, the path tracer's accumulation started afresh on each frame.

```sh
raea light check bench/light/scenes.json -o out/light/
raea light check bench/light/scenes.json -o out/light/ --scene kitchen --paths solari --spp 512 --fresh
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea light check SCENES.json -o DIR [--scene NAME] [--paths raster,traced-ao,solari,gi] [--spp N] [--fresh]   (each scene on each light path against its path-traced reference: distance in stops, the worst tile, a moving shot's flicker, a frame's cost; DIR/report.json)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--fresh` | – | Make the references again, even where they're kept from an earlier check. |
| `--paths` | `raster,traced-ao,solari,gi` | The light paths to measure, comma-separated (all four by default). |
| `--scene` | `NAME` | Check only the scene with this name. |
| `--spp` | `N` | How many times each reference frame is path-traced: its samples a pixel (256 by default). |
| `-o` | `DIR` | The folder for each scene's frames and report.json. |

<!-- generated:end -->
