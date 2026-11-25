---
title: "raea solid"
summary: "Measure, view and export a solid: parts built from code with exact volumes, joints and parameters."
order: 51
sources: ["apps/solid/src/lib.rs", "apps/solid/src/export.rs"]
---

A solid is a TypeScript file that builds 3D parts from shapes, with parameters you can change. `raea solid info`
measures each part: volume, bounding box, how many pieces, genus, and the joints between parts. It can compare the
result with what you expected, and show how each parameter changes the shape. `view` draws the parts from fixed
cameras, in colour and clay. `export` writes one `.glb` for a Blender step and reads it back to check it.

```sh
raea solid info chair.ts --json
raea solid info chair.ts --set legs=3,height=0.9          # with the change, and what it changed
raea solid info chair.ts --walk                           # every parameter at each end of its range
raea solid view chair.ts -o views/
raea solid export chair.ts -o chair.glb --part seat --colour '#c08040'
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea solid info FILE.ts [--json] [--set k=v,...] [--expect E.json] [--walk]   (volume, box, pieces, genus per part; joints; predictions and mutants)
raea solid view FILE.ts -o DIR [--set k=v,...] [--json]   (the solid's parts in colour and clay from fixed cameras)
raea solid export FILE.ts -o OUT.glb [--set k=v,...] [--part NAME] [--colour #RRGGBB]   (the solid as one .glb for a Blender step, read back and checked)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--colour` | `#RRGGBB` | With `export`: the colour the exported mesh carries. |
| `--expect` | `E.json` | What the parts should measure, checked against what they do. `FILE.expect.json` beside the solid is read by default. |
| `--json` | – | Print the measurements as JSON. |
| `--part` | `NAME` | With `export`: only this part. |
| `--set` | `k=v,...` | Change parameters for this run, as `name=value` pairs; `info` also reports what the change changed. |
| `--walk` | – | With `info`: set each parameter to each end of its range in turn and report what moves. |
| `-o` | `DIR` | Where `view` writes its sheets, or where `export` writes the `.glb`. |

<!-- generated:end -->
