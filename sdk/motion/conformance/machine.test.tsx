// State machines (sdk/motion/src/machine.ts, spec: "State machines, step by step"): polling, delays, nesting, history,
// events, deciders, the trace, snapshots as plain JSON, validation, and determinism inside a game's fold.
import { describe, expect, test } from "bun:test";
import {
  firstDecider, inState, keyEdges, machine, replayDecider, stateOf, stepAll, transitionOf, useGameState,
  type Decider, type Snapshot, type TransitionRecord,
} from "../src/games";
import { renderFrame } from "../src/test";

interface Ctx { x: number; log: string[]; sm: Snapshot }
const ctx0 = (x = 0): Ctx => ({ x, log: [], sm: { path: [], at: [], n: 0, e: -1 } });
const note = (s: string) => (c: Ctx): Ctx => ({ ...c, log: [...c.log, s] });
const run = <C,>(m: { step: (c: C, e: any, o?: any) => C }, c: C, steps: number, env: any = {}, o?: any) => {
  for (let i = 0; i < steps; i++) c = m.step(c, env, o);
  return c;
};

describe("polling and delays", () => {
  const m = machine<Ctx, { go: boolean }>()({
    id: "basic",
    initial: "a",
    states: {
      a: { always: [{ to: "b", when: (_c, e) => e.go }] },
      b: { always: { to: "c" } },
      c: { after: { 3: "a" } },
    },
  });
  test("the first step enters the initial state; a guard fires when it holds; one transition a step", () => {
    let c = m.step(ctx0(), { go: false });
    expect(m.state(c)).toBe("a");
    c = m.step(c, { go: false });
    expect(m.state(c)).toBe("a");
    c = m.step(c, { go: true });
    expect(m.state(c)).toBe("b");
    // b goes on to c unconditionally, but only on the next step
    c = m.step(c, { go: true });
    expect(m.state(c)).toBe("c");
  });
  test("after: N fires once the state has been active N steps", () => {
    let c = run(m, ctx0(), 1, { go: true }); // start in a
    c = m.step(c, { go: true }); // b
    c = m.step(c, { go: true }); // c, steps 0
    expect(m.steps(c)).toBe(0);
    c = m.step(c, { go: false });
    c = m.step(c, { go: false });
    expect(m.state(c)).toBe("c");
    expect(m.steps(c)).toBe(2);
    c = m.step(c, { go: false });
    expect(m.state(c)).toBe("a");
  });
  test("before the first step the state is the initial leaf", () => {
    expect(m.state(ctx0())).toBe("a");
    expect(m.is(ctx0(), "a")).toBe(true);
    expect(m.steps(ctx0())).toBe(0);
  });
  test("a transition's own after combines with its guard", () => {
    const t = machine<Ctx, { go: boolean }>()({
      id: "t", initial: "a", states: { a: { always: { to: "b", after: 5, when: (_c, e) => e.go } }, b: { always: "a" } },
    });
    let c = run(t, ctx0(), 5, { go: true });
    expect(t.state(c)).toBe("a");
    c = t.step(c, { go: true });
    expect(t.state(c)).toBe("b");
  });
});

describe("actions", () => {
  const m = machine<Ctx>()({
    id: "acts",
    initial: "a",
    states: {
      a: { entry: note("in a"), exit: note("out a"), tick: (c) => ({ ...c, x: c.x + 1 }), always: { to: "b", when: (c) => c.x >= 2, actions: note("a->b") } },
      b: { entry: [note("in b"), note("in b again")], type: "final" },
    },
  });
  test("entry, tick, exit and transition actions run in order and return new contexts", () => {
    const start = ctx0();
    let c = m.step(start, {});
    expect(c.log).toEqual(["in a"]);
    expect(c.x).toBe(1); // the entry step ticks too
    c = m.step(c, {});
    expect(c.x).toBe(2);
    c = m.step(c, {});
    expect(c.log).toEqual(["in a", "out a", "a->b", "in b", "in b again"]);
    expect(m.state(c)).toBe("b");
    expect(start.log).toEqual([]); // the old context is unchanged
  });
});

