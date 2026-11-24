# Checking a story

Three ways in, one set of checks:

- `bun test` or a script: `checkStory(story, telling)` gives findings, errors first.
- `raea check launch.story.ts`: the story alone, before anything is drawn (every taste rule whose check is over
  `"story"` or `"both"`, run once with no frames).
- `raea check EDIT.tsx`: the story and the film together, as an ordinary `raea/check` check.

The check is the project's own three lines, so it's explicit which story and which telling:

```ts
// checks/story.ts
import { storyCheck } from "raea/story/raea/checks";
import launch from "../launch.story";
export default storyCheck(launch, { telling: "30s", lenses: ["mckee", "pas"] });
```

and a taste line names it: `{ text: "The story holds and is on screen", check: "checks/story.ts" }`. On a composition
it also says when a beat has no Sequence named for it, when the shots come out of the telling's order, and when a
beat's line isn't spoken inside its shot.

## What each finding means, and the usual fix

| check | level | it means | fix |
|---|---|---|---|
| `cause` | error | an event has no cause, or its cause happens later | name the earlier event in `because`, or mark it `routine` |
| `belief` | error | a character acts on a fact they can't know yet | let an earlier event they're in set or teach it |
| `claim` | error | a claim with no proof, or proof not set by then | set the proof in an event shown at or before the claim |
| `arc` | warn | a want or need is never settled | `ends: { hero: "need" }` on the event that settles it |
| `setup` | warn | a setup never paid, or paid with none planted | pay it later, or cut it |
| `turns` | warn | a beat changes nothing | give a shown event a turn, or cut the beat |
| `and-then` | warn | a beat follows from nothing the audience saw | show the cause earlier |
| `surprise` | warn | a big turn nothing set up | plant a cause, pay a setup, or mark it `surprise: "meant"` |
| `idea` | warn | the focus value ends where it began | land the ending somewhere else |
| `rhythm` | note | four beats in a row turn the same way | break the run with a reversal |
| `dead-end` | note | an event causes nothing | let something follow from it, or cut it |

A beat can say it means a finding: `meant: ["mckee:complications"]` (or one string, `meant: "rhythm"`) takes those
findings about that beat out of what checks and lenses report, for a breather the journey wants or a run of losses
that's the point. `surprise: "meant"` is the same for a surprise. Write it where you decided, not to quiet a check.

A lens adds its own (`mckee:crisis`, `bernays:invented`, ...), each explained on its page. A lens error is as real as an
exact one when the brief names that lens.
