//! raea-render's ray tracing (docs/raytracing.feature): Bevy's Solari for a scene whose camera asks for it (RT3,
//! solari.rs), MetalFX's temporal denoiser on its lighting on the Mac (RT4, metalfx.rs), and its temporal scaler for a
//! view drawn below its size (RT9, upscale.rs). raea-render brings this
//! crate in under its `raytracing` feature and adds it to an engine only when the program asks.

use bevy::prelude::*;
use std::collections::HashMap;

mod solari;
#[cfg(target_os = "macos")]
mod metalfx;
#[cfg(target_os = "macos")]
mod emit_taa;
#[cfg(target_os = "macos")]
mod upscale;
// RAEA_LIGHT: one light path for every camera (docs/realism.feature LT1)
pub mod force;
// SolariSettings' environment overrides (docs/nobuild.feature NB1a)
mod settings;

/// RT9: the fraction of the view's size a side this camera's main pass draws at (a third to 1), which MetalFX's
/// temporal scaler brings up to the whole view after it (upscale.rs); 1 draws at the full size. <SolariLighting scale>
/// sets it. On the Mac only: elsewhere the view draws at its full size and raea says so once.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct RenderScale {
    pub scale: f32,
}

impl Default for RenderScale {
    fn default() -> Self {
        RenderScale { scale: 1.0 }
    }
}

/// A Solari camera's settings for the denoiser, its guides, the emission's history and the jitter (docs/nobuild.feature
/// NB1a): what <SolariLighting>'s props set in TypeScript, through <BevyComponent> by this type's path, so a value is
/// tried in a composition, never a build. Each field's environment variable overrides it for every camera, as a
/// developer's A/B (settings.rs); a camera without it has the defaults. Read on the view, each frame.
#[derive(Component, Reflect, Clone, Debug, PartialEq, bevy::render::extract_component::ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
#[reflect(Component, Default)]
pub struct SolariSettings {
    /// MetalFX's denoiser on the lighting (RT4); off shows Solari's noise (RAEA_SOLARI_DENOISE=0)
    pub denoise: bool,
    /// the denoiser guided by Solari's own guides, a mirror's pixel taking what it reflects (RT17); off, by the G-buffer
    /// (RAEA_SOLARI_PSR=0)
    pub guides: bool,
    /// a glossy pixel as far off as what it reflects in the denoiser's depth, so its reflection doesn't trail (RT34;
    /// RAEA_SOLARI_PSR_DEPTH=0)
    pub reflection_depth: bool,
    /// an emissive surface's own light: "taa", taken out before the denoiser and gathered over frames (RT11, RT32);
    /// "split", each frame's alone; "mask", left in for the denoiser (RAEA_SOLARI_EMISSIVE)
    pub emissive: String,
    /// a view drawn small: denoised and brought up in "one" pass, or denoised small and scaled in a second, "two"
    /// (RAEA_SOLARI_UPSCALE)
    pub upscale: String,
    /// what MetalFX divides the colour by before it denoises and multiplies it by after: bright light kept, at the cost
    /// of fireflies past 16 (metalfx.rs pre_exposure; RAEA_METALFX_PREEXPOSURE)
    pub preexposure: f32,
    /// a full-size view jittered too, so the denoiser anti-aliases its edges as it gathers frames (RAEA_SOLARI_AA=0)
    pub aa: bool,
    /// which of MetalFX's passes is told the jitter: "" the denoiser, "scaler", "both", "flip", "0" none (and none is
    /// made) (RAEA_SOLARI_JITTER)
    pub jitter: String,
    /// how fast a sample's weight falls with its distance from a pixel's centre in the emission's history, exp(-k d^2)
    /// in output pixels (RAEA_EMIT_TAA_SHARP): at Blackman-Harris's 2.29 the forest's canopy came out 1.28 times the
    /// tracer's as seen (tone mapped), its sky through the twigs spread wider than a pixel, and a screen's detail 0.38
    /// of the tracer's
    pub emissive_sharpness: f32,
    /// the jitter's cycle at the full size, in frames: a view drawn at s of its size a side cycles through
    /// `jitter_frames / s²` offsets (DLSS's 8), and emit_taa lets a twig glimpsed through it stay away that long
    pub jitter_frames: f32,
    /// the traced scene follows what moves: a skinned mesh's traced twin posed each frame (RT5, a trial;
    /// RAEA_SOLARI_DYNAMIC). A hidden part leaves the traced scene with or without it (SG9)
    pub dynamic: bool,
}

