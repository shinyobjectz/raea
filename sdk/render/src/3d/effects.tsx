// Image planes, decals, bloom, fog, grading, environment, ambient occlusion and shadows (volumetric fog in volumetric.tsx).
import { useCurrentFrame, useVideoConfig } from "@raea/app/clock";
import { ImagePlaneProps, Vec3, assetPath, checkShares, checkVec, colours, node3d } from "./all";

/** A picture on a flat card in the scene, unlit (a poster, a screen, a sprite).
 * @bevy bevy_image::image::Image */
export function ImagePlane(props: ImagePlaneProps) {
  if (typeof props.src !== "string" || props.src === "") throw new TypeError("ImagePlane: src must be a picture (staticFile(\"x.png\"))");
  const gr = props.grid;
  if (gr !== undefined && !(Array.isArray(gr) && gr.length === 2 && gr.every((n) => Number.isInteger(n) && n >= 1))) throw new TypeError(`ImagePlane: grid is [columns, rows], two whole numbers of 1 or more, got ${JSON.stringify(gr)}`);
  if (props.frame !== undefined && !(Number.isFinite(props.frame) && props.frame >= 0)) throw new RangeError(`ImagePlane: frame is a cell of the grid, 0 or more, got ${props.frame}`);
  for (const k of ["position", "rotation"] as const) if (props[k] !== undefined) checkVec("ImagePlane", k, props[k]);
  return node3d("ImagePlane", props as unknown as Record<string, unknown>);
}

export interface DecalProps {
  /** the picture pressed onto the surface: staticFile("label.png"); its clear parts show the surface through */
  src: string;
  /** the entity's name, for picking, the inspector and world.* */
  name?: string;
  /** a normal map pressed with the picture: the label's relief */
  normalMap?: string;
  /** width by height in metres (a number: a square) */
  size?: number | [number, number];
  /** where it stands, metres (x, y, z) */
  position?: Vec3;
  /** which way the surface faces at the decal (out of it; [0, 1, 0], a floor or counter, by default): turns the decal
   * so the picture lies on it, upright on a wall, its top toward -z on a floor. Instead of rotation */
  normal?: Vec3;
  /** a turn about the normal (radians) */
  spin?: number;
  /** radians, the decal's +y out of the surface (rather than normal) */
  rotation?: Vec3;
  /** a tint times the picture (keyed: a stain darkening) */
  color?: string;
  /** 0 to 1 (keyed: a label fading in) */
  opacity?: number;
  /** 0..1, smooth to matte */
  roughness?: number;
  /** 0..1, a dielectric to a metal */
  metalness?: number;
  /** drawn in its flat colour, without light or shade */
  unlit?: boolean;
  /** how far off its plane (metres) it still lies on a surface, fading to nothing there (0.05 by default). Anything
   * nearer the plane than this takes it, a cabinet's side or a fridge's as well as the wall: keep it small */
  fade?: number;
}

/** The turn that takes +y to `normal`, with the picture's top (a decal's -z) toward world up on a wall, toward -z on a
 * floor, turned `spin` about the normal: a quaternion [x, y, z, w]. */
export function surfaceTurn(normal: Vec3, spin = 0): [number, number, number, number] {
  const len = (v: Vec3) => Math.hypot(v[0], v[1], v[2]);
  const unit = (v: Vec3): Vec3 => { const l = len(v) || 1; return [v[0] / l, v[1] / l, v[2] / l]; };
  const cross = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  const Y = unit(normal);
  const up: Vec3 = Math.abs(Y[1]) < 0.99 ? [0, 1, 0] : [0, 0, -1];
  const d = up[0] * Y[0] + up[1] * Y[1] + up[2] * Y[2];
  let Z = unit([-(up[0] - d * Y[0]), -(up[1] - d * Y[1]), -(up[2] - d * Y[2])]);
  let X = cross(Y, Z);
  if (spin) {
    const [c, s] = [Math.cos(spin), Math.sin(spin)];
    [X, Z] = [[X[0] * c - Z[0] * s, X[1] * c - Z[1] * s, X[2] * c - Z[2] * s], [X[0] * s + Z[0] * c, X[1] * s + Z[1] * c, X[2] * s + Z[2] * c]];
  }
  // the rotation matrix's columns X, Y, Z as a quaternion
  const [m00, m11, m22] = [X[0], Y[1], Z[2]];
  const tr = m00 + m11 + m22;
  if (tr > 0) {
    const k = Math.sqrt(tr + 1) * 2;
    return [(Y[2] - Z[1]) / k, (Z[0] - X[2]) / k, (X[1] - Y[0]) / k, k / 4];
  }
  if (m00 > m11 && m00 > m22) {
    const k = Math.sqrt(1 + m00 - m11 - m22) * 2;
    return [k / 4, (Y[0] + X[1]) / k, (Z[0] + X[2]) / k, (Y[2] - Z[1]) / k];
  }
  if (m11 > m22) {
    const k = Math.sqrt(1 + m11 - m00 - m22) * 2;
    return [(Y[0] + X[1]) / k, k / 4, (Z[1] + Y[2]) / k, (Z[0] - X[2]) / k];
  }
  const k = Math.sqrt(1 + m22 - m00 - m11) * 2;
  return [(Z[0] + X[2]) / k, (Z[1] + Y[2]) / k, k / 4, (X[1] - Y[0]) / k];
}

