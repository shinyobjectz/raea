//! The camera's motion blur: @raea/motion/3d's MotionBlur, Bevy's own MotionBlur on the camera, its values keyed per
//! frame, so a blur ramp plays as it renders; a frame without it has the component removed, so off is Bevy without it.
//!
//! Camera columns: mb_on (keyed: 1 where the frame has a MotionBlur), mb_shutter (the shutter angle, a fraction of a
//!   frame, keyed), mb_samples
//!
//! Motion blur is made a function of the frame: Bevy takes each mesh's, skin's and the camera's previous pose from the
//! update before (its GPU instance builder keeps the last update's transforms), which in raea may be the same frame (a
//! careful readback updates a frame several times) or a frame jumped from. So with MotionBlur the engine draws frame
//! f - 1 before every update of frame f (`Engine::pre_step`, its readback ignored), and `pin_frame_count` sets the
//! shaders' frame counter (the blur's noise) to the frame: every update of frame f draws the same blur.

use super::*;
use bevy::core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass};
use bevy::post_process::motion_blur::MotionBlur;

/// Whether a camera row has a MotionBlur in some frame.
pub(super) fn has_blur(vals: &BTreeMap<String, V>) -> bool {
    vals.contains_key("mb_on")
}

/// What a camera row with motion blur needs from its load: the depth and motion vector prepasses the blur reads.
pub(super) fn load(world: &mut World, cam: Entity, vals: &BTreeMap<String, V>) {
    if has_blur(vals) {
        world.entity_mut(cam).insert((DepthPrepass, MotionVectorPrepass));
    }
}

/// The frame's blur, None where it is off.
fn blur_of(v: &BTreeMap<String, V>) -> Option<MotionBlur> {
    let b = MotionBlur { shutter_angle: num(v, "mb_shutter", 0.5).max(0.0), samples: num(v, "mb_samples", 4.0).round().clamp(1.0, 32.0) as u32 };
    (num(v, "mb_on", 0.0) > 0.5 && b.shutter_angle > 0.0).then_some(b)
}

/// The frame's blur onto the camera: inserted, changed or removed as the frame's columns say.
pub(super) fn sync_motion_blur(frame: Res<FrameRows>, mut commands: Commands, cams: Query<(Entity, Option<&MotionBlur>), With<Camera3d>>) {
    for (e, mb) in &cams {
        let Some(v) = frame.vals(e) else { continue };
        if !has_blur(&v) {
            continue;
        }
        match (blur_of(&v), mb) {
            (Some(want), Some(m)) if m.shutter_angle == want.shutter_angle && m.samples == want.samples => {}
            (Some(want), _) => {
                commands.entity(e).insert(want);
            }
            (None, Some(_)) => {
                commands.entity(e).remove::<MotionBlur>();
            }
            (None, None) => {}
        }
    }
}

/// The shaders' frame counter at the frame being drawn, while motion blur is on (its noise is seeded by it), so every
/// update of frame f samples the same.
pub(super) fn pin_frame_count(clock: Res<Clock>, cams: Query<Has<MotionBlur>, With<Camera3d>>, mut count: ResMut<bevy::diagnostic::FrameCount>) {
    if !cams.iter().any(|mb| mb) {
        return;
    }
    let want = clock.0.max(0) as u32;
    if count.0 != want {
        count.0 = want;
    }
}

/// The systems, on every engine (they do nothing without a MotionBlur).
pub(super) fn install(app: &mut App) {
    app.add_systems(Last, pin_frame_count.after(bevy::diagnostic::update_frame_count));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_frames_and_zero_shutters_are_no_blur() {
        let mut v = BTreeMap::new();
        v.insert("mb_on".to_string(), V::N(0.0));
        v.insert("mb_shutter".to_string(), V::N(1.0));
        assert!(blur_of(&v).is_none());
        v.insert("mb_on".to_string(), V::N(1.0));
        let b = blur_of(&v).unwrap();
        assert_eq!((b.shutter_angle, b.samples), (1.0, 4));
        v.insert("mb_shutter".to_string(), V::N(0.0));
        assert!(blur_of(&v).is_none());
    }
}
