---
title: "SDK"
summary: "Every raea import path: what it is for, and each component, hook, function, class and type it exports."
order: 1
sources: ["sdk/raea/src", "tools/manual-ref/sdk.ts"]
---

A composition, a game, an app or a spec is TypeScript that imports from `raea` and its sub-paths. Each page here
covers one import path: every export with its doc comment, its signature, a component's props with their types, a
class's members, the Bevy type it stands for when there is one, and the files that use it. Those files are Bevy's
own examples ported to raea, each checked against Bevy pixel for pixel, and the manual's examples, each rendered in
CI, so you can copy from them.

The pages are generated from the SDK by the TypeScript compiler, so they say what the code says. Where the SDK
mirrors a Bevy type and has no comment of its own, the page shows Bevy's doc for it, marked as Bevy's.

## Which path to import from

- `raea` holds what nearly every composition needs: `Composition`, `registerRoot`, `Sequence`, `AbsoluteFill`,
  `useCurrentFrame`, `interpolate`, `spring`, `Video`, `Audio` and the word and song helpers.
- The sub-paths follow Bevy's crates: `raea/camera`, `raea/light`, `raea/pbr`, `raea/mesh`, `raea/transform`,
  `raea/post_process`, `raea/sprite`, `raea/ui` and the rest, so a Bevy example's imports map onto them.
- `raea/3d` holds raea's scene layer: models, physics, splats, matchmoves and custom shader passes.
- `raea/brief` holds the specs: `brief`, `script`, `taste`, `plan` and `controller`.

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

## Start here

The composition, the app, and the specs and notes an agent writes.

| Import | What it's for | Exports |
| --- | --- | --- |
| [`raea`](raea.md) | The main entry: compositions, sequences, time, interpolation, springs, footage, sound and words. | 20 components, 8 hooks, 62 functions, 11 values, 98 types |
| [`raea/app`](app.md) | The App, its plugins and schedules, and the registry a video's compositions are kept in. | 1 component, 4 functions, 3 classes, 17 values, 8 types |
| [`raea/brief`](brief.md) | The specs an agent writes as data: brief, script, taste, plan and controller, checked against raea's schemas. | 6 functions, 32 types |
| [`raea/note`](note.md) | A person's note as a file in the project: their words kept as written, and the asks the agent restates it as. | 1 function, 4 types |

## Time and motion

Frames, animation, springs, states and input, and holding a frame until it's ready.

| Import | What it's for | Exports |
| --- | --- | --- |
| [`raea/time`](time.md) | The frame being drawn and the composition's time: useCurrentFrame, useVideoConfig and the Clock. | 1 component, 3 hooks, 7 types |
| [`raea/animation`](animation.md) | Declared animation: named moves, staggers, sequences and timelines, and a model's own clips. | 2 components, 1 hook, 27 functions, 2 values, 18 types |
| [`raea/math`](math.md) | Numbers through time: interpolation, easing, springs, curves and the vector types Bevy uses. | 7 functions, 3 values, 7 types |
| [`raea/state`](state.md) | State machines stepped by frame, for game logic and workflows, typed so a wrong transition doesn't compile. | 2 hooks, 6 functions, 1 value, 20 types |
| [`raea/input`](input.md) | The frame's input: keys, pointer, wheel and gamepads, the same live and in a scripted render. | 1 hook, 2 functions, 2 types |
| [`raea/hold`](hold.md) | Holding a frame until it's ready: delayRender, continueRender and cancelRender. | 3 functions, 1 value |

## Video, sound and text

Footage, pictures, sound, words and colour, and the timeline they are edited on.

| Import | What it's for | Exports |
| --- | --- | --- |
| [`raea/video`](video.md) | Footage and time-shifted sequences: Video, Sequence, Series, Loop and Freeze. | 5 components, 7 types |
| [`raea/audio`](audio.md) | Sound: Audio, the master Mix, speech, sound in 3D space, and a song's energy map. | 6 components, 1 hook, 7 functions, 1 value, 25 types |
| [`raea/text`](text.md) | Text in the 3D world, fonts, and time anchored to the words said. | 3 components, 1 hook, 3 functions, 6 types |
| [`raea/image`](image.md) | Pictures on the page and in the world, and texture atlases packed from them. | 3 components, 1 class, 5 types |
| [`raea/asset`](asset.md) | Files the project holds and assets made for it: staticFile, generated sound and music, and glTF bytes. | 9 functions, 1 value, 8 types |
| [`raea/color`](color.md) | Bevy's colour types: Srgba, LinearRgba, Hsla, Oklab, Oklch and the rest, with their conversions. | 2 functions, 7 classes |
| [`raea/editor`](editor.md) | The timeline the feed edits a film, a scene or a sound on, as data: tracks, clips, keys and markers, every edit an operation with its inverse, a film's scenes read in (fromRender) and its cut read back out as segments (segmentsOf). No React and no DOM: an agent's script, the app's host and a test use the same model the app's timeline does. | 63 functions, 28 values, 45 types |

