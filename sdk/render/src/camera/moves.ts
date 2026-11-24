// A crew's camera moves, focus and a handheld walk (docs/make.feature MK14, from AnyShot's camera language and camrig's
// walk profile, MIT). Each move is a CameraPath (path.ts), so it is paced by time and can't lurch: a pan or an orbit
// is keyed along its arc, a dolly zoom solves its lens every frame to hold the subject's size. Focus gives the
// DepthOfField's focal_distance at a frame (point, track, or a rack from one subject to another with holds either
// side). `handheld` adds a walking operator's motion to any path: steps at 1.8 Hz, the aim on a spring that lags and
// settles, slow small reframes, and nothing above 1.9 Hz, so it reads as a person and never as noise.
import { Transform, type Projection, type TransformBuilt } from "../bevy3d/core";
import { cameraPath, type CameraKey, type CameraPath, type CameraPose, type CameraVec3 } from "./path";

export type MoveKind = "static" | "dolly" | "truck" | "pedestal" | "crane" | "pan" | "tilt" | "orbit" | "dollyZoom" | "path";

/** A move: where the camera starts, what it looks at, and how it moves over `frames` from frame `at`. */
export type ShotMove = {
  kind: MoveKind;
  /** The camera's start. */
  from: CameraVec3;
  /** What it looks at (the subject). */
  look: CameraVec3;
  /** The frame it starts moving (0 if left out). */
  at?: number;
  /** How many frames it moves for. */
  frames: number;
  /** Metres: dolly in (negative: out), truck right (negative: left), pedestal or crane up (negative: down), or a dolly
   *  zoom's push in. */
  by?: number;
  /** Degrees: pan right, tilt up, orbit counter-clockwise seen from above (negative: the other way). */
  angle?: number;
  /** The vertical field of view in degrees at the start (38 if left out); see `lens` for a focal length. */
  fov?: number;
  /** The path's own keys, for `path`. */
  keys?: CameraKey[];
};

const UP: CameraVec3 = [0, 1, 0];
const add = (a: CameraVec3, b: CameraVec3): CameraVec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a: CameraVec3, b: CameraVec3): CameraVec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const mul = (a: CameraVec3, s: number): CameraVec3 => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a: CameraVec3, b: CameraVec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const len = (a: CameraVec3) => Math.hypot(a[0], a[1], a[2]);
const norm = (a: CameraVec3): CameraVec3 => mul(a, 1 / Math.max(1e-9, len(a)));
const cross = (a: CameraVec3, b: CameraVec3): CameraVec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
/** `v` turned by `deg` about the unit axis `k` (Rodrigues). */
function turn(v: CameraVec3, k: CameraVec3, deg: number): CameraVec3 {
  const t = (deg * Math.PI) / 180, c = Math.cos(t), s = Math.sin(t);
  return add(add(mul(v, c), mul(cross(k, v), s)), mul(k, dot(k, v) * (1 - c)));
}

/** The vertical field of view (degrees) of a focal length on a full-frame (36 x 24 mm) sensor. */
export function lens(mm: number): number {
  if (!(mm > 0)) throw new Error(`lens: ${mm} mm. Give a focal length in millimetres, more than 0.`);
  return (2 * Math.atan(12 / mm) * 180) / Math.PI;
}

/** The focal lengths a lens chart shows (docs/make.feature MK15). */
export const LENSES = [18, 24, 35, 50, 85, 135] as const;

/** A path whose poses come from a function, with cameraPath's camera() and an empty check. */
function poses(start: number, end: number, at: (f: number) => CameraPose, check: () => string[] = () => []): CameraPath {
  return {
    start,
    end,
    at,
    check,
    camera(frame: number): { transform: TransformBuilt; projection: Projection } {
      const { pos, look, fov } = at(frame);
      return { transform: Transform.fromXyz(...pos).lookingAt(look), projection: { Perspective: { fov: (fov * Math.PI) / 180 } } };
    },
  };
}

/** Keys along an arc: `n` steps of `angle` degrees in all, each pose from `pose(deg)`. */
function arc(t0: number, frames: number, angle: number, fov: number, pose: (deg: number) => { pos: CameraVec3; look: CameraVec3 }): CameraKey[] {
  const n = Math.max(1, Math.ceil(Math.abs(angle) / 10));
  return Array.from({ length: n + 1 }, (_, i) => ({ at: t0 + (frames * i) / n, ...pose((angle * i) / n), fov }));
}

