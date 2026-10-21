// @raea/motion/room: rooms laid out by relation. A room is a box of walls, each wall a frame (where it starts, which
// way it runs, which way is in); a run lays units (cupboards, appliances, wall cabinets) along a wall one after
// another, each set back against the wall and against the one before from its own measured box (scene.bounds, turned
// so its front faces into the room), never by typed coordinates; openings cut windows and doors into walls. The plan
// (rooms, runs, islands, the work points) is checked against a kitchen's clearances: `checkLayout`.
//
//   const room = defineRoom({ width: 5.4, depth: 4.8, height: 2.7 });
//   const back = layRun(room, "back", 0, [{ src: PANTRY }, { src: CUPBOARD_120 }, { src: SINK, role: "sink" }]);
//   <Room room={room} openings={[{ wall: "back", at: 3.6, width: 1.0, bottom: 1.15, top: 2.2, kind: "window" }]} />
//   <Run run={back} />
//   <Place src={MUG} on={COUNTER} at={onRun(back, 2.0, 0.3)} />
import type React from "react";
import { useCurrentFrame, useVideoConfig } from "./clock";
import { Box, Group, Mesh, MeshPhysicalMaterial, MeshStandardMaterial, Model, type Vec3 } from "./3d";
import { hasNative, native } from "./native";
import { modelBounds } from "./scene";

export type WallName = "back" | "right" | "front" | "left";

export interface RoomSpec {
  /** along x, metres (the back wall's length) */
  width: number;
  /** along z (the side walls' length) */
  depth: number;
  height: number;
}

/** A wall's frame: its interior face starts at `origin` (on the floor) and runs `length` along `along`; `inward` points
 * into the room. Walls go round clockwise seen from above: back (z = 0, along +x), right (x = width, along +z), front
 * (z = depth, along -x), left (x = 0, along -z), so a run turning a corner carries on along the next. */
export interface WallFrame {
  name: WallName;
  origin: Vec3;
  along: Vec3;
  inward: Vec3;
  length: number;
  /** the turn about +y that takes a unit's +x along the wall and its +z into the room */
  yaw: number;
}

export interface Room extends RoomSpec {
  walls: Record<WallName, WallFrame>;
}

export function defineRoom(spec: RoomSpec): Room {
  const { width: w, depth: d } = spec;
  const frame = (name: WallName, origin: Vec3, along: Vec3, length: number): WallFrame => {
    const inward: Vec3 = [-along[2], 0, along[0]];
    return { name, origin, along, inward, length, yaw: Math.atan2(-along[2], along[0]) };
  };
  return {
    ...spec,
    walls: {
      back: frame("back", [0, 0, 0], [1, 0, 0], w),
      right: frame("right", [w, 0, 0], [0, 0, 1], d),
      front: frame("front", [w, 0, d], [-1, 0, 0], w),
      left: frame("left", [0, 0, d], [0, 0, -1], d),
    },
  };
}

/** A point `u` along a wall, `v` into the room from its face, `y` up. */
export function onWall(f: WallFrame, u: number, v = 0, y = 0): Vec3 {
  return [f.origin[0] + f.along[0] * u + f.inward[0] * v, y, f.origin[2] + f.along[2] * u + f.inward[2] * v];
}

/** Where on a wall's face a <Decal> lies (`u` along it, `y` up) and the way the wall faces: spread into a Decal
 * (`<Decal src {...onSurface(room, "back", 2, 1.2)} size={0.3} />`), the picture upright on the wall. */
export function onSurface(room: Room, wall: WallName, u: number, y: number): { position: Vec3; normal: Vec3 } {
  const f = room.walls[wall];
  return { position: onWall(f, u, 0, y), normal: f.inward };
}

/** Which of a model's own axes is its front (the pack's cupboards face +z; its appliances +x or -x). */
export type Facing = "+x" | "-x" | "+z" | "-z";
const FACE_YAW: Record<Facing, number> = { "+z": 0, "+x": -Math.PI / 2, "-x": Math.PI / 2, "-z": Math.PI };

export interface Unit {
  src: string;
  /** its front (default "+z") */
  front?: Facing;
  /** room along the wall it takes, if not its measured width (an appliance in a slot wider than itself) */
  width?: number;
  /** set back off the wall (metres) */
  off?: number;
  /** its part in the kitchen's work: the work triangle's corners, the landing space beside them */
  role?: "sink" | "stove" | "fridge";
  name?: string;
  /** dressing in the unit's own frame (x along the wall from its left edge, z into the room from the wall): its doors,
   * drawers, handles, an appliance in a housing */
  children?: React.ReactNode;
}

/** A unit laid in its run: where along the wall it starts and ends, how deep it stands, and its model's offset. */
export interface Laid extends Unit {
  u0: number;
  u1: number;
  depth: number;
  /** the model in the unit's frame: turned to face into the room, its box's back left corner at the frame's origin */
  rotation: Vec3;
  position: Vec3;
  /** its top and its bottom over the floor (a wall cabinet's bottom is its underside) */
  height: number;
  bottom: number;
}

export interface LaidRun {
  wall: WallFrame;
  units: Laid[];
  /** where the run ends along its wall */
  end: number;
}

