// Physics in @raea/motion/3d: rigid bodies, glitches and goo, every one baked or closed-form so frame N is always the
// same picture (a render, `render --at N` and a play agree).
//
//   <Physics seconds={6} colliders={[{ min: [-0.47, 0, -0.73], max: [0.47, 0.75, 0.73] }]}>
//     <Body name="egg" src={TABLE} nodes={["egg_001"]} release={1.5} impulses={[{ t: 1.5, impulse: [0.02, 0, 0] }]} />
//     <Body name="ball" shape={{ sphere: 0.05 }} centre={[0, 1, 0]} restitution={0.7}>
//       <Mesh><Sphere args={[0.05]} /><MeshStandardMaterial color="#d33" /></Mesh>
//     </Body>
//   </Physics>
//
// - Rigid bodies (<Physics>, <Body>): Rapier in the engine (sdk/react/src/motion/physics.rs), stepped at fps x substeps
//   from rest, baked once per request (the bodies, their files, the events) into the composition's asset store and
//   played back by the frame. A body is a glTF's nodes (`src` + `nodes`: cut out with physics.part, its collider their
//   convex hull, its mass their volume x density) or a shape you give (`shape`, `centre`) drawn by its children.
//   Events: `release` (held still until then), `impulses` at times, a kinematic `path` (a function of seconds).
// - Glitches (`glitch` on <Part>, <Body>, or <Model glitch> through <Part>): the part's own triangles exploded,
//   scrambled, jittered, snapped or smeared by values of the frame (the engine's meshseq.rs); `material` variants
//   (the missing-texture checker, a flat glowing echo); `teleport`, `echoes`, `snapPose` and `zFight` helpers below.
// - Goo (<Goo>): objects melted by MLS-MPM (tools/goo, Taichi on Metal): elastic, then plastic, then a viscous fluid,
//   each from its own time, baked to a mesh sequence and drawn as <MeshSequence> with a glossy, slightly see-through
//   surface. `useGoo` says when each object's goo has taken over, so its own part can go. `solver: "sph"` is for thin
//   liquids instead (SPlisHSPlasH: coffee, water, honey), poured from cups and emitters over boxes and glTF obstacles.
import { Children, createContext, Fragment, isValidElement, useContext, type ReactElement, type ReactNode } from "react";
import { useCurrentFrame, useVideoConfig } from "./clock";
import { hasNative, native } from "./native";
import { toHex, type Vec3 } from "./3d";

type Quat = [number, number, number, number];

function node(kind: string, props: Record<string, unknown>, children?: ReactNode) {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(props)) if (v !== undefined && k !== "children" && typeof v !== "function") out[k] = v;
  return (
    <div data-3d={kind} data-props={JSON.stringify(out)}>
      {children}
    </div>
  );
}

// ---- seeded randomness and beats (pure) -------------------------------------------------------------------------

/** 0..1 from integers, the same on every run */
export function hash01(...xs: number[]): number {
  let h = 0x811c9dc5;
  for (const x of xs) {
    h ^= Math.floor(x) | 0;
    h = Math.imul(h, 0x01000193);
    h ^= h >>> 15;
    h = Math.imul(h, 0x2c1b3c6d);
    h ^= h >>> 12;
  }
  return ((h >>> 0) % 1000003) / 1000003;
}

/** The last of `times` at or before t: its index (-1 before the first) and how long ago it was, seconds. */
export function lastBeat(t: number, times: readonly number[]): { index: number; since: number; time: number } {
  let i = -1;
  for (let k = 0; k < times.length && times[k] <= t + 1e-9; k++) i = k;
  return { index: i, since: i < 0 ? Infinity : t - times[i], time: i < 0 ? -Infinity : times[i] };
}

/** A position that jumps to a new seeded spot on each beat in `times` (within `radius`, metres, on the axes `axes`
 *  scales) and holds it, back home `hold` seconds after the beat (Infinity: until the next). */
