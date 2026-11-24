# Story: the plan

Shane, 2026-11-22: "a story skill tree that takes the hero's journey of a thousand faces, pixar story principles eg.
creativity inc, robert mckee's storynomics, propaganda by edward bernays, etc. the goal here is to create an open
non-deterministic story framework for videos/games/dialogue, themes and things in a new type of story typescript sdk
that also integrates with the screenplay/avscript stuff but at a higher level so you could test and validate story
beats in the code or across themes, ideas, etc. ill look to you to see how SOTA we can actually make that system".

This is the one plan for `raea/story`: what it is, how it sits on `script.ts`, the frameworks it reads stories
through, how a story is generated without being fixed, how it's tested, what's state of the art and what's new
here, and the build order, with a confidence for each step.

**Where it stands (11-22):** `sdk/story` is built: the ECS, the systems, 12 lenses (the launch lens's priors measured),
seeded sampling, the mutations, story as data and as story lines, the state at a beat and windows (any length),
typed questions about a beat (story as decisions), and checks in `raea check`. The skill carries it as
`craft/story/`. Shane paused the small story models the same day ("screw the model, just finish and wrap the sdk"):
the code that built their data and trained them stays in `sdk/story/models/` with its corpus in `~/.vd/story`, not
part of the package, for whenever they're picked up again. Measured the same day against plain prompts (see "Measured: the same
prompts with and without it"), it makes a script better as the editor of a draft (read back, checked, fixed by beat:
tied with a strong editing prompt, both well over plain) and worse as the language a script is drafted in.

## In one paragraph

A story is a typed program, not a document. Its parts are a premise, the values at stake, a cast with wants and
needs, a knowledge model (who knows what, and when), and beats. Each beat turns a value, opens or closes a gap in
what the audience knows, and is caused by an earlier beat. The frameworks (Campbell, Pixar, McKee, Storynomics,
Bernays, Kishōtenketsu and others) are **lenses**: each one reads the same story, maps its beats onto its own stages
and checks it, and each can also *propose* beats. Generation is a seeded walk over choice points. Lenses and the
priors learned from real outcomes weight the choices, and the checks prune them, so every run is a different story
that still passes. A story compiles down to `script.ts` (the screenplay and A/V table the feed already shows), to a
storylet graph for games, and to dialogue. It's tested at four levels: the type checker (a payoff without a setup
doesn't compile), exact checks, learned judges (fingerprint text axes trained on real pairs), and measures of the
finished piece.

## Where it sits

Shane, 11-22: "it should live in brief and be an import or something like that ... we might use it for a few
different things in video and gaming so best to keep it open and explicitly used. story sdk can stay typescript."

So a story is a module of its own (`story.ts`, or several: `launch.story.ts`, `quest-1.story.ts`), written with
`raea/story`. The brief takes it as an import: `import launch from "./launch.story"` and `brief({ ..., story: launch })`,
or `story: { launch, quest1 }` for several. Nothing in raea reads a story unless something imports it: the brief, a
composition timing its scenes from the beats, a check, or a game's drama manager. That's what "explicitly used"
means here: an opt-in import, never a convention raea assumes.

```
brief.ts        what to make, for whom, the one thing to believe; `story:` imports one or more stories
  └─ *.story.ts  facts, events, beats, values, cast, lenses     ◄── NEW: raea/story (TypeScript only)
  │  compile
  ├─► NAME.script.ts   scenes of beats: on screen, lines, camera, cut, payoff  (exists: core/brief Script; the feed
  │                     shows it as a screenplay and as an A/V table)
  ├─► storylets        preconditions and effects over qualities, for a game's drama manager
  └─► dialogue         lines with intent and subtext, barks, branches (Ink and Yarn export)
render ─► film/game ─► measured (vd measure_film, fingerprint) ─► back onto the story's beats
```

`script.ts` stays the shot level: what's seen and heard. `story.ts` is the level above it: why each scene exists
and what changes in it. One story compiles to many scripts: a 30 s cut, a 90 s cut, a 9:16 cut, a game level.

## Story as data: how it works

### Three layers, kept apart

Narratology separates what happens from how it's told (the Russian formalists' *fabula* and *syuzhet*). Keeping
them apart as data is what lets one story serve a film, a game and a dialogue. It's also what makes reveals,
flashbacks, irony and branching computable instead of described.

