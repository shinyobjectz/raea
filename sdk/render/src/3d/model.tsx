// Bevy's parts, groups and <Model>: glTF scenes, clips and their events.
import { createContext, useId, type ReactNode } from "react";
import { useCurrentFrame, useVideoConfig } from "@raea/app/clock";
import { checkGrid } from "../overlays";
import { modelScreens, type ModelScreens } from "../screen";
import { pageLayers } from "../layers";
import { Place3d, useCamera3d, usePlace3d } from "../events3d";
import { MeshStandardMaterialProps, PerspectiveCameraProps, Vec3, checkMaterial, checkPass, checkVec, colours, node3d, samplerSpec, type SamplerSpec } from "./all";
import { PlannedLodOf, detailNode, usePlannedLod, type Detail, type PlannedLod } from "./detail";

/** What a light or camera holds: Bevy's own reflected components on it (<BevyComponent>, or a generated wrapper such
 * as bevy_light's ShadowFilteringMethod or bevy_pbr's ScreenSpaceReflections), applied through Bevy's TypeRegistry. */
export interface BevyParts {
  /** Bevy components on it: <BevyComponent>, or a generated wrapper */
  children?: ReactNode;
  /** a camera: the render layers it draws ([0] by default); a light: the layers it lights (Bevy's RenderLayers) */
  layers?: number[];
}

export interface OrthographicCameraProps extends Omit<PerspectiveCameraProps, "fov" | "focus" | "view"> {
  /** how much of the world the frame shows top to bottom, in metres (the width follows the frame's shape); keyed like
   * any prop, so animating it zooms. Bevy's OrthographicProjection with ScalingMode::FixedVertical */
  height?: number;
}

/** A camera without perspective: parallel lines stay parallel and size doesn't fall off with distance (isometric and
 * top-down games, technical views, flat-shaded product shots). Placed and aimed as a PerspectiveCamera is.
 * @deprecated <Camera3d> with `projection={{ Orthographic: {} }}`, raea/camera */
export function OrthographicCamera({ height = 10, children, ...rest }: OrthographicCameraProps & BevyParts) {
  if (rest.position !== undefined) checkVec("OrthographicCamera", "position", rest.position);
  if (rest.lookAt !== undefined) checkVec("OrthographicCamera", "lookAt", rest.lookAt);
  if (rest.up !== undefined) checkVec("OrthographicCamera", "up", rest.up);
  if (!(height > 0 && Number.isFinite(height))) throw new RangeError(`OrthographicCamera: height is the view's height in metres, above 0, got ${height}`);
  // a camera into an image (`target`, R3F23a) isn't the one the pointer picks through
  useCamera3d({ position: rest.position, lookAt: rest.lookAt, height, up: rest.up }, rest.active !== false && rest.target === undefined);
  return node3d("PerspectiveCamera", { ...rest, ortho: height }, children);
}

/** A part's Euler angles of a quaternion: a composition's `rotation` [x, y, z] is composed as three.js composes it,
 *  Quat::from_euler(EulerRot::XYZ, x, y, z) (runtime/scene `local`; the YXZ rot_x/rot_y/rot_z of
 *  raea-render's rows are read back from the world matrix), so this is that order's inverse. */
export function eulerOf(q: [number, number, number, number]): Vec3 {
  const [x, y, z, w] = q;
  // the rotation matrix's entries (row, column) that the angles come from
  const m02 = 2 * (x * z + w * y), m12 = 2 * (y * z - w * x), m22 = 1 - 2 * (x * x + y * y);
  const m01 = 2 * (x * y - w * z), m00 = 1 - 2 * (y * y + z * z);
  const ay = Math.asin(Math.max(-1, Math.min(1, m02)));
  // gimbal lock (y = ±π/2): x and z turn about the same axis, and the atan2s below would read rounding noise, so z
  // is 0 and x the whole turn
  if (Math.hypot(m12, m22) < 1e-7) return [Math.atan2(2 * (y * z + w * x), 1 - 2 * (x * x + z * z)), ay, 0];
  return [Math.atan2(-m12, m22), ay, Math.atan2(-m01, m00)];
}

