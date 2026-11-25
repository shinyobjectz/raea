// Materials: standard, toon and extended (their colours in colors.ts, the extensions in extensions.tsx).
import { isValidElement, memo, useContext, type ReactElement } from "react";
import type { TrackSpec } from "@raea/math/animate";
import { InModel, sameProps } from "../3d/all";
import { LinearRgba, MADE_FROM, baseOf, emissiveOf, GridExtension, PaintExtension, WearExtension, ShaderExtension, ToonExtension, wearOf, type Affine2, type Srgba, type Only, type GridExtensionProps, type PaintExtensionProps, type WearExtensionProps, type ShaderExtensionProps, type ToonExtensionProps } from "./all";
import { samplerSpec, surfaceShader, type SamplerSpec, type SurfaceDrawProps } from "../3d";
import { MeshBasicMaterial, MeshPhysicalMaterial, MeshStandardMaterial, MeshToonMaterial, type MeshStandardMaterialProps } from "../three";
import { ProceduralExtension, proceduralOf, type ProceduralExtensionProps } from "./procedural";

/** A picture read with its own sampler: Bevy's load_with_settings, whose ImageLoaderSettings give the Image its
 *  ImageSamplerDescriptor ({ path: "checker.png", sampler: { filter: "nearest" } } for hard pixels). */
export interface SampledImage {
  /** the picture, in the media folder */
  path: string;
  /** how it's sampled: filter, address modes */
  sampler: SamplerSpec;
}

/** a texture field: a picture in the media folder, or one with its sampler */
type Tex = string | SampledImage;

/** Separate greyscale maps packed into Bevy's metallic_roughness_texture when they load: the roughness map's grey as
 *  its green, the metalness map's as its blue (white without one, so metallic is its factor exactly). Poly Haven's
 *  "_rough" sets come this way; a packed "_arm" map is a plain path. */
export interface MetallicRoughnessMaps {
  /** a greyscale roughness map (white rough) */
  roughness?: string;
  /** a greyscale metalness map (white metal) */
  metalness?: string;
}

/** Bevy's AlphaMode, as it reflects. */
export type AlphaMode = "Opaque" | "Blend" | "Premultiplied" | "Add" | "Multiply" | "AlphaToCoverage" | { Mask: number };

/** Bevy's StandardMaterial fields (colours as CSS colours, textures as file paths), with Bevy's defaults. */
export interface StandardMaterialProps {
  /** a colour, or Bevy's Srgba (its alpha shows with alpha_mode "Blend": Srgba.hex("#88ccff").with_alpha(0.5)) */
  base_color?: string | Srgba;
  /** a picture in the media folder; null (Bevy's None) inside a Model drops the model's own base map */
  base_color_texture?: Tex | null;
  /** a colour, or Bevy's LinearRgba for one brighter than white (LinearRgba.from(Srgba.hex("#ff8800")).mul(3)) */
  emissive?: string | LinearRgba;
  emissive_texture?: Tex;
  /** 0.5 by Bevy's default */
  perceptual_roughness?: number;
  metallic?: number;
  metallic_roughness_texture?: Tex | MetallicRoughnessMaps;
  normal_map_texture?: Tex;
  occlusion_texture?: Tex;
  /** every texture's UVs scaled, turned and moved (Affine2.fromScale([5, 5]) tiles a map five times across each face;
   *  a scaled texture repeats) */
  uv_transform?: Affine2;
  /** 0.5 by Bevy's default */
  reflectance?: number;
  clearcoat?: number;
  clearcoat_perceptual_roughness?: number;
  /** the clearcoat layer's own normal map, read as data (not sRGB), as Bevy's examples load it with is_srgb false */
  clearcoat_normal_texture?: Tex;
  specular_transmission?: number; ior?: number; thickness?: number;
  unlit?: boolean;
  alpha_mode?: AlphaMode;
  double_sided?: boolean;
  /** the faces not drawn: "Back" (Bevy's default), "Front", or null for neither */
  cull_mode?: "Back" | "Front" | null;
  /** in a <Model>: the glTF material it re-covers, as MeshStandardMaterial's */
  material?: string;
  /** false: left out of the scene's <Fog> */
  fog_enabled?: boolean;
  /** the highlight's colour, over `reflectance` */
  specular_tint?: string;
  /** stretched highlights (brushed metal, hair), 0..1, turned anisotropy_rotation radians along the tangent */
  anisotropy_strength?: number;
  anisotropy_rotation?: number;
  /** light absorbed through a transmissive volume (with specular_transmission and thickness) */
  attenuation_distance?: number;
  attenuation_color?: string;
  /** a greyscale height map (white deep) the parallax fields carve a flat face with */
  depth_map?: Tex;
  /** 0.1 by Bevy's default */
  parallax_depth_scale?: number;
  /** "Occlusion" (Bevy's default) or { Relief: { max_steps } } */
  parallax_mapping_method?: "Occlusion" | { Relief: { max_steps: number } };
  /** 16 by Bevy's default */
  max_parallax_layer_count?: number;
  /** drawn this much nearer in depth (a decal-like layer over a coplanar face) */
  depth_bias?: number;
  /** "Forward" or "Deferred" (in the deferred pass, with a camera that has one) */
  opaque_render_method?: "Forward" | "Deferred";
  /** light passed through and scattered (leaves, paper, wax), 0..1; lightmap_exposure: a <Lightmap>'s bake times this */
  diffuse_transmission?: number; lightmap_exposure?: number;
  /** its fields' tracks, sampled by the engine from the frame clock: { base_color: track({ 0: "#3060ff", 30: "#ff4070" }) } */
  animate?: StandardMaterialAnimate;
}

