//! Springs (forked from animato-spring 1.7.1). Animato steps a spring with semi-implicit Euler each update(dt), so its
//! value depends on the frame rate and on how it was played. Here a spring is the damped harmonic oscillator's closed
//! form: m x'' + c x' + k (x - to) = 0 with x(0) = from, x'(0) = 0, solved for each damping regime and sampled at any
//! time, so a seek, a play-through and a different fps all agree. The semantics match @raea/motion's spring()
//! (sdk/motion/src/spring.ts): the same settle measure, delay, reverse, stretch to a duration and overshoot clamping.

use crate::interpolate::Interpolate;

/// |u| below this counts as settled (@raea/motion's durationRestThreshold default).
pub const DEFAULT_REST: f64 = 0.005;
const CRITICAL_TOLERANCE: f64 = 1e-9;
const SETTLE_SEARCH_LIMIT: u64 = 1_000_000;

/// A spring's physical settings (Animato's SpringConfig; `epsilon` became the rest threshold).
#[derive(Clone, Debug, PartialEq)]
pub struct SpringConfig {
    pub mass: f64,
    pub damping: f64,
    pub stiffness: f64,
    /// past its first crossing of the target it holds there (no overshoot)
    pub overshoot_clamping: bool,
}

impl Default for SpringConfig {
    /// mass 1, damping 10, stiffness 100: @raea/motion's default, which overshoots noticeably.
    fn default() -> Self {
        SpringConfig { mass: 1.0, damping: 10.0, stiffness: 100.0, overshoot_clamping: false }
    }
}

impl SpringConfig {
    pub fn new(mass: f64, damping: f64, stiffness: f64) -> Self {
        SpringConfig { mass, damping, stiffness, overshoot_clamping: false }
    }
    /// Damping at exactly critical for this stiffness (mass 1): the fastest settle with no overshoot.
    pub fn critically_damped(stiffness: f64) -> Self {
        let k = stiffness.max(0.0);
        SpringConfig::new(1.0, 2.0 * k.sqrt(), k)
    }
    pub fn overdamped(stiffness: f64, ratio: f64) -> Self {
        let mut c = Self::critically_damped(stiffness);
        c.damping *= ratio.max(1.0001);
        c
    }
    pub fn underdamped(stiffness: f64, ratio: f64) -> Self {
        let mut c = Self::critically_damped(stiffness);
        c.damping *= ratio.clamp(0.0, 1.0);
        c
    }
    /// Animato's presets.
    pub fn gentle() -> Self {
        SpringConfig::new(1.0, 14.0, 60.0)
    }
    pub fn wobbly() -> Self {
        SpringConfig::new(1.0, 12.0, 180.0)
    }
    pub fn stiff() -> Self {
        SpringConfig::new(1.0, 20.0, 210.0)
    }
    pub fn slow() -> Self {
        SpringConfig::new(1.0, 14.0, 37.0)
    }
    pub fn oscillator(&self) -> Result<Oscillator, String> {
        Oscillator::new(self.mass, self.damping, self.stiffness)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Regime {
    Critical { w0: f64 },
    Under { decay: f64, wd: f64, amp: f64 },
    Over { decay: f64, wh: f64 },
}

/// The oscillator's unit response u(t) = (x(t) - to) / (from - to): u(0) = 1, u -> 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oscillator {
    regime: Regime,
    /// the first time u reaches 0 (x crosses `to`), infinite when it never does
    pub first_crossing: f64,
}

impl Oscillator {
    pub fn new(mass: f64, damping: f64, stiffness: f64) -> Result<Oscillator, String> {
        if !(mass > 0.0 && mass.is_finite()) {
            return Err(format!("mass must be a positive number, got {mass}"));
        }
        if !(stiffness > 0.0 && stiffness.is_finite()) {
            return Err(format!("stiffness must be a positive number, got {stiffness}"));
        }
        if !(damping >= 0.0 && damping.is_finite()) {
            return Err(format!("damping must not be negative, got {damping}"));
        }
        let w0 = (stiffness / mass).sqrt();
        let zeta = damping / (2.0 * (stiffness * mass).sqrt());
        if (zeta - 1.0).abs() < CRITICAL_TOLERANCE {
            return Ok(Oscillator { regime: Regime::Critical { w0 }, first_crossing: f64::INFINITY });
        }
        if zeta < 1.0 {
            let decay = zeta * w0;
            let wd = w0 * (1.0 - zeta * zeta).sqrt();
            let amp = 1.0 / (1.0 - zeta * zeta).sqrt();
            let first_crossing = (std::f64::consts::PI - wd.atan2(decay)) / wd;
            return Ok(Oscillator { regime: Regime::Under { decay, wd, amp }, first_crossing });
        }
        let decay = zeta * w0;
        let wh = w0 * (zeta * zeta - 1.0).sqrt();
        Ok(Oscillator { regime: Regime::Over { decay, wh }, first_crossing: f64::INFINITY })
    }

    /// u(t), t in seconds.
    pub fn unit(&self, t: f64) -> f64 {
        match self.regime {
            Regime::Critical { w0 } => (-w0 * t).exp() * (1.0 + w0 * t),
            Regime::Under { decay, wd, .. } => (-decay * t).exp() * ((wd * t).cos() + (decay / wd) * (wd * t).sin()),
            Regime::Over { decay, wh } => (-decay * t).exp() * ((wh * t).cosh() + (decay / wh) * (wh * t).sinh()),
        }
    }

    /// An upper bound on |u| from t on (the decay envelope).
    pub fn envelope(&self, t: f64) -> f64 {
        match self.regime {
            Regime::Under { decay, amp, .. } => amp * (-decay * t).exp(),
            _ => self.unit(t),
        }
    }

