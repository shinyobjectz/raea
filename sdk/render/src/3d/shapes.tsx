// Shapes and meshes: Bevy's primitives, buffer meshes, 3D text's props, and <Mesh> with its shape from its children.
import { Children, Fragment, isValidElement, memo, useId, type ReactElement, type ReactNode } from "react";
import { Solid } from "../solid";
import { useAnimation3d, type Keys, type Move, type TrackSpec } from "@raea/math/animate";
import { Place3d, usePlace3d, usePointer3d, type LocalBounds, type PointerHandlers3d } from "../events3d";
import { Canvas3DProps, Vec3, checkArgs, checkPass, checkVec, node3d, sameProps, serialisable } from "./all";
import { InCanvas3D } from "./model";
import { usePlannedLod, type PlannedLod } from "./detail";
import type { MeshAttribute } from "./attribute";
import { firstKey, useSequenceFrames } from "./clock";
import { Text3D, text3dShape, type Text3DProps } from "./text3d";
export { Text3D, type Text3DProps };

const FILL = { position: "absolute", left: 0, top: 0, width: "100%", height: "100%" } as const;

/** Only for `layered`: the 3D is drawn in the page's order, at the box of the nearest positioned element around it
 * (the whole frame when there's none; B3 H7), at that box's size and aspect:
 *
 *   <div style={{ position: "absolute", left: 100, top: 50, width: 200, height: 150 }}>
 *     <Canvas3D layered><Camera3d transform={...} /><Mesh3d>...</Mesh3d></Canvas3D>
 *   </div>
 * Without `layered` it is its children, as written straight in the page (M6).
 * @deprecated without `layered`: not needed, a <Camera3d> makes a view */
export function Canvas3D(props: Canvas3DProps) {
  if (!props.layered) return <>{props.children}</>;
  // layered: a full-frame box in the page's flow, and the slot the 3D's frame is painted into (an <img> the renderer
  // points at the 3D's texture), first so the 3D parts after it stay out of the page's paint. Cleared to a colour, it
  // is under every other part of the page (z-index the least there is), as a Bevy camera's output is under all its UI
  return (
    <div data-3d="Canvas3D" data-props={JSON.stringify(serialisable(props))} style={props.clearColor === undefined ? FILL : { ...FILL, zIndex: -2147483648 }}>
      <img data-raea-scene="" data-clear={props.clearColor} alt="" style={{ ...FILL, objectFit: "fill" }} />
      <InCanvas3D.Provider value={true}>{props.children}</InCanvas3D.Provider>
    </div>
  );
}

// Geometries. Each knows its Manifold expression; the defaults are unit sizes.
export interface BoxProps {
  /** width, height and depth in metres ([1, 1, 1]) */
  args?: [number, number, number];
}

/** Bevy's Cuboid: half its size along each axis ([0.5, 0.5, 0.5], a 1 m cube, as Bevy's default). Bevy's UVs show a
 *  picture upside down on its front face (+z) and the right way up on its back (-z): turn a printed label half a turn. */
export interface CuboidProps {
  /** half its size along x, y and z, metres ([0.5, 0.5, 0.5]) */
  half_size?: [number, number, number];
}

/** `args`, or Bevy's Sphere field: radius (0.5 by Bevy's default). Its mesh is Bevy's Sphere::mesh(): an icosphere of 5
 *  subdivisions, or `ico` of them (Bevy's `.ico(n)`, 0 to 80), or a UV sphere of `uv` = [sectors, stacks] (`.uv(s, t)`). */
export interface SphereProps {
  /** [radius], as three.js's args */
  args?: [number];
  /** metres (0.5) */
  radius?: number;
  /** an icosphere of this many subdivisions (5 by default; 0 to 80) */
  ico?: number;
  /** a UV sphere instead: [sectors, stacks] */
  uv?: [number, number];
}

export interface PlaneProps {
  /** width and height in metres ([1, 1]) */
  args?: [number, number];
}

/** `args`, or Bevy's Cylinder fields: radius and half_height (0.5 each by Bevy's default). */
export interface CylinderProps {
  /** [radius top, radius bottom, height], as three.js's args */
  args?: [number, number, number];
  /** metres (0.5) */
  radius?: number;
  /** half its height, metres (0.5) */
  half_height?: number;
}

