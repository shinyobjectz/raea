// Fire, depth of field, outlines and lens distortion.
import { useCurrentFrame, useVideoConfig } from "@raea/app/clock";
import { toHex } from "../colour";
import { useAnimation3d } from "@raea/math/animate";
import { Animate3d, Vec3, apply3d, checkShares, checkVec, node3d } from "./all";
import { firstKey, useSequenceFrames } from "./clock";

export interface FireProps {
  /** parts are matched by name across frames: name a fire that comes and goes */
  name?: string;
  /** the flame's foot: where it stands on its fuel */
  position?: Vec3;
  /** its size in metres: one number for its height (it's half as wide), or [width, height, depth] (1 by default) */
  scale?: number | Vec3;
  /** how bright its core is, as a material's emissiveIntensity (6 by default): 3 a guttering fire, 10-20 a roaring
   * one in the dark. Values over 1 bloom under a <Bloom> */
  intensity?: number;
  /** the flame's body (orange, "#ff7a1f", by default); its edges go deep red and its core yellow-white */
  color?: string;
  /** which flame: fires with different seeds don't flicker together (0 by default) */
  seed?: number;
  /** how fast its tongues rise, 1 by default (a big fire's rise more slowly than a small one's on their own) */
  speed?: number;
  /** seconds on its clock (by default the Sequence's time: frame / fps) */
  time?: number;
  /** samples along each ray through it (48 by default): fewer is cheaper and blurrier */
  steps?: number;
  /** how much of what's behind it its body hides (1 by default): raise it (3-6) in daylight, where a bright wall
   * behind would wash out a flame that only adds light */
  opacity?: number;
  /** the light it casts on what's around it: a flickering PointLight's intensity (1 by default; 0 for none) */
  light?: number;
}

/** A fire: a flame drawn as a volume, its tongues rising, curling and tearing off as it burns, with a flickering light
 * on its surroundings. Every pixel of it is a function of the frame (rising 3D noise in a tapering envelope), so a
 * render of any frame equals its play. Stand it on its fuel (a pyre, a brazier); put embers and smoke round it
 * yourself.
 * @bevy bevy_pbr::mesh_material::MeshMaterial3d<raea_render::engine::fire::FireMaterial> */
export function Fire(props: FireProps) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (props.position !== undefined) checkVec("Fire", "position", props.position);
  const sc = props.scale ?? 1;
  if (typeof sc === "number") {
    if (!(sc > 0 && Number.isFinite(sc))) throw new RangeError(`Fire: scale must be a height above 0, got ${sc}`);
  } else {
    checkVec("Fire", "scale", sc);
    if (!sc.every((v) => v > 0)) throw new RangeError(`Fire: scale's sizes must be above 0, got ${JSON.stringify(sc)}`);
  }
  const scale: Vec3 = typeof sc === "number" ? [sc * 0.5, sc, sc * 0.5] : sc;
  for (const k of ["intensity", "light", "opacity"] as const) {
    const v = props[k];
    if (v !== undefined && !(v >= 0 && Number.isFinite(v))) throw new RangeError(`Fire: ${k} must be 0 or more, got ${v}`);
  }
  if (props.speed !== undefined && !(props.speed >= 0 && Number.isFinite(props.speed))) throw new RangeError(`Fire: speed must be 0 or more, got ${props.speed}`);
  if (props.seed !== undefined && !Number.isFinite(props.seed)) throw new RangeError(`Fire: seed must be a finite number, got ${props.seed}`);
  if (props.steps !== undefined && !(Number.isInteger(props.steps) && props.steps >= 4 && props.steps <= 256)) throw new RangeError(`Fire: steps must be a whole number from 4 to 256, got ${props.steps}`);
  const time = props.time ?? frame / fps;
  if (!Number.isFinite(time)) throw new RangeError(`Fire: time must be a finite number of seconds, got ${time}`);
  const color = toHex("Fire", "color", props.color ?? "#ff7a1f");
  const { light = 1, ...rest } = props;
  const [x, y, z] = props.position ?? [0, 0, 0];
  // the light flickers with the flame: three seeded sines, a function of the time alone
  const s = (props.seed ?? 0) * 1.37;
  const sp = (props.speed ?? 1) / Math.sqrt(scale[1]);
  const flicker = 0.82 + 0.1 * Math.sin(time * 11 * sp + s) + 0.06 * Math.sin(time * 17.3 * sp + 2.1 * s) + 0.04 * Math.sin(time * 5.1 * sp + 0.7 * s);
  return (
    <>
      {node3d("Fire", { ...rest, position: [x, y, z], scale, time, color })}
      {light > 0
        ? node3d("PointLight", { ...(props.name ? { name: `${props.name}-light` } : {}), position: [x, y + scale[1] * 0.3, z], intensity: light * flicker, color, distance: Math.max(6, scale[1] * 8) })
        : null}
    </>
  );
}

