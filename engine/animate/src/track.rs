//! Keyframe tracks and tweens (forked from animato-tween 1.7.1's KeyframeTrack, Tween and Loop). Animato's are
//! stateful: update(dt) adds to an elapsed time, so the value depends on how it got there. These are functions of
//! time: `value_at(t)` with t in whatever unit the keys use (raea uses frames), held before the first key and after
//! the last, or repeated.

use crate::easing::Easing;
use crate::interpolate::Interpolate;

/// One key: the value at `at`, and the curve from it to the next key.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe<T> {
    pub at: f64,
    pub value: T,
    pub ease: Easing,
}

/// What happens past the last key (Animato's Loop).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Repeat {
    /// holds the last value
    #[default]
    Once,
    /// plays the keys again, `n` times in all (None: for ever)
    Loop(Option<u32>),
    /// forwards then backwards, `n` passes in all (None: for ever)
    PingPong(Option<u32>),
}

/// Keys sorted by time; a key at the same time as another replaces it.
#[derive(Clone, Debug, PartialEq)]
pub struct Track<T> {
    keys: Vec<Keyframe<T>>,
    pub repeat: Repeat,
}

impl<T> Default for Track<T> {
    fn default() -> Self {
        Track { keys: Vec::new(), repeat: Repeat::Once }
    }
}

impl<T> Track<T> {
    pub fn new() -> Self {
        Track::default()
    }

    /// A key, eased linearly to the next.
    pub fn key(self, at: f64, value: T) -> Self {
        self.key_eased(at, value, Easing::Linear)
    }

    pub fn key_eased(mut self, at: f64, value: T, ease: Easing) -> Self {
        let k = Keyframe { at, value, ease };
        match self.keys.binary_search_by(|e| e.at.partial_cmp(&at).unwrap_or(std::cmp::Ordering::Equal)) {
            Ok(i) => self.keys[i] = k,
            Err(i) => self.keys.insert(i, k),
        }
        self
    }

    pub fn repeat(mut self, r: Repeat) -> Self {
        self.repeat = r;
        self
    }

    pub fn keys(&self) -> &[Keyframe<T>] {
        &self.keys
    }

    pub fn start(&self) -> f64 {
        self.keys.first().map_or(0.0, |k| k.at)
    }

    pub fn end(&self) -> f64 {
        self.keys.last().map_or(0.0, |k| k.at)
    }

}

impl<T: Interpolate + Clone> Track<T> {
    /// The value at time `t`; None for a track with no keys.
    pub fn value_at(&self, t: f64) -> Option<T> {
        let (i, p) = self.locate(t)?;
        if p == 0.0 || i + 1 >= self.keys.len() {
            return Some(self.keys[i].value.clone());
        }
        Some(self.keys[i].value.lerp(&self.keys[i + 1].value, p))
    }
}

impl<T> Track<T> {
    /// The time within the keys' span that `t` plays, after repeats.
    fn fold(&self, t: f64) -> f64 {
        let (a, b) = (self.keys.first().map_or(0.0, |k| k.at), self.keys.last().map_or(0.0, |k| k.at));
        let span = b - a;
        // a loop's pass starts again exactly at the end of the last (frame b of a loop is frame a of its next pass)
        let looping = matches!(self.repeat, Repeat::Loop(_));
        if span <= 0.0 || t < b || (t == b && !looping) {
            return t;
        }
        let k = (t - a) / span;
        match self.repeat {
            Repeat::Once => t,
            Repeat::Loop(n) => {
                if n.is_some_and(|n| k >= n.max(1) as f64) {
                    return b;
                }
                a + (t - a).rem_euclid(span)
            }
            Repeat::PingPong(n) => {
                if let Some(n) = n {
                    let n = n.max(1) as f64;
                    if k >= n {
                        return if (n as u64) % 2 == 1 { b } else { a };
                    }
                }
                let c = (t - a).rem_euclid(2.0 * span);
                if c <= span { a + c } else { a + 2.0 * span - c }
            }
        }
    }

