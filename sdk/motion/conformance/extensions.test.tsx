// The extensions, from spec/motion.feature ("Placement from perception", "The proof", "Generated assets as values",
// "3D in the same tree", "Toon surfaces and ink outlines", "The camera's lens", "Motion blur", "Page layers under and
// over the 3D", "Games: state from input, frame by frame").
import { beforeEach, describe, expect, test } from "bun:test";
import { Audio, Region, lyria, proofBoxes, sao, useGameEvents, useGameState, useInput, useRegion } from "../src";
import { Box, Canvas3D, Cylinder, DirectionalLight, Fisheye, LensDistortion, Mesh, MeshStandardMaterial, MeshToonMaterial, Model, MotionBlur, Plane, PointLight, Sphere } from "../src/3d";
import { renderFrame } from "../src/test";

let got: any;
beforeEach(() => { got = undefined; });
function Ask({ f }: { f: () => any }) { got = f(); return null; }
const base = { fps: 30, width: 1080, height: 1920, durationInFrames: 300 };
const face = { x: 300, y: 500, w: 480, h: 480 };
const near = (a: number, b: number) => expect(Math.abs(a - b)).toBeLessThan(1e-6);

describe("Region", () => {
  test("auto takes the taller band clear of the face", () => {
    renderFrame(<Region><Ask f={() => useRegion()} /></Region>, { ...base, frame: 0, perception: { face } });
    // m = 65; top band 153.6..435, bottom band 1045..1536
    expect(got.name).toBe("auto");
    expect(got.x).toBe(65); expect(got.w).toBe(950);
    near(got.y, 1045); near(got.h, 491);
  });
  test("top and center by name", () => {
    renderFrame(<Region name="top"><Ask f={() => useRegion()} /></Region>, { ...base, frame: 0, perception: { face } });
    near(got.y, 153.6); near(got.h, 435 - 153.6);
    renderFrame(<Region name="center"><Ask f={() => useRegion()} /></Region>, { ...base, frame: 0, perception: { face } });
    near(got.y, 0.33 * 1920); near(got.h, 0.34 * 1920);
  });
  test("without a face: top is 0.08 H to 0.45 H, and auto picks it", () => {
    renderFrame(<Region><Ask f={() => useRegion()} /></Region>, { ...base, frame: 0 });
    near(got.y, 153.6); near(got.h, 864 - 153.6);
  });
  test("the safe area moves the edges", () => {
    renderFrame(<Region name="top"><Ask f={() => useRegion()} /></Region>, { ...base, frame: 0, perception: { safe: { top: 200, bottom: 1500 } } });
    near(got.y, 200); near(got.h, 664);
  });
  test("no room renders nothing and says why", () => {
    const big = { x: 100, y: 100, w: 880, h: 1500 };
    const html = renderFrame(<Region name="top"><Ask f={() => useRegion()} /></Region>, { ...base, frame: 0, perception: { face: big } });
    expect(got).toBeUndefined();
    expect(html).toContain("data-refused");
    expect(html).toContain("no room top");
  });
  test("its box carries the band and the face as CSS properties", () => {
    const html = renderFrame(<Region name="bottom" />, { ...base, frame: 0, perception: { face } });
    expect(html).toContain('data-region="bottom"');
    expect(html).toContain("--face-top:500px");
    expect(html).toContain("--region-height:491px");
  });
});

