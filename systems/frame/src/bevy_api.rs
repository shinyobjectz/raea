//! The few places where Bevy's render API reaches into raea-render's own passes, kept here so a Bevy release that moves them
//! changes this file and not each pass (docs/bevy-upgrade.md, "Keeping the next bump small").

use bevy::render::render_resource::TextureView;
use bevy::render::view::ViewDepthStencilTexture;

/// The view's depth as a shader binding: its depth-only view (Bevy 0.20 split a depth texture's views by aspect; the
/// main pass's depth is Depth32Float, depth only, so there's always one).
pub fn depth_binding(depth: &ViewDepthStencilTexture) -> &TextureView {
    depth.attachment.depth_stencil_views().depth_only_view().expect("the view's depth texture has a depth aspect")
}

/// The main texture's format on an Hdr camera (Bevy 0.19's ViewTarget::TEXTURE_FORMAT_HDR, gone in 0.20; still
/// Rgba16Float).
pub const HDR_FORMAT: bevy::render::render_resource::TextureFormat = bevy::render::render_resource::TextureFormat::Rgba16Float;

/// One of Bevy's embedded shaders replaced by raea's (`ours`, given Bevy's bytes), after checking Bevy's is still the
/// one raea's was taken from: its FNV-1a hash is `upstream_fnv`. A Bevy release that changes or moves the shader stops
/// here, loudly, rather than drawing with a stale copy; re-take raea's from the new one (docs/bevy-upgrade.md, "Forked
/// Bevy shaders") and record the new hash. Must run after the plugin that embeds it has been built. False when Bevy
/// hasn't embedded it (its plugin off on this platform); the caller decides whether that's allowed.
#[must_use]
pub fn swap_embedded_shader(app: &mut bevy::app::App, path: &str, upstream_fnv: u64, ours: impl FnOnce(&str) -> String) -> bool {
    let reg = app.world().resource::<bevy::asset::io::embedded::EmbeddedAssetRegistry>();
    let path = std::path::Path::new(path);
    let Some(bevy) = reg.remove_asset(path) else { return false };
    let found = fnv1a(bevy.value());
    assert!(
        found == upstream_fnv,
        "raea: Bevy's {} changed (hash {found:#x}, raea's fork was taken from {upstream_fnv:#x}); re-take the fork (docs/bevy-upgrade.md)",
        path.display()
    );
    let mine = ours(std::str::from_utf8(bevy.value()).expect("Bevy's shader is UTF-8"));
    reg.insert_asset(std::path::PathBuf::from(file!()), path, mine.into_bytes());
    SWAPPED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(path.to_string_lossy().into_owned());
    true
}

/// The embedded shaders swap_embedded_shader replaced with raea's forks: RAEA_ENGINE_SHADERS leaves them as they are,
/// since the source tree holds Bevy's original, not the fork (raea-sys-engine-shaders).
static SWAPPED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Whether `path` (as Bevy embeds it) is one of raea's forks of a Bevy shader.
pub fn swapped(path: &str) -> bool {
    SWAPPED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().any(|p| p == path)
}

pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// A camera drawn with no tone curve, as Bevy 0.19's `Tonemapping::None` drew it. 0.20 split that in two: `None` now
/// skips tonemapping, colour grading, exposure and dither on every camera, and `Linear` is an identity curve that keeps
/// them. 0.19's `None` tonemapped in the material shaders on a camera without Hdr (grading and dither applied) and
/// skipped the tonemapping pass on one with Hdr (neither applied), so a marked camera is kept on `Linear` without Hdr
/// and `None` with it, and draws as it did on 0.19. Hdr comes and goes with Bloom and the like, so it's re-checked
/// each frame.
#[derive(bevy::prelude::Component, Clone, Copy, Default)]
pub struct Untonemapped;

/// Bevy 0.19's defaults that 0.20 changed, put back so renders don't move: cameras drawn with no tone curve as 0.19
/// drew them (`Untonemapped`), and screen-space specular transmission on every 3D camera (0.20 made it opt-in; 0.19
/// required it on Camera3d, one step at medium quality, which is what a clear liquid's refraction draws with).
pub fn install_0_19_defaults(app: &mut bevy::app::App) {
    if !adopt_0_20("tonemap") {
        app.add_systems(bevy::app::PostUpdate, keep_untonemapped);
    }
    if !adopt_0_20("transmission") {
        app.register_required_components::<bevy::prelude::Camera3d, bevy::pbr::ScreenSpaceTransmission>();
    }
}

/// TEMPORARY (the adopt-0.20 pass, 2026-10-22): RAEA_ADOPT_020=tonemap,transmission,envfilter (or "all") drops that
/// pin-back so one binary renders Bevy 0.20's default beside 0.19's for Shane's before/after stills. Goes once he has
/// decided each: the pin-back is then either deleted or kept with its reason in docs/bevy-upgrade.md.
pub fn adopt_0_20(what: &str) -> bool {
    std::env::var("RAEA_ADOPT_020").is_ok_and(|v| v.split(',').any(|w| w == what || w == "all"))
}

fn keep_untonemapped(
    mut cams: bevy::prelude::Query<(&mut bevy::core_pipeline::tonemapping::Tonemapping, bevy::prelude::Has<bevy::camera::Hdr>), bevy::prelude::With<Untonemapped>>,
) {
    use bevy::core_pipeline::tonemapping::Tonemapping;
    for (mut t, hdr) in &mut cams {
        let want = if hdr { Tonemapping::None } else { Tonemapping::Linear };
        if *t != want {
            *t = want;
        }
    }
}
