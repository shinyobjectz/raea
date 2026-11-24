// Ray-traced lighting: Bevy's Solari (docs/raytracing.feature RT3), on the camera it's inside.
import { BevyComponent } from "../3d";
import { shaderConstants } from "./shaders";

export interface SolariLightingProps {
  /** ReSTIR: reuse light samples between pixels and frames (finer shadows, steadier in hard light; slower). Off by
   * default, as Bevy has it */
  restir?: boolean;
  /** how many times light bounces (Bevy's default when left out) */
  bounces?: number;
  /** start the lighting's history again (a cut) */
  reset?: boolean;
  /** with ReSTIR, the most frames a pixel's reused samples may count for: higher is steadier and slower to follow a
   * change in the light (Bevy's 8) */
  confidenceCap?: number;
  /** direct light samples at the camera's first hit, a pixel a frame (Bevy's 8). After MetalFX's denoiser 4 bought
   * the window scene nothing over 8 and saved 1.5 ms at 1080p's pixels */
  primarySamples?: number;
  /** direct light samples at each bounce (Bevy's 4) */
  secondarySamples?: number;
  /** the chance a pixel traces its first bounce this frame, its light divided by it (Russian roulette): 0.5 halves the
   * bounce's tracing, most of a dense forest's (1, every pixel every frame) */
  firstBounce?: number;
  /** Solari's world cache, the bounce light it keeps in cells across the scene */
  cache?: {
    /** frames a cell's light is gathered over: steadier and slower to follow a change (Bevy's 32) */
    frames?: number;
    /** direct light samples a cell takes a frame (32) */
    lightSamples?: number;
    /** the farthest a ray between cells is traced, metres; past it the sky counts only if the ray reaches it (1000) */
    reach?: number;
    /** about how many cells are updated a frame (40000) */
    updates?: number;
    /** a nearest cell's size, metres (0.15) */
    cellSize?: number;
    /** how fast cells grow with distance from the camera (15) */
    lodScale?: number;
  };
  /** draw the view at this fraction of its size a side (a third to 1) and let MetalFX's temporal scaler bring it up to
   * the full size, so a game traces fewer pixels: 0.67 traces 45% of them (RT9). On the Mac; elsewhere it draws at the
   * full size and raea says so. 1 (or left out): the full size; RAEA_SOLARI_SCALE overrides it */
  scale?: number;
  /** MetalFX's temporal denoiser on the lighting (RT4). false shows Solari's raw noise. Default true;
   * RAEA_SOLARI_DENOISE=0 overrides it */
  denoise?: boolean;
  /** the denoiser guided by Solari's own guides, where a mirror's pixel takes what it reflects (RT17); false guides it
   * by the G-buffer. Default true; RAEA_SOLARI_PSR=0 overrides it */
  guides?: boolean;
  /** a glossy surface's pixel as far off as what it reflects in the denoiser's depth, so a reflection doesn't trail
   * what it shows (RT34). Default true; RAEA_SOLARI_PSR_DEPTH=0 overrides it */
  reflectionDepth?: boolean;
  /** an emissive surface's own light: "taa" taken out before the denoiser and gathered over frames (RT11, RT32),
   * "split" each frame's alone, "mask" left in for the denoiser. Default "taa"; RAEA_SOLARI_EMISSIVE overrides it */
  emissive?: "taa" | "split" | "mask";
  /** a view drawn small (scale below 1): denoised and brought up in "one" pass, or denoised small and scaled in "two".
   * Default "one"; RAEA_SOLARI_UPSCALE overrides it */
  upscale?: "one" | "two";
  /** what MetalFX divides the light by before it denoises and multiplies it by after. Higher keeps more of a rare
   * bright sample (a lamp found by one path in many) and lets more fireflies through: 1 kept 0.74 of the desk's
   * light, 16 0.86, 64 0.91. Default 16; RAEA_METALFX_PREEXPOSURE overrides it */
  preexposure?: number;
  /** jitter a full-size view too, so the denoiser anti-aliases its edges as it gathers frames. Default true;
   * RAEA_SOLARI_AA=0 overrides it */
  aa?: boolean;
  /** which of MetalFX's passes is told the camera's jitter: "denoiser", "scaler", "both", "flip" (the denoiser,
   * negated), "none" (and the view isn't jittered). For A/Bs of RT9; default "denoiser"; RAEA_SOLARI_JITTER ("" the
   * denoiser, "0" none) overrides it */
  jitter?: "denoiser" | "scaler" | "both" | "flip" | "none";
  /** how fast a sample's weight falls with its distance from a pixel's centre in the emission's history, exp(-k d²) in
   * output pixels: lower is softer. Default 6; RAEA_EMIT_TAA_SHARP overrides it */
  emissiveSharpness?: number;
  /** the jitter's cycle at the full size, in frames: a view drawn at s of its size a side cycles through
   * jitterFrames / s² offsets, and an emissive twig glimpsed through it may stay away that long. Default 8 (DLSS's) */
  jitterFrames?: number;
  /** the traced scene follows what moves (a trial, false by default): a skinned character's traced twin is posed each
   * frame, so it casts traced shadows and bounces light. Decided as each mesh is first traced, so set it from the first
   * frame. (A hidden part, or one a frame leaves out, leaves the traced scene with or without it.) */
  dynamic?: boolean;
  /** constants in Solari's and the denoiser's shaders, compiled in when the scene loads (a value changed later in the
   * composition isn't drawn); each left out keeps the shader's own, given in brackets. <ShaderConstants> reaches the
   * rest */
  constants?: {
    /** how far ReSTIR looks for a neighbour's light samples, pixels (30): smaller keeps a shadow's edge, larger evens
     * out noise */
    spatialReuse?: number;
    /** a surface smoother than this roughness is traced as a perfect mirror (0.001) */
    mirrorRoughness?: number;
    /** a bounce whose lobe is at least this wide, steradians, ends its path in the world cache (π): smaller ends paths
     * sooner, cheaper and blotchier */
    cacheEnd?: number;
    /** frames a world-cache cell lives without being read (10) */
    cacheLifetime?: number;
    /** tries to find a world-cache cell after a hash collision (3) */
    cacheSearch?: number;
    /** ReSTIR's reconnection of a path to a neighbour's: only at a surface at least this rough (0.6) */
    reconnectRoughness?: number;
    /** past this distance, metres, the far surface's roughness floor falls with the ray's length (1) */
    reconnectRelax?: number;
    /** only when the bounce's footprint is at least this percent of the pixel's (0.02) */
    reconnectFootprint?: number;
    /** the least chance a glossy surface's path samples a light directly, so a small bright light shows in it (0.25) */
    glossyLightFloor?: number;
    /** the mirror guides' rules (RT17): when a surface is drawn as what it reflects */
    mirror?: {
      /** under this roughness a reflection's own motion is traced, so the denoiser follows what it reflects (0.0625) */
      motionRoughness?: number;
      /** the specular share over which such a pixel takes its reflection's depth (RT34, 0.5) */
      depthShare?: number;
      /** the share of its reflectance that must leave through the mirror lobe (0.9) */
      replaceFraction?: number;
      /** how far its reflection's depth may shift between neighbouring pixels, as a share of that depth, before it
       * counts as too curved (0.002) */
      depthJitter?: number;
      /** the normal change a flat surface still measures, the G-buffer's rounding (0.002) */
      normalFloor?: number;
      /** metres standing in for "infinitely far" when a mirror ray finds nothing (10000) */
      skyDistance?: number;
    };
    /** the most frames an emissive surface's own light is gathered over (emissive "taa", 16): higher is smoother and
     * slower to follow a change */
    emissiveHistory?: number;
    /** how an emissive surface's history is held to the light about it now: a still pixel within the darkest times
     * stillLow (0.95) and the brightest times stillHigh (1.05); a moving one under the brightest times movingGain (4)
     * plus movingFloor (0.02) */
    emissiveClamp?: { stillLow?: number; stillHigh?: number; movingGain?: number; movingFloor?: number };
  };
}

