// What every 3D part shares: vectors and their checks.
import { type ReactNode } from "react";

export type Vec3 = [number, number, number];

/** The props worth sending to the renderer: everything but children and functions. */
export function serialisable(props: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(props)) {
    if (k === "children" || typeof v === "function" || v === undefined) continue;
    out[k] = v;
  }
  return out;
}

const same = (a: unknown, b: unknown, depth: number): boolean => {
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null || depth === 0) return false;
  // a React element is its identity: its props can hold anything
  if ("$$typeof" in a || Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
  const [ka, kb] = [Object.keys(a), Object.keys(b)];
  return ka.length === kb.length && ka.every((k) => Object.hasOwn(b, k) && same((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k], depth - 1));
};

/** Whether two elements' props say the same thing: `memo`'s compare for a part whose props are values. An array or a
 * colour written inline (`half_size={[1, 0.5, 1]}`, `Srgba.hex("#f80")`) is made again every frame, which the default
 * compare by identity reads as a change, so every such part was rebuilt every frame (#160). Functions, elements and
 * children still compare by identity. */
export const sameProps = (a: object, b: object): boolean => same(a, b, 4);

export function node3d(kind: string, props: Record<string, unknown>, children?: ReactNode, extra?: Record<string, string>) {
  return (
    <div data-3d={kind} data-props={JSON.stringify(serialisable(props))} {...extra}>
      {children}
    </div>
  );
}

export function checkVec(owner: string, name: string, v: unknown): void {
  if (!Array.isArray(v) || v.length !== 3 || !v.every((n) => typeof n === "number" && Number.isFinite(n))) {
    throw new TypeError(`${owner}: ${name} must be three finite numbers [x, y, z], got ${JSON.stringify(v)}`);
  }
}

/** A geometry's sizes. three.js puts segment counts after them (`[r, h, 32]`); those are accepted and dropped, since
 * the engine tessellates on its own. */
export function checkArgs(owner: string, args: unknown, count: number): number[] {
  if (!Array.isArray(args) || args.length < count || !args.every((n) => typeof n === "number" && Number.isFinite(n))) {
    throw new TypeError(`${owner}: args must be ${count} finite number${count === 1 ? "" : "s"}, got ${JSON.stringify(args)}`);
  }
  return args.slice(0, count);
}

export interface Canvas3DProps {
  /** the 3D takes its place in the page's tree order instead of lying under the whole page: page elements before the
   * Canvas3D (and the backgrounds of the elements holding it) are under the 3D, showing wherever it has nothing, and
   * hidden by its parts; positioned elements after it are over it. The 3D's background is transparent. Put the
   * Canvas3D straight in a full-frame AbsoluteFill. */
  layered?: boolean;
  /** only with `layered`: the 3D clears to this CSS colour instead of transparent, so its translucent parts blend over
   * it in linear light, as Bevy's do over its ClearColor (a Bevy 2D scene's rgb(43, 44, 47)), not over the page in sRGB.
   * The 3D then lies under the whole page, as Bevy's camera output lies under all its UI (a GlobalZIndex(-1) too). */
  clearColor?: string;
  /** the 3D scene: cameras, lights and parts */
  children?: ReactNode;
  [prop: string]: unknown;
}
