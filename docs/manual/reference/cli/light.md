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

`raea light steady` asks one thing of one frame: does its light hold still? It draws frame `--frame` again and again
(`raea render --hold`), so nothing in the scene or the camera moves, and measures what still changes. Grain is a
pixel or two wide and new every frame. A blotch is tens of pixels wide and drifts over lit floors and reflections for a
quarter of a second or more: light built over frames that hasn't settled (on Solari the world cache, ReSTIR's reuse or the denoiser; `DIR/held/render.log` keeps what the render said). The two are told apart by size: the
deviation in time of each 32-pixel square's mean luminance (`blotch`, percent of the mean) against each 8-pixel
square's (`grain`). Grain alone gives a `spread` of 0.25, and blotches bring it toward 1. A run over `--max` fails,
`DIR/held/steady.png` shows where (white is 6%), and `steady.json` holds the numbers. A light whose level moves over the held frames fails too (`drift`, more than 0.05 of a stop between the first and the last quarter measured): a cache gaining or losing energy the longer it runs.

`raea light check` holds each scene's first still on the traced paths (`solari`, `gi`) the same way, reports it under `steady`, and fails when one moved.

```sh
raea light steady compositions/shots.tsx --composition court --frame 1188 -o out/steady/
raea light steady out/steady/held/        # frames already held, measured again
```

```sh
raea light check bench/light/scenes.json -o out/light/
raea light check bench/light/scenes.json -o out/light/ --scene kitchen --paths solari --spp 512 --fresh
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea light check SCENES.json -o DIR [--scene NAME] [--paths raster,traced-ao,solari,gi] [--spp N] [--fresh] | raea light steady EDIT.tsx --frame N [--composition ID] [--frames 96] [--run M] [--max 1.5] -o DIR | raea light steady DIR [--max 1.5]   (steady: frame N held, and whether its light holds still)   (each scene on each light path against its path-traced reference: distance in stops, the worst tile, a moving shot's flicker, a frame's cost; DIR/report.json)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--composition` | `ID` | `steady`: the composition of the file to hold. |
| `--frame` | `N` | `steady`: the frame to hold. The scene and the camera stay as they are at it. |
| `--frames` | `96` | `steady`: how many times the frame is drawn. The second half is measured, once the light has built. |
| `--fresh` | – | Make the references again, even where they're kept from an earlier check. |
| `--max` | `1.5` | `steady`: the blotch, in percent, a run may show before it fails. The worst tile may show three times it. |
| `--paths` | `raster,traced-ao,solari,gi` | The light paths to measure, comma-separated (all four by default). |
| `--run` | `M` | `steady`: also draws the frame cold and at the end of the M frames played up to it, and fails when their levels are more than 0.25 of a stop apart (`energy`). A cache that gains or loses energy over minutes shows here and not in a hold. |
| `--scene` | `NAME` | Check only the scene with this name. |
| `--spp` | `N` | How many times each reference frame is path-traced: its samples a pixel (256 by default). |
| `-o` | `DIR` | The folder for each scene's frames and report.json. |

<!-- generated:end -->
