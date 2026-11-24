//! raea's Engine as Bevy plugins (docs/sdk.feature S6a). Each module that brings materials, passes or render-world
//! systems is a Plugin, and RaeaPlugins adds them in Engine::new's order, so any Bevy App can draw raea's looks beside
//! its own: `app.add_plugins((DefaultPlugins, RaeaPlugins))`, or `RaeaPlugins.build().disable::<LookPlugin>()` as with
//! Bevy's own groups. A plugin is the module's `install`, unchanged, so Engine::new and this group add the same things.
//!
//! RegisteredPlugins adds a project's Bevy plugins (package.json raea.bevy.plugins) and the clock they step by.
//!
//! Scene3dPlugin moves the 3D by the frame's rows (meshes, models, camera, lights, looks) each Update, as Engine::new
//! does: Engine::new runs the same systems, scene3d_systems(), inside its frame's chain between its own marks.
use bevy::app::{PluginGroup, PluginGroupBuilder};
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
// the frame's system sets, raea-sys-frame's: a plugin orders its systems by them
pub use raea_sys_frame::{FrameSystems, Scene3dSystems};

macro_rules! raea_plugin {
    ($($name:ident: $install:path, $doc:literal;)*) => {
        $(
            #[doc = $doc]
            #[derive(Default)]
            pub struct $name;
            impl Plugin for $name {
                fn build(&self, app: &mut App) {
                    $install(app);
                }
            }
        )*
        /// Every raea plugin, in the order Engine::new installs them.
        pub struct RaeaPlugins;
        impl PluginGroup for RaeaPlugins {
            fn build(self) -> PluginGroupBuilder {
                PluginGroupBuilder::start::<Self>().add(FramePlugin)$(.add($name))*
            }
        }
    };
}