/** A picture pressed onto whatever surface it lies across: a label on a jar, a stain on a counter, tiles on a wall, a
 * sticker on a laptop (Bevy's forward decal; the camera gets a depth prepass for it). Placed and keyed like any part,
 * so it moves and fades with a render's clock and a game's play alike.
 * @bevy bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, bevy_pbr::decal::forward::ForwardDecalMaterialExt>> */
export function Decal(props: DecalProps) {
  if (typeof props.src !== "string" || props.src === "") throw new TypeError("Decal: src must be a picture (staticFile(\"x.png\"))");
  for (const k of ["position", "rotation", "normal"] as const) if (props[k] !== undefined) checkVec("Decal", k, props[k]);
  const { size = 1, normal, spin, ...rest } = props;
  const [w, h] = typeof size === "number" ? [size, size] : size;
  const turn = props.rotation === undefined ? { quaternion: surfaceTurn(normal ?? [0, 1, 0], spin) } : {};
  return node3d("Decal", { ...rest, ...turn, scale: [w, 1, h] } as unknown as Record<string, unknown>);
}

export interface PlateProps {
  /** a video in the media folder: staticFile("clip.mp4") */
  src: string;
  /** the entity's name, for picking, the inspector and world.* */
  name?: string;
  /** the clip's time on this frame, in seconds (by default the Sequence's: frame / fps) */
  time?: number;
  /** the lens's radial distortion (SIMPLE_RADIAL's k1, from a solve): the plate is drawn undistorted, so the
   * footage's straight lines are straight where the 3D's are */
  k1?: number;
}

/** Footage behind the 3D: the clip's frame filling the camera's whole view (its `view` when it has one), on the far
 * side of everything, unlit. Give the scene `<ToneMapping mode="none" />` to keep its colours exact. <Matchmove> is
 * the usual way in. */
export function Plate(props: PlateProps) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (typeof props.src !== "string" || props.src === "") throw new TypeError("Plate: src must be a video (staticFile(\"x.mp4\"))");
  const time = props.time ?? frame / fps;
  if (!(Number.isFinite(time) && time >= 0)) throw new RangeError(`Plate: time must be 0 or more seconds, got ${time}`);
  if (props.k1 !== undefined && !(Number.isFinite(props.k1) && Math.abs(props.k1) < 1)) throw new RangeError(`Plate: k1 is a small distortion coefficient, got ${props.k1}`);
  return node3d("Plate", { ...props, time });
}

export interface BloomProps {
  /** 1 is a natural glow, 2 strong; 0 none */
  intensity?: number;
  /** only what's brighter than this glows (0 = everything a little; 1 = only emissive parts and highlights) */
  threshold?: number;
  /** composition passes (a part's `pass`) left out of it: they neither glow nor take the glow */
  skip?: string[];
  /** each named pass's share of it, 0 (as skip) to 1 (all of it, as "scene" has) */
  passes?: Record<string, number>;
  /** the glow added on top of the frame, brightening it, as three.js's and postprocessing's bloom do; by default it
   * takes its light from around the bright parts (Bevy's energy-conserving bloom) */
  additive?: boolean;
}

/** Glow around bright parts (emissive materials, strong highlights). Put it inside the Canvas3D.
 * @bevy bevy_post_process::bloom::settings::Bloom */
