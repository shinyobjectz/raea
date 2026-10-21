//! Framing, measured on a render's pictures (sdk/motion/bench/legible/framing.sh): where the subject is in each sampled
//! frame, and per shot whether the camera keeps it. The faults an editor would call: a subject cut by the picture's
//! top or bottom edge (a bust's head out of frame, the chest filling it), a push-in that ends on the body below the
//! face it began on, and a camera that drifts off its subject without a cut.
//!
//! The subject is what Vision's attention lands on (its saliency map, the blob of cells at half the peak or more
//! around the peak) grown to the thing it lands on: the lit pixels connected to that blob (a marble bust in a dark
//! room, a cat on a floor). Faces count where Vision finds them (it finds some statues' faces, not all). Everything
//! is in the picture's own coordinates, 0..1 inside the letterbox (legible.rs › bars), y from the top.

use crate::layout::Rect;
use crate::legible::luminance;
use serde_json::{json, Value};

/// One sampled frame's subject.
#[derive(Clone, Debug, Default)]
pub struct Look {
    /// seconds into the film
    pub t: f64,
    /// where attention lands: its blob around the peak (picture coordinates)
    pub focus: Option<Rect>,
    /// the attention-weighted centre of the blob
    pub centre: Option<(f64, f64)>,
    /// the thing attention lands on: lit pixels connected to the focus
    pub subject: Option<Rect>,
    /// the share of the picture's salient cells (at half the peak or more) that are in the focus blob: near 1 when
    /// attention has one place to go, low when it's spread over the frame
    pub single: f64,
    /// the share of the salient blob's pixels that are lit: low for a "focus" over black (a title, embers)
    pub fill: f64,
    /// faces Vision finds, in picture coordinates
    pub faces: Vec<Rect>,
    /// the picture's mean luminance on a 16 x 9 grid: consecutive looks far apart are a cut (or a dip through black)
    pub sig: Vec<f64>,
}

impl Look {
    /// A subject worth framing: one compact place for the eye, on something lit.
    pub fn has_subject(&self) -> bool {
        self.subject.is_some() && self.single >= 0.6 && self.fill >= 0.3
    }

    pub fn json(&self) -> Value {
        let r = |r: &Option<Rect>| r.as_ref().map(|r| json!([r3(r.x), r3(r.y), r3(r.w), r3(r.h)]));
        json!({
            "t": r3(self.t), "focus": r(&self.focus), "subject": r(&self.subject),
            "centre": self.centre.map(|c| json!([r3(c.0), r3(c.1)])), "single": r3(self.single), "fill": r3(self.fill),
            "faces": self.faces.iter().map(|f| json!([r3(f.x), r3(f.y), r3(f.w), r3(f.h)])).collect::<Vec<_>>(),
            "has_subject": self.has_subject(), "dark": r3(self.sig.iter().sum::<f64>() / self.sig.len().max(1) as f64),
        })
    }
}

fn r3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// The grid the lit silhouette is found on (cells of the picture).
const GW: usize = 96;
const GH: usize = 54;

