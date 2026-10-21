// Declared animation: moves, keyframe tracks and timelines, declared once and sampled by the engine each frame
// (raea-animate, a frame-pure fork of Animato, in Rust: animate/). An <Animate> sends its tracks once, as data, when it
// mounts; raea-react compiles them and writes each frame's opacity, translate, scale, rotate, clip, colours, sizes or
// counter text onto the element itself, from the composition's frame. Nothing here runs per frame but the check of
// whether the element is in the tree. Under plain bun (no engine) the same tracks are sampled here instead, so tests
// and renderFrame still see the animated style.
//
//   <Animate enter="rise" exit="fade" at={word("crack")} exitAt={90}>...</Animate>
//   <Animate enter={pop({ peak: 1.12 })} animate={{ backgroundColor: track({ 0: "#fff", 12: "#ffd400" }) }}>
//   const plan = timeline(sequence(["card", "rise"], parallel(["title", "pop"], ["cta", slide({ from: "right" })])));
//   <Animate timeline={plan} part="title">
//   <Stagger each={3} enter="rise">{chips}</Stagger>
import { Children, createElement, memo, useContext, useRef, type CSSProperties, type ReactNode } from "react";
import { ClockContext, LocalTimeContext, TimeBaseContext } from "./clock";
import { Easing } from "./easing";
import { spring as springValue, measureSpring } from "./spring";
import type { AnimKey, AnimTrack, Animation } from "./animate.gen";

export type { AnimKey, AnimTrack, Animation } from "./animate.gen";

// ---- curves: text names, which cross the bridge (the engine parses the same names) ----

/** Curve names. Any of these, or `bezier(x1,y1,x2,y2)`, `steps(n)`, `spring(mass,damping,stiffness)`,
 * `reverse(<curve>)`. */
export type CurveName =
  | "linear" | "ease"
  | `${"in" | "out" | "in-out"}-${"quad" | "cubic" | "quart" | "quint" | "sine" | "expo" | "circ" | "back" | "elastic" | "bounce"}`;
export type Curve = CurveName | (string & {});

/** Curve helpers: `curve.bezier(0.05, 0.7, 0.1, 1)`, `curve.spring({ damping: 15, stiffness: 120 })`. */
export const curve = {
  bezier: (x1: number, y1: number, x2: number, y2: number): Curve => `bezier(${x1},${y1},${x2},${y2})`,
  steps: (n: number): Curve => `steps(${n})`,
  /** a spring stretched over its segment (its settle lands on the next key) */
  spring: (c: { mass?: number; damping?: number; stiffness?: number } = {}): Curve => `spring(${c.mass ?? 1},${c.damping ?? 10},${c.stiffness ?? 100})`,
  reverse: (c: Curve): Curve => `reverse(${c})`,
};

// ---- tracks and moves ----

/** Keyframes by local frame: numbers, or colour text for colour properties. */
export type Keys = Record<number, number | string>;

/** One property's keyframes: values by frame, the curve between each pair (one for all, or one per segment). */
export interface TrackSpec {
  keys: [number, number | string][];
  ease: Curve | Curve[];
  unit?: "px" | "%";
  repeat?: "hold" | "loop" | "pingpong" | `${"loop" | "pingpong"}:${number}`;
  digits?: number;
}

// Declarations built inline in a render (`track({ 0: 30, 6: 0 })`, `pop({ peak: 1.1 })`) come back as the same object
// for the same arguments: kept by the arguments' fingerprint, checked equal on a hit, so a frame that builds the same
// declaration again allocates only its argument literal.
const MADE = new Map<number, { args: unknown[]; value: unknown }>();
const MADE_MAX = 4096;
function same(a: unknown, b: unknown, depth = 0): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null || depth > 32) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  let n = 0;
  for (const k in a as Record<string, unknown>) {
    if (!same((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k], depth + 1)) return false;
    n++;
  }
  for (const _ in b as Record<string, unknown>) n--;
  return n === 0;
}
function made<T>(tag: number, a0: unknown, a1: unknown, a2: unknown, build: () => T): T {
  const key = fingerprint(a2, fingerprint(a1, fingerprint(a0, mix(0x811c9dc5, tag))));
  const hit = MADE.get(key);
  if (hit && same(hit.args[0], a0) && same(hit.args[1], a1) && same(hit.args[2], a2)) return hit.value as T;
  const value = build();
  if (MADE.size >= MADE_MAX) MADE.clear();
  // the arguments copied as they were (a caller may reuse and change its object)
  MADE.set(key, { args: [structured(a0), structured(a1), structured(a2)], value });
  return value;
}
function structured(v: unknown): unknown {
  if (typeof v !== "object" || v === null) return v;
  if (Array.isArray(v)) return v.map(structured);
  const o: Record<string, unknown> = {};
  for (const k in v as Record<string, unknown>) o[k] = structured((v as Record<string, unknown>)[k]);
  return o;
}

/** A track: `track({ 0: 30, 6: 0 })` is a 30 px rise over 6 frames, out-cubic by default. */
export function track(keys: Keys, ease: Curve | Curve[] = "out-cubic", opts: Omit<TrackSpec, "keys" | "ease"> = NO_OPTS): TrackSpec {
  return made(1, keys, ease, opts, () => makeTrack(keys, ease, opts));
}
const NO_OPTS = {};
function makeTrack(keys: Keys, ease: Curve | Curve[], opts: Omit<TrackSpec, "keys" | "ease">): TrackSpec {
  const ks = Object.keys(keys).map(Number).sort((a, b) => a - b).map((f) => [f, keys[f]] as [number, number | string]);
  if (ks.length === 0) throw new Error("track: no keys");
  if (Array.isArray(ease) && ease.length !== ks.length - 1) throw new RangeError(`track: ${ks.length} keys take ${ks.length - 1} curves, got ${ease.length}`);
  return { keys: ks, ease, ...opts };
}