/**
 * A crew's move as a camera path. Each eases out of its start and into its end, and never lurches.
 *
 * ```tsx
 * const push = shotMove({ kind: "dolly", from: [0, 1.6, 4], look: [0, 1.2, 0], by: 1.5, frames: 120, fov: lens(35) });
 * <Camera3d {...push.camera(useCurrentFrame())} />
 * ```
 */
export function shotMove(m: ShotMove): CameraPath {
  const t0 = m.at ?? 0, fov = m.fov ?? 38, t1 = t0 + m.frames;
  if (m.kind !== "static" && m.kind !== "path" && !(m.frames > 0)) throw new Error(`shotMove: ${m.kind} over ${m.frames} frames. Give frames, more than 0.`);
  const view = sub(m.look, m.from);
  const dir = norm(view);
  const right = norm(cross(dir, UP));
  const by = m.by ?? 0, angle = m.angle ?? 0;
  const two = (pos: CameraVec3, look: CameraVec3) => cameraPath([{ at: t0, pos: m.from, look: m.look, fov }, { at: t1, pos, look, fov }]);
  switch (m.kind) {
    case "static":
      return cameraPath([{ at: t0, pos: m.from, look: m.look, fov }]);
    case "dolly": {
      // never through the subject: it stops 30 cm short
      const d = Math.min(by, len(view) - 0.3);
      return two(add(m.from, mul(dir, d)), m.look);
    }
    case "truck":
      return two(add(m.from, mul(right, by)), add(m.look, mul(right, by)));
    case "pedestal":
      return two(add(m.from, [0, by, 0]), add(m.look, [0, by, 0]));
    case "crane":
      // the camera rises, held on its subject (it tilts as it goes)
      return two(add(m.from, [0, by, 0]), m.look);
    case "pan":
      return cameraPath(arc(t0, m.frames, -angle, fov, (deg) => ({ pos: m.from, look: add(m.from, turn(view, UP, deg)) })));
    case "tilt":
      return cameraPath(arc(t0, m.frames, angle, fov, (deg) => ({ pos: m.from, look: add(m.from, turn(view, right, deg)) })));
    case "orbit":
      return cameraPath(arc(t0, m.frames, angle, fov, (deg) => ({ pos: add(m.look, turn(mul(view, -1), UP, deg)), look: m.look })));
    case "dollyZoom": {
      // the subject's frame height held: tan(fov/2) * distance stays what it was at the start
      const d = Math.min(by, len(view) - 0.3);
      const base = cameraPath([{ at: t0, pos: m.from, look: m.look, fov }, { at: t1, pos: add(m.from, mul(dir, d)), look: m.look, fov }]);
      const height = Math.tan((fov * Math.PI) / 360) * len(view);
      return poses(base.start, base.end, (f) => {
        const p = base.at(f);
        const dist = Math.max(0.05, len(sub(p.look, p.pos)));
        return { ...p, fov: (2 * Math.atan(height / dist) * 180) / Math.PI };
      });
    }
    case "path":
      if (!m.keys?.length) throw new Error("shotMove: a path needs its keys ({ at, pos, look }).");
      return cameraPath(m.keys);
  }
}

/** Where the lens is focused. */
export type Focus =
  | { kind: "point"; at: CameraVec3 }
  | { kind: "track"; target: (frame: number) => CameraVec3 }
  /** from one subject to the other over `over` frames starting at `at`, held on each before and after */
  | { kind: "rack"; from: CameraVec3; to: CameraVec3; at: number; over: number };

/** The focal distance (metres from the camera) at a frame: a DepthOfField's focal_distance. */
export function focusAt(f: Focus, path: CameraPath, frame: number): number {
  const cam = path.at(frame).pos;
  const d = (p: CameraVec3) => Math.max(0.05, len(sub(p, cam)));
  switch (f.kind) {
    case "point":
      return d(f.at);
    case "track":
      return d(f.target(frame));
    case "rack": {
      const s = Math.min(1, Math.max(0, (frame - f.at) / Math.max(1, f.over)));
      const e = s * s * (3 - 2 * s);
      return d(f.from) + (d(f.to) - d(f.from)) * e;
    }
  }
}

