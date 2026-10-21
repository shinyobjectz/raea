// Gaussian splats in @raea/motion/3d (engine/rdb/src/engine/splat.rs + splat.wgsl draw them): a real scene or object
// reconstructed from footage (`raea-motion splat train`) or any 3DGS .ply / .splat / .spz, placed like a Model and
// animated by props. Every effect is a closed-form function of the props at the frame and a seed, so a render of
// frame N equals frame N of play.
import { toHex, type Vec3 } from "./3d";

export interface SplatWave {
  /** how high the ripple lifts the splats, metres */
  amplitude: number;
  /** from crest to crest, metres (1 by default) */
  wavelength?: number;
  /** where the ripple is in its cycle, radians: animate it (frame * 0.2) to run it outward */
  phase?: number;
}

export interface SplatMorph {
  /** the splat file it becomes */
  to: string;
  /** 0 this splat, 1 the other (each splat flies to the one matched to it) */
  amount: number;
}

export interface SplatProps {
  /** parts are matched by name across frames: name a splat that comes and goes */
  name?: string;
  /** the splat file (staticFile("x.ply")): a 3DGS .ply (spherical harmonics up to degree 3), a .splat or a .spz. A
   * `x.ply.json` beside it (raea-motion splat train writes one) places it: +Y up, centred, metres */
  src: string;
  position?: Vec3;
  /** radians, Euler XYZ */
  rotation?: Vec3;
  scale?: number | Vec3;
  /** every gaussian's size times this (1 by default): below 1 the scene breaks into points */
  splatScale?: number;
  opacity?: number;
  /** grade: colour times brightness, saturation and contrast about mid grey, times tint */
  brightness?: number;
  saturation?: number;
  contrast?: number;
  tint?: string;
  /** reveal 0..1: each splat scales in as a front passes it, from `revealFrom` (a point; the centre by default)
   * outward, or along `revealAxis` (a plane sweeping through) when that's given */
  reveal?: number;
  revealFrom?: Vec3;
  revealAxis?: Vec3;
  /** how wide the front is, as a share of the sweep (0.15) */
  revealSoft?: number;
  /** the front's glow (0 none, 1 bright), in `glowColor` */
  revealGlow?: number;
  /** the colour of reveal and pulse glows (warm white) */
  glowColor?: string;
  /** disperse 0..1: blown off on a curl-noise wind, the upwind side first, shrinking to specks */
  disperse?: number;
  /** assemble 0..1: from a cloud of dust round the scene into its shape (1 whole) */
  assemble?: number;
  /** the wind's direction ([1, 0.15, 0]) */
  wind?: Vec3;
  /** how much the wind curls (1) */
  curl?: number;
  /** explode 0..1: thrown out from the centre, falling and fading */
  explode?: number;
  wave?: SplatWave;
  /** radians a metre about the vertical through the centre */
  twist?: number;
  morph?: SplatMorph;
  /** a value the composition passes (a beat's envelope 0..1): bigger by pulseScale, brighter by pulseGlow */
  pulse?: number;
  pulseScale?: number;
  pulseGlow?: number;
  /** nothing past this many metres from the centre (trims a trained scene's floaters) */
  crop?: number;
  /** the light in this many flat bands (0 none); a paint or cartoon <Look> sets it */
  bands?: number;
  /** 0..1: the splats themselves painted (flat bands, grown into strokes with hard edges), for a painted shot in a
   * timeline whose frame <Look> isn't paint */
  paint?: number;
  /** which randomness: the scatter, the jitter (0) */
  seed?: number;
}

const unit = (owner: string, k: string, v: number | undefined, lo = 0, hi = 1) => {
  if (v !== undefined && !(Number.isFinite(v) && v >= lo && v <= hi)) throw new RangeError(`${owner}: ${k} must be from ${lo} to ${hi}, got ${v}`);
};
const vec = (owner: string, k: string, v: unknown) => {
  if (v !== undefined && !(Array.isArray(v) && v.length === 3 && v.every((n) => typeof n === "number" && Number.isFinite(n)))) {
    throw new TypeError(`${owner}: ${k} must be three finite numbers [x, y, z], got ${JSON.stringify(v)}`);
  }
};

/** A gaussian splat: a reconstructed scene or object, drawn back to front for the frame's camera, behind any mesh in
 * front of it, through the frame's look. Animate it with reveal, assemble, disperse, explode, wave, twist, morph and
 * pulse, all values of the frame. */
export function Splat(props: SplatProps) {
  const o = "Splat";
  if (typeof props.src !== "string" || props.src === "") throw new TypeError(`${o}: src must be a splat file (staticFile("x.ply"))`);
  for (const k of ["position", "rotation", "revealFrom", "revealAxis", "wind"] as const) vec(o, k, props[k]);
  if (typeof props.scale !== "number") vec(o, "scale", props.scale);
  for (const k of ["reveal", "disperse", "assemble", "opacity", "paint"] as const) unit(o, k, props[k]);
  unit(o, "explode", props.explode, 0, 4);
  for (const k of ["splatScale", "brightness", "saturation", "contrast", "revealGlow", "pulseScale", "pulseGlow", "crop", "bands", "curl"] as const) unit(o, k, props[k], 0, 1e6);
  unit(o, "revealSoft", props.revealSoft, 0.001, 4);
  if (props.disperse !== undefined && props.assemble !== undefined) throw new TypeError(`${o}: disperse (blown away on the wind) or assemble (gathered from dust), not both`);
  if (props.morph) {
    if (typeof props.morph.to !== "string") throw new TypeError(`${o}: morph.to must be a splat file`);
    unit(o, "morph.amount", props.morph.amount);
  }
  if (props.wave && !(Number.isFinite(props.wave.amplitude))) throw new TypeError(`${o}: wave.amplitude must be a number of metres`);
  if (props.wave?.wavelength !== undefined && !(props.wave.wavelength > 0)) throw new RangeError(`${o}: wave.wavelength must be above 0`);
  for (const k of ["pulse", "twist", "seed"] as const) {
    const v = props[k];
    if (v !== undefined && !Number.isFinite(v)) throw new RangeError(`${o}: ${k} must be a finite number, got ${v}`);
  }
  const { morph, wave, assemble, disperse, tint, glowColor, ...rest } = props;
  const out: Record<string, unknown> = { ...rest };
  if (tint !== undefined) out.tint = toHex(o, "tint", tint);
  if (glowColor !== undefined) out.glowColor = toHex(o, "glowColor", glowColor);
  if (morph) Object.assign(out, { morphTo: morph.to, morph: morph.amount });
  if (wave) Object.assign(out, { waveAmplitude: wave.amplitude, wavelength: wave.wavelength ?? 1, wavePhase: wave.phase ?? 0 });
  if (disperse !== undefined) Object.assign(out, { disperse, scatter: "wind" });
  if (assemble !== undefined) Object.assign(out, { disperse: 1 - assemble, scatter: "dust" });
  if (props.revealAxis) out.revealMode = "plane";
  return <div data-3d="Splat" data-props={JSON.stringify(out)} />;
}
