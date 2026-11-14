// <Transition>: how a part comes in and goes out, as the timeline writes a transition on a cut
// (docs/timeline.feature TL7a).
import { describe, expect, test } from "bun:test";
import { COMBOS, comboStyle, joinStyles, Transition, transitionAt, transitionStyle } from "@raea/video/transition";
import { Sequence } from "../src";
import { renderFrame } from "../src/test";

const opts = { fps: 30, width: 1920, height: 1080, durationInFrames: 300 };

describe("a part comes in over its first frames and goes out over its last", () => {
  test("how far into view it is: 0 before, 1 once in, counted in the part's own frames", () => {
    const edge = { kind: "fade" as const, frames: 10 };
    expect([0, 4, 9, 50].map((f) => transitionAt(edge, "in", f, 60))).toEqual([0.1, 0.5, 1, 1]);
    expect([0, 49, 50, 55, 59].map((f) => transitionAt(edge, "out", f, 60))).toEqual([1, 1, 1, 0.5, 0.1]);
    expect(transitionAt(undefined, "in", 0, 60)).toBe(1);
    expect(transitionAt({ kind: "fade", frames: 0 }, "in", 0, 60)).toBe(1);
  });

  test("each kind's look: a fade's opacity, a slide's move, a wipe's cover, a zoom's size; nothing once it is in", () => {
    expect(transitionStyle("fade", "in", 0.5)).toEqual({ opacity: 0.5 });
    expect(transitionStyle("slide", "in", 0)).toEqual({ transform: "translateX(100%)" });
    expect(transitionStyle("slide", "out", 0)).toEqual({ transform: "translateX(-100%)" });
    expect(transitionStyle("wipe", "in", 0)).toEqual({ clipPath: "inset(0 100% 0 0)" });
    expect(transitionStyle("zoom", "in", 0)).toEqual({ transform: "scale(0.85)", opacity: 0 });
    expect(transitionStyle("slide", "in", 1)).toEqual({});
  });

  test("in a <Sequence> it plays at the sequence's ends and leaves the middle as it was", () => {
    const at = (frame: number) => renderFrame(<Sequence from={100} durationInFrames={60}><Transition in={{ kind: "fade", frames: 10 }} out={{ kind: "slide", frames: 10 }}><p>hi</p></Transition></Sequence>, { ...opts, frame });
    expect(at(104)).toContain("opacity:0.5");
    expect(at(130)).not.toContain("opacity");
    expect(at(130)).not.toContain("translateX");
    expect(at(159)).toContain("translateX(-81%)");
    expect(() => renderFrame(<Transition in={{ kind: "spin" as never, frames: 10 }}>x</Transition>, { ...opts, frame: 0 })).toThrow(/kind is one of/);
  });
});

describe("a Combo moves the part over its whole length (TL7e)", () => {
  const pulse = COMBOS.find((c) => c.kind === "pulse");
  test("its wave eased between evenly spaced values, at rest where a cycle starts and ends, looping", () => {
    expect(comboStyle(pulse, 0, 60)).toEqual({ transform: "scale(1)" });
    expect(comboStyle(pulse, 30, 60)).toEqual({ transform: "scale(1.08)" });
    expect(comboStyle(pulse, 15, 60)).toEqual({ transform: "scale(1.04)" });
    expect(comboStyle(pulse, 90, 60)).toEqual(comboStyle(pulse, 30, 60));
    // a spin keeps turning at one pace
    expect(comboStyle(COMBOS.find((c) => c.kind === "spin"), 15, 60)).toEqual({ transform: "rotate(90deg)" });
    expect(comboStyle(undefined, 15, 60)).toEqual({});
  });

  test("in a <Sequence> with its In and Out: the moves side by side, the fades multiplied", () => {
    const at = (frame: number) => renderFrame(<Sequence from={0} durationInFrames={60}><Transition in={{ kind: "fade", frames: 10 }} combo={{ kind: "pulse", frames: 60 }}><p>hi</p></Transition></Sequence>, { ...opts, frame });
    expect(at(4)).toContain("opacity:0.5");
    expect(at(30)).toContain("scale(1.08)");
    expect(joinStyles({ transform: "a", opacity: 0.5 }, { transform: "b", opacity: 0.5, clipPath: "c" })).toEqual({ transform: "a b", opacity: 0.25, clipPath: "c" });
    expect(() => renderFrame(<Transition combo={{ kind: "wobble", frames: 10 }}>x</Transition>, { ...opts, frame: 0 })).toThrow(/combo.kind is one of/);
  });
});
