//! J2b, a first learned try: is a patch of a rendered frame one of the edit's overlays or the footage? The truth comes
//! free from rdb: the edit rendered again with its titles, captions, boxes and images at opacity 0, and a patch is
//! overlay when over half its pixels differ (more than 40 levels). Each 24 px patch of every 2nd frame (at half size)
//! gets what the frame alone can tell: its brightness and spread, its colour and saturation, how uniform it is, its edge
//! density, how much it changed since the frame before and since 8 frames before, and where it sits. Perpetual learns
//! overlay or not on one render and is scored on another (never the same edit on both sides).

use perpetual::objective::Objective;
use perpetual::{Matrix, PerpetualBooster};

const P: usize = 24;

fn gray(rgba: &[u8], w: usize, h: usize) -> (Vec<f32>, Vec<[f32; 3]>, usize, usize) {
    // half size
    let (hw, hh) = (w / 2, h / 2);
    let mut g = vec![0f32; hw * hh];
    let mut c = vec![[0f32; 3]; hw * hh];
    for y in 0..hh {
        for x in 0..hw {
            let i = ((y * 2) * w + x * 2) * 4;
            let (r, gg, b) = (rgba[i] as f32, rgba[i + 1] as f32, rgba[i + 2] as f32);
            g[y * hw + x] = 0.299 * r + 0.587 * gg + 0.114 * b;
            c[y * hw + x] = [r, gg, b];
        }
    }
    (g, c, hw, hh)
}

/// Patches' features and labels for one render pair, every `step`th frame.
pub fn patches(full: &str, bare: &str, step: usize) -> Result<(Vec<Vec<f64>>, Vec<f64>), String> {
    let be = raea_media::backend("apple")?;
    let (mut a, mut b) = (be.open_video(std::path::Path::new(full))?, be.open_video(std::path::Path::new(bare))?);
    let info = a.info().clone();
    let n = (info.duration * info.fps).round() as usize;
    let (mut x, mut y) = (Vec::new(), Vec::new());
    let mut history: Vec<Vec<f32>> = Vec::new();
    for f in 0..n {
        let t = (f as f64 + 0.5) / info.fps;
        let fa = a.frame_at(t)?;
        let (g, c, hw, hh) = gray(&fa.rgba, fa.width as usize, fa.height as usize);
        history.push(g.clone());
        if history.len() > 9 {
            history.remove(0);
        }
        if f % step != 0 || history.len() < 9 {
            continue;
        }
        let fb = b.frame_at(t)?;
        let (gb, _, _, _) = gray(&fb.rgba, fb.width as usize, fb.height as usize);
        let prev = &history[history.len() - 2];
        let back8 = &history[0];
        for py in 0..hh / P {
            for px in 0..hw / P {
                let mut v = Vec::with_capacity(P * P);
                let (mut sr, mut sg, mut sb, mut sat, mut edges, mut d1, mut d8, mut diff) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64, 0f64, 0usize);
                for yy in 0..P {
                    for xx in 0..P {
                        let (gx, gy) = (px * P + xx, py * P + yy);
                        let i = gy * hw + gx;
                        v.push(g[i] as f64);
                        let [r, gg, bb] = c[i];
                        sr += r as f64;
                        sg += gg as f64;
                        sb += bb as f64;
                        let (mx, mn) = (r.max(gg).max(bb), r.min(gg).min(bb));
                        sat += if mx > 0.0 { ((mx - mn) / mx) as f64 } else { 0.0 };
                        if gx + 1 < hw && gy + 1 < hh {
                            let e = (g[i + 1] - g[i]).abs() + (g[i + hw] - g[i]).abs();
                            edges += (e > 40.0) as u8 as f64;
                        }
                        d1 += (g[i] - prev[i]).abs() as f64;
                        d8 += (g[i] - back8[i]).abs() as f64;
                        diff += ((g[i] - gb[i]).abs() > 40.0) as usize;
                    }
                }
                let k = (P * P) as f64;
                let mean = v.iter().sum::<f64>() / k;
                let var = v.iter().map(|q| (q - mean).powi(2)).sum::<f64>() / k;
                // uniformity: the share of pixels within 12 levels of the patch's most common brightness band
                let mut hist = [0usize; 22];
                for q in &v {
                    hist[(*q / 12.0).min(21.0) as usize] += 1;
                }
                let mode = *hist.iter().max().unwrap() as f64 / k;
                x.push(vec![mean, var.sqrt(), sr / k, sg / k, sb / k, sat / k, mode, edges / k, d1 / k, d8 / k,
                            (px as f64 + 0.5) / (hw / P) as f64, (py as f64 + 0.5) / (hh / P) as f64]);
                y.push((diff as f64 / k > 0.5) as u8 as f64);
            }
        }
    }
    Ok((x, y))
}