/** Units one after another along a wall from `from`, each against the wall and the one before, from their boxes. */
export function layRun(room: Room, wall: WallName, from: number, units: Unit[]): LaidRun {
  const f = room.walls[wall];
  let u = from;
  const laid = units.map((it) => {
    const rotation: Vec3 = [0, FACE_YAW[it.front ?? "+z"], 0];
    const b = modelBounds(it.src, rotation);
    const w = it.width ?? b.max[0] - b.min[0];
    // centred in its slot along the wall, its back on the wall
    const slack = w - (b.max[0] - b.min[0]);
    const position: Vec3 = [-b.min[0] + slack / 2, 0, -b.min[2] + (it.off ?? 0)];
    const l: Laid = { ...it, u0: u, u1: u + w, depth: b.max[2] - b.min[2] + (it.off ?? 0), rotation, position, height: b.max[1], bottom: b.min[1] };
    u += w;
    return l;
  });
  if (u > f.length + 1e-3) throw new Error(`layRun: the ${wall} run reaches ${u.toFixed(3)} m, past the wall's ${f.length} m`);
  return { wall: f, units: laid, end: u };
}

/** A point on a run's counter: `u` along its wall, `v` in from the wall. */
export function onRun(run: LaidRun, u: number, v: number, y = 0): Vec3 {
  return onWall(run.wall, u, v, y);
}

/** A laid unit's centre on the floor, in the world. */
export function unitCentre(run: LaidRun, l: Laid): Vec3 {
  return onWall(run.wall, (l.u0 + l.u1) / 2, l.depth / 2);
}

/** A run drawn: each unit at its place, turned into the room, with its dressing. */
export function Run({ run }: { run: LaidRun }) {
  const f = run.wall;
  return (
    <>
      {run.units.map((l, i) => (
        <Group key={i} position={onWall(f, l.u0)} rotation={[0, f.yaw, 0]}>
          <Model name={l.name} src={l.src} rotation={l.rotation} position={l.position} />
          {l.children}
        </Group>
      ))}
    </>
  );
}

/** A stretch of backsplash: tile on one wall from `u0` to `u1` along it, `y0` to `y1` up. */
export interface Splash {
  wall: WallName;
  u0: number;
  u1: number;
  y0: number;
  y1: number;
}

/** What stops a backsplash's top over a stretch of wall: a wall cabinet's or a hood's underside. */
export interface SplashCap {
  wall: WallName;
  u0: number;
  u1: number;
  bottom: number;
}

/** A backsplash round the kitchen: tile on the wall over every counter of `runs`, on each wall a unit stands against (a
 * corner unit's two), from its top (the stone's back lip) up to what's over it, a wall cabinet of `over` or a `caps`
 * (a hood, the tile up behind it), else `height` over the stone; under a window, up to its sill's underside (`sill` below the hole; `trim` either
 * side of it for the frame). A unit standing more than `counter` + 0.15 m (a pantry, a fridge's housing) breaks it, a
 * stove doesn't (the tile runs behind it, from the stone's height).
 *
 *   <Backsplash pieces={backsplash(ROOM, [BACK, RIGHT], { counter: 1.0, over: [BACK_UP, RIGHT_UP], openings })}
 *     src={staticFile("tiles.jpg")} tile={0.6} /> */
export function backsplash(room: Room, runs: LaidRun[], o: { counter: number; over?: LaidRun[]; caps?: SplashCap[]; openings?: Opening[]; height?: number; trim?: number; sill?: number; against?: number }): Splash[] {
  const { counter, over = [], caps = [], openings = [], height = 0.55, trim = 0.1, sill = 0.05, against = 0.03 } = o;
  const tall = counter + 0.15;
  // a laid unit's footprint on a wall it stands against: its span along it
  const spanOn = (run: LaidRun, l: Laid, f: WallFrame): [number, number] | null => {
    const c = [onWall(run.wall, l.u0, 0), onWall(run.wall, l.u1, 0), onWall(run.wall, l.u0, l.depth), onWall(run.wall, l.u1, l.depth)];
    const uv = c.map((p) => {
      const d = [p[0] - f.origin[0], p[2] - f.origin[2]];
      return [d[0] * f.along[0] + d[1] * f.along[2], d[0] * f.inward[0] + d[1] * f.inward[2]];
    });
    if (Math.min(...uv.map((x) => x[1])) > against) return null;
    const u0 = Math.max(0, Math.min(...uv.map((x) => x[0]))), u1 = Math.min(f.length, Math.max(...uv.map((x) => x[0])));
    return u1 - u0 > 1e-3 ? [u0, u1] : null;
  };
  const out: Splash[] = [];
  for (const [name, f] of Object.entries(room.walls) as [WallName, WallFrame][]) {
    const low: { u0: number; u1: number; y: number }[] = [], high: [number, number][] = [];
    for (const run of runs) {
      for (const l of run.units) {
        const s = spanOn(run, l, f);
        if (!s) continue;
        if (l.height > tall) high.push(s);
        else low.push({ u0: s[0], u1: s[1], y: Math.max(l.height, counter) });
      }
    }
    if (!low.length) continue;
    const tops: { u0: number; u1: number; y: number }[] = [...caps.filter((c) => c.wall === name).map((c) => ({ u0: c.u0, u1: c.u1, y: c.bottom }))];
    for (const run of over) {
      for (const l of run.units) {
        const s = spanOn(run, l, f);
        if (s) tops.push({ u0: s[0], u1: s[1], y: l.bottom });
      }
    }
    for (const op of openings.filter((p) => p.wall === name && p.kind === "window")) {
      tops.push({ u0: op.at - op.width / 2 - trim, u1: op.at + op.width / 2 + trim, y: op.bottom - sill });
    }
    // cut at every edge (edges within 2 cm one cut: a window's trim by a sink's side), and each piece between takes its
    // lowest top and its highest foot
    const cuts = [...low.flatMap((s) => [s.u0, s.u1]), ...high.flat(), ...tops.flatMap((t) => [t.u0, t.u1])].sort((a, b) => a - b)
      .filter((u, i, all) => i === 0 || u - all[i - 1] >= 0.02);
    for (let i = 0; i + 1 < cuts.length; i++) {
      const [a, b] = [cuts[i], cuts[i + 1]], m = (a + b) / 2;
      if (b - a < 1e-3 || high.some(([h0, h1]) => m > h0 && m < h1)) continue;
      const under = low.filter((s) => m > s.u0 && m < s.u1);
      if (!under.length) continue;
      const y0 = Math.max(...under.map((s) => s.y));
      const over = tops.filter((t) => m > t.u0 && m < t.u1).map((t) => t.y);
      const y1 = over.length ? Math.min(...over) : counter + height;
      if (y1 - y0 < 0.02) continue;
      const last = out[out.length - 1];
      if (last && last.wall === name && Math.abs(last.u1 - a) < 1e-4 && Math.abs(last.y0 - y0) < 1e-4 && Math.abs(last.y1 - y1) < 1e-4) last.u1 = b;
      else out.push({ wall: name, u0: a, u1: b, y0, y1 });
    }
  }
  return out;
}

