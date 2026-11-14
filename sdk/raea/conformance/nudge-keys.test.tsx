// <Nudge keys>: a placed part's properties over time, as the timeline writes them (docs/timeline.feature TL5d).
import { describe, expect, test } from "bun:test";
import { keyedAt, Nudge, type NudgeKey } from "@raea/video/nudge";
import { Sequence } from "../src";
import { renderFrame } from "../src/test";

const opts = { fps: 30, width: 1920, height: 1080, durationInFrames: 300 };

describe("a keyed property's value at a frame", () => {
  const scale: NudgeKey[] = [[10, 1], [40, 2]];

  test("holds before its first key and after its last, and travels in a straight line between", () => {
    expect([0, 10, 25, 40, 99].map((f) => keyedAt(scale, f))).toEqual([1, 1, 1.5, 2, 2]);
  });

  test("follows the first key's ease: hold stays then jumps, a bezier is CSS's", () => {
    expect(keyedAt([[10, 1, "hold"], [40, 2]], 39)).toBe(1);
    expect(keyedAt([[10, 1, "hold"], [40, 2]], 40)).toBe(2);
    const eased = keyedAt([[0, 0, [0.42, 0, 0.58, 1]], [100, 1]], 25);
    expect(eased).toBeCloseTo(0.1292, 3);
    // a handle above the graph overshoots
    expect(Math.max(...Array.from({ length: 50 }, (_, i) => keyedAt([[0, 0, [0.3, 1.6, 0.6, 1]], [49, 1]], i)))).toBeGreaterThan(1);
  });

  test("keys are read in time order whatever order they are written in", () => {
    expect(keyedAt([[40, 2], [10, 1]], 25)).toBe(1.5);
  });

  test("in a <Sequence> the keys run from the sequence's first frame, and a keyed property wins over its still value", () => {
    const at = (frame: number) => renderFrame(<Sequence from={50}><Nudge scale={3} keys={{ scale: [[0, 1], [30, 2]], opacity: [[0, 0], [10, 1]] }}><div>hi</div></Nudge></Sequence>, { ...opts, frame });
    expect(at(50)).toContain("opacity:0");
    expect(at(55)).toContain("opacity:0.5");
    expect(at(65)).toContain("scale(1.5)");
    expect(at(80)).toContain("scale(2)");
    expect(at(80)).not.toContain("scale(3)");
    // with no keys it is the still Nudge it was
    expect(renderFrame(<Nudge scale={3}><div>hi</div></Nudge>, { ...opts, frame: 0 })).toContain("scale(3)");
  });
});

describe("the numbers inside a mask, a key and a crop, keyed by a dotted name (TL5a)", () => {
  const at = (el: JSX.Element, frame: number) => renderFrame(el, { ...opts, frame });
  const mask = { kind: "circle" as const, x: 0.5, y: 0.5, w: 0.3, h: 0.5 };
  // the mask image's SVG, decoded
  const svg = (html: string) => decodeURIComponent(/mask-image:url\(&quot;data:image\/svg\+xml,([^&]*)&quot;\)/.exec(html)?.[1] ?? "");

  test("a mask moves and softens over time, its still kind kept", () => {
    const el = (frame: number) => at(<Nudge mask={mask} keys={{ "mask.x": [[0, 0.25], [30, 0.75]], "mask.feather": [[0, 0], [30, 0.4]] }}><div>hi</div></Nudge>, frame);
    // 0.25 of 1920 at the first frame, the middle halfway, 0.75 at the last
    expect(svg(el(0))).toContain('cx="480"');
    expect(svg(el(15))).toContain('cx="960"');
    expect(svg(el(30))).toContain('cx="1440"');
    expect(svg(el(0))).not.toContain("feGaussianBlur");
    expect(svg(el(30))).toContain('stdDeviation="21.6"');
  });

  test("a key's strength over time is a filter of its own each frame", () => {
    const el = (frame: number) => at(<Nudge chroma={{ color: "#00ff00" }} keys={{ "chroma.strength": [[0, 0.2], [10, 0.8]] }}><div>hi</div></Nudge>, frame);
    expect(el(0)).toContain("raea-key-00ff00-20-20");
    expect(el(10)).toContain("raea-key-00ff00-80-20");
  });

  test("a crop's side over time, the others as still", () => {
    const el = (frame: number) => at(<Nudge crop={[0, 0.1, 0, 0]} keys={{ "crop.left": [[0, 0], [20, 0.5]] }}><div>hi</div></Nudge>, frame);
    expect(el(10)).toContain("clip-path:inset(10% 0% 0% 25%)");
  });

  test("a curve that overshoots stays inside what the value can be", () => {
    const el = at(<Nudge mask={mask} keys={{ "mask.feather": [[0, 0, [0.3, 1.8, 0.6, 1]], [30, 1]] }}><div>hi</div></Nudge>, 12);
    expect(svg(el)).toContain('stdDeviation="54"');
  });

  test("keys for a mask with no mask, or a name it hasn't, fail", () => {
    expect(() => at(<Nudge keys={{ "mask.x": [[0, 0.2]] }}><div>hi</div></Nudge>, 0)).toThrow(/need a mask/);
    expect(() => at(<Nudge keys={{ "mask.kind": [[0, 1]] } as never}><div>hi</div></Nudge>, 0)).toThrow(/keys are for/);
  });
});

describe("a placed part mirrored and cropped (docs/timeline.feature TL3h)", () => {
  const at = (el: JSX.Element) => renderFrame(el, { ...opts, frame: 0 });

  test("mirror flips it left to right, with its scale", () => {
    expect(at(<Nudge mirror><div>hi</div></Nudge>)).toContain("transform:scale(-1, 1)");
    expect(at(<Nudge mirror scale={2} x={10}><div>hi</div></Nudge>)).toContain("transform:translate(10px, 0px) scale(-2, 2)");
  });

  test("crop cuts each side off by its share, on an element and on a component's layer", () => {
    expect(at(<Nudge crop={[0.1, 0, 0, 0.25]}><div>hi</div></Nudge>)).toContain("clip-path:inset(0% 0% 25% 10%)");
    const Part = () => <p>part</p>;
    expect(at(<Nudge crop={[0, 0.5, 0, 0]}><Part /></Nudge>)).toContain("clip-path:inset(50% 0% 0% 0%)");
    // no crop, no clip
    expect(at(<Nudge crop={[0, 0, 0, 0]}><div>hi</div></Nudge>)).not.toContain("clip-path");
  });

  test("a crop out of range fails", () => {
    expect(() => at(<Nudge crop={[1, 0, 0, 0]}><div>hi</div></Nudge>)).toThrow(/crop/);
    expect(() => at(<Nudge crop={[0, 0, 0] as unknown as [number, number, number, number]}><div>hi</div></Nudge>)).toThrow(/crop/);
  });
});