| layer | what it holds | who writes it | example |
|---|---|---|---|
| **world** (fabula) | facts, and events that change them: who did what, because of what, in story order | the author or an agent, once | `the backlog doubles` happens before `the team tries raea` |
| **telling** (syuzhet) | beats: which events the audience sees, in what order, what each one reveals and to whom, the feeling it aims for | one per cut: the 30 s, the 90 s, the game | the 30 s cut opens on the proof and flashes back to the backlog |
| **render** | `script.ts`: shots, lines, camera and cut, timed | compiled, then edited | scene 1: "the 50 finished ads, wide" |

A film is a single linear telling. A game is many possible tellings: storylets chosen at runtime from the same world.
A trailer is a telling that hides the ending.

### The primitives (written)

- **Value**: a quality at stake, with two poles (`freedom: buried ↔ free`). McKee's unit of change.
- **Fact**: a proposition that can be true or false at a point in the world (`team.knowsRecipeCanRun`).
- **Agent**: a character or a group, with a want, a need, the lie they believe, and their beliefs (the facts they
  hold true, which may be wrong).
- **Event**: an agent does something. It needs facts, changes facts, turns values, and is caused by earlier events
  (`because`). Every event lives in the world layer.
- **Beat**: a unit of telling. It shows one or more events, reveals facts to the audience or to characters, and
  names the feeling it aims for. It may set something up, or pay something off.
- **Idea**: the controlling idea and its counter (McKee). Each beat argues one side, which makes theme checkable
  (Pixar's "what is the story about").

All of these are typed keys: a beat that shows an event the world doesn't have, or pays a setup that doesn't exist,
is a type error. That's the same trick `NAME.parts.ts` uses for parts.

### What's derived (computed, never written)

From those primitives the SDK computes, over the telling's order:
- **value curves**: one per value, so the arc's shape can be named and checked (rise, fall, fall-rise, the six
  shapes Reagan et al. found);
- **the audience's knowledge**, beat by beat, against each character's. Where the audience knows more than a
  character, that's suspense or irony. Where it knows less, that's mystery. A reveal with no earlier setup is a
  surprise, which is a defect unless the beat says it's meant;
- **the cause graph**: beats with no cause ("and then"), and causes that never lead anywhere;
- **belief consistency**: an agent acting on a fact they don't hold yet, which is the plot hole FlawedFictions
  measures, caught here from the data instead of read from prose;
- **the tension curve**: from open questions (facts the audience is waiting on) and value pressure, laid against the
  beats' intended feelings.

Every check reads these derived curves. No lens or check parses prose to find structure: the structure is the data.

### A worked example

```ts
// launch.story.ts
import { story, value, choose } from "raea/story";

export default story({
  idea: { side: "your recipe, run for you, frees you", counter: "only hands-on work is good work" },
  values: { freedom: value("buried", "free"), trust: value("doubt", "trust") },
  agents: {
    team: { want: "ship every client's ads this week", need: "stop being the bottleneck", lie: "if I don't touch it, it's worse" },
    raea: { mask: "mentor" },                          // Storynomics: the customer is the hero, the brand the mentor
  },
  facts: { backlog: "the backlog is out of hand", recipeRuns: "the recipe can run without them" },
  world: {                                            // events in story order, each with its cause
    week:    { who: "team", does: "edits the same ad again", turns: { freedom: -1 } },
    doubles: { who: "team", does: choose(["the backlog doubles", "a client wants 50 versions"]), because: "week", sets: ["backlog"], turns: { freedom: -2 } },
    tries:   { who: "team", does: "hands raea the recipe once", because: "doubles", needs: ["backlog"], turns: { trust: -1 } },
    runs:    { who: "raea", does: "runs it on 50 courses; checks every cut", because: "tries", sets: ["recipeRuns"], turns: { trust: +2, freedom: +2 } },
  },
  tellings: {
    "30s": [                                           // a beat shows events and reveals facts; order is the telling's
      { shows: ["runs"], feel: "curiosity", reveal: { audience: ["recipeRuns"] }, setup: "how" },  // in medias res
      { shows: ["week", "doubles"], feel: "recognition" },                                         // flashback
      { shows: ["tries"], feel: "concern" },
      { shows: ["runs"], feel: "relief", pays: "how" },
    ],
  },
  lenses: ["mckee", "storynomics", "pas"],
});
```

Derived for the "30s" telling:
- `freedom` goes +2, -1, -2, +2: a fall-rise, the "man in a hole" shape;
- the opening reveal makes the middle beats suspense rather than mystery, because the audience already knows it ends
  well;
- `tries` needs `backlog`, which `doubles` set earlier, so the beliefs are consistent;
- the PAS lens maps problem → `week`/`doubles`, agitate → `tries`, solve → `runs`, and passes.

