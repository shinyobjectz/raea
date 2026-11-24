// A camera path that can't lurch (docs/stage-guards.feature SG7). The usual hand-written path is a Catmull-Rom curve
// whose every segment runs 0 to 1 over its own frames, so a short segment after a long one (or the reverse) jumps the
// camera's speed at the key between them: on 11-21, 51 of a film's 155 keys jumped 2x or more. Here every channel
// (position, look point, field of view) is one curve paced by time: each key has one velocity, shared by the segments
// on both sides, so the speed carries across it. The camera eases only at the first key, the last and a `hold`, and a
// channel never overshoots a key and comes back (a push in that pulls back, a pan that swings past and returns).
import { Transform, type Projection, type TransformBuilt } from "../bevy3d/core";

export type CameraVec3 = [number, number, number];

/** A key of a camera path: where the camera is at a frame, and what it looks at. */
export type CameraKey = {
  /** The frame the camera reaches this key, in the frames of the Sequence it plays in (useCurrentFrame). */
  at: number;
  /** Where the camera is. */
  pos: CameraVec3;
  /** The point it looks at. */
  look: CameraVec3;
  /** Its vertical field of view in degrees. Left out, the key before's (38 at the first). */
  fov?: number;
  /** Frames the camera holds still here: it arrives at `at` and leaves at `at + hold`. */
  hold?: number;
};

/** Where the camera is at a frame, what it looks at and its vertical field of view in degrees. */
export type CameraPose = { pos: CameraVec3; look: CameraVec3; fov: number };

/** A camera path built by cameraPath. */
export type CameraPath = {
  /** The camera at a frame (a fraction is fine): the first key's before it, the last key's after it. */
  at(frame: number): CameraPose;
  /** Camera3d's props at a frame: its transform, looking at the look point, and its projection (fov in radians, as
   *  Bevy's is; the keys' are degrees). `<Camera3d {...path.camera(frame)} />` */
  camera(frame: number): { transform: TransformBuilt; projection: Projection };
  /** Keys whose timing makes the camera brake or speed up hard: its speed changes 2x or more within 3 frames of a key
   *  it moves through (the film diagnosis's measure, and the render's camera check, SG13), each with the fix. The
   *  path through such keys still never jumps or overshoots; the keys ask a fast move to meet a slow one. Empty when
   *  every key reads smoothly. */
  check(): string[];
  /** The frame the path starts moving from: the first key's `at`. */
  readonly start: number;
  /** The frame it comes to rest at: the last key's `at` plus its `hold`. */
  readonly end: number;
};

const DEFAULT_FOV = 38;

/** a knot: a time, its seven channels (position, look point, fov) and whether the camera is still there */
type Knot = { t: number; v: number[]; still: boolean };

/**
 * A camera path through keys timed in frames, paced by time so the camera's speed carries smoothly across every key.
 * It eases at the first key and the last, stops at a key with `hold`, and never overshoots a key and comes back.
 *
 * ```tsx
 * const path = cameraPath([
 *   { at: 0, pos: [0, 2, 9], look: [0, 1, 0] },
 *   { at: 60, pos: [3, 2, 6], look: [0, 1, 0], hold: 30 },
 *   { at: 150, pos: [1, 1.5, 3], look: [0, 1.2, 0], fov: 30 },
 * ]);
 * <Camera3d {...path.camera(useCurrentFrame())} />
 * ```
 */
