// @raea/motion/3d's solids (spec: "Solids are native Manifold geometry held by handle"). The argument checks run
// anywhere; the geometry runs where the engine is (raea-motion test, the Mac or the iPhone), against closed-form answers.
import { describe, expect, test } from "bun:test";
import { manifold, Solid } from "../src/solid";
import { hasNative, native } from "../src/native";
import * as entry3d from "../src/3d";

const engine = hasNative();

describe("solids: arguments", () => {
  test("numbers must be finite, checked before the engine is asked", () => {
    expect(() => manifold.box(1, Number.NaN, 1)).toThrow(TypeError);
    expect(() => manifold.sphere(Infinity)).toThrow(/sphere: numbers only/);
    expect(() => manifold.cylinder(1, "2" as unknown as number)).toThrow(TypeError);
  });
  test("text3d needs text", () => {
    expect(() => manifold.text3d("", 1, 1)).toThrow(/give the text/);
  });
  test.skipIf(engine)("without the engine a solid says it needs it", () => {
    expect(() => manifold.box(1, 1, 1)).toThrow(/needs the raea engine/);
  });
});

describe.skipIf(!engine)("solids: geometry (engine)", () => {
  test("a box's volume and bounds", () => {
    const b = manifold.box(2, 1, 0.5);
    expect(b).toBeInstanceOf(Solid);
    expect(b.volume()).toBeCloseTo(1, 6);
    expect(b.bounds()).toEqual({ min: [-1, -0.5, -0.25], max: [1, 0.5, 0.25] });
    expect(b.expr).toBe("box(2, 1, 0.5)");
  });
  test("a bored box: volume, its middle slice's area and two contours", () => {
    const bored = manifold.box(2, 2, 2).minus(manifold.cylinder(0.5, 3));
    expect(Math.abs(bored.volume() - (8 - Math.PI * 0.5))).toBeLessThan(0.02);
    const s = bored.slice(0);
    expect(Math.abs(s.area - (4 - Math.PI * 0.25))).toBeLessThan(0.02);
    expect(s.contours).toBe(2);
    expect(s.polygons.length).toBe(2);
  });
  test("union and intersect of overlapping boxes", () => {
    const a = manifold.box(2, 2, 2);
    const b = manifold.box(2, 2, 2).move(1, 0, 0);
    expect(a.union(b).volume()).toBeCloseTo(12, 6);
    expect(a.intersect(b).volume()).toBeCloseTo(4, 6);
    expect(a.minus(b).volume()).toBeCloseTo(4, 6);
  });
  test("booleans need another solid", () => {
    expect(() => manifold.box(1, 1, 1).union()).toThrow(/at least one other solid/);
  });
  test("hull of two spheres covers both", () => {
    const h = manifold.sphere(1).hull(manifold.sphere(1).move(4, 0, 0));
    const b = h.bounds();
    expect(b.min[0]).toBeCloseTo(-1, 1);
    expect(b.max[0]).toBeCloseTo(5, 1);
    expect(h.volume()).toBeGreaterThan(2 * manifold.sphere(1).volume());
  });
  test("move, rotate and scale make new solids and leave the old one", () => {
    const b = manifold.box(2, 1, 0.5);
    expect(b.move(0, 0, 5).bounds().min[2]).toBeCloseTo(4.75, 6);
    const r = b.rotate(0, 0, 90).bounds();
    expect(r.max[0]).toBeCloseTo(0.5, 5);
    expect(r.max[1]).toBeCloseTo(1, 5);
    expect(b.scale(2).volume()).toBeCloseTo(8, 6);
    expect(b.scale(1, 2, 3).volume()).toBeCloseTo(6, 6);
    expect(b.volume()).toBeCloseTo(1, 6);
  });
  test("a boolean that removes everything is empty", () => {
    expect(manifold.box(1, 1, 1).minus(manifold.box(3, 3, 3)).isEmpty()).toBe(true);
    expect(manifold.box(1, 1, 1).isEmpty()).toBe(false);
  });
  test("triangles and the primitives", () => {
    expect(manifold.box(1, 1, 1).triangles()).toBe(12);
    expect(manifold.cone(1, 0, 2).volume()).toBeCloseTo((Math.PI * 2) / 3, 1);
    expect(manifold.torus(2, 0.5).volume()).toBeCloseTo(2 * Math.PI * Math.PI * 2 * 0.25, 0);
    expect(manifold.roundedBox(2, 2, 2, 0.2).volume()).toBeLessThan(8);
  });
  test("text3d cut through a sign takes volume away", () => {
    const plate = manifold.roundedBox(2.4, 1.1, 0.25, 0.08);
    const sign = plate.minus(manifold.text3d("SALE", 0.62, 0.6));
    expect(sign.volume()).toBeLessThan(plate.volume());
    expect(sign.expr).toContain("text3d(");
  });
  test("a freed solid can't be used again", () => {
    const b = manifold.box(1, 1, 1);
    b.free();
    expect(() => b.volume()).toThrow();
  });
});

