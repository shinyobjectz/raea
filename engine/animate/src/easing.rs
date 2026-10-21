//! Easing curves (forked from animato-core 1.7.1's `easing`, in f64). Every curve maps progress in [0, 1] to eased
//! progress, with e(0) = 0 and e(1) = 1; Back, Elastic and springs overshoot in between. A curve can be named in
//! text (`Easing::parse`), which is how @raea/motion's declarations name them: "out-cubic", "bezier(0.05,0.7,0.1,1)",
//! "steps(4)", "spring(1,15,120)", "reverse(out-cubic)".

use std::f64::consts::PI;

use crate::spring::Oscillator;

/// An easing curve.
#[derive(Clone, Debug, PartialEq)]
pub enum Easing {
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    EaseInQuint,
    EaseOutQuint,
    EaseInOutQuint,
    EaseInSine,
    EaseOutSine,
    EaseInOutSine,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseInCirc,
    EaseOutCirc,
    EaseInOutCirc,
    EaseInBack,
    EaseOutBack,
    EaseInOutBack,
    EaseInElastic,
    EaseOutElastic,
    EaseInOutElastic,
    EaseInBounce,
    EaseOutBounce,
    EaseInOutBounce,
    /// CSS `cubic-bezier(x1, y1, x2, y2)`
    CubicBezier(f64, f64, f64, f64),
    /// CSS `steps(n, end)`
    Steps(u32),
    RoughEase { strength: f64, points: u32 },
    SlowMo { linear_ratio: f64, power: f64 },
    Wiggle { wiggles: u32 },
    CustomBounce { strength: f64 },
    ExpoScale { start: f64, end: f64 },
    /// A damped spring stretched over the segment: its settle time (`natural`, seconds) maps to progress 1, as
    /// @raea/motion's spring() with durationInFrames stretches it. Not clamped: it overshoots.
    Spring { osc: Oscillator, natural: f64 },
    /// The curve played backwards: e'(p) = 1 - e(1 - p) (an out-curve becomes the in-curve that leaves the same way).
    Reverse(Box<Easing>),
}