/** The StandardMaterial fields `animate` takes, each a track of its own kind of value. An emissive brighter than white
 *  is LinearRgba keys of one colour: track({ 0: E.mul(0), 30: E.mul(2.5) }) with E = LinearRgba.from(Srgba.hex(c)). */
export interface StandardMaterialAnimate {
  /** a track of `base_color`, sampled each frame */
  base_color?: TrackSpec<string>;
  /** a track of `emissive`, sampled each frame */
  emissive?: TrackSpec<string> | TrackSpec<LinearRgba>;
  /** a track of `perceptual_roughness`, sampled each frame */
  perceptual_roughness?: TrackSpec<number>;
  /** a track of `metallic`, sampled each frame */
  metallic?: TrackSpec<number>;
  // the rest of the fields that interpolate, each set on the part's own material frame by frame (docs/control.feature FC1c)
  /** a track of `emissive_exposure_weight`, sampled each frame */
  emissive_exposure_weight?: TrackSpec<number>;
  /** a track of `reflectance`, sampled each frame */
  reflectance?: TrackSpec<number>;
  /** a track of `specular_tint`, sampled each frame */
  specular_tint?: TrackSpec<string>;
  /** a track of `diffuse_transmission`, sampled each frame */
  diffuse_transmission?: TrackSpec<number>;
  /** a track of `specular_transmission`, sampled each frame */
  specular_transmission?: TrackSpec<number>;
  /** a track of `thickness`, sampled each frame */
  thickness?: TrackSpec<number>;
  /** a track of `ior`, sampled each frame */
  ior?: TrackSpec<number>;
  /** a track of `attenuation_distance`, sampled each frame */
  attenuation_distance?: TrackSpec<number>;
  /** a track of `attenuation_color`, sampled each frame */
  attenuation_color?: TrackSpec<string>;
  /** a track of `clearcoat`, sampled each frame */
  clearcoat?: TrackSpec<number>;
  /** a track of `clearcoat_perceptual_roughness`, sampled each frame */
  clearcoat_perceptual_roughness?: TrackSpec<number>;
  /** a track of `anisotropy_strength`, sampled each frame */
  anisotropy_strength?: TrackSpec<number>;
  /** a track of `anisotropy_rotation`, sampled each frame */
  anisotropy_rotation?: TrackSpec<number>;
  /** a track of `depth_bias`, sampled each frame */
  depth_bias?: TrackSpec<number>;
  /** a track of `parallax_depth_scale`, sampled each frame */
  parallax_depth_scale?: TrackSpec<number>;
  /** a track of `max_parallax_layer_count`, sampled each frame */
  max_parallax_layer_count?: TrackSpec<number>;
  /** a track of `lightmap_exposure`, sampled each frame */
  lightmap_exposure?: TrackSpec<number>;
}

// the StandardMaterialAnimate fields past the four columns, by kind: sampled as `field:<name>` and set by reflection
const STANDARD_FIELDS: Record<string, "number" | "string"> = {
  emissive_exposure_weight: "number", reflectance: "number", specular_tint: "string", diffuse_transmission: "number", specular_transmission: "number",
  thickness: "number", ior: "number", attenuation_distance: "number", attenuation_color: "string", clearcoat: "number",
  clearcoat_perceptual_roughness: "number", anisotropy_strength: "number", anisotropy_rotation: "number", depth_bias: "number",
  parallax_depth_scale: "number", max_parallax_layer_count: "number", lightmap_exposure: "number",
};


