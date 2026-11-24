// Lines are lines (docs/stage-guards.feature SG6). A curve drawn as a chain of parts (a box a segment, named NAME-i)
// mounts a part a frame as it grows, and each new part came in a frame late or at another's place: the stage film's
// curves were drawn in chunks. Line3d and Tube are one part each, one mesh through every point, drawn up to `p` of the
// way along, solid at every frame.
import type { ReactNode } from "react";
import { Mesh3d, type Mesh3dProps } from "../bevy3d/core";
import { StandardMaterial } from "../bevy3d/materials";
import { BufferMesh, type BufferMeshProps } from "./shapes";
import type { Vec3 } from "./core";

type V = Vec3;

/** Line3d's props: a flat line of a width, its ends and joins round. */
export interface Line3dProps extends Omit<Mesh3dProps, "children"> {
  /** the points it passes through, in order (two or more); give enough for a curve to read smooth */
  points: readonly Vec3[];
  /** how much of it is drawn, 0 to 1 of its length from the first point (1): animate it to draw the line on */
  p?: number;
  /** its width in metres (0.05) */
  width?: number;
  /** the way its face looks: the normal of the plane it lies in (left out: the plane of its points, else +z) */
  normal?: Vec3;
  /** its material (a white StandardMaterial when left out) */
  children?: ReactNode;
}

/** Tube's props: a line with a round section. */
export interface TubeProps extends Omit<Mesh3dProps, "children"> {
  /** the points it passes through, in order (two or more); give enough for a curve to read smooth */
  points: readonly Vec3[];
  /** how much of it is drawn, 0 to 1 of its length from the first point (1): animate it to draw the line on */
  p?: number;
  /** its material (a white StandardMaterial when left out) */
  children?: ReactNode;
  /** its radius in metres (0.025) */
  radius?: number;
  /** sides around its section (12) */
  sides?: number;
}

/** A line through points in 3D: one flat mesh of a width, round at its ends and joins, drawn up to `p` of its length.
 *  One part, so a curve that grows is solid at every frame (never a chain of parts: SG6). Its material is its child.
 *  `<Line3d points={pts} width={0.04} p={t}><StandardMaterial base_color="#e5484d" unlit /></Line3d>` */
export function Line3d({ points, width = 0.05, normal, p = 1, children, ...mesh }: Line3dProps) {
  const pts = drawn("Line3d", points, p);
  if (!(width > 0) || !Number.isFinite(width)) throw new RangeError(`Line3d: width is in metres, more than 0, got ${width}`);
  if (pts.length < 2) return null;
  return <Mesh3d {...mesh}><BufferMesh {...ribbon(pts, width / 2, unit(normal ?? planeNormal(points)))} />{children ?? <StandardMaterial />}</Mesh3d>;
}

/** A line through points in 3D with a round section (a pipe, a wire): one mesh, drawn up to `p` of its length. */
export function Tube({ points, radius = 0.025, sides = 12, p = 1, children, ...mesh }: TubeProps) {
  const pts = drawn("Tube", points, p);
  if (!(radius > 0) || !Number.isFinite(radius)) throw new RangeError(`Tube: radius is in metres, more than 0, got ${radius}`);
  if (!(Number.isInteger(sides) && sides >= 3)) throw new RangeError(`Tube: sides is a whole number, 3 or more, got ${sides}`);
  if (pts.length < 2) return null;
  return <Mesh3d {...mesh}><BufferMesh {...tube(pts, radius, sides)} />{children ?? <StandardMaterial />}</Mesh3d>;
}

