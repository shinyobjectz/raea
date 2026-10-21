# State machines

Game logic in @raea/motion is a pure fold, `step(state, input, frame) -> state` (sdk/motion/src/games.ts). State machines
give that fold structure: what an entity is doing (idle, running, jumping, hurt; patrolling, waiting; playing, won, game
over) is a machine whose snapshot is plain data in the game state, stepped once a frame inside the fold. The core is
sdk/motion/src/machine.ts, exported from `@raea/motion`, `@raea/motion/games` and `@raea/motion/machine`, with no
dependencies. The spec is sdk/motion/spec/motion.feature ("State machines, step by step"); the agent-facing guide is the
"State machines" section of `bevy/games` (apps/void/prompts/craft/bevy/games.md).

## The design

It follows Bevy's [seldom_state](https://github.com/Seldom-SE/seldom_state): each entity carries its own machine, and
a transition is a trigger, a pure predicate over the game's state checked every frame, not an event someone sends.
The definition is in XState's config shape, so what an agent knows of XState carries over:

```ts
import { machine, stepAll, type Snapshot } from "@raea/motion";

interface Player { x: number; y: number; vy: number; ground: boolean; hit: boolean; lives: number; sm: Snapshot }
interface Env { left: boolean; right: boolean; jump: boolean }

export const player = machine<Player, Env>()({
  id: "player",
  initial: "alive",
  states: {
    alive: {
      initial: "ground",
      tick: physics,                                       // every frame while alive: (p, env) => p
      always: { to: "hurt", when: (p) => p.hit },          // applies from every substate
      states: {
        ground: {
          initial: "idle",
          always: [{ to: "jump", when: (_p, e) => e.jump }, { to: "fall", when: (p) => !p.ground }],
          states: {
            idle: { always: { to: "run", when: (_p, e) => e.left !== e.right } },
            run: { always: { to: "idle", when: (_p, e) => e.left === e.right } },
            land: { always: [{ to: "run", after: 6, when: (_p, e) => e.left !== e.right }, { to: "idle", after: 6 }] },
          },
        },
        air: {
          initial: "fall",
          always: { to: "land", when: (p) => p.ground },
          states: { jump: { entry: (p) => ({ ...p, vy: -28 }), always: { to: "fall", when: (p) => p.vy >= 0 } }, fall: {} },
        },
      },
    },
    hurt: { entry: (p) => ({ ...p, lives: p.lives - 1 }), always: [{ to: "dead", after: 40, when: (p) => p.lives <= 0 }, { to: "respawn", after: 40 }] },
    respawn: { entry: respawnAtCheckpoint, after: { 30: "alive" } },
    dead: { type: "final" },
  },
});

// inside the game's step, once a frame:
const p = player.step(s.player, { left, right, jump });
const walkers = stepAll(walker, s.walkers, null);      // one machine per enemy
```

The semantics, all deterministic:

- **One step a frame.** The first step enters the initial state and its initial substates. Each later step polls the
  active states' transitions, innermost state first (a substate's own transitions win over its parent's); within a
  state, `on` edges for this step's events, then `always`, `after` and `onDone` in order, then `choose`. The first
  enabled edge fires, and at most one fires a step. Then every active state's `tick` runs, outer first.