// each animatable field's name in the old material's animate
const STANDARD_ANIMATE: Record<string, string> = { base_color: "color", emissive: "emissive", perceptual_roughness: "roughness", metallic: "metalness" };
const TOON_ANIMATE: Record<string, string> = { shadow_color: "shadowColor", rim_intensity: "rimIntensity", highlight_intensity: "highlightIntensity", outline_width: "outlineWidth" };
const COLOUR_FIELDS = new Set(["color", "emissive", "shadowColor"]);

// an `animate` as the old material's: its keys renamed; an emissive's LinearRgba keys as their colour and an
// emissive_nits track, Bevy's strength as written (a colour track's emissive is at strength 1, as Bevy's emissive is the whole colour)
function animateOf(owner: string, a: object | undefined, names: Record<string, string>, fields: Record<string, "number" | "string"> = {}): { animate?: Record<string, TrackSpec>; emissive?: Record<string, unknown> } {
  if (!a) return {};
  const animate: Record<string, TrackSpec> = {};
  let emissive: Record<string, unknown> | undefined;
  for (const [k, t] of Object.entries(a)) {
    if (t === undefined) continue;
    const name = names[k];
    if (!name && !fields[k]) throw new TypeError(`${owner}: animate takes ${[...Object.keys(names), ...Object.keys(fields)].join(", ")}, not ${k}`);
    const spec = t as TrackSpec<unknown>;
    if (typeof spec !== "object" || spec === null || !Array.isArray(spec.keys)) throw new TypeError(`${owner}: animate's ${k} is a track(...)`);
    if (!name) {
      if (!spec.keys.every(([, v]) => typeof v === fields[k])) throw new TypeError(`${owner}: animate's ${k} keys are ${fields[k] === "string" ? "colours" : "numbers"}`);
      animate[`field:${k}`] = spec as TrackSpec;
      continue;
    }
    if (name === "emissive" && spec.keys.every(([, v]) => v instanceof LinearRgba)) {
      const from = spec.keys.map(([, v]) => (v as LinearRgba)[MADE_FROM]);
      if (from.some((f) => !f || f.hex !== from[0]!.hex)) throw new TypeError(`${owner}: an emissive track's LinearRgba keys are one colour, LinearRgba.from(Srgba.hex(c)).mul(k) each`);
      emissive = { emissive: from[0]!.hex };
      animate.emissive_nits = { ...spec, keys: spec.keys.map(([f], i) => [f, from[i]!.k]) };
      continue;
    }
    const kind = COLOUR_FIELDS.has(name) ? "string" : "number";
    if (!spec.keys.every(([, v]) => typeof v === kind)) throw new TypeError(`${owner}: animate's ${k} keys are ${kind === "string" ? "colours" : "numbers"}`);
    if (name === "emissive") emissive = { emissive_nits: 1 };
    animate[name] = spec as TrackSpec;
  }
  return { animate, emissive };
}