export interface EntityProps {
  /** what an Entity field elsewhere names it by (a ChildOf's parent, an observer's target); unique in the scene */
  name?: string;
  /** where it stands, metres (x, y, z) */
  position?: Vec3;
  /** Euler XYZ in radians */
  rotation?: Vec3;
  /** one number, or x, y and z */
  scale?: Vec3 | number;
  /** Bevy's Transform (Transform.fromXyz(..).withRotation(..)), in place of position, rotation and scale: the entity
   * is then a parent whose children (meshes, lights, models, other entities) move with it, as Bevy's ChildOf
   * children do (the old Group) */
  transform?: { translation?: Vec3; rotation?: [number, number, number, number]; scale?: Vec3 };
  /** with `transform`: the composition pass of every part inside that doesn't name its own (Mesh's `pass`) */
  pass?: string;
  /** with `transform`: the planned detail of every part inside that plans none of its own, 0 (the hero) to 4 (far) */
  lod?: PlannedLod;
  /** its Bevy components (<BevyComponent>, or the generated wrappers in raea/bevy-own); with `transform`, also
   * the parts it carries */
  children?: ReactNode;
}

/** A bare Bevy entity: a Transform where it's placed (static) and Visibility, and nothing else until its children add
 * Bevy's own components through the TypeRegistry. A Handle field takes an asset path ("ball.glb#Mesh0/Primitive0",
 * "wood.png"), an inline asset (a StandardMaterial's fields as an object) or, for a Mesh, a Manifold shape
 * ("shape:box(1, 1, 1)"); an Entity field takes another part's name or row id, or entity bits. So Mesh3d,
 * MeshMaterial3d<StandardMaterial>, Sprite, ChildOf and the rest of Bevy's components are reachable without a
 * hand-written element. */
export function Entity({ children, ...rest }: EntityProps) {
  if (rest.transform !== undefined) {
    const { transform: t, pass, lod, ...own } = rest;
    if (own.position !== undefined || own.rotation !== undefined || own.scale !== undefined) throw new TypeError("Entity: transform, or position, rotation and scale, not both");
    if (own.name !== undefined) throw new TypeError("Entity: a parent entity (with a transform) takes no name yet; name a bare Entity inside it");
    // a parent entity draws as the old Group: its place, and its children placed in it
    return (
      <Group position={t.translation ?? [0, 0, 0]} rotation={t.rotation ? eulerOf(t.rotation) : [0, 0, 0]} scale={t.scale ?? 1} {...(pass !== undefined ? { pass } : {})} {...(lod !== undefined ? { lod } : {})}>
        {children}
      </Group>
    );
  }
  if (rest.position !== undefined) checkVec("Entity", "position", rest.position);
  if (rest.rotation !== undefined) checkVec("Entity", "rotation", rest.rotation);
  if (rest.lod !== undefined) throw new TypeError("Entity: lod is a parent entity's (with a transform), for the parts inside it; a part takes its own");
  if (rest.name !== undefined && (typeof rest.name !== "string" || !rest.name)) throw new TypeError(`Entity: name is a non-empty string, got ${JSON.stringify(rest.name)}`);
  const id = useId();
  const m = usePlace3d(rest.position ?? [0, 0, 0], rest.rotation ?? [0, 0, 0], rest.scale ?? 1);
  return node3d("Entity", rest, <Place3d.Provider value={m}>{pageLayers("Entity", rest.name ?? `layer${id}`, children)}</Place3d.Provider>);
}

