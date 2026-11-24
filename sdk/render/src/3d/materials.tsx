// Materials: surfaces drawn on, standard and physical materials, toon bands and outlines.
import { toHex } from "../colour";
import { checkOverlays, type GridOverlay, type PaintOverlay, type ProceduralSurface, type WearOverlay } from "../overlays";
import { wornShader } from "../worn";
import { useAnimation3d } from "@raea/math/animate";
import { Animate3d, apply3d, node3d } from "./all";
import { firstKey, useSequenceFrames } from "./clock";

/** How a surface's alpha and faces draw (Bevy's StandardMaterial alpha_mode, alpha cutoff, double_sided and cull_mode),
 * from the first frame. */
export interface SurfaceDrawProps {
  /** "opaque" (the default), "blend" (what `transparent` gives), "mask" (cut out where the alpha is under
   * `alphaCutoff`: leaves, chain-link, a sprite's edge; no sorting), "add" (glows add up: sparks, light beams),
   * "multiply" (darkens what's behind: stained glass, a shadow decal), "premultiplied", "coverage" (soft cut-out edges
   * from the alpha over MSAA's samples: hair cards, foliage; needs <AntiAliasing mode="msaa">, without it draws solid) */
  alphaMode?: "opaque" | "blend" | "mask" | "add" | "multiply" | "premultiplied" | "coverage";
  /** with alphaMode="mask": the alpha under which it's cut away, 0..1 (0.5) */
  alphaCutoff?: number;
  /** which faces draw: "front" (the default), "back" (the inside only: a sky dome, a room seen from outside),
   * "double" (both, lit on both: a leaf, a flag, a sheet of paper) */
  side?: "front" | "back" | "double";
}

export interface MeshStandardMaterialProps extends SurfaceDrawProps {
  /** a picture of where it glows and in what colour, times `emissive` x `emissiveIntensity` (a screen's pixels, a
   * sign's lit letters); sRGB, like `map` */
  emissiveMap?: string;
  /** a hex colour (#rrggbb); animate it freely, each frame's colour is drawn */
  color?: string;
  /** in a <Model>: the glTF material it re-covers (without it, all of them); only the props given change, the rest
   * of the model's material is kept (a cabinet's wood repainted, a bottle's label texture swapped) */
  material?: string;
  /** the colour it glows (hex); with <Bloom> a strong glow blooms */
  emissive?: string;
  /** how strongly it glows (1 by default; 2-10 for a light source with bloom) */
  emissiveIntensity?: number;
  metalness?: number;
  roughness?: number;
  /** 0..1; below 1 (or with `transparent`) the surface is blended */
  opacity?: number;
  transparent?: boolean;
  /** a picture for the base colour: staticFile("label.png"); with a texture set, its diffuse ("_diff") file. In a
   * <Model>, null takes the model's own picture off (a painted cabinet without its wood grain) */
  map?: string | null;
  /** a tangent-space normal map in OpenGL's layout (green up: Poly Haven's "_nor_gl"), so the surface catches light
   * as if it had the bumps it pictures */
  normalMap?: string;
  /** a greyscale roughness map (Poly Haven's "_rough"): white rough, black glossy; `roughness` scales it (1 by
   * default when a roughness map is given) */
  roughnessMap?: string;
  /** a greyscale metalness map: white metal; `metalness` scales it (1 by default when given) */
  metalnessMap?: string;
  /** a packed map in glTF's layout, green roughness and blue metalness (Poly Haven's "_arm", whose red is the
   * ambient occlusion: give the same file as `aoMap`); wins over roughnessMap and metalnessMap */
  metallicRoughnessMap?: string;
  /** an ambient occlusion map, read from its red channel (Poly Haven's "_ao", or its "_arm") */
  aoMap?: string;
  /** how many times every map of this material repeats across each face, [u, v] ([1, 1] by default): a floor of 10 m
   * with repeat={[5, 5]} shows 2 m tiles. Each face of a Box or Plane is mapped over its whole width once */
  repeat?: [number, number];
  /** graph paper over the surface (greybox levels): GridOverlay, overlays.ts */
  grid?: GridOverlay;
  /** under a paint look, the surface broken up by noise: PaintOverlay, overlays.ts */
  paint?: PaintOverlay;
  /** a worn surface, fixed on the part as it moves (worn.ts, a surface shader; not with `shader` or `grid`):
   *  WearOverlay, overlays.ts */
  wear?: WearOverlay;
  /** a pattern made on the GPU with dust, dirt and worn edges (systems/procmat): ProceduralSurface, overlays.ts */
  procedural?: ProceduralSurface;
  /** how its maps are read (docs/shaders.feature SH5): { filter: "nearest" } for hard pixels */
  sampler?: SamplerSpec;
  /** one map's sampler, over `sampler`, by its prop name */
  samplers?: Partial<Record<"map" | "normalMap" | "roughnessMap" | "metalnessMap" | "metallicRoughnessMap" | "aoMap" | "emissiveMap" | "depthMap", SamplerSpec>>;
  /** a shader written in WESL over this lit surface (docs/shaders.feature SH1): on a Mesh, a Model's glTF materials
   * (all, or the one `material` names) and a Character's body */
  shader?: SurfaceShaderSpec;
  /** the highlight's colour (hex; white by default), over `reflectance` (docs/shaders.feature SH7, as the rest below) */
  specularTint?: string;
  /** stretched highlights (brushed metal, hair): a strength 0..1, or { strength, rotation } (radians, along the
   * surface's tangent) */
  anisotropy?: number | { strength?: number; rotation?: number };
  /** light absorbed through a transmissive volume (with transmission and thickness): { distance (metres), color } */
  attenuation?: { distance?: number; color?: string };
  /** a greyscale height map (white deep), read with `parallax` so a flat face looks carved */
  depthMap?: string;
  /** how depthMap carves: { scale (0.1 by default), method "occlusion" | "relief", layers, steps (relief's search) } */
  parallax?: { scale?: number; method?: "occlusion" | "relief"; layers?: number; steps?: number };
  /** drawn this much nearer in depth (a decal-like layer over a coplanar face) */
  depthBias?: number;
  /** false: left out of the scene's <Fog> */
  fog?: boolean;
  /** every map's UVs moved, turned (radians) and scaled; replaces `repeat`'s scale */
  uvTransform?: { offset?: [number, number]; rotation?: number; scale?: [number, number] };
  /** "deferred" draws it in the deferred pass (with a camera that has one); "forward" by default */
  renderMethod?: "forward" | "deferred";
  /** light passed through and scattered (leaves, paper, wax), 0..1 */
  diffuseTransmission?: number;
  /** declared animation of its values (color, emissive, emissiveIntensity, opacity, metalness, roughness) */
  animate?: Animate3d;
}