export function teleport(t: number, times: readonly number[], o: { radius: number; seed?: number; hold?: number; axes?: Vec3 }): Vec3 {
  const b = lastBeat(t, times);
  if (b.index < 0 || b.since > (o.hold ?? Infinity)) return [0, 0, 0];
  const s = o.seed ?? 0;
  const ax = o.axes ?? [1, 0.3, 1];
  return [0, 1, 2].map((a) => (hash01(s, b.index, a) * 2 - 1) * o.radius * ax[a]) as Vec3;
}

/** Positions snapped to a grid (`step` metres): a physics engine's position quantised. */
export function snapPose(p: Vec3, step: number): Vec3 {
  return step > 0 ? (p.map((x) => Math.round(x / step) * step) as Vec3) : p;
}

// ---- rotations ------------------------------------------------------------------------------------------------

export function quatMul(a: Quat, b: Quat): Quat {
  const [ax, ay, az, aw] = a;
  const [bx, by, bz, bw] = b;
  return [aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx, aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz];
}

export function quatAxis(axis: Vec3, angle: number): Quat {
  const l = Math.hypot(...axis) || 1;
  const s = Math.sin(angle / 2) / l;
  return [axis[0] * s, axis[1] * s, axis[2] * s, Math.cos(angle / 2)];
}

function rotate(q: Quat, v: Vec3): Vec3 {
  const [x, y, z, w] = q;
  const ix = w * v[0] + y * v[2] - z * v[1];
  const iy = w * v[1] + z * v[0] - x * v[2];
  const iz = w * v[2] + x * v[1] - y * v[0];
  const iw = -x * v[0] - y * v[1] - z * v[2];
  return [ix * w + iw * -x + iy * -z - iz * -y, iy * w + iw * -y + iz * -x - ix * -z, iz * w + iw * -z + ix * -y - iy * -x];
}

/** A rigid motion of a part about a point (`about`, its rest centre): turn by q there, then move by `move`. As a
 *  <Motion>/<Part> position and quaternion. */
export function about(about: Vec3, q: Quat, move: Vec3 = [0, 0, 0]): { position: Vec3; quaternion: Quat } {
  const r = rotate(q, about);
  return { position: [about[0] - r[0] + move[0], about[1] - r[1] + move[1], about[2] - r[2] + move[2]], quaternion: q };
}

// ---- parts ----------------------------------------------------------------------------------------------------

export interface PartInfo {
  file: string;
  centre: Vec3;
  lo: Vec3;
  hi: Vec3;
  tris: number;
}

export interface PartMaterial {
  /** the magenta-and-black checker of a texture that didn't load */
  missing?: boolean;
  color?: string;
  emissive?: string;
  opacity?: number;
  roughness?: number;
  unlit?: boolean;
}

const parts = new Map<string, PartInfo>();

/** glTF nodes cut out as their own file, standing where they stood (made once, kept in the asset store). */
export function part(src: string, nodes: string[], material?: PartMaterial): PartInfo {
  const key = JSON.stringify([src, nodes, material ?? null]);
  let p = parts.get(key);
  if (!p) {
    if (!hasNative()) throw new Error("part: cutting a glTF's nodes needs the raea engine (raea-motion render)");
    p = native("physics.part", { src, nodes, ...(material ? { material } : {}) }) as PartInfo;
    parts.set(key, p);
  }
  return p;
}

export interface Glitch {
  /** triangles thrown out from the centre and turned (0 whole, 1 about the part's size away) */
  explode?: number;
  /** 0..1: that share of the vertices pulled to other vertices of the part (spikes across it) */
  scramble?: number;
  /** metres of per-triangle noise */
  jitter?: number;
  /** metres: the vertices snapped to a grid that size */
  snap?: number;
  /** metres, world: the trailing side dragged back along it (motion smear) */
  smear?: Vec3;
  /** which randomness (change it on a beat for a new pattern) */
  seed?: number;
}

export interface PartProps {
  name?: string;
  /** the glTF (staticFile("x.glb")) and the nodes that make the part */
  src: string;
  nodes: string[];
  /** a rigid motion from where it stands in the file (see `about`): position, and a quaternion [x, y, z, w] */
  position?: Vec3;
  quaternion?: Quat;
  scale?: number;
  material?: PartMaterial;
  glitch?: Glitch;
  castShadow?: boolean;
}