export function Bloom(props: BloomProps) {
  if (props.intensity !== undefined && !(props.intensity >= 0)) throw new RangeError(`Bloom: intensity must be 0 or more, got ${props.intensity}`);
  checkShares("Bloom", props.skip, props.passes);
  return node3d("Bloom", props as Record<string, unknown>);
}

/** a fog colour: "#rrggbb" (or #rrggbbaa), or Bevy's Srgba or LinearRgba (raea/color) */
export type FogColor = string | { red: number; green: number; blue: number; alpha: number };

/** bevy_pbr's FogFalloff as it reflects; FogFalloff's constructors (raea/pbr) make one from a visibility distance */
export type FogFalloff =
  | { Linear: { start: number; end: number } }
  | { Exponential: { density: number } }
  | { ExponentialSquared: { density: number } }
  | { Atmospheric: { extinction: [number, number, number]; inscattering: [number, number, number] } };

export interface FogProps {
  /** what far things fade into: match the background */
  color?: FogColor;
  /** linear fog from `near` to `far` (in the Canvas3D 5 and 20 by default; on the camera Bevy's 0 and 100) */
  near?: number;
  /** where linear fog is full, from the camera (20 in the Canvas3D, 100 on a camera) */
  far?: number;
  /** or exponential fog of this density (0.05-0.3), instead of near and far */
  density?: number;
  /** on the camera: Bevy's FogFalloff, in place of near, far and density (Bevy's default Linear 0 to 100) */
  falloff?: FogFalloff;
  /** on the camera: the sun's colour in the fog looking towards it, its alpha its strength (Bevy's default none) */
  directional_light_color?: FogColor;
  /** on the camera: how tight the sun's glow is (Bevy's default 8) */
  directional_light_exponent?: number;
}

/** Distance fog: far things fade into `color`. Inside the <Camera3d> it is Bevy's DistanceFog on that camera, with all of
 * Bevy's fields (an atmospheric falloff, the sun's colour); inside the Canvas3D it fogs the scene's camera by near, far
 * or density.
 * @bevy bevy_pbr::fog::DistanceFog */
export function Fog(props: FogProps) {
  const bevy = (["falloff", "directional_light_color", "directional_light_exponent"] as const).filter((k) => props[k] !== undefined);
  if (bevy.length) throw new TypeError(`Fog: ${bevy.join(", ")} need the <Fog> inside the <Camera3d> it fogs, where it is Bevy's DistanceFog`);
  const c = props.color;
  // Bevy's colours to the hex the scene's fog reads: an Srgba as its to_hex, a LinearRgba through its to_srgba
  const color = c === undefined || typeof c === "string" ? c : (("to_srgba" in c ? (c as { to_srgba(): { to_hex(): string } }).to_srgba() : c) as { to_hex(): string }).to_hex();
  return node3d("Fog", colours("Fog", (color === undefined ? props : { ...props, color }) as Record<string, unknown>, ["color"]));
}

export interface ToneMappingProps {
  /** how bright light maps to the screen: "tony" (Bevy's default), "aces" (filmic, contrasty), "agx", "neutral",
   * "reinhard", "none" */
  mode?: "tony" | "aces" | "agx" | "neutral" | "reinhard" | "none";
  /** stops brighter (+) or darker (-) than the default exposure */
  exposure?: number;
}

/** The camera's tone mapping and exposure. Put it inside the Canvas3D.
 * @bevy bevy_render::view::Tonemapping
 * @bevy bevy_camera::camera::Exposure */
export function ToneMapping(props: ToneMappingProps) {
  return node3d("ToneMapping", props as Record<string, unknown>);
}

export interface GradeSection {
  /** 1 keeps the colour; 0 grey; above 1 richer */
  saturation?: number;
  /** 1 keeps it; above 1 harder */
  contrast?: number;
  /** 1 keeps it; above 1 darker mids */
  gamma?: number;
  /** a multiplier: 1 keeps it */
  gain?: number;
  /** added: 0 keeps it, positive lifts the blacks */
  lift?: number;
}