fn auc(y: &[f64], p: &[f64]) -> f64 {
    let mut idx: Vec<usize> = (0..y.len()).collect();
    idx.sort_by(|&a, &b| p[a].total_cmp(&p[b]));
    let mut rank = vec![0.0; y.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && p[idx[j + 1]] == p[idx[i]] {
            j += 1;
        }
        for t in i..=j {
            rank[idx[t]] = (i + j) as f64 / 2.0 + 1.0;
        }
        i = j + 1;
    }
    let pos = y.iter().filter(|v| **v > 0.5).count() as f64;
    let neg = y.len() as f64 - pos;
    ((0..y.len()).filter(|&i| y[i] > 0.5).map(|i| rank[i]).sum::<f64>() - pos * (pos + 1.0) / 2.0) / (pos * neg)
}

fn train(x: &[Vec<f64>], y: &[f64], budget: f32) -> Result<(PerpetualBooster, f64), String> {
    let cols = x[0].len();
    let flat: Vec<f64> = (0..cols).flat_map(|c| x.iter().map(move |r| r[c])).collect();
    let mut b = PerpetualBooster::default().set_objective(Objective::LogLoss).set_budget(budget).set_seed(0).set_num_threads(Some(2));
    b.fit(&Matrix::new(&flat, x.len(), cols), y, None, None).map_err(|e| format!("{e:?}"))?;
    // overlays are a few percent of patches: the threshold with the best F on the training rows (never the test)
    let ptr = b.predict_proba(&Matrix::new(&flat, x.len(), cols), false, false);
    // the threshold as if every footage patch were there: training keeps one in four (`thin`), so each counts 4 times
    let wf = |th: f64| {
        let (tp, fp, fnn) = (0..y.len()).fold((0.0f64, 0.0f64, 0.0f64), |(a, b, c), k| { let (pr, tr) = (ptr[k] > th, y[k] > 0.5); (a + (pr && tr) as u8 as f64, b + 4.0 * (pr && !tr) as u8 as f64, c + (!pr && tr) as u8 as f64) });
        2.0 * tp / (2.0 * tp + fp + fnn).max(1e-9)
    };
    let th = (1..100).map(|k| k as f64 / 100.0).max_by(|a, c| wf(*a).total_cmp(&wf(*c))).unwrap_or(0.5);
    Ok((b, th))
}

/// (F, precision, recall) at a threshold.
fn f_at(th: f64, p: &[f64], y: &[f64]) -> (f64, f64, f64) {
    let (tp, fp, fnn) = (0..y.len()).fold((0usize, 0usize, 0usize), |(a, b, c), k| { let (pr, tr) = (p[k] > th, y[k] > 0.5); (a + (pr && tr) as usize, b + (pr && !tr) as usize, c + (!pr && tr) as usize) });
    let (prec, rec) = (tp as f64 / (tp + fp).max(1) as f64, tp as f64 / (tp + fnn).max(1) as f64);
    (2.0 * prec * rec / (prec + rec).max(1e-9), prec, rec)
}

fn score(b: &PerpetualBooster, th: f64, x: &[Vec<f64>], y: &[f64]) -> serde_json::Value {
    let cols = x[0].len();
    let flat: Vec<f64> = (0..cols).flat_map(|c| x.iter().map(move |r| r[c])).collect();
    let p = b.predict_proba(&Matrix::new(&flat, x.len(), cols), false, false);
    let (f, prec, rec) = f_at(th, &p, y);
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    // cuts set per edit from its own scores, no truth: Otsu's split of the score histogram, and the cut that flags as
    // many patches as the probabilities add up to; the best cut (with truth) is the ceiling either can reach
    let per = |t: f64| { let (f, pr, rc) = f_at(t, &p, y); serde_json::json!({"threshold": r3(t), "f": r3(f), "precision": r3(pr), "recall": r3(rc)}) };
    let best = (1..100).map(|k| k as f64 / 100.0).max_by(|a, c| f_at(*a, &p, y).0.total_cmp(&f_at(*c, &p, y).0)).unwrap_or(0.5);
    serde_json::json!({"patches": y.len(), "overlay_patches": y.iter().filter(|v| **v > 0.5).count(), "threshold": th, "auc": r3(auc(y, &p)),
        "precision": r3(prec), "recall": r3(rec), "f": r3(f), "otsu": per(otsu(&p)), "expected_count": per(expected_count(&p)), "best": per(best)})
}

