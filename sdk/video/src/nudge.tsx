// Nudge: a part placed by hand on raea's picture (docs/editing.feature, E1). The editor writes a moved, scaled, turned
// or faded part back to the composition as one <Nudge x y scale rotate opacity> around the element that holds it (the
// timeline also writes mirror and crop, docs/timeline.feature TL3h, and a mask, a blend and a chroma key, TL7f); the
// next change to the same part re-writes that line.
import { cloneElement, createElement, isValidElement, type CSSProperties, type ReactElement, type ReactNode } from "react";
import { useCurrentFrame, useVideoConfig } from "@raea/app/clock";
import { Easing } from "@raea/math/easing";
import { ADJUST_KINDS, ADJUSTS, adjustError, adjustId, adjustSteps, vignetteStyle, type Adjust, type FilterStep } from "./adjust";
import { BLEND_KINDS, chromaError, chromaId, chromaMatrix, maskStyle, type Blend, type ChromaKey, type Mask } from "./cutout";
import { AbsoluteFill } from "./timeline";

export { ADJUSTS, type Adjust, type AdjustKind } from "./adjust";
export { BLENDS, MASK_KINDS, type Blend, type ChromaKey, type Mask, type MaskKind } from "./cutout";

/** A property's value at a frame, and how it leaves for the next key: a straight line (the default), no travel
 *  ("hold": it stays, then jumps), or a cubic bezier's two handles [x1, y1, x2, y2] as CSS writes them. The timeline
 *  writes these (docs/timeline.feature TL5d). */
export type NudgeKey = [frame: number, value: number, ease?: "linear" | "hold" | [number, number, number, number]];
/** What a <Nudge> keys: its placement, and the numbers inside its mask, its chroma key and its crop, by a dotted
 *  name ("mask.feather" is the mask's feather; a mask or key it keys must be given, as the still kind and colour). */
export const NUDGE_KEYED = [
  "x", "y", "scale", "rotate", "opacity",
  "mask.x", "mask.y", "mask.w", "mask.h", "mask.rotate", "mask.feather", "mask.round",
  "chroma.strength", "chroma.shadow",
  "crop.left", "crop.top", "crop.right", "crop.bottom",
  ...ADJUST_KINDS.map((k) => `adjust.${k}` as const),
] as const;
export type NudgeProp = (typeof NUDGE_KEYED)[number];
const CROP_SIDES = ["left", "top", "right", "bottom"] as const;
const KEY_RANGE: Partial<Record<NudgeProp, [number, number]>> = {
  opacity: [0, 1], "mask.w": [0, Infinity], "mask.h": [0, Infinity], "mask.feather": [0, 1], "mask.round": [0, 1],
  "chroma.strength": [0, 1], "chroma.shadow": [0, 1], "crop.left": [0, 0.9], "crop.top": [0, 0.9], "crop.right": [0, 0.9], "crop.bottom": [0, 0.9],
  ...Object.fromEntries(ADJUSTS.map((a) => [`adjust.${a.kind}`, [a.min, 1]])),
};

/** A keyed property's value at `frame`: before its first key and after its last it holds; between two it follows
 *  the first one's ease. */
export function keyedAt(keys: NudgeKey[], frame: number): number {
  const ks = [...keys].sort((a, b) => a[0] - b[0]);
  if (frame <= ks[0][0]) return ks[0][1];
  const last = ks[ks.length - 1];
  if (frame >= last[0]) return last[1];
  const i = ks.findIndex((k) => k[0] > frame) - 1;
  const [f0, v0, ease] = ks[i];
  const [f1, v1] = ks[i + 1];
  const p = (frame - f0) / (f1 - f0);
  const k = ease === "hold" ? 0 : Array.isArray(ease) ? Easing.bezier(...ease)(p) : p;
  return v0 + (v1 - v0) * k;
}