export interface GradeProps {
  /** warmer (+) or cooler (-): a shift of the CIE x chromaticity, about -0.3 to 0.3 */
  temperature?: number;
  /** magenta (+) or green (-) */
  tint?: number;
  /** the whole frame's saturation after tone mapping (1 keeps it) */
  saturation?: number;
  /** stops of exposure on every pixel just before tone mapping, unlit colours and emission included (0 keeps it):
   *  <ToneMapping exposure> is the camera's, which brightens only what lights shade */
  exposure?: number;
  /** the grade on the shadows: saturation, contrast, gamma, gain, lift */
  shadows?: GradeSection;
  /** the grade on the midtones */
  midtones?: GradeSection;
  /** the grade on the highlights */
  highlights?: GradeSection;
  /** the corners darkened: 0 none, 1 black; or {intensity, radius (0.75), smoothness (5), color} */
  vignette?: number | { intensity?: number; radius?: number; smoothness?: number; color?: string };
  /** a lens's colour fringes at edges, as a fraction of the frame (0.005 subtle, 0.02 strong) */
  aberration?: number;
  /** composition passes (a part's `pass`) the grade leaves as drawn: no fringes or vignette on them, and on an
   * untonemapped camera (a paint Look's) no colour grading either */
  skip?: string[];
  /** each named pass's share of the grade, 0 (as skip) to 1 */
  passes?: Record<string, number>;
}

/** The camera's colour grade (lift, gamma, gain and saturation per range of light, before tone mapping), a lens
 * vignette and colour fringes: one grade over every scene, so a film looks of a piece. Put it inside the Canvas3D.
 * @bevy bevy_render::view::ColorGrading
 * @bevy bevy_post_process::effect_stack::chromatic_aberration::ChromaticAberration
 * @bevy bevy_post_process::effect_stack::vignette::Vignette */
export function Grade(props: GradeProps) {
  checkShares("Grade", props.skip, props.passes);
  const v = props.vignette;
  return node3d("Grade", { ...props, vignette: typeof v === "object" && v.color ? { ...v, color: colours("Grade", { color: v.color }, ["color"]).color } : v } as Record<string, unknown>);
}

export interface EnvironmentProps {
  /** an equirectangular panorama in the media folder: a Radiance .hdr (Poly Haven's), an OpenEXR .exr, or an 8-bit
   * .png/.jpg. Change it between frames to cut from one place to another. Give it or `room`, not both */
  src?: string;
  /** three.js's RoomEnvironment in place of a panorama: a white studio room with soft light boxes, made by raea with
   * no file. It is the light a three.js scene gets from PMREMGenerator.fromScene(new RoomEnvironment(), blur), which
   * <R3F> turns into this */
  room?: boolean;
  /** with `room`, its blur in radians before it lights (fromScene's sigma: three's examples use 0.04); 0 by default */
  blur?: number;
  /** how strongly it lights the scene (1 by default: a sky about as bright as an AmbientLight of 2.5 overall); animate
   * it, or set 0 to switch it off for a shot */
  intensity?: number;
  /** a turn about the vertical axis, in radians, for both the light and the sky: puts the sun where the shot wants it */
  rotation?: number;
  /** draw the panorama as the sky behind everything (off by default: light only) */
  background?: boolean;
  /** the sky's brightness, when it should differ from the light's (`intensity` by default) */
  backgroundIntensity?: number;
  /** the openings the sky lights a room through (a skylight, a window), each its four corners in order round it, in
   * metres (a rectangle or any parallelogram). Traced lighting (Solari, the path tracer) samples the sky through each
   * as it samples a lamp, so a room lit through a small opening gets that light smooth and whole, where rays bounced at
   * random find it too rarely for the denoiser to keep (photo 5's hall: half its floor's sky light). Light passes both
   * ways; up to 8; the first frame's hold. Raster lighting doesn't read them */
  portals?: [number, number, number][][];
}

/** an <Environment>'s portals: each four corners, a parallelogram with some area */
function checkPortals(portals: unknown) {
  if (portals === undefined) return;
  const bad = (why: string) => new TypeError(`Environment: portals ${why}, got ${JSON.stringify(portals)}`);
  if (!Array.isArray(portals) || portals.length > 8) throw bad("is a list of up to 8 openings");
  for (const q of portals) {
    if (!(Array.isArray(q) && q.length === 4 && q.every((p) => Array.isArray(p) && p.length === 3 && p.every((x) => typeof x === "number" && Number.isFinite(x))))) throw bad("are each four corners [x, y, z]");
    const [a, b, c, d] = q as [number, number, number][];
    const u = b.map((x, i) => x - a[i]), v = d.map((x, i) => x - a[i]);
    const edge = Math.max(Math.hypot(...u), Math.hypot(...v));
    if (!(edge > 0) || Math.hypot(...a.map((x, i) => x + c[i] - b[i] - d[i])) > 0.01 * edge) throw bad("are each a parallelogram, corners in order round it");
    if (Math.hypot(u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]) < 1e-6) throw bad("each have some area");
  }
}

