---
title: "raea/ask"
summary: "Ask about a piece's picture and sound from a script: the same questions, runner and record as raea ask, typed."
order: 106
sources: ["sdk/raea/src/ask.ts"]
---

`raea/ask` asks from a script what [`raea ask`](../cli/ask.md) asks from the command line: a script run with `raea run`,
a `.ts` check, or plain Node or Bun. It runs `raea ask --batch - --json`, so every question about one clip goes in one
call, the clips are cut once, the calls go in parallel, and every answer is kept for calibration. It pays the same way:
your raea plan when the app is signed in, an agent key, or your own Cloudflare keys. It throws when nothing could be
asked, and the error says why.

```ts
import { ask, askMany } from "raea/ask";

const { answers } = ask("out/piece.mp4", "Does the music fight the voice?");
const run = askMany([
  { file: "out/a.mp4", set: "av", brief: "brief.ts" },
  { file: "out/b.mp4", std: ["defect.glitch"], window: { from: 0, to: 3 } },
]);
for (const a of run.answers) if (a.flagged) console.log(a.file, a.id, a.says, a.calibrated ? "finding" : "advice");
```

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { ask, askMany, SETS } from "raea/ask"
```

`raea/ask` exports 2 functions, 1 value, 12 types. Source: [sdk/raea/src/ask.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/ask.ts).

## Functions

### `ask`

Questions about one file: your own words (yes or no), written-out questions, or with `std`/`set` the standard ones.

```ts
function ask(file: string, question?: string | Question | (string | Question)[], more?: Omit<AskEntry, "file" | "questions"> & AskOptions): Asked
```

### `askMany`

Many files asked in one run, in parallel; throws when nothing could be asked (the error says why).

```ts
function askMany(entries: AskEntry[], o?: AskOptions): Asked
```

## Values

### `SETS`

## Types

### `Answer`

One question's answer, or why it wasn't asked or still waits: a line of `raea ask --json`.

| Field | Type | What it is |
| --- | --- | --- |
| `answer` | `string` | its id in the record, for `raea ask verdict` |
| `file` (required) | `string` | the file asked about |
| `id` (required) | `string` | the question's id |
| `version` (required) | `string` | a hash of its words, type and options: a new wording is a new version, calibrated on its own |
| `from` (required) | `number` | the window's start, in seconds |
| `to` | `number` | the window's end, in seconds (unset: the file's end) |
| `says` | `string` | the answer in words with its probability |
| `value` | `number` | yes's probability, a pick's probability, or a level |
| `problem` | `number` | 0 to 1, higher is likelier a problem (none for a pick) |
| `choice` | `string` | the option or level chosen |
| `flagged` (required) | `boolean` | past the question's threshold (its calibrated one, else even odds) |
| `calibrated` (required) | `boolean` | its labels separate well enough for its answers to be findings; until then they're advice |
| `not_asked` | `string` | why it wasn't asked (it doesn't apply to this piece, or the call failed) |
| `waits` | `string` | why it still waits (this month's pool is spent) |
| `route` | `string` | the way it was paid: your raea plan through the app, an agent key, or your Cloudflare account |
| `cached` (required) | `boolean` | answered from the cache, at no cost |

### `Asked`

A run's answers, the calls `dry` would make, and its summary.

| Field | Type | What it is |
| --- | --- | --- |
| `answers` (required) | `Answer[]` | every question's answer, or why it wasn't asked, in the order they came |
| `would` (required) | `unknown[]` | with `dry`: each call that would be made (its state, questions, media sizes and estimate) |
| `summary` (required) | `Summary` | what it came to |

### `AskEntry`

One file and what to ask of it: a line of `raea ask --batch`.

| Field | Type | What it is |
| --- | --- | --- |
| `file` (required) | `string` | the video, sound or picture asked about |
| `questions` | `(string \| Question)[]` | questions in your own words (yes or no) or written out |
| `set` | `string \| string[]` | standard sets by name: rubric, av, defects, platform, decide, make |
| `std` | `string[]` | standard questions by id (`raea ask list` lists them) |
| `window` | `Window` | the seconds to look at, for every question here (else each question's own, else the whole file) |
| `with` | `string[]` | files set beside it: to pick between, or a before and after |
| `brief` | `string` | the project's brief (else the nearest brief.ts above the file; "none": no brief, the state saying what the piece is) |
| `words` | `string` | a words file (a JSON list of the words heard), so ask knows what's said and whether anyone speaks |
| `voice` | `boolean` | whether anyone speaks in it, when no words file says |
| `state` | `string` | more for clef to read before the media: what raea measured, the platform |

### `AskOptions`

How a run goes.

| Field | Type | What it is |
| --- | --- | --- |
| `jobs` | `number` | calls at once (4 unless set) |
| `dry` | `boolean` | cut the media and say what would be sent, spending nothing |
| `bin` | `string` | the raea to run (RAEA_BIN, else raea on the PATH) |

### `Calibration`

```ts
type Calibration = { auc: number, n: number, positives: number, /** * the problem score at which it flags (10% false positives on its labels) */ threshold?: number, /** * YYYY-MM-DD */ date: string, /** * whose labels: "raea" (raea's bench, shipped) or "you" */ by: string, }
```

### `Kind`

```ts
type Kind = "yes" | "choose" | "grade" | "rate"
```

### `Picks`

```ts
type Picks = "all" | "after-first"
```

### `Question`

```ts
type Question = { /** * letters, digits, `_`, `.` and `-` */ id: string, /** * the question in words, as clef reads it */ ask: string, type?: Kind, /** * choose: its options; grade and rate: its levels, lowest first (a grade's are LEVELS unless given) */ options?: Array<string>, /** * what an option (or yes and …
```

### `Sees`

```ts
type Sees = "video" | "stills" | "nothing"
```

### `Summary`

What a run came to.

| Field | Type | What it is |
| --- | --- | --- |
| `asked` (required) | `number` | the questions sent |
| `answered` (required) | `number` | the answers read |
| `calls` (required) | `number` | the calls made (with `dry`, the calls that would be) |
| `cost` (required) | `number` | what the calls cost, in dollars |
| `estimate` (required) | `number` | with `dry`, what the calls would cost, in dollars |
| `pool_left` | `number` | what's left of this month's pool, in dollars, when the plan paid |
| `waiting` (required) | `number` | the questions waiting for the pool |
| `flagged` (required) | `number` | the answers past their threshold |
| `flagged_calibrated` (required) | `number` | the calibrated answers past their threshold: findings |
| `routes` (required) | `string[]` | the ways the calls were paid |
| `clips` (required) | `number` | the clips cut, each once |

### `When`

```ts
type When = { /** * the brief's modes it's for (pitch, story, explain, play, tool); empty is every mode */ modes?: Array<string>, /** * true: only when someone is heard speaking; false: only when no one is */ voice?: boolean, /** * true: only for a game; false: never for one */ game?: boolean, }
```

### `Window`

```ts
type Window = { from?: number, to?: number, }
```

<!-- generated:end -->
