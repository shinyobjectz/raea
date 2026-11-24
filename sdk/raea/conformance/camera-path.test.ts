// A camera path that can't lurch (docs/stage-guards.feature SG7): the speed carries across every key, the camera
// stops only at the ends and at a hold, and no channel overshoots a key and comes back. The measures are the film's
// camera diagnosis (11-21): the speed 3 frames either side of a key, and a framing that turns back within 2 s.
import { describe, expect, test } from "bun:test";
import { cameraPath, type CameraKey, type CameraVec3 } from "../src/camera";

type Pose = { pos: CameraVec3; look: CameraVec3; fov: number };

// uneven keys, as a film's are: a quick move, a long slow one, a short one, a long one with a narrowing lens
const KEYS: CameraKey[] = [
  { at: 0, pos: [0, 2, 12], look: [0, 1, 0] },
  { at: 20, pos: [1, 2, 9], look: [0.2, 1, 0] },
  { at: 110, pos: [2, 1.8, 5], look: [0.5, 1, 0] },
  { at: 125, pos: [2.2, 1.7, 4.2], look: [0.6, 1, 0] },
  { at: 200, pos: [2.6, 1.5, 3], look: [0.8, 1.2, 0], fov: 30 },
];

// the hand-written path the film used: each segment's Catmull-Rom runs 0 to 1 over its own frames
function perSegment(keys: CameraKey[]) {
  const cr = (p0: number, p1: number, p2: number, p3: number, t: number) =>
    0.5 * (2 * p1 + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t + (-p0 + 3 * p1 - 3 * p2 + p3) * t * t * t);
  return (f: number): Pose => {
    if (f <= keys[0].at) return { pos: keys[0].pos, look: keys[0].look, fov: 38 };
    const L = keys[keys.length - 1];
    if (f >= L.at) return { pos: L.pos, look: L.look, fov: L.fov ?? 38 };
    let i = 0;
    while (i < keys.length - 2 && f >= keys[i + 1].at) i++;
    const a = keys[Math.max(0, i - 1)], b = keys[i], c = keys[i + 1], d = keys[Math.min(keys.length - 1, i + 2)];
    const t = (f - b.at) / (c.at - b.at);
    const v = (j: number, w: "pos" | "look") => cr(a[w][j], b[w][j], c[w][j], d[w][j], t);
    return { pos: [v(0, "pos"), v(1, "pos"), v(2, "pos")], look: [v(0, "look"), v(1, "look"), v(2, "look")], fov: 38 };
  };
}

const speed = (at: (f: number) => Pose, f: number) => {
  const a = at(f).pos, b = at(f + 1).pos;
  return Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
};
// the speed at an instant (units a frame), over a thousandth of a frame
const rate = (at: (f: number) => Pose, f: number) => {
  const a = at(f).pos, b = at(f + 1e-3).pos;
  return Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]) / 1e-3;
};
// the worst ratio of the speed 3 frames before an inner key to 3 frames after it
const worstJump = (at: (f: number) => Pose, keys: CameraKey[]) =>
  Math.max(...keys.slice(1, -1).map((k) => {
    const a = speed(at, k.at - 3), b = speed(at, k.at + 3);
    return Math.max(a, b) / Math.max(1e-6, Math.min(a, b));
  }));
// framing turns: a series that moves one way, then the other, within 60 frames
const turns = (v: number[], min: number) => {
  let last = 0, lastF = -999, n = 0;
  for (let f = 1; f < v.length; f++) {
    const dv = v[f] - v[f - 1];
    if (Math.abs(dv) < min) continue;
    const s = Math.sign(dv);
    if (last && s !== last && f - lastF < 60) n++;
    if (s !== last) { last = s; lastF = f; }
  }
  return n;
};

