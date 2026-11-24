// Labels that read (docs/stage-guards.feature SG8): words that point at a part are page type, placed where the part
// lands through the shot's own camera, sized in pixels of the frame, inside title-safe, never turned and never tiny.
import { describe, expect, test } from "bun:test";
import { Label, cameraPath } from "../src/camera";
import { Camera3d } from "../src/camera";
import { Transform } from "../src/transform";
import { labelPlace, project } from "@raea/render/3d/label";
import { renderFrame } from "../src/test";

const W = 1920, H = 1080;
const html = (el: React.ReactElement, height = H, width = W) => renderFrame(el, { frame: 0, width, height, fps: 30, durationInFrames: 30 });
const cam = { transform: Transform.fromXyz(0, 1.5, 5).lookingAt([0, 1.5, 0]) };
const near = (a: number, b: number, e = 1e-6) => Math.abs(a - b) <= e;

describe("SG8: labels that read", () => {
  test("a point lands where the camera sees it: the centre, its height by the fov, behind and outside are nowhere", () => {
    const c = project([0, 1.5, 0], cam, W, H)!;
    expect(near(c[0], W / 2) && near(c[1], H / 2) && near(c[2], 5)).toBe(true);
    // a metre up at 5 m with a 45° vertical fov: (H/2) / tan(22.5°) / 5 px up
    const up = project([0, 2.5, 0], cam, W, H)!;
    expect(near(up[1], H / 2 - H / 2 / Math.tan(Math.PI / 8) / 5)).toBe(true);
    expect(project([0, 1.5, 9], cam, W, H)).toBeUndefined();
    expect(project([40, 1.5, 0], cam, W, H)).toBeUndefined();
    // the props a cameraPath hands its Camera3d, fov and all
    const path = cameraPath([{ at: 0, pos: [0, 1.5, 5], look: [0, 1.5, 0], fov: 30 }, { at: 30, pos: [0, 1.5, 4], look: [0, 1.5, 0], fov: 30 }]);
    const p30 = project([0, 2.5, 0], path.camera(0), W, H)!;
    expect(near(p30[1], H / 2 - H / 2 / Math.tan(Math.PI / 12) / 5)).toBe(true);
    // an orthographic camera: its view's height in metres
    const ortho = project([1, 1.5, 0], { ...cam, projection: { Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 4 } } } } }, W, H)!;
    expect(near(ortho[0], W / 2 + H / 4)).toBe(true);
  });

  test("the words stay inside title-safe and grow away from the nearer edge", () => {
    const mid = labelPlace([W / 2, H / 2], W, H, "above", 40, 40);
    expect(near(mid.ax, W / 2) && near(mid.ay, H / 2 - 40) && near(mid.along, 0.5)).toBe(true);
    const left = labelPlace([10, 30], W, H, "above", 40, 40);
    expect(left.ax).toBe(0.05 * W);
    expect(left.along).toBe(0);
    expect(left.ay).toBeGreaterThanOrEqual(0.05 * H + 48);
    const right = labelPlace([W - 1, H - 5], W, H, "below", 40, 40);
    expect(right.along).toBe(1);
    expect(right.ay).toBeLessThanOrEqual(0.95 * H - 48);
  });

  test("drawn in the page, sized in pixels of the frame, with a line to the point; nothing when the point isn't seen", () => {
    const out = html(<><Camera3d {...cam} /><Label at={[0, 1.5, 0]} camera={cam}>Reviews</Label></>);
    expect(out).toContain('data-label=""');
    expect(out).toContain("Reviews");
    expect(out).toContain("data-label-leader");
    // 32 px capitals at 1080 lines: a 45.7 px font; at 540 lines half that
    expect(out).toMatch(/font-size:\s*45\.7\d*px/);
    expect(html(<Label at={[0, 1.5, 0]} camera={cam}>Reviews</Label>, 540, 960)).toMatch(/font-size:\s*22\.8\d*px/);
    expect(html(<Label at={[0, 1.5, 9]} camera={cam}>Behind</Label>)).not.toContain("Behind");
    expect(html(<Label at={[0, 1.5, 0]} camera={cam} leader={false}>Reviews</Label>)).not.toContain("data-label-leader");
  });

  test("never tiny", () => {
    expect(() => html(<Label at={[0, 1.5, 0]} camera={cam} size={16}>Reviews</Label>)).toThrow("at least 22 px");
  });
});
