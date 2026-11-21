---
title: "raea scene"
summary: "Find floating, sunk and clipping 3D parts, say what each part rests on, and meter a shot's light."
order: 47
sources: ["runtime/export/src/scene.rs"]
---

`raea scene` steps a composition's frames and measures its 3D scene without rendering pictures. `lint` reports
parts that float, sink into what they stand on, clip into each other or tilt, and exits 1 on an error. `explain`
says what every part rests on. `light` meters the light on the shot's subject; given several compositions, it
compares their first and last frames across each cut.

```sh
raea scene lint room.tsx
raea scene explain room.tsx --frame 90 --json
raea scene light room.tsx --part hero
raea scene light film.tsx --composition shot1,shot2,shot3     # the light across the cuts
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea scene export EDIT.tsx -o DIR [--composition ID] [--frame N | --at A-B]   (the composition's 3D as data other tools build: DIR/scene.json, each part placed (y up, metres) with its shape, material or light, and the camera; meshes/ and models/ beside it)
raea scene lint|explain|light EDIT.tsx [--composition ID] [--frame N] [--part NAME] [--json]   (what each 3D part rests on: floating, sunk and clipping parts; light: the scene's lighting)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--at` | `A-B` | `export`: each part's placement and the camera's field of view on every frame from A to B, for keyframes in Blender or Godot. |
| `--composition` | `ID` | Which composition to measure. `light` takes several, comma-separated, to compare the light across their cuts. |
| `--frame` | `N` | The frame to measure (0 by default; `light` also takes `last`). |
| `--json` | – | Print as JSON: `lint` its findings (check, level, the parts they are about, frames, says, measure, fix), `explain` every part with its support, gap and box. |
| `--part` | `NAME` | With `light`: the part to meter, in place of the shot's subject. |
| `-o` | `DIR` | `export`: the folder scene.json is written to, with meshes/ and models/ beside it. |

<!-- generated:end -->

## See also

- [3D](../../guides/3d.md) covers building and checking a scene.