export interface GroupProps {
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  /** a rotation as a quaternion [x, y, z, w] (a baked body's pose, bodyPose), in place of `rotation` */
  quaternion?: [number, number, number, number];
  scale?: number | Vec3;
  /** the composition pass of every part inside that doesn't name its own (Mesh's `pass`) */
  pass?: string;
  /** the planned detail of every part inside that plans none of its own, 0 (the hero) to 4 (far): a skyline's
   * buildings, a street's far end (PlannedLod) */
  lod?: PlannedLod;
  children?: ReactNode;
}

/** Moves, turns and scales everything inside it together (a product and its stand, a car and its wheels). */
export interface RefinishProps {
  /** the finishes, each a <MeshStandardMaterial>'s props, over every glTF material or the one its `material` names */
  materials: MeshStandardMaterialProps[];
  /** the models refinished */
  children?: ReactNode;
}

/** Every <Model> inside re-covered: each of its glTF materials (all, or the one each names by `material`) given the
 * colour, texture or finish each gives, the rest of it kept (a run of cabinets painted, the doors and drawers with them;
 * `map: null` takes the wood's picture off). A model's own <MeshStandardMaterial> children go first. */
export function Refinish({ materials, children }: RefinishProps) {
  if (!Array.isArray(materials) || materials.length === 0) throw new TypeError("Refinish: materials is a list of MeshStandardMaterial props ([{ color: \"#8a9a7b\", map: null }])");
  const specs = materials.map((m) => {
    checkMaterial("Refinish", m as Record<string, unknown>);
    return colours("Refinish", { ...m }, ["color", "emissive"]);
  });
  return node3d("Refinish", { materials: specs }, children);
}

/** @deprecated <Entity transform={...}> */
export function Group(props: GroupProps) {
  const { position = [0, 0, 0], rotation = [0, 0, 0], scale = 1 } = props;
  checkVec("Group", "position", position);
  checkVec("Group", "rotation", rotation);
  if (typeof scale !== "number") checkVec("Group", "scale", scale);
  checkPass("Group", props.pass);
  const q = props.quaternion;
  if (q !== undefined && !(Array.isArray(q) && q.length === 4 && q.every(Number.isFinite))) throw new TypeError(`Group: quaternion is [x, y, z, w], got ${JSON.stringify(q)}`);
  const m = usePlace3d(position, rotation, scale, q);
  const lod = usePlannedLod("Group", props.lod);
  return node3d("Group", { position, rotation, scale, ...(q !== undefined ? { quaternion: q } : {}), ...(props.pass !== undefined ? { pass: props.pass } : {}) }, <Place3d.Provider value={m}><PlannedLodOf.Provider value={lod}>{props.children}</PlannedLodOf.Provider></Place3d.Provider>);
}

