// Step-driven state machines: the one core under game logic, playtests and (later) agent workflows (spec: "State
// machines, step by step"; sdk/react/docs/state-machines.md). Nothing here knows about games: games.ts adds per-entity
// stepping and input edges on top, and a game steps its machines once a frame.
//
// The design follows Bevy's seldom_state: each entity carries its own machine, and a transition is a trigger, a pure
// predicate polled every step, not an event someone sends. The definition is plain data in XState's shape (states /
// initial / entry / exit / after / on, `target` and `guard` accepted beside `to` and `when`), so what an agent knows of
// XState carries over; what differs is that guards are polled each step and delays are counted in steps.
//
// A transition is either guard-driven (the first candidate whose guard holds fires: the game case) or, at a choice point
// (a state's `choose` list), picked by a Decider the step is given: code, a replayed log, a search bot, a person, or a
// model whose answer was taken outside the step. Every transition can be reported as one row (TransitionRecord: step,
// machine, from, to, edge, decider, evidence) to a sink the step is given, so one trace records all of them.
//
// The machine's runtime state is a Snapshot, plain JSON kept inside the context it drives (`player.sm`): a fold stays
// pure, checkpoints and renders resume from any step, and a playtest reads every machine's state from the game state.
// Nothing here reads a clock or a random source: the same inputs and decisions give the same states step by step.

// ---- the definition ------------------------------------------------------------------------------------------------

/** A trigger: polled each step while its state is active (the context, and whatever the step passes as env). */
export type Guard<C, E> = (ctx: C, env: E) => boolean;
/** An action: a pure function returning the new context (never a changed old one). */
export type Action<C, E> = (ctx: C, env: E) => C;
export type Actions<C, E> = Action<C, E> | readonly Action<C, E>[];

type Target<N extends string> = { to: NoInfer<N>; target?: undefined } | { target: NoInfer<N>; to?: undefined };
type When<C, E> = { when?: Guard<C, E>; guard?: undefined } | { guard?: Guard<C, E>; when?: undefined };
/** A transition: where to go (`to`, or XState's `target`), when (`when`, or XState's `guard`; none: always), after how
 *  many steps in the state at least (`after`), and what to do on the way (`actions`). */
export type TransitionConfig<C, E, N extends string> = Target<N> & When<C, E> & {
  /** steps in the state before this transition may fire (0) */
  after?: number;
  actions?: Actions<C, E>;
  /** what the transition means: a decider reads it (a model's option description), reports show it */
  description?: string;
};
/** A target's name alone, one transition, or several checked in order. */
export type Transitions<C, E, N extends string> =
  | NoInfer<N>
  | TransitionConfig<C, E, N>
  | readonly (NoInfer<N> | TransitionConfig<C, E, N>)[];

export interface StateConfig<C, E, N extends string> {
  /** run when the state is entered (outer states before inner ones) */
  entry?: Actions<C, E>;
  /** run when the state is left (inner states before outer ones) */
  exit?: Actions<C, E>;
  /** run every step the state is active, after this step's transition (outer before inner, the entry step too) */
  tick?: Actions<C, E>;
  /** triggers polled every step, in order: the first whose guard holds fires (XState's eventless `always`) */
  always?: Transitions<C, E, N>;
  /** delays in steps: `after: { 30: "fall" }` fires once the state has been active 30 steps (and its guard holds) */
  after?: { readonly [steps: number]: Transitions<C, E, N> };
  /** named events the step is given this time (input edges: games.ts keyEdges gives "keydown:ArrowUp") */
  on?: { readonly [event: string]: Transitions<C, E, N> };
  /** a choice point: of these candidates, those whose guards hold go to the step's Decider, which picks one or waits
   *  (no decider: firstDecider takes the first). Polled after `always` and `after`. */
  choose?: Transitions<C, E, N>;
  /** a compound state's first substate */
  initial?: NoInfer<N>;
  /** substates: names are unique across the whole machine, so `to: "run"` names one state wherever it is */
  states?: { [K in N]?: StateConfig<C, E, N> };
  /** re-entering this compound state resumes the substate it left ("shallow"), or that substate's own (deep) */
  history?: "shallow" | "deep";
  /** a compound state's transition taken when its active substate is final */
  onDone?: Transitions<C, E, N>;
  /** a final state: no transitions out (a dead enemy, the game won, the work shipped) */
  type?: "final";
  description?: string;
}

