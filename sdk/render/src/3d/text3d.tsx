// Text3D: solid 3D letters as a Mesh's geometry (Manifold's text3d), split from shapes.tsx, which re-exports it.
import { node3d } from "./all";

export interface Text3DProps {
  /** the words, extruded */
  children?: string;
  /** the words, when they aren't its child (no quotes or backslashes) */
  text?: string;
  /** letter height in scene units (default 0.5) */
  size?: number;
  /** extrusion depth (default 0.2) */
  depth?: number;
  /** a family (Inter by default, or another of raea's bundled faces) */
  font?: string;
  /** 100..900 (800 by default) */
  weight?: number;
  /** false: scenery (a sign seen in passing), not held to the type checks raea render runs on 3D type: its size on screen,
   * its angle to the camera, the title-safe area, its reading time and parts in front of it (docs/stage-guards.feature
   * SG10). Words meant to be read are better as page type: a Label */
  read?: boolean;
}

/** Solid 3D letters (Manifold's text3d), as a Mesh's geometry: `<Mesh><Text3D size={0.6}>SALE</Text3D>...</Mesh>`. */
export function Text3D(props: Text3DProps) {
  return node3d("Text3D", { ...props, shape: text3dShape(props) });
}

export function text3dShape(p: Text3DProps): string {
  const text = p.text ?? (typeof p.children === "string" ? p.children : undefined);
  if (typeof text !== "string" || text === "") throw new TypeError("Text3D: give the words as its child or `text`");
  if (/["\\]/.test(text)) throw new TypeError(`Text3D: the words can't hold a quote or backslash, got ${JSON.stringify(text)}`);
  const size = p.size ?? 0.5, depth = p.depth ?? 0.2, weight = p.weight ?? 800, font = p.font ?? "Inter";
  for (const [k, v] of [["size", size], ["depth", depth]] as const) if (!(v > 0) || !Number.isFinite(v)) throw new RangeError(`Text3D: ${k} must be a positive number, got ${v}`);
  return `text3d("${text}", ${size}, ${depth}, "${font}", ${weight})`;
}
