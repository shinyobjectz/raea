---
title: "raea/app"
summary: "The App, its plugins and schedules, and the registry a video's compositions are kept in."
order: 3
sources: ["sdk/raea/src/app.ts"]
---

`raea/app` is Bevy's App in TypeScript. A video registers its compositions here (`registerRoot`, `<Composition>`).
A game or tool written as an App adds plugins with `definePlugin` and systems in Bevy's schedules (`Startup`,
`Update`, `FixedUpdate` and the rest, exported by name), and `BevyApp` opens a headless App you can step frame by
frame from a script or a test.

```ts
import { BevyApp, Startup, Update, definePlugin } from "raea/app";

let n = 0;
const Counter = definePlugin({
  name: "counter",
  build(app) {
    app.addSystems(Startup, () => { n = 0; });
    app.addSystems(Update, () => { n += 1; });
  },
});

const app = BevyApp.open({ plugins: [] }).addPlugins(Counter);
app.step(10);
```

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { App, BevyApp, clearRegistry, Composition } from "raea/app"
```

`raea/app` exports 1 component, 1 hook, 5 functions, 3 classes, 17 values, 18 types. Bevy's counterpart: `bevy_app`. Source: [sdk/raea/src/app.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/app.ts).

## Components

### `Composition`

Declares a composition inside the root. It draws nothing itself; a renderer finds it through registerRoot.

| Prop | Type | What it does |
| --- | --- | --- |
| `id` (required) | `string` | the composition's id: letters, digits and dashes (`raea render --composition` picks it) |
| `component` (required) | `ComponentType<P>` | the React component it renders |
| `width` (required) | `number` | in px |
| `height` (required) | `number` | in px |
| `fps` (required) | `number` | frames a second |
| `durationInFrames` (required) | `number` | its length in frames |
| `defaultProps` | `P` | the props it's rendered with (JSON-like values only) |
| `calculateMetadata` | `CalculateMetadata<P>` | its size, rate, length or props worked out from the props it's rendered with (`defaultProps` under the command line's `--props`): what it returns replaces the values above, so one composition renders a video per props file. It returns its values directly, not a promise |
| `controls` | `Controls<P>` | the composition's controls: which of its defaultProps a person may tune, and how. Each names a defaultProps key, whose value is the control's; the feed's Controls panel shows them on the timeline and a change reaches the live picture as that prop |
| `edit` | `Edit` | a hand edit laid over the composition (docs/craft-system.md, L): written back by the app's timeline |

## Hooks

### `useControl`

A control's value as the frame is drawn: the composition's prop of that name, the
value a person set on the live stage or, in a render, its defaultProps. For a part deep in the tree, so the value
needn't be passed down as a prop; the root component can read its props as any component does.

```ts
function useControl<T = unknown>(name: string): T
```

## Functions

### `clearRegistry`

Forgets the registered root and its compositions (for tests; exported from the test entry only).

```ts
function clearRegistry(): void
```

### `definePlugin`

A TypeScript plugin: `definePlugin({ name: "hud", build(app) { app.addSystems(Update,...) } })`.

```ts
function definePlugin(p: Plugin): Plugin
```

### `getCompositions`

The compositions the registered root declares, in order.

```ts
function getCompositions(): RegisteredComposition[]
```

### `getInputProps`

The props given on the command line (`raea render --props '{"n": 2}'` or `--props props.json`), anywhere in a
program; `{}` when none were. A composition's component gets them over its `defaultProps`.

```ts
function getInputProps<T extends Record<string, unknown> = Record<string, unknown>>(): T
```

### `registerRoot`

Names the root component the renderer starts from, and records its compositions. Call it once.

```ts
function registerRoot(given: ReactElement<unknown, string | JSXElementConstructor<any>> | ComponentType): void
```

## Classes

### `App`

A headless Bevy App driven from TypeScript through Bevy's remote protocol: open it with its plugins, then spawn,
insert, query and step it, and add TypeScript systems to its schedules.

| Member | What it does |
| --- | --- |
| `App.open(o: OpenOptions): BevyApp` | A headless App with these plugins; Startup has run, no frame has stepped. TypeScript plugins are built in order once the App is open, then finished and cleaned up; their Startup systems run before the first frame. |
| `App.stepTogether(apps: BevyApp[], frames?: number): number[]` | Apps stepped together, a frame at a time: a network's peers. Returns each one's frame. |
| `.methods: string[]` | the methods this App answers (BRP's, raea's and its plugins') |
| `.id: number` | the App's handle in the engine |
| `.isPluginAdded(name: string): boolean` | Whether a plugin of this name is in the App (Bevy's is_plugin_added), Rust or TypeScript. |
| `.addPlugins(...refs: PluginRef[]): BevyApp` | Adds TypeScript plugins and groups (Bevy's add_plugins): each built in order, then each finished, then each cleaned up. A Rust plugin is compiled in and must be named when the App opens; naming a new one here throws. |
| `.addSystems(schedule: string, systems: ((app: BevyApp, frame: number) => void) \| ((app: BevyApp, frame: number) => void)[], o?: SystemOptions): BevyApp` | Systems in a schedule (Bevy's add_systems), each as `system` adds one; one of the startup schedules (PreStartup, Startup, PostStartup) runs them once, before the App's next frame. Returns the App, to chain. |
| `.insertResource<T>(r: Resource<T>, value: T): BevyApp` | Bevy's insert_resource: the resource's value set (setResource). |
| `.addObserver<T>(e: string \| Event<T>, o?: { entity?: number \| undefined; then?: Then[] \| undefined; }): Observing<T>` | Bevy's add_observer: an observer of an event inside the App (observer). |
| `.step(frames?: number): number` | `frames` frames on. Returns the frame it's on. With TypeScript systems (`system`), each frame is run schedule by schedule and they run in theirs. |
| `.system(schedule: string, run: (app: BevyApp, frame: number) => void, o?: SystemOptions): () => void` | A system written in TypeScript: `run` is called every frame right after Bevy's own systems of `schedule` (one of the frame's: "First", "PreUpdate", "StateTransition", "RunFixedMainLoop", the fixed step's "FixedFirst", "FixedPreUpdate", "FixedUpdate", "FixedPostUpdate", "FixedLast" (one fixed step a frame: the clock's step is the timestep), "Update", "SpawnScene", "PostUpdate", "Last"), on that frame's World, before the next schedule's; it queries, changes and observes through the App's methods (query, get, insert, call("world.mutate_components"), observe...). Returns a function that removes it. `o` places it as Bevy's add_systems(...).run_if().before().after().in_set() would: `when` is its run condition (asked every frame before it runs; see `conditions`), `before`/`after` name other systems or sets of the same schedule it runs before or after (a cycle is refused), `set` puts it in a set that `configureSet` gives a condition and an order. Without an order, systems run in the order they were added. |
| `.configureSet(name: string, o: { when?: Condition \| undefined; before?: string \| string[] \| undefined; after?: string \| string[] \| undefined; }): void` | A set of TypeScript systems (Bevy's SystemSet with configure_sets): `when` is asked once per schedule run, before its first member, and a false skips them all; `before`/`after` order the whole set against systems or sets. |
| `.oneShot<R = void>(fn: (app: BevyApp, frame: number) => R): OneShot<R>` | A one-shot system (Bevy's register_system + run_system): `run()` runs it now, on the World as it is; `queue()` runs it once the current TypeScript system returns (as Commands::run_system applies at the next sync point), or at the end of the frame if called outside a step. `remove()` unregisters it. |
| `.frameSchedules(): string[]` | The schedules a frame runs, in order (Bevy's MainScheduleOrder). |
| `.call<A = unknown>(method: string, params?: unknown): A` | Any method of the App's table, run now: its answer, or a throw with the method's message. |
| `.spawn(...components: With[]): number` | A new entity with these components. |
| `.insert(entity: number, ...components: With[]): void` | Adds these components to an entity, replacing any it has of the same type. |
| `.get<T>(entity: number, c: Component<T>): T \| undefined` | One component of an entity, or undefined when it hasn't got one. |
| `.query(o: { data?: Component<unknown>[] \| undefined; has?: Component<unknown>[] \| undefined; without?: Component<unknown>[] \| undefined; }): { entity: number…` | Entities with every component in `has` (and none in `without`), with the `data` components' values. |
| `.resource<T>(r: Resource<T>): T` | A resource's value. |
| `.setResource<T>(r: Resource<T>, value: T): void` | Sets a resource's value, adding it if the app hasn't got one. |
| `.trigger<T>(e: Event<T>, value: T): void` | Triggers an event: its observers run now. |
| `.write<T>(m: Message<T>, value: T): void` | Writes a message, for its readers' next run. |
| `.observe<T>(e: Event<T>, entity?: number \| undefined): () => T[]` | Starts observing an event (BRP's world.observe+watch). Each call of the returned function gives the events triggered since the last call, in order. `entity` limits it to events aimed at that entity. |
| `.hook(on: "Add" \| "Insert" \| "Discard" \| "Remove" \| "Despawn", component: string \| Component<unknown>, then?: Then[]): Observing<unknown>` | A component hook as an observer inside the App: when `component` is added, inserted, discarded, removed or its entity despawned, `then`'s calls (BRP methods, "$entity" in their params standing for the entity) run right there, in the same flush, before anything else sees the World. `seen()` gives the entities it fired for since the last call; `stop()` removes it. |
| `.observer<T>(e: string \| Event<T>, o?: { entity?: number \| undefined; then?: Then[] \| undefined; }): Observing<T>` | An observer inside the App for a reflected event: on each trigger (or each aimed at `entity`), `then`'s calls run in the same flush, "$event" standing for the event's value and "$entity" for its target. |
| `.state<T>(s: State<T>): T \| null` | A state's value (null before it exists). |
| `.setState<T>(s: State<T>, value: T): void` | A state's next value: it changes at the next frame's StateTransition. |
| `.schema(crates?: string[] \| undefined): Record<string, unknown>` | The App's registered types as JSON schema (BRP's registry.schema), filtered by crate. |
| `.close(): void` | Shuts the app down; its methods fail after. |

### `BevyApp`

A headless Bevy App driven from TypeScript through Bevy's remote protocol: open it with its plugins, then spawn,
insert, query and step it, and add TypeScript systems to its schedules.

| Member | What it does |
| --- | --- |
| `BevyApp.open(o: OpenOptions): BevyApp` | A headless App with these plugins; Startup has run, no frame has stepped. TypeScript plugins are built in order once the App is open, then finished and cleaned up; their Startup systems run before the first frame. |
| `BevyApp.stepTogether(apps: BevyApp[], frames?: number): number[]` | Apps stepped together, a frame at a time: a network's peers. Returns each one's frame. |
| `.methods: string[]` | the methods this App answers (BRP's, raea's and its plugins') |
| `.id: number` | the App's handle in the engine |
| `.isPluginAdded(name: string): boolean` | Whether a plugin of this name is in the App (Bevy's is_plugin_added), Rust or TypeScript. |
| `.addPlugins(...refs: PluginRef[]): BevyApp` | Adds TypeScript plugins and groups (Bevy's add_plugins): each built in order, then each finished, then each cleaned up. A Rust plugin is compiled in and must be named when the App opens; naming a new one here throws. |
| `.addSystems(schedule: string, systems: ((app: BevyApp, frame: number) => void) \| ((app: BevyApp, frame: number) => void)[], o?: SystemOptions): BevyApp` | Systems in a schedule (Bevy's add_systems), each as `system` adds one; one of the startup schedules (PreStartup, Startup, PostStartup) runs them once, before the App's next frame. Returns the App, to chain. |
| `.insertResource<T>(r: Resource<T>, value: T): BevyApp` | Bevy's insert_resource: the resource's value set (setResource). |
| `.addObserver<T>(e: string \| Event<T>, o?: { entity?: number \| undefined; then?: Then[] \| undefined; }): Observing<T>` | Bevy's add_observer: an observer of an event inside the App (observer). |
| `.step(frames?: number): number` | `frames` frames on. Returns the frame it's on. With TypeScript systems (`system`), each frame is run schedule by schedule and they run in theirs. |
| `.system(schedule: string, run: (app: BevyApp, frame: number) => void, o?: SystemOptions): () => void` | A system written in TypeScript: `run` is called every frame right after Bevy's own systems of `schedule` (one of the frame's: "First", "PreUpdate", "StateTransition", "RunFixedMainLoop", the fixed step's "FixedFirst", "FixedPreUpdate", "FixedUpdate", "FixedPostUpdate", "FixedLast" (one fixed step a frame: the clock's step is the timestep), "Update", "SpawnScene", "PostUpdate", "Last"), on that frame's World, before the next schedule's; it queries, changes and observes through the App's methods (query, get, insert, call("world.mutate_components"), observe...). Returns a function that removes it. `o` places it as Bevy's add_systems(...).run_if().before().after().in_set() would: `when` is its run condition (asked every frame before it runs; see `conditions`), `before`/`after` name other systems or sets of the same schedule it runs before or after (a cycle is refused), `set` puts it in a set that `configureSet` gives a condition and an order. Without an order, systems run in the order they were added. |
| `.configureSet(name: string, o: { when?: Condition \| undefined; before?: string \| string[] \| undefined; after?: string \| string[] \| undefined; }): void` | A set of TypeScript systems (Bevy's SystemSet with configure_sets): `when` is asked once per schedule run, before its first member, and a false skips them all; `before`/`after` order the whole set against systems or sets. |
| `.oneShot<R = void>(fn: (app: BevyApp, frame: number) => R): OneShot<R>` | A one-shot system (Bevy's register_system + run_system): `run()` runs it now, on the World as it is; `queue()` runs it once the current TypeScript system returns (as Commands::run_system applies at the next sync point), or at the end of the frame if called outside a step. `remove()` unregisters it. |
| `.frameSchedules(): string[]` | The schedules a frame runs, in order (Bevy's MainScheduleOrder). |
| `.call<A = unknown>(method: string, params?: unknown): A` | Any method of the App's table, run now: its answer, or a throw with the method's message. |
| `.spawn(...components: With[]): number` | A new entity with these components. |
| `.insert(entity: number, ...components: With[]): void` | Adds these components to an entity, replacing any it has of the same type. |
| `.get<T>(entity: number, c: Component<T>): T \| undefined` | One component of an entity, or undefined when it hasn't got one. |
| `.query(o: { data?: Component<unknown>[] \| undefined; has?: Component<unknown>[] \| undefined; without?: Component<unknown>[] \| undefined; }): { entity: number…` | Entities with every component in `has` (and none in `without`), with the `data` components' values. |
| `.resource<T>(r: Resource<T>): T` | A resource's value. |
| `.setResource<T>(r: Resource<T>, value: T): void` | Sets a resource's value, adding it if the app hasn't got one. |
| `.trigger<T>(e: Event<T>, value: T): void` | Triggers an event: its observers run now. |
| `.write<T>(m: Message<T>, value: T): void` | Writes a message, for its readers' next run. |
| `.observe<T>(e: Event<T>, entity?: number \| undefined): () => T[]` | Starts observing an event (BRP's world.observe+watch). Each call of the returned function gives the events triggered since the last call, in order. `entity` limits it to events aimed at that entity. |
| `.hook(on: "Add" \| "Insert" \| "Discard" \| "Remove" \| "Despawn", component: string \| Component<unknown>, then?: Then[]): Observing<unknown>` | A component hook as an observer inside the App: when `component` is added, inserted, discarded, removed or its entity despawned, `then`'s calls (BRP methods, "$entity" in their params standing for the entity) run right there, in the same flush, before anything else sees the World. `seen()` gives the entities it fired for since the last call; `stop()` removes it. |
| `.observer<T>(e: string \| Event<T>, o?: { entity?: number \| undefined; then?: Then[] \| undefined; }): Observing<T>` | An observer inside the App for a reflected event: on each trigger (or each aimed at `entity`), `then`'s calls run in the same flush, "$event" standing for the event's value and "$entity" for its target. |
| `.state<T>(s: State<T>): T \| null` | A state's value (null before it exists). |
| `.setState<T>(s: State<T>, value: T): void` | A state's next value: it changes at the next frame's StateTransition. |
| `.schema(crates?: string[] \| undefined): Record<string, unknown>` | The App's registered types as JSON schema (BRP's registry.schema), filtered by crate. |
| `.close(): void` | Shuts the app down; its methods fail after. |

### `PluginGroup`

Plugins added together, as Bevy's PluginGroup: `group.set(p)` replaces the member of p's name, `disable(name)`
leaves one out, and the App adds the rest in order.

| Member | What it does |
| --- | --- |
| `new PluginGroup(name: string, plugins: (string \| Plugin \| { readonly plugin: string; })[])` | A group named `name` of these plugins, in the order the App adds them. |
| `.name: string` | the group's name |
| `.set(p: Plugin): PluginGroup` | Replaces the member of `p`'s name with `p`. |
| `.disable(name: string): PluginGroup` | Leaves the member of this name out. |
| `.enable(name: string): PluginGroup` | Puts a disabled member back. |
| `.build(): (string \| Plugin)[]` | The plugins to add, in order, without the disabled ones. |

## Values

### `conditions`

Bevy's common run conditions (bevy_ecs::schedule::common_conditions, bevy_state's in_state, bevy_time's on_timer),
as Conditions for `system(..., { when })` and `configureSet`. Each call makes a fresh condition with its own memory
(runOnce, resourceChanged, onTimer).

### `First`

The frame's schedules, in the order Bevy runs them (with StateTransition and the fixed step's inside).

### `FixedFirst`

The fixed step's schedules, run once a frame at the clock's step.

### `FixedLast`

### `FixedPostUpdate`

### `FixedPreUpdate`

### `FixedUpdate`

### `Last`

### `PostStartup`

### `PostUpdate`

### `PreStartup`

Bevy's schedules by their labels, for `app.addSystems(Update,...)` as in Rust.

### `PreUpdate`

### `RunFixedMainLoop`

### `SpawnScene`

### `Startup`

### `StateTransition`

### `Update`

The frame's main schedules: Update for game logic, then SpawnScene, PostUpdate and Last.

## Types

### `CalculateMetadata`

What `calculateMetadata` is given and may return.

```ts
type CalculateMetadata = (given: { defaultProps: P; props: P; compositionId: string }) => { durationInFrames?: number; width?: number; height?: number; fps?: number; props?: P; } | void
```

### `ChoiceControl`

One of a list: a string or a number from `options`.

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `"choice"` | what it is: one of a list |
| `options` (required) | `readonly (string \| number)[]` | what it may be, in the order the panel lists them (at least one, none twice) |
| `label` | `string` | what the panel calls it (the key when left out) |
| `group` | `string` | the panel's group it sits in ("Grade", "Camera"); ungrouped controls come first |
| `cost` | `ControlCost` | how a change reaches the picture: "live" unless said |
| `about` | `string` | one line on what it changes, for the panel's tooltip and an agent reading the code |

### `ColourControl`

A colour: a hex string, "#rgb", "#rgba", "#rrggbb" or "#rrggbbaa".

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `"colour"` | what it is: a colour |
| `label` | `string` | what the panel calls it (the key when left out) |
| `group` | `string` | the panel's group it sits in ("Grade", "Camera"); ungrouped controls come first |
| `cost` | `ControlCost` | how a change reaches the picture: "live" unless said |
| `about` | `string` | one line on what it changes, for the panel's tooltip and an agent reading the code |

### `CompositionProps`

The props of [`Composition`](#composition), tabled there.

### `Condition`

A run condition: asked on the frame's World before a system or set runs; false skips it this frame.

```ts
type Condition = (app: BevyApp, frame: number) => boolean
```

### `Control`

```ts
type Control = NumberControl | SwitchControl | ChoiceControl | ColourControl | TextControl
```

### `ControlCost`

How a change to a control reaches the picture. `live` (the default): the running stage draws it at once, as a
prop. `reopen`: the stage opens the file again first, for what a running 3D world keeps from its first frame (a
shape, a model's file, an instance count, a picture map, a shader). `resim`: everything from the frame it changes
on is worked out again, for state carried from frame to frame (physics, particles, a game's fold).

```ts
type ControlCost = "live" | "reopen" | "resim"
```

### `Controls`

A composition's controls, by the defaultProps key each one tunes.

```ts
type Controls = { readonly [K in keyof P & string]?: Control }
```

### `Edit`

A hand edit: `segments` are the composition's own frame ranges, [from, to), played one after another, so a trim,
a split, a cut word or a moved span is a change to this list; the length becomes their sum.

| Field | Type | What it is |
| --- | --- | --- |
| `segments` | `readonly (readonly [number, number])[]` | the composition's own frame ranges, [from, to), played one after another |
| `scale` | `number` | the composition drawn at this fraction of its size (above 0, at most 1), what is where kept: it is laid out at its width and height (useVideoConfig still says them) and the whole drawn smaller, so a 3D view traces fewer pixels. A draft's, where a smaller picture changes nothing that ships: the app's composition drafts and a scene's kept frames draw at 0.5 |

### `NumberControl`

A number, dragged on a slider when it has both ends.

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `"number"` | what it is: a number |
| `min` | `number` | the least it may be |
| `max` | `number` | the most it may be |
| `step` | `number` | what one step of the slider or an arrow key moves it by (above 0) |
| `unit` | `string` | its unit, shown after it ("px", "°", "s") |
| `label` | `string` | what the panel calls it (the key when left out) |
| `group` | `string` | the panel's group it sits in ("Grade", "Camera"); ungrouped controls come first |
| `cost` | `ControlCost` | how a change reaches the picture: "live" unless said |
| `about` | `string` | one line on what it changes, for the panel's tooltip and an agent reading the code |

### `OpenOptions`

| Field | Type | What it is |
| --- | --- | --- |
| `plugins` (required) | `PluginRef[]` | the App's plugins, in order: Rust ones by the name this build registered them under (a declaration's name, or its halves: lightyear_server), compiled in and added before Startup; TypeScript ones (definePlugin) built after |
| `fps` | `number` | frames a second: one step is 1/fps of Time (default 60) |
| `serve` | `number` | serve BRP over HTTP on this port (a dev tool, a BRP client, curl) |

### `Plugin`

A plugin written in TypeScript, as bevy_app's Plugin trait: `build` adds its systems, resources, observers and
states to the App; once every plugin is built, each one's `finish` runs, then each one's `cleanup`, as App::run
does. Unique by `name` unless `isUnique` is false (Bevy panics on a unique plugin added twice; this throws).

| Field | Type | What it is |
| --- | --- | --- |
| `name` (required) | `string` | the plugin's name: two unique plugins of one name can't both be added |
| `build` (required) | `(app: BevyApp) => void` | adds the plugin's systems, resources and other plugins to the app |
| `finish` | `((app: BevyApp) => void) \| undefined` | called once every plugin is built, for setup that needs the others |
| `cleanup` | `((app: BevyApp) => void) \| undefined` | called after every plugin's finish |
| `isUnique` | `boolean` | whether adding it twice is an error (true) |

### `PluginRef`

A plugin of any author: a Rust plugin in raea's plugin build by its declared name (package.json raea.plugins:
"avian", or a half: "lightyear_server") or by its generated handles (`avian` from avian.gen.tsx, which carries
`plugin: "avian"`), a TypeScript Plugin, or a group of them.

```ts
type PluginRef = string | { readonly plugin: string } | Plugin | PluginGroup
```

### `RegisteredComposition`

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | the composition's id: letters, digits and dashes (`raea render --composition` picks it) |
| `component` (required) | `ComponentType<any>` | the React component it renders |
| `width` (required) | `number` | in px |
| `height` (required) | `number` | in px |
| `fps` (required) | `number` | frames a second |
| `durationInFrames` (required) | `number` | its length in frames |
| `defaultProps` (required) | `Record<string, unknown>` | the props it's rendered with (JSON-like values only) |
| `controls` (required) | `Readonly<Record<string, RegisteredControl>>` | its controls, by defaultProps key, in the order they're declared ({} when it has none) |

### `RegisteredControl`

A control as a registered composition holds it: its cost said.

```ts
type RegisteredControl = Control & { cost: ControlCost }
```

### `SwitchControl`

On or off: a boolean.

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `"switch"` | what it is: on or off |
| `label` | `string` | what the panel calls it (the key when left out) |
| `group` | `string` | the panel's group it sits in ("Grade", "Camera"); ungrouped controls come first |
| `cost` | `ControlCost` | how a change reaches the picture: "live" unless said |
| `about` | `string` | one line on what it changes, for the panel's tooltip and an agent reading the code |

### `SystemOptions`

| Field | Type | What it is |
| --- | --- | --- |
| `name` | `string` | its name, for other systems' before/after |
| `set` | `string` | the set it's in (configureSet) |
| `when` | `Condition` | its run condition |
| `before` | `string \| string[]` | systems or sets (by name) it runs before |
| `after` | `string \| string[]` | systems or sets (by name) it runs after |

### `TextControl`

A string: a title, a caption, a name.

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `"text"` | what it is: a string |
| `multiline` | `boolean` | a box of several lines rather than one |
| `maxLength` | `number` | the most characters it may hold |
| `label` | `string` | what the panel calls it (the key when left out) |
| `group` | `string` | the panel's group it sits in ("Grade", "Camera"); ungrouped controls come first |
| `cost` | `ControlCost` | how a change reaches the picture: "live" unless said |
| `about` | `string` | one line on what it changes, for the panel's tooltip and an agent reading the code |

<!-- generated:end -->
