// Volumetric fog: light in the air, drawn inside every <FogVolume>.
import { Vec3, assetPath, checkVec, colours, node3d } from "./all";

export interface VolumetricFogProps {
  /** mode="march": samples along each ray through a fog volume (64 by default): too few stair-step a shaft's edges, so 128 for a
   * render with hard-edged shafts, or 32 with `dither`; about 0.1 ms a step at 1920x1080 on an M5 Pro */
  steps?: number;
  /** mode="march": start each pixel's march a fraction of a step in, by a fixed noise seeded from the pixel and the frame number
   * (false by default): the shafts' steps become fine grain instead of bands, so fewer steps look smooth. The noise
   * is a function of the frame, so a render still equals its play */
  dither?: boolean;
  /** mode="march": the fraction of the frame's resolution the fog is marched at (1 by default): 0.5 marches a quarter of the
   * pixels and puts the result over the frame guided by depth, so an edge in front of a shaft keeps no fog from
   * what's behind it. 0.5, 1/3 or 0.25; each frame on its own, so a render still equals its play */
  resolution?: number;
  /** light the fog has everywhere, lit or not (0 by default). Bevy lights a volume's whole box with it whatever its
   * density, so the box's edges show: keep it 0, or small with the camera inside the volume */
  ambient?: number;
  /** that light's colour (white by default) */
  ambientColor?: string;
  /** how the fog is drawn: "froxel" (the default: every volume written into one grid of cells along the view, lit
   * once a cell and integrated front to back once, with the `hero` lights' shafts integrated exactly along epipolar lines: no
   * steps, no dither, its cost the grid's whatever the resolution or the number of volumes, overlapping volumes mixed
   * as one medium, see-through surfaces fogged at their own depth) or "march" (Bevy's ray march through each
   * volume, as before). `steps`, `dither` and `resolution` are the march's; `slices`, `tile`, `samples` and `hero`
   * the grid's */
  mode?: "march" | "froxel";
  /** the grid's cells along the view, spaced evenly in log depth from the camera to the farthest volume (64 by
   * default; 32 to 256) */
  slices?: number;
  /** the grid's cells across the frame, in pixels a side (16 by default; 4, 8 or 16) */
  tile?: number;
  /** points lit in each cell, spread along its depth (1 by default; 1 to 8): more keeps thin shadows in the
   * grid's soft light (a hero light's shafts don't need them) */
  samples?: number;
  /** the grid's shafts: true (the default) integrates the first two volumetric spot lights and the first two
   * volumetric directional lights exactly, over their shadow maps texel by texel, so a blind's bars or a doorway's
   * edge are as crisp as the shadow map: integrated at samples along lines from the light across the frame (Chen et
   * al.'s epipolar sampling), more of them where the depth or the light changes, and each pixel read from the
   * samples either side of it at its depth; false lights them in the grid's cells with the rest */
  hero?: boolean;
  /** what dims a light on its way through a volume to a point of it: "absorb" (the default: what the medium absorbs,
   * which matched the path tracer best), "delta" (absorption and the share of scattering that turns light aside),
   * "full" (the whole extinction), "off" (not dimmed) or "bevy" (Bevy's march's rule: each volume's light dimmed
   * once by its own radius) */
  shadow?: "absorb" | "delta" | "full" | "off" | "bevy";
  /** the hero lights integrated at every pixel instead of at epipolar samples (false by default): several times
   * slower, the reference the samples are measured against */
  heroExact?: boolean;
  /** the hero lights' epipolar samples: lines `lineGap` pixels apart round each light's rectangle on screen (8),
   * samples `spacing` pixels apart along each (6), every `every`th traced (16) and those between traced too where
   * they differ by more than `relTolerance` (0.2), `absTolerance` (2/255) or `depthTolerance` (0.02, relative) */
  epipolar?: { lineGap?: number; spacing?: number; every?: number; relTolerance?: number; absTolerance?: number; depthTolerance?: number };
  /** the hero passes read the grid's transmittance tricubic, as the apply pass reads its light (false by default:
   * trilinear) */
  tricubic?: boolean;
  /** under <SolariLighting>, the fog's light from Solari's scene: false lights it as without Solari; or its
   * settings: light candidates a cell (8), frames its history averages (16), sky rays a cell (1), what lights it
   * (`lamps`, the emissive meshes, and `sky`, both on), the history's filter (on), and what the sky's guide is made
   * of (`guide` 3: 1 the cell's own last, 2 half a neighbour's, 3 both, 0 none) */
  solari?: boolean | { candidates?: number; frames?: number; skyRays?: number; lamps?: boolean; sky?: boolean; filter?: boolean; guide?: 0 | 1 | 2 | 3 };
}

