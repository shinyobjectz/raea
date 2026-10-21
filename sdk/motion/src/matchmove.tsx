// Matchmove: 3D standing in footage's own world (spec: "3D: matchmove"). The clip's camera is solved once (COLMAP,
// raea-motion track; cached in the edit's asset store by the clip's content and the params), and each frame draws the
// footage behind the 3D (<Plate>), puts the camera where the solve says (its roll and lens too), and draws the subject
// cut out over the 3D, so a person walks in front of the type standing behind them.
//
// The solved world: metres, +Y up, the ground at y = 0, the origin where frame 0's centre ray meets the ground, frame
// 0 looking down -Z. `<Text3D position={[0, 0, -4]}>` stands on the ground.
import { createContext, useContext, type ReactNode } from "react";
import { useCurrentFrame, useVideoConfig } from "./clock";
import { Canvas3D, PerspectiveCamera, Plate, ToneMapping, type Vec3 } from "./3d";
import { hasNative, native } from "./native";

export interface MatchmoveCamera {
  t: number;
  position: Vec3;
  /** camera to world, [x, y, z, w]; the camera looks down its -Z with +Y up */
  quaternion: [number, number, number, number];
  /** false: COLMAP didn't register this frame and its pose was interpolated */
  solved: boolean;
}

/** What `raea-motion track` writes and <Matchmove> draws from. */
export interface MatchmoveSolve {
  mode: "sfm" | "rotation" | "static";
  /** the footage's size in pixels, and its frame rate */
  width: number;
  height: number;
  fps: number;
  from: number;
  seconds: number;
  frames: number;
  /** vertical field of view, degrees; focal length in pixels; the lens's radial distortion */
  fov: number;
  focal: number;
  k1: number;
  registered: number;
  /** mean reprojection error in pixels (null without structure) */
  error: number | null;
  camera_height: number;
  cameras: MatchmoveCamera[];
  /** the sparse points, in the solved world */
  points: Vec3[];
  ground: { y: number; normal: Vec3 };
  /** the subject's cut-outs, a path with %04d for the frame (1-based), or null */
  cut: string | null;
  notes: string[];
}

export interface MatchmoveProps {
  /** the footage: staticFile("clip.mp4") */
  src: string;
  /** where in the clip the shot starts, and how long it runs (seconds; 0 = to the end) */
  from?: number;
  seconds?: number;
  /** the subject to keep in front of the 3D and out of the solve: a box around it on the first frame, [x, y, w, h] in
   * 0..1 of the frame (EdgeTAM tracks it from there) */
  subject?: [number, number, number, number];
  /** the camera's height over the ground in metres (1.6 by default: handheld) */
  height?: number;
  /** the vertical field of view in degrees, for a shot without parallax (a tripod, a pan); a solve with structure
   * finds its own */
  fov?: number;
  /** frames of smoothing on the solved path (1 by default; 0 for none) */
  smooth?: number;
  /** a solve to draw from instead of solving (`raea-motion track CLIP -o solve.json`, imported) */
  solve?: MatchmoveSolve;
  /** when the frame's shape isn't the footage's (9:16 from 16:9), where the frame sits across it: 0 its left edge,
   * 1 its right, 0.5 the middle (the default); [x, y] for both axes. It may change from frame to frame */
  crop?: number | [number, number];
  /** 1 or more: a tighter window on the footage (a punch-in), still locked */
  zoom?: number;
  /** draw the subject's cut-out over the 3D (true by default when there's a subject) */
  cutout?: boolean;
  /** the scene's tone mapping: "none" by default, which leaves the footage's colours as they are */
  toneMapping?: "none" | "tony" | "aces" | "agx" | "neutral" | "reinhard";
  /** the 3D that stands in the shot, in the solved world */
  children?: ReactNode;
}