### Lenses as code

A lens is a function from a story and a telling to a mapping, findings and proposals:

```ts
export const mckee = lens({
  name: "mckee",
  stages: ["inciting", "complications", "crisis", "climax", "resolution"],
  map: (s, t) => inferStages(s, t),                                     // or the beats' own `stage:` labels
  checks: [
    every("beat turns a value", (b, d) => d.turned(b).length > 0),
    rule("complications escalate", d => d.pressure.isRising("complications")),
    rule("the crisis is a dilemma", d => d.choiceAt("crisis")?.options.length >= 2),
  ],
  propose: (stage, s) => /* candidate beats for a missing stage */ [],
});
```

The frameworks become a library of these. A person's or an agency's own format is one more lens, written the same way.

### How it reaches `raea check` (tested 11-22)

A `raea/check` check is a TypeScript module, so it can import a story directly and compare it against the world's
facts. Those facts already carry `Shot` (each `Sequence` by name and frames), `Line` (narration and dialogue with their
words) and `Word`. Tested on 11-22 with a three-beat story and a composition naming its Sequences by beat id:
- a check listed in `taste.ts` read every Shot frame by frame;
- in `after` it walked the story's beats;
- it reported `error story problem: the beat problem has no shot named for it`.

This runs on main's build (0.1.9). The installed 0.1.8 refuses a check given by path: TypeScript checks in taste
landed in 0.1.9 (PJ9, 822b73fc7d).

So the story SDK ships its checks as ordinary checks, in three kinds:
- **story-only**: structure, knowledge and lens checks, run once in `after`. They need no frames, but today they
  still need a composition to run against;
- **story on screen**: each beat is a shot in order, and the beat's line is spoken inside its frames (Line and Word
  against the beat's frames);
- **story in the render**: once rendered, the measures of the made piece, laid on the beats.

A composition uses the story explicitly: `useBeat(launch, "30s", 2)` gives a beat's frames, and `<Beat>` is a
`Sequence` named by its id. The world then carries the beats with no engine change.

### The Rust changes it needs (small, and the only ones)

The SDK stays TypeScript. Rust only has to let a story through and point back at it:
1. **`Brief.story`**: the brief refuses unknown fields (`deny_unknown_fields`), so `brief({ story })` was refused.
   It's now `story?: unknown` (Rust keeps it as written and reads nothing in it).
2. **`Scene.beat`**: an optional beat id on a script's scene; `compileScript` writes it, so a scene says which beat it
   plays.
3. **Story checks without a composition**: `raea check launch.story.ts` runs every taste rule whose `.ts` check is
   `over: "story"` or `"both"` once, with no frames, so only its `after` speaks. A story is checked before anything is
   drawn.