/** Tracks by property, in frames from the move's own start. */
export interface Move {
  tracks: Record<string, TrackSpec | TrackSpec[]>;
  /** how long it runs (its last key) */
  frames: number;
}

export function move(tracks: Record<string, TrackSpec | TrackSpec[] | Keys>): Move {
  const out: Record<string, TrackSpec | TrackSpec[]> = {};
  let frames = 0;
  for (const [k, v] of Object.entries(tracks)) {
    const t = Array.isArray(v) || (v as TrackSpec).keys ? (v as TrackSpec | TrackSpec[]) : track(v as Keys);
    out[k] = t;
    for (const s of Array.isArray(t) ? t : [t]) frames = Math.max(frames, s.keys[s.keys.length - 1][0]);
  }
  return { tracks: out, frames };
}

/** The same move later by `frames`. */
export function delay(m: MoveLike, frames: number): Move {
  const mm = toMove(m);
  return { frames: mm.frames + frames, tracks: mapTracks(mm, (t) => ({ ...t, keys: t.keys.map(([f, v]) => [f + frames, v]) })) };
}

/** A move played backwards: it leaves the way it came (each curve reversed, so an out-curve becomes the matching
 * in-curve). An exit is its entrance reversed. */
export function reverse(m: MoveLike): Move {
  const mm = toMove(m);
  const L = mm.frames;
  return {
    frames: L,
    tracks: mapTracks(mm, (t) => {
      const n = t.keys.length;
      const eases = Array.isArray(t.ease) ? t.ease : Array(Math.max(0, n - 1)).fill(t.ease);
      const keys = t.keys.map(([f, v]) => [L - f, v] as [number, number | string]).reverse();
      const rev = eases.map((e: Curve) => (e === "linear" || e === "" ? "linear" : `reverse(${e})`)).reverse();
      return { ...t, keys, ease: rev };
    }),
  };
}

function mapTracks(m: Move, f: (t: TrackSpec) => TrackSpec): Record<string, TrackSpec | TrackSpec[]> {
  const out: Record<string, TrackSpec | TrackSpec[]> = {};
  for (const [k, v] of Object.entries(m.tracks)) out[k] = Array.isArray(v) ? v.map(f) : f(v);
  return out;
}

// ---- the named moves, from the timings the approved pieces were measured to use (craft: animating.md) ----

type Dir = "left" | "right" | "up" | "down";
const OUT: Curve = "out-cubic";

/** y offset `distance` to 0 over `frames` (30 px over 6), fading in over `fade` (5). */
function riseImpl(o: { distance?: number; frames?: number; fade?: number; ease?: Curve } = {}): Move {
  const { distance = 30, frames = 6, fade = 5, ease = OUT } = o;
  return move({ y: track({ 0: distance, [frames]: 0 }, ease), opacity: track({ 0: 0, [fade]: 1 }, ease) });
}
/** scale `from` (0.6), `peak` (1.08) at `frames`/2, 1 at `frames` (8); in over 3 frames. */
function popImpl(o: { from?: number; peak?: number; frames?: number; ease?: Curve } = {}): Move {
  const { from = 0.6, peak = 1.08, frames = 8, ease = OUT } = o;
  return move({ scale: track({ 0: from, [Math.round(frames / 2)]: peak, [frames]: 1 }, ease), opacity: track({ 0: 0, 3: 1 }, ease) });
}
/** a hit: scale `from` (1.3), `dip` (0.96) at `frames`/2, 1 at `frames` (8); in over 2 frames. */
function slamImpl(o: { from?: number; dip?: number; frames?: number; ease?: Curve } = {}): Move {
  const { from = 1.3, dip = 0.96, frames = 8, ease = OUT } = o;
  return move({ scale: track({ 0: from, [Math.round(frames / 2)]: dip, [frames]: 1 }, ease), opacity: track({ 0: 0, 2: 1 }, ease) });
}
/** arrives from a side: `distance` px (30) over `frames` (7), fading in over 5. `from` is where it comes from. */
function slideImpl(o: { from?: Dir; distance?: number; frames?: number; ease?: Curve } = {}): Move {
  const { from = "left", distance = 30, frames = 7, ease = OUT } = o;
  const axis = from === "left" || from === "right" ? "x" : "y";
  const sign = from === "left" || from === "up" ? -1 : 1;
  return move({ [axis]: track({ 0: sign * distance, [frames]: 0 }, ease), opacity: track({ 0: 0, 5: 1 }, ease) });
}
/** a reveal wiping across from `from` (left) over `frames` (8), in-out-cubic, by clipping. */
function wipeImpl(o: { from?: Dir; frames?: number; ease?: Curve } = {}): Move {
  const { from = "left", frames = 8, ease = "in-out-cubic" } = o;
  const side = { left: "clipRight", right: "clipLeft", up: "clipBottom", down: "clipTop" }[from];
  return move({ [side]: track({ 0: 100, [frames]: 0 }, ease) });
}
/** a bar filling: width 0 to `to` (100%) over `frames` (24), out-cubic. */
function fillImpl(o: { to?: number; frames?: number; unit?: "px" | "%"; ease?: Curve } = {}): Move {
  const { to = 100, frames = 24, unit = "%", ease = OUT } = o;
  return move({ width: track({ 0: 0, [frames]: to }, ease, { unit }) });
}
/** a number counting from `from` (0) to `to` over `frames` (24), written as the element's text. */
function tickImpl(o: { to: number; from?: number; frames?: number; digits?: number; ease?: Curve }): Move {
  const { to, from = 0, frames = 24, digits = 0, ease = OUT } = o;
  return move({ text: track({ 0: from, [frames]: to }, ease, { digits }) });
}
/** a strike-through line drawn: width 0 to 100% over `frames` (8). */
function strikeImpl(o: { frames?: number; ease?: Curve } = {}): Move {
  const { frames = 8, ease = OUT } = o;
  return move({ width: track({ 0: 0, [frames]: 100 }, ease, { unit: "%" }) });
}
/** a slow push-in on footage: scale 1 to 1 + `amount` (0.04) across `frames` (90), linear. */
function pushInImpl(o: { amount?: number; frames?: number } = {}): Move {
  const { amount = 0.04, frames = 90 } = o;
  return move({ scale: track({ 0: 1, [frames]: 1 + amount }, "linear") });
}
/** opacity 0 to 1 over `frames` (6). */
function fadeImpl(o: { frames?: number; ease?: Curve } = {}): Move {
  const { frames = 6, ease = OUT } = o;
  return move({ opacity: track({ 0: 0, [frames]: 1 }, ease) });
}

