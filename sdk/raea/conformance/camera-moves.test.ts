// A crew's camera moves, focus and the handheld walk (docs/make.feature MK14): each move ends where its words say, a
// pan and an orbit follow their arc, a dolly zoom holds the subject's size, a rack holds on each subject either side,
// and the walk has nothing above 1.9 Hz.
import { describe, expect, test } from "bun:test";
import { focusAt, handheld, lens, shotMove, STEP_HZ, walkPath, WALK, type CameraVec3 } from "../src/camera";
import { GRADES, gradeLook } from "../src/post_process";

const dist = (a: CameraVec3, b: CameraVec3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
const near = (a: CameraVec3, b: CameraVec3, tol = 1e-6) => expect(dist(a, b)).toBeLessThan(tol);
const base = { from: [0, 1.6, 5] as CameraVec3, look: [0, 1.2, 0] as CameraVec3, frames: 100 };

describe("moves", () => {
  test("each ends where its words say", () => {
    near(shotMove({ ...base, kind: "static" }).at(50).pos, base.from);
    const d = shotMove({ ...base, kind: "dolly", by: 2 }).at(100);
    expect(dist(d.pos, base.look)).toBeCloseTo(dist(base.from, base.look) - 2, 6);
    near(d.look, base.look);
    // never through the subject
    expect(dist(shotMove({ ...base, kind: "dolly", by: 50 }).at(100).pos, base.look)).toBeCloseTo(0.3, 6);
    const t = shotMove({ ...base, kind: "truck", by: 1 }).at(100);
    expect(t.pos[0]).toBeCloseTo(1, 6);
    expect(t.look[0]).toBeCloseTo(1, 6);
    const p = shotMove({ ...base, kind: "pedestal", by: 0.5 }).at(100);
    expect(p.pos[1]).toBeCloseTo(2.1, 6);
    expect(p.look[1]).toBeCloseTo(1.7, 6);
    const c = shotMove({ ...base, kind: "crane", by: 1 }).at(100);
    expect(c.pos[1]).toBeCloseTo(2.6, 6);
    near(c.look, base.look);
  });

  test("a pan and an orbit follow their arc and a tilt turns up", () => {
    const pan = shotMove({ ...base, kind: "pan", angle: 90 });
    // the look stays on the circle around the camera, and ends 90 degrees right
    const r = dist(base.look, base.from);
    for (const f of [0, 25, 50, 75, 100]) expect(dist(pan.at(f).look, base.from)).toBeCloseTo(r, 1);
    expect(pan.at(100).look[0]).toBeGreaterThan(4.5);
    const orbit = shotMove({ ...base, kind: "orbit", angle: 180 });
    for (const f of [0, 33, 66, 100]) expect(dist(orbit.at(f).pos, base.look)).toBeCloseTo(r, 1);
    expect(orbit.at(100).pos[2]).toBeLessThan(-4.5);
    near(orbit.at(60).look, base.look, 1e-9);
    const tilt = shotMove({ ...base, kind: "tilt", angle: 20 });
    expect(tilt.at(100).look[1]).toBeGreaterThan(base.look[1] + 1);
  });

  test("a dolly zoom holds the subject's frame height", () => {
    const dz = shotMove({ ...base, kind: "dollyZoom", by: 3, fov: lens(35) });
    const held = (f: number) => {
      const p = dz.at(f);
      return Math.tan((p.fov * Math.PI) / 360) * dist(p.pos, p.look);
    };
    for (const f of [0, 30, 60, 100]) expect(held(f)).toBeCloseTo(held(0), 6);
    expect(dz.at(100).fov).toBeGreaterThan(dz.at(0).fov * 1.8);
    expect(dz.camera(50).projection).toHaveProperty("Perspective");
  });

  test("a path is its keys, and a lens is its field of view", () => {
    const p = shotMove({ kind: "path", from: base.from, look: base.look, frames: 0, keys: [{ at: 0, pos: [0, 1, 4], look: [0, 1, 0] }, { at: 60, pos: [2, 1, 3], look: [0, 1, 0] }] });
    near(p.at(60).pos, [2, 1, 3]);
    expect(lens(50)).toBeCloseTo(26.99, 1);
    expect(() => lens(0)).toThrow();
  });
});

describe("focus", () => {
  test("a rack holds on each subject either side", () => {
    const path = shotMove({ ...base, kind: "static" });
    const near1: CameraVec3 = [0, 1.6, 3], far1: CameraVec3 = [0, 1.6, -3];
    const rack = { kind: "rack" as const, from: near1, to: far1, at: 40, over: 20 };
    expect(focusAt(rack, path, 0)).toBeCloseTo(2, 6);
    expect(focusAt(rack, path, 40)).toBeCloseTo(2, 6);
    expect(focusAt(rack, path, 50)).toBeCloseTo(5, 6);
    expect(focusAt(rack, path, 60)).toBeCloseTo(8, 6);
    expect(focusAt(rack, path, 99)).toBeCloseTo(8, 6);
    expect(focusAt({ kind: "point", at: base.look }, path, 10)).toBeCloseTo(dist(base.from, base.look), 6);
  });
});

describe("handheld walk", () => {
  const fps = 30, frames = 20 * fps;
  const walk = handheld(shotMove({ kind: "dolly", from: [0, 1.6, 12], look: [0, 1.5, 0], by: 8, frames }), { fps });
  const still = shotMove({ kind: "dolly", from: [0, 1.6, 12], look: [0, 1.5, 0], by: 8, frames });

  // the share of a signal's energy above `hz` (a plain DFT over the samples)
  function above(x: number[], hz: number): number {
    const n = x.length, mean = x.reduce((a, b) => a + b, 0) / n;
    let hi = 0, all = 0;
    for (let k = 1; k < n / 2; k++) {
      let re = 0, im = 0;
      for (let i = 0; i < n; i++) {
        const a = (2 * Math.PI * k * i) / n;
        re += (x[i] - mean) * Math.cos(a);
        im -= (x[i] - mean) * Math.sin(a);
      }
      const e = re * re + im * im;
      all += e;
      if ((k * fps) / n > hz) hi += e;
    }
    return all > 0 ? hi / all : 0;
  }

  test("it moves like a walking operator and has nothing above 1.9 Hz", () => {
    const off = (c: 0 | 1 | 2, w: "pos" | "look") => Array.from({ length: frames }, (_, f) => walk.at(f)[w][c] - still.at(f)[w][c]);
    const bob = off(1, "pos");
    // it moves: a centimetre or so of bob
    expect(Math.max(...bob) - Math.min(...bob)).toBeGreaterThan(0.01);
    expect(Math.max(...bob.map(Math.abs))).toBeLessThan(0.05);
    for (const s of [off(0, "pos"), bob, off(0, "look"), off(1, "look")]) expect(above(s, 1.9)).toBeLessThan(0.01);
    expect(STEP_HZ).toBeLessThan(1.9);
  });

  test("it's the same every time, and its aim lags and settles", () => {
    const again = handheld(shotMove({ kind: "dolly", from: [0, 1.6, 12], look: [0, 1.5, 0], by: 8, frames }), { fps });
    near(again.at(123).pos, walk.at(123).pos, 1e-12);
    const pan = shotMove({ kind: "pan", from: [0, 1.6, 0], look: [0, 1.6, -5], angle: 60, frames: 30 });
    const op = handheld(pan, { fps, amount: 0 });
    // mid-turn the operator's aim is behind the path's; a second after it ends it has caught up
    expect(dist(op.at(15).look, pan.at(15).look)).toBeGreaterThan(0.05);
    expect(dist(op.at(90).look, pan.at(90).look)).toBeLessThan(0.05);
  });
});

// MK17: a path drawn on the floor plan walks at a person's pace and eye height, turning where it was drawn
describe("walkPath", () => {
  test("at eye height and an even walking pace, turning corners without stopping", () => {
    const p = walkPath([[0, 0], [0, 3], [2, 3]], { fps: 30 });
    expect(p.at(0).pos).toEqual([0, WALK.eye, 0]);
    expect(p.at(p.end).pos.map((v) => +v.toFixed(3))).toEqual([2, 1.6, 3]);
    // about 5 m (the corner rounded) at 0.5 m/s: about 10 s
    expect(p.end).toBeGreaterThan(280);
    expect(p.end).toBeLessThan(300);
    // at the start it looks 2 m along the first leg
    expect(p.at(0).look.map((v) => +v.toFixed(3))).toEqual([0, 1.6, 2]);
    // after the first second, until the last, it walks at 0.5 m/s give or take a fifth: through the corner too
    const speed = (f: number) => { const a = p.at(f).pos, b = p.at(f + 1).pos; return Math.hypot(b[0] - a[0], b[2] - a[2]) * 30; };
    for (let f = 30; f < p.end - 30; f += 5) expect(Math.abs(speed(f) - 0.5)).toBeLessThan(0.1);
    // and it stays at eye height
    expect(Math.abs(p.at(150).pos[1] - 1.6)).toBeLessThan(1e-9);
    expect(p.check()).toEqual([]);
  });
  test("a fixed look point, and what's refused", () => {
    expect(walkPath([[0, 0], [1, 0]], { look: [5, 1, 5] }).at(10).look).toEqual([5, 1, 5]);
    expect(() => walkPath([[0, 0]])).toThrow("at least two points");
    expect(() => walkPath([[0, 0], [1, 0]], { speed: 0 })).toThrow("above 0");
  });
});

// MK17: grade looks are GradeProps to spread; your changes go over them, sections merged
describe("grade looks", () => {
  test("four looks, merged with changes", () => {
    expect(Object.keys(GRADES)).toEqual(["clean", "cinematic", "gritty", "vintage"]);
    expect(GRADES.vintage.shadows?.lift).toBeGreaterThan(0);
    expect(GRADES.gritty.saturation).toBeLessThan(0.8);
    const g = gradeLook("cinematic", { vignette: 0.1, midtones: { gamma: 1.05 } });
    expect(g.vignette).toBe(0.1);
    expect(g.midtones).toEqual({ contrast: 1.08, gamma: 1.05 });
    expect(GRADES.cinematic.midtones).toEqual({ contrast: 1.08 });
    expect(() => gradeLook("noir" as never)).toThrow("clean, cinematic, gritty, vintage");
  });
});
