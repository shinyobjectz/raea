---
title: "raea scene"
summary: "Find floating, sunk and clipping 3D parts, say what each part rests on, meter a shot's light, hold every frame to the stage checks a render runs, and list each part's planned detail beside how large the frames show it."
order: 47
sources: ["apps/scene/src/lib.rs", "apps/scene/src/light.rs", "apps/scene/src/stage.rs", "apps/scene/src/lod.rs", "runtime/lod-plan/src/lib.rs", "runtime/lod-plan/src/report.rs", "runtime/lod-plan/src/plan.rs", "systems/stage/src/lib.rs", "systems/stage/src/clearance.rs", "systems/contact/src/lint.rs", "systems/contact/src/zfight.rs"]
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

`lod` lists every part's planned detail beside what the frames show. A part's plan is how large it is meant to be
seen, which is how much of it gets built:

| lod | for | across the frame |
| --- | --- | --- |
| 0 | the hero | 50% or more |
| 1 | featured | 20% to 50% |
| 2 | set dressing | 8% to 20% |
| 3 | background: a grey box reads | 2% to 8% |
| 4 | far: a silhouette | under 2% |

The plan is the part's own `lod` prop (`<Mesh3d lod={3}>`, `<Model lod={4}>`, or a parent `<Entity transform lod>`
or `<Group lod>` for every part inside), else the `lod` of the object with the part's name in the job's plan (`make`
on the brief.ts above the composition). What a frame shows is how much of it the part's box spans through that
frame's camera, the larger of its width's and its height's share. The table puts parts seen closer than planned
first, then the rest of the planned parts, then the parts nobody planned by how large they get, so a part forgotten
since the plan was written is found by reading down it. Under it: each `lod-closer` finding, and any level the plan
gives a name that no part in the frames goes by. A part seen one level finer than planned is a warning; two or more
is an error, and `lod` exits 1. `raea render`, `raea scene stage` and `raea check --quick` say the same finding, and
`raea make status` holds a job at it. A part hidden behind other meshes isn't counted as seen; RAEA_LOD_CHECK=0
leaves the check out of a render.

```sh
raea scene lint room.tsx
raea scene explain room.tsx --frame 90 --json
raea scene light room.tsx --part hero
raea scene light film.tsx --composition shot1,shot2,shot3     # the light across the cuts
raea scene stage film.tsx --from 1800 --to 2400                # the stage checks on 20 s of it
raea scene stage walk.tsx --clearance 0.45                     # and the camera kept off the set
raea scene lod walk.tsx                                        # every part's planned detail against the frames
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea scene export EDIT.tsx -o DIR [--composition ID] [--frame N | --at A-B]   (the composition's 3D as data other tools build: DIR/scene.json, each part placed (y up, metres) with its shape, material or light, and the camera; meshes/ and models/ beside it)
raea scene lint|explain|light|stage|lod EDIT.tsx [--composition ID] [--frame N] [--part NAME] [--from N] [--to N] [--clearance M] [--json]   (what each 3D part rests on: floating, sunk and clipping parts; light: the scene's lighting)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--at` | `A-B` | `export`: each part's placement and the camera's field of view on every frame from A to B, for keyframes in Blender or Godot. |
| `--clearance` | `M` | `stage`: every run of frames where the camera is nearer a solid part than M metres (0.45 carried, 0.15 on a rig). |
| `--composition` | `ID` | Which composition to measure. `light` takes several, comma-separated, to compare the light across their cuts. |
| `--frame` | `N` | The frame to measure (0 by default; `light` also takes `last`). |
| `--from` | `N` | With `stage` and `lod`: the first frame held to the checks (0 when left out). Every frame before it is still stepped, so a scene that builds over time is as it would render. |
| `--json` | – | Print as JSON: `lint` its findings (check, level, the parts they are about, frames, says, measure, fix), `explain` every part with its support, gap and box, `lod` every part's row (planned, seen, largest, at, frames, closer), the plan's names no part goes by, and its findings. |
| `--part` | `NAME` | With `light`: the part to meter, in place of the shot's subject. |
| `--to` | `N` | With `stage` and `lod`: the last frame held to the checks (the composition's last when left out). |
| `-o` | `DIR` | `export`: the folder scene.json is written to, with meshes/ and models/ beside it. |

<!-- generated:end -->

## See also

- [3D](../../guides/3d.md) covers building and checking a scene.
