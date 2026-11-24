//! <Pathtracer>'s settings beside Bevy's Pathtracer (docs/nobuild.feature NB1a): how many times a render draws each
//! frame, each draw one more sample a pixel (`samples`), and whether the traced air walks the fog volumes (`media`).
//! Here, not in raea-render-raytrace, because raea-sys-fog reads `media` and a system can't use the runtime layer.
//! RAEA_LIGHT_SPP and RAEA_PT_MEDIA=0 override them on every camera, said once.
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;

#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
#[reflect(Component, Default)]
pub struct PathtracerSettings {
    pub samples: u32,
    pub media: bool,
}

impl Default for PathtracerSettings {
    fn default() -> Self {
        Self { samples: 1, media: true }
    }
}

fn say(var: &str, v: &str, prop: &str) {
    static SAID: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
    if !said.iter().any(|s| s == var) {
        said.push(var.into());
        eprintln!("raea: <Pathtracer> on every camera: {var}={v} over {prop} (docs/nobuild.feature NB1a)");
    }
}

/// The draws a render takes of each frame: RAEA_LIGHT_SPP, else the most any 3D camera's <Pathtracer samples> asks
/// for (1 with none).
pub fn samples(world: &mut World) -> u32 {
    if let Some(n) = std::env::var("RAEA_LIGHT_SPP").ok().and_then(|v| v.parse::<u32>().ok()).filter(|n| *n >= 1) {
        say("RAEA_LIGHT_SPP", &n.to_string(), "samples");
        return n;
    }
    let mut q = world.query_filtered::<&PathtracerSettings, With<Camera3d>>();
    q.iter(world).map(|s| s.samples).max().unwrap_or(1).max(1)
}

/// Whether the path tracer walks the fog volumes: not with RAEA_PT_MEDIA=0, nor where a view's <Pathtracer media> is
/// false (the volumes are one list for every view).
pub fn media<'a>(views: impl IntoIterator<Item = &'a PathtracerSettings>) -> bool {
    if std::env::var("RAEA_PT_MEDIA").is_ok_and(|v| v == "0") {
        say("RAEA_PT_MEDIA", "0", "media");
        return false;
    }
    views.into_iter().all(|s| s.media)
}