/** <SolariLighting constants> as the engine's shader constants (docs/nobuild.feature NB1a), or undefined. */
export function solariConstants(c: SolariLightingProps["constants"]) {
  if (!c) return undefined;
  const at = (v: number | undefined, prop: string, whole = false) => {
    if (v !== undefined && (!(Number.isFinite(v) && v >= 0) || (whole && !Number.isInteger(v)))) throw new RangeError(`SolariLighting: constants.${prop} is ${whole ? "a whole number" : "a number"} of 0 or more, got ${v}`);
    return v;
  };
  const restir = "bevy_solari/realtime/restir.wesl";
  const path = "bevy_solari/realtime/initial_path.wesl";
  const cache = "bevy_solari/realtime/world_cache_query.wesl";
  const mirror = "bevy_solari/realtime/resolve_dlss_rr_textures.wesl";
  const m = c.mirror ?? {};
  const e = c.emissiveClamp ?? {};
  const taa = "raea_render_raytrace/emit_taa.wgsl";
  return shaderConstants([
    [restir, "SPATIAL_REUSE_RADIUS_PIXELS", at(c.spatialReuse, "spatialReuse")],
    ["bevy_solari/scene/bindings.wesl", "MIRROR_ROUGHNESS_THRESHOLD", at(c.mirrorRoughness, "mirrorRoughness")],
    [path, "CACHE_TERMINATION_MIN_SOLID_ANGLE", at(c.cacheEnd, "cacheEnd")],
    [cache, "WORLD_CACHE_CELL_LIFETIME", at(c.cacheLifetime, "cacheLifetime", true)],
    [cache, "WORLD_CACHE_MAX_SEARCH_STEPS", at(c.cacheSearch, "cacheSearch", true)],
    [path, "RECONNECTION_ROUGHNESS_MIN", at(c.reconnectRoughness, "reconnectRoughness")],
    [path, "RECONNECTION_RELAX_DISTANCE", at(c.reconnectRelax, "reconnectRelax")],
    [path, "RECONNECTION_FOOTPRINT_KAPPA", at(c.reconnectFootprint, "reconnectFootprint")],
    [path, "GLOSSY_NEE_FLOOR", at(c.glossyLightFloor, "glossyLightFloor")],
    [mirror, "SPECULAR_MOTION_ROUGHNESS", at(m.motionRoughness, "mirror.motionRoughness")],
    [mirror, "REFLECTION_DEPTH_SHARE", at(m.depthShare, "mirror.depthShare")],
    [mirror, "FULL_REPLACEMENT_FRACTION", at(m.replaceFraction, "mirror.replaceFraction")],
    [mirror, "MAX_VIRTUAL_DEPTH_JITTER", at(m.depthJitter, "mirror.depthJitter")],
    [mirror, "NORMAL_QUANTIZATION_FLOOR", at(m.normalFloor, "mirror.normalFloor")],
    [mirror, "ENVIRONMENT_VIRTUAL_DISTANCE", at(m.skyDistance, "mirror.skyDistance")],
    [taa, "HISTORY_CAP", at(c.emissiveHistory, "emissiveHistory")],
    [taa, "STILL_LOW", at(e.stillLow, "emissiveClamp.stillLow")],
    [taa, "STILL_HIGH", at(e.stillHigh, "emissiveClamp.stillHigh")],
    [taa, "MOVING_GAIN", at(e.movingGain, "emissiveClamp.movingGain")],
    [taa, "MOVING_FLOOR", at(e.movingFloor, "emissiveClamp.movingFloor")],
  ]);
}

