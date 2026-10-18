// word() and useWords(), from spec/motion.feature ("Time anchored to words"), with renderFrame's `words` standing in
// for the timeline's transcript.
import { beforeEach, describe, expect, test } from "bun:test";
import { Sequence, useWords, word } from "../src";
import { renderFrame } from "../src/test";

const words = [
  { text: "So", from: 0, to: 3 }, { text: "according", from: 4, to: 14 }, { text: "to", from: 14, to: 17 },
  { text: "a", from: 17, to: 18 }, { text: "recent", from: 18, to: 27 }, { text: "sleep", from: 27, to: 36 },
  { text: "study,", from: 36, to: 48 }, { text: "sleep", from: 90, to: 99 }, { text: "better.", from: 99, to: 110 },
];
let got: any;
beforeEach(() => { got = undefined; });
const at = (node: any, frame = 0) => renderFrame(node, { frame, fps: 30, width: 1080, height: 1920, durationInFrames: 300, words });
function Ask({ f }: { f: () => any }) { got = f(); return null; }

describe("word", () => {
  test("the frame a phrase starts", () => {
    at(<Ask f={() => word("recent sleep study")} />);
    expect(got).toBe(18);
  });
  test("case and punctuation are ignored", () => {
    at(<Ask f={() => word("Sleep Study")} />);
    expect(got).toBe(27);
  });
  test("occurrence picks a later one", () => {
    at(<Ask f={() => word("sleep", { occurrence: 2 })} />);
    expect(got).toBe(90);
  });
  test("edge end is the frame after the last word ends", () => {
    at(<Ask f={() => word("sleep study", { edge: "end" })} />);
    expect(got).toBe(48);
  });
  test("a phrase nobody says fails", () => {
    expect(() => at(<Ask f={() => word("deep sleep")} />)).toThrow();
  });
});

describe("useWords", () => {
  test("every word on the timeline, in order", () => {
    at(<Ask f={() => useWords()} />);
    expect(got.map((w: any) => w.text)).toEqual(words.map((w) => w.text));
    expect(got[5]).toEqual({ text: "sleep", from: 27, to: 36 });
  });
  test("in the frames of the enclosing Sequence", () => {
    at(<Sequence from={20}><Ask f={() => useWords()} /></Sequence>, 25);
    expect(got.find((w: any) => w.text === "recent")).toEqual({ text: "recent", from: -2, to: 7 });
  });
});
