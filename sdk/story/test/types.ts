// tsc only (ST0a): each @ts-expect-error is a reference the story doesn't declare.
import { story, value } from "../src/model.ts";
const base = { values: { v: value("a", "b") }, facts: { f: "a fact" }, agents: { x: {} } } as const;
story({ ...base, world: { e1: { who: "x", does: "acts" } }, tellings: { t: [{ shows: ["e1"] }] } });
// @ts-expect-error an agent it doesn't have
story({ ...base, world: { e1: { who: "y", does: "acts" } }, tellings: { t: [{ shows: ["e1"] }] } });
// @ts-expect-error a value it doesn't have
story({ ...base, world: { e1: { who: "x", does: "acts", turns: { w: 1 } } }, tellings: { t: [{ shows: ["e1"] }] } });
// @ts-expect-error a fact it doesn't have
story({ ...base, world: { e1: { who: "x", does: "acts", sets: ["g"] } }, tellings: { t: [{ shows: ["e1"] }] } });
// @ts-expect-error a cause that isn't an event
story({ ...base, world: { e1: { who: "x", does: "acts" }, e2: { who: "x", does: "acts", because: "e3" } }, tellings: { t: [{ shows: ["e1"] }] } });
// @ts-expect-error a beat showing an event the world doesn't have
story({ ...base, world: { e1: { who: "x", does: "acts" } }, tellings: { t: [{ shows: ["e2"] }] } });
// @ts-expect-error a payoff of a setup nothing planted
story({ ...base, world: { e1: { who: "x", does: "acts" } }, tellings: { t: [{ shows: ["e1"], setup: "gun" }, { shows: ["e1"], pays: "gnu" }] } });