export interface ModelProps {
  /** a glTF file in the media folder: staticFile("models/shoe.glb") */
  src: string;
  /** the entity's name, for picking and the inspector */
  name?: string;
  /** how its own glTF textures are read (docs/shaders.feature SH5): { filter: "nearest" } for hard pixels, whatever
   * the file says; a <MeshStandardMaterial> child's maps take their own `sampler` */
  sampler?: SamplerSpec;
  /** the composition pass it draws in ("scene" by default): a frame-wide effect told to skip a pass (<Look skip>,
   * <Outline skip>, <Bloom skip>, <Grade skip>) leaves its pixels as drawn, and <Look passes> gives one its own paint;
   * a piece names at most three passes besides "scene"; keyed per frame */
  pass?: string;
  /** a skinned model's own vertices replaced each frame by a baked cloth simulation (docs/cloth.md K2): `src` a .rseq
   * of the model's vertices in its bind pose, in the glTF's order (raea/body tools/cloth.ts --bind), `frame` the one
   * drawn (keyed per frame). The skin poses them, so its materials, alpha, morphs and bones are kept. */
  cloth?: { src: string; frame: number };
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
  /** where it stands, metres (x, y, z) */
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  /** one number, or x, y and z */
  scale?: number | Vec3;
  /** it may pass through other parts on purpose (a blade into a body, a ghost through a wall): raea's clipping
   * check leaves it out */
  allowOverlap?: boolean;
  /** set at an angle on purpose (a bowl knocked over, a mug on its side): `raea scene lint` doesn't call it
   * tilted */
  posed?: boolean;
  /** a clip to play: its name in the file, or its index */
  animation?: string | number;
  /** live pictures on its materials, by the glTF material's name: `{ monitor_screen: <Screen resolution={[384, 272]}>
   * <Game /></Screen> }` (or a bare `<Game />` for the defaults). Each mesh with that material shows the screen, the
   * picture fitted to the mesh's texture coordinates and stood upright */
  screens?: ModelScreens;
  /** nodes of the glTF hidden, by name (a robot's painted-on eyes under the screen that replaces them) */
  hide?: string[];
  /** glTF nodes' local transforms, by name, set each frame (a joint turned by hand: Bevy's gltf_skinned_mesh turns
   * GltfNode2). Bevy's loader names an unnamed node GltfNode{index}. A part left out stays as the file has it; give
   * the same nodes and parts every frame */
  nodes?: Record<string, { translation?: Vec3; rotation?: [number, number, number, number]; scale?: Vec3 }>;
  /** graph paper over the model's surfaces, as MeshStandardMaterial's `grid`, but drawn in the model's own texture
   * coordinates (taken as metres), so the lines stay on the surface as it moves; no outlines. `toon`: its light in
   * three flat bands (toon shading) */
  grid?: { cell: number; major?: number; color?: string; toon?: boolean };
  /** several clips blended by weight (Bevy's AnimationGraph): each its name or index, its time in seconds (the
   * Sequence's time by default) and weight (1 by default). A transition is weights moved over frames (interpolate one
   * down as the next comes up); a layer is a second clip at a lower weight. Up to 4; not with `animation` */
  animations?: { clip: string | number; time?: number; weight?: number }[];
  /** an authored animation graph (Bevy's AnimationGraph): a clip node, or a blend (its children's weights normalised)
   * or an additive node (each child times its weight, added on) over children; any node may leave the subtrees of the
   * bones in its `mask` alone (a wave on the arm over a walk: { add: [{ clip: "walk" }, { clip: "wave", mask: ["leg_l",
   * "leg_r"] }] }). Weights and clip times may change every frame. Up to 16 nodes; not with animation or animations */
  graph?: AnimGraphNode;
  /** morph target (blend shape) weights, 0 to 1: by target name ({ smile: 0.8 }) or index ([0.8, 0]); up to 64. The
   * names are fixed when the model is made: pass the same set every frame (0 for a target at rest) */
  morphs?: Record<string, number> | number[];
  /** seconds into the clip (by default the Sequence's time: frame / fps); the clip loops unless loop={false} */
  time?: number;
  /** false plays the clip once and holds its last pose */
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
  /** false: its meshes cast no shadow (true by default) */
  shadow?: boolean;
  /** where it's drawn: its distances or sizes on screen, cross-faded, and its shadow (Detail; raea-sys-lod) */
  detail?: Detail;
  /** its planned detail, 0 (the hero) to 4 (far): how large it is meant to be seen. A render that shows it finer
   * says so (`lod-closer`); a stand-in or a far backdrop is planned at 3 or 4 (PlannedLod) */
  lod?: PlannedLod;
  /** false: no shadow falls on its meshes, its own included (true by default): a furry coat of many cards that would
   * otherwise speckle itself in the shadow maps */
  receiveShadow?: boolean;
  /** a cell's size in frame pixels for the pixel and site looks (6 by default) */
  pixelSize?: number;
  /** the pixel look's one-cell outline, a hex colour ("#000000" by default), or "none" */
  edgeColor?: string;
  /** the site look's greys: "dark" (light ink on black, the default) or "light" */
  theme?: "dark" | "light";
  /** the site look's ink: lift + light * gain, before the dither (0.28 and 0.9 by default; on the light greys light is inverted, 0.35 and 0.75) */
  lift?: number;
  /** the site look's ink gain: lift + light * gain (0.9 by default, 0.75 on the light greys) */
  gain?: number;
  /** <MeshToonMaterial> children: toon surfaces over the model's own materials (all of them, or one each by name);
   * <MeshStandardMaterial> children: its materials (all, or one each by `material`) given the colour, texture or
   * finish each gives, the rest kept */
  children?: ReactNode;
}