/** Volumetric fog on the camera: light in the air, drawn inside every <FogVolume>, with shafts from the lights marked
 * `volumetric`. Put it inside the Canvas3D, once; without it fog volumes draw nothing.
 * @bevy bevy_light::volumetric::VolumetricFog
 * @bevy bevy_light::volumetric::VolumetricLight */
export function VolumetricFog(props: VolumetricFogProps) {
  if (props.steps !== undefined && !(Number.isInteger(props.steps) && props.steps >= 1 && props.steps <= 256)) throw new RangeError(`VolumetricFog: steps must be a whole number from 1 to 256, got ${props.steps}`);
  if (props.dither !== undefined && typeof props.dither !== "boolean") throw new TypeError(`VolumetricFog: dither must be true or false, got ${JSON.stringify(props.dither)}`);
  if (props.resolution !== undefined && !(props.resolution > 0 && props.resolution <= 1 && [1, 2, 3, 4].some((k) => Math.abs(props.resolution! * k - 1) < 1e-6))) throw new RangeError(`VolumetricFog: resolution must be 1, 0.5, 1/3 or 0.25, got ${props.resolution}`);
  if (props.ambient !== undefined && !(props.ambient >= 0)) throw new RangeError(`VolumetricFog: ambient must be 0 or more, got ${props.ambient}`);
  if (props.mode !== undefined && !["march", "froxel"].includes(props.mode)) throw new RangeError(`VolumetricFog: mode must be "march" or "froxel", got ${JSON.stringify(props.mode)}`);
  if (props.slices !== undefined && !(Number.isInteger(props.slices) && props.slices >= 32 && props.slices <= 256)) throw new RangeError(`VolumetricFog: slices must be a whole number from 32 to 256, got ${props.slices}`);
  if (props.tile !== undefined && ![4, 8, 16].includes(props.tile)) throw new RangeError(`VolumetricFog: tile must be 4, 8 or 16, got ${props.tile}`);
  if (props.samples !== undefined && !(Number.isInteger(props.samples) && props.samples >= 1 && props.samples <= 8)) throw new RangeError(`VolumetricFog: samples must be a whole number from 1 to 8, got ${props.samples}`);
  if (props.shadow !== undefined && !["absorb", "delta", "full", "off", "bevy"].includes(props.shadow)) throw new RangeError(`VolumetricFog: shadow must be "absorb", "delta", "full", "off" or "bevy", got ${JSON.stringify(props.shadow)}`);
  for (const [k, v] of Object.entries(props.epipolar ?? {})) if (!(typeof v === "number" && v > 0)) throw new RangeError(`VolumetricFog: epipolar.${k} must be a number over 0, got ${v}`);
  if (typeof props.solari === "object") {
    for (const k of ["candidates", "frames", "skyRays"] as const) {
      const v = props.solari[k];
      if (v !== undefined && !(Number.isInteger(v) && v >= 1)) throw new RangeError(`VolumetricFog: solari.${k} must be a whole number from 1, got ${v}`);
    }
    if (props.solari.guide !== undefined && ![0, 1, 2, 3].includes(props.solari.guide)) throw new RangeError(`VolumetricFog: solari.guide must be 0, 1, 2 or 3, got ${props.solari.guide}`);
  }
  return node3d("VolumetricFog", colours("VolumetricFog", props as Record<string, unknown>, ["ambientColor"]));
}