impl Easing {
    /// Eased progress at `t` (clamped to [0, 1]).
    pub fn apply(&self, t: f64) -> f64 {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        match self {
            Easing::Linear => t,
            Easing::EaseInQuad => t * t,
            Easing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u / 2.0
                }
            }
            Easing::EaseInCubic => t * t * t,
            Easing::EaseOutCubic => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Easing::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u / 2.0
                }
            }
            Easing::EaseInQuart => t * t * t * t,
            Easing::EaseOutQuart => {
                let u = 1.0 - t;
                1.0 - u * u * u * u
            }
            Easing::EaseInOutQuart => {
                if t < 0.5 {
                    8.0 * t * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u * u / 2.0
                }
            }
            Easing::EaseInQuint => t * t * t * t * t,
            Easing::EaseOutQuint => {
                let u = 1.0 - t;
                1.0 - u * u * u * u * u
            }
            Easing::EaseInOutQuint => {
                if t < 0.5 {
                    16.0 * t * t * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u * u * u / 2.0
                }
            }
            Easing::EaseInSine => 1.0 - (t * PI / 2.0).cos(),
            Easing::EaseOutSine => (t * PI / 2.0).sin(),
            Easing::EaseInOutSine => -((t * PI).cos() - 1.0) / 2.0,
            Easing::EaseInExpo => {
                if t == 0.0 {
                    0.0
                } else {
                    2f64.powf(10.0 * t - 10.0)
                }
            }
            Easing::EaseOutExpo => {
                if t == 1.0 {
                    1.0
                } else {
                    1.0 - 2f64.powf(-10.0 * t)
                }
            }
            Easing::EaseInOutExpo => {
                if t == 0.0 || t == 1.0 {
                    t
                } else if t < 0.5 {
                    2f64.powf(20.0 * t - 10.0) / 2.0
                } else {
                    (2.0 - 2f64.powf(-20.0 * t + 10.0)) / 2.0
                }
            }
            Easing::EaseInCirc => 1.0 - (1.0 - t * t).sqrt(),
            Easing::EaseOutCirc => (1.0 - (t - 1.0) * (t - 1.0)).sqrt(),
            Easing::EaseInOutCirc => {
                if t < 0.5 {
                    (1.0 - (1.0 - (2.0 * t) * (2.0 * t)).sqrt()) / 2.0
                } else {
                    let u = -2.0 * t + 2.0;
                    ((1.0 - u * u).sqrt() + 1.0) / 2.0
                }
            }
            Easing::EaseInBack => BACK_C3 * t * t * t - BACK_C1 * t * t,
            Easing::EaseOutBack => {
                let u = t - 1.0;
                1.0 + BACK_C3 * u * u * u + BACK_C1 * u * u
            }
            Easing::EaseInOutBack => {
                if t < 0.5 {
                    let u = 2.0 * t;
                    (u * u * ((BACK_C2 + 1.0) * u - BACK_C2)) / 2.0
                } else {
                    let u = 2.0 * t - 2.0;
                    (u * u * ((BACK_C2 + 1.0) * u + BACK_C2) + 2.0) / 2.0
                }
            }
            Easing::EaseInElastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    -2f64.powf(10.0 * t - 10.0) * ((10.0 * t - 10.75) * ELASTIC_C4).sin()
                }
            }
            Easing::EaseOutElastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    2f64.powf(-10.0 * t) * ((10.0 * t - 0.75) * ELASTIC_C4).sin() + 1.0
                }
            }
            Easing::EaseInOutElastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else if t < 0.5 {
                    -(2f64.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * ELASTIC_C5).sin()) / 2.0
                } else {
                    (2f64.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * ELASTIC_C5).sin()) / 2.0 + 1.0
                }
            }
            Easing::EaseInBounce => 1.0 - ease_out_bounce(1.0 - t),
            Easing::EaseOutBounce => ease_out_bounce(t),
            Easing::EaseInOutBounce => {
                if t < 0.5 {
                    (1.0 - ease_out_bounce(1.0 - 2.0 * t)) / 2.0
                } else {
                    (1.0 + ease_out_bounce(2.0 * t - 1.0)) / 2.0
                }
            }
            Easing::CubicBezier(x1, y1, x2, y2) => cubic_bezier(t, *x1, *y1, *x2, *y2),
            Easing::Steps(n) => {
                if t == 0.0 {
                    0.0
                } else {
                    let n = (*n).max(1) as f64;
                    ((t * n).ceil() / n).clamp(0.0, 1.0)
                }
            }
            Easing::RoughEase { strength, points } => rough_ease(t, *strength, *points),
            Easing::SlowMo { linear_ratio, power } => slow_mo(t, *linear_ratio, *power),
            Easing::Wiggle { wiggles } => wiggle(t, *wiggles),
            Easing::CustomBounce { strength } => {
                if t <= 0.0 || t >= 1.0 {
                    t
                } else {
                    let s = strength.clamp(0.0, 1.0);
                    t * (1.0 - s) + ease_out_bounce(t) * s
                }
            }
            Easing::ExpoScale { start, end } => expo_scale(t, *start, *end),
            Easing::Spring { osc, natural } => 1.0 - osc.unit(t * natural),
            Easing::Reverse(e) => 1.0 - e.apply(1.0 - t),
        }
    }

    /// A curve named in text. Names: `linear`, `{in,out,in-out}-{quad,cubic,quart,quint,sine,expo,circ,back,elastic,
    /// bounce}`, `ease` (bezier(0.42, 0, 1, 1), as @raea/motion's Easing.ease), `bezier(x1,y1,x2,y2)` (also
    /// `cubic-bezier(...)`), `steps(n)`, `spring(mass,damping,stiffness)` (stretched over its segment; its settle
    /// time is measured at `fps`), `reverse(<curve>)`, `rough(strength,points)`, `slowmo(ratio,power)`,
    /// `wiggle(n)`, `custom-bounce(strength)`, `expo-scale(start,end)`. Empty text is linear.
    pub fn parse(s: &str, fps: f64) -> Result<Easing, String> {
        let s = s.trim();
        let (name, args) = match s.find('(') {
            Some(i) if s.ends_with(')') => (s[..i].trim(), Some(&s[i + 1..s.len() - 1])),
            _ => (s, None),
        };
        let nums = |n: usize| -> Result<Vec<f64>, String> {
            let a = args.ok_or_else(|| format!("easing {s:?} needs {n} numbers in brackets"))?;
            let v: Vec<f64> = a.split(',').map(|x| x.trim().parse::<f64>().map_err(|_| format!("easing {s:?}: {x:?} isn't a number"))).collect::<Result<_, _>>()?;
            if v.len() != n {
                return Err(format!("easing {s:?} takes {n} numbers, got {}", v.len()));
            }
            Ok(v)
        };
        Ok(match name {
            "" | "linear" => Easing::Linear,
            "in-quad" => Easing::EaseInQuad,
            "out-quad" => Easing::EaseOutQuad,
            "in-out-quad" => Easing::EaseInOutQuad,
            "in-cubic" => Easing::EaseInCubic,
            "out-cubic" => Easing::EaseOutCubic,
            "in-out-cubic" => Easing::EaseInOutCubic,
            "in-quart" => Easing::EaseInQuart,
            "out-quart" => Easing::EaseOutQuart,
            "in-out-quart" => Easing::EaseInOutQuart,
            "in-quint" => Easing::EaseInQuint,
            "out-quint" => Easing::EaseOutQuint,
            "in-out-quint" => Easing::EaseInOutQuint,
            "in-sine" => Easing::EaseInSine,
            "out-sine" => Easing::EaseOutSine,
            "in-out-sine" => Easing::EaseInOutSine,
            "in-expo" => Easing::EaseInExpo,
            "out-expo" => Easing::EaseOutExpo,
            "in-out-expo" => Easing::EaseInOutExpo,
            "in-circ" => Easing::EaseInCirc,
            "out-circ" => Easing::EaseOutCirc,
            "in-out-circ" => Easing::EaseInOutCirc,
            "in-back" => Easing::EaseInBack,
            "out-back" => Easing::EaseOutBack,
            "in-out-back" => Easing::EaseInOutBack,
            "in-elastic" => Easing::EaseInElastic,
            "out-elastic" => Easing::EaseOutElastic,
            "in-out-elastic" => Easing::EaseInOutElastic,
            "in-bounce" => Easing::EaseInBounce,
            "out-bounce" => Easing::EaseOutBounce,
            "in-out-bounce" => Easing::EaseInOutBounce,
            "ease" => Easing::CubicBezier(0.42, 0.0, 1.0, 1.0),
            "bezier" | "cubic-bezier" => {
                let v = nums(4)?;
                if !(0.0..=1.0).contains(&v[0]) || !(0.0..=1.0).contains(&v[2]) {
                    return Err(format!("easing {s:?}: x1 and x2 must lie in [0, 1]"));
                }
                Easing::CubicBezier(v[0], v[1], v[2], v[3])
            }
            "steps" => Easing::Steps(nums(1)?[0].max(1.0) as u32),
            "spring" => {
                let v = nums(3)?;
                let osc = Oscillator::new(v[0], v[1], v[2]).map_err(|e| format!("easing {s:?}: {e}"))?;
                let fps = if fps > 0.0 { fps } else { 30.0 };
                let natural = osc.natural_frames(fps, crate::spring::DEFAULT_REST) as f64 / fps;
                Easing::Spring { osc, natural }
            }
            "reverse" => Easing::Reverse(Box::new(Easing::parse(args.unwrap_or(""), fps)?)),
            "rough" => {
                let v = nums(2)?;
                Easing::RoughEase { strength: v[0], points: v[1].max(2.0) as u32 }
            }
            "slowmo" => {
                let v = nums(2)?;
                Easing::SlowMo { linear_ratio: v[0], power: v[1] }
            }
            "wiggle" => Easing::Wiggle { wiggles: nums(1)?[0].max(1.0) as u32 },
            "custom-bounce" => Easing::CustomBounce { strength: nums(1)?[0] },
            "expo-scale" => {
                let v = nums(2)?;
                Easing::ExpoScale { start: v[0], end: v[1] }
            }
            other => return Err(format!("no easing named {other:?}")),
        })
    }
}