/** A backsplash drawn: each piece a slab `thick` on its wall, its tile picture (`src`, `normalMap`) repeating every
 * `tile` metres from the piece's corner, glazed (`roughness`). */
export function Backsplash({ room, pieces, src, normalMap, tile, color, roughness = 0.25, thick = 0.008 }: { room: Room; pieces: Splash[]; src: string; normalMap?: string; tile: number; color?: string; roughness?: number; thick?: number }) {
  return (
    <>
      {pieces.map((p, i) => {
        const f = room.walls[p.wall], w = p.u1 - p.u0, h = p.y1 - p.y0;
        return (
          <Mesh key={i} name={`backsplash-${p.wall}-${i}`} position={onWall(f, (p.u0 + p.u1) / 2, thick / 2, (p.y0 + p.y1) / 2)} rotation={[0, f.yaw, 0]}>
            <Box args={[w, h, thick]} />
            <MeshStandardMaterial map={src} normalMap={normalMap} color={color} roughness={roughness} repeat={[w / tile, h / tile]} />
          </Mesh>
        );
      })}
    </>
  );
}

export interface Opening {
  wall: WallName;
  /** its middle along the wall */
  at: number;
  width: number;
  bottom: number;
  top: number;
  kind: "window" | "door" | "bay";
  /** drawn in the wall's frame at the opening's middle, on the floor (x along, z in): a frame, glass, a sill, a door */
  children?: React.ReactNode;
}

/** A bay window's make: `width` its mouth in the wall, `depth` how far it stands out, its sides at `angle` (degrees)
 * to the wall; windows from `sill` to `head` in each of its three faces; a window seat (`seat` high, cushioned) fills its
 * floor unless `seat` is false. */
export interface BayOptions {
  width: number;
  depth: number;
  angle?: number;
  sill?: number;
  head?: number;
  seat?: number | false;
  thick?: number;
  wall?: string;
  trim?: string;
  cushion?: string;
}

/** A bay window: the room's wall opened full height `width` wide at `at`, and out past it three faces in a trapezoid,
 * each a window between a panel under the sill and one over the head, mullion posts at its corners, its own floor and
 * ceiling, and a window seat to the trapezoid's shape. Its seat comes as a footprint that faces into the room, for the
 * plan's checks. */