// each move by its options' content: the same options give the same Move object
export const rise: typeof riseImpl = (o?: any) => made(20, o, undefined, undefined, () => riseImpl(o));
export const pop: typeof popImpl = (o?: any) => made(21, o, undefined, undefined, () => popImpl(o));
export const slam: typeof slamImpl = (o?: any) => made(22, o, undefined, undefined, () => slamImpl(o));
export const slide: typeof slideImpl = (o?: any) => made(23, o, undefined, undefined, () => slideImpl(o));
export const wipe: typeof wipeImpl = (o?: any) => made(24, o, undefined, undefined, () => wipeImpl(o));
export const fill: typeof fillImpl = (o?: any) => made(25, o, undefined, undefined, () => fillImpl(o));
export const tick: typeof tickImpl = (o?: any) => made(26, o, undefined, undefined, () => tickImpl(o));
export const strike: typeof strikeImpl = (o?: any) => made(27, o, undefined, undefined, () => strikeImpl(o));
export const pushIn: typeof pushInImpl = (o?: any) => made(28, o, undefined, undefined, () => pushInImpl(o));
export const fade: typeof fadeImpl = (o?: any) => made(29, o, undefined, undefined, () => fadeImpl(o));

export const moves = { rise, pop, slam, slide, wipe, fill, tick, strike, pushIn, fade };
export type MoveName = "rise" | "pop" | "slam" | "slide" | "wipe" | "fill" | "strike" | "pushIn" | "fade";
export type MoveLike = MoveName | Move | MoveLike[];

const NAMED = new Map<string, Move>();
/** A name, a move or several moves together, as one move. */
export function toMove(m: MoveLike): Move {
  if (typeof m === "string") {
    let v = NAMED.get(m);
    if (!v) {
      const f = (moves as Record<string, (o?: object) => Move>)[m];
      if (!f || (m as string) === "tick") throw new Error(`<Animate>: no move named "${m}" (${Object.keys(moves).filter((k) => k !== "tick").join(", ")})`);
      NAMED.set(m, (v = f()));
    }
    return v;
  }
  if (Array.isArray(m)) {
    const all = m.map(toMove);
    const tracks: Record<string, TrackSpec[]> = {};
    for (const mm of all) for (const [k, v] of Object.entries(mm.tracks)) (tracks[k] ??= []).push(...(Array.isArray(v) ? v : [v]));
    return { tracks, frames: Math.max(0, ...all.map((x) => x.frames)) };
  }
  return m;
}

// ---- timelines: who moves when, decided once ----

/** A timeline step: [part, move], a group from sequence/parallel/stagger, or a pause in frames. */
export type Step = [part: string, move: MoveLike] | { part: string; move: MoveLike; offset?: number } | Group | number;
export interface Group {
  kind: "sequence" | "parallel";
  steps: Step[];
  /** frames between steps of a sequence (negative overlaps) */
  gap: number;
}
/** Each step when the one before it ends (`gap` frames apart: sequenceGap). */
export function sequence(...steps: Step[]): Group {
  return { kind: "sequence", steps, gap: 0 };
}
export function sequenceGap(gap: number, ...steps: Step[]): Group {
  return { kind: "sequence", steps, gap };
}
/** Every step at once. */
export function parallel(...steps: Step[]): Group {
  return { kind: "parallel", steps, gap: 0 };
}
/** The parts `each` frames apart, each with `m`. With `grid`, delays spread by grid distance from an origin. */
export function stagger(parts: string[], each: number, m: MoveLike, grid?: { cols: number; rows?: number; from?: GridFrom }): Group {
  if (!grid) return { kind: "parallel", steps: parts.map((p, i) => ({ part: p, move: m, offset: i * each })), gap: 0 };
  const rows = grid.rows ?? Math.ceil(parts.length / grid.cols);
  return { kind: "parallel", steps: parts.map((p, i) => ({ part: p, move: m, offset: gridDelay(grid.cols, rows, grid.from ?? "top-left", i) * each })), gap: 0 };
}

export type GridFrom = "top-left" | "top-right" | "bottom-left" | "bottom-right" | "center" | "top" | "bottom" | "left" | "right";
/** A grid item's distance from an origin, in steps (raea-animate's StaggerPattern::Grid, the same rule). */
export function gridDelay(cols: number, rows: number, from: GridFrom, index: number): number {
  cols = Math.max(1, cols);
  rows = Math.max(1, rows);
  const i = Math.min(index, cols * rows - 1);
  const x = i % cols;
  const y = Math.floor(i / cols);
  const d = (a: number, b: number) => Math.abs(a - b);
  switch (from) {
    case "top-left": return x + y;
    case "top-right": return cols - 1 - x + y;
    case "bottom-left": return x + (rows - 1 - y);
    case "bottom-right": return cols - 1 - x + (rows - 1 - y);
    case "center": return Math.min(d(x, Math.floor((cols - 1) / 2)), d(x, Math.floor(cols / 2))) + Math.min(d(y, Math.floor((rows - 1) / 2)), d(y, Math.floor(rows / 2)));
    case "top": return y;
    case "bottom": return rows - 1 - y;
    case "left": return x;
    case "right": return cols - 1 - x;
  }
}