// Bevy's further StandardMaterial fields as the old material's props (docs/shaders.feature SH7)
function further(p: StandardMaterialProps): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (p.fog_enabled !== undefined) out.fog = p.fog_enabled;
  if (p.specular_tint !== undefined) out.specularTint = p.specular_tint;
  if (p.anisotropy_strength !== undefined || p.anisotropy_rotation !== undefined) out.anisotropy = { ...(p.anisotropy_strength !== undefined ? { strength: p.anisotropy_strength } : {}), ...(p.anisotropy_rotation !== undefined ? { rotation: p.anisotropy_rotation } : {}) };
  if (p.attenuation_distance !== undefined || p.attenuation_color !== undefined) out.attenuation = { ...(p.attenuation_distance !== undefined ? { distance: p.attenuation_distance } : {}), ...(p.attenuation_color !== undefined ? { color: p.attenuation_color } : {}) };
  const m = p.parallax_mapping_method;
  if (p.parallax_depth_scale !== undefined || m !== undefined || p.max_parallax_layer_count !== undefined) {
    if (m !== undefined && m !== "Occlusion" && !(typeof m === "object" && m !== null && "Relief" in m)) throw new TypeError(`StandardMaterial: parallax_mapping_method is "Occlusion" or { Relief: { max_steps } }, got ${JSON.stringify(m)}`);
    out.parallax = {
      ...(p.parallax_depth_scale !== undefined ? { scale: p.parallax_depth_scale } : {}),
      ...(m === undefined ? {} : m === "Occlusion" ? { method: "occlusion" } : { method: "relief", steps: m.Relief.max_steps }),
      ...(p.max_parallax_layer_count !== undefined ? { layers: p.max_parallax_layer_count } : {}),
    };
  }
  if (p.depth_bias !== undefined) out.depthBias = p.depth_bias;
  if (p.opaque_render_method !== undefined) {
    if (p.opaque_render_method !== "Forward" && p.opaque_render_method !== "Deferred") throw new TypeError(`StandardMaterial: opaque_render_method is "Forward" or "Deferred", got ${JSON.stringify(p.opaque_render_method)}`);
    out.renderMethod = p.opaque_render_method.toLowerCase();
  }
  for (const [k, o] of [["diffuse_transmission", "diffuseTransmission"], ["lightmap_exposure", "lightmapExposure"]] as const) if (p[k] !== undefined) out[o] = p[k];
  return out;
}

function draw(p: StandardMaterialProps): SurfaceDrawProps {
  const a = p.alpha_mode;
  const alpha =
    a === undefined || a === "Opaque" ? {} : typeof a === "object" ? { alphaMode: "mask" as const, alphaCutoff: a.Mask } : { alphaMode: a === "AlphaToCoverage" ? ("coverage" as const) : (a.toLowerCase() as "opaque" | "blend" | "premultiplied" | "add" | "multiply") };
  // no culling draws both faces; culling the front draws the inside (double_sided is the lighting, cull_mode the faces)
  const side: SurfaceDrawProps["side"] = p.cull_mode === null ? "double" : p.cull_mode === "Front" ? "back" : undefined;
  // Bevy's Blend is the old material's `transparent`; every mode but Opaque reads the base colour's alpha as its
  // opacity (a Mask cuts by it, AlphaToCoverage dithers by it), and Opaque ignores it, as Bevy's do
  const out: SurfaceDrawProps & { transparent?: boolean; opacity?: number } = a === "Blend" ? { transparent: true, opacity: baseOf(p.base_color).alpha } : a === undefined || a === "Opaque" ? {} : { ...alpha, opacity: baseOf(p.base_color).alpha };
  if (side) out.side = side;
  return out;
}

// a scale alone is the old material's `repeat`; a turn or a move adds its uvTransform (over repeat, which keeps the
// textures repeating)
function uvOf(t: Affine2 | undefined): { repeat?: [number, number]; uvTransform?: { offset: [number, number]; rotation: number; scale: [number, number] } } {
  if (!t) return {};
  const [sx, sy] = t.scale;
  if (!(sx > 0 && sy > 0)) throw new RangeError(`StandardMaterial: uv_transform's scale is two positive numbers, got ${JSON.stringify(t.scale)}`);
  const plain = t.angle === 0 && t.translation[0] === 0 && t.translation[1] === 0;
  if (plain) return sx === 1 && sy === 1 ? {} : { repeat: [sx, sy] };
  return { repeat: [sx, sy], uvTransform: { offset: t.translation, rotation: t.angle, scale: t.scale } };
}

/** Bevy's StandardMaterial for the Mesh3d it's inside. Unlit draws as MeshBasicMaterial; a clearcoat or transmission
 *  as MeshPhysicalMaterial; anything else as MeshStandardMaterial, all on Bevy's StandardMaterial in the engine.
 * @bevy bevy_pbr::pbr_material::StandardMaterial */
export const StandardMaterial = memo(function StandardMaterial<A>(p: StandardMaterialProps & { animate?: Only<A, StandardMaterialAnimate> }) {
  return standard(p, {});
}, sameProps) as unknown as <A>(p: StandardMaterialProps & { animate?: Only<A, StandardMaterialAnimate> }) => ReactElement;

// raea's extensions an ExtendedMaterial puts on its StandardMaterial, as the old material's props
type Extended = { grid?: GridExtensionProps; paint?: PaintExtensionProps; wear?: MeshStandardMaterialProps["wear"]; procedural?: ReturnType<typeof proceduralOf>; shader?: Record<string, unknown> };

