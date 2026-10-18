// @raea/motion/3d: 3D in the same tree (spec: "3D in the same tree"). Every component renders as an element that
// carries its name (data-3d) and props (data-props, JSON) for Bevy to draw; a Mesh also carries its Manifold
// expression (data-shape). Engine-safe: this module imports nothing beyond React.
import { Children, isValidElement, type ReactElement, type ReactNode } from "react";
import { useCurrentFrame, useVideoConfig } from "./clock";
import type { World } from "./ecs";
import { Solid } from "./solid";

export { manifold, outline, Solid, type Bounds, type Polygons, type Section } from "./solid";

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

/** The 3D scene's root; everything 3D goes inside it. */
export function Canvas3D(props: { children?: ReactNode; [prop: string]: unknown }) {
  return node3d("Canvas3D", props, props.children);
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
  /** a Manifold solid (manifold.box(...).minus(...)) or its expression; wins over the geometry child */
  shape?: string | Solid;
  children?: ReactNode;
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
  const { position = [0, 0, 0], rotation = [0, 0, 0], scale = 1, shape } = props;
  checkVec("Mesh", "position", position);
  checkVec("Mesh", "rotation", rotation);
  if (typeof scale === "number") {
    if (!(scale > 0) || !Number.isFinite(scale)) throw new RangeError(`Mesh: scale must be a positive number, got ${scale}`);
  } else checkVec("Mesh", "scale", scale);
  const manifold = shape instanceof Solid ? shape.expr : shape ?? shapeFromChildren(props.children);
  return node3d(
    "Mesh",
    { ...props, shape: undefined, position, rotation, scale },
    props.children,
    manifold === undefined ? undefined : { "data-shape": manifold },
  );
}

export interface MeshStandardMaterialProps {
  /** a hex colour (#rrggbb); animate it freely, each frame's colour is drawn */
  color?: string;
  /** the colour it glows (hex); with <Bloom> a strong glow blooms */
  emissive?: string;
  /** how strongly it glows (1 by default; 2-10 for a light source with bloom) */
  emissiveIntensity?: number;
  metalness?: number;
  roughness?: number;
  /** 0..1; below 1 (or with `transparent`) the surface is blended */
  opacity?: number;
  transparent?: boolean;
  /** a picture for the base colour: staticFile("label.png") */
  map?: string;
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

function checkMaterial(owner: string, props: Record<string, unknown>): void {
  for (const k of ["metalness", "roughness", "opacity", "clearcoat", "clearcoatRoughness", "reflectance"]) {
    const v = props[k];
    if (v !== undefined && !(typeof v === "number" && v >= 0 && v <= 1)) throw new RangeError(`${owner}: ${k} must be in [0, 1], got ${v}`);
  }
}

/** A physically based surface for the Mesh it's inside. */
export function MeshStandardMaterial(props: MeshStandardMaterialProps) {
  checkMaterial("MeshStandardMaterial", props as Record<string, unknown>);
  return node3d("MeshStandardMaterial", colours("MeshStandardMaterial", props as Record<string, unknown>, ["color", "emissive"]));
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
  checkMaterial("MeshPhysicalMaterial", props as Record<string, unknown>);
  return node3d("MeshPhysicalMaterial", colours("MeshPhysicalMaterial", props as Record<string, unknown>, ["color", "emissive"]));
}

export interface MeshBasicMaterialProps {
  color?: string;
  opacity?: number;
  transparent?: boolean;
  map?: string;
}

/** A flat colour no light changes (unlit): signs, screens, backdrops. */
export function MeshBasicMaterial(props: MeshBasicMaterialProps) {
  checkMaterial("MeshBasicMaterial", props as Record<string, unknown>);
  return node3d("MeshBasicMaterial", colours("MeshBasicMaterial", props as Record<string, unknown>, ["color", "emissive"]));
}

export interface AmbientLightProps {
  intensity?: number;
  color?: string;
}

/** Light from everywhere at once. */
export function AmbientLight({ intensity = 1, color = "#ffffff", ...rest }: AmbientLightProps) {
  return node3d("AmbientLight", { ...rest, intensity, color: toHex("AmbientLight", "color", color) });
}

export interface DirectionalLightProps {
  position?: Vec3;
  /** where it shines towards (the origin by default) */
  target?: Vec3;
  intensity?: number;
  color?: string;
  /** shadows (on by default for a directional light) */
  castShadow?: boolean;
}

function checkLight(owner: string, p: { position?: Vec3; target?: Vec3; intensity?: number; color?: string }) {
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
  position?: Vec3;
  /** 1 lights a surface 3 units away about as brightly as a DirectionalLight of 1 */
  intensity?: number;
  color?: string;
  /** how far it reaches (20 by default) */
  distance?: number;
  /** shadows (off by default; each costs six shadow maps) */
  castShadow?: boolean;
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
}

/** The viewpoint the scene is drawn from. */
export function PerspectiveCamera({ fov = 50, ...rest }: PerspectiveCameraProps) {
  if (rest.position !== undefined) checkVec("PerspectiveCamera", "position", rest.position);
  if (rest.lookAt !== undefined) checkVec("PerspectiveCamera", "lookAt", rest.lookAt);
  if (!(fov > 0 && fov < 180)) throw new RangeError(`PerspectiveCamera: fov must be between 0 and 180 degrees, got ${fov}`);
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
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  scale?: number | Vec3;
  /** a clip to play: its name in the file, or its index */
  animation?: string | number;
  /** seconds into the clip (by default the Sequence's time: frame / fps); the clip loops unless loop={false} */
  time?: number;
  loop?: boolean;
  /** which of the file's scenes (0 by default) */
  scene?: number;
}

/** A glTF model (.glb/.gltf) with its materials, and its animation clip posed at the frame's time. */
export function Model(props: ModelProps) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (typeof props.src !== "string" || !/\.(glb|gltf)$/i.test(props.src)) throw new TypeError(`Model: src must be a .glb or .gltf file (staticFile("x.glb")), got ${JSON.stringify(props.src)}`);
  for (const k of ["position", "rotation"] as const) if (props[k] !== undefined) checkVec("Model", k, props[k]);
  const time = props.time ?? frame / fps;
  if (!Number.isFinite(time)) throw new RangeError(`Model: time must be a finite number of seconds, got ${time}`);
  return node3d("Model", { ...props, time });
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
