// Grade looks for shots (docs/make.feature MK17, from AnyShot's grade presets): a few named starting points for
// <Grade>, each a plain GradeProps you can spread and change. clean: as shot, a touch of contrast. cinematic: slightly
// warm, softer blacks, held highlights, a light vignette. gritty: cool, desaturated, hard mids, a heavy vignette.
// vintage: warm and faded (lifted blacks, rolled highlights), less colour, fringes at the edges.
import type { GradeProps } from "./effects";

export type GradeLook = "clean" | "cinematic" | "gritty" | "vintage";

export const GRADES: Record<GradeLook, GradeProps> = {
  clean: { saturation: 1.02, midtones: { contrast: 1.04 } },
  cinematic: {
    temperature: 0.02,
    saturation: 0.92,
    shadows: { saturation: 0.85, lift: 0.01 },
    midtones: { contrast: 1.08 },
    highlights: { gain: 0.96 },
    vignette: 0.25,
    aberration: 0.002,
  },
  gritty: {
    temperature: -0.03,
    saturation: 0.72,
    shadows: { gamma: 1.12, saturation: 0.7 },
    midtones: { contrast: 1.22 },
    highlights: { gain: 1.02 },
    vignette: 0.42,
    aberration: 0.003,
  },
  vintage: {
    temperature: 0.08,
    tint: 0.015,
    saturation: 0.78,
    shadows: { lift: 0.035, contrast: 0.9 },
    midtones: { contrast: 0.96 },
    highlights: { gain: 0.93 },
    vignette: 0.35,
    aberration: 0.004,
  },
};

/** A grade look's props, with your changes over it: `<Grade {...gradeLook("cinematic", { vignette: 0.1 })} />`. */
export function gradeLook(look: GradeLook, over: GradeProps = {}): GradeProps {
  const base = GRADES[look];
  if (!base) throw new Error(`gradeLook: no look "${look}" (clean, cinematic, gritty, vintage).`);
  const merged: GradeProps = { ...base, ...over };
  for (const k of ["shadows", "midtones", "highlights"] as const) {
    if (base[k] || over[k]) merged[k] = { ...base[k], ...over[k] };
  }
  return merged;
}
