//! The target's side of a frame: the view box a tagged row draws in (raea-sys-viewbox), supersampling (its downsample:
//! raea-runtime-gpuframe), the render world's diagnostics forwarded.

use super::*;
pub(crate) use raea_sys_viewbox::{TagRowTarget, ViewBox, tag_row_view};

/// Bevy's render diagnostics (each pass's elapsed time, `render/...`): each new measurement into the run's totals (stats.read, the stats table: nobuild NB2a), the smoothed ones into its trace once a second.
#[cfg(feature = "native")]
pub(crate) fn forward_render_diagnostics(store: Res<bevy::diagnostic::DiagnosticsStore>, mut last: Local<Option<std::time::Instant>>, mut seen: Local<std::collections::HashMap<String, std::time::Instant>>) {
    for d in store.iter().filter(|d| d.path().as_str().starts_with("render/")) { if let Some(m) = d.measurement().filter(|m| seen.insert(d.path().as_str().to_string(), m.time) != Some(m.time)) { raea_io_trace::pass_ms(&format!("bevy/{}", d.path().as_str().trim_start_matches("render/")), m.value); } }
    if last.is_some_and(|t| t.elapsed().as_secs_f64() < 1.0) { return; } *last = Some(std::time::Instant::now());
    let passes: serde_json::Map<String, serde_json::Value> = store.iter().filter(|d| d.path().as_str().starts_with("render/"))
        .filter_map(|d| d.smoothed().map(|v| (d.path().as_str().trim_start_matches("render/").to_string(), serde_json::json!((v * 1000.0).round() / 1000.0)))).collect();
    if !passes.is_empty() { raea_io_trace::event("bevy_passes", serde_json::json!({"ms": passes})); }
}

/// RAEA_SS (2, 3 or 4; 1 without it): how many times the frame's size it's drawn at. Not live, and only with the
/// lossless readback, which the downsampling reads.
pub(crate) fn supersampling(live: bool) -> u32 {
    let ss = std::env::var("RAEA_SS").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1).clamp(1, 4);
    if ss > 1 && (live || std::env::var("RAEA_READBACK").map(|v| v != "rgba").unwrap_or(true)) {
        eprintln!("raea: RAEA_SS needs RAEA_READBACK=rgba and a render, not a live view; drawing at 1x");
        return 1;
    }
    ss
}
