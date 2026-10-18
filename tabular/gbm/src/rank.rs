//! Dreaming proper: given one content, rank the ways to lay it out before rendering any (the roadmap K3 and Track D).
//! Each content is a row of cards (titles over spaced monospace labels, like the founder's 2026-10-15 screenshot) laid out
//! VARIANTS ways (card width, title and label size, letter spacing); `raea lint --layout` labels each layout, and the
//! best layout is the clean one with the largest type (readable on a phone), so the score is a trade-off: a spill 0, a
//! crowded row 1, clean 2 plus up to 3 for the title's size and 1 for the label's. (Scored only clean or not, the plain
//! estimate wins: pick the roomiest layout, 40 of 40 clean against the model's 39; the trade-off is the real choice.) Perpetual's list-wise ranking (ListNet) learns from the plan's
//! numbers alone, grouped by content; contents are held out by fold. Scored by how often the top pick is clean, against
//! the plain estimate's pick (the narrowest margin of the widest title and label over the card's inner width) and the
//! clean share (a random pick). Everything here runs in Rust; the labels come from raea's own engine binary.

use perpetual::objective::Objective;
use perpetual::{Matrix, PerpetualBooster};
use std::path::PathBuf;

const TITLES: [&str; 15] = ["Hire", "Film", "Check the footage", "Edit the cuts", "Check the edit", "Client review", "Plan", "Ship it",
    "Write the brief", "Find a creator", "Approve", "Post", "Measure the results", "Pay", "Review the rough cut"];
const LABELS: [&str; 12] = ["FULFILLMENT", "CREATOR", "QA", "EDITORS", "CLIENT", "OPS", "STRATEGY", "PRODUCTION", "FINANCE", "BRAND", "LEGAL", "SOCIAL"];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[(self.next() as usize) % v.len()]
    }
}

struct Layout {
    card_w: f64,
    title_px: f64,
    label_px: f64,
    spacing: f64,
}