describe("proofBoxes", () => {
  const frame = { width: 1080, height: 1920, face };
  test("clean parts have no faults", () => {
    expect(proofBoxes({ ...frame, parts: [{ name: "a", x: 65, y: 1045, w: 950, h: 300, band: [1045, 1536] }] })).toEqual([]);
  });
  test("taller than its band", () => {
    const f = proofBoxes({ ...frame, parts: [{ name: "b", x: 65, y: 1045, w: 950, h: 600, band: [1045, 1536] }] });
    expect(f.length).toBe(1); expect(f[0]).toContain("b is 600 px tall and its room is 491 px");
  });
  test("off the frame", () => {
    const f = proofBoxes({ ...frame, parts: [{ name: "c", x: -10, y: 100, w: 200, h: 100 }] });
    expect(f[0]).toContain("c runs off the frame");
  });
  test("over the face", () => {
    const f = proofBoxes({ ...frame, parts: [{ name: "d", x: 300, y: 500, w: 480, h: 240 }] });
    expect(f[0]).toContain("d covers 50% of the face");
  });
  test("under 8% of the face is fine", () => {
    expect(proofBoxes({ ...frame, parts: [{ name: "e", x: 300, y: 950, w: 480, h: 30 }] })).toEqual([]);
  });
});

describe("generated assets", () => {
  test("lyria and sao are stable descriptors", () => {
    const a = lyria("warm bed"), b = lyria("warm bed"), c = lyria("cold bed");
    expect(a.kind).toBe("lyria"); expect(a.seconds).toBe(30);
    expect(a.id).toBe(b.id); expect(a.id).not.toBe(c.id);
    expect(a.src).toBe(`asset:lyria/${a.id}`);
    expect(sao("whoosh").seconds).toBe(2);
    expect(lyria("warm bed", { seconds: 12 }).id).not.toBe(a.id);
  });
  test("an Audio given one carries what to generate", () => {
    const html = renderFrame(<Audio src={lyria("warm bed")} />, { ...base, frame: 0 });
    expect(html).toContain('data-generate="lyria"');
    expect(html).toContain('data-prompt="warm bed"');
    expect(html).toContain('data-seconds="30"');
  });
});

describe("3D", () => {
  test("a mesh carries its Manifold shape and props", () => {
    const html = renderFrame(
      <Canvas3D><Mesh position={[1, 2, 3]}><Box args={[1, 2, 3]} /><MeshStandardMaterial color="#ff0000" /></Mesh></Canvas3D>,
      { ...base, frame: 0 });
    expect(html).toContain('data-3d="Canvas3D"');
    expect(html).toContain('data-3d="Mesh"');
    expect(html).toContain('data-shape="box(1, 2, 3)"');
    expect(html).toContain("&quot;position&quot;:[1,2,3]");
  });
  test("other geometries and an explicit shape", () => {
    const shape = (node: any) => /data-shape="([^"]*)"/.exec(renderFrame(node, { ...base, frame: 0 }))?.[1];
    expect(shape(<Mesh><Plane args={[4, 2]} /></Mesh>)).toBe("box(4, 2, 0)");
    expect(shape(<Mesh><Cylinder args={[1, 0.5, 2]} /></Mesh>)).toBe("rotate(cone(0.5, 1, 2), -90, 0, 0)");
    expect(shape(<Mesh shape="sphere(2)"><Box args={[1, 1, 1]} /></Mesh>)).toBe("sphere(2)");
  });
});