export type HandheldProfile = "walk" | "subtle";

/** What a profile moves, at amount 1 (metres and degrees), and how fast its aim catches up (Hz). */
const PROFILES: Record<HandheldProfile, { bob: number; sway: number; roll: number; reframe: number; spring: number }> = {
  walk: { bob: 0.012, sway: 0.009, roll: 0.5, reframe: 0.7, spring: 1.2 },
  subtle: { bob: 0.002, sway: 0.003, roll: 0.15, reframe: 0.35, spring: 0.8 },
};
/** A walker's steps, and the sway at half of it (one foot then the other). Every motion here is at or under STEP. */
export const STEP_HZ = 1.8;

/**
 * A walking operator's motion on a path: the camera bobs with each step (1.8 Hz) and sways with each stride (0.9 Hz),
 * rolls a little with the sway, reframes slowly, and its aim follows the path's on a spring, so it lags a turn and
 * settles after it. Deterministic: the same path, fps and seed give the same frames. Nothing moves faster than 1.9 Hz.
 *
 * ```tsx
 * const walk = handheld(shotMove({ kind: "dolly", from: [0, 1.6, 6], look: [0, 1.5, 0], by: 4, frames: 300 }), { fps: 30 });
 * ```
 */
export function handheld(path: CameraPath, o: { profile?: HandheldProfile; fps?: number; seed?: number; amount?: number } = {}): CameraPath {
  const p = PROFILES[o.profile ?? "walk"];
  if (!p) throw new Error(`handheld: profile ${o.profile}. Give "walk" or "subtle".`);
  const fps = o.fps ?? 30, amount = o.amount ?? 1, seed = o.seed ?? 1;
  const ph = (k: number) => ((Math.sin(seed * 12.9898 + k * 78.233) * 43758.5453) % 1) * 2 * Math.PI;
  // the aim on a critically damped spring, stepped once a frame from the start (kept, so a frame costs one lookup)
  const aims: CameraVec3[] = [];
  const omega = 2 * Math.PI * p.spring;
  const aimAt = (frame: number): CameraVec3 => {
    const k = Math.max(0, Math.floor(frame - path.start));
    if (!aims.length) aims.push(path.at(path.start).look);
    let vel: CameraVec3 = [0, 0, 0];
    for (let i = aims.length; i <= k + 1; i++) {
      const target = path.at(path.start + i).look, cur = aims[i - 1], dt = 1 / fps;
      const prev = aims[i - 2] ?? cur;
      vel = mul(sub(cur, prev), fps);
      const acc = sub(mul(sub(target, cur), omega * omega), mul(vel, 2 * omega));
      aims.push(add(cur, mul(add(vel, mul(acc, dt)), dt)));
    }
    const a = aims[k], b = aims[k + 1], t = frame - path.start - k;
    return add(a, mul(sub(b, a), Math.min(1, Math.max(0, t))));
  };
  const at = (frame: number): CameraPose => {
    const base = path.at(frame);
    const s = Math.max(0, frame - path.start) / fps;
    const look = aimAt(frame);
    const dir = norm(sub(look, base.pos)), right = norm(cross(dir, UP)), up = cross(right, dir);
    const w = 2 * Math.PI;
    const bob = p.bob * amount * Math.sin(w * STEP_HZ * s + ph(1));
    const sway = p.sway * amount * Math.sin(w * (STEP_HZ / 2) * s + ph(2));
    const yaw = p.reframe * amount * (0.6 * Math.sin(w * 0.13 * s + ph(3)) + 0.4 * Math.sin(w * 0.29 * s + ph(4)));
    const pitch = p.reframe * amount * 0.6 * (0.6 * Math.sin(w * 0.11 * s + ph(5)) + 0.4 * Math.sin(w * 0.23 * s + ph(6)));
    const roll = p.roll * amount * Math.sin(w * (STEP_HZ / 2) * s + ph(2));
    const pos = add(add(base.pos, mul(up, bob)), mul(right, sway));
    let view = sub(look, base.pos);
    view = turn(view, UP, yaw);
    view = turn(view, right, pitch);
    // roll as a sideways lean of the look point is what Camera3d's lookingAt can carry (its up stays world up), so a
    // small roll reads as the frame's horizon swaying with the stride
    const lean = mul(right, Math.tan((roll * Math.PI) / 180) * 0.02 * len(view));
    return { pos, look: add(add(pos, view), lean), fov: base.fov };
  };
  return poses(path.start, path.end, at, () => path.check());
}

