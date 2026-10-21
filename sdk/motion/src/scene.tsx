// @raea/motion/scene: parts placed by where they meet a surface, not by typed heights. A model's reach below its
// origin is measured from its own triangles under its turn and scale (the engine's scene.bounds, every node of the
// glTF), so a bowl knocked onto its rim, a mug tipped onto its side or a crumb at any angle sits on its surface exactly,
// at every frame of a pose. `raea-motion scene lint` checks the result (docs/scene.feature).
//
//   <Place src={staticFile("mug.glb")} on={island.top} at={[1.36, 1.8]} rotation={[tip, Math.PI / 2, 0]} />
//   const y = standOn(staticFile("mug.glb"), island.top, [tip, Math.PI / 2, 0]);   // the same height, for a pose track
//   <Mesh position={[x, top + boxReach(size, rot), z]} rotation={rot}><Box args={size} /></Mesh>
import { hasNative, native } from "./native";
import { Model, type ModelProps, type Vec3 } from "./3d";

export interface Bounds {
  min: Vec3;
  max: Vec3;
}

const cache = new Map<string, Bounds>();

/** A glTF model's box turned (Euler XYZ, radians, as Model takes it) and scaled about its origin, from all its
 * nodes' triangles. */
export function modelBounds(src: string, rotation: Vec3 = [0, 0, 0], scale: number | Vec3 = 1): Bounds {
  const key = JSON.stringify([src, rotation, scale]);
  let b = cache.get(key);
  if (!b) {
    if (!hasNative()) throw new Error("modelBounds: measuring a glTF needs the raea engine (raea-motion render)");
    b = native("scene.bounds", { src, rotation, scale }) as Bounds;
    // a pose track asks at every frame's angle: keep the cache from growing without end
    if (cache.size > 4096) cache.clear();
    cache.set(key, b);
  }
  return b;
}

/** The y for a model's origin that sets its lowest point on a surface at height `surface`. */
export function standOn(src: string, surface: number, rotation: Vec3 = [0, 0, 0], scale: number | Vec3 = 1): number {
  return surface - modelBounds(src, rotation, scale).min[1];
}

/** How far a box of `size`, turned by `rotation` (Euler XYZ, radians, as Mesh takes it), reaches below its centre: on a
 * surface at h its centre goes at h + boxReach(size, rotation). */
export function boxReach(size: Vec3, [x, y, z]: Vec3 = [0, 0, 0]): number {
  const [cx, sx, cy, sy, cz, sz] = [Math.cos(x), Math.sin(x), Math.cos(y), Math.sin(y), Math.cos(z), Math.sin(z)];
  // the world-y row of Rx·Ry·Rz
  const row = [sx * sy * cz + cx * sz, -sx * sy * sz + cx * cz, -sx * cy];
  return row.reduce((a, r, k) => a + (Math.abs(r) * size[k]) / 2, 0);
}

export interface PlaceProps extends Omit<ModelProps, "position"> {
  /** the surface's height it stands on (a table's top: 0.78) */
  on: number;
  /** where on it, x and z */
  at: [number, number];
}

/** A glTF model standing on a surface: its lowest point, at its turn and scale, on `on`, at `at`. */
export function Place({ on, at, ...props }: PlaceProps) {
  const y = standOn(props.src, on, props.rotation, props.scale);
  return <Model {...props} position={[at[0], y, at[1]]} />;
}