/** A shader over a lit standard surface (raea-sys-shader). Its source defines either hook, or both,
 * or its own @fragment:
 *   fn surface(in: VertexOutput, pbr: ptr<function, PbrInput>)              before lighting: (*pbr).material.base_color, ...
 *   fn lit(in: VertexOutput, pbr: PbrInput, color: vec4<f32>) -> vec4<f32>  after lighting, before fog and the frame's post
 * Each uniform reads as `u_NAME()`, each texture as `tex_NAME` with `samp_NAME`; `view` and `globals` (globals.time) are
 * imported. Shadows, depth and the prepass stay the standard surface's. */
const IDENT = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** A texture a shader binds: an asset path, "render:NAME", or a picture with its sampler ({ src, sampler }). */
export type ShaderTexture = string | Record<string, unknown>;

export interface SurfaceShaderSpec {
  /** the source in WESL (Bevy 0.20's shader language, a superset of WGSL) */
  wesl?: string;
  /** the same in plain WGSL */
  wgsl?: string;
  /** the source as an asset path (staticFile("shaders/x.wesl")), as Bevy's MaterialExtension::fragment_shader names one:
   *  read as written but for its own bindings' numbers (any, in the order declared: one uniform struct, which gets its
   *  uniforms packed in its layout, up to 4 texture_2d and their one sampler; one under @if(BINDLESS) compiles out),
   *  as Bevy's extended_material and extended_material_bindless declare theirs (docs/control.feature FC2b); in place
   *  of wesl or wgsl */
  fragment?: string;
  /** up to 16: a number (f32), [x, y], [x, y, z], [x, y, z, w] or "#rrggbb" (a linear vec4) */
  uniforms?: Record<string, number | number[] | string>;
  /** up to 4: an asset path, a picture made in code ({ width, height, pixels }) or "render:NAME"; with a sampler,
   * { src, sampler } (or a picture with its `sampler`). One more may be an array, read as texture_2d_array<f32> by
   * layer: { layers: [picture, ...] } (one size and format) or { src, layers: N } (N layers stacked top to bottom). A
   * <Material>'s may also bind one binding_array<texture_2d<f32>>, a list of 1 to 16 pictures (Bevy's
   * texture_binding_array; a GPU without texture binding arrays refuses it) */
  textures?: Record<string, ShaderTexture | ShaderTexture[]>;
  /** shader defs: each NAME a WESL condition for `@if(NAME)` (a number is true), and a whole number also
   * `constants::NAME`; each set of defs compiles its own pipeline */
  defs?: ShaderDefs;
  /** a <Material>'s own (FC2a): what its Material methods give, and its storage buffers; set by <Material> only */
  own?: OwnMaterialFields;
  /** the material's storage buffers by binding name; set by <Material> only */
  buffers?: Record<string, StorageValue>;
}

