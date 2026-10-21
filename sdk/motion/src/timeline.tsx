// AbsoluteFill, Sequence, Series, Loop and Freeze (spec: "Sequence shifts time and limits it").
import { Children, Fragment, isValidElement, useContext, useRef, type CSSProperties, type ReactElement, type ReactNode } from "react";
import { LocalTimeContext, TimeBaseContext, timeBase, useClockState, useLocalTime, type LocalTime, type TimeBase } from "./clock";
import { isGeneratedAsset } from "./assets";
import { Audio } from "./media";

export interface AbsoluteFillProps {
  style?: CSSProperties;
  className?: string;
  children?: ReactNode;
  [attr: string]: unknown;
}

const FILL_BASE: CSSProperties = {
  position: "absolute",
  top: 0,
  left: 0,
  right: 0,
  bottom: 0,
  display: "flex",
  flexDirection: "column",
};

/** A box over the whole of its parent, laid out as a flex column; `style` is merged over that. */
export function AbsoluteFill({ style, children, ...rest }: AbsoluteFillProps) {
  return (
    <div {...rest} style={{ ...FILL_BASE, ...style }}>
      {children}
    </div>
  );
}

export type SequenceLayout = "absolute-fill" | "none";

export interface SequenceProps {
  from?: number;
  durationInFrames?: number;
  name?: string;
  layout?: SequenceLayout;
  width?: number;
  height?: number;
  style?: CSSProperties;
  className?: string;
  children?: ReactNode;
}

function checkDuration(owner: string, d: number): void {
  if (d === Infinity) return;
  if (!Number.isInteger(d) || d < 0) {
    throw new RangeError(`${owner}: durationInFrames must be a positive integer or Infinity, got ${d}`);
  }
}

function checkFinite(owner: string, name: string, v: number): void {
  if (typeof v !== "number" || !Number.isFinite(v)) throw new RangeError(`${owner}: ${name} must be a finite number, got ${v}`);
}

/** Wraps children in the box a layout asks for, under a new local time. */
function Shifted(props: {
  owner: string;
  time: LocalTime;
  base: TimeBase;
  layout: SequenceLayout;
  style?: CSSProperties;
  className?: string;
  label?: string;
  children?: ReactNode;
}) {
  const { owner, time, base, layout, style, className, label, children } = props;
  if (layout !== "absolute-fill" && layout !== "none") {
    throw new RangeError(`${owner}: layout must be "absolute-fill" or "none", got "${layout}"`);
  }
  if (layout === "none" && (style !== undefined || className !== undefined)) {
    throw new Error(`${owner}: style and className need a box, so they can't be used with layout="none"`);
  }
  // layout "none": the children as they are (a Fragment around them was one more element a frame for each Sequence).
  // data-raea-start: the box's absolute start frame, where the engine runs its CSS animations from (so a frame comes
  // out the same rendered alone or after the ones before it)
  const inner =
    layout === "absolute-fill" ? (
      <AbsoluteFill style={style} className={className} data-sequence={label} data-raea-start={time.origin}>
        {children}
      </AbsoluteFill>
    ) : (
      children
    );
  return (
    <TimeBaseContext.Provider value={base}>
      <LocalTimeContext.Provider value={time}>{inner}</LocalTimeContext.Provider>
    </TimeBaseContext.Provider>
  );
}

/** The same LocalTime object as last render when nothing in it changed (a Freeze's subtree, a held frame). */
function useStableTime(frame: number, origin: number, width: number, height: number): LocalTime {
  const r = useRef<LocalTime | null>(null);
  const p = r.current;
  if (p && p.frame === frame && p.origin === origin && p.width === width && p.height === height) return p;
  return (r.current = { frame, origin, width, height });
}

/** Shifts time by `from` and shows its children only for `durationInFrames` frames. */
export function Sequence(props: SequenceProps) {
  const { from = 0, durationInFrames = Infinity, name, layout = "absolute-fill", width, height, style, className } = props;
  checkFinite("Sequence", "from", from);
  checkDuration("Sequence", durationInFrames);
  const parent = useLocalTime("Sequence");
  const pbase = useContext(TimeBaseContext);
  const r = useRef<LocalTime | null>(null);
  const f = parent.frame;
  if (durationInFrames === 0 || f < from || f >= from + durationInFrames) return null;
  // a sound cue (a game's: a Sequence without an end around one generated sound) whose sound has ended: <Audio> would
  // give nothing, by the same rule (media.tsx), so nothing is rendered for it at all. Every cue a game ever played
  // stayed rendered each frame, ~1 KB of garbage a frame apiece (lightspeed Z3: 39 of pong's 48 KB a frame)
  const c = props.children as any;
  if (isValidElement(c) && c.type === Audio) {
    const a = c.props as any;
    if (isGeneratedAsset(a.src) && !a.loop && a.endAt === undefined && a.trimAfter === undefined) {
      const rate = a.playbackRate ?? 1;
      if (rate > 0 && ((a.startFrom ?? a.trimBefore ?? 0) + (f - from) * rate) / useClockState("Sequence").config.fps >= a.src.seconds) return null;
    }
  }
  // (checked without arrays: a game's sound cues are Sequences, hundreds rendered a frame)
  if (width !== undefined && (!Number.isInteger(width) || width <= 0)) throw new RangeError(`Sequence: width must be a positive integer, got ${width}`);
  if (height !== undefined && (!Number.isInteger(height) || height <= 0)) throw new RangeError(`Sequence: height must be a positive integer, got ${height}`);
  const p = r.current;
  const w = width ?? parent.width;
  const h = height ?? parent.height;
  const time: LocalTime = p && p.frame === f - from && p.origin === parent.origin + from && p.width === w && p.height === h ? p : (r.current = { frame: f - from, origin: parent.origin + from, width: w, height: h });
  const base = timeBase(pbase.origin + from, pbase.freeze === null ? null : pbase.freeze - from);
  // no box: the new time around the children here, as Shifted would put it (a game's sound cues, a component less each)
  if (layout === "none" && style === undefined && className === undefined) {
    return (
      <TimeBaseContext.Provider value={base}>
        <LocalTimeContext.Provider value={time}>{props.children}</LocalTimeContext.Provider>
      </TimeBaseContext.Provider>
    );
  }
  return (
    <Shifted owner="Sequence" time={time} base={base} layout={layout} style={style} className={className} label={name}>
      {props.children}
    </Shifted>
  );
}