export interface FogVolumeProps {
  /** parts are matched by name across frames: name a volume that comes and goes */
  name?: string;
  /** the box's centre */
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  /** the box's size in metres: one number for a cube, or [x, y, z] (1 by default) */
  scale?: number | Vec3;
  /** how thick the air is (0.1 by default: dusty; 0.02 a faint haze; 0.5 smoke). Animate it to let dust settle,
   * smoke roll in or a volume vanish (0) */
  density?: number;
  /** the fog's own colour, what light scattering out of it takes on (white by default; a dark red for blood in the
   * air) */
  color?: string;
  /** how much light it swallows (0.3 by default): higher reads as soot or smoke */
  absorption?: number;
  /** how much light it throws back toward the eye (0.3 by default): higher reads as bright dust or mist */
  scattering?: number;
  /** how far forward it scatters, -1 to 1 (0.5 by default): near 1 the shafts glow when you look toward the light */
  anisotropy?: number;
  /** how fast it thins going up, per metre from the box's floor (0 by default: even throughout): 0.5 is a ground
   * fog that's a third as thick two metres up. A box wide and tall with a falloff is height fog. Only
   * <VolumetricFog mode="froxel"> draws it; the march draws the box evenly */
  falloff?: number;
  /** how far in from each face its density ramps up, as a share of the box's half-size, 0 to 1 (0 by default: a hard
   * box): 0.5 and up hides a small volume's edges (steam, a smoke column). Only <VolumetricFog mode="froxel"> */
  soft?: number;
  /** how much narrower its floor is than its top, 0 to 1 (0 by default: a box): 0.7 is a plume rising off a tank,
   * widening as it climbs. Only <VolumetricFog mode="froxel"> */
  taper?: number;
  /** Bevy's density_texture: a 3D .ktx2 image in the media folder whose values scale the density through the box (a
   * cloud, a bunny of smoke); the box draws evenly without one. Only the march draws it */
  density_texture?: string;
}

/** A box of air that light shows in: dust in a sunbeam, smoke, a haze of blood. Needs a <VolumetricFog> in the
 * Canvas3D; lights marked `volumetric` throw shafts through it, and every light's glow fills it softly. A part, so it
 * keeps its name across frames: animate its density, colour and placement.
 * @bevy bevy_light::volumetric::FogVolume */
export function FogVolume(props: FogVolumeProps) {
  for (const k of ["position", "rotation"] as const) if (props[k] !== undefined) checkVec("FogVolume", k, props[k]);
  if (props.density !== undefined && !(props.density >= 0 && Number.isFinite(props.density))) throw new RangeError(`FogVolume: density must be 0 or more, got ${props.density}`);
  if (props.anisotropy !== undefined && !(props.anisotropy > -1 && props.anisotropy < 1)) throw new RangeError(`FogVolume: anisotropy must be between -1 and 1, got ${props.anisotropy}`);
  const tex = props.density_texture;
  if (tex !== undefined && !(typeof tex === "string" && /\.ktx2$/i.test(tex))) throw new TypeError(`FogVolume: density_texture is a 3D .ktx2 image, got ${JSON.stringify(tex)}`);
  if (props.falloff !== undefined && !(props.falloff >= 0 && Number.isFinite(props.falloff))) throw new RangeError(`FogVolume: falloff must be 0 or more, got ${props.falloff}`);
  for (const k of ["soft", "taper"] as const) if (props[k] !== undefined && !(props[k]! >= 0 && props[k]! <= 1)) throw new RangeError(`FogVolume: ${k} must be between 0 and 1, got ${props[k]}`);
  return node3d("FogVolume", colours("FogVolume", (tex === undefined ? props : { ...props, density_texture: assetPath(tex) }) as Record<string, unknown>, ["color"]));
}
