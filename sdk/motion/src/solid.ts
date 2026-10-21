// Solids: Manifold geometry held by the engine. `manifold.box(2, 1, 0.3)` returns a Solid, a handle to a native solid;
// booleans and transforms make new handles from native solids, and volume, bounds and slices are answered natively.
// A Solid also knows the expression that rebuilds it, which is how a <Mesh shape={solid}> reaches the renderer.

import { native } from "./native";

export type Vec3 = [number, number, number];

export interface Bounds {
  min: Vec3;
  max: Vec3;
}

/** A cross-section of a solid at a height: its area, how many contours, and the contours themselves. */
export interface Section {
  area: number;
  contours: number;
  polygons: [number, number][][];
}

function finite(owner: string, ns: unknown[]): number[] {
  for (const n of ns) {
    if (typeof n !== "number" || !Number.isFinite(n)) throw new TypeError(`${owner}: numbers only, got ${JSON.stringify(n)}`);
  }
  return ns as number[];
}

/** A handle to a native solid. Free it when done with many (or let the run end). */
export class Solid {
  readonly id: number;
  /** the expression that rebuilds this solid (what a mesh row draws) */
  readonly expr: string;

  constructor(id: number, expr: string) {
    this.id = id;
    this.expr = expr;
  }

  private static made(out: { id: number; expr: string }): Solid {
    return new Solid(out.id, out.expr);
  }

  private combine(fn: "union" | "minus" | "intersect", others: Solid[]): Solid {
    if (others.length === 0) throw new TypeError(`${fn}: give at least one other solid`);
    return Solid.made(native("solid.combine", { fn, ids: [this.id, ...others.map((o) => o.id)] }));
  }