export function bay(room: Room, wall: WallName, at: number, o: BayOptions): Opening & { seat?: Footprint } {
  const f = room.walls[wall], h = room.height;
  const { width: w, depth: d, angle = 45, sill = 0.55, head = 2.2, seat = 0.45, thick: t = 0.1 } = o;
  const [wallC, trim, cushion] = [o.wall ?? "#ece4d4", o.trim ?? "#f4efe4", o.cushion ?? "#c9b79c"];
  const s = d / Math.tan((angle * Math.PI) / 180);
  if (2 * s >= w) throw new Error(`bay: sides at ${angle} degrees out ${d} m meet before the back (mouth ${w} m)`);
  // the inside outline on the plan, in the wall's frame (x along, z into the room: the bay is at -z)
  const pts: [number, number][] = [[-w / 2, 0], [-w / 2 + s, -d], [w / 2 - s, -d], [w / 2, 0]];
  const faces = [0, 1, 2].map((i) => {
    const [a, b] = [pts[i], pts[i + 1]];
    const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
    const u: [number, number] = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
    // the outward normal (to the left walking a to b, the outline goes round the bay clockwise seen from above)
    const out: [number, number] = [u[1], -u[0]];
    return { mid: [(a[0] + b[0]) / 2 + (out[0] * t) / 2, (a[1] + b[1]) / 2 + (out[1] * t) / 2] as [number, number], len, yaw: Math.atan2(-u[1], u[0]) };
  });
  const glassH = head - sill, m = 0.07;
  // the outline for extrude, counter-clockwise (a clockwise one extrudes to nothing), turned up by rotate(-90, 0, 0)
  const poly = (z0: number) => [...pts].reverse().map(([x, z]) => `${x.toFixed(4)} ${(-z + z0).toFixed(4)}`).join(", ");
  const seatTop = seat === false ? 0 : seat;
  const inward = onWall(f, at, 1), here = onWall(f, at, 0);
  const seatFoot: Footprint | undefined = seat === false ? undefined : (() => {
    const c = [onWall(f, at - w / 2, 0), onWall(f, at + w / 2, 0), onWall(f, at - w / 2, -d), onWall(f, at + w / 2, -d)];
    return { name: `${wall} bay seat`, min: [Math.min(...c.map((p) => p[0])), Math.min(...c.map((p) => p[2]))], max: [Math.max(...c.map((p) => p[0])), Math.max(...c.map((p) => p[2]))], faces: [inward[0] - here[0], inward[2] - here[2]] };
  })();
  return {
    wall, at, width: w, bottom: 0, top: h, kind: "bay", seat: seatFoot,
    children: (
      <Group>
        {faces.map((fc, i) => (
          <Group key={i} position={[fc.mid[0], 0, fc.mid[1]]} rotation={[0, fc.yaw, 0]}>
            <Mesh name={`bay-${i}-under`} position={[0, sill / 2, 0]}><Box args={[fc.len, sill, t]} /><MeshStandardMaterial color={wallC} roughness={0.9} /></Mesh>
            <Mesh name={`bay-${i}-over`} position={[0, (h + head) / 2, 0]}><Box args={[fc.len, h - head, t]} /><MeshStandardMaterial color={wallC} roughness={0.9} /></Mesh>
            {/* the sash: a frame round the glass, a muntin across, the glass */}
            {[[0, head - m / 2, fc.len, m], [0, sill + m / 2, fc.len, m], [-fc.len / 2 + m / 2, (sill + head) / 2, m, glassH], [fc.len / 2 - m / 2, (sill + head) / 2, m, glassH], [0, sill + glassH * 0.62, fc.len, 0.035]].map(([x, y, bw, bh], k) => (
              <Mesh key={k} name={`bay-${i}-sash-${k}`} position={[x, y, 0]}><Box args={[bw, bh, t * 0.9]} /><MeshStandardMaterial color={trim} roughness={0.5} /></Mesh>
            ))}
            <Mesh name={`bay-${i}-glass`} position={[0, (sill + head) / 2, 0]} shadow={false}>
              <Box args={[fc.len - 2 * m, glassH - 2 * m, 0.006]} />
              <MeshPhysicalMaterial color="#dfe8ea" roughness={0.03} opacity={0.12} transparent clearcoat={1} />
            </Mesh>
          </Group>
        ))}
        {/* posts where the faces meet and where the bay meets the wall */}
        {pts.map(([x, z], i) => (
          <Mesh key={`p${i}`} name={`bay-post-${i}`} position={[x, h / 2, z - (i === 0 || i === 3 ? 0 : t / 2)]}><Box args={[t * 1.1, h, t * 1.1]} /><MeshStandardMaterial color={trim} roughness={0.5} /></Mesh>
        ))}
        <Mesh name="bay-floor" position={[0, -0.01, -(d + t) / 2]}><Box args={[w + 2 * t, 0.02, d + t]} /><MeshStandardMaterial color="#b08a62" roughness={0.7} /></Mesh>
        <Mesh name="bay-ceiling" position={[0, h + 0.02, -(d + t) / 2]}><Box args={[w + 2 * t, 0.04, d + t]} /><MeshStandardMaterial color="#f6f1e6" roughness={0.95} /></Mesh>
        {seat !== false && (
          <>
            <Mesh name="bay-seat" shape={`rotate(extrude("${poly(0)}", ${(seatTop - 0.07).toFixed(3)}), -90, 0, 0)`}><MeshStandardMaterial color={trim} roughness={0.55} /></Mesh>
            <Mesh name="bay-cushion" position={[0, seatTop - 0.07, 0]} shape={`rotate(extrude("${poly(0)}", 0.07), -90, 0, 0)`}><MeshStandardMaterial color={cushion} roughness={0.95} /></Mesh>
          </>
        )}
      </Group>
    ),
  };
}

/** The room's shell: floor, ceiling and walls (outside its interior faces, `thick` deep), cut for its openings.
 * `ceiling={false}` leaves the room open above, to look down on it as a plan. */
export function Room({ room, openings = [], thick = 0.1, wall = "#ece4d4", floor = "#b08a62", ceiling = "#f6f1e6", children }: { room: Room; openings?: Opening[]; thick?: number; wall?: string; floor?: string; ceiling?: string | false; children?: React.ReactNode }) {
  const { width: w, depth: d, height: h } = room;
  return (
    <>
      <Mesh name="floor" position={[w / 2, -0.01, d / 2]}><Box args={[w + 2 * thick, 0.02, d + 2 * thick]} /><MeshStandardMaterial color={floor} roughness={0.7} /></Mesh>
      {ceiling !== false && <Mesh name="ceiling" position={[w / 2, h + 0.02, d / 2]}><Box args={[w + 2 * thick, 0.04, d + 2 * thick]} /><MeshStandardMaterial color={ceiling} roughness={0.95} /></Mesh>}
      {(Object.keys(room.walls) as WallName[]).map((name) => {
        const f = room.walls[name];
        const holes = openings.filter((o) => o.wall === name).map((o) => [o.at - o.width / 2, o.at + o.width / 2, o.bottom, o.top] as const).sort((p, q) => p[0] - q[0]);
        // slabs between and round the holes; the wall runs on past both ends by its thickness, closing the corners
        const slabs: [number, number, number, number][] = [];
        let a = -thick;
        for (const [h0, h1, y0, y1] of holes) {
          if (h0 > a) slabs.push([a, h0, 0, h]);
          slabs.push([h0, h1, 0, y0], [h0, h1, y1, h]);
          a = h1;
        }
        if (a < f.length + thick) slabs.push([a, f.length + thick, 0, h]);
        return (
          <Group key={name} position={f.origin} rotation={[0, f.yaw, 0]}>
            {slabs.filter(([p, q, y0, y1]) => q - p > 1e-3 && y1 - y0 > 1e-3).map(([p, q, y0, y1], i) => (
              <Mesh key={i} name={`${name}-wall-${i}`} position={[(p + q) / 2, (y0 + y1) / 2, -thick / 2]}>
                <Box args={[q - p, y1 - y0, thick]} />
                <MeshStandardMaterial color={wall} roughness={0.9} />
              </Mesh>
            ))}
            {openings.filter((o) => o.wall === name).map((o, i) => <Group key={`o${i}`} position={[o.at, 0, 0]}>{o.children}</Group>)}
          </Group>
        );
      })}
      {children}
    </>
  );
}

