// spring, from spec/motion.feature ("spring is a damped harmonic oscillator sampled at frames"). The expected values
// come from the closed-form solution written in the spec, computed here independently.
import { describe, expect, test } from "bun:test";
import { spring } from "../src";

const near = (a: number, b: number, eps = 1e-4) => expect(Math.abs(a - b)).toBeLessThan(eps);

// x(t) for m x'' + c x' + k (x - to) = 0, x(0) = from, x'(0) = 0
function physics(t: number, from: number, to: number, m: number, c: number, k: number): number {
  const w0 = Math.sqrt(k / m), z = c / (2 * Math.sqrt(k * m)), a = from - to;
  if (z < 1) {
    const wd = w0 * Math.sqrt(1 - z * z);
    return to + a * Math.exp(-z * w0 * t) * (Math.cos(wd * t) + ((z * w0) / wd) * Math.sin(wd * t));
  }
  if (z === 1) return to + a * Math.exp(-w0 * t) * (1 + w0 * t);
  const wh = w0 * Math.sqrt(z * z - 1);
  return to + a * Math.exp(-z * w0 * t) * (Math.cosh(wh * t) + ((z * w0) / wh) * Math.sinh(wh * t));
}

describe("the physics", () => {
  test("defaults: from 0 to 1, mass 1, damping 10, stiffness 100", () => {
    for (const f of [0, 1, 5, 12, 30, 60]) near(spring({ frame: f, fps: 30 }), physics(f / 30, 0, 1, 1, 10, 100));
  });
  test("from, to and config", () => {
    const config = { mass: 2, damping: 30, stiffness: 180 };
    for (const f of [0, 4, 9, 20, 45]) near(spring({ frame: f, fps: 60, from: 200, to: -50, config }), physics(f / 60, 200, -50, 2, 30, 180));
  });
  test("critically damped and overdamped never overshoot", () => {
    for (const c of [20, 40]) {
      for (let f = 0; f <= 90; f++) expect(spring({ frame: f, fps: 30, config: { damping: c } })).toBeLessThanOrEqual(1 + 1e-9);
    }
  });
  test("the default spring overshoots", () => {
    const peak = Math.max(...Array.from({ length: 60 }, (_, f) => spring({ frame: f, fps: 30 })));
    expect(peak).toBeGreaterThan(1);
  });
  test("before frame 0 it is at from", () => {
    expect(spring({ frame: -4, fps: 30, from: 3, to: 9 })).toBe(3);
  });
  test("overshootClamping stops at to", () => {
    const xs = Array.from({ length: 60 }, (_, f) => spring({ frame: f, fps: 30, config: { overshootClamping: true } }));
    expect(Math.max(...xs)).toBeLessThanOrEqual(1);
    expect(xs[59]).toBe(1);
  });
});

describe("settling, stretching, reversing, delaying", () => {
  const natural = (() => {
    // the first frame after which |x - to| stays under 0.005
    let last = 0;
    for (let f = 0; f < 600; f++) if (Math.abs(physics(f / 30, 0, 1, 1, 10, 100) - 1) >= 0.005) last = f;
    return last + 1;
  })();
  test("durationInFrames lands the settle on that frame", () => {
    const d = 20;
    expect(Math.abs(spring({ frame: d, fps: 30, durationInFrames: d }) - 1)).toBeLessThan(0.006);
    near(spring({ frame: 10, fps: 30, durationInFrames: d }), physics((10 * natural) / d / 30, 0, 1, 1, 10, 100), 2e-3);
  });
  test("delay holds from, then starts", () => {
    for (const f of [0, 5, 9]) expect(spring({ frame: f, fps: 30, delay: 10 })).toBe(0);
    near(spring({ frame: 17, fps: 30, delay: 10 }), spring({ frame: 7, fps: 30 }));
  });
  test("reverse starts at to and ends at from", () => {
    const d = 30;
    near(spring({ frame: 0, fps: 30, durationInFrames: d, reverse: true }), spring({ frame: d, fps: 30, durationInFrames: d }));
    near(spring({ frame: d, fps: 30, durationInFrames: d, reverse: true }), 0, 1e-6);
  });
  test("stretch, then reverse, then delay", () => {
    near(spring({ frame: 25, fps: 30, durationInFrames: 30, reverse: true, delay: 10 }), spring({ frame: 15, fps: 30, durationInFrames: 30, reverse: true }));
  });
});