- **Delays count steps**: `after: { 30: "fall" }`, or `{ to, after: 6, when }`. No clock, no timers.
- **Actions are pure** and return the new context (`entry`, `exit`, `tick`, a transition's `actions`). There is no
  `assign`.
- **Nested states with history**: names are unique across the machine, so `to: "run"` reaches it from anywhere;
  targeting a state that encloses the source leaves and re-enters it; `history: "shallow" | "deep"` resumes where it
  left; `type: "final"` and `onDone` end a compound state.
- **The snapshot is plain JSON** in the context, at `sm`: `{ path, at, n, e, by?, hist? }` (the active states, the step
  each was entered on, the step count, the last edge taken, the decider that chose it, history). The fold, its
  checkpoints and renders stay pure and resumable; a playtest reads every machine's state from the game state.
- **Typed.** `machine<Ctx, Env>()({...})` is curried so state names are inferred while the context and env are given. A
  transition, an initial or a query naming a missing state, or a guard reading a missing field, doesn't compile
  (sdk/motion/conformance/machine.typetest.ts holds 12 such machines, each refused). What types can't hold throws when the
  machine is built: an initial that isn't a substate, duplicate names, a final state with transitions, and, unless
  `strict: false`, unreachable states and non-final dead ends.
- **Input edges** are optional events: `keyEdges(prevKeys, keys)` gives `"keydown:ArrowUp"`, and `on: { "keydown:ArrowUp":
  "jump" }` fires only on the press. The platformer doesn't need them (holding jump hops, as it always did).

The game layer sits on top of the generic core: `stepAll` and `keyEdges` live in games.ts, and machine.ts knows nothing
about frames, keys or games.

## Deciders: who picks a transition

A transition is either guard-driven (a trigger: the game case) or chosen. A state's `choose` list is a choice point:
its candidates whose guards hold go to the step's **Decider**, `{ name, decide(point) => { edge | null, evidence? } }`,
which picks one of them or waits. With no decider, `firstDecider` takes the first. The decider is called inside the
step, so it answers synchronously: a deterministic decider (code, a seeded bot) keeps the fold pure, and a person or a
model (clef-omni's `choice` over the candidates, which is the playtest DSL's `decide()`) is asked outside the step and
its answer replayed with `replayDecider(log)`, keyed by machine and step. The record of choices is then input like any
other, and the run replays exactly.

## The trace: one record

Every transition is one row: `{ step, machine, from, to, edge, kind, by, evidence? }`, where `by` is `"start"`,
`"guard"` or the decider's name. A step given `trace` reports rows as it takes them, with the decider's evidence. A
playtest over a game's pure fold, which sees only states, recovers the same rows from consecutive snapshots
(`transitionOf`). sdk/react/bench/playtest/run.ts writes the run's rows to `transitions.jsonl`, one JSON object per line
with flat columns, so they load into rdb beside the run's frames (`playtest_transition`, next to the `playtest_frame`
table sdk/react/docs/playtest.md describes). trace.jsonl gains each frame's machine states and transitions, and
moments.json gains each transition as a moment named `"<entity>.<state>"`, so a view can be timed on one
(`when: (m) => m.name === "player.hurt"`).

## Checks: machine-generic rules

The rules read report.json's `machines`, which sdk/react/bench/playtest/machines.ts counts over all the bots' play. They
know machines, states, edges, inputs and a progress number, never what a machine is for:

| DSL (sdk/motion/src/playtest.ts) | Rule | Faults when |
|---|---|---|
| `coversStates(id, m, { min })` | `state_coverage` | fewer than `min` of the machine's states were entered |
| `coversTransitions(id, m, { min })` | `transition_coverage` | fewer than `min` of its transitions fired |
| `noSoftlock(id, { machine?, loops? })` | `machine_softlock` | a state can't be left (static), or one was entered and never left anywhere in the bots' play while a linear play stayed in it through 3 or more input changes; with `loops`, also an edge firing 12 times while progress stays flat |
| `noUnreachable(id, { machine? })` | `unreachable_states` | the definition has states no transition reaches |
| `reaches(id, m, state)` | `reached_state` | the bots never entered `state` (a name the machine lacks is a type error) |

They compile to check-pack rules (`eye: "rule"`, the rule's name, and `args` with the machine's id), as the other rule
checks do, and judge.ts answers them with no model call. Two details keep them honest. The bots stop at a win or a game
over, so run.ts plays on for 10 s with no keys after the first of each, which covers restarts. And a state entered only
as the game ends (its outcome no longer null) isn't a softlock suspect. sdk/react/bench/playtest/machines.test.ts plants
each fault in a toy corridor game and checks that each rule finds it.

## One core: playtests, check packs and an agent's workflow

These are design notes; only the game and playtest parts are built. The user's direction (2026-10-18) is that game
logic, the playtest framework, clef's decisions and an agent's task management are a single kind of system. They map
onto the same four parts:

| | Machine | Deciders at choice points | Trace | Checks |
|---|---|---|---|---|
| **A game** | each entity's machine (player, walkers) and the flow (playing, won, game over) | none: guards over the state (triggers) | the run's transitions.jsonl and trace.jsonl | coverage, softlock, unreachable, reached, plus the game's rules (finished, death loop) |
| **A playtest** | the game's machines, plus the tester's own (explore, retry, give up) | a search bot (beam, novelty), a replayed input log, a person, or clef's `choice` over the game's actions | the bot's moves and the game's transitions, one row each, with the decider's evidence (a beam score, clef's probabilities) | the same rules, over all the bots' play |
| **A video check pack** | the render's stages (cut, caption, mix, render, check) | rules and clef, as checks.rs asks them today | the findings as rows beside perception's | coverage of the pack's checks; a check that never ran is a missing state |
| **An agent's workflow** | brief, plan, build, render, check, fix, ship | code where the next step is known; clef's `choice` over the candidate steps where it isn't; GLM 5.3 Flash only to write free text (a fix's prose), never to decide | every step as a row: from, to, decider, evidence (clef's probabilities, the check that failed) | `reaches("ship")`; transition coverage over many runs; softlock as "looping without progress" (`noSoftlock` with `loops`: check, fix, check firing again and again while the check's score stays flat) |