  /** This and the others together. */
  union(...others: Solid[]): Solid {
    return this.combine("union", others);
  }
  /** This with the others cut away. */
  minus(...others: Solid[]): Solid {
    return this.combine("minus", others);
  }
  /** Only where this and every other overlap. */
  intersect(...others: Solid[]): Solid {
    return this.combine("intersect", others);
  }
  /** The convex hull of this and the others. */
  hull(...others: Solid[]): Solid {
    return Solid.made(native("solid.combine", { fn: "hull", ids: [this.id, ...others.map((o) => o.id)] }));
  }
  /** Moved by (x, y, z). */
  move(x: number, y: number, z: number): Solid {
    return Solid.made(native("solid.move", { id: this.id, fn: "move", args: finite("move", [x, y, z]) }));
  }
  /** Turned by x, y and z degrees. */
  rotate(x: number, y: number, z: number): Solid {
    return Solid.made(native("solid.move", { id: this.id, fn: "rotate", args: finite("rotate", [x, y, z]) }));
  }
  /** Scaled uniformly, or per axis. */
  scale(s: number, y?: number, z?: number): Solid {
    const args = y === undefined ? [s] : [s, y, z ?? 1];
    return Solid.made(native("solid.move", { id: this.id, fn: "scale", args: finite("scale", args) }));
  }
  /** Its volume. */
  volume(): number {
    return native("solid.query", { id: this.id, what: "volume" }) as number;
  }
  /** Its bounding box. */
  bounds(): Bounds {
    return native("solid.query", { id: this.id, what: "bounds" }) as Bounds;
  }
  /** How many triangles its mesh has. */
  triangles(): number {
    return native("solid.query", { id: this.id, what: "triangles" }) as number;
  }
  /** Whether nothing is left (a boolean that removed everything). */
  isEmpty(): boolean {
    return native("solid.query", { id: this.id, what: "empty" }) as boolean;
  }
  /** Its cross-section at height z (in a space-time mask, z is the frame). */
  slice(z: number): Section {
    return native("solid.query", { id: this.id, what: "slice", z: finite("slice", [z])[0] }) as Section;
  }
  /** Mirrored across the plane through the origin with this normal. */
  mirror(nx: number, ny: number, nz: number): Solid {
    return op(this, "mirror", finite("mirror", [nx, ny, nz]));
  }
  /** Transformed by a 3x4 matrix given column by column (x axis, y axis, z axis, translation). */
  transform(m: number[]): Solid {
    if (m.length !== 12) throw new TypeError("transform: 12 numbers (a 3x4 matrix, by columns)");
    return op(this, "transform", finite("transform", m));
  }
  /** What's on the normal's side of the plane n . p = offset. */
  trim(nx: number, ny: number, nz: number, offset = 0): Solid {
    return op(this, "trim", finite("trim", [nx, ny, nz, offset]));
  }
  /** Cut in two by the plane n . p = offset: [the normal's side, the other side]. */
  splitByPlane(nx: number, ny: number, nz: number, offset = 0): [Solid, Solid] {
    return [this.trim(nx, ny, nz, offset), this.trim(-nx, -ny, -nz, -offset)];
  }
  /** Cut in two by another solid: [inside it, outside it]. */
  split(cutter: Solid): [Solid, Solid] {
    return [this.intersect(cutter), this.minus(cutter)];
  }
  /** Each triangle cut into n x n. */
  refine(n: number): Solid {
    return op(this, "refine", finite("refine", [n]));
  }
  /** Edges sharper than `angle` degrees kept, the rest rounded (smoothness 0..1 for the sharp ones too), then refined. */
  smooth(o: { angle?: number; smoothness?: number; refine?: number } = {}): Solid {
    return op(this, "smooth", finite("smooth", [o.angle ?? 60, o.smoothness ?? 0, o.refine ?? 4]));
  }
  /** This swept by another solid (the Minkowski sum): rounded by a sphere, thickened by a box. */
  minkowski(other: Solid): Solid {
    return op(this, "minkowski", [], other);
  }
  /** This eroded by another solid (the Minkowski difference). */
  minkowskiDiff(other: Solid): Solid {
    return op(this, "minkowski_diff", [], other);
  }
  /** Its triangles: positions (x y z) and indices by threes. */
  mesh(): { verts: Float32Array; tris: Uint32Array } {
    const m = native("solid.mesh", { id: this.id });
    return { verts: Float32Array.from(m.verts), tris: Uint32Array.from(m.tris) };
  }
  /** Every vertex moved by `fn` (the topology kept): a bend, a twist, a wave. */
  warp(fn: (x: number, y: number, z: number) => [number, number, number]): Solid {
    const m = this.mesh();
    const v = m.verts;
    for (let i = 0; i < v.length; i += 3) {
      const [x, y, z] = fn(v[i], v[i + 1], v[i + 2]);
      v[i] = x;
      v[i + 1] = y;
      v[i + 2] = z;
    }
    return manifold.fromMesh(v, m.tris);
  }
  /** The mesh coarsened: vertices closer than `tolerance` merged where the shape allows. */
  simplify(tolerance: number): Solid {
    return op(this, "simplify", finite("simplify", [tolerance]));
  }
  /** Refined until no edge is longer than `length`. */
  refineToLength(length: number): Solid {
    return op(this, "refine_length", finite("refineToLength", [length]));
  }
  /** Refined until its smoothed surface is within `tolerance` of the true curve. */
  refineToTolerance(tolerance: number): Solid {
    return op(this, "refine_tolerance", finite("refineToTolerance", [tolerance]));
  }
  /** Its cross-section at height z as a CrossSection (to offset, combine or extrude again). */
  crossSection(z: number): CrossSection {
    return CrossSection.from(native("solid.section", { id: this.id, z: finite("crossSection", [z])[0] }));
  }
  /** Its silhouette seen from above (+z), on the x-y plane. */
  project(): CrossSection {
    return CrossSection.from(native("solid.section", { id: this.id }));
  }
  /** Its disconnected pieces, each a Solid. */
  decompose(): Solid[] {
    return (native("solid.decompose", { id: this.id }) as { id: number; expr: string }[]).map((o) => new Solid(o.id, o.expr));
  }
  /** The smallest gap between this and another, looking no further than `searchLength` (which it answers when nothing is closer). */
  minGap(other: Solid, searchLength: number): number {
    return native("solid.query", { id: this.id, what: "min_gap", other: other.id, z: finite("minGap", [searchLength])[0] }) as number;
  }
  /** Where the segment from `from` to `to` crosses the surface, nearest first (distance 0..1 along it). */
  rayCast(from: Vec3, to: Vec3): { distance: number; position: Vec3; normal: Vec3 }[] {
    return native("solid.query", { id: this.id, what: "ray", args: finite("rayCast", [...from, ...to]) }) as { distance: number; position: Vec3; normal: Vec3 }[];
  }
  /** Its surface area. */
  area(): number {
    return native("solid.query", { id: this.id, what: "area" }) as number;
  }
  /** Its genus (how many holes go through it: a torus has 1). */
  genus(): number {
    return native("solid.query", { id: this.id, what: "genus" }) as number;
  }
  /** Written as OBJ or glTF binary (by the extension, or `format`); the file's size. */
  export(path: string, format?: "obj" | "glb"): number {
    return native("solid.export", format ? { id: this.id, path, format } : { id: this.id, path }).bytes;
  }
  /** Let the engine drop it. */
  free(): void {
    native("solid.free", { id: this.id });
  }
}

