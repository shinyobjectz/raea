// interpolate and Easing, from spec/motion.feature ("interpolate maps a number through keyframes", "Easing curves").
// Fails until src/ implements them.
import { describe, expect, test } from "bun:test";
import { Easing, interpolate } from "../src";

const near = (a: number, b: number, eps = 1e-6) => expect(Math.abs(a - b)).toBeLessThan(eps);

describe("interpolate inside the range", () => {
  test("maps linearly between keyframes", () => {
    near(interpolate(15, [0, 30], [0, 1]), 0.5);
    near(interpolate(45, [0, 30, 60], [0, 100, 40]), 70);
  });
  test("one keyframe gives that output everywhere", () => {
    expect(interpolate(-5, [10], [3])).toBe(3);
    expect(interpolate(500, [10], [3])).toBe(3);
  });
  test("an easing applies to each segment's progress", () => {
    near(interpolate(15, [0, 30], [0, 1], { easing: Easing.quad }), 0.25);
  });
  test("an easing array gives each segment its own", () => {
    const v = interpolate(45, [0, 30, 60], [0, 1, 2], { easing: [Easing.linear, Easing.quad] });
    near(v, 1.25);
  });
  test("an easing array of the wrong length fails", () => {
    expect(() => interpolate(5, [0, 30, 60], [0, 1, 2], { easing: [Easing.linear] })).toThrow();
  });
  test("ranges of different lengths fail", () => {
    expect(() => interpolate(5, [0, 30], [0, 1, 2])).toThrow();
  });
  test("an input range that doesn't strictly increase fails", () => {
    expect(() => interpolate(5, [0, 30, 30], [0, 1, 2])).toThrow();
    expect(() => interpolate(5, [30, 0], [0, 1])).toThrow();
  });
  test("NaN or infinite x fails", () => {
    expect(() => interpolate(NaN, [0, 1], [0, 1])).toThrow();
    expect(() => interpolate(Infinity, [0, 1], [0, 1])).toThrow();
  });
});

describe("interpolate outside the range", () => {
  test("extend continues the end segment (the default)", () => {
    near(interpolate(60, [0, 30], [0, 1]), 2);
    near(interpolate(-30, [0, 30], [0, 1]), -1);
  });
  test("clamp holds the nearest end", () => {
    expect(interpolate(60, [0, 30], [0, 1], { extrapolateRight: "clamp" })).toBe(1);
    expect(interpolate(-9, [0, 30], [0, 1], { extrapolateLeft: "clamp" })).toBe(0);
  });
  test("left and right are independent", () => {
    near(interpolate(-30, [0, 30], [0, 1], { extrapolateRight: "clamp" }), -1);
  });
  test("identity returns x itself", () => {
    expect(interpolate(77, [0, 30], [0, 1], { extrapolateRight: "identity" })).toBe(77);
  });
  test("wrap folds x back into the input range", () => {
    near(interpolate(45, [0, 30], [0, 1], { extrapolateRight: "wrap" }), 0.5);
    near(interpolate(-15, [0, 30], [0, 1], { extrapolateLeft: "wrap" }), 0.5);
  });
});

describe("Easing", () => {
  const curves: [string, (p: number) => number][] = [
    ["linear", Easing.linear], ["quad", Easing.quad], ["cubic", Easing.cubic], ["poly(4)", Easing.poly(4)],
    ["sin", Easing.sin], ["circle", Easing.circle], ["exp", Easing.exp], ["back", Easing.back()],
    ["elastic", Easing.elastic()], ["bounce", Easing.bounce], ["ease", Easing.ease],
    ["bezier", Easing.bezier(0.25, 0.1, 0.25, 1)],
  ];
  for (const [name, e] of curves) {
    test(`${name} runs from 0 to 1`, () => {
      near(e(0), 0, 1e-3);
      near(e(1), 1, 1e-3);
    });
  }
  test("the formulas", () => {
    near(Easing.quad(0.5), 0.25);
    near(Easing.cubic(0.5), 0.125);
    near(Easing.poly(5)(0.5), 1 / 32);
    near(Easing.sin(0.5), 1 - Math.cos(Math.PI / 4));
    near(Easing.circle(0.6), 1 - Math.sqrt(1 - 0.36));
    near(Easing.exp(0.5), Math.pow(2, -5));
    near(Easing.back(1.70158)(0.5), 0.25 * (2.70158 * 0.5 - 1.70158));
    near(Easing.bounce(0.2), 7.5625 * 0.04);
  });
  test("back dips below 0 before it rises", () => {
    expect(Easing.back()(0.2)).toBeLessThan(0);
  });
  test("bezier solves y at x like CSS cubic-bezier", () => {
    // a symmetric curve passes through its centre
    near(Easing.bezier(0.42, 0, 0.58, 1)(0.5), 0.5, 1e-4);
    // the straight line is linear
    near(Easing.bezier(0.3, 0.3, 0.7, 0.7)(0.37), 0.37, 1e-4);
  });
  test("ease is bezier(0.42, 0, 1, 1)", () => {
    for (const p of [0.1, 0.3, 0.5, 0.8]) near(Easing.ease(p), Easing.bezier(0.42, 0, 1, 1)(p), 1e-6);
  });
  test("out and inOut are built from the curve", () => {
    for (const p of [0.1, 0.25, 0.7]) {
      near(Easing.out(Easing.cubic)(p), 1 - Math.pow(1 - p, 3));
      near(Easing.in(Easing.cubic)(p), Math.pow(p, 3));
    }
    near(Easing.inOut(Easing.quad)(0.25), 0.125);
    near(Easing.inOut(Easing.quad)(0.75), 1 - 0.125);
    near(Easing.inOut(Easing.quad)(0.5), 0.5);
  });
});
