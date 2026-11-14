// A part's mask, blend and chroma key (docs/timeline.feature TL7f): CapCut's Mask tab, Basic's Blend and Cutout's
// Chroma key, as data the timeline writes and <Nudge> draws, and the editor's preview draws the same way. A mask is
// an SVG drawn as the part's mask image (a shape, turned, feathered by a blur, or inverted); a chroma key is an SVG
// filter whose colour matrix makes the key colour's hue transparent. raea's renderer draws both as Chrome does.

/** A mask's shapes, in CapCut's order: a line (what is above it is kept), a mirror (a band), a circle, a rectangle,
 *  a heart and a star. */
export const MASK_KINDS = ["line", "mirror", "circle", "rect", "heart", "star"] as const;
export type MaskKind = (typeof MASK_KINDS)[number];

/** A mask, in shares of the part's frame. */
export interface Mask {
  /** its shape: line, mirror, circle, rect, heart or star */
  kind: MaskKind;
  /** its centre, a share of the frame's width from the left (0.5 the middle) */
  x?: number;
  /** its centre, a share of the frame's height from the top */
  y?: number;
  /** its width, a share of the frame's (a line has none) */
  w?: number;
  /** its height, a share of the frame's (a mirror's band is this thick) */
  h?: number;
  /** a turn, in degrees */
  rotate?: number;
  /** how soft its edge is, 0 sharp to 1 (a blur a twentieth of the frame's shorter side) */
  feather?: number;
  /** a rectangle's corners, 0 square to 1 as round as its shorter side allows */
  round?: number;
  /** keeps what is outside it instead */
  invert?: boolean;
}

/** A mask's values when a kind is chosen: in the middle, half the frame's shorter side across, so a circle is round
 *  on any frame; a mirror a band a fifth of the frame's height. */
export function maskPlain(kind: MaskKind, width: number, height: number): Required<Mask> {
  const side = Math.min(width, height) / 2;
  return { kind, x: 0.5, y: 0.5, w: r5(side / width), h: kind === "mirror" ? 0.2 : r5(side / height), rotate: 0, feather: 0, round: 0, invert: false };
}

const r2 = (n: number) => Math.round(n * 100) / 100;
const r3 = (n: number) => Math.round(n * 1000) / 1000;
// shares to five places: a 1920 px frame's 540 px is 0.28125, which three places would leave 0.48 px short
const r5 = (n: number) => Math.round(n * 100000) / 100000;

/** A mask's errors: a kind it doesn't have, or a value out of its range; empty when it is right. */
export function maskError(m: Mask): string | undefined {
  if (!m || !MASK_KINDS.includes(m.kind)) return `a mask's kind is one of ${MASK_KINDS.join(", ")}, got ${JSON.stringify(m?.kind)}`;
  for (const k of ["x", "y", "w", "h", "rotate", "feather", "round"] as const)
    if (m[k] !== undefined && !Number.isFinite(m[k])) return `a mask's ${k} must be a finite number, got ${m[k]}`;
  if ((m.w ?? 0) < 0 || (m.h ?? 0) < 0) return `a mask's w and h are 0 or more, got ${m.w}, ${m.h}`;
  for (const k of ["feather", "round"] as const) if (m[k] !== undefined && !(m[k]! >= 0 && m[k]! <= 1)) return `a mask's ${k} is from 0 to 1, got ${m[k]}`;
}

// a 5-point star in a unit box, its top point up (the inner points at 0.382 of the outer, a regular star's)
const STAR = Array.from({ length: 10 }, (_, n) => {
  const r = n % 2 ? 0.191 : 0.5, a = (n * Math.PI) / 5 - Math.PI / 2;
  return `${r3(0.5 + r * Math.cos(a))},${r3(0.53 + r * Math.sin(a))}`;
}).join(" ");
const HEART = "M0.5 0.92C0.12 0.64 0 0.42 0.04 0.26C0.09 0.06 0.34 -0.02 0.5 0.2C0.66 -0.02 0.91 0.06 0.96 0.26C1 0.42 0.88 0.64 0.5 0.92Z";