export interface MachineConfig<C, E, N extends string, K extends string> {
  /** the machine's name in traces and reports */
  id: string;
  initial: NoInfer<N>;
  states: { [S in N]?: StateConfig<C, E, N> };
  /** the context's field holding the snapshot ("sm") */
  key?: K;
  /** refuse unreachable states and non-final dead ends when the machine is built (true) */
  strict?: boolean;
}

// ---- the snapshot: the machine's runtime state, plain JSON ----------------------------------------------------------

/** The machine's runtime state, kept in the context it drives. `path` is the active states outermost first ([] before
 *  the first step); `at[i]` is the step on which path[i] was entered; `n` counts the machine's steps from 0; `e` is the
 *  edge the last transition took (an index into machine.edges); `by` names the decider that chose it (absent: a
 *  guard); `hist` is each history state's last substate. */
export interface Snapshot<N extends string = string> {
  path: N[];
  at: number[];
  n: number;
  e: number;
  by?: string;
  hist?: { [S in N]?: N };
}

/** A machine's state in some context, for a trace (`Playtest.machines`). */
export interface Watched {
  machine: AnyMachine;
  snapshot: Snapshot;
}

/** The active leaf of a snapshot (undefined before the first step). */
export function stateOf<N extends string>(snap: Snapshot<N>): N | undefined {
  return snap.path[snap.path.length - 1];
}
/** Whether `name` is active in the snapshot (the leaf or any state around it), as XState's `state.matches`. */
export function inState<N extends string>(snap: Snapshot<N>, name: N): boolean {
  return snap.path.indexOf(name) >= 0;
}

// ---- deciders and the trace ----------------------------------------------------------------------------------------

/** A choice point as a decider sees it: which machine, where, and the candidates whose guards hold. */
export interface ChoicePoint<C = unknown, E = unknown> {
  machine: string;
  /** the state whose `choose` list this is, and the active leaf */
  state: string;
  leaf: string;
  /** the machine's step */
  n: number;
  /** the candidates whose guards hold, in their order (each an edge of machine.edges) */
  candidates: { edge: number; to: string; description?: string }[];
  ctx: C;
  env: E;
}
/** What a decider answers: the edge it picks (one of the candidates' `edge`), or null to wait this step; `evidence` is
 *  whatever supports the choice (a bot's score, a model's probabilities), recorded beside the transition. */
export interface Decision {
  edge: number | null;
  evidence?: unknown;
}
/** Picks a transition at a choice point. Called inside the step, so it must answer synchronously: a model or a person
 *  is asked outside the step and their answer replayed (`replayDecider`). Deterministic deciders keep a fold pure. */
export interface Decider {
  name: string;
  decide(point: ChoicePoint): Decision;
}
/** The default decider: the first candidate. */
export const firstDecider: Decider = { name: "first", decide: (p) => ({ edge: p.candidates[0].edge }) };
/** A decider that replays recorded decisions: `log[machine][n]` is the edge chosen at that machine's step n (missing:
 *  wait). How a model's or a person's choices, taken outside the step, re-enter it exactly. */
export function replayDecider(log: Record<string, Record<number, number | { edge: number; evidence?: unknown }>>, name = "replay"): Decider {
  return {
    name,
    decide: (p) => {
      const got = log[p.machine]?.[p.n];
      if (got === undefined) return { edge: null };
      return typeof got === "number" ? { edge: got } : got;
    },
  };
}

/** One transition as a row: what a trace keeps and rdb loads (step, machine, from, to, edge, decider, evidence). `from` is
 *  null for the first step's entry into the initial states. */
export interface TransitionRecord {
  machine: string;
  step: number;
  from: string | null;
  to: string;
  edge: number;
  kind: Edge["kind"] | "start";
  /** "guard" for a trigger, "start" for the first entry, else the decider's name */
  by: string;
  evidence?: unknown;
}

/** What a step may be given besides the context and env. */
export interface StepOptions {
  /** named events this step (an `on` edge waits for one) */
  events?: readonly string[];
  /** picks at choice points (default firstDecider) */
  decide?: Decider;
  /** receives every transition this step takes, as a row */
  trace?: (r: TransitionRecord) => void;
}