fn html(words: &[(&str, &str)], l: &Layout) -> String {
    let cards: String = words.iter().enumerate().map(|(i, (t, lab))| format!(r#"<div class="card" id="c{i}"><div class="t">{t}</div><div class="l">{lab}</div></div>"#)).collect();
    format!(r#"<!doctype html><html><head><style>
html,body{{margin:0;width:1920px;height:400px;background:#fff;font-family:Inter,Helvetica,sans-serif;color:#111}}
#root{{position:relative;width:1920px;height:400px;overflow:hidden}}
.row{{position:absolute;left:40px;top:100px;display:flex;gap:16px}}
.card{{width:{}px;height:105px;box-sizing:border-box;padding:20px 16px 0 50px;border:1px solid #ddd;border-radius:6px}}
.t{{font-size:{}px;font-weight:700;line-height:1.3}}
.l{{font-family:Menlo,monospace;font-size:{}px;letter-spacing:{}px;color:#666}}
</style></head><body><div id="root" data-composition-id="main" data-start="0" data-duration="1" data-width="1920" data-height="400">
<div class="row">{cards}</div></div>
<script>window.__timelines = {{ main: {{ duration: function () {{ return 1; }}, seek: function () {{}}, pause: function () {{}}, time: function () {{ return 0; }} }} }};</script>
</body></html>"#, l.card_w, l.title_px, l.label_px, l.spacing)
}

/// What a plan knows before rendering: its numbers and the estimated widths against the card's inner width.
fn features(words: &[(&str, &str)], l: &Layout) -> Vec<f64> {
    let inner = l.card_w - 2.0 - 16.0 - 50.0;
    let tw = words.iter().map(|(t, _)| t.len() as f64 * l.title_px * 0.55).fold(0.0, f64::max);
    let lw = words.iter().map(|(_, lab)| lab.len() as f64 * (l.label_px * 0.6 + l.spacing)).fold(0.0, f64::max);
    let word = words.iter().flat_map(|(t, _)| t.split(' ')).map(|w| w.len() as f64 * l.title_px * 0.55).fold(0.0, f64::max);
    let wpt = words.iter().map(|(t, _)| t.split(' ').count() as f64).sum::<f64>() / words.len() as f64;
    vec![words.len() as f64, l.card_w, inner, l.title_px, l.label_px, l.spacing, tw, lw, word, tw / inner, lw / inner, word / inner, wpt]
}

/// The plain estimate's score for a layout: the tighter of the widest title and label against the inner width (lower
/// is better).
fn estimate(f: &[f64]) -> f64 {
    f[9].max(f[10])
}

pub fn lint(raea: &str, path: &std::path::Path) -> Option<f64> {
    let out = std::process::Command::new(raea).arg("lint").arg(path).args(["--layout", "--json", "--at", "0.5", "-o"]).arg(path.with_extension("crops")).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let j: serde_json::Value = serde_json::from_str(&text[text.find('{')?..]).ok()?;
    let spill = j["errors"].as_array()?.iter().any(|e| e.as_str().is_some_and(|s| s.contains(" spills ")));
    let crowded = j["warnings"].as_array()?.iter().any(|e| e.as_str().is_some_and(|s| s.contains(" wraps in ")));
    Some(if spill { 0.0 } else if crowded { 1.0 } else { 2.0 })
}

pub fn run(contents: usize, variants: usize, budget: f32) -> Result<serde_json::Value, String> {
    let raea = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("raea/engine/target/release/raea");
    let dir = std::env::temp_dir().join(format!("raea-gbm-rank-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut rng = Rng(7);
    // (content, features, relevance)
    let mut rows: Vec<(usize, Vec<f64>, f64)> = Vec::new();
    for c in 0..contents {
        let n = 3 + (rng.next() % 6) as usize;
        let words: Vec<(&str, &str)> = (0..n).map(|_| (*rng.pick(&TITLES), *rng.pick(&LABELS))).collect();
        for v in 0..variants {
            let l = Layout { card_w: *rng.pick(&[120.0, 150.0, 180.0, 220.0, 260.0]), title_px: *rng.pick(&[16.0, 18.0, 20.0, 24.0]),
                             label_px: *rng.pick(&[12.0, 14.0, 17.0]), spacing: *rng.pick(&[0.0, 1.0, 2.0, 3.0]) };
            let path = dir.join(format!("c{c}-v{v}.html"));
            std::fs::write(&path, html(&words, &l)).map_err(|e| e.to_string())?;
            if let Some(fit) = lint(raea.to_str().unwrap_or("raea"), &path) {
                let rel = if fit == 2.0 { 2.0 + (l.title_px - 16.0) / 8.0 * 3.0 + (l.label_px - 12.0) / 5.0 } else { fit };
                rows.push((c, features(&words, &l), rel));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    let mut out = score_ranking(&rows, budget, estimate, |f| f[3] * 100.0 + f[4])?;
    out["contents"] = serde_json::json!(contents);
    Ok(out)
}

/// Rows (content, features, relevance) ranked by ListNet with contents held out by fold (5); the model's top pick
/// against the plain estimate's (lowest `est`), the largest type's (highest `large`) and a random pick.
pub fn score_ranking(rows: &[(usize, Vec<f64>, f64)], budget: f32, est: impl Fn(&[f64]) -> f64, large: impl Fn(&[f64]) -> f64) -> Result<serde_json::Value, String> {
    // contents held out by fold (5), each content's candidates kept together and in order (ListNet's groups)
    let k = 5;
    let mut picked_model = Vec::new();
    let mut picked_estimate = Vec::new();
    let mut picked_large = Vec::new();
    let mut clean_share = Vec::new();
    // each pick's score over the best in its content (1 = the best layout chosen)
    let (mut q_model, mut q_est, mut q_large, mut q_rand) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for f in 0..k {
        let train: Vec<&(usize, Vec<f64>, f64)> = rows.iter().filter(|r| r.0 % k != f).collect();
        let test: Vec<&(usize, Vec<f64>, f64)> = rows.iter().filter(|r| r.0 % k == f).collect();
        let cols = train[0].1.len();
        let data: Vec<f64> = (0..cols).flat_map(|c| train.iter().map(move |r| r.1[c])).collect();
        let y: Vec<f64> = train.iter().map(|r| r.2).collect();
        let mut groups: Vec<u64> = Vec::new();
        let mut last = usize::MAX;
        for r in &train {
            if r.0 != last {
                groups.push(0);
                last = r.0;
            }
            *groups.last_mut().unwrap() += 1;
        }
        let mut b = PerpetualBooster::default().set_objective(Objective::ListNetLoss).set_budget(budget).set_seed(0).set_num_threads(Some(1));
        b.fit(&Matrix::new(&data, train.len(), cols), &y, None, Some(&groups)).map_err(|e| format!("{e:?}"))?;
        let tdata: Vec<f64> = (0..cols).flat_map(|c| test.iter().map(move |r| r.1[c])).collect();
        let score = b.predict(&Matrix::new(&tdata, test.len(), cols), false);
        let mut contents_in: Vec<usize> = test.iter().map(|r| r.0).collect();
        contents_in.dedup();
        for c in contents_in {
            let idx: Vec<usize> = (0..test.len()).filter(|&i| test[i].0 == c).collect();
            let best_model = *idx.iter().max_by(|&&a, &&b| score[a].total_cmp(&score[b])).unwrap();
            let best_est = *idx.iter().min_by(|&&a, &&b| est(&test[a].1).total_cmp(&est(&test[b].1))).unwrap();
            picked_model.push(test[best_model].2);
            picked_estimate.push(test[best_est].2);
            // the biggest type: the largest title, then label
            let best_large = *idx.iter().max_by(|&&a, &&b| large(&test[a].1).total_cmp(&large(&test[b].1))).unwrap();
            picked_large.push(test[best_large].2);
            let top = idx.iter().map(|&i| test[i].2).fold(0.0, f64::max).max(1e-9);
            q_model.push(test[best_model].2 / top);
            q_est.push(test[best_est].2 / top);
            q_large.push(test[best_large].2 / top);
            q_rand.push(idx.iter().map(|&i| test[i].2 / top).sum::<f64>() / idx.len() as f64);
            clean_share.push(idx.iter().filter(|&&i| test[i].2 >= 2.0).count() as f64 / idx.len() as f64);
        }
    }
    let share = |v: &[f64], x: f64| (v.iter().filter(|r| **r == x).count() as f64 / v.len().max(1) as f64 * 1000.0).round() / 1000.0;
    let clean = |v: &[f64]| (v.iter().filter(|r| **r >= 2.0).count() as f64 / v.len().max(1) as f64 * 1000.0).round() / 1000.0;
    let mean = |v: &[f64]| (v.iter().sum::<f64>() / v.len().max(1) as f64 * 1000.0).round() / 1000.0;
    let any_clean = clean_share.iter().filter(|s| **s > 0.0).count();
    Ok(serde_json::json!({
        "layouts": rows.len(), "labels": {"clean": rows.iter().filter(|r| r.2 >= 2.0).count(), "crowded_only": rows.iter().filter(|r| r.2 == 1.0).count(), "spill": rows.iter().filter(|r| r.2 == 0.0).count()},
        "contents_with_a_clean_layout": any_clean,
        "top_pick_clean": {"model": clean(&picked_model), "roomiest": clean(&picked_estimate), "largest_type": clean(&picked_large), "random": mean(&clean_share)},
        "top_pick_spills": {"model": share(&picked_model, 0.0), "roomiest": share(&picked_estimate, 0.0), "largest_type": share(&picked_large, 0.0)},
        "top_pick_quality_of_best": {"model": mean(&q_model), "roomiest": mean(&q_est), "largest_type": mean(&q_large), "random": mean(&q_rand)},
    }))
}
