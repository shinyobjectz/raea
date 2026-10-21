// registerRoot, from spec/motion.feature ("Declaring and registering"), including a root with no Composition.
import { beforeEach, expect, test } from "bun:test";
import { Composition, Sequence, getCompositions, registerRoot, useCurrentFrame } from "../src";
import { resetRegistry } from "../src/test";

beforeEach(() => resetRegistry());
function Uses() { useCurrentFrame(); return null; }

test("compositions are found in the root", () => {
  registerRoot(() => (<><Composition id="ad-1" component={Uses} width={1080} height={1350} fps={30} durationInFrames={90} /></>));
  const c = getCompositions();
  expect(c.length).toBe(1);
  expect(c[0]).toMatchObject({ id: "ad-1", width: 1080, height: 1350, fps: 30, durationInFrames: 90 });
});

test("a bad id fails", () => {
  expect(() => registerRoot(() => <Composition id="ad 1" component={Uses} width={1080} height={1920} fps={30} durationInFrames={90} />)).toThrow();
});

test("registering twice fails", () => {
  registerRoot(() => <Composition id="a" component={Uses} width={1080} height={1920} fps={30} durationInFrames={9} />);
  expect(() => registerRoot(() => null)).toThrow();
});

test("a root with no Composition is itself one, sized from its Sequences", () => {
  registerRoot(() => (
    <>
      <Sequence from={0} durationInFrames={120}><Uses /></Sequence>
      <Sequence from={100} durationInFrames={80}><Uses /></Sequence>
    </>
  ));
  const c = getCompositions();
  expect(c.length).toBe(1);
  expect(c[0]).toMatchObject({ id: "main", fps: 30, width: 1080, height: 1920, durationInFrames: 180 });
});

test("without finite Sequences the length is 300", () => {
  registerRoot(() => <Uses />);
  expect(getCompositions()[0]).toMatchObject({ id: "main", durationInFrames: 300 });
});
