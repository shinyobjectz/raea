// Transition: how a part comes in and goes out (docs/timeline.feature TL7a). The timeline writes a transition on a
// cut as one <Transition> around what each of the two parts holds: `out` on the part before the cut, `in` on the
// part after it, and a clip's own In and Out animation the same way (TL7d). It plays inside the part's own time (its
// <Sequence>'s first frames for `in`, its last for `out`), so nothing outside the two parts changes and the film is
// as long as it was. The kinds and their styles are edges.ts, which the timeline's preview draws with too.
import type { ReactNode } from "react";
import { useLocalTime } from "@raea/app/clock";
import { COMBO_KINDS, COMBOS, type ComboLoop, comboStyle, edgesStyle, joinStyles, TRANSITION_KINDS, type TransitionEdge } from "./edges";
import { AbsoluteFill } from "./timeline";

export { COMBO_KINDS, COMBOS, type ComboKind, type ComboLoop, comboStyle, type ComboWave, edgesStyle, type EdgeStyle, joinStyles, TRANSITION_KINDS, transitionAt, type TransitionEdge, type TransitionKind, transitionStyle } from "./edges";

export interface TransitionProps {
  /** over the part's first frames */
  in?: TransitionEdge;
  /** over the part's last frames */
  out?: TransitionEdge;
  /** a Combo over the whole part (TL7e): a cycle every `frames`, from its first frame */
  combo?: ComboLoop;
  /** the part it brings in and takes out: what the <Sequence> holds */
  children?: ReactNode;
}

/** Its children, coming in over the part's first frames, going out over its last, and moving by a Combo throughout. */
export function Transition({ in: enter, out: exit, combo, children }: TransitionProps) {
  const t = useLocalTime("Transition");
  for (const [side, edge] of [["in", enter], ["out", exit]] as const) {
    if (!edge) continue;
    if (!TRANSITION_KINDS.includes(edge.kind)) throw new RangeError(`Transition: ${side}.kind is one of ${TRANSITION_KINDS.join(", ")}, got ${String(edge.kind)}`);
    if (!Number.isFinite(edge.frames) || edge.frames < 0) throw new RangeError(`Transition: ${side}.frames must be 0 or more, got ${edge.frames}`);
  }
  if (combo && !COMBO_KINDS.includes(combo.kind)) throw new RangeError(`Transition: combo.kind is one of ${COMBO_KINDS.join(", ")}, got ${String(combo.kind)}`);
  if (combo && (!Number.isFinite(combo.frames) || combo.frames < 0)) throw new RangeError(`Transition: combo.frames must be 0 or more, got ${combo.frames}`);
  const style = joinStyles(edgesStyle(enter, exit, t.frame, t.duration), combo ? comboStyle(COMBOS.find((c) => c.kind === combo.kind), t.frame, combo.frames) : {});
  if (!style.transform && style.opacity === undefined && !style.clipPath) return <>{children}</>;
  return <AbsoluteFill style={style}>{children}</AbsoluteFill>;
}
