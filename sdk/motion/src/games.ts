// Games: state from input, frame by frame (spec: "Games: state from input, frame by frame").
// The state at a frame is a pure fold over the earlier frames' input, so any frame renders on its own.
import { useRef } from "react";
import { useClockState, useLocalTime, type FrameInput, type InputTimeline } from "./clock";
import type { Machine, StepOptions, Watched } from "./machine";

// State machines (machine.ts) are the generic core; a game steps each machine once a frame inside its fold, so a
// machine's steps are the game's frames (`after: 30` is 30 frames). The helpers below are the game-specific layer.
export * from "./machine";

/** One frame for each entity of a list, each carrying its own machine (one per enemy), in order. */
export function stepAll<C, E, N extends string, K extends string>(m: Machine<C, E, N, K>, entities: readonly C[], env: E, opts?: readonly string[] | StepOptions): C[] {
  const out = new Array<C>(entities.length);
  for (let i = 0; i < entities.length; i++) out[i] = m.step(entities[i], env, opts);
  return out;
}

/** The input edges between two frames' keys, as events for a machine's `on`: "keydown:K" for each key pressed this
 *  frame, "keyup:K" for each released (keep last frame's keys in the game state to have `prev`). */
export function keyEdges(prev: readonly string[] | undefined, now: readonly string[] | undefined): string[] {
  const out: string[] = [];
  const p = prev ?? [], q = now ?? [];
  for (const k of q) if (p.indexOf(k) < 0) out.push("keydown:" + k);
  for (const k of p) if (q.indexOf(k) < 0) out.push("keyup:" + k);
  return out;
}

function inputAt(timeline: InputTimeline, frame: number): FrameInput {
  const entry = timeline[frame];
  if (!entry) return { keys: [] };
  return { ...entry, keys: entry.keys ? [...entry.keys] : [] };
}

/** This frame's input ({keys: []} when there was none). */
export function useInput(): FrameInput {
  const clock = useClockState("useInput");
  const { frame, origin } = useLocalTime("useInput");
  return inputAt(clock.input, origin + frame);
}

// The fold is computed once and kept (spec: "the fold is computed once and kept"). States are cached per step
// function, then per Sequence origin and initial state; each cache is the fold's own states for frames 0, 1, 2 ...,
// extended only as far as a frame needs, beside the input entry each step used. Rendering a game's frames in order
// therefore costs one step per frame, and any frame still equals the fold from frame 0. States are shared between
// frames, so step must return new states rather than mutate the old ones (as a pure fold does anyway).
//
// Live play (void's player) records an input entry every frame: the cache isn't keyed by the timeline's content (each
// new entry would start a fresh fold from frame 0 and keep the old one, which grew without bound and froze the machine,
// 2026-10-17); an entry that differs from the one a step used drops the states from there, and only those are folded
// again.

type Step<S> = (state: S, input: FrameInput, frame: number) => S;

interface Fold {
  /** states[f] is the state at local frame f */
  states: unknown[];
  /** entries[g] is the timeline entry step g was given (undefined: none) */
  entries: (Partial<FrameInput> | undefined)[];
  /** event frames per isEvent (by its source: inline functions are new each render), and the frame they reach */
  events: Map<string, Kept>;
  /** the last timeline checked against entries, and how many of them it confirmed */
  checked?: { timeline: InputTimeline; through: number };
  /** live play: entries and checkpoints before this frame let go (a live game only goes forward) */
  pruned?: number;
}

const folds = new WeakMap<Function, Map<number, Map<unknown, Fold>>>();

function isPlainData(v: unknown, depth = 0): boolean {
  if (depth > 64) return false;
  if (v === null || typeof v === "string" || typeof v === "boolean") return true;
  if (typeof v === "number") return Number.isFinite(v);
  if (Array.isArray(v)) return v.every((x) => isPlainData(x, depth + 1));
  if (typeof v === "object" && Object.getPrototypeOf(v) === Object.prototype) {
    return Object.values(v as object).every((x) => isPlainData(x, depth + 1));
  }
  return false;
}

/** Equal initial states share a cache: primitives by value, plain data by its JSON, anything else by identity. An
 * object's key is worked out once (states are values, never changed in place): every useGameState and useGameEvents
 * call made it again each frame, a JSON of the whole initial state each. */
const initialKeys = new WeakMap<object, unknown>();
function initialKey(initial: unknown): unknown {
  if (initial === null || typeof initial !== "object") return initial;
  let k = initialKeys.get(initial);
  if (k === undefined) {
    k = isPlainData(initial) ? `json:${JSON.stringify(initial)}` : initial;
    initialKeys.set(initial, k);
  }
  return k;
}

/** An event's frames folded so far (ascending; only ever appended to), and the list last handed out with the frame it
 * ran through: handed out again while it's the same, so a frame's call allocates nothing (ZeroHeap; five calls a frame
 * were ~5.6 KB of pong's garbage). */
interface Kept {
  through: number;
  frames: number[];
  out?: readonly number[];
  outLast?: number;
}

const NO_EVENTS: readonly number[] = Object.freeze([]);

