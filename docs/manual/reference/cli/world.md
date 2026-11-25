---
title: "raea world"
summary: "A frame as one world: its layers, text, 3D parts, camera, shots, clips, cues and lines as entities, queried across media."
order: 50
sources: ["apps/direction/src/world.rs", "runtime/world/src/lib.rs", "core/facts/src/components.rs"]
---

`raea world` reads a frame of a composition into one world of facts, as raea/ecs holds a world: entities with
components by name. React is stepped to the frame and nothing is drawn. The page is laid out (styles and layout,
no paint), so a run of text has its box, size, font and line count. Every medium is in the same world: the 3D
layer with its camera and parts (each measured from its triangles), the page's text, pictures and footage in their
stacking order, the Sequences as shots, the sounds as cues and the lines sung or said on the frame. Each entity
carries its Source: the file and line that wrote it and the component it was used in.

`--find` is one query across all of it: the entities with every component `with` names, none of those `without`
names, and each `where` field equal to its value.

What only a render knows joins the same world. The newest render of the composition in its project's record (or the
one `--render` names) adds each text's `Contrast` against the pixels behind it, the frame's and each shot's
`Picture` (light and movement), the `Mix`'s loudness and each cue's `Onset` in it, and, from a render with
`--layers`, which layers `Covers` which. Each is measured on the output the first time it's asked for and kept in the
render's manifest. The `Rendered` resource names the render, and says `stale` when the composition changed after it.
With no render, or on a frame the render didn't draw, the `Waits` resource says what waits for one.

```sh
raea world edit.tsx --frame 120
raea world edit.tsx --at 0-29 --json > world.json
raea world edit.tsx --frame 48 --find '{"with": ["Text", "Layer"], "where": {"Layer.medium": "2d-type"}}'
raea world edit.tsx --frame 48 --find '{"with": ["Part"], "where": {"Part.kind": "text3d"}}'   # the words that are 3D
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea world EDIT.tsx [--composition ID] [--frame N | --at A-B|A,B] [--find JSON] [--no-layout] [--render OUT | --no-render] [--json]   (a frame as one world: its layers, text, parts, camera, shots, clips, cues and lines as entities with components, each measured, with what its newest render measured (contrast, light and movement, loudness, onsets, covers) or what waits for one; --find '{"with": ["Text", "Layer"], "where": {"Layer.medium": "2d-type"}}' is one query across media)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--at` | `A-B\|A,B` | Several frames, each its own world: a range (`120-150`) or a list (`0,48,96`). Not with `--frame`. |
| `--composition` | `ID` | Which composition of the file to read, when it has more than one. |
| `--find` | `JSON` | Only the entities that match: `with` names components each must have, `without` those it must not, and `where` maps `Component.field` to the value it must equal. |
| `--frame` | `N` | The frame to read (0 when neither this nor `--at` is given). |
| `--json` | – | Print each frame as JSON: its size, fps and every entity with its id, parent and components. |
| `--no-layout` | – | Don't lay the page out. Faster, but text has no box, size or line count and carries `needs: "render"`. |
| `--no-render` | – | Join no render: the world as the composition says, with what a render would add listed in `Waits`. |
| `--render` | `OUT` | Join this render (an output of `raea render`, its manifest beside it) instead of the newest one in the project's record. |

<!-- generated:end -->

## Components

| Component | On | What it holds |
| --- | --- | --- |
| `Layer` | each composited layer | `medium` (`2d-type`, `2d-graphic`, `3d`, `footage`, `page`), `order` from the back, `opacity`, `blend` |
| `Text` | a run of type, 2D or 3D | `words`, `font`, `size` (px of a frame 1080 on its short side), `box` (0 to 1), `extruded` (metres), `lines` |
| `Part` | a 3D mesh or model | `kind`, `name`, `at`, `rotation`, `scale`, its world box (`min`, `max`), `triangles`, `src`, `material`, `color` |
| `Camera` | the frame's camera | `eye`, `look`, `fov` or `ortho`, `near`, `far`, `velocity` (m/s to the next frame) |
| `Shot` | a Sequence on the frame | `name`, `from`, `to` (composition frames); the shot around it is its parent |
| `Clip` | footage on the timeline | `source`, `hash` (SHA-256), `time`, `in`, `out`, `at`, `words` (TAKE.words.json's) |
| `Cue` | a sound on the timeline | `source`, `at` (seconds), `gain` (dB), `bus` |
| `Line` | a line sung or said now | `text`, `from`, `to`, `words` with their times (the project's words.json) |
| `Source` | whatever was written | `file`, `line`, `component`, `key` (the layer key) |
| `Contrast` | a run of 2D type, from a render | `ratio`: its colour against the pixels behind it (WCAG, 1 to 21), as `raea check` measures it |
| `Picture` | a shot, and the frame as a resource, from a render | `luma` (0 to 1), `motion` (the mean change from the frame before, 0 to 1; a shot leaves out its first frame, the cut into it), `frames` measured on a shot |
| `Onset` | a cue, from a video render | `at`: when the mix rises 6 dB where it starts, to within 10 dB of where it goes (seconds), `late_ms`; both empty when nothing starts there |
| `Covers` | a layer drawn over others, from a render with `--layers` | each layer it covers and the `share` of that layer's box |

Resources belong to the frame, not to one entity: `Mix` (the render's `lufs` and true `peak`), `Rendered` (its
output, manifest, record entry and whether it is `stale`) and `Waits` (each component a render would add, and what
it waits for).

A field raea can't know without a render says so: text from a page that wasn't laid out has `needs: "render"`, and
a `<Speech>` line has `needs: "transcript"` for its word times.