describe("nested states", () => {
  interface E { hurt?: boolean; move?: boolean; air?: boolean; leave?: boolean }
  const m = machine<Ctx, E>()({
    id: "nest",
    initial: "alive",
    states: {
      alive: {
        initial: "ground",
        entry: note("+alive"),
        exit: note("-alive"),
        always: [{ to: "hurt", when: (_c, e) => !!e.hurt }],
        states: {
          ground: {
            initial: "idle",
            entry: note("+ground"),
            exit: note("-ground"),
            always: [{ to: "air", when: (_c, e) => !!e.air }],
            states: {
              idle: { entry: note("+idle"), exit: note("-idle"), always: { to: "run", when: (_c, e) => !!e.move } },
              run: { entry: note("+run"), exit: note("-run"), always: [{ to: "idle", when: (_c, e) => !e.move }, { to: "ground", when: (_c, e) => !!e.leave }] },
            },
          },
          air: { always: { to: "ground", when: (_c, e) => !e.air } },
        },
      },
      hurt: { after: { 2: "alive" } },
    },
  });
  test("entering a compound state enters its initial substates, outer first", () => {
    const c = m.step(ctx0(), {});
    expect(m.path(c)).toEqual(["alive", "ground", "idle"]);
    expect(c.log).toEqual(["+alive", "+ground", "+idle"]);
  });
  test("an enclosing state's transitions apply from any substate; the innermost fires first", () => {
    let c = m.step(ctx0(), {});
    c = m.step(c, { move: true, air: true });
    // idle's own move transition beats ground's air transition
    expect(m.state(c)).toBe("run");
    c = m.step(c, { move: true, air: true });
    expect(m.path(c)).toEqual(["alive", "air"]);
    c = m.step(c, { hurt: true, air: true });
    expect(m.path(c)).toEqual(["hurt"]);
  });
  test("leaving exits inner states first; a sibling transition keeps the shared parent", () => {
    let c = m.step(ctx0(), {});
    c = { ...c, log: [] };
    c = m.step(c, { move: true });
    expect(c.log).toEqual(["-idle", "+run"]);
    c = { ...c, log: [] };
    c = m.step(c, { hurt: true, move: true });
    expect(c.log).toEqual(["-run", "-ground", "-alive"]);
  });
  test("targeting an enclosing state leaves and re-enters it", () => {
    let c = m.step(ctx0(), {});
    c = m.step(c, { move: true });
    c = { ...c, log: [] };
    c = m.step(c, { move: true, leave: true });
    // run's first transition (to idle) doesn't hold; its second goes to ground, which is re-entered at idle
    expect(c.log).toEqual(["-run", "-ground", "+ground", "+idle"]);
    expect(m.steps(c, "alive")).toBe(2);
    expect(m.steps(c, "ground")).toBe(0);
  });
  test("is() matches any active state; steps() counts per level", () => {
    let c = m.step(ctx0(), {});
    c = m.step(c, {});
    c = m.step(c, { move: true });
    expect(m.is(c, "alive")).toBe(true);
    expect(m.is(c, "ground")).toBe(true);
    expect(m.is(c, "idle")).toBe(false);
    expect(m.steps(c, "alive")).toBe(2);
    expect(m.steps(c)).toBe(0);
    expect(m.steps(c, "hurt")).toBe(-1);
    expect(inState(c.sm, "run")).toBe(true);
    expect(stateOf(c.sm)).toBe("run");
  });
});

