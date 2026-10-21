// The clock: which frame of which composition is being rendered, and the hooks that read it
// (spec: "A composition is a component rendered once per frame", "The clock renderers use").
import { createContext, useContext, useRef, type ReactNode } from "react";

export interface TimelineWord {
  text: string;
  /** first frame of the word */
  from: number;
  /** frame after the word ends (exclusive) */
  to: number;
}

export interface VideoConfig {
  id: string;
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
  defaultProps: Record<string, unknown>;
}

/** A box in composition px. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** What perception saw at this frame: the largest face, and the platform's safe area (composition px). */
export interface Perception {
  face?: Rect;
  safe?: { top?: number; bottom?: number };
}

/** One frame's input for a game. */
export interface FrameInput {
  keys: string[];
  pointer?: { x: number; y: number; down: boolean };
}

/** Recorded input by composition frame; frames without an entry had no input. */
export type InputTimeline = Record<number, Partial<FrameInput> | undefined>;

/** The composition-wide state a Clock provides. */
export interface ClockState {
  config: VideoConfig;
  /** the composition's frame */
  frame: number;
  /** the timeline's transcript, in composition frames */
  words: readonly TimelineWord[];
  perception: Perception;
  input: InputTimeline;
}

/** The local time a subtree sees: its own frame, and the composition frame its frame 0 falls on. */
export interface LocalTime {
  frame: number;
  origin: number;
  /** width and height a Sequence overrides for its children */
  width: number;
  height: number;
}

export const ClockContext = createContext<ClockState | null>(null);
export const LocalTimeContext = createContext<LocalTime | null>(null);

/** Where a subtree sits in time, as data that changes only when its place does (not every frame): the composition
 * frame its local frame 0 falls on, and a local frame it's held at (inside a Freeze). Declared animation reads this
 * instead of the frame, so it doesn't depend on a per-frame value; the engine knows the frame. */
export interface TimeBase {
  origin: number;
  freeze: number | null;
}
const BASES = new Map<number, Map<number, TimeBase>>();
const LIVE_BASES = new Map<number, TimeBase>();
/** The one TimeBase object for these values (the same object every frame, so it never reads as a change). */
export function timeBase(origin: number, freeze: number | null): TimeBase {
  if (freeze === null) {
    let b = LIVE_BASES.get(origin);
    if (!b) LIVE_BASES.set(origin, (b = { origin, freeze: null }));
    return b;
  }
  let m = BASES.get(origin);
  if (!m) BASES.set(origin, (m = new Map()));
  let b = m.get(freeze);
  if (!b) m.set(freeze, (b = { origin, freeze }));
  return b;
}
export const ROOT_BASE: TimeBase = timeBase(0, null);
export const TimeBaseContext = createContext<TimeBase>(ROOT_BASE);

export function useClockState(caller: string): ClockState {
  const clock = useContext(ClockContext);
  if (!clock) {
    throw new Error(`${caller} was called outside a Clock: render the tree inside <Clock> or through renderFrame`);
  }
  return clock;
}

/** The root's local time (no enclosing Sequence), one per clock and frame: every useCurrentFrame outside a Sequence
 * made one before (ZeroHeap) */
const rootTimes = new WeakMap<ClockState, LocalTime>();

export function useLocalTime(caller: string): LocalTime {
  const clock = useClockState(caller);
  const local = useContext(LocalTimeContext);
  if (local) return local;
  let t = rootTimes.get(clock);
  if (!t || t.frame !== clock.frame || t.width !== clock.config.width || t.height !== clock.config.height) {
    t = { frame: clock.frame, origin: 0, width: clock.config.width, height: clock.config.height };
    rootTimes.set(clock, t);
  }
  return t;
}

/** The frame being rendered, counted from the start of the nearest enclosing Sequence. */
export function useCurrentFrame(): number {
  return useLocalTime("useCurrentFrame").frame;
}

/** The settings of the composition being rendered (a Sequence's width and height win inside it). */
export function useVideoConfig(): VideoConfig {
  const clock = useClockState("useVideoConfig");
  const local = useContext(LocalTimeContext);
  if (!local || (local.width === clock.config.width && local.height === clock.config.height)) return clock.config;
  return { ...clock.config, width: local.width, height: local.height };
}

export interface ClockProps {
  frame: number;
  fps: number;
  width: number;
  height: number;
  durationInFrames: number;
  id?: string;
  defaultProps?: Record<string, unknown>;
  words?: readonly TimelineWord[];
  perception?: Perception;
  input?: InputTimeline;
  children?: ReactNode;
}

function requirePositiveInt(name: string, v: number) {
  if (!Number.isInteger(v) || v <= 0) throw new RangeError(`Clock: ${name} must be a positive integer, got ${v}`);
}

/** Puts the tree inside it at one frame of a composition with these settings. */
// a Clock's defaults, shared (each was a new object or array every frame)
const NO_PROPS = {};
const NO_WORDS: never[] = [];
const NO_PERCEPTION = {};
const NO_INPUT = {};
export function Clock(props: ClockProps) {
  const { frame, fps, width, height, durationInFrames, id = "main", defaultProps = NO_PROPS, words = NO_WORDS, perception = NO_PERCEPTION, input = NO_INPUT } = props;
  if (typeof frame !== "number" || !Number.isFinite(frame)) {
    throw new RangeError(`Clock: frame must be a finite number, got ${frame}`);
  }
  if (!(fps > 0) || !Number.isFinite(fps)) throw new RangeError(`Clock: fps must be a positive number, got ${fps}`);
  requirePositiveInt("width", width);
  requirePositiveInt("height", height);
  requirePositiveInt("durationInFrames", durationInFrames);
  // the config one object while its settings hold (useVideoConfig hands it out): it was made again every frame
  const kept = useRef<ClockState["config"] | null>(null);
  const k = kept.current;
  const config =
    k && k.id === id && k.width === width && k.height === height && k.fps === fps && k.durationInFrames === durationInFrames && k.defaultProps === defaultProps
      ? k
      : (kept.current = { id, width, height, fps, durationInFrames, defaultProps });
  const state: ClockState = {
    config,
    frame,
    words,
    perception,
    input,
  };
  return (
    <ClockContext.Provider value={state}>
      <LocalTimeContext.Provider value={{ frame, origin: 0, width, height }}>{props.children}</LocalTimeContext.Provider>
    </ClockContext.Provider>
  );
}