What carries over unchanged: the definition (states and guards over facts), choice points that name their deciders,
the snapshot as plain JSON (a workflow resumes from any step, as a game resumes from any frame), the trace rows, and the
rules, which already take "progress" as a number the host supplies (the platformer's x; a workflow's passed checks).
What a workflow would add is outside the step: an async loop that asks clef or a person at a choice point, appends the
answer to the decision log, and steps the machine with `replayDecider`. That keeps the machine pure and the run
replayable from its log. void's agent loop belongs to raea-01, and nothing here touches it.

## Is XState needed? (assessment, 2026-10-18)

**No.** This was decided after the machines, the playtest integration, the platformer port, the skill and the spec were
built and committed. The question was whether anything still calls for [XState](https://stately.ai/docs/xstate) (v5):
nested parallel statecharts for menus or game flow, the visual editor, actor systems, or anything these machines can't
express cleanly. The answer, part by part:

- **Hierarchy, history, delays, guards, final states, onDone.** Built here, in XState's own config shape. An agent
  writing XState-style config gets it right: `target` and `guard` are accepted as written.
- **Parallel regions** (`type: "parallel"`). This is the one statechart feature not built. Every case seen so far is
  covered without it. Orthogonal concerns of one entity are two machines in its context (movement and a power-up),
  each with its own snapshot, and a cross-region condition (XState's `stateIn`) is a guard reading the other snapshot,
  `inState(p.power, "shield")`. A menu over gameplay is the flow machine's nested states with `history`: pause leaves
  `playing`, and resume re-enters it where it was. If a real case ever needs one machine with regions (one snapshot,
  one trace entity), it fits this core as a `parallel` state kind: the path becomes a small tree, and each region is
  polled in turn. That is a contained change, and no reason to take a dependency.
- **Events.** XState is event-driven: `send` an event, and transitions react. A frame-stepped game wants the opposite:
  conditions polled every frame inside a pure fold. Events here are optional edges passed to a step (`keyEdges`).
  Driving XState from the fold would need an actor per entity, a `send` per frame and `getSnapshot` back, plus a way to
  rebuild actors from persisted snapshots at every checkpoint.
- **Delays.** XState's `after` is in milliseconds on a clock (a SimulatedClock in tests). Render equals play needs
  delays counted in frames, so XState's delays would have to be wrapped or avoided anyway.
- **Actors** (`invoke`, `spawn`, promise and callback actors, the actor system). These are the right tool for async
  work, but async work doesn't belong inside a game's step. Per-entity machines cover "one per enemy" with plain data.
  For the agent workflow sketched above, async work (renders, clef, GLM) belongs to the host loop, which records each
  decision and steps the machine with `replayDecider`. That keeps the run replayable from its log. Actors would add
  their own scheduler and mailboxes beside the trace, a second record of what happened.
- **The visual editor and inspector** (Stately Studio, @statelyai/inspect). These are the strongest reason to want
  XState, and they are optional. The trace rows and the static analysis already give what the inspector shows
  (states, transitions, coverage), as data that rules and rdb can query. A view of a machine's graph is cheap without
  the library: the config is already XState-shaped, so writing an XState-JSON or Mermaid export of `analyze()` (states,
  edges, guards named by their descriptions) is about 50 lines, if someone wants to look at a machine in Stately's
  visualiser.
- **Model-based testing** (@xstate/graph's path generation). The search bots do this against the real game: they find
  which states and transitions are reachable in play, not just in the graph, and the static analysis covers the graph.
- **Cost of adding it.** XState is a dependency, against this module's no-dependency rule. Each
  actor allocates far more than a snapshot object, inside a fold that runs every frame at 120 fps and also four million
  times a second under the bots. Determinism would rest on how XState is used, rather than on how the module is built.

The recommendation is not to adopt XState. Two cheap additions are worth keeping in reserve, to build only when a real
game or workflow asks for them: a `parallel` state kind in this core, and an XState-JSON or Mermaid export for viewing
a machine's graph. Neither needs XState, so there is nothing to confirm before starting.
