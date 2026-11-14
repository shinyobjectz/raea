// Transition: how a part comes in and goes out (docs/timeline.feature TL7a). The timeline writes a transition on a
// cut as one <Transition> around what each of the two parts holds: `out` on the part before the cut, `in` on the
// part after it, and a clip's own In and Out animation the same way (TL7d). It plays inside the part's own time (its
// <Sequence>'s first frames for `in`, its last for `out`), so nothing outside the two parts changes and the film is
// as long as it was. The kinds and their styles are edges.ts, which the timeline's preview draws with too.
import type { ReactNode } from "react";
import { useLocalTime } from "@raea/app/clock";
import { edgesStyle, TRANSITION_KINDS, type TransitionEdge } from "./edges";
import { AbsoluteFill } from "./timeline";

export { edgesStyle, type EdgeStyle, TRANSITION_KINDS, transitionAt, type TransitionEdge, type TransitionKind, transitionStyle } from "./edges";

export interface TransitionProps {
  /** over the part's first frames */
  in?: TransitionEdge;
  /** over the part's last frames */
  out?: TransitionEdge;
  /** the part it brings in and takes out: what the <Sequence> holds */
  children?: ReactNode;
}

/** Its children, coming in over the part's first frames and going out over its last. */
export function Transition({ in: enter, out: exit, children }: TransitionProps) {
  const t = useLocalTime("Transition");
  for (const [side, edge] of [["in", enter], ["out", exit]] as const) {
    if (!edge) continue;
    if (!TRANSITION_KINDS.includes(edge.kind)) throw new RangeError(`Transition: ${side}.kind is one of ${TRANSITION_KINDS.join(", ")}, got ${String(edge.kind)}`);
    if (!Number.isFinite(edge.frames) || edge.frames < 0) throw new RangeError(`Transition: ${side}.frames must be 0 or more, got ${edge.frames}`);
  }
  const style = edgesStyle(enter, exit, t.frame, t.duration);
  if (!style.transform && style.opacity === undefined && !style.clipPath) return <>{children}</>;
  return <AbsoluteFill style={style}>{children}</AbsoluteFill>;
}
