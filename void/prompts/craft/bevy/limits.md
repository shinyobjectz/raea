# Limits: how much a scene holds, and what isn't there yet

## Budgets

- **Meshes:** each mesh draws on its own (no batching), so a few hundred animated meshes are fine and thousands are
  not. For crowds, particles or debris, keep the count under about 300 and fake the rest with an ImagePlane or fog.
- **Distinct shapes** are built once each; reusing the same geometry and args is cheap, a new solid per part is not.
  Build Manifold solids at module level.
- **Shadows:** a directional light's are cheap. A shadow-casting PointLight renders six maps, a SpotLight one; keep
  shadowed point lights to one or two. At most 10 directional lights.
- **Length:** every animated value is stored per frame, and live play works the 3D out for every frame before it
  shows it. A 3D piece of a minute is fine; ten minutes of busy 3D starts slowly.
- **Models:** load before the first frame, at most 60 s; keep a `.glb` under about 20 MB.
- **Bloom** adds a float target the size of the frame; use it when something should glow, not by default.

## Rules that keep a render equal to its play

The router's seven, plus:

- Parts are matched across frames by `name`, else kind and shape in tree order: name what comes and goes.
- The first camera, and the first frame's Bloom, Fog and ToneMapping, are the piece's.
- A partly transparent page element over the 3D blends slightly differently in a render than in play; keep page
  parts over 3D opaque where it matters.

## Not there yet (don't reach for these; they won't render)

- Environment maps and skyboxes (image-based reflections): use lights and a backdrop mesh. Fully metallic surfaces
  look dark without them; keep `metalness` at 0.2-0.6.
- Normal, roughness and emissive texture maps; real glass (transmission, IOR): use `transparent` + low `opacity`.
- Ambient occlusion, depth of field, motion blur, vignette, colour grading in 3D (a CSS vignette on the page works).
- Orthographic cameras, several cameras, render layers.
- Picking (pointer on a mesh), physics engines, particle systems: do hit tests and motion in the game's step.
- Blending two model clips, morph-target control.
- Live 3D for a game a person plays (the stage shows 3D for scripted pieces only).
- Custom shaders; react-three-fiber's `useFrame`, refs, `<primitive>` and drei helpers: none of them exist here.

The full list, with what each would need, is `motion/docs/bevy-coverage.md` in raea.