raea_plugin! {
    VolumetricFogPlugin: super::scene3d::install_fog_shader, "Volumetric fog's shader, its dither seeded from the film frame (scene3d.rs).";
    DepthOfFieldPlugin: super::dof::install, "Bevy's depth of field, keeping the frame's alpha (dof.rs).";
    EffectStackPlugin: super::effect_stack::install, "The effect stack (a <Grade>'s aberration and vignette), keeping alpha (effect_stack.rs).";
    Bevy019DefaultsPlugin: super::bevy_api::install_0_19_defaults, "Bevy 0.19's tone curve \"none\" and screen-space transmission, which 0.20 changed (bevy_api.rs).";
    HalfFogPlugin: super::fog_half::install, "Volumetric fog at a fraction of the frame's resolution, upsampled by depth (fog_half.rs).";
    EnvironmentPlugin: super::environment::install, "Each panorama's light filtered once (environment.rs).";
    FroxelFogPlugin: super::froxel::install, "Fog as one medium in a grid along the view (froxel.rs).";
    FroxelGlassPlugin: super::froxel_glass::install, "Glass seen through froxel fog (froxel_glass.rs).";
    LookPlugin: super::look::install, "Models drawn as pixel art or the site's grain (look.rs).";
    OutlinePlugin: super::outline::install, "Ink outlines (outline.rs).";
    SteadyPlugin: raea_sys_post::steady::install, "raea's temporal filter: each frame jittered and blended with those before it (raea-sys-post steady.rs, docs/realism.feature LT12).";
    ComposePlugin: super::compose::install, "Composition passes: each object's pass mask, read by Look, Outline, Bloom and Grade (compose.rs).";
    Text3dPlugin: super::text3d::install, "Text in the 3D world: bevy_rich_text3d and its faces (raea-sys-text3d).";
    FirePlugin: super::fire::install, "Flames drawn as volumes (fire.rs).";
    SplatPlugin: super::splat::install, "Gaussian splats, sorted per frame for the scene camera (raea-sys-splat).";
    MeshSequencePlugin: super::meshseq::install, "Mesh sequences by the frame, glitches on parts (meshseq.rs).";
    Draws2dPlugin: raea_sys_sprite::install, "Plugins' 2D drawing brings up the scene's 2D camera (raea-sys-sprite's Draws2d).";
    ScreenPlugin: super::screen::install, "Live pictures on surfaces, drawn as CRTs (screen.rs).";
    GridPlugin: super::grid::install, "Greybox surfaces: graph paper and outlines (grid.rs).";
    ProcMatPlugin: raea_sys_procmat::install, "Procedural surfaces: wood, stone, brick, tiles and the rest, with dust and dirt (systems/procmat).";
    PaintPlugin: super::paint::install, "The paint and cartoon looks' surfaces (paint.rs).";
    PassesPlugin: super::passes::install, "Render passes per view (passes.rs).";
    TracedAoPlugin: raea_sys_rt::passes, "Traced passes: ambient occlusion, raea GI's world probes and the sun's far shadow map (raea-sys-rt).";
    GiScreenPlugin: raea_sys_gi::install, "raea GI's light on screen: each pixel's indirect diffuse from the world cache, read by the main pass (raea-sys-gi, RAEA_GI=1).";
    VolumePlugin: super::volume::install, "Simulated smoke, clouds and contrails, raymarched (volume.rs).";
    GrassPlugin: raea_sys_grass::install, "Fields of grass, made in tiles as detailed as the camera's distance asks (systems/grass).";
    DetailPlugin: raea_sys_lod::install, "Where each part is drawn: levels by distance or size on screen, cross-faded, draw distance and shadows left to a coarser level (raea-sys-lod, docs/realism.feature LT21).";
    ShadowOnlyPlugin: super::shadow_only::install, "Meshes in the shadow maps only (shadow_only.rs).";
    ShadowCatchPlugin: super::shadow_catch::install, "Surfaces that show only the shadows falling on them (shadow_catch.rs).";
    TintPlugin: raea_sys_material::tint::install, "A colour a copy over an <Instances>' surface, three's instanceColor (tint.rs).";
    ToonPlugin: super::toon::install, "The toon surface and its ink outline's materials (toon.rs).";
    ShaderExtPlugin: super::shader_ext::install, "Shaders written in TypeScript over the lit standard surface (raea-sys-shader).";
    ThreeShaderPlugin: raea_sys_three_shader::install, "three.js's shaders: a ShaderMaterial's GLSL (naga) and a node material's WGSL, with three's built-ins (raea-sys-three-shader).";
    FramePassPlugin: raea_sys_framepass::install, "Full-screen passes written in TypeScript (raea-sys-framepass).";
    ComputePlugin: raea_sys_compute::install, "Compute shaders written in TypeScript, run before the cameras draw (raea-sys-compute).";
    PhaseItemPlugin: raea_sys_phase::install, "Your own draws in a camera's opaque phase, as Bevy's custom_phase_item queues one (raea-sys-phase).";
    LensPlugin: super::lens::install, "The camera's lens (Fisheye, LensDistortion), after tone mapping (lens.rs).";
    ParityPlugin: raea_sys_parity::install, "The parity probe: a frame's cost and World, written when RAEA_PARITY is set (raea-sys-parity, docs/control.feature FC0a).";
    CameraCutPlugin: super::cut::install, "A camera that jumps is a cut: its history starts again and its motion blur waits (cut.rs, RT31).";
    MotionBlurPlugin: super::motion_blur::install, "Motion blur's frame counter pinned to the frame (motion_blur.rs).";
}

/// The frame's state that raea's plugins' systems read (the clock, the frame's rows, the loaded edit, the error), empty
/// until something loads an edit: first in RaeaPlugins, so a plain Bevy App that adds the group runs it with nothing
/// of raea's drawn. Engine::new inserts its own over these when it loads.
pub struct FramePlugin;

impl Plugin for FramePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<super::Clock>().init_resource::<super::Error>().init_resource::<super::FrameRows>();
        if !app.world().contains_resource::<super::Edit>() {
            let prog = raea_core_program::ir::Program::default();
            let timeline = raea_core_program::timeline::Timeline::new(&prog, &[]).expect("an empty timeline");
            app.insert_resource(super::Edit { prog, timeline: std::sync::Arc::new(timeline), media: Vec::new() });
        }
    }
}