export interface NudgeProps {
  /** how far to move it, in the composition's pixels */
  x?: number;
  /** how far to move it down, in the composition's pixels */
  y?: number;
  /** how much bigger (2) or smaller (0.5), about its centre */
  scale?: number;
  /** a turn, in degrees, about its centre */
  rotate?: number;
  /** 0 (gone) to 1 (as it was), times its own opacity */
  opacity?: number;
  /** flipped left to right */
  mirror?: boolean;
  /** how much is cut off its left, top, right and bottom, each a share of that side from 0 to 0.9 */
  crop?: [left: number, top: number, right: number, bottom: number];
  /** what of it is seen: a shape in shares of the frame (line, mirror, circle, rect, heart, star), turned, feathered
   *  or inverted (sdk/video cutout.ts, TL7f) */
  mask?: Mask;
  /** how it is drawn over what is under it: screen, multiply, overlay and the rest of CSS's blend modes */
  blend?: Blend;
  /** a colour taken out of it, as a green screen is: { color, strength, shadow } */
  chroma?: ChromaKey;
  /** its brightness, contrast, saturation, exposure, sharpen, highlights, shadows, temperature, tint, fade, vignette
   *  and grain, each 0 as shot (sdk/video adjust.ts, TL7g) */
  adjust?: Adjust;
  /** any of the above over time: its keys, each [frame, value, ease?], in frames from where this part's time starts
   *  (its <Sequence>'s first frame), by the names in NUDGE_KEYED ("mask.x", "chroma.strength", "crop.left" for what
   *  is inside a mask, a key or a crop). A keyed property's still value is ignored. */
  keys?: Partial<Record<NudgeProp, NudgeKey[]>>;
  /** the part it places: an element, or a component, which moves inside a full-frame layer */
  children?: ReactNode;
}

/** Places its child: an element gets the move in front of its own transform and the fade times its own opacity; a
 *  component, whose box it can't reach, or a part with a mask, is placed inside a full-frame layer. */
export function Nudge({ keys, ...still }: NudgeProps) {
  // keyed: the frame is read where the keys are (a hook, so in a component of its own), and the values placed as still ones
  return keys && Object.values(keys).some((k) => k?.length) ? <Keyed keys={keys} {...still} /> : <Placed {...still} />;
}

function Keyed({ keys, children, ...still }: NudgeProps & { keys: NonNullable<NudgeProps["keys"]> }) {
  const frame = useCurrentFrame();
  const now: Omit<NudgeProps, "keys" | "children"> = {};
  for (const [prop, ks] of Object.entries(keys) as [NudgeProp, NudgeKey[] | undefined][]) {
    if (!ks?.length) continue;
    if (!NUDGE_KEYED.includes(prop)) throw new RangeError(`Nudge: keys are for ${NUDGE_KEYED.join(", ")}, got ${JSON.stringify(prop)}`);
    // a curve that overshoots stays inside what the value can be
    const [lo, hi] = KEY_RANGE[prop] ?? [-Infinity, Infinity];
    const v = Math.min(hi, Math.max(lo, keyedAt(ks, frame)));
    const [whole, part] = prop.split(".") as [string, string | undefined];
    if (!part) (now as Record<string, number>)[whole] = v;
    else if (whole === "crop") {
      const crop = [...(now.crop ?? still.crop ?? [0, 0, 0, 0])] as [number, number, number, number];
      crop[CROP_SIDES.indexOf(part as (typeof CROP_SIDES)[number])] = v;
      now.crop = crop;
    } else {
      // a mask's or a key's number: the still one holds its kind or colour; an adjustment's start as shot
      const own = (now[whole as "mask" | "chroma" | "adjust"] ?? still[whole as "mask" | "chroma" | "adjust"] ?? (whole === "adjust" ? {} : undefined)) as Record<string, unknown> | undefined;
      if (!own) throw new RangeError(`Nudge: keys for ${prop} need a ${whole} to key`);
      (now as Record<string, unknown>)[whole] = { ...own, [part]: v };
    }
  }
  return <Placed {...still} {...now}>{children}</Placed>;
}

/** The SVG filter a chroma key is drawn by (cutout.ts), beside the part it keys: the key's colour matrix, its alpha
 *  then times the part's own so what was clear stays clear. */
function KeyFilter({ chroma }: { chroma: ChromaKey }) {
  return (
    <svg width={0} height={0} style={{ position: "absolute" }} aria-hidden>
      <defs>
        <filter id={chromaId(chroma)} colorInterpolationFilters="sRGB" x="0" y="0" width="1" height="1">
          <feColorMatrix type="matrix" in="SourceGraphic" result="keyed" values={chromaMatrix(chroma)} />
          <feComposite in="keyed" in2="SourceGraphic" operator="in" />
        </filter>
      </defs>
    </svg>
  );
}

/** An adjustment's SVG filter (adjust.ts), beside the part it adjusts. */
function AdjustFilter({ id, steps }: { id: string; steps: FilterStep[] }) {
  const made = (s: FilterStep, i: number): ReactElement => createElement(s.tag, { key: i, ...s.attrs }, ...(s.children ?? []).map(made));
  return (
    <svg width={0} height={0} style={{ position: "absolute" }} aria-hidden>
      <defs>
        <filter id={id} colorInterpolationFilters="sRGB" x="0" y="0" width="1" height="1">
          {steps.map(made)}
        </filter>
      </defs>
    </svg>
  );
}