describe("3D toon", () => {
  const props = (node: any, kind = "MeshToonMaterial") => {
    const html = renderFrame(node, { ...base, frame: 0 });
    const m = new RegExp(`data-3d="${kind}" data-props="([^"]*)"`).exec(html);
    return JSON.parse((m?.[1] ?? "{}").replace(/&quot;/g, '"'));
  };
  test("MeshToonMaterial carries its bands, rim, highlight and outline, colours as hex", () => {
    const p = props(<Mesh><Sphere args={[1]} /><MeshToonMaterial color="yellow" steps={3} shadowColor="rgb(120, 60, 20)" rim outline={{ width: 3, color: "black" }} highlight={{ size: 0.1 }} /></Mesh>);
    expect(p).toEqual({ color: "#ffff00", steps: 3, shadowColor: "#783c14", rim: {}, outline: { width: 3, color: "#000000" }, highlight: { size: 0.1 } });
  });
  test("false or missing parts are left out; an explicit ramp is ordered darkest first", () => {
    const p = props(<MeshToonMaterial color="#ffd23f" rim={false} bands={[{ at: 0, color: "#40205a" }, { at: 0.4 }, { at: 0.8, color: "#fff" }]} />);
    expect(p.rim).toBeUndefined();
    expect(p.bands).toEqual([{ at: 0, color: "#40205a" }, { at: 0.4 }, { at: 0.8, color: "#ffffff" }]);
    expect(() => props(<MeshToonMaterial bands={[{ at: 0.5 }, { at: 0.2 }]} />)).toThrow(/in order/);
    expect(() => props(<MeshToonMaterial steps={0} />)).toThrow(/steps/);
    expect(() => props(<MeshToonMaterial outline={{ width: 100 }} />)).toThrow(/pixels/);
    expect(() => props(<MeshToonMaterial rim={{ width: 2 }} />)).toThrow(/rim.width/);
  });
  test("a Model passes its toon materials through", () => {
    const html = renderFrame(<Model src="/media/nana.glb"><MeshToonMaterial material="Peel" outline /></Model>, { ...base, frame: 0 });
    expect(html).toContain('data-3d="Model"');
    expect(html).toContain('data-3d="MeshToonMaterial"');
    expect(html).toContain("&quot;material&quot;:&quot;Peel&quot;");
  });
});

describe("3D camera effects", () => {
  const props = (node: any, kind: string) => {
    const html = renderFrame(node, { ...base, frame: 0 });
    const m = new RegExp(`data-3d="${kind}" data-props="([^"]*)"`).exec(html);
    return JSON.parse((m?.[1] ?? "{}").replace(/&quot;/g, '"'));
  };
  test("Fisheye and LensDistortion carry their lens; bad values throw", () => {
    expect(props(<Fisheye strength={0.5} fov={120} mode="stereographic" />, "Fisheye")).toEqual({ strength: 0.5, fov: 120, mode: "stereographic" });
    expect(props(<LensDistortion k1={0.2} k2={-0.05} edges="black" />, "LensDistortion")).toEqual({ k1: 0.2, k2: -0.05, edges: "black" });
    expect(() => props(<Fisheye strength={2} />, "Fisheye")).toThrow(/strength/);
    expect(() => props(<Fisheye fov={200} />, "Fisheye")).toThrow(/fov/);
    expect(() => props(<LensDistortion zoom={0} />, "LensDistortion")).toThrow(/zoom/);
    expect(() => props(<Fisheye edges={"wrap" as any} />, "Fisheye")).toThrow(/edges/);
  });
  test("MotionBlur carries its shutter and samples; bad values throw", () => {
    expect(props(<MotionBlur shutterAngle={0.5} samples={6} />, "MotionBlur")).toEqual({ shutterAngle: 0.5, samples: 6 });
    expect(() => props(<MotionBlur samples={0} />, "MotionBlur")).toThrow(/samples/);
    expect(() => props(<MotionBlur shutterAngle={5} />, "MotionBlur")).toThrow(/shutterAngle/);
  });
  test("lights carry soft shadows: true or a size", () => {
    expect(props(<DirectionalLight softShadow={10} />, "DirectionalLight").softShadow).toBe(10);
    expect(props(<PointLight softShadow castShadow />, "PointLight").softShadow).toBe(true);
    expect(() => props(<DirectionalLight softShadow={-1} />, "DirectionalLight")).toThrow(/softShadow/);
  });
});

describe("3D page layers", () => {
  test("a layered Canvas3D fills the frame and holds the 3D's slot first; a plain one doesn't", () => {
    const html = renderFrame(<Canvas3D layered><Box /></Canvas3D>, { ...base, frame: 0 });
    expect(html).toMatch(/data-3d="Canvas3D"[^>]*style="position:absolute;left:0;top:0;width:100%;height:100%"><img data-raea-scene=""/);
    expect(renderFrame(<Canvas3D><Box /></Canvas3D>, { ...base, frame: 0 })).not.toContain("data-raea-scene");
  });
});

