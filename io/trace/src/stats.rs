//! What a run says about itself without a build (docs/nobuild.feature NB2a): the stat channels that turn raea's traces
//! and timings on, and the run's totals (each pass's time, counters) that TypeScript reads while it runs
//! (`native("stats.read", {})`) and that the run's end prints and keeps (`stats.json` in its trace folder) when any
//! channel was chosen.
//!
//! A channel is a RAEA_ variable's name without RAEA_, in lower case: RAEA_BAKE_TRACE is `bake_trace`, RAEA_BEVY_PASSES
//! (each of Bevy's render passes timed) `bevy_passes`. It's on when the run's TypeScript chose it
//! (`native("stats.on", { channels: [...] })` at a composition's top level, which runs before the Engine opens), when
//! RAEA_STATS lists it (`RAEA_STATS=bevy_passes,bake_trace`, or `all`), or when its own variable is set (not to 0), the
//! override. A debug view that takes a value takes it the same way: `pass=depth` chooses RAEA_PASS's depth pass, and a
//! channel chosen bare carries "1".

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, RwLock};

/// the channels chosen: RAEA_STATS's, read at the first ask, and then the TypeScript's
static CHOSEN: RwLock<Option<Vec<String>>> = RwLock::new(None);

/// each pass's runs, total and worst ms; each counter's total
#[derive(Default)]
struct Totals {
    passes: BTreeMap<String, (u64, f64, f64)>,
    counters: BTreeMap<String, u64>,
}
static TOTALS: Mutex<Option<Totals>> = Mutex::new(None);

fn from_env() -> Vec<String> {
    std::env::var("RAEA_STATS").unwrap_or_default().split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect()
}

/// The channels this run has chosen (RAEA_STATS's and the TypeScript's), in the order they came.
pub fn chosen() -> Vec<String> {
    if let Some(c) = CHOSEN.read().ok().and_then(|c| c.clone()) {
        return c;
    }
    let Ok(mut w) = CHOSEN.write() else { return from_env() };
    w.get_or_insert_with(from_env).clone()
}

/// Whether the stat channel of the variable `var` (`"RAEA_BAKE_TRACE"`) is on for this run.
pub fn stat(var: &str) -> bool {
    if let Some(v) = std::env::var_os(var) {
        return v != "0";
    }
    let name = var.trim_start_matches("RAEA_");
    let on = |c: &Vec<String>| c.iter().any(|n| n == "all" || n.split('=').next().is_some_and(|n| n.eq_ignore_ascii_case(name)));
    match CHOSEN.read().ok().and_then(|c| c.as_ref().map(on)) {
        Some(o) => o,
        None => on(&chosen()),
    }
}

/// The value the channel of the variable `var` carries: the variable's own (the override), else the chosen channel's
/// (`pass=depth`; chosen bare, or by `all`, "1"). None when it's off.
pub fn stat_value(var: &str) -> Option<String> {
    if let Ok(v) = std::env::var(var) {
        return Some(v);
    }
    let name = var.trim_start_matches("RAEA_");
    let c = chosen();
    let own = c.iter().find_map(|n| match n.split_once('=') {
        Some((k, v)) if k.eq_ignore_ascii_case(name) => Some(v.to_string()),
        None if n.eq_ignore_ascii_case(name) => Some("1".to_string()),
        _ => None,
    });
    own.or_else(|| c.iter().any(|n| n == "all").then(|| "1".to_string()))
}

/// Channels turned on for the rest of this run (`stats.on`); a name is a RAEA_ variable's without RAEA_, or `all`, and
/// `name=value` gives a view its value (a channel chosen again takes the later value).
pub fn choose(channels: &[String]) {
    let mut c = chosen();
    for n in channels {
        let n = n.trim().trim_start_matches("RAEA_").to_ascii_lowercase();
        if n.is_empty() {
            continue;
        }
        let key = n.split('=').next().unwrap_or_default().to_string();
        c.retain(|o| o.split('=').next() != Some(key.as_str()));
        c.push(n);
    }
    if let Ok(mut w) = CHOSEN.write() {
        *w = Some(c);
    }
}

/// A pass's time this frame (ms), by its name (`bevy/main_opaque_pass_3d/elapsed_cpu`, `quads/composite`).
pub fn pass_ms(name: &str, ms: f64) {
    let Ok(mut g) = TOTALS.lock() else { return };
    let t = g.get_or_insert_with(Totals::default);
    match t.passes.get_mut(name) {
        Some(p) => *p = (p.0 + 1, p.1 + ms, p.2.max(ms)),
        None => {
            t.passes.insert(name.to_string(), (1, ms, ms));
        }
    }
}

/// `n` more of something this run counts (`engine/updates`, `engine/draws`): a counter needs no line printed for it.
pub fn count(name: &str, n: u64) {
    let Ok(mut g) = TOTALS.lock() else { return };
    let t = g.get_or_insert_with(Totals::default);
    match t.counters.get_mut(name) {
        Some(c) => *c += n,
        None => {
            t.counters.insert(name.to_string(), n);
        }
    }
}

/// The run so far: the channels chosen, each pass's runs and mean and worst ms, and each counter.
pub fn stats() -> Value {
    let g = TOTALS.lock().ok();
    let t = g.as_ref().and_then(|g| g.as_ref());
    let passes: serde_json::Map<String, Value> = t.into_iter().flat_map(|t| t.passes.iter()).map(|(k, (n, sum, max))| (k.clone(), json!({"n": n, "mean_ms": round(sum / *n as f64), "max_ms": round(*max)}))).collect();
    let counters = t.map_or(json!({}), |t| json!(t.counters));
    json!({"chosen": chosen(), "passes": passes, "counters": counters})
}

fn round(ms: f64) -> f64 {
    (ms * 1000.0).round() / 1000.0
}

/// The stats as a table: the passes slowest first (by mean), then the counters.
pub fn table(v: &Value) -> String {
    let mut out = format!("raea stats ({}):\n", v["chosen"].as_array().map(|c| c.iter().filter_map(|n| n.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default());
    let mut passes: Vec<(&String, &Value)> = v["passes"].as_object().into_iter().flatten().collect();
    passes.sort_by(|a, b| b.1["mean_ms"].as_f64().unwrap_or(0.0).total_cmp(&a.1["mean_ms"].as_f64().unwrap_or(0.0)));
    for (k, p) in &passes {
        out += &format!("  {k:<56} {:>6} runs  mean {:>8.3} ms  max {:>8.3} ms\n", p["n"], p["mean_ms"].as_f64().unwrap_or(0.0), p["max_ms"].as_f64().unwrap_or(0.0));
    }
    for (k, n) in v["counters"].as_object().into_iter().flatten() {
        out += &format!("  {k:<56} {n:>6}\n");
    }
    if passes.is_empty() {
        out += "  no pass timed (bevy_passes times Bevy's render passes)\n";
    }
    out
}

/// At a run's end, when it chose any channel: the table on stderr and `stats.json` in its trace folder.
pub(crate) fn finish(dir: Option<&Path>) {
    if chosen().is_empty() {
        return;
    }
    let v = stats();
    eprint!("{}", table(&v));
    if let Some(d) = dir {
        let _ = std::fs::write(d.join("stats.json"), format!("{v:#}"));
        eprintln!("  (kept in {})", d.join("stats.json").display());
    }
}
