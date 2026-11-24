---
title: "raea/fingerprint"
summary: "A fingerprint (fingerprint.ts, docs/fingerprint.md): a person's or a team's sense of good work as axes, each with a small head over on-device encoders that `raea fingerprint` trains from their labels. `raea fingerprint new NAME` writes one with raea's base taxonomy; add an axis, reword one, or remove one, and raea retrains. Rust reads the default export (systems/fingerprint/src/taxonomy.rs), so these types follow its fields."
order: 103
sources: ["sdk/raea/src/fingerprint.ts"]
---

A fingerprint (fingerprint.ts, docs/fingerprint.md): a person's or a team's sense of good work as axes, each with a small head over on-device encoders that `raea fingerprint` trains from their labels. `raea fingerprint new NAME` writes one with raea's base taxonomy; add an axis, reword one, or remove one, and raea retrains. Rust reads the default export (systems/fingerprint/src/taxonomy.rs), so these types follow its fields.

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { fingerprint } from "raea/fingerprint"
```

`raea/fingerprint` exports 1 function, 4 types. Source: [sdk/raea/src/fingerprint.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/fingerprint.ts).

## Functions

### `fingerprint`

A fingerprint (fingerprint.ts): its axes and the encoders its heads were trained on.

```ts
function fingerprint(f: Fingerprint): Fingerprint
```

## Types

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

### `Modality`

What an axis's head reads: the frames (SigLIP 2), how they change, their look, the sound (CLAP) or its events (YAMNet).

```ts
type Modality = "picture" | "change" | "look" | "sound" | "events" | "text"
```

<!-- generated:end -->
