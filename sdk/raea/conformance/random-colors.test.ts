// Names a project moving from another React video tool uses (#176, docs/move-in.feature MV1-MV3): random,
// interpolateColors and OffthreadVideo, each as the issue describes it.
import { describe, expect, test } from "bun:test";
import { OffthreadVideo, Video, interpolateColors, random } from "../src";

describe("random(seed)", () => {
  test("the same seed gives the same number, in [0, 1)", () => {
    expect(random("a")).toBe(random("a"));
    expect(random(7)).toBe(random(7));
    expect(random("a")).not.toBe(random("b"));
    for (let i = 0; i < 2000; i++) {
      const v = random("r" + i);
      expect(v >= 0 && v < 1).toBe(true);
    }
  });
  test("seeds a letter apart spread over the range", () => {
    const vs = Array.from({ length: 1000 }, (_, i) => random("star" + i));
    const mean = vs.reduce((a, b) => a + b, 0) / vs.length;
    expect(Math.abs(mean - 0.5)).toBeLessThan(0.05);
    expect(new Set(vs).size).toBe(vs.length);
  });
  test("a number seeds as its text; null is Math.random", () => {
    expect(random(1)).toBe(random("1"));
    const v = random(null);
    expect(v >= 0 && v < 1).toBe(true);
    expect(() => random({} as unknown as string)).toThrow(TypeError);
  });
  test("pinned values, so a change to the hash (which moves every seeded scatter) is seen", () => {
    expect(random("a")).toBe(0.10352621669881046);
    expect(random(0)).toBe(0.3942956554237753);
    expect(random("star1")).toBe(0.8393113499041647);
  });
});

describe("interpolateColors", () => {
  test("halfway from red to blue, and the ends exactly", () => {
    expect(interpolateColors(5, [0, 10], ["#ff0000", "#0000ff"])).toBe("rgba(128, 0, 128, 1)");
    expect(interpolateColors(0, [0, 10], ["#ff0000", "#0000ff"])).toBe("rgba(255, 0, 0, 1)");
    expect(interpolateColors(10, [0, 10], ["#ff0000", "#0000ff"])).toBe("rgba(0, 0, 255, 1)");
  });
  test("held at the end colours outside the range", () => {
    expect(interpolateColors(-5, [0, 10], ["red", "blue"])).toBe("rgba(255, 0, 0, 1)");
    expect(interpolateColors(50, [0, 10], ["red", "blue"])).toBe("rgba(0, 0, 255, 1)");
  });
  test("names, rgba() and hsl(), alpha mixed too, through more than two keyframes", () => {
    expect(interpolateColors(1, [0, 2], ["rgba(0, 0, 0, 0)", "rgba(255, 255, 255, 1)"])).toBe("rgba(128, 128, 128, 0.5)");
    expect(interpolateColors(15, [0, 10, 20], ["white", "hsl(120, 100%, 50%)", "black"])).toBe("rgba(0, 128, 0, 1)");
  });
  test("a colour that doesn't parse is named", () => {
    expect(() => interpolateColors(0, [0, 1], ["#ff0000", "nonsense"])).toThrow(/"nonsense" isn't a CSS colour/);
  });
});

test("OffthreadVideo is Video", () => {
  expect(OffthreadVideo).toBe(Video);
});
