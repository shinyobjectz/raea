//! raea-animate: raea's animation substrate, a fork of Animato 1.7.1 (MIT OR Apache-2.0, Aarambh Dev Hub; see NOTICE)
//! rebuilt to be frame-pure. Every animation here is a function of time, `value_at(t)`, evaluated from the
//! composition's frame index; nothing accumulates a delta, so a seek forwards or backwards gives exactly the value a
//! play-through gives, and ten minutes of frames carry no drift (f64 time from integer frames).
//!
//! - `easing`: Animato's curves, plus text names, `reverse()` and stretched springs
//! - `track`: keyframe tracks and tweens, held or repeated past their keys
//! - `timeline`: sequence, parallel, stagger (grid, centre-out, edges-in, seeded random) as schedules of spans
//! - `spring`: the damped oscillator in closed form, @raea/motion's spring() semantics
//! - `path`: Bezier curves and Catmull-Rom splines by arc length
//! - `color`: sRGB parsed from CSS, blended in Oklab, Oklch or Lab
//! - `spec`: the declared-animation wire format @raea/motion's <Animate> sends once, compiled and sampled per frame
//! - `bevy` (feature): the frame clock resource, Transform animations and AnimatePlugin

pub mod color;
pub mod easing;
pub mod interpolate;
pub mod path;
pub mod spec;
pub mod spring;
pub mod timeline;
pub mod track;

#[cfg(feature = "bevy")]
pub mod bevy;

pub use color::{Color, Space};
pub use easing::Easing;
pub use interpolate::Interpolate;
pub use spec::{AnimKey, AnimTrack, Animation, Compiled, Sampled, compile};
pub use spring::{Spring, SpringConfig};
pub use timeline::{At, StaggerPattern, Timeline};
pub use track::{Repeat, Track, Tween};

/// The upstream this is forked from.
pub const UPSTREAM: &str = "animato 1.7.1 (crates.io), github.com/AarambhDevHub/animato";
