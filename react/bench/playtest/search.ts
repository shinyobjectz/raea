// Tier (i) playtesters (react/docs/playtest.md): bots with no model that step a game's pure fold headlessly. States are
// values, so any state a bot has seen can be restored and searched from; every run is an input timeline that
// `raea-motion render --input` replays as the same frames. Deterministic: a seeded RNG, no clocks.
import type { Playtest } from "../../../motion/src/games";

export type Act = { a: number; hold: number };
/** A node of a search: a state at a frame, how it got there (parent chain), what happened on the way. */
export interface Node<S = any> {
  s: S;
  f: number;
  parent: Node<S> | null;
  move: Act | null;
  deaths: number;
  best: number;
}

export interface Event { name: string; frame: number; at: Record<string, unknown> }

export function rng(seed: number) {
  let t = seed >>> 0;
  return () => {
    t += 0x6d2b79f5;
    let r = Math.imul(t ^ (t >>> 15), 1 | t);
    r ^= r + Math.imul(r ^ (r >>> 7), 61 | r);
    return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
  };
}

export class Game<S> {
  steps = 0;
  constructor(public p: Playtest<S, any>) {}
  progress(s: S, f: number) { return this.p.progress ? this.p.progress(s, f) : 0; }
  outcome(s: S, f: number) { return this.p.outcome ? this.p.outcome(s, f) : null; }
  /** Hold one action for `hold` frames; the events that fired, by frame. */
  advance(n: Node<S>, move: Act, events?: Event[]): Node<S> {
    let { s, f, deaths, best } = n;
    const keys = this.p.actions[move.a];
    for (let i = 0; i < move.hold; i++) {
      const after = this.p.step(s, { keys: [...keys] } as any, f);
      this.steps++;
      if (this.p.events) {
        for (const [name, is] of Object.entries(this.p.events)) {
          if (is(s, after)) {
            if (name === "died") deaths++;
            events?.push({ name, frame: f + 1, at: this.where(name === "died" ? s : after, name === "died" ? f : f + 1) });
          }
        }
      }
      s = after;
      f++;
      best = Math.max(best, this.progress(s, f));
      if (this.outcome(s, f)) break;
    }
    return { s, f, parent: n, move, deaths, best };
  }
  where(s: S, f: number): Record<string, unknown> {
    const o = this.p.observe ? this.p.observe(s, f) : {};
    const out: Record<string, unknown> = { progress: +this.progress(s, f).toFixed(4) };
    for (const k of ["x", "y", "lives", "score"]) if (o && k in o) out[k] = o[k];
    return out;
  }
  root(): Node<S> { return { s: this.p.initial, f: 0, parent: null, move: null, deaths: 0, best: this.progress(this.p.initial, 0) }; }
  /** A cell for novelty: progress in 1/500ths, and x, y buckets when observe gives them. */
  cell(n: Node<S>): string {
    const o = this.p.observe ? this.p.observe(n.s, n.f) : null;
    const pr = Math.floor(this.progress(n.s, n.f) * 500);
    if (o && typeof o.x === "number" && typeof o.y === "number") return `${Math.floor(o.x / 48)},${Math.floor(o.y / 48)},${o.onGround ? 1 : 0}`;
    return String(pr);
  }
}

/** The moves from the root to n, oldest first. */
export function path<S>(n: Node<S>): Act[] {
  const out: Act[] = [];
  for (let m: Node<S> | null = n; m && m.move; m = m.parent) out.push(m.move);
  return out.reverse();
}

/** A run as the input timeline render --input takes: every frame's keys, from frame 0. */
export function timeline(p: Playtest<any, any>, moves: Act[], pad = 0) {
  const t: Record<string, { keys: string[] }> = {};
  let f = 0;
  for (const m of moves) for (let i = 0; i < m.hold; i++) t[String(f++)] = { keys: [...p.actions[m.a]] };
  const last = moves.length ? p.actions[moves[moves.length - 1].a] : [];
  for (let i = 0; i < pad; i++) t[String(f++)] = { keys: [...last] };
  return t;
}