impl Default for SolariSettings {
    fn default() -> Self {
        SolariSettings {
            denoise: true,
            guides: true,
            reflection_depth: true,
            emissive: "taa".into(),
            upscale: "one".into(),
            preexposure: 16.0,
            aa: true,
            jitter: String::new(),
            emissive_sharpness: 6.0,
            jitter_frames: 8.0,
            dynamic: false,
        }
    }
}

/// Solari's plugins, and RAEA_LIGHT's camera override when it names a path.
pub fn add(app: &mut App) {
    app.register_type::<RenderScale>().register_type::<SolariSettings>().register_type::<raea_sys_rt::pathtracer::PathtracerSettings>();
    app.add_plugins(bevy::render::extract_component::ExtractComponentPlugin::<SolariSettings>::default());
    app.add_plugins(bevy::render::extract_component::ExtractComponentPlugin::<raea_sys_rt::pathtracer::PathtracerSettings>::default());
    // the denoiser's own shaders, read from a project's engine-shaders/ or the checkout as its pipelines are made
    #[cfg(target_os = "macos")]
    raea_sys_engine_shaders::declare(&[("raea_render_raytrace/metalfx_guides.wgsl", metalfx::DECODE_WGSL), ("raea_render_raytrace/emit_taa.wgsl", emit_taa::WGSL)]);
    solari::add(app);
    force::add(app);
}

/// The program asks for Solari (a camera's SolariLighting or Pathtracer component; `prog` is raea-render's Program),
/// or RAEA_LIGHT forces it.
pub fn wanted(prog: &impl std::fmt::Debug) -> bool {
    force::needs_solari() || format!("{prog:?}").contains("bevy_solari::")
}

/// After app.finish(): MetalFX's denoiser after Solari's lighting and its scaler after the main pass, on the Mac;
/// elsewhere a RenderScale below 1 is said once to be ignored.
pub fn add_denoiser(app: &mut App) {
    #[cfg(target_os = "macos")]
    {
        metalfx::add(app);
        upscale::add(app);
    }
    #[cfg(not(target_os = "macos"))]
    app.add_systems(PostUpdate, |q: Query<&RenderScale>, mut said: Local<bool>| {
        if !*said && q.iter().any(|r| r.scale < 0.999) {
            *said = true;
            eprintln!("raea: <SolariLighting scale>: no scaler off the Mac; the view draws at its full size (docs/raytracing.feature RT9)");
        }
    });
}

/// The path tracer starts its accumulation again on the next draw (a reference frame of its own, RAEA_LIGHT_SPP,
/// docs/realism.feature LT1; it starts again by itself only when the camera moves).
pub fn restart_pathtracer(world: &mut World) {
    for mut p in world.query::<&mut bevy::solari::pathtracer::Pathtracer>().iter_mut(world) {
        p.reset = true;
    }
}

/// A cut to another camera (docs/rerender.feature RR2): Solari's and the path tracer's history start again, as on their
/// first frame, so the last shot's light isn't carried into this one (Bevy's `reset`, for "sudden camera cuts").
pub fn cut(world: &mut World) {
    for mut s in world.query::<&mut bevy::solari::prelude::SolariLighting>().iter_mut(world) {
        s.reset = true;
    }
    for mut p in world.query::<&mut bevy::solari::pathtracer::Pathtracer>().iter_mut(world) {
        p.reset = true;
    }
}