/** `kept`'s frames through `last`: the list handed out last time when it holds the same frames, else a new frozen one. */
function eventsThrough(kept: Kept, last: number): number[] {
  const fr = kept.frames;
  let n = fr.length;
  while (n > 0 && fr[n - 1] > last) n--;
  const out = kept.out;
  // frames are only appended, so the same count is the same frames
  if (out && out.length === n) {
    kept.outLast = last;
    return out as number[];
  }
  kept.out = Object.freeze(fr.slice(0, n));
  kept.outLast = last;
  return kept.out as number[];
}

function foldFor<S>(step: Step<S>, initial: S, origin: number): Fold {
  let byOrigin = folds.get(step);
  if (!byOrigin) folds.set(step, (byOrigin = new Map()));
  let byInitial = byOrigin.get(origin);
  if (!byInitial) byOrigin.set(origin, (byInitial = new Map()));
  const ik = initialKey(initial);
  let fold = byInitial.get(ik);
  if (!fold) byInitial.set(ik, (fold = { states: [initial], entries: [], events: new Map() }));
  return fold;
}

const json = new WeakMap<object, string>();
function sameEntry(a: Partial<FrameInput> | undefined, b: Partial<FrameInput> | undefined): boolean {
  if (a === b) return true;
  if (!a || !b) return (!a || !a.keys?.length && !a.pointer) && (!b || !b.keys?.length && !b.pointer);
  const ja = json.get(a) ?? JSON.stringify(a);
  const jb = json.get(b) ?? JSON.stringify(b);
  json.set(a, ja);
  json.set(b, jb);
  return ja === jb;
}

/** Drop what the timeline changed under the fold: states after the first step whose input entry isn't the one it used. */
function validate<S>(fold: Fold, timeline: InputTimeline, origin: number, step: Step<S>) {
  // a timeline already checked (renders pass the same one each frame) is checked again only past where it was
  const from = fold.checked && fold.checked.timeline === timeline ? fold.checked.through : 0;
  for (let g = from; g < fold.entries.length; g++) {
    if (!sameEntry(timeline[origin + g], fold.entries[g])) {
      // the state the changed step starts from, folded again if it was let go
      const at = stateOf(fold, step, g);
      fold.states.length = g + 1;
      fold.states[g] = at;
      fold.entries.length = g;
      for (const [k, e] of fold.events) {
        if (e.through > g) fold.events.set(k, { through: g, frames: e.frames.filter((f) => f <= g) });
      }
      break;
    }
  }
  // in place (a call a frame made a new one)
  if (fold.checked) {
    fold.checked.timeline = timeline;
    fold.checked.through = fold.entries.length;
  } else fold.checked = { timeline, through: fold.entries.length };
}

// Kept states: the last KEEP frames whole, and every CHECK-th before them, so a long game's memory stays flat (a pong
// state per frame grew a live session by 8 MB a second); an older frame is folded again from its checkpoint.
const KEEP = 512;
const CHECK = 64;

/** The state at local frame f, extending the kept states up to f if they don't reach it yet. */
function stateAt<S>(fold: Fold, step: Step<S>, timeline: InputTimeline, origin: number, f: number): S {
  const states = fold.states as (S | undefined)[];
  for (let g = states.length - 1; g < f; g++) {
    fold.entries[g] = timeline[origin + g];
    states.push(step(states[g] as S, inputAt(timeline, origin + g), g));
    const old = states.length - 1 - KEEP;
    if (old > 0 && old % CHECK !== 0) states[old] = undefined;
  }
  if (fold.checked && fold.checked.timeline === timeline) fold.checked.through = fold.entries.length;
  if (isLive(timeline, origin + f)) prune(fold, f);
  return stateOf(fold, step, Math.max(0, f));
}

/** How far back a live game keeps its inputs and checkpoints: a live game only moves forward, so what's older than this
 * is never folded again, and keeping every frame's input grew a live session ~1.3 MB a minute at 120 fps. */
const LIVE_KEEP = 4096;

/** Live play: let go of entries before `f - LIVE_KEEP`, and of checkpoints before the one a kept frame refolds from. */
function prune(fold: Fold, f: number) {
  const keepFrom = f - LIVE_KEEP;
  const from = fold.pruned ?? 0;
  if (keepFrom - from < CHECK) return;
  const checkFrom = keepFrom - (keepFrom % CHECK);
  for (let g = from; g < keepFrom; g++) fold.entries[g] = undefined;
  for (let g = from; g < checkFrom; g++) fold.states[g] = undefined;
  fold.pruned = keepFrom;
}

/** A kept state, or one folded again from the checkpoint before it with the inputs its steps used. */
function stateOf<S>(fold: Fold, step: Step<S>, f: number): S {
  const states = fold.states as (S | undefined)[];
  const have = states[f];
  if (have !== undefined || f >= states.length) return have as S;
  let g = f - (f % CHECK);
  let s = states[g] as S;
  for (; g < f; g++) {
    const e = fold.entries[g];
    s = step(s, { ...e, keys: e?.keys ? [...e.keys] : [] } as FrameInput, g);
  }
  return s;
}

/** How far ahead of this frame events are folded at most: 30 minutes at 120 fps. */
const AHEAD = 216_000;

