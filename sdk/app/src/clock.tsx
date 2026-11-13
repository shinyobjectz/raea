// The clock: which frame of which composition is being rendered, and the hooks that read it
// (spec: "A composition is a component rendered once per frame", "The clock renderers use").
import { createContext, useContext, useLayoutEffect, useReducer, useRef, version, type ReactNode } from "react";

export interface TimelineWord {
  /** the word as spoken */
  text: string;
  /** first frame of the word */
  from: number;
  /** frame after the word ends (exclusive) */
  to: number;
}

export interface VideoConfig {
  /** the composition's id ("main" by default) */
  id: string;
  /** in px */
  width: number;
  /** in px */
  height: number;
  /** frames a second */
  fps: number;
  /** its length in frames */
  durationInFrames: number;
  /** the props the composition is rendered with */
  defaultProps: Record<string, unknown>;
}

/** A box in composition px. */
export interface Rect {
  /** the left edge, px */
  x: number;
  /** the top edge, px */
  y: number;
  /** width, px */
  w: number;
  /** height, px */
  h: number;
}

/** What perception saw at this frame: the largest face, and the platform's safe area (composition px). */
export interface Perception {
  /** the largest face's box, when there's one */
  face?: Rect;
  /** px the platform's UI covers at the top and bottom */
  safe?: { top?: number; bottom?: number };
}

/** One frame's input for a game. */
export interface FrameInput {
  /** the keys held (KeyboardEvent.key names: "ArrowLeft", "a", " ") */
  keys: string[];
  /** the primary pointer in composition px, and whether it's pressed */
  pointer?: { x: number; y: number; down: boolean };
  /** the mouse buttons held besides the primary one: "Right", "Middle", "Back", "Forward" (Bevy's MouseButton names) */
  buttons?: string[];
  /** the wheel turned this frame, in CSS px, down and right positive (the DOM's deltaX and deltaY) */
  wheel?: { x: number; y: number };
  /** each connected gamepad: the buttons held and the sticks' and triggers' axes, by Bevy's GamepadButton and
   * GamepadAxis names ("South", "DPadUp", "RightTrigger2"; "LeftStickX" -1..1, "LeftZ") */
  gamepads?: { buttons: string[]; axes: Record<string, number> }[];
  /** the fingers down, in composition px */
  touches?: { id: number; x: number; y: number }[];
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
  /** the subtree's own frame: the composition's frame minus `origin` */
  frame: number;
  /** the composition frame the subtree's frame 0 falls on */
  origin: number;
  /** width and height a Sequence overrides for its children */
  width: number;
  /** in px, what the Sequence gives its children */
  height: number;
  /** frames the subtree runs for from its frame 0, which useVideoConfig reports as durationInFrames: a Sequence's own
   * length cut where its parent's ends, a Loop's one pass, the composition's at the root */
  duration: number;
}

export const ClockContext = createContext<ClockState | null>(null);

// A Clock's state is one object while its settings, words, perception and input hold, its frame moved on in place: a
// new object each frame changed the context, and React looked through every part under it for a reader (10 000 cubes
// beside one turning camera, 1.5 ms a frame; docs/control.feature FC1b). What reads the frame subscribes instead.
interface Ticker {
  v: number;
  /** told by tickClocks ahead of this frame's render, so the Clock's commit doesn't tell them again */
  pre: boolean;
  subs: Set<() => void>;
}
const TICKERS = new WeakMap<ClockState, Ticker>();
// the mounted Clocks' tickers, for tickClocks
const MOUNTED = new Set<Ticker>();
function ticker(): Ticker {
  const t: Ticker = { v: 0, pre: false, subs: new Set() };
  return t;
}
// its readers called: each re-renders unless it already rendered at the version now
function tell(t: Ticker) {
  for (const cb of [...t.subs]) cb();
}
/** Tells every mounted Clock's frame readers that the frame is about to move on, in the same batch as the update that
 * moves it, so they render in that one commit. A host that steps frames back to back calls it with its frame update:
 * else each Clock tells them as its frame commits, a second commit each frame, and React stops a chain of more than 50
 * (raea's bake steps a range of frames as one chain: many_cubes stalled at frame 26). */
export function tickClocks(): void {
  for (const t of MOUNTED) {
    t.pre = true;
    t.v++;
    tell(t);
  }
}
// raea's live runtime (sdk/react/js/lite) re-renders by the contexts read, so it keeps a new state each frame, as before
const LITE = version.endsWith("-raea-lite");
/** Re-renders the caller when its Clock's frame moves on. A state not a Clock's own (a copy given to a subtree, as
 * InputRegion's and R3F's) is a new object each frame, so its readers hear of it through the context as before. */