describe.skipIf(!engine)("solids: the rest of Manifold (engine)", () => {
  const { outline } = require("../src/solid") as typeof import("../src/solid");
  test("extrude and revolve a 2D outline", () => {
    const prism = manifold.extrude(outline.rect(2, 1), 3);
    expect(prism.volume()).toBeCloseTo(6, 6);
    expect(prism.bounds()).toEqual({ min: [-1, -0.5, 0], max: [1, 0.5, 3] });
    const twisted = manifold.extrude(outline.rect(2, 2), 2, { divisions: 16, twist: 90, scaleTop: [0.5, 0.5] });
    expect(twisted.volume()).toBeLessThan(8);
    expect(twisted.volume()).toBeGreaterThan(8 / 3);
    // a rectangle 1 wide (x from 1 to 2) revolved about y: a ring, volume pi (R^2 - r^2) h
    const ring = manifold.revolve([[[1, 0], [2, 0], [2, 1], [1, 1]]]);
    expect(Math.abs(ring.volume() - Math.PI * 3)).toBeLessThan(0.05);
    expect(ring.genus()).toBe(1);
    expect(() => manifold.extrude([[[0, 0], [1, 1]]], 1)).toThrow(/3 or more/);
  });
  test("mirror, transform, trim and split", () => {
    const b = manifold.box(2, 2, 2).move(3, 0, 0);
    expect(b.mirror(1, 0, 0).bounds().min[0]).toBeCloseTo(-4, 6);
    const doubled = manifold.box(1, 1, 1).transform([2, 0, 0, 0, 1, 0, 0, 0, 1, 5, 0, 0]);
    expect(doubled.volume()).toBeCloseTo(2, 6);
    expect(doubled.bounds().min[0]).toBeCloseTo(4, 6);
    const half = manifold.box(2, 2, 2).trim(0, 0, 1, 0);
    expect(half.volume()).toBeCloseTo(4, 6);
    expect(half.bounds().min[2]).toBeCloseTo(0, 6);
    const [top, bottom] = manifold.box(2, 2, 2).splitByPlane(0, 0, 1, 0.5);
    expect(top.volume()).toBeCloseTo(2, 6);
    expect(bottom.volume()).toBeCloseTo(6, 6);
    const [inside, outside] = manifold.box(2, 2, 2).split(manifold.box(1, 1, 4));
    expect(inside.volume()).toBeCloseTo(2, 6);
    expect(outside.volume()).toBeCloseTo(6, 6);
  });
  test("refine, smooth and Minkowski", () => {
    const c = manifold.box(1, 1, 1);
    expect(c.refine(2).triangles()).toBe(48);
    expect(c.refine(2).volume()).toBeCloseTo(1, 6);
    // a smoothed cube is a curved surface through its corners: rounder, so it bulges past its faces
    const round = c.smooth({ angle: 120, refine: 3 });
    expect(Math.abs(round.volume() - 1)).toBeGreaterThan(0.05);
    expect(round.triangles()).toBeGreaterThan(12);
    // a unit box swept by a box 1 a side is a box 2 a side
    expect(c.minkowski(manifold.box(1, 1, 1)).volume()).toBeCloseTo(8, 4);
    expect(manifold.box(3, 3, 3).minkowskiDiff(manifold.box(1, 1, 1)).volume()).toBeCloseTo(8, 4);
  });
  test("warp, a level set, and the surface area", () => {
    const b = manifold.box(2, 2, 2).refine(4);
    const sheared = b.warp((x, y, z) => [x + 0.5 * z, y, z]);
    expect(sheared.volume()).toBeCloseTo(8, 3);
    expect(sheared.bounds().max[0]).toBeCloseTo(1.5, 3);
    const ball = manifold.levelSet((x, y, z) => Math.hypot(x, y, z) - 1, { min: [-1.2, -1.2, -1.2], max: [1.2, 1.2, 1.2], resolution: 40 });
    expect(Math.abs(ball.volume() - (4 / 3) * Math.PI)).toBeLessThan(0.1);
    expect(manifold.box(1, 2, 3).area()).toBeCloseTo(22, 6);
  });
  test("OBJ and glTF out and back in, the same solid", () => {
    const tmp = (native("host.env", { name: "TMPDIR" }) as string) + `raea-solid-${Date.now()}`;
    const shape = manifold.roundedBox(2, 1, 0.5, 0.1).minus(manifold.cylinder(0.2, 2));
    for (const ext of ["obj", "glb"]) {
      expect(shape.export(`${tmp}.${ext}`)).toBeGreaterThan(100);
      const back = manifold.import(`${tmp}.${ext}`);
      expect(back.volume()).toBeCloseTo(shape.volume(), 3);
      expect(back.genus()).toBe(shape.genus());
      expect(back.expr).toContain(`${ext}(`);
    }
  });
  test("solids made from meshes render: their expression rebuilds them", () => {
    const fromMesh = manifold.box(1, 1, 1).warp((x, y, z) => [x * 2, y, z]);
    expect(fromMesh.expr).toMatch(/^obj\(/);
    const again = manifold.import(JSON.parse(fromMesh.expr.slice(4, -1)));
    expect(again.volume()).toBeCloseTo(2, 4);
  });
});

describe.skipIf(!engine)("solids: 2D cross-sections, batches and the last of Manifold (engine)", () => {
  const { CrossSection, crossSection } = require("../src/solid") as typeof import("../src/solid");
  test("cross-section constructors, area and bounds", () => {
    const r = crossSection.rect(2, 1);
    expect(r).toBeInstanceOf(CrossSection);
    expect(r.area).toBeCloseTo(2, 9);
    expect(r.bounds).toEqual({ min: [-1, -0.5], max: [1, 0.5] });
    expect(crossSection.rect(2, 1, false).bounds.min).toEqual([0, 0]);
    expect(Math.abs(crossSection.circle(1, 256).area - Math.PI)).toBeLessThan(0.001);
    expect(crossSection.polygon([[0, 0], [1, 0], [0, 1]]).area).toBeCloseTo(0.5, 9);
    expect(crossSection.hull([[0, 0], [2, 0], [2, 2], [0, 2], [1, 1]]).area).toBeCloseTo(4, 9);
    // overlapping contours are unioned as Manifold's positive fill rule has it
    expect(crossSection.of([[[0, 0], [2, 0], [2, 2], [0, 2]], [[1, 1], [3, 1], [3, 3], [1, 3]]]).area).toBeCloseTo(7, 9);
  });
  test("cross-section booleans, hull and Minkowski", () => {
    const a = crossSection.rect(2, 2);
    const b = crossSection.rect(2, 2).move(1, 0);
    expect(a.union(b).area).toBeCloseTo(6, 9);
    expect(a.minus(b).area).toBeCloseTo(2, 9);
    expect(a.intersect(b).area).toBeCloseTo(2, 9);
    expect(a.minus(crossSection.circle(0.5, 64)).contours).toBe(2);
    expect(a.hull(crossSection.rect(2, 2).move(4, 0)).area).toBeCloseTo(12, 9);
    expect(a.minkowski(crossSection.rect(1, 1)).area).toBeCloseTo(9, 6);
    expect(() => a.union()).toThrow(/at least one other/);
  });
  test("offset: round, square and miter corners; negative shrinks", () => {
    const sq = crossSection.rect(2, 2);
    expect(sq.offset(0.5, { join: "miter" }).area).toBeCloseTo(9, 6);
    expect(sq.offset(0.5, { join: "square" }).area).toBeLessThan(9);
    // a square grown by r with round corners: (2 + 2r)^2 - (4 - pi) r^2
    expect(Math.abs(sq.offset(0.5, { segments: 256 }).area - (9 - (4 - Math.PI) * 0.25))).toBeLessThan(0.01);
    expect(sq.offset(-0.5, { join: "miter" }).area).toBeCloseTo(1, 6);
    expect(() => sq.offset(0.1, { join: "wavy" as never })).toThrow(/square, round, miter or bevel/);
  });
  test("cross-section moves, warp, simplify and decompose", () => {
    const r = crossSection.rect(2, 1);
    expect(r.rotate(90).bounds.max[1]).toBeCloseTo(1, 9);
    expect(r.scale(2).area).toBeCloseTo(8, 9);
    expect(r.scale(2, 1).bounds.max[0]).toBeCloseTo(2, 9);
    expect(r.move(3, 0).mirror(1, 0).bounds.min[0]).toBeCloseTo(-4, 9);
    expect(r.warp((x, y) => [x * 3, y]).area).toBeCloseTo(6, 9);
    const dense = crossSection.polygon([[0, 0], [1, 0], [2, 0], [2, 1], [1, 1], [0, 1]]);
    expect(dense.simplify(1e-6).polygons[0].length).toBe(4);
    const two = crossSection.rect(1, 1).union(crossSection.rect(1, 1).move(3, 0));
    expect(two.decompose().length).toBe(2);
    expect(r.minus(crossSection.rect(4, 4)).isEmpty()).toBe(true);
  });
  test("a cross-section extruded and revolved into solids", () => {
    const ring = crossSection.circle(1, 128).minus(crossSection.circle(0.5, 128));
    const tube = ring.extrude(2);
    expect(Math.abs(tube.volume() - 2 * Math.PI * 0.75)).toBeLessThan(0.01);
    expect(tube.genus()).toBe(1);
    const disc = crossSection.rect(1, 1, false).move(1, 0).revolve();
    expect(Math.abs(disc.volume() - Math.PI * 3)).toBeLessThan(0.05);
    expect(manifold.extrude(crossSection.rect(2, 1), 3).volume()).toBeCloseTo(6, 6);
    expect(() => crossSection.rect(1, 1).minus(crossSection.rect(2, 2)).extrude(1)).toThrow(/empty/);
  });
  test("slice and project as cross-sections", () => {
    const bored = manifold.box(2, 2, 2).minus(manifold.cylinder(0.5, 3));
    const s = bored.crossSection(0);
    expect(s).toBeInstanceOf(CrossSection);
    expect(s.contours).toBe(2);
    expect(Math.abs(s.area - bored.slice(0).area)).toBeLessThan(1e-9);
    const shadow = manifold.sphere(1).move(0, 0, 5).project();
    expect(Math.abs(shadow.area - Math.PI)).toBeLessThan(0.05);
    // a cross-section of one solid is the outline of the next
    expect(s.offset(-0.1, { join: "miter" }).extrude(1).volume()).toBeGreaterThan(0);
  });
  test("batch booleans, compose, decompose and hull of points", () => {
    const boxes = [0, 3, 6].map((x) => manifold.box(1, 1, 1).move(x, 0, 0));
    const row = manifold.union(...boxes);
    expect(row.volume()).toBeCloseTo(3, 6);
    expect(row.decompose().length).toBe(3);
    expect(manifold.compose(...boxes).volume()).toBeCloseTo(3, 6);
    expect(manifold.compose(...boxes).expr).toMatch(/^union\(/);
    expect(manifold.difference(manifold.box(2, 2, 2), manifold.box(1, 1, 4), manifold.box(4, 1, 1)).volume()).toBeCloseTo(8 - 2 - 2 + 1, 6);
    expect(manifold.intersection(manifold.box(2, 2, 2), manifold.box(2, 2, 2).move(1, 0, 0), manifold.box(2, 2, 2).move(0, 1, 0)).volume()).toBeCloseTo(2, 6);
    expect(() => manifold.union(manifold.box(1, 1, 1))).toThrow(/two solids or more/);
    const tet = manifold.hull([[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1], [0.1, 0.1, 0.1]]);
    expect(tet.volume()).toBeCloseTo(1 / 6, 6);
    expect(manifold.hull(manifold.sphere(1), manifold.sphere(1).move(4, 0, 0)).bounds().max[0]).toBeCloseTo(5, 1);
  });
  test("simplify, refine to a length or tolerance, the gap and rays", () => {
    const c = manifold.box(1, 1, 1);
    expect(c.refineToLength(0.25).triangles()).toBeGreaterThan(c.refine(2).triangles());
    expect(c.refineToLength(0.25).volume()).toBeCloseTo(1, 6);
    expect(c.refineToLength(0.25).simplify(0.01).triangles()).toBe(12);
    // a flat-faced mesh is already exact, so refining to a tolerance keeps it (it refines smoothed surfaces)
    expect(manifold.sphere(1).refineToTolerance(0.01).triangles()).toBeGreaterThanOrEqual(manifold.sphere(1).triangles());
    expect(manifold.sphere(1).refineToTolerance(0.01).volume()).toBeCloseTo(manifold.sphere(1).volume(), 6);
    expect(c.refineToLength(0.25).expr).toBe("refine_length(box(1, 1, 1), 0.25)");
    expect(c.minGap(manifold.box(1, 1, 1).move(3, 0, 0), 10)).toBeCloseTo(2, 6);
    expect(c.minGap(manifold.box(1, 1, 1).move(3, 0, 0), 1)).toBeCloseTo(1, 6);
    const hits = c.rayCast([-5, 0, 0], [5, 0, 0]);
    expect(hits.length).toBe(2);
    expect(hits[0].position[0]).toBeCloseTo(-0.5, 6);
    expect(hits[0].distance).toBeCloseTo(0.45, 6);
    expect(hits[0].normal[0]).toBeCloseTo(-1, 6);
  });
  test("mesh out and fromMesh back, and the outline helpers", () => {
    const { outline } = require("../src/solid") as typeof import("../src/solid");
    const m = manifold.box(1, 2, 3).mesh();
    expect(m.tris.length).toBe(36);
    expect(manifold.fromMesh(m.verts, m.tris).volume()).toBeCloseTo(6, 5);
    expect(Math.abs(manifold.extrude(outline.circle(1, 128), 1).volume() - Math.PI)).toBeLessThan(0.01);
    expect(manifold.extrude(outline.polygon([[0, 0], [1, 0], [0, 1]]), 2).volume()).toBeCloseTo(1, 6);
  });
  test("every new solid's expression rebuilds the same solid (what a Mesh draws)", () => {
    const shapes = [
      manifold.box(1, 1, 1).refineToLength(0.3),
      manifold.sphere(1).refineToTolerance(0.05),
      manifold.box(1, 1, 1).refine(3).simplify(0.01),
      crossSection.rect(2, 1).offset(0.2).extrude(0.5),
      manifold.compose(manifold.box(1, 1, 1), manifold.sphere(0.5).move(3, 0, 0)),
      ...manifold.union(manifold.box(1, 1, 1), manifold.box(1, 1, 1).move(3, 0, 0)).decompose(),
      manifold.hull([[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]]),
    ];
    for (const s of shapes) {
      const again = (native("solid.expr", { expr: s.expr }) as { id: number });
      expect(native("solid.query", { id: again.id, what: "volume" }) as number).toBeCloseTo(s.volume(), 5);
    }
  });
});

// The guard: Manifold's whole TypeScript API must stay, and every export must be exercised. A name removed from
// solid.ts, or added without a test here, fails this file (and so CI's motion:solid gate), under bun and on the engine.
describe("solids: the Manifold API is all there (the guard)", () => {
  const mod = require("../src/solid") as typeof import("../src/solid");
  const methods = (cls: { prototype: object }) => Object.getOwnPropertyNames(cls.prototype).filter((n) => n !== "constructor").sort();
  // Manifold (manifold3d's docs) -> ours. Every entry is required; the right side is what this file's tests call.
  const SOLID = ["area", "bounds", "crossSection", "decompose", "export", "free", "genus", "hull", "intersect", "isEmpty", "mesh", "minGap", "minkowski", "minkowskiDiff", "minus", "mirror", "move", "project", "rayCast", "refine", "refineToLength", "refineToTolerance", "rotate", "scale", "simplify", "slice", "smooth", "split", "splitByPlane", "transform", "triangles", "trim", "union", "volume", "warp"];
  const MANIFOLD = ["box", "compose", "cone", "cylinder", "difference", "extrude", "fromMesh", "hull", "import", "intersection", "levelSet", "revolve", "roundedBox", "sphere", "text3d", "torus", "union"];
  const SECTION = ["decompose", "extrude", "hull", "intersect", "isEmpty", "minkowski", "minus", "mirror", "move", "offset", "revolve", "rotate", "scale", "simplify", "union", "warp"];
  const CROSS = ["circle", "hull", "of", "polygon", "rect"];
  const OUTLINE = ["circle", "polygon", "rect"];
  test("Solid's methods are exactly the covered set", () => {
    expect(methods(mod.Solid).filter((n) => !n.startsWith("combine"))).toEqual(SOLID);
  });
  test("manifold's constructors and batches are exactly the covered set", () => {
    expect(Object.keys(mod.manifold).sort()).toEqual(MANIFOLD);
  });
  test("CrossSection's methods and crossSection's constructors are exactly the covered set", () => {
    expect(methods(mod.CrossSection)).toEqual(SECTION);
    expect(Object.keys(mod.crossSection).sort()).toEqual(CROSS);
    expect(Object.keys(mod.outline).sort()).toEqual(OUTLINE);
  });
  test("the 3D entry agents import (@raea/motion/3d) exports all of it", () => {
    for (const name of ["manifold", "Solid", "CrossSection", "crossSection", "outline"]) expect((entry3d as Record<string, unknown>)[name]).toBe((mod as Record<string, unknown>)[name]);
  });
  // reads this file's source, so it runs under bun (the engine has no file API here); CI runs both
  test.skipIf(engine)("every covered name is called somewhere in this file's engine tests", async () => {
    const src = await (globalThis as { Bun?: { file(p: string): { text(): Promise<string> } } }).Bun!.file(__filename).text();
    const engineTests = src.slice(0, src.indexOf("// The guard:"));
    const missing: string[] = [];
    for (const n of [...SOLID, ...SECTION]) if (!new RegExp(`\\.${n}\\(`).test(engineTests)) missing.push(n);
    for (const n of MANIFOLD) if (!new RegExp(`manifold\\.${n}\\(`).test(engineTests)) missing.push(`manifold.${n}`);
    for (const n of CROSS) if (!new RegExp(`crossSection\\.${n}\\(`).test(engineTests)) missing.push(`crossSection.${n}`);
    for (const n of OUTLINE) if (!new RegExp(`outline\\.${n}\\(`).test(engineTests)) missing.push(`outline.${n}`);
    expect(missing).toEqual([]);
  });
});