/** Each frame's state under a <Matchmove>, for the 3D inside it. */
export interface MatchmoveState {
  solve: MatchmoveSolve;
  /** the solve's frame shown now (0-based), and the clip's time */
  index: number;
  time: number;
  camera: MatchmoveCamera;
  /** where a point of the solved world lands in the composition, in pixels ([x, y], and its distance in front of the
   * camera; behind it when that's 0 or less) */
  project(p: Vec3): [number, number, number];
}

const MatchmoveContext = createContext<MatchmoveState | null>(null);

/** The solve and this frame's camera, inside a <Matchmove>. */
export function useMatchmove(): MatchmoveState {
  const m = useContext(MatchmoveContext);
  if (!m) throw new Error("useMatchmove: use it inside a <Matchmove>");
  return m;
}

const solves = new Map<string, MatchmoveSolve>();

/** The solve for these props: given, or asked of the engine (made on first use, then the store's). Pure: the same
 * props give the same solve, so it's kept for the run. */
function solveOf(p: MatchmoveProps): MatchmoveSolve {
  if (p.solve) return p.solve;
  const args = { src: p.src, from: p.from ?? 0, seconds: p.seconds ?? 0, subject: p.subject ?? null, height: p.height ?? 1.6, fov: p.fov ?? 0, smooth: p.smooth ?? 1 };
  const key = JSON.stringify(args);
  let s = solves.get(key);
  if (!s) {
    if (!hasNative()) throw new Error("Matchmove: solving needs the raea engine (raea-motion render); under plain bun give it solve={...}");
    s = native("matchmove.solve", args) as MatchmoveSolve;
    solves.set(key, s);
  }
  return s;
}

/** Checks a Matchmove's props; throws a TypeError or RangeError naming the prop. */
export function checkMatchmove(p: MatchmoveProps): void {
  if (typeof p.src !== "string" || p.src === "") throw new TypeError(`Matchmove: src must be the footage (staticFile("clip.mp4")), got ${JSON.stringify(p.src)}`);
  for (const k of ["from", "seconds"] as const) {
    const v = p[k];
    if (v !== undefined && !(Number.isFinite(v) && v >= 0)) throw new RangeError(`Matchmove: ${k} must be 0 or more seconds, got ${v}`);
  }
  if (p.subject !== undefined && !(Array.isArray(p.subject) && p.subject.length === 4 && p.subject.every((v) => Number.isFinite(v) && v >= 0 && v <= 1))) {
    throw new TypeError(`Matchmove: subject is [x, y, w, h] in 0..1 of the first frame, got ${JSON.stringify(p.subject)}`);
  }
  if (p.height !== undefined && !(p.height > 0)) throw new RangeError(`Matchmove: height must be above 0 metres, got ${p.height}`);
  if (p.fov !== undefined && !(p.fov >= 0 && p.fov < 180)) throw new RangeError(`Matchmove: fov must be 0..180 degrees, got ${p.fov}`);
  if (p.smooth !== undefined && !(p.smooth >= 0)) throw new RangeError(`Matchmove: smooth must be 0 or more frames, got ${p.smooth}`);
  if (p.zoom !== undefined && !(p.zoom >= 1)) throw new RangeError(`Matchmove: zoom must be 1 or more, got ${p.zoom}`);
  const c = p.crop;
  const inUnit = (v: unknown) => typeof v === "number" && v >= 0 && v <= 1;
  if (c !== undefined && !(inUnit(c) || (Array.isArray(c) && c.length === 2 && c.every(inUnit)))) throw new RangeError(`Matchmove: crop is 0..1 (or [x, y] in 0..1), got ${JSON.stringify(c)}`);
}

/** The footage's view in a frame of another shape: the whole view's size (the footage scaled to cover the frame, times
 * zoom) and the frame's window in it, in frame pixels. */
export function matchmoveView(footage: { width: number; height: number }, frame: { width: number; height: number }, crop: number | [number, number] = 0.5, zoom = 1) {
  const s = Math.max(frame.width / footage.width, frame.height / footage.height) * zoom;
  const [cx, cy] = Array.isArray(crop) ? crop : [crop, 0.5];
  const width = footage.width * s;
  const height = footage.height * s;
  const clamp = (v: number, hi: number) => Math.min(Math.max(v, 0), Math.max(hi, 0));
  return { width, height, x: clamp(cx * width - frame.width / 2, width - frame.width), y: clamp(cy * height - frame.height / 2, height - frame.height) };
}