/** A timeline's plan: each part's start frame and move, and the whole length. */
export interface Plan {
  parts: Record<string, { at: number; move: Move }>;
  frames: number;
}

/** Steps placed in time (a sequence of them). Each part's start is decided here, once. */
export function timeline(...steps: Step[]): Plan {
  const parts: Plan["parts"] = {};
  const place = (s: Step, at: number): number => {
    if (typeof s === "number") return at + s;
    if (Array.isArray(s)) s = { part: s[0], move: s[1] };
    if ("kind" in s) {
      if (s.kind === "parallel") return Math.max(at, ...s.steps.map((x) => place(x, at)));
      let t = at;
      s.steps.forEach((x, i) => (t = place(x, i === 0 ? t : t + s.gap)));
      return t;
    }
    const m = toMove(s.move);
    const start = at + (s.offset ?? 0);
    if (parts[s.part]) throw new Error(`timeline: the part "${s.part}" is placed twice`);
    parts[s.part] = { at: start, move: m };
    return start + m.frames;
  };
  const frames = place(sequence(...steps), 0);
  return { parts, frames };
}

// ---- the wire: an Animation for the engine ----

function wireTracks(out: AnimTrack[], m: Move, at: number) {
  for (const [prop, v] of Object.entries(m.tracks)) {
    for (const t of Array.isArray(v) ? v : [v]) {
      const eases = Array.isArray(t.ease) ? t.ease : null;
      out.push({
        prop,
        keys: t.keys.map(([f, val], i): AnimKey => ({
          at: f + at,
          value: typeof val === "number" ? val : 0,
          color: typeof val === "string" ? val : "",
          ease: i === t.keys.length - 1 ? "" : String(eases ? eases[i] : t.ease),
        })),
        unit: t.unit ?? "",
        repeat: t.repeat ?? "",
        digits: t.digits ?? 0,
      });
    }
  }
}

/** Under the engine raea-react samples declarations; under plain bun they are sampled here. */
function engineSamples(): boolean {
  return typeof (globalThis as { __bevyHost?: unknown }).__bevyHost !== "undefined";
}

/** Where an element is in time. Under the engine only the TimeBase (the Sequence's offset and any Freeze, an object
 * that changes when the element's place does, not each frame): Rust knows the frame, and the fps is the clock's.
 * Under plain bun, the frame and fps too, since the tracks are sampled here. */
type Time = { frame: number; origin: number; freeze: number | null; fps: number };
const NATIVE = engineSamples();
function useTime(caller: string): Time {
  const base = useContext(TimeBaseContext);
  const t = useRef<Time | null>(null);
  const cur = t.current ?? (t.current = { frame: 0, origin: 0, freeze: null, fps: 0 });
  cur.origin = base.origin;
  cur.freeze = base.freeze;
  if (!NATIVE) {
    // (a hook read only off the engine: NATIVE is fixed for the process, so the hook order never changes)
    const clock = useContext(ClockContext);
    if (!clock) throw new Error(`${caller} was called outside a Clock: render it inside a composition`);
    const local = useContext(LocalTimeContext);
    cur.frame = local ? local.frame : clock.frame;
    cur.fps = clock.config.fps;
  }
  return cur;
}

// ---- fingerprints: a declaration (or a whole props object) by content, walked without allocating ----

const FN_IDS = new WeakMap<object, number>();
const SYM_IDS = new Map<symbol, number>();
let nextId = 1;
const mix = (h: number, x: number) => Math.imul(h ^ x, 0x01000193) >>> 0;
const idOf = (f: object) => {
  let i = FN_IDS.get(f);
  if (i === undefined) FN_IDS.set(f, (i = nextId++));
  return i;
};

/** FNV-1a over a value's numbers and strings. Functions (components, handlers) count by identity, React elements by
 * type, key and props; an inline `animate={{ y: track(...) }}` or `style={{...}}` is a new object every render, and this
 * tells it's the same content. Assumes props aren't mutated in place (React's own assumption). */
export function fingerprint(v: unknown, h: number, depth = 0): number {
  if (v === null || v === undefined) return mix(h, v === null ? 0x9e37 : 0x9e38);
  switch (typeof v) {
    case "number": {
      // the number's bits, through its integer and fractional parts (no typed-array view, no string)
      const i = Math.floor(v);
      return mix(mix(h, i | 0), ((v - i) * 4294967296) | 0);
    }
    case "string":
      for (let i = 0; i < v.length; i++) h = mix(h, v.charCodeAt(i));
      return mix(h, 0x22);
    case "boolean":
      return mix(h, v ? 0x31 : 0x30);
    case "function":
      return mix(mix(h, 0x66), idOf(v));
    case "symbol": {
      let i = SYM_IDS.get(v);
      if (i === undefined) SYM_IDS.set(v, (i = nextId++));
      return mix(mix(h, 0x73), i);
    }
    case "object": {
      if (depth > 48) return mix(h, nextId++); // too deep to walk: never the same
      const o = v as Record<string, unknown>;
      if (typeof o.$$typeof === "symbol") {
        // a React element: its type, key and props
        h = fingerprint(o.type, mix(h, 0x3c), depth + 1);
        h = fingerprint(o.key, h, depth + 1);
        return fingerprint(o.props, h, depth + 1);
      }
      if (Array.isArray(v)) {
        h = mix(h, 0x5b);
        for (let i = 0; i < v.length; i++) h = fingerprint(v[i], h, depth + 1);
        return mix(h, 0x5d);
      }
      h = mix(h, 0x7b);
      for (const k in o) {
        h = fingerprint(k, h, depth + 1);
        h = fingerprint(o[k], h, depth + 1);
      }
      return mix(h, 0x7d);
    }
    default:
      return h;
  }
}

