// The playtest's machine checks on a toy game with known faults (sdk/react/docs/state-machines.md): a state entered and
// never left while the input changes, an unreachable state, a loop without progress, a state the bots never reach.
// Machine-generic: the toy is a corridor, not a platformer. Run: bun test sdk/react/bench/playtest/machines.test.ts
import { describe, expect, test } from "bun:test";
import { machine, type Snapshot } from "../../../motion/src/machine";
import type { Playtest } from "../../../motion/src/games";
import { MachineTrace, type TransitionRow } from "./machines";
import { Game, novelty, random } from "./search";
import { machineRule } from "./judge";

interface Bot { x: number; sm: Snapshot }
interface S { bot: Bot; tick: number }
type In = { keys: string[] };
const bot = machine<Bot, In>()({
  id: "bot",
  strict: false,
  initial: "walk",
  states: {
    walk: {
      initial: "step",
      tick: (b, i) => ({ ...b, x: Math.max(0, b.x + (i.keys.includes("R") ? 1 : 0) - (i.keys.includes("L") ? 1 : 0)) }),
      always: { to: "trap", when: (b) => b.x >= 30 },
      states: {
        // flickers between two substates every 2 frames whatever happens: a loop without progress when standing still
        step: { after: { 2: "stride" } },
        stride: { after: { 2: "step" } },
      },
    },
    // its way out never holds: entered, never left
    trap: { always: { to: "walk", when: (b) => b.x < 0 } },
    // nothing goes here
    island: { always: "walk" },
    // a final state nothing goes to: unreachable, and never entered
    moon: { type: "final" },
  },
});
const step = (s: S, i: In): S => ({ bot: bot.step(s.bot, i), tick: s.tick + 1 });
const p: Playtest<S> = {
  initial: { bot: { x: 0, sm: bot.start() }, tick: 0 },
  step: (s, i) => step(s, { keys: i.keys ?? [] }),
  actions: [["R"], ["L"], []],
  progress: (s) => s.bot.x / 100,
  observe: (s) => ({ x: s.bot.x, y: 0, onGround: true }),
  machines: (s) => ({ bot: bot.watch(s.bot) }),
};

describe("machine checks on a game with known faults", () => {
  const g = new Game(p);
  const mt = new MachineTrace(p.machines!);
  g.trace = mt;
  random(g, { episodes: 10, frames: 600, seed: 3 });
  novelty(g, { iterations: 300, explore: 8, seed: 4, maxFrames: 2000 });
  // the run: standing still for 200 frames (the flicker loops while progress is flat)
  const rows: TransitionRow[] = [];
  let s = p.initial;
  for (let f = 0; f < 200; f++) {
    const after = p.step(s, { keys: [] } as any, f);
    rows.push(...mt.observe(s, after, { frame: f + 1, keys: [], progress: p.progress!(after, f + 1), outcome: null }, true));
    s = after;
  }
  mt.endPlay(200);
  const report = { machines: mt.report(rows, () => 0) } as any;
  const m = report.machines.bot;

  test("coverage counts reached states and fired edges", () => {
    expect(m.stateCoverage.missing.sort()).toEqual(["island", "moon"]);
    expect(m.transitionCoverage.missing).toEqual(["trap->walk (always)", "island->walk (always)"]);
    expect(machineRule(report, "state_coverage", { machine: "bot", min: 1 })!.faults).toBe(2);
    expect(machineRule(report, "state_coverage", { machine: "bot", min: 0.5 })!.faults).toBe(0);
    expect(machineRule(report, "transition_coverage", { machine: "bot" })!.says).toContain("never: trap->walk (always)");
  });
  test("a state entered and never left while the input changed is a softlock", () => {
    expect(m.softlocks.map((x: any) => x.state)).toEqual(["trap"]);
    expect(m.softlocks[0].longestStay.changes).toBeGreaterThanOrEqual(3);
    expect(machineRule(report, "machine_softlock", { machine: "bot" })!.faults).toBe(1);
  });
  test("an edge firing over and over while progress is flat is a loop, counted when asked for", () => {
    expect(m.loops.length).toBeGreaterThan(0);
    expect(machineRule(report, "machine_softlock", { machine: "bot", loops: true })!.faults).toBe(1 + m.loops.length);
  });
  test("static analysis: unreachable states and reached_state", () => {
    expect(m.unreachable).toEqual(["island", "moon"]);
    expect(machineRule(report, "unreachable_states", {})!.faults).toBe(2);
    expect(machineRule(report, "reached_state", { machine: "bot", state: "trap" })!.faults).toBe(0);
    expect(machineRule(report, "reached_state", { machine: "bot", state: "moon" })!.faults).toBe(1);
    expect(machineRule(report, "reached_state", { machine: "nope", state: "x" })!.says).toContain("no machine nope");
  });
  test("the report is the same on a second run (deterministic)", () => {
    const g2 = new Game(p);
    const mt2 = new MachineTrace(p.machines!);
    g2.trace = mt2;
    random(g2, { episodes: 10, frames: 600, seed: 3 });
    novelty(g2, { iterations: 300, explore: 8, seed: 4, maxFrames: 2000 });
    let s2 = p.initial;
    const rows2: TransitionRow[] = [];
    for (let f = 0; f < 200; f++) {
      const after = p.step(s2, { keys: [] } as any, f);
      rows2.push(...mt2.observe(s2, after, { frame: f + 1, keys: [], progress: 0, outcome: null }, true));
      s2 = after;
    }
    mt2.endPlay(200);
    expect(JSON.stringify(mt2.report(rows2, () => 0))).toBe(JSON.stringify(report.machines));
    expect(rows2).toEqual(rows);
  });
});
