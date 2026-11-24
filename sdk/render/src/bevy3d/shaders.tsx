// Constants in the engine's own shaders, set from TypeScript when the scene loads (docs/nobuild.feature NB1a).
import { BevyComponent, Entity } from "../3d";

/** A shader constant's value: a number (a float or a whole number, as the const is), a boolean, or for an array or a
 * vector its values in order (as many as it holds) */
export type ShaderConstantValue = number | boolean | readonly number[];

export interface ShaderConstantsProps {
  /** the shader as the engine embeds it, `<crate>/<path under its src>`: "bevy_solari/realtime/restir.wesl",
   * "raea_render_raytrace/emit_taa.wgsl". To read its constants, put an empty file of that name in the project's
   * engine-shaders/ folder and render once: raea fills it with the shader */
  shader: string;
  /** each `const NAME` to set, and its value; the const keeps its type (an array's or a vector's values as a list) */
  values: Record<string, ShaderConstantValue>;
}

/** The engine's component for a list of [shader, const, value] (those whose value is given), or undefined when it sets
 * nothing: the typed props built on <ShaderConstants> (<SolariLighting constants>) put it inside their part. */
export function shaderConstants(list: ReadonlyArray<readonly [string, string, ShaderConstantValue | undefined]>) {
  const values = list
    .filter(([, , v]) => v !== undefined)
    .map(([shader, name, v]) => {
      if (Array.isArray(v)) {
        if (!v.length || !v.every(Number.isFinite)) throw new RangeError(`ShaderConstants: ${name} in ${shader} is a list of numbers, got ${v}`);
        return { shader, name, value: 0, list: [...v] };
      }
      const value = typeof v === "boolean" ? (v ? 1 : 0) : (v as number);
      if (!Number.isFinite(value)) throw new RangeError(`ShaderConstants: ${name} in ${shader} is a number or a boolean, got ${v}`);
      return { shader, name, value, list: [] };
    });
  return values.length ? <BevyComponent type="raea_sys_engine_shaders::consts::ShaderConstants" value={{ values }} /> : undefined;
}

/** Sets constants in one of the engine's own shaders (raea's, Bevy's, Solari's) for this scene. Each `const NAME = ...;`
 * named is rewritten when the scene loads, before anything compiles, so a value is tried in a composition instead of a
 * build; the shader's own value is the default. Compiled in once: a value that changes later in the composition isn't
 * drawn, and raea says so. A name the shader doesn't declare is reported and left. One per shader. The common ones have
 * typed props (<SolariLighting constants>); this reaches the rest. */
export function ShaderConstants({ shader, values }: ShaderConstantsProps) {
  if (!/^[a-z0-9_]+\/\S+\.(wgsl|wesl)$/.test(shader)) throw new RangeError(`ShaderConstants: shader is <crate>/<path under its src>, a .wgsl or .wesl file, got ${shader}`);
  const c = shaderConstants(Object.entries(values).map(([n, v]) => [shader, n, v] as const));
  return c ? <Entity name={`shader-constants:${shader}`}>{c}</Entity> : null;
}
