// A part's Adjust (docs/timeline.feature TL7g): CapCut's Adjust tab as numbers the timeline writes and <Nudge adjust>
// draws, and the editor's preview draws the same way. Its tone (exposure, brightness, contrast, highlights, shadows,
// fade) is one curve read through a table, its colour (saturation, temperature, tint) two colour matrices, sharpen a
// 3x3 convolution and grain a fractal noise added and centred: one SVG filter, the steps raea's renderer draws as
// Chrome does. A vignette is a darkening laid over the part's frame, a radial gradient.

/** CapCut's Adjust, in its order: each from -1 to 1 (0 as shot), but sharpen, fade, vignette and grain, from 0. */
export const ADJUSTS = [
  { kind: "brightness", title: "Brightness", min: -1 },
  { kind: "contrast", title: "Contrast", min: -1 },
  { kind: "saturation", title: "Saturation", min: -1 },
  { kind: "exposure", title: "Exposure", min: -1 },
  { kind: "sharpen", title: "Sharpen", min: 0 },
  { kind: "highlights", title: "Highlights", min: -1 },
  { kind: "shadows", title: "Shadows", min: -1 },
  { kind: "temperature", title: "Temperature", min: -1 },
  { kind: "tint", title: "Tint", min: -1 },
  { kind: "fade", title: "Fade", min: 0 },
  { kind: "vignette", title: "Vignette", min: 0 },
  { kind: "grain", title: "Grain", min: 0 },
] as const;
export type AdjustKind = (typeof ADJUSTS)[number]["kind"];
export const ADJUST_KINDS: AdjustKind[] = ADJUSTS.map((a) => a.kind);

/** A part's adjustments, each 0 (or left out) as shot: brightness, contrast, saturation, exposure, highlights,
 *  shadows, temperature (warm above 0) and tint (magenta above 0) from -1 to 1; sharpen, fade, vignette and grain from
 *  0 to 1. */
export type Adjust = Partial<Record<AdjustKind, number>>;

/** An adjustment's errors: a kind it hasn't got, or a value out of its range; undefined when it is right. */
export function adjustError(a: Adjust): string | undefined {
  if (!a || typeof a !== "object") return `an adjust is { brightness, contrast, ... }, got ${JSON.stringify(a)}`;
  for (const [k, v] of Object.entries(a)) {
    const spec = ADJUSTS.find((x) => x.kind === k);
    if (!spec) return `an adjust is ${ADJUST_KINDS.join(", ")}: ${JSON.stringify(k)} isn't one`;
    if (typeof v !== "number" || !(v >= spec.min && v <= 1)) return `an adjust's ${k} is from ${spec.min} to 1, got ${v}`;
  }
}

const r4 = (n: number) => Math.round(n * 10000) / 10000;
const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

/** The tone curve the table holds: an input level (0 to 1) to an output one. */
export function toneAt(a: Adjust, v: number): number {
  const { exposure = 0, brightness = 0, contrast = 0, highlights = 0, shadows = 0, fade = 0 } = a;
  // exposure: a stop either way at its ends
  let o = clamp01(v * 2 ** exposure);
  // brightness: the mids raised or lowered by a gamma, the ends held
  o = o ** (2 ** -brightness);
  // contrast: about the middle grey, down to a third of it at -1
  o = clamp01(0.5 + (o - 0.5) * (contrast >= 0 ? 1 + contrast : 1 + contrast * 0.66));
  // highlights and shadows: the bright and the dark parts moved, each by a bump that leaves the ends where they are
  o = clamp01(o + 0.35 * highlights * 4 * o * o * (1 - o) + 0.35 * shadows * 4 * o * (1 - o) * (1 - o));
  // fade: the blacks lifted and the whites lowered, washed out
  return clamp01(0.22 * fade + o * (1 - 0.32 * fade));
}

/** One step of an SVG filter: its tag and its attributes, so React and the DOM can each make it. */
export interface FilterStep {
  tag: "feComponentTransfer" | "feFuncR" | "feFuncG" | "feFuncB" | "feColorMatrix" | "feConvolveMatrix" | "feTurbulence" | "feComposite";
  attrs: Record<string, string>;
  /** a component transfer's functions */
  children?: FilterStep[];
}