/// Otsu's threshold over 100 bins of the scores: the split with the largest between-class variance.
fn otsu(p: &[f64]) -> f64 {
    let mut h = [0f64; 100];
    for v in p {
        h[((v * 100.0) as usize).min(99)] += 1.0;
    }
    let n = p.len().max(1) as f64;
    let total: f64 = (0..100).map(|i| i as f64 * h[i]).sum();
    let (mut w0, mut s0, mut best, mut at) = (0.0, 0.0, -1.0, 50usize);
    for i in 0..99 {
        w0 += h[i];
        s0 += i as f64 * h[i];
        let w1 = n - w0;
        if w0 == 0.0 || w1 == 0.0 {
            continue;
        }
        let d = s0 / w0 - (total - s0) / w1;
        let v = w0 * w1 * d * d;
        if v > best {
            best = v;
            at = i;
        }
    }
    (at + 1) as f64 / 100.0
}

/// The cut that flags as many patches as the scores sum to (the model's own estimate of how much is overlay).
fn expected_count(p: &[f64]) -> f64 {
    let k = p.iter().sum::<f64>().round() as usize;
    let mut s = p.to_vec();
    s.sort_by(|a, b| b.total_cmp(a));
    s.get(k.min(s.len().saturating_sub(1))).copied().unwrap_or(0.5)
}

/// Generated pairs in 5 folds by pair (a pair's patches on one side only), then a model on all of them scored on each of
/// `extra` (renders of real edits, never generated).
/// A pair's patches, cached beside its render (decoding 240 videos again would take most of lane's 15 minutes).
fn cached_patches(full: &str, bare: &str, step: usize) -> Result<(Vec<Vec<f64>>, Vec<f64>), String> {
    let cache = std::path::PathBuf::from(format!("{full}.patches-{step}.json"));
    if let Some(v) = std::fs::read_to_string(&cache).ok().and_then(|t| serde_json::from_str::<(Vec<Vec<f64>>, Vec<f64>)>(&t).ok()) {
        return Ok(v);
    }
    let v = patches(full, bare, step)?;
    let _ = std::fs::write(&cache, serde_json::to_string(&v).unwrap_or_default());
    Ok(v)
}

/// Training rows: every overlay patch and every 4th footage patch (overlays are a few percent; scoring keeps all).
fn thin(x: Vec<Vec<f64>>, y: Vec<f64>) -> (Vec<Vec<f64>>, Vec<f64>) {
    let mut n = 0usize;
    x.into_iter().zip(y).filter(|(_, l)| { n += 1; *l > 0.5 || n % 4 == 0 }).unzip()
}

pub fn run_generated(pairs: &[(String, String)], extra: &[(String, String)], budget: f32) -> Result<serde_json::Value, String> {
    let data: Vec<(Vec<Vec<f64>>, Vec<f64>)> = pairs.iter().map(|(f, b)| cached_patches(f, b, 3)).collect::<Result<_, _>>()?;
    let mut folds = Vec::new();
    let (mut all_p, mut all_y) = (Vec::new(), Vec::new());
    for f in 0..5 {
        let (mut x, mut y) = (Vec::new(), Vec::new());
        for (i, (xx, yy)) in data.iter().enumerate() {
            if i % 5 != f {
                x.extend(xx.iter().cloned());
                y.extend(yy.iter().cloned());
            }
        }
        let (x, y) = thin(x, y);
        let (b, th) = train(&x, &y, budget)?;
        let (mut tx, mut ty) = (Vec::new(), Vec::new());
        for (i, (xx, yy)) in data.iter().enumerate() {
            if i % 5 == f {
                tx.extend(xx.iter().cloned());
                ty.extend(yy.iter().cloned());
            }
        }
        folds.push(score(&b, th, &tx, &ty));
        let cols = tx[0].len();
        let flat: Vec<f64> = (0..cols).flat_map(|c| tx.iter().map(move |r| r[c])).collect();
        all_p.extend(b.predict_proba(&Matrix::new(&flat, tx.len(), cols), false, false).into_iter().map(|p| (p > th) as u8 as f64));
        all_y.extend(ty);
    }
    let (x, y): (Vec<Vec<f64>>, Vec<f64>) = data.iter().fold((Vec::new(), Vec::new()), |(mut a, mut b), (xx, yy)| { a.extend(xx.iter().cloned()); b.extend(yy.iter().cloned()); (a, b) });
    let (x, y) = thin(x, y);
    let (b, th) = train(&x, &y, budget)?;
    let mut real = serde_json::Map::new();
    for (f, bare) in extra {
        let (ex, ey) = patches(f, bare, 2)?;
        real.insert(std::path::Path::new(f).file_name().unwrap().to_string_lossy().into(), score(&b, th, &ex, &ey));
    }
    let (f, prec, rec) = f_at(0.5, &all_p, &all_y);
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    Ok(serde_json::json!({"generated_pairs": pairs.len(), "patches": y.len(), "held_out_by_pair": {"f": r3(f), "precision": r3(prec), "recall": r3(rec), "folds": folds},
        "trained_on_all_generated_scored_on_real_renders": real}))
}