/** A glTF's nodes drawn on their own, where they stood, moved by a rigid motion, glitched by the frame's values. */
export function Part(p: PartProps) {
  const info = part(p.src, p.nodes, p.material);
  const g = p.glitch;
  const glitch = g ? { explode: g.explode ?? 0, scramble: g.scramble ?? 0, jitter: g.jitter ?? 0, snap: g.snap ?? 0, smear: g.smear ?? [0, 0, 0], seed: g.seed ?? 0 } : undefined;
  return node("Group", { position: p.position ?? [0, 0, 0], quaternion: p.quaternion ?? [0, 0, 0, 1], scale: p.scale ?? 1 },
    node("Model", { src: info.file, name: p.name, time: 0, castShadow: p.castShadow, allowOverlap: true, ...(glitch ? { glitch } : {}) }));
}

/** Copies of a part trailing it: `count` echoes at offsets `step` apart (metres; or a function of the echo's number),
 *  in a flat glowing `material`, so a fork "duplicates" along a line. */
export function Echoes(p: PartProps & { count: number; step: Vec3 | ((k: number) => Vec3); echo?: PartMaterial }) {
  const out: ReactNode[] = [];
  for (let k = 1; k <= p.count; k++) {
    const o = typeof p.step === "function" ? p.step(k) : (p.step.map((x) => x * k) as Vec3);
    const base = p.position ?? [0, 0, 0];
    out.push(<Part key={k} {...p} name={`${p.name ?? p.nodes[0]}-echo${k}`} material={p.echo ?? p.material} position={[base[0] + o[0], base[1] + o[1], base[2] + o[2]]} glitch={undefined} />);
  }
  return <>{out}</>;
}

/** The part drawn twice in the same place, the second a hair turned, in another material: the two surfaces fight
 *  for the depth buffer and flicker in stripes (z-fighting), stronger with `amount` (radians, ~0.002). */
export function ZFight(p: PartProps & { amount: number; other?: PartMaterial }) {
  const info = part(p.src, p.nodes);
  const q = quatMul(p.quaternion ?? [0, 0, 0, 1], quatAxis([0.3, 1, 0.2], p.amount));
  const m = about(info.centre, quatAxis([0.3, 1, 0.2], p.amount));
  const pos = p.position ?? [0, 0, 0];
  return (
    <>
      <Part {...p} />
      {p.amount > 0 && <Part {...p} name={`${p.name ?? p.nodes[0]}-fight`} material={p.other ?? { missing: true }} quaternion={q} position={[pos[0] + m.position[0], pos[1] + m.position[1], pos[2] + m.position[2]]} glitch={undefined} />}
    </>
  );
}

// ---- rigid bodies ---------------------------------------------------------------------------------------------

export type Shape = "hull" | "box" | "mesh" | { box: Vec3 } | { sphere: number } | { capsule: [number, number] };

export interface Impulse {
  /** seconds from the simulation's start */
  t: number;
  /** N·s, world */
  impulse?: Vec3;
  /** where it's applied (world, metres): off the centre it spins the body too */
  at?: Vec3;
  torque?: Vec3;
}

export interface BodyProps {
  name: string;
  /** a glTF's nodes as the body (drawn as a <Part>, its collider their hull) */
  src?: string;
  nodes?: string[];
  /** "hull" (default for nodes), "box" (their bounds), "mesh" (exact triangles, fixed bodies only), or a shape */
  shape?: Shape;
  /** where a given shape's centre stands at rest, world (a nodes' body: the middle of their bounds) */
  centre?: Vec3;
  fixed?: boolean;
  /** kg (else volume x density) */
  mass?: number;
  /** kg/m³ (1000) */
  density?: number;
  friction?: number;
  restitution?: number;
  velocity?: Vec3;
  angularVelocity?: Vec3;
  linearDamping?: number;
  angularDamping?: number;
  /** seconds: held where it is until then (a cup sits until it's knocked) */
  release?: number;
  impulses?: Impulse[];
  /** a kinematic mover: its rigid motion from rest at t seconds (see `about`); it pushes the others */
  path?: (t: number) => { position: Vec3; quaternion: Quat };
  /** drawn as the body's nodes are, or by its children (inside its motion: draw them where it stands at rest) */
  material?: PartMaterial;
  glitch?: Glitch | ((t: number) => Glitch | undefined);
  castShadow?: boolean;
  children?: ReactNode;
}