/** A props object's fingerprint: what <Animate> and <Stagger> compare to know a render would give the same element,
 * and what a runtime's bailout can key on (`Animate.fingerprint`). */
export function propsFingerprint(props: object): number {
  return fingerprint(props, 0x811c9dc5);
}

/** The Animation text for these tracks at this place in time: built and serialised only when the declaration's
 * content or its place in time changes, and the same string object otherwise (so React sends it once and a frame
 * allocates nothing for it). */
type In = unknown;
function useWire(build: () => AnimTrack[], i0: In, i1: In, i2: In, i3: In, i4: In, i5: In, i6: In, origin: number, freeze: number | null, fps: number, window: boolean): Wire {
  const last = useRef<(Wire & { key: number; ins: In[] }) | null>(null);
  const prev = last.current;
  // the same objects as last time (a declaration hoisted out of the render): nothing to look at, nothing allocated
  if (prev && prev.anim.origin === origin && prev.anim.freeze === freeze && prev.anim.fps === fps && prev.window === window) {
    const p = prev.ins;
    if (p[0] === i0 && p[1] === i1 && p[2] === i2 && p[3] === i3 && p[4] === i4 && p[5] === i5 && p[6] === i6) return prev;
  }
  let key = 0x811c9dc5;
  key = fingerprint(i6, fingerprint(i5, fingerprint(i4, fingerprint(i3, fingerprint(i2, fingerprint(i1, fingerprint(i0, key)))))));
  key = fingerprint(origin, fingerprint(freeze, fingerprint(fps, fingerprint(window, key))));
  if (prev && prev.key === key) {
    // the same content in new objects: remembered in place (nothing allocated), so the next frame's identity check
    // can hit if the caller starts passing the same objects
    const p = prev.ins;
    p[0] = i0; p[1] = i1; p[2] = i2; p[3] = i3; p[4] = i4; p[5] = i5; p[6] = i6;
    return prev;
  }
  const ins = [i0, i1, i2, i3, i4, i5, i6];
  const tracks = build();
  const anim: Animation = { origin, freeze, fps, tracks, show_from: null, show_until: null };
  if (window) {
    // mount="during": shown from the first key, and (with an exit) hidden again after the last
    let from = Infinity;
    let until = -Infinity;
    for (const tr of tracks) for (const k of tr.keys) { from = Math.min(from, k.at); until = Math.max(until, k.at); }
    if (from !== Infinity) anim.show_from = from;
    if (until !== -Infinity && i4 !== undefined) anim.show_until = until + 1;
  }
  const json = JSON.stringify(anim);
  if (prev && prev.json === json) {
    prev.key = key;
    prev.ins = ins;
    return prev;
  }
  const w = { json, anim, key, ins, window };
  last.current = w;
  return w;
}

// ---- sampling here, for plain bun (the engine does this in Rust: engine/animate/src/spec.rs, the same rules) ----

