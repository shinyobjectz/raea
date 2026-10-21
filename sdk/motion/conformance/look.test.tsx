// <Look mode="paint" | "cartoon"> (spec/motion.feature, "3D: the rest of a scene"): its props checked, its boil folded
// into the seed from the frame, and a cartoon's outline emitted beside it.
import { describe, expect, test } from "bun:test";
import { Canvas3D, Look, Outline } from "../src/3d";
import { renderFrame } from "../src/test";

const base = { fps: 24, width: 1080, height: 1920, durationInFrames: 48 };
const props = (html: string, kind: string) => {
  const m = html.match(new RegExp(`data-3d="${kind}" data-props="([^"]*)"`));
  return m ? JSON.parse(m[1].replace(/&quot;/g, '"')) : undefined;
};

describe("Look: paint and cartoon", () => {
  test("paint carries its bands, colours, sky and palette as hex", () => {
    const html = renderFrame(<Canvas3D><Look mode="paint" bands={4} shadow="rgb(10, 20, 30)" sky={["#123", "#456"]} palette={["red", "#00ff00"]} /></Canvas3D>, { ...base, frame: 0 });
    const p = props(html, "Look");
    expect(p.mode).toBe("paint");
    expect(p.bands).toBe(4);
    expect(p.shadow).toBe("#0a141e");
    expect(p.sky).toEqual(["#112233", "#445566"]);
    expect(p.palette).toEqual(["#ff0000", "#00ff00"]);
    expect(props(html, "Outline")).toBeUndefined();
  });

  test("a boil moves the seed on its beat, and only on it", () => {
    const seed = (frame: number) => props(renderFrame(<Canvas3D><Look mode="paint" boil={2} seed={5} /></Canvas3D>, { ...base, frame }), "Look").seed;
    expect([seed(0), seed(1), seed(2), seed(3), seed(4)]).toEqual([5, 5, 6, 6, 7]);
  });

  test("cartoon is paint with two bands, a boil on twos and a thick outline", () => {
    const html = renderFrame(<Canvas3D><Look mode="cartoon" outlineWidth={8} /></Canvas3D>, { ...base, frame: 6 });
    const p = props(html, "Look");
    expect(p.mode).toBe("paint");
    expect(p.bands).toBe(2);
    expect(p.seed).toBe(3);
    expect(props(html, "Outline")).toMatchObject({ color: "#2b1d14", width: 8 });
    expect(props(renderFrame(<Canvas3D><Look mode="cartoon" outline="none" /></Canvas3D>, { ...base, frame: 0 }), "Outline")).toBeUndefined();
  });

  test("bad values throw, naming the prop", () => {
    const r = (el: any) => () => renderFrame(<Canvas3D>{el}</Canvas3D>, { ...base, frame: 0 });
    expect(r(<Look mode="paint" bands={0} />)).toThrow(/bands/);
    expect(r(<Look mode="paint" bands={2.5} />)).toThrow(/bands/);
    expect(r(<Look mode="paint" grain={2} />)).toThrow(/grain/);
    expect(r(<Look mode="paint" rag={-1} />)).toThrow(/rag/);
    expect(r(<Look mode="paint" palette={[]} />)).toThrow(/palette/);
    expect(r(<Look mode="paint" sky={["#000"] as any} />)).toThrow(/sky/);
    expect(r(<Look mode="paint" shadow="var(--x)" />)).toThrow(/shadow/);
    expect(r(<Outline width={0} />)).toThrow(/width/);
  });
});
