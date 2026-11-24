//! The camera's motion blur: raea/3d's MotionBlur, raea's Shutter on the camera (shutter.rs; RAEA_MOTION_BLUR=bevy
//! puts Bevy's own MotionBlur there instead, to compare), its values keyed per frame, so a blur ramp plays as it
//! renders; a frame without it has the component removed, so off is the camera without it.
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

use crate::shutter::Shutter;

/// Whether a camera row has a MotionBlur in some frame.
pub fn has_blur(vals: &BTreeMap<String, V>) -> bool {
    vals.contains_key("mb_on")
}

/// What a camera row with motion blur needs from its load: the depth and motion vector prepasses the blur reads.
pub fn load(world: &mut World, cam: Entity, vals: &BTreeMap<String, V>) {
    if has_blur(vals) {
        world.entity_mut(cam).insert((DepthPrepass, MotionVectorPrepass));
    }
}

/// The frame's blur, None where it is off.
fn blur_of(v: &BTreeMap<String, V>) -> Option<MotionBlur> {
    let b = MotionBlur { shutter_angle: num(v, "mb_shutter", 0.5).max(0.0), samples: num(v, "mb_samples", 4.0).round().clamp(1.0, 32.0) as u32 };
    (num(v, "mb_on", 0.0) > 0.5 && b.shutter_angle > 0.0).then_some(b)
}

/// Whether the camera's blur is Bevy's MotionBlur (<MotionBlur kind="bevy">) rather than raea's Shutter; RAEA_MOTION_BLUR
/// (bevy or shutter) overrides it on every camera, said once (docs/nobuild.feature NB1a).
fn bevy_blur(v: &BTreeMap<String, V>) -> bool {
    match std::env::var("RAEA_MOTION_BLUR").as_deref() {
        Ok(k @ ("bevy" | "shutter")) => {
            static SAID: std::sync::Once = std::sync::Once::new();
            SAID.call_once(|| eprintln!("raea: <MotionBlur> on every camera: RAEA_MOTION_BLUR={k} over kind (docs/nobuild.feature NB1a)"));
            k == "bevy"
        }
        _ => num(v, "mb_bevy", 0.0) > 0.5,
    }
}

/// raea's shutter for a frame's blur: as long open, two taps a pixel for each of Bevy's samples each way.
fn shutter_of(b: &MotionBlur) -> Shutter {
    Shutter { open: b.shutter_angle, taps: 2 * b.samples }
}

/// The frame's blur onto the camera: inserted, changed or removed as the frame's columns say.
pub fn sync_motion_blur(frame: Res<FrameRows>, mut commands: Commands, cams: Query<(Entity, Option<&MotionBlur>, Option<&Shutter>), With<Camera3d>>) {
    for (e, mb, shutter) in &cams {
        let Some(v) = frame.vals(e) else { continue };
        if !has_blur(&v) {
            continue;
        }
        if !bevy_blur(&v) {
            match (blur_of(&v).map(|b| shutter_of(&b)), shutter) {
                (Some(want), Some(s)) if *s == want => {}
                (Some(want), _) => {
                    commands.entity(e).insert(want);
                }
                (None, Some(_)) => {
                    commands.entity(e).remove::<Shutter>();
                }
                (None, None) => {}
            }
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
pub fn pin_frame_count(clock: Res<Clock>, cams: Query<(Has<MotionBlur>, Has<Shutter>), With<Camera3d>>, mut count: ResMut<bevy::diagnostic::FrameCount>) {
    if !cams.iter().any(|(mb, s)| mb || s) {
        return;
    }
    let want = clock.0.max(0) as u32;
    if count.0 != want {
        count.0 = want;
    }
}

/// The systems and raea's shutter pass, on every engine (they do nothing without a blur).
pub fn install(app: &mut App) {
    crate::shutter::install(app);
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

    #[test]
    fn the_shutter_takes_two_taps_for_each_sample_each_way() {
        let s = shutter_of(&MotionBlur { shutter_angle: 0.5, samples: 16 });
        assert_eq!((s.open, s.taps), (0.5, 32));
    }
}
