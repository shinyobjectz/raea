# Sampling: many stories from one

Put a choice where you'd otherwise pick: `choose(["the backlog doubles", "a client walks"])` for an event's `does`, a
beat, or a whole telling. A story with choices is a space of stories; a seed picks one.

```ts
import { choose, sample, samples, matrix } from "raea/story";

const one = sample(launch, { seed: 7, telling: "30s", lenses: [mckee] });  // the first passing story from seed 7
const five = samples(launch, 5, { telling: "30s" });                        // five different passing ones
```

`sample` walks seeds from the one you give until a story passes the exact checks and every lens you name, so a sampled
story always holds. The same seed gives the same story.

## One pattern, many premises

`matrix(pattern, themes)` runs one story pattern (a function from a premise to a story) over many premises and says how
often each holds and which checks fail where. Use it before committing to a structure for a series: if the pattern
only works for one premise, it's that premise working, not the pattern.

## Choices in a telling

`choose([beatA, beatB, null])` in a telling makes a beat optional (`null` leaves it out). That's how one story yields a
15, 30 and 45 second cut, or a game's branches, all checked the same way.
