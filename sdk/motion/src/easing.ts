// Easing curves: functions on [0, 1] with e(0) = 0 and e(1) = 1 (spec: "Easing curves are functions on [0, 1]").
// Every plain curve eases in; the modifiers `out` and `inOut` derive the other shapes from it.

export type EasingFn = (p: number) => number;

const HALF_PI = Math.PI / 2;

const power = (n: number): EasingFn => (p) => Math.pow(p, n);

// Bounce: four falling arcs with gain n and breakpoints at multiples of 1 / d.
const ARC_GAIN = 7.5625;
const ARC_DIV = 2.75;
function bounceCurve(p: number): number {
  if (p < 1 / ARC_DIV) return ARC_GAIN * p * p;
  if (p < 2 / ARC_DIV) {
    const q = p - 1.5 / ARC_DIV;
    return ARC_GAIN * q * q + 0.75;
  }
  if (p < 2.5 / ARC_DIV) {
    const q = p - 2.25 / ARC_DIV;
    return ARC_GAIN * q * q + 0.9375;
  }
  const q = p - 2.625 / ARC_DIV;
  return ARC_GAIN * q * q + 0.984375;
}

// A cubic Bezier through (0,0), (x1,y1), (x2,y2), (1,1). Each coordinate is
// B(t) = 3(1-t)^2 t c1 + 3(1-t) t^2 c2 + t^3, written in polynomial form a t^3 + b t^2 + c t.
function cubicBezier(x1: number, y1: number, x2: number, y2: number): EasingFn {
  for (const [label, v] of [["x1", x1], ["x2", x2]] as const) {
    if (!(v >= 0 && v <= 1)) throw new RangeError(`Easing.bezier: ${label} must lie in [0, 1], got ${v}`);
  }
  const coeffs = (c1: number, c2: number) => {
    const c = 3 * c1;
    const b = 3 * (c2 - c1) - c;
    const a = 1 - c - b;
    return { a, b, c };
  };
  const X = coeffs(x1, x2);
  const Y = coeffs(y1, y2);
  const xAt = (t: number) => ((X.a * t + X.b) * t + X.c) * t;
  const yAt = (t: number) => ((Y.a * t + Y.b) * t + Y.c) * t;
  const dxAt = (t: number) => (3 * X.a * t + 2 * X.b) * t + X.c;

  // Find t with xAt(t) = p: Newton steps from t = p, falling back to bisection when the slope is flat
  // or Newton fails to converge. x(t) is monotonic on [0, 1] because x1 and x2 lie in [0, 1].
  const solveT = (p: number): number => {
    let t = p;
    for (let i = 0; i < 8; i++) {
      const err = xAt(t) - p;
      if (Math.abs(err) < 1e-9) return t;
      const slope = dxAt(t);
      if (Math.abs(slope) < 1e-7) break;
      t -= err / slope;
    }
    let lo = 0;
    let hi = 1;
    t = p;
    for (let i = 0; i < 60; i++) {
      const xv = xAt(t);
      if (Math.abs(xv - p) < 1e-10) break;
      if (xv < p) lo = t;
      else hi = t;
      t = (lo + hi) / 2;
    }
    return t;
  };

  if (x1 === y1 && x2 === y2) return (p) => p; // the straight line
  return (p) => {
    if (p <= 0) return p === 0 ? 0 : yAt(0) + p * (y1 / (x1 || 1)); // linear continuation below 0
    if (p >= 1) return p === 1 ? 1 : 1 + (p - 1) * ((1 - y2) / ((1 - x2) || 1)); // and above 1
    return yAt(solveT(p));
  };
}

// out() and inOut() of the same curve hand back the same function (a component calls them in its render: each call
// made a new one, every frame; lightspeed Z3)
const OUT = new WeakMap<EasingFn, EasingFn>();
const IN_OUT = new WeakMap<EasingFn, EasingFn>();

export const Easing = {
  linear: ((p) => p) as EasingFn,
  quad: power(2),
  cubic: power(3),
  poly: (n: number): EasingFn => power(n),
  sin: ((p) => 1 - Math.cos(p * HALF_PI)) as EasingFn,
  circle: ((p) => 1 - Math.sqrt(1 - p * p)) as EasingFn,
  exp: ((p) => (p > 0 ? Math.pow(2, 10 * (p - 1)) : 0)) as EasingFn,
  back:
    (s = 1.70158): EasingFn =>
    (p) =>
      p * p * ((s + 1) * p - s),
  elastic:
    (bounciness = 1): EasingFn =>
    (p) =>
      1 - Math.pow(Math.cos(p * HALF_PI), 3) * Math.cos(p * bounciness * Math.PI),
  bounce: bounceCurve as EasingFn,
  bezier: cubicBezier,
  ease: cubicBezier(0.42, 0, 1, 1),
  in: (e: EasingFn): EasingFn => e,
  out: (e: EasingFn): EasingFn => {
    let f = OUT.get(e);
    if (f === undefined) OUT.set(e, (f = (p) => 1 - e(1 - p)));
    return f;
  },
  inOut: (e: EasingFn): EasingFn => {
    let f = IN_OUT.get(e);
    if (f === undefined) IN_OUT.set(e, (f = (p) => (p < 0.5 ? e(2 * p) / 2 : 1 - e(2 - 2 * p) / 2)));
    return f;
  },
};
