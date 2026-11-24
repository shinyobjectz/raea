---
title: "raea/check"
summary: "A check written as a system over a composition's world: what `raea check` runs for a taste line's rule."
order: 102
sources: ["sdk/raea/src/check.ts", "sdk/raea/src/facts.gen.ts", "apps/direction/src/check.rs"]
---

A check holds the work to one line. It is a system over each frame's world, the same world
[`raea world`](../cli/world.md) prints: a caption, a footage cut, a sound cue and a 3D part are entities with
components, so one check reads the same for any of them. It finds entities, measures, and calls `found` for each
thing that breaks its line; it never prints. A line in a project's taste.ts names it by its path, with its values,
and [`raea check`](../cli/check.md) runs it on every composition, on the frames it says it ranges over. A check that
reads what only a render has (`needs: "render"`) waits for one, and says so. raea's own checks are the same thing
written in Rust, under the names `text-hold`, `text-above`, `text-below`, `text-margin`, `loudness`, `still-max`,
`text-read` and `floor`.

```ts
// taste.ts holds the rule:
//   { text: "Captions clear the button band", check: "checks/caption.ts", values: { band: 0.8 } }

// checks/caption.ts
import { check, found, type Layer, type Text } from "raea/check";

export default check({ id: "caption", text: "a caption clears the platform's button band" }, (world, _frame, values) => {
  const band = Number(values.band ?? 0.8);
  for (const t of world.find({ with: ["Text", "Layer"] })) {
    if (world.component<Layer>(t, "Layer")!.medium !== "2d-type") continue;
    const box = world.component<Text>(t, "Text")!.box; // in frame fractions: left, top, right, bottom
    if (box && box[3] > band) found({ about: [t], says: `the caption reaches ${Math.round(box[3] * 100)}% down the frame` });
  }
});
```