// the texture fields as the old material's maps, and each one's sampler by the map's name
const MAPS = { base_color_texture: "map", emissive_texture: "emissiveMap", metallic_roughness_texture: "metallicRoughnessMap", normal_map_texture: "normalMap", occlusion_texture: "aoMap", depth_map: "depthMap", clearcoat_normal_texture: "clearcoatNormalMap" } as const;

function maps(p: StandardMaterialProps): { maps: Record<string, string | null>; samplers?: Record<string, SamplerSpec> } {
  const out: Record<string, string | null> = {};
  const samplers: Record<string, SamplerSpec> = {};
  for (const [field, map] of Object.entries(MAPS)) {
    const t = p[field as keyof typeof MAPS];
    if (t === undefined || (t === null && field !== "base_color_texture")) continue;
    if (t === null || typeof t === "string") {
      if (t === "" && field !== "base_color_texture") continue;
      out[map] = t;
      continue;
    }
    if (field === "metallic_roughness_texture" && typeof t === "object" && !("path" in t)) {
      const g = t as MetallicRoughnessMaps;
      if (g.roughness === undefined && g.metalness === undefined) throw new TypeError("StandardMaterial: metallic_roughness_texture's maps need a roughness or a metalness");
      if (g.roughness !== undefined) out.roughnessMap = g.roughness;
      if (g.metalness !== undefined) out.metalnessMap = g.metalness;
      continue;
    }
    if (typeof t !== "object" || typeof (t as SampledImage).path !== "string") throw new TypeError(`StandardMaterial: ${field} is a path or { path, sampler }`);
    const si = t as SampledImage;
    out[map] = si.path;
    samplers[map] = samplerSpec(`StandardMaterial: ${field}`, si.sampler);
  }
  return Object.keys(samplers).length ? { maps: out, samplers } : { maps: out };
}

function standard(p: StandardMaterialProps, ext: Extended) {
  const color = baseOf(p.base_color).color;
  // (a PaintExtension that only names the mesh's world is fine on an unlit one: the cut is the paint shader's, lit or not)
  const worldOnly = ext.paint !== undefined && Object.keys(ext.paint).every((k) => k === "world" || k === "children");
  if (p.unlit && (ext.grid || (ext.paint && !worldOnly) || ext.wear || ext.procedural || ext.shader)) throw new TypeError("ExtendedMaterial: an unlit StandardMaterial takes no GridExtension, PaintExtension, WearExtension or ShaderExtension");
  if (ext.shader && (ext.grid || ext.paint || ext.wear || ext.procedural)) throw new TypeError("ExtendedMaterial: a ShaderExtension goes alone over its StandardMaterial (no other extension with it)");
  const m = maps(p);
  if (p.unlit && p.animate) throw new TypeError("StandardMaterial: an unlit material takes no animate yet");
  if (p.unlit) return <MeshBasicMaterial color={color} {...(m.maps.map ? { map: m.maps.map } : {})} {...(m.samplers?.map ? { sampler: m.samplers.map } : {})} {...draw(p)} {...(p.fog_enabled !== undefined ? { fog: p.fog_enabled } : {})} {...(ext.paint ? { paint: ext.paint } : {})} />;
  // inside a Model only the fields given re-cover its material; on a Mesh3d the rest are Bevy's defaults
  const own = useContext(InModel);
  const anim = animateOf("StandardMaterial", p.animate, STANDARD_ANIMATE, STANDARD_FIELDS);
  const byField = Object.keys(anim.animate ?? {}).filter((k) => k.startsWith("field:")).map((k) => k.slice(6));
  if (byField.length && (own || ext.grid || ext.paint || ext.wear || ext.shader)) throw new TypeError(`StandardMaterial: animate's ${byField.join(", ")} set a plain StandardMaterial on a Mesh3d; ${own ? "a Model's material" : "an ExtendedMaterial"} takes base_color, emissive, perceptual_roughness and metallic`);
  const common = {
    ...(own && p.base_color === undefined ? {} : { color }),
    ...(own && p.perceptual_roughness === undefined ? {} : { roughness: p.perceptual_roughness ?? 0.5 }),
    ...(own && p.metallic === undefined ? {} : { metalness: p.metallic ?? 0 }),
    ...m.maps,
    ...(m.samplers ? { samplers: m.samplers } : {}),
    ...(p.emissive ? emissiveOf(p.emissive) : {}),
    ...anim.emissive,
    ...(anim.animate ? { animate: anim.animate } : {}),
    ...uvOf(p.uv_transform),
    ...further(p),
    ...(p.material ? { material: p.material } : {}),
    ...ext,
    ...draw(p),
  };
  const physical = p.clearcoat !== undefined || p.specular_transmission !== undefined || p.ior !== undefined || p.thickness !== undefined || p.reflectance !== undefined;
  if (!physical) return <MeshStandardMaterial {...common} />;
  return (
    <MeshPhysicalMaterial
      {...common}
      {...(own && p.reflectance === undefined ? {} : { reflectance: p.reflectance ?? 0.5 })}
      {...(p.clearcoat !== undefined ? { clearcoat: p.clearcoat, clearcoatRoughness: p.clearcoat_perceptual_roughness ?? 0.5 } : {})}
      {...(p.specular_transmission !== undefined ? { transmission: p.specular_transmission } : {})}
      {...(p.ior !== undefined ? { ior: p.ior } : {})}
      {...(p.thickness !== undefined ? { thickness: p.thickness } : {})}
    />
  );
}


