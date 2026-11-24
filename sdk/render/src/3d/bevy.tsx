// Bevy's own components and resources by type path, and gizmos.
import { Entity, Vec3, checkTextures, checkVec, colours, node3d, shaderDefs, type ShaderDefs } from "./all";
import { useSequenceFrames } from "./clock";

export interface BevyComponentProps {
  /** the component's reflected type path, as Bevy's TypeRegistry names it ("avian3d::dynamics::rigid_body::RigidBody") */
  type: string;
  /** its value as Bevy's reflect serde reads it: a struct an object (fields left out take the type's default, when it
   * has one), a newtype its inner value, an enum "Unit" or { Variant: ... }, a Vec3 [x, y, z] */
  value?: unknown;
  /** the frame it's added on (0, the start, by default); a plugin steps after it from that frame */
  from?: number;
}

/** A Bevy component on the part it's inside (a Mesh, a Model, an ImagePlane, a light or the camera), applied through the
 * TypeRegistry: how a compiled-in Bevy plugin is driven (sdk/raea/docs/bevy-plugins.md), and how any of Bevy's own
 * reflected components reaches a part in every build (docs/bevy-ts.feature G1; a `from` frame needs the plugin
 * build's frame clock). Agents use the generated, typed wrappers (one per component), not this directly. */
export function BevyComponent(props: BevyComponentProps) {
  if (typeof props.type !== "string" || !props.type.includes("::")) throw new TypeError(`BevyComponent: type is a full type path (crate::module::Type), got ${JSON.stringify(props.type)}`);
  if (props.from !== undefined && !(Number.isInteger(props.from) && props.from >= 0)) throw new RangeError(`BevyComponent: from is a frame (0 or more), got ${props.from}`);
  useSequenceFrames("BevyComponent", undefined, [["from", props.from]]);
  return node3d("BevyComponent", { type: props.type, value: props.value ?? null, ...(props.from ? { from: props.from } : {}) });
}

/** a gizmo point's colour: "#rrggbb", linear [r, g, b, a], or Bevy's LinearRgba / Srgba (bevy3d's colours) */
export type GizmoColor = string | [number, number, number, number] | { red: number; green: number; blue: number; alpha?: number };

/** bevy_color's sRGB to linear for one channel (at or below 0 kept, as Bevy's gamma function does) */
const toLinear = (c: number) => (c <= 0 ? c : c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);