export interface SeriesSequenceProps extends Omit<SequenceProps, "from" | "layout"> {
  offset?: number;
}

/** A slot in a Series; it only has meaning as a direct child of <Series>. */
function SeriesSequence(_props: SeriesSequenceProps): ReactElement {
  throw new Error("Series.Sequence must be a direct child of <Series>");
}

function flatten(children: ReactNode): ReactNode[] {
  const out: ReactNode[] = [];
  Children.forEach(children, (child) => {
    if (isValidElement(child) && child.type === Fragment) {
      out.push(...flatten((child.props as { children?: ReactNode }).children));
    } else if (child !== null && child !== undefined && typeof child !== "boolean") {
      out.push(child);
    }
  });
  return out;
}

/** Plays its Series.Sequence children back to back, each shifted by its own offset. */
function SeriesImpl({ children }: { children?: ReactNode }) {
  const slots = flatten(children);
  let start = 0;
  const placed: ReactNode[] = [];
  slots.forEach((child, i) => {
    if (!isValidElement(child) || child.type !== SeriesSequence) {
      throw new Error(`Series: child ${i + 1} is not a <Series.Sequence>; only those may go inside a Series`);
    }
    const { offset = 0, durationInFrames, children: slotChildren, ...rest } = child.props as SeriesSequenceProps;
    checkFinite("Series.Sequence", "offset", offset);
    if (durationInFrames === undefined || durationInFrames === Infinity) {
      if (i !== slots.length - 1) {
        throw new RangeError(`Series: only the last Series.Sequence may be unbounded, but child ${i + 1} is`);
      }
    }
    const d = durationInFrames ?? Infinity;
    checkDuration("Series.Sequence", d);
    const from = start + offset;
    placed.push(
      <Sequence key={child.key ?? i} {...rest} from={from} durationInFrames={d} layout="absolute-fill">
        {slotChildren}
      </Sequence>,
    );
    start = from + d;
  });
  return <Fragment>{placed}</Fragment>;
}

export const Series = Object.assign(SeriesImpl, { Sequence: SeriesSequence });

export interface LoopProps {
  durationInFrames: number;
  times?: number;
  name?: string;
  layout?: SequenceLayout;
  style?: CSSProperties;
  className?: string;
  children?: ReactNode;
}

/** Repeats its children every `durationInFrames` frames, `times` times (forever by default). */
export function Loop(props: LoopProps) {
  const { durationInFrames, times = Infinity, name, layout = "absolute-fill", style, className } = props;
  if (!Number.isInteger(durationInFrames) || durationInFrames <= 0) {
    throw new RangeError(`Loop: durationInFrames must be a positive integer, got ${durationInFrames}`);
  }
  if (times !== Infinity && (!Number.isInteger(times) || times < 0)) {
    throw new RangeError(`Loop: times must be a non-negative integer or Infinity, got ${times}`);
  }
  const parent = useLocalTime("Loop");
  const pbase = useContext(TimeBaseContext);
  const r = useRef<LocalTime | null>(null);
  const f = parent.frame;
  if (f < 0 || f >= times * durationInFrames) return null;
  const pass = Math.floor(f / durationInFrames);
  const lf = f - pass * durationInFrames;
  const lo = parent.origin + pass * durationInFrames;
  const p = r.current;
  const time: LocalTime = p && p.frame === lf && p.origin === lo && p.width === parent.width && p.height === parent.height ? p : (r.current = { frame: lf, origin: lo, width: parent.width, height: parent.height });
  const base = timeBase(pbase.origin + pass * durationInFrames, pbase.freeze === null ? null : pbase.freeze - pass * durationInFrames);
  return (
    <Shifted owner="Loop" time={time} base={base} layout={layout} style={style} className={className} label={name}>
      {props.children}
    </Shifted>
  );
}

export interface FreezeProps {
  frame: number;
  children?: ReactNode;
}

/** Shows its children as they are at `frame`, whatever the parent's frame. */
export function Freeze(props: FreezeProps) {
  checkFinite("Freeze", "frame", props.frame);
  const parent = useLocalTime("Freeze");
  const pbase = useContext(TimeBaseContext);
  const time = useStableTime(props.frame, parent.origin, parent.width, parent.height);
  return (
    <TimeBaseContext.Provider value={timeBase(pbase.origin, props.frame)}>
      <LocalTimeContext.Provider value={time}>{props.children}</LocalTimeContext.Provider>
    </TimeBaseContext.Provider>
  );
}