const sub = (a: V, b: V): V => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a: V, b: V, s = 1): V => [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s];
const cross = (a: V, b: V): V => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const dot = (a: V, b: V) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const len = (a: V) => Math.hypot(a[0], a[1], a[2]);
const unit = (a: V): V => { const l = len(a); return l > 1e-12 ? [a[0] / l, a[1] / l, a[2] / l] : [0, 0, 1]; };
// a unit vector square to n
const across = (n: V): V => unit(cross(n, Math.abs(n[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0]));

/** The points drawn: the line cut at `p` of its length (a point between two where it ends), repeats dropped. */
export function drawn(owner: string, points: readonly V[], p: number): V[] {
  if (!Array.isArray(points) || points.length < 2 || !points.every((q) => Array.isArray(q) && q.length === 3 && q.every(Number.isFinite)))
    throw new TypeError(`${owner}: points is two or more [x, y, z], got ${JSON.stringify(points)?.slice(0, 80)}`);
  if (!(p >= 0 && p <= 1)) throw new RangeError(`${owner}: p is how much of it is drawn, 0 to 1, got ${p}`);
  const pts: V[] = [points[0]];
  for (const q of points.slice(1)) if (len(sub(q, pts[pts.length - 1])) > 1e-9) pts.push(q);
  const segs = pts.slice(1).map((q, i) => len(sub(q, pts[i])));
  let left = p * segs.reduce((a, b) => a + b, 0);
  if (left <= 0) return [];
  const out: V[] = [pts[0]];
  for (let i = 0; i < segs.length; i++) {
    if (left >= segs[i]) { out.push(pts[i + 1]); left -= segs[i]; continue; }
    if (left > 1e-9) out.push(add(pts[i], sub(pts[i + 1], pts[i]), left / segs[i]));
    break;
  }
  return out;
}

/** The normal of the plane the points lie in (Newell's), +z when they lie on a line. */
export function planeNormal(points: readonly V[]): V {
  const n: V = [0, 0, 0];
  for (let i = 0; i < points.length; i++) {
    const a = points[i], b = points[(i + 1) % points.length];
    n[0] += (a[1] - b[1]) * (a[2] + b[2]);
    n[1] += (a[2] - b[2]) * (a[0] + b[0]);
    n[2] += (a[0] - b[0]) * (a[1] + b[1]);
  }
  return len(n) > 1e-9 * Math.max(1, ...points.map(len)) ** 2 ? unit(n) : [0, 0, 1];
}

const ROUND = 16;

/** A flat line half `h` wide facing n: a quad a segment and a disc at each point (its round joins and ends), drawn on
 *  both faces. The pieces overlap where they meet, which an opaque material draws as one. */
export function ribbon(pts: V[], h: number, n: V): Required<Pick<BufferMeshProps, "positions" | "normals" | "indices">> {
  const positions: number[] = [], normals: number[] = [], indices: number[] = [];
  const back: V = [-n[0], -n[1], -n[2]];
  const tri = (a: V, b: V, c: V) => {
    for (const [face, w] of [[n, [a, b, c]], [back, [a, c, b]]] as [V, V[]][]) {
      const k = positions.length / 3;
      for (const q of w) positions.push(...q), normals.push(...face);
      indices.push(k, k + 1, k + 2);
    }
  };
  for (let i = 0; i + 1 < pts.length; i++) {
    const a = pts[i], b = pts[i + 1], s = unit(cross(n, sub(b, a)));
    const al = add(a, s, h), ar = add(a, s, -h), br = add(b, s, -h), bl = add(b, s, h);
    tri(al, ar, br);
    tri(al, br, bl);
  }
  const u = across(n), v = cross(n, u);
  for (const c of pts) {
    for (let j = 0; j < ROUND; j++) {
      const t0 = (2 * Math.PI * j) / ROUND, t1 = (2 * Math.PI * (j + 1)) / ROUND;
      tri(c, add(add(c, u, h * Math.cos(t0)), v, h * Math.sin(t0)), add(add(c, u, h * Math.cos(t1)), v, h * Math.sin(t1)));
    }
  }
  return { positions, normals, indices };
}

/** A round line: a ring of `sides` at each point, square to the line there (frames carried along it, so the rings
 *  never twist), joined into one surface, with a flat cap at each end. */
export function tube(pts: V[], r: number, sides: number): Required<Pick<BufferMeshProps, "positions" | "normals" | "indices">> {
  const positions: number[] = [], normals: number[] = [], indices: number[] = [];
  const dirs = pts.slice(1).map((q, i) => unit(sub(q, pts[i])));
  const tangent = (i: number) => unit(add(dirs[Math.max(0, i - 1)], dirs[Math.min(dirs.length - 1, i)]));
  let side = across(tangent(0));
  pts.forEach((c, i) => {
    const t = tangent(i);
    side = unit(sub(side, t.map((x) => x * dot(side, t)) as V));
    const up = cross(t, side);
    for (let j = 0; j < sides; j++) {
      const a = (2 * Math.PI * j) / sides, d = add(side.map((x) => x * Math.cos(a)) as V, up, Math.sin(a));
      positions.push(...add(c, d, r)); normals.push(...d);
    }
  });
  for (let i = 0; i + 1 < pts.length; i++)
    for (let j = 0; j < sides; j++) {
      const a = i * sides + j, b = i * sides + ((j + 1) % sides), c = a + sides, d = b + sides;
      indices.push(a, b, d, a, d, c);
    }
  // the caps: a centre and its own ring (so their normals are the line's), wound to face out
  for (const [at, out] of [[0, -1], [pts.length - 1, 1]] as const) {
    const t = tangent(at).map((x) => x * out) as V, base = positions.length / 3;
    positions.push(...pts[at]); normals.push(...t);
    for (let j = 0; j < sides; j++) { positions.push(...positions.slice((at * sides + j) * 3, (at * sides + j) * 3 + 3)); normals.push(...t); }
    for (let j = 0; j < sides; j++) {
      const a = base + 1 + j, b = base + 1 + ((j + 1) % sides);
      out > 0 ? indices.push(base, a, b) : indices.push(base, b, a);
    }
  }
  return { positions, normals, indices };
}
