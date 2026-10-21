// Declared animation (<Animate>, moves, tracks, timelines): what crosses to the engine, and the same tracks sampled
// here under plain bun. The engine samples them in Rust (engine/animate/src/spec.rs); these expected values are the ones its
// own tests check, so the two agree.
import { describe, expect, test } from "bun:test";
import {
  Animate, Sequence, Stagger, curveFn, fade, gridDelay, parallel, pop, reverse, rise, sampleAnimation, sequence, stagger, timeline,
  tick, track, type Animation,
} from "../src";
import { renderFrame } from "../src/test";
import { useContext } from "react";
import { TimeBaseContext, type TimeBase } from "../src/clock";
import { propsFingerprint, Freeze } from "../src";

const opts = { fps: 30, width: 1080, height: 1920, durationInFrames: 300 };
const anim = (html: string): Animation => JSON.parse(/data-raea-anim="([^"]*)"/.exec(html)![1].replace(/&quot;/g, '"').replace(/&amp;/g, "&"));

describe("tracks and moves", () => {
  test("track sorts keys and takes one curve or one per segment", () => {
    expect(track({ 6: 0, 0: 30 }).keys).toEqual([[0, 30], [6, 0]]);
    expect(() => track({ 0: 0, 4: 1, 8: 2 }, ["out-cubic"])).toThrow(/3 keys take 2 curves/);
  });
  test("the named moves keep the measured timings", () => {
    expect(rise().tracks.y).toEqual(track({ 0: 30, 6: 0 }));
    expect(pop().tracks.scale).toEqual(track({ 0: 0.6, 4: 1.08, 8: 1 }));
    expect(fade().frames).toBe(6);
  });
  test("reverse leaves the way it came, with reversed curves", () => {
    const r = reverse(rise());
    expect((r.tracks.y as { keys: unknown }).keys).toEqual([[0, 0], [6, 30]]);
    expect((r.tracks.y as { ease: unknown }).ease).toEqual(["reverse(out-cubic)"]);
    expect(curveFn("reverse(out-cubic)")(0.5)).toBeCloseTo(curveFn("in-cubic")(0.5), 12);
  });
});

describe("timelines", () => {
  test("sequence, parallel and stagger place each part once", () => {
    const plan = timeline(sequence(["card", rise({ frames: 10 })], parallel(["title", pop()], { part: "cta", move: fade(), offset: 6 })), stagger(["a", "b", "c"], 3, "rise"));
    expect(plan.parts.card.at).toBe(0);
    expect(plan.parts.title.at).toBe(10);
    expect(plan.parts.cta.at).toBe(16);
    expect(plan.parts.c.at).toBe(22 + 6);
    expect(() => timeline(["x", "rise"], ["x", "pop"])).toThrow(/placed twice/);
  });
  test("a grid staggers by distance from its origin (raea-animate's rule)", () => {
    expect([0, 1, 2, 3, 4, 5, 6, 7, 8].map((i) => gridDelay(3, 3, "center", i))).toEqual([2, 1, 2, 1, 0, 1, 2, 1, 2]);
  });
});

describe("sampling (the engine's rules)", () => {
  const a = (tracks: Animation["tracks"], origin = 0): Animation => ({ origin, freeze: null, fps: 30, tracks });
  test("a rise at composition frames", () => {
    const r = a([{ prop: "y", keys: [{ at: 0, value: 30, color: "", ease: "out-cubic" }, { at: 6, value: 0, color: "", ease: "" }], unit: "", repeat: "", digits: 0 }], 100);
    expect(sampleAnimation(r, 100).style.translate).toBe("0px 30px");
    expect(sampleAnimation(r, 103).style.translate).toBe("0px 3.75px");
    expect(sampleAnimation(r, 500).style.translate).toBe("0px 0px");
  });
  test("an entrance and an exit compose (opacity multiplies, x adds)", () => {
    const k = (at: number, value: number, ease = "") => ({ at, value, color: "", ease });
    const t = (prop: string, keys: ReturnType<typeof k>[]) => ({ prop, keys, unit: "", repeat: "", digits: 0 });
    const s = a([t("opacity", [k(0, 0, "out-cubic"), k(8, 1)]), t("opacity", [k(40, 1, "in-cubic"), k(48, 0)]), t("x", [k(0, -64, "out-cubic"), k(8, 0)]), t("x", [k(40, 0, "in-cubic"), k(48, -64)])]);
    expect(sampleAnimation(s, 20).style.opacity).toBe(1);
    expect(sampleAnimation(s, 48).style).toMatchObject({ opacity: 0, translate: "-64px 0px" });
  });
  test("a counter writes its text", () => {
    const t = tick({ to: 20, frames: 24 }).tracks.text as { keys: [number, number][] };
    expect(t.keys).toEqual([[0, 0], [24, 20]]);
  });
});

