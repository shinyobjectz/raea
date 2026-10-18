# Playtests and the creative judge

Two jobs share one machine. A playtest finds out whether a game an agent built works: whether it can be finished,
whether it softlocks, and where players die or get stuck. A judge finds out whether a piece is decent or good, in any
medium, with verdicts that hold steady from run to run. Both reduce to the same three steps:

1. Run the piece deterministically and keep everything that happened as facts.
2. Prepare exactly the media a question needs. That is a view: its modality, its frames and its layer.
3. Ask each question of the cheapest eye that can answer it. A rule answers from the facts with no model call.
   clef-omni answers from the prepared media.

Only two models are used (the founder's rule, 2026-10-18). **clef-omni decides**: every playtest move and every judgement
goes to it. **GLM 5.3 Flash** (`z-ai/glm-5.3-flash` on OpenRouter) writes free text where a design needs some, such as
an issue's description or the fix an agent reads, and never decides. No MiniMax, Anthropic or Gemini models are used.
checks.rs's `vote` eye calls a Gemma writer and void's reviewer calls Gemini, so neither is reused as it stands; game
packs use rules and clef only.

The prototype is in `react/bench/playtest/`; the results it found are listed at the end of this document.

## What games give us

A game in @raea/motion is a pure fold, `step(state, input, frame) -> state` (motion/src/games.ts). Three properties
follow, and the whole design leans on them:

- **A bot can step a game headlessly.** No renderer is involved: a bot calls the same `step` that `useGameState` is
  given. In Bun on JavaScriptCore the platformer steps about 4 million frames a second.
- **States are values, so any state can be restored.** Search (Go-Explore, beam, MCTS) can go back to any state it has
  seen without replaying from frame 0.
- **Every session replays exactly.** A run is an input timeline, `{"0": {"keys": [...]}, ...}`, which is the format
  `raea-motion render --input` takes and the format `play --input-log` writes. Any moment of any bot's session renders
  as stills or a clip, and the pixels are the play's (react/bench/games/replay.sh).

### The optional `playtest` export

A game author exports what a playtester needs beside the view. The type is in motion/src/games.ts and is exported as
`Playtest` from `@raea/motion`:

```ts
import type { Playtest } from "@raea/motion";
export const playtest: Playtest<Game> = {
  initial: INITIAL,
  step: fold,                                   // the same fold useGameState is given
  actions: [["ArrowRight"], ["ArrowRight", "ArrowUp"], ["ArrowUp"], [], ["ArrowLeft"], ["ArrowLeft", "ArrowUp"]],
  observe: (s, frame) => ({ x, y, onGround, lives, groundEndsIn, gapWidth, walkerDx, ... }),  // plain JSON a model reads
  progress: (s) => s.x / GOAL_X,                // 0..1: search follows it; "stuck" means it stopped rising
  outcome: (s) => (s.x >= GOAL_X ? "won" : null),
  events: { died: (a, b) => ..., coin: (a, b) => b.score > a.score },   // named moments, as useGameEvents takes them
};
```

The import is type-only, so the export adds no runtime dependency, and the engine's bundle is unchanged apart from
one unused object. `observe` is the cleanest observation there is: it is typed and written by the one who knows the
game, and it costs nothing to compute. `progress` and `outcome` are what turn "the bot played" into "the level can be
finished". The author is the only one who knows what the goal is. The platformer had no goal of its own, which is
itself a finding.

**Fallback when a game has no export.** The bot still has `step` if the module exports its fold. If it does not, the
fold can be pulled out of the bundle by wrapping `useGameState`, because the hook receives `(initial, step)`. Progress
then comes from pixels and events: novelty measured on downscaled frames (a perceptual hash of each render as the
Go-Explore cell), `useGameEvents` frames as events, and clef asked "did the player win, lose, or neither?" only at the
end of a session. This fallback is designed but not built yet. It is much slower, because each cell costs a render,
and it is the reason the export is worth asking every game agent to write.

## Playtest tiers

### Tier (i): search bots with no model (CI)

These bots are deterministic, given a seeded RNG, and cheap: seconds on one core. They report:

- **Finished or not, and in how many frames.** A beam search runs over macro-actions (each action held 6 or 14
  frames), scored by `progress` minus a penalty per death and deduplicated by cell. It stops once 2400 frames of play
  go by without new ground.
- **The reachable set and the ceiling.** Go-Explore keeps the first arrival at every cell (x and y in 48-px buckets,
  plus whether the player is on the ground). It returns to rarely chosen cells, explores randomly from them, and gives
  the highest progress any state reached.
- **Death spots.** Every bot's `died` events are bucketed by position.
- **Stuck spots.** These are cells the explorer chose at least 8 times without finding anything new from them.
- **Softlocks.** From a spread of 24 reachable states, a small beam tries to get back to 98% of the ceiling. A state
  that can't is a softlock: it is reachable, and the game can't be continued from it.
- **A random bot.** It is the floor any search has to beat, and it shows where careless play dies.

The output is `report.json`, `run.input.json` (the winning run, or the furthest one), `trace.jsonl` (each frame's keys,
observation, progress and events) and `moments.json` (the run's events, plus the stuck spots, each with its own input
log). That output is everything the later tiers need.

MCTS is the next search to add, for games whose progress signal is deceptive (keys and doors, backtracking). Beam with
novelty covers platformers and pong. MCTS also slots into `search.ts` without any change to the interface.

### Tier (ii): a model in the loop

A few times a second of game time, the model gets the frame the run has reached and the game's observation, and it
picks the next move. The bot steps the fold with that move for `hold` frames, and then the loop repeats. Each frame is
rendered by the engine from the input so far, so the model sees what a player would see.

This tier is for what search can't do: playing as a person would, noticing that a goal is unclear, and judging whether
a game is fun to try rather than whether it can be finished. It is not for proving that a game can be finished. Search
is far better at that, and free. The spike results are below.

### Tier (iii): a judge pass

A check pack runs on the run. Its rules read the report and the trace. clef looks at the views, which are the run's
interesting moments rendered from its input log, clean or with debug overlays. The pack, the views and the manifest
format are described next.

## The action interface: not free JSON mode

My recommendation is to use **a closed menu of actions**, not free-form JSON. clef-omni is not a chat model. It answers
typed questions about media: `noul` is a probability of yes, and `choice` is a distribution over named options, with a
confidence. The natural action interface is therefore one `choice` question whose options are the game's
`playtest.actions`, with names such as `run_right` and `jump_right`. The reasons:

- **It can't be malformed.** A JSON-mode reply can be unparseable, invent keys that don't exist, give a hold of 900
  frames, or wrap itself in prose. A choice is always one of the options.
- **It returns a distribution, not one sample.** A probability for every move is free, so a tester can take the argmax,
  sample from it, or treat a low `confidence` as "the model is unsure here". An unsure model at a spot is itself a
  playtest finding (a goal or hazard that doesn't read). A `why` field in JSON mode is prose, which can't be measured.
- **The action space is the game's.** `actions` already lists the key chords worth trying, and the search bots use the
  same list. Hold time is better fixed per tester (0.25 s worked for the platformer) or offered as a second choice
  (`short`, `long`) than left as a free number.
- **It matches how checks.rs already asks clef**, so one client (judge.ts `clef()`) serves the player and the judge.

Free JSON has no job in the loop. Under the two-model rule, GLM 5.3 Flash writes only prose about a session (an
issue's description, the fix an agent reads), from the facts and clef's answers, after the session is over. The
decisions stay with clef. If a JSON-writing model were ever put in the loop, it would need a schema with an enum for
`keys` and an enum for `hold`, which is the same closed menu with more failure modes.

## clef-omni: what it can do (verified 2026-10-18)

The DSL is written in clef-omni's own terms, so its surface comes first. These limits come from Cloudflare's model
page and its JSON schemas
([model page](https://developers.cloudflare.com/workers-ai/models/clef-omni/),
[input schema](https://developers.cloudflare.com/workers-ai/models/clef-omni/schema-input.json),
[output schema](https://developers.cloudflare.com/workers-ai/models/clef-omni/schema-output.json)). Each one marked
"probed" was confirmed by a call. The probe run made 31 calls (8 of them refused before billing) and cost $0.0022.

**Question types.** There are exactly three, and clef refuses any other `type` (probed). It returns no free text.

| clef type | Parameters | Answer | Limits |
|---|---|---|---|
| `noul` | `instructions` (a string, or an object or array holding the question and referenced data), and optionally `criteria: {true, false}` describing what yes and no mean | `{type, noul}`: the probability of yes. There is no confidence field (probed). | none beyond the shared ones |
| `choice` | `instructions`, and `criteria`: a map from option id to a description (a string, object, array, or null) | `{type, choice, probabilities, confidence}`: the top option, a probability for each option summing to 1, and a confidence | 2 to 255 options (1 and 256 refused, probed) |
| `score` | `instructions`, and `criteria`: an ordered array of level descriptions, lowest first | `{type, score, legend, probabilities, confidence}`: the probability-weighted level (it can fall between levels, with 0 the lowest), the levels by index, a probability per level, and a confidence | 2 to 10 levels (1 and 11 refused, probed) |

**The request.**

| Field | What it takes | Limits |
|---|---|---|
| `model` | `"clef-omni"` | required |
| `state` | text, or JSON (an object or array). An empty string is accepted (probed). | required (a call without it is refused, probed). It is truncated to fit the context. |
| `questions` | a map from id to question; all are answered in one call | 1 to 64 per call (65 refused, probed). Ids match `^[A-Za-z0-9_.-]{1,100}$` (a space is refused, probed). |
| `images` | data URLs or `{content_type, base64}` (both forms probed); PNG, JPEG or WebP | at most 4 (5 refused, probed). 4 MiB and 16 megapixels each, 8 MiB in all. 64 to 1,024 tokens each: one token per 32x32 patch after resizing to between 256x256 and about 1 megapixel, plus 3. |
| `videos` | `video/*` data URLs (mp4 and webm both probed), sampled at 2 frames a second. The soundtrack is heard when every video in the request has one. | at most 2 (3 refused, probed). 60 s each (62 s refused, probed), 16 MiB each. Up to 256 tokens a second (144 at 480p). |
| `audio` | `audio/*` data URLs (wav and mpeg both probed) | at most 4. 8 MiB and 300 s each, and audio and video together at most 16 MiB (documented, not probed). About 780 tokens a minute. |
| other fields | none: an extra field such as `temperature` is refused (probed) | |

The model has a 64,000-token context; media plus questions over that fails the request. Input costs $0.15 per million
tokens, and output is free. Behaviour, from the probes:

- Image, audio and video can all go in one call (probed).
- Audio is really heard: "Is the clip silent?" gave p=0.95 on silence and 0.06 on a tone.
- The same request gives the same answer: a repeated call matched to four decimal places, and so did three judge runs.
- clef has no sampling parameters, so it can't be made to vary.

## Views: preparing exactly what the model sees

A check gets the right modality and the right frames because a view prepares them, and the check names the view. The
design has three layers.

### 1. Views, in TypeScript beside the game (`edit.playtest.ts`)

```ts
import { clean, debug } from "@raea/motion/playtest";
const death      = clean("death", { when: (m) => m.name === "died", window: [-90, 30], as: "clip", stride: 4, limit: 1 });
const deathDebug = debug("death.debug", ["hitboxes", "state"], { when: (m) => m.name === "died", window: [-90, 30], as: "clip", stride: 4 });
const stuck      = debug("stuck", ["hitboxes", "state"], { when: (m) => m.name === "stuck", window: [0, 0], as: "stills" });
const finish     = clean("finish", { when: (m) => m.name === "finish", window: [-120, 30], as: "clip" });
const text       = clean("session.text", { when: "session", as: "text", stride: 30 });
const look       = clean("session.look", { when: "session", as: "sheet", stride: 120 });
```

- **`when`** says when to capture. It is a moment predicate (a game event such as `died` or `coin`, or a moment the
  bots found: `stuck`, `furthest`, `finish`), a state predicate over trace rows (`{ frame: (row) => row.lives === 1 }`,
  which picks the first frame of each stretch where it holds), or `"session"` for the whole run.
- **`window`** gives the frames before and after the moment. Markers are therefore timed by the run itself: a death at
  frame 380 gives frames 290 to 410, whichever run produced it.
- **The layer is the constructor.** `clean(...)` gives a `View<"clean">`. `debug(name, overlays, ...)` gives a
  `View<"debug">` and needs at least one named overlay.
- **`as`** says what the view makes, and so which clef input it becomes:
  - `text`: the trace rows as JSON, sent as `state` only.
  - `stills`: up to three images (the window's ends and the moment).
  - `sheet`: frames tiled into one image.
  - `clip`: a video, x264 at 480 wide and crf 30 as clef was calibrated in checks.rs, at most 60 s. `playtest()` checks
    a moment clip's window, and `views.ts` checks a session clip when it renders.
  - `audio`: the window's mix alone, at most 300 s. This is designed but not built, because LEDGE has no sound.

Views are evaluated over the run deterministically. `views.ts` writes the media and a manifest, `views.json`. Each
entry gives the view, the instance, the moment, the frame span, the modality, the layer and overlays, the media paths,
and the facts at the span's ends.

### 2. Debug overlays as a separate layer

A game draws its overlays in the view, inside `<Overlay name="hitboxes">…</Overlay>` (motion/src/overlay.tsx). They
render only in a render that asks for them by name. They are absent from the clean render and from live play. The same
frame of the same run therefore renders clean for a taste question and annotated for a diagnosis, and the two differ
only by the overlays. **A taste or quality question must name a clean view**, and the types enforce it: a question marked
`taste: true` accepts only a `View<"clean">`, so passing it a debug view doesn't compile.

The engine side needs no change today. The render asks for overlays by setting `globalThis.__RAEA_OVERLAYS__` before
the game runs. `views.ts` writes a two-line wrapper module beside the edit, `_playtest-<pid>-<hash>.tsx`, which imports
a module that sets the global and then imports `./edit.tsx`. It renders the wrapper with `render --at a,b,c` (one engine
process per input log and overlay set) and deletes it afterwards. The smallest engine change that would make this
first-class is a `--overlay NAME` flag (repeatable) on `render` and `bundle`, which would add
`globalThis.__RAEA_OVERLAYS__=[...]` to the start of `bundle.js` in `bundle_composition` (react/src/motion.rs). That is
about 6 lines, and it removes the temporary file. I did not make the change, because it needs a rebuild of raea-motion
while react/src/motion.rs holds other sessions' uncommitted work.

A `path` overlay (the bot's trail, or where other bots died) needs data from the run, not just the state. It would go
in through the same mechanism as a second global, `__RAEA_PLAYTEST__ = { trail, deaths }`, read by an
`<Overlay name="path">` the game or SDK draws.

### 3. Facts as rows in rdb

Each frame's state fields, input and events (`trace.jsonl`), the moments, and the report are rows. rdb already holds a
render's facts (`raea-motion render -o OUT.sql`), so the playtest's rows go beside them as a `playtest_frame` table
(frame, keys, the observation's columns, progress, events) and a `playtest_moment` table, and they are queried with
rdb's SQL. Rules are SQL over those rows: "finished" is `SELECT max(progress) >= 1`, and "death loop" is a GROUP BY on
the death x. A check's `context` keys pick the rows that go into clef's state paragraph. There is no second query
language. Datalog would add a language to learn and buy nothing that SQL over one run's rows lacks. The prototype keeps
the rows as JSONL. Loading them into rdb is the next step.

### Why not YAML, Datalog or a vitest-style runner

- **YAML views** can't hold predicates (`when: m => m.name === "died" && m.at.x > 5000`) without becoming a weak
  expression language. Views are code because moments are code, and they sit next to the game in the same language as
  the game.
- **Datalog for facts:** as above, rdb's SQL already exists, and one run's trace is small.
- **A vitest-style runner:** plain assertions ("it finishes", "no softlock") are rule checks in the pack. They give the
  same pass and fail, plus a fix the agent reads, plus calibration. A test runner would duplicate the pack format and
  would have no place for model checks with thresholds.

## Questions: a DSL in clef's own types, compiled to void's check packs

Questions are written in TypeScript beside the views (motion/src/playtest.ts, imported as `@raea/motion/playtest`).
Each primitive is one clef question type, so nothing is translated from a free-form design:

| clef type | DSL primitive | Reads as | Faults when |
|---|---|---|---|
| `noul` | `yes(id, ask, { view, fault: "yes" \| "no", threshold?, means?: {yes, no} })` | the probability of yes; `means` becomes noul's `criteria` | the probability of the `fault` answer reaches `threshold` (0.5 by default) |
| `choice` | `choose(id, ask, options, { view, fault: [option, ...], threshold?, describe? })` | the probability on each option | the probability on the `fault` options together reaches `threshold` |
| `choice` over ordered levels | `grade(id, ask, levels, { view, below })` | the expected level | it falls below the level `below` names. Over `LEVELS` (poor to excellent) this is exactly checks.rs's `score` check. |
| `score` | `rate(id, ask, levels, { view, below })` | clef's own probability-weighted level | it falls below `below` |
| `choice` over the game's actions | `decide(id, ask, actions, { sees: "frame" \| "state", hold })` | the bot holds the top action for `hold` frames | (a bot's move, not a check) |
| none | `rule(id, measure, { scope, status, fix })` | a measurement from the trace or report | the measurement finds a fault |

Every question also takes `scope` (render, hook, sheet, session, moment, page or turntable), `fix`, `context` (the
fact keys that go into clef's `state`), `status`, and a calibration slot. A `playtest({ pack, version, about, fps,
views, checks, decisions })` default export ties a game's views, checks and decisions together.

**The types enforce clef's limits**, so an invalid question doesn't compile. `react/bench/playtest/dsl.typetest.ts`
holds one of each, and `typecheck.sh` confirms that every one fails:

- An id with a character clef refuses, such as a space, has the type `never`; so does an empty id. Ids are checked
  character by character with template-literal types.
- A choice needs at least two options (`readonly [string, string, ...string[]]`), and `fault` must name some of those
  options.
- A score has 2 to 10 levels (a union of tuple lengths), and `below` must be one of them.
- A `taste: true` question takes only a clean view.
- A model check may have `status: "warn"` only with a `calibration` filled in. Rules may warn without one, because they
  are exact.
- `fault` on a yes question is `"yes"` or `"no"`, scopes come from the list above, a debug view names at least one
  overlay, and a view's media kind is one clef has an input for.

Limits a type can't hold are checked when the playtest is built: at most 255 options, ids of at most 100 characters,
at most 64 questions per view (one call), and clips of at most 60 s.

The principles:

- **Anything the trace can answer is a rule, with no model call.** Examples are "it finishes", "no softlock", "a death
  loop", and "the game has an end".
- **clef gets only the questions that need eyes, on the input they need.** "Is it clear why the player died?" goes to
  a clean clip. "Does the death look unfair against the hitboxes?" goes to a debug clip. "Does it look polished?" goes
  to a clean sheet. Audio questions get audio alone.
- **Judgement is clef's too.** Under the two-model rule, checks.rs's `vote` eye (clef plus a writer model) is left out.
  GLM writes each finding's prose fix from the check's `fix` template and the facts, and never changes a verdict.

**Compiled to void's check pack.** `bun react/bench/playtest/compile.ts EDIT.playtest.ts -o PACK.json` writes the
Pack and Check JSON checks.rs reads: `pack`, `version`, `about`, and per check `id`, `scope`, `eye`, `rule`, `type`,
`ask`, `context`, `threshold`, `below`, `invert`, `status`, `calibration`, `label_words` and `fix`. It adds `view`,
`layer` and `input` (state, images, videos or audio), plus `clef`, the native question sent verbatim.

- `yes` compiles to `type: "yes"` (with `invert` when a no is the fault), and a grade over `LEVELS` compiles to
  `type: "score"`. Both run in void's checks.rs unchanged. checks.rs ignores the added fields, since it has no
  `deny_unknown_fields`.
- `choose` compiles to `type: "choice"` with `fault_options`. `rate` and a grade over other levels compile to
  `type: "rate"` and `"grade"` with `levels`.

void/src/checks.rs runs all of these (on top of d91f1e6). Check gained three fields: `clef`, sent verbatim as the
question, plus `fault_options` and `levels`. A new `value()` reads the answers the way the DSL's `read()` does:

- **choice:** the summed probability on `fault_options`.
- **grade:** the expected index over `levels`.
- **rate:** clef's `score`.

`finding()` shows a score as "of N levels", where it always used to say "of 4". A unit test parses the compiled game
pack and reads one answer of each type. Nothing changed for existing packs: a check without `clef` is asked as before.

**Native `score` works.** One agent reported that clef refused `score`. It does if `criteria` is sent as a map, as
choice takes it (clef replies "Input should be a valid list"). It needs an array of levels, lowest first. This body
works:

```json
{"model": "clef-omni", "state": "A support ticket: checkout has failed for every customer for an hour.",
 "questions": {"severity": {"type": "score", "instructions": "How severe is the customer impact?",
                            "criteria": ["No impact", "Minor", "Major", "Critical"]}}}
```

It returned `{"type": "score", "score": 2.6902, "legend": {"0": "No impact", ...}, "probabilities": {"0": 0.0107,
"1": 0.0096, "2": 0.2585, "3": 0.7212}, "confidence": 0.4495}`. The design judge and review tool in void/src/eval.rs
ask `choice` over levels and compute a probability-weighted index themselves. They could ask native `score` instead,
which returns that index (and a confidence) directly. That is not changed here, and switching means calibrating
again, because the two forms are different questions to the model.

The compiled game
pack is react/bench/playtest/packs/game.json, generated from react/bench/games/platformer/edit.playtest.ts. Compared
with the hand-written pack it replaced, every field checks.rs reads is identical.

### Across media

The same pipeline (run, facts, views, pack) covers each medium. Only the run and the view kinds change.

| Medium | The run | Facts (rdb rows) | Views (scope) | Rule checks | clef checks |
|---|---|---|---|---|---|
| Video | the render | perception: cuts, faces, words, text boxes, loudness | `render`, `hook`, a window around a cut or a word (`moment`) | cuts clean, text readable, captions present, loudness | framing, text over face, hook stops the scroll (clef score) |
| Game | a bot's session (tier i or ii) | trace, moments, report | `session`, `moment` (death, stuck, finish, first coin) | finishes, softlocks, death loop, has an end | death readable (clean), death fair (debug), look (clean sheet), fun (clef score on a clean session clip) |
| Deck | a render of each page, plus the deck's transitions | each page's layout boxes, text runs, contrast, the overflows it measured | `page` (one still per page), plus `session` (a sheet of every page) | text overflows, contrast, font count, words per page | hierarchy reads, page on brand, deck tells a story (clef score on the sheet) |
| 3D | a turntable render: the camera orbits in N frames, with fixed lights | the scene's bounds, triangle counts, materials, frames where the object leaves the frame | `turntable` (a clip, or a sheet of 8 angles), `moment` (a closeup) | in frame at every angle, ground contact, no NaN or black frames | silhouette reads from every side, materials look physical, model on brief |

Debug layers carry across media too. A deck's debug layer draws its layout boxes and safe area. 3D's draws its
wireframe and bounds. Video's draws perception's face and text boxes. Each is drawn only for diagnosis questions and
never for taste.

## Calibration: consistent verdicts

A model check is advisory, and only recorded, until calibration shows it separates good from bad on judged examples.
This is checks.rs `calibrate()`: area under the ROC curve, and a cut chosen on labelled pairs. Then it may warn. For
consistency:

- **Fixed questions.** The questions, the media preparation (frame size, crf, stride) and the pack version are part of
  the cache key, as in checks.rs. Changing any of them means calibrating again.
- **Deterministic media.** The same run gives the same views pixel for pixel, so a verdict moves only when the piece
  moves.
- **Read distributions, not samples.** Use a choice's probabilities as an expected level from 0 to 4, and noul as a
  probability. clef returns no sampled text, so the same media gives the same verdict.
- **Measure repeatability.** Re-judge each reference several times, including small perturbations (another run seed,
  the next instance of a moment). A check whose verdict flips on equivalent views is not consistent, and stays advisory.

**Golden references.** Verdicts are checked against references the user approves. For video, references are real ad
concepts on real footage, with video and audio, approved only after the user has watched them (the standing rule).
Their footage never goes to a cloud model unless it is synthetic or committed. Calibration that needs clef on
creators' footage stays on whatever local path the user approves for it. For games, a reference is a game plus a
judged run: the user plays it, or watches the bot's run, and labels it (finishable, fun, fair deaths). The repaired
LEDGE and the broken LEDGE are the first pair. The broken one is a labelled bad example for `play.finishes` and
`play.death_loop`, with no taste labels until the user has watched both. Decks and 3D follow the same rule: real decks
and real models, approved by the user after viewing, before any check built on them may warn.

## The prototype

```
react/bench/playtest/
  load.ts      bundles an edit with Bun (JavaScriptCore), @raea/motion pointed at the SDK source; returns `playtest`
  search.ts    Game (advance, events, cells), random, novelty (Go-Explore), beam; timeline() writes input logs
  run.ts       tier (i): bots, ceiling, death and stuck spots, softlocks -> report.json, run.input.json, trace.jsonl, moments.json
  views.ts     EDIT.playtest.ts's views over the run, rendered by the engine (clean or overlays) -> views/, views.json
  judge.ts     tier (iii): the playtest's rules, plus one clef call per view instance (native questions) -> judge.json
  drive.ts     tier (ii) spike: the playtest's decide() played by clef from the engine's frame and the observation
  compile.ts   EDIT.playtest.ts -> the check-pack JSON void reads
  dsl.typetest.ts, typecheck.sh, tsconfig.json   the DSL's types against clef's limits (every invalid question fails)
  packs/game.json   compiled from the platformer's edit.playtest.ts
  fixtures/ledge-fixed/   LEDGE with a jump that clears its ledges and a ledge over every gap (finishable)
react/bench/games/platformer/edit.tsx            + the playtest export and the hitboxes and state overlays
react/bench/games/platformer/edit.playtest.ts    its views, checks and decision, in the DSL
motion/src/games.ts (Playtest), overlay.tsx (Overlay), playtest.ts (the DSL: clean, debug, yes, choose, grade, rate,
  decide, rule, playtest, compile, read; CLEF's limits; @raea/motion/playtest)
```

Usage:

```sh
bun react/bench/playtest/run.ts react/bench/games/platformer/edit.tsx OUT        # about 5 s
bun react/bench/playtest/views.ts react/bench/games/platformer/edit.tsx OUT      # renders at utility QoS
bun react/bench/playtest/judge.ts OUT                                            # rules, plus clef: cost printed
bun react/bench/playtest/drive.ts react/bench/playtest/fixtures/ledge-fixed/edit.tsx OUT2 --decisions 36
bun react/bench/playtest/compile.ts react/bench/games/platformer/edit.playtest.ts -o react/bench/playtest/packs/game.json
react/bench/playtest/typecheck.sh [path/to/tsc]
```

## Findings (2026-10-18)

### LEDGE, the gate's platformer, can't be finished

The tier (i) run took 4.5 s, about 17 million steps in all. Random, novelty (509 cells) and beam bots all stop at
progress 0.175, at x = 2081 of 11943. That is the first no-ground stretch (k = 3, level x 1650 to 2502, 852 px wide).
The level's geometry explains it:

- A jump rises 232 px (`JUMP = -21`, `GRAV = 0.95`), and every raised ledge is 315 to 400 px above the floor. No ledge
  can be reached from the ground, so no coin on a ledge can be taken.
- A jump from the floor carries the player about 400 px, and the gap stretches are 852 px (k = 3, with an unreachable
  ledge over it) and 955 px (k = 8, with no ledge at all). Even from a ledge, k = 8 can't be crossed.
- A death respawns the player 600 px back, in the air, and lives wrap from 1 back to 3. So the game has no game over
  and no win. It loops at the first gap forever, which is what the autopilot and the gate's key script do too.

There are no softlocks: 24 probed states all got back to the frontier. Death spots: 87% of 1906 search deaths happen
at x 1600 to 2000. Stuck spots are the cells in the gap the explorer returns to. The engine renders the bot's run
identically: the debug layer's state panel at frame 290 reads x 1640, y 674, vy -6.8, exactly the bot's trace row.

### The repaired fixture can be finished

With `JUMP = -28` and a ledge over every gap (fixtures/ledge-fixed), the beam finishes in **1315 frames (10.96 s)**.
That is the minimum at run speed, with no deaths and 13 coins, and 0 softlocks were found. The engine's render of
the winning run ends with the player at the level's right wall, score 130, and three lives. The replay is the
`session.run` view (`views/session.run-0.mp4`).

### Views and the judge

`views.ts` rendered the broken level's views in 20 s (201 frames, in four engine processes) and the repaired level's
in 33 s (397 frames). The views were the death clip clean and with hitboxes and state, a debug death sheet, a stuck
still, the furthest sheet, the session as text, the session clip and the look sheet. The judge on the broken level:

| Check | Answer |
|---|---|
| play.finishes (rule) | WARN: stops at progress 0.175 |
| play.no_softlock (rule) | ok: 0 of 24 |
| play.ends (rule) | advisory fault: no win, no game over |
| play.death_loop (rule) | WARN: 87% of deaths in two spots |
| moment.death_readable (clef, clean clip) | p(yes) = 0.30: the cause doesn't read |
| moment.death_fair (clef, debug clip) | p(unfair) = 0.20 |
| session.looks (clef, clean sheet) | 1.35 of 4 (repaired level: 1.51) |

That took 3 clef calls for $0.00034. Three runs of the judge on the same views gave the same numbers to the reported
precision, so a verdict moves only when the media moves. On the repaired level all four rules pass, and the death
checks report that there is no death view to look at.

### The model in the loop (tier ii spike)

There were 36 decisions on the repaired level, each holding its move for 30 frames (4 decisions a second of game
time):

| Mode | Progress | Deaths | Calls | Cost |
|---|---|---|---|---|
| clef, frame plus observation | 0.103 | 4 | 36 | $0.0031 |
| clef, observation alone (text) | 0.100 | 1 | 36 | $0.0019 |
| beam search (tier i) | 1.0 in 1315 frames | 0 | 0 | 0 |

clef runs the easy stretch confidently (`run_right` at p = 0.84 to 0.88). At each gap its distribution flattens
(`run_right` 0.39, `jump_right` 0.29, `jump_up` 0.24), it chooses badly, and after a respawn it repeats `jump_up` in
place. A 0.25 s hold is too coarse to time a jump at an edge, and clef has no memory between decisions. What this
shows:

- **clef should not be the controller for timing-sensitive play.** Search is far better at that, and free.
- **clef's uncertainty is a signal.** The flat distribution appears exactly where a player has to make a decision,
  which is useful for "does the hazard read?" tests.
- **If a model is to play, offer it macro options** ("jump at the next edge", "wait for the walker", "take the
  ledge"), carried out by a scripted controller over the fold. Each choice then spans seconds and the menu stays
  closed. That is the next spike.

The spend for the whole prototype was about 80 clef calls, roughly $0.007. GLM wasn't needed: nothing here wrote free
text.

### The port to the DSL changed nothing

After the port, the platformer's playtest, judge and drives ran through the DSL. Each was compared with its earlier
run:

- **Bots:** `report.json` and the run's input log were byte-identical on both levels, apart from the run time.
- **Judge:** every finding had the same value and fault, with the same 3 calls and $0.00034 on the broken level, and
  1 call and $0.00018 on the repaired one.
- **Text drive:** the same 36 moves with the same probabilities, for $0.00189.

### The engine and QoS (2026-10-18)

`taskpolicy -b raea-motion render` never commits a frame under load. Background QoS throttles the GPU, so the engine
hits its 20 s commit timeout. The playtest scripts render at `taskpolicy -c utility`, which took 1.7 s for a frame
that took 1.0 s at normal priority.