// ---- random -----------------------------------------------------------------------------------------------
export function random<S>(g: Game<S>, o: { episodes: number; frames: number; seed: number }) {
  const r = rng(o.seed);
  const deaths: Event[] = [];
  let best = 0, won: Node<S> | null = null;
  for (let e = 0; e < o.episodes && !won; e++) {
    let n = g.root();
    while (n.f < o.frames) {
      const ev: Event[] = [];
      n = g.advance(n, { a: Math.floor(r() * g.p.actions.length), hold: 4 + Math.floor(r() * 28) }, ev);
      deaths.push(...ev.filter((x) => x.name === "died"));
      if (g.outcome(n.s, n.f) === "won") { won = n; break; }
      if (g.outcome(n.s, n.f) === "lost") break;
    }
    best = Math.max(best, n.best);
  }
  return { best, won, deaths };
}

// ---- novelty (Go-Explore): keep the first arrival at every cell, go back to rare cells and explore from them -----
export function novelty<S>(g: Game<S>, o: { iterations: number; explore: number; seed: number; maxFrames: number }) {
  const r = rng(o.seed);
  const archive = new Map<string, { n: Node<S>; chosen: number; grew: number }>();
  const root = g.root();
  archive.set(g.cell(root), { n: root, chosen: 0, grew: 0 });
  const deaths: Event[] = [];
  let won: Node<S> | null = null;
  for (let it = 0; it < o.iterations && !won; it++) {
    // weight 1/sqrt(1+chosen), favouring the far end a little (a cell's progress)
    const cells = [...archive.values()];
    let total = 0;
    const w = cells.map((c) => (total += (1 / Math.sqrt(1 + c.chosen)) * (0.5 + c.n.best)));
    const pick = r() * total;
    const from = cells[w.findIndex((x) => x >= pick)];
    from.chosen++;
    let n = from.n;
    for (let k = 0; k < o.explore && n.f < o.maxFrames; k++) {
      const ev: Event[] = [];
      n = g.advance(n, { a: Math.floor(r() * g.p.actions.length), hold: 4 + Math.floor(r() * 24) }, ev);
      deaths.push(...ev.filter((x) => x.name === "died"));
      const c = g.cell(n);
      const had = archive.get(c);
      if (!had) { archive.set(c, { n, chosen: 0, grew: 0 }); from.grew++; }
      else if (n.f < had.n.f && n.deaths <= had.n.deaths) had.n = n;
      if (g.outcome(n.s, n.f) === "won") { won = n; break; }
      if (g.outcome(n.s, n.f) === "lost") break;
    }
  }
  return { archive, won, deaths };
}

// ---- beam: the W most advanced states, every action at two holds, deduplicated by cell ----------------------------
export function beam<S>(g: Game<S>, o: { width: number; holds: number[]; maxFrames: number; start?: Node<S>[]; target?: number; stall?: number }) {
  let front = o.start ?? [g.root()];
  let best = front[0];
  let improved = front[0].f;
  const score = (n: Node<S>) => g.progress(n.s, n.f) - 0.01 * n.deaths;
  while (front.length) {
    const next = new Map<string, Node<S>>();
    for (const n of front) {
      for (let a = 0; a < g.p.actions.length; a++) {
        for (const hold of o.holds) {
          const m = g.advance(n, { a, hold });
          if (g.outcome(m.s, m.f) === "won") return { won: m, best: m };
          if (o.target !== undefined && g.progress(m.s, m.f) >= o.target) return { won: null, best: m, reached: m };
          if (m.f > o.maxFrames || g.outcome(m.s, m.f) === "lost") continue;
          const c = g.cell(m) + "|" + (m.f % 4);
          const had = next.get(c);
          if (!had || score(m) > score(had)) next.set(c, m);
          if (m.best > best.best + 1e-4) { best = m; improved = m.f; }
        }
      }
    }
    front = [...next.values()].sort((a, b) => score(b) - score(a)).slice(0, o.width);
    // no new ground for `stall` frames of play: the search has met the game's ceiling from here
    if (o.stall && front.length && Math.min(...front.map((n) => n.f)) > improved + o.stall) break;
  }
  return { won: null as Node<S> | null, best, reached: null as Node<S> | null };
}