/// Measure one frame. `rgba` is the whole frame `w` x `h`; `att` Vision's attention map of it (rows from the top,
/// covering the whole frame); `faces` Vision's faces (0..1 of the whole frame, from the top); `bars` the letterbox
/// (top, bottom) as shares of the height.
pub fn look(t: f64, rgba: &[u8], w: usize, h: usize, att: Option<&(Vec<f32>, usize, usize)>, faces: &[Rect], bars: (f64, f64)) -> Look {
    let (top, bottom) = bars;
    let ph = (1.0 - top - bottom).max(1e-6);
    // frame y (0..1) to picture y
    let py = |y: f64| (y - top) / ph;
    let faces: Vec<Rect> = faces
        .iter()
        .filter(|f| f.y + f.h > top && f.y < 1.0 - bottom)
        .map(|f| Rect { x: f.x, y: py(f.y), w: f.w, h: f.h / ph })
        .collect();
    let mut out = Look { t, faces, ..Default::default() };
    // the picture's luminance on a GW x GH grid: the lit silhouette below is found on it, and `sig` sums it up
    let p0 = (top * h as f64).round() as usize;
    let p1 = ((1.0 - bottom) * h as f64).round() as usize;
    let pic_h = p1.saturating_sub(p0).max(1);
    let mut cell = vec![0.0f64; GW * GH];
    let mut count = vec![0usize; GW * GH];
    let mut lums = Vec::with_capacity(w * pic_h / 4);
    for y in (p0..p1).step_by(2) {
        for x in (0..w).step_by(2) {
            let i = (y * w + x) * 4;
            let l = luminance([rgba[i], rgba[i + 1], rgba[i + 2]]);
            lums.push(l);
            let (gx, gy) = (x * GW / w, (y - p0) * GH / pic_h);
            cell[gy * GW + gx] += l;
            count[gy * GW + gx] += 1;
        }
    }
    out.sig = (0..9)
        .flat_map(|sy| (0..16).map(move |sx| (sx, sy)))
        .map(|(sx, sy)| {
            let (mut v, mut n) = (0.0, 0usize);
            for gy in sy * GH / 9..(sy + 1) * GH / 9 {
                for gx in sx * GW / 16..(sx + 1) * GW / 16 {
                    v += cell[gy * GW + gx];
                    n += count[gy * GW + gx];
                }
            }
            v / n.max(1) as f64
        })
        .collect();
    let Some((m, mw, mh)) = att else { return out };
    let (mw, mh) = (*mw, *mh);
    // the map's cells inside the picture
    let (r0, r1) = (((top * mh as f64).ceil() as usize).min(mh), (((1.0 - bottom) * mh as f64).floor() as usize).min(mh));
    if r1 <= r0 {
        return out;
    }
    let mut peak = (0.0f32, 0usize, 0usize);
    for y in r0..r1 {
        for x in 0..mw {
            if m[y * mw + x] > peak.0 {
                peak = (m[y * mw + x], x, y);
            }
        }
    }
    if peak.0 <= 0.0 {
        return out;
    }
    let hot = |x: usize, y: usize| y >= r0 && y < r1 && m[y * mw + x] >= peak.0 * 0.5;
    let blob = flood(mw, mh, (peak.1, peak.2), &hot);
    let all_hot = (r0..r1).flat_map(|y| (0..mw).map(move |x| (x, y))).filter(|&(x, y)| hot(x, y)).count();
    out.single = blob.len() as f64 / all_hot.max(1) as f64;
    let (mut x0, mut y0, mut x1, mut y1) = (mw, mh, 0, 0);
    let (mut sx, mut sy, mut sw) = (0.0, 0.0, 0.0);
    for &(x, y) in &blob {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + 1);
        y1 = y1.max(y + 1);
        let v = m[y * mw + x] as f64;
        sx += v * (x as f64 + 0.5);
        sy += v * (y as f64 + 0.5);
        sw += v;
    }
    let fy = |cy: f64| py(cy / mh as f64);
    out.focus = Some(Rect { x: x0 as f64 / mw as f64, y: fy(y0 as f64), w: (x1 - x0) as f64 / mw as f64, h: (y1 - y0) as f64 / mh as f64 / ph });
    out.centre = Some((sx / sw / mw as f64, fy(sy / sw)));

    lums.sort_by(f64::total_cmp);
    let hi = lums.get(lums.len() * 999 / 1000).copied().unwrap_or(0.0);
    if hi <= 0.0 {
        return out;
    }
    let lit = |x: usize, y: usize| count[y * GW + x] > 0 && cell[y * GW + x] / count[y * GW + x] as f64 >= hi * 0.25;
    // the blob's cells on the grid
    let f = out.focus.unwrap();
    let in_focus = |x: usize, y: usize| {
        let (cx, cy) = ((x as f64 + 0.5) / GW as f64, (y as f64 + 0.5) / GH as f64);
        cx >= f.x && cx <= f.x + f.w && cy >= f.y && cy <= f.y + f.h
    };
    let seeds: Vec<(usize, usize)> = (0..GH).flat_map(|y| (0..GW).map(move |x| (x, y))).filter(|&(x, y)| in_focus(x, y)).collect();
    out.fill = seeds.iter().filter(|&&(x, y)| lit(x, y)).count() as f64 / seeds.len().max(1) as f64;
    let mut seen = vec![false; GW * GH];
    let mut sil: Vec<(usize, usize)> = Vec::new();
    for &(x, y) in &seeds {
        if lit(x, y) && !seen[y * GW + x] {
            for c in flood(GW, GH, (x, y), &lit) {
                if !seen[c.1 * GW + c.0] {
                    seen[c.1 * GW + c.0] = true;
                    sil.push(c);
                }
            }
        }
    }
    if !sil.is_empty() {
        let (a0, b0) = (sil.iter().map(|c| c.0).min().unwrap(), sil.iter().map(|c| c.1).min().unwrap());
        let (a1, b1) = (sil.iter().map(|c| c.0).max().unwrap() + 1, sil.iter().map(|c| c.1).max().unwrap() + 1);
        out.subject = Some(Rect { x: a0 as f64 / GW as f64, y: b0 as f64 / GH as f64, w: (a1 - a0) as f64 / GW as f64, h: (b1 - b0) as f64 / GH as f64 });
    }
    out
}