// ---------------------------------------------------------------------------------------------------- on the walls

/** Which way a flat thing that hangs (a picture, a mirror, a shelf) faces, read from its box: its thinnest level axis
 * is its depth, and its back is the side its origin is on (a pack hangs its pictures by their backs). */
export function wallFacing(src: string): Facing {
  const b = modelBounds(src);
  const [dx, dz] = [b.max[0] - b.min[0], b.max[2] - b.min[2]];
  if (dx <= dz) return Math.abs(b.min[0]) < Math.abs(b.max[0]) ? "+x" : "-x";
  return Math.abs(b.min[2]) < Math.abs(b.max[2]) ? "+z" : "-z";
}

/** What decor hangs over: a span along the wall and the height of its top (a counter's stone, a sideboard, a bench). */
export interface Over {
  name: string;
  u0: number;
  u1: number;
  top: number;
}

/** A laid unit (or a span of a run from one unit to another) as something to hang over. */
export function overUnits(run: LaidRun, first: string, last = first): Over {
  const a = run.units.find((u) => u.name === first), b = run.units.find((u) => u.name === last);
  if (!a || !b) throw new Error(`overUnits: no unit ${!a ? first : last} in the ${run.wall.name} run`);
  return { name: a === b ? a.name! : `${a.name} to ${b.name}`, u0: a.u0, u1: b.u1, top: Math.max(a.height, b.height) };
}

/** A hung thing: on a wall, its span along it and up it, its model in the wall's frame. */
export interface Hung {
  name: string;
  src: string;
  wall: WallName;
  u0: number;
  u1: number;
  y0: number;
  y1: number;
  /** how far its back stands off the wall */
  off: number;
  over?: Over;
  /** its height given by the caller (a shelf, a clock over a door), not the art rule: its height isn't checked */
  given?: boolean;
  position: Vec3;
  rotation: Vec3;
}

/** The decorator's rules (metres): art centred 1.45-1.55 m up on a bare wall, its bottom 0.20-0.25 m over the furniture
 * it hangs over and centred on it, a group's pieces a hand apart on one centre line. */
export const HANGING = { centre: 1.5, centreTol: 0.05, over: 0.22, overMin: 0.18, overMax: 0.3, apart: 0.08, offCentre: 0.03 };

/** Decor hung on a wall by the rules: one piece or a group side by side, a hand apart (`apart`), on one centre line.
 * Over furniture (`over`), centred on it with its bottom `HANGING.over` above it; else centred at `at` along the wall,
 * its centre `HANGING.centre` up. Each piece faces into the room by its own measured front (wallFacing). */
export function hang(room: Room, wall: WallName, items: { src: string; name?: string; front?: Facing }[], o: { at?: number; over?: Over; centre?: number; apart?: number; off?: number } = {}): Hung[] {
  const apart = o.apart ?? HANGING.apart, off = o.off ?? 0.003;
  const boxes = items.map((it) => {
    const rotation: Vec3 = [0, FACE_YAW[it.front ?? wallFacing(it.src)], 0];
    return { it, rotation, b: modelBounds(it.src, rotation) };
  });
  const widths = boxes.map(({ b }) => b.max[0] - b.min[0]);
  const tall = Math.max(...boxes.map(({ b }) => b.max[1] - b.min[1]));
  const total = widths.reduce((a, w) => a + w, 0) + apart * (items.length - 1);
  const mid = o.over ? (o.over.u0 + o.over.u1) / 2 : o.at;
  if (mid === undefined) throw new Error("hang: say where along the wall (at) or what it hangs over (over)");
  const yc = o.centre ?? (o.over ? o.over.top + HANGING.over + tall / 2 : HANGING.centre);
  let u = mid - total / 2;
  return boxes.map(({ it, rotation, b }, i) => {
    const w = widths[i], h = b.max[1] - b.min[1];
    const h0: Hung = {
      name: it.name ?? `${wall} decor ${i + 1}`, src: it.src, wall, u0: u, u1: u + w, y0: yc - h / 2, y1: yc + h / 2, off, over: o.over, given: o.centre !== undefined,
      position: [u - b.min[0], yc - (b.min[1] + b.max[1]) / 2, off - b.min[2]], rotation,
    };
    u += w + apart;
    return h0;
  });
}

/** Hung decor drawn, each piece in its wall's frame. */
export function Hang({ room, hung }: { room: Room; hung: Hung[] }) {
  return (
    <>
      {hung.map((h, i) => (
        <Group key={i} position={onWall(room.walls[h.wall], 0)} rotation={[0, room.walls[h.wall].yaw, 0]}>
          <Model name={h.name} src={h.src} position={h.position} rotation={h.rotation} />
        </Group>
      ))}
    </>
  );
}

/** Hung decor checked: on its wall (not standing off it: floating), clear of the wall's openings and its wall
 * cabinets and counters, centred on what it hangs over at the right height above it, or at eye height on a bare wall. */