export interface PhysicsProps {
  /** how long to simulate, seconds */
  seconds: number;
  gravity?: Vec3;
  /** steps per frame (8) */
  substeps?: number;
  /** static boxes (world, metres): the floor, the table top */
  colliders?: { min: Vec3; max: Vec3; friction?: number; restitution?: number }[];
  /** the frame (of the enclosing Sequence) the simulation starts on (0); before it everything is at rest */
  from?: number;
  children?: ReactNode;
}

export interface Baked {
  fps: number;
  frames: number;
  file: string;
  bodies: Record<string, { p: number[]; q: number[]; centre: Vec3 }>;
}

const bakes = new Map<string, Baked>();
const Ctx = createContext<{ baked: Baked; k: number; t: number } | null>(null);

function bodiesIn(children: ReactNode, out: ReactElement<BodyProps>[] = []): ReactElement<BodyProps>[] {
  Children.forEach(children, (c) => {
    if (!isValidElement(c)) return;
    if (c.type === Body) out.push(c as ReactElement<BodyProps>);
    else if (c.type === Fragment) bodiesIn((c.props as { children?: ReactNode }).children, out);
  });
  return out;
}

/** The simulation's request: every body's props as Rapier takes them, events in frames. */
export function physicsRequest(props: PhysicsProps, fps: number): Record<string, unknown> {
  const frames = Math.max(1, Math.round(props.seconds * fps));
  const bodies = bodiesIn(props.children).map(({ props: b }) => {
    const out: Record<string, unknown> = { name: b.name };
    for (const k of ["src", "nodes", "shape", "centre", "fixed", "mass", "density", "friction", "restitution", "velocity", "angularVelocity", "linearDamping", "angularDamping"] as const) {
      if (b[k] !== undefined) out[k] = b[k];
    }
    if (b.release !== undefined) out.release = Math.round(b.release * fps);
    if (b.impulses) out.impulses = b.impulses.map((e) => ({ frame: Math.round(e.t * fps), impulse: e.impulse ?? [0, 0, 0], ...(e.at ? { at: e.at } : {}), ...(e.torque ? { torque: e.torque } : {}) }));
    if (b.path) {
      const p: number[] = [];
      const q: number[] = [];
      for (let f = 0; f <= frames; f++) {
        const m = b.path(f / fps);
        p.push(...m.position);
        q.push(...m.quaternion);
      }
      out.path = { p, q };
    }
    return out;
  });
  const names = new Set<string>();
  for (const b of bodies) {
    if (names.has(b.name as string)) throw new TypeError(`Physics: two bodies named ${JSON.stringify(b.name)}`);
    names.add(b.name as string);
  }
  return { fps, frames, substeps: props.substeps ?? 8, gravity: props.gravity ?? [0, -9.81, 0], colliders: props.colliders ?? [], bodies };
}

/** The bake for these props (made once and kept). */
export function useBake(props: PhysicsProps): Baked {
  const { fps } = useVideoConfig();
  const req = physicsRequest(props, fps);
  const key = JSON.stringify(req);
  let b = bakes.get(key);
  if (!b) {
    if (!hasNative()) throw new Error("Physics: baking needs the raea engine (raea-motion render)");
    b = native("physics.bake", req as never) as Baked;
    bakes.set(key, b);
  }
  return b;
}

/** A body's rigid motion from rest at the current frame. */
export function bodyPose(baked: Baked, name: string, k: number): { position: Vec3; quaternion: Quat } {
  const b = baked.bodies[name];
  if (!b) throw new Error(`Physics: no body named ${JSON.stringify(name)}`);
  const i = Math.min(Math.max(k, 0), baked.frames - 1);
  return { position: [b.p[i * 3], b.p[i * 3 + 1], b.p[i * 3 + 2]], quaternion: [b.q[i * 4], b.q[i * 4 + 1], b.q[i * 4 + 2], b.q[i * 4 + 3]] };
}