/** A storage buffer's value in its WGSL type's layout: a number, a list or { field: value }. @bevy bevy_render::storage::ShaderBuffer */
export type StorageValue = number | StorageValue[] | { [field: string]: StorageValue };

/** What a <Material>'s Material methods give: vertex_shader, alpha_mode, depth_bias and the faces its specialize culls. */
export type OwnMaterialFields = { vertex?: string; alpha_mode?: unknown; depth_bias?: number; cull_mode?: "Front" | "Back" | null };
export type ShaderDefs = Record<string, boolean | number>;

/** Shader defs checked (docs/shaders.feature SH8). */
export function shaderDefs(owner: string, defs: ShaderDefs): ShaderDefs {
  if (typeof defs !== "object" || defs === null || Array.isArray(defs)) throw new TypeError(`${owner}: defs are { NAME: true | false | a whole number }`);
  for (const [k, v] of Object.entries(defs)) {
    if (!IDENT.test(k)) throw new TypeError(`${owner}: def ${JSON.stringify(k)} isn't a WGSL name`);
    if (typeof v !== "boolean" && !(Number.isInteger(v) && Math.abs(v) < 2 ** 31)) throw new TypeError(`${owner}: def ${k} is true, false or a whole number, got ${JSON.stringify(v)}`);
  }
  return defs;
}

/** How a texture is read (Bevy's ImageSamplerDescriptor; raea-sys-sampler). Each part is optional, over
 * the texture's own default. A texture file loaded with two samplers is drawn with the last (the load says so). */
export interface SamplerSpec {
  /** "nearest": hard pixels; "linear": smoothed (a material map's default) */
  filter?: "nearest" | "linear";
  /** between mip levels; "none" reads the full-size texture only */
  mipmap?: "nearest" | "linear" | "none";
  /** past the edge: one mode, or [u, v] */
  wrap?: SamplerWrap | [SamplerWrap, SamplerWrap];
  /** 1 to 16 (a map's default is 16); needs every filter linear, so nearest turns it off */
  anisotropy?: number;
}
export type SamplerWrap = "repeat" | "clamp" | "mirror";

/** A sampler checked (a TypeError or RangeError naming the owner and the part). */
export function samplerSpec(owner: string, s: SamplerSpec): SamplerSpec {
  if (typeof s !== "object" || s === null || Array.isArray(s)) throw new TypeError(`${owner}: a sampler is { filter, mipmap, wrap, anisotropy }`);
  for (const k of Object.keys(s)) if (!["filter", "mipmap", "wrap", "anisotropy"].includes(k)) throw new TypeError(`${owner}: a sampler has filter, mipmap, wrap and anisotropy, not ${JSON.stringify(k)}`);
  if (s.filter !== undefined && !["nearest", "linear"].includes(s.filter)) throw new TypeError(`${owner}: sampler.filter is "nearest" or "linear", got ${JSON.stringify(s.filter)}`);
  if (s.mipmap !== undefined && !["nearest", "linear", "none"].includes(s.mipmap)) throw new TypeError(`${owner}: sampler.mipmap is "nearest", "linear" or "none", got ${JSON.stringify(s.mipmap)}`);
  const w = (x: unknown) => ["repeat", "clamp", "mirror"].includes(x as string);
  if (s.wrap !== undefined && !(w(s.wrap) || (Array.isArray(s.wrap) && s.wrap.length === 2 && s.wrap.every(w)))) throw new TypeError(`${owner}: sampler.wrap is "repeat", "clamp" or "mirror", or [u, v] of them, got ${JSON.stringify(s.wrap)}`);
  if (s.anisotropy !== undefined && !(Number.isInteger(s.anisotropy) && s.anisotropy >= 1 && s.anisotropy <= 16)) throw new RangeError(`${owner}: sampler.anisotropy is a whole number from 1 to 16, got ${s.anisotropy}`);
  return s;
}

