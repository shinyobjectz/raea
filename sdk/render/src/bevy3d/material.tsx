// Your own Material (docs/control.feature FC2a): Bevy's Material, its shaders the author's files compiled as written.
import { ExtendedMaterial, ShaderExtension, StandardMaterial, type AlphaMode } from "./materials";
import { surfaceShader, type SurfaceShaderSpec } from "../3d";
import type { StorageValue } from "../3d/materials";
import type { MeshVertexAttribute } from "../3d/attribute";

/** Your own Material (docs/control.feature FC2a): Bevy's `impl Material`, not an extension of StandardMaterial. Its
 *  fragment_shader() (and vertex_shader(), when `vertex` names one) is the file compiled as written: its imports (a
 *  `super::NAME` one reads NAME.wesl beside it), its own @fragment and its own `@group(MATERIAL_BIND_GROUP)` bindings,
 *  up to 8 uniforms of any type (a struct's from { field: value }), 2 read-only storage buffers, a texture_2d and
 *  sampler 4 times and one texture of each other dimension, in its layout; one it declares and isn't given binds
 *  Bevy's fallback image. A uniform, buffer or texture it doesn't declare is refused. Its uniforms and buffers may
 *  change from frame to frame, as an example's update system changes them. `alpha_mode` and `depth_bias` are its
 *  Material methods', and `cull_mode` the faces its specialize culls (the pipeline's by default: Back in 3D, none in
 *  2D; null culls none). In a Mesh2d it is Bevy's Material2d (its group 2), in place of the ColorMaterial.
 *  @bevy bevy_pbr::material::Material */
export interface MaterialProps {
  /** the shader file, staticFile("shaders/x.wesl"), as Bevy's Material::fragment_shader names one */
  fragment: string;
  /** a vertex shader file, as Material::vertex_shader names one (Bevy's mesh vertex shader when not given) */
  vertex?: string;
  /** the mesh attributes its vertex shader reads, each at its @location: [[ATTRIBUTE_POSITION, 0], [BLEND_COLOR, 1]], the
   *  vertex buffer its specialize lays out (layout.0.get_layout); the mesh's whole layout when not given */
  vertex_attributes?: [MeshVertexAttribute, number][];
  uniforms?: Record<string, number | number[] | string | Record<string, number | number[] | string>>;
  /** its read-only storage buffers (`var<storage, read> NAME: TYPE`, up to 2), each in TYPE's storage layout: an
   *  array<T(, N)> a list of its elements, a number or [x, y(, z(, w))] each, as Bevy's ShaderBuffer::from(Vec<[f32; 4]>)
   *  holds them; a struct of the source's { field: value } (gpu_component_array_buffer's { data_array: [...] }); a
   *  matrix its columns */
  buffers?: Record<string, StorageValue>;
  textures?: SurfaceShaderSpec["textures"];
  defs?: SurfaceShaderSpec["defs"];
  alpha_mode?: AlphaMode;
  depth_bias?: number;
  cull_mode?: "Front" | "Back" | null;
  /** its fragment stage's pipeline-overridable constants (WGSL `override NAME: f32 = ...;`), each a number: compiled
   *  into its pipeline, as a Material's specialize pushes them into descriptor.fragment.constants */
  constants?: Record<string, number>;
}

const ALPHA = ["Opaque", "Blend", "Premultiplied", "Add", "Multiply", "AlphaToCoverage"];

/** A storage buffer's value: numbers, lists of them and { field: value } structs, as StorageValue. */
const storageValue = (x: unknown): boolean => Number.isFinite(x) || (Array.isArray(x) && x.every(storageValue)) || (typeof x === "object" && x !== null && !Array.isArray(x) && Object.values(x).every(storageValue));

/** A Material's props checked, as the shader spec it is drawn by (in a Mesh3d an OwnMaterial, in a Mesh2d its
 *  Material2d). */
function ownSpec({ fragment, vertex, vertex_attributes, uniforms, buffers, textures, defs, alpha_mode, depth_bias, cull_mode, constants }: MaterialProps): SurfaceShaderSpec {
  for (const [k, b] of Object.entries(buffers ?? {})) if (typeof b !== "object" || b === null || !storageValue(b)) throw new TypeError(`Material: buffer ${k} is its elements' list or a struct's { field: value }, of numbers and lists, got ${JSON.stringify(b)}`);
  if (typeof fragment !== "string" || !fragment) throw new TypeError(`Material: fragment is the shader file, staticFile("shaders/x.wesl"), got ${JSON.stringify(fragment)}`);
  if (vertex !== undefined && (typeof vertex !== "string" || !vertex)) throw new TypeError(`Material: vertex is a shader file, got ${JSON.stringify(vertex)}`);
  const mask = typeof alpha_mode === "object" && alpha_mode !== null && Number.isFinite((alpha_mode as { Mask?: number }).Mask);
  if (alpha_mode !== undefined && !mask && !ALPHA.includes(alpha_mode as string)) throw new TypeError(`Material: alpha_mode is ${ALPHA.join(", ")} or { Mask: cutoff }, got ${JSON.stringify(alpha_mode)}`);
  if (depth_bias !== undefined && !Number.isFinite(depth_bias)) throw new TypeError(`Material: depth_bias is a number, got ${depth_bias}`);
  if (cull_mode !== undefined && cull_mode !== null && cull_mode !== "Front" && cull_mode !== "Back") throw new TypeError(`Material: cull_mode is "Front", "Back" or null, got ${JSON.stringify(cull_mode)}`);
  for (const [k, n] of Object.entries(constants ?? {})) if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(k) || !Number.isFinite(n)) throw new TypeError(`Material: constant ${k} is a WGSL name with a number, got ${JSON.stringify(n)}`);
  for (const e of vertex_attributes ?? []) if (!Array.isArray(e) || typeof e[0] !== "object" || !Number.isInteger(e[1]) || e[1] < 0) throw new TypeError(`Material: vertex_attributes are [attribute, location] pairs, got ${JSON.stringify(e)}`);
  const own = { ...(vertex !== undefined ? { vertex } : {}), ...(vertex_attributes !== undefined ? { vertex_attributes } : {}), ...(alpha_mode !== undefined ? { alpha_mode } : {}), ...(depth_bias !== undefined ? { depth_bias } : {}), ...(cull_mode !== undefined ? { cull_mode } : {}), ...(constants !== undefined ? { constants } : {}) };
  return { fragment, uniforms: uniforms as SurfaceShaderSpec["uniforms"], textures, defs, own, ...(buffers ? { buffers } : {}) };
}

export function Material(p: MaterialProps) {
  // drawn by the engine as raea-sys-shader's OwnMaterial; the StandardMaterial is only what the rows carry it on
  return <ExtendedMaterial base={<StandardMaterial />} extension={<ShaderExtension {...ownSpec(p)} />} />;
}

/** A Material in a Mesh2d: the spec its Shape2D carries as `shader` (drawn as its own Material2d). */
export function materialShader(p: MaterialProps): Record<string, unknown> {
  return surfaceShader("Material", ownSpec(p));
}
// what a 2D part reads a <Material> child by (3d/sprite.tsx: a Sprite's SpriteMaterial), without importing this file
(Material as unknown as { shaderOf: typeof materialShader }).shaderOf = materialShader;