describe("games", () => {
  const input = { 2: { keys: ["ArrowRight"] }, 3: { keys: ["ArrowRight"] }, 5: { keys: ["ArrowLeft"] } };
  const step = (s: number, i: { keys: string[] }) => s + (i.keys.includes("ArrowRight") ? 1 : 0) - (i.keys.includes("ArrowLeft") ? 1 : 0);
  const at = (frame: number) => { renderFrame(<Ask f={() => useGameState(0, step)} />, { ...base, frame, input }); return got; };
  test("the state folds the earlier frames' input", () => {
    expect(at(0)).toBe(0);
    expect(at(3)).toBe(1);
    expect(at(4)).toBe(2);
    expect(at(6)).toBe(1);
  });
  test("useInput is this frame's", () => {
    renderFrame(<Ask f={() => useInput()} />, { ...base, frame: 2, input });
    expect(got.keys).toEqual(["ArrowRight"]);
    renderFrame(<Ask f={() => useInput()} />, { ...base, frame: 1, input });
    expect(got.keys).toEqual([]);
  });
});

describe("game events", () => {
  const input = { 2: { keys: ["ArrowRight"] }, 3: { keys: ["ArrowRight"] }, 5: { keys: ["ArrowLeft"] }, 9: { keys: ["ArrowRight"] } };
  const step = (s: number, i: { keys: string[] }) => s + (i.keys.includes("ArrowRight") ? 1 : 0) - (i.keys.includes("ArrowLeft") ? 1 : 0);
  test("the frames where the state goes up", () => {
    renderFrame(<Ask f={() => useGameEvents(0, step, (a: number, b: number) => b > a)} />, { ...base, frame: 0, durationInFrames: 12, input });
    // step at frame g makes the state of frame g + 1
    expect(got).toEqual([3, 4, 10]);
  });
  test("an event that never happens gives no frames", () => {
    renderFrame(<Ask f={() => useGameEvents(0, step, (a: number, b: number) => b > a + 5)} />, { ...base, frame: 0, durationInFrames: 12, input });
    expect(got).toEqual([]);
  });
});

describe("game state is kept, not replayed", () => {
  test("rendering every frame in order costs linear steps", () => {
    let calls = 0;
    const step = (s: number, i: { keys: string[] }) => { calls++; return s + (i.keys.includes("ArrowRight") ? 1 : 0); };
    const input: any = {};
    for (let f = 0; f < 600; f += 7) input[f] = { keys: ["ArrowRight"] };
    let last = -1;
    for (let frame = 0; frame < 600; frame++) {
      renderFrame(<Ask f={() => useGameState(0, step)} />, { ...base, frame, durationInFrames: 600, input });
      expect(got).toBeGreaterThanOrEqual(last);
      last = got;
    }
    expect(last).toBe(Math.ceil(599 / 7));
    expect(calls).toBeLessThan(2000);
  });
  test("useGameEvents over a long game is linear too", () => {
    let calls = 0;
    const step = (s: number) => { calls++; return s + 1; };
    for (let frame = 0; frame < 300; frame++) {
      renderFrame(<Ask f={() => useGameEvents(0, step, (a: number, b: number) => b % 50 === 0)} />, { ...base, frame, durationInFrames: 300 });
    }
    expect(got).toEqual([50, 100, 150, 200, 250]);
    expect(calls).toBeLessThan(3000);
  });
});

