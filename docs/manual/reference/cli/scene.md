---
title: "raea scene"
summary: "Find floating, sunk and clipping 3D parts, say what each part rests on, meter a shot's light, and hold every frame to the stage checks a render runs."
order: 47
sources: ["apps/scene/src/lib.rs", "apps/scene/src/light.rs", "apps/scene/src/stage.rs", "systems/stage/src/lib.rs", "systems/stage/src/clearance.rs", "systems/contact/src/lint.rs", "systems/contact/src/zfight.rs"]
---

`raea scene` steps a composition's frames and measures its 3D scene without rendering pictures. `lint` reports
parts that float, sink into what they stand on, clip into each other or tilt, and exits 1 on an error. A float or
sink is measured at the part's feet (rays down from its lowest points), not its box, so a stool whose box clips a
table's edge but whose feet are on the floor rests. A clip comes with the move that clears it (the part, the axis,
how far) and, when the move is sideways, where it settles once moved. Faces of two parts lying in one plane over each
other (a rug's top in the floor's, which flickers as the camera moves) are an error from 1 cm² up. `explain`
says what every part rests on. `light` meters the light on the shot's subject; given several compositions, it
compares their first and last frames across each cut. `stage` steps every frame and holds it to the checks `raea
render` runs at its end (docs/stage-guards.feature): 3D words too small or turned too far to read, a reveal that
never starts or never finishes, a part whose size jumps in one frame, a camera that lurches or goes in and back out,
frames with almost nothing on screen. It exits 1 on an error, as the render would, without drawing a frame, so it's
the cheap look before one. RichText `read={false}` leaves a sign nobody is meant to read out of it. `--clearance M`
also holds the camera M metres off every solid part on every frame (0.45 for a carried camera, less for a rig): each
part it comes closer to, the frames and the nearest it gets, or that it's inside the part. Glass, decals, volumes and
parts under 5 cm are passed over.

```sh
raea scene lint room.tsx
raea scene explain room.tsx --frame 90 --json
raea scene light room.tsx --part hero
raea scene light film.tsx --composition shot1,shot2,shot3     # the light across the cuts
raea scene stage film.tsx --from 1800 --to 2400                # the stage checks on 20 s of it
raea scene stage walk.tsx --clearance 0.45                     # and the camera kept off the set
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea scene export EDIT.tsx -o DIR [--composition ID] [--frame N | --at A-B]   (the composition's 3D as data other tools build: DIR/scene.json, each part placed (y up, metres) with its shape, material or light, and the camera; meshes/ and models/ beside it)
raea scene lint|explain|light|stage EDIT.tsx [--composition ID] [--frame N] [--part NAME] [--from N] [--to N] [--clearance M] [--json]   (what each 3D part rests on: floating, sunk and clipping parts; light: the scene's lighting)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--at` | `A-B` | `export`: each part's placement and the camera's field of view on every frame from A to B, for keyframes in Blender or Godot. |
| `--clearance` | `M` | `stage`: every run of frames where the camera is nearer a solid part than M metres (0.45 carried, 0.15 on a rig). |
| `--composition` | `ID` | Which composition to measure. `light` takes several, comma-separated, to compare the light across their cuts. |
| `--frame` | `N` | The frame to measure (0 by default; `light` also takes `last`). |
| `--from` | `N` | With `stage`: the first frame held to the checks (0 when left out). Every frame before it is still stepped, so a scene that builds over time is as it would render. |
| `--json` | – | Print as JSON: `lint` its findings (check, level, the parts they are about, frames, says, measure, fix), `explain` every part with its support, gap and box. |
| `--part` | `NAME` | With `light`: the part to meter, in place of the shot's subject. |
| `--to` | `N` | With `stage`: the last frame held to the checks (the composition's last when left out). |
| `-o` | `DIR` | `export`: the folder scene.json is written to, with meshes/ and models/ beside it. |

<!-- generated:end -->

## See also

- [3D](../../guides/3d.md) covers building and checking a scene.
