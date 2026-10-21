// Sequence, Series, Loop, Freeze, useCurrentFrame and useVideoConfig, from spec/motion.feature ("A composition is a
// component rendered once per frame", "Sequence shifts time and limits it"), through renderFrame (the test clock).
import { beforeEach, describe, expect, test } from "bun:test";
import { AbsoluteFill, Freeze, Loop, Sequence, Series, useCurrentFrame, useVideoConfig } from "../src";
import { renderFrame } from "../src/test";

let seen: Record<string, number> = {};
beforeEach(() => { seen = {}; });
// records the frame it sees; renders nothing
function Probe({ id }: { id: string }) {
  seen[id] = useCurrentFrame();
  return null;
}
const at = (frame: number, node: any) => renderFrame(node, { frame, fps: 30, width: 1080, height: 1920, durationInFrames: 300 });

describe("useCurrentFrame and Sequence", () => {
  test("outside any Sequence it is the composition's frame", () => {
    at(42, <Probe id="a" />);
    expect(seen.a).toBe(42);
  });
  test("inside a Sequence it counts from its start", () => {
    at(45, <Sequence from={30}><Probe id="a" /></Sequence>);
    expect(seen.a).toBe(15);
  });
  test("children render only inside [from, from + duration)", () => {
    for (const [f, shown] of [[29, false], [30, true], [59, true], [60, false]] as const) {
      seen = {};
      at(f, <Sequence from={30} durationInFrames={30}><Probe id="a" /></Sequence>);
      expect("a" in seen).toBe(shown);
    }
  });
  test("offsets add through nesting", () => {
    at(95, <Sequence from={30}><Sequence from={60}><Probe id="a" /></Sequence></Sequence>);
    expect(seen.a).toBe(5);
    seen = {};
    at(89, <Sequence from={30}><Sequence from={60}><Probe id="a" /></Sequence></Sequence>);
    expect("a" in seen).toBe(false);
  });
  test("a negative from starts partway", () => {
    at(0, <Sequence from={-20}><Probe id="a" /></Sequence>);
    expect(seen.a).toBe(20);
  });
  test("an inner duration is limited by the outer one", () => {
    at(50, <Sequence from={0} durationInFrames={40}><Sequence from={10}><Probe id="a" /></Sequence></Sequence>);
    expect("a" in seen).toBe(false);
  });
});

describe("Series", () => {
  const series = (
    <Series>
      <Series.Sequence durationInFrames={30}><Probe id="a" /></Series.Sequence>
      <Series.Sequence durationInFrames={20}><Probe id="b" /></Series.Sequence>
      <Series.Sequence durationInFrames={10} offset={5}><Probe id="c" /></Series.Sequence>
      <Series.Sequence durationInFrames={10} offset={-4}><Probe id="d" /></Series.Sequence>
    </Series>
  );
  test("back to back", () => {
    at(29, series); expect(seen).toEqual({ a: 29 });
    seen = {}; at(30, series); expect(seen).toEqual({ b: 0 });
  });
  test("a positive offset leaves a gap", () => {
    at(52, series); expect(seen).toEqual({});
    seen = {}; at(55, series); expect(seen.c).toBe(0);
  });
  test("a negative offset overlaps the one before", () => {
    // c runs 55..65; d starts at 65 - 4 = 61
    at(62, series); expect(seen).toEqual({ c: 7, d: 1 });
  });
});

describe("Loop and Freeze", () => {
  test("Loop repeats its children", () => {
    at(25, <Loop durationInFrames={10}><Probe id="a" /></Loop>);
    expect(seen.a).toBe(5);
  });
  test("Loop with times stops after them", () => {
    at(19, <Loop durationInFrames={10} times={2}><Probe id="a" /></Loop>);
    expect(seen.a).toBe(9);
    seen = {}; at(20, <Loop durationInFrames={10} times={2}><Probe id="a" /></Loop>);
    expect("a" in seen).toBe(false);
  });
  test("Freeze holds one frame", () => {
    at(200, <Freeze frame={12}><Probe id="a" /></Freeze>);
    expect(seen.a).toBe(12);
  });
});

describe("useVideoConfig and AbsoluteFill", () => {
  test("the composition's settings", () => {
    let cfg: any;
    function Read() { cfg = useVideoConfig(); return null; }
    at(0, <Read />);
    expect(cfg).toMatchObject({ width: 1080, height: 1920, fps: 30, durationInFrames: 300 });
  });
  test("a Sequence given width and height reports them to its children", () => {
    let cfg: any;
    function Read() { cfg = useVideoConfig(); return null; }
    at(0, <Sequence width={540} height={960}><Read /></Sequence>);
    expect(cfg).toMatchObject({ width: 540, height: 960 });
  });
  test("AbsoluteFill renders its children", () => {
    at(3, <AbsoluteFill style={{ background: "black" }}><Probe id="a" /></AbsoluteFill>);
    expect(seen.a).toBe(3);
  });
});
