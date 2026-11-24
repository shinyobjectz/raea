---
title: "raea/brief"
summary: "The specs an agent writes as data: brief, script, taste and an object's parts, checked against raea's schemas."
order: 4
sources: ["sdk/raea/src/brief.ts", "core/brief/src"]
---

A spec is a TypeScript module whose default export is data: what a project is for (`brief`), a storyboard
(`script`), the person's, the house's or a project's taste and rules (`taste`), and a frozen plan for a build (`plan`).
It renders nothing and imports nothing that renders. The types are generated from the Rust schemas
raea reads them with, so what TypeScript accepts is what raea accepts; [`raea spec`](../cli/spec.md) prints one as
checked JSON.

```ts
// brief.ts
import { brief } from "raea/brief";

export default brief({
  title: "Spring launch",
  for: "Acme Running",
  audience: "runners who already own our shoes, on their phones",
  believe: "the new sole makes an easy run feel easier",
  mode: ["pitch"],
  deliver: [{ name: "launch-9x16", specs: ["30 s", "9:16", "with sound"] }],
});
```

See [Workspaces and projects](../../concepts/workspaces.md) for where each spec lives.

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { brief, fingerprint, kit, parts } from "raea/brief"
```

`raea/brief` exports 8 functions, 34 types. Source: [sdk/raea/src/brief.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/brief.ts).

## Functions

### `brief`

A project's brief (brief.ts): what to make, for whom, the one thing to believe and its deliverables. Its tasks are
the project's plan.org.

```ts
function brief(b: Brief): Brief
```

### `fingerprint`

A fingerprint (fingerprint.ts): its axes and the encoders its heads were trained on.

```ts
function fingerprint(f: Fingerprint): Fingerprint
```

### `kit`

A kit: props in one style at real size (`style` is the paragraph every reference carries: era, palette, materials,
wear and one fictional maker). Done is a GLB, a LOD1 and a thumbnail per object, a lineup and kit.json.

```ts
function kit(m: Omit<Make, "kind">): Make
```

### `parts`

An object's parts (NAME.parts.ts), written before any geometry: each by name, its level, count and material.

```ts
function parts(p: Parts): Parts
```

### `scene`

A job made to a frozen plan and held to the review loop: `make: scene({...})` on brief.ts. Every
object has its real size and whether it came from the reference or was invented; `raea make status` decides when
it's done, and the agent's Stop hook holds it there.

```ts
function scene(m: Omit<Make, "kind">): Make
```

### `script`

A storyboard or a screenplay (script.ts): scenes, beats, lines, camera and cut.

```ts
function script(s: Script): Script
```

### `shot`

Shots of a built scene, each named for the file it lands as and described in a crew's words.

```ts
function shot(m: Omit<Make, "kind">): Make
```

### `taste`

Taste (taste.ts): standing rules, the person's, a workspace's or a project's. A rule raea can hold the work to
carries a check: one of six measured forms (`{ form: "text-above", value: 75 }`), one of raea's checks by name
(`"loudness"`), or the project's own by its path from this file (`"../checks/clear.ts"`), with its `values`.
`raea check` runs every one that applies.

```ts
function taste(t: Taste): Taste
```

## Types

### `Agent`

```ts
type Agent = { name: string, role?: string, }
```

### `Applies`

Which pieces a fingerprint judges, when a project or workspace holds more than one.

| Field | Type | What it is |
| --- | --- | --- |
| `kinds` | `string[]` | film, ad, game, playable, still, sound |
| `tags` | `string[]` | the project's tags |

### `Axis`

| Field | Type | What it is |
| --- | --- | --- |
| `name` (required) | `string` | Its name, short and lowercase (blur, clip, barren): what reports, marks and `--axis` call it. |
| `words` (required) | `string[]` | What people say for it, best first: the word head scores it from these on day one. |
| `modality` (required) | `Modality` | What its head reads. |
| `scope` | `string` | "moment" (the default), "shot" or "whole". |
| `polarity` | `"bad" \| "good"` | "bad" (the default): a high score is the defect; "good": a high score is the quality. |
| `check` | `string` | The `raea check` rule that measures it exactly, when one does: the check wins over the head. |
| `word_head` | `boolean` | False where words can't find it (silence matches no description). |
| `group` | `string` | The sense it's shown under in the app: pictures, sound or motion. |

### `Beat`

```ts
type Beat = { /** * what's seen (a screenplay's action) */ onScreen: string, /** * the move ("push in on the knife", "orbit left, keeps moving through the cut") */ camera?: string, /** * how this beat is cut into ("match on motion", "hard cut") */ cut?: string, lines?: Array<Line>, /** * sound and music cues…
```

### `Brief`

```ts
type Brief = { title: string, logline?: string, /** * who the work is for (a person or a client) */ for?: string, /** * YYYY-MM-DD */ due?: string, /** * who watches or plays it, and where */ audience?: string, /** * the one thing they should think afterwards */ believe?: string, /** * pitch, explain, story, …
```

### `Check`

```ts
type Check = { "form": "text-hold", "value": number } | { "form": "text-above", "value": number } | { "form": "text-below", "value": number } | { "form": "text-margin", "value": number } | { "form": "loudness", "value": number } | { "form": "still-max", "value": number }
```

### `Concept`

```ts
type Concept = { /** * what it is ("flight feather", "toy rocking horse") */ what: string, style: string, /** * its largest extent, in metres */ size: number, /** * how close it's seen, in metres: sets how much detail up close must hold */ seenFrom: number, material?: string, use?: string, }
```

### `Count`

```ts
type Count = number | { min: number, max: number, }
```

### `Deliverable`

```ts
type Deliverable = { name: string, /** * "90 s", "16:9", "with sound" */ specs?: Array<string>, /** * the feed item it lands as */ item?: string, /** * its own brief module, when it differs from the project in mode, look or audience */ brief?: string, /** * its script module (storyboard or screenplay) */ script?: s…
```

### `Fingerprint`

| Field | Type | What it is |
| --- | --- | --- |
| `name` (required) | `string` | Its folder's name under ~/.raea/fingerprints, or a project's or workspace's.raea/fingerprints. |
| `who` | `string` | Whose taste it is, for evidence rows and the app. |
| `about` | `string` | One line: what it tracks and when it judges ("Short ads for one brand: pace, type on screen, the brand's colours"). |
| `extends` | `string` | The fingerprint it builds on, by name: it started from that one's axes. |
| `applies` | `Applies` | Which pieces it judges when its project or workspace holds more than one. |
| `ask` | `"flags" \| "unsure"` | Its flags asked again of the decision model as it scores: "flags", or "unsure" for the moments it's unsure of too (`raea fingerprint score --ask`). Costs by the flags; each answer is clef's evidence, never yours. |
| `axes` (required) | `Axis[]` | What it notices, each axis with its words and the sense its head reads. |
| `encoders` | `Record<string, string>` | Each encoder's id and its file's sha256: heads trained on other encoders' columns are refused. |

### `Level`

```ts
type Level = "primary" | "secondary" | "tertiary"
```

### `Line`

```ts
type Line = { /** * a character, or "narrator" */ who: string, says: string, /** * a screenplay's parenthetical ("quietly") */ how?: string, }
```

### `Make`

```ts
type Make = { kind: MakeKind, /** * the pictures the job answers to (paths beside the brief): what is in it and where for a scene, the style for a kit */ references?: Array<string>, /** * one paragraph every reference prompt carries: era, palette, materials, wear, and one fictional maker */ style?: string, /…
```

### `MakeDial`

```ts
type MakeDial = MakeDialValue | { value: MakeDialValue, source?: MakeSource, /** * what the value meant for this job, in one line */ means?: string, }
```

### `MakeDialValue`

```ts
type MakeDialValue = number | string
```

### `MakeKind`

```ts
type MakeKind = "scene" | "kit" | "shot"
```

### `MakeObject`

```ts
type MakeObject = { /** * snake_case, unique in the job */ id: string, label: string, /** * real height in metres */ h?: number, /** * [width, depth, height] in metres, for boxy things (generators invent depth) */ dims?: [number, number, number], /** * largest extent in metres, for things that lie flat (tools, pap…
```

### `MakeOrigin`

```ts
type MakeOrigin = "reference" | "invented"
```

### `MakePick`

```ts
type MakePick = { /** * "tripo-h31", "meshy", ... */ generator: string, /** * one line: why this one ("sharper dial numerals; Tripo fused the handset to the cradle") */ why: string, }
```

### `MakeRoute`

```ts
type MakeRoute = "solid" | "rule" | "gen" | "library" | "picture"
```

### `MakeShot`

```ts
type MakeShot = { /** * [a-z0-9_]: the file it lands as (out/shots/<name>.png or .mp4) */ name: string, /** * in a crew's words: "slow push along the counter, rack focus to the register" */ description?: string, /** * static, dolly, truck, pedestal, crane, pan, tilt, orbit, dollyZoom, path */ move?: string, seco…
```

### `MakeSource`

```ts
type MakeSource = "customer" | "reference"
```

### `Modality`

What an axis's head reads: the frames (SigLIP 2), how they change, their look, the sound (CLAP) or its events (YAMNet).

```ts
type Modality = "picture" | "change" | "look" | "sound" | "events" | "text"
```

### `Part`

```ts
type Part = { /** * the label it carries into the world ("seat", "leaf", "gem") */ role: string, level: Level, /** * the part it sits on or in; none for the root */ parent?: string, count?: Count, /** * its own, else its parent's, else the concept's */ material?: string, style?: string, /** * what a near cro…
```

### `Parts`

```ts
type Parts = { concept: Concept, /** * by name; the build's parts are typed from these keys, so a planned part the build skips is a tsc error */ parts: { [key in string]: Part }, }
```

### `Reference`

```ts
type Reference = { target: string, note?: string, }
```

### `Rule`

```ts
type Rule = { text: string, check?: RuleCheck, /** * what a named or a .ts check is held to (`{ band: 0.8 }`); a form carries its own value */ values?: Record<string, number | string | boolean>, /** * how bad what its check finds is: error, warn or note (unset: a form's findings are errors, another check's k…
```

### `RuleCheck`

```ts
type RuleCheck = Check | string
```

### `RuleLevel`

```ts
type RuleLevel = "error" | "warn" | "note"
```

### `Scene`

```ts
type Scene = { /** * where and when ("INT. SENATE - NIGHT", or "The brief page") */ heading?: string, beats: Array<Beat>, /** * what the scene must land */ payoff?: string, /** * the story beat it plays, by its id (raea/story's compileScript writes it; docs/story.feature ST0c) */ beat?: string, }
```

### `Script`

```ts
type Script = { title: string, logline?: string, scenes: Array<Scene>, }
```

### `Section`

```ts
type Section = { heading: string, text: string, }
```

### `Taste`

```ts
type Taste = { title: string, /** * when this file is read ("Read for pitch, explain and story") */ note?: string, for?: string, /** * the kinds of work with their own taste file (motion, games, sound, ui) */ kinds?: Array<string>, sections: Array<TasteSection>, }
```

### `TasteSection`

```ts
type TasteSection = { /** * "Always", "Never", "Camera" */ heading: string, rules: Array<Rule>, }
```

<!-- generated:end -->
