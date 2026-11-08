# Bevy plugins in raea: how they join, and TypeScript generated from them (2026-10-18)

raea draws 3D with Bevy 0.20. Agents never write Rust: they write TSX against `raea/3d`, which becomes raea-render rows,
which raea-render's engine (`engine/render/src/engine.rs`) turns into Bevy entities. This page answers three questions:

- Can a Bevy plugin join that engine?
- Does a plugin need a TypeScript SDK?
- Can that SDK be generated from the plugin itself?

The answers come from a working prototype tried on three real plugins.

## Any plugin from TypeScript (2026-10-22)

Shane, 10-22: "at the very least we should make sure any and all plugins and tools are able to be loaded and managed
in the typescript sdk". A project now declares its plugins in package.json, and one script resolves, builds and
generates them. TypeScript then drives them two ways:
- in a scene, through the generated wrappers (render equals play, as below);
- in a program, through a headless `BevyApp` and bevy_remote's method table.

Rust is written only where a plugin's API isn't data (see "What still needs Rust").

### The declaration, and how the build resolves it

A project's package.json names its plugins under `raea.plugins` (`raea.bevy.plugins`, its first name, is still read). Each key is the plugin's name, which is also its
cargo feature and its name in raea-render's registry. The bench's declaration is `sdk/raea/bench/bevy-plugins/package.json`:

```json
"avian": {
  "crate": "avian3d",
  "versions": { "0.19": "=0.7.0", "0.20": "=0.8.0" },
  "features": ["3d", "f32", "parry-f32", "xpbd_joints", "collider-from-mesh"],
  "plugin": "avian3d::prelude::PhysicsPlugins::default()",
  "calls": "avian",
  "headless": ["assets", "mesh"]
}
```