/** `args`, or Bevy's Torus fields: major_radius (0.75) and minor_radius (0.25), the tube's. */
export interface TorusProps {
  /** [major radius, minor radius] */
  args?: [number, number];
  /** from the centre to the middle of the tube, metres (0.75) */
  major_radius?: number;
  /** the tube's radius, metres (0.25) */
  minor_radius?: number;
}

const list = (ns: number[]) => ns.map((n) => String(n)).join(", ");

// each geometry's Manifold expression from its element's props (args, or Bevy's fields)
const GEOMETRY_SHAPES = new Map<unknown, (props: Record<string, unknown>) => string>();

// and its bounds in its own space, for picking (events3d.tsx)
const GEOMETRY_BOUNDS = new Map<unknown, (props: Record<string, unknown>) => LocalBounds | undefined>();

/** A geometry's bounds in its own space by its kind and args, as its Manifold expression draws it about its origin. */
function boundsOf(name: string, a: number[]): LocalBounds | undefined {
  const box = (x: number, y: number, z: number): LocalBounds => ({ box: [[-x, -y, -z], [x, y, z]] });
  switch (name) {
    case "Box": return box(a[0] / 2, a[1] / 2, a[2] / 2);
    case "Sphere": return { radius: a[0] };
    case "Plane": return box(a[0] / 2, a[1] / 2, 0);
    case "Cylinder": return box(Math.max(a[0], a[1]), a[2] / 2, Math.max(a[0], a[1]));
    case "Torus": return box(a[0] + a[1], a[0] + a[1], a[1]);
    case "Cone": return box(a[0], a[1] / 2, a[0]);
    case "Capsule": return box(a[0], a[1] / 2 + a[0], a[0]);
  }
  return undefined;
}

/** Bevy's fields for a shape (docs/sdk.feature S2a): their names, and the args they make, Bevy's defaults filling in. */
interface BevyShape<P> {
  keys: string[];
  args: (p: P) => number[];
}

function geometry<P extends { args?: number[] }>(name: string, defaults: number[], shape: (a: number[], props: P) => string, bevy?: BevyShape<P>, display = name) {
  const own = (props: P) => bevy !== undefined && bevy.keys.some((k) => (props as Record<string, unknown>)[k] !== undefined);
  const argsOf = (props: P) => {
    if (own(props) && props.args !== undefined) throw new TypeError(`${display}: args or Bevy's fields (${bevy!.keys.join(", ")}), not both`);
    return checkArgs(display, own(props) ? bevy!.args(props) : (props.args ?? defaults), defaults.length);
  };
  const toShape = (props: Record<string, unknown>) => shape(argsOf(props as P), props as P);
  const Component = (props: P) => {
    const args = argsOf(props);
    shape(args, props);
    const rest = own(props) ? Object.fromEntries(Object.entries(props).filter(([k]) => !bevy!.keys.includes(k))) : props;
    return node3d(name, { ...rest, args });
  };
  Object.defineProperty(Component, "name", { value: display });
  // memo: a geometry's props seldom change while its Mesh3d moves every frame, so React skips it (FC1b, bloom_3d)
  const Memo = Object.assign(memo(Component, sameProps), { displayName: display });
  GEOMETRY_SHAPES.set(Memo, toShape);
  GEOMETRY_BOUNDS.set(Memo, (props) => boundsOf(name, argsOf(props as P)));
  return Memo as unknown as typeof Component;
}

/** @deprecated <Cuboid half_size>, raea/mesh */
export const Box = /* @__PURE__ */ geometry<BoxProps>("Box", [1, 1, 1], (a) => `box(${list(a)})`);

/** Bevy's Cuboid: `<Cuboid half_size={[1, 0.5, 1]} />` is a Box of args [2, 1, 2]. */
const CuboidShape = /* @__PURE__ */ geometry<CuboidProps & Pick<BoxProps, "args">>("Box", [1, 1, 1], (a) => `box(${list(a)})`, { keys: ["half_size"], args: (p) => (p.half_size ?? [0.5, 0.5, 0.5]).map((h) => 2 * h) }, "Cuboid");