/** Light from all around, out of a panorama: soft light from the sky and reflections of it in every glossy or metal
 * surface (image-based lighting), and with `background` the panorama as the sky. Put it inside the Canvas3D, once a
 * frame; its src, intensity, rotation and background may change from frame to frame (a cut from a hall to a street).
 * With `room` in place of a panorama it is three.js's RoomEnvironment, soft studio light with no file.
 * Bevy's EnvironmentMapLight with maps already filtered is <EnvironmentMapLight>.
 * Its panorama is filtered by Bevy's generator and, with `background`, drawn as the sky (engine/render
 * environment.rs): @bevy bevy_light::probe::GeneratedEnvironmentMapLight. Bevy's Skybox from its own cubemap is <Skybox>. */
export function Environment(props: EnvironmentProps) {
  if (props.room !== undefined && typeof props.room !== "boolean") throw new TypeError(`Environment: room is true or false, got ${JSON.stringify(props.room)}`);
  if (props.room) {
    if (props.src !== undefined) throw new TypeError(`Environment: give src (a panorama) or room, not both`);
  } else {
    if (typeof props.src !== "string" || props.src === "") throw new TypeError(`Environment: src must be a panorama's path, staticFile("sky.hdr"), or room for three's RoomEnvironment, got ${JSON.stringify(props.src)}`);
    if (!/\.(hdr|exr|png|jpe?g)$/i.test(props.src.split("?")[0])) throw new TypeError(`Environment: src must be an equirectangular .hdr, .exr, .png or .jpg, got ${JSON.stringify(props.src)}`);
    if (props.blur !== undefined) throw new TypeError(`Environment: blur is the room's (with room), not a panorama's`);
  }
  for (const k of ["intensity", "backgroundIntensity", "blur"] as const) {
    const v = props[k];
    if (v !== undefined && !(typeof v === "number" && v >= 0 && Number.isFinite(v))) throw new RangeError(`Environment: ${k} must be 0 or more, got ${v}`);
  }
  if (props.rotation !== undefined && !(typeof props.rotation === "number" && Number.isFinite(props.rotation))) throw new RangeError(`Environment: rotation must be a number of radians, got ${props.rotation}`);
  checkPortals(props.portals);
  return node3d("Environment", props as unknown as Record<string, unknown>);
}

export interface EnvironmentMapLightProps {
  /** the prefiltered diffuse (irradiance) cubemap, a .ktx2 file: staticFile("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2") */
  diffuse_map: string;
  /** the prefiltered specular (radiance) cubemap with its mips, a .ktx2 file */
  specular_map: string;
  /** cd/m² (Bevy's default 1) */
  intensity?: number;
  /** its turn, glam's Quat [x, y, z, w] (Bevy's identity): Quat.from_rotation_y(0.2 * t) turns it each frame */
  rotation?: [number, number, number, number];
}

/** a rotation as glam's Quat, unit length */
function checkQuat(who: string, q: unknown) {
  if (q === undefined) return;
  if (!(Array.isArray(q) && q.length === 4 && q.every((x) => typeof x === "number" && Number.isFinite(x)) && Math.abs(Math.hypot(...q) - 1) < 1e-3)) throw new TypeError(`${who}: rotation is a unit Quat [x, y, z, w], got ${JSON.stringify(q)}`);
}

/** Bevy's EnvironmentMapLight with its own prefiltered maps, as Bevy's examples put one on the camera (docs/control.feature
 *  FC0a): light from all around and its reflections, from cubemaps already filtered (an <Environment> filters a panorama
 *  itself). Put it inside the Canvas3D, once; the first frame's holds.
 *  @bevy bevy_light::probe::EnvironmentMapLight */