export function useClockTick(clock: ClockState | null): void {
  const t = clock ? TICKERS.get(clock) : undefined;
  // the version it rendered at: told of a frame it already rendered (its parent re-rendered it), it isn't rendered again
  // (useSyncExternalStore's own record moves on only in a passive effect, after the Clock tells, so every reader
  // rendered twice a frame)
  const [, force] = useReducer((n: number) => n + 1, 0);
  const seen = useRef(-1);
  if (t) seen.current = t.v;
  useLayoutEffect(() => {
    if (!t) return;
    const cb = () => void (seen.current !== t.v && force());
    t.subs.add(cb);
    return () => void t.subs.delete(cb);
  }, [t]);
}
/** Calls `cb` each time `clock`'s frame moves on (in the batch that renders the new frame), until the returned stop is
 * called: for a store whose value can change only from frame to frame (useWorldAnswer). */
export function onClockTick(clock: ClockState, cb: () => void): () => void {
  const t = TICKERS.get(clock);
  if (!t) return () => {};
  t.subs.add(cb);
  return () => void t.subs.delete(cb);
}
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

/** The time where it's called: the nearest Sequence's (its local frame, where it starts, and the size it gives its
 * children), or the Clock's outside any. `caller` names the hook in errors. The hook under useCurrentFrame, for components of your own that need more
 * than the frame. */
export function useLocalTime(caller: string): LocalTime {
  const clock = useClockState(caller);
  const local = useContext(LocalTimeContext);
  // inside a Sequence its time is a new object each frame; outside one, the frame is the Clock's own
  useClockTick(local ? null : clock);
  if (local) return local;
  let t = rootTimes.get(clock);
  const c = clock.config;
  if (!t || t.frame !== clock.frame || t.width !== c.width || t.height !== c.height || t.duration !== c.durationInFrames) {
    t = { frame: clock.frame, origin: 0, width: c.width, height: c.height, duration: c.durationInFrames };
    rootTimes.set(clock, t);
  }
  return t;
}

/** The frame being rendered, counted from the start of the nearest enclosing Sequence. */
export function useCurrentFrame(): number {
  return useLocalTime("useCurrentFrame").frame;
}

/** The settings of the composition being rendered (inside a Sequence, its width, height and length win). */
export function useVideoConfig(): VideoConfig {
  const clock = useClockState("useVideoConfig");
  const local = useContext(LocalTimeContext);
  const c = clock.config;
  if (!local || (local.width === c.width && local.height === c.height && local.duration === c.durationInFrames)) return c;
  return { ...c, width: local.width, height: local.height, durationInFrames: local.duration };
}

export interface ClockProps {
  /** the composition frame to draw */
  frame: number;
  /** frames a second */
  fps: number;
  /** in px */
  width: number;
  /** in px */
  height: number;
  /** the composition's length in frames */
  durationInFrames: number;
  /** the composition's id ("main" by default) */
  id?: string;
  /** the props the composition is rendered with */
  defaultProps?: Record<string, unknown>;
  /** the transcript, in composition frames, that captions and word cuts read */
  words?: readonly TimelineWord[];
  /** what perception saw at this frame (faces, the safe area) */
  perception?: Perception;
  /** a game's recorded input by frame */
  input?: InputTimeline;
  /** the composition */
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
/** What time a composition is at: the frame, frame rate, size and length its hooks read, and its words, perception
 * and input. The renderer, the live view and tests put one around every composition; a composition never needs its
 * own. */
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
  // the state kept while what it holds holds (perception compared shallowly: the host's is a new {} each frame when
  // there's none), its frame moved on in place and its readers told once the frame commits
  const st = useRef<ClockState | null>(null);
  let state = st.current;
  if (LITE || !state || state.config !== config || state.words !== words || state.input !== input || !samePerception(state.perception, perception)) {
    state = st.current = { config, frame, words, perception, input };
    TICKERS.set(state, ticker());
  } else if (state.frame !== frame) {
    // the version moved on as the frame does, so a reader rendering in this pass (its parent re-rendered) isn't
    // rendered again when the commit tells the rest
    state.frame = frame;
    TICKERS.get(state)!.v++;
  }
  const told = useRef<{ state: ClockState; frame: number } | null>(null);
  useLayoutEffect(() => {
    const s = st.current!;
    const t = TICKERS.get(s)!;
    const pre = t.pre;
    t.pre = false;
    if (told.current?.state === s && told.current.frame === s.frame) return;
    const fresh = told.current?.state !== s;
    if (fresh && told.current) MOUNTED.delete(TICKERS.get(told.current.state)!);
    MOUNTED.add(t);
    told.current = { state: s, frame: s.frame };
    if (!fresh && !pre) tell(t);
  });
  useLayoutEffect(() => () => void (told.current && MOUNTED.delete(TICKERS.get(told.current.state)!)), []);
  // no LocalTime at the root: useLocalTime makes the root's from the Clock (a new one each frame changed the context)
  if (!LITE) return <ClockContext.Provider value={state}>{props.children}</ClockContext.Provider>;
  return (
    <ClockContext.Provider value={state}>
      <LocalTimeContext.Provider value={{ frame, origin: 0, width, height, duration: durationInFrames }}>{props.children}</LocalTimeContext.Provider>
    </ClockContext.Provider>
  );
}

function samePerception(a: Perception, b: Perception): boolean {
  if (a === b) return true;
  const ka = Object.keys(a), kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => (a as Record<string, unknown>)[k] === (b as Record<string, unknown>)[k]);
}
