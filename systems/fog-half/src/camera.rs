//! Volumetric fog on the camera as its row has it, a group of the camera's post-processing (raea_sys_frame::camera::
//! Group): on with the frames whose set has it, taken off and put on again when they change (docs/rerender.feature
//! RR1). Here rather than in raea-sys-post's GROUPS because it puts on this crate's HalfFog, and a system can't use
//! another sideways (tools/layers.toml): raea-runtime-scene3d hands it to raea-sys-post's apply and sync beside them.

use std::collections::BTreeMap;

use bevy::prelude::*;
use raea_core_program::ir::V;
use raea_sys_fog::froxel;
use raea_sys_frame::camera::{Group, remove};
use raea_sys_frame::{colour, num, Clock};

type Vals = BTreeMap<String, V>;

/// The volumetric fog group: every `vfog` column.
pub const VFOG: Group = Group { keys: |k| k.starts_with("vfog"), on: vfog, off: remove::<(froxel::FroxelFog, crate::HalfFog, bevy::light::VolumetricFog, FogDither)> };

/// Volumetric fog: light in the air inside each fog volume. Its jitter is raea's dither (install_fog_shader): 0 here,
/// and with `vfog_dither` set each frame by sync_fog_dither from the film frame, so a frame's fog is a function of the
/// frame either way (none when drawing a pass: passes.rs).
pub fn vfog(world: &mut World, e: Entity, vals: &Vals) {
    if !vals.get("vfog").is_some_and(V::truthy) || raea_sys_compose::passes::wanted().is_some() {
        return;
    }
    // RAEA_FOG_REFERENCE=1: the fog's ground truth (sdk/raea/bench/bevy/fog-score.py scores against it): Bevy's march at
    // 512 steps (RAEA_FOG_REFERENCE_STEPS), undithered, at full resolution, whatever the composition asks for. Converged
    // by then (the rome curia: 256 against 512 SSIM 0.99988); 1024 at 1080p runs past what Metal lets one pass take and
    // the fog pass is dropped without an error, the frame drawn with no fog at all
    let reference = fog_reference();
    let fog = bevy::light::VolumetricFog {
        step_count: if reference { std::env::var("RAEA_FOG_REFERENCE_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(512) } else { num(vals, "vfog_steps", 64.0).round().max(1.0) as u32 },
        ambient_intensity: num(vals, "vfog_ambient", 0.0),
        ambient_color: colour(vals.get("vfog_ambient_color"), [1.0, 1.0, 1.0, 1.0]),
        jitter: 0.0,
    };
    // every volume in one grid along the view (froxel.rs), unless vfog_mode is march (or RAEA_FOG_MODE=march, for
    // comparing): Bevy's march, where vfog_resolution below 1 marches at that fraction of the frame, upsampled by depth
    // (fog_half.rs). The grid scored at or above the march on every scene fog-score.py judges (the fixtures, the rome
    // film's plot and curia, Mean Streets), in SSIM, PSNR, ms a frame and bitrate, so it's the default;
    // RAEA_FOG_MODE=froxel forces it over a march row
    let env_mode = std::env::var("RAEA_FOG_MODE");
    let march = vals.get("vfog_mode").is_some_and(|m| matches!(m, V::S(s) if s == "march")) && env_mode.as_deref() != Ok("froxel");
    let grid = !reference && !march && env_mode.as_deref() != Ok("march");
    if grid {
        let lin = fog.ambient_color.to_linear();
        // the props, each RAEA_FOG_* variable over its own (raea_sys_fog::froxel::tuning)
        let (tile, slices, samples) = froxel::tuning::grid(
            num(vals, "vfog_tile", 16.0).round().clamp(4.0, 16.0) as u32,
            num(vals, "vfog_slices", 64.0).round().clamp(32.0, 256.0) as u32,
            num(vals, "vfog_samples", 1.0).round().clamp(1.0, 8.0) as u32,
        );
        let f = froxel::FroxelFog {
            ambient: Vec3::new(lin.red, lin.green, lin.blue) * fog.ambient_intensity,
            slices,
            tile,
            samples,
            // (the ground truth integrates every light itself)
            hero: froxel::tuning::hero(num(vals, "vfog_hero", 1.0) != 0.0) && froxel::reference_steps().is_none(),
            tune: froxel::tuning::tuning(vals),
        };
        froxel::tuning::record(e, "froxel", Some(&f));
        froxel::insert(world, e, f);
    } else {
        froxel::tuning::record(e, if reference { "reference" } else { "march" }, None);
        crate::insert(world, e, fog, if reference { 1.0 } else { num(vals, "vfog_resolution", 1.0) });
        if !reference && vals.get("vfog_dither").is_some_and(V::truthy) {
            world.entity_mut(e).insert(FogDither);
        }
    }
}

/// Whether RAEA_FOG_REFERENCE asks for the fog's ground truth instead of what the composition asks for.
pub fn fog_reference() -> bool {
    std::env::var("RAEA_FOG_REFERENCE").is_ok_and(|v| v == "1")
}

/// A camera whose volumetric fog is dithered (`<VolumetricFog dither>`).
#[derive(Component)]
pub struct FogDither;

/// The dither's seed for this frame: Bevy's `jitter`, which raea's shader reads as 1 + the film frame mod 64 (0 is no
/// dither). The film frame, not the engine's update count, so a frame rendered alone and the same frame reached in play
/// march the same samples.
pub fn sync_fog_dither(clock: Res<Clock>, mut q: Query<&mut bevy::light::VolumetricFog, With<FogDither>>, mut half: Query<&mut crate::HalfFog, With<FogDither>>) {
    let seed = 1.0 + clock.0.rem_euclid(64) as f32;
    for mut f in &mut q {
        if f.jitter != seed {
            f.jitter = seed;
        }
    }
    for mut f in &mut half {
        if f.fog.jitter != seed {
            f.fog.jitter = seed;
        }
    }
}