export function checkDecor(room: Room, hung: Hung[], { openings = [], runs = [] }: { openings?: Opening[]; runs?: LaidRun[] } = {}): Finding[] {
  const out: Finding[] = [];
  const H = HANGING;
  const cross = (a0: number, a1: number, b0: number, b1: number) => Math.min(a1, b1) - Math.max(a0, b0) > 0.01;
  // a group shares one centre line and one anchor: judge it once
  const groups = new Map<string, Hung[]>();
  for (const h of hung) {
    const key = `${h.wall}|${h.over?.name ?? ""}|${((h.y0 + h.y1) / 2).toFixed(3)}`;
    groups.set(key, [...(groups.get(key) ?? []), h]);
  }
  for (const h of hung) {
    if (h.off > 0.02) out.push({ level: "error", rule: "decor", says: `${h.name} floats ${h.off.toFixed(2)} m off the ${h.wall} wall` });
    for (const o of openings.filter((o) => o.wall === h.wall)) {
      if (cross(h.u0, h.u1, o.at - o.width / 2 - 0.05, o.at + o.width / 2 + 0.05) && cross(h.y0, h.y1, o.bottom - 0.05, o.top + 0.05)) out.push({ level: "error", rule: "decor", says: `${h.name} hangs over the ${h.wall} wall's ${o.kind}` });
    }
    for (const r of runs.filter((r) => r.wall.name === h.wall)) {
      for (const l of r.units) {
        if (h.over && l.u0 >= h.over.u0 - 0.01 && l.u1 <= h.over.u1 + 0.01 && l.height <= h.over.top + 0.01) continue;
        if (cross(h.u0, h.u1, l.u0, l.u1) && cross(h.y0, h.y1, l.bottom, l.height)) out.push({ level: "error", rule: "decor", says: `${h.name} runs into ${l.name ?? "a unit"} on the ${h.wall} wall` });
      }
    }
  }
  for (const g of groups.values()) {
    const [u0, u1, y0, y1] = [Math.min(...g.map((h) => h.u0)), Math.max(...g.map((h) => h.u1)), Math.min(...g.map((h) => h.y0)), Math.max(...g.map((h) => h.y1))];
    const what = g.length > 1 ? `the group of ${g.length} (${g.map((h) => h.name).join(", ")})` : g[0].name;
    const over = g[0].over;
    if (over) {
      const gap = y0 - over.top, off = (u0 + u1) / 2 - (over.u0 + over.u1) / 2;
      if (!g[0].given) out.push({ level: gap < H.overMin || gap > H.overMax ? "warning" : "ok", rule: "decor", says: `${what}: ${gap.toFixed(2)} m over ${over.name} (${H.overMin}-${H.overMax})` });
      else out.push({ level: "ok", rule: "decor", says: `${what}: hung ${gap.toFixed(2)} m over ${over.name} (its height given)` });
      if (Math.abs(off) > H.offCentre) out.push({ level: "warning", rule: "decor", says: `${what}: ${off.toFixed(2)} m off ${over.name}'s centre` });
      if (u1 - u0 > over.u1 - over.u0) out.push({ level: "warning", rule: "decor", says: `${what} is wider than ${over.name}` });
    } else if (!g[0].given) {
      const c = (y0 + y1) / 2;
      out.push({ level: Math.abs(c - H.centre) > H.centreTol ? "warning" : "ok", rule: "decor", says: `${what}: centred ${c.toFixed(2)} m up (${H.centre - H.centreTol}-${H.centre + H.centreTol})` });
    }
  }
  return out;
}

// ---------------------------------------------------------------------------------------------------- outside

/** A kind of planting round the house: `count` of `src` between `band[0]` and `band[1]` metres out from the walls, no
 * two closer than `spacing`, each turned at random and scaled within `scale`; it sways `sway` radians in the wind. */
export interface Planting {
  src: string;
  count: number;
  band: [number, number];
  spacing?: number;
  scale?: [number, number];
  sway?: number;
}

/** A planted thing: where, its turn and scale, and its sway (radians, its phase and period in seconds). */
export interface Plant {
  src: string;
  at: Vec3;
  yaw: number;
  scale: number;
  sway: { amp: number; phase: number; period: number };
}

/** A seeded stream of numbers in [0, 1) (mulberry32): the same garden every frame, every render, every play. */
export function seeded(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The plantings laid round the room, outside its walls (and clear of `avoid`: a bay, a patio), each kind in its band:
 * a point is drawn in the room's box grown by the band's far edge and kept when it is past the near edge and `spacing`
 * from everything already planted. Deterministic for a seed. */
export function plant(room: Room, plantings: Planting[], seed = 1, avoid: Footprint[] = []): Plant[] {
  const rnd = seeded(seed);
  const out: Plant[] = [];
  const { width: w, depth: d } = room;
  const outside = (x: number, z: number) => Math.hypot(Math.max(0, -x, x - w), Math.max(0, -z, z - d));
  for (const p of plantings) {
    const [near, far] = p.band, sp = p.spacing ?? 1;
    let placed = 0;
    for (let tries = 0; placed < p.count && tries < p.count * 400; tries++) {
      const x = -far + rnd() * (w + 2 * far), z = -far + rnd() * (d + 2 * far);
      const o = outside(x, z);
      if (o < near || o > far) continue;
      if (avoid.some((f) => x > f.min[0] - sp && x < f.max[0] + sp && z > f.min[1] - sp && z < f.max[1] + sp)) continue;
      if (out.some((q) => Math.hypot(q.at[0] - x, q.at[2] - z) < Math.max(sp, q.src === p.src ? sp : sp / 2))) continue;
      const [s0, s1] = p.scale ?? [1, 1];
      out.push({ src: p.src, at: [x, 0, z], yaw: rnd() * Math.PI * 2, scale: s0 + rnd() * (s1 - s0), sway: { amp: p.sway ?? 0, phase: rnd() * Math.PI * 2, period: 3 + rnd() * 2 } });
      placed++;
    }
  }
  return out;
}

/** A plant's lean in the wind at `t` seconds: a slow swing about x and z, out of step with its neighbours. */
export function swayAt(p: Plant, t: number): Vec3 {
  const w = (2 * Math.PI * t) / p.sway.period + p.sway.phase;
  return [p.sway.amp * Math.sin(w), p.yaw, p.sway.amp * 0.6 * Math.sin(0.73 * w + 1.1)];
}

/** Outside the room: the ground to the horizon (`size` across, just under the room's floor) and what is planted on it,
 * swaying with the clock (a render's frame, a game's time). */
export function Outside({ plants, ground = "#6d7f45", size = 120, centre = [0, 0] }: { plants: Plant[]; ground?: string; size?: number; centre?: [number, number] }) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = frame / fps;
  return (
    <>
      <Mesh name="ground" position={[centre[0], -0.03, centre[1]]}><Box args={[size, 0.02, size]} /><MeshStandardMaterial color={ground} roughness={0.95} /></Mesh>
      {plants.map((p, i) => (
        // foliage meets foliage: a hedge's leaves in a tree's are planted so, not clipped
        <Model key={i} name={`plant-${i}`} src={p.src} position={p.at} rotation={p.sway.amp ? swayAt(p, t) : [0, p.yaw, 0]} scale={p.scale} allowOverlap />
      ))}
    </>
  );
}

