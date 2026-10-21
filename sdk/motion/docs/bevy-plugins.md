# Bevy plugins in raea: how they join, and TypeScript generated from them (2026-10-18)

raea draws 3D with Bevy 0.19.1. Agents never write Rust: they write TSX against `@raea/motion/3d`, which becomes rdb rows,
which rdb's engine (`engine/rdb/src/engine.rs`) turns into Bevy entities. This page answers three questions:

- Can a Bevy plugin join that engine?
- Does a plugin need a TypeScript SDK?
- Can that SDK be generated from the plugin itself?

The answers come from a working prototype tried on three real plugins.

## The short answers

**Plugins are acceptable, with conditions.** A plugin is compiled into raea behind a cargo feature and registered with
rdb's plugin hooks. Its components are then driven from TSX by one generic row kind. To keep a render equal to its play,
a plugin must be **step-driven and deterministic**:

- its state at frame f must be f steps from the load, whatever way f was reached;
- it may not read the wall clock;
- it may not change state on updates that don't step a frame;
- it may not use randomness or threads that change results.

rdb now provides the frame clock that makes this possible. Each plugin still has to be checked against it.
- avian3d physics passes with SSIM 1.000000 once its `parallel` feature is off.
- bevy_hanabi GPU particles pass the 0.999 gate (worst 0.99990 against play, 0.99991 against seeks) after two
  hand-written fixes. Its opaque particles match exactly; its alpha-blended ones do not.

**Yes, a plugin needs a TypeScript SDK.** Something has to write the rows, and agents only write TypeScript.

**Most of that SDK can be generated.** `raea-bevy-sdk` starts raea's own Bevy App with the plugin registered and walks
Bevy's TypeRegistry. It emits:
- a TypeScript type for every reflected component, resource and every type their fields reach;
- one typed TSX wrapper per component and per resource.

rdb applies the wrappers' rows through `ReflectComponent::insert`, so no plugin needs a hand-written rdb system per
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
| register it | `raea_bevy_plugins::install()` calls `rdb::engine::plugins::register` | a name, an `install(&mut App)`, and optionally a change to `DefaultPlugins` |
| build the App | `Engine::build` (engine/rdb/src/engine.rs) | `plugins::group` applies the DefaultPlugins changes, then `plugins::install` adds each plugin and the frame clock |
| drive it from TSX | `<BevyComponent type value from>` and `<BevyResource type value>` in `sdk/motion/src/3d.tsx`, and `<Mesh driven>` | they become `component` and `resource` rows (`sdk/react/src/motion/scene3d.rs`) |
| apply the rows | `plugins::apply_rows` at load, `timed_rows` from a frame | JSON goes through `TypedReflectDeserializer` and is inserted with `ReflectComponent::insert`; a `driven` mesh's Transform belongs to the plugin, and `sync_meshes` leaves it alone |
| step it | `plugins::arm` before every update, `Engine::catch_up` before every frame | see "Determinism" below |
| run it | react's features `avian`, `hanabi`, `tweening` build `raea-bevy-sdk` | `raea-bevy-sdk motion ARGS` is raea-motion with the plugins compiled in; the default `raea-motion` binary stays plugin-free |

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
  which switches on `bevy_winit`. Its `WinitPlugin` would then try to open an event loop inside rdb's App (rdb has no
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

rdb draws a frame with as many `app.update()` calls as it needs:
- a render waits for a tagged readback;
- live play draws at display rate;
- a seek jumps.

A plugin left on Bevy's automatic time would advance by wall-clock time, a different amount on every update and in
every run. With a plugin installed, rdb makes Bevy's clock the frame index instead (`engine/rdb/src/engine/plugins.rs`):

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
    `EffectSystems::CompileEffects` behind `rdb::engine::plugins::advancing`, a run condition that is true only on
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

`raea-bevy-sdk`, release and unstripped, compared with the default `raea-motion` (203.3 MB):

| features | size | over no plugins |
|---|---|---|
| none | 203.6 MB | 0 |
| avian | 220.8 MB | +17.1 MB (+8.4%): parry3d, nalgebra, obvhs |
| hanabi | 215.9 MB | +12.3 MB (+6.0%): naga_oil, ron, bevy_winit and winit |
| tweening | 203.9 MB | +0.24 MB |
| all three | 233.2 MB | +29.5 MB (+14.5%) |

The default build doesn't grow:
- the plugins are optional dependencies behind features;
- rdb gained only `serde`, which was already in the tree;
- react's Cargo.lock gained 52 entries for the optional crates, none of which the default build compiles.

A plugin build with all three compiled in 2 min 47 s from a warm target directory.

## The generator

`sdk/react/src/bin/raea-bevy-sdk.rs` generates the SDK, and `engine/rdb/src/engine/plugins.rs` holds the walk and the insert path.

1. **Start raea's App.** It registers the compiled plugins, makes a real `rdb::engine::Engine` with one camera row, and
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
   rdb code at all for its reflected components.

The generated TypeScript type-checks under `tsc --strict`. Generated wrappers reject wrong values: `<RigidBody
value="Bouncy" />` is TS2322. rdb's unit tests (`engine::plugins::tests`) cover a partial struct, an enum, a resource,
refusals and the schema.

## The three plugins tried

