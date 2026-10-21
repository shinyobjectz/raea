// @raea/motion/3d: 3D in the same tree (spec: "3D in the same tree"). Every component renders as an element that
// carries its name (data-3d) and props (data-props, JSON) for Bevy to draw; a Mesh also carries its Manifold
// expression (data-shape). Engine-safe: this module imports nothing beyond React.
import { Children, isValidElement, type ReactElement, type ReactNode } from "react";
import { useCurrentFrame, useVideoConfig } from "./clock";
import type { World } from "./ecs";
import { Solid } from "./solid";
import { useAnimation3d, type Keys, type Move, type TrackSpec } from "./animate";

export { CrossSection, crossSection, manifold, outline, Solid, type Bounds, type Polygons, type Section } from "./solid";
// footage with 3D standing in its world: the camera solved, the plate behind, the subject in front (matchmove.tsx)
// gaussian splats: a reconstructed scene or object, placed and animated by props (splat.tsx)
export { Splat, type SplatProps, type SplatWave, type SplatMorph } from "./splat";
// physics: rigid bodies baked by Rapier, glitches on parts, goo baked by MLS-MPM (physics.tsx)
export { Physics, Body, Part, Echoes, ZFight, MeshSequence, Goo, useGoo, gooSpec, useBake, usePhysics, bodyPose, bodyVelocity, physicsRequest, part, teleport, snapPose, lastBeat, hash01, about, quatAxis, quatMul, type PhysicsProps, type BodyProps, type PartProps, type PartInfo, type PartMaterial, type Glitch, type Impulse, type Shape, type Baked, type MeshSequenceProps, type GooProps, type GooObject, type GooBake } from "./physics";
export { Matchmove, useMatchmove, matchmoveView, matchmoveIndex, checkMatchmove, type MatchmoveProps, type MatchmoveSolve, type MatchmoveState, type MatchmoveCamera } from "./matchmove";

export type Vec3 = [number, number, number];

/** The props worth sending to the renderer: everything but children and functions. */
function serialisable(props: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(props)) {
    if (k === "children" || typeof v === "function" || v === undefined) continue;
    out[k] = v;
  }
  return out;
}

function node3d(kind: string, props: Record<string, unknown>, children?: ReactNode, extra?: Record<string, string>) {
  return (
    <div data-3d={kind} data-props={JSON.stringify(serialisable(props))} {...extra}>
      {children}
    </div>
  );
}

function checkVec(owner: string, name: string, v: unknown): void {
  if (!Array.isArray(v) || v.length !== 3 || !v.every((n) => typeof n === "number" && Number.isFinite(n))) {
    throw new TypeError(`${owner}: ${name} must be three finite numbers [x, y, z], got ${JSON.stringify(v)}`);
  }
}

/** A geometry's sizes. three.js puts segment counts after them (`[r, h, 32]`); those are accepted and dropped, since
 * the engine tessellates on its own. */
function checkArgs(owner: string, args: unknown, count: number): number[] {
  if (!Array.isArray(args) || args.length < count || !args.every((n) => typeof n === "number" && Number.isFinite(n))) {
    throw new TypeError(`${owner}: args must be ${count} finite number${count === 1 ? "" : "s"}, got ${JSON.stringify(args)}`);
  }
  return args.slice(0, count);
}

export interface Canvas3DProps {
  /** the 3D takes its place in the page's tree order instead of lying under the whole page: page elements before the
   * Canvas3D (and the backgrounds of the elements holding it) are under the 3D, showing wherever it has nothing, and
   * hidden by its parts; positioned elements after it are over it. The 3D's background is transparent. Put the
   * Canvas3D straight in a full-frame AbsoluteFill. */
  layered?: boolean;
  children?: ReactNode;
  [prop: string]: unknown;
}

const FILL = { position: "absolute", left: 0, top: 0, width: "100%", height: "100%" } as const;

/** The 3D scene's root; everything 3D goes inside it. */
export function Canvas3D(props: Canvas3DProps) {
  if (!props.layered) return node3d("Canvas3D", props, props.children);
  // layered: a full-frame box in the page's flow, and the slot the 3D's frame is painted into (an <img> the renderer
  // points at the 3D's texture), first so the 3D parts after it stay out of the page's paint
  return (
    <div data-3d="Canvas3D" data-props={JSON.stringify(serialisable(props))} style={FILL}>
      <img data-raea-scene="" alt="" style={{ ...FILL, objectFit: "fill" }} />
      {props.children}
    </div>
  );
}


// Geometries. Each knows its Manifold expression; the defaults are unit sizes.
export interface BoxProps { args?: [number, number, number] }
export interface SphereProps { args?: [number] }
export interface PlaneProps { args?: [number, number] }
export interface CylinderProps { args?: [number, number, number] }
export interface TorusProps { args?: [number, number] }

const list = (ns: number[]) => ns.map((n) => String(n)).join(", ");

const GEOMETRY_SHAPES = new Map<unknown, (args: unknown) => string>();

function geometry<P extends { args?: number[] }>(name: string, defaults: number[], shape: (a: number[]) => string) {
  const toShape = (args: unknown) => shape(checkArgs(name, args ?? defaults, defaults.length));
  const Component = (props: P) => {
    const args = checkArgs(name, props.args ?? defaults, defaults.length);
    toShape(args);
    return node3d(name, { ...props, args });
  };
  Object.defineProperty(Component, "name", { value: name });
  GEOMETRY_SHAPES.set(Component, toShape);
  return Component;
}

export const Box = geometry<BoxProps>("Box", [1, 1, 1], (a) => `box(${list(a)})`);
export const Sphere = geometry<SphereProps>("Sphere", [1], (a) => `sphere(${list(a)})`);
export const Plane = geometry<PlaneProps>("Plane", [1, 1], (a) => `box(${list(a)}, 0)`);
/** [radiusTop, radiusBottom, height], standing up (along y) as three.js has it. */
export const Cylinder = geometry<CylinderProps>("Cylinder", [1, 1, 1], (a) => `rotate(cone(${list([a[1], a[0], a[2]])}), -90, 0, 0)`);
export const Torus = geometry<TorusProps>("Torus", [1, 0.4], (a) => `torus(${list(a)})`);
export interface ConeProps { args?: [number, number] }
/** A cone: [radius, height], its point up (along y). */
export const Cone = geometry<ConeProps>("Cone", [0.5, 1], (a) => `rotate(cone(${list([a[0], 0, a[1]])}), -90, 0, 0)`);

export interface Text3DProps {
  /** the words, extruded */
  children?: string;
  text?: string;
  /** letter height in scene units (default 0.5) */
  size?: number;
  /** extrusion depth (default 0.2) */
  depth?: number;
  font?: string;
  weight?: number;
}