/** The adjustment as an SVG filter's steps, or none when nothing in it changes the picture's colour or detail (a
 *  vignette alone is an overlay, vignetteStyle). `seed` moves the grain: a render passes the frame, so it lives. */
export function adjustSteps(a: Adjust, seed = 0): FilterStep[] {
  const e = adjustError(a);
  if (e) throw new RangeError(`Adjust: ${e}`);
  const steps: FilterStep[] = [];
  if (["exposure", "brightness", "contrast", "highlights", "shadows", "fade"].some((k) => a[k as AdjustKind])) {
    const table = Array.from({ length: 17 }, (_, i) => r4(toneAt(a, i / 16))).join(" ");
    steps.push({ tag: "feComponentTransfer", attrs: {}, children: (["feFuncR", "feFuncG", "feFuncB"] as const).map((tag) => ({ tag, attrs: { type: "table", tableValues: table } })) });
  }
  if (a.saturation) steps.push({ tag: "feColorMatrix", attrs: { type: "saturate", values: String(r4(1 + a.saturation)) } });
  if (a.temperature || a.tint) {
    const t = a.temperature ?? 0, n = a.tint ?? 0;
    const [r, g, b] = [1 + 0.12 * t + 0.04 * n, 1 - 0.08 * n, 1 - 0.12 * t + 0.04 * n].map(r4);
    steps.push({ tag: "feColorMatrix", attrs: { type: "matrix", values: `${r} 0 0 0 0 0 ${g} 0 0 0 0 0 ${b} 0 0 0 0 0 1 0` } });
  }
  if (a.sharpen) {
    const s = r4(0.4 * a.sharpen);
    steps.push({ tag: "feConvolveMatrix", attrs: { order: "3", kernelMatrix: `0 ${-s} 0 ${-s} ${r4(1 + 4 * s)} ${-s} 0 ${-s} 0`, preserveAlpha: "true", edgeMode: "duplicate" } });
  }
  if (a.grain) {
    const g = r4(0.3 * a.grain);
    const before = steps.length ? "toned" : "SourceGraphic";
    if (steps.length) steps[steps.length - 1].attrs.result = "toned";
    steps.push(
      { tag: "feTurbulence", attrs: { type: "fractalNoise", baseFrequency: "0.9", numOctaves: "1", seed: String(Math.round(seed) % 1000), result: "noise" } },
      // the noise as one grey, fully seen
      { tag: "feColorMatrix", attrs: { in: "noise", type: "matrix", values: "1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 0 0 0 0 1", result: "grey" } },
      // the picture, plus the grey less its middle: as much lighter as darker
      { tag: "feComposite", attrs: { in: before, in2: "grey", operator: "arithmetic", k1: "0", k2: "1", k3: String(g), k4: String(r4(-g / 2)), result: "grained" } },
      // only where the part is
      { tag: "feComposite", attrs: { in: "grained", in2: "SourceGraphic", operator: "in" } },
    );
  }
  return steps;
}

/** The id of the SVG filter an adjustment is drawn by: one id for one adjustment (and grain's seed). */
export function adjustId(a: Adjust, seed = 0): string {
  const parts = ADJUST_KINDS.filter((k) => a[k] && k !== "vignette").map((k) => `${k.slice(0, 3)}${Math.round(a[k]! * 100)}`);
  return `raea-adjust-${parts.join("-")}${a.grain ? `-s${Math.round(seed) % 1000}` : ""}`;
}

/** A vignette as the CSS of a layer over the part's frame: the edges darkened, more and further in as it rises. */
export function vignetteStyle(v: number): { background: string } | undefined {
  if (!(v > 0)) return undefined;
  return { background: `radial-gradient(ellipse at center, rgba(0, 0, 0, 0) ${r4(65 - 30 * v)}%, rgba(0, 0, 0, ${r4(0.85 * v)}) 100%)` };
}
