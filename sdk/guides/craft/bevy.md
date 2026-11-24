# 3D and games with Bevy, in Bevy's names

You write 3D the way you write the rest of a composition: TSX components whose props are worked out from the frame.
raea's engine turns them into Bevy entities (meshes, lights, cameras, glTF models, post-processing) and draws them
under the page, so 2D parts (titles, captions, a score) sit over the 3D in the same composition. You never write Rust
or a render loop. Bevy's own components (`<BevyComponent>`, `raea/bevy-own`) and a shader of your own
(`<ShaderMesh>`) are there for what the components here don't reach (`bevy/limits`).

A render and a live play of the same file draw the same rows with the same code, so what you check in a render is what
plays. That holds only if every value comes from the frame (the rules below).

## Which file to read

Read one or two at a time with `raea guide bevy/NAME`, when the task reaches them. Two or three files cover most
pieces; then write the file. Each file's example is a complete composition you can start from.

| You are... | Read |
|---|---|
| setting up a scene: the Canvas3D, the camera, camera moves, groups, 2D over 3D | `bevy/scene` |
| lighting: sun, bulbs, spots, ambient, shadows, how bright | `bevy/lights` |
| surfaces: colour, metal, glass, glow, clearcoat, pictures on surfaces, toon (cel) shading and ink outlines, decals (labels, stains, tiles), a model's materials repainted | `bevy/materials` |
| people: bodies posed by goals (a hand on the mouse, sat, looking at the screen), motion capture, walking round the furniture to a spot | `bevy/characters` |
| interiors: rooms, cabinets along walls, windows and bays, chairs turned to tables, layout checks, wall decor, the garden outside, daylight, sounds placed in the scene | `bevy/rooms` |
| a whole scene to build and make believable: the brief, which tool makes which object, build order, review sheets, the judge, when to stop | `bevy/scenes` |
| a kit of props in one style: references from style images, a reference sheet, Tripo and Meshy picked on clay, LODs, landed as components with a lineup | `bevy/kits` |
| shots of a built set from a crew's words: moves, walkthroughs from the floor plan, handheld, focus pulls, lens charts, framing sheets, camera clearance, grade looks | `bevy/shots` |
| the words to paste for a whole scene: the brief template, the builder prompt, the judge prompt | `bevy/prompts` |
| a Godot project: its scenes and resources scripted from TypeScript, run headless, recorded as frames to use as footage | `bevy/godot` |
| shapes: boxes to modelled solids, blended organic bodies (`sdf`), trees, terrain, rocks and plants (`grow`), 3D letters, lines and curves drawn on (`Line3d`, `Tube`) | `bevy/meshes` |
| moving things: spins, springs, reveals, things arriving later, model clips | `bevy/animation` |
| a 3D game: state, input, many entities, collisions, state machines | `bevy/games` |
| glTF models, pictures in space, where files go | `bevy/assets` |
| an asset to fix in Blender first (scale, pivot, heavy mesh, FBX/OBJ, rig, bake); never a whole scene | `bevy/blender` |
| a real scene or object from footage as gaussian splats, and animating them (assemble, disperse, morph...) | `bevy/splats` |
| liquids and goo: coffee dumped from a mug, a tap's stream, honey, things melting | `bevy/liquids` |
| water: seas, lakes and pools; waves, reflection, refraction, absorption, caustics, foam | `bevy/water` |
| ray-traced light (Solari): what it lights and traces, glowing lamps, rooms that don't leak, cuts, metering | `bevy/solari` |
| bloom, fog, tone mapping, exposure, fisheye and lens distortion, motion blur | `bevy/post` |
| three.js work: an R3F scene (`<R3F>`, useFrame, drei), a plain three.js page (`<Three program>`), TSL, three's shaders | `bevy/r3f` |
| how much the scene can hold, what isn't there yet | `bevy/limits` |

## What exists

Bevy's names, with Bevy's fields and units: `Camera3d {transform, projection}` (`raea/camera`), `Transform`
and `Quat` (`raea/transform`: `Transform.fromXyz(0, 2, 7).lookingAt([0, 0.5, 0])`), `Mesh3d {name, transform}`
holding one shape (`Cuboid`, `Sphere`, `Plane3d`, `Cylinder`, `Cone`, `Torus`, `Capsule3d`: `raea/mesh`; `Text3D`, or
`shape={manifold...}`) and one material (`StandardMaterial` with `base_color`, `perceptual_roughness`, `metallic`,
`emissive`, `unlit`, clearcoat and transmission as its fields; `ToonMaterial` for cel shading and ink outlines;
`ExtendedMaterial` for a shader of your own: `raea/pbr`), `Entity {transform}` to group parts, the lights in lux and lumens
(`DirectionalLight {illuminance}`, `PointLight`, `SpotLight`, `RectLight {intensity}`, `AmbientLight {brightness}`,
and `light_consts` for Bevy's named values: `raea/light`), `Model {src, animation, time}`, `ImagePlane {src, width}`,
`Decal {src, size, normal}`, `Refinish {materials}`, `Bloom`, `Fog`, `ToneMapping`, and the ECS world (`raea/ecs`)
drawn with `<Entities>`. `Canvas3D` (`raea/3d`) holds the 3D under the page; `staticFile`, `useCurrentFrame` and
the rest come from `raea`.

## The rules that keep a render equal to its play

1. **Every value from the frame.** Positions, angles, colours and intensities come from `useCurrentFrame()` (through
   `interpolate`, `spring` or plain arithmetic) or from a game's state at the frame. No `useState`, `useEffect`,
   `useRef` or `useFrame` mutation on raea's components: there is no render loop to mutate in. (three.js work keeps
   its own: an R3F scene inside `<R3F>`, a plain three.js page in `<Three program>`, stepped from the frame: `bevy/r3f`.) `Math.random()` is seeded for each frame, so
   it draws new numbers every frame: seed a scatter or anything that holds still from an index. `Date` is the frame's
   time.
