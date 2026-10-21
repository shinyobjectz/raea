// State machines in a playtest (sdk/react/docs/state-machines.md): every machine a run's states hold (Playtest.machines),
// read step by step from the snapshots alone, so nothing in the game changes to be traced. Machine-generic: it knows
// machines, states, edges, inputs and a progress number, never what the machine is for, so a game's player, a menu or an
// agent workflow is checked the same way.
//
// For each machine (by id, over all its entities) it keeps, across everything the bots play:
//   reached   how often each state was entered          -> state coverage, "reached state X"
//   fired     how often each edge fired                  -> transition coverage
//   left      how often each state was left              -> a state entered and never left
//   stays     in linear play (the run, random episodes, the play after an ending): the longest time in each state and
//             how often the input changed meanwhile     -> evidence that a never-left state is a softlock
//   loops     in the run: one edge firing over and over while progress stays flat (an agent's "looping without
//             progress"; a game's death loop)
// plus the static analysis (unreachable states, dead ends). Every transition of the run is also a row
// (TransitionRow), the shape rdb loads.
import { transitionOf, type Analysis, type AnyMachine, type Snapshot, type TransitionRecord, type Watched } from "../../../motion/src/machine";

/** One transition of a run, as a row: the frame, the entity, and the machine's own record. */
export type TransitionRow = { frame: number; entity: string } & TransitionRecord;

interface Stay { machine: string; state: string; since: number; changes: number; keys: string; progress0: number }
interface PerMachine {
  machine: AnyMachine;
  analysis: Analysis;
  entities: Set<string>;
  reached: Map<string, number>;
  /** entered while the game was still on (its outcome null): states entered only as it ends aren't softlock suspects */
  reachedLive: Map<string, number>;
  fired: Map<number, number>;
  left: Map<string, number>;
  /** the longest stay per state in linear play: frames, and input changes during it */
  stays: Map<string, { frames: number; changes: number; frame: number; ended: "left" | "play ended" }>;
}

export interface StepInfo { frame: number; keys: readonly string[]; progress: number; outcome: string | null }

export class MachineTrace {
  machines = new Map<string, PerMachine>();
  private open = new Map<string, Stay>();
  constructor(private watch: (state: any) => Record<string, Watched>) {}

  private of(w: Watched, entity: string): PerMachine {
    const id = w.machine.id;
    let m = this.machines.get(id);
    if (!m) {
      m = { machine: w.machine, analysis: w.machine.analyze(), entities: new Set(), reached: new Map(), reachedLive: new Map(), fired: new Map(), left: new Map(), stays: new Map() };
      this.machines.set(id, m);
    }
    m.entities.add(entity);
    return m;
  }

  /** The machines in a state, by entity: { entity: leaf state }. */
  states(s: any): Record<string, string> {
    const out: Record<string, string> = {};
    for (const [entity, w] of Object.entries(this.watch(s))) out[entity] = w.snapshot.path.length ? w.snapshot.path.join(".") : "(not started)";
    return out;
  }

  /** One step of the game, from `before` to `after`: counts what the machines did, and returns its transitions as rows.
   *  `linear` play (one timeline, not a search's branches) also measures stays. */
  observe(before: any, after: any, info: StepInfo, linear: boolean): TransitionRow[] {
    const was = before === undefined ? {} : this.watch(before);
    const now = this.watch(after);
    const rows: TransitionRow[] = [];
    const keys = linear ? info.keys.join("+") : "";
    for (const entity in now) {
      const w = now[entity];
      const m = this.of(w, entity);
      const b: Snapshot | undefined = was[entity]?.snapshot;
      const t = transitionOf(w.machine, b, w.snapshot);
      if (t) {
        const a = w.snapshot;
        // the states entered this step, and (for a transition) the states left
        for (let d = 0; d < a.path.length; d++) {
          if (b && b.path.length && a.at[d] !== a.n) continue;
          m.reached.set(a.path[d], (m.reached.get(a.path[d]) ?? 0) + 1);
          if (info.outcome === null) m.reachedLive.set(a.path[d], (m.reachedLive.get(a.path[d]) ?? 0) + 1);
        }
        if (b && b.path.length) {
          for (let d = 0; d < b.path.length; d++) {
            if (a.path[d] === b.path[d] && a.at[d] !== a.n) continue;
            m.left.set(b.path[d], (m.left.get(b.path[d]) ?? 0) + 1);
          }
          if (t.edge >= 0) m.fired.set(t.edge, (m.fired.get(t.edge) ?? 0) + 1);
        }
        rows.push({ frame: info.frame, entity, ...t });
      }
      if (!linear) continue;
      // stays: one open per entity, closed when it leaves its leaf
      const leaf = w.snapshot.path[w.snapshot.path.length - 1];
      if (leaf === undefined) continue;
      const key = `${m.machine.id}\u0000${entity}`;
      const open = this.open.get(key);
      if (open && (t || open.state !== leaf)) {
        this.close(open, info.frame, "left");
        this.open.delete(key);
      }
      const cur = this.open.get(key);
      if (!cur) this.open.set(key, { machine: m.machine.id, state: leaf, since: info.frame, changes: 0, keys, progress0: info.progress });
      else if (cur.keys !== keys) { cur.changes++; cur.keys = keys; }
    }
    return rows;
  }