const BACK_C1: f64 = 1.701_58;
const BACK_C2: f64 = BACK_C1 * 1.525;
const BACK_C3: f64 = BACK_C1 + 1.0;
const ELASTIC_C4: f64 = (2.0 * PI) / 3.0;
const ELASTIC_C5: f64 = (2.0 * PI) / 4.5;

fn ease_out_bounce(t: f64) -> f64 {
    const N1: f64 = 7.5625;
    const D1: f64 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / D1;
        N1 * t * t + 0.984_375
    }
}

/// A cubic Bezier through (0,0), (x1,y1), (x2,y2), (1,1) at x = `p`: Newton steps from t = p, then bisection, as
/// @raea/motion's Easing.bezier solves it (so the same curve gives the same numbers in both).
pub fn cubic_bezier(p: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    if x1 == y1 && x2 == y2 {
        return p;
    }
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return 1.0;
    }
    let co = |c1: f64, c2: f64| {
        let c = 3.0 * c1;
        let b = 3.0 * (c2 - c1) - c;
        (1.0 - c - b, b, c)
    };
    let (xa, xb, xc) = co(x1, x2);
    let (ya, yb, yc) = co(y1, y2);
    let x_at = |t: f64| ((xa * t + xb) * t + xc) * t;
    let y_at = |t: f64| ((ya * t + yb) * t + yc) * t;
    let dx_at = |t: f64| (3.0 * xa * t + 2.0 * xb) * t + xc;
    let mut t = p;
    for _ in 0..8 {
        let err = x_at(t) - p;
        if err.abs() < 1e-9 {
            return y_at(t);
        }
        let slope = dx_at(t);
        if slope.abs() < 1e-7 {
            break;
        }
        t -= err / slope;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    t = p;
    for _ in 0..60 {
        let xv = x_at(t);
        if (xv - p).abs() < 1e-10 {
            break;
        }
        if xv < p {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) / 2.0;
    }
    y_at(t)
}