/** Bevy's Cuboid::from_size: the fields for a cuboid of that whole size, `<Cuboid {...Cuboid.fromSize(size)} />`. */
function fromSize(size: [number, number, number]): Required<CuboidProps> {
  checkVec("Cuboid.fromSize", "size", size);
  return { half_size: [size[0] / 2, size[1] / 2, size[2] / 2] };
}

export const Cuboid = /* @__PURE__ */ Object.assign(CuboidShape, { fromSize });

/** A sphere's expression: sphere(r), sphere(r, ico n) as sphere(r, n), or uv_sphere(r, sectors, stacks). */
function sphereOf(r: number, p: SphereProps): string {
  if (p.ico !== undefined && p.uv !== undefined) throw new TypeError("Sphere: ico or uv, not both");
  if (p.ico !== undefined) {
    if (!Number.isInteger(p.ico) || p.ico < 0 || p.ico > 80) throw new TypeError(`Sphere: ico is a whole number of subdivisions from 0 to 80, not ${p.ico}`);
    return `sphere(${r}, ${p.ico})`;
  }
  if (p.uv !== undefined) {
    const [s, t] = p.uv;
    if (!Number.isInteger(s) || !Number.isInteger(t) || s < 3 || t < 2) throw new TypeError(`Sphere: uv is [sectors, stacks], whole numbers, at least [3, 2], not [${p.uv}]`);
    return `uv_sphere(${r}, ${s}, ${t})`;
  }
  return `sphere(${r})`;
}

export const Sphere = /* @__PURE__ */ geometry<SphereProps>("Sphere", [1], (a, p) => `${sphereOf(a[0], p)}`, { keys: ["radius", "ico", "uv"], args: (p) => [p.radius ?? 0.5] });

/** @deprecated <Plane3d>, raea/mesh */
export const Plane = /* @__PURE__ */ geometry<PlaneProps>("Plane", [1, 1], (a) => `box(${list(a)}, 0)`);

/** Bevy's Plane3d: a flat rectangle facing `normal` (Bevy's default +Y, a floor), `half_size` its half extent along
 *  its two axes ([0.5, 0.5]), laid out as Bevy's PlaneMeshBuilder does: facing +Z it is a Plane of args
 *  [2 x half_size]. The normal is along an axis; a tilted plane is a rotated Mesh3d. */
export interface Plane3dProps {
  /** the way it faces, along an axis (+Y by default, a floor) */
  normal?: Vec3;
  /** half its extent along its two axes, metres ([0.5, 0.5]) */
  half_size?: [number, number];
}

// Bevy turns the +Y plane (half_size x along x, y along -z) to its normal by the shortest arc; along an axis that
// leaves its two sizes on these axes. Facing z it is the old Plane exactly; facing x or y a Box with no depth.
const PLANE_AXES: Record<string, (w: number, h: number) => number[]> = {
  "0,1,0": (w, h) => [w, 0, h], "0,-1,0": (w, h) => [w, 0, h],
  "0,0,1": (w, h) => [w, h], "0,0,-1": (w, h) => [w, h],
  "1,0,0": (w, h) => [0, w, h], "-1,0,0": (w, h) => [0, w, h],
};

function planeArgs(p: Plane3dProps): number[] {
  const n = p.normal ?? [0, 1, 0];
  const along = n.filter((c) => c !== 0).length === 1 ? PLANE_AXES[n.map(Math.sign).join(",")] : undefined;
  if (!along) throw new TypeError(`Plane3d: normal is along an axis ([0, 1, 0], [0, 0, 1], ...), got ${JSON.stringify(n)}; turn a Mesh3d for a tilted plane`);
  const [hx, hy] = checkArgs("Plane3d", p.half_size ?? [0.5, 0.5], 2);
  return along(2 * hx, 2 * hy);
}

export function Plane3d(props: Plane3dProps) {
  const { normal: _n, half_size: _h, ...rest } = props;
  const args = planeArgs(props);
  return args.length === 2 ? <Plane {...rest} args={args as [number, number]} /> : <Box {...rest} args={args as [number, number, number]} />;
}