/** A body's velocity at the frame (metres a second, world, of its centre): for a smear along its motion. */
export function bodyVelocity(baked: Baked, name: string, k: number): Vec3 {
  const b = baked.bodies[name];
  const at = (i: number) => {
    const m = bodyPose(baked, name, i);
    const r = rotate(m.quaternion, b.centre);
    return [m.position[0] + r[0], m.position[1] + r[1], m.position[2] + r[2]];
  };
  const a = at(k - 1);
  const c = at(k);
  return [0, 1, 2].map((x) => (c[x] - a[x]) * baked.fps) as Vec3;
}

/** Rigid bodies, simulated once and played back by the frame: the <Body>s among its children (directly, or in
 *  fragments). Before `from` and after `seconds` they hold their first and last poses. */
export function Physics(props: PhysicsProps) {
  const baked = useBake(props);
  const frame = useCurrentFrame();
  const k = frame - (props.from ?? 0);
  return <Ctx.Provider value={{ baked, k, t: k / baked.fps }}>{props.children}</Ctx.Provider>;
}

/** A rigid body inside <Physics>: drawn where the simulation has it this frame. */
export function Body(p: BodyProps) {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error(`Body ${p.name}: inside a <Physics>`);
  const m = bodyPose(ctx.baked, p.name, ctx.k);
  const glitch = typeof p.glitch === "function" ? p.glitch(ctx.t) : p.glitch;
  if (p.src && p.nodes && !p.children) {
    return <Part name={p.name} src={p.src} nodes={p.nodes} position={m.position} quaternion={m.quaternion} material={p.material} glitch={glitch} castShadow={p.castShadow} />;
  }
  return node("Group", { position: m.position, quaternion: m.quaternion }, p.children);
}

/** Inside <Physics>: the bake and the frame in it. */
export function usePhysics(): { baked: Baked; frame: number; t: number } {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("usePhysics: inside a <Physics>");
  return { baked: ctx.baked, frame: ctx.k, t: ctx.t };
}

// ---- mesh sequences and goo -----------------------------------------------------------------------------------

export interface MeshSequenceProps {
  name?: string;
  /** a .rseq (tools/goo writes them; rdb::meshseq) */
  src: string;
  /** which of its frames (clamped) */
  frame: number;
  position?: Vec3;
  rotation?: Vec3;
  scale?: number;
  /** multiplies the vertex colours */
  color?: string;
  roughness?: number;
  metalness?: number;
  clearcoat?: number;
  clearcoatRoughness?: number;
  /** 0..1 light let through (diffuse transmission): a jelly's glow */
  subsurface?: number;
  thickness?: number;
  /** 0..1 light let straight through and bent (a clear liquid: water, tea, juice), with its index of refraction (1.33
   * water) and, for a tinted one, the colour it takes after `attenuationDistance` metres through it. Screen space: it
   * refracts what's on screen behind it, and one clear layer doesn't show another through it */
  transmission?: number;
  ior?: number;
  attenuationColor?: string;
  attenuationDistance?: number;
  opacity?: number;
  /** a picture over its texture coordinates */
  map?: string;
  doubleSided?: boolean;
}

/** A baked simulation, one mesh a frame. */
export function MeshSequence(p: MeshSequenceProps) {
  if (typeof p.src !== "string" || !p.src.endsWith(".rseq")) throw new TypeError(`MeshSequence: src must be a .rseq file, got ${JSON.stringify(p.src)}`);
  if (!Number.isFinite(p.frame)) throw new RangeError(`MeshSequence: frame must be a number, got ${p.frame}`);
  const hexOf = (k: "color" | "attenuationColor") => (p[k] !== undefined ? toHex("MeshSequence", k, p[k] as string) : undefined);
  return node("MeshSequence", { ...p, frame: Math.max(0, Math.round(p.frame)), color: hexOf("color"), attenuationColor: hexOf("attenuationColor") });
}