const HOOK = /(^|[^A-Za-z0-9_])fn\s+(surface|lit)\s*\(|@fragment/;

/** A shader file's path as the engine reads it: found from the edit's folder (staticFile's /media/x as a model's src is)
 *  by the native asset.load where there is one, as written elsewhere (plain bun) */
export function assetPath(p: string): string {
  const call = (globalThis as { __raea_call?: (n: string, a: string) => string | undefined }).__raea_call;
  if (typeof call !== "function") return p;
  const out = call("asset.load", JSON.stringify({ path: p }));
  return out ? (JSON.parse(out) as { path: string }).path : p;
}

/** A surface shader checked and given as the row's spec (its source, uniforms, textures). */
export function surfaceShader(owner: string, sh: SurfaceShaderSpec, hook: RegExp = HOOK, hooks = "fn surface(in: VertexOutput, pbr: ptr<function, PbrInput>), fn lit(in: VertexOutput, pbr: PbrInput, color: vec4<f32>) -> vec4<f32>, or its own @fragment"): Record<string, unknown> {
  if (typeof sh !== "object" || sh === null) throw new TypeError(`${owner}: shader is { wesl | wgsl, uniforms, textures }`);
  const { wesl, wgsl, fragment, uniforms = {}, textures = {}, defs, own, buffers } = sh;
  if ([wesl, wgsl, fragment].filter((s) => s !== undefined).length !== 1) throw new TypeError(`${owner}: shader gives its source as wesl, wgsl or a fragment path, one of them`);
  if (fragment !== undefined && (typeof fragment !== "string" || !fragment)) throw new TypeError(`${owner}: fragment is an asset path, got ${JSON.stringify(fragment)}`);
  const src = wesl ?? wgsl;
  // a file's source is checked where it's read (systems/shader)
  if (fragment === undefined && (typeof src !== "string" || !hook.test(src))) throw new TypeError(`${owner}: shader defines ${hooks}`);
  const us = Object.entries(uniforms);
  if (us.length > 16) throw new RangeError(`${owner}: a shader has at most 16 uniforms, got ${us.length}`);
  for (const [k, v] of us) {
    if (!IDENT.test(k)) throw new TypeError(`${owner}: uniform ${JSON.stringify(k)} isn't a WGSL name`);
    // a Material's uniform of a struct type is { field: value }, each field checked where the struct is read
    if (own !== undefined && typeof v === "object" && v !== null && !Array.isArray(v)) continue;
    const ok = (typeof v === "number" && Number.isFinite(v)) || (Array.isArray(v) && v.length >= 2 && v.length <= 4 && v.every((x) => typeof x === "number" && Number.isFinite(x))) || (typeof v === "string" && /^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(v));
    if (!ok) throw new TypeError(`${owner}: uniform ${k} is a number, [x, y(, z(, w))] or "#rrggbb", got ${JSON.stringify(v)}`);
  }
  if (own !== undefined && fragment === undefined) throw new TypeError(`${owner}: a Material's own fields go with its fragment file`);
  checkTextures(owner, textures);
  // a picture's path found as the fragment's is (staticFile's /media/x); "render:NAME", data: URLs and pictures made in
  // code kept as given
  const file = (s: unknown) => (typeof s === "string" && !s.includes(":") ? assetPath(s) : s);
  const one = (t: ShaderTexture) => (typeof t === "string" ? file(t) : t !== null && typeof t === "object" && "src" in t ? { ...t, src: file(t.src) } : t);
  const found = Object.fromEntries(Object.entries(textures).map(([k, t]) => [k, Array.isArray(t) ? t.map(one) : one(t)]));
  return { ...(fragment !== undefined ? { fragment: assetPath(fragment) } : wesl !== undefined ? { wesl } : { wgsl }), uniforms, textures: found, ...(defs !== undefined ? { defs: shaderDefs(owner, defs) } : {}), ...(own !== undefined ? { own, ...(own.vertex !== undefined ? { vertex: assetPath(own.vertex) } : {}), ...(buffers !== undefined ? { buffers } : {}) } : {}) };
}

/** A shader's textures checked: up to 4 pictures and one array (docs/shaders.feature SH9). */
export function checkTextures(owner: string, textures: Record<string, ShaderTexture | ShaderTexture[]>): void {
  const lists = Object.keys(textures).filter((k) => Array.isArray(textures[k]));
  if (lists.length > 1) throw new RangeError(`${owner}: a shader has one binding_array, got ${lists.join(", ")}`);
  for (const k of lists) {
    const l = textures[k] as ShaderTexture[];
    if (l.length < 1 || l.length > 16) throw new RangeError(`${owner}: binding_array ${k} takes 1 to 16 pictures, got ${l.length}`);
    for (const t of l) if (typeof t === "object" && t !== null && t.sampler !== undefined) samplerSpec(`${owner}: texture ${k}`, t.sampler as SamplerSpec);
  }
  const ts = Object.keys(textures).filter((k) => !lists.includes(k));
  const arrays = ts.filter((k) => typeof textures[k] === "object" && textures[k] !== null && "layers" in (textures[k] as object));
  if (arrays.length > 1) throw new RangeError(`${owner}: a shader has one texture array, got ${arrays.join(", ")}`);
  if (ts.length - arrays.length > 4) throw new RangeError(`${owner}: a shader has at most 4 textures (and one array), got ${ts.length - arrays.length}`);
  for (const k of ts) {
    if (!IDENT.test(k)) throw new TypeError(`${owner}: texture ${JSON.stringify(k)} isn't a WGSL name`);
    const t = textures[k] as Record<string, unknown>;
    if (typeof t === "object" && t !== null && t.sampler !== undefined) samplerSpec(`${owner}: texture ${k}`, t.sampler as SamplerSpec);
    if (!arrays.includes(k)) continue;
    const l = t.layers;
    const list = Array.isArray(l) && l.length >= 2 && t.src === undefined;
    const stack = Number.isInteger(l) && (l as number) >= 2 && (l as number) <= 2048 && t.src !== undefined;
    if (!list && !stack) throw new TypeError(`${owner}: texture ${k} is an array as { layers: [picture, ...] } (2 or more) or { src, layers: N } (a picture of N layers stacked top to bottom), got ${JSON.stringify(t)}`);
  }
}

/** One shader over every lit standard surface in the scene that has none of its own (meshes, Models' materials,
 * Characters), so the whole scene shares one look (docs/shaders.feature SH4). Unlit, toon, paint, flat and looked
 * surfaces and ShaderMesh keep theirs. In the 3D beside the camera; one per scene.
 * @bevy bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, raea_sys_shader::ShaderExt>> */
export function SurfaceShader(props: SurfaceShaderSpec) {
  return node3d("SurfaceShader", { shader: surfaceShader("SurfaceShader", props) });
}

export function colours<T extends Record<string, unknown>>(owner: string, props: T, keys: string[]): T {
  const out: Record<string, unknown> = { ...props };
  for (const k of keys) if (out[k] !== undefined) out[k] = toHex(owner, k, out[k]);
  return out as T;
}

const isNum = (x: unknown) => typeof x === "number" && Number.isFinite(x);
const isPair = (x: unknown) => Array.isArray(x) && x.length === 2 && x.every(isNum);

/** StandardMaterial's other fields (docs/shaders.feature SH7) checked, their colours made hex. */
function checkFields(owner: string, props: Record<string, unknown>): void {
  if (props.specularTint !== undefined) props.specularTint = toHex(owner, "specularTint", props.specularTint);
  const an = props.anisotropy as number | { strength?: number; rotation?: number } | undefined;
  if (an !== undefined && !(isNum(an) || (typeof an === "object" && an !== null && Object.keys(an).every((k) => k === "strength" || k === "rotation") && Object.values(an).every(isNum)))) throw new TypeError(`${owner}: anisotropy is a strength, or { strength, rotation }, got ${JSON.stringify(an)}`);
  const at = props.attenuation as { distance?: number; color?: string } | undefined;
  if (at !== undefined) {
    if (typeof at !== "object" || at === null || Object.keys(at).some((k) => k !== "distance" && k !== "color")) throw new TypeError(`${owner}: attenuation is { distance, color }`);
    if (at.distance !== undefined && !(isNum(at.distance) && at.distance > 0)) throw new RangeError(`${owner}: attenuation.distance must be above 0 metres, got ${at.distance}`);
    if (at.color !== undefined) props.attenuation = { ...at, color: toHex(owner, "attenuation.color", at.color) };
  }
  const px = props.parallax as Record<string, unknown> | undefined;
  if (px !== undefined) {
    if (typeof px !== "object" || px === null || Object.keys(px).some((k) => !["scale", "method", "layers", "steps"].includes(k))) throw new TypeError(`${owner}: parallax is { scale, method, layers, steps }`);
    if (px.method !== undefined && px.method !== "occlusion" && px.method !== "relief") throw new TypeError(`${owner}: parallax.method is "occlusion" or "relief", got ${JSON.stringify(px.method)}`);
    for (const k of ["scale", "layers", "steps"]) if (px[k] !== undefined && !(isNum(px[k]) && (px[k] as number) >= 0)) throw new RangeError(`${owner}: parallax.${k} must be 0 or more, got ${px[k]}`);
  }
  if (props.depthBias !== undefined && !isNum(props.depthBias)) throw new TypeError(`${owner}: depthBias is a number, got ${JSON.stringify(props.depthBias)}`);
  if (props.fog !== undefined && typeof props.fog !== "boolean") throw new TypeError(`${owner}: fog is true or false, got ${JSON.stringify(props.fog)}`);
  const uv = props.uvTransform as Record<string, unknown> | undefined;
  if (uv !== undefined) {
    if (typeof uv !== "object" || uv === null || Object.keys(uv).some((k) => !["offset", "rotation", "scale"].includes(k))) throw new TypeError(`${owner}: uvTransform is { offset, rotation, scale }`);
    if ((uv.offset !== undefined && !isPair(uv.offset)) || (uv.scale !== undefined && !isPair(uv.scale)) || (uv.rotation !== undefined && !isNum(uv.rotation))) throw new TypeError(`${owner}: uvTransform's offset and scale are [u, v] and its rotation a number, got ${JSON.stringify(uv)}`);
  }
  if (props.renderMethod !== undefined && props.renderMethod !== "forward" && props.renderMethod !== "deferred") throw new TypeError(`${owner}: renderMethod is "forward" or "deferred", got ${JSON.stringify(props.renderMethod)}`);
}

export function checkMaterial(owner: string, props: Record<string, unknown>): void {
  checkOverlays(owner, props);
  for (const k of ["metalness", "roughness", "opacity", "clearcoat", "clearcoatRoughness", "reflectance", "transmission", "alphaCutoff", "diffuseTransmission"]) {
    const v = props[k];
    if (v !== undefined && !(typeof v === "number" && v >= 0 && v <= 1)) throw new RangeError(`${owner}: ${k} must be in [0, 1], got ${v}`);
  }
  const am = props.alphaMode;
  if (am !== undefined && !["opaque", "blend", "mask", "add", "multiply", "premultiplied", "coverage"].includes(am as string)) throw new TypeError(`${owner}: alphaMode is opaque, blend, mask, add, multiply, premultiplied or coverage, got ${JSON.stringify(am)}`);
  if (props.side !== undefined && !["front", "back", "double"].includes(props.side as string)) throw new TypeError(`${owner}: side is front, back or double, got ${JSON.stringify(props.side)}`);
  if (props.ior !== undefined && !(typeof props.ior === "number" && props.ior >= 1)) throw new RangeError(`${owner}: ior must be 1 or more, got ${props.ior}`);
  if (props.thickness !== undefined && !(typeof props.thickness === "number" && props.thickness >= 0)) throw new RangeError(`${owner}: thickness must be 0 or more metres, got ${props.thickness}`);
  for (const k of ["map", "normalMap", "roughnessMap", "metalnessMap", "metallicRoughnessMap", "aoMap", "emissiveMap", "depthMap"]) {
    const v = props[k];
    if (v !== undefined && !(k === "map" && v === null) && !(typeof v === "string" && v !== "")) throw new TypeError(`${owner}: ${k} must be a picture's path (staticFile("floor_diff.jpg")) or a data: URL, got ${JSON.stringify(v)}`);
  }
  // `wear` is a surface shader of raea's own (worn.ts, docs/examples.feature EX9); none at amount 0
  if (props.wear !== undefined) {
    if (props.shader !== undefined) throw new TypeError(`${owner}: wear is a surface shader of its own; give wear or shader, not both`);
    if (props.grid !== undefined) throw new TypeError(`${owner}: grid draws alone; give grid or wear, not both`);
    const w = props.wear as WearOverlay;
    delete props.wear;
    if (w.amount > 0) props.shader = wornShader(w);
  }
  if (props.shader !== undefined) props.shader = surfaceShader(owner, props.shader as SurfaceShaderSpec);
  if (props.sampler !== undefined) samplerSpec(owner, props.sampler as SamplerSpec);
  if (props.samplers !== undefined) {
    const own = props.samplers as Record<string, SamplerSpec>;
    if (typeof own !== "object" || own === null) throw new TypeError(`${owner}: samplers is { map, normalMap, ...: a sampler }`);
    for (const [k, v] of Object.entries(own)) {
      if (!["map", "normalMap", "roughnessMap", "metalnessMap", "metallicRoughnessMap", "aoMap", "emissiveMap", "depthMap"].includes(k)) throw new TypeError(`${owner}: samplers.${k} isn't one of its maps (map, normalMap, roughnessMap, metalnessMap, metallicRoughnessMap, aoMap, emissiveMap, depthMap)`);
      samplerSpec(`${owner}: samplers.${k}`, v);
    }
  }
  checkFields(owner, props);
  const r = props.repeat;
  if (r !== undefined && !(Array.isArray(r) && r.length === 2 && r.every((x) => typeof x === "number" && x > 0 && Number.isFinite(x)))) throw new RangeError(`${owner}: repeat must be [u, v], two positive numbers, got ${JSON.stringify(r)}`);
}

/** A physically based surface for the Mesh it's inside.
 * `grid` draws graph paper over it (engine/render grid.rs):
 * `wear` chips, grimes and rusts it in the part's own space (worn.ts):
 * @deprecated <StandardMaterial>, raea/pbr (Bevy's fields: base_color, perceptual_roughness, metallic)
 */
export function MeshStandardMaterial(props: MeshStandardMaterialProps) {
  useSequenceFrames("MeshStandardMaterial", undefined, [["animate's first key", firstKey(props.animate)]]);
  const anim = useAnimation3d(props.animate);
  const p = apply3d({ ...props, animate: undefined } as Record<string, unknown>, anim.values);
  checkMaterial("MeshStandardMaterial", p);
  return node3d("MeshStandardMaterial", colours("MeshStandardMaterial", p, ["color", "emissive"]), undefined, anim.attr ? { "data-raea-anim": anim.attr } : undefined);
}

export interface MeshPhysicalMaterialProps extends MeshStandardMaterialProps {
  /** a clear lacquer layer over the surface, 0..1 (car paint, a phone's glass back) */
  clearcoat?: number;
  clearcoatRoughness?: number;
  /** specular strength of non-metals, 0..1 (0.5 by default) */
  reflectance?: number;
  /** how much light passes through it, 0..1: real glass, water, a gem (Bevy's specular_transmission; what's behind
   * shows through, bent by `ior` over `thickness`); unlike `opacity`, it stays as bright as clear glass is */
  transmission?: number;
  /** index of refraction (1.5, glass; water 1.33, diamond 2.42) */
  ior?: number;
  /** how thick it is, in metres, for the bending and tint of what shows through (0: a thin sheet) */
  thickness?: number;
}

/** The standard surface plus a clearcoat layer and reflectance.
 * @deprecated <StandardMaterial>, raea/pbr (clearcoat and specular_transmission are its fields) */
export function MeshPhysicalMaterial(props: MeshPhysicalMaterialProps) {
  useSequenceFrames("MeshPhysicalMaterial", undefined, [["animate's first key", firstKey(props.animate)]]);
  const anim = useAnimation3d(props.animate);
  const p = apply3d({ ...props, animate: undefined } as Record<string, unknown>, anim.values);
  checkMaterial("MeshPhysicalMaterial", p);
  return node3d("MeshPhysicalMaterial", colours("MeshPhysicalMaterial", p, ["color", "emissive"]), undefined, anim.attr ? { "data-raea-anim": anim.attr } : undefined);
}

export interface MeshBasicMaterialProps extends SurfaceDrawProps {
  color?: string;
  opacity?: number;
  transparent?: boolean;
  map?: string;
  /** how many times the map repeats across each face, [u, v] */
  repeat?: [number, number];
  /** how its map is read (docs/shaders.feature SH5): { filter: "nearest" } for hard pixels */
  sampler?: SamplerSpec;
}

/** A flat colour no light changes (unlit): signs, screens, backdrops.
 * @deprecated <StandardMaterial unlit>, raea/pbr */
export function MeshBasicMaterial(props: MeshBasicMaterialProps) {
  checkMaterial("MeshBasicMaterial", props as Record<string, unknown>);
  return node3d("MeshBasicMaterial", colours("MeshBasicMaterial", props as Record<string, unknown>, ["color", "emissive"]));
}

export interface ToonBand {
  /** where the band starts, as r: the share of the light reaching a point that it faces (0 faces none of it, 1 faces
   * every light); the first band starts at 0 whatever this says */
  at: number;
  /** the band's colour; left out, it's mixed (in Oklab) between the shadow colour and `color` by its place */
  color?: string;
}

export interface ToonRim {
  /** 0..1, how much the rim colour covers the band (1 by default) */
  intensity?: number;
  /** "#ffffff" by default */
  color?: string;
  /** 0..1 of the Fresnel range from the silhouette inwards (0.3 by default) */
  width?: number;
}

export interface ToonHighlight {
  /** 0..1 (1 by default) */
  intensity?: number;
  /** "#ffffff" by default */
  color?: string;
  /** 0..1: the highlight's angular radius, up to half a radian (0.2 by default) */
  size?: number;
}

export interface ToonOutline {
  /** pixels at the composition's size, the same near and far (2 by default) */
  width?: number;
  /** ink colour; by default the surface colour's hue, very dark */
  color?: string;
}

export interface MeshToonMaterialProps {
  /** the lit (top) band's colour, hex. On a Model, left out: each glTF material keeps its own colour and texture */
  color?: string;
  /** hard light bands, 1-8 (2 by default), evenly spaced in r; `bands` instead for an explicit ramp */
  steps?: number;
  /** an explicit ramp, darkest first: [{ at: 0, color: "#3a2a5a" }, { at: 0.45 }, { at: 0.85, color: "#fff7d6" }] */
  bands?: ToonBand[];
  /** the bottom band's colour; by default `color` darker with its hue nudged towards violet (never grey) */
  shadowColor?: string;
  /** a crisp Fresnel band at the silhouette (true for the defaults) */
  rim?: boolean | ToonRim;
  /** a crisp specular spot where the light glints (true for the defaults) */
  highlight?: boolean | ToonHighlight;
  /** an ink line around the silhouette and along creases (true for the defaults) */
  outline?: boolean | ToonOutline;
  emissive?: string;
  emissiveIntensity?: number;
  opacity?: number;
  transparent?: boolean;
  /** a picture for the base colour (a Mesh's); its texels are the lit colour */
  map?: string;
  /** on a Model: only the glTF material with this name (one MeshToonMaterial without it covers the rest) */
  material?: string;
  /** animate names: color, shadowColor, emissive, emissiveIntensity, opacity, rimIntensity, highlightIntensity,
   * outlineWidth */
  animate?: Animate3d;
  rimIntensity?: number;
  highlightIntensity?: number;
  outlineWidth?: number;
}

/** A cel-shaded surface (three.js's MeshToonMaterial): lit by the scene's lights in hard bands, with an optional rim,
 * highlight and ink outline. Inside a Mesh, or inside a Model to toon its glTF materials.
 * @deprecated <ToonMaterial>, raea/pbr */
export function MeshToonMaterial(props: MeshToonMaterialProps) {
  const owner = "MeshToonMaterial";
  useSequenceFrames("MeshToonMaterial", undefined, [["animate's first key", firstKey(props.animate)]]);
  const anim = useAnimation3d(props.animate);
  const p = apply3d({ ...props, animate: undefined } as Record<string, unknown>, anim.values);
  checkMaterial(owner, p);
  const unit = (k: string, v: unknown) => {
    if (v !== undefined && !(typeof v === "number" && v >= 0 && v <= 1)) throw new RangeError(`${owner}: ${k} must be in [0, 1], got ${v}`);
  };
  if (p.steps !== undefined && !(Number.isInteger(p.steps) && (p.steps as number) >= 1 && (p.steps as number) <= 8)) throw new RangeError(`${owner}: steps must be a whole number from 1 to 8, got ${p.steps}`);
  const out: Record<string, unknown> = colours(owner, p, ["color", "emissive", "shadowColor"]);
  if (p.bands !== undefined) {
    const bands = p.bands as ToonBand[];
    if (!Array.isArray(bands) || bands.length < 1 || bands.length > 8) throw new RangeError(`${owner}: bands must be 1 to 8 bands, darkest first`);
    let last = -Infinity;
    out.bands = bands.map((b, i) => {
      if (typeof b?.at !== "number" || !Number.isFinite(b.at)) throw new TypeError(`${owner}: bands[${i}].at must be a number`);
      if (b.at < last) throw new RangeError(`${owner}: bands must be in order, darkest first (bands[${i}].at ${b.at} < ${last})`);
      last = b.at;
      return b.color === undefined ? { at: b.at } : { at: b.at, color: toHex(owner, `bands[${i}].color`, b.color) };
    });
  }
  const part = (k: "rim" | "highlight" | "outline") => {
    const v = p[k];
    if (v === undefined || v === false) return undefined;
    const o = v === true ? {} : { ...(v as Record<string, unknown>) };
    if (o.color !== undefined) o.color = toHex(owner, `${k}.color`, o.color);
    return o;
  };
  for (const k of ["rim", "highlight", "outline"] as const) {
    const o = part(k);
    if (o === undefined) delete out[k];
    else out[k] = o;
  }
  const rim = out.rim as ToonRim | undefined, hl = out.highlight as ToonHighlight | undefined, ol = out.outline as ToonOutline | undefined;
  unit("rim.intensity", rim?.intensity); unit("rim.width", rim?.width); unit("rimIntensity", p.rimIntensity);
  unit("highlight.intensity", hl?.intensity); unit("highlight.size", hl?.size); unit("highlightIntensity", p.highlightIntensity);
  for (const [k, v] of [["outline.width", ol?.width], ["outlineWidth", p.outlineWidth]] as const) {
    if (v !== undefined && !(typeof v === "number" && v >= 0 && v <= 64)) throw new RangeError(`${owner}: ${k} is pixels, 0 to 64, got ${v}`);
  }
  return node3d(owner, out, undefined, anim.attr ? { "data-raea-anim": anim.attr } : undefined);
}