/** The mask's shape as SVG markup in the frame's pixels: black (what a mask image keeps) on nothing, turned about
 *  its centre and blurred by its feather. maskStyle inverts it. */
export function maskSvg(m: Mask, width: number, height: number): string {
  const e = maskError(m);
  if (e) throw new RangeError(`Mask: ${e}`);
  const p = { ...maskPlain(m.kind, width, height), ...m };
  const cx = r2(p.x * width), cy = r2(p.y * height), w = r2(p.w * width), h = r2(p.h * height), far = width + height;
  const turn = p.rotate ? ` transform="rotate(${r2(p.rotate)} ${cx} ${cy})"` : "";
  const unit = (d: string) => `<g${turn}><g transform="translate(${r2(cx - w / 2)} ${r2(cy - h / 2)}) scale(${w || 0.01} ${h || 0.01})">${d}</g></g>`;
  // black: what a mask image keeps
  const fill = "#000";
  const shape = {
    line: `<rect x="${r2(cx - far)}" y="${r2(cy - far)}" width="${2 * far}" height="${far}"${turn} fill="${fill}"/>`,
    mirror: `<rect x="${r2(cx - far)}" y="${r2(cy - h / 2)}" width="${2 * far}" height="${h}"${turn} fill="${fill}"/>`,
    circle: `<ellipse cx="${cx}" cy="${cy}" rx="${r2(w / 2)}" ry="${r2(h / 2)}"${turn} fill="${fill}"/>`,
    rect: `<rect x="${r2(cx - w / 2)}" y="${r2(cy - h / 2)}" width="${w}" height="${h}" rx="${r2((p.round * Math.min(w, h)) / 2)}"${turn} fill="${fill}"/>`,
    heart: unit(`<path d="${HEART}" fill="${fill}"/>`),
    star: unit(`<polygon points="${STAR}" fill="${fill}"/>`),
  }[p.kind];
  const sigma = r2(p.feather * Math.min(width, height) * 0.05);
  const blur = sigma > 0 ? ` filter="url(#f)"` : "";
  // the blur's area: the frame and as far past it as a blur reaches (three of its widths), no larger, since a filter
  // is a layer that size (three frames each way overran Vello's buffers)
  const reach = Math.ceil(3 * sigma);
  const defs = sigma > 0 ? `<defs><filter id="f" filterUnits="userSpaceOnUse" x="${-reach}" y="${-reach}" width="${width + 2 * reach}" height="${height + 2 * reach}"><feGaussianBlur stdDeviation="${sigma}"/></filter></defs>` : "";
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${defs}<g${blur}>${shape}</g></svg>`;
}

/** The mask as CSS mask images the size of the part's box: the shape, or inverted, the shape excluded from a solid
 *  layer (1 less the shape, its feather kept; an SVG <mask> drawing the same overran Vello's buffers). The SVG's
 *  parentheses are escaped too, so its own url()s aren't read as the page's. */
export function maskStyle(m: Mask, width: number, height: number) {
  const shape = `url("data:image/svg+xml,${encodeURIComponent(maskSvg(m, width, height)).replace(/\(/g, "%28").replace(/\)/g, "%29")}")`;
  const image = m.invert ? `linear-gradient(#000, #000), ${shape}` : shape;
  return {
    maskImage: image,
    WebkitMaskImage: image,
    maskSize: "100% 100%",
    WebkitMaskSize: "100% 100%",
    maskRepeat: "no-repeat",
    WebkitMaskRepeat: "no-repeat",
    ...(m.invert ? { maskComposite: "exclude", WebkitMaskComposite: "xor" } : {}),
  };
}

/** How a part is drawn over what is under it: CapCut's Blend modes that CSS has (Add is plus-lighter), in its order,
 *  then difference and exclusion. */
export const BLENDS = [
  { kind: "normal", title: "Normal" },
  { kind: "darken", title: "Darken" },
  { kind: "screen", title: "Screen" },
  { kind: "overlay", title: "Overlay" },
  { kind: "multiply", title: "Multiply" },
  { kind: "lighten", title: "Lighten" },
  { kind: "hard-light", title: "Hard light" },
  { kind: "soft-light", title: "Soft light" },
  { kind: "color-burn", title: "Color burn" },
  { kind: "color-dodge", title: "Color dodge" },
  { kind: "plus-lighter", title: "Add" },
  { kind: "difference", title: "Difference" },
  { kind: "exclusion", title: "Exclusion" },
] as const;
export type Blend = (typeof BLENDS)[number]["kind"];
export const BLEND_KINDS: Blend[] = BLENDS.map((b) => b.kind);

/** A chroma key: the colour taken out (a green or blue screen), how much of what is near it goes, and how softly. */
export interface ChromaKey {
  /** the key colour, #rrggbb or #rgb */
  color: string;
  /** how far from the key colour is still taken out, 0 (only it) to 1 (most of its hue); 0.5 when not said */
  strength?: number;
  /** how much of the screen's shadows (its darker, duller parts, nearer grey than the key) is let through half seen
   *  rather than kept whole, 0 (none) to 1; it softens the edge too. 0.2 when not said */
  shadow?: number;
}

/** A #rgb or #rrggbb colour as 0-1 channels, or undefined. */
export function rgbOf(color: string): [number, number, number] | undefined {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color?.trim() ?? "");
  if (!m) return undefined;
  const hex = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
  return [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255) as [number, number, number];
}