/** Whether a person is playing: the live player marks each frame's input (a render's input is a fixed script). */
function isLive(timeline: InputTimeline, frame: number): boolean {
  return !!(timeline[frame] as { live?: boolean } | undefined)?.live;
}

/**
 * The game's state at this frame: `initial` at frame 0, then `step(state, input, g)` applied for each earlier frame g
 * in order. Frames count from the nearest enclosing Sequence, so a game starts when its Sequence does.
 */
export function useGameState<S>(initial: S, step: Step<S>): S {
  const clock = useClockState("useGameState");
  const { frame, origin } = useLocalTime("useGameState");
  if (typeof step !== "function") throw new TypeError("useGameState: step must be a function (state, input, frame) => state");
  if (frame <= 0) return initial;
  const fold = foldFor(step, initial, origin);
  validate(fold, clock.input, origin, step);
  return stateAt(fold, step, clock.input, origin, Math.floor(frame));
}

/**
 * The frames, in order, at which `isEvent(before, after)` holds: `after` is the state `step` makes at frame f from
 * `before`, the state at f - 1. It runs the same (kept) fold as useGameState through the composition's last frame,
 * counted from the nearest enclosing Sequence, so sound can be cued to what happens in the game. While a person plays
 * (live input) it runs only through this frame: what comes next hasn't been played yet. The list is read-only (frozen):
 * the same one is handed out while nothing new happened, so copy it before changing it.
 */
export function useGameEvents<S>(initial: S, step: Step<S>, isEvent: (before: S, after: S) => boolean): number[] {
  // the event's key, its source text, taken once per call site (toString each frame was a string a call)
  const keyRef = useRef<string | null>(null);
  const clock = useClockState("useGameEvents");
  const { frame, origin } = useLocalTime("useGameEvents");
  if (typeof step !== "function") throw new TypeError("useGameEvents: step must be a function (state, input, frame) => state");
  if (typeof isEvent !== "function") throw new TypeError("useGameEvents: isEvent must be a function (before, after) => boolean");
  // the composition's last frame, in this Sequence's frames; this frame when it's being played
  const end = clock.config.durationInFrames - 1 - origin;
  // played live, or a composition too long to fold ahead (an endless game's 1e9 frames): only through this frame. A
  // fold toward 1e9 took a webview to 19 GB in under a minute (2026-10-17), before any input had marked the play live
  const endless = end - frame > AHEAD;
  const lastFrame = isLive(clock.input, clock.frame) || endless ? Math.min(end, Math.floor(frame)) : end;
  if (lastFrame < 1) return NO_EVENTS as number[];
  const fold = foldFor(step, initial, origin);
  validate(fold, clock.input, origin, step);
  if (keyRef.current === null) keyRef.current = isEvent.toString();
  const key = keyRef.current;
  let kept = fold.events.get(key);
  if (!kept) fold.events.set(key, (kept = { through: 0, frames: [] }));
  if (kept.through < lastFrame) {
    stateAt(fold, step, clock.input, origin, lastFrame);
    let before = stateOf<S>(fold, step, kept.through);
    for (let f = kept.through + 1; f <= lastFrame; f++) {
      const after = stateOf<S>(fold, step, f);
      if (isEvent(before, after)) kept.frames.push(f);
      before = after;
    }
    kept.through = lastFrame;
  }
  // frozen: the same list may be handed out next frame, so a game can't change it under the next call
  return eventsThrough(kept, lastFrame);
}

/**
 * What a playtester needs from a game, exported beside the view as `export const playtest: Playtest<S> = {...}`
 * (sdk/react/docs/playtest.md). Optional: a game without one is played from pixels and events alone. Everything here is
 * pure, so a bot steps the game headlessly, restores any state it has seen (states are values) and searches from it;
 * whatever it plays is an input timeline `raea-motion render --input` turns back into the same frames.
 */
export interface Playtest<S, O = unknown> {
  /** the state at frame 0, as useGameState is given it */
  initial: S;
  /** the fold useGameState is given: the same function, so a bot plays the game the render shows */
  step: Step<S>;
  /** the key chords worth trying ([] is "nothing held"); a bot's moves are these, each held some frames */
  actions: string[][];
  /** what a player could know at this frame, as plain JSON (positions, lives, what's near): a model reads it */
  observe?: (state: S, frame: number) => O;
  /** how far toward the goal, 0..1: search follows it, and "stuck" means it stopped rising */
  progress?: (state: S, frame: number) => number;
  /** "won" when the goal is reached, "lost" on game over, null while play goes on */
  outcome?: (state: S, frame: number) => "won" | "lost" | null;
  /** named moments as useGameEvents takes them (a death, a coin): counted, placed and rendered for the judge */
  events?: Record<string, (before: S, after: S) => boolean>;
  /** every state machine in the state, by entity name (`{ player: player.watch(s.player), flow: flow.watch(s) }`): the
   *  trace records each one's state and transitions, and the machine checks (coverage, softlocks, unreachable states,
   *  reached) read them (sdk/react/docs/state-machines.md) */
  machines?: (state: S) => Record<string, Watched>;
}