/** Inside a <Model>: a material there re-covers the model's own, so only the fields it's given change (Bevy's
 *  StandardMaterial sends the rest's defaults on a Mesh3d, which would overwrite the model's). */
export const InModel = createContext(false);

/** Inside a layered <Canvas3D>: what's there is read as 3D parts, so a page element there isn't drawn (Text2d says so). */
export const InCanvas3D = createContext(false);

/** A node of a <Model graph>. */
export type AnimGraphNode =
  | { clip: string | number; time?: number; weight?: number; mask?: string[] }
  | { blend: AnimGraphNode[]; weight?: number; mask?: string[] }
  | { add: AnimGraphNode[]; weight?: number; mask?: string[] };

/** A <Model graph> depth-first, each node with its parent's index (-1 for the top one), clip times defaulted. */
function flattenGraph(root: AnimGraphNode, time: number): Record<string, unknown>[] {
  const out: Record<string, unknown>[] = [];
  const walk = (n: AnimGraphNode, parent: number, path: string) => {
    if (typeof n !== "object" || n === null) throw new TypeError(`Model: graph${path} is a node ({ clip }, { blend: [...] } or { add: [...] })`);
    const w = n.weight ?? 1;
    if (!(w >= 0 && Number.isFinite(w))) throw new RangeError(`Model: graph${path}.weight is 0 or more, got ${w}`);
    if (n.mask !== undefined && !(Array.isArray(n.mask) && n.mask.every((m) => typeof m === "string"))) throw new TypeError(`Model: graph${path}.mask is a list of bone names`);
    const i = out.length;
    if (i >= 16) throw new RangeError("Model: graph has at most 16 nodes");
    if ("clip" in n) {
      if (!(typeof n.clip === "string" || (Number.isInteger(n.clip) && (n.clip as number) >= 0))) throw new TypeError(`Model: graph${path}.clip is a clip's name or index`);
      const t = n.time ?? time;
      if (!Number.isFinite(t)) throw new RangeError(`Model: graph${path}.time is seconds, got ${t}`);
      out.push({ kind: "clip", clip: n.clip, parent, mask: n.mask ?? [], weight: w, time: t });
      return;
    }
    const kids = "blend" in n ? n.blend : "add" in n ? n.add : undefined;
    if (!Array.isArray(kids) || kids.length === 0) throw new TypeError(`Model: graph${path} is { clip }, or { blend } or { add } with at least one child`);
    out.push({ kind: "blend" in n ? "blend" : "add", parent, mask: n.mask ?? [], weight: w, time: 0 });
    kids.forEach((k, j) => walk(k, i, `${path}.${"blend" in n ? "blend" : "add"}[${j}]`));
  };
  walk(root, -1, "");
  return out;
}

/** The clip events that fire between two frames (Bevy's AnimationClip events, as data): an event `time` seconds into
 * the clip fires on the frame whose clip time first reaches it, that is when it lies in (prev, now], once per loop
 * when the clip loops (`duration` its length). Give the previous frame's clip time and this frame's, as a <Model>'s
 * `time` runs; on the first frame pass prev -1 so an event at 0 fires. */
export function clipEvents<E extends { time: number }>(events: E[], prev: number, now: number, duration: number, loop = true): E[] {
  if (!(duration > 0)) throw new RangeError(`clipEvents: duration is the clip's length in seconds, above 0, got ${duration}`);
  if (!loop) {
    const lo = Math.min(prev, duration);
    const hi = Math.min(now, duration);
    return events.filter((e) => e.time > lo && e.time <= hi);
  }
  const fired: E[] = [];
  for (let k = Math.max(0, Math.floor(prev / duration)); k * duration <= now; k++) {
    for (const e of events) {
      const at = k * duration + e.time;
      if (at > prev && at <= now) fired.push(e);
    }
  }
  return fired;
}