describe("live play: input recorded as it's played", () => {
  test("a timeline growing by a frame each frame costs linear steps, and events stop at this frame", () => {
    let calls = 0;
    const step = (s: number, i: { keys: string[] }) => { calls++; return s + (i.keys.includes("ArrowRight") ? 1 : 0); };
    const input: any = {};
    const press = (a: number, b: number) => b > a;
    for (let frame = 1; frame < 1200; frame++) {
      input[frame] = { keys: frame % 5 === 0 ? ["ArrowRight"] : [], live: true };
      const timeline = { ...input }; // the player passes a fresh copy each frame
      renderFrame(<Ask f={() => [useGameState(0, step), useGameEvents(0, step, press), useGameEvents(0, step, (a: number, b: number) => b > a)]} />, { ...base, frame, durationInFrames: 1_000_000_000, input: timeline });
      expect(Math.max(0, ...got[1])).toBeLessThanOrEqual(frame);
    }
    expect(got[0]).toBe(Math.floor(1198 / 5));
    expect(got[1].length).toBe(Math.floor(1198 / 5));
    expect(calls).toBeLessThan(3000);
  });
  test("an input that changes under the fold refolds from there, and matches a fresh fold", () => {
    const step = (s: number, i: { keys: string[] }) => s * 2 + (i.keys.includes("x") ? 1 : 0);
    const input: any = { 3: { keys: ["x"] }, 7: { keys: ["x"] } };
    renderFrame(<Ask f={() => useGameState(0, step)} />, { ...base, frame: 10, input: { ...input } });
    const before = got;
    input[5] = { keys: ["x"] };
    renderFrame(<Ask f={() => useGameState(0, step)} />, { ...base, frame: 10, input: { ...input } });
    let s = 0;
    for (let g = 0; g < 10; g++) s = step(s, { keys: input[g]?.keys ?? [] });
    expect(got).toBe(s);
    expect(got).not.toBe(before);
  });
});

test("an endless composition never folds toward its end, even before any input marks it live", () => {
  let calls = 0;
  const step = (s: number) => { calls++; return s + 1; };
  renderFrame(<Ask f={() => useGameEvents(0, step, (a: number, b: number) => b % 10 === 0)} />, { ...base, frame: 0, durationInFrames: 1_000_000_000 });
  expect(got).toEqual([]);
  expect(calls).toBeLessThan(10);
});

test("a long game keeps its memory flat, and an old frame still equals the fold", () => {
  const step = (s: { n: number; trail: number[] }, i: { keys: string[] }) => ({ n: s.n + (i.keys.includes("x") ? 2 : 1), trail: [...s.trail.slice(-50), s.n] });
  const input: any = {};
  for (let f = 0; f < 5000; f += 3) input[f] = { keys: ["x"] };
  const init = { n: 0, trail: [] as number[] };
  renderFrame(<Ask f={() => useGameState(init, step)} />, { ...base, frame: 4999, durationInFrames: 6000, input });
  const late = got.n;
  renderFrame(<Ask f={() => useGameState(init, step)} />, { ...base, frame: 1234, durationInFrames: 6000, input });
  let s = init;
  for (let g = 0; g < 1234; g++) s = step(s, { keys: input[g]?.keys ?? [] });
  expect(got).toEqual(s);
  s = init;
  for (let g = 0; g < 4999; g++) s = step(s, { keys: input[g]?.keys ?? [] });
  expect(late).toBe(s.n);
});

test("an input changed far back refolds from a let-go state correctly", () => {
  const step = (s: number, i: { keys: string[] }, g: number) => s * 3 + (i.keys.includes("x") ? g : 1);
  const input: any = {};
  for (let f = 0; f < 3000; f += 7) input[f] = { keys: ["x"] };
  renderFrame(<Ask f={() => useGameState(0, step)} />, { ...base, frame: 2999, durationInFrames: 4000, input: { ...input } });
  input[1001] = { keys: ["x"] };
  renderFrame(<Ask f={() => useGameState(0, step)} />, { ...base, frame: 2999, durationInFrames: 4000, input: { ...input } });
  let s = 0;
  for (let g = 0; g < 2999; g++) s = step(s, { keys: input[g]?.keys ?? [] }, g);
  expect(got).toBe(s);
});
