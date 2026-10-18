// What every part may know about the edit it's placed in: the frame (rate and size), the spoken words on the edit's
// timeline (mapped through the cut, so a cue is a phrase, not a frame), the brand's theme, and text widths (so a part
// sizes itself, never overflows).
import { createContext, useContext } from "react";
import metrics from "./metrics.json";

export type Word = { text: string; ws: number; we: number }; // output frames

/** One level of the brand's type scale, in the website's px (convert with `typeScale`). */
export type Level = { size?: number; weight?: number; line?: number; tracking?: number };
/** The brand's look, from the space's brand/brand.json (the brand tool writes it), the edit row, or a part's props.
 *  Nothing here has a default: a part that needs a value nobody gave says so. */
export type Theme = {
  accent?: string; surface?: string; textColor?: string; accentText?: string;
  displayFont?: string; textFont?: string;
  /** website px -> frame px (2.5 on a 1080-wide frame, after the host app's brand kit note), scaled by the frame width */
  typeScale?: number;
  scale?: Partial<Record<"h1" | "h2" | "h3" | "h4" | "p", Level>>;
  /** the site's primary button, for CTA pills */
  button?: { bg?: string; color?: string; font?: string; size?: number; weight?: number; radius?: number };
  /** card corner radius and shadow, in site px / CSS (the site's card and its largest shadow) */
  radius?: number;
  shadow?: string;
};
export type Edit = { fps: number; width: number; height: number; words: Word[]; theme?: Theme };
export const EditCtx = createContext<Edit>({ fps: 30, width: 1080, height: 1920, words: [] });
export const useEdit = () => useContext(EditCtx);

const THEME_PROPS: Record<string, string> = {
  accent: "accent", surface: "surface", textColor: "text_color", accentText: "accent_text",
  displayFont: "display_font", textFont: "text_font", typeScale: "type_scale",
};
/** The theme a part works in: its own props over the edit's theme (edit row over brand.json). `need` throws when a
 *  value is missing, naming the prop that gives it, instead of falling back to some look. */
export function useTheme(part: string, props: Record<string, any>) {
  const { theme = {}, width } = useEdit();
  const t: Theme = { ...theme };
  for (const k of Object.keys(THEME_PROPS)) if (props[k] != null) (t as any)[k] = props[k];
  const need = <K extends keyof Theme>(k: K): NonNullable<Theme[K]> => {
    const v = t[k];
    if (v == null || v === "") throw new Error(`${part}: no ${THEME_PROPS[k] ?? k}. The space has no brand (run the brand tool, which writes brand/brand.json) or the brand doesn't give one: pass ${THEME_PROPS[k] ?? k} in the part's props`);
    return v as NonNullable<Theme[K]>;
  };
  // the site's sizes are for a browser: px on a 1080-wide frame = site px * 2.5 (the host app's brand kit note)
  const typeScale = t.typeScale ?? 2.5 * width / 1080;
  /** A level of the brand scale on this frame: size, weight and tracking in frame px. */
  // `size` (frame px, from the part's props) wins over the brand's; with no brand scale, weight comes from props.weight
  const level = (name: "h1" | "h2" | "h3" | "h4" | "p", size?: number) => {
    const l = t.scale?.[name];
    if (!l?.size && size == null) throw new Error(`${part}: the brand has no ${name} size (no type scale in brand/brand.json): pass the part's size props (max, small, size)`);
    if (!l?.size && props.weight == null) throw new Error(`${part}: the brand has no type scale to take a weight from: pass weight`);
    return {
      size: Math.round(size ?? l!.size! * typeScale), weight: Number(props.weight ?? l?.weight ?? 400),
      tracking: (l?.tracking ?? 0) * typeScale * (size != null && l?.size ? size / (l.size * typeScale) : 1),
    };
  };
  return { t, need, level, typeScale };
}

/** The frame: `u` is px per 1080-wide unit (sizes and x), `Y(v)` places a y given in px, or as a fraction (0-1) of
 *  the height. Parts lay out from these, so they sit on any frame (1080x1920, 1080x1350, 1920x1080). */
export function useFrame() {
  const { width, height } = useEdit();
  const u = width / 1080;
  return { width, height, u, Y: (v: number) => (v > 0 && v <= 1 ? Math.round(v * height) : Math.round(v)), X: (v: number) => (v > 0 && v <= 1 ? Math.round(v * width) : Math.round(v)) };
}

