//! The camera row's post-processing as Bevy's components on the camera: put on where the camera is made (`apply`), and
//! on any frame whose row changes them taken off and put on again (`sync`, docs/rerender.feature RR1), so a piece whose
//! shots each have their own sky, fog, grade or anti-aliasing renders as one composition, each frame as it would alone.

use std::collections::BTreeMap;

use bevy::prelude::*;
use raea_core_program::ir::V;
use raea_sys_frame::*;

mod on;
use raea_sys_compose::passes::PrepassRead;
use raea_sys_frame::camera::{Group, remove};

type Vals = BTreeMap<String, V>;

/// The camera row's post-processing by group (raea_sys_frame::camera::Group), in the order `apply` puts them on.
const GROUPS: [Group; 11] = [
    Group { keys: |k| k.starts_with("shadow_"), on: on::shadows, off: |_, _| {} },
    Group { keys: |k| k == "atmosphere" || k.starts_with("atmo_"), on: on::atmosphere, off: on::atmosphere_off },
    Group { keys: |k| k.starts_with("bloom"), on: on::bloom, off: remove::<bevy::post_process::bloom::Bloom> },
    Group { keys: |k| k.starts_with("fog_"), on: on::fog, off: remove::<DistanceFog> },
    Group { keys: |k| k == "tonemap", on: on::tonemap, off: on::tonemap_off },
    Group { keys: |k| k == "exposure", on: on::exposure, off: |w, e| { w.entity_mut(e).insert(bevy::camera::Exposure::default()); } },
    Group { keys: |k| k.starts_with("grade_"), on: on::grading, off: |w, e| { w.entity_mut(e).insert(bevy::render::view::ColorGrading::default()); } },
    Group { keys: |k| k.starts_with("vignette"), on: on::vignette, off: remove::<bevy::post_process::effect_stack::Vignette> },
    // the temporal filter goes with its jitter, which would otherwise keep moving the levels' dither with no filter
    Group { keys: |k| k.starts_with("ssao") || k == "aa" || k.starts_with("aa_"), on: on::aa, off: remove::<(bevy::pbr::ScreenSpaceAmbientOcclusion, bevy::anti_alias::smaa::Smaa, bevy::anti_alias::fxaa::Fxaa, crate::steady::Steady, bevy::render::camera::TemporalJitter)> },
    // depth of field: on with the frames that have one (their post set has dof_max), its aperture and focus following
    // the frame in sync_camera (RR1: it went on once, with the camera, so a play opening on a shot without it had none)
    Group { keys: |k| k == "dof_max" || k == "dof_mode", on: on::dof, off: remove::<bevy::post_process::dof::DepthOfField> },
    // put on once: their values follow the frame elsewhere (the fringes and the lens in sync_camera, the outline in
    // sync_frame_look)
    Group { keys: |_| false, on: on::once, off: |_, _| {} },
];

/// What each group last put on a camera (its columns as they were), the multisampling the camera was made with, which a
/// frame without AO or another anti-aliasing goes back to, and what the groups' components brought with them.
#[derive(Component)]
pub struct Applied {
    sets: Vec<Vals>,
    msaa: Msaa,
    brought: [bool; 3],
}

/// What Bevy puts on with a group's components (their required components: Bloom and the sky need Hdr, AO the depth
/// and normal prepasses) and never takes off. Brought by a group (the camera hadn't them before), each comes off with
/// the last component that needs it, so a frame after the night shot's bloom isn't drawn through HDR when it would
/// alone (RR0's noir after the night: SSIM 0.989).
fn brought(world: &World, e: Entity) -> [bool; 3] {
    use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
    let has = |f: fn(&World, Entity) -> bool| f(world, e);
    [has(|w, e| w.get::<bevy::camera::Hdr>(e).is_some()), has(|w, e| w.get::<DepthPrepass>(e).is_some()), has(|w, e| w.get::<NormalPrepass>(e).is_some())]
}