export interface DepthOfFieldProps {
  /** the lens's f-number: lower blurs more (1.4 dreamy, 2.8 cinematic, 8 nearly sharp); 2.8 by default */
  aperture?: number;
  /** "bokeh" (round, as a real lens; the default) or "gaussian" (soft, cheaper) */
  mode?: "bokeh" | "gaussian";
  /** the largest blur, in pixels of the frame (64 by default) */
  maxBlur?: number;
  /** Bevy's fields, as Bevy's DepthOfField component on a Camera3d (inside it), Bevy's defaults: the distance in focus
   * (10; the camera's `focus`), the f-number (1), the largest blur in pixels (64), "Bokeh" or "Gaussian" */
  focal_distance?: number;
  /** the f-number (1): lower is a shallower focus */
  aperture_f_stops?: number;
  /** the largest blur, in px (64) */
  max_circle_of_confusion_diameter?: number;
}

/** Bevy's DepthOfField fields as the old ones; their defaults are Bevy's (f/1, where the old one is f/2.8) */
export function dofOf(owner: string, props: DepthOfFieldProps): DepthOfFieldProps {
  const { focal_distance: _f, aperture_f_stops: f, max_circle_of_confusion_diameter: m, ...rest } = props;
  if (f === undefined && m === undefined && !["Bokeh", "Gaussian"].includes(String(rest.mode))) return rest;
  if (rest.aperture !== undefined || rest.maxBlur !== undefined) throw new TypeError(`${owner}: Bevy's fields or aperture and maxBlur, not both`);
  const mode = rest.mode === undefined ? undefined : (String(rest.mode).toLowerCase() as "bokeh" | "gaussian");
  return { ...rest, aperture: f ?? 1, ...(m !== undefined ? { maxBlur: m } : {}), ...(mode ? { mode } : {}) };
}

/** A lens's depth of field: what's nearer or farther than the camera's `focus` blurs. Put it inside the Canvas3D;
 * pull focus by animating the camera's `focus`. Inside a Camera3d it is Bevy's component on the camera, its
 * focal_distance the camera's focus.
 * @bevy bevy_post_process::dof::DepthOfField */
export function DepthOfField(raw: DepthOfFieldProps) {
  if (raw.focal_distance !== undefined) throw new TypeError("DepthOfField: focal_distance is a Camera3d's: put the DepthOfField inside the Camera3d");
  const props = dofOf("DepthOfField", raw);
  if (props.aperture !== undefined && !(props.aperture > 0)) throw new RangeError(`DepthOfField: aperture must be an f-number above 0, got ${props.aperture}`);
  if (props.mode !== undefined && props.mode !== "bokeh" && props.mode !== "gaussian") throw new RangeError(`DepthOfField: mode is "bokeh" or "gaussian", got ${props.mode}`);
  return node3d("DepthOfField", props as Record<string, unknown>);
}

export interface OutlineProps {
  /** the line's colour, a hex colour (black by default) */
  color?: string;
  /** a silhouette: how far behind the nearer surface (a fraction of its distance) a neighbour must be (0.04) */
  depth?: number;
  /** a crease: how far the surface must turn between neighbours, 1 - cos of the angle (0.3, about 45 degrees; 0 for
   * none) */
  normal?: number;
  /** the line's width in pixels (1 by default; a cartoon's line is about 6) */
  width?: number;
  /** how far from the camera lines reach, in metres: they fade out over its last third, so a far field meeting the
   * sky draws no line along the horizon (no limit by default) */
  far?: number;
  /** composition passes (a part's `pass`) drawn with no line: none on their pixels and no crease against them (a
   * scene object's silhouette in front of one still draws, on the object) */
  skip?: string[];
}

/** Ink lines where the scene's depth steps or its surfaces turn, drawn before bloom and tone mapping; under a <Look>
 * a cell wide (Return of the Obra Dinn's line). Put it inside the Canvas3D. */
export function Outline(props: OutlineProps) {
  checkShares("Outline", props.skip, undefined);
  if (props.depth !== undefined && !(props.depth > 0)) throw new RangeError(`Outline: depth must be above 0, got ${props.depth}`);
  if (props.normal !== undefined && !(props.normal >= 0 && props.normal <= 2)) throw new RangeError(`Outline: normal must be between 0 and 2, got ${props.normal}`);
  if (props.width !== undefined && !(props.width >= 1 && props.width <= 32)) throw new RangeError(`Outline: width must be from 1 to 32 pixels, got ${props.width}`);
  if (props.far !== undefined && !(props.far > 0)) throw new RangeError(`Outline: far must be above 0, got ${props.far}`);
  return node3d("Outline", props as Record<string, unknown>);
}

export interface LensCommon {
  /** what shows where the lens looks past the picture: its edge pixels ("clamp", the default) or "black" */
  edges?: "clamp" | "black";
  /** declared animation of its numbers (strength, fov, k1, k2, zoom) */
  animate?: Animate3d;
}