// ---- colour -------------------------------------------------------------------------------------------------------
export function rgb(hex: string): [number, number, number] | undefined {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})(?:[0-9a-f]{2})?$/i.exec(hex.trim());
  if (!m) return undefined;
  const h = m[1].length === 3 ? m[1].split("").map((c) => c + c).join("") : m[1];
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
}
/** WCAG relative luminance. */
export function luminance(hex: string): number {
  const c = rgb(hex);
  if (!c) return NaN;
  const [r, g, b] = c.map((v) => { const s = v / 255; return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4; });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
/** WCAG contrast ratio of two colours (1-21). */
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}
/** `hex` at `alpha` (0-1) as #rrggbbaa. */
export const alpha = (hex: string, a: number) => {
  const c = rgb(hex);
  return c ? "#" + [...c, Math.round(a * 255)].map((v) => v.toString(16).padStart(2, "0")).join("") : hex;
};
/** A halo that separates `text` from footage: dark behind light text, light behind dark text (not a brand colour,
 *  the neutral opposite of the text's own lightness). */
export const halo = (text: string) => {
  const d = luminance(text) > 0.4;
  const c = d ? "0,0,0" : "255,255,255";
  return `0 3px 0 rgba(${c},${d ? 0.35 : 0.3}), 0 0 3px rgba(${c},0.5), 0 10px 36px rgba(${c},${d ? 0.55 : 0.4})`;
};

// ---- cues ---------------------------------------------------------------------------------------------------------
const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9%]+/g, "");

/** The frame where `phrase` starts being said inside [from, to) (its first word's start), or undefined. */
export function cue(words: Word[], phrase: string | undefined, from = 0, to = 1e9, end = false): number | undefined {
  if (!phrase) return undefined;
  const want = phrase.split(/\s+/).map(norm).filter(Boolean);
  const ws = words.filter((w) => w.ws >= from && w.ws < to);
  for (let i = 0; i + want.length <= ws.length; i++) {
    if (want.every((t, j) => norm(ws[i + j].text) === t)) return end ? ws[i + want.length - 1].we : ws[i].ws;
  }
  throw new Error(`cue: "${phrase}" isn't said between frames ${from} and ${to}`);
}

// ---- text widths --------------------------------------------------------------------------------------------------
// metrics.json: {family: {weight: {char: advance at 100px}}} for the engine's bundled faces and the installed ones the
// SOPs offer (metrics.py). A family it doesn't know is sized with the widest known face's advances, so text set in
// it can come out a little small but never overflows; a warning goes to stderr.
const M = metrics as Record<string, Record<string, Record<string, number>>>;
const avg = (m: Record<string, number>) => { const v = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("").map((c) => m[c]).filter(Boolean); return v.reduce((a, b) => a + b, 0) / v.length; };
let widest: Record<string, number> | undefined;
const warned = new Set<string>();
function face(font: string, weight: number): Record<string, number> {
  const fam = M[font] ?? M[Object.keys(M).find((k) => k.toLowerCase() === font.trim().toLowerCase()) ?? ""];
  if (fam) {
    const w = Object.keys(fam).map(Number).sort((a, b) => Math.abs(a - weight) - Math.abs(b - weight) || b - a)[0];
    return fam[String(w)];
  }
  if (!warned.has(font)) { warned.add(font); console.error(`measure: no metrics for "${font}": sizing it with the widest known face, so it won't overflow (it may set a little small)`); }
  return widest ??= Object.values(M).flatMap((f) => Object.values(f)).sort((a, b) => avg(b) - avg(a))[0];
}

/** Width in px of `text` set in `font` at `size` and `weight` (uppercase when `upper`), with letter-spacing `tracking`. */
export function measure(text: string, font: string, size: number, tracking = 0, upper = true, weight = 700): number {
  const m = face(font, weight);
  const fallback = avg(m);
  const t = upper ? text.toUpperCase() : text;
  let w = 0;
  for (const ch of t) w += (m[ch] ?? fallback) * size / 100 + tracking;
  return w;
}

/** The largest size <= max at which `text` fits `width` on one line. */
export function fit(text: string, font: string, max: number, width: number, tracking = 0, upper = true, weight = 700): number {
  const w = measure(text, font, max, tracking, upper, weight);
  return w <= width ? max : Math.floor(max * width / w);
}

/** "*half* the spend" -> [{t: "half", hot: true}, {t: " the spend", hot: false}] */
export function marks(s: string) {
  return s.split(/(\*[^*]+\*)/).filter(Boolean).map((t) => t.startsWith("*") ? { t: t.slice(1, -1), hot: true } : { t, hot: false });
}
export const plain = (s: string) => s.replace(/\*/g, "");
