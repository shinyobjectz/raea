// A keyed property's value at a frame (docs/timeline.feature TL5): between two keys it follows the first key's ease,
// before the first and after the last it holds. An ease is a cubic bezier's two handles, as CSS writes them.
import type { Ease, Frames, Key, Value } from "./model"

/** The easing presets raea registers (TL5c); a plugin adds its own beside them (TL11a). */
export const EASES: Record<string, Ease> = {
  linear: "linear",
  hold: "hold",
  "ease in": [0.42, 0, 1, 1],
  "ease out": [0, 0, 0.58, 1],
  "ease in-out": [0.42, 0, 0.58, 1],
}

/** y at x on the cubic bezier (0,0) (x1,y1) (x2,y2) (1,1): x is solved by Newton's steps, then bisection where the
 *  slope is too flat for them. The handles' x are kept in 0..1 so the curve is a function of time. */
export function bezier(x1: number, y1: number, x2: number, y2: number): (x: number) => number {
  const a = Math.min(1, Math.max(0, x1)), b = Math.min(1, Math.max(0, x2))
  const at = (p1: number, p2: number, t: number) => 3 * p1 * t * (1 - t) * (1 - t) + 3 * p2 * t * t * (1 - t) + t * t * t
  const slope = (p1: number, p2: number, t: number) => 3 * p1 * (1 - t) * (1 - 3 * t) + 3 * p2 * t * (2 - 3 * t) + 3 * t * t
  return (x) => {
    if (x <= 0) return 0
    if (x >= 1) return 1
    let t = x
    for (let i = 0; i < 8; i++) {
      const d = slope(a, b, t)
      if (Math.abs(d) < 1e-6) break
      const next = t - (at(a, b, t) - x) / d
      if (Math.abs(next - t) < 1e-7) {
        t = next
        break
      }
      t = next
    }
    if (!(t >= 0 && t <= 1) || Math.abs(at(a, b, t) - x) > 1e-5) {
      let lo = 0, hi = 1
      for (let i = 0; i < 40; i++) {
        t = (lo + hi) / 2
        if (at(a, b, t) < x) lo = t
        else hi = t
      }
    }
    return at(y1, y2, t)
  }
}

/** How far along (0..1, or past it for a handle that overshoots) a value is at `p` of the way between two keys. */
export function eased(ease: Ease | undefined, p: number): number {
  if (ease === "hold") return p >= 1 ? 1 : 0
  if (!ease || ease === "linear") return Math.min(1, Math.max(0, p))
  return bezier(...ease)(p)
}

const mix = (a: Value, b: Value, k: number): Value => {
  if (typeof a === "number" && typeof b === "number") return a + (b - a) * k
  if (Array.isArray(a) && Array.isArray(b) && a.length === b.length) return a.map((v, i) => v + (b[i] - v) * k)
  // what can't be mixed (words, lists of different lengths) holds, then jumps
  return k >= 1 ? b : a
}

/** The keys in time order (two at one frame: the later one written wins). */
export const sorted = (keys: Key[]): Key[] => {
  const by = new Map<Frames, Key>()
  for (const k of keys) by.set(k.at, k)
  return [...by.values()].sort((a, b) => a.at - b.at)
}

/** The property's value at `frame` (frames from its clip's start), or undefined with no keys. */
export function valueAt(keys: Key[] | undefined, frame: Frames): Value | undefined {
  if (!keys?.length) return undefined
  const ks = sorted(keys)
  if (frame <= ks[0].at) return ks[0].value
  const last = ks[ks.length - 1]
  if (frame >= last.at) return last.value
  const i = ks.findIndex((k) => k.at > frame) - 1
  const a = ks[i], b = ks[i + 1]
  return mix(a.value, b.value, eased(a.ease, (frame - a.at) / (b.at - a.at)))
}

/** A clip's props as they stand `frame` frames from its start: each keyed one at its value there, the rest as set
 *  (what a mask's, a key's or a crop's look is drawn from while its numbers are keyed, TL5a). */
export function propsAt(clip: { props?: Record<string, Value | boolean>; keys?: Record<string, Key[]> }, frame: Frames): Record<string, Value | boolean> {
  const keyed = Object.entries(clip.keys ?? {}).flatMap(([p, ks]) => {
    const v = valueAt(ks, frame)
    return v === undefined ? [] : [[p, v] as const]
  })
  return { ...clip.props, ...Object.fromEntries(keyed) }
}

/** The keys either side of a cut at `frame` (TL3d: a split clip's keys stay at their frames): the left clip's, and
 *  the right clip's in its own frames. A cut between two keys puts a key of the value there on both sides, so each
 *  half moves as the whole did up to the cut. */
export function cut(keys: Key[], frame: Frames): [Key[], Key[]] {
  const ks = sorted(keys)
  const left = ks.filter((k) => k.at < frame)
  const right = ks.filter((k) => k.at >= frame).map((k) => ({ ...k, at: k.at - frame }))
  const between = left.length > 0 && ks.some((k) => k.at > frame) && !ks.some((k) => k.at === frame)
  if (between) {
    const value = valueAt(ks, frame)!
    left.push({ at: frame, value })
    right.unshift({ at: 0, value, ease: left[left.length - 2].ease })
  }
  return [left, right]
}
