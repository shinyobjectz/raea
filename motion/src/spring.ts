// spring: a damped harmonic oscillator sampled at frames (spec: "spring is a damped harmonic oscillator").
// m x'' + c x' + k (x - to) = 0, x(0) = from, x'(0) = 0, solved in closed form for each damping regime.

export interface SpringConfig {
  mass?: number;
  damping?: number;
  stiffness?: number;
  overshootClamping?: boolean;
}

export interface SpringOptions {
  frame: number;
  fps: number;
  from?: number;
  to?: number;
  config?: SpringConfig;
  durationInFrames?: number;
  durationRestThreshold?: number;
  delay?: number;
  reverse?: boolean;
}

const DEFAULT_REST = 0.005;
const CRITICAL_TOLERANCE = 1e-9;
const SETTLE_SEARCH_LIMIT = 1_000_000;

/** The oscillator's unit response: u(t) = (x(t) - to) / (from - to), so u(0) = 1 and u -> 0. */
interface Oscillator {
  unit(t: number): number;
  /** an upper bound on |u| from t on (the decay envelope), used to know when it has settled for good */
  envelope(t: number): number;
  /** the first time u reaches 0 (x crosses `to`), or Infinity if it never does */
  firstCrossing: number;
}

function oscillator(mass: number, damping: number, stiffness: number): Oscillator {
  const w0 = Math.sqrt(stiffness / mass);
  const zeta = damping / (2 * Math.sqrt(stiffness * mass));
  if (Math.abs(zeta - 1) < CRITICAL_TOLERANCE) {
    const unit = (t: number) => Math.exp(-w0 * t) * (1 + w0 * t);
    return { unit, envelope: unit, firstCrossing: Infinity };
  }
  if (zeta < 1) {
    const decay = zeta * w0;
    const wd = w0 * Math.sqrt(1 - zeta * zeta);
    const unit = (t: number) => Math.exp(-decay * t) * (Math.cos(wd * t) + (decay / wd) * Math.sin(wd * t));
    // cos + r sin has amplitude sqrt(1 + r^2) = 1 / sqrt(1 - zeta^2)
    const amp = 1 / Math.sqrt(1 - zeta * zeta);
    const envelope = (t: number) => amp * Math.exp(-decay * t);
    // u = 0 where tan(wd t) = -wd / decay: the first positive root is (pi - atan(wd / decay)) / wd
    const firstCrossing = (Math.PI - Math.atan2(wd, decay)) / wd;
    return { unit, envelope, firstCrossing };
  }
  const decay = zeta * w0;
  const wh = w0 * Math.sqrt(zeta * zeta - 1);
  const unit = (t: number) => Math.exp(-decay * t) * (Math.cosh(wh * t) + (decay / wh) * Math.sinh(wh * t));
  // overdamped: u falls monotonically to 0 without crossing, so it bounds itself
  return { unit, envelope: unit, firstCrossing: Infinity };
}

/** The settle time in frames: the first frame after which |u| stays below the threshold. */
function naturalFrames(osc: Oscillator, fps: number, threshold: number): number {
  let lastAbove = -1;
  for (let f = 0; f < SETTLE_SEARCH_LIMIT; f++) {
    const t = f / fps;
    if (osc.envelope(t) < threshold) break; // below the envelope bound it can never come back
    if (Math.abs(osc.unit(t)) >= threshold) lastAbove = f;
  }
  return lastAbove + 1;
}

function positive(name: string, v: number): number {
  if (typeof v !== "number" || !(v > 0) || !Number.isFinite(v)) {
    throw new RangeError(`spring: ${name} must be a positive number, got ${v}`);
  }
  return v;
}

function finite(name: string, v: number): number {
  if (typeof v !== "number" || !Number.isFinite(v)) {
    throw new RangeError(`spring: ${name} must be a finite number, got ${v}`);
  }
  return v;
}

/** How many frames a spring with these settings takes to settle (before any stretching). */
export function measureSpring(opts: {
  fps: number;
  config?: SpringConfig;
  durationRestThreshold?: number;
}): number {
  const { mass = 1, damping = 10, stiffness = 100 } = opts.config ?? {};
  const osc = oscillator(positive("mass", mass), finite("damping", damping), positive("stiffness", stiffness));
  return naturalFrames(osc, positive("fps", opts.fps), opts.durationRestThreshold ?? DEFAULT_REST);
}

export function spring(opts: SpringOptions): number {
  const fps = positive("fps", opts.fps);
  const frame = finite("frame", opts.frame);
  const from = finite("from", opts.from ?? 0);
  const to = finite("to", opts.to ?? 1);
  const delay = finite("delay", opts.delay ?? 0);
  const { mass = 1, damping = 10, stiffness = 100, overshootClamping = false } = opts.config ?? {};
  if (damping < 0) throw new RangeError(`spring: damping must not be negative, got ${damping}`);
  const threshold = opts.durationRestThreshold ?? DEFAULT_REST;
  if (!(threshold > 0)) throw new RangeError(`spring: durationRestThreshold must be positive, got ${threshold}`);
  if (opts.durationInFrames !== undefined) positive("durationInFrames", opts.durationInFrames);

  const osc = oscillator(positive("mass", mass), damping, positive("stiffness", stiffness));
  const stretched = opts.durationInFrames !== undefined;
  // natural length is only needed when stretching or reversing
  const natural = stretched || opts.reverse ? naturalFrames(osc, fps, threshold) : 0;
  const length = stretched ? (opts.durationInFrames as number) : natural;

  // applied outermost first: delay, then reverse, then stretch (so the effects compose stretch -> reverse -> delay)
  const local = frame - delay;
  if (local < 0) return from;
  const forwardFrame = opts.reverse ? length - local : local;
  if (forwardFrame < 0) return from;
  const scale = stretched ? (length === 0 ? 0 : natural / length) : 1;
  const t = (forwardFrame * scale) / fps;

  if (from === to) return to;
  if (overshootClamping && t >= osc.firstCrossing) return to;
  return to + (from - to) * osc.unit(t);
}
