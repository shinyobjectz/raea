//! The bench's table and its JSON (docs/denoise.feature DN0): a line a case and backend, and against MetalFX, the first
//! bar, how many of its six numbers (FLIP, flicker, the worst whole-frame spike, blotches, |bias|, the worst lag) each backend meets on that case.

use raea_runtime_bundle::R;
use serde_json::{json, Value};
use std::path::Path;

/// The numbers the bar compares, each smaller is better (bias by its size)
const BAR: [&str; 5] = ["flip", "flicker_stops", "spike_stops", "blotch_stops", "bias_stops"];

fn worst_lag(v: &Value) -> Option<f64> {
    let lags = v["lag"].as_array()?;
    // an event that never settled in the shot counts as the longest lag there is
    lags.iter().fold(None, |worst: Option<f64>, l| Some(worst.unwrap_or(0.0).max(l["frames"].as_f64().unwrap_or(f64::INFINITY))))
}

/// Of the bar's numbers this backend and MetalFX both have on a case, how many it meets (no worse), and of how many
pub fn against(v: &Value, metalfx: &Value) -> Option<(usize, usize)> {
    if v["ran"] != json!(true) || metalfx["ran"] != json!(true) {
        return None;
    }
    let mut pairs: Vec<(f64, f64)> = BAR.iter().filter_map(|k| Some((v[*k].as_f64()?.abs(), metalfx[*k].as_f64()?.abs()))).collect();
    if let (Some(a), Some(b)) = (worst_lag(v), worst_lag(metalfx)) {
        pairs.push((a, b));
    }
    (!pairs.is_empty()).then(|| (pairs.iter().filter(|(a, b)| a <= b).count(), pairs.len()))
}

/// The table a person reads
pub fn table(reports: &[Value]) -> String {
    let mut s = format!("{:<14} {:<8} {:>7} {:>7} {:>7} {:>8} {:>7} {:>7} {:>5} {:>8} {:>8} {:>8}\n", "case", "backend", "flip", "rmse", "bias", "flicker", "spike", "blotch", "lag", "s/still", "live ms", "metalfx");
    for r in reports {
        let name = r["name"].as_str().unwrap_or("");
        let Some(bs) = r["backends"].as_object() else { continue };
        for (b, v) in bs {
            if v["ran"] != json!(true) {
                s += &format!("{name:<14} {b:<8} (not run: {})\n", v["why"].as_str().unwrap_or(""));
                continue;
            }
            let f = |k: &str| raea_light::cell(v, k);
            let lag = worst_lag(v).map_or("-".to_string(), |l| if l.is_finite() { format!("{l}") } else { "never".into() });
            let live = v["live"]["ms_a_frame"].as_f64().map_or("-".to_string(), |x| format!("{x:.1}"));
            let bar = if b == "metalfx" { "bar".to_string() } else { against(v, &bs.get("metalfx").cloned().unwrap_or_default()).map_or("-".to_string(), |(m, n)| format!("{m}/{n}")) };
            s += &format!("{name:<14} {b:<8} {:>7} {:>7} {:>7} {:>8} {:>7} {:>7} {lag:>5} {:>8} {live:>8} {bar:>8}\n", f("flip"), f("rmse"), f("bias_stops"), f("flicker_stops"), f("spike_stops"), f("blotch_stops"), f("seconds_a_still"));
        }
        if r["truth"]["made"] == json!(false) {
            s += &format!("{name:<14} (no truth: {})\n", r["truth"]["why"].as_str().unwrap_or(""));
        }
    }
    s
}

/// DIR/report.json, and the table with where it went
pub fn write(reports: &[Value], out: &Path) -> R<String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join("report.json");
    std::fs::write(&path, serde_json::to_string_pretty(&json!({ "cases": reports })).unwrap_or_default()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("{}raea bench denoise: wrote {}", table(reports), path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backend_is_held_to_metalfx_number_by_number() {
        let m = json!({ "ran": true, "flip": 0.10, "flicker_stops": 0.05, "blotch_stops": 0.02, "bias_stops": -0.1, "lag": [{ "frames": 6 }] });
        let r = json!({ "ran": true, "flip": 0.09, "flicker_stops": 0.06, "blotch_stops": 0.02, "bias_stops": 0.05, "lag": [{ "frames": null }] });
        // flip, blotch and |bias| met; flicker and the lag (never settled) not
        assert_eq!(against(&r, &m), Some((3, 5)));
        assert_eq!(against(&json!({ "ran": false }), &m), None);
    }
}
