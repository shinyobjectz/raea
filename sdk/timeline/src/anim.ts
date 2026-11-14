// A clip's own In and Out animations as the picture shows them (docs/timeline.feature TL7d): the edges a
// composition's <Transition> draws (sdk/video edges.ts), so the timeline's preview of a film matches the render.
import { edgesStyle, type EdgeStyle, TRANSITION_KINDS, type TransitionKind } from "@raea/video/edges"
import type { Anim, Clip, Frames } from "./model"

export type { EdgeStyle } from "@raea/video/edges"

/** raea's own kind, or a fade for a plugin's, which only its own code draws. */
const kindOf = (k: string): TransitionKind => (TRANSITION_KINDS.includes(k as TransitionKind) ? (k as TransitionKind) : "fade")

/** The clip's style `into` frames from its start, coming in by its In and going out by its Out, each over no more
 *  of it than it has. Empty when neither plays there. */
export function animStyle(c: Clip, into: Frames): EdgeStyle {
  const edge = (a?: Anim) => (a ? { kind: kindOf(a.kind), frames: Math.min(a.length, c.length) } : undefined)
  return edgesStyle(edge(c.enter), edge(c.exit), into, c.length)
}
