// A part's mask, blend and chroma key (docs/timeline.feature TL7f): CapCut's Mask, Blend and Chroma key, drawn by
// <Nudge> from sdk/video cutout.ts.
import { describe, expect, test } from "bun:test";
import { BLEND_KINDS, chromaError, chromaId, chromaMatrix, maskError, maskPlain, maskStyle, maskSvg } from "@raea/video/cutout";
import { Nudge } from "@raea/video/nudge";
import { renderFrame } from "../src/test";

const opts = { fps: 30, width: 1920, height: 1080, durationInFrames: 30, frame: 0 };
const attr = (svg: string, name: string) => [...svg.matchAll(new RegExp(` ${name}="([^"]*)"`, "g"))].map((m) => m[1]);

describe("a mask keeps a shape of the part (TL7f)", () => {
  test("a circle chosen is round on any frame: half the shorter side across, in the middle", () => {
    expect(maskPlain("circle", 1920, 1080)).toMatchObject({ x: 0.5, y: 0.5, w: 0.28125, h: 0.5, feather: 0, invert: false });
    const svg = maskSvg({ kind: "circle" }, 1920, 1080);
    expect(attr(svg, "cx")).toEqual(["960"]);
    expect(attr(svg, "rx")).toEqual(["270"]);
    expect(attr(svg, "ry")).toEqual(["270"]);
    expect(svg).not.toContain("<filter");
  });

  test("each shape: a line keeps what is above it, a mirror a band, a rectangle its corners rounded, a heart and a star in their box", () => {
    const line = maskSvg({ kind: "line", y: 0.25 }, 1000, 500).split("<rect")[1];
    // a rect reaching far up, its bottom edge on the line at 125 px
    expect(Number(attr(line, "y")[0]) + Number(attr(line, "height")[0])).toBe(125);
    const mirror = maskSvg({ kind: "mirror", h: 0.2 }, 1000, 500).split("<rect")[1];
    expect(attr(mirror, "height")).toEqual(["100"]);
    expect(attr(maskSvg({ kind: "rect", w: 0.4, h: 0.4, round: 1 }, 1000, 500), "rx")).toEqual(["100"]);
    expect(maskSvg({ kind: "heart" }, 1000, 500)).toContain("<path");
    expect(maskSvg({ kind: "star" }, 1000, 500)).toContain("<polygon");
  });

  test("turned about its centre, feathered by a blur no larger than the frame and its reach, inverted by excluding it from a solid layer", () => {
    expect(maskSvg({ kind: "rect", x: 0.25, y: 0.5, rotate: 30 }, 1000, 500)).toContain('transform="rotate(30 250 250)"');
    const soft = maskSvg({ kind: "circle", feather: 1 }, 1000, 500);
    expect(attr(soft, "stdDeviation")).toEqual(["25"]);
    expect(soft).toContain('x="-75" y="-75" width="1150" height="650"');
    const inverted = maskStyle({ kind: "circle", invert: true }, 1000, 500);
    expect(inverted.maskImage.startsWith("linear-gradient(#000, #000), url(")).toBe(true);
    expect(inverted).toMatchObject({ maskComposite: "exclude", WebkitMaskComposite: "xor" });
    expect(maskStyle({ kind: "circle" }, 1000, 500)).not.toHaveProperty("maskComposite");
  });

  test("as a mask image the size of the part, its own url()s escaped so they aren't read as the page's", () => {
    const s = maskStyle({ kind: "circle", feather: 0.5 }, 1920, 1080);
    expect(s.maskImage.startsWith('url("data:image/svg+xml,')).toBe(true);
    expect(s.maskImage.slice(5)).not.toMatch(/url\(/);
    expect(s).toMatchObject({ maskSize: "100% 100%", maskRepeat: "no-repeat" });
  });

  test("a kind it hasn't got, or a value out of range, is refused", () => {
    expect(maskError({ kind: "moon" as never })).toMatch(/kind is one of line, mirror, circle, rect, heart, star/);
    expect(maskError({ kind: "circle", feather: 2 })).toMatch(/feather is from 0 to 1/);
    expect(maskError({ kind: "circle", w: -1 })).toMatch(/0 or more/);
    expect(maskError({ kind: "circle", x: 0.2 })).toBeUndefined();
  });
});

describe("a chroma key takes a colour out (TL7f)", () => {
  // what the matrix gives a colour's alpha, as the filter clamps it
  const alpha = (values: string, [r, g, b]: number[]) => {
    const a = values.split(" ").map(Number).slice(15);
    return Math.min(1, Math.max(0, a[0] * r + a[1] * g + a[2] * b + a[4]));
  };

  test("a green screen goes, red and white and grey stay, a dark green is half seen", () => {
    const m = chromaMatrix({ color: "#00ff00" });
    expect(alpha(m, [0, 1, 0])).toBe(0);
    expect(alpha(m, [0.1, 0.9, 0.15])).toBe(0);
    expect(alpha(m, [1, 0, 0])).toBe(1);
    expect(alpha(m, [1, 1, 1])).toBe(1);
    expect(alpha(m, [0.4, 0.4, 0.4])).toBe(1);
    const dark = alpha(m, [0, 0.5, 0]);
    expect(dark).toBeGreaterThan(0);
    expect(dark).toBeLessThan(1);
    // the colour rows are as they were
    expect(m.split(" ").slice(0, 15).join(" ")).toBe("1 0 0 0 0 0 1 0 0 0 0 0 1 0 0");
  });

  test("more strength takes more of the hue; more shadow lets the screen's shadows through half seen", () => {
    const pale = [0.5, 0.8, 0.5];
    expect(alpha(chromaMatrix({ color: "#0f0", strength: 0.2 }), pale)).toBe(1);
    expect(alpha(chromaMatrix({ color: "#0f0", strength: 1 }), pale)).toBe(0);
    // a shadow on the screen: a darker, duller green
    const shade = [0.05, 0.5, 0.05];
    expect(alpha(chromaMatrix({ color: "#0f0", shadow: 0 }), shade)).toBe(1);
    const half = alpha(chromaMatrix({ color: "#0f0", shadow: 1 }), shade);
    expect(half).toBeGreaterThan(0.1);
    expect(half).toBeLessThan(0.5);
  });

  test("a blue screen keys blue, not green", () => {
    const m = chromaMatrix({ color: "#1040e0" });
    expect(alpha(m, [0.06, 0.25, 0.88])).toBe(0);
    expect(alpha(m, [0, 1, 0])).toBe(1);
  });

  test("a grey has no hue to key by, and a colour must be one", () => {
    expect(chromaError({ color: "#808080" })).toMatch(/is grey/);
    expect(chromaError({ color: "green" })).toMatch(/#rrggbb or #rgb/);
    expect(chromaError({ color: "#0f0", strength: 3 })).toMatch(/strength is from 0 to 1/);
    expect(chromaId({ color: "#0f0" })).toBe("raea-key-00ff00-50-20");
  });
});

describe("<Nudge> draws them on the part it places", () => {
  test("an element gets the blend, and the key as its filter before its own, drawn beside it", () => {
    const html = renderFrame(<Nudge blend="screen" chroma={{ color: "#00ff00" }}><div style={{ filter: "blur(2px)" }}>hi</div></Nudge>, opts);
    expect(html).toContain("mix-blend-mode:screen");
    expect(html).toContain("filter:url(#raea-key-00ff00-50-20) blur(2px)");
    expect(html).toContain('<filter id="raea-key-00ff00-50-20" color-interpolation-filters="sRGB"');
    expect(html).toContain("<feColorMatrix");
    expect(html).toContain('operator="in"');
  });

  test("a masked element is placed in a full-frame layer, so the mask is laid over the frame and moves with it", () => {
    const html = renderFrame(<Nudge x={40} mask={{ kind: "circle" }}><div style={{ width: 100 }}>hi</div></Nudge>, opts);
    expect(html).toMatch(/<div style="[^"]*transform:translate\(40px, 0px\)[^"]*mask-image:url\(&quot;data:image\/svg\+xml,[^"]*"><div style="width:100px">hi<\/div><\/div>/);
  });

  test("a component is placed in a full-frame layer that carries them; normal is no blend", () => {
    const Part = () => <p>part</p>;
    const html = renderFrame(<Nudge mask={{ kind: "line", rotate: 10 }} blend="normal"><Part /></Nudge>, opts);
    expect(html).toContain("mask-image:url(");
    expect(html).not.toContain("mix-blend-mode");
  });

  test("a blend it hasn't got, a grey key and a bad mask are refused", () => {
    expect(BLEND_KINDS).toContain("plus-lighter");
    expect(() => renderFrame(<Nudge blend={"glow" as never}><div /></Nudge>, opts)).toThrow(/blend is one of normal, darken, screen/);
    expect(() => renderFrame(<Nudge chroma={{ color: "#777777" }}><div /></Nudge>, opts)).toThrow(/is grey/);
    expect(() => renderFrame(<Nudge mask={{ kind: "circle", round: 4 }}><div /></Nudge>, opts)).toThrow(/round is from 0 to 1/);
  });
});