/** A glTF model. Its clips are the file's (@bevy bevy_animation::AnimationClip), played by `animation`, blended by
 * `animations` or composed by `graph`, each built by the engine into its AnimationGraph
 * (@bevy bevy_animation::graph::AnimationGraph; engine/render scene3d.rs). `look` draws the model in its own grain
 * (engine/render look.rs): @bevy bevy_pbr::mesh_material::MeshMaterial3d<raea_render::engine::look::LookMaterial>
 */
export function Model(props: ModelProps) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (typeof props.src !== "string" || !/\.(glb|gltf)$/i.test(props.src)) throw new TypeError(`Model: src must be a .glb or .gltf file (staticFile("x.glb")), got ${JSON.stringify(props.src)}`);
  for (const k of ["position", "rotation"] as const) if (props[k] !== undefined) checkVec("Model", k, props[k]);
  const time = props.time ?? frame / fps;
  if (!Number.isFinite(time)) throw new RangeError(`Model: time must be a finite number of seconds, got ${time}`);
  if (props.look !== undefined && !["solid", "pixel", "site"].includes(props.look)) throw new TypeError(`Model: look must be "solid", "pixel" or "site", got ${JSON.stringify(props.look)}`);
  checkPass("Model", props.pass);
  if (props.cloth !== undefined && !(typeof props.cloth?.src === "string" && /\.rseq$/.test(props.cloth.src) && Number.isFinite(props.cloth.frame))) throw new TypeError(`Model: cloth is { src: a .rseq file, frame: a number }, got ${JSON.stringify(props.cloth)}`);
  if (props.sampler !== undefined) samplerSpec("Model", props.sampler);
  if (props.pixelSize !== undefined && !(props.pixelSize >= 1)) throw new RangeError(`Model: pixelSize must be at least 1, got ${props.pixelSize}`);
  if (props.bones !== undefined && (typeof props.bones !== "object" || Object.values(props.bones).some((v) => typeof v !== "string"))) throw new TypeError(`Model: bones maps each joint's name to a model's name, got ${JSON.stringify(props.bones)}`);
  const p: Record<string, unknown> = { ...props, time, lod: usePlannedLod("Model", props.lod) };
  if (props.animations !== undefined) {
    const a = props.animations;
    if (props.animation !== undefined) throw new TypeError("Model: give animation (one clip) or animations (a blend), not both");
    if (!Array.isArray(a) || a.length < 1 || a.length > 4) throw new RangeError(`Model: animations is a list of 1 to 4 clips, got ${JSON.stringify(a)}`);
    p.animations = a.map((c, i) => {
      if (typeof c !== "object" || c === null || !(typeof c.clip === "string" || (Number.isInteger(c.clip) && (c.clip as number) >= 0))) throw new TypeError(`Model: animations[${i}].clip is a clip's name or index`);
      const t = c.time ?? time;
      const w = c.weight ?? 1;
      if (!Number.isFinite(t)) throw new RangeError(`Model: animations[${i}].time is seconds, got ${t}`);
      if (!(w >= 0 && Number.isFinite(w))) throw new RangeError(`Model: animations[${i}].weight is 0 or more, got ${w}`);
      return { clip: c.clip, time: t, weight: w };
    });
  }
  if (props.graph !== undefined) {
    if (props.animation !== undefined || props.animations !== undefined) throw new TypeError("Model: give graph, animations or animation, one of them");
    p.graph = flattenGraph(props.graph, time);
  }
  if (props.morphs !== undefined) {
    const m = props.morphs;
    const entries: [string | number, number][] = Array.isArray(m) ? m.map((w, i) => [i, w]) : typeof m === "object" && m !== null ? Object.entries(m) : [];
    if (entries.length < 1 || entries.length > 64) throw new RangeError(`Model: morphs names 1 to 64 targets (by name or index), got ${JSON.stringify(m)}`);
    for (const [k, w] of entries) if (!Number.isFinite(w)) throw new RangeError(`Model: morphs.${k} is a weight, got ${w}`);
    p.morphs = entries.map(([k, w]) => ({ target: k, weight: w }));
  }
  checkGrid("Model", p);
  if (props.nodes !== undefined) checkNodes(props.nodes);
  if (props.hide !== undefined && !(Array.isArray(props.hide) && props.hide.every((h) => typeof h === "string"))) throw new TypeError(`Model: hide is a list of node names, got ${JSON.stringify(props.hide)}`);
  delete p.detail;
  const kids = <InModel.Provider value={true}>{props.children}{detailNode("Model", props.detail)}</InModel.Provider>;
  if (props.screens === undefined) return node3d("Model", p, kids);
  if (typeof props.screens !== "object" || props.screens === null) throw new TypeError("Model: screens maps a material's name to a screen: { monitor_screen: <Screen><Game /></Screen> }");
  const { specs, parts } = modelScreens("Model", props.name ?? props.src.split("/").pop()!.replace(/\.(glb|gltf)$/i, ""), props.screens);
  const power = Object.values(props.screens).map((el) => (el.props as { power?: number }).power).find((v) => v !== undefined);
  return (
    <>
      {node3d("Model", { ...p, screens: specs, ...(power !== undefined ? { screenPower: power } : {}) }, kids)}
      {parts}
    </>
  );
}

