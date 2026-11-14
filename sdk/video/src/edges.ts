// How a part comes in and goes out, as data and style with no React (docs/timeline.feature TL7a, TL7d): one table,
// so <Transition> in a composition and the timeline's preview of a clip's In and Out on a film draw the same thing.

/** fade: through black. slide: in from the right, out to the left. wipe: uncovered from the left, covered from the
 *  left. zoom: grows in from 85%, shrinks out to 85%, fading. A dissolve between two parts that only touch has no
 *  frames of the other to mix with, so it is written as a fade. */
export type TransitionKind = "fade" | "slide" | "wipe" | "zoom";
export const TRANSITION_KINDS: TransitionKind[] = ["fade", "slide", "wipe", "zoom"];

export interface TransitionEdge {
  /** which transition: fade, slide, wipe or zoom */
  kind: TransitionKind;
  /** how many of the part's frames it takes */
  frames: number;
}

/** What an edge does to the part at a frame: a move, a fade and a cut-away, as CSS writes them. */
export interface EdgeStyle {
  /** a move or a scale */
  transform?: string;
  /** how much of it shows, 0 to 1 */
  opacity?: number;
  /** how much of it is covered */
  clipPath?: string;
}

/** How far a part is into view: 0 not at all, 1 fully. `frame` is the part's own, `duration` its length. */
export function transitionAt(edge: TransitionEdge | undefined, side: "in" | "out", frame: number, duration: number): number {
  if (!edge || !(edge.frames > 0)) return 1;
  const p = side === "in" ? (frame + 1) / edge.frames : (duration - frame) / edge.frames;
  return Math.min(1, Math.max(0, p));
}

/** The style a kind gives at `p` (0 out of view, 1 in view), on the side it plays. */
export function transitionStyle(kind: TransitionKind, side: "in" | "out", p: number): EdgeStyle {
  if (p >= 1) return {};
  // eased out, so it settles
  const e = 1 - (1 - p) * (1 - p);
  if (kind === "slide") return { transform: `translateX(${(side === "in" ? 1 - e : e - 1) * 100}%)` };
  if (kind === "wipe") return { clipPath: side === "in" ? `inset(0 ${(1 - e) * 100}% 0 0)` : `inset(0 0 0 ${(1 - e) * 100}%)` };
  if (kind === "zoom") return { transform: `scale(${0.85 + 0.15 * e})`, opacity: e };
  return { opacity: p };
}

/** A part's style at its own `frame` of `duration`, coming in by `enter` and going out by `exit`: both at once (a part
 *  shorter than its two edges) puts the moves side by side and multiplies the fades. Empty when neither plays. */
export function edgesStyle(enter: TransitionEdge | undefined, exit: TransitionEdge | undefined, frame: number, duration: number): EdgeStyle {
  const a = enter ? transitionStyle(enter.kind, "in", transitionAt(enter, "in", frame, duration)) : {};
  const b = exit ? transitionStyle(exit.kind, "out", transitionAt(exit, "out", frame, duration)) : {};
  const transform = [a.transform, b.transform].filter(Boolean).join(" ") || undefined;
  const opacity = a.opacity === undefined && b.opacity === undefined ? undefined : Number(a.opacity ?? 1) * Number(b.opacity ?? 1);
  const clipPath = a.clipPath ?? b.clipPath;
  return { ...(transform ? { transform } : {}), ...(opacity !== undefined ? { opacity } : {}), ...(clipPath ? { clipPath } : {}) };
}