A check keeps what it needs from frame to frame in its `memo` (empty on the first frame, and each rule's own, so two
asks that run one check don't share it), and its `after` runs once past the last frame with that memo: for what only
the whole run can say, such as how long a shot held, or that no frame had the part the check was about (a check that
finds nothing to measure must say so, or it passes for the wrong reason). A finding from `after` names what it is
about by name, since there is no world then. `named(world, id, kinds)` is how a check finds what its values name: a
part's, shot's or layer's name or a model file's, as written or as its start, in any case (`desk` is `desk-3` and
`Desk.glb`), `component:Orb` for what the Orb component drew, `file:props/orb.glb` for that model. `turned(q, v)`
turns a vector by a part's rotation, and `inside(a, b)` says how far two parts are in each other, in metres, from
their own boxes turned as they are. A note's ask names its check the same way a rule does
([`raea notes`](../cli/notes.md)); the skill carries worked checks in `craft/checks`.

```ts
// checks/shot-length.ts (the skill's craft/checks has it in full): a shot held at least N seconds
import { check, found, type Shot } from "raea/check";

export default check(
  { id: "shot-length", text: "the shot holds long enough" },
  (world, frame, values, memo) => {
    memo.fps ??= (world.resource("Frame") as { fps: number }).fps;
    for (const s of world.find({ with: ["Shot"] })) {
      const shot = world.component<Shot>(s, "Shot")!;
      if (shot.name !== String(values.shot)) continue;
      memo.shot ??= { from: shot.from, to: shot.to ?? null, last: frame };
      memo.shot.last = frame;
    }
  },
  (values, memo) => {
    if (!memo.shot) return found({ says: `no shot is named ${values.shot}`, fix: "name the shot's Sequence as the ask does" });
    const end = memo.shot.to ?? memo.shot.last + 1;
    const secs = (end - memo.shot.from) / memo.fps;
    if (secs < Number(values.seconds)) found({ about: [String(values.shot)], frames: [memo.shot.from, end], says: `the shot ${values.shot} is ${secs.toFixed(1)} s, under ${values.seconds} s` });
  },
);
```

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { check, declared, found, inside } from "raea/check"
```

`raea/check` exports 8 functions, 1 class, 38 types. Source: [sdk/raea/src/check.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/check.ts).

## Functions

### `check`

A check: what it holds the work to, the system that finds where the work doesn't hold, and what it says once the
frames are run.

```ts
function check(meta: CheckMeta, run: CheckFn, after?: AfterFn): Check
```

### `declared`

Each check's declaration, as raea check reads it before it reads any frame.

```ts
function declared(checks: Check[]): CheckMeta[]
```

### `found`

Says what the running check found. Only inside a check's system, as raea check runs it.

```ts
function found(f: Found): void
```

### `inside`

How far two parts are in each other, in metres: the least overlap of their own boxes, turned as they are, along
the axes that could part them (0: apart, or a part with no box).

```ts
function inside(a: Part, b: Part): number
```

### `named`

Whether entity `id` is one of `kinds` (a list, or one string of them split at commas): a bare kind is a part's,
shot's or layer's name or a model file's, as written or as its start ("desk" is desk-3 and Desk.glb), any case;
`component:Orb` is what the Orb component drew, and `file:props/orb.glb` what that model file is. Until a
component's taxonomy says what a thing is, an ask's check names it this way.

```ts
function named(world: World, id: number, kinds: string | string[] | undefined): boolean
```

### `runChecks`

Every planned check run on its frames' worlds: what they found, a run of frames finding the same thing as one
finding over those frames, and each check that threw, with what it threw.

```ts
function runChecks(checks: Check[], plan: Planned[], worlds: Facts[]): { findings: Finding[]; threw: { check: string; says: string; }[]; }
```

### `turned`

`v` turned by a rotation `q` (x, y, z, w), as a part's `rotation` turns its own space into the world's.

```ts
function turned(q: number[], v: number[]): number[]
```

### `worldOf`

A frame's facts as a raea/ecs world: the same components, the frame's size and rate as the `Frame` resource, and
what the render added as resources. An entity's references to others (a Covered's `layer`, a relation's `to`)
are its id in this world.

```ts
function worldOf(facts: Facts): World
```

## Classes

### `World`

A world of entities: rows in tables by kind.

| Member | What it does |
| --- | --- |
| `new World(store?: Store \| undefined)` | A world on `store`: the engine's tables when it has them, else TypeScript's (createWorld picks for you). |
| `.native: boolean` | whether the tables are the engine's (native) or TypeScript's |
| `.spawn(kind: string, cols?: Cols): number` | A new entity of `kind` with its columns; its id. |
| `.set(id: number, cols: Cols): void` | Columns set on an entity. |
| `.get(id: number): Row` | An entity's row. |
| `.despawn(id: number): void` | An entity taken out of the world, with its children. |
| `.query(kind: string, where?: Where): number[]` | The ids of `kind`'s entities whose columns pass every condition, filtered by the engine. |
| `.column(kind: string, col: string): (number \| null)[]` | One numeric column of `kind`, in id order (null where unset). |
| `.count(kind: string): number` | How many entities of `kind` there are. |
| `.rows(): Row[]` | Every row, by kind then id. |
| `.add(components: Components, opts?: { parent?: number \| undefined; }): number` | A new entity with components ({Position: {x, y}, Ball: {}}), optionally a child of `parent`. |
| `.insert(id: number, components: Components): void` | Components added to (or replacing those on) an entity. |
| `.remove(id: number, ...names: string[]): void` | Components taken off an entity. |
| `.components(id: number): Components` | An entity's components by name. |
| `.component<T = Record<string, Value>>(id: number, name: string): T \| undefined` | One component of an entity (undefined when it hasn't it). |
| `.find(f: Find): number[]` | Entities with every `with` component and no `without` one, passing every condition ("Comp.field", or a field of the first `with`), in id order: Bevy's query in the engine. |
| `.setParent(id: number, parent: number \| null): void` | An entity's parent (despawning a parent despawns its children). |
| `.parent(id: number): number \| null` | An entity's parent (null when it has none). |
| `.children(id: number): number[]` | An entity's children's ids. |
| `.resource<T = unknown>(name: string, value?: T \| undefined): T \| null` | A resource: one value for the world (set it with a value; null when unset). |
| `.send(name: string, value?: unknown): void` | An event for the systems after this one, this frame (each frame starts with none). |
| `.events<T = unknown>(name: string): T[]` | This frame's events of a name, in the order sent. |
| `.scene(): unknown[]` | What draws: Mesh + Transform entities (world space, parents composed) and lights (`<Entities world={w.at(frame)} />` hands this to the renderer). From the engine's tables natively; from the world's own store in a browser (raea-web), the same list as runtime/calls ecs.scene makes. |
| `.handle: number \| null` | The engine's id of this world's tables (for the renderer), or null under plain bun. |
| `.system(fn: System): World` | A system run on every frame, after the ones before it. |
| `.step(frame: number): void` | Run the systems once, for `frame`. |
| `.at(frame: number): World` | The world after frames 0..frame-1 have run (a copy; this world as built is the state before frame 0). |

## Types

### `About`

```ts
type About = { name: string, key?: string, }
```

### `AfterFn`

Run once after the last frame, with what the frames kept: for what only the whole run can say (how long a shot
held, that no frame had the part an ask is about). A finding here has no frames unless it gives them.

```ts
type AfterFn = (values: Values, memo: Memo) => void
```

### `Ask`

```ts
type Ask = { /** * the one thing asked */ text: string, /** * the medium the person's words name for it, in their word: "2D", "3D", "flat", "extruded", "real", * "modelled", "voxel" */ medium?: string, /** * what it is about: the part the pin is on (found in the world), or what a check would query */ about?…
```

### `Camera`

```ts
type Camera = { eye: [number, number, number], look: [number, number, number], /** * vertical field of view in degrees (a perspective camera) */ fov?: number, /** * the height it sees in metres (an orthographic camera) */ ortho?: number, near: number, far: number, /** * how its eye moves, metres a second, to t…
```

### `Check`

| Field | Type | What it is |
| --- | --- | --- |
| `run` (required) | `CheckFn` | its system, run on each frame it ranges over |
| `after` | `AfterFn` | run once past its last frame |
| `id` (required) | `string` | what `raea check --only` names, and its findings' check |
| `text` (required) | `string` | the line it holds the work to |
| `needs` | `"render"` | "render": it reads what only a render has (Contrast, Picture, Mix, Onset, Covers); with none, it waits for one |
| `level` | `Level` | how bad what it finds is, where a finding doesn't say (error when unset) |
| `frames` | `Frames` | where it runs ("all" when unset) |
| `over` | `"frames" \| "story" \| "both"` | what it reads: a composition's frames (the default), a story with no frames (`raea check NAME.story.ts` runs only these and "both", with its `after` alone), or both |

### `CheckFn`

The system: a frame's world, the frame, the rule's values, and what this run has kept.

```ts
type CheckFn = (world: World, frame: number, values: Values, memo: Memo) => void
```

### `CheckMeta`

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | what `raea check --only` names, and its findings' check |
| `text` (required) | `string` | the line it holds the work to |
| `needs` | `"render"` | "render": it reads what only a render has (Contrast, Picture, Mix, Onset, Covers); with none, it waits for one |
| `level` | `Level` | how bad what it finds is, where a finding doesn't say (error when unset) |
| `frames` | `Frames` | where it runs ("all" when unset) |
| `over` | `"frames" \| "story" \| "both"` | what it reads: a composition's frames (the default), a story with no frames (`raea check NAME.story.ts` runs only these and "both", with its `after` alone), or both |

### `Clip`

```ts
type Clip = { source: string, hash?: string, time?: number, in: number, out?: number, at: number, words: Array<Word>, }
```

### `Contrast`

```ts
type Contrast = { ratio: number, }
```

### `Covered`

```ts
type Covered = { layer: number, share: number, }
```

### `Covers`

```ts
type Covers = Array<Covered>
```

### `Cue`

```ts
type Cue = { source: string, at: number, gain?: number, bus?: string, }
```

### `Facts`

A frame's facts as `raea world --json` prints them.

| Field | Type | What it is |
| --- | --- | --- |
| `frame` (required) | `number` | the frame, from the composition's first |
| `width` (required) | `number` | the composition's width in pixels |
| `height` (required) | `number` | the composition's height in pixels |
| `fps` (required) | `number` | the composition's frames a second |
| `entities` (required) | `{ id: number; parent?: number \| null; components: Record<string, unknown> }[]` | each entity: its id, its parent's, and its components by name |
| `resources` | `Record<string, unknown>` | what is the frame's or the piece's, not an entity's (Rendered, Picture, Mix, Waits) |

### `Finding`

```ts
type Finding = { check: string, level: Level, about: Array<About>, /** * from its first frame to the frame after its last; None when it is about the whole piece */ frames: [number, number] | null, says: string, measure: Measure | null, fix: string | null, by: string, }
```

### `Found`

What a check found: the entities it is about, what it says, and, where they aren't the check's or the frame's,
its level and frames (its first, and the frame after its last); the number it turns on and the fix, when there
are.

| Field | Type | What it is |
| --- | --- | --- |
| `about` | `(number \| string)[]` | the entities it is about, by their ids in this frame's world, or by name (a check's `after` has no world) |
| `says` (required) | `string` | what is wrong, in a line a person reads |
| `level` | `Level` | its level, where it isn't its rule's or its check's |
| `frames` | `[number, number]` | its first frame and the frame after its last (this frame alone when unset) |
| `measure` | `Omit<Measure, "limit"> & { limit?: number }` | the number it turns on: its name, value and unit, and the limit it broke |
| `fix` | `string` | what to change, when there is one thing to change |

### `Frames`

The frames a check runs on: every frame; the first frame of each shot after the first, and the frame before it;
every frame a line is said or sung on; once, on the first frame (for what is the piece's, as its mix); or these.

```ts
type Frames = "all" | "cuts" | "lines" | "once" | number[]
```

### `Layer`

```ts
type Layer = { medium: Medium, /** * its place in the stack, 0 at the back */ order: number, opacity: number, /** * how it blends with what's under it (CSS mix-blend-mode), when not normally */ blend?: string, }
```

### `Level`

```ts
type Level = "error" | "warn" | "note"
```

### `Line`

```ts
type Line = { text: string, from: number, to?: number, words: Array<Word>, needs?: string, }
```

### `Measure`

```ts
type Measure = { name: string, value: number, unit: string, limit?: number, }
```

### `Medium`

```ts
type Medium = "2d-type" | "2d-graphic" | "3d" | "footage" | "page"
```

### `Memo`

What one rule's run of a check keeps from frame to frame: empty on its first frame, its own (two asks that run
one check each have theirs).

```ts
type Memo = Record<string, any>
```

### `Mix`

```ts
type Mix = { lufs?: number, peak?: number, }
```

### `Note`

```ts
type Note = { words: Array<string>, from: string, round: number, pin?: Pin, redesign?: boolean, corrected?: Array<number>, asks: Array<Ask>, }
```

### `Ocr`

```ts
type Ocr = { words: string, share: number, }
```

### `Onset`

```ts
type Onset = { at?: number, late_ms?: number, }
```

### `Part`

```ts
type Part = { /** * what draws it: "mesh", "model", "text3d" ... */ kind: string, /** * its name in the scene (`<Mesh3d name>`), or what it draws */ name: string, /** * its taxonomy tags (a component's `meta`, DR11), none until it has one */ tags: Array<string>, /** * its world position, rotation (x, y, z, w…
```

### `Picture`

```ts
type Picture = { luma: number, motion?: number, frames?: number, }
```

### `Pin`

```ts
type Pin = { t?: number, x?: number, y?: number, part?: string, }
```

### `Planned`

Which check runs on which frames with which values, as raea check plans it from a rule: the rule's level (none:
the check's own) and where the rule was written.

| Field | Type | What it is |
| --- | --- | --- |
| `check` (required) | `number` | the check, by its place in the list run |
| `frames` (required) | `number[]` | the frames it runs on |
| `values` (required) | `Values` | the rule's values, which the check's system is given |
| `level` | `Level \| null` | the rule's level, which its findings take where they don't say their own |
| `by` | `string` | where the rule was written, which its findings carry |

### `Proof`

```ts
type Proof = { shot?: string, frames?: [number, number], sign: boolean, }
```

### `Rendered`

```ts
type Rendered = { out: string, manifest: string, record?: string, stale: boolean, }
```

### `Shot`

```ts
type Shot = { name?: string, from: number, /** * None: it runs to the end */ to?: number, }
```

### `Source`

```ts
type Source = { file?: string, line?: number, component?: string, /** * the layer key as the engine has it */ key?: string, }
```

### `Text`

```ts
type Text = { words: string, font?: string, /** * its letters' height in px of a frame 1080 on its short side (None: not laid out without a render) */ size?: number, /** * its box in the frame, each 0 to 1: x0, y0, x1, y1 (None: not laid out without a render) */ box?: [number, number, number, number], extrud…
```

### `Values`

A rule's values for its check (a taste line's `values`, an ask's).

```ts
type Values = Record<string, number | string | boolean>
```

### `Waits`

```ts
type Waits = { [key in string]: string }
```

### `Word`

```ts
type Word = { text: string, start: number, end: number, }
```

<!-- generated:end -->
