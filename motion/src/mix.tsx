// <Mix>: how the renderer masters the summed sound of a composition (spec: "The mix"). It draws nothing; it carries
// the target loudness and the true-peak ceiling for the renderer. One per composition.
import { useId } from "react";
import { useClockState, type ClockState } from "./clock";

export interface MixProps {
  /** integrated loudness the mix is brought to (EBU R128, LUFS) */
  lufs?: number;
  /** the limiter's true-peak ceiling (dBTP) */
  ceiling?: number;
}

// Which Mix (by its React id) each rendered composition already has. Keyed by the Clock's state object, so each
// rendered frame starts fresh; the id lets a repeated render of the same Mix through without counting it twice.
const claimed = new WeakMap<ClockState, string>();

export function Mix({ lufs = -14, ceiling = -1.5 }: MixProps) {
  const clock = useClockState("Mix");
  const id = useId();
  for (const [name, v] of [["lufs", lufs], ["ceiling", ceiling]] as const) {
    if (typeof v !== "number" || !Number.isFinite(v)) throw new RangeError(`Mix: ${name} must be a finite number, got ${v}`);
  }
  if (ceiling > 0) throw new RangeError(`Mix: ceiling is in dBTP and must be at most 0, got ${ceiling}`);
  const owner = claimed.get(clock);
  if (owner !== undefined && owner !== id) {
    throw new Error("Mix: a composition has one Mix, and this tree already has one; remove the extra <Mix>");
  }
  claimed.set(clock, id);
  return <div data-mix="true" data-lufs={lufs} data-ceiling={ceiling} style={{ display: "none" }} />;
}