A fourth was planned (naming the SDK's checks by package path in taste.ts) and dropped: a project's own three-line
`checks/story.ts` that imports its story and calls `storyCheck` is more explicit, which is what Shane asked for.

## The model, as a module

Every field is data, written in TypeScript, so `tsc` catches what can be caught without running anything (the way
`NAME.parts.ts` makes a skipped part a type error). The example below is the short form of the worked example above.

```ts
import { story, value, beat } from "raea/story";

export default story({
  premise: "A video team drowning in footage gets its week back",
  // McKee's controlling idea: a value plus its cause. The story argues it; the counter-idea gets a fair voice.
  idea: { value: "freedom", because: "the recipe they already know is run for them", counter: "only hands-on work is good work" },
  values: {
    freedom: value("buried", "free"),           // each scene turns one of these, + or -
    trust:   value("doubt", "trust"),
  },
  cast: {
    team:  { want: "ship every client's ads this week", need: "stop being the bottleneck", lie: "if I don't touch it, it's worse" },
    raea:  { role: "mentor" },                  // Storynomics: the customer is the hero, the brand is the mentor
  },
  setups: { recipe: "the hook, best moment, captions, logo they repeat by hand" },
  beats: [
    beat("ordinary", { who: "team", turns: { freedom: -1 }, shows: "the same edit, again" }),
    beat("inciting", { who: "team", turns: { freedom: -2 }, because: "ordinary", reveals: { audience: "the backlog doubles" } }),
    beat("refusal",  { who: "team", turns: { trust: -1 }, because: "inciting", says: "a tool can't know our clients" }),
    beat("proof",    { who: "raea", turns: { trust: +2 }, because: "refusal", pays: "recipe" }),
    // ...
  ],
  lenses: ["mckee", "storynomics", "pixar-spine"],   // read and checked through these; others may still read it
});
```

- **Values** (McKee): the qualities at stake, each with two poles. A beat that turns no value is a non-event, and
  the checks say so.
- **Cause** (`because`): every beat after the first names the beat that caused it. A story of "and then" links fails
  the "therefore / but" rule (Parker and Stone's rule, the same idea as McKee's causality).
- **Setups and payoffs** are typed keys. `pays: "recpie"` doesn't compile, and an unpaid setup is a check failure
  (Chekhov).
- **Knowledge**: each beat can reveal a fact to the audience, to a character, or to both. From that the SDK computes
  **suspense** (the audience knows more than a character), **mystery** (less) and **surprise** (a reveal nobody was
  set up for, which is a defect unless the story marks it as meant). It also catches the plot holes FlawedFictions
  measures: a character acting on something they can't yet know.
- **Cast**: a want (conscious), a need (unconscious), the lie they believe, and an arc (positive, negative, flat).
  Campbell's and Vogler's archetypes are **masks a character wears in a beat** (mentor, threshold guardian,
  shapeshifter), not fixed types.
- **The audience's feeling** per beat (curiosity, concern, relief, delight) is a target, so a story can be checked
  against the curve it means to draw.

## Any length

A story has no length cap. The checks and lenses are ordinary code over the whole telling, so they run on 55 beats
or 2,000 alike in milliseconds. Only the models have a context window, so no model call ever holds the whole story.
Each one reads the story's state at a beat plus a window around it. Length becomes a number of steps, like a game
tick, never a bigger prompt.

- **State** (`src/state.ts`, `stateAt`): what the systems hold just before beat k. That is what the audience knows,
  the open questions, suspense, each value's level, tension, setups planted but not paid, each agent's unsettled
  want, need or lie, and the events not yet shown. It is about the same size at minute 3 or minute 100. It carries
  what the earlier text would have carried.
- **Story lines** (`src/lines.ts`, `toLines` / `fromLines`): one keyword-led line per value, fact, agent, event and
  beat, the way Fountain writes a screenplay. They read back exactly: every valid corpus story round-trips. The
  TypeScript data stays the story; lines are what people skim and models read and write a line at a time. They save
  only about 22% against JSON (Birds: 2,924 tokens to 2,294), so the format is not what solves length; windows are.
- **The generator** reads `windowAt`: the world's values, facts and agents, the events it can still use (unshown,
  or shown by a neighbour), the state, three beats before and two after, and writes the beat between, as JSON
  under that story's beat schema. On Birds the window shrinks from 7,300 characters at beat 3 to 2,900 at beat 55
  as events get shown.
- **The extractor** reads a script six beats at a time (`models/lib.ts` `chunkSteps`). Each step gets KNOWN (the
  keys earlier steps found, as story lines) and its chunk of script, and answers only what's new plus the chunk's
  beats. `story_models.py chain` runs the steps in order and merges them. On gold answers the merge is exact for all
  1,571 corpus stories. The whole-story extractor stays as the baseline the chained one has to beat on Birds.
- **The critic**: for structure, the exact checks are the critic at any length, on the extractor's reading. The
  learned critic earns its place on what code can't see: prose read a window at a time, and taste learned from the
  launch pairs.
- **Hierarchy, next**: a feature is about 200 world events (that fits in a window) but 1,500 or more shots (that
  doesn't and needn't). A beat's `scene` groups its shots. Work goes top-down: premise, then world, then scenes,
  then shots a scene at a time, each level checked before the next. KNOWN and the generator's event list grow with
  the world, not the telling; past a few hundred events they'll be cut to the current act's.
- **Proof at length**: Birds (55 shots, by another author) now. Next is a public-domain feature (Keaton's
  *Sherlock Jr.*, 1924) to prove the loop at real length.

## Lenses: the frameworks

A lens is a module with the same shape for every framework:
- **stages**, with their order and where they fall in the running time;
- **map**: which of the story's beats play each stage, inferred or declared;
- **checks**, each one exact or learned;
- **propose**: candidate beats for a stage that's missing.

A story is read through every lens at once, so `raea story lenses` can say "this passes McKee and Storynomics, and
reads as a Kishōtenketsu with no second turn". The lenses' words go in the skill tree, in our own words, with sources.