/** Solid 3D letters (Manifold's text3d), as a Mesh's geometry: `<Mesh><Text3D size={0.6}>SALE</Text3D>...</Mesh>`. */
export function Text3D(props: Text3DProps) {
  return node3d("Text3D", { ...props, shape: text3dShape(props) });
}
function text3dShape(p: Text3DProps): string {
  const text = p.text ?? (typeof p.children === "string" ? p.children : undefined);
  if (typeof text !== "string" || text === "") throw new TypeError("Text3D: give the words as its child or `text`");
  if (/["\\]/.test(text)) throw new TypeError(`Text3D: the words can't hold a quote or backslash, got ${JSON.stringify(text)}`);
  const size = p.size ?? 0.5, depth = p.depth ?? 0.2, weight = p.weight ?? 800, font = p.font ?? "Inter";
  for (const [k, v] of [["size", size], ["depth", depth]] as const) if (!(v > 0) || !Number.isFinite(v)) throw new RangeError(`Text3D: ${k} must be a positive number, got ${v}`);
  return `text3d("${text}", ${size}, ${depth}, "${font}", ${weight})`;
}

export interface MeshProps {
  /** what the mesh is called across frames: give one when meshes come and go, so each keeps its own motion */
  name?: string;
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  scale?: number | Vec3;
  /** it may pass through other parts on purpose (a blade into a body, a ghost through a wall): raea-motion's clipping
   * check leaves it out */
  allowOverlap?: boolean;
  /** set at an angle on purpose (a bowl knocked over, a mug on its side): `raea-motion scene lint` doesn't call it
   * tilted */
  posed?: boolean;
  /** a Manifold solid (manifold.box(...).minus(...)) or its expression; wins over the geometry child */
  shape?: string | Solid;
  /** a Bevy plugin moves it (a rigid body): position, rotation and scale set where it starts, then the plugin's */
  driven?: boolean;
  /** false: it casts no shadow (a window's glass, which a blended surface otherwise does; a sky card; a UI floating in
   * the scene). True by default */
  shadow?: boolean;
  /** declared animation, sampled by the engine each frame: x, y, z set the position, rotateX, rotateY, rotate the
   * rotation (radians), scale; `animate={{ y: track({ 0: -2, 12: 0 }) }}` or a move (@raea/motion animate) */
  animate?: Animate3d;
  children?: ReactNode;
}

/** A 3D node's declared animation: tracks by property, or a move. */
export type Animate3d = Record<string, TrackSpec | TrackSpec[] | Keys> | Move;

/** Under plain bun, what a 3D animation sets now, onto the node's props (raea-animate's patch_3d, the same rules). */
function apply3d<T extends object>(props: T, v: Record<string, number | string> | undefined): T {
  if (!v) return props;
  const out = { ...props } as Record<string, unknown>;
  const vec = (k: string, d: number): number[] => { const x = out[k]; return Array.isArray(x) ? [...x] : typeof x === "number" ? [x, x, x] : [d, d, d]; };
  if (v.x !== undefined || v.y !== undefined || v.z !== undefined) { const p = vec("position", 0); ["x", "y", "z"].forEach((k, i) => { if (v[k] !== undefined) p[i] = v[k] as number; }); out.position = p; }
  if (v.rotateX !== undefined || v.rotateY !== undefined || v.rotate !== undefined) { const r = vec("rotation", 0); ["rotateX", "rotateY", "rotate"].forEach((k, i) => { if (v[k] !== undefined) r[i] = v[k] as number; }); out.rotation = r; }
  if (v.scaleX !== undefined || v.scaleY !== undefined) { const sx = (v.scaleX as number) ?? 1, sy = (v.scaleY as number) ?? 1; out.scale = sx === sy ? sx : [sx, sy, 1]; }
  for (const [k, x] of Object.entries(v)) if (!["x", "y", "z", "rotateX", "rotateY", "rotate", "scaleX", "scaleY"].includes(k)) out[k] = x;
  return out as T;
}

function shapeFromChildren(children: ReactNode): string | undefined {
  let found: string | undefined;
  Children.forEach(children, (child) => {
    if (found !== undefined || !isValidElement(child)) return;
    if (child.type === Text3D) {
      found = text3dShape((child as ReactElement<Text3DProps>).props);
      return;
    }
    const toShape = GEOMETRY_SHAPES.get(child.type);
    if (toShape) found = toShape((child as ReactElement<{ args?: unknown }>).props.args);
  });
  return found;
}

/** A drawn object: a geometry child (or `shape`) and a material child, placed by position, rotation and scale. */
export function Mesh(props: MeshProps) {
  const anim = useAnimation3d(props.animate);
  const { position = [0, 0, 0], rotation = [0, 0, 0], scale = 1, shape } = apply3d(props, anim.values);
  checkVec("Mesh", "position", position);
  checkVec("Mesh", "rotation", rotation);
  if (typeof scale === "number") {
    if (!(scale > 0) || !Number.isFinite(scale)) throw new RangeError(`Mesh: scale must be a positive number, got ${scale}`);
  } else checkVec("Mesh", "scale", scale);
  const manifold = shape instanceof Solid ? shape.expr : shape ?? shapeFromChildren(props.children);
  const extra: Record<string, string> = {};
  if (manifold !== undefined) extra["data-shape"] = manifold as string;
  if (anim.attr) extra["data-raea-anim"] = anim.attr;
  return node3d("Mesh", { ...props, animate: undefined, shape: undefined, position, rotation, scale }, props.children, Object.keys(extra).length ? extra : undefined);
}

export interface MeshStandardMaterialProps {
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
  /** graph paper over the surface (greybox levels): lines every `cell` world units (fixed in the world, so they run on
   * across shapes), a heavier one every `major` cells (4), in `color`, and each face of a Box outlined in `edge`
   * ("" for no outlines). Its colours are the first frame's. */
  grid?: { cell: number; major?: number; color?: string; edge?: string };
  /** declared animation of its values (color, emissive, emissiveIntensity, opacity, metalness, roughness) */
  animate?: Animate3d;
}

const NAMED: Record<string, string> = {
  white: "#ffffff", black: "#000000", red: "#ff0000", green: "#008000", blue: "#0000ff", yellow: "#ffff00", orange: "#ffa500",
  purple: "#800080", gray: "#808080", grey: "#808080", pink: "#ffc0cb", cyan: "#00ffff", magenta: "#ff00ff", gold: "#ffd700", silver: "#c0c0c0",
};

/** A colour as the 3D renderer takes it (#rrggbb): hex, rgb()/rgba() or a common name; CSS variables throw. */
export function toHex(owner: string, k: string, v: unknown): string {
  if (typeof v === "string") {
    const c = v.trim().toLowerCase();
    if (/^#[0-9a-f]{6}$/.test(c)) return c;
    if (/^#[0-9a-f]{3}$/.test(c)) return "#" + [...c.slice(1)].map((x) => x + x).join("");
    const m = c.match(/^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)/);
    if (m) return "#" + m.slice(1, 4).map((x) => Math.max(0, Math.min(255, Math.round(Number(x)))).toString(16).padStart(2, "0")).join("");
    if (NAMED[c]) return NAMED[c];
  }
  throw new TypeError(`${owner}: ${k} must be a colour like "#ff8800" (the 3D renderer reads no CSS variables), got ${JSON.stringify(v)}`);
}

function colours<T extends Record<string, unknown>>(owner: string, props: T, keys: string[]): T {
  const out: Record<string, unknown> = { ...props };
  for (const k of keys) if (out[k] !== undefined) out[k] = toHex(owner, k, out[k]);
  return out as T;
}

/** a `grid` prop checked, its colours as hex (MeshStandardMaterial's and Model's) */
function checkGrid(owner: string, props: Record<string, unknown>): void {
  const grid = props.grid as MeshStandardMaterialProps["grid"] | undefined;
  if (grid === undefined) return;
  if (!(typeof grid === "object" && grid !== null && typeof grid.cell === "number" && grid.cell > 0)) throw new TypeError(`${owner}: grid needs a cell size above 0 ({ cell: 0.5 }), got ${JSON.stringify(grid)}`);
  if (grid.major !== undefined && !(grid.major >= 1)) throw new RangeError(`${owner}: grid.major must be at least 1, got ${grid.major}`);
  props.grid = { ...grid, color: grid.color === undefined ? undefined : toHex(owner, "grid.color", grid.color), edge: grid.edge === undefined || grid.edge === "" ? grid.edge : toHex(owner, "grid.edge", grid.edge) };
}

function checkMaterial(owner: string, props: Record<string, unknown>): void {
  checkGrid(owner, props);
  for (const k of ["metalness", "roughness", "opacity", "clearcoat", "clearcoatRoughness", "reflectance"]) {
    const v = props[k];
    if (v !== undefined && !(typeof v === "number" && v >= 0 && v <= 1)) throw new RangeError(`${owner}: ${k} must be in [0, 1], got ${v}`);
  }
  for (const k of ["map", "normalMap", "roughnessMap", "metalnessMap", "metallicRoughnessMap", "aoMap"]) {
    const v = props[k];
    if (v !== undefined && !(k === "map" && v === null) && !(typeof v === "string" && v !== "")) throw new TypeError(`${owner}: ${k} must be a picture's path (staticFile("floor_diff.jpg")) or a data: URL, got ${JSON.stringify(v)}`);
  }
  const r = props.repeat;
  if (r !== undefined && !(Array.isArray(r) && r.length === 2 && r.every((x) => typeof x === "number" && x > 0 && Number.isFinite(x)))) throw new RangeError(`${owner}: repeat must be [u, v], two positive numbers, got ${JSON.stringify(r)}`);
}

/** A physically based surface for the Mesh it's inside. */
export function MeshStandardMaterial(props: MeshStandardMaterialProps) {
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
}

/** The standard surface plus a clearcoat layer and reflectance. */
export function MeshPhysicalMaterial(props: MeshPhysicalMaterialProps) {
  const anim = useAnimation3d(props.animate);
  const p = apply3d({ ...props, animate: undefined } as Record<string, unknown>, anim.values);
  checkMaterial("MeshPhysicalMaterial", p);
  return node3d("MeshPhysicalMaterial", colours("MeshPhysicalMaterial", p, ["color", "emissive"]), undefined, anim.attr ? { "data-raea-anim": anim.attr } : undefined);
}

export interface MeshBasicMaterialProps {
  color?: string;
  opacity?: number;
  transparent?: boolean;
  map?: string;
  /** how many times the map repeats across each face, [u, v] */
  repeat?: [number, number];
}

/** A flat colour no light changes (unlit): signs, screens, backdrops. */
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
 * highlight and ink outline. Inside a Mesh, or inside a Model to toon its glTF materials. */
export function MeshToonMaterial(props: MeshToonMaterialProps) {
  const owner = "MeshToonMaterial";
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

export interface AmbientLightProps {
  /** matched by name across frames (a light that comes and goes, or one of several) */
  name?: string;
  intensity?: number;
  color?: string;
}

/** Light from everywhere at once. */
export function AmbientLight({ intensity = 1, color = "#ffffff", ...rest }: AmbientLightProps) {
  return node3d("AmbientLight", { ...rest, intensity, color: toHex("AmbientLight", "color", color) });
}

export interface DirectionalLightProps {
  /** matched by name across frames (a light that comes and goes, or one of several) */
  name?: string;
  position?: Vec3;
  /** where it shines towards (the origin by default) */
  target?: Vec3;
  intensity?: number;
  color?: string;
  /** shadows (on by default for a directional light) */
  castShadow?: boolean;
  /** its light shows in the air as shafts through a <FogVolume> (needs a <VolumetricFog>; casts shadows) */
  volumetric?: boolean;
  /** soft shadows (PCSS): true, or the sun's size as its shadows see it, in metres (0.5 by default; 2-10 for a
   * studio's big softbox): the larger, the softer, and a shadow sharpens where it touches what casts it, as a real one
   * does. A shadow map's texels show as grain in a wide penumbra: raise <Shadows mapSize> */
  softShadow?: boolean | number;
}

function checkLight(owner: string, p: { position?: Vec3; target?: Vec3; intensity?: number; color?: string; softShadow?: unknown }) {
  if (p.softShadow !== undefined && !(typeof p.softShadow === "boolean" || (typeof p.softShadow === "number" && p.softShadow >= 0 && Number.isFinite(p.softShadow)))) throw new RangeError(`${owner}: softShadow is true, false or a size in metres (0 or more), got ${JSON.stringify(p.softShadow)}`);
  if (p.position !== undefined) checkVec(owner, "position", p.position);
  if (p.target !== undefined) checkVec(owner, "target", p.target);
  if (p.intensity !== undefined && !(p.intensity >= 0 && Number.isFinite(p.intensity))) throw new RangeError(`${owner}: intensity must be 0 or more, got ${p.intensity}`);
  if (p.color !== undefined) toHex(owner, "color", p.color);
}

/** Parallel light (the sun) shining from `position` towards `target`. */
export function DirectionalLight({ intensity = 1, color = "#ffffff", ...rest }: DirectionalLightProps) {
  checkLight("DirectionalLight", { ...rest, intensity, color });
  return node3d("DirectionalLight", { ...rest, intensity, color: toHex("DirectionalLight", "color", color) });
}

export interface PointLightProps {
  /** matched by name across frames (a light that comes and goes, or one of several) */
  name?: string;
  position?: Vec3;
  /** 1 lights a surface 3 units away about as brightly as a DirectionalLight of 1 */
  intensity?: number;
  color?: string;
  /** how far it reaches (20 by default) */
  distance?: number;
  /** shadows (off by default; each costs six shadow maps) */
  castShadow?: boolean;
  /** soft shadows (PCSS) when it casts them: true, or the bulb's radius in metres (0.25 by default) */
  softShadow?: boolean | number;
  /** its light shows in the air through a <FogVolume> (needs a <VolumetricFog>; turns its shadows on, which a
   * point light pays six shadow maps for: prefer a spot) */
  volumetric?: boolean;
}

/** A bulb: light from a point, every way. */
export function PointLight({ intensity = 1, color = "#ffffff", ...rest }: PointLightProps) {
  checkLight("PointLight", { ...rest, intensity, color });
  return node3d("PointLight", { ...rest, intensity, color: toHex("PointLight", "color", color) });
}

export interface SpotLightProps extends PointLightProps {
  /** where it points (the origin by default) */
  target?: Vec3;
  /** the cone's half-angle in radians (π/3 by default), up to π/2 */
  angle?: number;
  /** 0..1: how soft the cone's edge is */
  penumbra?: number;
}

/** A cone of light from `position` towards `target`: a stage spot, a product hero light. */
export function SpotLight({ intensity = 1, color = "#ffffff", ...rest }: SpotLightProps) {
  checkLight("SpotLight", { ...rest, intensity, color });
  if (rest.angle !== undefined && !(rest.angle > 0 && rest.angle <= Math.PI / 2)) throw new RangeError(`SpotLight: angle is a half-angle in radians, in (0, π/2], got ${rest.angle}`);
  if (rest.penumbra !== undefined && !(rest.penumbra >= 0 && rest.penumbra <= 1)) throw new RangeError(`SpotLight: penumbra must be in [0, 1], got ${rest.penumbra}`);
  return node3d("SpotLight", { ...rest, intensity, color: toHex("SpotLight", "color", color) });
}

export interface PerspectiveCameraProps {
  position?: Vec3;
  /** the point it looks at (the origin by default); animate position and lookAt for camera moves */
  lookAt?: Vec3;
  /** vertical field of view in degrees */
  fov?: number;
  near?: number;
  far?: number;
  /** the distance in focus, in metres, with a <DepthOfField>; animate it to pull focus (by default, the lookAt
   * point's distance) */
  focus?: number;
  /** which way is up for the camera ([0, 1, 0] by default): tilt it to roll the camera (a solved camera rolls) */
  up?: Vec3;
  /** the frame is a window on a larger view: the view's `width` x `height` (frame pixels; `fov` is its vertical field)
   * and the window's top-left `x`, `y` in it. A crop of footage that keeps its lens's centre (<Matchmove> in 9:16 from
   * 16:9 footage) */
  view?: { width: number; height: number; x: number; y: number };
}

/** The viewpoint the scene is drawn from. */
export function PerspectiveCamera({ fov = 50, ...rest }: PerspectiveCameraProps) {
  if (rest.position !== undefined) checkVec("PerspectiveCamera", "position", rest.position);
  if (rest.lookAt !== undefined) checkVec("PerspectiveCamera", "lookAt", rest.lookAt);
  if (!(fov > 0 && fov < 180)) throw new RangeError(`PerspectiveCamera: fov must be between 0 and 180 degrees, got ${fov}`);
  if (rest.focus !== undefined && !(rest.focus > 0)) throw new RangeError(`PerspectiveCamera: focus must be a distance above 0, got ${rest.focus}`);
  if (rest.up !== undefined) checkVec("PerspectiveCamera", "up", rest.up);
  const v = rest.view;
  if (v !== undefined && !(v && v.width > 0 && v.height > 0 && Number.isFinite(v.x) && Number.isFinite(v.y))) throw new TypeError(`PerspectiveCamera: view is { width, height, x, y } in frame pixels, got ${JSON.stringify(v)}`);
  return node3d("PerspectiveCamera", { ...rest, fov });
}

export interface GroupProps {
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  scale?: number | Vec3;
  children?: ReactNode;
}

/** Moves, turns and scales everything inside it together (a product and its stand, a car and its wheels). */
/** Every <Model> inside re-covered: each of its glTF materials (all, or the one each names by `material`) given the
 * colour, texture or finish each gives, the rest of it kept (a run of cabinets painted, the doors and drawers with them;
 * `map: null` takes the wood's picture off). A model's own <MeshStandardMaterial> children go first. */
export function Refinish({ materials, children }: { materials: MeshStandardMaterialProps[]; children?: ReactNode }) {
  if (!Array.isArray(materials) || materials.length === 0) throw new TypeError("Refinish: materials is a list of MeshStandardMaterial props ([{ color: \"#8a9a7b\", map: null }])");
  const specs = materials.map((m) => {
    checkMaterial("Refinish", m as Record<string, unknown>);
    return colours("Refinish", { ...m }, ["color", "emissive"]);
  });
  return node3d("Refinish", { materials: specs }, children);
}

export function Group(props: GroupProps) {
  const { position = [0, 0, 0], rotation = [0, 0, 0], scale = 1 } = props;
  checkVec("Group", "position", position);
  checkVec("Group", "rotation", rotation);
  if (typeof scale !== "number") checkVec("Group", "scale", scale);
  return node3d("Group", { position, rotation, scale }, props.children);
}

export interface ModelProps {
  /** a glTF file in the media folder: staticFile("models/shoe.glb") */
  src: string;
  name?: string;
  /** fixed to another model's bone (`to`: that model's name; `bone`: the node's name in its glTF, "Head"): it follows
   * the bone's animated pose, and its position, rotation and scale are in the bone's space (a glTF's armature is
   * often scaled, so a hat may need scale 0.01) */
  attach?: { to: string; bone: string };
  /** a skinned model's joints placed where other models are, by name: `{ pelvis: "hero-pelvis", ... }` puts the joint
   * named pelvis wherever the model named hero-pelvis is each frame (its position and rotation; shrink it to hide it:
   * scale 0.001, not 0, which takes the skin with it), so
   * a ragdoll's bodies move one skin that bends at its joints rather than rigid pieces. The model's own placement
   * stays at the origin. */
  bones?: Record<string, string>;
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  scale?: number | Vec3;
  /** it may pass through other parts on purpose (a blade into a body, a ghost through a wall): raea-motion's clipping
   * check leaves it out */
  allowOverlap?: boolean;
  /** set at an angle on purpose (a bowl knocked over, a mug on its side): `raea-motion scene lint` doesn't call it
   * tilted */
  posed?: boolean;
  /** a clip to play: its name in the file, or its index */
  animation?: string | number;
  /** graph paper over the model's surfaces, as MeshStandardMaterial's `grid`, but drawn in the model's own texture
   * coordinates (taken as metres), so the lines stay on the surface as it moves; no outlines. `toon`: its light in
   * three flat bands (toon shading) */
  grid?: { cell: number; major?: number; color?: string; toon?: boolean };
  /** seconds into the clip (by default the Sequence's time: frame / fps); the clip loops unless loop={false} */
  time?: number;
  loop?: boolean;
  /** which of the file's scenes (0 by default) */
  scene?: number;
  /** how the model itself is drawn (the rest of the scene is unchanged): "solid" (its own surfaces, the default),
   * "pixel" (pixel art: a cell per `pixelSize` frame pixels, outlined in `edgeColor`) or "site" (raea.io's grain:
   * the cells' light ordered-dithered into the site's six greys). It may change from frame to frame. */
  look?: "solid" | "pixel" | "site";
  /** its surfaces without light: shape and colour alone, like a drawing (the site look keeps the light: its grain
   * is made of it) */
  flat?: boolean;
  /** a cell's size in frame pixels for the pixel and site looks (6 by default) */
  pixelSize?: number;
  /** the pixel look's one-cell outline, a hex colour ("#000000" by default), or "none" */
  edgeColor?: string;
  /** the site look's greys: "dark" (light ink on black, the default) or "light" */
  theme?: "dark" | "light";
  /** the site look's ink: lift + light * gain, before the dither (0.28 and 0.9 by default; on the light greys light is inverted, 0.35 and 0.75) */
  lift?: number;
  gain?: number;
  /** <MeshToonMaterial> children: toon surfaces over the model's own materials (all of them, or one each by name) */
  children?: ReactNode;
}

/** A glTF model (.glb/.gltf) with its materials, and its animation clip posed at the frame's time. */
export function Model(props: ModelProps) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (typeof props.src !== "string" || !/\.(glb|gltf)$/i.test(props.src)) throw new TypeError(`Model: src must be a .glb or .gltf file (staticFile("x.glb")), got ${JSON.stringify(props.src)}`);
  for (const k of ["position", "rotation"] as const) if (props[k] !== undefined) checkVec("Model", k, props[k]);
  const time = props.time ?? frame / fps;
  if (!Number.isFinite(time)) throw new RangeError(`Model: time must be a finite number of seconds, got ${time}`);
  if (props.look !== undefined && !["solid", "pixel", "site"].includes(props.look)) throw new TypeError(`Model: look must be "solid", "pixel" or "site", got ${JSON.stringify(props.look)}`);
  if (props.pixelSize !== undefined && !(props.pixelSize >= 1)) throw new RangeError(`Model: pixelSize must be at least 1, got ${props.pixelSize}`);
  if (props.bones !== undefined && (typeof props.bones !== "object" || Object.values(props.bones).some((v) => typeof v !== "string"))) throw new TypeError(`Model: bones maps each joint's name to a model's name, got ${JSON.stringify(props.bones)}`);
  const p: Record<string, unknown> = { ...props, time };
  checkGrid("Model", p);
  return node3d("Model", p, props.children);
}

export interface ImagePlaneProps {
  /** a picture in the media folder: staticFile("poster.png") */
  src: string;
  name?: string;
  /** width in scene units (1 by default); height follows the picture unless given */
  width?: number;
  height?: number;
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  scale?: number | Vec3;
}

/** A picture on a flat card in the scene, unlit (a poster, a screen, a sprite). */
export function ImagePlane(props: ImagePlaneProps) {
  if (typeof props.src !== "string" || props.src === "") throw new TypeError("ImagePlane: src must be a picture (staticFile(\"x.png\"))");
  for (const k of ["position", "rotation"] as const) if (props[k] !== undefined) checkVec("ImagePlane", k, props[k]);
  return node3d("ImagePlane", props as unknown as Record<string, unknown>);
}

export interface DecalProps {
  /** the picture pressed onto the surface: staticFile("label.png"); its clear parts show the surface through */
  src: string;
  name?: string;
  normalMap?: string;
  /** width by height in metres (a number: a square) */
  size?: number | [number, number];
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
  roughness?: number;
  metalness?: number;
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
 * so it moves and fades with a render's clock and a game's play alike. */
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
}

/** Glow around bright parts (emissive materials, strong highlights). Put it inside the Canvas3D. */
export function Bloom(props: BloomProps) {
  if (props.intensity !== undefined && !(props.intensity >= 0)) throw new RangeError(`Bloom: intensity must be 0 or more, got ${props.intensity}`);
  return node3d("Bloom", props as Record<string, unknown>);
}

export interface FogProps {
  /** hex; match the background so far things fade into it */
  color?: string;
  /** linear fog from `near` to `far` (5 and 20 by default) */
  near?: number;
  far?: number;
  /** or exponential fog of this density (0.05-0.3), instead of near and far */
  density?: number;
}

/** Distance fog: far things fade into `color`. Put it inside the Canvas3D. */
export function Fog(props: FogProps) {
  return node3d("Fog", colours("Fog", props as Record<string, unknown>, ["color"]));
}

export interface ToneMappingProps {
  /** how bright light maps to the screen: "tony" (Bevy's default), "aces" (filmic, contrasty), "agx", "neutral",
   * "reinhard", "none" */
  mode?: "tony" | "aces" | "agx" | "neutral" | "reinhard" | "none";
  /** stops brighter (+) or darker (-) than the default exposure */
  exposure?: number;
}

/** The camera's tone mapping and exposure. Put it inside the Canvas3D. */
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
  shadows?: GradeSection;
  midtones?: GradeSection;
  highlights?: GradeSection;
  /** the corners darkened: 0 none, 1 black; or {intensity, radius (0.75), smoothness (5), color} */
  vignette?: number | { intensity?: number; radius?: number; smoothness?: number; color?: string };
  /** a lens's colour fringes at edges, as a fraction of the frame (0.005 subtle, 0.02 strong) */
  aberration?: number;
}

/** The camera's colour grade (lift, gamma, gain and saturation per range of light, before tone mapping), a lens
 * vignette and colour fringes: one grade over every scene, so a film looks of a piece. Put it inside the Canvas3D. */
export function Grade(props: GradeProps) {
  const v = props.vignette;
  return node3d("Grade", { ...props, vignette: typeof v === "object" && v.color ? { ...v, color: colours("Grade", { color: v.color }, ["color"]).color } : v } as Record<string, unknown>);
}

export interface EnvironmentProps {
  /** an equirectangular panorama in the media folder: a Radiance .hdr (Poly Haven's), an OpenEXR .exr, or an 8-bit
   * .png/.jpg. Change it between frames to cut from one place to another */
  src: string;
  /** how strongly it lights the scene (1 by default: a sky about as bright as an AmbientLight of 2.5 overall); animate
   * it, or set 0 to switch it off for a shot */
  intensity?: number;
  /** a turn about the vertical axis, in radians, for both the light and the sky: puts the sun where the shot wants it */
  rotation?: number;
  /** draw the panorama as the sky behind everything (off by default: light only) */
  background?: boolean;
  /** the sky's brightness, when it should differ from the light's (`intensity` by default) */
  backgroundIntensity?: number;
}

/** Light from all around, out of a panorama: soft light from the sky and reflections of it in every glossy or metal
 * surface (image-based lighting), and with `background` the panorama as the sky. Put it inside the Canvas3D, once a
 * frame; its src, intensity, rotation and background may change from frame to frame (a cut from a hall to a street). */
export function Environment(props: EnvironmentProps) {
  if (typeof props.src !== "string" || props.src === "") throw new TypeError(`Environment: src must be a panorama's path, staticFile("sky.hdr"), got ${JSON.stringify(props.src)}`);
  if (!/\.(hdr|exr|png|jpe?g)$/i.test(props.src.split("?")[0])) throw new TypeError(`Environment: src must be an equirectangular .hdr, .exr, .png or .jpg, got ${JSON.stringify(props.src)}`);
  for (const k of ["intensity", "backgroundIntensity"] as const) {
    const v = props[k];
    if (v !== undefined && !(typeof v === "number" && v >= 0 && Number.isFinite(v))) throw new RangeError(`Environment: ${k} must be 0 or more, got ${v}`);
  }
  if (props.rotation !== undefined && !(typeof props.rotation === "number" && Number.isFinite(props.rotation))) throw new RangeError(`Environment: rotation must be a number of radians, got ${props.rotation}`);
  return node3d("Environment", props as unknown as Record<string, unknown>);
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
 * Put it inside the Canvas3D, once; the first frame's holds for the piece. */
export function AmbientOcclusion(props: AmbientOcclusionProps) {
  if (props.quality !== undefined && !["low", "medium", "high", "ultra"].includes(props.quality)) throw new RangeError(`AmbientOcclusion: quality must be "low", "medium", "high" or "ultra", got ${JSON.stringify(props.quality)}`);
  if (props.thickness !== undefined && !(typeof props.thickness === "number" && props.thickness > 0 && Number.isFinite(props.thickness))) throw new RangeError(`AmbientOcclusion: thickness must be a positive number of metres, got ${props.thickness}`);
  return node3d("AmbientOcclusion", props as unknown as Record<string, unknown>);
}

export interface AntiAliasingProps {
  /** "msaa" (4x multisampling, the default), "smaa" (morphological, high preset: the default with AmbientOcclusion,
   * which can't multisample), "fxaa" (cheapest, softest) or "none". Each draws a frame from that frame alone, so a
   * render equals its play (temporal anti-aliasing isn't offered: it needs the frames before) */
  mode: "msaa" | "smaa" | "fxaa" | "none";
}

/** How the 3D's edges are smoothed. Put it inside the Canvas3D, once; the first frame's holds for the piece. */
export function AntiAliasing(props: AntiAliasingProps) {
  if (!["msaa", "smaa", "fxaa", "none"].includes(props.mode)) throw new RangeError(`AntiAliasing: mode must be "msaa", "smaa", "fxaa" or "none", got ${JSON.stringify(props.mode)}`);
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
 * Canvas3D, once; the first frame's holds for the piece. */
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
}

/** Volumetric fog on the camera: light in the air, drawn inside every <FogVolume>, with shafts from the lights marked
 * `volumetric`. Put it inside the Canvas3D, once; without it fog volumes draw nothing. */
export function VolumetricFog(props: VolumetricFogProps) {
  if (props.steps !== undefined && !(Number.isInteger(props.steps) && props.steps >= 1 && props.steps <= 256)) throw new RangeError(`VolumetricFog: steps must be a whole number from 1 to 256, got ${props.steps}`);
  if (props.dither !== undefined && typeof props.dither !== "boolean") throw new TypeError(`VolumetricFog: dither must be true or false, got ${JSON.stringify(props.dither)}`);
  if (props.resolution !== undefined && !(props.resolution > 0 && props.resolution <= 1 && [1, 2, 3, 4].some((k) => Math.abs(props.resolution! * k - 1) < 1e-6))) throw new RangeError(`VolumetricFog: resolution must be 1, 0.5, 1/3 or 0.25, got ${props.resolution}`);
  if (props.ambient !== undefined && !(props.ambient >= 0)) throw new RangeError(`VolumetricFog: ambient must be 0 or more, got ${props.ambient}`);
  if (props.mode !== undefined && !["march", "froxel"].includes(props.mode)) throw new RangeError(`VolumetricFog: mode must be "march" or "froxel", got ${JSON.stringify(props.mode)}`);
  if (props.slices !== undefined && !(Number.isInteger(props.slices) && props.slices >= 32 && props.slices <= 256)) throw new RangeError(`VolumetricFog: slices must be a whole number from 32 to 256, got ${props.slices}`);
  if (props.tile !== undefined && ![4, 8, 16].includes(props.tile)) throw new RangeError(`VolumetricFog: tile must be 4, 8 or 16, got ${props.tile}`);
  if (props.samples !== undefined && !(Number.isInteger(props.samples) && props.samples >= 1 && props.samples <= 8)) throw new RangeError(`VolumetricFog: samples must be a whole number from 1 to 8, got ${props.samples}`);
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
}

/** A box of air that light shows in: dust in a sunbeam, smoke, a haze of blood. Needs a <VolumetricFog> in the
 * Canvas3D; lights marked `volumetric` throw shafts through it, and every light's glow fills it softly. A part, so it
 * keeps its name across frames: animate its density, colour and placement. */
export function FogVolume(props: FogVolumeProps) {
  for (const k of ["position", "rotation"] as const) if (props[k] !== undefined) checkVec("FogVolume", k, props[k]);
  if (props.density !== undefined && !(props.density >= 0 && Number.isFinite(props.density))) throw new RangeError(`FogVolume: density must be 0 or more, got ${props.density}`);
  if (props.anisotropy !== undefined && !(props.anisotropy > -1 && props.anisotropy < 1)) throw new RangeError(`FogVolume: anisotropy must be between -1 and 1, got ${props.anisotropy}`);
  if (props.falloff !== undefined && !(props.falloff >= 0 && Number.isFinite(props.falloff))) throw new RangeError(`FogVolume: falloff must be 0 or more, got ${props.falloff}`);
  return node3d("FogVolume", colours("FogVolume", props as Record<string, unknown>, ["color"]));
}

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
 * yourself. */
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
}

/** A lens's depth of field: what's nearer or farther than the camera's `focus` blurs. Put it inside the Canvas3D;
 * pull focus by animating the camera's `focus`. */
export function DepthOfField(props: DepthOfFieldProps) {
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
}

/** Ink lines where the scene's depth steps or its surfaces turn, drawn before bloom and tone mapping; under a <Look>
 * a cell wide (Return of the Obra Dinn's line). Put it inside the Canvas3D. */
export function Outline(props: OutlineProps) {
  if (props.depth !== undefined && !(props.depth > 0)) throw new RangeError(`Outline: depth must be above 0, got ${props.depth}`);
  if (props.normal !== undefined && !(props.normal >= 0 && props.normal <= 2)) throw new RangeError(`Outline: normal must be between 0 and 2, got ${props.normal}`);
  if (props.width !== undefined && !(props.width >= 1 && props.width <= 32)) throw new RangeError(`Outline: width must be from 1 to 32 pixels, got ${props.width}`);
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
  k2?: number;
  /** by default just enough for a barrel's corners to stay inside the picture (1 for pincushion) */
  zoom?: number;
}

function lensNode(owner: string, props: Record<string, unknown>) {
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
  animate?: Animate3d;
}

/** Motion blur from each part's and the camera's motion over the frame before (Bevy's MotionBlur): with it, each frame
 * is drawn after the one before it, so a frame's blur is the same in a render, a seek and play. A frame without it has
 * none. Put it inside the Canvas3D. */
export function MotionBlur(props: MotionBlurProps) {
  const owner = "MotionBlur";
  const anim = useAnimation3d(props.animate);
  const p = apply3d({ ...props, animate: undefined } as Record<string, unknown>, anim.values);
  for (const [k, lo, hi] of [["shutterAngle", 0, 4], ["samples", 1, 32]] as const) {
    const v = p[k];
    if (v !== undefined && !(typeof v === "number" && v >= lo && v <= hi)) throw new RangeError(`${owner}: ${k} must be in [${lo}, ${hi}], got ${v}`);
  }
  return node3d(owner, p, undefined, anim.attr ? { "data-raea-anim": anim.attr } : undefined);
}

export interface LookProps {
  /** "site" (raea.io's grain: each cell's light ordered-dithered into the site's six greys), "pixel" (each cell
   * its colour) or "ink" (two colours, as Return of the Obra Dinn: each cell's light 8x8-Bayer-dithered into `paper`
   * or `ink`, with a line of ink where the light steps by more than `line`), "paint" (an editorial illustration: every
   * surface lit in `bands` flat tones by the suns, hard cast shadows as shapes, ragged edges, paper grain, a flat
   * sky) or "cartoon" (paint's preset for a cartoon: a flat fill and one shade, a thick even `outline` that boils) */
  mode?: "site" | "pixel" | "ink" | "paint" | "cartoon";
  /** a cell's size in the frame's pixels (6 by default) */
  pixelSize?: number;
  /** the site's dark greys (the default) or its light ones */
  theme?: "dark" | "light";
  /** how much of each cell's hue its grey keeps, 0 (greys only) to 1 (the default) */
  color?: number;
  /** the site look's ink: light = lift + light x gain (0 and 1 by default, so black stays black) */
  lift?: number;
  gain?: number;
  /** the ink look's ink and paper, hex colours (a warm black and an old paper by default) */
  ink?: string;
  paper?: string;
  /** the ink look's line: how big a step in a cell's light (0 to 1) from a neighbour's draws ink there (0, none, by
   * default; about 0.15 rings figures and edges without picking up texture) */
  line?: number;
  /** the ink look's split: light below it is ink, above it paper (0.5 by default) */
  threshold?: number;
  /** how wide a band of light around `threshold` is dithered (1, the whole range, by default; 0.1 gives hard ink and
   * paper with a thin dithered edge between, a silhouette's look) */
  band?: number;
  /** paint, cartoon: how many flat tones the suns' light makes on a surface, 1-8 (3 for paint: shadow, mid and lit;
   * 2 for cartoon: shade and fill); keyed per frame */
  bands?: number;
  /** paint, cartoon: the darkest tone (turned from the sun, or in a cast shadow), as a colour the surface's own is
   * multiplied by (a cool grey-blue by default; a warm blush for a cartoon) */
  shadow?: string;
  /** paint: the tone between shadow and full light, a colour the surface's own is multiplied by (a warm pink-grey) */
  mid?: string;
  /** paint, cartoon: paper grain, 0 (none) to 1 (0.3 for paint, 0.06 for cartoon); a fixed pattern; keyed */
  grain?: number;
  /** paint, cartoon: how far edges (silhouettes, shadows, lines) wander, in pixels (2 for paint, 1.5 for cartoon; 0
   * for clean edges); keyed */
  rag?: number;
  /** how wide the wander's waves are, in pixels (24) */
  ragScale?: number;
  /** the wander's seed (0); the same seed paints the same edges */
  seed?: number;
  /** a new wander every this many frames: the line boils (0: a fixed wander, paint's default; 2, on twos, cartoon's) */
  boil?: number;
  /** where nothing's drawn: one flat colour, or [top, bottom] for a two-tone gradient (the Canvas3D's background
   * otherwise) */
  sky?: string | [string, string];
  /** colours every pixel snaps to (nearest by Oklab, lightness counted double), up to 16; none by default */
  palette?: string[];
  /** cartoon: the outline's colour (a dark brown, "#2b1d14"; "none" for no line) and width in pixels (6) */
  outline?: string;
  outlineWidth?: number;
}

const PAINT = { bands: 3, grain: 0.3, rag: 2, boil: 0 };
const CARTOON = { bands: 2, grain: 0.06, rag: 1.5, boil: 2, shadow: "#f2c7b8" };

/** The whole frame drawn as a model's look draws its model: every part of the scene in the one grain, its bloom,
 * fog and blur inside it. Put it inside the Canvas3D. */
export function Look(props: LookProps) {
  const frame = useCurrentFrame();
  if (props.mode === "paint" || props.mode === "cartoon") return paintLook(props, frame);
  if (props.mode !== undefined && props.mode !== "site" && props.mode !== "pixel" && props.mode !== "ink") throw new RangeError(`Look: mode is "site", "pixel", "ink", "paint" or "cartoon", got ${props.mode}`);
  if (props.pixelSize !== undefined && !(props.pixelSize >= 1)) throw new RangeError(`Look: pixelSize must be 1 or more, got ${props.pixelSize}`);
  if (props.band !== undefined && !(props.band > 0)) throw new RangeError(`Look: band must be above 0, got ${props.band}`);
  if (props.line !== undefined && !(props.line >= 0 && props.line <= 1)) throw new RangeError(`Look: line must be between 0 and 1, got ${props.line}`);
  if (props.color !== undefined && !(props.color >= 0 && props.color <= 1)) throw new RangeError(`Look: color must be between 0 and 1, got ${props.color}`);
  return node3d("Look", props as Record<string, unknown>);
}

/** <Look mode="paint" | "cartoon">: the look's element (mode "paint" for the engine, the boil folded into the seed so
 * the engine never reads a clock) and, for a cartoon, its outline. */
function paintLook(props: LookProps, frame: number) {
  const cartoon = props.mode === "cartoon";
  const d: Record<string, unknown> = cartoon ? CARTOON : PAINT;
  const p = { ...d, ...props } as LookProps & typeof PAINT;
  if (!(Number.isInteger(p.bands) && p.bands >= 1 && p.bands <= 8)) throw new RangeError(`Look: bands must be a whole number from 1 to 8, got ${p.bands}`);
  for (const k of ["grain", "rag", "ragScale", "seed", "boil", "outlineWidth"] as const) {
    const v = p[k];
    if (v !== undefined && !(typeof v === "number" && Number.isFinite(v) && v >= 0)) throw new RangeError(`Look: ${k} must be a number 0 or more, got ${JSON.stringify(v)}`);
  }
  if (p.grain !== undefined && p.grain > 1) throw new RangeError(`Look: grain must be from 0 to 1, got ${p.grain}`);
  if (p.ragScale !== undefined && p.ragScale < 1) throw new RangeError(`Look: ragScale must be 1 pixel or more, got ${p.ragScale}`);
  if (p.pixelSize !== undefined && !(p.pixelSize >= 1)) throw new RangeError(`Look: pixelSize must be 1 or more, got ${p.pixelSize}`);
  if (p.palette !== undefined && !(Array.isArray(p.palette) && p.palette.length >= 1 && p.palette.length <= 16)) throw new RangeError(`Look: palette must be 1 to 16 colours, got ${JSON.stringify(p.palette)}`);
  if (p.sky !== undefined && Array.isArray(p.sky) && p.sky.length !== 2) throw new RangeError(`Look: sky is one colour or [top, bottom], got ${JSON.stringify(p.sky)}`);
  const boil = p.boil ?? 0;
  const seed = Math.round(p.seed ?? 0) + (boil > 0 ? Math.floor(frame / boil) : 0);
  const look: Record<string, unknown> = {
    mode: "paint", pixelSize: p.pixelSize, bands: p.bands, grain: p.grain, rag: p.rag, ragScale: p.ragScale, seed,
    shadow: p.shadow === undefined ? undefined : toHex("Look", "shadow", p.shadow),
    mid: p.mid === undefined ? undefined : toHex("Look", "mid", p.mid),
    sky: p.sky === undefined ? undefined : Array.isArray(p.sky) ? p.sky.map((c) => toHex("Look", "sky", c)) : toHex("Look", "sky", p.sky),
    palette: p.palette?.map((c) => toHex("Look", "palette", c)),
  };
  if (!cartoon || p.outline === "none") return node3d("Look", look);
  const line = toHex("Look", "outline", p.outline ?? "#2b1d14");
  return (
    <>
      {node3d("Look", look)}
      {node3d("Outline", { color: line, width: p.outlineWidth ?? 6, depth: 0.04, normal: 0 })}
    </>
  );
}

export interface SceneProps {
  /** the world whose rows are drawn; its systems run up to the current frame */
  world: World;
}

/** A world drawn at the current frame: its `mesh` rows (shape, x y z, rot_x rot_y rot_z in radians, scale, color,
 * rough, metal), `camera` (x y z, look_x look_y look_z, fov), `light` (x y z, intensity) and `ambient` (intensity)
 * rows as the 3D elements above. Inside a Canvas3D. */
export function Scene({ world }: SceneProps) {
  const frame = useCurrentFrame();
  const rows = world.at(Math.max(0, Math.floor(frame))).rows();
  const n = (r: Record<string, unknown>, k: string, d: number) => (typeof r[k] === "number" ? (r[k] as number) : d);
  return (
    <>
      {rows.map((r) => {
        const key = `${r.kind}-${r.id}`;
        switch (r.kind) {
          case "camera":
            return <PerspectiveCamera key={key} position={[n(r, "x", 0), n(r, "y", 0), n(r, "z", 5)]} fov={n(r, "fov", 50)} />;
          case "light":
            return <DirectionalLight key={key} position={[n(r, "x", 0), n(r, "y", 1), n(r, "z", 0)]} intensity={n(r, "intensity", 1)} />;
          case "ambient":
            return <AmbientLight key={key} intensity={n(r, "intensity", 1)} />;
          case "mesh":
            if (typeof r.shape !== "string") return null;
            return (
              <Mesh key={key} shape={r.shape} position={[n(r, "x", 0), n(r, "y", 0), n(r, "z", 0)]} rotation={[n(r, "rot_x", 0), n(r, "rot_y", 0), n(r, "rot_z", 0)]} scale={n(r, "scale", 1)}>
                <MeshStandardMaterial color={typeof r.color === "string" ? r.color : "#cccccc"} roughness={n(r, "rough", 0.5)} metalness={n(r, "metal", 0)} />
              </Mesh>
            );
          default:
            return null;
        }
      })}
    </>
  );
}

/** A world's entities drawn by the engine (no React element per entity): every entity with a Mesh component ({shape:
 * a Manifold expression or a Solid's expr, color, roughness, metalness}) and a Transform ({x, y, z, rx, ry, rz in
 * radians, scale}) is a mesh, placed in world space (a child's Transform is in its parent's). Give it the world at the
 * frame: `<Entities world={game.at(useCurrentFrame())} />`. Needs the engine's tables. */
export function Entities({ world }: { world: { scene(): unknown[] } }) {
  return node3d("Entities", { scene: world.scene() });
}

export interface BevyComponentProps {
  /** the component's reflected type path, as Bevy's TypeRegistry names it ("avian3d::dynamics::rigid_body::RigidBody") */
  type: string;
  /** its value as Bevy's reflect serde reads it: a struct an object (fields left out take the type's default, when it
   * has one), a newtype its inner value, an enum "Unit" or { Variant: ... }, a Vec3 [x, y, z] */
  value?: unknown;
  /** the frame it's added on (0, the start, by default); a plugin steps after it from that frame */
  from?: number;
}

/** A Bevy component on the Mesh it's inside, applied through the TypeRegistry: how a compiled-in Bevy plugin is driven
 * (sdk/motion/docs/bevy-plugins.md). Agents use the generated, typed wrappers (one per component), not this directly. */
export function BevyComponent(props: BevyComponentProps) {
  if (typeof props.type !== "string" || !props.type.includes("::")) throw new TypeError(`BevyComponent: type is a full type path (crate::module::Type), got ${JSON.stringify(props.type)}`);
  if (props.from !== undefined && !(Number.isInteger(props.from) && props.from >= 0)) throw new RangeError(`BevyComponent: from is a frame (0 or more), got ${props.from}`);
  return node3d("BevyComponent", { type: props.type, value: props.value ?? null, ...(props.from ? { from: props.from } : {}) });
}

/** A Bevy resource (a plugin's settings: gravity), applied through the TypeRegistry. Inside the Canvas3D. */
export function BevyResource(props: { type: string; value: unknown }) {
  if (typeof props.type !== "string" || !props.type.includes("::")) throw new TypeError(`BevyResource: type is a full type path, got ${JSON.stringify(props.type)}`);
  return node3d("BevyResource", { type: props.type, value: props.value ?? null });
}