function op(base: Solid, fn: "mirror" | "transform" | "trim" | "refine" | "refine_length" | "refine_tolerance" | "smooth" | "simplify" | "minkowski" | "minkowski_diff", args: number[], other?: Solid): Solid {
  const out = native("solid.op", other ? { id: base.id, fn, args, other: other.id } : { id: base.id, fn, args });
  return new Solid(out.id, out.expr);
}

/** A 2D outline: contours of [x, y] points (counter-clockwise for solid, clockwise for a hole). */
export type Polygons = [number, number][][];

function checkPolygons(f: string, p: Polygons): Polygons {
  if (!Array.isArray(p) || !p.length || p.some((c) => !Array.isArray(c) || c.length < 3)) throw new TypeError(`${f}: contours of 3 or more [x, y] points`);
  for (const c of p) for (const pt of c) finite(f, pt);
  return p;
}

type SectionAnswer = { polygons: Polygons; area: number; contours: number; bounds: { min: [number, number]; max: [number, number] } };
type JoinType = "square" | "round" | "miter" | "bevel";
const JOINS: Record<JoinType, number> = { square: 0, round: 1, miter: 2, bevel: 3 };

function section(fn: string, polygons: Polygons | undefined, args: number[] = [], others?: CrossSection[]): SectionAnswer {
  const call: Record<string, unknown> = { fn, args };
  if (polygons) call.polygons = polygons;
  if (others) call.others = others.map((o) => o.polygons);
  return native("section.op", call as never) as SectionAnswer;
}

/** A 2D shape (Manifold's CrossSection), held by value: its contours (counter-clockwise solid, clockwise holes),
 * area and bounds. Offset, combine and move it, then extrude or revolve it into a Solid. */
export class CrossSection {
  readonly polygons: Polygons;
  readonly area: number;
  readonly contours: number;
  readonly bounds: { min: [number, number]; max: [number, number] };

