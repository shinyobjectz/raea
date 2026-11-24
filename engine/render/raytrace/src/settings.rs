//! SolariSettings' environment overrides (docs/nobuild.feature NB1a): each field's variable, read once, over every
//! camera's value, so an A/B on any scene needs neither a build nor an edit. A camera without SolariSettings has the
//! defaults, under the same overrides.
use super::SolariSettings;
use bevy::prelude::Entity;
use std::collections::HashMap;

#[derive(Default)]
struct Overrides {
    denoise: Option<bool>,
    guides: Option<bool>,
    reflection_depth: Option<bool>,
    emissive: Option<String>,
    upscale: Option<String>,
    preexposure: Option<f32>,
    aa: Option<bool>,
    jitter: Option<String>,
    emissive_sharpness: Option<f32>,
    dynamic: Option<bool>,
}

fn overrides() -> &'static Overrides {
    static O: std::sync::OnceLock<Overrides> = std::sync::OnceLock::new();
    O.get_or_init(|| {
        let var = |n: &str| std::env::var(n).ok();
        // "0" turns a switch off and anything else on, as each variable did on its own
        let switch = |n: &str| var(n).map(|v| v != "0");
        let num = |n: &str| var(n).and_then(|v| v.parse::<f32>().ok()).filter(|v| (f32::MIN_POSITIVE..f32::INFINITY).contains(v));
        Overrides {
            denoise: switch("RAEA_SOLARI_DENOISE"),
            guides: switch("RAEA_SOLARI_PSR"),
            reflection_depth: switch("RAEA_SOLARI_PSR_DEPTH"),
            emissive: var("RAEA_SOLARI_EMISSIVE").filter(|v| matches!(v.as_str(), "taa" | "split" | "mask")),
            upscale: var("RAEA_SOLARI_UPSCALE").filter(|v| matches!(v.as_str(), "one" | "two")),
            preexposure: num("RAEA_METALFX_PREEXPOSURE"),
            aa: switch("RAEA_SOLARI_AA"),
            jitter: var("RAEA_SOLARI_JITTER"),
            emissive_sharpness: num("RAEA_EMIT_TAA_SHARP"),
            dynamic: switch("RAEA_SOLARI_DYNAMIC"),
        }
    })
}

/// A view's settings as they apply: its own (or the defaults), each overridden by its variable when that is set.
pub(crate) fn of(s: Option<&SolariSettings>) -> SolariSettings {
    let o = overrides();
    let mut s = s.cloned().unwrap_or_default();
    if let Some(v) = o.denoise { s.denoise = v; }
    if let Some(v) = o.guides { s.guides = v; }
    if let Some(v) = o.reflection_depth { s.reflection_depth = v; }
    if let Some(v) = &o.emissive { s.emissive = v.clone(); }
    if let Some(v) = &o.upscale { s.upscale = v.clone(); }
    if let Some(v) = o.preexposure { s.preexposure = v; }
    if let Some(v) = o.aa { s.aa = v; }
    if let Some(v) = &o.jitter { s.jitter = v.clone(); }
    if let Some(v) = o.emissive_sharpness { s.emissive_sharpness = v; }
    if let Some(v) = o.dynamic { s.dynamic = v; }
    s
}

/// The variables that override a prop, each with the prop.
const VARS: &[(&str, &str)] = &[
        ("RAEA_SOLARI_DENOISE", "denoise"),
        ("RAEA_SOLARI_PSR", "guides"),
        ("RAEA_SOLARI_PSR_DEPTH", "reflectionDepth"),
        ("RAEA_SOLARI_EMISSIVE", "emissive"),
        ("RAEA_SOLARI_UPSCALE", "upscale"),
        ("RAEA_METALFX_PREEXPOSURE", "preexposure"),
        ("RAEA_SOLARI_AA", "aa"),
        ("RAEA_SOLARI_JITTER", "jitter"),
        ("RAEA_SOLARI_DYNAMIC", "dynamic"),
        ("RAEA_EMIT_TAA_SHARP", "emissiveSharpness"),
        ("RAEA_SOLARI_SCALE", "scale"),
];

/// A view's settings as they apply, into the run's trace (`solari_settings`: the view, each field, and the variables
/// that overrode a prop) the first frame and whenever they change, so a render says what it drew with; and once a run,
/// on stderr, which variables override <SolariLighting>'s props.
/// `scale`: the fraction of the view's width its main pass draws (RenderScale, or RAEA_SOLARI_SCALE's).
pub(crate) fn record(view: Entity, s: &SolariSettings, scale: f32, seen: &mut HashMap<Entity, SolariSettings>) {
    if seen.get(&view) == Some(s) {
        return;
    }
    seen.insert(view, s.clone());
    let env = raea_io_trace::overrides("SolariLighting", VARS);
    raea_io_trace::event(
        "solari_settings",
        serde_json::json!({
            "view": view.to_bits(),
            "denoise": s.denoise, "guides": s.guides, "reflection_depth": s.reflection_depth, "emissive": s.emissive,
            "upscale": s.upscale, "preexposure": s.preexposure, "aa": s.aa, "jitter": s.jitter,
            "emissive_sharpness": s.emissive_sharpness, "jitter_frames": s.jitter_frames, "dynamic": s.dynamic, "scale": (f64::from(scale) * 1000.0).round() / 1000.0,
            "env": env.iter().map(|(n, v, _)| format!("{n}={v}")).collect::<Vec<_>>(),
        }),
    );
}