const BACK1 = 1.70158, BACK2 = BACK1 * 1.525, BACK3 = BACK1 + 1;
const EL4 = (2 * Math.PI) / 3, EL5 = (2 * Math.PI) / 4.5;
function outBounce(t: number): number {
  const n = 7.5625, d = 2.75;
  if (t < 1 / d) return n * t * t;
  if (t < 2 / d) return n * (t -= 1.5 / d) * t + 0.75;
  if (t < 2.5 / d) return n * (t -= 2.25 / d) * t + 0.9375;
  return n * (t -= 2.625 / d) * t + 0.984375;
}
const CURVES: Record<string, (t: number) => number> = {
  linear: (t) => t,
  "in-quad": (t) => t * t, "out-quad": (t) => 1 - (1 - t) * (1 - t), "in-out-quad": (t) => (t < 0.5 ? 2 * t * t : 1 - ((-2 * t + 2) ** 2) / 2),
  "in-cubic": (t) => t * t * t, "out-cubic": (t) => { const u = 1 - t; return 1 - u * u * u; }, "in-out-cubic": (t) => { if (t < 0.5) return 4 * t * t * t; const u = -2 * t + 2; return 1 - (u * u * u) / 2; },
  "in-quart": (t) => t ** 4, "out-quart": (t) => 1 - (1 - t) ** 4, "in-out-quart": (t) => (t < 0.5 ? 8 * t ** 4 : 1 - (-2 * t + 2) ** 4 / 2),
  "in-quint": (t) => t ** 5, "out-quint": (t) => 1 - (1 - t) ** 5, "in-out-quint": (t) => (t < 0.5 ? 16 * t ** 5 : 1 - (-2 * t + 2) ** 5 / 2),
  "in-sine": (t) => 1 - Math.cos((t * Math.PI) / 2), "out-sine": (t) => Math.sin((t * Math.PI) / 2), "in-out-sine": (t) => -(Math.cos(t * Math.PI) - 1) / 2,
  "in-expo": (t) => (t === 0 ? 0 : 2 ** (10 * t - 10)), "out-expo": (t) => (t === 1 ? 1 : 1 - 2 ** (-10 * t)),
  "in-out-expo": (t) => (t === 0 || t === 1 ? t : t < 0.5 ? 2 ** (20 * t - 10) / 2 : (2 - 2 ** (-20 * t + 10)) / 2),
  "in-circ": (t) => 1 - Math.sqrt(1 - t * t), "out-circ": (t) => Math.sqrt(1 - (t - 1) ** 2),
  "in-out-circ": (t) => (t < 0.5 ? (1 - Math.sqrt(1 - (2 * t) ** 2)) / 2 : (Math.sqrt(1 - (-2 * t + 2) ** 2) + 1) / 2),
  "in-back": (t) => BACK3 * t * t * t - BACK1 * t * t, "out-back": (t) => { const u = t - 1; return 1 + BACK3 * u * u * u + BACK1 * u * u; },
  "in-out-back": (t) => (t < 0.5 ? ((2 * t) ** 2 * ((BACK2 + 1) * 2 * t - BACK2)) / 2 : ((2 * t - 2) ** 2 * ((BACK2 + 1) * (2 * t - 2) + BACK2) + 2) / 2),
  "in-elastic": (t) => (t === 0 || t === 1 ? t : -(2 ** (10 * t - 10)) * Math.sin((10 * t - 10.75) * EL4)),
  "out-elastic": (t) => (t === 0 || t === 1 ? t : 2 ** (-10 * t) * Math.sin((10 * t - 0.75) * EL4) + 1),
  "in-out-elastic": (t) => (t === 0 || t === 1 ? t : t < 0.5 ? -(2 ** (20 * t - 10) * Math.sin((20 * t - 11.125) * EL5)) / 2 : (2 ** (-20 * t + 10) * Math.sin((20 * t - 11.125) * EL5)) / 2 + 1),
  "in-bounce": (t) => 1 - outBounce(1 - t), "out-bounce": outBounce,
  "in-out-bounce": (t) => (t < 0.5 ? (1 - outBounce(1 - 2 * t)) / 2 : (1 + outBounce(2 * t - 1)) / 2),
  ease: Easing.bezier(0.42, 0, 1, 1),
};
const parsed = new Map<string, (t: number) => number>();
/** A curve name as a function (the engine's easing::Easing::parse, the same names). */
export function curveFn(name: string, fps = 30): (t: number) => number {
  const key = `${name}@${fps}`;
  let f = parsed.get(key);
  if (f) return f;
  const s = name.trim();
  const m = /^([a-z-]+)\((.*)\)$/.exec(s);
  const nums = () => m![2].split(",").map((x) => Number(x.trim()));
  if (s === "") f = CURVES.linear;
  else if (CURVES[s]) f = CURVES[s];
  else if (m && (m[1] === "bezier" || m[1] === "cubic-bezier")) { const [a, b, c, d] = nums(); f = Easing.bezier(a, b, c, d); }
  else if (m && m[1] === "steps") { const n = Math.max(1, nums()[0]); f = (t) => (t === 0 ? 0 : Math.min(1, Math.ceil(t * n) / n)); }
  else if (m && m[1] === "spring") {
    const [mass, damping, stiffness] = nums();
    const config = { mass, damping, stiffness };
    const n = measureSpring({ fps, config });
    f = (t) => springValue({ frame: t * n, fps, config });
  } else if (m && m[1] === "reverse") { const g = curveFn(m[2], fps); f = (t) => 1 - g(1 - t); }
  else throw new Error(`no easing named "${s}"`);
  const clamped = (t: number) => f!(Math.min(1, Math.max(0, t)));
  parsed.set(key, clamped);
  return clamped;
}

function fold(t: number, a: number, b: number, repeat: string): number {
  const span = b - a;
  const [kind, count] = repeat.split(":");
  const n = count === undefined ? null : Math.max(1, Number(count));
  if (span <= 0 || t < b || (t === b && kind !== "loop")) return t;
  const k = (t - a) / span;
  const mod = (x: number, y: number) => ((x % y) + y) % y;
  if (kind === "loop") return n !== null && k >= n ? b : a + mod(t - a, span);
  if (kind === "pingpong") {
    if (n !== null && k >= n) return n % 2 === 1 ? b : a;
    const c = mod(t - a, 2 * span);
    return c <= span ? a + c : a + 2 * span - c;
  }
  return t;
}

/** One track at local frame `t`: [key index, eased progress to the next]. */
function locate(tr: AnimTrack, t: number, fps: number): [number, number] {
  const k = tr.keys;
  if (k.length <= 1) return [0, 0];
  t = fold(t, k[0].at, k[k.length - 1].at, tr.repeat);
  if (t <= k[0].at) return [0, 0];
  if (t >= k[k.length - 1].at) return [k.length - 1, 0];
  let i = 0;
  while (i + 1 < k.length && k[i + 1].at <= t) i++;
  const p = Math.min(1, Math.max(0, (t - k[i].at) / Math.max(1e-12, k[i + 1].at - k[i].at)));
  return [i, curveFn(k[i].ease, fps)(p)];
}

const COLOR_PROPS = new Set(["color", "backgroundColor", "borderColor", "fill", "stroke", "outlineColor", "emissive", "textDecorationColor"]);
const ADD = new Set(["x", "y", "z", "rotate", "rotateZ", "rotateX", "rotateY"]);
const MUL = new Set(["opacity", "scale", "scaleX", "scaleY"]);
const r6 = (v: number) => { const x = Math.abs(v) < 1e-9 ? 0 : Math.round(v * 1e6) / 1e6; return Object.is(x, -0) ? 0 : x; };

/** What an Animation sets at composition frame `frame`: page style keys, a counter's text, or 3D props
 * (raea-animate's Sampled, here for plain bun). */