## 3D

Bevy's 3D: cameras, lights, materials, meshes and the frame after it's drawn.

| Import | What it's for | Exports |
| --- | --- | --- |
| [`raea/3d`](3d.md) | raea's scene layer: models, physics, splats, matchmoves, instancing, custom shader passes and 3D text. | 70 components, 4 hooks, 26 functions, 3 classes, 12 values, 126 types |
| [`raea/camera`](camera.md) | Cameras and the views they make: Camera3d, the Canvas3D a 2D scene draws in, HDR and matchmoved cameras. | 5 components, 1 hook, 3 functions, 9 types |
| [`raea/light`](light.md) | Bevy's lights in Bevy's units, environments, skies and daylight. | 15 components, 9 functions, 2 values, 15 types |
| [`raea/pbr`](pbr.md) | Materials and volumes: StandardMaterial, extended and toon materials, custom shaders, decals, fog and fire. | 22 components, 1 function, 1 value, 35 types |
| [`raea/mesh`](mesh.md) | Shapes and solids: Mesh3d and Mesh2d, Bevy's primitives, buffer meshes, instancing and exact solids. | 14 components, 1 function, 2 classes, 10 values, 24 types |
| [`raea/transform`](transform.md) | Where things are: Bevy's Transform and Quat, and nesting. | 2 functions, 2 values, 4 types |
| [`raea/post_process`](post_process.md) | What's done to the frame after it's drawn: tone mapping, bloom, depth of field, motion blur, grading and outlines. | 12 components, 14 types |
| [`raea/picking`](picking.md) | What the pointer is over: the nearest part under it, and the ray it casts. | 2 functions, 3 types |
| [`raea/gizmos`](gizmos.md) | Lines drawn for looking: Bevy's gizmos and debug overlays. | 2 components, 1 function, 3 types |

## 2D and UI

Sprites and flat shapes, bevy_ui and Bevy's widgets.

| Import | What it's for | Exports |
| --- | --- | --- |
| [`raea/sprite`](sprite.md) | 2D pictures and flat shapes, drawn by Bevy's 2D renderer inside a Canvas3D. | 17 components, 1 class, 20 types |
| [`raea/ui`](ui.md) | bevy_ui: Node and its layout fields, Text, images, scrolling, gradients and units. | 20 components, 1 hook, 12 functions, 30 values, 37 types |
| [`raea/feathers`](feathers.md) | Bevy's feathers widgets: buttons, sliders, checkboxes, menus, inputs and colour pickers, in its dark theme. | 33 components, 2 hooks, 24 functions, 11 values, 27 types |

## Tools

Checks, lints and a headless App you can drive.

| Import | What it's for | Exports |
| --- | --- | --- |
| [`raea/check`](check.md) | A check written as a system over a composition's world: what `raea check` runs for a rule a controller holds. | 8 functions, 1 class, 38 types |
| [`raea/fingerprint`](fingerprint.md) | A fingerprint (fingerprint.ts, docs/fingerprint.md): a person's or a team's sense of good work as axes, each with a small head over on-device encoders that `raea fingerprint` trains from their labels. `raea fingerprint new NAME` writes one with raea's base taxonomy; add an axis, reword one, or remove one, and raea retrains. Rust reads the default export (systems/fingerprint/src/taxonomy.rs), so these types follow its fields. | 1 function, 4 types |
| [`raea/dev_tools`](dev_tools.md) | Checks and lints you can run from code: a motion performance's lint report. | 2 functions, 3 types |
| [`raea/remote`](remote.md) | A headless Bevy App you open, step and query from TypeScript, through Bevy's remote protocol. | 1 class, 1 type |

<!-- generated:end -->