/** what fields.rs's gizmo_asset reads: the hex as it is, anything else linear [r, g, b, a] */
function gizmoColour(c: GizmoColor): string | number[] {
  if (typeof c === "string") {
    if (!/^#?[0-9a-f]{6}([0-9a-f]{2})?$/i.test(c)) throw new TypeError(`Gizmo: a colour string is "#rrggbb", got ${JSON.stringify(c)}`);
    return c;
  }
  if (Array.isArray(c)) {
    if (c.length !== 4 || !c.every(Number.isFinite)) throw new TypeError(`Gizmo: a colour array is linear [r, g, b, a], got ${JSON.stringify(c)}`);
    return c;
  }
  if (!c || ![c.red, c.green, c.blue].every(Number.isFinite)) throw new TypeError(`Gizmo: a colour is "#rrggbb", [r, g, b, a] or a LinearRgba / Srgba, got ${JSON.stringify(c)}`);
  // Srgba (it has to_hex) is gamma-encoded; LinearRgba is as the gizmo takes it
  const srgb = typeof (c as { to_hex?: unknown }).to_hex === "function";
  const ch = srgb ? [toLinear(c.red), toLinear(c.green), toLinear(c.blue)] : [c.red, c.green, c.blue];
  return [...ch, c.alpha ?? 1];
}

export interface GizmoProps {
  /** segments, each [from, to] */
  lines?: [Vec3, Vec3][];
  /** one line through these points (null breaks it) */
  strip?: (Vec3 | null)[];
  /** one colour for every point, or one per point of `lines` (two per segment) then `strip`: "#rrggbb", Bevy's
   * LinearRgba (above 1 for HDR, as bevy_gizmos takes it) or Srgba, or linear [r, g, b, a] */
  color?: GizmoColor | GizmoColor[];
  /** line width in pixels (Bevy's default 2) */
  width?: number;
  /** -1 always in front of geometry, 0 depth-tested as it is (Bevy's GizmoLineConfig and depth_bias) */
  depthBias?: number;
  /** where it stands, metres (x, y, z) */
  position?: Vec3;
  /** its rotation, Euler angles in radians (x, y, z) */
  rotation?: Vec3;
  /** its scale: one number, or x, y and z */
  scale?: Vec3 | number;
  /** the entity's name, for picking, the inspector and world.* */
  name?: string;
}

/** Debug lines drawn by Bevy's gizmo renderer (a retained bevy_gizmos Gizmo and its GizmoAsset), placed like a part:
 * a path, a bounding box, an axis, a ray. They cast and take no light, and draw over nothing unless depthBias says.
 * @bevy bevy_gizmos::retained::Gizmo */
export function Gizmo({ lines = [], strip = [], color = "#ffffff", width, depthBias, ...place }: GizmoProps) {
  if (!Array.isArray(lines) || !lines.every((l) => Array.isArray(l) && l.length === 2)) throw new TypeError("Gizmo: lines is a list of [from, to] segments");
  lines.forEach(([a, b]) => { checkVec("Gizmo", "lines", a); checkVec("Gizmo", "lines", b); });
  if (!Array.isArray(strip)) throw new TypeError("Gizmo: strip is a list of points");
  strip.forEach((p) => p === null || checkVec("Gizmo", "strip", p));
  if (lines.length === 0 && strip.length === 0) throw new TypeError("Gizmo: give lines or a strip");
  if (width !== undefined && !(width > 0 && Number.isFinite(width))) throw new RangeError(`Gizmo: width is in pixels, above 0, got ${width}`);
  if (depthBias !== undefined && !(depthBias >= -1 && depthBias <= 1)) throw new RangeError(`Gizmo: depthBias is -1 to 1, got ${depthBias}`);
  const n = lines.length * 2;
  // one colour, or a list of them (an array of numbers is one linear colour)
  const list = Array.isArray(color) && !color.every((x) => typeof x === "number");
  const colours = list ? (color as GizmoColor[]).map(gizmoColour) : null;
  if (colours && colours.length !== n + strip.length) throw new RangeError(`Gizmo: color lists one colour per point (${n + strip.length}), got ${colours.length}`);
  const handle: Record<string, unknown> = {
    list_positions: lines.flat(),
    strip_positions: strip,
    ...(colours ? { list_colors: colours.slice(0, n), strip_colors: colours.slice(n) } : { color: gizmoColour(color as GizmoColor) }),
  };
  const value: Record<string, unknown> = { handle };
  if (width !== undefined) value.line_config = { width };
  if (depthBias !== undefined) value.depth_bias = depthBias;
  return (
    <Entity {...place}>
      <BevyComponent type="bevy_gizmos::retained::Gizmo" value={value} />
    </Entity>
  );
}

export interface ShaderMeshProps {
  /** its geometry: a Manifold shape ("box(1, 1, 1)") or one of Bevy's primitives ({ primitive: "Sphere", radius: 1 }) */
  shape: string | ({ primitive: string } & Record<string, unknown>);
  /** WESL (Bevy 0.20's standard; give this or `wgsl`): a `fn shade(in: VertexOutput) -> vec4<f32>` (in.uv,
   * in.world_position, in.world_normal), or a module with its own @fragment, which may `import` Bevy's WESL modules
   * (bevy_pbr::render::...). Each uniform is read as `u_NAME()`, each texture as `tex_NAME` with `samp_NAME`;
   * `globals.time` and `view` are imported */
  wesl?: string;
  /** the same in plain WGSL (compiled as WESL, its superset: Bevy 0.20 has no naga_oil, so no `#import`) */
  wgsl?: string;
  /** up to 16: a number (f32), [x, y], [x, y, z], [x, y, z, w] or "#rrggbb" (a linear vec4) */
  uniforms?: Record<string, number | number[] | string>;
  /** up to 4: an asset path, a picture made in code ({ width, height, pixels }) or "render:NAME"; and one array,
   * { layers: [picture, ...] } or { src, layers: N }, read as texture_2d_array<f32> tex_NAME by layer */
  textures?: Record<string, string | Record<string, unknown>>;
  /** shader defs: each NAME a WESL condition for `@if(NAME)`, and a whole number also `constants::NAME` */
  defs?: ShaderDefs;
  /** how it blends: opaque, blend, add or premultiplied */
  alpha?: "opaque" | "blend" | "add" | "premultiplied";
  /** draw the back faces too */
  doubleSided?: boolean;
  /** the entity's name, for picking, the inspector and world.* */
  name?: string;
  /** where it stands, metres (x, y, z) */
  position?: Vec3;
  /** its rotation, Euler angles in radians (x, y, z) */
  rotation?: Vec3;
  /** its scale: one number, or x, y and z */
  scale?: Vec3 | number;
}

/** A mesh drawn by a material written here in WESL or WGSL (docs/bevy-ts.feature G20): one generic Bevy material
 * (rdb's ShaderMaterial, an AsBindGroup of 16 uniform slots and 4 textures) whose fragment shader is this source, so
 * a dissolve, a toon ramp, a hologram or a procedural pattern needs no Rust. Uniforms may change from frame to frame
 * (they're set again on any frame where they change); the shader isn't compiled again (#28).
 * @bevy bevy_pbr::mesh_material::MeshMaterial3d<raea_sys_material::shader_material::ShaderMaterial> */
export function ShaderMesh({ shape, wesl, wgsl, uniforms = {}, textures = {}, defs, alpha, doubleSided, ...place }: ShaderMeshProps) {
  if ((wesl === undefined) === (wgsl === undefined)) throw new TypeError("ShaderMesh: give its source as wesl (or wgsl), one of them");
  const src = wesl ?? wgsl;
  if (typeof src !== "string" || !/fn\s+shade\s*\(|@fragment/.test(src)) throw new TypeError(`ShaderMesh: ${wesl !== undefined ? "wesl" : "wgsl"} defines fn shade(in: VertexOutput) -> vec4<f32>, or its own @fragment`);
  const ident = /^[A-Za-z_][A-Za-z0-9_]*$/;
  const us = Object.entries(uniforms);
  if (us.length > 16) throw new RangeError(`ShaderMesh: at most 16 uniforms, got ${us.length}`);
  for (const [k, v] of us) {
    if (!ident.test(k)) throw new TypeError(`ShaderMesh: uniform ${JSON.stringify(k)} isn't a WGSL name`);
    const ok = (typeof v === "number" && Number.isFinite(v)) || (typeof v === "string" && /^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(v)) || (Array.isArray(v) && v.length >= 2 && v.length <= 4 && v.every(Number.isFinite));
    if (!ok) throw new TypeError(`ShaderMesh: uniform ${k} is a number, [x, y(, z(, w))] or "#rrggbb", got ${JSON.stringify(v)}`);
  }
  checkTextures("ShaderMesh", textures);
  const mesh = typeof shape === "string" ? `shape:${shape}` : shape;
  const material: Record<string, unknown> = { ...(wesl !== undefined ? { wesl } : { wgsl }), uniforms, textures, ...(defs !== undefined ? { defs: shaderDefs("ShaderMesh", defs) } : {}), ...(alpha ? { alpha } : {}), ...(doubleSided ? { double_sided: true } : {}) };
  return (
    <Entity {...place}>
      <BevyComponent type="bevy_mesh::components::Mesh3d" value={mesh} />
      <BevyComponent type="bevy_pbr::mesh_material::MeshMaterial3d<raea_sys_material::shader_material::ShaderMaterial>" value={material} />
    </Entity>
  );
}

export interface BevyResourceProps {
  /** the resource's Rust type path, as the TypeRegistry names it */
  type: string;
  /** its value as Bevy reflects it (JSON) */
  value: unknown;
}

/** A Bevy resource (a plugin's settings: gravity), applied through the TypeRegistry. Inside the Canvas3D. */
export function BevyResource(props: BevyResourceProps) {
  if (typeof props.type !== "string" || !props.type.includes("::")) throw new TypeError(`BevyResource: type is a full type path, got ${JSON.stringify(props.type)}`);
  return node3d("BevyResource", { type: props.type, value: props.value ?? null });
}
