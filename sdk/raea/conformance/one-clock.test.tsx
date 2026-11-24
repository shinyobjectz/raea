// One clock (docs/stage-guards.feature SG5): a 3D part's frame props are frames of the Sequence it plays in. One at or
// past that Sequence's last frame is refused at the part's first frame, naming the prop and the fix: the stage film's
// store quote was given `at` 1013 in a 150-frame scene, and its reveal never started.
import { describe, expect, test } from "bun:test";
import { Cuboid, RichText } from "../src/3d";
import { Mesh3d, StandardMaterial } from "@raea/render/bevy3d";
import { BevyComponent } from "@raea/render/3d";
import { checkSequenceFrames, firstKey } from "@raea/render/3d/clock";
import { renderFrame } from "../src/test";

const html = (el: React.ReactElement) => renderFrame(el, { frame: 0, width: 320, height: 240, fps: 30, durationInFrames: 150 });
const FIX = "Give at in this Sequence's frames (useCurrentFrame), never a frame passed down as a prop, or put the scene in a <Sequence from>.";

describe("SG5: one clock", () => {
  test("RichText refuses an `at` past its Sequence, and takes one inside it", () => {
    expect(() => html(<RichText name="quote" at={1013}>Five stars</RichText>)).toThrow(`RichText 'quote': at 1013 is after this Sequence's last frame (149). ${FIX}`);
    expect(() => html(<RichText at={149}>Five stars</RichText>)).not.toThrow();
  });

  test("a mesh's animation that starts after its Sequence ends is refused; one cut off by the end is fine", () => {
    const late = { opacity: { 400: 0, 420: 1 } };
    expect(() => html(<Mesh3d name="dot" animate={late}><Cuboid /><StandardMaterial /></Mesh3d>)).toThrow("Mesh3d 'dot': animate's first key 400 is after this Sequence's last frame (149).");
    expect(() => html(<Mesh3d animate={{ opacity: { 100: 0, 300: 1 } }}><Cuboid /><StandardMaterial /></Mesh3d>)).not.toThrow();
  });

  test("a Bevy component added after its Sequence ends is refused", () => {
    expect(() => html(<Mesh3d><Cuboid /><BevyComponent type="avian3d::dynamics::rigid_body::RigidBody" value="Dynamic" from={200} /></Mesh3d>)).toThrow("BevyComponent: from 200 is after this Sequence's last frame (149).");
  });

  test("the check itself: each prop named, nothing left out checked, an endless Sequence never refused", () => {
    expect(() => checkSequenceFrames("Part", undefined, [["from", 3], ["until", 30]], 30)).toThrow("Part: until 30 is after this Sequence's last frame (29).");
    expect(() => checkSequenceFrames("Part", undefined, [["from", undefined]], 30)).not.toThrow();
    expect(() => checkSequenceFrames("Part", undefined, [["from", 1e6]], Infinity)).not.toThrow();
  });

  test("an animation's first key, from any of its shapes", () => {
    expect(firstKey({ opacity: { 12: 0, 30: 1 }, y: { 6: 10, 20: 0 } })).toBe(6);
    expect(firstKey({ x: { keys: [[40, 0], [60, 1]] } })).toBe(40);
    expect(firstKey({ x: [{ keys: [[9, 0], [12, 1]] }, { keys: [[3, 1], [5, 0]] }] })).toBe(3);
    expect(firstKey({ tracks: { scale: { keys: [[15, 1], [25, 2]] } }, frames: 25 })).toBe(15);
    expect(firstKey(undefined)).toBeUndefined();
  });
});