// ---------------------------------------------------------------------------------------------------- the checks

/** A box on the floor plan: x and z, metres. */
export interface Footprint {
  name: string;
  min: [number, number];
  max: [number, number];
  /** a seat's: the way a diner faces, on the plan (x, z) */
  faces?: [number, number];
}

/** A laid run's footprint: from the wall to its deepest unit, along its length. */
export function runFootprint(run: LaidRun, name = `${run.wall.name} run`): Footprint {
  const depth = Math.max(...run.units.map((l) => l.depth));
  const a = onWall(run.wall, run.units[0]?.u0 ?? 0, 0), b = onWall(run.wall, run.end, depth);
  return { name, min: [Math.min(a[0], b[0]), Math.min(a[2], b[2])], max: [Math.max(a[0], b[0]), Math.max(a[2], b[2])] };
}

/** A model standing at `at` turned by `yaw`, as a footprint. A seat says which way a diner on it faces before it is
 * turned (`faces`, on the plan; a chair that faces +z, [0, 1]): turned by `yaw` with it. */
export function modelFootprint(name: string, src: string, at: Vec3, yaw = 0, faces?: [number, number]): Footprint {
  const b = modelBounds(src, [0, yaw, 0]);
  const f: [number, number] | undefined = faces && [faces[0] * Math.cos(yaw) + faces[1] * Math.sin(yaw), -faces[0] * Math.sin(yaw) + faces[1] * Math.cos(yaw)];
  return { name, min: [at[0] + b.min[0], at[2] + b.min[2]], max: [at[0] + b.max[0], at[2] + b.max[2]], faces: f };
}

const facings = new Map<string, [number, number] | null>();
/** Which way a seat faces as it comes, on the plan (x, z): read from its triangles by the engine, from its back's
 * centre across its seat's; null for a seat with nothing over it (a backless stool faces any way). */
export function seatFacing(src: string): [number, number] | null {
  let f = facings.get(src);
  if (f === undefined) {
    if (!hasNative()) throw new Error("seatFacing: reading a glTF needs the raea engine (raea-motion render)");
    f = (native("scene.facing", { src }) as { faces: [number, number] | null }).faces;
    facings.set(src, f);
  }
  return f;
}

/** The yaw that turns a seat standing at `at` to face `target` (a table's or a counter's middle): never typed by hand,
 * a model's own front is read from it (seatFacing); a seat that faces any way stays as it comes. */
export function faceToward(src: string, at: Vec3, target: Vec3): number {
  const f = seatFacing(src);
  if (!f) return 0;
  return Math.atan2(target[0] - at[0], target[2] - at[2]) - Math.atan2(f[0], f[1]);
}

/** A seat's footprint, its facing read from the model and turned with it. */
export function seatFootprint(name: string, src: string, at: Vec3, yaw = 0): Footprint {
  return modelFootprint(name, src, at, yaw, seatFacing(src) ?? undefined);
}

/** The clear distance from a seat's back to the first thing behind it, across the seat's width (a seat that faces
 * neither along x nor z is read along the nearer one; a thing the seat is tucked into, 0); undefined when nothing is
 * behind it. */
export function behind(seat: Footprint, others: Footprint[]): { o: Footprint; g: number } | undefined {
  const [fx, fz] = seat.faces!;
  const ax = Math.abs(fx) >= Math.abs(fz) ? 0 : 1, across = 1 - ax, back = -Math.sign(ax === 0 ? fx : fz);
  let best: { o: Footprint; g: number } | undefined;
  for (const o of others) {
    if (o === seat || o.max[across] <= seat.min[across] || o.min[across] >= seat.max[across]) continue;
    const g = back > 0 ? o.min[ax] - seat.max[ax] : seat.min[ax] - o.max[ax];
    // on that side of the seat: its middle past the seat's
    const side = back * ((o.min[ax] + o.max[ax]) / 2 - (seat.min[ax] + seat.max[ax]) / 2) > 0;
    if (side && (!best || g < best.g)) best = { o, g: Math.max(0, g) };
  }
  return best;
}