describe("<Animate>", () => {
  test("sends its tracks once, at the Sequence's origin, and is sampled here under bun", () => {
    const at = (frame: number) => renderFrame(<Sequence from={50}><Animate enter="rise" at={4}>hi</Animate></Sequence>, { ...opts, frame });
    const html = at(57);
    const w = anim(html);
    expect(w.origin).toBe(50);
    expect(w.tracks.map((t) => t.prop).sort()).toEqual(["opacity", "y"]);
    expect(w.tracks.find((t) => t.prop === "y")!.keys[0].at).toBe(4);
    // the same declaration on every frame (React sends it once)
    expect(anim(at(80))).toEqual(w);
    expect(html).toContain("translate:0px 3.75px");
  });
  test("an exit needs exitAt; mount=during leaves the tree after it", () => {
    expect(() => renderFrame(<Animate exit="fade">x</Animate>, { ...opts, frame: 0 })).toThrow(/exitAt/);
    expect(renderFrame(<Animate enter="fade" exit="fade" exitAt={20} mount="during">x</Animate>, { ...opts, frame: 30 })).toBe("");
  });
  test("Stagger spaces its children's entrances", () => {
    const html = renderFrame(<Stagger each={3} enter="rise">{["a", "b", "c"].map((x) => <span key={x}>{x}</span>)}</Stagger>, { ...opts, frame: 0 });
    const ats = [...html.matchAll(/data-raea-anim="([^"]*)"/g)].map((m) => JSON.parse(m[1].replace(/&quot;/g, '"')).tracks[0].keys[0].at);
    expect(ats).toEqual([0, 3, 6]);
  });
});

describe("declarations that don't change frame to frame", () => {
  test("a Sequence's TimeBase is the same object on every frame, and a Freeze's holds its frame", () => {
    const seen: TimeBase[] = [];
    function Probe() { seen.push(useContext(TimeBaseContext)); return null; }
    for (const frame of [60, 61, 90]) renderFrame(<Sequence from={50}><Sequence from={5}><Probe /></Sequence></Sequence>, { ...opts, frame });
    expect(seen[0]).toBe(seen[1]);
    expect(seen[1]).toBe(seen[2]);
    expect(seen[0]).toEqual({ origin: 55, freeze: null });
    seen.length = 0;
    renderFrame(<Freeze frame={20}><Sequence from={5}><Probe /></Sequence></Freeze>, { ...opts, frame: 200 });
    expect(seen[0]).toEqual({ origin: 5, freeze: 15 });
  });
  test("the fingerprint is content: new objects with the same content match, a change doesn't", () => {
    const a = { enter: rise({ distance: 40 }), style: { color: "red" }, children: <span key="x">hi</span> };
    const b = { enter: rise({ distance: 40 }), style: { color: "red" }, children: <span key="x">hi</span> };
    expect(propsFingerprint(a)).toBe(propsFingerprint(b));
    expect(propsFingerprint(a)).not.toBe(propsFingerprint({ ...b, style: { color: "blue" } }));
    expect(propsFingerprint(a)).not.toBe(propsFingerprint({ ...b, children: <span key="x">ho</span> }));
  });
  test("mount=during sends its show window (the engine hides it outside)", () => {
    const w = anim(renderFrame(<Animate enter="fade" at={10} exit="fade" exitAt={40} mount="during">x</Animate>, { ...opts, frame: 12 }));
    expect([w.show_from, w.show_until]).toEqual([10, 47]);
  });
});