    /// The settle time in frames at `fps`: the first frame after which |u| stays below `threshold`.
    pub fn natural_frames(&self, fps: f64, threshold: f64) -> u64 {
        let mut last_above: i64 = -1;
        for f in 0..SETTLE_SEARCH_LIMIT {
            let t = f as f64 / fps;
            if self.envelope(t) < threshold {
                break;
            }
            if self.unit(t).abs() >= threshold {
                last_above = f as i64;
            }
        }
        (last_above + 1) as u64
    }
}

/// How many frames a spring takes to settle (@raea/motion's measureSpring).
pub fn measure(config: &SpringConfig, fps: f64) -> Result<u64, String> {
    Ok(config.oscillator()?.natural_frames(fps, DEFAULT_REST))
}

/// One spring from `from` to `to`, sampled at any frame (@raea/motion's spring(), the same options).
#[derive(Clone, Debug, PartialEq)]
pub struct Spring<T: Interpolate + Clone = f64> {
    pub from: T,
    pub to: T,
    pub config: SpringConfig,
    /// frames it holds at `from` first
    pub delay: f64,
    /// stretched so its settle lands on this many frames
    pub duration: Option<f64>,
    /// played backwards (from its end to its start)
    pub reverse: bool,
    pub rest_threshold: f64,
}

impl<T: Interpolate + Clone> Spring<T> {
    pub fn new(from: T, to: T, config: SpringConfig) -> Self {
        Spring { from, to, config, delay: 0.0, duration: None, reverse: false, rest_threshold: DEFAULT_REST }
    }

    /// The spring's progress at `frame` (0 at `from`, 1 at `to`, past 1 when it overshoots), at `fps`.
    pub fn progress_at(&self, frame: f64, fps: f64) -> f64 {
        let Ok(osc) = self.config.oscillator() else { return 1.0 };
        let natural = if self.duration.is_some() || self.reverse { osc.natural_frames(fps, self.rest_threshold) as f64 } else { 0.0 };
        let length = self.duration.unwrap_or(natural);
        let local = frame - self.delay;
        if local < 0.0 {
            return 0.0;
        }
        let forward = if self.reverse { length - local } else { local };
        if forward < 0.0 {
            return 0.0;
        }
        let scale = match self.duration {
            Some(_) if length == 0.0 => 0.0,
            Some(_) => natural / length,
            None => 1.0,
        };
        let t = forward * scale / fps;
        if self.config.overshoot_clamping && t >= osc.first_crossing {
            return 1.0;
        }
        1.0 - osc.unit(t)
    }

    /// The value at `frame`.
    pub fn value_at(&self, frame: f64, fps: f64) -> T {
        self.from.lerp(&self.to, self.progress_at(frame, fps))
    }
}

/// @raea/motion's spring() for numbers: `to + (from - to) * u(t)`, written as that file writes it.
pub fn spring_value(frame: f64, fps: f64, from: f64, to: f64, config: &SpringConfig, duration: Option<f64>, delay: f64, reverse: bool) -> f64 {
    if from == to {
        return to;
    }
    let mut s = Spring::new(from, to, config.clone());
    s.duration = duration;
    s.delay = delay;
    s.reverse = reverse;
    let p = s.progress_at(frame, fps);
    if p == 0.0 {
        return from;
    }
    if p == 1.0 {
        return to;
    }
    to + (from - to) * (1.0 - p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spring_settles_and_overshoots_like_motions() {
        let c = SpringConfig::default();
        // mass 1, damping 10, stiffness 100: zeta 0.5, so it overshoots
        let peak = (0..60).map(|f| spring_value(f as f64, 30.0, 0.0, 1.0, &c, None, 0.0, false)).fold(0.0, f64::max);
        assert!(peak > 1.1 && peak < 1.2, "peak {peak}");
        let n = measure(&c, 30.0).unwrap();
        assert!((25..40).contains(&n), "settles in {n} frames");
        // past the settle it stays within the threshold
        for f in n..n + 300 {
            assert!((spring_value(f as f64, 30.0, 0.0, 1.0, &c, None, 0.0, false) - 1.0).abs() < DEFAULT_REST);
        }
    }

    #[test]
    fn critical_damping_never_overshoots() {
        let c = SpringConfig::new(1.0, 20.0, 100.0);
        for f in 0..120 {
            assert!(spring_value(f as f64, 30.0, 0.0, 1.0, &c, None, 0.0, false) <= 1.0 + 1e-12);
        }
    }

    #[test]
    fn a_seek_is_the_same_as_a_play_through() {
        let c = SpringConfig::new(1.0, 12.0, 180.0);
        let played: Vec<f64> = (0..18_000).map(|f| spring_value(f as f64, 30.0, -50.0, 50.0, &c, Some(20.0), 3.0, false)).collect();
        for f in (0..18_000).rev().step_by(97) {
            assert_eq!(played[f], spring_value(f as f64, 30.0, -50.0, 50.0, &c, Some(20.0), 3.0, false));
        }
    }

    #[test]
    fn stretch_delay_and_reverse() {
        let c = SpringConfig::new(1.0, 20.0, 100.0);
        assert_eq!(spring_value(4.0, 30.0, 0.0, 1.0, &c, None, 5.0, false), 0.0);
        let s = |f: f64| spring_value(f, 30.0, 0.0, 1.0, &c, Some(10.0), 0.0, false);
        assert!(s(10.0) > 0.99 && s(5.0) > 0.3);
        let r = |f: f64| spring_value(f, 30.0, 0.0, 1.0, &c, Some(10.0), 0.0, true);
        assert!(r(0.0) > 0.99 && r(10.0) < 0.01);
    }
}
