// docs/make.feature MK21: a part's `lod` is its planned detail, a prop on its row that the stage checks read.
import { describe, expect, test } from "bun:test";
import { Cuboid, Entity, Model } from "../src/3d";
import { Group } from "@raea/render/three";
import { Mesh3d, StandardMaterial } from "@raea/render/bevy3d";
import { renderFrame } from "../src/test";

const html = (el: React.ReactElement) => renderFrame(el, { frame: 0, width: 320, height: 240, fps: 30, durationInFrames: 30 });
const props = (out: string, kind: string) => [...out.matchAll(new RegExp(`data-3d="${kind}" data-props="([^"]*)"`, "g"))].map((m) => JSON.parse(m[1]!.replaceAll("&quot;", '"')));

describe("MK21: planned detail", () => {
  test("a Mesh3d's and a Model's lod ride on their rows, and a part that plans none carries none", () => {
    const out = html(<><Mesh3d name="kiosk" lod={3}><Cuboid /><StandardMaterial /></Mesh3d><Mesh3d name="desk"><Cuboid /></Mesh3d><Model src="tower.glb" lod={4} /></>);
    expect(props(out, "Mesh").map((p) => [p.name, p.lod])).toEqual([["kiosk", 3], ["desk", undefined]]);
    expect(props(out, "Model")[0].lod).toBe(4);
  });

  test("a Group's lod is the plan of every part inside that plans none of its own", () => {
    const out = html(<Group lod={4}><Mesh3d name="far"><Cuboid /></Mesh3d><Mesh3d name="hero" lod={0}><Cuboid /></Mesh3d><Group><Model src="a.glb" /></Group></Group>);
    expect(props(out, "Mesh").map((p) => p.lod)).toEqual([4, 0]);
    expect(props(out, "Model")[0].lod).toBe(4);
    // Bevy's parent entity is the same
    const bevy = html(<Entity transform={{ translation: [0, 0, -80] }} lod={3}><Mesh3d name="skyline"><Cuboid /></Mesh3d></Entity>);
    expect(props(bevy, "Mesh")[0].lod).toBe(3);
  });

  test("a level that isn't one of the five is refused", () => {
    expect(() => html(<Mesh3d lod={5 as 4}><Cuboid /></Mesh3d>)).toThrow("0 (the hero");
    expect(() => html(<Model src="a.glb" lod={1.5 as 1} />)).toThrow("lod is its planned detail");
  });
});
