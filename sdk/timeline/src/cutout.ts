// A clip's mask, blend and chroma key (docs/timeline.feature TL7f: CapCut's Mask, Basic's Blend, Cutout's Chroma
// key) as its props, a prop a value as a film's crop is a prop a side, so a preset sets them, the editor's preview
// draws them and a composition's <Nudge mask blend chroma> holds them, drawn by sdk/video cutout.ts in both.
// Taken off as crop and mirror are, by their plain value: the mask "none", the blend "normal", the key colour "".
import { BLEND_KINDS, BLENDS, chromaError, chromaId, chromaMatrix, MASK_KINDS, maskError, maskPlain, maskStyle, type Blend, type ChromaKey, type Mask, type MaskKind } from "@raea/video/cutout"
import type { Value } from "./model"

export { BLEND_KINDS, BLENDS, MASK_KINDS, maskPlain, type Blend, type ChromaKey, type Mask, type MaskKind }

/** A mask's values as a clip's props. */
export const MASK_PROPS = { kind: "mask", x: "maskX", y: "maskY", w: "maskW", h: "maskH", rotate: "maskRotate", feather: "maskFeather", round: "maskRound", invert: "maskInvert" } as const
/** A chroma key's values as a clip's props. */
export const KEY_PROPS = { color: "keyColor", strength: "keyStrength", shadow: "keyShadow" } as const
/** Every prop a clip's cutout is held in. */
export const CUTOUT_PROPS: string[] = [...Object.values(MASK_PROPS), "blend", ...Object.values(KEY_PROPS)]

/** A clip's mask, blend and key, each there only when it has one. */
export interface Cutout {
  /** what of it is seen: a shape in shares of the frame */
  mask?: Mask
  /** how it is drawn over what is under it */
  blend?: Blend
  /** a colour taken out of it */
  chroma?: ChromaKey
}

type Props = Record<string, Value | boolean>
const NUMS = ["x", "y", "w", "h", "rotate", "feather", "round"] as const

/** A clip's cutout read from its props: a mask when its kind is one, a blend other than normal, a key when it has a
 *  colour. */
export function cutoutOf(props: Props = {}): Cutout {
  const num = (k: string) => (typeof props[k] === "number" && Number.isFinite(props[k]) ? (props[k] as number) : undefined)
  const kind = props[MASK_PROPS.kind]
  const mask: Mask | undefined =
    typeof kind === "string" && (MASK_KINDS as readonly string[]).includes(kind)
      ? { kind: kind as MaskKind, ...Object.fromEntries(NUMS.flatMap((k) => (num(MASK_PROPS[k]) === undefined ? [] : [[k, num(MASK_PROPS[k])]]))), ...(props[MASK_PROPS.invert] === true && { invert: true }) }
      : undefined
  const blend = typeof props.blend === "string" && props.blend !== "normal" && (BLEND_KINDS as string[]).includes(props.blend) ? (props.blend as Blend) : undefined
  const color = props[KEY_PROPS.color]
  const chroma: ChromaKey | undefined =
    typeof color === "string" && color
      ? { color, ...(num(KEY_PROPS.strength) !== undefined && { strength: num(KEY_PROPS.strength) }), ...(num(KEY_PROPS.shadow) !== undefined && { shadow: num(KEY_PROPS.shadow) }) }
      : undefined
  return { ...(mask && { mask }), ...(blend && { blend }), ...(chroma && { chroma }) }
}

/** A cutout as a clip's props: every one of them, what it hasn't got at its plain value, so a set takes off the
 *  rest. Values to five places. */
export function cutoutProps(c: Cutout): Props {
  const r = (n: number) => Math.round(n * 100000) / 100000
  const m = c.mask
  return {
    [MASK_PROPS.kind]: m?.kind ?? "none",
    ...(m ? Object.fromEntries(NUMS.flatMap((k) => (m[k] === undefined ? [] : [[MASK_PROPS[k], r(m[k]!)]]))) : {}),
    [MASK_PROPS.invert]: !!m?.invert,
    blend: c.blend ?? "normal",
    [KEY_PROPS.color]: c.chroma?.color ?? "",
    ...(c.chroma?.strength !== undefined && { [KEY_PROPS.strength]: r(c.chroma.strength) }),
    ...(c.chroma?.shadow !== undefined && { [KEY_PROPS.shadow]: r(c.chroma.shadow) }),
  }
}

/** What is wrong with a clip's cutout props as they stand, or undefined: a mask kind or blend it hasn't got, a value
 *  out of range, a grey key. */
export function cutoutError(props: Props = {}): string | undefined {
  const kind = props[MASK_PROPS.kind]
  if (kind !== undefined && kind !== "none" && !(MASK_KINDS as readonly unknown[]).includes(kind)) return `a mask is one of ${MASK_KINDS.join(", ")}, or none: "${String(kind)}" isn't one`
  if (props.blend !== undefined && !(BLEND_KINDS as unknown[]).includes(props.blend)) return `a blend is one of ${BLEND_KINDS.join(", ")}: "${String(props.blend)}" isn't one`
  const c = cutoutOf(props)
  return (c.mask && maskError(c.mask)) || (c.chroma && chromaError(c.chroma)) || undefined
}

/** How the editor's preview draws a clip's cutout on its picture, as <Nudge> draws it in a composition: CSS for the
 *  picture, and the SVG filter its key needs (an feColorMatrix's values under `id`, to put beside the picture). */
export function cutoutLook(props: Props | undefined, width: number, height: number): { style: Record<string, string>; key?: { id: string; matrix: string } } {
  const c = cutoutOf(props)
  if (cutoutError(props)) return { style: {} }
  const key = c.chroma && { id: chromaId(c.chroma), matrix: chromaMatrix(c.chroma) }
  return {
    style: { ...(c.mask ? (maskStyle(c.mask, width, height) as Record<string, string>) : {}), ...(c.blend && { mixBlendMode: c.blend }), ...(key && { filter: `url(#${key.id})` }) },
    ...(key && { key }),
  }
}