/** <SolariLighting>'s denoiser, guide, emission and jitter props as raea_render_raytrace::SolariSettings' fields (only
 * those given; the rest keep the engine's defaults). An environment variable of the same setting overrides it, for an
 * A/B across scenes (RAEA_SOLARI_DENOISE, RAEA_METALFX_PREEXPOSURE, ...; docs/nobuild.feature NB1a). */
export function solariSettings(p: SolariLightingProps): Record<string, unknown> | undefined {
  const pos = (n: string, v: number | undefined) => {
    if (v !== undefined && !(Number.isFinite(v) && v > 0)) throw new RangeError(`SolariLighting: ${n} is a number above 0, got ${v}`);
  };
  pos("preexposure", p.preexposure);
  pos("emissiveSharpness", p.emissiveSharpness);
  pos("jitterFrames", p.jitterFrames);
  const one = <T extends string>(n: string, v: T | undefined, of: readonly T[]) => {
    if (v !== undefined && !of.includes(v)) throw new RangeError(`SolariLighting: ${n} is one of ${of.join(", ")}, got ${v}`);
  };
  one("emissive", p.emissive, ["taa", "split", "mask"] as const);
  one("upscale", p.upscale, ["one", "two"] as const);
  one("jitter", p.jitter, ["denoiser", "scaler", "both", "flip", "none"] as const);
  const v: Record<string, unknown> = {};
  if (p.denoise !== undefined) v.denoise = p.denoise;
  if (p.guides !== undefined) v.guides = p.guides;
  if (p.reflectionDepth !== undefined) v.reflection_depth = p.reflectionDepth;
  if (p.emissive !== undefined) v.emissive = p.emissive;
  if (p.upscale !== undefined) v.upscale = p.upscale;
  if (p.preexposure !== undefined) v.preexposure = p.preexposure;
  if (p.aa !== undefined) v.aa = p.aa;
  // the engine's names for the passes (upscale.rs jitter_gain): "" the denoiser, "0" none
  if (p.jitter !== undefined) v.jitter = p.jitter === "denoiser" ? "" : p.jitter === "none" ? "0" : p.jitter;
  if (p.emissiveSharpness !== undefined) v.emissive_sharpness = p.emissiveSharpness;
  if (p.jitterFrames !== undefined) v.jitter_frames = p.jitterFrames;
  if (p.dynamic !== undefined) v.dynamic = p.dynamic;
  return Object.keys(v).length ? v : undefined;
}