export interface FisheyeProps extends LensCommon {
  /** 0..1 (1 by default): 0 is the camera's own picture, pixel for pixel; ramp it in and out on beats */
  strength?: number;
  /** "equidistant" (the default) or "stereographic" (gentler at the edges) */
  mode?: "equidistant" | "stereographic";
  /** the vertical field of view, in degrees, the camera widens to at strength 1 (its own by default): 100-150 for a
   * real fisheye; the picture is still remapped corner to corner, so nothing goes black */
  fov?: number;
  /** above 1 crops in, below 1 shows past the picture's edge (see `edges`) */
  zoom?: number;
}

export interface LensDistortionProps extends LensCommon {
  /** Brown-Conrady: r' = r (1 + k1 r^2 + k2 r^4), r a share of the half-diagonal. Above 0 barrel, below 0 pincushion */
  k1?: number;
  /** the r^4 term of the distortion */
  k2?: number;
  /** by default just enough for a barrel's corners to stay inside the picture (1 for pincushion) */
  zoom?: number;
}

function lensNode(owner: string, props: Record<string, unknown>) {
  useSequenceFrames(owner, undefined, [["animate's first key", firstKey(props.animate)]]);
  const anim = useAnimation3d(props.animate as Animate3d | undefined);
  const p = apply3d({ ...props, animate: undefined } as Record<string, unknown>, anim.values);
  const st = p.strength;
  if (st !== undefined && !(typeof st === "number" && st >= 0 && st <= 1)) throw new RangeError(`${owner}: strength must be in [0, 1], got ${st}`);
  for (const k of ["k1", "k2", "zoom", "fov"]) {
    const v = p[k];
    if (v !== undefined && !(typeof v === "number" && Number.isFinite(v))) throw new TypeError(`${owner}: ${k} must be a number, got ${JSON.stringify(v)}`);
  }
  if (p.zoom !== undefined && !((p.zoom as number) > 0)) throw new RangeError(`${owner}: zoom must be above 0, got ${p.zoom}`);
  if (p.fov !== undefined && !((p.fov as number) > 0 && (p.fov as number) < 170)) throw new RangeError(`${owner}: fov is degrees, between 0 and 170, got ${p.fov}`);
  if (p.edges !== undefined && p.edges !== "clamp" && p.edges !== "black") throw new TypeError(`${owner}: edges is "clamp" or "black", got ${JSON.stringify(p.edges)}`);
  if (p.mode !== undefined && p.mode !== "equidistant" && p.mode !== "stereographic") throw new TypeError(`${owner}: mode is "equidistant" or "stereographic", got ${JSON.stringify(p.mode)}`);
  return node3d(owner, p, undefined, anim.attr ? { "data-raea-anim": anim.attr } : undefined);
}

/** A fisheye lens on the camera: a pass over the 3D after tone mapping (the page over it stays straight). Every value
 * animates per frame; a lens's vignette and colour fringes are <Grade>'s. Put it inside the Canvas3D. */
export function Fisheye(props: FisheyeProps) {
  return lensNode("Fisheye", props as Record<string, unknown>);
}

/** Barrel or pincushion distortion (Brown-Conrady k1, k2) on the camera, a pass after tone mapping as Fisheye's.
 * Put it inside the Canvas3D. */
export function LensDistortion(props: LensDistortionProps) {
  return lensNode("LensDistortion", props as Record<string, unknown>);
}

export interface MotionBlurProps {
  /** the fraction of a frame the shutter is open (0.5 by default, a 180-degree shutter); above 1 smears further */
  shutterAngle?: number;
  /** samples each way per pixel (4 by default) */
  samples?: number;
  /** "shutter" (the default: raea's, two taps a pixel for each sample each way) or "bevy" (Bevy's MotionBlur, to
   * compare against) */
  kind?: "shutter" | "bevy";
  /** declared animation of its numbers */
  animate?: Animate3d;
}

/** Motion blur from each part's and the camera's motion over the frame before (Bevy's MotionBlur): with it, each frame
 * is drawn after the one before it, so a frame's blur is the same in a render, a seek and play. A frame without it has
 * none. Put it inside the Canvas3D.
 * @bevy bevy_post_process::motion_blur::MotionBlur */
export function MotionBlur(props: MotionBlurProps) {
  const owner = "MotionBlur";
  useSequenceFrames("MotionBlur", undefined, [["animate's first key", firstKey(props.animate)]]);
  const anim = useAnimation3d(props.animate);
  const p = apply3d({ ...props, animate: undefined } as Record<string, unknown>, anim.values);
  if (p.kind !== undefined && p.kind !== "shutter" && p.kind !== "bevy") throw new RangeError(`${owner}: kind must be "shutter" or "bevy", got ${JSON.stringify(p.kind)}`);
  for (const [k, lo, hi] of [["shutterAngle", 0, 4], ["samples", 1, 32]] as const) {
    const v = p[k];
    if (v !== undefined && !(typeof v === "number" && v >= lo && v <= hi)) throw new RangeError(`${owner}: ${k} must be in [${lo}, ${hi}], got ${v}`);
  }
  return node3d(owner, p, undefined, anim.attr ? { "data-raea-anim": anim.attr } : undefined);
}