    /// Where time `t` falls: the key it's past and the eased progress toward the next (0 at the key itself; the
    /// last key with 0 past the end). None for a track with no keys. Any value type blends from this (colours do).
    pub fn locate(&self, t: f64) -> Option<(usize, f64)> {
        let first = self.keys.first()?;
        if self.keys.len() == 1 {
            return Some((0, 0.0));
        }
        let t = self.fold(t);
        if t <= first.at {
            return Some((0, 0.0));
        }
        let last = self.keys.len() - 1;
        if t >= self.keys[last].at {
            return Some((last, 0.0));
        }
        let i = self.keys.partition_point(|k| k.at <= t).saturating_sub(1);
        let (cur, next) = (&self.keys[i], &self.keys[i + 1]);
        let span = (next.at - cur.at).max(f64::EPSILON);
        let p = ((t - cur.at) / span).clamp(0.0, 1.0);
        Some((i, cur.ease.apply(p)))
    }
}

/// One value moving from `from` to `to` over `duration` starting at `start` (Animato's Tween, stateless).
#[derive(Clone, Debug, PartialEq)]
pub struct Tween<T> {
    pub from: T,
    pub to: T,
    pub start: f64,
    pub duration: f64,
    pub ease: Easing,
    pub repeat: Repeat,
}

impl<T: Interpolate + Clone> Tween<T> {
    pub fn new(from: T, to: T, duration: f64) -> Self {
        Tween { from, to, start: 0.0, duration, ease: Easing::Linear, repeat: Repeat::Once }
    }
    pub fn delay(mut self, start: f64) -> Self {
        self.start = start;
        self
    }
    pub fn ease(mut self, e: Easing) -> Self {
        self.ease = e;
        self
    }
    pub fn repeat(mut self, r: Repeat) -> Self {
        self.repeat = r;
        self
    }
    pub fn track(&self) -> Track<T> {
        Track::new().key_eased(self.start, self.from.clone(), self.ease.clone()).key(self.start + self.duration.max(0.0), self.to.clone()).repeat(self.repeat)
    }
    pub fn value_at(&self, t: f64) -> T {
        if self.duration <= 0.0 {
            return if t < self.start { self.from.clone() } else { self.to.clone() };
        }
        self.track().value_at(t).unwrap_or_else(|| self.to.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_outside_and_eases_inside() {
        let t = Track::<f64>::new().key_eased(0.0, 30.0, Easing::EaseOutCubic).key(6.0, 0.0);
        assert_eq!(t.value_at(-5.0), Some(30.0));
        assert_eq!(t.value_at(0.0), Some(30.0));
        assert_eq!(t.value_at(6.0), Some(0.0));
        assert_eq!(t.value_at(100.0), Some(0.0));
        let mid = t.value_at(3.0).unwrap();
        assert!((mid - 30.0 * (1.0 - (1.0 - 0.125))).abs() < 1e-9, "{mid}");
    }

    #[test]
    fn three_keys_pop() {
        let t = Track::<f64>::new().key_eased(0.0, 0.6, Easing::EaseOutCubic).key_eased(4.0, 1.08, Easing::EaseOutCubic).key(8.0, 1.0);
        assert_eq!(t.value_at(4.0), Some(1.08));
        assert!(t.value_at(6.0).unwrap() < 1.08 && t.value_at(6.0).unwrap() > 1.0);
    }

    #[test]
    fn loops_and_ping_pongs() {
        let t = Track::<f64>::new().key(0.0, 0.0).key(10.0, 1.0).repeat(Repeat::Loop(None));
        assert!((t.value_at(15.0).unwrap() - 0.5).abs() < 1e-12);
        let p = Track::<f64>::new().key(0.0, 0.0).key(10.0, 1.0).repeat(Repeat::PingPong(None));
        assert!((p.value_at(15.0).unwrap() - 0.5).abs() < 1e-12);
        assert!((p.value_at(18.0).unwrap() - 0.2).abs() < 1e-12);
        let l2 = Track::<f64>::new().key(0.0, 0.0).key(10.0, 1.0).repeat(Repeat::Loop(Some(2)));
        assert_eq!(l2.value_at(25.0), Some(1.0));
    }

    #[test]
    fn no_drift_over_ten_minutes_of_frames() {
        // a 2 s loop at 60 fps sampled at frame indices across ten minutes: every pass is the first, exactly
        let t = Track::<f64>::new().key_eased(0.0, 0.0, Easing::EaseInOutSine).key(120.0, 1.0).repeat(Repeat::Loop(None));
        let first: Vec<f64> = (0..120).map(|f| t.value_at(f as f64).unwrap()).collect();
        for f in 0..36_000u64 {
            assert_eq!(t.value_at(f as f64).unwrap(), first[(f % 120) as usize]);
        }
    }
}
