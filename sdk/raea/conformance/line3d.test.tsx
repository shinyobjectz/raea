// Lines are lines (docs/stage-guards.feature SG6): a curve is one part at every frame of its growth, never a chain of
// parts mounted one a frame. Line3d is a flat line of a width with round joins and ends, Tube a round one.
import { describe, expect, test } from "bun:test";
import { Line3d, Tube } from "../src/mesh";
import { drawn, planeNormal, ribbon, tube } from "@raea/render/3d/line";
import { renderFrame } from "../src/test";

type V = [number, number, number];
const html = (el: React.ReactElement) => renderFrame(el, { frame: 0, width: 320, height: 240, fps: 30, durationInFrames: 30 });
const L: V[] = [[0, 0, 0], [1, 0, 0], [1, 1, 0]];
const arc: V[] = Array.from({ length: 33 }, (_, i) => [Math.cos((Math.PI * i) / 32), Math.sin((Math.PI * i) / 32), 0]);
const near = (a: number[], b: number[], e = 1e-9) => a.every((x, i) => Math.abs(x - b[i]) <= e);
const verts = (ps: number[]) => Array.from({ length: ps.length / 3 }, (_, i) => ps.slice(3 * i, 3 * i + 3) as V);

/** A point's distance from the polyline through pts. */
function fromLine(q: V, pts: V[]): number {
  let best = Infinity;
  for (let i = 0; i + 1 < pts.length; i++) {
    const a = pts[i], b = pts[i + 1], ab = b.map((x, k) => x - a[k]), aq = q.map((x, k) => x - a[k]);
    const t = Math.max(0, Math.min(1, aq.reduce((s, x, k) => s + x * ab[k], 0) / ab.reduce((s, x) => s + x * x, 0)));
    best = Math.min(best, Math.hypot(...q.map((x, k) => x - (a[k] + t * ab[k]))));
  }
  return best;
}

describe("SG6: lines are lines", () => {
  test("a growing curve is one part, by its own name, at every share drawn", () => {
    for (const p of [0.02, 0.25, 0.5, 0.75, 1]) {
      const out = html(<Line3d name="curve" points={arc} p={p} />);
      expect(out.match(/data-3d="Mesh"/g)?.length).toBe(1);
      expect(out.match(/data-shape="buffer:/g)?.length).toBe(1);
      expect(out).toContain("&quot;name&quot;:&quot;curve&quot;");
      const tubeOut = html(<Tube name="wire" points={arc} p={p} />);
      expect(tubeOut.match(/data-3d="Mesh"/g)?.length).toBe(1);
    }
    expect(html(<Line3d points={arc} p={0} />)).not.toContain('data-3d="Mesh"');
  });

  test("drawn up to p of its length, ending between two points", () => {
    expect(drawn("Line3d", L, 0.75)).toEqual([[0, 0, 0], [1, 0, 0], [1, 0.5, 0]]);
    expect(drawn("Line3d", L, 0.5)).toEqual([[0, 0, 0], [1, 0, 0]]);
    expect(drawn("Line3d", L, 1)).toEqual(L);
    expect(drawn("Line3d", [[0, 0, 0], [0, 0, 0], [2, 0, 0]], 0.25)).toEqual([[0, 0, 0], [0.5, 0, 0]]);
    expect(drawn("Line3d", L, 0)).toEqual([]);
  });

  test("a flat line keeps its width everywhere, faces its plane on both sides, and is round at its joins", () => {
    const m = ribbon(L, 0.05, [0, 0, 1]);
    const vs = verts(m.positions);
    expect(Math.max(...vs.map((q) => fromLine(q, L)))).toBeLessThanOrEqual(0.05 + 1e-9);
    // every vertex lies in the line's plane, and the corner's outside is covered by its round join
    expect(vs.every((q) => q[2] === 0)).toBe(true);
    expect(vs.some((q) => near(q, [1 + 0.05 * Math.cos(-Math.PI / 4), 0.05 * Math.sin(-Math.PI / 4), 0], 1e-9))).toBe(true);
    expect(new Set(verts(m.normals).map((n) => n.join())).size).toBe(2);
    expect(m.indices.length % 3).toBe(0);
    expect(planeNormal(arc.map(([x, y]) => [x, 0, y] as V)).map(Math.abs)).toEqual([0, 1, 0]);
    expect(planeNormal([[0, 0, 0], [1, 1, 1], [2, 2, 2]])).toEqual([0, 0, 1]);
  });

  test("a tube is round at its radius along the whole line, its rings never twist, and its ends are capped", () => {
    const sides = 12, m = tube(arc, 0.02, sides);
    const ring = verts(m.positions).slice(0, arc.length * sides);
    expect(Math.max(...ring.map((q) => Math.abs(fromLine(q, arc) - 0.02)))).toBeLessThan(2e-3);
    // vertex j of one ring and of the next point nearly the same way (a twist would turn them apart)
    const dir = (i: number, j: number) => ring[i * sides + j].map((x, k) => (x - arc[i][k]) / 0.02);
    for (let i = 0; i + 1 < arc.length; i++) expect(dir(i, 0).reduce((s, x, k) => s + x * dir(i + 1, 0)[k], 0)).toBeGreaterThan(0.9);
    // two caps of a centre and a ring each
    expect(m.positions.length / 3).toBe(arc.length * sides + 2 * (1 + sides));
    expect(m.indices.length / 3).toBe((arc.length - 1) * sides * 2 + 2 * sides);
  });

  test("refusals name the prop", () => {
    expect(() => html(<Line3d points={[[0, 0, 0]]} />)).toThrow("Line3d: points is two or more [x, y, z]");
    expect(() => html(<Line3d points={L} p={1.5} />)).toThrow("Line3d: p is how much of it is drawn, 0 to 1, got 1.5");
    expect(() => html(<Line3d points={L} width={0} />)).toThrow("Line3d: width is in metres, more than 0, got 0");
    expect(() => html(<Tube points={L} sides={2} />)).toThrow("Tube: sides is a whole number, 3 or more, got 2");
    expect(() => html(<Tube points={L} radius={-1} />)).toThrow("Tube: radius is in metres, more than 0, got -1");
  });
});
