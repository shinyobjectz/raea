//! void's tracer (raea-otel): every agent turn, model call and tool call as OpenTelemetry spans, to Arize Phoenix
//! (project "void", http://127.0.0.1:6006) and to ~/.raea-void/traces/<date>.jsonl. An eval run points it at its own
//! file instead, so each result keeps its trace.

use std::sync::OnceLock;

use raea_otel::Tracer;
use serde_json::{json, Map};

pub fn tracer() -> &'static Tracer {
    static T: OnceLock<Tracer> = OnceLock::new();
    T.get_or_init(|| {
        let mut res = Map::new();
        res.insert("service.version".into(), json!(env!("CARGO_PKG_VERSION")));
        if let Ok(h) = std::process::Command::new("hostname").output() {
            res.insert("host.name".into(), json!(String::from_utf8_lossy(&h.stdout).trim()));
        }
        let project = std::env::var("RAEA_VOID_TRACE_PROJECT").unwrap_or_else(|_| "void".into());
        let t = Tracer::new("void", &project, res);
        let day = std::process::Command::new("date").arg("+%Y-%m-%d").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        let file = std::env::var_os("RAEA_VOID_TRACE_FILE").map(std::path::PathBuf::from)
            .unwrap_or_else(|| crate::spaces::home().join("traces").join(format!("{day}.jsonl")));
        t.to_file(Some(file));
        t
    })
}

/// What a model call cost (USD), from the provider's rates per million tokens: MiniMax M3 at 0.30 in, 1.20 out,
/// 0.06 cache read, cache writes at the input rate (the host app's docs/pricing.feature; OpenRouter's minimax/minimax-m3 lists
/// the same rates); Claude Sonnet 5.5 at 3 / 15 /
/// 0.30 / 3.75. Unknown models cost 0 and say so in the trace.
pub fn cost(model: &str, input: u64, output: u64, cache_read: u64, cache_write: u64) -> Option<f64> {
    let (i, o, r, w) = if model.starts_with("MiniMax-M3") || model.starts_with("minimax/minimax-m3") {
        (0.30, 1.20, 0.06, 0.30)
    } else if model.starts_with("xiaomi/mimo-v2.6-pro") {
        // OpenRouter's rates (2026-10-16): 0.435 in, 0.87 out, 0.004 cache read; writes at the input rate
        (0.435, 0.87, 0.004, 0.435)
    } else if model.starts_with("z-ai/glm-5.3-flash") {
        // OpenRouter's rates (2026-10-17): 0.15 in, 0.50 out; cache reads assumed at a tenth of the input rate
        (0.15, 0.50, 0.015, 0.15)
    } else if model.starts_with("elorian/glimpse") {
        // OpenRouter's rates (2026-10-17, Together): 0.60 in, 3.00 out, 0.06 cache read
        (0.60, 3.0, 0.06, 0.60)
    } else if model.starts_with("claude-sonnet") {
        (3.0, 15.0, 0.30, 3.75)
    } else {
        return None;
    };
    Some((input as f64 * i + output as f64 * o + cache_read as f64 * r + cache_write as f64 * w) / 1e6)
}