fn rough_ease(t: f64, strength: f64, points: u32) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    let n = points.clamp(2, 20);
    let boundary = 4.0 * t * (1.0 - t);
    let mut noise = 0.0;
    for i in 1..=n {
        noise += (i as f64 * PI * t).sin() / i as f64;
    }
    noise /= (n as f64).ln().max(1.0);
    t + boundary * strength.clamp(0.0, 2.0) * noise
}

fn slow_mo(t: f64, linear_ratio: f64, power: f64) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    let lr = linear_ratio.clamp(0.0, 1.0);
    let p = power.max(0.0);
    if lr >= 1.0 {
        return t;
    }
    let mid_start = (1.0 - lr) * 0.5;
    let mid_end = mid_start + lr;
    let s_mid = 1.0 / (1.0 + p * (1.0 - lr));
    let s_edge = if (1.0 - lr) > f64::EPSILON { (1.0 - s_mid * lr) / (1.0 - lr) } else { 1.0 };
    if t < mid_start {
        t * s_edge
    } else if t > mid_end {
        mid_start * s_edge + lr * s_mid + (t - mid_end) * s_edge
    } else {
        mid_start * s_edge + (t - mid_start) * s_mid
    }
}

fn wiggle(t: f64, wiggles: u32) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    let n = wiggles.max(1) as f64;
    t + (t * n * PI * 2.0).sin() * (t * PI).sin() * 0.25
}

fn expo_scale(t: f64, start: f64, end: f64) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    let (s, e) = (start.max(0.001), end.max(0.001));
    let k = e / s;
    if (s - e).abs() < 0.001 || (k - 1.0).abs() < 0.001 {
        return t;
    }
    (k.powf(t) - 1.0) / (k - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_hold_for_every_named_curve() {
        for n in ["linear", "in-quad", "out-cubic", "in-out-cubic", "out-quint", "in-out-sine", "out-expo", "in-out-circ", "out-back", "out-elastic", "out-bounce", "ease", "bezier(0.05,0.7,0.1,1)", "steps(4)", "reverse(out-cubic)"] {
            let e = Easing::parse(n, 30.0).unwrap();
            assert!(e.apply(0.0).abs() < 1e-9, "{n} at 0");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-9, "{n} at 1");
        }
    }

    #[test]
    fn out_cubic_is_motions() {
        // @raea/motion: Easing.out(Easing.cubic)(p) = 1 - (1 - p)^3
        let e = Easing::parse("out-cubic", 30.0).unwrap();
        for i in 0..=10 {
            let p = i as f64 / 10.0;
            assert!((e.apply(p) - (1.0 - (1.0 - p).powi(3))).abs() < 1e-12);
        }
    }

    #[test]
    fn reverse_turns_out_into_in() {
        let r = Easing::parse("reverse(out-cubic)", 30.0).unwrap();
        let i = Easing::parse("in-cubic", 30.0).unwrap();
        for k in 0..=20 {
            let p = k as f64 / 20.0;
            assert!((r.apply(p) - i.apply(p)).abs() < 1e-12);
        }
    }

    #[test]
    fn bezier_matches_known_values() {
        // css ease-out (0, 0, 0.58, 1) is monotonic and above the diagonal
        let e = Easing::parse("bezier(0,0,0.58,1)", 30.0).unwrap();
        let mut last = 0.0;
        for k in 1..100 {
            let v = e.apply(k as f64 / 100.0);
            assert!(v > last && v >= k as f64 / 100.0);
            last = v;
        }
        assert!(Easing::parse("bezier(1.2,0,0,1)", 30.0).is_err());
    }

    #[test]
    fn bad_names_say_so() {
        assert!(Easing::parse("outCubic", 30.0).unwrap_err().contains("no easing named"));
        assert!(Easing::parse("bezier(1,2)", 30.0).is_err());
    }
}