  private constructor(a: SectionAnswer) {
    this.polygons = a.polygons;
    this.area = a.area;
    this.contours = a.contours;
    this.bounds = a.bounds;
  }
  /** From an engine answer (a slice, a projection, an operation). */
  static from(a: SectionAnswer): CrossSection {
    return new CrossSection(a);
  }
  /** Contours, overlaps merged and self-crossings resolved (Manifold's positive fill rule). */
  static of(polygons: Polygons): CrossSection {
    return new CrossSection(section("make", checkPolygons("crossSection.of", polygons)));
  }
  private static check(f: string, others: CrossSection[]): CrossSection[] {
    if (others.length === 0) throw new TypeError(`${f}: give at least one other cross-section`);
    for (const o of others) if (!(o instanceof CrossSection)) throw new TypeError(`${f}: cross-sections only`);
    return others;
  }
  /** This and the others together. */
  union(...others: CrossSection[]): CrossSection {
    return new CrossSection(section("union", this.polygons, [], CrossSection.check("union", others)));
  }
  /** This with the others cut away. */
  minus(...others: CrossSection[]): CrossSection {
    return new CrossSection(section("minus", this.polygons, [], CrossSection.check("minus", others)));
  }
  /** Only where this and every other overlap. */
  intersect(...others: CrossSection[]): CrossSection {
    return new CrossSection(section("intersect", this.polygons, [], CrossSection.check("intersect", others)));
  }
  /** The convex hull of this and the others. */
  hull(...others: CrossSection[]): CrossSection {
    return new CrossSection(section("hull", this.polygons, [], others));
  }
  /** Grown outward by `delta` (shrunk when negative), corners joined round (default), square, miter or bevel. */
  offset(delta: number, o: { join?: JoinType; miterLimit?: number; segments?: number } = {}): CrossSection {
    const join = o.join ?? "round";
    if (!(join in JOINS)) throw new TypeError(`offset: join is square, round, miter or bevel, got ${JSON.stringify(join)}`);
    return new CrossSection(section("offset", this.polygons, finite("offset", [delta, JOINS[join], o.miterLimit ?? 2, o.segments ?? 0])));
  }
  /** Swept by another shape (the 2D Minkowski sum). */
  minkowski(other: CrossSection): CrossSection {
    return new CrossSection(section("minkowski", this.polygons, [], CrossSection.check("minkowski", [other])));
  }
  /** Fewer points: those within `epsilon` of a straight line dropped. */
  simplify(epsilon = 1e-6): CrossSection {
    return new CrossSection(section("simplify", this.polygons, finite("simplify", [epsilon])));
  }
  /** Moved by (x, y). */
  move(x: number, y: number): CrossSection {
    return new CrossSection(section("translate", this.polygons, finite("move", [x, y])));
  }
  /** Turned by `degrees` about the origin. */
  rotate(degrees: number): CrossSection {
    return new CrossSection(section("rotate", this.polygons, finite("rotate", [degrees])));
  }
  /** Scaled uniformly, or per axis. */
  scale(x: number, y = x): CrossSection {
    return new CrossSection(section("scale", this.polygons, finite("scale", [x, y])));
  }
  /** Mirrored across the line through the origin with this normal. */
  mirror(nx: number, ny: number): CrossSection {
    return new CrossSection(section("mirror", this.polygons, finite("mirror", [nx, ny])));
  }
  /** Every point moved by `fn`, then re-unioned (crossings resolved). */
  warp(fn: (x: number, y: number) => [number, number]): CrossSection {
    return CrossSection.of(this.polygons.map((c) => c.map(([x, y]) => fn(x, y))));
  }
  /** Its separate pieces (each outline with its holes). */
  decompose(): CrossSection[] {
    return (native("section.op", { fn: "decompose", polygons: this.polygons, args: [] } as never) as SectionAnswer[]).map((a) => new CrossSection(a));
  }
  /** Whether nothing is left. */
  isEmpty(): boolean {
    return this.polygons.length === 0 || this.area === 0;
  }
  /** Extruded along z into a Solid (see manifold.extrude). */
  extrude(height: number, o: { divisions?: number; twist?: number; scaleTop?: [number, number] } = {}): Solid {
    return manifold.extrude(this, height, o);
  }
  /** Revolved about the z axis into a Solid (see manifold.revolve). */
  revolve(o: { degrees?: number; segments?: number } = {}): Solid {
    return manifold.revolve(this, o);
  }
}