// plane(nx, ny, nz, w, h): one face, as Bevy's PlaneMeshBuilder makes it (systems/material mesh.rs), where a box with no
// depth has two (mesh_ray_cast's box of inward planes, seen from outside, drew its near walls); its Manifold is that box
GEOMETRY_SHAPES.set(Plane3d, (props) => {
  const p = props as Plane3dProps;
  planeArgs(p);
  const [hx, hy] = p.half_size ?? [0.5, 0.5];
  return `plane(${list([...(p.normal ?? [0, 1, 0]), 2 * hx, 2 * hy])})`;
});

GEOMETRY_BOUNDS.set(Plane3d, (props) => {
  const a = planeArgs(props as Plane3dProps);
  return a.length === 2 ? boundsOf("Plane", a) : boundsOf("Box", a);
});

/** [radiusTop, radiusBottom, height], standing up (along y) as three.js has it. */
export const Cylinder = /* @__PURE__ */ geometry<CylinderProps>("Cylinder", [1, 1, 1], (a) => `rotate(cone(${list([a[1], a[0], a[2]])}), -90, 0, 0)`, { keys: ["radius", "half_height"], args: (p) => [p.radius ?? 0.5, p.radius ?? 0.5, 2 * (p.half_height ?? 0.5)] });

/** Bevy's ConicalFrustum: a cylinder whose ends differ, radius_top and radius_bottom (Bevy's 0.25 and 0.5) and its
 *  height (0.5), standing up: a tapered Cylinder of args [radius_top, radius_bottom, height]. */
export interface ConicalFrustumProps {
  /** the top end's radius, metres (0.25) */
  radius_top?: number;
  /** the bottom end's radius, metres (0.5) */
  radius_bottom?: number;
  /** metres (0.5) */
  height?: number;
  /** [radius_top, radius_bottom, height] */
  args?: [number, number, number];
}

export const ConicalFrustum = /* @__PURE__ */ geometry<ConicalFrustumProps>("Cylinder", [0.25, 0.5, 0.5], (a) => `rotate(cone(${list([a[1], a[0], a[2]])}), -90, 0, 0)`, { keys: ["radius_top", "radius_bottom", "height"], args: (p) => [p.radius_top ?? 0.25, p.radius_bottom ?? 0.5, p.height ?? 0.5] }, "ConicalFrustum");

export const Torus = /* @__PURE__ */ geometry<TorusProps>("Torus", [1, 0.4], (a) => `torus(${list(a)})`, { keys: ["major_radius", "minor_radius"], args: (p) => [p.major_radius ?? 0.75, p.minor_radius ?? 0.25] });

export interface ConeProps {
  /** [radius, height] */
  args?: [number, number];
  /** the base's radius, metres (0.5) */
  radius?: number;
  /** metres (1) */
  height?: number;
}

/** A cone: [radius, height], its point up (along y). */
export const Cone = /* @__PURE__ */ geometry<ConeProps>("Cone", [0.5, 1], (a) => `rotate(cone(${list([a[0], 0, a[1]])}), -90, 0, 0)`, { keys: ["radius", "height"], args: (p) => [p.radius ?? 0.5, p.height ?? 1] });

export interface CapsuleProps {
  /** [radius, length of the straight part] ([0.5, 1]) */
  args?: [number, number];
}

/** A capsule (Bevy's Capsule3d, three.js's CapsuleGeometry): [radius, length of the straight part], standing up
 * (along y); its whole height is length + 2 x radius. A character's stand-in, a pill, a collider's look.
 * @deprecated <Capsule3d>, raea/mesh */
export const Capsule = /* @__PURE__ */ geometry<CapsuleProps>("Capsule", [0.5, 1], (a) => `hull(move(sphere(${a[0]}), 0, ${a[1] / 2}, 0), move(sphere(${a[0]}), 0, ${-a[1] / 2}, 0))`);

