// <Matchmove> (spec/motion.feature, "3D: matchmove"): props checked, the camera placed from the solve, the footage's
// window in a frame of another shape, and the plate and cut-out drawn for the frame shown.
import { describe, expect, test } from "bun:test";
import { Plate, Canvas3D } from "../src/3d";
import { Matchmove, checkMatchmove, matchmoveIndex, matchmoveView, rotate, type MatchmoveSolve } from "../src/matchmove";
import { renderFrame } from "../src/test";

const props = (html: string, kind: string) => {
  const m = html.match(new RegExp(`data-3d="${kind}" data-props="([^"]*)"`));
  return m ? JSON.parse(m[1].replace(/&quot;/g, '"')) : undefined;
};

// a camera 1.6 m up, stepping 0.1 m along +X a frame, level, looking down -Z
const solve: MatchmoveSolve = {
  mode: "sfm", width: 1920, height: 1080, fps: 30, from: 2, seconds: 1, frames: 30, fov: 40, focal: 1484, k1: -0.05,
  registered: 30, error: 0.5, camera_height: 1.6,
  cameras: Array.from({ length: 30 }, (_, i) => ({ t: 2 + i / 30, position: [i * 0.1, 1.6, 0] as [number, number, number], quaternion: [0, 0, 0, 1] as [number, number, number, number], solved: true })),
  points: [], ground: { y: 0, normal: [0, 1, 0] }, cut: "/store/cut/%04d.png", notes: [],
};

describe("Matchmove", () => {
  test("props are checked, naming the prop", () => {
    expect(() => checkMatchmove({ src: "" })).toThrow(/src/);
    expect(() => checkMatchmove({ src: "a.mp4", from: -1 })).toThrow(/from/);
    expect(() => checkMatchmove({ src: "a.mp4", subject: [0.1, 0.2, 0.3] as never })).toThrow(/subject/);
    expect(() => checkMatchmove({ src: "a.mp4", subject: [0.1, 0.2, 0.3, 1.5] })).toThrow(/subject/);
    expect(() => checkMatchmove({ src: "a.mp4", height: 0 })).toThrow(/height/);
    expect(() => checkMatchmove({ src: "a.mp4", fov: 200 })).toThrow(/fov/);
    expect(() => checkMatchmove({ src: "a.mp4", zoom: 0.5 })).toThrow(/zoom/);
    expect(() => checkMatchmove({ src: "a.mp4", crop: 1.2 })).toThrow(/crop/);
    expect(() => checkMatchmove({ src: "a.mp4", crop: [0.2, 0.5], zoom: 1.5, subject: [0.4, 0.2, 0.2, 0.7], fov: 30 })).not.toThrow();
  });

  test("a plate's props are checked", () => {
    expect(() => renderFrame(<Canvas3D><Plate src="" /></Canvas3D>, { fps: 30, width: 64, height: 36, durationInFrames: 1, frame: 0 })).toThrow(/src/);
    expect(() => renderFrame(<Canvas3D><Plate src="a.mp4" k1={2} /></Canvas3D>, { fps: 30, width: 64, height: 36, durationInFrames: 1, frame: 0 })).toThrow(/k1/);
  });

  test("the footage covers a frame of its own shape exactly, and a 9:16 frame through a window across it", () => {
    expect(matchmoveView(solve, { width: 1920, height: 1080 })).toEqual({ width: 1920, height: 1080, x: 0, y: 0 });
    // 9:16 from 16:9: the footage scaled to the frame's height, 3413 wide; crop 0 is its left edge, 1 its right
    const v = matchmoveView(solve, { width: 1080, height: 1920 }, 0);
    expect(v.height).toBe(1920);
    expect(v.width).toBeCloseTo(3413.33, 1);
    expect(v.x).toBe(0);
    expect(matchmoveView(solve, { width: 1080, height: 1920 }, 1).x).toBeCloseTo(3413.33 - 1080, 1);
    expect(matchmoveView(solve, { width: 1080, height: 1920 }, 0.5).x).toBeCloseTo((3413.33 - 1080) / 2, 1);
    // a punch-in keeps the centre
    const z = matchmoveView(solve, { width: 1920, height: 1080 }, 0.5, 2);
    expect([z.width, z.height, z.x, z.y]).toEqual([3840, 2160, 960, 540]);
  });

  test("the frame shown is the latest solved at or before the clip's time", () => {
    expect(matchmoveIndex(solve, 2)).toBe(0);
    expect(matchmoveIndex(solve, 2 + 5 / 30)).toBe(5);
    expect(matchmoveIndex(solve, 1)).toBe(0);
    expect(matchmoveIndex(solve, 99)).toBe(29);
    expect(rotate([0, Math.SQRT1_2, 0, Math.SQRT1_2], [0, 0, -1]).map((v) => Math.round(v * 1e6) / 1e6)).toEqual([-1, 0, 0]);
  });

  test("each frame places the camera from the solve and draws its plate and cut-out", () => {
    const cfg = { fps: 30, width: 1920, height: 1080, durationInFrames: 30 };
    const html = renderFrame(<Matchmove src="a.mp4" from={2} solve={solve} subject={[0.4, 0.2, 0.2, 0.7]} />, { ...cfg, frame: 10 });
    const cam = props(html, "PerspectiveCamera");
    expect(cam.position).toEqual([1, 1.6, 0]);
    expect(cam.fov).toBe(40);
    expect(cam.view).toBeUndefined();
    const plate = props(html, "Plate");
    expect(plate.src).toBe("a.mp4");
    expect(plate.time).toBeCloseTo(2 + 10 / 30, 6);
    expect(plate.k1).toBe(-0.05);
    expect(html).toContain("/store/cut/0011.png");
    // without a subject there's no cut-out; in 9:16 the camera looks through the crop's window
    const tall = renderFrame(<Matchmove src="a.mp4" from={2} solve={solve} crop={0} />, { ...cfg, width: 1080, height: 1920, frame: 0 });
    expect(tall).not.toContain("/store/cut/");
    expect(props(tall, "PerspectiveCamera").view).toEqual({ width: matchmoveView(solve, { width: 1080, height: 1920 }, 0).width, height: 1920, x: 0, y: 0 });
  });
});