  private close(s: Stay, frame: number, ended: "left" | "play ended") {
    const m = this.machines.get(s.machine);
    if (!m || s.state === undefined) return;
    const frames = frame - s.since;
    const had = m.stays.get(s.state);
    if (!had || s.changes > had.changes || (s.changes === had.changes && frames > had.frames)) m.stays.set(s.state, { frames, changes: s.changes, frame: s.since, ended });
  }

  /** A linear play ends (an episode, the run): its open stays close as "play ended". */
  endPlay(frame: number) {
    for (const s of this.open.values()) this.close(s, frame, "play ended");
    this.open.clear();
  }

  /** The report's `machines` section: per machine, coverage, softlocks, loops, the static analysis. */
  report(runRows: TransitionRow[], runProgress: (frame: number) => number) {
    const out: Record<string, unknown> = {};
    for (const [id, m] of this.machines) {
      const a = m.analysis;
      const name = (e: { from: string; to: string; kind: string; event?: string; after: number }) =>
        `${e.from}->${e.to} (${e.kind}${e.event ? " " + e.event : ""}${e.after ? " after " + e.after : ""})`;
      const finals = new Set(a.states.filter((s) => s.kind === "final").map((s) => s.name));
      const missing = a.states.map((s) => s.name).filter((s) => !m.reached.has(s));
      const unfired = a.edges.filter((e) => !m.fired.has(e.index));
      // softlocks: a non-final state entered while the game was on and never left anywhere in the bots' play; a
      // static dead end is one by construction. Confirmed when some linear play stayed in it while the input changed
      // (3 changes or more); unconfirmed otherwise (seen only at the end of a search branch, say).
      const softlocks: unknown[] = [];
      const unconfirmed: unknown[] = [];
      for (const s of a.states) {
        if (finals.has(s.name)) continue;
        const dead = a.deadEnds.includes(s.name);
        const neverLeft = (m.reachedLive.get(s.name) ?? 0) > 0 && !(m.left.get(s.name) ?? 0);
        if (!dead && !neverLeft) continue;
        const stay = m.stays.get(s.name);
        const entry = { state: s.name, why: dead ? "no transition out (static)" : "entered, never left", entered: m.reached.get(s.name) ?? 0, longestStay: stay ?? null };
        if (dead || (stay && stay.changes >= 3)) softlocks.push(entry);
        else unconfirmed.push(entry);
      }
      // loops in the run: an edge firing 12 times or more within a stretch where progress rose under 0.005
      const loops: unknown[] = [];
      const byEntity = new Map<string, TransitionRow[]>();
      for (const r of runRows) if (r.machine === id && r.edge >= 0) byEntity.set(r.entity, [...(byEntity.get(r.entity) ?? []), r]);
      for (const [entity, rows] of byEntity) {
        const counts = new Map<number, TransitionRow[]>();
        for (const r of rows) counts.set(r.edge, [...(counts.get(r.edge) ?? []), r]);
        for (const [edge, rs] of counts) {
          for (let i = 0; i + 12 <= rs.length; i++) {
            const from = rs[i].frame, to = rs[i + 11].frame;
            let lo = Infinity, hi = -Infinity;
            for (let f = from; f <= to; f += Math.max(1, Math.floor((to - from) / 64))) { const p = runProgress(f); lo = Math.min(lo, p); hi = Math.max(hi, p); }
            if (hi - lo < 0.005) { loops.push({ entity, edge: name(a.edges[edge]), times: 12, frames: [from, to], progress: +lo.toFixed(4) }); break; }
          }
        }
      }
      out[id] = {
        entities: m.entities.size,
        stateCoverage: { reached: a.states.length - missing.length, of: a.states.length, missing },
        transitionCoverage: { fired: a.edges.length - unfired.length, of: a.edges.length, missing: unfired.map(name) },
        reached: Object.fromEntries(a.states.map((s) => [s.name, m.reached.get(s.name) ?? 0])),
        fired: Object.fromEntries(a.edges.map((e) => [name(e), m.fired.get(e.index) ?? 0])),
        softlocks,
        unconfirmed,
        loops,
        unreachable: a.unreachable,
        deadEnds: a.deadEnds,
        analysis: a,
      };
    }
    return out;
  }
}