/** A walkthrough's pace and eye height, as AnyShot's shot language keeps them: a person walking a set slowly. */
export const WALK = { speed: 0.5, eye: 1.6 } as const;

/**
 * A path drawn on the floor plan (`raea make plan`'s floor-plan.png: x right, z away from the entrance, metres) as a
 * camera walking it: at eye height, at an even walking pace (0.4-0.6 m/s reads as a person; 0.5 by default), easing
 * only where it starts and stops, its corners rounded (`turn` metres either side) so it turns as a walker does rather
 * than stopping; looking `ahead` metres along the way, or at `look` throughout. Give it to `handheld` for the step.
 */
export function walkPath(points: readonly [number, number][], o: { speed?: number; eye?: number; fps?: number; ahead?: number; look?: CameraVec3; start?: number; fov?: number; turn?: number } = {}): CameraPath {
  if (points.length < 2) throw new Error("walkPath: draw at least two points on the plan ([x, z] in metres).");
  const speed = o.speed ?? WALK.speed;
  if (speed <= 0) throw new Error("walkPath: speed is metres a second, above 0 (0.4-0.6 reads as walking).");
  const eye = o.eye ?? WALK.eye;
  const fps = o.fps ?? 30;
  const ahead = o.ahead ?? 2;
  const raw: CameraVec3[] = points.map(([x, z]) => [x, eye, z]);
  // the corners rounded: each inner point replaced by a curve from `turn` before it to `turn` after (a quadratic
  // through the corner), so the walker's heading turns over half a metre instead of on the spot
  const line: CameraVec3[] = [raw[0]];
  for (let i = 1; i < raw.length - 1; i++) {
    const [a, p, b] = [raw[i - 1], raw[i], raw[i + 1]];
    const r = Math.min(o.turn ?? 0.5, len(sub(p, a)) / 2, len(sub(b, p)) / 2);
    const p0 = add(p, mul(sub(a, p), r / Math.max(len(sub(a, p)), 1e-9)));
    const p2 = add(p, mul(sub(b, p), r / Math.max(len(sub(b, p)), 1e-9)));
    for (let k = 0; k <= 8; k++) {
      const t = k / 8;
      line.push(add(add(mul(p0, (1 - t) * (1 - t)), mul(p, 2 * (1 - t) * t)), mul(p2, t * t)));
    }
  }
  line.push(raw[raw.length - 1]);
  const lengths = line.slice(1).map((p, i) => len(sub(p, line[i])));
  const total = lengths.reduce((a, b) => a + b, 0);
  const along = (d: number): CameraVec3 => {
    let left = Math.min(Math.max(d, 0), total);
    for (let i = 0; i < lengths.length; i++) {
      if (left <= lengths[i] || i === lengths.length - 1) return add(line[i], mul(sub(line[i + 1], line[i]), lengths[i] > 0 ? Math.min(left / lengths[i], 1) : 0));
      left -= lengths[i];
    }
    return line[line.length - 1];
  };
  const end = line[line.length - 1];
  const lastDir = sub(end, line[line.length - 2]);
  const lookAt = (d: number): CameraVec3 => (o.look ? o.look : d + ahead <= total ? along(d + ahead) : add(end, mul(lastDir, (d + ahead - total) / Math.max(len(lastDir), 1e-9))));
  // a key every quarter metre at the time the walker reaches it: between them the path keeps its pace
  const start = o.start ?? 0;
  const n = Math.max(1, Math.ceil(total / 0.25));
  const keys: CameraKey[] = [];
  for (let k = 0; k <= n; k++) {
    const d = (total * k) / n;
    keys.push({ at: start + (d / speed) * fps, pos: along(d), look: lookAt(d), ...(o.fov != null && k === 0 ? { fov: o.fov } : {}) });
  }
  return cameraPath(keys);
}
