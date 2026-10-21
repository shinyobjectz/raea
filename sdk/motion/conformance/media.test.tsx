// Video and Audio sound props, from spec/motion.feature ("Video and Audio"): gain above unity, gainDb, sub-frame fades.
import { expect, test } from "bun:test";
import { Audio, Video } from "../src";
import { renderFrame } from "../src/test";

const base = { frame: 0, fps: 30, width: 1080, height: 1920, durationInFrames: 60 };
const attr = (html: string, name: string) => new RegExp(`${name}="([^"]*)"`).exec(html)?.[1];

test("volume above 1 is a gain, not an error", () => {
  const html = renderFrame(<Audio src="media/a.wav" volume={1.78} />, base);
  expect(Number(attr(html, "data-volume"))).toBeCloseTo(1.78, 3);
});

test("a negative volume fails", () => {
  expect(() => renderFrame(<Audio src="media/a.wav" volume={-0.1} />, base)).toThrow();
});

test("gainDb gives the linear volume and wins over volume", () => {
  const html = renderFrame(<Audio src="media/a.wav" volume={0.2} gainDb={5} />, base);
  expect(Number(attr(html, "data-volume"))).toBeCloseTo(Math.pow(10, 5 / 20), 3);
  const quiet = renderFrame(<Video src="media/v.mp4" gainDb={-6} />, base);
  expect(Number(attr(quiet, "data-volume"))).toBeCloseTo(Math.pow(10, -6 / 20), 3);
});

test("fades shorter than a frame are carried in seconds for the renderer", () => {
  const html = renderFrame(<Video src="media/v.mp4" fadeIn={0.012} fadeOut={0.012} />, base);
  expect(Number(attr(html, "data-fade-in"))).toBeCloseTo(0.012, 6);
  expect(Number(attr(html, "data-fade-out"))).toBeCloseTo(0.012, 6);
});
