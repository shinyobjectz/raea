//! raea-gbm: tabular dreaming's base, all Rust (the founder, 2026-10-15: "no sidecars, no python"): gradient-boosted trees
//! from Perpetual (pure Rust, Apache-2.0; it sets its own learning rate and stopping from one "budget"), grouped
//! cross-validation (no plan family, scenario or composition on both sides, as sklearn's GroupKFold) and AUC, over the
//! tables raea's benches built in ~/.cache/raea-tabular. Each table's model, trained on every row, is saved as JSON
//! (~/.cache/raea-tabular/gbm/NAME.json) and reloaded to check it predicts the same.
//!
//!   raea-gbm cv [--budget 0.5]
//!   raea-gbm layers-gen [--n 60] [--frames 60] REAL_FULL REAL_BARE ...   J2b on generated render pairs, then real ones
//!   raea-gbm layers FULL BARE FULL BARE ...   J2b: overlay patches learned on some renders, scored on the others
//!   raea-gbm rank [--contents 40] [--variants 8] [--budget 0.5]   dreaming: rank a content's layouts before rendering
//!   raea-gbm decks [--contents 40] [--variants 8]   the same over deck and dashboard slides (KPI tiles, pills, cards, agendas)

mod deck;
mod gen;
mod layers;
mod rank;

use perpetual::objective::Objective;
use perpetual::{Matrix, PerpetualBooster};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

struct Table {
    name: String,
    keys: Vec<String>,
    x: Vec<Vec<f64>>,
    y: Vec<f64>,
    group: Vec<String>,
    /// a plain estimate to beat (one of the features), when the table has one
    estimate: Option<usize>,
}

fn cache() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/raea-tabular")
}