/// The 4-connected cells from `start` where `on` holds.
fn flood(w: usize, h: usize, start: (usize, usize), on: &dyn Fn(usize, usize) -> bool) -> Vec<(usize, usize)> {
    if !on(start.0, start.1) {
        return Vec::new();
    }
    let mut seen = vec![false; w * h];
    let mut stack = vec![start];
    let mut out = Vec::new();
    seen[start.1 * w + start.0] = true;
    while let Some((x, y)) = stack.pop() {
        out.push((x, y));
        let mut go = |nx: usize, ny: usize| {
            if !seen[ny * w + nx] && on(nx, ny) {
                seen[ny * w + nx] = true;
                stack.push((nx, ny));
            }
        };
        if x > 0 {
            go(x - 1, y);
        }
        if x + 1 < w {
            go(x + 1, y);
        }
        if y > 0 {
            go(x, y - 1);
        }
        if y + 1 < h {
            go(x, y + 1);
        }
    }
    out
}

/// A dark sample (a dip through black, a title over black): no shot.
const DARK: f64 = 0.004;

/// How far apart two looks' pictures are: the mean difference of their 16 x 9 luminance grids, relative to the
/// brighter one's mean. Within a moving shot sampled every half second it stays under ~0.25; a cut is far over it.
pub fn distance(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let n = a.len() as f64;
    let d = a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f64>() / n;
    let m = (a.iter().sum::<f64>() / n).max(b.iter().sum::<f64>() / n).max(0.01);
    d / m
}

