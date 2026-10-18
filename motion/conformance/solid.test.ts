// @raea/motion/3d's solids (spec: "Solids are native Manifold geometry held by handle"). The argument checks run
// anywhere; the geometry runs where the engine is (raea-motion test, the Mac or the iPhone), against closed-form answers.
import { describe, expect, test } from "bun:test";
import { manifold, Solid } from "../src/solid";
import { hasNative, native } from "../src/native";

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
