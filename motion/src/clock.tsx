// The clock: which frame of which composition is being rendered, and the hooks that read it
// (spec: "A composition is a component rendered once per frame", "The clock renderers use").
import { createContext, useContext, type ReactNode } from "react";

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

export function useClockState(caller: string): ClockState {
  const clock = useContext(ClockContext);
  if (!clock) {
    throw new Error(`${caller} was called outside a Clock: render the tree inside <Clock> or through renderFrame`);
  }
  return clock;
}

export function useLocalTime(caller: string): LocalTime {
  const clock = useClockState(caller);
  const local = useContext(LocalTimeContext);
  return local ?? { frame: clock.frame, origin: 0, width: clock.config.width, height: clock.config.height };
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
export function Clock(props: ClockProps) {
  const { frame, fps, width, height, durationInFrames, id = "main", defaultProps = {}, words = [], perception = {}, input = {} } = props;
  if (typeof frame !== "number" || !Number.isFinite(frame)) {
    throw new RangeError(`Clock: frame must be a finite number, got ${frame}`);
  }
  if (!(fps > 0) || !Number.isFinite(fps)) throw new RangeError(`Clock: fps must be a positive number, got ${fps}`);
  requirePositiveInt("width", width);
  requirePositiveInt("height", height);
  requirePositiveInt("durationInFrames", durationInFrames);
  const state: ClockState = {
    config: { id, width, height, fps, durationInFrames, defaultProps },
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
