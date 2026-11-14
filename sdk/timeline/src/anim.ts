// A clip's own animations as the picture shows them (docs/timeline.feature TL7d, TL7e): the In and Out edges and the
// Combo a composition's <Transition> draws (sdk/video edges.ts), so the timeline's preview of a film matches the render.
import { COMBOS, type ComboKind, comboStyle, edgesStyle, type EdgeStyle, joinStyles, TRANSITION_KINDS, type TransitionKind } from "@raea/video/edges"
import type { Anim, Clip, Frames } from "./model"

export { COMBO_KINDS, COMBOS, type ComboKind, type ComboWave, type EdgeStyle } from "@raea/video/edges"

/** raea's own kind, or a fade for a plugin's, which only its own code draws. */
const kindOf = (k: string): TransitionKind => (TRANSITION_KINDS.includes(k as TransitionKind) ? (k as TransitionKind) : "fade")

/** The clip's style `into` frames from its start: coming in by its In, going out by its Out (each over no more of it
 *  than it has) and moving by its Combo, one of `combos` (a plugin's among them). Empty when none plays there. */
export function animStyle(c: Clip, into: Frames, combos: ComboKind[] = COMBOS): EdgeStyle {
  const edge = (a?: Anim) => (a ? { kind: kindOf(a.kind), frames: Math.min(a.length, c.length) } : undefined)
  const combo = c.combo ? comboStyle(combos.find((k) => k.kind === c.combo!.kind), into, c.combo.length) : {}
  return joinStyles(edgesStyle(edge(c.enter), edge(c.exit), into, c.length), combo)
}