/** A quaternion [x, y, z, w] turning a vector. */
export function rotate(q: [number, number, number, number], v: Vec3): Vec3 {
  const [x, y, z, w] = q;
  const [vx, vy, vz] = v;
  // t = 2 q.xyz x v; v' = v + w t + q.xyz x t
  const tx = 2 * (y * vz - z * vy), ty = 2 * (z * vx - x * vz), tz = 2 * (x * vy - y * vx);
  return [vx + w * tx + (y * tz - z * ty), vy + w * ty + (z * tx - x * tz), vz + w * tz + (x * ty - y * tx)];
}

/** The solve's frame at a time in the clip: the latest at or before it, clamped to the solve. */
export function matchmoveIndex(solve: MatchmoveSolve, time: number): number {
  const i = Math.floor((time - solve.from) * solve.fps + 1e-4);
  return Math.min(Math.max(i, 0), solve.cameras.length - 1);
}

/**
 * Footage with real 3D standing in it: the clip behind, its camera solved and followed, the subject in front. Fills
 * the frame (it is its own Canvas3D); its children are the 3D in the shot's world, lights included.
 */
export function Matchmove(props: MatchmoveProps) {
  checkMatchmove(props);
  const frame = useCurrentFrame();
  const { fps, width, height } = useVideoConfig();
  const solve = solveOf(props);
  const time = (props.from ?? solve.from) + frame / fps;
  const index = matchmoveIndex(solve, time);
  const cam = solve.cameras[index];
  const fwd = rotate(cam.quaternion, [0, 0, -1]);
  const up = rotate(cam.quaternion, [0, 1, 0]);
  const p = cam.position;
  const lookAt: Vec3 = [p[0] + fwd[0] * 10, p[1] + fwd[1] * 10, p[2] + fwd[2] * 10];
  const view = matchmoveView(solve, { width, height }, props.crop, props.zoom ?? 1);
  const plain = Math.abs(view.width - width) < 1e-6 && Math.abs(view.height - height) < 1e-6;
  // the whole view's focal length in its pixels: the footage's, scaled with it
  const f = (view.height / 2) / Math.tan((solve.fov * Math.PI) / 360);
  const project = (q: Vec3): [number, number, number] => {
    const d: Vec3 = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
    const inv: [number, number, number, number] = [-cam.quaternion[0], -cam.quaternion[1], -cam.quaternion[2], cam.quaternion[3]];
    const c = rotate(inv, d);
    const z = -c[2];
    return [view.width / 2 + (f * c[0]) / z - view.x, view.height / 2 - (f * c[1]) / z - view.y, z];
  };
  const state: MatchmoveState = { solve, index, time, camera: cam, project };
  const cutout = (props.cutout ?? props.subject !== undefined) && solve.cut;
  const cut = cutout ? solve.cut!.replace("%04d", String(index + 1).padStart(4, "0")) : null;
  return (
    <MatchmoveContext.Provider value={state}>
      <div style={{ position: "absolute", left: 0, top: 0, width, height, overflow: "hidden", background: "#000000" }}>
        <Canvas3D>
          <PerspectiveCamera position={p} lookAt={lookAt} up={up} fov={solve.fov} near={0.05} far={2000} {...(plain ? {} : { view })} />
          <ToneMapping mode={props.toneMapping ?? "none"} />
          <Plate src={props.src} time={time} k1={solve.k1} />
          {props.children}
        </Canvas3D>
        {cut ? <img fetchPriority="low" src={cut} style={{ position: "absolute", left: -view.x, top: -view.y, width: view.width, height: view.height }} /> : null}
      </div>
    </MatchmoveContext.Provider>
  );
}
