//! The animation principles that can be read off an edit's frames (the roadmap G's catalogue): every numeric column of
//! every overlay row (titles, captions, shapes, anything not a clip or a sound) followed through the frames, split into
//! moves, and each move measured: how long it takes (timing), whether it starts and ends slower than it travels
//! (easing), whether it passes its target and settles back (follow-through), whether it first goes the other way
//! (anticipation). Staging, arcs, appeal and silhouette are a viewer's call and aren't measured here.

use std::collections::BTreeMap;

use crate::eval::Tup;
use crate::ir::{Program, V};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Move {
    pub row: String,
    pub col: String,
    pub from: usize,
    pub frames: usize,
    pub eased: bool,
    pub overshoot: bool,
    pub anticipation: bool,
    /// a looping or oscillating stretch (more than two reversals), not a move: the principles above don't apply
    pub looping: bool,
}

/// The moves in one column's values (one per frame, None where the row isn't showing).
pub fn moves_in(row: &str, col: &str, v: &[Option<f64>]) -> Vec<Move> {
    let mut out = Vec::new();
    let mut f = 1;
    while f < v.len() {
        let moving = |i: usize| matches!((v[i - 1], v[i]), (Some(a), Some(b)) if (b - a).abs() > 1e-6);
        if !moving(f) {
            f += 1;
            continue;
        }
        let start = f - 1;
        while f < v.len() && moving(f) {
            f += 1;
        }
        let end = f - 1;
        let seg: Vec<f64> = v[start..=end].iter().map(|x| x.unwrap_or(0.0)).collect();
        let (a, b) = (seg[0], *seg.last().unwrap());
        let range = seg.iter().cloned().fold(f64::MIN, f64::max) - seg.iter().cloned().fold(f64::MAX, f64::min);
        let steps: Vec<f64> = seg.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        let k = (steps.len() / 5).max(1);
        let mid = steps[steps.len() / 2 - steps.len().min(2) / 2..].iter().take(k.max(1)).cloned().fold(0.0, f64::max);
        let ends = steps[..k].iter().chain(steps[steps.len() - k..].iter()).cloned().fold(0.0, f64::max);
        let dir = (b - a).signum();
        let eased = steps.len() >= 4 && ends < 0.6 * mid;
        let overshoot = dir != 0.0 && seg.iter().any(|x| (x - b) * dir > 0.02 * range.max(1e-9));
        let anticipation = dir != 0.0 && seg.iter().take((seg.len() / 3).max(1)).any(|x| (x - a) * dir < -0.02 * range.max(1e-9));
        let d: Vec<f64> = seg.windows(2).map(|w| w[1] - w[0]).filter(|x| x.abs() > 1e-9).collect();
        let reversals = d.windows(2).filter(|w| w[0].signum() != w[1].signum()).count();
        let looping = reversals > 2;
        out.push(Move { row: row.into(), col: col.into(), from: start, frames: end - start, eased: eased && !looping, overshoot: overshoot && !looping,
                        anticipation: anticipation && !looping, looping });
    }
    out
}

/// Every move of every overlay row in the edit's frame states.
pub fn moves(prog: &Program, states: &[Vec<Tup>]) -> Vec<Move> {
    let skip_kinds = ["clip", "audio", "media", "sound", "camera", "light", "edit"];
    let skip_cols = ["from", "to", "z", "size", "font", "weight", "text", "src"];
    let mut series: BTreeMap<(String, String), Vec<Option<f64>>> = BTreeMap::new();
    for (f, st) in states.iter().enumerate() {
        for t in st.iter().filter(|t| !skip_kinds.contains(&t.kind.as_str()) && crate::previz::showing(t, f as i64)) {
            for (c, v) in &t.vals {
                if skip_cols.contains(&c.as_str()) {
                    continue;
                }
                if let V::N(x) = v {
                    let s = series.entry((t.id.clone(), c.clone())).or_insert_with(|| vec![None; states.len()]);
                    s[f] = Some(*x);
                }
            }
        }
    }
    let _ = prog;
    series.iter().flat_map(|((r, c), v)| moves_in(r, c, v)).collect()
}

/// The moves summed up.
pub fn report(prog: &Program, states: &[Vec<Tup>]) -> serde_json::Value {
    let all = moves(prog, states);
    let loops = all.iter().filter(|x| x.looping).count();
    let m: Vec<Move> = all.into_iter().filter(|x| !x.looping).collect();
    let mut durs: Vec<f64> = m.iter().map(|x| x.frames as f64 / prog.fps).collect();
    durs.sort_by(f64::total_cmp);
    let n = m.len().max(1) as f64;
    serde_json::json!({
        "moves": m.len(),
        "loops": loops,
        "rows_animated": m.iter().map(|x| x.row.clone()).collect::<std::collections::BTreeSet<_>>().len(),
        "duration_s": {"median": durs.get(durs.len() / 2).map(|d| (d * 100.0).round() / 100.0), "shortest": durs.first().map(|d| (d * 100.0).round() / 100.0)},
        "eased_share": (m.iter().filter(|x| x.eased).count() as f64 / n * 100.0).round() / 100.0,
        "overshoot_share": (m.iter().filter(|x| x.overshoot).count() as f64 / n * 100.0).round() / 100.0,
        "anticipation_share": (m.iter().filter(|x| x.anticipation).count() as f64 / n * 100.0).round() / 100.0,
        "too_fast": m.iter().filter(|x| x.frames < 4).count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ease(t: f64) -> f64 {
        t * t * (3.0 - 2.0 * t)
    }

    #[test]
    fn an_eased_move_reads_as_eased() {
        let v: Vec<Option<f64>> = (0..30).map(|i| Some(if i < 5 { 0.0 } else if i > 25 { 100.0 } else { 100.0 * ease((i - 5) as f64 / 20.0) })).collect();
        let m = moves_in("t", "x", &v);
        assert_eq!(m.len(), 1);
        assert!(m[0].eased && !m[0].overshoot && !m[0].anticipation);
    }

    #[test]
    fn a_linear_move_isnt_eased() {
        let v: Vec<Option<f64>> = (0..30).map(|i| Some(if i < 5 { 0.0 } else if i > 25 { 100.0 } else { 5.0 * (i - 5) as f64 })).collect();
        let m = moves_in("t", "x", &v);
        assert!(!m[0].eased);
    }

    #[test]
    fn overshoot_and_anticipation_are_seen() {
        // back a little, then past the target, then settle
        let path = [0.0, -5.0, -8.0, 10.0, 40.0, 80.0, 110.0, 115.0, 108.0, 102.0, 100.0];
        let v: Vec<Option<f64>> = path.iter().map(|x| Some(*x)).collect();
        let m = moves_in("t", "y", &v);
        assert!(m[0].overshoot && m[0].anticipation);
    }
}