/** Bevy's Capsule3d: radius and half_length, the straight part's half (0.5 each by Bevy's default), standing up. */
export interface Capsule3dProps {
  /** metres (0.5) */
  radius?: number;
  /** half the straight part's length, metres (0.5) */
  half_length?: number;
  /** [radius, length of the straight part] */
  args?: [number, number];
}

export const Capsule3d = /* @__PURE__ */ geometry<Capsule3dProps>("Capsule", [0.5, 1], (a) => `hull(move(sphere(${a[0]}), 0, ${a[1] / 2}, 0), move(sphere(${a[0]}), 0, ${-a[1] / 2}, 0))`, { keys: ["radius", "half_length"], args: (p) => [p.radius ?? 0.5, 2 * (p.half_length ?? 0.5)] }, "Capsule3d");

export interface BufferMeshProps {
  /** x, y, z of each vertex */
  positions: number[];
  /** three vertex indices a triangle (counter-clockwise seen from its front); without them each three positions are one.
   * With normals too it is Bevy's indexed Mesh, its index buffer kept, as a compute that writes a mesh expects */
  indices?: number[];
  /** x, y, z of each vertex's normal (without them each face is flat, facing its front) */
  normals?: number[];
  /** u, v of each vertex (without them a picture map is projected onto the mesh's bounds) */
  uvs?: number[];
  /** each vertex's colour: r, g, b, a (linear, 0 to 1) per vertex, or one "#rrggbb" per vertex; the material's colour
   * is multiplied by it (Bevy's ATTRIBUTE_COLOR), so leave the material white to see them as given */
  colors?: number[] | string[];
  /** a mesh's own attributes beyond those, for a three.js shader that reads them (<ThreeShaderMaterial>): each one's
   * size (1 to 4 numbers a vertex) and values, per vertex */
  attributes?: Record<string, { size: number; values: number[] }>;
  /** the mesh repeated `count` times as one, for a three.js node material drawn as copies (an InstancedMesh's,
   * docs/r3f.feature R3F22): each copy's number is its own attribute `tsl_copy`, each of `attributes` (size numbers
   * a copy) is on every vertex of its copy, and `matrices` (16 numbers a copy, column-major) place the copies (none:
   * each where the mesh is) */
  copies?: { count: number; attributes?: Record<string, { size: number; values: number[] }>; matrices?: number[] };
}

/** Each mesh's shape text, checked and written once: React draws a <BufferMesh> every frame with the same arrays, and
 * a terrain's are tens of thousands of numbers (checking and writing them took half a live frame in a browser). Kept by
 * its positions array, and remade when any of its other arrays is a new one. */
const shaped = new WeakMap<object, { of: unknown[]; text: string }>();

function bufferShape(p: BufferMeshProps): string {
  const of = [p.indices, p.normals, p.uvs, p.colors, p.attributes, p.copies];
  const had = Array.isArray(p.positions) ? shaped.get(p.positions) : undefined;
  if (had && had.of.every((a, i) => a === of[i])) return had.text;
  const text = writeBufferShape(p);
  shaped.set(p.positions, { of, text });
  return text;
}

