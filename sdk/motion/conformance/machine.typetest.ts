// State machines' types (sdk/motion/src/machine.ts): every line marked @ts-expect-error must fail to compile (tsc reports
// an unused directive if it doesn't). Run: sdk/react/bench/playtest/typecheck.sh
import { machine, type Snapshot, type StateName } from "../src/machine";

interface Player { x: number; vy: number; sm: Snapshot }
interface Env { jump: boolean }

// valid: nested states, guards typed by the context and env, XState's spellings
const player = machine<Player, Env>()({
  id: "player",
  initial: "alive",
  states: {
    alive: {
      initial: "idle",
      always: { to: "dead", when: (p) => p.x < 0 },
      states: {
        idle: { always: { target: "jump", guard: (_p, e) => e.jump } },
        jump: { entry: (p) => ({ ...p, vy: -20 }), always: { to: "idle", when: (p) => p.vy >= 0 }, after: { 90: "idle" } },
      },
    },
    dead: { type: "final" },
  },
});
const ok: StateName<typeof player> = "jump";
player.is({ x: 0, vy: 0, sm: player.start() }, "alive");

// @ts-expect-error a transition to a state that doesn't exist
machine<Player, Env>()({ id: "m", initial: "a", states: { a: { always: { to: "b" } } } });
// @ts-expect-error a bare target that doesn't exist
machine<Player, Env>()({ id: "m", initial: "a", states: { a: { after: { 30: "nowhere" } } } });
// @ts-expect-error an initial state that doesn't exist
machine<Player, Env>()({ id: "m", initial: "zz", states: { a: { type: "final" } } });
// @ts-expect-error a substate's initial that doesn't exist
machine<Player, Env>()({ id: "m", initial: "a", states: { a: { initial: "q", states: { b: { type: "final" } } } } });
// @ts-expect-error a guard reading a field the context lacks
machine<Player, Env>()({ id: "m", initial: "a", states: { a: { always: { to: "a", when: (p) => p.lives > 0 } } } });
// @ts-expect-error a guard reading a field the env lacks
machine<Player, Env>()({ id: "m", initial: "a", states: { a: { always: { to: "a", when: (_p, e) => e.fire } } } });
// @ts-expect-error an action that doesn't return the context
machine<Player, Env>()({ id: "m", initial: "a", states: { a: { entry: (p) => p.x, always: "a" } } });
// @ts-expect-error both to and target
machine<Player, Env>()({ id: "m", initial: "a", states: { a: { always: { to: "a", target: "a" } } } });
// @ts-expect-error a transition with no target
machine<Player, Env>()({ id: "m", initial: "a", states: { a: { always: { when: () => true } } } });
// @ts-expect-error a query for a state the machine doesn't have
player.is({ x: 0, vy: 0, sm: player.start() }, "flying");
// @ts-expect-error a machine without an id
machine<Player, Env>()({ initial: "a", states: { a: { type: "final" } } });
// @ts-expect-error a state name that isn't one
const nope: StateName<typeof player> = "swim";
void ok; void nope;
