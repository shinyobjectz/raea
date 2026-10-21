// @raea/motion/seedling: <Seedling> writes the spec raea-seedling plays (sdk/react/src/audio.rs seedling_plays).
import { expect, test } from "bun:test";
import { Seedling } from "../src/seedling";
import { sao } from "../src/assets";

const spec = (el: any) => JSON.parse(el.props["data-seedling"]);

test("a media file, its key the src unless an id is given", () => {
  expect(spec(Seedling({ src: "/media/hit.wav" }))).toEqual({ key: "/media/hit.wav", src: "/media/hit.wav", volume: 1, speed: 1, loop: false, fx: [] });
  expect(spec(Seedling({ src: "/media/hit.wav", id: "hit-3", volume: 0.5, speed: 1.5 })).key).toBe("hit-3");
});

test("a looping bed through raea's Tone effects as seedling nodes", () => {
  const s = spec(Seedling({ src: "bed.wav", loop: true, fx: [{ kind: "Freeverb", options: { roomSize: 0.8 } }, { kind: "Filter" }] }));
  expect(s.loop).toBe(true);
  expect(s.fx).toEqual([{ kind: "Freeverb", options: { roomSize: 0.8 } }, { kind: "Filter", options: {} }]);
});

test("a generated sound goes to the asset store by prompt", () => {
  const s = spec(Seedling({ src: sao("a dry wooden knock", { seconds: 0.4 }) }));
  expect(s.src).toMatchObject({ generate: "sao", prompt: "a dry wooden knock", seconds: 0.4 });
  expect(s.key).toBe("sao:a dry wooden knock");
});

test("a src that is neither throws", () => {
  expect(() => Seedling({ src: "" })).toThrow(/media file or a generated sound/);
});