/** 2D shapes as CrossSections, made natively. */
export const crossSection = {
  /** Contours, unioned. */
  of: (polygons: Polygons) => CrossSection.of(polygons),
  /** A circle of radius r, centred (segments chosen from the size when not given). */
  circle: (r: number, segments = 0) => CrossSection.from(section("circle", undefined, finite("crossSection.circle", [r, segments]))),
  /** A w x h rectangle, centred (or from the origin with `centered: false`). */
  rect: (w: number, h: number, centered = true) => CrossSection.from(section("square", undefined, finite("crossSection.rect", [w, h, centered ? 1 : 0]))),
  /** One contour from points. */
  polygon: (points: [number, number][]) => CrossSection.of([points]),
  /** The convex hull of points. */
  hull: (points: [number, number][]) => {
    if (!Array.isArray(points) || points.length < 3) throw new TypeError("crossSection.hull: 3 or more [x, y] points");
    for (const p of points) finite("crossSection.hull", p);
    return CrossSection.from(section("hull_points", [points]));
  },
};

function outlineOf(f: string, p: Polygons | CrossSection): Polygons {
  if (p instanceof CrossSection) {
    if (p.isEmpty()) throw new TypeError(`${f}: the cross-section is empty`);
    return p.polygons;
  }
  return checkPolygons(f, p);
}

type Primitive = "box" | "rounded_box" | "sphere" | "cylinder" | "cone" | "torus" | "text3d";

function make(fn: Primitive, args: (number | string)[]): Solid {
  const out = native("solid.make", { fn, args });
  return new Solid(out.id, out.expr);
}

function batch(fn: "union" | "minus" | "intersect", solids: Solid[]): Solid {
  if (solids.length < 2) throw new TypeError(`${fn === "minus" ? "difference" : fn === "intersect" ? "intersection" : "union"}: give two solids or more`);
  return solids[0][fn](...solids.slice(1));
}

/** Manifold solids, built natively. Sizes are the scene's units; text3d uses the bundled faces. */
/** 2D outlines for extrude and revolve. */
export const outline = {
  /** A circle of radius r, centred, as `segments` points. */
  circle: (r: number, segments = 64): Polygons => [Array.from({ length: segments }, (_, i) => [r * Math.cos((2 * Math.PI * i) / segments), r * Math.sin((2 * Math.PI * i) / segments)] as [number, number])],
  /** A w x h rectangle, centred (or from the origin with `centered: false`). */
  rect: (w: number, h: number, centered = true): Polygons => {
    const [x, y] = centered ? [-w / 2, -h / 2] : [0, 0];
    return [[[x, y], [x + w, y], [x + w, y + h], [x, y + h]]];
  },
  /** An outline from points (one contour). */
  polygon: (points: [number, number][]): Polygons => [points],
};

