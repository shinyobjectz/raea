//! Brief checks as volumes (docs/media.feature M4): a row's box over the edit's time is a solid in (x, y, frame), so
//! "inside the frame", "clear of the face" and "no two overlap" are one boolean each over the whole edit, not a test per
//! frame. Each frame's box is a one-frame prism (runs of identical frames merge into one), so a solid answers exactly
//! what the per-frame check would; the failing frames are read back from the result's slices.

use std::collections::BTreeMap;

use manifold_rust::linalg::Vec3;
use manifold_rust::manifold::Manifold;

use crate::eval::Tup;
use crate::ir::{Program, V};

fn num(t: &Tup, k: &str) -> Option<f64> {
    match t.vals.get(k) {
        Some(V::N(x)) => Some(*x),
        _ => None,
    }
}

/// A box per frame: frame -> [x0, y0, x1, y1].
pub type Track = BTreeMap<i64, [f64; 4]>;

/// The solid of a track: one prism per run of frames with the same box.
pub fn solid(track: &Track) -> Manifold {
    let mut prisms: Vec<Manifold> = Vec::new();
    let mut run: Option<(i64, i64, [f64; 4])> = None;
    let mut flush = |r: Option<(i64, i64, [f64; 4])>, prisms: &mut Vec<Manifold>| {
        if let Some((f0, f1, b)) = r {
            let (w, h) = (b[2] - b[0], b[3] - b[1]);
            if w > 0.0 && h > 0.0 {
                prisms.push(Manifold::cube(Vec3::new(w, h, (f1 - f0 + 1) as f64), false).translate(Vec3::new(b[0], b[1], f0 as f64)));
            }
        }
    };
    for (&f, b) in track {
        run = match run {
            Some((f0, f1, rb)) if f == f1 + 1 && rb == *b => Some((f0, f, rb)),
            r => {
                flush(r, &mut prisms);
                Some((f, f, *b))
            }
        };
    }
    flush(run, &mut prisms);
    if prisms.is_empty() {
        return Manifold::default();
    }
    Manifold::batch_boolean(&prisms, manifold_rust::types::OpType::Add)
}

/// The frames a solid occupies (its slice through each frame's middle has area).
pub fn frames(m: &Manifold) -> Vec<i64> {
    if m.is_empty() {
        return Vec::new();
    }
    let b = m.bounding_box();
    ((b.min.z.floor() as i64)..=(b.max.z.ceil() as i64)).filter(|f| m.slice(*f as f64 + 0.5).area() > 1e-9).collect()
}

/// Every frame's rows, through the timeline (rules, keys, transitions applied), as rdb's checks see them.
pub fn states(prog: &Program, rows: &[Tup]) -> Result<Vec<Vec<Tup>>, String> {
    let tl = crate::timeline::Timeline::new(prog, rows)?;
    Ok((0..prog.frames).map(|f| tl.tups(f)).collect())
}

/// Each row of `kind`'s track over the frames, its box from `bx` (None: not there that frame).
pub fn tracks(states: &[Vec<Tup>], kind: &str, bx: &dyn Fn(&Tup) -> Option<[f64; 4]>) -> BTreeMap<String, Track> {
    let mut out: BTreeMap<String, Track> = BTreeMap::new();
    for (f, st) in states.iter().enumerate() {
        for t in st.iter().filter(|t| t.kind == kind) {
            let live = num(t, "from").is_none_or(|a| f as f64 >= a) && num(t, "to").is_none_or(|b| (f as f64) < b);
            if let (true, Some(b)) = (live, bx(t)) {
                out.entry(t.id.clone()).or_default().insert(f as i64, b);
            }
        }
    }
    out
}

/// The failing frames of each track against a region it must stay inside: frames where its solid minus the region
/// has volume.
pub fn outside(tracks: &BTreeMap<String, Track>, region: &Manifold) -> BTreeMap<String, Vec<i64>> {
    tracks.iter().map(|(id, t)| (id.clone(), frames(&solid(t).difference(region)))).filter(|(_, f)| !f.is_empty()).collect()
}

/// The failing frames of each track against a region it must keep out of: frames where they intersect.
pub fn into(tracks: &BTreeMap<String, Track>, region: &Manifold) -> BTreeMap<String, Vec<i64>> {
    tracks.iter().map(|(id, t)| (id.clone(), frames(&solid(t).intersection(region)))).filter(|(_, f)| !f.is_empty()).collect()
}

