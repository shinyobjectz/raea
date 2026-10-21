//! GSAP ease strings ("power3.out", "back.out(1.7)", "elastic.inOut(1, 0.4)", "steps(6)",
//! "cubic-bezier(.2,.8,.2,1)") mapped onto animato's easing curves.

use animato::Easing;
use std::f32::consts::PI;

#[derive(Clone, Debug)]
pub enum Ease {
    /// A curve animato ships as-is (GSAP and animato share the Penner set and GSAP's defaults).
    Animato(Easing),
    /// back with a non-default overshoot
    Back { dir: Dir, overshoot: f32 },
    /// elastic with non-default amplitude / period
    Elastic { dir: Dir, amplitude: f32, period: f32 },
    /// A function ease sampled in JS (65 points over [0, 1])
    Sampled(Vec<f32>),
    /// GSAP steps(n): ((n + 1) * p | 0) / n with p kept below 1, so n + 1 levels reaching 1
    /// (animato's Steps is CSS jump-end)
    Steps(u32),
    /// GSAP's Expo, which blends to exact 0/1 ends: easeIn(p) = 2^(10(p-1)) p + p^6 (1-p)
    /// (not Penner's 2^(10(p-1)); expo.out at 0.5 is 0.9766, not 0.9688)
    Expo(Dir),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dir {
    In,
    Out,
    InOut,
}

impl Ease {
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Animato(e) => e.apply(t),
            Ease::Back { dir, overshoot } => {
                let s = *overshoot;
                let ease_in = |p: f32| p * p * ((s + 1.0) * p - s);
                with_dir(*dir, t, ease_in)
            }
            Ease::Elastic { dir, amplitude, period } => {
                let a = amplitude.max(1.0);
                let p = *period;
                let s = p / (2.0 * PI) * (1.0 / a).asin();
                let ease_out = |q: f32| {
                    if q <= 0.0 || q >= 1.0 {
                        return q.clamp(0.0, 1.0);
                    }
                    a * 2f32.powf(-10.0 * q) * ((q - s) * (2.0 * PI) / p).sin() + 1.0
                };
                match dir {
                    Dir::Out => ease_out(t),
                    Dir::In => 1.0 - ease_out(1.0 - t),
                    Dir::InOut => {
                        if t < 0.5 {
                            (1.0 - ease_out(1.0 - t * 2.0)) / 2.0
                        } else {
                            ease_out(t * 2.0 - 1.0) / 2.0 + 0.5
                        }
                    }
                }
            }
            Ease::Steps(n) => {
                let n = (*n).max(1) as f32;
                ((n + 1.0) * t.min(1.0 - 1e-7)).floor() / n
            }
            Ease::Expo(dir) => with_dir(*dir, t, |p| 2f32.powf(10.0 * (p - 1.0)) * p + p.powi(6) * (1.0 - p)),
            Ease::Sampled(samples) => {
                if samples.len() < 2 {
                    return t;
                }
                let x = t * (samples.len() - 1) as f32;
                let i = (x.floor() as usize).min(samples.len() - 2);
                let f = x - i as f32;
                samples[i] + (samples[i + 1] - samples[i]) * f
            }
        }
    }

    pub const fn linear() -> Ease {
        Ease::Animato(Easing::Linear)
    }
}

fn with_dir(dir: Dir, t: f32, ease_in: impl Fn(f32) -> f32) -> f32 {
    match dir {
        Dir::In => ease_in(t),
        Dir::Out => 1.0 - ease_in(1.0 - t),
        Dir::InOut => {
            if t < 0.5 {
                ease_in(t * 2.0) / 2.0
            } else {
                1.0 - ease_in((1.0 - t) * 2.0) / 2.0
            }
        }
    }
}

/// GSAP's default ease when neither the tween nor any default sets one.
pub fn default_ease() -> Ease {
    Ease::Animato(Easing::EaseOutQuad)
}