/// Train on one pair, score on the other, both ways.
pub fn run(pairs: &[(String, String)], budget: f32) -> Result<serde_json::Value, String> {
    let data: Vec<(Vec<Vec<f64>>, Vec<f64>)> = pairs.iter().map(|(f, b)| patches(f, b, 2)).collect::<Result<_, _>>()?;
    let mut out = serde_json::Map::new();
    for (i, (f, _)) in pairs.iter().enumerate() {
        let (xt, yt) = &data[i];
        // train on every other pair
        let (mut x, mut y) = (Vec::new(), Vec::new());
        for (j, (xx, yy)) in data.iter().enumerate() {
            if j != i {
                x.extend(xx.iter().cloned());
                y.extend(yy.iter().cloned());
            }
        }
        let cols = x[0].len();
        let flat: Vec<f64> = (0..cols).flat_map(|c| x.iter().map(move |r| r[c])).collect();
        let mut b = PerpetualBooster::default().set_objective(Objective::LogLoss).set_budget(budget).set_seed(0).set_num_threads(Some(2));
        b.fit(&Matrix::new(&flat, x.len(), cols), &y, None, None).map_err(|e| format!("{e:?}"))?;
        // overlays are 1-2% of patches: the threshold is the one with the best F on the training renders (never the test)
        let ptr = b.predict_proba(&Matrix::new(&flat, x.len(), cols), false, false);
        let f_at = |th: f64, pp: &[f64], yy: &[f64]| {
            let (tp, fp, fnn) = (0..yy.len()).fold((0usize, 0usize, 0usize), |(a, b2, c), k| { let (pr, tr) = (pp[k] > th, yy[k] > 0.5); (a + (pr && tr) as usize, b2 + (pr && !tr) as usize, c + (!pr && tr) as usize) });
            2.0 * tp as f64 / (2 * tp + fp + fnn).max(1) as f64
        };
        let th = (1..100).map(|k| k as f64 / 100.0).max_by(|a, c| f_at(*a, &ptr, &y).total_cmp(&f_at(*c, &ptr, &y))).unwrap_or(0.5);
        let tflat: Vec<f64> = (0..cols).flat_map(|c| xt.iter().map(move |r| r[c])).collect();
        let p = b.predict_proba(&Matrix::new(&tflat, xt.len(), cols), false, false);
        let (tp, fp, fnn) = (0..yt.len()).fold((0, 0, 0), |(a, b2, c), k| {
            let (pred, truth) = (p[k] > th, yt[k] > 0.5);
            (a + (pred && truth) as usize, b2 + (pred && !truth) as usize, c + (!pred && truth) as usize)
        });
        let (prec, rec) = (tp as f64 / (tp + fp).max(1) as f64, tp as f64 / (tp + fnn).max(1) as f64);
        let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
        out.insert(std::path::Path::new(f).file_name().unwrap().to_string_lossy().into(), serde_json::json!({
            "test_patches": yt.len(), "overlay_patches": yt.iter().filter(|v| **v > 0.5).count(), "trained_on_patches": y.len(),
            "threshold_from_training": th, "auc": r3(auc(yt, &p)), "precision": r3(prec), "recall": r3(rec), "f": r3(2.0 * prec * rec / (prec + rec).max(1e-9))}));
    }
    Ok(serde_json::Value::Object(out))
}