fn num(v: &Value) -> f64 {
    match v {
        Value::Bool(b) => *b as u8 as f64,
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

fn tables() -> Vec<Table> {
    let mut out = Vec::new();
    // K3's generated card-row plans: spill and crowded, the plain estimate the widest label (title) over the inner width
    if let Ok(t) = std::fs::read_to_string(cache().join("layout_plans.json")) {
        let rows: Vec<Value> = serde_json::from_str(&t).unwrap_or_default();
        if let Some(first) = rows.first().and_then(Value::as_object) {
            let keys: Vec<String> = first.keys().filter(|k| !["family", "spill", "crowded", "bad"].contains(&k.as_str())).cloned().collect();
            for (target, est) in [("spill", "label_over_inner"), ("crowded", "title_over_inner")] {
                out.push(Table {
                    name: format!("plans-{target}"),
                    x: rows.iter().map(|r| keys.iter().map(|k| num(&r[k])).collect()).collect(),
                    y: rows.iter().map(|r| num(&r[target])).collect(),
                    group: rows.iter().map(|r| r["family"].to_string()).collect(),
                    estimate: keys.iter().position(|k| k == est),
                    keys: keys.clone(),
                });
            }
        }
    }
    // D3's moves: does a move help, from the move against the request (the kind as its index)
    if let Ok(t) = std::fs::read_to_string(cache().join("moves.jsonl")) {
        let rows: Vec<Value> = t.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let mut kinds: Vec<String> = rows.iter().filter_map(|r| r["kind"].as_str().map(String::from)).collect();
        kinds.sort();
        kinds.dedup();
        let keys: Vec<String> = ["input_chars", "input_lines", "has_why", "kind", "req_overlap", "req_overlap_share", "target_in_request", "target_existed", "numbers_changed", "is_expect"]
            .iter().map(|s| s.to_string()).collect();
        out.push(Table {
            name: "moves-helps".into(),
            x: rows.iter().map(|r| keys.iter().map(|k| if k == "kind" { kinds.iter().position(|x| Some(x.as_str()) == r["kind"].as_str()).unwrap_or(0) as f64 } else { num(&r[k]) }).collect()).collect(),
            y: rows.iter().map(|r| (r["effect"].as_str() == Some("helps")) as u8 as f64).collect(),
            group: rows.iter().map(|r| r["scenario"].to_string()).collect(),
            estimate: None,
            keys,
        });
    }
    // K3's what-if on real compositions, when it has signal
    if let Ok(t) = std::fs::read_to_string(cache().join("layout_whatif.json")) {
        let rows: Vec<Value> = serde_json::from_str(&t).unwrap_or_default();
        let keys: Vec<String> = ["spacing", "chars", "words", "lines", "text_w", "comp_w", "room", "estimate"].iter().map(|s| s.to_string()).collect();
        out.push(Table {
            name: "whatif-spill".into(),
            x: rows.iter().map(|r| keys.iter().map(|k| num(&r[k])).collect()).collect(),
            y: rows.iter().map(|r| num(&r["spills"])).collect(),
            group: rows.iter().map(|r| r["comp"].to_string()).collect(),
            estimate: Some(7),
            keys,
        });
    }
    out
}

/// sklearn's GroupKFold: groups, largest first, each to the fold with the fewest rows so far.
fn group_folds(group: &[String], k: usize) -> Vec<usize> {
    let mut sizes: HashMap<&str, usize> = HashMap::new();
    for g in group {
        *sizes.entry(g.as_str()).or_default() += 1;
    }
    let mut order: Vec<(&str, usize)> = sizes.into_iter().collect();
    order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let mut load = vec![0usize; k];
    let mut fold_of: HashMap<&str, usize> = HashMap::new();
    for (g, n) in order {
        let f = (0..k).min_by_key(|&f| (load[f], f)).unwrap();
        load[f] += n;
        fold_of.insert(g, f);
    }
    group.iter().map(|g| fold_of[g.as_str()]).collect()
}

/// The area under the ROC curve: the chance a positive outranks a negative (ties count half).
fn auc(y: &[f64], p: &[f64]) -> f64 {
    let mut idx: Vec<usize> = (0..y.len()).collect();
    idx.sort_by(|&a, &b| p[a].total_cmp(&p[b]));
    // average ranks over ties
    let mut rank = vec![0.0; y.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && p[idx[j + 1]] == p[idx[i]] {
            j += 1;
        }
        let r = (i + j) as f64 / 2.0 + 1.0;
        for t in i..=j {
            rank[idx[t]] = r;
        }
        i = j + 1;
    }
    let pos = y.iter().filter(|v| **v > 0.5).count() as f64;
    let neg = y.len() as f64 - pos;
    let sum: f64 = (0..y.len()).filter(|&i| y[i] > 0.5).map(|i| rank[i]).sum();
    (sum - pos * (pos + 1.0) / 2.0) / (pos * neg)
}

/// Column-major data for Perpetual's Matrix.
fn columns(x: &[Vec<f64>], rows: &[usize]) -> Vec<f64> {
    let cols = x.first().map(Vec::len).unwrap_or(0);
    (0..cols).flat_map(|c| rows.iter().map(move |&r| x[r][c])).collect()
}

fn fit(x: &[Vec<f64>], y: &[f64], rows: &[usize], budget: f32) -> Result<PerpetualBooster, String> {
    let data = columns(x, rows);
    let m = Matrix::new(&data, rows.len(), x[0].len());
    let yy: Vec<f64> = rows.iter().map(|&r| y[r]).collect();
    let mut b = PerpetualBooster::default().set_objective(Objective::LogLoss).set_budget(budget).set_seed(0).set_num_threads(Some(1));
    b.fit(&m, &yy, None, None).map_err(|e| format!("{e:?}"))?;
    Ok(b)
}

fn predict(b: &PerpetualBooster, x: &[Vec<f64>], rows: &[usize]) -> Vec<f64> {
    let data = columns(x, rows);
    b.predict_proba(&Matrix::new(&data, rows.len(), x[0].len()), false, false)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let budget: f32 = args.iter().position(|a| a == "--budget").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(0.5);
    let flag = |k: &str, d: usize| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(d);
    if args.first().map(String::as_str) == Some("layers-gen") {
        // raea-gbm layers-gen [--n 60] [--frames 60] REAL_FULL REAL_BARE ...: J2b on generated render pairs, then real renders
        let dir = cache().join("layers-gen");
        let real: Vec<String> = args[1..].iter().filter(|a| a.ends_with(".mp4")).cloned().collect();
        let extra: Vec<(String, String)> = real.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].clone(), c[1].clone())).collect();
        match gen::generate(flag("--n", 60), flag("--frames", 60), &dir).and_then(|pairs| layers::run_generated(&pairs, &extra, budget)) {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
            Err(e) => eprintln!("raea-gbm layers-gen: {e}"),
        }
        return;
    }
    if args.first().map(String::as_str) == Some("layers") {
        // raea-gbm layers FULL BARE FULL BARE ...: overlay patches learned on some renders, scored on the others
        let files: Vec<String> = args[1..].iter().filter(|a| !a.starts_with("--")).cloned().collect();
        let pairs: Vec<(String, String)> = files.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].clone(), c[1].clone())).collect();
        match layers::run(&pairs, budget) {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
            Err(e) => eprintln!("raea-gbm layers: {e}"),
        }
        return;
    }
    if args.first().map(String::as_str) == Some("decks") {
        match deck::run(flag("--contents", 40), flag("--variants", 8), budget) {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
            Err(e) => eprintln!("raea-gbm decks: {e}"),
        }
        return;
    }
    if args.first().map(String::as_str) == Some("rank") {
        match rank::run(flag("--contents", 40), flag("--variants", 8), budget) {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
            Err(e) => eprintln!("raea-gbm rank: {e}"),
        }
        return;
    }
    let out_dir = cache().join("gbm");
    let _ = std::fs::create_dir_all(&out_dir);
    let mut report = serde_json::Map::new();
    for t in tables() {
        let pos = t.y.iter().filter(|v| **v > 0.5).count();
        let pos_groups = { let mut g: Vec<&String> = (0..t.y.len()).filter(|&i| t.y[i] > 0.5).map(|i| &t.group[i]).collect(); g.sort(); g.dedup(); g.len() };
        if pos < 5 || t.y.len() - pos < 5 || pos_groups < 5 {
            report.insert(t.name.clone(), Value::String(format!("too little signal: {pos} positives in {pos_groups} groups")));
            continue;
        }
        let folds = group_folds(&t.group, 5);
        let mut p = vec![0.0; t.y.len()];
        let started = std::time::Instant::now();
        for f in 0..5 {
            let tr: Vec<usize> = (0..t.y.len()).filter(|&i| folds[i] != f).collect();
            let te: Vec<usize> = (0..t.y.len()).filter(|&i| folds[i] == f).collect();
            let one_class = tr.iter().all(|&i| t.y[i] == t.y[tr[0]]);
            let preds = if one_class { vec![t.y[tr[0]]; te.len()] } else {
                match fit(&t.x, &t.y, &tr, budget) {
                    Ok(b) => predict(&b, &t.x, &te),
                    Err(e) => { eprintln!("{}: {e}", t.name); vec![0.5; te.len()] }
                }
            };
            for (k, &i) in te.iter().enumerate() {
                p[i] = preds[k];
            }
        }
        let mut res = serde_json::Map::new();
        res.insert("rows".into(), t.y.len().into());
        res.insert("positives".into(), pos.into());
        res.insert("gbm_auc".into(), ((auc(&t.y, &p) * 1000.0).round() / 1000.0).into());
        if let Some(e) = t.estimate {
            let est: Vec<f64> = t.x.iter().map(|r| r[e]).collect();
            res.insert("estimate_auc".into(), ((auc(&t.y, &est) * 1000.0).round() / 1000.0).into());
        }
        res.insert("cv_seconds".into(), ((started.elapsed().as_secs_f64() * 100.0).round() / 100.0).into());
        // the model on every row, saved and reloaded: the same predictions
        let all: Vec<usize> = (0..t.y.len()).collect();
        if let Ok(b) = fit(&t.x, &t.y, &all, budget) {
            let path = out_dir.join(format!("{}.json", t.name));
            let json = serde_json::to_string(&b).unwrap_or_default();
            let _ = std::fs::write(&path, &json);
            let back: Option<PerpetualBooster> = serde_json::from_str(&json).ok();
            let same = back.map(|r| predict(&r, &t.x, &all).iter().zip(predict(&b, &t.x, &all)).all(|(a, b)| (a - b).abs() < 1e-12));
            res.insert("model".into(), path.display().to_string().into());
            res.insert("reloaded_predicts_the_same".into(), same.into());
            res.insert("trees".into(), b.get_prediction_trees().len().into());
        }
        res.insert("features".into(), t.keys.clone().into());
        report.insert(t.name.clone(), Value::Object(res));
    }
    println!("{}", serde_json::to_string_pretty(&Value::Object(report)).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auc_counts_rank_order() {
        assert_eq!(auc(&[0.0, 0.0, 1.0, 1.0], &[0.1, 0.2, 0.3, 0.4]), 1.0);
        assert_eq!(auc(&[1.0, 1.0, 0.0, 0.0], &[0.1, 0.2, 0.3, 0.4]), 0.0);
        assert_eq!(auc(&[0.0, 1.0], &[0.5, 0.5]), 0.5);
    }

    #[test]
    fn groups_never_split() {
        let g: Vec<String> = ["a", "a", "b", "c", "c", "c", "d", "e"].iter().map(|s| s.to_string()).collect();
        let f = group_folds(&g, 3);
        for i in 0..g.len() {
            for j in 0..g.len() {
                if g[i] == g[j] {
                    assert_eq!(f[i], f[j]);
                }
            }
        }
    }

    #[test]
    fn learns_a_threshold() {
        let x: Vec<Vec<f64>> = (0..200).map(|i| vec![i as f64, (i * 7 % 13) as f64]).collect();
        let y: Vec<f64> = (0..200).map(|i| (i >= 100) as u8 as f64).collect();
        let all: Vec<usize> = (0..200).collect();
        let b = fit(&x, &y, &all, 0.5).unwrap();
        let p = predict(&b, &x, &all);
        assert!(auc(&y, &p) > 0.99);
    }
}