/** The clear distance between two footprints on the plan (0 when they overlap). */
export function gap(a: Footprint, b: Footprint): number {
  const dx = Math.max(0, a.min[0] - b.max[0], b.min[0] - a.max[0]);
  const dz = Math.max(0, a.min[1] - b.max[1], b.min[1] - a.max[1]);
  return Math.hypot(dx, dz);
}

/** A kitchen's clearances (the NKBA's guidelines, in metres): a work aisle in front of a work run, a walkway
 * elsewhere, room behind a seated diner to walk past, and the work triangle between sink, stove and fridge. */
export const CLEARANCE = { workAisle: 1.07, walkway: 0.91, behindSeat: 0.91, legMin: 1.22, legMax: 2.74, triangleMax: 7.92 };

export interface Finding {
  level: "error" | "warning" | "ok";
  rule: string;
  says: string;
}

export interface Plan {
  room: Room;
  runs: LaidRun[];
  /** free-standing work pieces, held to a work aisle from the runs and a walkway from the walls: an island */
  islands: Footprint[];
  /** furniture that is in the way but owes no aisle: a table, a bench */
  furniture?: Footprint[];
  /** seats (with `faces`, room is kept behind a diner; without, to the nearest wall or run) */
  seats?: Footprint[];
}

/** The plan checked: every aisle between an island and a run or a wall, the walkways, room behind the seats, and the
 * work triangle. */
export function checkLayout(plan: Plan): Finding[] {
  const out: Finding[] = [];
  const c = CLEARANCE;
  const { width: w, depth: d } = plan.room;
  const walls: Footprint[] = [
    { name: "back wall", min: [0, -1], max: [w, 0] }, { name: "front wall", min: [0, d], max: [w, d + 1] },
    { name: "left wall", min: [-1, 0], max: [0, d] }, { name: "right wall", min: [w, 0], max: [w + 1, d] },
  ];
  const runs = plan.runs.map((r) => runFootprint(r));
  for (const isl of plan.islands) {
    for (const r of runs) {
      const g = gap(isl, r);
      if (g < 3) out.push({ level: g < c.workAisle ? "error" : "ok", rule: "work aisle", says: `${isl.name} to the ${r.name}: ${g.toFixed(2)} m (a work aisle wants ${c.workAisle})` });
    }
    for (const wl of walls) {
      const g = gap(isl, wl);
      if (g < 2 && !runs.some((r) => gap(r, wl) < 0.01 && overlapsAlong(r, wl, isl))) {
        out.push({ level: g < c.walkway ? "error" : "ok", rule: "walkway", says: `${isl.name} to the ${wl.name}: ${g.toFixed(2)} m (a walkway wants ${c.walkway})` });
      }
    }
  }
  const things = [...walls, ...runs, ...plan.islands, ...(plan.furniture ?? []), ...(plan.seats ?? [])];
  // a seat faces what it is sat at: an island or a table within half a metre before it
  for (const s of plan.seats ?? []) {
    if (!s.faces) continue;
    const front = behind({ ...s, faces: [-s.faces[0], -s.faces[1]] }, [...plan.islands, ...(plan.furniture ?? [])]);
    const ok = front && front.g < 0.5;
    out.push({ level: ok ? "ok" : "error", rule: "facing", says: ok ? `${s.name} faces the ${front.o.name}` : `${s.name} faces nothing to sit at${front ? ` (the ${front.o.name} is ${front.g.toFixed(2)} m before it)` : ""}: turn it with faceToward` });
  }
  for (const s of plan.seats ?? []) {
    const near = s.faces ? behind(s, things) : [...walls, ...runs].map((o) => ({ o, g: gap(s, o) })).sort((a, b) => a.g - b.g)[0];
    // a bench or a chair with its back to a wall: nobody walks behind it
    if (near && s.faces && walls.includes(near.o) && near.g < 0.15) out.push({ level: "ok", rule: "behind a seat", says: `${s.name}: its back to the ${near.o.name}` });
    else if (near) out.push({ level: near.g < c.behindSeat ? "error" : "ok", rule: "behind a seat", says: `behind ${s.name}: ${near.g.toFixed(2)} m to the ${near.o.name} (to walk past a diner: ${c.behindSeat})` });
  }
  const at = (role: string) => plan.runs.flatMap((r) => r.units.filter((u) => u.role === role).map((u) => unitCentre(r, u)))[0];
  const [sink, stove, fridge] = [at("sink"), at("stove"), at("fridge")];
  if (sink && stove && fridge) {
    const leg = (a: Vec3, b: Vec3) => Math.hypot(a[0] - b[0], a[2] - b[2]);
    const legs: [string, number][] = [["sink to stove", leg(sink, stove)], ["stove to fridge", leg(stove, fridge)], ["fridge to sink", leg(fridge, sink)]];
    for (const [n, l] of legs) {
      out.push({ level: l < c.legMin || l > c.legMax ? "warning" : "ok", rule: "work triangle", says: `${n}: ${l.toFixed(2)} m (${c.legMin}-${c.legMax})` });
    }
    const total = legs.reduce((a, [, l]) => a + l, 0);
    out.push({ level: total > c.triangleMax ? "error" : "ok", rule: "work triangle", says: `the triangle: ${total.toFixed(2)} m (at most ${c.triangleMax})` });
  }
  return out;
}

/** whether a run along a wall covers the stretch of it beside `isl` (so the island faces the run, not bare wall) */
function overlapsAlong(r: Footprint, wl: Footprint, isl: Footprint): boolean {
  const horizontal = wl.max[0] - wl.min[0] > wl.max[1] - wl.min[1];
  const k = horizontal ? 0 : 1;
  return r.min[k] < isl.max[k] && r.max[k] > isl.min[k];
}