/// Each brought component the camera no longer needs taken off.
fn take_off_brought(world: &mut World, e: Entity, by: &mut [bool; 3]) {
    use bevy::core_pipeline::prepass::{DeferredPrepass, DepthPrepass, NormalPrepass};
    let w = world.entity(e);
    let ao = w.contains::<bevy::pbr::ScreenSpaceAmbientOcclusion>() || w.contains::<crate::outline::InkOutline>();
    // a pass after the camera reads its prepasses (--aov, RAEA_PASS): they stay
    let read = w.contains::<PrepassRead>();
    let need = [
        w.contains::<bevy::post_process::bloom::Bloom>() || w.contains::<bevy::pbr::AtmosphereSettings>() || w.contains::<DeferredPrepass>(),
        ao || read || w.contains::<bevy::post_process::motion_blur::MotionBlur>() || w.contains::<DeferredPrepass>(),
        ao || read,
    ];
    let mut em = world.entity_mut(e);
    for (i, b) in by.iter_mut().enumerate() {
        if *b && !need[i] {
            *b = false;
            match i {
                0 => { em.remove::<bevy::camera::Hdr>(); }
                1 => { em.remove::<DepthPrepass>(); }
                _ => { em.remove::<NormalPrepass>(); }
            }
        }
    }
}

fn sets(vals: &Vals, extra: &[Group]) -> Vec<Vals> {
    GROUPS.iter().chain(extra).map(|g| vals.iter().filter(|(k, _)| (g.keys)(k)).map(|(k, v)| (k.clone(), v.clone())).collect()).collect()
}

/// The camera row's post-processing put on the camera `e`, as `vals` has it (the camera's first row): GROUPS, then
/// `extra`, a system's own groups that this crate can't name (raea-sys-fog-half's VFOG, which raea-runtime-scene3d gives).
pub fn apply(world: &mut World, e: Entity, vals: &Vals, extra: &[Group]) {
    let had = brought(world, e);
    for g in GROUPS.iter().chain(extra) {
        (g.on)(world, e, vals);
    }
    let now = brought(world, e);
    let msaa = world.get::<Msaa>(e).copied().unwrap_or_default();
    world.entity_mut(e).insert(Applied { sets: sets(vals, extra), msaa, brought: [0, 1, 2].map(|i| now[i] && !had[i]) });
}

