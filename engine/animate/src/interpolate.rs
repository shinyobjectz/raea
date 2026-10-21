//! Values that blend (forked from animato-core 1.7.1's `Interpolate`, in f64).

/// A value that can be blended between two states: `a.lerp(b, 0) == a`, `a.lerp(b, 1) == b`; `t` outside [0, 1]
/// extrapolates (a spring's overshoot).
pub trait Interpolate: Sized {
    fn lerp(&self, other: &Self, t: f64) -> Self;
}

impl Interpolate for f64 {
    #[inline]
    fn lerp(&self, other: &Self, t: f64) -> Self {
        // exact at both ends, whatever the magnitudes
        if t == 0.0 {
            *self
        } else if t == 1.0 {
            *other
        } else {
            self + (other - self) * t
        }
    }
}

impl Interpolate for f32 {
    #[inline]
    fn lerp(&self, other: &Self, t: f64) -> Self {
        (*self as f64).lerp(&(*other as f64), t) as f32
    }
}

impl<const N: usize> Interpolate for [f64; N] {
    fn lerp(&self, other: &Self, t: f64) -> Self {
        std::array::from_fn(|i| self[i].lerp(&other[i], t))
    }
}

impl<const N: usize> Interpolate for [f32; N] {
    fn lerp(&self, other: &Self, t: f64) -> Self {
        std::array::from_fn(|i| self[i].lerp(&other[i], t))
    }
}

impl Interpolate for i64 {
    fn lerp(&self, other: &Self, t: f64) -> Self {
        (*self as f64).lerp(&(*other as f64), t).round() as i64
    }
}
