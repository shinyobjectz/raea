// AbsoluteFill, Sequence, Series, Loop and Freeze (spec: "Sequence shifts time and limits it").
import { Children, Fragment, isValidElement, type CSSProperties, type ReactElement, type ReactNode } from "react";
import { LocalTimeContext, useLocalTime, type LocalTime } from "./clock";

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
  layout: SequenceLayout;
  style?: CSSProperties;
  className?: string;
  label?: string;
  children?: ReactNode;
}) {
  const { owner, time, layout, style, className, label, children } = props;
  if (layout !== "absolute-fill" && layout !== "none") {
    throw new RangeError(`${owner}: layout must be "absolute-fill" or "none", got "${layout}"`);
  }
  if (layout === "none" && (style !== undefined || className !== undefined)) {
    throw new Error(`${owner}: style and className need a box, so they can't be used with layout="none"`);
  }
  // layout "none": the children as they are (a Fragment around them was one more element a frame for each Sequence)
  const inner =
    layout === "absolute-fill" ? (
      <AbsoluteFill style={style} className={className} data-sequence={label}>
        {children}
      </AbsoluteFill>
    ) : (
      children
    );
  return <LocalTimeContext.Provider value={time}>{inner}</LocalTimeContext.Provider>;
}

/** Shifts time by `from` and shows its children only for `durationInFrames` frames. */
export function Sequence(props: SequenceProps) {
  const { from = 0, durationInFrames = Infinity, name, layout = "absolute-fill", width, height, style, className } = props;
  checkFinite("Sequence", "from", from);
  checkDuration("Sequence", durationInFrames);
  const parent = useLocalTime("Sequence");
  const f = parent.frame;
  if (durationInFrames === 0 || f < from || f >= from + durationInFrames) return null;
  // (checked without arrays: a game's sound cues are Sequences, hundreds rendered a frame)
  if (width !== undefined && (!Number.isInteger(width) || width <= 0)) throw new RangeError(`Sequence: width must be a positive integer, got ${width}`);
  if (height !== undefined && (!Number.isInteger(height) || height <= 0)) throw new RangeError(`Sequence: height must be a positive integer, got ${height}`);
  const time: LocalTime = {
    frame: f - from,
    origin: parent.origin + from,
    width: width ?? parent.width,
    height: height ?? parent.height,
  };
  return (
    <Shifted owner="Sequence" time={time} layout={layout} style={style} className={className} label={name}>
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
  const f = parent.frame;
  if (f < 0 || f >= times * durationInFrames) return null;
  const pass = Math.floor(f / durationInFrames);
  const time: LocalTime = {
    ...parent,
    frame: f - pass * durationInFrames,
    origin: parent.origin + pass * durationInFrames,
  };
  return (
    <Shifted owner="Loop" time={time} layout={layout} style={style} className={className} label={name}>
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
  return (
    <LocalTimeContext.Provider value={{ ...parent, frame: props.frame }}>{props.children}</LocalTimeContext.Provider>
  );
}