export interface GooObject {
  name: string;
  /** a glTF's nodes (world space as they stand in it), moved by `transform` (column-major 4x4) if given; or, with no
   * src, `cup` and `level`: a liquid filling one of the goo's cups to `level` metres above its foot */
  src?: string;
  nodes?: string[];
  cup?: number;
  level?: number;
  transform?: number[];
  /** sph: what it is ("water" by default: coffee, tea, milk, juice, oil, syrup, honey set its colour, density and
   * viscosity), and where it comes from if not a cup or a glTF (filled): a box of it, or an emitter pouring a round
   * stream from `at` along `dir` (down) at `speed` m/s, `radius` m (6 mm) across, from `start` to `end` seconds */
  liquid?: "water" | "coffee" | "tea" | "milk" | "juice" | "oil" | "syrup" | "honey";
  block?: { min: Vec3; max: Vec3 };
  emitter?: { at: Vec3; dir?: Vec3; radius?: number; speed?: number; start?: number; end?: number };
  /** seconds from the goo's start when it begins to soften, and how long until it runs (mpm) */
  start: number;
  melt?: number;
  /** of the solid (3e4) and the fluid it becomes (4) */
  stiffness?: number;
  viscosity?: number;
  density?: number;
  /** the share of its pressure it keeps under tension (0.6): goo holds together in strands, a liquid (0.02) barely */
  cohesion?: number;
  /** the share of its shear stiffness a fluid keeps (0.12) and the strain it must pass to flow (0.005): 0 and 0 for a
   * liquid */
  residual?: number;
  yield?: number;
  /** "solid" fills its volume; "shell" a skin `thickness` thick (plates, petals) */
  fill?: "solid" | "shell";
  thickness?: number;
  /** its goo's colour (else its texture's) */
  color?: string;
}

export interface GooCup {
  radius: number;
  height: number;
  base?: number;
  wall?: number;
  friction?: number;
  poses: number[][];
}

export interface GooProps {
  name?: string;
  /** how long, seconds, from `from` (a frame of the enclosing Sequence) */
  seconds: number;
  from?: number;
  objects: GooObject[];
  /** static boxes the goo lies on and runs off (world) */
  colliders?: { box: { min: Vec3; max: Vec3 }; friction?: number }[];
  /** hollow cylinders on a pose track (a mug lifted and tipped) that carry the goo and pour it (tools/goo's cups):
   * radius inside, height foot to rim, base (the inside floor's height), wall, friction, and poses [t, ...the cup's
   * frame to world, 16 numbers column-major], t in seconds from the goo's start, its axis +y */
  cups?: GooCup[];
  /** the surface drawn round the particles: blur (grid nodes, 1), iso level (of the rest density, 0.33) and flat (the
   * blur across over the blur up, 1); a wider, flatter blur and a lower level close a thin spill into one sheet.
   * maxTris (120000): past it the grid coarsens, which drops a film one particle thick; a wide one needs more */
  surface?: { blur?: number; level?: number; flat?: number; maxTris?: number };
  /** "mpm" (the default: goo, melting things) or "sph": thin liquids that pour, splash and spread (coffee dumped on a
   * counter, water from a tap), simulated by SPlisHSPlasH (tools/goo/sph.py); each object a liquid, from a cup, a
   * block, a filled glTF or an emitter */
  solver?: "mpm" | "sph";
  /** the sph solver's settings: particle radius (m, 0.001), viscosity (0.002), surface tension (0: off; at millimetre
   * particles the solver alone holds a spill 4-5 mm deep, as coffee is, and even 30 beads it into a ball) with how much
   * the liquid wets what it lies on (0.5) and its damping (0.05), the largest time step (s, 0.001) */
  sph?: { radius?: number; viscosity?: number; tension?: number; tensionBoundary?: number; tensionViscosity?: number; step?: number;
    /** threads (1: a bake the same byte for byte every time; more is faster and differs run to run), the speed (m/s,
     * 25) past which the bake stops as blown up, and the boundary handling ("volume" maps or sampled "particles") */
    threads?: number; maxSpeed?: number; boundary?: "volume" | "particles" };
  /** sph: glTF meshes the liquid runs over and around (all their meshes, or the named nodes, moved by `transform`),
   * standing, or moved along `poses` like a cup's ([t, 16 numbers column-major], from where the glTF has them) */
  obstacles?: { src: string; nodes?: string[]; transform?: number[]; poses?: number[][] }[];
  /** the bake's own frame rate (the composition's by default): one bake shared by compositions at different rates */
  fps?: number;
  /** the box it lives in: goo leaving it is gone */
  bounds?: { min: Vec3; max: Vec3 };
  /** grid cell, metres (0.005): finer is slower and smoother */
  dx?: number;
  seed?: number;
  gravity?: Vec3;
  /** the surface: glossy, a little light through it */
  roughness?: number;
  clearcoat?: number;
  subsurface?: number;
  color?: string;
}