describe("cameraPath", () => {
  test("the speed carries across every key (the per-segment path jumps 2x or more on the same keys)", () => {
    const path = cameraPath(KEYS);
    expect(worstJump(perSegment(KEYS), KEYS)).toBeGreaterThan(2);
    expect(worstJump(path.at, KEYS)).toBeLessThan(1.5);
  });

  test("it passes through every key, and holds the first and last pose outside them", () => {
    const path = cameraPath(KEYS);
    for (const k of KEYS) {
      const p = path.at(k.at);
      for (let j = 0; j < 3; j++) {
        expect(p.pos[j]).toBeCloseTo(k.pos[j], 9);
        expect(p.look[j]).toBeCloseTo(k.look[j], 9);
      }
    }
    expect(path.at(-50)).toEqual({ pos: KEYS[0].pos, look: KEYS[0].look, fov: 38 });
    expect(path.at(900).fov).toBe(30);
    expect([path.start, path.end]).toEqual([0, 200]);
  });

  test("it eases only at the ends: still at the first and last key, moving at every key between", () => {
    const path = cameraPath(KEYS);
    expect(rate(path.at, 0)).toBeLessThan(0.01 * rate(path.at, 10));
    expect(rate(path.at, 200 - 1e-3)).toBeLessThan(0.01 * rate(path.at, 160));
    for (const k of KEYS.slice(1, -1)) expect(rate(path.at, k.at)).toBeGreaterThan(0.005);
  });

  test("a hold stops the camera at its key, and it leaves from rest", () => {
    const keys: CameraKey[] = [KEYS[0], { ...KEYS[1], hold: 30 }, { ...KEYS[2], at: 140 }];
    const path = cameraPath(keys);
    for (const f of [20, 25, 35, 49.5, 50]) expect(path.at(f).pos).toEqual(keys[1].pos);
    expect(rate(path.at, 50)).toBeLessThan(0.01 * rate(path.at, 95));
    expect(path.end).toBe(140);
  });

  test("no channel overshoots a key: keys that move one way give a framing that never turns back", () => {
    const path = cameraPath(KEYS);
    const zoom: number[] = [], pan: number[] = [];
    for (let f = 0; f <= 200; f++) {
      const c = path.at(f);
      const d = Math.hypot(c.pos[0] - c.look[0], c.pos[1] - c.look[1], c.pos[2] - c.look[2]);
      zoom.push(1 / (d * Math.tan((c.fov * Math.PI) / 360)));
      pan.push(c.look[0]);
    }
    expect(turns(zoom, 0.0004)).toBe(0);
    expect(turns(pan, 0.0001)).toBe(0);
    // a key that turns back in x (1 -> 3 -> 2.9) is the furthest the camera goes: it never swings past it
    const back = cameraPath([{ at: 0, pos: [1, 0, 5], look: [0, 0, 0] }, { at: 10, pos: [3, 0, 5], look: [0, 0, 0] }, { at: 100, pos: [2.9, 0, 5], look: [0, 0, 0] }]);
    for (let f = 0; f <= 100; f += 0.5) expect(back.at(f).pos[0]).toBeLessThanOrEqual(3 + 1e-12);
  });

  test("Camera3d's props: the transform at the camera, the field of view in radians", () => {
    const path = cameraPath(KEYS);
    const { transform, projection } = path.camera(200);
    expect(JSON.stringify(transform)).toContain("2.6");
    expect("Perspective" in projection && projection.Perspective.fov).toBeCloseTo((30 * Math.PI) / 180, 12);
  });

  test("check() names a key whose timing makes the camera brake hard, with the fix, and nothing on smooth keys", () => {
    expect(cameraPath(KEYS).check()).toEqual([]);
    // the film's pattern (11-21): 10 m in 30 frames, then 0.7 m over 134
    const hard = cameraPath([{ at: 0, pos: [0, 2, 20], look: [0, 1, 0] }, { at: 60, pos: [0, 2, 18], look: [0, 1, 0] }, { at: 90, pos: [0, 2, 8], look: [0, 1, 0] }, { at: 224, pos: [0, 2, 7.3], look: [0, 1, 0] }]);
    const said = hard.check();
    // it speeds up hard into the quick move (key 1) and brakes hard out of it (key 2)
    expect(said.map((l) => l.match(/key \d+ \(frame \d+\)/)?.[0])).toEqual(["key 1 (frame 60)", "key 2 (frame 90)"]);
    expect(said[1]).toContain("the move before covers 10 in 30 frames and the one after 0.70 in 134. Give the faster move more frames, or end the slower one in a hold.");
    // a hold is a stop the keys ask for, never a lurch
    expect(cameraPath([KEYS[0], { ...KEYS[1], hold: 20 }, { ...KEYS[2], at: 130 }]).check()).toEqual([]);
  });

  test("keys out of order, or inside the one before's hold, are refused with the fix", () => {
    expect(() => cameraPath([])).toThrow("no keys");
    expect(() => cameraPath([KEYS[1], KEYS[0]])).toThrow("key 1 is at frame 0, not after the key before (at 20)");
    expect(() => cameraPath([{ ...KEYS[0], hold: 30 }, KEYS[1]])).toThrow("key 1 is at frame 20, not after the key before (held until 30)");
    expect(() => cameraPath([{ ...KEYS[0], pos: [0, NaN, 1] }])).toThrow("isn't a number");
  });
});