/// Pairs that overlap: each row's failing frames are where its solid meets another's.
pub fn overlaps(tracks: &BTreeMap<String, Track>) -> BTreeMap<String, Vec<i64>> {
    let solids: Vec<(&String, Manifold)> = tracks.iter().map(|(id, t)| (id, solid(t))).collect();
    let mut out: BTreeMap<String, std::collections::BTreeSet<i64>> = BTreeMap::new();
    for i in 0..solids.len() {
        for j in i + 1..solids.len() {
            let hit = frames(&solids[i].1.intersection(&solids[j].1));
            if !hit.is_empty() {
                out.entry(solids[i].0.clone()).or_default().extend(hit.iter());
                out.entry(solids[j].0.clone()).or_default().extend(hit.iter());
            }
        }
    }
    out.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect()
}

/// The same pairs, frame by frame (the per-frame reference the volume answer is checked against).
pub fn overlaps_per_frame(tracks: &BTreeMap<String, Track>) -> BTreeMap<String, Vec<i64>> {
    let mut out: BTreeMap<String, std::collections::BTreeSet<i64>> = BTreeMap::new();
    let ids: Vec<&String> = tracks.keys().collect();
    for i in 0..ids.len() {
        for j in i + 1..ids.len() {
            for (f, a) in &tracks[ids[i]] {
                if let Some(b) = tracks[ids[j]].get(f) {
                    if a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3] {
                        out.entry(ids[i].clone()).or_default().insert(*f);
                        out.entry(ids[j].clone()).or_default().insert(*f);
                    }
                }
            }
        }
    }
    out.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect()
}

/// A slab of the picture's x over frames f0..f1 (end exclusive) between y0 and y1, wide enough for any box.
pub fn band(y0: f64, y1: f64, f0: i64, f1: i64) -> Manifold {
    let big = 1.0e6;
    Manifold::cube(Vec3::new(2.0 * big, y1 - y0, (f1 - f0) as f64), false).translate(Vec3::new(-big, y0, f0 as f64))
}

/// The zone below a line that moves per frame (frame -> y), down to `bottom`: one prism per run of frames with the same
/// line; frames not in `line` add nothing.
pub fn below(line: &BTreeMap<i64, f64>, bottom: f64) -> Manifold {
    let mut prisms = Vec::new();
    let mut run: Option<(i64, i64, f64)> = None;
    let flush = |r: Option<(i64, i64, f64)>, prisms: &mut Vec<Manifold>| {
        if let Some((f0, f1, y)) = r {
            if bottom > y {
                prisms.push(band(y, bottom, f0, f1 + 1));
            }
        }
    };
    for (&f, &y) in line {
        run = match run {
            Some((f0, f1, ry)) if f == f1 + 1 && ry == y => Some((f0, f, ry)),
            r => {
                flush(r, &mut prisms);
                Some((f, f, y))
            }
        };
    }
    flush(run, &mut prisms);
    if prisms.is_empty() {
        return Manifold::default();
    }
    Manifold::batch_boolean(&prisms, manifold_rust::types::OpType::Add)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(boxes: &[(i64, [f64; 4])]) -> Track {
        boxes.iter().copied().collect()
    }

    #[test]
    fn frames_come_back_exactly() {
        // a box sliding down 10 px a frame over frames 0..20 out of a 0..100 band from frame 8 (y1 = 30 + 10 f > 100)
        let t: Track = (0..20).map(|f| (f, [0.0, 10.0 * f as f64, 50.0, 30.0 + 10.0 * f as f64])).collect();
        let tr: BTreeMap<String, Track> = [("cap".to_string(), t)].into();
        let out = outside(&tr, &band(0.0, 100.0, 0, 20));
        assert_eq!(out["cap"], (8..20).collect::<Vec<_>>());
        // touching the edge is not outside
        let edge: BTreeMap<String, Track> = [("e".to_string(), track(&[(3, [0.0, 70.0, 10.0, 100.0])]))].into();
        assert!(outside(&edge, &band(0.0, 100.0, 0, 20)).is_empty());
    }

    #[test]
    fn overlaps_match_frame_by_frame() {
        let a: Track = (0..30).map(|f| (f, [f as f64 * 5.0, 0.0, f as f64 * 5.0 + 40.0, 20.0])).collect();
        let b: Track = (10..40).map(|f| (f, [100.0, 0.0, 140.0, 20.0])).collect();
        let c: Track = (0..40).map(|f| (f, [0.0, 50.0, 40.0, 70.0])).collect();
        let tr: BTreeMap<String, Track> = [("a".into(), a), ("b".into(), b), ("c".into(), c)].into();
        let v = overlaps(&tr);
        assert_eq!(v, overlaps_per_frame(&tr));
        // a reaches b at frame 13 and only touches it at 28 (x 140 = its right edge)
        assert_eq!(v["a"], (13..28).collect::<Vec<_>>());
        assert!(!v.contains_key("c"));
    }
}