describe("history and onDone", () => {
  interface E { next?: boolean; pause?: boolean }
  const build = (history: "shallow" | "deep") => machine<Ctx, E>()({
    id: "menu-" + history,
    initial: "menu",
    states: {
      menu: {
        initial: "main",
        history,
        always: { to: "paused", when: (_c, e) => !!e.pause },
        states: {
          main: { always: { to: "options", when: (_c, e) => !!e.next } },
          options: {
            initial: "video",
            states: {
              video: { always: { to: "sound", when: (_c, e) => !!e.next } },
              sound: { always: { to: "main", when: (_c, e) => !!e.next } },
            },
          },
        },
      },
      paused: { always: { to: "menu", when: (_c, e) => !e.pause } },
    },
  });
  test("shallow history resumes the substate, entering it at its initial", () => {
    const m = build("shallow");
    let c = m.step(ctx0(), {});
    c = m.step(c, { next: true }); // options.video
    c = m.step(c, { next: true }); // options.sound
    c = m.step(c, { pause: true });
    expect(m.state(c)).toBe("paused");
    c = m.step(c, {});
    expect(m.path(c)).toEqual(["menu", "options", "video"]);
  });
  test("deep history resumes the leaf", () => {
    const m = build("deep");
    let c = m.step(ctx0(), {});
    c = m.step(c, { next: true });
    c = m.step(c, { next: true });
    c = m.step(c, { pause: true });
    c = m.step(c, {});
    expect(m.path(c)).toEqual(["menu", "options", "sound"]);
    expect(JSON.parse(JSON.stringify(c.sm)).hist).toEqual({ menu: "options", options: "sound" });
  });
  test("onDone fires when the active substate is final", () => {
    const m = machine<Ctx>()({
      id: "level",
      initial: "level",
      states: {
        level: { initial: "play", onDone: "won", states: { play: { after: { 2: "exit" } }, exit: { type: "final" } } },
        won: { type: "final" },
      },
    });
    let c = run(m, ctx0(), 3);
    expect(m.path(c)).toEqual(["level", "exit"]);
    c = m.step(c, {});
    expect(m.state(c)).toBe("won");
  });
});

describe("events", () => {
  test("keyEdges turns two frames' keys into keydown and keyup events", () => {
    expect(keyEdges(["ArrowLeft"], ["ArrowLeft", "ArrowUp"])).toEqual(["keydown:ArrowUp"]);
    expect(keyEdges(["ArrowUp"], [])).toEqual(["keyup:ArrowUp"]);
    expect(keyEdges(undefined, undefined)).toEqual([]);
  });
  test("on fires only on a step given its event, before always", () => {
    const m = machine<Ctx>()({
      id: "jumper",
      initial: "ground",
      states: {
        ground: { on: { "keydown:ArrowUp": "jump" }, always: { to: "duck", when: (c) => c.x > 0 } },
        jump: { after: { 3: "ground" } },
        duck: { always: { to: "ground", when: (c) => c.x <= 0 } },
      },
    });
    let c = m.step(ctx0(1), {});
    c = m.step(c, {}, ["keydown:ArrowUp"]);
    expect(m.state(c)).toBe("jump");
    c = run(m, ctx0(0), 2);
    c = m.step(c, {}, { events: ["keyup:ArrowUp"] });
    expect(m.state(c)).toBe("ground");
  });
});