// the base's fields a toon surface takes; the rest (roughness, metal, maps) it has no use for
const TOON_BASE = new Set(["base_color", "base_color_texture", "emissive", "alpha_mode", "material", "animate"]);
const TOON_BASE_ANIMATE: Record<string, string> = { base_color: "color", emissive: "emissive" };

/** a StandardMaterial with ToonExtension as the old MeshToonMaterial */
function toon(p: StandardMaterialProps, t: ToonExtensionProps) {
  const extra = Object.keys(p).filter((k) => !TOON_BASE.has(k) && (p as Record<string, unknown>)[k] !== undefined);
  if (extra.length) throw new TypeError(`ExtendedMaterial: a ToonExtension's base takes base_color, base_color_texture, emissive, alpha_mode, material and animate, not ${extra.join(", ")}`);
  const { shadow_color, rim_intensity, highlight_intensity, outline_width, animate, ...rest } = t;
  const base = animateOf("ExtendedMaterial: a ToonExtension's base", p.animate, TOON_BASE_ANIMATE);
  const own = animateOf("ToonExtension", animate, TOON_ANIMATE);
  const anim = base.animate || own.animate ? { ...base.animate, ...own.animate } : undefined;
  const m = maps(p);
  if (m.samplers) throw new TypeError("ToonMaterial: its base_color_texture is a path (a toon's picture takes no sampler yet)");
  const map = m.maps.map;
  return (
    <MeshToonMaterial
      {...(p.base_color !== undefined ? { color: baseOf(p.base_color).color } : {})}
      {...rest}
      {...(shadow_color !== undefined ? { shadowColor: shadow_color } : {})}
      {...(rim_intensity !== undefined ? { rimIntensity: rim_intensity } : {})}
      {...(highlight_intensity !== undefined ? { highlightIntensity: highlight_intensity } : {})}
      {...(outline_width !== undefined ? { outlineWidth: outline_width } : {})}
      {...(map ? { map } : {})}
      {...(p.emissive ? emissiveOf(p.emissive) : {})}
      {...base.emissive}
      {...(anim ? { animate: anim } : {})}
      {...(p.material ? { material: p.material } : {})}
      {...(draw(p) as { transparent?: boolean; opacity?: number })}
    />
  );
}

export type ToonMaterialProps = Pick<StandardMaterialProps, "base_color" | "base_color_texture" | "emissive" | "alpha_mode" | "material"> & ToonExtensionProps;

/** raea's ToonMaterial: a StandardMaterial's colour, texture and emission drawn in bands, as
 *  `<ExtendedMaterial base={<StandardMaterial />} extension={<ToonExtension />} />` is (the old MeshToonMaterial).
 * @bevy bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, raea_render::engine::toon::ToonExt>>
 * @bevy bevy_pbr::mesh_material::MeshMaterial3d<raea_render::engine::toon::OutlineMaterial> */
export function ToonMaterial(p: ToonMaterialProps) {
  // the extension's fields to ToonExtension, the rest to the base (which refuses what a toon surface has no use for)
  const base: Record<string, unknown> = {}, t: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(p)) (TOON_FIELDS.has(k) ? t : base)[k] = v;
  return toon(base as StandardMaterialProps, t as ToonExtensionProps);
}

const TOON_FIELDS = new Set(["steps", "bands", "shadow_color", "rim", "highlight", "outline", "rim_intensity", "highlight_intensity", "outline_width"]);