export interface GooBake {
  file: string;
  stats: { objects?: { name: string; hideFrame: number }[]; [k: string]: unknown };
}

const goos = new Map<string, GooBake>();

/** The spec tools/goo bakes for these props at the composition's `fps`: what `physics goo SPEC.json` takes, and the bake's
 *  key, so a script can bake (or check) a goo without rendering. */
export function gooSpec(p: GooProps, fps: number): Record<string, unknown> {
  return {
    fps: p.fps ?? fps, frames: Math.max(1, Math.round(p.seconds * (p.fps ?? fps))), seed: p.seed ?? 0, dx: p.dx ?? 0.005, gravity: p.gravity ?? [0, -9.81, 0],
    bounds: p.bounds ?? { min: [-0.75, 0.3, -0.95], max: [0.75, 1.25, 0.95] }, colliders: p.colliders ?? [],
    ...(p.cups ? { cups: p.cups } : {}),
    ...(p.surface ? { surface: p.surface } : {}),
    ...(p.solver ? { solver: p.solver } : {}),
    ...(p.sph ? { sph: p.sph } : {}),
    ...(p.obstacles ? { obstacles: p.obstacles } : {}),
    // mpm's defaults; an sph liquid takes its preset's
    objects: p.solver === "sph" ? p.objects : p.objects.map((o) => ({ fill: "solid", thickness: 0.006, stiffness: 3e4, viscosity: 4, density: 1000, color: null, transform: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1], ...o })),
  };
}

/** The goo's bake for these props (made once and kept), and the frame (of the enclosing Sequence) at which each
 *  object's own part should go, the goo having taken it over. */
export function useGoo(p: GooProps): GooBake & { gone: (name: string) => boolean; frame: number } {
  const { fps } = useVideoConfig();
  const f = useCurrentFrame();
  const spec = gooSpec(p, fps);
  const key = JSON.stringify(spec);
  let g = goos.get(key);
  if (!g) {
    if (!hasNative()) throw new Error("Goo: baking needs the raea engine (raea-motion render)");
    g = native("physics.goo", { spec } as never) as GooBake;
    goos.set(key, g);
  }
  // the bake's frame now: the composition's frames since `from`, at the bake's rate
  const k = p.fps ? Math.round(((f - (p.from ?? 0)) / fps) * p.fps) : f - (p.from ?? 0);
  const hide = new Map((g.stats.objects ?? []).map((o) => [o.name, o.hideFrame]));
  return { ...g, frame: k, gone: (name: string) => k >= (hide.get(name) ?? Math.round((p.objects.find((o) => o.name === name)?.start ?? Infinity) * (p.fps ?? fps)) + 6) };
}

/** Objects melting into goo: the bake drawn by the frame (nothing before `from`). */
export function Goo(p: GooProps) {
  const g = useGoo(p);
  if (g.frame < 0) return null;
  return (
    <MeshSequence name={p.name ?? "goo"} src={g.file} frame={g.frame} color={p.color ?? "#ffffff"} roughness={p.roughness ?? 0.12} clearcoat={p.clearcoat ?? 1} clearcoatRoughness={0.05}
      subsurface={p.subsurface ?? 0.25} thickness={0.03} />
  );
}