/** Ray-traced direct and indirect light for the <Camera3d> it's inside (Bevy's SolariLighting): shadows, bounce
 * light and reflections traced against the scene, in place of shadow maps. Needs a GPU with ray queries (Apple
 * silicon on Metal, RTX on Vulkan); elsewhere the scene draws raster-lit and raea says so. On the Mac MetalFX's
 * temporal denoiser cleans it up (RT4; denoise={false} shows the raw noise). A <Character> (skinned, morphed) stays
 * raster-lit: Bevy 0.20's Solari can't trace it (RT5), unless `dynamic` gives a skinned one a traced twin. */
export function SolariLighting(p: SolariLightingProps = {}) {
  if (p.bounces !== undefined && !(Number.isInteger(p.bounces) && p.bounces >= 0)) throw new RangeError(`SolariLighting: bounces is a whole number, got ${p.bounces}`);
  if (p.scale !== undefined && !(p.scale >= 1 / 3 && p.scale <= 1)) throw new RangeError(`SolariLighting: scale is a third to 1 (of the view's size a side), got ${p.scale}`);
  const value: Record<string, unknown> = {};
  if (p.restir !== undefined) value.restir = p.restir;
  if (p.bounces !== undefined) value.max_bounces = p.bounces;
  if (p.reset !== undefined) value.reset = p.reset;
  const field = (prop: string, name: string, v: number | undefined, whole = false) => {
    if (v === undefined) return;
    if (!(Number.isFinite(v) && v >= 0) || (whole && !Number.isInteger(v))) throw new RangeError(`SolariLighting: ${prop} is ${whole ? "a whole number" : "a number"} of 0 or more, got ${v}`);
    value[name] = v;
  };
  field("confidenceCap", "confidence_weight_cap", p.confidenceCap);
  field("primarySamples", "primary_di_samples", p.primarySamples, true);
  field("secondarySamples", "secondary_di_samples", p.secondarySamples, true);
  if (p.firstBounce !== undefined && !(p.firstBounce > 0 && p.firstBounce <= 1)) throw new RangeError(`SolariLighting: firstBounce is a chance above 0, up to 1, got ${p.firstBounce}`);
  field("firstBounce", "first_bounce_probability", p.firstBounce);
  const c = p.cache ?? {};
  field("cache.frames", "world_cache_max_temporal_samples", c.frames);
  field("cache.lightSamples", "world_cache_direct_light_sample_count", c.lightSamples, true);
  field("cache.reach", "world_cache_max_gi_ray_distance", c.reach);
  field("cache.updates", "world_cache_cell_updates_soft_target", c.updates, true);
  field("cache.cellSize", "world_cache_position_base_cell_size", c.cellSize);
  field("cache.lodScale", "world_cache_position_lod_scale", c.lodScale);
  const settings = solariSettings(p);
  const constants = solariConstants(p.constants);
  const lighting = <BevyComponent type="bevy_solari::realtime::SolariLighting" value={value} />;
  const scaled = p.scale !== undefined && p.scale !== 1;
  if (!scaled && !settings && !constants) return lighting;
  return (
    <>
      {lighting}
      {scaled && <BevyComponent type="raea_render_raytrace::RenderScale" value={{ scale: p.scale }} />}
      {settings && <BevyComponent type="raea_render_raytrace::SolariSettings" value={settings} />}
      {constants}
    </>
  );
}

export interface PathtracerProps {
  /** start the accumulation again */
  reset?: boolean;
  /** how many times a render draws each frame, each draw one more sample a pixel, the last kept (1 by default): 256 or
   * more converges a reference still. Its accumulation starts afresh each frame, so a moved part isn't smeared */
  samples?: number;
  /** whether its rays walk the fog volumes, scattering in the air (true by default); false for a frame without the
   * air */
  media?: boolean;
}

/** Bevy's reference path tracer for the <Camera3d> it's inside: slow and exact, accumulating over frames while the
 * view holds still. For stills to check a look against, not for motion. Same GPU needs as <SolariLighting>. */
export function Pathtracer(p: PathtracerProps = {}) {
  if (p.samples !== undefined && !(Number.isInteger(p.samples) && p.samples >= 1)) throw new RangeError(`Pathtracer: samples must be a whole number from 1, got ${p.samples}`);
  const settings = p.samples !== undefined || p.media !== undefined;
  return (
    <>
      <BevyComponent type="bevy_solari::pathtracer::Pathtracer" value={{ reset: p.reset ?? false }} />
      {settings && <BevyComponent type="raea_sys_rt::pathtracer::PathtracerSettings" value={{ samples: p.samples ?? 1, media: p.media ?? true }} />}
    </>
  );
}