/** A chroma key's errors, or undefined. A grey can't be keyed by its hue, which it hasn't got. */
export function chromaError(k: ChromaKey): string | undefined {
  const rgb = rgbOf(k?.color);
  if (!rgb) return `a chroma key's colour is #rrggbb or #rgb, got ${JSON.stringify(k?.color)}`;
  const mean = (rgb[0] + rgb[1] + rgb[2]) / 3;
  if (rgb.reduce((s, c) => s + (c - mean) ** 2, 0) < 0.02) return `a chroma key's colour needs a hue to key by: ${k.color} is grey`;
  for (const p of ["strength", "shadow"] as const) if (k[p] !== undefined && !(k[p]! >= 0 && k[p]! <= 1)) return `a chroma key's ${p} is from 0 to 1, got ${k[p]}`;
}

/** The chroma key as an feColorMatrix's 20 values. A colour's share of the key's hue is p = (C · d) / |d|², d the key
 *  colour less its grey (1 for the key, 0 for any grey, below 0 for the opposite hue); alpha falls from 1 to 0 as p
 *  rises through the strength's cut, over the shadow's width. The colour rows are left as they are. */
export function chromaMatrix(k: ChromaKey): string {
  const e = chromaError(k);
  if (e) throw new RangeError(`Chroma key: ${e}`);
  const rgb = rgbOf(k.color)!;
  const mean = (rgb[0] + rgb[1] + rgb[2]) / 3;
  const d = rgb.map((c) => c - mean);
  const dd = d.reduce((s, c) => s + c * c, 0);
  const cut = 1 - 0.85 * (k.strength ?? 0.5), width = 0.03 + 0.4 * (k.shadow ?? 0.2);
  const a = d.map((c) => r3(-c / dd / width));
  return ["1 0 0 0 0", "0 1 0 0 0", "0 0 1 0 0", `${a[0]} ${a[1]} ${a[2]} 0 ${r3(cut / width)}`].join(" ");
}

/** The id of the SVG filter a chroma key is drawn by: one id for one key, so two parts keyed alike share it. */
export const chromaId = (k: ChromaKey) => `raea-key-${rgbOf(k.color)!.map((c) => Math.round(c * 255).toString(16).padStart(2, "0")).join("")}-${Math.round((k.strength ?? 0.5) * 100)}-${Math.round((k.shadow ?? 0.2) * 100)}`;
