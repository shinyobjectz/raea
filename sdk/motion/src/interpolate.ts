// interpolate: map a number through keyframes (spec: "interpolate maps a number through keyframes").
import type { EasingFn } from "./easing";

export type ExtrapolateMode = "extend" | "clamp" | "identity" | "wrap";

export interface InterpolateOptions {
  easing?: EasingFn | EasingFn[];
  extrapolateLeft?: ExtrapolateMode;
  extrapolateRight?: ExtrapolateMode;
}

const identityEase: EasingFn = (p) => p;

function checkRanges(input: readonly number[], output: readonly number[]): void {
  if (!Array.isArray(input) || !Array.isArray(output)) {
    throw new TypeError("interpolate: the input and output keyframes must be arrays");
  }
  if (input.length !== output.length) {
    throw new RangeError(
      `interpolate: input has ${input.length} keyframes but output has ${output.length}; they must match`,
    );
  }
  if (input.length < 1) throw new RangeError("interpolate: at least one keyframe is needed");
  for (let i = 0; i < input.length; i++) {
    if (typeof input[i] !== "number" || Number.isNaN(input[i])) {
      throw new TypeError(`interpolate: input[${i}] is not a number (${input[i]})`);
    }
    if (typeof output[i] !== "number" || Number.isNaN(output[i])) {
      throw new TypeError(`interpolate: output[${i}] is not a number (${output[i]})`);
    }
  }
  for (let i = 0; i + 1 < input.length; i++) {
    if (!(input[i + 1] > input[i])) {
      throw new RangeError(
        `interpolate: input must strictly increase, but input[${i}] = ${input[i]} is followed by input[${i + 1}] = ${input[i + 1]}`,
      );
    }
  }
}

function easeFor(easing: InterpolateOptions["easing"], segment: number, segments: number): EasingFn {
  if (easing === undefined) return identityEase;
  if (typeof easing === "function") return easing;
  if (Array.isArray(easing)) {
    if (easing.length !== segments) {
      throw new RangeError(
        `interpolate: an easing array needs one function per segment (${segments}), got ${easing.length}`,
      );
    }
    return easing[segment];
  }
  throw new TypeError("interpolate: easing must be a function or an array of functions");
}

function checkMode(mode: string, side: string): ExtrapolateMode {
  if (mode === "extend" || mode === "clamp" || mode === "identity" || mode === "wrap") return mode;
  throw new RangeError(`interpolate: ${side} must be extend, clamp, identity or wrap, got "${mode}"`);
}

export function interpolate(
  x: number,
  input: readonly number[],
  output: readonly number[],
  options: InterpolateOptions = {},
): number {
  if (typeof x !== "number" || !Number.isFinite(x)) {
    throw new RangeError(`interpolate: x must be a finite number, got ${x}`);
  }
  checkRanges(input, output);
  const segments = input.length - 1;
  // validate the easing option up front so a wrong-length array fails whatever x is
  if (Array.isArray(options.easing)) easeFor(options.easing, 0, segments);
  if (segments === 0) return output[0];

  const left = checkMode(options.extrapolateLeft ?? "extend", "extrapolateLeft");
  const right = checkMode(options.extrapolateRight ?? "extend", "extrapolateRight");
  const first = input[0];
  const last = input[segments];

  let v = x;
  if (v < first || v > last) {
    const mode = v < first ? left : right;
    if (mode === "clamp") return v < first ? output[0] : output[segments];
    if (mode === "identity") return v;
    if (mode === "wrap") {
      const width = last - first;
      v = first + ((((v - first) % width) + width) % width);
    }
    // extend: fall through with v outside the range; the end segment's line is continued below
  }

  // pick the segment: the end segments also cover the extension beyond them
  let i = 0;
  while (i < segments - 1 && v > input[i + 1]) i++;
  const span = input[i + 1] - input[i];
  const p = (v - input[i]) / span;
  const eased = easeFor(options.easing, i, segments)(p);
  return output[i] + eased * (output[i + 1] - output[i]);
}
