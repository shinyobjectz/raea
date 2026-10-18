// <Mix>, from spec/motion.feature ("The mix"): an element the renderer masters the sum with.
import { expect, test } from "bun:test";
import { AbsoluteFill, Mix } from "../src";
import { renderFrame } from "../src/test";

const base = { frame: 0, fps: 30, width: 1080, height: 1920, durationInFrames: 30 };

test("Mix carries its loudness and ceiling for the renderer", () => {
  const html = renderFrame(<AbsoluteFill><Mix lufs={-14} ceiling={-1.5} /></AbsoluteFill>, base);
  expect(html).toContain('data-mix="true"');
  expect(html).toContain('data-lufs="-14"');
  expect(html).toContain('data-ceiling="-1.5"');
});

test("its defaults are -14 LUFS and -1.5 dBTP", () => {
  const html = renderFrame(<Mix />, base);
  expect(html).toContain('data-lufs="-14"');
  expect(html).toContain('data-ceiling="-1.5"');
});

test("two in one composition is an error", () => {
  expect(() => renderFrame(<><Mix /><Mix /></>, base)).toThrow();
});
