//! The Bevy side (after animato-bevy 1.7.1's plugin, which drives tweens from bevy_time's delta): here everything is
//! sampled from raea's frame clock, a resource holding the composition's integer frame and its rate. Nothing reads
//! Time; a seek is setting the frame. `AnimatePlugin` adds the clock and samples `TransformAnimation` components
//! (entities a game runner spawns itself) into their Transform each update. React-mirrored elements are sampled by
//! raea-react (sdk/react/src/animate.rs) from the same clock.

use bevy::prelude::*;

use crate::spec::{Animation, Compiled, compile};

/// raea's frame clock: the composition frame being drawn and its frames per second. Whoever steps frames sets it
/// (raea-react sets it from each commit's frame marker); every animation is a function of it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct FrameClock {
    pub frame: i64,
    pub fps: f64,
}

impl Default for FrameClock {
    fn default() -> Self {
        FrameClock { frame: 0, fps: 30.0 }
    }
}

impl FrameClock {
    /// Seconds since frame 0, from the index (no accumulated deltas, so no drift).
    pub fn secs(&self) -> f64 {
        self.frame as f64 / self.fps
    }
}

/// A declared animation on an entity's Transform: `x`, `y`, `z` set its translation, `rotateX`, `rotateY`, `rotate`
/// its rotation (radians, applied X then Y then Z), `scale`, `scaleX`, `scaleY` its scale; properties it doesn't set
/// keep `base`'s.
#[derive(Component, Clone, Debug)]
pub struct TransformAnimation {
    pub compiled: Compiled,
    pub base: Transform,
}

impl TransformAnimation {
    pub fn new(a: &Animation, base: Transform) -> Result<Self, String> {
        Ok(TransformAnimation { compiled: compile(a)?, base })
    }

    /// The Transform at composition frame `frame`.
    pub fn at(&self, frame: f64) -> Transform {
        let s = self.compiled.sample(frame);
        let mut t = self.base;
        if let Some((x, _)) = &s.x {
            t.translation.x = *x as f32;
        }
        if let Some((y, _)) = &s.y {
            t.translation.y = *y as f32;
        }
        if let Some(z) = s.z {
            t.translation.z = z as f32;
        }
        if s.rotate_x.is_some() || s.rotate_y.is_some() || s.rotate.is_some() {
            let (bx, by, bz) = self.base.rotation.to_euler(EulerRot::XYZ);
            t.rotation = Quat::from_euler(EulerRot::XYZ, s.rotate_x.map_or(bx, |v| v as f32), s.rotate_y.map_or(by, |v| v as f32), s.rotate.map_or(bz, |v| v as f32));
        }
        if let Some(sx) = s.scale_x {
            t.scale.x = sx as f32;
        }
        if let Some(sy) = s.scale_y {
            t.scale.y = sy as f32;
        }
        if s.scale_x.is_some() && s.scale_y.is_some() && s.scale_x == s.scale_y {
            t.scale.z = s.scale_x.unwrap() as f32;
        }
        t
    }
}

/// The frame clock, and Transform animations sampled from it before transforms propagate.
pub struct AnimatePlugin;

impl Plugin for AnimatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameClock>().register_type::<Animation>().add_systems(PostUpdate, sample_transforms.before(bevy::transform::TransformSystems::Propagate));
    }
}

fn sample_transforms(clock: Res<FrameClock>, mut q: Query<(&TransformAnimation, &mut Transform)>) {
    let f = clock.frame as f64;
    for (a, mut t) in &mut q {
        let next = a.at(f);
        if *t != next {
            *t = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{AnimKey, AnimTrack};

    #[test]
    fn transforms_follow_the_frame_clock_forwards_and_back() {
        let a = Animation { fps: 30.0, tracks: vec![AnimTrack { prop: "y".into(), keys: vec![AnimKey { at: 0.0, value: 0.0, ease: "out-cubic".into(), ..Default::default() }, AnimKey { at: 10.0, value: 2.0, ..Default::default() }], ..Default::default() }], ..Default::default() };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(AnimatePlugin);
        let e = app.world_mut().spawn((Transform::default(), TransformAnimation::new(&a, Transform::default()).unwrap())).id();
        let mut played = Vec::new();
        for f in 0..20 {
            app.world_mut().resource_mut::<FrameClock>().frame = f;
            app.update();
            played.push(app.world().get::<Transform>(e).unwrap().translation.y);
        }
        for f in (0..20).rev() {
            app.world_mut().resource_mut::<FrameClock>().frame = f;
            app.update();
            assert_eq!(app.world().get::<Transform>(e).unwrap().translation.y, played[f as usize]);
        }
        assert_eq!(played[10], 2.0);
    }
}