function writeBufferShape(p: BufferMeshProps): string {
  const nums = (k: keyof BufferMeshProps, per: number) => {
    const a = p[k];
    if (a === undefined) return;
    if (!Array.isArray(a) || !a.every((x) => typeof x === "number" && Number.isFinite(x))) throw new TypeError(`BufferMesh: ${k} is an array of numbers`);
    if (k !== "indices" && a.length !== (p.positions.length / 3) * per) throw new RangeError(`BufferMesh: ${k} has ${per} numbers for each of the ${p.positions.length / 3} vertices, got ${a.length}`);
  };
  if (!Array.isArray(p.positions) || p.positions.length < 9 || p.positions.length % 3 !== 0) throw new RangeError(`BufferMesh: positions is x, y, z for each vertex, three or more, got ${p.positions?.length} numbers`);
  nums("positions", 3);
  nums("normals", 3);
  nums("uvs", 2);
  nums("indices", 1);
  const n = p.positions.length / 3;
  if (p.colors !== undefined) {
    const c = p.colors as unknown[];
    const hex = c.every((x) => typeof x === "string" && /^#[0-9a-fA-F]{6}$/.test(x as string));
    if (hex ? c.length !== n : !(c.every((x) => typeof x === "number" && Number.isFinite(x)) && c.length === n * 4)) throw new RangeError(`BufferMesh: colors is r, g, b, a for each of the ${n} vertices, or one "#rrggbb" each`);
  }
  if (p.indices && ((p.normals === undefined && p.indices.length % 3 !== 0) || p.indices.some((i) => !Number.isInteger(i) || i < 0 || i >= n))) throw new RangeError(`BufferMesh: indices are three a triangle (any number with normals), each a vertex 0..${n - 1}`);
  for (const [k, a] of Object.entries(p.attributes ?? {})) {
    if (!(a && Number.isInteger(a.size) && a.size >= 1 && a.size <= 4 && Array.isArray(a.values) && a.values.length === n * a.size)) throw new RangeError(`BufferMesh: attribute ${k} is {size: 1 to 4, values: size numbers for each of the ${n} vertices}`);
  }
  const c = p.copies;
  if (c !== undefined) {
    if (!(Number.isInteger(c.count) && c.count >= 1)) throw new RangeError(`BufferMesh: copies.count is a whole number of copies, 1 or more, got ${c.count}`);
    for (const [k, a] of Object.entries(c.attributes ?? {})) {
      if (!(a && Number.isInteger(a.size) && a.size >= 1 && a.size <= 4 && Array.isArray(a.values) && a.values.length === c.count * a.size)) throw new RangeError(`BufferMesh: copies attribute ${k} is {size: 1 to 4, values: size numbers for each of the ${c.count} copies}`);
    }
    if (c.matrices !== undefined && !(Array.isArray(c.matrices) && c.matrices.length === c.count * 16)) throw new RangeError(`BufferMesh: copies.matrices are 16 numbers for each of the ${c.count} copies`);
  }
  return "buffer:" + JSON.stringify({ positions: p.positions, indices: p.indices, normals: p.normals, uvs: p.uvs, colors: p.colors, attributes: p.attributes, copies: c });
}

/** A mesh from its own vertex buffers, as the geometry of a <Mesh> (Bevy's Mesh built from attributes): a terrain from
 * a height field, a procedural shape, a ribbon, an imported point list. Drawn as flat triangles, or with indices and
 * normals as Bevy's indexed Mesh.
 * @bevy bevy_mesh::mesh::Mesh */
export function BufferMesh(props: BufferMeshProps) {
  bufferShape(props);
  return node3d("BufferMesh", {});
}

export interface MeshProps extends PointerHandlers3d {
  /** what the mesh is called across frames: give one when meshes come and go, so each keeps its own motion */
  name?: string;
  /** false: its 3D type (a Text3D child) is scenery, not held to raea render's type checks (SG10); a Text3D's own
   * `read` sets it */
  read?: boolean;
  /** the composition pass it draws in ("scene" by default): a frame-wide effect told to skip a pass (<Look skip>,
   * <Outline skip>, <Bloom skip>, <Grade skip>) leaves its pixels as drawn, and <Look passes> gives one its own paint;
   * a piece names at most three passes besides "scene"; keyed per frame */
  pass?: string;
  /** a rotation as a quaternion [x, y, z, w], Bevy's Transform.rotation, in place of `rotation` (<Mesh3d transform> sends it) */
  quaternion?: [number, number, number, number];
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  scale?: number | Vec3;
  /** it may pass through other parts on purpose (a blade into a body, a ghost through a wall): raea's clipping
   * check leaves it out */
  allowOverlap?: boolean;
  /** set at an angle on purpose (a bowl knocked over, a mug on its side): `raea scene lint` doesn't call it tilted */
  posed?: boolean;
  /** its planned detail, 0 (the hero) to 4 (far): how large it is meant to be seen. A render that shows it finer
   * says so (`lod-closer`); a grey box or a far backdrop is planned at 3 or 4 (PlannedLod) */
  lod?: PlannedLod;
  /** a Manifold solid (manifold.box(...).minus(...)) or its expression; wins over the geometry child */
  shape?: string | Solid;
  /** a Bevy plugin moves it (a rigid body): position, rotation and scale set where it starts, then the plugin's */
  driven?: boolean;
  /** false: it casts no shadow (a window's glass, which a blended surface otherwise does; a sky card; a UI floating in
   * the scene). True by default. "only": it casts its shadow but isn't drawn (a roof or wall out of frame whose shadow
   * falls into it, a blind's slats cutting the sun's shafts in the fog, a stand-in shape for something drawn another
   * way); its surface is ignored. "catch": it shows only the shadows falling on it, in its material's colour at its
   * material's opacity, and is clear elsewhere (three.js's ShadowMaterial: a floor under a product over a picture) */
  shadow?: boolean | "only" | "catch";
  /** false: no material at all, as a Bevy Mesh3d without a MeshMaterial3d, so only a pass of its own draws it (a
   *  <MeshPass>: Bevy's shader_advanced/specialized_mesh_pipeline); its surface is ignored */
  material?: false;
  /** three.js's name for `shadow`'s true and false (`shadow` wins when both are given) */
  castShadow?: boolean;
  /** a page drawn on it over its UVs: the screen spec <Mesh3d> makes for a page element with layer="surface"
   * (sdk/render layers.tsx meshLayers), not written by hand */
  surface?: object;
  /** false: no shadow falls on it (Bevy's NotShadowReceiver: a backdrop that stays clean under a floating product, a
   * glowing sign). True by default */
  receiveShadow?: boolean;
  /** its geometry's own vertex attributes, as Bevy's Mesh::with_inserted_attribute adds them: [[attribute, values]],
   *  a value a vertex (a Cuboid has 24), for a Material whose vertex_attributes read them */
  attributes?: MeshAttribute[];
  /** its geometry compressed as Bevy's Mesh::compressed_mesh(&MeshCompressionArgs::regular()) does (positions snorm16 in
   * its bounds, normals octahedral, u16 indices): smaller on the GPU, and a shader of your own reads its positions
   * under VERTEX_POSITIONS_COMPRESSED */
  compressed?: boolean;
  /** the render layers it's on ([0] by default; Bevy's RenderLayers): a camera draws it only when they share a layer,
   * and only lights sharing a layer light it */
  layers?: number[];
  /** declared animation, sampled by the engine each frame: x, y, z set the position, rotateX, rotateY, rotate the
   * rotation (radians), scale; `animate={{ y: track({ 0: -2, 12: 0 }) }}` or a move (raea animate) */
  animate?: Animate3d;
  children?: ReactNode;
}

/** A 3D node's declared animation: tracks by property, or a move. */
export type Animate3d = Record<string, TrackSpec | TrackSpec[] | Keys> | Move;

/** Under plain bun, what a 3D animation sets now, onto the node's props (raea-animate's patch_3d, the same rules). */
export function apply3d<T extends object>(props: T, v: Record<string, number | string> | undefined): T {
  if (!v) return props;
  const out = { ...props } as Record<string, unknown>;
  const vec = (k: string, d: number): number[] => { const x = out[k]; return Array.isArray(x) ? [...x] : typeof x === "number" ? [x, x, x] : [d, d, d]; };
  if (v.x !== undefined || v.y !== undefined || v.z !== undefined) { const p = vec("position", 0); ["x", "y", "z"].forEach((k, i) => { if (v[k] !== undefined) p[i] = v[k] as number; }); out.position = p; }
  if (v.rotateX !== undefined || v.rotateY !== undefined || v.rotate !== undefined) { const r = vec("rotation", 0); ["rotateX", "rotateY", "rotate"].forEach((k, i) => { if (v[k] !== undefined) r[i] = v[k] as number; }); out.rotation = r; }
  if (v.scaleX !== undefined || v.scaleY !== undefined) { const sx = (v.scaleX as number) ?? 1, sy = (v.scaleY as number) ?? 1; out.scale = sx === sy ? sx : [sx, sy, 1]; }
  for (const [k, x] of Object.entries(v)) if (!["x", "y", "z", "rotateX", "rotateY", "rotate", "scaleX", "scaleY"].includes(k)) out[k] = x;
  return out as T;
}

/** a mesh's element children with fragments opened: <><Sphere /><StandardMaterial /></> is the same mesh as its two */
const flat = (kids: ReactNode): ReactElement[] => Children.toArray(kids).flatMap((c) => (!isValidElement(c) ? [] : c.type === Fragment ? flat((c as ReactElement<{ children?: ReactNode }>).props.children) : [c]));

/** A Text3D child's `read`, when it has one. */
function text3dRead(children: ReactNode): boolean | undefined {
  for (const child of flat(children)) if (child.type === Text3D) return (child as ReactElement<Text3DProps>).props.read;
  return undefined;
}

export function shapeFromChildren(children: ReactNode): string | undefined {
  for (const child of flat(children)) {
    if (child.type === Text3D) {
      return text3dShape((child as ReactElement<Text3DProps>).props);
    }
    if (child.type === BufferMesh) {
      return bufferShape((child as ReactElement<BufferMeshProps>).props);
    }
    const s = GEOMETRY_SHAPES.get(child.type)?.((child as ReactElement<Record<string, unknown>>).props);
    if (s !== undefined) return s;
  }
  return undefined;
}

/** The first geometry child's bounds in its own space. */
function boundsFromChildren(children: ReactNode): LocalBounds | undefined {
  for (const child of flat(children)) {
    const b = GEOMETRY_BOUNDS.get(child.type)?.(child.props as Record<string, unknown>);
    if (b !== undefined) return b;
  }
  return undefined;
}

/** A drawn object: a geometry child (or `shape`) and a material child, placed by position, rotation and scale. Its
 * on* props (onClick, onPointerDown, onPointerOver ...) are called as a page element's (events3d.tsx).
 * castShadow={false} and receiveShadow={false} put Bevy's markers on it
 * `layers` is its RenderLayers (a light's and a camera's too)
 * shadow="only" draws it with the shadow-only material (engine/render shadow_only.rs):
 * @deprecated <Mesh3d>, raea/mesh (2D: <Mesh2d>, raea/sprite)
 */
export function Mesh(props: MeshProps) {
  useSequenceFrames("Mesh3d", props.name, [["animate's first key", firstKey(props.animate)]]);
  const anim = useAnimation3d(props.animate);
  const { position = [0, 0, 0], rotation = [0, 0, 0], scale = 1, shape } = apply3d(props, anim.values);
  checkVec("Mesh", "position", position);
  checkVec("Mesh", "rotation", rotation);
  if (typeof scale === "number") {
    if (!(scale > 0) || !Number.isFinite(scale)) throw new RangeError(`Mesh: scale must be a positive number, got ${scale}`);
  } else checkVec("Mesh", "scale", scale);
  if (props.shadow !== undefined && typeof props.shadow !== "boolean" && props.shadow !== "only" && props.shadow !== "catch") throw new TypeError(`Mesh: shadow is true, false, "only" or "catch", got ${JSON.stringify(props.shadow)}`);
  if (props.material !== undefined && props.material !== false) throw new TypeError(`Mesh: material is false (no material) or left out, got ${JSON.stringify(props.material)}`);
  checkPass("Mesh", props.pass);
  const manifold = shape instanceof Solid ? shape.expr : shape ?? shapeFromChildren(props.children);
  // a Text3D's read={false} rides on its mesh, which the stage checks read (SG10)
  const read = props.read ?? text3dRead(props.children);
  const lod = usePlannedLod("Mesh", props.lod);
  const extra: Record<string, string> = {};
  if (manifold !== undefined) extra["data-shape"] = manifold as string;
  if (anim.attr) extra["data-raea-anim"] = anim.attr;
  const id = useId();
  const m = usePlace3d(position, rotation, scale, props.quaternion);
  usePointer3d(props.name ?? id, m, () => (shape === undefined ? boundsFromChildren(props.children) : undefined), props as Record<string, unknown>);
  return node3d("Mesh", { ...props, read, lod, animate: undefined, shape: undefined, position, rotation, scale, attributes: props.attributes && JSON.stringify(props.attributes) }, <Place3d.Provider value={m}>{props.children}</Place3d.Provider>, Object.keys(extra).length ? extra : undefined);
}
