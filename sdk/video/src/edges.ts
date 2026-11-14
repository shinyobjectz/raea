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

/** Styles drawn at once: the moves side by side in order, the fades multiplied, the first cover. */
export function joinStyles(...styles: EdgeStyle[]): EdgeStyle {
  const transform = styles.map((s) => s.transform).filter(Boolean).join(" ") || undefined;
  const fades = styles.filter((s) => s.opacity !== undefined);
  const opacity = fades.length ? fades.reduce((p, s) => p * Number(s.opacity), 1) : undefined;
  const clipPath = styles.find((s) => s.clipPath)?.clipPath;
  return { ...(transform ? { transform } : {}), ...(opacity !== undefined ? { opacity } : {}), ...(clipPath ? { clipPath } : {}) };
}

/** A part's style at its own `frame` of `duration`, coming in by `enter` and going out by `exit`: both at once (a part
 *  shorter than its two edges) puts the moves side by side and multiplies the fades. Empty when neither plays. */
export function edgesStyle(enter: TransitionEdge | undefined, exit: TransitionEdge | undefined, frame: number, duration: number): EdgeStyle {
  const a = enter ? transitionStyle(enter.kind, "in", transitionAt(enter, "in", frame, duration)) : {};
  const b = exit ? transitionStyle(exit.kind, "out", transitionAt(exit, "out", frame, duration)) : {};
  return joinStyles(a, b);
}

/** How a part moves over one cycle of a Combo (CapCut's third kind of animation, docs/timeline.feature TL7e): each
 *  property's values, evenly spaced over the cycle and eased between, the cycle starting again where it ends. x and y
 *  move it by a share of its own size in percent, rotate turns it in degrees, scale sizes it, opacity fades it. A wave
 *  that ends where it starts plays once over the part as CapCut's Combo does, and loops seamlessly as its text Loop does. */
export interface ComboWave {
  /** across, in percent of its width */
  x?: number[];
  /** down, in percent of its height */
  y?: number[];
  /** its size, 1 as it is */
  scale?: number[];
  /** turned, in degrees */
  rotate?: number[];
  /** how much of it shows, 0 to 1 */
  opacity?: number[];
}

/** A kind of Combo: its name, what the person sees, and its wave. */
export interface ComboKind {
  /** its name, as code writes it */
  kind: string;
  /** what the person sees */
  title: string;
  /** how it moves over one cycle */
  wave: ComboWave;
  /** straight from value to value, not eased: a spin that keeps turning */
  linear?: boolean;
}

/** raea's own Combos: the kinds <Transition combo> draws. A plugin adds its own as data the same way. */
export const COMBOS: ComboKind[] = [
  { kind: "pulse", title: "Pulse", wave: { scale: [1, 1.08, 1] } },
  { kind: "sway", title: "Sway", wave: { rotate: [0, 4, 0, -4, 0] } },
  { kind: "float", title: "Float", wave: { y: [0, -3, 0] } },
  { kind: "shake", title: "Shake", wave: { x: [0, 1.2, -1, 0.8, -1.2, 0], y: [0, -0.8, 1, -0.6, 0.5, 0] } },
  { kind: "swing", title: "Swing", wave: { rotate: [0, 8, -6, 4, -2, 0], scale: [1, 1.04, 1, 1.02, 1, 1] } },
  { kind: "spin", title: "Spin", wave: { rotate: [0, 360] }, linear: true },
  { kind: "flicker", title: "Flicker", wave: { opacity: [1, 0.35, 1, 1, 0.55, 1] } },
];
export const COMBO_KINDS: string[] = COMBOS.map((c) => c.kind);

/** A Combo as a part plays it: which kind, and how many frames one cycle takes (the part's length plays it once). */
export interface ComboLoop {
  /** which Combo: one of COMBO_KINDS */
  kind: string;
  /** the frames one cycle takes */
  frames: number;
}

const three = (v: number) => Math.round(v * 1000) / 1000;

/** A Combo's style at the part's own `frame`, a cycle every `frames`. */
export function comboStyle(combo: ComboKind | undefined, frame: number, frames: number): EdgeStyle {
  if (!combo || !(frames >= 1)) return {};
  const p = (((frame % frames) + frames) % frames) / frames;
  const at = (vs: number[] | undefined): number | undefined => {
    if (!vs?.length) return undefined;
    if (vs.length === 1) return vs[0];
    const x = p * (vs.length - 1), n = Math.min(vs.length - 2, Math.floor(x)), f = x - n;
    const e = combo.linear ? f : f * f * (3 - 2 * f);
    return vs[n] + (vs[n + 1] - vs[n]) * e;
  };
  const w = combo.wave, x = at(w.x), y = at(w.y), r = at(w.rotate), s = at(w.scale), o = at(w.opacity);
  const moves = [
    x !== undefined || y !== undefined ? `translate(${three(x ?? 0)}%, ${three(y ?? 0)}%)` : "",
    r !== undefined ? `rotate(${three(r)}deg)` : "",
    s !== undefined ? `scale(${three(s)})` : "",
  ].filter(Boolean);
  return { ...(moves.length ? { transform: moves.join(" ") } : {}), ...(o !== undefined ? { opacity: three(Math.min(1, Math.max(0, o))) } : {}) };
}