function Placed({ x = 0, y = 0, scale = 1, rotate = 0, opacity = 1, mirror = false, crop, mask, blend, chroma, adjust, children }: Omit<NudgeProps, "keys">) {
  const { width, height } = useVideoConfig();
  const frame = useCurrentFrame();
  for (const [k, v] of Object.entries({ x, y, scale, rotate, opacity }))
    if (!Number.isFinite(v)) throw new RangeError(`Nudge: ${k} must be a finite number, got ${v}`);
  if (scale <= 0) throw new RangeError(`Nudge: scale must be above 0, got ${scale}`);
  const fade = Math.min(1, Math.max(0, opacity));
  const move = [
    (x || y) && `translate(${x}px, ${y}px)`,
    rotate && `rotate(${rotate}deg)`,
    (scale !== 1 || mirror) && (mirror ? `scale(${-scale}, ${scale})` : `scale(${scale})`),
  ].filter(Boolean).join(" ") || undefined;
  if (crop !== undefined && (!Array.isArray(crop) || crop.length !== 4 || crop.some((v) => !(v >= 0 && v <= 0.9)))) throw new RangeError(`Nudge: crop is [left, top, right, bottom], each from 0 to 0.9, got ${JSON.stringify(crop)}`);
  // (inset's order is top, right, bottom, left)
  const cut = crop?.some((v) => v > 0) ? `inset(${crop[1] * 100}% ${crop[2] * 100}% ${crop[3] * 100}% ${crop[0] * 100}%)` : undefined;
  if (blend !== undefined && !BLEND_KINDS.includes(blend)) throw new RangeError(`Nudge: blend is one of ${BLEND_KINDS.join(", ")}, got ${JSON.stringify(blend)}`);
  const keyError = chroma && chromaError(chroma);
  if (keyError) throw new RangeError(`Nudge: ${keyError}`);
  const adjustWrong = adjust && adjustError(adjust);
  if (adjustWrong) throw new RangeError(`Nudge: ${adjustWrong}`);
  // the mask, laid over the frame's shape, and the blend: CSS on the part, so they move with it
  const cutout: CSSProperties = { ...(mask ? (maskStyle(mask, width, height) as CSSProperties) : {}), ...(blend && blend !== "normal" ? { mixBlendMode: blend } : {}) };
  // its adjustment's filter (the grain's seed the frame, so it lives) and its vignette
  const steps = adjust ? adjustSteps(adjust, frame) : [];
  const tuned = steps.length ? adjustId(adjust!, frame) : undefined;
  const vignette = adjust?.vignette ? vignetteStyle(adjust.vignette) : undefined;
  // the key first, then the adjustment, then the part's own filter: CapCut keys the screen before it grades
  const own = [chroma && `url(#${chromaId(chroma)})`, tuned && `url(#${tuned})`].filter(Boolean).join(" ") || undefined;
  const placed = (filter?: CSSProperties["filter"]): CSSProperties => ({ ...(cut ? { clipPath: cut } : {}), ...cutout, ...(own ? { filter: filter ? `${own} ${filter}` : own } : {}) });
  const withKey = (el: ReactNode) => (chroma || tuned ? <>{chroma && <KeyFilter chroma={chroma} />}{tuned && <AdjustFilter id={tuned} steps={steps} />}{el}</> : el);
  // a mask is laid over the frame, so a masked part is placed in a full-frame layer, as a component is, and the mask
  // moves with it; on an element's own box it would be squeezed into that box. A vignette is a layer over that frame.
  if (isValidElement(children) && typeof children.type === "string" && !mask && !vignette) {
    const style = ((children.props as { style?: CSSProperties }).style ?? {}) as CSSProperties;
    const own = typeof style.opacity === "number" ? style.opacity : style.opacity === undefined ? 1 : Number(style.opacity);
    return withKey(cloneElement(children as ReactElement<{ style?: CSSProperties }>, {
      style: {
        ...style,
        ...(move ? { transform: style.transform ? `${move} ${style.transform}` : move } : {}),
        ...(fade !== 1 ? { opacity: own * fade } : {}),
        ...placed(style.filter),
      },
    }));
  }
  return withKey(
    <AbsoluteFill style={{ ...(move ? { transform: move } : {}), ...(fade !== 1 ? { opacity: fade } : {}), ...placed() }}>
      {children}
      {vignette && <AbsoluteFill style={vignette} />}
    </AbsoluteFill>,
  );
}
