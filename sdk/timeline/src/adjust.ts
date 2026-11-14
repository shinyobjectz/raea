// A clip's own Adjust (docs/timeline.feature TL7g; CapCut's Adjust tab) as its props: one a kind (brightness is
// adjustBrightness), 0 as shot, each keyed as any number is (TL5a). The preview's look and a composition's
// <Nudge adjust> are drawn by sdk/video adjust.ts, one SVG filter and a vignette layer.
import { ADJUST_KINDS, ADJUSTS, adjustError as error, adjustId, adjustSteps, vignetteStyle, type Adjust, type AdjustKind, type FilterStep } from "@raea/video/adjust"
import type { Value } from "./model"

export { ADJUST_KINDS, ADJUSTS, type Adjust, type AdjustKind, type FilterStep }

type Props = Record<string, Value | boolean>

/** Each adjustment as a clip's prop: brightness is adjustBrightness. */
export const ADJUST_PROPS = Object.fromEntries(ADJUST_KINDS.map((k) => [k, `adjust${k[0].toUpperCase()}${k.slice(1)}`])) as Record<AdjustKind, string>
/** The clip props an adjustment is held in. */
export const ADJUST_PROP_LIST: string[] = Object.values(ADJUST_PROPS)

/** A clip's adjustment read from its props: the kinds that aren't 0. */
export function adjustOf(props: Props = {}): Adjust {
  return Object.fromEntries(ADJUST_KINDS.flatMap((k) => {
    const v = props[ADJUST_PROPS[k]]
    return typeof v === "number" && Number.isFinite(v) && v !== 0 ? [[k, v]] : []
  })) as Adjust
}

/** An adjustment as a clip's props, to three places: every kind, 0 for what it hasn't got, so a set puts it whole. */
export function adjustProps(a: Adjust): Props {
  return Object.fromEntries(ADJUST_KINDS.map((k) => [ADJUST_PROPS[k], Math.round((a[k] ?? 0) * 1000) / 1000]))
}

/** What is wrong with a clip's adjustment props as they stand, or undefined: a value out of its range. */
export function adjustError(props: Props = {}): string | undefined {
  for (const k of ADJUST_KINDS) {
    const v = props[ADJUST_PROPS[k]]
    if (v !== undefined && typeof v !== "number") return `an adjust's ${k} is a number, got ${JSON.stringify(v)}`
  }
  return error(adjustOf(props))
}

/** How the editor's preview draws a clip's adjustment on its picture, as <Nudge adjust> draws it: its SVG filter's
 *  id and steps (to put beside the picture), and its vignette's background (a layer over the picture). A wrong one
 *  draws nothing rather than throwing in the editor. */
export function adjustLook(props: Props | undefined, seed = 0): { filter?: { id: string; steps: FilterStep[] }; vignette?: string } {
  const a = adjustOf(props)
  if (error(a)) return {}
  const steps = adjustSteps(a, seed)
  const v = vignetteStyle(a.vignette ?? 0)
  return { ...(steps.length && { filter: { id: adjustId(a, seed), steps } }), ...(v && { vignette: v.background }) }
}