// ---- the compiled machine ------------------------------------------------------------------------------------------

export interface Edge<N extends string = string> {
  index: number;
  from: N;
  to: N;
  kind: "always" | "after" | "on" | "done" | "choose";
  /** the event an `on` edge waits for */
  event?: string;
  /** steps in `from` before it may fire */
  after: number;
  guarded: boolean;
  description?: string;
}

interface CEdge<C, E> extends Edge {
  when?: Guard<C, E>;
  actions: readonly Action<C, E>[];
}

interface Node<C, E> {
  name: string;
  parent: string | null;
  depth: number;
  /** the names from the top-level state down to this one */
  line: string[];
  children: string[];
  initial?: string;
  history?: "shallow" | "deep";
  final: boolean;
  entry: readonly Action<C, E>[];
  exit: readonly Action<C, E>[];
  tick: readonly Action<C, E>[];
  edges: CEdge<C, E>[];
  /** whether `on` / `choose` edges exist here (most states have none: skip those scans) */
  hasOn: boolean;
  hasChoose: boolean;
}

export interface Analysis<N extends string = string> {
  id: string;
  states: { name: N; parent: N | null; depth: number; kind: "leaf" | "compound" | "final"; initial: boolean }[];
  edges: Edge<N>[];
  /** states some sequence of transitions reaches from the initial state (guards assumed satisfiable) */
  reachable: N[];
  unreachable: N[];
  /** non-final leaves with no transition out (their own or an enclosing state's): once entered, never left */
  deadEnds: N[];
}

export interface Machine<C, E, N extends string, K extends string = "sm"> {
  readonly id: string;
  readonly key: K;
  /** every state, in definition order (outer before inner) */
  readonly states: readonly N[];
  /** every transition, in the order they're checked within a state */
  readonly edges: readonly Edge<N>[];
  /** a snapshot before the first step: the first step enters the initial states (running their entry actions) */
  start(): Snapshot<N>;
  /** One step: poll the active states' transitions (innermost first, then outward; within a state `on` for this step's
   *  events, then `always`, `after`, `onDone`, then the `choose` candidates for the decider), take the first that fires
   *  (at most one a step), then run the active states' tick actions. Returns the new context with its snapshot; the
   *  old one is unchanged. The third argument is this step's events, or StepOptions. */
  step(ctx: C, env: E, opts?: readonly string[] | StepOptions): C;
  /** the active leaf (the initial leaf before the first step) */
  state(ctx: C): N;
  /** the active states, outermost first */
  path(ctx: C): N[];
  /** whether `name` is active (the leaf or a state around it) */
  is(ctx: C, name: N): boolean;
  /** steps since `name` (the leaf by default) was entered; -1 when it isn't active */
  steps(ctx: C, name?: N): number;
  /** whether the step from `before` to `after` entered `name` (any state when omitted) */
  entered(before: C, after: C, name?: N): boolean;
  /** this machine's state in `ctx`, for a trace */
  watch(ctx: C): Watched;
  /** the static analysis machine checks read */
  analyze(): Analysis<N>;
}
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyMachine = Machine<any, any, string, string>;
/** The state names of a machine. */
export type StateName<M> = M extends Machine<any, any, infer N, any> ? N : never;

function list<T>(x: T | readonly T[] | undefined): readonly T[] {
  return x === undefined ? [] : Array.isArray(x) ? (x as readonly T[]) : [x as T];
}

/**
 * A machine for contexts of type C, stepped with an env of type E (a game's frame input and world, a workflow's facts).
 * Curried so the state names are inferred from the definition while C and E are given:
 * `machine<Player, Env>()({ id, initial, states })`. A transition to a state that doesn't exist, or a guard reading a
 * field C lacks, fails to compile; what types can't hold (an initial that isn't a substate, unreachable states, dead
 * ends) throws here, when the module loads.
 */
export function machine<C, E = unknown>() {
  return function define<const N extends string, const K extends string = "sm">(config: MachineConfig<C, E, N, K>): Machine<C, E, N, K> {
    return build<C, E, N, K>(config);
  };
}

