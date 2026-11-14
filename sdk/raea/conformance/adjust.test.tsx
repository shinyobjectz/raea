// <Nudge adjust>: a part's brightness, contrast, saturation, exposure, sharpen, highlights, shadows, temperature, tint,
// fade, vignette and grain (docs/timeline.feature TL7g; sdk/video adjust.ts).
import { describe, expect, test } from "bun:test";
import { adjustError, adjustId, adjustSteps, toneAt, vignetteStyle } from "@raea/video/adjust";
import { Nudge } from "@raea/video/nudge";
import { renderFrame } from "../src/test";

const opts = { fps: 30, width: 1920, height: 1080, durationInFrames: 300 };
const at = (el: JSX.Element, frame = 0) => renderFrame(el, { ...opts, frame });

describe("the tone curve", () => {
  test("as shot it is the picture as it was", () => {
    for (const v of [0, 0.25, 0.5, 0.75, 1]) expect(toneAt({}, v)).toBeCloseTo(v, 6);
    expect(adjustSteps({})).toEqual([]);
  });

  test("exposure is a stop either way, brightness lifts the mids and holds the ends", () => {
    expect(toneAt({ exposure: 1 }, 0.25)).toBeCloseTo(0.5, 6);
    expect(toneAt({ exposure: -1 }, 0.5)).toBeCloseTo(0.25, 6);
    expect(toneAt({ brightness: 1 }, 0.25)).toBeCloseTo(0.5, 6);
    expect([toneAt({ brightness: 1 }, 0), toneAt({ brightness: 1 }, 1)]).toEqual([0, 1]);
  });

  test("contrast turns about the middle grey; highlights and shadows each move their own end", () => {
    expect(toneAt({ contrast: 1 }, 0.5)).toBeCloseTo(0.5, 6);
    expect(toneAt({ contrast: 1 }, 0.75)).toBeCloseTo(1, 6);
    expect(toneAt({ contrast: -1 }, 1)).toBeLessThan(0.7);
    // highlights moves the bright more than the dark, shadows the dark more than the bright
    expect(toneAt({ highlights: 1 }, 0.75) - 0.75).toBeGreaterThan(toneAt({ highlights: 1 }, 0.25) - 0.25);
    expect(toneAt({ shadows: 1 }, 0.25) - 0.25).toBeGreaterThan(toneAt({ shadows: 1 }, 0.75) - 0.75);
    expect(toneAt({ highlights: -1 }, 0.8)).toBeLessThan(0.8);
  });

  test("fade lifts the blacks and lowers the whites", () => {
    expect(toneAt({ fade: 1 }, 0)).toBeCloseTo(0.22, 6);
    expect(toneAt({ fade: 1 }, 1)).toBeCloseTo(0.9, 6);
  });
});

describe("the filter's steps", () => {
  test("only what is changed: a tone table, a saturation, a warmth, a sharpen, a grain", () => {
    expect(adjustSteps({ contrast: 0.3 }).map((s) => s.tag)).toEqual(["feComponentTransfer"]);
    expect(adjustSteps({ saturation: -1 })).toEqual([{ tag: "feColorMatrix", attrs: { type: "saturate", values: "0" } }]);
    expect(adjustSteps({ temperature: 1 })[0].attrs.values).toBe("1.12 0 0 0 0 0 1 0 0 0 0 0 0.88 0 0 0 0 0 1 0");
    expect(adjustSteps({ sharpen: 1 })[0].attrs.kernelMatrix).toBe("0 -0.4 0 -0.4 2.6 -0.4 0 -0.4 0");
    // the tone table: 17 levels each for red, green and blue, alpha left as it is
    const table = adjustSteps({ exposure: 1 })[0];
    expect(table.children?.map((c) => c.tag)).toEqual(["feFuncR", "feFuncG", "feFuncB"]);
    expect(table.children?.[0].attrs.tableValues.split(" ")).toHaveLength(17);
  });

  test("grain is a noise added about its middle, kept to where the part is, its seed the frame", () => {
    const steps = adjustSteps({ contrast: 0.2, grain: 0.5 }, 42);
    expect(steps.map((s) => s.tag)).toEqual(["feComponentTransfer", "feTurbulence", "feColorMatrix", "feComposite", "feComposite"]);
    expect(steps[0].attrs.result).toBe("toned");
    expect(steps[1].attrs.seed).toBe("42");
    expect(steps[3].attrs).toMatchObject({ in: "toned", in2: "grey", operator: "arithmetic", k3: "0.15", k4: "-0.075" });
    expect(steps[4].attrs).toEqual({ in: "grained", in2: "SourceGraphic", operator: "in" });
    expect(adjustId({ grain: 0.5 }, 7)).not.toBe(adjustId({ grain: 0.5 }, 8));
    expect(adjustId({ contrast: 0.5 }, 7)).toBe(adjustId({ contrast: 0.5 }, 8));
  });

  test("a vignette is a darkening over the frame, none at 0", () => {
    expect(vignetteStyle(0)).toBeUndefined();
    expect(vignetteStyle(1)?.background).toBe("radial-gradient(ellipse at center, rgba(0, 0, 0, 0) 35%, rgba(0, 0, 0, 0.85) 100%)");
  });

  test("what is wrong is said", () => {
    expect(adjustError({ glow: 1 } as never)).toMatch(/"glow" isn't one/);
    expect(adjustError({ sharpen: -0.5 })).toMatch(/sharpen is from 0 to 1/);
    expect(adjustError({ contrast: 2 })).toMatch(/contrast is from -1 to 1/);
    expect(adjustError({ contrast: -1, vignette: 1 })).toBeUndefined();
  });
});

describe("<Nudge adjust>", () => {
  test("the part wears the adjustment's filter, after its key, with the filter beside it", () => {
    const html = at(<Nudge adjust={{ contrast: 0.4, saturation: 0.2 }} chroma={{ color: "#00ff00" }}><div>hi</div></Nudge>);
    const id = adjustId({ contrast: 0.4, saturation: 0.2 });
    expect(html).toContain(`filter:url(#raea-key-00ff00-50-20) url(#${id})`);
    expect(html).toContain(`<filter id="${id}"`);
    expect(html).toContain('type="table"');
    expect(html).toContain('type="saturate" values="1.2"');
  });

  test("a vignette is a layer over the part's frame", () => {
    const html = at(<Nudge adjust={{ vignette: 0.5 }}><div>hi</div></Nudge>);
    expect(html).toContain("radial-gradient(ellipse at center, rgba(0, 0, 0, 0) 50%, rgba(0, 0, 0, 0.425) 100%)");
    // a vignette alone needs no filter
    expect(html).not.toContain("<filter");
  });

  test("keyed: an adjustment's number over time, from as shot, without a still adjust", () => {
    const el = (frame: number) => at(<Nudge keys={{ "adjust.saturation": [[0, 0], [10, -1]] }}><div>hi</div></Nudge>, frame);
    expect(el(0)).not.toContain("<filter");
    expect(el(5)).toContain('type="saturate" values="0.5"');
    expect(el(10)).toContain('type="saturate" values="0"');
  });

  test("an adjustment out of range fails", () => {
    expect(() => at(<Nudge adjust={{ grain: 2 }}><div>hi</div></Nudge>)).toThrow(/grain is from 0 to 1/);
  });
});