describe("deciders and the trace", () => {
  interface W { ready: boolean }
  const m = machine<Ctx, W>()({
    id: "agent",
    initial: "plan",
    states: {
      plan: {
        always: { to: "stuck", after: 50 },
        choose: [
          { to: "build", when: (_c, w) => w.ready, description: "start building" },
          { to: "research", description: "look something up first" },
        ],
      },
      research: { after: { 1: "plan" } },
      build: { type: "final" },
      stuck: { type: "final" },
    },
  });
  test("with no decider the first candidate whose guard holds fires", () => {
    let c = m.step(ctx0(), { ready: false });
    c = m.step(c, { ready: false });
    expect(m.state(c)).toBe("research");
    c = m.step(c, { ready: true });
    c = m.step(c, { ready: true });
    expect(m.state(c)).toBe("build");
  });
  test("a decider sees the candidates and picks, waits, or is refused", () => {
    const seen: number[][] = [];
    const wait: Decider = { name: "wait", decide: (p) => { seen.push(p.candidates.map((x) => x.edge)); return { edge: null }; } };
    let c = m.step(ctx0(), { ready: true });
    c = m.step(c, { ready: true }, { decide: wait });
    expect(m.state(c)).toBe("plan");
    expect(seen[0].length).toBe(2);
    const research = m.edges.find((x) => x.to === "research")!.index;
    const pick: Decider = { name: "bot", decide: () => ({ edge: research, evidence: { p: 0.7 } }) };
    const rows: TransitionRecord[] = [];
    c = m.step(c, { ready: true }, { decide: pick, trace: (r) => rows.push(r) });
    expect(m.state(c)).toBe("research");
    expect(rows).toEqual([{ machine: "agent", step: 2, from: "plan", to: "research", edge: research, kind: "choose", by: "bot", evidence: { p: 0.7 } }]);
    expect(c.sm.by).toBe("bot");
    const stuck = m.edges.find((x) => x.to === "stuck")!.index;
    expect(() => m.step(m.step(c, { ready: true }), { ready: true }, { decide: { name: "bad", decide: () => ({ edge: stuck }) } })).toThrow(/isn't a candidate/);
  });
  test("a replayed log of decisions gives the same run, and transitionOf recovers the trace from snapshots", () => {
    const research = m.edges.find((x) => x.to === "research")!.index;
    const build = m.edges.find((x) => x.to === "build")!.index;
    const log = { agent: { 3: research, 6: { edge: build, evidence: "clef p=0.81" } } };
    const live: TransitionRecord[] = [];
    const fromSnaps: TransitionRecord[] = [];
    let c = ctx0();
    let prev: Snapshot | undefined;
    for (let i = 0; i < 10; i++) {
      c = m.step(c, { ready: true }, { decide: replayDecider(log, "clef"), trace: (r) => live.push(r) });
      const t = transitionOf(m.watch(c).machine, prev, c.sm);
      if (t) fromSnaps.push(t);
      prev = c.sm;
    }
    expect(m.state(c)).toBe("build");
    expect(live.map((r) => `${r.step}:${r.from}->${r.to}:${r.by}`)).toEqual(["0:null->plan:start", "3:plan->research:clef", "4:research->plan:guard", "6:plan->build:clef"]);
    expect(fromSnaps.map(({ evidence: _e, ...r }) => r)).toEqual(live.map(({ evidence: _e, ...r }) => r));
    expect(firstDecider.name).toBe("first");
  });
});

describe("snapshots are plain JSON, so a fold resumes anywhere", () => {
  interface E { k: number }
  const m = machine<Ctx, E>()({
    id: "loop",
    initial: "a",
    states: {
      a: { initial: "a1", history: "shallow", tick: (c) => ({ ...c, x: c.x + 1 }), always: { to: "b", when: (c, e) => (c.x + e.k) % 7 === 0 },
        states: { a1: { after: { 4: "a2" } }, a2: { after: { 3: "a1" } } } },
      b: { after: { 5: "a" }, tick: (c) => ({ ...c, x: c.x + 2 }) },
    },
  });
  const envAt = (f: number): E => ({ k: (f * 13) % 5 });
  test("stepping on from a JSON copy of any step's context gives the same states as stepping straight through", () => {
    const straight: Ctx[] = [ctx0()];
    for (let f = 0; f < 200; f++) straight.push(m.step(straight[f], envAt(f)));
    for (const cut of [1, 17, 64, 133]) {
      let c: Ctx = JSON.parse(JSON.stringify(straight[cut]));
      for (let f = cut; f < 200; f++) {
        c = m.step(c, envAt(f));
        expect(c).toEqual(straight[f + 1]);
      }
    }
  });
});

describe("per-entity machines", () => {
  test("stepAll steps one machine per entity, each with its own snapshot", () => {
    const walker = machine<Ctx, { frame: number }>()({
      id: "walker",
      initial: "right",
      states: {
        right: { tick: (c) => ({ ...c, x: c.x + 1 }), always: { to: "left", when: (c) => c.x >= 3 } },
        left: { tick: (c) => ({ ...c, x: c.x - 1 }), always: { to: "right", when: (c) => c.x <= 0 } },
      },
    });
    let ws = [ctx0(0), ctx0(2)];
    for (let f = 0; f < 3; f++) ws = stepAll(walker, ws, { frame: f });
    expect(ws.map((w) => walker.state(w))).toEqual(["right", "left"]);
    expect(ws.map((w) => w.x)).toEqual([3, 1]);
  });
});

describe("validation and analysis", () => {
  const bad = (f: () => unknown, re: RegExp) => expect(f).toThrow(re);
  test("structural mistakes throw when the machine is built", () => {
    bad(() => machine<Ctx>()({ id: "m", initial: "a", states: { a: { always: "nope" as any } } }), /nope, which isn't a state/);
    bad(() => machine<Ctx>()({ id: "m", initial: "x" as any, states: { a: { type: "final" } } }), /isn't a top-level state/);
    bad(() => machine<Ctx>()({ id: "m", initial: "p", states: { p: { initial: "q" as any, states: { c: { type: "final" } } }, q: { type: "final" } } }), /needs initial/);
    bad(() => machine<Ctx>()({ id: "m", initial: "a", states: { a: { states: { a: {} } as any, initial: "a" } } }), /two states named a/);
    bad(() => machine<Ctx>()({ id: "m", initial: "a", states: { a: { type: "final", always: "a" } } }), /final state a/);
    bad(() => machine<Ctx>()({ id: "", initial: "a", states: { a: { type: "final" } } }), /give it an id/);
  });
  test("unreachable states and dead ends throw unless strict is off, and analyze() reports them", () => {
    bad(() => machine<Ctx>()({ id: "m", initial: "a", states: { a: { always: "b" }, b: { always: "a" }, island: { always: "a" } } }), /no transition reaches island/);
    bad(() => machine<Ctx>()({ id: "m", initial: "a", states: { a: { always: "b" }, b: {} } }), /b can't be left/);
    const loose = machine<Ctx>()({ id: "m", strict: false, initial: "a", states: { a: { always: "b" }, b: {}, island: { always: "a" } } });
    const a = loose.analyze();
    expect(a.unreachable).toEqual(["island"]);
    expect(a.deadEnds).toEqual(["b"]);
    expect(a.reachable).toEqual(["a", "b"]);
    expect(a.edges.map((e) => `${e.from}->${e.to}`)).toEqual(["a->b", "island->a"]);
    expect(JSON.parse(JSON.stringify(a))).toEqual(a);
  });
  test("XState's spellings work: target and guard", () => {
    const m = machine<Ctx>()({ id: "x", initial: "a", states: { a: { always: { target: "b", guard: (c) => c.x > 0 } }, b: { after: { 1: { target: "a" } } } } });
    expect(m.state(m.step(m.step(ctx0(1), {}), {}))).toBe("b");
  });
});

describe("inside a game's fold: the same input gives the same states, frame by frame", () => {
  interface G { x: number; sm: Snapshot; log: string[] }
  type In = { keys: string[] };
  const mover = machine<G, In>()({
    id: "mover",
    initial: "idle",
    states: {
      idle: { always: { to: "run", when: (_g, i) => i.keys.includes("ArrowRight") } },
      run: { tick: (g) => ({ ...g, x: g.x + 3 }), always: [{ to: "rest", when: (g) => g.x % 30 === 0 }, { to: "idle", when: (_g, i) => !i.keys.length }] },
      rest: { after: { 10: "idle" } },
    },
  });
  const step = (g: G, i: In) => mover.step(g, i);
  const initial: G = { x: 0, sm: mover.start(), log: [] };
  const input: Record<number, In> = {};
  for (let f = 0; f < 900; f++) if (((f * 7919) >> 3) % 3) input[f] = { keys: ["ArrowRight"] };
  let got: G;
  function Ask() { got = useGameState(initial, step); return null; }
  const at = (frame: number) => { renderFrame(<Ask />, { fps: 30, width: 100, height: 100, durationInFrames: 900, frame, input }); return got; };
  test("frames rendered in order, out of order and again (refolded from checkpoints) equal a plain loop", () => {
    const loop: G[] = [initial];
    for (let f = 0; f < 899; f++) loop.push(step(loop[f], input[f] ?? { keys: [] }));
    for (let f = 0; f < 899; f += 1) expect(at(f)).toEqual(loop[f]);
    // the fold kept the last 512 states whole and every 64th before: early frames are refolded from checkpoints
    for (const f of [5, 63, 64, 65, 200, 333, 640, 898, 1]) expect(at(f)).toEqual(loop[f]);
    expect(new Set(loop.map((g) => mover.state(g)))).toEqual(new Set(["idle", "run", "rest"]));
  });
});