/// Each camera's post-processing as this frame's row has it: a group whose columns changed since the camera last took
/// them is taken off and put on again (a frame's own sky, fog or anti-aliasing: frame_sets, raea-sys-frame tables.rs).
/// Multisampling goes off with AO or SMAA/FXAA and back to the camera's own without, except on a deferred camera (Solari),
/// which can't multisample, and on one whose prepasses a pass reads (PrepassRead: --aov, RAEA_PASS), which stays off
/// (#159). `extra` as `apply` had it.
pub fn sync(world: &mut World, extra: &[Group]) {
    let groups: Vec<&Group> = GROUPS.iter().chain(extra).collect();
    let cams: Vec<Entity> = world.query_filtered::<Entity, With<Applied>>().iter(world).collect();
    for e in cams {
        let frame = world.resource::<FrameRows>();
        if frame.still(e) {
            continue;
        }
        let Some(vals) = frame.vals(e).map(std::borrow::Cow::into_owned) else { continue };
        let sets_now = sets(&vals, extra);
        let Some(was) = world.get::<Applied>(e) else { continue };
        let changed: Vec<usize> = (0..groups.len()).filter(|&i| sets_now[i] != was.sets[i]).collect();
        if changed.is_empty() {
            continue;
        }
        let (own, mut by) = (was.msaa, was.brought);
        let had = brought(world, e);
        for &i in &changed {
            (groups[i].off)(world, e);
            (groups[i].on)(world, e, &vals);
        }
        let now = brought(world, e);
        for i in 0..3 {
            by[i] |= now[i] && !had[i];
        }
        take_off_brought(world, e, &mut by);
        let single = vals.contains_key("ssao") || vals.contains_key("aa") || world.get::<PrepassRead>(e).is_some();
        let want = if single { Msaa::Off } else { own };
        let deferred = world.get::<bevy::core_pipeline::prepass::DeferredPrepass>(e).is_some();
        if world.get::<Msaa>(e) != Some(&want) && (want == Msaa::Off || !deferred) {
            world.entity_mut(e).insert(want);
        }
        if let Some(mut a) = world.get_mut::<Applied>(e) {
            (a.sets, a.brought) = (sets_now, by);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world with one camera whose row is a live row (`rows`' first), as raea-render's FrameRows lends it.
    fn world(row: Vals) -> (World, Entity) {
        let mut w = World::new();
        w.init_resource::<Assets<bevy::light::atmosphere::ScatteringMedium>>();
        let e = w.spawn((Camera3d::default(), Msaa::Sample4)).id();
        let mut frame = FrameRows::new([(e, 0)].into_iter().collect());
        frame.live = Some(vec![row.clone()]);
        w.insert_resource(frame);
        apply(&mut w, e, &row, &[]);
        (w, e)
    }

    fn set(w: &mut World, row: Vals) {
        w.resource_mut::<FrameRows>().live = Some(vec![row]);
    }

    fn row(cols: &[(&str, V)]) -> Vals {
        cols.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn a_frame_puts_on_and_takes_off_its_own_effects() {
        let (mut w, e) = world(row(&[("bloom", V::N(0.05)), ("tonemap", V::S("agx".into()))]));
        assert!(w.get::<bevy::post_process::bloom::Bloom>(e).is_some());
        // the next shot: fog and AO, no bloom, Bevy's tone mapping
        set(&mut w, row(&[("fog_color", V::S("#c9ced4".into())), ("fog_start", V::N(10.0)), ("ssao", V::S("ultra".into())), ("aa", V::S("smaa".into()))]));
        sync(&mut w, &[]);
        assert!(w.get::<bevy::post_process::bloom::Bloom>(e).is_none());
        assert!(w.get::<DistanceFog>(e).is_some() && w.get::<bevy::pbr::ScreenSpaceAmbientOcclusion>(e).is_some());
        assert_eq!(w.get::<Msaa>(e), Some(&Msaa::Off));
        // the bloom's Hdr went with it; AO brought its prepasses
        assert!(w.get::<bevy::camera::Hdr>(e).is_none() && w.get::<bevy::core_pipeline::prepass::NormalPrepass>(e).is_some());
        assert_eq!(w.get::<bevy::core_pipeline::tonemapping::Tonemapping>(e), Some(&bevy::core_pipeline::tonemapping::Tonemapping::default()));
        // a sky shot: the atmosphere on, the fog and AO off, multisampling back
        set(&mut w, row(&[("atmosphere", V::N(1.0)), ("atmo_light", V::N(1.0))]));
        sync(&mut w, &[]);
        assert!(w.get::<DistanceFog>(e).is_none() && w.get::<bevy::pbr::ScreenSpaceAmbientOcclusion>(e).is_none() && w.get::<bevy::anti_alias::smaa::Smaa>(e).is_none());
        assert_eq!(w.get::<Msaa>(e), Some(&Msaa::Sample4));
        assert!(w.get::<bevy::pbr::AtmosphereSettings>(e).is_some() && w.get::<bevy::camera::Hdr>(e).is_some());
        assert!(w.get::<bevy::core_pipeline::prepass::DepthPrepass>(e).is_none() && w.get::<bevy::core_pipeline::prepass::NormalPrepass>(e).is_none());
        assert_eq!(w.query::<&bevy::light::Atmosphere>().iter(&w).count(), 1);
        // and off again: the sky's own entity goes with it
        set(&mut w, row(&[]));
        sync(&mut w, &[]);
        assert!(w.get::<bevy::pbr::AtmosphereSettings>(e).is_none() && w.get::<bevy::light::AtmosphereEnvironmentMapLight>(e).is_none());
        assert_eq!(w.query::<&bevy::light::Atmosphere>().iter(&w).count(), 0);
        assert!(w.get::<bevy::camera::Hdr>(e).is_none());
    }

    #[test]
    fn what_the_camera_had_before_stays() {
        // a camera made HDR (its own, not a group's) keeps it when the bloom goes
        let mut w = World::new();
        w.init_resource::<Assets<bevy::light::atmosphere::ScatteringMedium>>();
        let e = w.spawn((Camera3d::default(), bevy::camera::Hdr)).id();
        let mut frame = FrameRows::new([(e, 0)].into_iter().collect());
        frame.live = Some(vec![row(&[("bloom", V::N(0.05))])]);
        w.insert_resource(frame);
        apply(&mut w, e, &row(&[("bloom", V::N(0.05))]), &[]);
        set(&mut w, row(&[]));
        sync(&mut w, &[]);
        assert!(w.get::<bevy::post_process::bloom::Bloom>(e).is_none() && w.get::<bevy::camera::Hdr>(e).is_some());
    }

    #[test]
    fn an_unchanged_frame_puts_nothing_on_again() {
        let (mut w, e) = world(row(&[("bloom", V::N(0.05))]));
        w.get_mut::<bevy::post_process::bloom::Bloom>(e).unwrap().intensity = 0.5;
        // a frame whose other columns change (the camera moves) leaves the bloom as it is
        set(&mut w, row(&[("bloom", V::N(0.05)), ("x", V::N(3.0))]));
        sync(&mut w, &[]);
        assert_eq!(w.get::<bevy::post_process::bloom::Bloom>(e).unwrap().intensity, 0.5);
    }

    #[test]
    fn a_camera_whose_prepasses_are_read_stays_single_sampled() {
        use bevy::core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass};
        // made multisampled with AO on its first shot (AO brought the depth prepass), then given its prepasses as
        // --aov gives them (#159)
        let (mut w, e) = world(row(&[("bloom", V::N(0.1)), ("ssao", V::S("ultra".into()))]));
        w.entity_mut(e).insert((DepthPrepass, MotionVectorPrepass, Msaa::Off, PrepassRead));
        // the AO goes and the bloom ramps: still single-sampled, and the depth prepass stays
        for b in [0.2, 0.3] {
            set(&mut w, row(&[("bloom", V::N(b))]));
            sync(&mut w, &[]);
            assert_eq!(w.get::<Msaa>(e), Some(&Msaa::Off), "bloom {b}");
            assert!(w.get::<DepthPrepass>(e).is_some() && w.get::<MotionVectorPrepass>(e).is_some(), "bloom {b}");
        }
        // a live scene's frame with 3D in it leaves it single-sampled too
        crate::msaa_for_3d(&mut w, true);
        assert_eq!(w.get::<Msaa>(e), Some(&Msaa::Off));
        // a camera nothing reads the prepasses of still multisamples again (#38)
        w.entity_mut(e).remove::<PrepassRead>();
        crate::msaa_for_3d(&mut w, true);
        assert_eq!(w.get::<Msaa>(e), Some(&Msaa::Sample4));
    }

    #[test]
    fn depth_of_field_comes_with_the_shot_that_has_it() {
        use bevy::post_process::dof::DepthOfField;
        // a play opening on a shot without it, then one with it, then one without (RR0's film: shots 3 and 6)
        let (mut w, e) = world(row(&[("bloom", V::N(0.05))]));
        assert!(w.get::<DepthOfField>(e).is_none());
        set(&mut w, row(&[("dof_aperture", V::N(1.8)), ("dof_max", V::N(64.0)), ("focus", V::N(3.0))]));
        sync(&mut w, &[]);
        let d = w.get::<DepthOfField>(e).expect("on with the shot");
        assert_eq!((d.aperture_f_stops, d.focal_distance), (1.8, 3.0));
        // its aperture changing alone doesn't put it on again (sync_camera follows it)
        w.get_mut::<DepthOfField>(e).unwrap().focal_distance = 7.0;
        set(&mut w, row(&[("dof_aperture", V::N(2.8)), ("dof_max", V::N(64.0)), ("focus", V::N(3.0))]));
        sync(&mut w, &[]);
        assert_eq!(w.get::<DepthOfField>(e).unwrap().focal_distance, 7.0);
        // a play's merged row keeps the last aperture; dof_max gone takes it off
        set(&mut w, row(&[("dof_aperture", V::N(2.8))]));
        sync(&mut w, &[]);
        assert!(w.get::<DepthOfField>(e).is_none());
    }
}
