# 3D and games with Bevy, through @raea/motion/3d

You write 3D the way you write the rest of a composition: TSX components whose props are worked out from the frame.
raea's engine turns them into Bevy entities (meshes, lights, cameras, glTF models, post-processing) and draws them
under the page, so 2D parts (titles, captions, a score) sit over the 3D in the same composition. You never write Rust,
shaders or a render loop, and you never touch Bevy's API directly.

A render and a live play of the same file draw the same rows with the same code, so what you check in a render is what
plays. That holds only if every value comes from the frame (the rules below).

## Which file to read

Read one or two at a time with `raea craft bevy/NAME`, when the task reaches them. Two or three files cover most
pieces; then write the file. Each file's example is a complete composition you can start from.

| You are... | Read |
|---|---|
| setting up a scene: the Canvas3D, the camera, camera moves, groups, 2D over 3D | `bevy/scene` |
| lighting: sun, bulbs, spots, ambient, shadows, how bright | `bevy/lights` |
| surfaces: colour, metal, glass, glow, clearcoat, pictures on surfaces, toon (cel) shading and ink outlines, decals (labels, stains, tiles), a model's materials repainted | `bevy/materials` |
| interiors: rooms, cabinets along walls, windows and bays, chairs turned to tables, layout checks, wall decor, the garden outside, daylight, sounds placed in the scene | `bevy/rooms` |
| shapes: boxes to modelled solids, 3D letters | `bevy/meshes` |
| moving things: spins, springs, reveals, things arriving later, model clips | `bevy/animation` |
| a 3D game: state, input, many entities, collisions, state machines | `bevy/games` |
| glTF models, pictures in space, where files go | `bevy/assets` |
| a real scene or object from footage as gaussian splats, and animating them (assemble, disperse, morph...) | `bevy/splats` |
| liquids and goo: coffee dumped from a mug, a tap's stream, honey, things melting | `bevy/liquids` |
| bloom, fog, tone mapping, exposure, fisheye and lens distortion, motion blur | `bevy/post` |
| how much the scene can hold, what isn't there yet | `bevy/limits` |
| finding a model, rig, animation, a Bevy example's TSX, or a game-design pattern to build from | `bevy/library` |

## What exists

`Canvas3D`, `PerspectiveCamera {position, lookAt, fov}`, `Group`, `Mesh {name, position, rotation, scale}` holding
one geometry (`Box`, `Sphere`, `Plane`, `Cylinder`, `Cone`, `Torus`, `Text3D`, or `shape={manifold...}`) and one
material (`MeshStandardMaterial`, `MeshPhysicalMaterial`, `MeshBasicMaterial`, `MeshToonMaterial` with ink `outline`),
`AmbientLight`, `DirectionalLight`,
`PointLight`, `SpotLight`, `Model {src, animation, time}`, `ImagePlane {src, width}`, `Decal {src, size, normal}`,
`Refinish {materials}`, `Bloom`, `Fog`, `ToneMapping`,
and the ECS world (`@raea/motion/ecs`) drawn with `<Entities>`. Everything 3D imports from `@raea/motion/3d`;
`staticFile`, `useCurrentFrame` and the rest from `@raea/motion`.

## The rules that keep a render equal to its play

1. **Every value from the frame.** Positions, angles, colours and intensities come from `useCurrentFrame()` (through
   `interpolate`, `spring` or plain arithmetic) or from a game's state at the frame. No `Math.random()` unless seeded
   by an index, no `Date`, no `useState`, `useEffect`, `useRef` or `useFrame` mutation: there is no render loop to
   mutate in.
2. **Radians for rotation**, on Mesh, Group, Model and ImagePlane (`Math.PI / 4`, not `45`). Manifold's
   `.rotate()` is the one place that takes degrees.
3. **Colours are hex** (`"#ff7a00"`), from the brand kit in a branded piece. CSS variables don't reach the 3D.
4. **Name what comes and goes.** Parts are matched across frames by `name`, else by kind and shape in tree order.
   A Mesh that appears, disappears or swaps places with one of the same shape needs a `name`.
5. **A shape is fixed for the life of a part.** Animate position, rotation, scale, colour, opacity, glow,
   roughness and metalness; don't change `args` or `shape` frame to frame (that's a new part each frame).
6. **One camera, one set of effects.** The first camera in the tree is used. Bloom, Fog and ToneMapping take their
   first frame's values; the lens (Fisheye, LensDistortion) and MotionBlur animate per frame.
7. **Import what you use.** Every component in the scene (`PointLight`, `Sphere`, `Bloom`...) comes from
   `@raea/motion/3d` in the import line; a missing one fails the render. `staticFile`, `Sequence`, `spring` and the
   hooks come from `@raea/motion`.
8. **Render and look.** Render stills where things happen (`raea still`) and look at them. A 3D frame that renders
   black usually has no light or a camera inside or behind the subject.