export const manifold = {
  /** A w x h x d box, centred. */
  box: (w: number, h: number, d: number) => make("box", finite("box", [w, h, d])),
  /** A box with its edges rounded by r. */
  roundedBox: (w: number, h: number, d: number, r: number) => make("rounded_box", finite("roundedBox", [w, h, d, r])),
  sphere: (r: number) => make("sphere", finite("sphere", [r])),
  /** A cylinder of radius r and height h along z, centred. */
  cylinder: (r: number, h: number) => make("cylinder", finite("cylinder", [r, h])),
  cone: (r1: number, r2: number, h: number) => make("cone", finite("cone", [r1, r2, h])),
  torus: (major: number, minor: number) => make("torus", finite("torus", [major, minor])),
  /** A 2D outline extruded along z by `height`: with divisions, a twist (degrees, top against bottom) and a top scale. */
  extrude: (polygons: Polygons | CrossSection, height: number, o: { divisions?: number; twist?: number; scaleTop?: [number, number] } = {}) => {
    const st = o.scaleTop ?? [1, 1];
    const out = native("solid.extrude", { polygons: outlineOf("extrude", polygons), args: finite("extrude", [height, o.divisions ?? 1, o.twist ?? 0, st[0], st[1]]) });
    return new Solid(out.id, out.expr);
  },
  /** A 2D outline (x >= 0) revolved about the z axis (the outline's y becomes z), by `degrees` (360) in `segments`. */
  revolve: (polygons: Polygons | CrossSection, o: { degrees?: number; segments?: number } = {}) => {
    const args = o.segments !== undefined ? [o.degrees ?? 360, o.segments] : o.degrees !== undefined ? [o.degrees] : [];
    const out = native("solid.revolve", { polygons: outlineOf("revolve", polygons), args: finite("revolve", args) });
    return new Solid(out.id, out.expr);
  },
  /** The surface of a signed distance function (negative inside) inside [min, max], sampled `resolution` steps a side. */
  levelSet: (sdf: (x: number, y: number, z: number) => number, o: { min: [number, number, number]; max: [number, number, number]; resolution?: number; level?: number }) => {
    const n = Math.max(2, Math.round(o.resolution ?? 32));
    finite("levelSet", [...o.min, ...o.max]);
    const grid: number[] = new Array(n * n * n);
    const at = (k: number, i: number) => o.min[k] + ((o.max[k] - o.min[k]) * i) / (n - 1);
    for (let z = 0; z < n; z++) for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) grid[x + n * (y + n * z)] = sdf(at(0, x), at(1, y), at(2, z));
    const out = native("solid.level_set", { grid, dims: [n, n, n], min: o.min, max: o.max, level: o.level ?? 0 });
    return new Solid(out.id, out.expr);
  },
  /** The union of all of them in one batch (Manifold's BatchBoolean): faster than chaining for many. */
  union: (...solids: Solid[]) => batch("union", solids),
  /** The first with all the others cut away, in one batch. */
  difference: (...solids: Solid[]) => batch("minus", solids),
  /** Where all of them overlap, in one batch. */
  intersection: (...solids: Solid[]) => batch("intersect", solids),
  /** The convex hull of solids (`hull(a, b, c)`), or of points (`hull([[x, y, z], ...])`). */
  hull: (...items: Solid[] | [Vec3[]]) => {
    if (items.length === 1 && Array.isArray(items[0])) {
      const pts = items[0] as Vec3[];
      if (pts.length < 4) throw new TypeError("hull: 4 or more [x, y, z] points");
      for (const p of pts) finite("hull", p);
      const out = native("solid.hull_points", { points: pts });
      return new Solid(out.id, out.expr);
    }
    const solids = items as Solid[];
    if (solids.length === 0) throw new TypeError("hull: give solids or points");
    return solids[0].hull(...solids.slice(1));
  },
  /** Solids that don't overlap held as one, with no boolean (Manifold's Compose). */
  compose: (...solids: Solid[]) => {
    if (solids.length === 0) throw new TypeError("compose: give at least one solid");
    const out = native("solid.compose", { ids: solids.map((s) => s.id) });
    return new Solid(out.id, out.expr);
  },
  /** A solid from closed triangles: positions (x y z) and indices by threes. */
  fromMesh: (verts: ArrayLike<number>, tris: ArrayLike<number>) => {
    const out = native("solid.from_mesh", { verts: Array.from(verts), tris: Array.from(tris) });
    return new Solid(out.id, out.expr);
  },
  /** A solid read from an OBJ or a glTF binary (.glb) file. */
  import: (path: string) => {
    const out = native("solid.import", { path });
    return new Solid(out.id, out.expr);
  },
  /** A word as a solid: `size` units an em, `depth` deep, in a bundled face. */
  text3d: (text: string, size: number, depth: number, font = "Inter", weight = 800) => {
    if (typeof text !== "string" || text === "") throw new TypeError("text3d: give the text");
    return make("text3d", [text, ...finite("text3d", [size, depth]), font, ...finite("text3d", [weight])]);
  },
};
