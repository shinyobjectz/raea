//! When the references don't hold together (docs/videoform.feature VF8b): the references in groups across all the
//! measures at once, not one measure at a time (ranges.rs says that), so two kinds of video read as one are told
//! apart and offered as a videoform each, never averaged. Each measure is set on one scale (its median and spread
//! across the references), the references split in two by their nearest centre, and the split kept when it's clear:
//! each reference much nearer its own group than the other (the mean silhouette). How clear it is is said with it.

use crate::ranges::Measures;
use serde::{Deserialize, Serialize};

/// A split is said when the references sit this much nearer their own group than the other, on average (silhouette)
const CLEAR: f64 = 0.5;
/// What two groups differ on: measures whose groups' means are this many of their own spreads apart (Cohen's d)
const APART: f64 = 2.0;

/// Two groups of references, how clearly they're apart, and what they differ on most
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sets {
    pub groups: [Vec<usize>; 2],
    /// the mean silhouette, 0 to 1
    pub clear: f64,
    /// each measure they differ on, most first, with each group's median
    pub on: Vec<(String, f64, f64)>,
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    if v.is_empty() { 0.0 } else { (v[(v.len() - 1) / 2] + v[v.len() / 2]) / 2.0 }
}

fn dist(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).fold(0.0, |acc, (x, y)| acc + (x - y) * (x - y)).sqrt()
}

/// A group's mean on a measure and its spread's square, the sample's
fn spread(v: &[f64]) -> (f64, f64) {
    let m = v.iter().sum::<f64>() / v.len().max(1) as f64;
    (m, v.iter().fold(0.0, |acc, x| acc + (x - m) * (x - m)) / (v.len() - 1).max(1) as f64)
}

/// The references in two clear groups across the measures they all have, when they fall in two
pub fn sets(refs: &[Measures]) -> Option<Sets> {
    let n = refs.len();
    if n < 4 {
        return None;
    }
    // the measures every reference has, each on its own scale (a spread of 0 says nothing)
    let keys: Vec<&String> = refs[0].keys().filter(|k| !["loudness", "true_peak"].contains(&k.as_str()) && refs.iter().all(|r| r.get(*k).is_some_and(|v| v.is_finite()))).collect();
    let mut scaled: Vec<Vec<f64>> = vec![Vec::new(); n];
    let mut kept: Vec<(&String, f64, f64)> = Vec::new();
    for k in keys {
        let mut v: Vec<f64> = refs.iter().map(|r| r[k]).collect();
        let m = median(&mut v.clone());
        let mut dev: Vec<f64> = v.iter().map(|x| (x - m).abs()).collect();
        let s = median(&mut dev) * 1.4826;
        if s <= 1e-9 {
            continue;
        }
        v.iter_mut().for_each(|x| *x = (*x - m) / s);
        scaled.iter_mut().zip(&v).for_each(|(row, x)| row.push(x.clamp(-6.0, 6.0)));
        kept.push((k, m, s));
    }
    if kept.len() < 2 {
        return None;
    }
    // two centres, started at the two references farthest apart, moved to their groups' means
    let (mut a, mut b) = (0, 1);
    for i in 0..n {
        for j in i + 1..n {
            if dist(&scaled[i], &scaled[j]) > dist(&scaled[a], &scaled[b]) {
                (a, b) = (i, j);
            }
        }
    }
    let mut c = [scaled[a].clone(), scaled[b].clone()];
    let mut of = vec![0usize; n];
    for _ in 0..20 {
        of = scaled.iter().map(|x| usize::from(dist(x, &c[1]) < dist(x, &c[0]))).collect();
        for (g, centre) in c.iter_mut().enumerate() {
            let members: Vec<&Vec<f64>> = scaled.iter().zip(&of).filter(|(_, o)| **o == g).map(|x| x.0).collect();
            if !members.is_empty() {
                *centre = (0..kept.len()).map(|d| members.iter().map(|m| m[d]).sum::<f64>() / members.len() as f64).collect();
            }
        }
    }
    let groups = [0, 1].map(|g| (0..n).filter(|&i| of[i] == g).collect::<Vec<_>>());
    if groups.iter().any(|g| g.len() < 2) {
        return None;
    }
    let mean_to = |i: usize, g: usize| groups[g].iter().filter(|&&j| j != i).map(|&j| dist(&scaled[i], &scaled[j])).sum::<f64>() / (groups[g].len() - usize::from(of[i] == g)).max(1) as f64;
    let clear = (0..n).map(|i| {
        let (own, other) = (mean_to(i, of[i]), mean_to(i, 1 - of[i]));
        (other - own) / own.max(other).max(1e-9)
    }).sum::<f64>() / n as f64;
    if clear < CLEAR {
        return None;
    }
    let mut on: Vec<(String, f64, f64, f64)> = kept.iter().map(|(k, _, _)| {
        let v = |g: usize| groups[g].iter().map(|&i| refs[i][*k]).collect::<Vec<f64>>();
        let (a, b) = (v(0), v(1));
        let ((ma, va), (mb, vb)) = (spread(&a), spread(&b));
        let pooled = ((va * (a.len() - 1) as f64 + vb * (b.len() - 1) as f64) / (a.len() + b.len() - 2).max(1) as f64).sqrt();
        let d = (ma - mb).abs() / pooled.max(1e-9 * ma.abs().max(1.0));
        ((*k).clone(), median(&mut a.clone()), median(&mut b.clone()), d)
    }).filter(|x| x.3 >= APART).collect();
    on.sort_by(|p, q| q.3.total_cmp(&p.3));
    let mut said = Vec::new();
    for (k, a, b, _) in on {
        said.push((k, a, b));
    }
    Some(Sets { groups, clear: (clear * 100.0).round() / 100.0, on: said })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(cuts: f64, shot: f64, words: f64) -> Measures {
        [("cuts_per_min", cuts), ("median_shot", shot), ("words_per_min", words)].into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    #[test]
    fn fast_and_slow_cutting_are_two_videoforms() {
        let refs = [m(48.0, 1.1, 170.0), m(50.0, 1.0, 165.0), m(46.0, 1.2, 172.0), m(22.0, 2.6, 120.0), m(20.0, 2.8, 118.0), m(24.0, 2.5, 125.0)];
        let s = sets(&refs).expect("two clear groups");
        assert_eq!(s.groups, [vec![0, 1, 2], vec![3, 4, 5]]);
        assert!(s.on.iter().any(|x| x.0 == "cuts_per_min"));
    }

    #[test]
    fn one_kind_of_video_isnt_split() {
        let refs = [m(40.0, 1.5, 150.0), m(42.0, 1.4, 155.0), m(38.0, 1.6, 148.0), m(41.0, 1.5, 152.0), m(39.0, 1.45, 151.0)];
        assert_eq!(sets(&refs), None);
    }
}