2. **Radians and quaternions for rotation**: a Transform turns by `Quat.fromRotationY(Math.PI / 4)` or
   `.lookingAt(...)`, never degrees; a Model and an ImagePlane take radians. Manifold's `.rotate()` is the one place
   that takes degrees.
3. **Colours are hex** (`"#ff7a00"`), from the brand kit in a branded piece. CSS variables don't reach the 3D.
4. **Name what comes and goes.** Parts are matched across frames by `name`, else by kind and shape in tree order.
   A Mesh that appears, disappears or swaps places with one of the same shape needs a `name`, and so does a Fire or a
   fog volume that comes and goes (a plate, a splat and a sequence are known by their file): SG2.
5. **Grow by scale; a changed shape is a new mesh.** Animate position, rotation, scale, colour, opacity, glow,
   roughness and metalness. A part whose `shape`, `args` or words change is built again on each frame they change
   (docs/stage-guards.feature SG1), which is right but costs a mesh a frame: a bar that grows is a fixed box scaled, and
   a counter's words change only when its number does. A line that draws on is one `Line3d` or `Tube` cut at `p`
   (`bevy/meshes`), never a chain of boxes: a chain of more than 8 parts named `NAME-i` that grows is the render's
   `line-of-parts` warning, and a part whose size on screen jumps over 25% in a frame with no cut is its `part-snap`
   (SG6, SG12).
6. **One camera a frame; each frame's own effects.** The first active camera in the tree is the piece's: give each
   shot its own `<Camera3d>` and switch them with `is_active={...}`, so a cut between cameras is one render, not one
   render a camera joined after (the frame another camera becomes active is drawn as a shot rendered alone starts,
   with no motion blur from the other camera's frame). The effects (the sky, Bloom, Fog, Grade, ToneMapping, Shadows
   and the rest) and each light's colour, shadows and shafts take each frame's values, so shots with different looks
   are one composition and one render too; the lens (Fisheye, LensDistortion) and MotionBlur animate per frame.
7. **Import what you use.** Every component in the scene comes from its module in the import line (`Camera3d`
   from `raea/camera`, `Mesh3d` and `Cuboid` from `raea/mesh`, `StandardMaterial` from `raea/pbr`, the lights from
   `raea/light`, `Transform` from `raea/transform`, `Canvas3D`, `Bloom` and the rest from `raea/3d`); a missing one
   fails the render. `staticFile`, `Sequence`, `spring` and the
   hooks come from `raea`.
8. **One clock.** A scene is a `<Sequence from>`, and every part reads `useCurrentFrame()`. Never pass a frame down as
   a prop to time a part: RichText's `at` and `exitAt` count in its Sequence's frames, and one past its last frame is
   refused (SG5). Render a piece whole, one frame after another; a piece rendered in parts is a workaround, never the
   plan.
9. **Type that reads.** Words that name or point at a part are a `Label` (`raea/camera`): page type over the 3D, placed
   where the part lands through the shot's own camera, sized in pixels of the frame, inside title-safe, with a line to
   the point: `<Label at={partTop} camera={path.camera(frame)}>Reviews</Label>` (SG8). Page type stays off the parts:
   words in the page over a tenth of a part for a quarter second fail the render (`type-over-part`, SG11), and words
   meant to sit on the scene (a title across it) go in an element with `data-allow-overlap`. Words in the world (a sign,
   a page in a book) are RichText: at least 22 px tall (caps) at 1080 lines, turned within 35° of the camera, inside the
   title-safe area, held still for their reading time (words ÷ 3 a second, plus half a second) and fully revealed before
   they go. `raea render` fails on world type it can't read (`type-small`, `type-angle` past 50°, `type-reveal`) and
   warns on the rest (`type-safe`, `type-time`, an angle past 35°): SG10. A sign nobody is meant to read takes
   `read={false}`.
10. **A camera that settles.** Build a moving camera with `cameraPath` from `raea/camera`: keys timed by frames, the
   path paced by time so its speed carries across each key, `hold` where it stops, and `<Camera3d
   {...path.camera(frame)} />`. A hand-written spline whose segments each run 0 to 1 whatever their length jumps speed
   at every key (11-21: 51 of 155 keys jumped 2x or more). `path.check()` names a key whose timing still makes the
   camera brake hard (a quick move against a slow one): give the quick move more frames or end the slow one in a hold.
   Never push in and pull back, or pan one way then the other, within 1.5 s. A camera still moving while words are up
   gives them their reading time first. `raea render` fails on a speed that changes over 2x within 3 frames
   (`camera-lurch`) and on a dolly in and back out (`camera-reverse`): SG13.
11. **Render and look.** Before a render, `raea scene stage edit.tsx` steps every frame without drawing and runs the
   checks above, and the render runs them again at its end, beside clipping: it fails when almost nothing is on screen
   for a second (`frame-empty`: the page's background colour or gradient isn't something on screen; a room's walls, or
   words in the page, are) and warns when the shot's part is cut by the edge (`subject-cut`), SG14. Render stills
   where things happen (`raea render edit.tsx --at A,B,C -o stills/`) and look at them. A 3D frame that renders black
   usually has no light or a camera inside or behind the subject. `raea show` refuses a render that still holds an
   error, naming each: fix it, or say why it goes anyway with `--despite CHECK "why"`, which the person reads (SG15).