export function sampleAnimation(a: Animation, frame: number): { style: Record<string, string | number>; text?: string; values: Record<string, number | string> } {
  const t = a.freeze ?? frame - a.origin;
  const v: Record<string, number | string> = {};
  const units: Record<string, string> = {};
  const best: Record<string, AnimTrack> = {};
  const start = (tr: AnimTrack) => Math.min(...tr.keys.map((k) => k.at));
  for (const tr of a.tracks) {
    if (tr.keys.length === 0 || ADD.has(tr.prop) || MUL.has(tr.prop)) continue;
    const cur = best[tr.prop];
    const s = start(tr);
    if (!cur) best[tr.prop] = tr;
    else {
      const cs = start(cur);
      if (s <= t ? cs > t || s >= cs : cs > t && s < cs) best[tr.prop] = tr;
    }
  }
  let text: string | undefined;
  for (const tr of a.tracks) {
    if (tr.keys.length === 0) continue;
    const [i, p] = locate(tr, t, a.fps);
    const k = tr.keys;
    const isColor = COLOR_PROPS.has(tr.prop) || k.some((x) => x.color !== "");
    if (!ADD.has(tr.prop) && !MUL.has(tr.prop) && best[tr.prop] !== tr) continue;
    if (isColor) {
      v[tr.prop] = p === 0 || i + 1 >= k.length ? k[i].color : `color-mix(in oklab, ${k[i].color} ${r6((1 - p) * 100)}%, ${k[i + 1].color})`;
      continue;
    }
    const val = p === 0 || i + 1 >= k.length ? k[i].value : k[i].value + (k[i + 1].value - k[i].value) * p;
    const prop = tr.prop === "rotateZ" ? "rotate" : tr.prop;
    if (ADD.has(prop)) v[prop] = ((v[prop] as number) ?? 0) + val;
    else if (prop === "scale") { v.scaleX = ((v.scaleX as number) ?? 1) * val; v.scaleY = ((v.scaleY as number) ?? 1) * val; }
    else if (MUL.has(prop)) v[prop] = ((v[prop] as number) ?? 1) * val;
    else if (prop === "text") { const s = val.toFixed(Math.min(6, tr.digits)); text = /^-?[0.]+$/.test(s) ? s.replace("-", "") : s; }
    else v[prop] = val;
    if (prop === "x" || prop === "y" || prop === "width" || prop === "height") units[prop] = tr.unit || "px";
  }
  const style: Record<string, string | number> = {};
  const n = (x: unknown) => String(r6(x as number));
  if (v.opacity !== undefined) style.opacity = r6(Math.min(1, Math.max(0, v.opacity as number)));
  if (v.x !== undefined || v.y !== undefined || v.z !== undefined) style.translate = `${n(v.x ?? 0)}${units.x ?? "px"} ${n(v.y ?? 0)}${units.y ?? "px"}${v.z !== undefined ? ` ${n(v.z)}px` : ""}`;
  if (v.scaleX !== undefined || v.scaleY !== undefined) style.scale = `${n(v.scaleX ?? 1)} ${n(v.scaleY ?? 1)}`;
  if (v.rotate !== undefined) style.rotate = `${n(v.rotate)}deg`;
  for (const k of ["width", "height"]) if (v[k] !== undefined) style[k] = `${n(Math.max(0, v[k] as number))}${units[k]}`;
  const clips = ["clipTop", "clipRight", "clipBottom", "clipLeft"];
  if (clips.some((c) => v[c] !== undefined)) style.clipPath = `inset(${clips.map((c) => `${n(Math.min(100, Math.max(0, (v[c] as number) ?? 0)))}%`).join(" ")})`;
  if (v.blur !== undefined) style.filter = `blur(${n(Math.max(0, v.blur as number))}px)`;
  const own = new Set(["opacity", "x", "y", "z", "scaleX", "scaleY", "rotate", "rotateX", "rotateY", "width", "height", "blur", ...clips]);
  for (const [k, x] of Object.entries(v)) if (!own.has(k)) style[k] = typeof x === "number" ? r6(x) : x;
  return { style, text, values: v };
}

// ---- the components ----

export interface AnimateOwnProps {
  /** the entrance: a move's name, a move, or several together */
  enter?: MoveLike;
  /** the exit, played at `exitAt` (an entrance reversed: `exit="rise"` sinks back out) */
  exit?: MoveLike;
  /** free tracks by property, in local frames: `{ backgroundColor: track({ 0: "#fff", 20: "#000" }) }` */
  animate?: Record<string, TrackSpec | TrackSpec[] | Keys> | Move;
  /** local frame the entrance starts (0) */
  at?: number;
  /** local frame the exit starts */
  exitAt?: number;
  /** a timeline's plan and this element's part in it: the part's move, at its start (plus `at`) */
  timeline?: Plan;
  part?: string;
  /** "always" (default): in the tree, and its layout, the whole time (invisible before an entrance that fades in);
   * "during": only from the entrance's start until the exit ends (it takes its space when it arrives) */
  mount?: "during" | "always";
  /** the element: "div" by default */
  as?: string;
  style?: CSSProperties;
  className?: string;
  children?: ReactNode;
}
export type AnimateProps = AnimateOwnProps & { [attr: string]: unknown };

const OWN = new Set(["enter", "exit", "animate", "at", "exitAt", "timeline", "part", "mount", "as", "style", "children"]);

/** An element whose animation is declared once and drawn by the engine each frame. Under the engine it reads no
 * per-frame value: its place in time comes from the TimeBase, the frame and fps from Rust, and a mount window
 * (mount="during") is Rust's to show and hide. A render whose props have the same content as the last gives the same
 * element object back, so the frame allocates nothing for it. */