/// The looks split into shots: (first, last) indices, at `cuts` (seconds), at pictures far apart (`distance` over
/// 0.45), and around dark samples, which belong to no shot.
pub fn shots(looks: &[Look], cuts: &[f64]) -> Vec<(usize, usize)> {
    let dark = |l: &Look| l.sig.iter().sum::<f64>() / l.sig.len().max(1) as f64 <= DARK;
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..looks.len() {
        if dark(&looks[i]) {
            if let Some(s) = start.take() {
                out.push((s, i - 1));
            }
            continue;
        }
        if let Some(s) = start {
            let (a, b) = (looks[i - 1].t, looks[i].t);
            if cuts.iter().any(|c| *c > a && *c <= b) || distance(&looks[i - 1].sig, &looks[i].sig) > 0.45 {
                out.push((s, i - 1));
                start = Some(i);
            }
        } else {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push((s, looks.len() - 1));
    }
    out
}

/// The subject's top runs out of the picture: a compact lit thing (not a frame-wide wash, not a thin column) whose
/// silhouette touches the picture's top edge, with the eye drawn to its upper half (`chest` = false) or, with no face
/// found, landing low on it while its top is out of frame (`chest` = true: the camera sits on the body, not the head).
pub fn top_cut(l: &Look) -> Option<bool> {
    let (s, c) = (l.subject?, l.centre?);
    if !l.has_subject() || s.y > 0.005 || s.h < 0.45 || s.w < 0.12 || s.w > 0.6 {
        return None;
    }
    if c.1 >= 0.55 && l.faces.is_empty() {
        Some(true)
    } else if c.1 <= 0.5 {
        Some(false)
    } else {
        None
    }
}

/// How much of `b` lies inside `a`, as a share of `b`'s area.
fn overlap(a: &Rect, b: &Rect) -> f64 {
    let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    if w <= 0.0 || h <= 0.0 { 0.0 } else { w * h / (b.w * b.h).max(1e-9) }
}

/// The framing faults of a film's looks, sampled every `every` s, split into shots at `cuts` (seconds). Each is
/// {from, to, kind, said}: `kind` one of "top-cut", "push-low", "lost", "aimless", and the note "no-subject".
pub fn faults(looks: &[Look], cuts: &[f64], every: f64) -> Vec<Value> {
    let mut out = Vec::new();
    let clock = |t: f64| format!("{}:{:04.1}", (t / 60.0) as u32, t % 60.0);
    for (a, b) in shots(looks, cuts) {
        let shot = &looks[a..=b];
        let span = |i: usize, j: usize| (shot[i].t, shot[j].t + every);
        // runs of samples where `f` holds, at least `min` seconds long
        let runs = |f: &dyn Fn(usize) -> bool, min: f64| -> Vec<(usize, usize)> {
            let mut r = Vec::new();
            let mut s: Option<usize> = None;
            for i in 0..=shot.len() {
                let on = i < shot.len() && f(i);
                match (on, s) {
                    (true, None) => s = Some(i),
                    (false, Some(k)) => {
                        if (i - k) as f64 * every >= min {
                            r.push((k, i - 1));
                        }
                        s = None;
                    }
                    _ => {}
                }
            }
            r
        };
        for (i, j) in runs(&|i| top_cut(&shot[i]).is_some(), 1.0) {
            let (f, t) = span(i, j);
            let low = shot[i..=j].iter().filter(|l| top_cut(l) == Some(true)).count() * 2 > j - i + 1;
            let cy = shot[i..=j].iter().filter_map(|l| l.centre).map(|c| c.1).sum::<f64>() / (j - i + 1) as f64;
            let said = if low {
                format!("{}–{}: the subject's top runs out of the picture and the eye lands low on it ({:.0}% down, no face found; on a bust, the camera is on the chin or chest, not the eyes): aim ~{:.2} picture-heights higher, or pull back ~25%", clock(f), clock(t), cy * 100.0, (cy - 0.35).max(0.1))
            } else {
                format!("{}–{}: the subject's top runs out of the picture (its silhouette meets the top edge, the eye on its upper half, {:.0}% down): aim ~0.15 picture-heights higher, or pull back ~20%", clock(f), clock(t), cy * 100.0)
            };
            out.push(json!({"from": r3(f), "to": r3(t), "kind": "top-cut", "said": said}));
        }
        // a push that ends lower on its subject than it began: the subject grows, the eye's centre sinks
        let with: Vec<&Look> = shot.iter().filter(|l| l.has_subject()).collect();
        if with.len() >= 4 {
            let n = (with.len() / 4).max(1);
            let avg = |ls: &[&Look]| -> (f64, f64) {
                let k = ls.len() as f64;
                (ls.iter().map(|l| l.subject.unwrap().h).sum::<f64>() / k, ls.iter().map(|l| l.centre.unwrap().1).sum::<f64>() / k)
            };
            let (h0, y0) = avg(&with[..n]);
            let (h1, y1) = avg(&with[with.len() - n..]);
            if h1 >= h0 * 1.25 && y1 - y0 >= 0.15 {
                let (f, t) = (with[0].t, with[with.len() - 1].t + every);
                out.push(json!({"from": r3(f), "to": r3(t), "kind": "push-low", "said": format!("{}–{}: the push ends lower on its subject than it began (the eye's centre from {:.0}% to {:.0}% down as it grows {:.1}x): keep the aim on what it opened on, or end the push wider", clock(f), clock(t), y0 * 100.0, y1 * 100.0, h1 / h0)}));
            }
        }
        // a subject held, then lost without a cut: the eye's centre goes to the frame's edge, or nothing holds it and
        // what's lit no longer overlaps the subject held (the camera has moved off it; a subject dimming, or the
        // eye moving to something else on it, isn't this)
        let first: Vec<Rect> = shot.iter().take((1.0 / every).ceil() as usize).filter(|l| l.has_subject()).filter_map(|l| l.subject).collect();
        if first.len() as f64 >= 1.0 / every * 0.75 {
            let held = first[first.len() - 1];
            let off = |l: &Look| -> bool {
                l.centre.is_some_and(|c| c.0 < 0.06 || c.0 > 0.94) || (!l.has_subject() && l.subject.map_or(true, |s| overlap(&s, &held) < 0.2))
            };
            for (i, j) in runs(&|i| off(&shot[i]), 1.5) {
                if i == 0 {
                    continue;
                }
                let (f, t) = span(i, j);
                out.push(json!({"from": r3(f), "to": r3(t), "kind": "lost", "said": format!("{}–{}: the camera leaves its subject without a cut (the eye goes to the frame's edge, or nothing holds it where the subject was): keep the subject in frame, or cut", clock(f), clock(t))}));
            }
        }
        // the camera moves with nothing to look at: no subject, the eye spread over a picture lit edge to edge or
        // over nothing lit at all, for 2.5 s of a moving shot (a dolly into stairs, a push onto a blank wall). Title
        // cards and quotes over a scrim are dark or still, and pass.
        let moving = |i: usize| -> f64 {
            let (p, q) = if i > 0 { (i - 1, i) } else { (0, 1.min(shot.len() - 1)) };
            distance(&shot[p].sig, &shot[q].sig)
        };
        let blank = |i: usize| -> bool {
            let l = &shot[i];
            let dark = l.sig.iter().sum::<f64>() / l.sig.len().max(1) as f64;
            !l.has_subject()
                && dark >= 0.03
                && moving(i) >= 0.02
                && l.subject.map_or(true, |s| (s.w >= 0.95 && l.single < 0.45) || s.w * s.h < 0.01)
        };
        for (i, j) in runs(&blank, 2.5) {
            let (f, t) = span(i, j);
            if let Some(last) = out.last_mut().filter(|o: &&mut Value| o["kind"] == "aimless" && (o["to"].as_f64().unwrap_or(0.0) - f).abs() <= every + 1e-6) {
                let from = last["from"].as_f64().unwrap_or(f);
                last["to"] = json!(r3(t));
                last["said"] = json!(format!("{}–{}: the camera moves with nothing to look at (no subject: the eye spread over a picture lit edge to edge, or nothing lit to hold it): give the move something to find, or end it before it fills the frame with a surface", clock(from), clock(t)));
                continue;
            }
            out.push(json!({"from": r3(f), "to": r3(t), "kind": "aimless", "said": format!("{}–{}: the camera moves with nothing to look at (no subject: the eye spread over a picture lit edge to edge, or nothing lit to hold it): give the move something to find, or end it before it fills the frame with a surface", clock(f), clock(t))}));
        }
        let none = runs(&|i| !shot[i].has_subject(), 6.0);
        for (i, j) in none {
            let (f, t) = span(i, j);
            out.push(json!({"from": r3(f), "to": r3(t), "kind": "no-subject", "said": format!("{}–{}: (note) no subject for {:.0} s: nothing compact holds the eye", clock(f), clock(t), t - f)}));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 192 x 108 frame, black but for a lit block (x0..x1, y0..y1 in pixels), Vision's attention on the block.
    fn bust(x0: usize, x1: usize, y0: usize, y1: usize) -> Look {
        let (w, h) = (192, 108);
        let mut rgba = vec![0u8; w * h * 4];
        for y in y0..y1 {
            for x in x0..x1 {
                rgba[(y * w + x) * 4..(y * w + x) * 4 + 3].copy_from_slice(&[220, 200, 180]);
            }
        }
        let (mw, mh) = (32, 18);
        let mut m = vec![0.0f32; mw * mh];
        for y in y0 * mh / h..(y1 * mh).div_ceil(h) {
            for x in x0 * mw / w..(x1 * mw).div_ceil(w) {
                // brightest towards the block's top, as attention favours a face
                m[y * mw + x] = 1.0 - 0.3 * (y as f32 / mh as f32);
            }
        }
        look(0.0, &rgba, w, h, Some(&(m, mw, mh)), &[], (0.0, 0.0))
    }

    fn at(l: &Look, t: f64) -> Look {
        Look { t, ..l.clone() }
    }

    #[test]
    fn flood_is_four_connected() {
        let on = |x: usize, y: usize| (x, y) == (0, 0) || (x, y) == (1, 0) || (x, y) == (2, 1);
        let mut got = flood(3, 2, (0, 0), &on);
        got.sort();
        assert_eq!(got, vec![(0, 0), (1, 0)]);
        assert!(flood(3, 2, (0, 1), &on).is_empty());
    }

    #[test]
    fn a_head_out_of_the_top_is_cut_and_a_framed_one_is_not() {
        let cut = bust(80, 112, 0, 80);
        let s = cut.subject.unwrap();
        assert!(cut.has_subject(), "{cut:?}");
        assert!(s.y <= 0.005 && s.h > 0.7 && (s.w - 1.0 / 6.0).abs() < 0.03, "{s:?}");
        assert_eq!(top_cut(&cut), Some(false));
        let framed = bust(80, 112, 20, 90);
        assert!(framed.has_subject());
        assert!(framed.subject.unwrap().y > 0.15);
        assert_eq!(top_cut(&framed), None);
    }

    #[test]
    fn letterbox_rows_are_not_picture() {
        // bars of 12 rows top and bottom: a block from row 12 down touches the picture's top
        let (w, h) = (192, 108);
        let mut rgba = vec![0u8; w * h * 4];
        for y in 12..70 {
            for x in 80..112 {
                rgba[(y * w + x) * 4..(y * w + x) * 4 + 3].copy_from_slice(&[220, 200, 180]);
            }
        }
        let mut m = vec![0.0f32; 32 * 18];
        for y in 2..12 {
            for x in 13..19 {
                m[y * 32 + x] = 1.0;
            }
        }
        let l = look(0.0, &rgba, w, h, Some(&(m, 32, 18)), &[], (12.0 / 108.0, 12.0 / 108.0));
        assert!(l.subject.unwrap().y <= 0.005, "{:?}", l.subject);
    }

    #[test]
    fn a_held_crop_is_a_fault_and_a_framed_shot_passes() {
        let cut = bust(80, 112, 0, 80);
        let framed = bust(80, 112, 20, 90);
        let looks: Vec<Look> = (0..8).map(|k| at(&framed, k as f64 * 0.5)).chain((8..12).map(|k| at(&cut, k as f64 * 0.5))).collect();
        let f = faults(&looks, &[], 0.5);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0]["kind"], "top-cut");
        assert_eq!((f[0]["from"].as_f64(), f[0]["to"].as_f64()), (Some(4.0), Some(6.0)));
        let ok: Vec<Look> = (0..12).map(|k| at(&framed, k as f64 * 0.5)).collect();
        assert!(faults(&ok, &[], 0.5).is_empty());
        // one sample of a crop is a passing frame, not a fault
        let mut blip = ok.clone();
        blip[5] = at(&cut, 2.5);
        assert!(faults(&blip, &[], 0.5).is_empty());
    }

    #[test]
    fn a_camera_moving_off_its_subject_is_lost_unless_cut() {
        let held = bust(80, 112, 20, 90);
        // something small at the frame's edge; the same picture overall, as a pan's samples are (a far one is a cut)
        let gone = Look { sig: held.sig.clone(), ..bust(176, 192, 40, 60) };
        let looks: Vec<Look> = (0..4).map(|k| at(&held, k as f64 * 0.5)).chain((4..10).map(|k| at(&gone, k as f64 * 0.5))).collect();
        assert!(faults(&looks, &[], 0.5).iter().any(|f| f["kind"] == "lost"), "{:?}", faults(&looks, &[], 0.5));
        assert!(!faults(&looks, &[1.9], 0.5).iter().any(|f| f["kind"] == "lost"));
    }

    #[test]
    fn shots_split_at_cuts_and_dark_samples() {
        let lit = bust(80, 112, 20, 90);
        let dark = Look { sig: vec![0.0; 144], ..Default::default() };
        let looks: Vec<Look> = (0..10).map(|k| at(if k == 4 { &dark } else { &lit }, k as f64 * 0.5)).collect();
        assert_eq!(shots(&looks, &[3.7]), vec![(0, 3), (5, 7), (8, 9)]);
        assert!(distance(&lit.sig, &lit.sig) < 1e-9);
        assert!(distance(&lit.sig, &bust(0, 40, 0, 108).sig) > 0.45);
    }
}
