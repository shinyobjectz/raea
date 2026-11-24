// Labels that read (docs/stage-guards.feature SG8). Words that name or point at a 3D part were written into the world as
// RichText, where they turned with the floor and shrank with distance: the essay's floor labels read at 60 to 75
// degrees, its code at 3 to 7 px. A Label is page type instead: drawn flat over the frame at the point its part
// projects to through the shot's own camera (the props its Camera3d is given), sized in pixels of the frame, kept
// inside the title-safe area, with a line to the point. It never turns, never shrinks and stands clear of the part.
import type { CSSProperties, ReactNode } from "react";
import { useVideoConfig } from "@raea/app/clock";
import { Quat, type Projection, type TransformValue } from "../bevy3d/core";
import type { Vec3 } from "./core";

/** The least capital height a Label takes, in px at 1080 lines (SG10's floor for words that read). */
export const LABEL_MIN = 22;
/** The title-safe area's inset from each edge, a share of the frame (as raea-sys-stage's type-safe has it). */
const SAFE = 0.05;
/** A capital's height as a share of the font's size. */
const CAP = 0.7;

/** The camera a Label projects through: the props its Camera3d is given. */
export interface LabelCamera {
  /** where the camera stands and faces, as its Camera3d has it */
  transform?: TransformValue;
  /** its perspective (fov in radians, Bevy's default π/4) or orthographic view, as its Camera3d has it */
  projection?: Projection;
}

/** Label's props: page type that points at a 3D part. */
export interface LabelProps {
  /** the point it names, in metres in the world: give the part's top, so the words stand clear above it */
  at: Vec3;
  /** the shot's camera: the props its Camera3d is given (`path.camera(frame)`), so the label follows it exactly */
  camera: LabelCamera;
  /** its words */
  children: ReactNode;
  /** capital height in px at 1080 lines, scaled to the frame (32; at least 22, so it reads) */
  size?: number;
  /** which way the words stand from the point ("above") */
  side?: "above" | "below";
  /** the words' distance from the point, in px at 1080 lines (40) */
  gap?: number;
  /** a line from the words to a dot on the point (true) */
  leader?: boolean;
  /** the words' and the line's colour (white) */
  color?: string;
  /** more style for the words: font, weight, a background, padding */
  style?: CSSProperties;
}

/** Where a world point lands in a frame of width x height through a camera: [x, y] in px from the top left and its
 *  distance in front of the camera, or nothing when it's behind the camera or outside the frame. */
export function project(at: Vec3, camera: LabelCamera, width: number, height: number): [number, number, number] | undefined {
  const eye = camera.transform?.translation ?? [0, 0, 0];
  const q = camera.transform?.rotation ?? Quat.IDENTITY;
  const proj = camera.projection ?? { Perspective: {} };
  // into the camera's own space: it looks along its -Z, +Y up
  const c = Quat.mulVec3([-q[0], -q[1], -q[2], q[3]], [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]]);
  const z = -c[2];
  let s: number;
  if ("Orthographic" in proj) {
    s = height / (proj.Orthographic.scaling_mode?.FixedVertical.viewport_height ?? 10);
  } else {
    if (z <= (proj.Perspective.near ?? 0.1)) return undefined;
    s = height / 2 / Math.tan((proj.Perspective.fov ?? Math.PI / 4) / 2) / z;
  }
  const x = width / 2 + c[0] * s;
  const y = height / 2 - c[1] * s;
  return x < 0 || x > width || y < 0 || y > height ? undefined : [x, y, z];
}

/** Where a Label's words go: the point, the words' anchor (on their near edge, inside title-safe) and how far along
 *  their width the anchor sits (0 at their left, 1 at their right), so they grow away from the nearer side. */
export function labelPlace(point: [number, number], width: number, height: number, side: "above" | "below", gap: number, font: number) {
  const [lo, hi] = [SAFE * width, (1 - SAFE) * width];
  const ax = Math.min(hi, Math.max(lo, point[0]));
  const want = point[1] + (side === "above" ? -gap : gap);
  // a line of words fits between the anchor and the safe area's edge
  const ay = side === "above" ? Math.max(SAFE * height + font * 1.2, Math.min((1 - SAFE) * height, want)) : Math.min((1 - SAFE) * height - font * 1.2, Math.max(SAFE * height, want));
  return { ax, ay, along: (ax - lo) / (hi - lo), lo, hi };
}

/** Page type that names a 3D part: drawn over the frame where the part's point lands through the shot's camera, at a
 *  size in pixels of the frame, inside title-safe, with a line to the point. Hidden while the point is behind the camera
 *  or out of frame. Put it beside the 3D, in the page, and give it the props the shot's Camera3d gets:
 *
 *  `<Label at={[0, 1.2, 0]} camera={path.camera(frame)}>Reviews</Label>` */
export function Label({ at, camera, children, size = 32, side = "above", gap = 40, leader = true, color = "#ffffff", style }: LabelProps) {
  if (!(size >= LABEL_MIN)) throw new RangeError(`Label: size is at least ${LABEL_MIN} px of capital height at 1080 lines, so it reads (SG10); got ${size}`);
  if (side !== "above" && side !== "below") throw new TypeError(`Label: side is "above" or "below", got ${JSON.stringify(side)}`);
  const { width, height } = useVideoConfig();
  const p = project(at, camera, width, height);
  if (!p) return null;
  const k = height / 1080;
  const font = (size * k) / CAP;
  const { ax, ay, along, lo, hi } = labelPlace([p[0], p[1]], width, height, side, gap * k, font);
  // the words' box spans the safe width; spacers grown in the ratio along : 1 - along put the point `along` its width
  // at ax (no transform: the page lays it out as it is)
  const row: CSSProperties = { position: "absolute", left: lo, width: hi - lo, display: "flex", flexDirection: "row", ...(side === "above" ? { bottom: height - ay } : { top: ay }) };
  return (
    <>
      {leader ? (
        <svg data-label-leader="" width={width} height={height} style={{ position: "absolute", left: 0, top: 0 }}>
          <line x1={p[0]} y1={p[1]} x2={ax} y2={ay} stroke={color} strokeWidth={Math.max(1, 2 * k)} />
          <circle cx={p[0]} cy={p[1]} r={Math.max(2, 4 * k)} fill={color} />
        </svg>
      ) : null}
      <div style={row}>
        <div style={{ flexGrow: along, flexShrink: 1, flexBasis: 0 }} />
        {/* data-points: the world point it names, carried to the stage checks, which make the part there the shot's subject (SG14) */}
        <div data-label="" data-points={at.join(" ")} style={{ flexGrow: 0, flexShrink: 0, fontSize: font, lineHeight: 1.15, color, whiteSpace: "nowrap", ...style }}>{children}</div>
        <div style={{ flexGrow: 1 - along, flexShrink: 1, flexBasis: 0 }} />
      </div>
    </>
  );
}