function build<C, E, N extends string, K extends string>(config: MachineConfig<C, E, N, K>): Machine<C, E, N, K> {
  const id = config.id;
  const where = `machine ${id}`;
  if (typeof id !== "string" || !id) throw new Error("machine: give it an id (its name in traces and reports)");
  const key = (config.key ?? "sm") as K;
  const nodes = new Map<string, Node<C, E>>();
  const order: string[] = [];
  const pending: { from: string; kind: Edge["kind"]; event?: string; after: number; t: unknown }[] = [];

  const fns = (x: unknown, what: string): readonly Action<C, E>[] => {
    const l = list(x as Actions<C, E> | undefined);
    for (const f of l) if (typeof f !== "function") throw new Error(`${where}: ${what} must be functions (ctx, env) => ctx`);
    return l;
  };
  const walk = (states: Record<string, StateConfig<C, E, N> | undefined>, parent: Node<C, E> | null) => {
    for (const name of Object.keys(states)) {
      const s = states[name];
      if (!s) continue;
      if (nodes.has(name)) throw new Error(`${where}: two states named ${name} (names are unique across the machine)`);
      const children = s.states ? Object.keys(s.states).filter((k) => (s.states as Record<string, unknown>)[k]) : [];
      const node: Node<C, E> = {
        name, parent: parent ? parent.name : null, depth: parent ? parent.depth + 1 : 0,
        line: parent ? [...parent.line, name] : [name], children,
        initial: s.initial, history: s.history, final: s.type === "final",
        entry: fns(s.entry, `${name}'s entry actions`), exit: fns(s.exit, `${name}'s exit actions`), tick: fns(s.tick, `${name}'s tick actions`),
        edges: [], hasOn: false, hasChoose: false,
      };
      nodes.set(name, node);
      order.push(name);
      if (node.final && (children.length || s.always || s.after || s.on || s.onDone || s.choose || s.tick)) {
        throw new Error(`${where}: final state ${name} can't have substates, transitions or tick actions`);
      }
      if (children.length && (s.initial === undefined || children.indexOf(s.initial) < 0)) {
        throw new Error(`${where}: compound state ${name} needs initial set to one of its substates (${children.join(", ")})`);
      }
      if (!children.length && s.initial !== undefined) throw new Error(`${where}: ${name} has an initial but no substates`);
      if (s.history && !children.length) throw new Error(`${where}: history on ${name}, which has no substates`);
      if (s.onDone && !children.length) throw new Error(`${where}: onDone on ${name}, which has no substates`);
      if (s.on) for (const ev of Object.keys(s.on)) for (const t of list(s.on[ev] as unknown)) pending.push({ from: name, kind: "on", event: ev, after: 0, t });
      for (const t of list(s.always as unknown)) pending.push({ from: name, kind: "always", after: 0, t });
      if (s.after) {
        const ks = Object.keys(s.after).map(Number).sort((a, b) => a - b);
        for (const k of ks) {
          if (!Number.isInteger(k) || k < 0) throw new Error(`${where}: ${name}'s after key ${k} isn't a whole number of steps`);
          for (const t of list((s.after as Record<number, unknown>)[k])) pending.push({ from: name, kind: "after", after: k, t });
        }
      }
      for (const t of list(s.onDone as unknown)) pending.push({ from: name, kind: "done", after: 0, t });
      for (const t of list(s.choose as unknown)) pending.push({ from: name, kind: "choose", after: 0, t });
      if (children.length) walk(s.states as Record<string, StateConfig<C, E, N>>, node);
    }
  };
  if (!config.states || typeof config.states !== "object") throw new Error(`${where}: no states`);
  walk(config.states as Record<string, StateConfig<C, E, N>>, null);
  const root = config.initial;
  if (!nodes.has(root) || nodes.get(root)!.parent !== null) throw new Error(`${where}: initial ${String(root)} isn't a top-level state`);

  const edges: CEdge<C, E>[] = [];
  for (const p of pending) {
    const t = typeof p.t === "string" ? { to: p.t } : (p.t as { to?: string; target?: string; when?: Guard<C, E>; guard?: Guard<C, E>; after?: number; actions?: Actions<C, E>; description?: string });
    const to = t.to ?? t.target;
    if (to === undefined) throw new Error(`${where}: a transition from ${p.from} has no target (to)`);
    if (!nodes.has(to)) throw new Error(`${where}: ${p.from} goes to ${to}, which isn't a state (${order.join(", ")})`);
    const when = t.when ?? t.guard;
    if (when !== undefined && typeof when !== "function") throw new Error(`${where}: a guard from ${p.from} isn't a function`);
    const after = Math.max(p.after, t.after ?? 0);
    if (!Number.isInteger(after) || after < 0) throw new Error(`${where}: a transition from ${p.from} waits ${after} steps (a whole number)`);
    const e: CEdge<C, E> = {
      index: edges.length, from: p.from, to, kind: p.kind, after, guarded: when !== undefined, when,
      actions: fns(t.actions, `actions from ${p.from}`),
    };
    if (p.event !== undefined) e.event = p.event;
    if (t.description !== undefined) e.description = t.description;
    edges.push(e);
    const n = nodes.get(p.from)!;
    n.edges.push(e);
    if (p.kind === "on") n.hasOn = true;
    if (p.kind === "choose") n.hasChoose = true;
  }

  // history is recorded only where some re-entry reads it: a history state, and every compound inside a deep one
  const keepsHistory = new Set<string>();
  for (const n of nodes.values()) {
    if (!n.history) continue;
    keepsHistory.add(n.name);
    if (n.history === "deep") {
      const stack = [...n.children];
      while (stack.length) {
        const c = nodes.get(stack.pop()!)!;
        if (c.children.length) { keepsHistory.add(c.name); stack.push(...c.children); }
      }
    }
  }

  // ---- the static analysis ---------------------------------------------------------------------------------------
  const descend = (name: string): string[] => {
    const out = [...nodes.get(name)!.line];
    let n = nodes.get(name)!;
    while (n.children.length) { n = nodes.get(n.initial!)!; out.push(n.name); }
    return out;
  };
  const analysis = (): Analysis<N> => {
    // entering a state makes its enclosing states and its initial substates active; a history state only resumes
    // substates that were reached some other way first, so it adds none
    const reached = new Set<string>(descend(root));
    const queue = [...reached];
    while (queue.length) {
      const s = queue.shift()!;
      for (const e of nodes.get(s)!.edges) {
        for (const r of descend(e.to)) if (!reached.has(r)) { reached.add(r); queue.push(r); }
      }
    }
    const deadEnds = order.filter((s) => {
      const n = nodes.get(s)!;
      if (n.final || n.children.length) return false;
      for (let m: Node<C, E> | undefined = n; m; m = m.parent ? nodes.get(m.parent) : undefined) {
        if (m.edges.some((e) => e.kind !== "done")) return false;
      }
      return true;
    });
    return {
      id,
      states: order.map((s) => {
        const n = nodes.get(s)!;
        return { name: s as N, parent: n.parent as N | null, depth: n.depth, kind: n.final ? "final" : n.children.length ? "compound" : "leaf", initial: n.parent === null ? s === root : nodes.get(n.parent)!.initial === s };
      }),
      edges: edges.map(({ when: _w, actions: _a, ...e }) => e as Edge<N>),
      reachable: order.filter((s) => reached.has(s)) as N[],
      unreachable: order.filter((s) => !reached.has(s)) as N[],
      deadEnds: deadEnds as N[],
    };
  };
  const a = analysis();
  if (config.strict !== false) {
    if (a.unreachable.length) throw new Error(`${where}: no transition reaches ${a.unreachable.join(", ")} (remove them, add a way in, or build with strict: false)`);
    if (a.deadEnds.length) throw new Error(`${where}: ${a.deadEnds.join(", ")} can't be left once entered: give each a transition out, or mark it type: "final"`);
  }

  // ---- the runtime -----------------------------------------------------------------------------------------------
  const initialLeaf = descend(root);
  const START: Snapshot = { path: [], at: [], n: 0, e: -1 };
  const snapOf = (ctx: C): Snapshot<N> => ((ctx as Record<string, unknown>)?.[key] as Snapshot<N> | undefined) ?? (START as Snapshot<N>);

  const run = (fs: readonly Action<C, E>[], ctx: C, env: E): C => {
    for (let i = 0; i < fs.length; i++) ctx = fs[i](ctx, env);
    return ctx;
  };

  /** Enter `line` below the kept depth k, then the initial (or remembered) substates. */
  const enter = (ctx: C, env: E, path: string[], at: number[], k: number, line: string[], n: number, hist: Record<string, string> | undefined): C => {
    for (let d = k; d < line.length; d++) {
      const node = nodes.get(line[d])!;
      path.push(node.name);
      at.push(n);
      ctx = run(node.entry, ctx, env);
    }
    let deep = false;
    for (let d = 0; d < path.length; d++) if (nodes.get(path[d])!.history === "deep") deep = true;
    let node = nodes.get(path[path.length - 1])!;
    while (node.children.length) {
      const remembered = hist && (node.history || deep) ? hist[node.name] : undefined;
      if (node.history === "deep") deep = true;
      node = nodes.get(remembered ?? node.initial!)!;
      path.push(node.name);
      at.push(n);
      ctx = run(node.entry, ctx, env);
    }
    return ctx;
  };

  const enabled = (x: CEdge<C, E>, inSteps: number, ctx: C, env: E) => inSteps >= x.after && (!x.when || x.when(ctx, env));

  const step = (ctx: C, env: E, o?: readonly string[] | StepOptions): C => {
    const opts: StepOptions | undefined = o === undefined ? undefined : Array.isArray(o) ? { events: o as readonly string[] } : (o as StepOptions);
    const events = opts?.events;
    const snap = snapOf(ctx);
    if (snap.path.length === 0) {
      const path: string[] = [], at: number[] = [];
      ctx = enter(ctx, env, path, at, 0, nodes.get(root)!.line, 0, undefined);
      for (let d = 0; d < path.length; d++) ctx = run(nodes.get(path[d])!.tick, ctx, env);
      opts?.trace?.({ machine: id, step: 0, from: null, to: path[path.length - 1], edge: -1, kind: "start", by: "start" });
      const out: Snapshot<N> = { path: path as N[], at, n: 0, e: -1 };
      return { ...ctx, [key]: out } as C;
    }
    const n = snap.n + 1;
    let path = snap.path as string[], at = snap.at, e = snap.e, hist = snap.hist as Record<string, string> | undefined;
    // the first transition that fires, innermost state first
    let fired: CEdge<C, E> | undefined, from = -1, by: string | undefined, evidence: unknown;
    for (let d = path.length - 1; d >= 0 && !fired; d--) {
      const node = nodes.get(path[d])!;
      const inSteps = n - at[d];
      const es = node.edges;
      if (node.hasOn && events && events.length) {
        for (let i = 0; i < es.length; i++) {
          const x = es[i];
          if (x.kind === "on" && events.indexOf(x.event!) >= 0 && enabled(x, inSteps, ctx, env)) { fired = x; break; }
        }
      }
      if (!fired) {
        for (let i = 0; i < es.length; i++) {
          const x = es[i];
          if (x.kind === "on" || x.kind === "choose") continue;
          if (x.kind === "done") {
            const child = path[d + 1];
            if (child === undefined || !nodes.get(child)!.final || d + 2 !== path.length) continue;
          }
          if (enabled(x, inSteps, ctx, env)) { fired = x; break; }
        }
      }
      if (!fired && node.hasChoose) {
        const cands: CEdge<C, E>[] = [];
        for (let i = 0; i < es.length; i++) if (es[i].kind === "choose" && enabled(es[i], inSteps, ctx, env)) cands.push(es[i]);
        if (cands.length) {
          const decider = opts?.decide ?? firstDecider;
          const got = decider.decide({
            machine: id, state: node.name, leaf: path[path.length - 1], n, ctx, env,
            candidates: cands.map((x) => (x.description === undefined ? { edge: x.index, to: x.to } : { edge: x.index, to: x.to, description: x.description })),
          });
          if (got.edge !== null) {
            const pick = cands.find((x) => x.index === got.edge);
            if (!pick) throw new Error(`${where}: decider ${decider.name} chose edge ${got.edge}, which isn't a candidate at ${node.name} (${cands.map((x) => x.index).join(", ")})`);
            fired = pick;
            by = decider.name;
            evidence = got.evidence;
          }
        }
      }
      if (fired) from = d;
    }
    if (fired) {
      const target = nodes.get(fired.to)!;
      const source = path[from];
      // the states kept: up to the source when the target is inside it, else up to the common ancestor (a target that
      // is the source or encloses it is left and entered again)
      let k: number;
      if (target.depth > from && target.line[from] === source) k = from + 1;
      else {
        let common = 0;
        while (common < target.line.length && common <= from && target.line[common] === path[common]) common++;
        k = Math.min(common, target.depth);
      }
      const leaf = path[path.length - 1];
      for (let d = path.length - 1; d >= k; d--) {
        const node = nodes.get(path[d])!;
        ctx = run(node.exit, ctx, env);
        if (node.parent && keepsHistory.has(node.parent)) hist = { ...(hist ?? {}), [node.parent]: node.name };
      }
      ctx = run(fired.actions, ctx, env);
      const np = path.slice(0, k), na = at.slice(0, k);
      ctx = enter(ctx, env, np, na, k, target.line, n, hist);
      path = np;
      at = na;
      e = fired.index;
      if (opts?.trace) {
        const r: TransitionRecord = { machine: id, step: n, from: leaf, to: path[path.length - 1], edge: e, kind: fired.kind, by: by ?? "guard" };
        if (evidence !== undefined) r.evidence = evidence;
        opts.trace(r);
      }
    }
    for (let d = 0; d < path.length; d++) {
      const t = nodes.get(path[d])!.tick;
      if (t.length) ctx = run(t, ctx, env);
    }
    const out: Snapshot<N> = { path: path as N[], at, n, e };
    const keptBy = fired ? by : snap.by;
    if (keptBy !== undefined) out.by = keptBy;
    if (hist) out.hist = hist as Snapshot<N>["hist"];
    return { ...ctx, [key]: out } as C;
  };

  const self: Machine<C, E, N, K> = {
    id,
    key,
    states: order as N[],
    edges: a.edges,
    start: () => ({ path: [], at: [], n: 0, e: -1 }),
    step,
    state: (ctx) => {
      const p = snapOf(ctx).path;
      return (p.length ? p[p.length - 1] : initialLeaf[initialLeaf.length - 1]) as N;
    },
    path: (ctx) => {
      const p = snapOf(ctx).path;
      return (p.length ? [...p] : [...initialLeaf]) as N[];
    },
    is: (ctx, name) => {
      const p = snapOf(ctx).path;
      return (p.length ? p : initialLeaf).indexOf(name) >= 0;
    },
    steps: (ctx, name) => {
      const s = snapOf(ctx);
      if (!s.path.length) return name === undefined || initialLeaf.indexOf(name) >= 0 ? 0 : -1;
      const d = name === undefined ? s.path.length - 1 : s.path.indexOf(name);
      return d < 0 ? -1 : s.n - s.at[d];
    },
    entered: (before, after, name) => {
      const b0 = snapOf(before), b = snapOf(after);
      if (!b.path.length) return false;
      const stepped = b0.path.length === 0 || b.n !== b0.n;
      if (!stepped) return false;
      if (name === undefined) return b0.path.length === 0 || b.at[b.path.length - 1] === b.n;
      const d = b.path.indexOf(name);
      return d >= 0 && (b0.path.length === 0 || b.at[d] === b.n);
    },
    watch: (ctx) => ({ machine: self as unknown as AnyMachine, snapshot: snapOf(ctx) }),
    analyze: analysis,
  };
  return self;
}

/** The transition a step took, read from two snapshots of one machine (null when none fired): how a trace that only
 *  sees states (a playtest over a game's pure fold) recovers the TransitionRecord a sink would have had, minus evidence. */
export function transitionOf(m: AnyMachine, before: Snapshot | undefined, after: Snapshot): TransitionRecord | null {
  if (!after.path.length) return null;
  const b = before && before.path.length ? before : null;
  const to = after.path[after.path.length - 1];
  if (!b) return { machine: m.id, step: after.n, from: null, to, edge: -1, kind: "start", by: "start" };
  if (after.n === b.n || after.at[after.path.length - 1] !== after.n) return null;
  return { machine: m.id, step: after.n, from: b.path[b.path.length - 1], to, edge: after.e, kind: m.edges[after.e]?.kind ?? "always", by: after.by ?? "guard" };
}
