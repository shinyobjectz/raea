//! A camera that jumps is a cut (docs/raytracing.feature RT31). A cut to another camera is marked (raea-sys-frame's
//! Shots, RR2), but one camera moved far in one frame, as a game's respawn or teleport moves it, wasn't: Solari's and
//! MetalFX's history carried the last view into the new one for half a second (lightbench window-cut: the worst tile
//! 0.50 stops off on the first frame, 0.38 on the third, against 0.14 and 0.12 with the history dropped), steady's
//! filter blended it in, and motion blur streaked every pixel along the jump.
//!
//! A jump is a turn of more than half the field of view in one frame, a lens changed by half or more, or a step ten
//! times the camera's recent pace and longer than ten near planes. So a fast, steady move (a flight, a whip pan eased
//! in, a player's flick of 90 degrees in a quarter second) is no cut, and a teleport from rest is. A camera's first move
//! since it appeared (or cut) is a shot opening on a move, no step cut, unless it was seen resting first, unmoved from
//! one film frame to the next twice: so a shot opening on a fast move keeps its blur, as a frame drawn alone (`--at`,
//! where the engine loads the frame's pose at frame 0 and then steps from the frame before) does.

use bevy::{
    prelude::*,
    render::extract_component::{ExtractComponent, ExtractComponentPlugin},
};

/// Whether the camera jumped this frame, set on each 3D camera every frame: its temporal history starts again (Solari's,
/// MetalFX's, steady's) and its motion blur draws nothing.
#[derive(Component, Clone, Copy, Default, PartialEq, Debug, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
pub struct CameraCut(pub bool);

/// a camera's pose and lens at its last update, and that update's film frame; how far it has been stepping a frame (an
/// average over its last few moves), once it has moved since it appeared or last cut (`paced`); and for how many film
/// frames in a row it has been seen unmoved (`still`)
#[derive(Component, Clone, Copy, Debug)]
struct Pace {
    at: Vec3,
    rot: Quat,
    fov: f32,
    frame: i64,
    step: f32,
    paced: bool,
    still: u32,
}

/// the lens's field of view (radians; an orthographic camera's counted as 45 degrees), and the shortest step that can be
/// a cut however still the camera was (ten near planes, or half an orthographic view's height)
fn lens(p: &Projection) -> (f32, f32) {
    match p {
        Projection::Perspective(p) => (p.fov, 10.0 * p.near),
        Projection::Orthographic(o) => (std::f32::consts::FRAC_PI_4, 0.5 * o.area.height()),
        _ => (std::f32::consts::FRAC_PI_4, 1.0),
    }
}

/// Whether a move from `was` to (`at`, `rot`, `fov`) is a cut, `least` the shortest step that can be one.
fn jumped(was: &Pace, at: Vec3, rot: Quat, fov: f32, least: f32) -> bool {
    let step = at.distance(was.at);
    let turn = was.rot.angle_between(rot);
    let lens = (fov / was.fov).max(was.fov / fov);
    let opening = !was.paced && was.still < 2;
    turn > 0.5 * fov || lens > 1.5 || (!opening && step > 10.0 * was.step && step > least)
}

/// Where each camera's CameraCut is set, in PostUpdate after the transforms: a system that acts on a cut runs after it.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DetectCuts;

pub fn install(app: &mut App) {
    app.add_plugins(ExtractComponentPlugin::<CameraCut>::default());
    app.add_systems(PostUpdate, detect.in_set(DetectCuts).after(bevy::transform::TransformSystems::Propagate));
}

/// Each 3D camera's CameraCut this frame (the film frame, raea-sys-frame's Clock; an engine without one counts its
/// updates). An update that leaves a camera where it was (motion blur's step to the frame before draws a frame's pose
/// twice) is no cut and doesn't count toward its pace; a cut's step doesn't either.
fn detect(
    mut commands: Commands,
    clock: Option<Res<raea_sys_frame::Clock>>,
    mut updates: Local<i64>,
    mut cams: Query<(Entity, &GlobalTransform, &Projection, Option<&mut Pace>, Option<&mut CameraCut>), With<Camera3d>>,
) {
    let frame = clock.map_or(*updates, |c| c.0);
    *updates += 1;
    for (e, t, p, pace, cut) in &mut cams {
        let (_, rot, at) = t.to_scale_rotation_translation();
        let (fov, least) = lens(p);
        let now = match pace {
            None => {
                commands.entity(e).insert(Pace { at, rot, fov, frame, step: 0.0, paced: false, still: 0 });
                false
            }
            Some(mut was) => {
                let moved = was.at != at || was.rot != rot || was.fov != fov;
                let now = moved && jumped(&was, at, rot, fov, least);
                if now {
                    *was = Pace { at, rot, fov, frame, step: 0.0, paced: false, still: 0 };
                } else if moved {
                    let d = at.distance(was.at);
                    let step = if was.paced { 0.75 * was.step + 0.25 * d } else { d };
                    *was = Pace { at, rot, fov, frame, step, paced: true, still: 0 };
                } else if frame != was.frame {
                    // unmoved from one film frame to the next, or seen again after a gap (a frame drawn alone)
                    was.still = if frame == was.frame + 1 { was.still + 1 } else { 0 };
                    was.frame = frame;
                }
                now
            }
        };
        match cut {
            Some(mut c) if c.0 != now => c.0 = now,
            Some(_) => {}
            None => {
                commands.entity(e).insert(CameraCut(now));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOV: f32 = 44.0 * std::f32::consts::PI / 180.0;

    /// a camera seen unmoved for ten frames
    fn at_rest() -> Pace {
        Pace { at: Vec3::ZERO, rot: Quat::IDENTITY, fov: FOV, frame: 10, step: 0.0, paced: false, still: 10 }
    }

    #[test]
    fn a_teleport_is_a_cut_and_a_first_step_is_not() {
        assert!(jumped(&at_rest(), Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, FOV, 1.0));
        assert!(!jumped(&at_rest(), Vec3::new(0.02, 0.0, 0.0), Quat::IDENTITY, FOV, 1.0));
    }

    #[test]
    fn a_shot_that_opens_on_a_fast_move_is_no_cut() {
        let new = Pace { still: 0, ..at_rest() };
        assert!(!jumped(&new, Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, FOV, 1.0));
        // a turn past half the view still is
        assert!(jumped(&new, Vec3::ZERO, Quat::from_rotation_y(1.0), FOV, 1.0));
    }

    #[test]
    fn a_steady_fast_flight_is_no_cut() {
        let flying = Pace { step: 4.0, paced: true, ..at_rest() };
        assert!(!jumped(&flying, Vec3::new(4.5, 0.0, 0.0), Quat::IDENTITY, FOV, 1.0));
    }

    #[test]
    fn a_turn_past_half_the_view_is_a_cut_and_a_flick_is_not() {
        let deg = |d: f32| Quat::from_rotation_y(d.to_radians());
        assert!(jumped(&at_rest(), Vec3::ZERO, deg(30.0), FOV, 1.0));
        // 90 degrees in 15 frames, eased: 9 degrees at its fastest
        assert!(!jumped(&at_rest(), Vec3::ZERO, deg(9.0), FOV, 1.0));
    }

    #[test]
    fn another_lens_is_a_cut() {
        assert!(jumped(&at_rest(), Vec3::ZERO, Quat::IDENTITY, FOV / 2.0, 1.0));
        assert!(!jumped(&at_rest(), Vec3::ZERO, Quat::IDENTITY, FOV * 1.1, 1.0));
    }
}