/// The registered Bevy plugins (plugins.rs), stepping by a clock at `fps` frames a second. Nothing when none are.
pub struct RegisteredPlugins {
    pub fps: f64,
}

impl Plugin for RegisteredPlugins {
    fn build(&self, app: &mut App) {
        super::plugins::install(app, self.fps);
    }
}

/// The systems that move the 3D by the frame's rows, in order: models set up, toon materials over a model's own, then
/// meshes, camera, lights, fog, models, bones, looks, paint, decals and 3D text.
pub fn scene3d_systems() -> ScheduleConfigs<ScheduleSystem> {
    (
        // 2D over the 3D: its camera once there's a part, then each part's place, cell and colour (raea-sys-sprite)
        (super::sprite2d::camera_2d, super::sprite2d::sync_2d).chain(),
        super::scene3d::model_setup,
        super::scene3d::instance_models,
        super::scene3d::model_shadow_markers,
        super::scene3d::toon_models,
        ((super::scene3d::sync_meshes, super::scene3d::sync_instances, super::scene3d::sync_pictures, raea_sys_rendermap::sync_render_maps, super::shader_ext::sync_shaded, raea_sys_sampler::arrays::sync_arrays, raea_sys_sampler::sync_samplers).chain(), super::toon::sync_toon, (super::scene3d::sync_camera, super::scene3d::sync_post).chain(), super::motion_blur::sync_motion_blur, (super::environment::sync_environment, super::environment::sync_prefiltered, super::environment::sync_suns).chain(), (super::scene3d::sync_light_sets, super::scene3d::sync_lights).chain(), super::scene3d::sync_fog_volumes, super::scene3d::sync_fog_dither, super::scene3d::sync_models, super::scene3d::sync_morphs, super::scene3d::sync_attach, super::grid::grid_models, super::scene3d::sync_bones, super::look::sync_frame_look, super::look::sync_looks, super::compose::sync_passes, super::look::sync_flat, super::paint::sync_paint, super::decal::sync_decals, super::text3d::sync_texts).chain(),
        // --clay and --review's clay half: every standard surface grey, after the rows set their colours
        super::scene3d::clay::clay_materials,
    )
        .chain()
}

/// The 3D moved by the frame's rows each Update (scene3d_systems), in the Scene3dSystems set.
pub struct Scene3dPlugin;

impl Plugin for Scene3dPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, scene3d_systems().in_set(Scene3dSystems));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_group_holds_every_plugin_and_disables_one_as_bevys_do() {
        let all = RaeaPlugins.build();
        assert!(all.contains::<VolumetricFogPlugin>() && all.contains::<LookPlugin>() && all.contains::<MotionBlurPlugin>());
        let off = RaeaPlugins.build().disable::<LookPlugin>();
        assert!(off.contains::<LookPlugin>() && !off.enabled::<LookPlugin>() && off.enabled::<ToonPlugin>());
    }

    #[test]
    fn the_frame_plugin_gives_a_plain_app_the_state_raeas_systems_read() {
        let mut app = App::new();
        app.add_plugins(FramePlugin);
        let w = app.world();
        assert!(w.contains_resource::<super::super::Clock>() && w.contains_resource::<super::super::Error>());
        assert!(w.contains_resource::<super::super::FrameRows>() && w.contains_resource::<super::super::Edit>());
        assert!(RaeaPlugins.build().enabled::<FramePlugin>());
    }

    #[test]
    fn scene3d_plugin_adds_its_systems_to_update_in_its_set() {
        let mut app = App::new();
        app.add_plugins(Scene3dPlugin);
        let update = app.get_schedule(Update).expect("Update");
        assert!(update.systems_len() >= 21);
        assert!(update.graph().system_sets.contains(Scene3dSystems));
    }
}
