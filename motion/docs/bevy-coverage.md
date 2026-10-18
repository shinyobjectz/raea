# Bevy through @raea/motion: what's reachable (2026-10-18)

raea draws 3D with Bevy **0.19.1** (`rdb/Cargo.toml`: `bevy = "=0.19.1"` with `3d_bevy_render`, `ui_bevy_render`,
`png`, `tonemapping_luts`, `ktx2`; `engine/Cargo.toml` uses `bevy_ecs 0.19.1`). Agents never write Rust: they write
TSX against `@raea/motion` and `@raea/motion/3d`, which compiles to rdb rows (a table per kind, a column per component
field, keys per frame), and rdb's engine turns rows into Bevy entities. This page says which of Bevy's features an agent
can reach that way, how, and what each missing one would need.

The path, end to end:

| step | where | what happens |
|---|---|---|
| 1 | `motion/src/3d.tsx` | each component renders a `<div data-3d=Kind data-props=JSON>` (a Mesh adds `data-shape`, its Manifold expression) |
| 2 | `react/src/motion.rs` `read3d`, `scene_of` | each frame's 3D elements read back as a tree (through plain wrapper elements such as a Sequence's div) |
| 3 | `react/src/motion/scene3d.rs` | the frames become rows: parts matched across frames by `name` (else kind and shape, in tree order), every changing value keyed per frame, a part missing from a frame hidden there |
| 4 | `rdb/src/engine.rs` `load_shared` + `rdb/src/engine/scene3d.rs` | rows become Bevy entities (`Camera3d`, `DirectionalLight`/`PointLight`/`SpotLight`, `Mesh3d` + `StandardMaterial`, `WorldAssetRoot` for glTF); `sync_meshes`, `sync_camera`, `sync_lights`, `sync_models` move them each frame from the frame's rows |
| 5 | render: `raea-motion render` → `rdb::render` (Engine::new); play: `Live::scene_3d` → `scene_program` → `Engine::new_live` | the same rows and the same code draw both, so a render equals its play by construction (checked by `motion/bench/bevy/check.sh`) |

## Summary

59 features: **27 covered, 11 partial, 21 missing**. Three of the covered ones (2D text, UI, audio) are covered by the page and raea's mixer rather than by Bevy. This work moved 12 rows to covered: visibility, transforms, hierarchy, the perspective camera, point and spot lights, the standard material, bloom, tonemapping, fog, glTF models and their clips.

The top gaps, in the order a game or ad agent would hit them:

1. **Live 3D for games.** void's stage asks for 3D only for `scene3d` and `motion` objects (`void/src/stage_native.rs`),
   and the 3D rows are baked from React stepped with the *scripted* input. A 3D game renders, but a person playing it
   live sees no 3D, and if it were enabled their keys would not move it. Needed: the game's step state streamed into
   rdb's engine each frame (the frame's rows overridden from `useGameState`), not baked.
2. **Image-based lighting and environment** (`EnvironmentMapLight`, `Skybox`): product shots look flat without
   reflections. Needs a KTX2/HDR cubemap asset story (the `ktx2` feature is on).
3. **More material maps** (normal, roughness/metalness, emissive maps; transmission, IOR, thickness for real glass).
4. **Ambient occlusion, depth of field, motion blur, vignette**: the camera effects an ad reaches for next.
5. **Physics.** No Bevy physics crate is in our tree (`bevy_rapier3d 0.36` sits in the cargo cache, in no lockfile).
   Games do collision by hand in their pure step today.
6. **Picking** (pointer on a mesh) and **orthographic cameras** for games.
7. **Clip blending** (AnimationGraph weights, transitions) for characters.

## The matrix

Status is what an agent writing TSX can do today. "Rows" is what the rdb row path carries. "Needed" is for partial and
missing features: the SDK component or hook, the rows and columns, and the rdb system.

### ECS, transforms, hierarchy, time, input