function AnimateImpl(props: AnimateProps) {
  const { enter, exit, animate, at = 0, exitAt, timeline: plan, part, mount = "always", as = "div", style, children } = props as AnimateOwnProps;
  const time = useTime("<Animate>");
  const wire = useWire(
    () => {
      const out: AnimTrack[] = [];
      let start = at;
      let enterMove: Move | null = enter !== undefined ? toMove(enter) : null;
      if (plan) {
        const p = part !== undefined ? plan.parts[part] : undefined;
        if (!p) throw new Error(`<Animate>: the timeline has no part "${part}" (${Object.keys(plan.parts).join(", ")})`);
        start = p.at + at;
        enterMove = enterMove ? toMove([p.move, enterMove]) : p.move;
      }
      if (enterMove) wireTracks(out, enterMove, start);
      if (exit !== undefined) {
        if (exitAt === undefined) throw new Error("<Animate>: an exit needs exitAt (the local frame it starts)");
        wireTracks(out, reverse(exit), exitAt);
      }
      if (animate) wireTracks(out, (animate as Move).frames !== undefined && (animate as Move).tracks ? (animate as Move) : move(animate as Record<string, TrackSpec>), 0);
      return out;
    },
    enter, exit, animate, at, exitAt, plan, part,
    time.origin,
    time.freeze,
    time.fps,
    mount === "during",
  );
  const memo = useRef<{ fp: number; wire: Wire; el: ReturnType<typeof createElement> | null } | null>(null);
  if (NATIVE) {
    // the same content as last render: the same element (nothing allocated, nothing for React or the runtime to diff)
    const fp = propsFingerprint(props);
    const m = memo.current;
    if (m && m.fp === fp && m.wire === wire) return m.el;
    const el = element(as, props, style, children ?? textOf(wire), wire.json);
    memo.current = { fp, wire, el };
    return el;
  }
  // plain bun: sampled here, at this frame
  if (mount === "during") {
    const [from, until] = spanOf(wire);
    const f = time.freeze ?? time.frame;
    if (from !== Infinity && f < from) return null;
    if (exitAt !== undefined && f >= until + 1) return null;
  }
  const sampled = sampleAnimation(wire.anim, time.origin + (time.freeze ?? time.frame));
  return element(as, props, sampled ? { ...style, ...sampled.style } : style, sampled.text !== undefined ? sampled.text : children ?? textOf(wire), wire.json);
}

/** The element's own props (everything but the declaration's) in one object, with its declaration's text. */
function element(as: string, props: AnimateProps, style: unknown, kids: ReactNode, json: string) {
  const out: Record<string, unknown> = {};
  for (const k in props) if (!OWN.has(k)) out[k] = props[k];
  out.style = style;
  out["data-raea-anim"] = json;
  return createElement(as, out, kids);
}

/** <Animate>, memoised by its props' content (React skips a render whose props fingerprint matches the last). The
 * fingerprint is on it as `Animate.fingerprint` for a runtime's own bailout. */
export const Animate = Object.assign(memo(AnimateImpl, (a, b) => propsFingerprint(a) === propsFingerprint(b)), { fingerprint: propsFingerprint });

type Wire = { json: string; anim: Animation; window?: boolean; span?: [number, number]; text?: string | null };
/** The first and last key of a declaration, worked out once. */
function spanOf(w: Wire): [number, number] {
  if (!w.span) {
    let from = Infinity;
    let until = -Infinity;
    for (const tr of w.anim.tracks) for (const k of tr.keys) { from = Math.min(from, k.at); until = Math.max(until, k.at); }
    w.span = [from, until];
  }
  return w.span;
}
/** A counter's first text, worked out once. */
function textOf(w: Wire): string | undefined {
  if (w.text === undefined) w.text = initialText(w.anim) ?? null;
  return w.text ?? undefined;
}

/** A counter's first value, as its text (the engine rewrites it each frame). */
function initialText(a: Animation): string | undefined {
  const t = a.tracks.find((x) => x.prop === "text");
  return t ? sampleAnimation({ ...a, tracks: [t] }, a.origin + (t.keys[0]?.at ?? 0)).text : undefined;
}

export interface StaggerProps extends Omit<AnimateOwnProps, "timeline" | "part"> {
  /** frames between one child's entrance and the next */
  each: number;
  /** spread by grid distance instead of order */
  grid?: { cols: number; rows?: number; from?: GridFrom };
  /** the exit staggered too, `exitEach` frames apart (0: all together) */
  exitEach?: number;
}

/** Each child in its own <Animate>, entrances `each` frames apart (or by grid distance). The same content as last
 * render gives the same element back. */
function StaggerImpl(props: StaggerProps) {
  const memoEl = useRef<{ fp: number; el: ReturnType<typeof createElement> } | null>(null);
  const fp = propsFingerprint(props);
  if (memoEl.current && memoEl.current.fp === fp) return memoEl.current.el;
  const { each, grid, children, at = 0, exitAt, exitEach = 0, as, style, className, ...anim } = props;
  const items = Children.toArray(children);
  const rows = grid ? grid.rows ?? Math.ceil(items.length / grid.cols) : 0;
  const el = createElement(
    as ?? "div",
    { style, className },
    items.map((child, i) => {
      const d = grid ? gridDelay(grid.cols, rows, grid.from ?? "top-left", i) * each : i * each;
      return createElement(Animate, { key: i, ...anim, at: at + d, exitAt: exitAt === undefined ? undefined : exitAt + i * exitEach }, child);
    }),
  );
  memoEl.current = { fp, el };
  return el;
}
export const Stagger = Object.assign(memo(StaggerImpl, (a, b) => propsFingerprint(a) === propsFingerprint(b)), { fingerprint: propsFingerprint });

/** For a 3D node (<Mesh animate>, materials): the declaration's text, or under plain bun the props it sets now. */
export function useAnimation3d(spec: Record<string, TrackSpec | TrackSpec[] | Keys> | Move | undefined): { attr?: string; values?: Record<string, number | string> } {
  const time = useTime("animate");
  const wire = useWire(() => {
    const out: AnimTrack[] = [];
    if (spec) wireTracks(out, (spec as Move).tracks && (spec as Move).frames !== undefined ? (spec as Move) : move(spec as Record<string, TrackSpec>), 0);
    return out;
  }, spec, undefined, undefined, undefined, undefined, undefined, undefined, time.origin, time.freeze, time.fps, false);
  if (!spec) return {};
  if (NATIVE) return { attr: wire.json };
  return { attr: wire.json, values: sampleAnimation(wire.anim, time.origin + (time.freeze ?? time.frame)).values };
}