export interface ImagePlaneProps {
  /** a picture in the media folder: staticFile("poster.png") */
  src: string;
  /** the entity's name, for picking and the inspector */
  name?: string;
  /** width in scene units (1 by default); height follows the picture unless given */
  width?: number;
  /** in scene units (follows the picture's shape by default) */
  height?: number;
  /** where its centre stands, metres (x, y, z) */
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  /** one number, or x, y and z */
  scale?: number | Vec3;
  /** a turn as a quaternion [x, y, z, w], instead of `rotation` (a card turned to face a point) */
  quaternion?: [number, number, number, number];
  /** false: the card casts no shadow (a cursor or a UI floating in the scene); true by default */
  shadow?: boolean;
  /** the picture cut out at half alpha instead of blended: its clear parts are no surface at all (no fog on them, no
   * sorting) and its shadow is the picture's shape, not the card's. For hard-edged pictures: a baked cloud, a sprite */
  cutout?: boolean;
  /** a sprite sheet: the picture is `[columns, rows]` frames of equal size, read left to right, top to bottom, and the
   * card shows one cell (its height follows one cell's shape). A texture atlas of a regular grid */
  grid?: [number, number];
  /** with `grid`: which cell shows (0 is the top left; whole numbers, wrapping); animate it to play the sheet, as
   * Bevy's TextureAtlas index does: frame={Math.floor(f / 3)} */
  frame?: number;
}

function checkNodes(nodes: NonNullable<ModelProps["nodes"]>): void {
  if (typeof nodes !== "object" || nodes === null || Array.isArray(nodes)) throw new TypeError(`Model: nodes maps a node's name to its transform, got ${JSON.stringify(nodes)}`);
  const len = { translation: 3, rotation: 4, scale: 3 } as const;
  for (const [name, t] of Object.entries(nodes))
    for (const [k, v] of Object.entries(t ?? {})) {
      const n = len[k as keyof typeof len];
      if (n === undefined) throw new TypeError(`Model: nodes.${name} takes translation, rotation and scale, got ${k}`);
      if (!Array.isArray(v) || v.length !== n || !v.every(Number.isFinite)) throw new RangeError(`Model: nodes.${name}.${k} is ${n} numbers, got ${JSON.stringify(v)}`);
    }
}