export interface ExtendedMaterialProps {
  /** a <StandardMaterial>, or another <ExtendedMaterial> to put a second extension on (grid and paint both) */
  base: ReactElement;
  /** a <GridExtension>, <PaintExtension> or <ToonExtension> */
  extension: ReactElement;
}

/** Bevy's ExtendedMaterial: a StandardMaterial with one of raea's extensions (GridExtension, PaintExtension, ToonExtension) drawn on it.
 *
 *    <ExtendedMaterial base={<StandardMaterial base_color="#cfc8bb" />} extension={<GridExtension cell={0.5} major={2} />} />
 */
export function ExtendedMaterial(p: ExtendedMaterialProps) {
  const ext: Extended = {};
  let base: ReactElement = p.base, extension: ReactElement = p.extension;
  if (isValidElement(extension) && extension.type === ToonExtension) {
    if (!(isValidElement(base) && base.type === StandardMaterial)) throw new TypeError("ExtendedMaterial: a ToonExtension's base is a <StandardMaterial> (toon takes no other extension)");
    return toon(base.props as StandardMaterialProps, extension.props as ToonExtensionProps);
  }
  for (;;) {
    if (!isValidElement(extension)) throw new TypeError("ExtendedMaterial: extension is a <GridExtension>, <PaintExtension>, <WearExtension> or <ShaderExtension>");
    if (extension.type === ShaderExtension) {
      if (ext.shader) throw new TypeError("ExtendedMaterial: two ShaderExtensions on one material");
      ext.shader = surfaceShader("ShaderExtension", extension.props as ShaderExtensionProps);
    } else {
      if (extension.type === ProceduralExtension) {
        ext.procedural = proceduralOf(extension.props as ProceduralExtensionProps);
      } else if (extension.type === WearExtension) {
        if (ext.wear) throw new TypeError("ExtendedMaterial: two WearExtensions on one material");
        ext.wear = wearOf(extension.props as WearExtensionProps);
      } else {
        const key = extension.type === GridExtension ? "grid" : extension.type === PaintExtension ? "paint" : undefined;
        if (!key) throw new TypeError("ExtendedMaterial: extension is a <GridExtension>, <PaintExtension>, <WearExtension> or <ShaderExtension>");
        if (ext[key]) throw new TypeError(`ExtendedMaterial: two ${key === "grid" ? "GridExtension" : "PaintExtension"}s on one material`);
        ext[key] = extension.props as GridExtensionProps & PaintExtensionProps;
      }
    }
    if (!isValidElement(base)) throw new TypeError("ExtendedMaterial: base is a <StandardMaterial> or an <ExtendedMaterial>");
    if (base.type === StandardMaterial) return standard(base.props as StandardMaterialProps, ext);
    if (base.type !== ExtendedMaterial) throw new TypeError("ExtendedMaterial: base is a <StandardMaterial> or an <ExtendedMaterial>");
    ({ base, extension } = base.props as ExtendedMaterialProps);
  }
}

/** Bevy's named light values (bevy_light light_consts), for a light's own units: `illuminance={light_consts.lux.DIRECT_SUNLIGHT}`,
 *  `intensity={light_consts.lumens.VERY_LARGE_CINEMA_LIGHT}`. */
export const light_consts = {
  lumens: { LUMENS_PER_LED_WATTS: 90, LUMENS_PER_INCANDESCENT_WATTS: 13.8, LUMENS_PER_HALOGEN_WATTS: 19.8, VERY_LARGE_CINEMA_LIGHT: 1_000_000 },
  lux: {
    MOONLESS_NIGHT: 0.0001, FULL_MOON_NIGHT: 0.05, CIVIL_TWILIGHT: 3.4, LIVING_ROOM: 50, HALLWAY: 80, DARK_OVERCAST_DAY: 100, OFFICE: 320,
    CLEAR_SUNRISE: 400, OVERCAST_DAY: 1000, AMBIENT_DAYLIGHT: 10_000, FULL_DAYLIGHT: 20_000, DIRECT_SUNLIGHT: 100_000, RAW_SUNLIGHT: 130_000,
  },
} as const;

/** What a Bevy light's fields don't name: 3d.tsx's placement and shadow props, refused so a light is written one way. */
export const OLD_LIGHT_PROPS = ["position", "target", "castShadow", "softShadow", "softness", "distance", "angle", "penumbra", "intensity"] as const;
