# Bevy through @raea/motion: what's reachable

raea draws 3D with Bevy **0.19.1** (`engine/rdb/Cargo.toml`: `bevy = "=0.19.1"` with `3d_bevy_render`, `ui_bevy_render`,
`png`, `tonemapping_luts`, `ktx2`; `engine/html/Cargo.toml` uses `bevy_ecs 0.19.1`). Agents never write Rust: they write
TSX against `@raea/motion` and `@raea/motion/3d`, which compiles to rdb rows (a table per kind, a column per component
field, keys per frame), and rdb's engine turns rows into Bevy entities. This page says which of Bevy's features an agent
can reach that way, how, and what each missing one would need.

The path, end to end:

| step | where | what happens |
|---|---|---|
| 1 | `sdk/motion/src/3d.tsx` | each component renders a `<div data-3d=Kind data-props=JSON>` (a Mesh adds `data-shape`, its Manifold expression) |
| 2 | `sdk/react/src/motion.rs` `read3d`, `scene_of` | each frame's 3D elements read back as a tree (through plain wrapper elements such as a Sequence's div) |
| 3 | `sdk/react/src/motion/scene3d.rs` | the frames become rows: parts matched across frames by `name` (else kind and shape, in tree order), every changing value keyed per frame, a part missing from a frame hidden there |
| 4 | `engine/rdb/src/engine.rs` `load_shared` + `engine/rdb/src/engine/scene3d.rs` | rows become Bevy entities (`Camera3d`, `DirectionalLight`/`PointLight`/`SpotLight`, `Mesh3d` + `StandardMaterial`, `WorldAssetRoot` for glTF); `sync_meshes`, `sync_camera`, `sync_lights`, `sync_models` move them each frame from the frame's rows |
| 5 | render: `raea-motion render` → `rdb::render` (Engine::new); play: `Live::scene_3d` → `scene_program` → `Engine::new_live`; a game played live: `motion::live_scene` → `LiveScene` (each frame's own state) → `Engine::set_rows` | the same rows and the same code draw both, so a render equals its play by construction (checked by `sdk/motion/bench/bevy/check.sh`) |

## Summary

59 features: **28 covered, 10 partial, 21 missing**. Three of the covered ones (2D text, UI, audio) are covered by the page and raea's mixer rather than by Bevy.

The top gaps, in the order a game or ad agent would hit them:

1. **Image-based lighting and environment** landed: `<Environment>` (an equirectangular HDR made a cubemap at load,
   `GeneratedEnvironmentMapLight` and `Skybox`, switchable per frame; the environment fixture).
2. **More material maps** (emissive maps; transmission, IOR, thickness for real glass). Normal, roughness/metalness and AO maps with tiling landed (texmaps fixture).
3. **Motion blur** landed (with ambient occlusion, depth of field and the vignette): Bevy's `MotionBlur`, made a function of the frame by drawing the frame before first (see its row).
4. **Physics.** No Bevy physics crate is in our tree (`bevy_rapier3d 0.36` sits in the cargo cache, in no lockfile).
   Games do collision by hand in their pure step today.
5. **Picking** (pointer on a mesh) and **orthographic cameras** for games.
6. **Clip blending** (AnimationGraph weights, transitions) for characters.

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
| Toon (cel) material, ink outlines | covered | `MeshToonMaterial {color, steps \| bands, shadowColor, rim, highlight, outline: {width (px), color}, emissive, opacity, map}` on a Mesh, or inside a `<Model>` over its glTF materials (all, or by `material` name; skinned and morphed included): the surface is `ExtendedMaterial<StandardMaterial, ToonExt>` lit in hard bands by the scene's lights and shadows (`engine/rdb/src/engine/toon.rs`, `toon.wgsl`), the outline an inverted hull (the mesh again with smoothed normals, front faces culled, pushed `width` pixels out on screen: `toon_outline.wgsl`) | `mesh.toon` (JSON: steps, bands, shadow, rim, spec, outline), keyed `col_r..`, `sh_r sh_g sh_b`, `rim_k`, `spec_k`, `outline_w`, `emissive_k`, `opacity`; `model.toon` (a list) | a model's toon values are the first frame's; a toon part isn't swapped by `<Look mode="paint">` (paint swaps standard materials); a see-through part shows its own hull |
| Material maps (normal, roughness/metal, AO) and tiling | partial | `normalMap` (OpenGL layout, green up), `roughnessMap`, `metalnessMap` (greyscale, packed by the engine), `metallicRoughnessMap` (glTF: G rough, B metal; Poly Haven `_arm`), `aoMap` (red), `repeat={[u, v]}` tiling every map of the material; Standard, Physical (Basic: `map`, `repeat`). A map makes its factor default to 1 | `normal_map rough_map metal_map mr_map ao_map repeat_u repeat_v` (first frame's); textures mipmapped (box filter, linear light for colour), trilinear, 16x anisotropic, Repeat when tiled; one texture per file per load; tangents by mikktspace on meshes with a normal map | `emissiveMap`; UVs are box-projected per face (each face of a Box/Plane gets the map once, times `repeat`), so maps smear on curved shapes; a world-scale tiling (same texel density on a long and a short face) |
| Transmission, IOR, thickness | missing | `opacity` blends only | | `transmission`, `ior`, `thickness` → `StandardMaterial.specular_transmission` etc. and `ScreenSpaceTransmission` on the camera |
| Custom shaders | missing | deliberately: agents don't write WGSL | | |
| Model looks (pixel art, the site's dither) | covered | `<Model look="pixel"\|"site" pixelSize edgeColor theme lift gain flat>`: the model alone on its own render layer, drawn at a cell per texel by a camera following the scene's, laid over the frame by `engine/rdb/src/engine/look.wgsl` | `look look_px look_edge look_theme look_lift look_gain flat` | parts in front of a looked model don't hide it |
| Frame look, depth of field | covered | `<Look mode pixelSize theme color lift gain ink paper line>` (mode site, pixel or ink: two colours 8x8-Bayer-dithered, Return of the Obra Dinn's; or paint: every standard material swapped for paint.wgsl's (the suns' light in `bands` flat tones: lit, form shadow x `mid`, cast shadow x `shadow`, the shadow cut hard; `engine/rdb/src/engine/paint.rs`), then look.wgsl's mode 6 over the frame at a pixel a cell: edges wandered by seeded noise (`rag ragScale seed boil`), a fixed paper `grain`, a flat or two-tone `sky` where nothing's drawn, an optional `palette` snapped to in Oklab, four taps a pixel; or cartoon: paint with two bands, a boil on twos and an `<Outline width>` round silhouettes): the scene's camera draws into a target a cell per texel and the composite lays it over the frame (opaque, under any model's look; `engine/rdb/src/engine/look.rs` sync_frame_look); `<DepthOfField aperture mode maxBlur>` with the camera's `focus` keyed per frame | `frame_look frame_look_px frame_look_color frame_look_lift frame_look_gain frame_look_theme frame_look_ink frame_look_paper frame_look_line paint_bands paint_grain paint_rag paint_seed paint_rag_scale paint_shadow paint_mid paint_sky_top paint_sky_bottom paint_palette dof_aperture dof_mode dof_max focus` | the frame look is set from the first frame, but paint's bands, grain, rag and seed are keyed (fixture `sdk/motion/bench/bevy/paint`); paint lights by suns only |
| Matchmove (camera solve) | covered | `<Matchmove src from seconds subject height fov crop zoom>`: the clip's camera solved once (`raea-motion track`, tools/matchmove/run.py, COLMAP; rotation-only fallback without parallax) and kept in the asset store; each frame the footage as a `<Plate>` behind the 3D, the camera from the solve (roll through `up`, fov from the focal length, an off-axis `view` window when the frame's shape isn't the footage's), the subject's EdgeTAM cut-out drawn over it as page; `useMatchmove()` | `plate` rows (`src t k1`, `t` keyed); camera `up_x up_y up_z view_w view_h view_x view_y`, keyed; `engine/rdb/src/engine/matchmove.rs` (`OffAxis` projection, `sync_view`, `sync_plates`) | real scene geometry doesn't hide the 3D (only the subject's matte does); one subject; plates and view/up columns aren't in live play's row diff |
| Attaching a model to a bone | covered | `<Model attach={{ to, bone }}>`: once both models are ready the model is made a child of the other's named node (rdb `sync_attach`), so it follows the animated pose and its placement is in the bone's space (a fedora on Quaternius's `Head`: scale 0.0135 under the armature's x100) | `attach_to attach_bone` | set once; an unknown model or bone is an error |
| Ink outlines | covered | `<Outline color depth normal width>`: a pass after the main pass (before bloom and tone mapping) draws a line where the depth prepass steps (a silhouette, on the nearer side) or the normal prepass turns (a crease); under a `<Look>` a cell wide; `width` (px) draws an even, thick line (16 directions at three radii). Turns MSAA off (`engine/rdb/src/engine/outline.rs`) | `outline outline_depth outline_normal outline_width` | set from the first frame; no fixture yet |
| Directional light | covered | `DirectionalLight {position, target, intensity, color, castShadow}` | `light: lux dir_x dir_y dir_z color shadows`, keyed | |
| Point light | covered | `PointLight {position, intensity, color, distance, castShadow}` | `light type='point': x y z lumens range color shadows`, keyed | |
| Spot light | covered | `SpotLight {position, target, angle, penumbra, ...}` | `light type='spot': ... dir_* outer inner` | |
| Ambient light | covered | `AmbientLight {intensity, color}` (summed) | `camera.ambient ambient_color`, keyed | |
| Environment map / IBL, skybox | covered | `<Environment src intensity rotation background backgroundIntensity>` inside Canvas3D: an equirectangular `.hdr` (image crate), `.exr` (decoded by ffmpeg as a PFM) or `.png`/`.jpg` (sRGB); src, intensity, rotation and the sky change per frame | `camera.env_srcs` (JSON list) `env_i` (index, -1 none) `env_k` (cd/m², intensity 1 = 1000) `env_rot` `sky_k`, keyed; engine/rdb/src/engine/environment.rs: each panorama a CPU-built Rgba16Float cubemap (face = widest width / 4, a power of two from 64 to 2048, one size for all), `GeneratedEnvironmentMapLight` (GPU-filtered every frame from the source the frame picks, fixed blue noise) + `Skybox`; `sync_environment` swaps source, intensity and rotation on both it and the `EnvironmentMapLight` it made | untested in a live game's scene; looked models (`look=`) are drawn by their own camera, which doesn't copy it; light probes / irradiance volumes (one environment for the whole scene); filtering every frame costs 4.5 ms at 1080p from a 1k panorama: caching each panorama's filtered maps would make it free after the first frame |
| Shadows | partial | `castShadow` per light (directional on by default); every mesh casts and receives; `<Shadows mapSize distance firstCascade cascades overlap bias normalBias filter>` (defaults 4096, 150 m, 8 m, 4, 0.3, Bevy's biases, gaussian); `softShadow` on a light: PCSS (`experimental_pbr_pcss`), `true` or a sun's `soft_shadow_size` / a bulb's radius in metres, its penumbra dithered in a fixed pattern (the gaussian or hardware filter, never the temporal one), so a render equals its play | `light.shadows shadow_dist shadow_first shadow_cascades shadow_overlap shadow_bias shadow_nbias soft`; `camera.shadow_map shadow_filter` (DirectionalLightShadowMap, PointLightShadowMap at half, ShadowFilteringMethod) | `castShadow`/`receiveShadow` on Mesh → `NotShadowCaster`, `NotShadowReceiver`; the temporal filter (it reseeds by the frame count); per-frame shadow settings |
| Perspective camera | covered | `PerspectiveCamera {position, lookAt, fov, near, far}`, animated | `camera: x y z look_* fov near far`, keyed | |
| Orthographic camera | missing | | | `<OrthographicCamera zoom>` → `camera.ortho` → `Projection::Orthographic` |
| Several cameras, viewports | missing | the first camera wins | | split-screen is rare in ads; a `viewport` column per camera |
| Render layers | missing | | | `layer` on parts and cameras → `RenderLayers` |
| Anti-aliasing | covered | MSAA 4x by default (off in preview quality); `<AntiAliasing mode="msaa" \| "smaa" \| "fxaa" \| "none">`; SMAA (high preset) when `<AmbientOcclusion>` is on | `camera.aa` → `Smaa` (the `smaa_luts` feature) or `Fxaa`, `Msaa::Off` | TAA: it accumulates earlier frames, so a frame rendered alone (`render --at`) can't equal it in play |
| Bloom | covered | `<Bloom intensity threshold>` (1 = Bevy's natural preset) | `camera.bloom bloom_threshold` | |
| Tonemapping and exposure | covered | `<ToneMapping mode exposure>` (tony, aces, agx, neutral, reinhard, none; exposure in stops) | `camera.tonemap exposure` | |
| Distance fog | covered | `<Fog color near far>` or `density` | `camera.fog_*` | |
| Volumetric fog | covered | `<VolumetricFog steps dither resolution ambient ambientColor mode slices tile samples hero>` on the camera; `<FogVolume name position rotation scale density color absorption scattering anisotropy falloff>` boxes of air; `volumetric` on DirectionalLight, PointLight, SpotLight (turns their shadows on) | `camera.vfog vfog_steps vfog_dither vfog_resolution vfog_ambient vfog_ambient_color vfog_mode vfog_slices vfog_tile vfog_samples vfog_hero`; `fogvolume: x y z rot_* scale_* density color col_r col_g col_b absorption scattering anisotropy falloff visible`, keyed; `light.volumetric` | Bevy's jitter (seeded by its update count) is replaced: rdb registers its own copy of the fog shader (engine/rdb/src/engine/volumetric_fog.wgsl) whose `dither` is seeded from the pixel and the film frame, so a render equals its play; `resolution` below 1 marches at 1/2, 1/3 or 1/4 of the frame each way and upsamples by depth (engine/rdb/src/engine/fog_half.rs: a copy of Bevy's private fog prepare and draw into a smaller target, a checkerboard min/max depth downsample, and a joint bilateral composite; no history); overlapping volumes composite in row order (pinned in the render world each frame); the default (`mode="froxel"`; `mode="march"` keeps Bevy's march, to which `steps`, `dither` and `resolution` belong) is one medium in a grid along the view (engine/rdb/src/engine/froxel.rs: every volume, its `falloff` and every <Fire>'s light written into one grid of cells, lit once a cell, integrated front to back once and read per pixel through a tricubic B-spline, so overlapping volumes are one medium, a volume with no density shows nothing, the ambient light goes with density, and the cost is the grid's whatever the resolution or the number of volumes; no dither, no history), and its `hero` lights (the first two volumetric spots and directional lights, on by default) are integrated exactly along each pixel's ray over their shadow maps (froxel_hero.wgsl: the ray cut at volumes, cones and cascades, walked down a min-max pyramid of the map (4, 16, 64 and 256 texel blocks) to texel cells only where a shadow edge crosses it, the hardware 2x2 filter integrated exactly), so a blind's bars are as crisp as the shadow map; in the grid see-through surfaces (blended, premultiplied and added materials, image planes) and <Fire>s fog themselves at their own depth from the same grid (engine/rdb/src/engine/froxel_glass.rs; the grid's passes run between the opaque and transparent ones), though a hero shaft in front of one is dimmed by its opacity; scored against a reference march (Bevy's at 512 undithered steps, marching the whole ray and scattering the ambient light by density) by sdk/motion/bench/bevy/fog-score.py; density textures; fog volumes in a live game (LiveScene draws lights, meshes, models and panels only) |
| SSAO | covered | `<AmbientOcclusion quality thickness>` (low, medium (default), high, ultra; thickness in metres) | `camera.ssao ssao_thickness` (first frame's) → `ScreenSpaceAmbientOcclusion` (requires the depth and normal prepasses), `Msaa::Off` | no TAA, so no temporal noise: its pattern is fixed (a render equals its play); it shades ambient and environment light only |
| Depth of field | missing | | | `<DepthOfField focus aperture>` → `bevy::post_process::dof::DepthOfField` |
| Motion blur | covered | `<MotionBlur shutterAngle samples>`, on in the frames that have it: Bevy's `MotionBlur` (motion vectors from the depth and motion-vector prepasses). Bevy blurs by the motion since the previous update, and the engine may update a frame several times (until two readbacks agree) or jump to it, so with a MotionBlur in the edit every update of frame f is preceded by one of frame f - 1 (`Engine::pre_step`, its readback tagged with generation 0 and passed over), which poses meshes, skins and the camera at f - 1, and the shaders' frame counter (the blur's noise) is pinned to the frame (`engine/rdb/src/engine/motion_blur.rs`) | `camera.mb_on mb_shutter` (keyed) `mb_samples` | each frame drawn twice; Bevy's method blurs inside moving parts, not a silhouette over a still background, and toon ink hulls (no prepass) stay sharp; skinned models under it not checked by a fixture |
| Vignette, chromatic aberration, colour grading | covered | `<Grade temperature tint saturation shadows midtones highlights vignette aberration>` | `camera.grade_* vignette* aberration` (first frame's) | |
| Fisheye, lens distortion | covered | `<Fisheye strength mode fov zoom edges>`, `<LensDistortion k1 k2 zoom edges>`: raea's own pass over the 3D after tone mapping (`engine/rdb/src/engine/lens.rs`, `lens.wgsl`), radial about the centre; every value keyed per frame; a frame whose lens does nothing skips the pass (strength 0 = no lens, byte for byte) | `camera.lens_mode lens_s lens_k1 lens_k2 lens_zoom lens_fov lens_black`, keyed | the page drawn over the 3D is never bent; ink outlines are drawn before it |

### 2D, text, UI

| feature | status | the SDK today | rows | needed |
|---|---|---|---|---|
| Page layers under and over the 3D | covered | `<Canvas3D layered>`: an `<img data-raea-scene>` slot as its first child takes the 3D's frame as a texture in the page's paint order (blitz-paint `draw_texture`; `sdk/react/src/motion.rs` scene_slot, set_scene_slot, `sdk/react/src/live.rs` prepare), so page elements before it (and its holder's background) are under the 3D and positioned ones after it over it; the 3D clears to transparent (`scene_clear`) | `camera.bg #00000000` | a render forced through page files (`RAEA_MOTION_3D=files`) still draws the page over the 3D |
| Sprites | partial | 2D is the page (HTML/CSS through Blitz); `<ImagePlane>` puts a picture in 3D | `panel` | sprite sheets: `ImagePlane frame={n} grid={[cols, rows]}` → UV rect columns |
| Text in 2D | covered (by the page) | HTML text, brand fonts, captions; not `Text2d` | page | |
| Text in 3D | partial | `Text3D` (extruded solid letters) | `mesh.shape = text3d(...)` | flat text on a card: a DOM subtree painted to a `panel` exposed as `<Html3D>` |
| 2D meshes and materials | missing | use the page (SVG, CSS) | | not needed while the page draws 2D |
| UI (bevy_ui) | covered (by the page) | CSS flex and grid, not bevy_ui | page | |

### Animation, assets, audio, scenes, tools

| feature | status | the SDK today | rows | needed |
|---|---|---|---|---|
| Tweening | covered | `interpolate`, `spring`, `Easing` on any prop; values keyed per frame | keys | |
| glTF animation clips | covered | `<Model animation="Run" time loop>` posed by seeking each frame | `model.anim anim_t anim_loop` | |
| Skeletal (skinned) animation | covered | Bevy skins glTF meshes; skin weights are normalised on load as three.js does (RobotExpressive's sum to 1.75); fixture `character` (CC0 RobotExpressive from the game library, render = baked play) | `model` | |
| Clip blending, transitions | missing | one clip per Model | | `animations={[{name, time, weight}]}` → several graph nodes with weights |
| Morph targets | partial | Bevy's `morph` feature is on; untested | `model` | a fixture; `morphs={{smile: 0.5}}` → `MorphWeights` |
| glTF models | covered | `<Model src={staticFile("x.glb")}>`; loaded before the first frame; `<MeshToonMaterial>` children tone its materials; their RGBA8 textures get mip chains at load (Bevy's loader makes none, so a minified normal map speckled in the sun), trilinear and 16x anisotropic | `model.src scene` | mips for compressed (KTX2) and float textures that ship without them |
| Images | covered | `map`, `ImagePlane` (PNG, JPEG through the image crate) | `map`, `panel.src` | |
| HDR / EXR environment maps | covered | `<Environment src>` (`.hdr`, `.exr`, `.png`, `.jpg`, equirectangular) | see Environment above | KTX2 cubemaps |
| Audio | covered (by raea) | `<Audio>`, `lyria(...)`, `sao(...)`, `<Mix>`, `useGameEvents` cues; raea's mixer, not bevy_audio | `audio` rows | |
| Spatial audio | missing | | | pan and gain from a part's position: a `useSpatial(position)` hook returning `{pan, volume}` for `<Audio>` |
| Scenes (world serialization) | partial | glTF scenes; the TSX itself is the scene | | not needed |
| Gizmos | missing | | | debug only: `<Gizmo line={[a, b]} color>` → immediate-mode lines in the engine; useful for agents checking layouts |
| Picking | missing | pointer in screen pixels only | | a pure TS `pick(pointer, parts)` ray test against bounds (keeps the step pure), not `MeshPickingPlugin` |
| Physics | missing | collisions by hand in the step | | `@raea/motion/physics`: a pure, fixed-step rigid-body step (`stepBodies(state, dt)`) in TS so renders equal play; a Bevy crate would need the engine to own the state |
| Particles | missing | many small meshes or `<Entities>` | | `<Particles count seed emit>` computed in TS from the frame (deterministic), drawn as instanced meshes |
| Fire | covered | `<Fire name position scale intensity color seed speed time steps opacity light>`: a flame drawn as a volume (rising, domain-warped 3D noise in a tapering envelope, a function of `time` alone), a flickering PointLight unless `light={0}`; fixture `fire` (render = baked play, SSIM 1.000) | `fire: x y z (foot) scale_x scale_y scale_z time intensity color col_r col_g col_b seed speed steps opacity visible`, time and intensity keyed; engine/rdb/src/engine/fire.rs + fire.wgsl: a unit box standing on its foot with a custom Material (premultiplied, writes its depth where it shows), each pixel's ray marched through the box and stopped at the depth prepass when there is one | fires in a live game (LiveScene draws lights, meshes, models and panels only); the camera inside a fire's box sees nothing of it (only front faces are drawn) |
| Gaussian splats | covered | `<Splat src position rotation scale splatScale opacity brightness saturation contrast tint reveal revealFrom revealAxis revealSoft revealGlow glowColor assemble disperse wind curl explode wave twist morph pulse pulseScale pulseGlow crop bands paint seed>` (sdk/motion/src/splat.tsx): a 3DGS `.ply` (SH up to degree 3), `.splat` or `.spz`, placed by its `x.ply.json` sidecar; `raea-motion splat train CLIP -o x.ply` makes one from footage (COLMAP + Brush on Metal); fixture `sdk/motion/bench/bevy/splat` (render = baked play, SSIM 1.000) | `splat: src morph_to x y z rot_* scale_* visible morph twist wave_amp wave_len wave_phase explode disperse disperse_mode wind_* curl reveal reveal_mode reveal_* reveal_a* reveal_soft reveal_glow pulse pulse_scale pulse_glow splat_scale opacity brightness saturation contrast tint glow seed crop bands`, numbers keyed; engine/rdb/src/engine/splat.rs + splat.wgsl: one mesh of a quad a splat with a custom Material (transparent pass, depth-tested against the meshes, layer 0 so frame looks apply), the splats in storage buffers, EWA projection in the vertex shader, every effect closed-form in the row's values and a seed; sorted far to near on the CPU each frame for the scene camera from the effects' own positions (stable radix, ties by index), so no clock and no GPU sort order. bevy_gaussian_splatting 8.0.2 (Bevy 0.19) was not used: its sort runs on a wall-clock timer and only when the camera moves, its effects are stateful compute, and it wants nightly | splats in a live game (LiveScene); no depth written (depth of field focuses past them); under `<Bloom>` (HDR) the tone map shifts their trained colours; 4D (per-frame) splat sequences; SH evaluated per vertex (4x per splat), about 80 ms a frame for 1M splats at 1600x900 |

## Proposed TypeScript for the missing pieces

Each keeps the rules: declarative TSX, every value a function of the frame, no React reconcile per frame for games,
compiled to rows the engine draws. Names follow three.js and drei where agents know them.

```ts
// lighting and camera
export interface OrthographicCameraProps { position?: Vec3; lookAt?: Vec3; zoom?: number; near?: number; far?: number }
export declare function OrthographicCamera(p: OrthographicCameraProps): JSX.Element; // rows: camera.ortho zoom

// materials
export interface MeshStandardMaterialProps {
  emissiveMap?: string;                                                            // (normal, roughness, metalness, ao maps landed)
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
- The first camera in the tree is the camera. Post-processing (Bloom, Fog, ToneMapping, Grade) is the first frame's;
  the lens (Fisheye, LensDistortion) and MotionBlur are keyed per frame (a frame without one has it off).
- With MotionBlur, every update of frame f is preceded by an update of frame f - 1. Bevy takes each mesh's, skin's
  and the camera's previous pose from the last update, and raea may update a frame several times (a careful readback)
  or jump to it (a seek). Without the pre-step, the blur would depend on the path taken.
- glTF clips are posed by `time` (seconds), never advanced by a clock.
- Known difference outside the 3D rows: a page element that is partly transparent over the 3D blends in linear light
  in a render and in sRGB in play (`sdk/react/bench/corpus/fixtures/box3d`'s fading title: SSIM 0.96 at frame 30). Opaque
  page parts over 3D match.

## Performance limits (measured or read from the code)

- Each mesh row has its own `StandardMaterial`, so Bevy can't batch them; hundreds of meshes are fine, thousands are
  not. Manifold builds each distinct shape expression once (`shape::cached`), so repeated shapes are cheap to build.
- Every changing value is keyed on every frame: rows grow with frames x animated columns.
- Live play of a timeline (scene3d, motion) builds the 3D rows by stepping React through every frame on a thread before
  the 3D shows (`scene_program`); long compositions take longer to show their 3D. A game played live makes its rows
  from each frame's own state instead (`LiveScene`, sdk/react/src/motion/scene3d.rs): only the columns that changed are
  set on the engine (`Engine::set_rows`), a part seen for the first time is spawned then, and one missing 240 frames
  is dropped (`Engine::remove_rows`). sdk/react/bench/games/arena3d is its fixture.
- A point light with `castShadow` renders six shadow maps; keep shadowed point lights to one or two. Bevy allows at
  most 10 directional lights.
- `Bloom` turns the camera HDR (`#[require(Hdr)]` on `Bloom`): an extra full-size float target.
- Models load before the first frame (at most 60 s).

## Checks

- `sdk/motion/bench/bevy/check.sh`: every fixture (`character`, `lights`, `materials`, `model`, `planes`) rendered and played baked
  with its 3D on a layer, each at SSIM 1.000 at frames 0, 30 and 59.
- `sdk/react/bench/timelines/check.sh` (titles: 1.000), rdb's tests (41 pass), `sdk/motion/tools/golden.sh` 3D cases 04-06 against their frozen references (all pass),
  `bun test conformance` (126 pass).

## Existing Bevy knowledge packs

What the skills library (`apps/void/prompts/craft/bevy/`, mirrored in `sdk/skills/raea-bevy/`) took from each. Nothing was
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