| | avian3d 0.7.0 (physics) | bevy_hanabi 0.19.0 (GPU particles) | bevy_tweening 0.16.0 (tweens) |
|---|---|---|---|
| works in our engine | yes | yes, on the shared paint device | compiles and registers |
| render = baked play | **SSIM 1.000000** at 0, 30, 60, 61, 90, 149 (with `parallel` off) | **0.99990** worst: opaque half 1.000000, blended half 0.99981 | not exercised: nothing to drive |
| render = backward seeks | 1.000000 | 0.99991 worst, opaque half 1.000000 | – |
| generated | 93 roots, 92 wrappers, 119 types (`sdk/motion/bench/bevy-plugins/avian/avian3d.tsx`) | 4 roots; `ParticleEffect` blocked by its `Handle<EffectAsset>` (`hanabi/bevy_hanabi.tsx`) | **0**: no type in the crate derives Reflect (`generated/bevy_tweening.tsx`) |
| hand-written | nothing beyond the 10-line registration | `Particles`, a reflected component that builds an EffectAsset from numbers; the fixed CPU seed; `CompileEffects` behind `advancing`; WinitPlugin off (`engine/bevy-plugins/src/hanabi.rs`, 90 lines) | everything: a tween holds a boxed closure built from a typed lens, which isn't data |
| fixture | `sdk/motion/bench/bevy-plugins/avian/edit.tsx`: balls and crates on a tilted floor, gravity, restitution, friction, a kick at frame 60 | `hanabi/edit.tsx`: an opaque and a blended fountain | – |

What the generated avian SDK looks like in use. Everything below comes from `avian3d.tsx`:

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

`sdk/motion/bench/bevy-plugins/generated/bevy-own.tsx` is the generator run over the Bevy types behind our hand-written
components: Transform, the four lights, DistanceFog, Bloom, Tonemapping, Projection and Visibility. Compared with
`sdk/motion/src/3d.tsx`:

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
  - Ours: a value that changes is a keyed column, interpolated by rdb per frame.
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
  data-driven, and `@raea/motion` already eases and springs in TypeScript.
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
- **Plugins that need a window or event loop** (winit-driven input, egui-style UI). rdb drives updates and has no window.
- **Thread-count-dependent plugins**: parallel solvers or reductions whose order follows the task pool.
- **Plugins whose randomness comes from the OS, or that re-seed per update**, unless an adapter fixes it as hanabi's
  does.
- **Plugins whose state lives in assets or closures built in Rust**: their TypeScript can't be generated, only
  hand-written.

## The recommended path

1. **Keep plugins out of the default build.** Each plugin is a feature of `raea-bevy-plugins`, and its build is its own
   binary (`raea-bevy-sdk motion`) until the plugin is adopted. Adopting one means moving its feature into raea-motion's
   defaults, after its fixture passes `sdk/motion/bench/bevy-plugins/check.sh` and its size cost is accepted.
2. **Vet each plugin against the rules**, then add a fixture compared against play and backward seeks at SSIM >= 0.999.
   Most work is finding where a plugin changes state on every update or reads the OS, and fixing it with a run
   condition or a seed, as hanabi needed.
3. **Generate the TypeScript, then curate it.** Keep the generator as the source of the types and wrappers, and add a
   per-plugin allow-list so agents see `RigidBody`, `ColliderConstructor`, `Restitution`, `Friction`, `LinearVelocity`
   and `Gravity`, not `SolverBody` or `LastPhysicsTick`. Put the result in `@raea/motion` behind the plugin's feature,
   such as a `./physics` entry, and teach it in the raea-bevy skill.
4. **Hand-write only the asset and event edges**, as reflected components of ours that build the plugin's runtime
   objects from numbers (hanabi's `Particles`), so the generator covers them too.
5. **Physics first.** avian3d is the best fit: deterministic, step-driven and data-described, with exact render equals
   play. GPU particles come second: opaque presets first, blended ones gated loosely. Tweening isn't worth adopting.

Not done yet:
- live games with plugins on void's stage, where input drives a stepping plugin: the frame clock steps by the frames
  void draws, but this wasn't tested;
- events from plugins back to TypeScript, such as collision events for game logic or sound;
- per-plugin allow-lists;
- a CI gate for the plugin build.

## Running it

```sh
# the plugin build (its own binary; raea-motion is untouched)
cd react && lane build -- cargo build --release --features avian,hanabi,tweening --bin raea-bevy-sdk
# TypeScript from the registry: by plugin, or Bevy's own types
sdk/react/target/release/raea-bevy-sdk --prefix avian3d:: -o sdk/motion/bench/bevy-plugins/avian/avian3d.tsx
sdk/react/target/release/raea-bevy-sdk --type bevy_light::point_light::PointLight --schema /tmp/schema.json
# render equals play and seeks for the plugin fixtures
sh sdk/motion/bench/bevy-plugins/check.sh
# a plugin's frame clock, update by update
RAEA_PLUGIN_TRACE=1 sdk/motion/bench/bevy-plugins/motion.sh render sdk/motion/bench/bevy-plugins/avian/edit.tsx -o /tmp/f.png --frame 30
```

Files:
- `engine/rdb/src/engine/plugins.rs`: hooks, the frame clock, generic reflect rows, the schema, unit tests.
- `engine/rdb/src/engine.rs`: build and load hooks, `catch_up`, rebuild on rewind.
- `bevy-plugins/`: the plugins and their hand-written adapters.
- `sdk/react/src/bin/raea-bevy-sdk.rs`: the generator, and the plugin build's raea-motion.
- `sdk/motion/src/3d.tsx`: `BevyComponent`, `BevyResource` and `Mesh driven`.
- `sdk/react/src/motion/scene3d.rs`: the component and resource rows.
- `sdk/motion/bench/bevy-plugins/`: fixtures, generated SDKs and `check.sh`.