| key | meaning |
|---|---|
| `versions` | the source for each Bevy minor: a crates.io requirement, `{git, rev}` (rev must be a commit hash), or `{path}`. `version` is one requirement for every Bevy. |
| `features` | the plugin's cargo features. Its default features are always off. |
| `plugin` | the Rust expression `app.add_plugins` takes. This is the one line of Rust a plain plugin needs, and it lives in the declaration. |
| `calls` | an escape hatch: `<name>.rs`'s `calls(app)` adds methods to the call bridge. It lives in engine/bevy-calls (raea-bevy-calls, which then owns the crate and its `deps`, behind a feature of the plugin's name) unless it needs the bridge itself (avian's), then in engine/bevy-plugins/src |
| `adapter` | an escape hatch: `<name>.rs`'s `register()` registers the plugin itself, often through a reflected component of ours that builds its unreflected types (enoki's Sparks). It lives in engine/bevy-adapters (raea-bevy-adapters, which then owns the crate and its `deps`) unless it needs raea-bevy-plugins' bridge (lightyear, hanabi), then in engine/bevy-plugins/src |
| `deps` | crates the adapter needs, behind the same feature |
| `types` | type-path prefixes to generate (default: the crate's own). A crate that re-exports a workspace of its own lists each (`bevy_flair_core::`, `bevy_flair_style::`) |
| `reflect` | `{components, resources, messages, defaults}`: public paths of types that derive Reflect but that the crate never registers (a library) or registers without `#[reflect(Component)]` (`Resource`, `Message`, or `Default` for a type that implements it, so a value may give only some fields). The plugin's install registers each with that type data, so TypeScript can insert or write it |
| `headless` | what a plain headless App needs for it (engine/bevy-plugins/src/needs.rs): `assets`, `mesh`, `shader`, `image`, `pbr`, `states`, `input`, `window` (its messages, no window), `primary-window` (a Window entity no backend opens: pancam reads its cursor), `scene` (the world assets and their spawner: vox's loader makes a scene), `picking` (pointers, hover and interaction, the mesh backend and the cameras it casts from), `gizmos` (GizmoPlugin and the skin assets it reads, nothing drawing: vleue_navigator's debug drawing); `own-transform` leaves Bevy's TransformPlugin out (big_space) |
| `onDemand` | only an App that names it gets it (a network peer's server or client half) |
| `builtin` | always compiled in. For bevy_remote, the bridge, its features sit behind a feature of its own name. A builtin whose crate the manifest already depends on by hand (`bevy` itself) adds only that feature: Bevy's optional features, in the plugin build alone. |
| `library` | no plugin to add: the crate is compiled for its reflected types and its `calls` (noise, hexx, bevy_asset_loader) |
| `widgets` | Blitz custom widgets, `{"x-gauge": "gauge::Gauge::default()"}`: each tag's elements in every page (the render's, play's and a plain App's) backed by the blitz-dom `Widget` its expression makes, registered with raea-sys-page's `register_page_plugin`. A widget crate written for Blitz has no Bevy plugin, so it's a `library` (the catalog's `swatch`, bench/bridge/widget.tsx). TSX types the tag with a `declare module "react"` IntrinsicElements entry |
| `verdict` | why a plugin's line for this Bevy is null (docs/plugins.feature PL20): the release it waits for and the date crates.io's index was checked, or what replaces it |
| `needs` | other declared plugins it needs, joined to its cargo feature (tnua's avian backend needs `avian`) |
| `proof` | why the plugin can't be opened in a headless App (it draws), so run.sh proves it in the render Engine instead |

A version of `null` (`"0.20": null`) says the plugin has no release for that Bevy yet: a build on it leaves the plugin
out, and sync names it. `features` may be lists by Bevy minor when a feature's name moved (Bevy's own do).

`tools/bevy-plugins.sh` (bash and jq) does the rest:

```sh
tools/bevy-plugins.sh sync [package.json ...]   # every declaration in the repo, merged
tools/bevy-plugins.sh build DIR                 # sync, build raea-bevy-sdk with DIR's plugins (lane), gen if behind
tools/bevy-plugins.sh gen DIR                   # DIR/bevy/<plugin>.gen.tsx and DIR/bevy/plugins.lock.json
                                                # (or in raea.generated: the catalog's go to sdk/plugins/src)
tools/bevy-plugins.sh check DIR                 # fails loudly on drift
```

**sync** merges every declaration in the repo. The same name declared two ways is an error, because one Bevy links one
version of a plugin. It picks each source for the tree's Bevy minor (read from the plugin crate's exact `bevy` pin), so
a plugin without a version for that minor is an error that names it. It then writes:
- the crate's dependencies and features, between markers in `engine/bevy-plugins/Cargo.toml`;
- `engine/bevy-plugins/src/declared.rs`, which holds each plugin's registration with raea-render, the modules its escape hatches
  live in, and the list the generator and the bridge read.

**Why a generated plugin crate.** There were three ways to resolve a declaration:
- **cargo features in a hand-written crate** (what we had): every plugin costs a hand edit of Cargo.toml and lib.rs.
- **a dynamic plugin library** (a dylib per plugin): Rust has no stable ABI. A dylib must come from the same rustc, the
  same Bevy build and the same features as the host, or TypeIds and layouts disagree. Bevy removed `bevy_dynamic_plugin`
  in 0.15 for exactly this. Every rebuild of the host would invalidate every plugin dylib, so nothing would be saved.
- **a generated plugin crate, linked statically**, which is what this does. Each plugin is an optional dependency behind
  its own feature, so:
  - the default build doesn't compile it;
  - a build's size is what its plugins add (the table under "Binary size": avian about 17 MB, tweening 0.24 MB);
  - only the plugin crates, raea-render's dependents and the final link rebuild, as long as the plugin doesn't change Bevy's
    features (see the next point).

  The upgrade cost per Bevy bump is one line per plugin in each declaration (its version for the new minor), then a
  sync. The generator, the bridge and the TS runtime are version-agnostic: they read the registry and BRP's table.

**A plugin's Bevy features unify into the plugin build's Bevy.** lightyear turns on `libm` in bevy_math and
bevy_transform, so adding it recompiles every Bevy crate for the plugin build (the default raea is unaffected,
because it doesn't link the plugin crate). Check a new plugin with:

```sh
cargo tree -p raea-cli --features bevy-plugins,raea-bevy-plugins/NAME -e normal --prefix none -f '{p} [{f}]'
```

Compare that against the build without the plugin. bevy_remote's `http` adds `async-io` to bevy_tasks and
bevy_platform, which the legacy shell's `bevy_remote` feature already turns on for the workspace build.

**Regenerated when a version moves, and loud on drift.** `gen` writes `DIR/bevy/plugins.lock.json`. It records the
Bevy minor, and for each plugin its crate, the version Cargo.lock resolved for it, and a hash of its registry schema.
`build` regenerates when the build's versions differ from the lock. `check`, which CI should run, regenerates into a
temp folder and fails when any of these differ from what's committed:
- the Bevy minor;
- a plugin's resolved version;
- a plugin's generated TypeScript;
- a plugin declared but not locked.

It prints the diff.

### What's generated

`raea-bevy-sdk --plugin NAME --prefix P...` starts raea's App with the plugins and walks its TypeRegistry
(`raea_render::engine::plugins::schema`). The walk now records each type's role from its type data: component and resource as
before, plus `event` (ReflectEvent), `message` (ReflectMessage), `state` (ReflectState and ReflectFreelyMutableState)
and `computed_state`. `<plugin>.gen.tsx` has:
- **types**: a TypeScript type for every reflected type the roots reach, with no `any`:

  | Rust | TypeScript |
  |---|---|
  | struct | interface |
  | enum | union of `"Unit"` and `{ Variant: ... }` |
  | Option | `\| null` |
  | glam vectors | tuples |

  Opaque types without a serde form are `unknown /* path */`. Asset handles and entities are `never`, and their roots
  say why.
- **TSX wrappers**: one per component and resource, as before (`<RigidBody value="Dynamic" />`).
- **settings**: `<AvianSettings Gravity={...} SubstepCount={...} />`. Each prop is one of the plugin's reflected
  resources, typed, and is set at load. A plugin's settings are its config resources; a constructor's own arguments are
  Rust (the declaration's `plugin` expression).
- **handles**: `export const avian = { RigidBody: component<RigidBody>("avian3d::..."), Gravity: resource<...>,
  ... }`. There is one handle per component, resource, event, message and state, typed by its value. They are what
  a TypeScript program uses: `app.spawn(with(avian.RigidBody, "Dynamic"))`, `app.trigger(x.SomeEvent, value)`,
  `app.observe(x.SomeEvent)`, `app.write(x.SomeMessage, value)`, `app.setState(x.SomeState, "Playing")`. A wrong value
  is a type error.

### The call bridge: bevy_remote's method table

A plugin's API that isn't a component is reached through **bevy_remote's own method registry** (`RemoteMethods`), in
process and with no HTTP. This follows Plinth's shape (RemotePlugin with `with_method` beside BRP's built-ins) and
Shane's rule to prefer managed code. A method is a Bevy system from `In<Option<Value>>` to `BrpResult`, so it runs on
the World of the frame it's asked in.

What every plugin App answers:
- BRP's built-in methods: `world.query`, `world.get_components`, `world.spawn_entity`, `world.insert_components`,
  `world.get_resources`, `world.insert_resources`, `world.trigger_event`, `world.write_message`,
  `world.observe+watch` (each call drains the events since the last), `registry.schema`, `schedule.list` and
  `rpc.discover`. Components, resources, events and messages need no plugin code at all.
- raea's methods (`engine/bevy-plugins/src/bridge.rs`):
  - `raea/state.get` and `raea/state.set`, for states through ReflectState and ReflectFreelyMutableState, which BRP
    lacks;
  - `raea/stop` and `raea/stopped`, so a BRP client can end a served session.
- a plugin's calls: `avian3d/cast_ray`, `avian3d/cast_rays`, `avian3d/collisions`; `lightyear/start`, `link`,
  `connect`, `disconnect`, `spawn`, `input`, `status`.

The bridge has two hosts:
- **`raea/bevy`** (sdk/raea/src/bevy.ts), for a TypeScript program run by `raea-bevy-sdk motion run`.
  - `BevyApp.open({ plugins, fps, serve? })` is raea-native's `bevy.app.open`. It makes a headless App
    (MinimalPlugins, TransformPlugin, what the plugins' `headless` asks for, BRP), adds exactly the plugins named
    (`raea_render::engine::plugins::install_named`), and runs Startup.
  - `app.step(n)` advances Time by one frame per step with the plugin clock advancing, so a plugin steps exactly as it
    does under raea-render's Engine.
  - `BevyApp.stepTogether(apps, n)` steps several Apps in lockstep. Hooks registered with `bridge::on_step` (such as
    lightyear's mock clock) run once per frame for all of them.
  - `app.call(method, params)` runs any method.
  - TypeScript plugins join the same App (docs/sdk.feature S6c, 10-24): `definePlugin({ name, build(app) })` and
    `PluginGroup` mirror bevy_app's, `BevyApp.open({ plugins: ["avian", hud] })` takes Rust names and TS plugins
    together, and `addPlugins`, `addSystems(Update, ...)`, `insertResource` and `addObserver` are Bevy's names
    (`raea/app` exports BevyApp as `App`).
- **`bridge::remote_call(world, name, args)`**, for a host that holds its own World. The legacy shell's element layer added
  `HostCalls::otherwise` for this, so a space's `call("avian3d/cast_ray", ...)` reaches the same methods without
  the legacy shell depending on bevy_remote. This was agreed with the legacy shell's owner on 10-22. A plugin host
  passes it to `otherwise` in its setup; none does yet.

Under raea-render's Engine (a render or a live play), a plugin's `calls` module adds BRP to the Engine's App too, so the same
methods exist there. A live game doesn't call them yet (see "Not done yet").

### The three proofs

`sdk/raea/bench/bevy-plugins/run.sh [avian|lightyear|remote]` runs them headless (`raea-bevy-sdk motion run`) and
prints PASS or FAIL with what each one measured. All three passed on 2026-10-22 (Bevy 0.19.1):

| proof | program | result |
|---|---|---|
| **avian** | `avian/raycast.ts`: a static floor, a ball dropped from y 4, a `RayCaster` entity, 90 frames | the ball rests at y 0.49997; `avian3d/cast_ray` from y 10 hits it at 9.00003 with normal [0,1,0]; with the ball in `exclude` it hits the floor at 10; the RayCaster's `RayHits` (a component, read over BRP) hold both hits; `avian3d/collisions` reports the one ball–floor start; `Gravity` reads [0,-9.81,0] |
| **lightyear** | `lightyear/peers.ts`: a server App and a client App stepped in lockstep over crossbeam | connected on the first frame; the player spawned on the server is predicted and controlled on the client 2 frames later; input held in the `Lobby` state moves nothing; with `Session` set to `Playing` on both, 60 frames of held input put the client ahead (x 3.25 at tick 77) of the server (x 3.05 at tick 73), which is prediction; once input stops both agree at x 3.25; a `Teleport` event triggered from TS moves the server's player and replicates ([5,0,-2] on both), `observe` sees it, the `Announce` message written from TS is read by the protocol's reader, and a BRP `world.query` on the replicated `Position` answers |
| **remote** (the dev tool) | `remote/devtool.ts` serves BRP on port 15703; `run.sh` drives it with curl | 30 methods listed by `rpc.discover`; two `world.query` reads of avian's `Position` show the ball falling (y 4.38, then -5.60); `registry.schema` lists 211 avian types; `raea/stop` ends the session after 189 frames |

Render = play still holds with the fixtures on the generated imports (`check.sh avian hanabi`, 10-22): avian worst SSIM
0.99971 (frame 149; 1.000000 at 0 and 30) for render against baked play and against backward seeks; hanabi worst
0.99991 against baked play and 0.999902 against seeks. tweening has no fixture (nothing in it is reflected).

bevy-inspector-egui is declared in the catalog below (with egui), but it draws into a window, and raea runs
headless. bevy_remote stays the dev tool for a headless App, and any BRP client (the curl lines in `run.sh`, or an editor that speaks BRP)
connects to a served App.

One lightyear fix the proof needed: a `RawServer` is `Started` only once its entity is `Linked`, and crossbeam has no
listening socket to link it (each pair links its own ends), so replicon's `ServerState` stayed `Stopped` and nothing
replicated. `lightyear/start` now inserts `Linked` on the server entity.


### What still needs Rust, and why

| case | why | where |
|---|---|---|
| a plugin's constructor arguments | a Plugin struct isn't reflected, so `PhysicsPlugins::new(schedule)` is the declaration's `plugin` expression | package.json |
| non-component APIs | a SystemParam's methods (`SpatialQuery::cast_ray`) can't be reached by reflection, so each is a BRP method of a few lines | `calls` module |
| events that aren't reflected | avian's `CollisionStart` derives no Reflect, so an observer buffers it for a method (reflected events need nothing) | `calls` module |
| protocol registration | lightyear serializes a replicated component by its own serde impl and keeps a typed history to roll it back (`app.component::<Position>().replicate().predict()`), so a type path isn't enough | `adapter` |
| predicted simulation | rollback re-runs FixedUpdate systems, so the simulation the client predicts must be a system in the App, not TypeScript between frames | `adapter` |
| generic plugins | `InputPlugin::<A>` and the like need a concrete type, which the adapter names | `adapter` |
| assets and closures | a `Handle<EffectAsset>` or a tween's lens isn't data, so a reflected component of ours builds it from numbers (hanabi's `Particles`) | `adapter` |
| determinism fixes | an OS seed, per-update work held to `advancing`, WinitPlugin turned off | `adapter` |
| windowed tools | bevy-inspector-egui needs a window and egui's renderer; raea-render and the headless Apps have neither, so BRP over HTTP is the dev tool | – |

### The catalog: every Bevy feature and the ecosystem's plugins (2026-10-22)

Shane, 10-22: "add all dependencies if it even might be used by a game dev or animator or anyone using bevy". The
bench's declaration (`sdk/raea/bench/bevy-plugins/package.json`) now holds 78 entries, each typed and driven from
TypeScript the same way as avian.

**Bevy's own features** are the builtin `bevy` entry: its `features` are Bevy's optional cargo features, listed per
minor, because shader_format_glsl and shader_format_wesl are gone in 0.20 and 0.20 adds bevy_curve,
complex_script_segmentation, compressed_image_saver_universal, pan_orbit_camera, pan_orbit_gizmo and render_dev_tools.
They are, by group:
- input: bevy_gilrs and gamepad (live gamepads, G3b in docs/bevy-ts.feature), picking with the mesh, sprite and UI
  backends, bevy_input_focus;
- rendering: area_light_luts (rect lights), experimental_pbr_pcss (soft shadows), every pbr_* texture, the dfg, smaa
  and tonemapping LUTs, bluenoise_texture, meshlet and meshlet_processor, bevy_solari;
- images: png, jpeg, ktx2, zstd_rust, basis-universal, exr, hdr, webp, dds, tga, bmp, gif, ico, qoi, pnm, tiff, and
  the compressed image saver;
- audio: bevy_audio with mp3, flac, vorbis, wav and aac;
- animation: bevy_animation, gltf_animation, morph and morph_animation;
- tools: bevy_gizmos, bevy_dev_tools, debug, bevy_ui_debug, bevy_debug_stepping, sysinfo_plugin, reflect_functions,
  reflect_documentation, file_watcher, embedded_watcher, asset_processor;
- UI: bevy_ui_widgets, bevy_feathers, bevy_camera_controller (free and pan cameras), the clipboard, ghost_nodes,
  system_font_discovery, bevy_settings, http and https assets.

Left off, each for a reason: dlss (NVIDIA only), hotpatching and dynamic_linking (dev builds), trace_* (profiling
builds), webgl2, webgpu and web (wasm), x11, wayland, winit and custom_cursor (raea draws headless), and glam_assert
(glam's features once broke the 0.20 build; `tools/bevy-pins.sh` on the 0.20 branch checks them).

Two of them meet raea's own systems:
- **bevy_audio** plays to the output device, not into raea's mixer, so the engine leaves its `AudioPlugin` out of
  DefaultPlugins. A TypeScript App that wants it names it: `plugins: ["bevy_audio"]`. raea's audio stays seedling on
  raea's mixer, decoded by raea-media's symphonia; the declaration's `symphonia` dep adds vorbis, ogg, adpcm, alac, aiff
  and caf to those decoders by feature unification.
- **file_watcher** makes the AssetPlugin watch its folder by default. check.sh's render-equals-play
  runs are what show it changes nothing in a render.

**The ecosystem's plugins** come from awesome-bevy's list, each with a release for Bevy 0.19 when the list was made (2026-10-22, found from crates.io's
index: the newest version whose `bevy` dependency matches the minor):

| use | plugins (declaration name) |
|---|---|
| input | leafwing, enhanced_input |
| characters and physics | tnua (on avian), avian2d, rapier3d, rapier2d, avian_pickup, ahoy, fps_controller |
| tiles and levels | tilemap, ldtk, tiled, simple_tilemap, trenchbroom, yoleck |
| assets and saving | asset_loader, common_assets, embedded_assets, obj, svg, vox, aseprite, progress, save, pkv |
| tools | egui, inspector, debugdump, console, transform_gizmo, skein (Blender) |
| drawing | outline, vector_shapes, lyon, sprite3d, spritesheet, light_2d, text3d, smud, mipmaps |
| particles | hanabi, enoki, firework |
| worlds | big_space, voxel_world, noise, noiz, hexx, proc_gen, noisy, rand |
| audio | kira (on demand: it plays to the device) |
| cameras | panorbit, pancam, editor_cam, trauma_shake |
| UI | lunex, hui, flair, text_input, ui_anchor |
| AI and paths | navigator, northstar |
| animation | tween, easings, tweening, animation_graph, lookup_curve, ik |
| network | lightyear, renet and quinnet (on demand), ggrs, http |
| story | yarn |

Most are a plain `plugin` expression. Rust was written only where the API isn't data:
- **leafwing** (`src/leafwing.rs`): its plugin is generic over the action enum, so `GameAction` names a common set (move
  and look on two axes; jump, crouch, sprint, fire, alt fire, interact, reload, pause). `leafwing/player` binds keys,
  gamepad buttons and sticks to it, and `leafwing/state` reads it.
- **enhanced_input** (`src/enhanced_input.rs`): an `OnFoot` context with the same actions; `enhanced_input/pawn` spawns
  one and `enhanced_input/state` reads its actions.
- **tnua** (`src/tnua.rs`): a scheme (walk, jump, crouch, dash) and a reflected `TnuaInput` component, so TypeScript
  steers a character by writing a component; `tnua/character` makes a body a character.
- **common_assets** (`src/common_assets.rs`): JSON, RON, TOML, YAML, MessagePack, CBOR and CSV loaded as untyped data;
  `common_assets/load` and `common_assets/get` return it to TypeScript.
- **noise** (`src/noise.rs`): `noise/sample` and `noise/grid` over the noise crate and fastnoise-lite.
- **debugdump** (`src/debugdump.rs`): `debugdump/schedules`, and `debugdump/schedule` as Graphviz dot.
- **navigator** (`src/navigator.rs`): `navigator/mesh {edges, obstacles?, mesh?}` builds (or, given `mesh`, rebuilds)
  a NavMesh in the plugin's assets from its outline and obstacles; `navigator/path {mesh, from, to}` gives `{length,
  points}` or null.
- **save** (`src/save.rs`, PL16): `save/mark {entities, unload?}` marks entities Save (or Unload); `save/save {path}`
  writes every Save entity's reflected components as RON and gives `{saved}`; `save/load {path}` despawns the Save
  and Unload entities, spawns the file's, marks them Save again and gives `{loaded}`.
- **northstar** (`src/northstar.rs`): `northstar/grid {size, walls?, chunk?, grid?}` spawns (or rebuilds) an entity
  with an OrdinalGrid3d; `northstar/path {grid, from, to, mode?}` gives `{cost, points}` or null.

The bridge gained `raea/input` (engine/bevy-plugins/src/input.rs), which presses what a headless program has no device
for, so any input plugin reads it as the real thing: `{key | mouse, pressed?}` a key or mouse button; `{gamepad,
pressed?, value?}` a button of a virtual gamepad (connected through Bevy's GamepadConnectionEvent on the first call,
whose entity every gamepad call returns) and `{axis, value}` one of its sticks; `{pointer: [x, y]}` picking's mouse
pointer in the primary window, which with `mouse` also presses or lets go there.

`raea/asset.load {path, type?}` and `raea/asset.state {id}` do the same for asset plugins
(engine/bevy-plugins/src/asset.rs): load gives a number; state gives `{state: "Loading" | "Loaded" | "Failed", type?,
error?, vertices?}`, where `type` is the asset's type path (or its loader's name for it, when it isn't reflected) and a
Mesh gives its vertex count. `type` on load picks the loader when an extension has several.

**easings** has an adapter (engine/bevy-adapters/src/easings.rs), since bevy_easings' EasingComponent is generic,
unreflected and private: `EaseTransform {to, seconds, function, repeat}` eases the entity's Transform to `to`, and
`EaseSpriteColor` its Sprite's colour, from where it is when the component is added or changed. `function` is Bevy's
EaseFunction; Linear and the 30 curves bevy_easings also has map across, and the rest ease linearly with a warning.
`repeat` is Once, Loop or PingPong. catalog/games.ts holds the game plugins' drives, with the helpers in catalog/drive.ts.

Plugins with a type parameter of the crate's own take it in the expression (`RapierPhysicsPlugin::<NoUserData>`,
`EntropyPlugin::<WyRand>`). A `library` entry (bevy_asset_loader, iyes_progress, moonshine-save, hexx, ggrs and others)
has no plugin to add, or one that only a game's own types make sense of, so it is compiled for its reflected types
and any calls.

**Which plugins stay behind their own feature.** Bevy's features go in the default build that raea and the SDK
ship. The ecosystem's plugins stay behind their declared features, as before, because each would otherwise cost every
build:
- lightyear turns on `libm` in bevy_math and bevy_transform, which changes the float results of every Bevy crate;
- rapier3d and rapier2d are a second physics engine beside avian;
- kira and bevy_audio play to the device, not into raea's mixer;
- the rest add compile time and binary size that a video render doesn't use.

**Their proof** is `run.sh catalog`. Each plugin is opened alone, with the plugins it needs, in a headless App from
`catalog/proof.ts`, in its own process, and stepped 30 frames. The ones with a TypeScript API are also driven and
checked:
- leafwing: Space presses Jump and D moves right, then Jump releases;
- enhanced_input: Space fires Jump and W moves forward;
- tnua: a character floats at 1.5, walks along +x and jumps;
- avian2d: a 2D ball rests on the floor;
- common_assets: the JSON, TOML, YAML and CSV fixtures in `catalog/data/` load;
- noise: a seed repeats, an fbm grid varies within its range, and fastnoise samples;
- debugdump: Update's graph comes back as dot.

A library with no calls passes on being compiled and generated. A plugin that draws and can't open without a renderer
says so in its declaration's `proof`; its proof is the render Engine, which the generator starts with every plugin
added, and check.sh's renders with the whole catalog in.

**Bevy 0.20.** Each entry has its 0.20 line. Thirty-two plugins have no 0.20 release yet, so their 0.20 line is
`null`: a 0.20 build leaves them out, and sync names them. They are blocked, not dropped: avian_pickup, ahoy,
fps_controller, ldtk, embedded_assets, obj, svg, pkv, transform_gizmo, outline, lyon, smud, mipmaps, firework,
editor_cam, lunex, text_input, ui_anchor, navigator, northstar, tween, easings, animation_graph, ik, trauma_shake,
hexx, noisy, voxel_world, renet, ggrs, yarn and trenchbroom. When one releases, its line becomes the version.

### Empty generated files, plugin by plugin (2026-10-22)

After the 0.20 cutover, 26 of the 46 generated files were empty: only the header and "0 roots, 0 types". There were
four causes, each fixed where it starts:
1. **The generator skipped every generic root.** The registry only holds concrete instantiations
   (`ActionState<GameAction>`, `RngSeed<WyRand>`, and in bevy.gen `Time<Fixed>`, `ButtonInput<KeyCode>`,
   `MeshMaterial3d<StandardMaterial>`), so `raea-bevy-sdk` now writes them. They are named with their arguments folded
   in: `ActionStateGameAction`.
2. **Types under a sibling crate's path.** bevy_flair's types live in bevy_flair_core and bevy_flair_style, bevy_rand's
   generator in bevy_prng, and bevy_ghx_proc_gen's components in ghx_proc_gen and ghx_grid. The declaration's `types`
   now lists each of them.
3. **Components that derive Reflect without `#[reflect(Component)]`.** The crate registers the type but never adds
   ReflectComponent, so the type has no role and TypeScript can't insert it (aseprite, hui, vector_shapes, egui). The
   new `reflect` key lists them, and the plugin's install adds the missing type data.
4. **Libraries that nothing links or registers.** A `library` entry without `calls` had no install, so its types were
   never registered (proc_gen, yoleck). With `reflect`, the library gets an install that registers them. yoleck also
   needed its `bevy_reflect` feature.

A crate whose types don't derive Reflect at all has nothing for the registry to give. Making those types usable from
TypeScript needs an upstream derive, or a remote-reflect wrapper or `calls` of our own (see "What still needs Rust").

| plugin | before → after | cause | fix / verdict |
|---|---|---|---|
| aseprite | 0 → 3 components, 1 message | `AseAnimation`, `AnimationState` and `AseSlice` are registered without `#[reflect(Component)]`, and the `AnimationEvents` message without `#[reflect(Message)]` | `reflect.components` and `reflect.messages` |
| asset_loader | 0 → 0 | no Reflect derive in bevy_asset_loader 0.28.1. Its resources are generic over a game's own state and collection types | no reflected types |
| common_assets | 0 → 0 | no Reflect derive. Its `Data` and `Row` assets are ours (`src/common_assets.rs`), reached through `common_assets/load` and `common_assets/get` | no reflected types (the calls are its API) |
| console | 0 → 0 | no Reflect derive in bevy_console 0.19 (ConsoleConfiguration, ConsoleOpen and the console events are plain types) | no reflected types |
| debugdump | 0 → 0 | bevy_mod_debugdump has no types, only functions over the App, which `debugdump/schedule` reaches | no reflected types |
| egui | 0 → 1 component, 1 resource | `EguiContextSettings` and `EguiGlobalSettings` derive Reflect and are registered, but lack `#[reflect(Component)]` and `#[reflect(Resource)]` | `reflect.components` and `reflect.resources` |
| flair | 0 → 15 components | its types live in `bevy_flair_style::` and `bevy_flair_core::`, while the prefix was `bevy_flair::` | `types: ["bevy_flair::", "bevy_flair_core::", "bevy_flair_style::"]` |
| http | 0 → 0 | no Reflect derive in bevy_http_client 0.12 (HttpClient and its requests, responses and events) | no reflected types |
| hui | 0 → 16 components | 16 public components (`HtmlNode`, `Tags`, `UiTarget`, the `OnUi*` handlers, `HtmlStyle`, the timers and others) are registered without `#[reflect(Component)]` | `reflect.components`, with paths through `bevy_hui::prelude`. The crate-private ones are left out |
| inspector | 0 → 0 | bevy-inspector-egui 0.38 adds no components or resources of its own. Its Reflect derives are test types, and it is a reader of other types' reflection | no reflected types |
| kira | 0 → 0 | no Reflect derive in bevy_kira_audio 0.27.1 (AudioChannel, SpatialAudioEmitter, AudioSettings and the rest are plain types) | no reflected types |
| leafwing | 0 → 2 components | `ActionState<GameAction>` and `InputMap<GameAction>` were registered (by `src/leafwing.rs`) but skipped as generic | the generator fix |
| lookup_curve | 0 → 0 | `LookupCurve` and its knots derive Reflect, but they make up an asset. The editor's component isn't reflected. The schema's roots are components, resources, events, messages and states, so an asset has no root | no reflected components or resources. A curve is reached through a `Handle` field, and none exists yet |
| noise | 0 → 0 | the noise crate has no Bevy types. `noise/sample` and `noise/grid` are its API | no reflected types |
| noiz | 0 → 0 | 88 Reflect derives, all noise-function value types (generic, no Component or Resource), with nothing to put on an entity | no reflected components or resources |
| proc_gen | 0 → 10 components | a library, so it had no install: nothing registered the `#[reflect(Component)]` types of ghx_proc_gen and ghx_grid, and they live under those crates' paths | `types` adds `ghx_proc_gen::`, `ghx_grid::` and `bevy_ghx_grid::`. `reflect.components` registers `ModelInstance`, `ModelInfo`, `CartesianPosition`, `GridDelta` and `CartesianGrid<Cartesian2D or 3D>` (their dependencies come with them) |
| progress | 0 → 0 | no Reflect derive in iyes_progress 0.18. Its types are generic over a game's state | no reflected types |
| quinnet | 0 → 0 | no Reflect derive in bevy_quinnet 0.22 (and it is onDemand) | no reflected types |
| rand | 0 → 2 components | `RngSeed<WyRand>` was skipped as generic, and `WyRand` is under `bevy_prng::` | the generator fix, plus `types: ["bevy_rand::", "bevy_prng::"]`. `WyRand` is opaque (`unknown`); `RngSeed` is the writable one |
| save | 0 → 0 | moonshine-save 0.8's `Save` and `Unload` markers don't derive Reflect. It saves whatever the game registers | no reflected types |
| simple_tilemap | 0 → 0 | no Reflect derive in bevy_simple_tilemap 0.21 (TileMap) | no reflected types |
| skein | 0 → 0 | bevy_skein applies glTF extras to types other crates register. Its own `SkeinPresetRegistry` isn't reflected | no reflected types |
| sprite3d | 0 → 0 | no Reflect derive in bevy_sprite3d 10.1 (Sprite3d) | no reflected types |
| tweening | 0 → 0 | the 0.20 port (2341b0c) derives Reflect only on a test asset. `TweenAnim` holds a boxed `dyn Tweenable`, and its lenses are trait objects, so they can't be reflected | no reflected types. Tweens from TypeScript need `calls` (a lens by name and its numbers) |
| vector_shapes | 0 → 10 components | the shape components (`DiscComponent`, `LineComponent`, `RectangleComponent`, `RegularPolygonComponent`, `TriangleComponent`), `ShapeMaterial`, `ShapeFill`, `Shape3d`, `ShapeOrigin` and `Canvas` are registered without `#[reflect(Component)]` | `reflect.components` |
| yoleck | 0 → 1 component, 1 resource | a library with no install, and its Reflect derives sit behind its `bevy_reflect` feature, which was off | `features` adds `bevy_reflect`. `reflect` registers `VpeolOverrideDragPlane` and `VpeolDragPlane` |

The proof, on the build: a headless App (`BevyApp.open`) spawned `procGen.CartesianPosition {x 3, y 4, z 5}` and
`yoleck.VpeolOverrideDragPlane {normal [0,1,0]}` from the generated handles and read both back unchanged. It read
`leafwing.InputMapGameAction` off `leafwing/player`'s entity and inserted `rand.RngSeedWyRand`. The ten changed
generated files, and a TSX file using `<DiscComponent>`, `<ShapeFill>`, `<HtmlNode value="menu.html">` and the
handles, type-check under strict tsc, apart from the React typings sdk/raea lacks. Ending the generic skip also
adds avian's `AncestorMarker<ColliderMarker>` and bevy.gen's concrete `Time<…>`, `ButtonInput<…>`,
`MeshMaterial3d<…>` and `MeshMaterial2d<…>` roots.

### Versions

Each plugin's pin is in `engine/bevy-plugins/Cargo.toml` (hanabi and tweening are git revs of their 0.20 ports). The 0.20 port also brings beet's prior art in reach. beet (beet_core/src/bsx) lowers markup literals
to reflected values and checks props against the TypeRegistry at run time, through 0.20's `insert_template`. The rows
here are typed JSON from TypeScript, checked by tsc and then by Bevy's TypedReflectDeserializer, so that lowering isn't
needed. On 0.20, `insert_template` could replace `ReflectComponent::insert` in `insert_reflect`.

## The short answers

**Plugins are acceptable, with conditions.** A plugin is compiled into raea behind a cargo feature and registered with
raea-render's plugin hooks. Its components are then driven from TSX by one generic row kind. To keep a render equal to its play,
a plugin must be **step-driven and deterministic**:

- its state at frame f must be f steps from the load, whatever way f was reached;
- it may not read the wall clock;
- it may not change state on updates that don't step a frame;
- it may not use randomness or threads that change results.

raea-render now provides the frame clock that makes this possible. Each plugin still has to be checked against it.
- avian3d physics passes with SSIM 1.000000 once its `parallel` feature is off.
- bevy_hanabi GPU particles pass the 0.999 gate (worst 0.99990 against play, 0.99991 against seeks) after two
  hand-written fixes. Its opaque particles match exactly; its alpha-blended ones do not.

**Yes, a plugin needs a TypeScript SDK.** Something has to write the rows, and agents only write TypeScript.

**Most of that SDK can be generated.** `raea-bevy-sdk` starts raea's own Bevy App with the plugin registered and walks
Bevy's TypeRegistry. It emits:
- a TypeScript type for every reflected component, resource and every type their fields reach;
- one typed TSX wrapper per component and per resource.

raea-render applies the wrappers' rows through `ReflectComponent::insert`, so no plugin needs a hand-written raea-render system per
component. The generator can't reach anything that isn't reflected plain data:
- assets behind `Handle<T>` (a particle effect, a mesh, a material);
- closures and trait objects (a tween's lens);
- events, observers and one-shot commands;
- the plugin's own systems and schedules.

Those parts stay hand-written, as a small adapter per plugin.

## How a plugin joins the engine today

| step | where | what |
|---|---|---|
| compile it in | `bevy-plugins/` (crate `raea-bevy-plugins`), one cargo feature per plugin | the plugin is an optional dependency; none is in the default build |
| register it | `raea_bevy_plugins::install()` calls `raea_render::engine::plugins::register` | a name, an `install(&mut App)`, and optionally a change to `DefaultPlugins` |
| build the App | `Engine::build` (engine/render/src/engine.rs) | `plugins::group` applies the DefaultPlugins changes, then `plugins::install` adds each plugin and the frame clock |
| drive it from TSX | `<BevyComponent type value from>` and `<BevyResource type value>` in `sdk/raea/src/3d.tsx`, and `<Mesh driven>` | they become `component` and `resource` rows (`runtime/scene`) |
| apply the rows | `plugins::apply_rows` at load, `timed_rows` from a frame | JSON goes through `TypedReflectDeserializer` and is inserted with `ReflectComponent::insert`; a `driven` mesh's Transform belongs to the plugin, and `sync_meshes` leaves it alone |
| step it | `plugins::arm` before every update, `Engine::catch_up` before every frame | see "Determinism" below |
| run it | raea-cli's `bevy-plugins` feature with the plugins' features builds `raea-bevy-sdk` | `raea-bevy-sdk motion ARGS` is raea with the plugins compiled in; the default `raea` binary stays plugin-free |

### Version pinning

A plugin must name the exact Bevy we ship (`=0.19.1`), or it doesn't link: two Bevys can't share one World. All three
plugins tried have releases for 0.19:

| plugin | version | Bevy | licence |
|---|---|---|---|
| avian3d | 0.7.0 | ^0.19.0 | MIT OR Apache-2.0 |
| bevy_hanabi | 0.19.0 | ^0.19 | MIT OR Apache-2.0 |
| bevy_tweening | 0.16.0 | ^0.19 | MIT OR Apache-2.0 |

They are pinned with `=` in `engine/bevy-plugins/Cargo.toml`.
- **Bevy features.** A plugin's Bevy features are unified into ours, so check each one. bevy_hanabi asks for `x11`,
  which switches on `bevy_winit`. Its `WinitPlugin` would then try to open an event loop inside raea-render's App (raea-render has no
  window and drives its own updates), so hanabi's registration turns `WinitPlugin` off through the `group` hook.
- **Default features.** Turn them off and choose the plugin's features one by one. For avian, that drops its debug
  gizmos, scene and picking support.
- **Licences.** All three are MIT OR Apache-2.0. bevy_rapier3d, the other physics option, is Apache-2.0 only.

### Render plugins and the shared device

Bevy renders on raea's paint device (`raea::quads::handles`, handed to `RenderPlugin` as `RenderCreation::manual`). A
render plugin that takes `RenderDevice` and `RenderQueue` from the render world works unchanged.
- bevy_hanabi runs its compute and render pipelines on that device. Its WGSL compiles through naga_oil, and its wgpu
  requirement (^29.0.3) unifies with ours (29.0.4).
- A plugin that creates its own wgpu device, window or surface doesn't fit. hanabi only does that in its GPU tests.
- Pipelines compile synchronously (`synchronous_pipeline_compilation: true`), so a plugin's first frame isn't missing a
  pipeline.

### Determinism: render equals play

raea-render draws a frame with as many `app.update()` calls as it needs:
- a render waits for a tagged readback;
- live play draws at display rate;
- a seek jumps.

A plugin left on Bevy's automatic time would advance by wall-clock time, a different amount on every update and in
every run. With a plugin installed, raea-render makes Bevy's clock the frame index instead (`runtime/plugins/src/lib.rs`):

- **One step per frame.** `TimeUpdateStrategy::ManualDuration` is set before every update. An update that draws frame
  f right after frame f - 1 advances Time by exactly 1/fps, with `Time<Fixed>` at the same step, so FixedUpdate runs
  exactly once. Every other update advances Time by zero. Durations are whole nanoseconds, so the steps are exact.
- **Jumps step through.** A frame reached by a jump is preceded by one update for each frame between, with their
  readbacks dropped.
- **Going back starts over.** A frame before the last one stepped rebuilds the Engine, a fresh App, from the stored
  load and steps from 0. Reloading rows into the old App wasn't enough: hanabi keeps particle state in render-world GPU
  buffers that outlive the entities.
- **Timed rows.** A component row with `from: k` is inserted on the update that draws frame k, so the steps after it
  see it. The avian fixture's crate is kicked at frame 60 this way.

So the clock keeps a plugin's state equal to "f steps from the load", if the plugin keeps the rules. What each plugin
needed:

- **avian3d.** It steps in FixedPostUpdate by `Time<Fixed>`, and has no wall clock and no randomness.
  - **Its `parallel` feature breaks render equals play.** With it on, two renders of a frame are bit-identical and so
    are two plays, but render and play differ: SSIM 0.99971 at frames 90 and 149. That passes the 0.999 gate, but a
    chaotic scene would fail it.
  - The likely cause is that each process's task pools have a different size, so the parallel solver sums in a
    different order. With `parallel` off, render, play and backward seeks are SSIM 1.000000 at every frame checked.
  - `enhanced-determinism` (libm) is only needed across machines.
- **bevy_hanabi.** Three fixes were needed:
  - It seeds its CPU random source from the OS at build time. The adapter overwrites it with a fixed `Pcg32`.
  - It re-seeds every effect's GPU random state on **every update** (`compile_effects`), so particles depended on how
    many updates a frame took. Before the fix, render vs play scored SSIM 0.85. The adapter puts
    `EffectSystems::CompileEffects` behind a run condition on `PluginClock::advancing` (hanabi.rs), one that is true only on
    stepping updates.
  - Backward seeks needed the fresh App described above.
  - After the fixes, **opaque particles match exactly** (SSIM 1.000000 on the opaque half) in renders, play and seeks.
    **Alpha-blended particles don't.** GPU atomics hand out particle slots and the sort order, so the blend order varies
    from run to run: SSIM about 0.9998 on the blended half, even between two renders.
- **bevy_tweening.** It ticks in Update by `Time::delta()`, so the frame clock would make it step exactly. It was
  compiled and registered, but there was nothing to drive from TSX (see its row in "The three plugins tried").

The rules for any plugin follow from this:
- it steps by Time, never `Instant::now()`;
- its state changes only on updates that advance Time (otherwise, a run condition on `advancing`);
- its randomness is seeded from data;
- it doesn't depend on thread count or GPU scheduling, or it renders in ways where order doesn't show (opaque, depth
  tested).

### Binary size

`raea-bevy-sdk`, release and unstripped, compared with the default `raea` (203.3 MB):

| features | size | over no plugins |
|---|---|---|
| none | 203.6 MB | 0 |
| avian | 220.8 MB | +17.1 MB (+8.4%): parry3d, nalgebra, obvhs |
| hanabi | 215.9 MB | +12.3 MB (+6.0%): naga_oil, ron, bevy_winit and winit |
| tweening | 203.9 MB | +0.24 MB |
| all three | 233.2 MB | +29.5 MB (+14.5%) |

The default build doesn't grow:
- the plugins are optional dependencies behind features;
- raea-render gained only `serde`, which was already in the tree;
- react's Cargo.lock gained 52 entries for the optional crates, none of which the default build compiles.

A plugin build with all three compiled in 2 min 47 s from a warm target directory.

## The generator

`engine/cli/src/bin/raea-bevy-sdk.rs` generates the SDK, and `runtime/plugins/src/lib.rs` holds the walk and the insert path.

1. **Start raea's App.** It registers the compiled plugins, makes a real `raea_render::engine::Engine` with one camera row, and
   reads its World.
2. **Walk the TypeRegistry** (`plugins::schema`). It takes every registered type with `ReflectComponent` or
   `ReflectResource` under a prefix (by default the compiled plugins' crates), or each type named with `--type`, and
   follows every type their fields reach. For each type it records:
   - its kind (struct, tuple struct, enum, list, map and so on);
   - its fields with their type paths, and its enum variants;
   - whether it is a component or a resource;
   - whether it has its own serde form;
   - its default, written through `TypedReflectSerializer` in exactly the JSON the deserializer reads back.

   Bevy's `reflect_auto_register` is on in our `default_app`, so every reflected type linked into the binary is
   present, which is why the walk filters by prefix. bevy_remote's `registry.schema` covers the same ground, but it
   brings an HTTP server; our walker is about 100 lines with no new dependency.
3. **Write TypeScript.** It maps the reflect-serde shapes to TypeScript:

   | Rust | TypeScript |
   |---|---|
   | struct | an interface |
   | newtype | its inner type |
   | enum | a union of `"Unit"` and `{ Variant: ... }` |
   | `Option<T>` | `T \| null` |
   | `Vec<T>` | `T[]` |
   | glam vectors and quaternions | tuples |
   | `Duration` | `{ secs, nanos }` |

   Then it writes one wrapper per component:

   ```tsx
   /** avian3d::dynamics::rigid_body::RigidBody (component) */
   export function RigidBody(props: { value?: RigidBody; from?: number }) {
     return <BevyComponent type="avian3d::dynamics::rigid_body::RigidBody" value={props.value ?? "Dynamic"} from={props.from} />;
   }
   ```

   A struct with a default takes `Partial<T>`. One without a default must be given whole, because Bevy's insert panics
   on a partial value, so `insert_reflect` checks first and refuses. A component that reaches a `Handle<T>` or an
   `Entity` is marked "Not drivable from TSX alone" in its doc comment.
4. **Apply the rows: one generic path.** A `component` row `{target, type, value, from?}` is applied to the target
   row's entity by `plugins::insert_reflect`:

   ```
   registry.get_with_type_path(type) → TypedReflectDeserializer → guard → ReflectComponent::insert
   ```

   The guard checks that FromReflect succeeds, or that the type has ReflectDefault or ReflectFromWorld. `resource` rows
   go through `insert_reflect_resource`: in 0.19 resources are components on a resource entity. A new plugin needs no
   raea-render code at all for its reflected components.

The generated TypeScript type-checks under `tsc --strict`. Generated wrappers reject wrong values: `<RigidBody
value="Bouncy" />` is TS2322. raea-render's unit tests (`engine::plugins::tests`) cover a partial struct, an enum, a resource,
refusals and the schema.

## The three plugins tried

| | avian3d 0.7.0 (physics) | bevy_hanabi 0.19.0 (GPU particles) | bevy_tweening 0.16.0 (tweens) |
|---|---|---|---|
| works in our engine | yes | yes, on the shared paint device | compiles and registers |
| render = baked play | **SSIM 1.000000** at 0, 30, 60, 61, 90, 149 (with `parallel` off) | **0.99990** worst: opaque half 1.000000, blended half 0.99981 | not exercised: nothing to drive |
| render = backward seeks | 1.000000 | 0.99991 worst, opaque half 1.000000 | – |
| generated | 93 roots, 92 wrappers, 119 types (now `sdk/plugins/src/avian.gen.tsx`, imported as `raea/plugins/avian`) | 4 roots; `ParticleEffect` blocked by its `Handle<EffectAsset>` (now `raea/plugins/hanabi`) | **0**: no type in the crate derives Reflect (`generated/bevy_tweening.tsx`) |
| hand-written | nothing beyond the 10-line registration | `Particles`, a reflected component that builds an EffectAsset from numbers; the fixed CPU seed; `CompileEffects` behind `advancing`; WinitPlugin off (`engine/bevy-plugins/src/hanabi.rs`, 90 lines) | everything: a tween holds a boxed closure built from a typed lens, which isn't data |
| fixture | `sdk/raea/bench/bevy-plugins/avian.tsx`: balls and crates on a tilted floor, gravity, restitution, friction, a kick at frame 60 | `hanabi.tsx`: an opaque and a blended fountain | – |

What the generated avian SDK looks like in use. Everything below comes from `raea/plugins/avian` (`sdk/plugins/src/avian.gen.tsx`):

```tsx
<GravityResource value={[0, -9.81, 0]} />
<Mesh name="ball0" driven position={[-1.6, 3, 0]}>
  <Sphere args={[0.35]} />
  <MeshStandardMaterial color="#e11d48" />
  <RigidBody value="Dynamic" />
  <ColliderConstructor value={{ Sphere: { radius: 0.35 } }} />
  <Restitution value={{ coefficient: 0.8, combine_rule: "Max" }} />
</Mesh>
<Mesh name="crate1" driven position={[1, 7, 0]}>
  ...
  <LinearVelocity value={[-4, 3, 0]} from={60} />
</Mesh>
```

avian's `Collider` isn't reflected (it holds a parry shape), but `ColliderConstructor` is, and avian builds the
collider from it. That is the pattern to look for in a plugin: **a reflected description that the plugin turns into its
runtime object.**

The generated set also includes avian's internals, which agents shouldn't set: `SolverBody`, `ComputedMass`, the joints'
solver data and `LastPhysicsTick`. A curated allow-list per plugin should go into the generator before agents see it.

## Generated against our hand-written components

`sdk/raea/bench/bevy-plugins/generated/bevy-own.tsx` is the generator run over the Bevy types behind our hand-written
components: Transform, the four lights, DistanceFog, Bloom, Tonemapping, Projection and Visibility. Compared with
`sdk/raea/src/3d.tsx`:

- **Units and names.**
  - Ours: three.js's names, `intensity={1}` scaled to lux or lumens, colours as hex, rotation in radians as Euler angles.
  - Generated: Bevy's raw fields. `PointLight.intensity` is in lumens (default 1,000,000), `color` is `{ LinearRgba:
    { red, green, blue, alpha } }`, and `Transform.rotation` is a quaternion.
  - An agent writes the hand-written ones correctly far more often.
- **Structure.**
  - Ours: a light's position is a prop, and a mesh's geometry and material are children.
  - Generated: a Transform is its own component, and meshes and materials are assets (`Handle<Mesh>`,
    `Handle<StandardMaterial>`) that the generator can't reach at all. `StandardMaterial` isn't a component.
- **Animation.**
  - Ours: a value that changes is a keyed column, interpolated by raea-render per frame.
  - Generated: each change of value is a new component row, deserialized and inserted on its frame. That is fine for
    settings that change now and then, and wasteful for a value that moves every frame.
- **Validation.**
  - Ours: checks ranges and says what's wrong in words.
  - Generated: checks types only (TypeScript, then the reflect deserializer).
- **Ownership.**
  - The hand-written path owns some components. `sync_meshes` writes a mesh's Transform from its rows each frame, so a
    generated `<Transform>` on a mesh that isn't `driven` is overwritten.
  - The generic path suits components that rows don't already own.

So the generated SDK is the right tool for a plugin's own data, such as physics bodies or a particle preset's numbers.
It is not a replacement for the hand-written core, which encodes units, defaults and validation an agent needs.

## What fits and what doesn't

**Fits: deterministic and step-driven.**
- **Physics** (avian3d with `parallel` off; bevy_rapier with its deterministic options): state is f fixed steps from the
  load. Its data components are reflected.
- **Procedural generation that runs at load**, and anything stepped from a seed.
- **Animation driven by time**, where the time comes from Time or is set from the frame. Note that bevy_tweening isn't
  data-driven, and `raea` already eases and springs in TypeScript.
- **Render plugins with opaque output**: post effects, outlines, shading models. They are stateless per frame, or their
  state is stepped by Time on our device.

**Fits with care: GPU simulation** (bevy_hanabi). It needs:
- a seeded CPU source;
- per-update work held to stepping updates;
- a fresh App on rewind;
- opaque or depth-tested output for exact frames.

Alpha-blended GPU particles are nondeterministic in draw order (SSIM about 0.9998), so they need a looser gate or should
be treated as non-exact.

**Doesn't fit:**
- **Wall-clock plugins**: anything reading `Instant::now()`, real-time timers, or frame-rate-dependent smoothing outside
  Time.
- **Networked plugins** (replication, matchmaking, remote assets): the frame no longer depends only on the edit.
- **Plugins that need a window or event loop** (winit-driven input, egui-style UI). raea-render drives updates and has no window.
- **Thread-count-dependent plugins**: parallel solvers or reductions whose order follows the task pool.
- **Plugins whose randomness comes from the OS, or that re-seed per update**, unless an adapter fixes it as hanabi's
  does.
- **Plugins whose state lives in assets or closures built in Rust**: their TypeScript can't be generated, only
  hand-written.

## The recommended path

1. **Keep plugins out of the default build.** Each plugin is a feature of `raea-bevy-plugins`, and its build is its own
   binary (`raea-bevy-sdk motion`) until the plugin is adopted. Adopting one means moving its feature into raea's
   defaults, after its fixture passes `sdk/raea/bench/bevy-plugins/check.sh` and its size cost is accepted.
2. **Vet each plugin against the rules**, then add a fixture compared against play and backward seeks at SSIM >= 0.999.
   Most work is finding where a plugin changes state on every update or reads the OS, and fixing it with a run
   condition or a seed, as hanabi needed.
3. **Generate the TypeScript, then curate it.** Keep the generator as the source of the types and wrappers, and add a
   per-plugin allow-list so agents see `RigidBody`, `ColliderConstructor`, `Restitution`, `Friction`, `LinearVelocity`
   and `Gravity`, not `SolverBody` or `LastPhysicsTick`. Put the result in `raea` behind the plugin's feature,
   such as a `./physics` entry, and teach it in the raea skill's 3D guide.
4. **Hand-write only the asset and event edges**, as reflected components of ours that build the plugin's runtime
   objects from numbers (hanabi's `Particles`), so the generator covers them too.
5. **Physics first.** avian3d is the best fit: deterministic, step-driven and data-described, with exact render equals
   play. GPU particles come second: opaque presets first, blended ones gated loosely. Tweening isn't worth adopting.

Live games call plugin methods since 2026-10-22: `raea/world`'s `send(method, params)` queues a call
(`bevy.world.send`), raea-render's Engine runs it at the start of its next update through engine/bevy-world (the one BRP
layer bridge.rs, the legacy shell's world.rs and raea-render share), and `answer(ticket)` reads it from the next frame
(sdk/raea/bench/bevy-ts/cases/live-plugin.tsx casts avian3d/cast_ray from a live game). A `BevyApp` program can also
run TypeScript systems in a named schedule of its frame (`app.system("Update", fn)`, `bevy.app.run`).

Not done yet:
- **Plugin constructor arguments from TypeScript**, beyond resources. A `plugin` row read at `Engine::build` would carry
  them; today they are the declaration's expression.
- **Per-plugin allow-lists.** The generated handles still include avian's solver internals.
- **A CI gate.** CI should run `tools/bevy-plugins.sh check sdk/raea/bench/bevy-plugins`, `run.sh` and `check.sh` on
  the plugin build, nightly (the plugin build recompiles Bevy).

## Running it

```sh
# resolve the declarations, build the plugin binary (its own; raea is untouched), regenerate if behind
tools/bevy-plugins.sh build sdk/raea/bench/bevy-plugins
tools/bevy-plugins.sh check sdk/raea/bench/bevy-plugins          # drift fails loudly
# the TypeScript-driven proofs: avian raycast, lightyear peers, BRP over HTTP
sdk/raea/bench/bevy-plugins/run.sh
# render equals play and seeks for the scene fixtures
sh sdk/raea/bench/bevy-plugins/check.sh avian hanabi
# a program of your own
target/release/raea-bevy-sdk motion run my-program.ts
# one plugin's TypeScript by hand, or Bevy's own types
target/release/raea-bevy-sdk --plugin avian --prefix avian3d:: -o avian.gen.tsx --schema /tmp/avian.json
# a plugin's frame clock, update by update
RAEA_PLUGIN_TRACE=1 sdk/raea/bench/bevy-plugins/motion.sh render sdk/raea/bench/bevy-plugins/avian.tsx -o /tmp/f.png --frame 30
```

Files:
- `tools/bevy-plugins.sh`: declarations to the plugin crate; build, gen and check.
- `engine/bevy-plugins/`: the plugin crate. `Cargo.toml`'s declared blocks and `src/declared.rs` are generated.
  `src/bridge.rs` is the call bridge and the headless Apps. `src/avian.rs`, `src/hanabi.rs` and `src/lightyear.rs` are
  the escape hatches.
- `runtime/plugins/src/lib.rs`: the hooks (`install_named`, on-demand plugins), the frame clock, generic reflect
  rows, the schema with roles, and unit tests.
- `engine/render/src/engine.rs`: the build and load hooks, `catch_up`, and the rebuild on rewind.
- `engine/cli/src/bin/raea-bevy-sdk.rs`: the generator (types, wrappers, settings, handles), and the plugin build's
  raea.
- `sdk/raea/src/bevy.ts`: `raea/bevy`, which holds `BevyApp`, the handles and `with`.
- `sdk/raea/src/3d.tsx`: `BevyComponent`, `BevyResource` and `Mesh driven`.
- `sdk/raea/bench/bevy-plugins/`: the declaration (package.json), generated SDKs (`bevy/`), the scene fixtures
  (`avian.tsx`, `hanabi.tsx`) with `check.sh`, and the program proofs with `run.sh`. A composition sits beside `bevy/`, not
  below it: a baked play copies only the composition's own folder, so `../bevy/` doesn't resolve there.
