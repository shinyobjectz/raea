# Where raea's engine is going

raea plans its engine work in feature files kept outside this repository. This page summarizes each area so you can
see what is being built and why. Issues about any of it are welcome (CONTRIBUTING.md).

## Bevy is the only engine

Bevy schedules, stores and renders everything. React and TypeScript run on JavaScriptCore inside a Bevy app and
render as Bevy entities and systems. There is no second engine or browser beside it. raea is on Bevy 0.20.

## Everything Bevy does, from TypeScript

Anything you can do in Bevy should be possible from raea's TypeScript SDK: components, materials, lights, cameras,
post-processing, assets and ECS worlds. Each gap is written as a small headless case first, so it closes only when that
case renders correctly. Work in progress:

- **Full control without house conventions.** Custom shaders, complex lighting strategies, instancing and live
  control of games, each measured against Bevy's own examples rendered by Bevy.
- **Shaders, materials, textures and frame passes.** These work the way Bevy has them: WGSL and GLSL, material
  extensions, render targets and passes.
- **The plugin catalog.** Third-party Bevy plugins are declared, typed and usable from TSX through the same SDK.

## One SDK, laid out like Bevy and written like React

The SDK follows Bevy's taxonomy (`raea/3d`, `raea/ui` and so on), so people and agents who know Bevy find things where
they expect them. Authoring has no split between 2D and 3D, or between CPU and GPU. Recipes such as paint looks and
retro shaders live in `examples/`, not in the engine, and import only `raea` and `raea/<part>`.

## HTML, CSS and React at browser parity

Blitz runs inside Bevy. It resolves CSS with Stylo and lays the page out with Taffy, and Vello paints it on Bevy's GPU
device. One bridge joins the page to the world, so any element can become a layer in 3D. Parity is measured, not
judged: the same TSX is rendered by Chrome as the reference and by raea, then compared pixel by pixel. Full React
parity with the browser is a long-term goal, not a promise of the canary releases.

## Rendering: ray tracing and denoising

raea is adding Bevy's Solari path tracer and MetalFX denoising and upscaling on the Mac. It also has an optional
integration for MLX-DLSS, whose weights are the user's own and never ship with raea. All of these need motion vectors
and depth from raea's render, so those come first.

## Characters

There is one character skeleton, MakeHuman's game_engine rig, used natively by the character SDK for clips, retargeting,
foot placement, motion matching, grasping and ragdolls.

## Targets and ports

An app targets the web or native by a setting, with one SDK. Releases build for macOS and Linux. A Windows build is in
progress.

## Rune as a second UI host

raea's React-to-host op wire is not tied to a framework. Rune (a native GPU UI framework in Rust) is being tried as a
second host beside Blitz, for native apps, video and layers in Bevy.

## Skills that stay true

The public agent skill (`raea`, in raea-skills) and the craft guides are generated from the code where they can be and
checked against it where they can't, so they don't drift from the engine.

## Releases

Releases are built from a tag on GitHub's runners with attested provenance, and the binaries are attached to the
GitHub release.
