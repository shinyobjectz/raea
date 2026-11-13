//! The camera's passes after the scene is drawn, each a function of the frame: the lens (Fisheye, LensDistortion), motion
//! blur, Bevy's depth of field and effect stack patched to keep the frame's alpha, and ink outlines; and the camera row's
//! post-processing (the sky, bloom, fog, the grade, AO and anti-aliasing, volumetric fog) put on the camera frame by frame.

use std::collections::BTreeMap;

use bevy::prelude::*;
use raea_core_program::ir::V;
use raea_sys_frame::*;
use raea_sys_frame::bevy_api;

pub mod camera;
pub mod lens;
pub mod views;
pub mod motion_blur;
pub mod dof;
pub mod effect_stack;
pub mod outline;

/// A live scene's camera made before its first 3D part (MSAA off, as a 2D scene's is) multisamples once a frame brings
/// one, as a render's camera does from its load, which sees every frame's rows (#38: kept-mount's cube mounts at frame
/// 10 and had hard edges live). Not one whose row asks for a single-sampled frame (ssao, aa, outline, frame_looks,
/// passes: spawn_camera's rule), not in preview, and not a deferred camera (a ray-traced one): Bevy turns MSAA off
/// on those itself each frame and says so, so this turned it on again every frame, a warning a frame in a render's
/// log and the camera's pipelines made again for nothing.
pub fn msaa_for_3d(world: &mut World, has_3d: bool) {
    if !has_3d || preview() {
        return;
    }
    let cams: Vec<Entity> = world.query_filtered::<Entity, With<Camera3d>>().iter(world).collect();
    for e in cams {
        let single = world.resource::<FrameRows>().vals(e).is_none_or(|v| ["ssao", "aa", "outline", "frame_looks", "passes"].iter().any(|k| v.contains_key(*k)));
        let deferred = world.entity(e).contains::<bevy::core_pipeline::prepass::DeferredPrepass>();
        if !single && !deferred && world.get::<Msaa>(e) == Some(&Msaa::Off) {
            world.entity_mut(e).insert(Msaa::default());
        }
    }
}