pub fn parse(name: &str) -> Ease {
    let s = name.trim();
    if s.is_empty() {
        return default_ease();
    }
    let lower = s.to_ascii_lowercase();
    if let Some(args) = lower.strip_prefix("cubic-bezier(").and_then(|r| r.strip_suffix(')')) {
        let v = numbers(args);
        if v.len() == 4 {
            return Ease::Animato(Easing::CubicBezier(v[0], v[1], v[2], v[3]));
        }
    }
    let (head, args) = match s.find('(') {
        Some(i) => (&s[..i], numbers(s[i + 1..].trim_end_matches(')'))),
        None => (s, Vec::new()),
    };
    let (base, dir) = match head.split_once('.') {
        Some((b, d)) => (b, d),
        None => (head, "out"),
    };
    // Legacy GSAP 2 names: Power2.easeOut, Expo.easeInOut, Linear.easeNone
    let dir = match dir.to_ascii_lowercase().as_str() {
        "in" | "easein" => Dir::In,
        "inout" | "easeinout" => Dir::InOut,
        _ => Dir::Out,
    };
    let base = base.to_ascii_lowercase();
    use Easing::*;
    let pick = |i: Easing, o: Easing, io: Easing| {
        Ease::Animato(match dir {
            Dir::In => i,
            Dir::Out => o,
            Dir::InOut => io,
        })
    };
    match base.as_str() {
        "none" | "linear" | "power0" => Ease::linear(),
        "power1" | "quad" => pick(EaseInQuad, EaseOutQuad, EaseInOutQuad),
        "power2" | "cubic" => pick(EaseInCubic, EaseOutCubic, EaseInOutCubic),
        "power3" | "quart" => pick(EaseInQuart, EaseOutQuart, EaseInOutQuart),
        "power4" | "quint" | "strong" => pick(EaseInQuint, EaseOutQuint, EaseInOutQuint),
        "sine" => pick(EaseInSine, EaseOutSine, EaseInOutSine),
        "expo" => Ease::Expo(dir),
        "circ" => pick(EaseInCirc, EaseOutCirc, EaseInOutCirc),
        "bounce" => pick(EaseInBounce, EaseOutBounce, EaseInOutBounce),
        // GSAP builds back.inOut symmetrically from the same overshoot (no Penner 1.525 factor),
        // so only in/out with the default overshoot can use animato's curves.
        "back" => match args.first() {
            Some(&o) if (o - 1.70158).abs() > 1e-4 => Ease::Back { dir, overshoot: o },
            _ if dir == Dir::InOut => Ease::Back { dir, overshoot: 1.70158 },
            _ => pick(EaseInBack, EaseOutBack, EaseInOutBack),
        },
        "elastic" => {
            let amplitude = args.first().copied().unwrap_or(1.0);
            let period = args.get(1).copied().unwrap_or(if dir == Dir::InOut { 0.45 } else { 0.3 });
            if (amplitude - 1.0).abs() < 1e-4 && (period - 0.3).abs() < 1e-4 && dir != Dir::InOut {
                pick(EaseInElastic, EaseOutElastic, EaseInOutElastic)
            } else {
                Ease::Elastic { dir, amplitude, period }
            }
        }
        "steps" => Ease::Steps(args.first().map(|n| *n as u32).unwrap_or(1).max(1)),
        "rough" => Ease::Animato(RoughEase { strength: args.first().copied().unwrap_or(1.0).min(1.0), points: 20 }),
        "slow" | "slowmo" => Ease::Animato(SlowMo {
            linear_ratio: args.first().copied().unwrap_or(0.7),
            power: args.get(1).copied().unwrap_or(0.7),
        }),
        "exposcale" => Ease::Animato(ExpoScale {
            start: args.first().copied().unwrap_or(1.0).max(1e-3),
            end: args.get(1).copied().unwrap_or(2.0).max(1e-3),
        }),
        _ => default_ease(),
    }
}

fn numbers(s: &str) -> Vec<f32> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|p| p.trim().parse::<f32>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every named ease against values sampled from GSAP 3.14.2 itself (tests/gsap-eases.json, generated
    /// in the test environment with `gsap.parseEase(name)(p)`; no GSAP code ships).
    #[test]
    fn eases_match_gsap_samples() {
        let data: serde_json::Value = serde_json::from_str(include_str!("../tests/gsap-eases.json")).unwrap();
        let ps: Vec<f64> = data["ps"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let mut bad = Vec::new();
        for (name, vals) in data["eases"].as_object().unwrap() {
            let e = parse(name);
            for (p, want) in ps.iter().zip(vals.as_array().unwrap()) {
                let want = want.as_f64().unwrap();
                let got = e.apply(*p as f32) as f64;
                if (got - want).abs() > 1e-4 {
                    bad.push(format!("{name} @{p}: ours {got:.6} gsap {want:.6}"));
                }
            }
        }
        assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
    }

    #[test]
    fn named_eases_match_gsap() {
        // power3.out = 1 - (1-t)^4
        let e = parse("power3.out");
        assert!((e.apply(0.5) - (1.0 - 0.5f32.powi(4))).abs() < 1e-5);
        // power2.inOut at 0.25 = (0.5^3)/2
        assert!((parse("power2.inOut").apply(0.25) - 0.0625).abs() < 1e-5);
        assert!((parse("none").apply(0.3) - 0.3).abs() < 1e-6);
        // back.out(1.7) overshoots
        assert!(parse("back.out(1.7)").apply(0.6) > 1.0);
        // elastic.out(1, 0.3) is animato's default elastic
        let a = parse("elastic.out(1, 0.3)").apply(0.3);
        let b = Easing::EaseOutElastic.apply(0.3);
        assert!((a - b).abs() < 1e-5);
        // custom period goes through our implementation and ends at 1
        assert!((parse("elastic.out(1, 0.6)").apply(1.0) - 1.0).abs() < 1e-6);
        assert!((parse("steps(4)").apply(0.3) - 0.25).abs() < 1e-6);
    }
}
