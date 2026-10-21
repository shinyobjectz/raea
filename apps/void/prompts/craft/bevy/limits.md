# Limits: how much a scene holds, and what isn't there yet

## Budgets

- **Meshes:** each mesh draws on its own (no batching), so a few hundred animated meshes are fine and thousands are
  not. For crowds, particles or debris, keep the count under about 300 and fake the rest with an ImagePlane or fog.
- **Distinct shapes** are built once each; reusing the same geometry and args is cheap, a new solid per part is not.
  Build Manifold solids at module level.
- **Shadows:** a directional light's are cheap. A shadow-casting PointLight renders six maps, a SpotLight one; keep
  shadowed point lights to one or two. At most 10 directional lights. Shadow maps are 4096 by default (each
  directional light with shadows holds four of them, about 256 MB of GPU memory); `<Shadows mapSize={8192} />` costs
  about 2-3 ms a frame more at 1080p and four times the memory, so use it for a still or a hero shot only.
- **Length:** every animated value is stored per frame, and live play works the 3D out for every frame before it
  shows it. A 3D piece of a minute is fine; ten minutes of busy 3D starts slowly.
- **Models:** load before the first frame, at most 60 s; keep a `.glb` under about 20 MB.
- **Fire:** each `<Fire>` marches its pixels through its box (32 samples each by default), so it costs by screen
  area: about 1 ms a frame at 1920x1080 for a fire filling a third of the frame. A handful are fine; for a wall of
  flame use one wide fire, not dozens.
- **Splats:** a million gaussians draw at about 80 ms a frame at 1600x900 (12 fps live; renders are fine), 1.9 GB of
  memory; a trained scene of 100k-500k is the comfortable size. Each frame sorts every splat on the CPU (a few ms).
  Splats don't show in a live game's 3D yet (LiveScene), only in timelines and their play.
- **Bloom** adds a float target the size of the frame; use it when something should glow, not by default.
- **Ambient occlusion** costs about 0.3 ms a frame at 1920x1080 at "medium", 0.4 at "high", 1 at "ultra" (with the
  SMAA it brings).
- **Environment:** its light is filtered on the GPU every frame, about 4.5 ms a frame at 1920x1080 from a 1k panorama
  and 6.3 ms from a 2k one (an M5 Pro). Each panorama the piece uses is loaded before the first frame (about 0.1 s for
  a 1k `.hdr`, 0.3 s for a 2k); a 4k or 8k panorama is slow to load and gains little, its light being blurred anyway.
  Checked in renders and baked play; a live game's scene is untested with it.

## Rules that keep a render equal to its play

The router's eight, plus:

- Parts are matched across frames by `name`, else kind and shape in tree order: name what comes and goes.
- The first camera, and the first frame's Bloom, Fog, ToneMapping and Shadows, are the piece's.
- Overlapping FogVolumes are composited in the order of their rows (the engine pins it), so a render of any frame
  equals its play; give each a `name` so its row, and so the order, stays put when others come and go.
- A partly transparent page element over the 3D blends slightly differently in a render than in play; keep page
  parts over 3D opaque where it matters.

## Not there yet (don't reach for these; they won't render)

- Emissive texture maps; real glass (transmission, IOR): use `transparent` + low `opacity`. (Normal, roughness,
  metalness and AO maps work: `bevy/materials`.)
- A lens over the page: Fisheye and LensDistortion bend the 3D only. (Ambient occlusion, anti-aliasing modes, depth of
  field, a grade with a vignette, the lens and motion blur are in `bevy/post`.)
- `<Canvas3D layered>` in a render forced through page files (`RAEA_MOTION_3D=files`): there the page is drawn over
  the 3D.
- Orthographic cameras, several cameras, render layers.
- Picking (pointer on a mesh), physics engines, particle systems: do hit tests and motion in the game's step. (Fire is
  there: `<Fire>` in `bevy/post`.)
- Blending two model clips, morph-target control.
- A looked model (`look="pixel"`/`"site"`) is drawn over the scene: parts in front of it don't hide it. At most 30.
- Custom shaders (the looks are the engine's own); react-three-fiber's `useFrame`, refs, `<primitive>` and drei helpers: none of them exist here.

The full list, with what each would need, is `sdk/motion/docs/bevy-coverage.md` in raea.