export function cameraPath(keys: readonly CameraKey[]): CameraPath {
  if (!keys.length) throw new Error("cameraPath: no keys. Give at least one { at, pos, look }.");
  const knots: Knot[] = [];
  let fov = keys[0].fov ?? DEFAULT_FOV;
  const keyKnot: number[] = [];
  keys.forEach((k, i) => {
    if (!Number.isFinite(k.at)) throw new Error(`cameraPath: key ${i}'s at is ${k.at}. Give the frame it's reached.`);
    const hold = k.hold ?? 0;
    if (!(hold >= 0)) throw new Error(`cameraPath: key ${i}'s hold is ${k.hold}. Give the frames it holds still, 0 or more.`);
    const last = knots[knots.length - 1];
    if (last && !(k.at > last.t))
      throw new Error(`cameraPath: key ${i} is at frame ${k.at}, not after the key before (${last.t === keys[i - 1].at ? "at" : "held until"} ${last.t}). Give keys in time order, each after the one before and its hold.`);
    fov = k.fov ?? fov;
    const v = [...k.pos, ...k.look, fov];
    if (v.some((x) => !Number.isFinite(x))) throw new Error(`cameraPath: key ${i} (frame ${k.at}) has a pos, look or fov that isn't a number.`);
    keyKnot.push(knots.length);
    knots.push({ t: k.at, v, still: hold > 0 });
    if (hold > 0) knots.push({ t: k.at + hold, v, still: true });
  });
  knots[0].still = true;
  knots[knots.length - 1].still = true;
  const vel = knots.map((k, i) => (k.still ? k.v.map(() => 0) : k.v.map((_, c) => velocity(knots, i, c))));
  return {
    start: knots[0].t,
    end: knots[knots.length - 1].t,
    at,
    check() {
      const speed = (f: number) => dist(at(f).pos, at(f + 1).pos);
      let peak = 0;
      for (let f = knots[0].t; f < knots[knots.length - 1].t; f++) peak = Math.max(peak, speed(f));
      const said: string[] = [];
      keys.forEach((k, i) => {
        const j = keyKnot[i];
        if (i === 0 || i === keys.length - 1 || knots[j].still || vel[j].slice(0, 3).every((x) => x === 0)) return;
        const a = speed(k.at - 3), b = speed(k.at + 3), r = Math.max(a, b) / Math.max(1e-9, Math.min(a, b));
        if (r < 2 || Math.max(a, b) <= 0.01 * peak) return;
        const p = keys[i - 1], n = keys[i + 1], h0 = k.at - (p.at + (p.hold ?? 0)), h1 = n.at - k.at;
        const d0 = dist(p.pos, k.pos), d1 = dist(k.pos, n.pos), fmt = (x: number) => x.toPrecision(2);
        said.push(`cameraPath: key ${i} (frame ${k.at}): the camera's speed goes ${fmt(a)} -> ${fmt(b)} a frame across it (${r.toFixed(1)}x within 3 frames): the move before covers ${fmt(d0)} in ${h0} frames and the one after ${fmt(d1)} in ${h1}. Give the faster move more frames, or end the slower one in a hold.`);
      });
      return said;
    },
    camera(frame: number) {
      const { pos, look, fov } = at(frame);
      return { transform: Transform.fromXyz(...pos).lookingAt(look), projection: { Perspective: { fov: (fov * Math.PI) / 180 } } };
    },
  };
  function at(frame: number): CameraPose {
    const v = sample(knots, vel, frame);
    return { pos: [v[0], v[1], v[2]], look: [v[3], v[4], v[5]], fov: v[6] };
  }
}

const dist = (a: CameraVec3, b: CameraVec3) => Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]);

// The velocity of channel c at an inner knot, in units a frame: the three-point estimate over the times on both sides
// (exact for a curve that is a parabola in time), then limited so the channel doesn't overshoot (Fritsch and Carlson):
// zero where the keys turn back in this channel, and at most three times the slope of either side.
function velocity(knots: Knot[], i: number, c: number): number {
  const a = knots[i - 1], b = knots[i], n = knots[i + 1];
  const h0 = b.t - a.t, h1 = n.t - b.t;
  const d0 = (b.v[c] - a.v[c]) / h0, d1 = (n.v[c] - b.v[c]) / h1;
  if (d0 * d1 <= 0) return 0;
  const m = (h1 * d0 + h0 * d1) / (h0 + h1);
  const cap = 3 * Math.min(Math.abs(d0), Math.abs(d1));
  return Math.sign(m) * Math.min(Math.abs(m), cap);
}

// Channel values at a frame: cubic Hermite between the two knots around it, each with its own velocity.
function sample(knots: Knot[], vel: number[][], f: number): number[] {
  if (f <= knots[0].t) return knots[0].v.slice();
  const last = knots.length - 1;
  if (f >= knots[last].t) return knots[last].v.slice();
  let lo = 0, hi = last;
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1;
    if (knots[mid].t <= f) lo = mid;
    else hi = mid;
  }
  const a = knots[lo], b = knots[hi], h = b.t - a.t, s = (f - a.t) / h;
  const s2 = s * s, s3 = s2 * s;
  const h00 = 2 * s3 - 3 * s2 + 1, h10 = s3 - 2 * s2 + s, h01 = -2 * s3 + 3 * s2, h11 = s3 - s2;
  return a.v.map((p0, c) => h00 * p0 + h10 * h * vel[lo][c] + h01 * b.v[c] + h11 * h * vel[hi][c]);
}