export function EnvironmentMapLight(props: EnvironmentMapLightProps) {
  for (const k of ["diffuse_map", "specular_map"] as const) {
    if (typeof props[k] !== "string" || !/\.ktx2$/i.test(props[k])) throw new TypeError(`EnvironmentMapLight: ${k} is a prefiltered cubemap's .ktx2 file, got ${JSON.stringify(props[k])}`);
  }
  if (props.intensity !== undefined && !(typeof props.intensity === "number" && props.intensity >= 0 && Number.isFinite(props.intensity))) throw new RangeError(`EnvironmentMapLight: intensity is cd/m², 0 or more, got ${props.intensity}`);
  checkQuat("EnvironmentMapLight", props.rotation);
  return node3d("Environment", { diffuse_map: assetPath(props.diffuse_map), specular_map: assetPath(props.specular_map), intensity: props.intensity ?? 1, ...(props.rotation ? { rotation: props.rotation } : {}) });
}

export interface SkyboxProps {
  /** the sky's cubemap, a .ktx2 file (Bevy's examples use the specular map: environment_maps/pisa_specular_rgb9e5_zstd.ktx2) */
  image: string;
  /** cd/m² (Bevy's default 0, a black sky: Bevy's examples give it 1000 to 5000) */
  brightness?: number;
  /** its turn, glam's Quat [x, y, z, w] (Bevy's identity) */
  rotation?: [number, number, number, number];
}

/** Bevy's Skybox: a cubemap drawn behind everything, as Bevy's examples put one on the camera. Put it inside the
 *  Canvas3D, once; its brightness and rotation may change each frame (an <Environment>'s `background` is the sky of
 *  a panorama raea filters).
 *  @bevy bevy_light::probe::Skybox */
export function Skybox(props: SkyboxProps) {
  if (typeof props.image !== "string" || !/\.ktx2$/i.test(props.image)) throw new TypeError(`Skybox: image is a cubemap's .ktx2 file, got ${JSON.stringify(props.image)}`);
  const b = props.brightness ?? 0;
  if (!(typeof b === "number" && b >= 0 && Number.isFinite(b))) throw new RangeError(`Skybox: brightness is cd/m², 0 or more, got ${b}`);
  checkQuat("Skybox", props.rotation);
  return node3d("Skybox", { image: assetPath(props.image), brightness: b, ...(props.rotation ? { rotation: props.rotation } : {}) });
}

export interface AmbientOcclusionProps {
  /** how many samples a pixel takes: "low" (4), "medium" (8, the default), "high" (18), "ultra" (54) */
  quality?: "low" | "medium" | "high" | "ultra";
  /** how thick, in metres, a surface is taken to be behind what the camera sees (0.25 by default): larger darkens more
   * behind thin things */
  thickness?: number;
}

/** Screen-space ambient occlusion: creases, corners and the ground under things darken where the sky's and the room's
 * light can't reach. It darkens ambient and environment light (an <AmbientLight>, an <Environment>), not the direct
 * light of a sun or a lamp. Turns multisampling off, so the edges are smoothed by SMAA instead (see <AntiAliasing>).
 * Put it inside the Canvas3D, once; each frame's is its own, so a shot can have it and the next not.
 * @bevy bevy_pbr::ssao::ScreenSpaceAmbientOcclusion */
export function AmbientOcclusion(props: AmbientOcclusionProps) {
  if (props.quality !== undefined && !["low", "medium", "high", "ultra"].includes(props.quality)) throw new RangeError(`AmbientOcclusion: quality must be "low", "medium", "high" or "ultra", got ${JSON.stringify(props.quality)}`);
  if (props.thickness !== undefined && !(typeof props.thickness === "number" && props.thickness > 0 && Number.isFinite(props.thickness))) throw new RangeError(`AmbientOcclusion: thickness must be a positive number of metres, got ${props.thickness}`);
  return node3d("AmbientOcclusion", props as unknown as Record<string, unknown>);
}

export interface AntiAliasingProps {
  /** "msaa" (4x multisampling, the default), "smaa" (morphological, high preset: the default with AmbientOcclusion,
   * which can't multisample), "fxaa" (cheapest, softest), "none", or "steady": raea's temporal filter, each frame
   * drawn from a slightly different place inside its pixels and blended with the frames before it, so what one frame
   * can only draw as a pattern of pixels (levels cross-fading by distance, far detail a pixel wide, noisy light)
   * settles to its true share; single-sampled, with depth and motion prepasses (on Berlin's fly, faster than msaa).
   * The others draw a frame from that frame alone, so a render equals its play; "steady" needs the frames before, so a
   * render that starts partway through differs from its play for its first few frames */
  mode: "msaa" | "smaa" | "fxaa" | "none" | "steady";
  /** "steady"'s rates: the frames a still pixel's history averages (32), a moving one's (8), and the motion in pixels
   * at which a pixel counts as moving (4). Fewer frames follow change sooner and settle less */
  steady?: { still?: number; moving?: number; px?: number };
}