| lens | what it adds | checks it brings |
|---|---|---|
| **Campbell / Vogler** (monomyth: 17 stages; Vogler's 12) | departure, initiation, return; archetypes as masks | stage order; the return carries something back (the "elixir") |
| **Pixar** (story spine; Emma Coats' 22 rules; *Creativity, Inc.*'s Braintrust) | "Once upon a time... every day... until one day... because of that... until finally" | a causal spine with no gaps; the Braintrust is a *review* lens: candid notes on the problem, never the fix (our readers and the fingerprint's notes) |
| **McKee** (*Story*: scene turning, gaps, controlling idea; inciting incident, progressive complications, crisis, climax, resolution) | values, the gap between what's expected and what happens | every scene turns; complications escalate; the crisis is a real dilemma (two goods or two evils) |
| **Storynomics** (McKee and Gerace) | purpose-told stories for brands: the customer is the protagonist, the core value shifts, the brand's role is mentor or gift | the brand never steals the protagonist's climax; the value shift is measurable |
| **Bernays** (*Propaganda*, 1928, public domain in the US since 2024) | persuasion through groups: trusted third parties, the leaders a group follows, events that make news, symbols | a **persuasion lens** for ads, held to raea's rule that claims are true (Film copy rule): it can arrange proof and authority; it can't invent them |
| **Harmon circle** (8 steps) | a compact cycle for episodes and short films | you, need, go, search, find, take, return, change |
| **Save the Cat** (15 beats) | page-percentage beats for features | proportions in the running time |
| **Kishōtenketsu** | four parts with no conflict: introduction, development, twist, reconciliation | the twist recontextualises; no antagonist needed (good for brand and calm pieces) |
| **Propp** (31 functions) | folk-tale functions as a generative vocabulary | ordering constraints between functions |
| **Problem, agitate, solve; AIDA** | the ad formats launch videos use | problem stated before the product; the product shown doing the job |
| **Launch** (learned) | the launch fingerprint's findings (docs: launch corpus below) | priors over hook, reveal time, length, voice, measured from real outcomes |

New lenses are plain modules, so a person or an agent can add one ("my agency's format") with the same four parts.

## Generation without a fixed answer

A story module can hold **choice points** instead of fixed beats:

```ts
beat("inciting", { choose: ["the backlog doubles", "the client asks for 50 versions", "the editor quits"] })
```

- **Seeded.** `raea story sample story.ts --seed 7 --n 12` walks the choice points. The same seed gives the same
  story, and a new seed gives a new one.
- **Weighted by lenses and by data.** A lens's `propose` adds candidates for stages that are missing. Priors learned
  from real outcomes (first, the launch corpus's pairs) weight them. The result is varied, but biased toward what has
  worked.
- **Pruned by the checks.** A candidate that breaks a type or an exact check is never shown. This is generate-and-test,
  as `vd evolve` does for worlds.
- **Leaves written by a model, structure kept by the SDK.** An agent writes a beat's words, but the SDK owns which
  beats exist, why, and what they change. This split (plan, then write) is what the best LLM story systems do
  (Dramatron, Re3/DOC, Agents' Room). Our difference is that the plan is typed and checked, not prose.
- **Across themes.** A story *pattern* is a function from a theme to a story. `raea story matrix pattern.ts --themes
  themes.ts` runs one pattern across many premises (a coffee brand, a game studio, a skincare line) and reports where
  it breaks. This is how a beat is tested "across themes and ideas".

## Testing: four levels

1. **Types** (`tsc`, free, instant):
   - unknown characters, setups and values;
   - a lens's required stage missing when the story declares that lens strict.
2. **Exact checks** (`raea story check`, milliseconds, no model):
   - every beat turns a value;
   - every beat after the first has a cause;
   - every setup is paid off and every payoff was set up;
   - the knowledge model is consistent (no acting on unknown facts);
   - each lens's order and proportions hold;
   - the rhythm: no long run of beats turning the same way;
   - the arc completes (want met or refused, need met or failed).
3. **Learned judges** (the fingerprint's text modality, EmbeddingGemma heads, milliseconds once trained):
   - axes such as clear stakes, surprise that's earned, a crisis that's a real choice, and an ending that pays the
     premise;
   - each axis starts from its words (zero-shot) and trains from pairs: LitBench's result is that Bradley-Terry heads
     trained on preference pairs beat every zero-shot LLM judge (78% against 73% agreement with people);
   - an LLM judge is a checker on top, never the score (as with Clef-Omni).
4. **Measures of the made piece**:
   - the finished film's words (measure_film's transcript) and pictures are scored back onto the story's beats: did
     the turn happen at its time, did the reveal land;
   - the narrative-arc measures of Boyd, Blackburn and Pennebaker (staging, plot progression and cognitive tension from
     word categories) are computed on the dialogue and narration, and the emotional arc on the six shapes of Reagan et
     al.;
   - for a launch, the outcome itself (engagement against the account's own baseline) is the last word.

Tests are code:

```ts
test("the turn lands by the midpoint in every theme", () =>
  forEachTheme(themes, t => expect(sample(pattern(t), { seed })).toTurn("trust", { from: "-", to: "+", by: 0.5 })));
```

## Games and dialogue

- **Storylets** (quality-based narrative, as Failbetter's and Emily Short's): each beat can compile to a storylet with
  preconditions on state and effects on it. A **drama manager** picks the next storylet to keep the live game near the
  story's target tension curve (Façade's beat selection, made a library).
- **Planning, where characters must act on their own goals and beliefs:** the knowledge model is the same structure a
  belief-aware narrative planner searches (Ware's Sabre: intention, theory of mind, conflict). A small planner can fill
  a gap between two beats with actions each character would plausibly take.
- **Dialogue**: a line carries what's said, what's meant (subtext), and the value it turns. It compiles to `script.ts`
  lines for a film, and to Ink or Yarn Spinner for a game, so authors can use either engine.

## How state of the art this is

| part | the state of the art | this plan |
|---|---|---|
| structure as data | Dramatica's storyform (1990s, closed); narrative planners (Sabre, AIIDE 2021) | typed TypeScript an agent writes; tsc and exact checks |
| LLM story writing | plan, then write: Dramatron (2022), Re3/DOC (2022-23), Agents' Room (2024) | same split, but the plan is a checked program |
| consistency | FlawedFictions (2025): plot holes beat nearly every model on longer stories; ConStory-Bench (2026): consistency bugs in long generation | the knowledge model catches the commonest kind by construction, before any words exist |
| judging quality | LitBench (2025, EACL 2026): reward models trained on pairs reach 78% agreement and beat zero-shot judges | fingerprint text heads trained on our own pairs (launch outcomes, Shane's notes) |
| measuring arcs | Boyd et al. (2020) narrative arc; Reagan et al. (2016) emotional arcs | computed on the finished piece and laid against the planned beats |
| interactive story | storylets, drama managers (Façade, Versu), Ink, Yarn | one story model compiles to film and to game |

What's new is the combination: one typed story model, read through many frameworks at once, generated without a
fixed answer, and tested from the type checker through to the outcome of the finished piece. Nobody ships that, as far
as I found. Each part is proven somewhere; the risk is in their joins, not in any one part.

What it won't do: no lens and no judge makes a story good. They catch what's broken and rank options. The person still
chooses (Shane's notes are the training data that matter most).

## The launch corpus: the first data

`~/.vd/launch` (11-22): the whatships.com catalog of 3,328 launch videos posted on X, with each post's engagement.
- **The pairs:** 158 accounts posted several launches, so each account's best and worst launch is a pair with the
  audience held fixed.
- **What's measured:** each film's transcript, shots, sound and pictures (vd's measure_film).
- **What it gives the story SDK:**
  - its first learned priors (the launch lens);
  - its first text-axis training pairs;
  - a check on whether story structure predicts outcomes at all, apart from distribution.

### What it found (11-22)

The question was whether craft shows in the films, or whether distribution and the news explain the gap.
Within an account, the audience is the same, so distribution is held as far as this data allows.

- **Paired comparison** (`tools/compare.py`, 156 of 158 pairs measured, sign tests with ties dropped, 23 features):
  - **Length is the one finding that holds.** The better launch is the longer film in 104 of 155 pairs (67%, p < 0.001).
    The medians are 50.5 s against 33.1 s.
  - **A voice speaking:** the better film has one in 40 of 63 pairs where they differ (63%, p = 0.043).
  - **A weaker beat:** p = 0.026, a tiny difference (strength 0.29 against 0.31).
  - Neither of those two survives a correction for testing 23 features (Bonferroni: p < 0.002).
  - Nothing measured about the cut shows any difference: cuts a second, shot length, first cut, motion, sync to the
    music, colour, loudness, brightness.
- **Blind read** (30 pairs drawn at random from those measured; a strip of eight frames a film, the length and the
  transcript, with no engagement shown; each guess and its reason written down before the key was read):
  - **23 of 30 right** (77%, one-sided sign test p = 0.003).
  - **But always picking the longer film gets 25 of 30.** The guesses agreed with "the longer one" on 24 of 30, beat it
    on 2 pairs, and lost to it on 4.
  - Where the engagement gap is large (1.5 or more in `own`, each film's engagement against its own account's, the gap a pair is ranked by), the read was right on 18 of 19; where
    it is small, 5 of 11, a coin.
  - The reasons given for right guesses were the size of the news (a flagship release against a variant, a raise, a
    famous face), not the cut.
- **Plainly:** craft, as anything we can measure or a reader can see in the film, doesn't separate the better launch
  from the worse one here. What does is how big the news is, and the film's length, which goes with it: a flagship
  launch gets a longer film, a voice and a founder. Length is a marker, not a lever, so these pairs give no reason to
  pad a film.
- **What carries into the stack:**
  - The launch lens's priors are the shape of the films that won, as norms, not as causes: 40 to 60 s, a voice, the
    problem in the viewer's words, a named proof (a number, a benchmark, a raise), how to start.
  - `raea fingerprint` gets a launch profile (the measured medians) and the pair evidence, with these limits written
    beside it.
  - Taste for the decision model comes from Shane's notes, not from engagement, which mostly measures the news.

## Measured: the same prompts with and without it (11-22)

`sdk/story/bench/` holds three benches: `edges.ts` (29 odd and broken stories, each with what should happen), `fuzz.ts`
(valid stories with one field made the wrong type, run through every reader) and `ab.ts` with `ab-report.ts` (the
same prompts written five ways, judged blind). Run data is in `~/.vd/story/ab/run1` to `run3`; run3 is the complete
one; the three runs and their smoke tests cost about $14.70 together.

### What the edges and the fuzzer found, and what changed

The fuzzer found 21 places where `fromData` accepted something a reader then crashed on, and a test story found that
`unsets` did nothing. All are fixed (ST10a):

- **`fromData` holds a story to `storySchema()`** (`shapeErrors`): a field of the wrong type, or a field in the wrong
  place, is refused with where it is. 270 of the 1,572 corpus stories carried one that used to be dropped silently:
  `turns` on a beat (103, so the writer thought a beat moved a value and it didn't), `believes` as text (97), a
  `dilemma`, `claim` or `ends` on a beat, a `setup`, `asks` or `reveals` on an event.
- **`unsets`** takes a fact back from the event's agents and from the audience, and a claim can't rest on a fact that
  has since been undone.
- **Two beats with one id** are an error, both in `fromData` and in the checks: a composition finds its Sequence by id.
- **`seconds`** can't be negative, a telling that shows no event is a finding, and `asker` says when it's asked about a
  beat that isn't there.

What held without a change: cause cycles and self-causes are refused; a mystery's detective acting on an unseen clue is
a belief error that a fair-play clue removes; a flashback hook passes when its fact is hidden then paid; a negative arc
needs no settled need; Japanese keys and text round-trip through story lines; 20 seeds of a story with choice points
give 20 distinct stories; every mutant raises a finding. Scale: checks, the McKee lens and a window over 2,000 events
and 2,000 beats take about 0.4 s each, and one beat's 20 typed questions take 8 ms after a 1.2 s setup.

### The A/B: five ways to write the same prompt

Eight prompts (a 30 s tent launch, a 45 s coffee brand film, a 3 minute whodunit with fair play, a wordless
kishotenketsu, a 40-shot short film, an RPG quest with a lying quest giver, a tragedy, a 15 s sunscreen ad; made-up
brands), two samples each, one writer (Claude Sonnet 5.5) for every arm:

- **A, plain**: the prompt, a beat script back (what's on screen, who says what, seconds), one call.
- **C, plain + an editor**: A, then one round where the writer critiques its own script against the same things the
  checks and the prompt's lenses hold (said in words: therefore or but, never and then; setups paid; no acting on
  what a character can't know; the arc settled; proof shown; the framework's stages), and rewrites.
- **D, plain + raea/story as the editor**: A's script read back blind into story data by another model (Gemini 3.7
  Flash), `checkStory` and the prompt's lenses run on it, the findings handed to the writer by beat number, and one
  rewrite. C's round with the editor's eye replaced by the exact checks.
- **B, written as story data**: the writer writes the story itself in the data, under the guide; the checks and
  lenses find what's wrong and the writer repairs it, up to two rounds; its beats become the beat script.
- **E, story data, then a screenwriter**: B's checked story handed to the writer as story lines, to write the beat
  script from.

Every arm ends as the same numbered beat script. Two judges of other families (GPT-5.4, Gemini 3.1 Pro) ranked all five
for each prompt, letters shuffled, and scored each 1 to 10 on cause, setups, clarity, the brief, feeling and overall.

| arm | mean rank (of 5) | ranked first | overall | over plain |
|---|---|---|---|---|
| D, plain + raea/story as the editor | **1.91** | 47% | **8.44** | 26 of 32 (p = 0.001) |
| C, plain + an editor | 1.97 | 44% | 8.22 | 26 of 32 (p = 0.001) |
| A, plain | 3.22 | 3% | 7.31 | |
| E, story data, then a screenwriter | 3.59 | 6% | 7.03 | 12 of 32 (p = 0.22) |
| B, written as story data | 4.31 | 0% | 6.38 | 7 of 32 (p = 0.002) |

D and C tie (16 of 32). D came first on both the long film and the whodunit, where there's most to track (40 shots;
clues the detective may only act on once shown), and on the tent launch; C came first on the coffee film,
the kishotenketsu, the quest and the tragedy. Their cost was alike ($0.12 a story against $0.12).

**What it means.** raea/story makes a script better as the editor of a draft, not as the language it's drafted in.
Written straight into the data, a story passes its checks (13 of 16 by the last round) and reads worst: fewer, more
abstract beats, a narrator saying what the screen should show ("You want to go. The tent is what stops you."), causes
that hold in the data and don't show on screen. The judges scored B's cause and setups lower than plain's (6.4 against
7.2), although its data has every cause. Handing that story to a screenwriter (E) recovers most of the loss but not
all of it. Read back from a draft instead, the checks name holes by beat (on one tent launch: four events with no cause
the viewer saw, four beats that only follow the one before, the hero's arc never settled, the brand never in the
mentor's place), and the rewrite fixes what a viewer can see.

So the way to use it, for a person or an agent: **write the script, read it back as story data, check it, fix by beat,
and keep the story file for what the checks hold from then on** (the composition's beats, `raea check`, the cuts). The
data is the checker's language, and the script is the viewer's.

**How far to trust it.** 16 prompt samples and two LLM judges, who agreed on the best of five 9 times in 16; one writer
model; the extraction is noisy (a B story that passed its own checks still reads back with 1.3 findings a beat, about
the level of every arm), so it's a guide to the rewrite, not a score. D's own extraction score is the lowest (1.44 a
beat) partly because it was fixed against the same kind of reading. The edits that won are the ones a human editor
would also make; the checks don't know whether a line lands.

## Build order

| step | what | confidence |
|---|---|---|
| ST0 | the model's types (world, telling, render) and `story()`, compiled to `script.ts`; the three Rust changes (`Brief.story`, `Scene.beat`, story-only checks) | high |
| ST1 | exact checks: turns, causes, setups and payoffs, rhythm, arc | high |
| ST2 | the knowledge model: suspense, mystery, surprise curves and the plot-hole check | medium-high |
| ST3 | lenses: McKee, Pixar spine, Storynomics, PAS first (the ones ads need), then Campbell/Vogler, Harmon, Save the Cat, Kishōtenketsu, Propp, Bernays | high for the exact parts; their maps are inferred, so medium |
| ST4 | sampling: choice points, seeds, lens proposals, pruning; `raea story sample`, `matrix` | medium-high |
| ST5 | the skill tree: craft/story/ in the raea skill, one page a framework in our own words with sources, and a router | high |
| ST6 | learned judges: text axes on the fingerprint, trained from launch pairs and Shane's notes | medium: depends on whether text predicts outcomes in our data |
| ST7 | measures of the made piece laid back onto the beats (transcript arcs, beat timing) | medium |
| ST8 | games: storylets, the drama manager, Ink and Yarn export | medium |
| ST9 | a gap-filling planner for character actions | low-medium: research-grade, after ST2 shows the knowledge model holds |

Decided 11-22 (Shane):
- A story is its own module, imported into the brief explicitly (`story:`); nothing reads it unless something
  imports it.
- The SDK stays TypeScript. Rust gets only the four small changes above.

Sources:
- [Sabre](https://www.cs.uky.edu/~sgware/projects/sabre) (Ware and Siler, AIIDE 2021)
- [FlawedFictions](https://arxiv.org/abs/2504.11900) (2025)
- [ConStory-Bench](https://arxiv.org/abs/2603.05890) (2026)
- [LitBench](https://arxiv.org/abs/2507.00769) (2025, EACL 2026)
- Campbell, *The Hero with a Thousand Faces* (1949)
- Vogler, *The Writer's Journey*
- Catmull, *Creativity, Inc.* (2014)
- McKee, *Story* (1997)
- McKee and Gerace, *Storynomics* (2018)
- Bernays, *Propaganda* (1928)
- Boyd, Blackburn and Pennebaker, "The narrative arc" (Science Advances, 2020)
- Reagan et al., "The emotional arcs of stories" (EPJ Data Science, 2016)
