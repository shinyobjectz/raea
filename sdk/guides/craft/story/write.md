# Writing a story

**Draft the script first, then the story.** Measured on eight prompts written five ways (docs/story.md, "Measured"):
a script drafted straight into story data passes its checks and reads worst of all (fewer, more abstract beats, a
narrator saying what the screen should show); a plain draft read back into story data, checked, and fixed by beat
reads best, tied with a hard editing pass and well over the plain draft (26 of 32 judge votes). So:

1. Write the script as a screenwriter would: what's on screen and what's said, beat by beat.
2. Read it back into a story (below), recording only what the script shows: an event with no shown reason gets no
   `because`, a beat that moves nothing gets no `turns`.
3. `checkStory` and the brief's lenses: each finding names a beat. Fix the script there, so the screen shows it.
4. Keep the story file. From then on it's what the composition's beats, `raea check` and the other cuts hold to.

A story is a module, `NAME.story.ts`, with one default export made by `story()`. tsc checks its references: a beat
that shows an event you didn't write, an agent you didn't declare, a setup nobody planted, won't compile.

```ts
import { story, value } from "raea/story";

export default story({
  title: "Halden Outdoor: the first tent",
  idea: { value: "nerve", side: "you're braver than your gear", counter: "only experts go out" },
  values: { nerve: value("afraid", "bold") },
  facts: { pitched: "the tent goes up in one pull" },
  agents: {
    june: { want: "a night outside", need: "to trust herself", role: "customer", mask: "hero" },
    tent: { role: "brand", mask: "gift" },
  },
  world: {                                            // what happens, in the order it happens
    stays: { who: "june", does: "watches the hills from her window", routine: true, turns: { nerve: -1 } },
    dusk: { who: "june", does: "drives out as the light goes", because: "stays", turns: { nerve: -1 }, dilemma: ["turn back", "keep going"] },
    pitch: { who: "june", with: ["tent"], does: "pulls the tent up in one go", because: "dusk", sets: ["pitched"], turns: { nerve: 3 }, ends: { june: "need" } },
  },
  tellings: {                                         // how each cut tells it
    "20s": [
      { id: "hook", shows: ["pitch"], hides: ["pitched"], asks: ["pitched"], setup: "how", feel: "curiosity", onScreen: "A tent glows on a dark hill." },
      { id: "before", shows: ["stays"], feel: "recognition" },
      { id: "dusk", shows: ["dusk"], feel: "concern" },
      { id: "pull", shows: ["pitch"], reveals: ["pitched"], pays: "how", feel: "delight" },
    ],
  },
  lenses: ["mckee", "storynomics"],
});
```

## The three layers

- **The world** is what happens: events in story order, each with who acts, why (`because`: the earlier event that
  caused it), what it makes true (`sets`), what it moves (`turns`: a value, -3 to 3) and what it settles (`ends`).
  An event that's just how things are is `routine`.
- **A telling** is how one cut tells it: beats in the order the audience sees them. A beat `shows` events (a hook can
  show the ending first), `hides` and `asks` what the audience doesn't know yet, `reveals` it later, plants a `setup`
  and `pays` it. Give each beat an `id`: a composition names its Sequence by it and `raea check` looks for it.
- **The render** is the film: `compileScript(story, telling)` makes the brief's script (a scene a beat, each naming its
  `beat`), and `<Beat times={...} id="hook">` in a composition is a Sequence timed from the telling.

## Into the project

The brief imports the story explicitly: `brief({ ..., story })` (raea keeps it as written). A check imports it too,
`checks/story.ts` (see [check](check.md)). Nothing finds a story by itself: what uses it says so.

## Rules that save a rewrite

- Write the world before any telling. Two cuts of one world (a 15 s and a 45 s) share their causes.
- Every event after the first has a cause or is routine. "And then" is the most common hole.
- A value is a pair of poles ("buried", "free"), not a topic. The `idea.value` is the one the story is about.
- Facts are keys, not sentences: `believes`, `needs`, `sets`, `proof`, `asks` all name declared facts.
- A claim (`claim`) names its `proof`, facts an event makes true. Never write proof nothing shows.

## Any length

A story has no length cap: the checks and lenses run over 55 beats or 2,000 alike. For a feature or a game, give each
beat a `scene` (consecutive beats with one scene are its shots) and write top-down: the world, then the scenes, then
the shots a scene at a time, checking each level before the next.

- `stateAt(story, telling, k)` is everything the story holds just before beat k: what the audience knows, open
  questions, suspense, each value's level, tension, setups not yet paid, wants and needs not yet settled, events not
  yet shown. It's about the same size at minute 3 or minute 100, so a step that writes or judges beat k (a person, an
  agent, a model) reads that and the beats around it, never the whole telling.
- `windowAt(story, telling, k)` is that state plus the events still usable and three beats before and two after, as
  story lines: what to hand an agent writing the beat between.

## Story lines

`toLines(story)` writes a story as one keyword-led line per value, fact, agent, event and beat, the way Fountain
writes a screenplay, and `fromLines(text)` reads it back exactly (a broken line says which):

```
VALUE nerve: afraid / bold
EVENT pitch: june with tent -- pulls the tent up in one go | because dusk | sets pitched | turns nerve+3 | ends june:need
BEAT pull: shows pitch | reveals pitched | pays how | feel delight
  SAYS june (quietly): Okay. // she's talking herself into it
```

It's for skimming a long story whole, diffing two versions, and anything that writes a story a line at a time. The
TypeScript stays the story; `toData` / `fromData` are the same story as JSON, with `storySchema()` and a story's own
`beatSchema(story)` for anything that must answer in that shape.