/** How the 3D's edges are smoothed. Put it inside the Canvas3D, once; each frame's is its own.
 * @bevy bevy_anti_alias::smaa::Smaa
 * @bevy bevy_anti_alias::fxaa::Fxaa */
export function AntiAliasing(props: AntiAliasingProps) {
  if (!["msaa", "smaa", "fxaa", "none", "steady"].includes(props.mode)) throw new RangeError(`AntiAliasing: mode must be "msaa", "smaa", "fxaa", "none" or "steady", got ${JSON.stringify(props.mode)}`);
  for (const [k, v] of Object.entries(props.steady ?? {})) if (!(typeof v === "number" && v > 0)) throw new RangeError(`AntiAliasing: steady.${k} must be a number over 0, got ${v}`);
  return node3d("AntiAliasing", props as unknown as Record<string, unknown>);
}

export interface ShadowsProps {
  /** each shadow map's size in texels (4096 by default; 8192 is sharper still, at four times the memory and about
   * 2-3 ms a frame at 1080p) */
  mapSize?: 1024 | 2048 | 4096 | 8192;
  /** how far from the camera the sun's shadows reach, in metres (150 by default): the shorter, the sharper they are */
  distance?: number;
  /** where the sun's first, sharpest cascade ends, in metres (8 by default): the distance of the nearest subject */
  firstCascade?: number;
  /** how many cascades split the distance (4 by default, 1-4) */
  cascades?: number;
  /** how much neighbouring cascades overlap, 0-1 (0.3 by default) so the seam between them doesn't show */
  overlap?: number;
  /** depth bias: raise it if surfaces facing the light show stripes of their own shadow (acne); too high and shadows
   * detach from what casts them (0.02 by default for the sun, as Bevy's) */
  bias?: number;
  /** normal bias, in texels: the same trade-off along the surface's normal (1.8 by default for the sun) */
  normalBias?: number;
  /** "gaussian" (the default: soft, smooth edges) or "hardware" (2x2, crisper and cheaper) */
  filter?: "gaussian" | "hardware";
}

/** Shadow quality for every light in the Canvas3D: map size, the sun's cascades, biases, filtering. Put it inside the
 * Canvas3D, once; each frame's is its own. mapSize sets both shadow maps' size, filter the camera's
 * filtering, and the cascade props the sun's cascades (engine/render scene3d.rs):
 * @bevy bevy_light::directional_light::DirectionalLightShadowMap
 * @bevy bevy_light::point_light::PointLightShadowMap
 * @bevy bevy_light::ShadowFilteringMethod
 * @bevy bevy_light::cascade::CascadeShadowConfig */
export function Shadows(props: ShadowsProps) {
  if (props.mapSize !== undefined && ![1024, 2048, 4096, 8192].includes(props.mapSize)) throw new RangeError(`Shadows: mapSize must be 1024, 2048, 4096 or 8192, got ${props.mapSize}`);
  for (const k of ["distance", "firstCascade"] as const) {
    const v = props[k];
    if (v !== undefined && !(typeof v === "number" && v > 0 && Number.isFinite(v))) throw new RangeError(`Shadows: ${k} must be a positive number of metres, got ${v}`);
  }
  if (props.cascades !== undefined && !(Number.isInteger(props.cascades) && props.cascades >= 1 && props.cascades <= 4)) throw new RangeError(`Shadows: cascades must be 1, 2, 3 or 4, got ${props.cascades}`);
  if (props.overlap !== undefined && !(props.overlap >= 0 && props.overlap < 1)) throw new RangeError(`Shadows: overlap must be from 0 to below 1, got ${props.overlap}`);
  for (const k of ["bias", "normalBias"] as const) {
    const v = props[k];
    if (v !== undefined && !(typeof v === "number" && v >= 0 && Number.isFinite(v))) throw new RangeError(`Shadows: ${k} must be 0 or more, got ${v}`);
  }
  if (props.filter !== undefined && !["gaussian", "hardware"].includes(props.filter)) throw new RangeError(`Shadows: filter must be "gaussian" or "hardware", got ${JSON.stringify(props.filter)}`);
  return node3d("Shadows", props as unknown as Record<string, unknown>);
}