| feature | status | the SDK today | rows | needed |
|---|---|---|---|---|
| Entities and components | partial | `@raea/motion/ecs` `createWorld()`: entities with named components in Bevy's own archetype tables (`ecs.*` native calls); `<Entities world>` draws every entity with `Mesh` + `Transform` | Entities → `mesh` rows | draw more component kinds from a world (`Light`, `Model`, `Camera` components → the same rows as the JSX) |
| Systems | covered | `world.system(fn)`; games: a pure `step(state, input, frame)` | (TS) | |
| Resources | covered | `world.resource(name, value?)` | (TS) | |
| Events / messages | covered | `world.send(name, v)`, `world.events(name)`, cleared each frame; `useGameEvents` for sound cues | (TS) | |
| Queries | covered | `world.find({with, without, where})`, `world.query(kind, where)` | (TS) | |
| States | partial | a game's state is the fold's value; no named state machine | (TS) | `useGameState` already covers it; a `phase` field by convention (documented in the skills) |
| Schedules | partial | systems run in order once per frame; frame = tick, no separate fixed timestep | (TS) | `world.fixed(hz, fn)` sub-stepping inside a frame if physics needs it |
| Transforms | covered | `position`, `rotation` (radians, Euler XYZ as three.js), `scale` (number or vec) on Mesh, Model, ImagePlane, Group | `x y z rot_x rot_y rot_z scale_x scale_y scale_z`, keyed | |
| Hierarchy | covered | `<Group>` composes into world space; ECS `setParent` and child transforms in `<Entities>` | composed on the React side | |
| Visibility | covered | conditional rendering and Sequences; a part absent from a frame is hidden there | `visible`, keyed | |
| Time | covered | `useCurrentFrame()`, `useVideoConfig().fps`; a frame is a function of its number (Bevy's `Time` is never read) | keys per frame | |
| Keyboard and pointer input | covered | `useInput()` → `{keys, pointer}`, scripted input for renders, live input in play | (TS) | |
| Gamepad | missing | | | `FrameInput.gamepad?: {axes: number[], buttons: boolean[]}` from `bevy_gilrs` or the stage's own reader |
| Observers, hooks | missing | not needed: no per-entity callbacks run while a frame draws | | |

### 3D rendering

| feature | status | the SDK today | rows | needed |
|---|---|---|---|---|
| Primitive meshes | covered | `Box`, `Sphere`, `Plane`, `Cylinder`, `Cone`, `Torus`, `Text3D` | `mesh.shape` (Manifold expression) | |
| Modelled solids (CSG) | covered | `manifold.box(...).minus(...)`, `roundedBox`, `text3d`, `hull`, extrude, revolve; `<Mesh shape={solid}>` | `mesh.shape` | |
| Custom vertex meshes | partial | Manifold's extrude and revolve only | | `<BufferMesh positions normals uvs indices>` → a `mesh.verts` asset row |
| StandardMaterial (PBR) | covered | `color`, `roughness`, `metalness`, `emissive`, `emissiveIntensity`, `opacity`/`transparent`, `map`; Physical: `clearcoat`, `clearcoatRoughness`, `reflectance`; Basic: unlit | `color col_r col_g col_b rough metal emissive emissive_k opacity unlit clearcoat clearcoat_rough reflectance map` | |
| Material maps (normal, roughness/metal, emissive, AO) | missing | | | `normalMap`, `roughnessMap`, `metalnessMap`, `emissiveMap` props → `normal_map ...` columns; UVs exist (box-projected) |
| Transmission, IOR, thickness | missing | `opacity` blends only | | `transmission`, `ior`, `thickness` → `StandardMaterial.specular_transmission` etc. and `ScreenSpaceTransmission` on the camera |
| Custom shaders | missing | deliberately: agents don't write WGSL | | |
| Directional light | covered | `DirectionalLight {position, target, intensity, color, castShadow}` | `light: lux dir_x dir_y dir_z color shadows`, keyed | |
| Point light | covered | `PointLight {position, intensity, color, distance, castShadow}` | `light type='point': x y z lumens range color shadows`, keyed | |
| Spot light | covered | `SpotLight {position, target, angle, penumbra, ...}` | `light type='spot': ... dir_* outer inner` | |
| Ambient light | covered | `AmbientLight {intensity, color}` (summed) | `camera.ambient ambient_color`, keyed | |
| Environment map / IBL, skybox | missing | | | `<Environment src="studio.ktx2" intensity background>` → `camera.env_diffuse env_specular env_k sky` → `EnvironmentMapLight`, `Skybox` |
| Shadows | partial | `castShadow` per light (directional on by default); every mesh casts and receives | `shadows` | `castShadow`/`receiveShadow` on Mesh → `NotShadowCaster`, `NotShadowReceiver`; cascade distance for big scenes |
| Perspective camera | covered | `PerspectiveCamera {position, lookAt, fov, near, far}`, animated | `camera: x y z look_* fov near far`, keyed | |
| Orthographic camera | missing | | | `<OrthographicCamera zoom>` → `camera.ortho` → `Projection::Orthographic` |
| Several cameras, viewports | missing | the first camera wins | | split-screen is rare in ads; a `viewport` column per camera |
| Render layers | missing | | | `layer` on parts and cameras → `RenderLayers` |
| Anti-aliasing | partial | MSAA 4x always (off in preview quality) | | `aa: 'msaa' \| 'smaa' \| 'taa'` on Canvas3D |
| Bloom | covered | `<Bloom intensity threshold>` (1 = Bevy's natural preset) | `camera.bloom bloom_threshold` | |
| Tonemapping and exposure | covered | `<ToneMapping mode exposure>` (tony, aces, agx, neutral, reinhard, none; exposure in stops) | `camera.tonemap exposure` | |
| Distance fog | covered | `<Fog color near far>` or `density` | `camera.fog_*` | |
| SSAO | missing | | | `<AmbientOcclusion>` → `ScreenSpaceAmbientOcclusion` (needs `Msaa::Off` and the depth and normal prepasses) |
| Depth of field | missing | | | `<DepthOfField focus aperture>` → `bevy::post_process::dof::DepthOfField` |
| Motion blur | missing | | | `<MotionBlur shutter>` → `MotionBlur` (needs the motion-vector prepass; deterministic per frame) |
| Vignette, chromatic aberration, colour grading | missing | (CSS over the page can vignette) | | `<Vignette>`, `<ColorGrading>` → 0.19's `Vignette`, `ColorGrading` |

### 2D, text, UI

| feature | status | the SDK today | rows | needed |
|---|---|---|---|---|
| Sprites | partial | 2D is the page (HTML/CSS through Blitz); `<ImagePlane>` puts a picture in 3D | `panel` | sprite sheets: `ImagePlane frame={n} grid={[cols, rows]}` → UV rect columns |
| Text in 2D | covered (by the page) | HTML text, brand fonts, captions; not `Text2d` | page | |
| Text in 3D | partial | `Text3D` (extruded solid letters) | `mesh.shape = text3d(...)` | flat text on a card: a DOM subtree painted to a `panel` (engine/docs/bevy.feature B4) exposed as `<Html3D>` |
| 2D meshes and materials | missing | use the page (SVG, CSS) | | not needed while the page draws 2D |
| UI (bevy_ui) | covered (by the page) | CSS flex and grid, not bevy_ui | page | |

### Animation, assets, audio, scenes, tools

| feature | status | the SDK today | rows | needed |
|---|---|---|---|---|
| Tweening | covered | `interpolate`, `spring`, `Easing` on any prop; values keyed per frame | keys | |
| glTF animation clips | covered | `<Model animation="Run" time loop>` posed by seeking each frame | `model.anim anim_t anim_loop` | |
| Skeletal (skinned) animation | partial | Bevy skins glTF meshes; untested in our fixtures (the fixture's clips are node animation) | `model` | a skinned fixture (a CC0 or self-made rig) |
| Clip blending, transitions | missing | one clip per Model | | `animations={[{name, time, weight}]}` → several graph nodes with weights |
| Morph targets | partial | Bevy's `morph` feature is on; untested | `model` | a fixture; `morphs={{smile: 0.5}}` → `MorphWeights` |
| glTF models | covered | `<Model src={staticFile("x.glb")}>`; loaded before the first frame | `model.src scene` | |
| Images | covered | `map`, `ImagePlane` (PNG, JPEG through the image crate) | `map`, `panel.src` | |
| HDR / KTX2 environment maps | missing | | | see Environment above |
| Audio | covered (by raea) | `<Audio>`, `lyria(...)`, `sao(...)`, `<Mix>`, `useGameEvents` cues; raea's mixer, not bevy_audio | `audio` rows | |
| Spatial audio | missing | | | pan and gain from a part's position: a `useSpatial(position)` hook returning `{pan, volume}` for `<Audio>` |
| Scenes (world serialization) | partial | glTF scenes; the TSX itself is the scene | | not needed |
| Gizmos | missing | | | debug only: `<Gizmo line={[a, b]} color>` → immediate-mode lines in the engine; useful for agents checking layouts |
| Picking | missing | pointer in screen pixels only | | a pure TS `pick(pointer, parts)` ray test against bounds (keeps the step pure), not `MeshPickingPlugin` |
| Physics | missing | collisions by hand in the step | | `@raea/motion/physics`: a pure, fixed-step rigid-body step (`stepBodies(state, dt)`) in TS so renders equal play; a Bevy crate would need the engine to own the state |
| Particles | missing | many small meshes or `<Entities>` | | `<Particles count seed emit>` computed in TS from the frame (deterministic), drawn as instanced meshes |

## Proposed TypeScript for the missing pieces

Each keeps the rules: declarative TSX, every value a function of the frame, no React reconcile per frame for games,
compiled to rows the engine draws. Names follow three.js and drei where agents know them.

```ts
// lighting and camera
export interface EnvironmentProps { src: string /* .ktx2 or .hdr in media */; intensity?: number; background?: boolean; rotation?: number }
export declare function Environment(p: EnvironmentProps): JSX.Element;            // rows: camera.env_src env_k sky
export interface OrthographicCameraProps { position?: Vec3; lookAt?: Vec3; zoom?: number; near?: number; far?: number }
export declare function OrthographicCamera(p: OrthographicCameraProps): JSX.Element; // rows: camera.ortho zoom
export interface AmbientOcclusionProps { quality?: "low" | "medium" | "high" }
export interface DepthOfFieldProps { focus: number /* distance */; aperture?: number /* f-stops */ }
export interface MotionBlurProps { shutter?: number /* 0..1 of a frame */ }
export interface VignetteProps { intensity?: number; color?: string }

// materials
export interface MeshStandardMaterialProps {
  normalMap?: string; roughnessMap?: string; metalnessMap?: string; emissiveMap?: string; aoMap?: string;
  transmission?: number; ior?: number; thickness?: number;                         // real glass
}
export interface MeshProps { castShadow?: boolean; receiveShadow?: boolean; layer?: number }

// animation
export interface ClipLayer { name: string | number; time?: number; weight?: number; loop?: boolean }
export interface ModelProps { animations?: ClipLayer[]; morphs?: Record<string, number> }

// games
export interface FrameInput { keys: string[]; pointer?: { x: number; y: number; down: boolean }; gamepad?: { axes: number[]; buttons: boolean[] } }
export declare function pick(pointer: { x: number; y: number }, camera: CameraState, parts: Pickable[]): Pickable | null; // pure ray test
// @raea/motion/physics: pure, fixed-step, in the fold
export interface Body { id: string; shape: { box: Vec3 } | { sphere: number }; position: Vec3; velocity: Vec3; mass?: number; restitution?: number; fixed?: boolean }
export declare function stepBodies(bodies: Body[], dt: number, gravity?: Vec3): { bodies: Body[]; contacts: [string, string][] };
```

## Render equals play: the rules the row path keeps

- A frame is a function of its number: `useCurrentFrame()` and the input timeline. No `Math.random()` without a seed,
  no `Date`, no `useState`/`useEffect` holding scene state.
- Parts are matched across frames by `name`, else by kind and shape (or source) in tree order. Give a Mesh or Model a
  `name` when parts of the same shape come and go in a different order.
- A part's shape and its material's static fields (map, clearcoat, unlit) are its first frame's: a new shape is a new
  part. Animate position, rotation, scale, colour, opacity, glow, roughness and metalness instead.
- The first camera in the tree is the camera. Post-processing (Bloom, Fog, ToneMapping) is the first frame's.
- glTF clips are posed by `time` (seconds), never advanced by a clock.
- Known difference outside the 3D rows: a page element that is partly transparent over the 3D blends in linear light
  in a render and in sRGB in play (`react/bench/corpus/fixtures/box3d`'s fading title: SSIM 0.96 at frame 30). Opaque
  page parts over 3D match.

## Performance limits (measured or read from the code)

- Each mesh row has its own `StandardMaterial`, so Bevy can't batch them; hundreds of meshes are fine, thousands are
  not. Manifold builds each distinct shape expression once (`shape::cached`), so repeated shapes are cheap to build.
- Every changing value is keyed on every frame: rows grow with frames x animated columns.
- Live play builds the 3D rows by stepping React through every frame on a thread before the 3D shows
  (`scene_program`); long compositions take longer to show their 3D.
- A point light with `castShadow` renders six shadow maps; keep shadowed point lights to one or two. Bevy allows at
  most 10 directional lights.
- `Bloom` turns the camera HDR (`#[require(Hdr)]` on `Bloom`): an extra full-size float target.
- Models load before the first frame (at most 60 s).

## Checks

- `motion/bench/bevy/check.sh`: every fixture (`lights`, `materials`, `model`, `planes`) rendered and played baked
  with its 3D on a layer; on 2026-10-18 all four at SSIM 1.000 at frames 0, 30 and 59.
- `react/bench/timelines/check.sh` (titles: 1.000), `engine/bench/parity.sh` (93 exact, 75 ok, 2 fail, as before),
  rdb's tests (41 pass), `motion/tools/golden.sh` 3D cases 04-06 against their frozen references (all pass),
  `bun test conformance` (126 pass).

## Existing Bevy knowledge packs (surveyed 2026-10-18)

What the skills library (`void/prompts/craft/bevy/`, mirrored in `skills/raea-bevy/`) took from each. Nothing was
copied from a pack without a permissive license; the library is written for our SDK, so most packs informed its
structure and its list of mistakes rather than its text.

| pack | targets | license | used for |
|---|---|---|---|
| [chrisgliddon/bevy-skills](https://github.com/chrisgliddon/bevy-skills) | 0.19 | MIT | a router skill over topic skills; its "cardinal rules" of 0.19 API changes informed the Rust-facing notes in `rows.md` |
| [gamedev-skills `bevy-ecs`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills) | 0.19 | Apache-2.0 | symptom-to-fix lists as the shape of each topic's "common mistakes" |
| [pavlov-net/claude-plugins `bevy`](https://github.com/pavlov-net/claude-plugins) | 0.20 | none stated | cited only |
| [laurigates/claude-plugins `bevy-plugin`](https://github.com/laurigates/claude-plugins/tree/main/bevy-plugin) | pre-0.15 (bundles, EventReader) | MIT | not used: stale |
| [jarrodmedrano/jarrod-claude-skills `bevy`](https://github.com/jarrodmedrano/jarrod-claude-skills) | 0.17 | MIT (README) | not used: partly wrong |
| [Unofficial Bevy Cheat Book](https://bevy-cheatbook.github.io/) | 0.13 and older; unmaintained | code MIT-0, text CC-BY-NC-SA-4.0 | concepts only; no text copied (non-commercial license) |
| [Bevy v0.19.1 examples](https://github.com/bevyengine/bevy/tree/v0.19.1/examples) (also in the cargo cache) | 0.19 | MIT OR Apache-2.0 | the API facts behind the row path (`WorldAssetRoot`, `shadow_maps_enabled`, `Bloom` requiring `Hdr`, `AnimationGraph::from_clip`, seeking a paused clip) |
| [Bevy 0.18→0.19 migration guide](https://bevy.org/learn/migration-guides/0-18-to-0-19/) | 0.19 | MIT OR Apache-2.0 | the renames above |
| [EnzeD/r3f-skills](https://github.com/EnzeD/r3f-skills) | r3f 9, three r185 | none stated | cited only; its one-skill-per-topic layout with pitfalls and one checked example is the model the library follows |
| [emalorenzo/three-agent-skills](https://github.com/emalorenzo/three-agent-skills) | r3f, three | MIT | the r3f habits agents bring (degrees vs radians, `useFrame` mutation) listed as mistakes to unlearn |
| [r3f](https://r3f.docs.pmnd.rs/llms.txt), [drei](https://drei.docs.pmnd.rs/llms.txt), [three.js](https://threejs.org/llms.txt) docs | current | MIT | component and prop names the SDK mirrors |
| Cursor rules ([awesome-cursorrules](https://github.com/PatrickJS/awesome-cursorrules), CC0) | | | no Bevy rules worth adapting; one Bevy `.cursorrules` found is GPL-3.0 and was not used |
| [bevy_brp_mcp](https://github.com/natepiano/bevy_brp) | 0.20 now | MIT OR Apache-2.0 | a live-inspection tool, not text; not adopted (our engine has no BRP endpoint) |

Bevy 0.20 shipped on 2026-10-08; packs that moved to 0.20 describe APIs we don't have. Bevy's own repository has no
`AGENTS.md`, and `bevy.org/llms.txt` does not exist.
