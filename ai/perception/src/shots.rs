//! Where a video's shots are and how each begins (Perception.swift › video, sampleFrames, sample, shotBounds).
//!
//! Frames are sampled at 10 a second (4 past three minutes, 2 past ten), each reduced to a 48 x 27 grid of RGB, a
//! 64-bin colour histogram and its luma. A cut is a jump that stands out from its neighbourhood; a fade goes through
//! black; a dissolve is a stretch whose middle frame is the blend of its two ends. The arithmetic is f32 in the
//! Swift's order, so the same samples give the same bounds.

use serde::{Deserialize, Serialize};

pub const GRID_W: usize = 48;
pub const GRID_H: usize = 27;
/// The size the Swift asks the decoder for before sampling the grid (aspect not kept).
pub const SCALED: usize = 192;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub t: f64,
    pub grid: Vec<f32>,
    pub hist: Vec<f32>,
    pub luma: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bound {
    pub start: f64,
    pub end: f64,
    pub transition: String,
}

/// Samples a second for footage this long: the decode is the cost.
pub fn sample_rate(duration: f64) -> f64 {
    if duration > 600.0 {
        2.0
    } else if duration > 180.0 {
        4.0
    } else {
        10.0
    }
}

/// One frame (RGBA8, `w` x `h`) as a Sample. The Swift's decoder scales to 192 x 192 and the grid takes each cell's
/// centre pixel of that; here each of those 192 x 192 pixels is the mean of the source pixels it covers.
pub fn sample(rgba: &[u8], w: usize, h: usize, t: f64) -> Sample {
    let mut grid = vec![0f32; GRID_W * GRID_H * 3];
    let mut hist = vec![0f32; 64];
    let mut luma = 0f32;
    for gy in 0..GRID_H {
        for gx in 0..GRID_W {
            // the cell's centre in the 192 x 192 picture, then the source pixels under that one pixel
            let (sx, sy) = ((gx * 2 + 1) * SCALED / (GRID_W * 2), (gy * 2 + 1) * SCALED / (GRID_H * 2));
            let (x0, x1) = (sx * w / SCALED, ((sx + 1) * w / SCALED).max(sx * w / SCALED + 1).min(w));
            let (y0, y1) = (sy * h / SCALED, ((sy + 1) * h / SCALED).max(sy * h / SCALED + 1).min(h));
            let mut acc = [0u32; 3];
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = (y * w + x) * 4;
                    acc[0] += rgba[p] as u32;
                    acc[1] += rgba[p + 1] as u32;
                    acc[2] += rgba[p + 2] as u32;
                }
            }
            let n = ((x1 - x0) * (y1 - y0)) as u32;
            // rounded to a byte, as the scaled picture is
            let byte = |v: u32| ((v + n / 2) / n) as f32 / 255.0;
            let (r, g, b) = (byte(acc[0]), byte(acc[1]), byte(acc[2]));
            let i = (gy * GRID_W + gx) * 3;
            grid[i] = r;
            grid[i + 1] = g;
            grid[i + 2] = b;
            let q = |v: f32| ((v * 4.0) as usize).min(3);
            hist[q(r) * 16 + q(g) * 4 + q(b)] += 1.0;
            luma += 0.2126 * r + 0.7152 * g + 0.0722 * b;
        }
    }
    let n = (GRID_W * GRID_H) as f32;
    Sample { t, grid, hist: hist.iter().map(|v| v / n).collect(), luma: luma / n }
}

pub fn pixel_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut d = 0f32;
    for i in 0..a.len() {
        d += (a[i] - b[i]).abs();
    }
    d / a.len() as f32
}

pub fn hist_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut d = 0f32;
    for i in 0..a.len() {
        d += (a[i] - b[i]).abs();
    }
    d / 2.0
}

fn median(v: &[f32]) -> f32 {
    let mut x = v.to_vec();
    x.sort_by(f32::total_cmp);
    if x.is_empty() { 0.0 } else { x[x.len() / 2] }
}

/// Where the shots are and how each begins.
pub fn shot_bounds(s: &[Sample], duration: f64) -> Vec<Bound> {
    let n = s.len();
    if n <= 2 {
        return vec![Bound { start: 0.0, end: duration, transition: "start".into() }];
    }
    let mut change = vec![0f32; n]; // change[i]: from sample i-1 to i
    for i in 1..n {
        change[i] = hist_distance(&s[i].hist, &s[i - 1].hist).max(pixel_distance(&s[i].grid, &s[i - 1].grid) * 1.6);
    }
    // (time, transition, end of the transition) in order
    let mut marks: Vec<(f64, &str, f64)> = Vec::new();
    let dark: Vec<bool> = s.iter().map(|x| x.luma < 0.06).collect();
    let mut step = vec![0f32; n]; // pixel change from sample i-1 to i
    for i in 1..n {
        step[i] = pixel_distance(&s[i].grid, &s[i - 1].grid);
    }
    let second = 3usize.max((n as f64 / duration.max(0.1)).round() as usize); // samples in about a second
    let mut i = 1;
    while i < n {
        // through black: a dark stretch between two pictures, widened to the dimming before and the brightening after
        if dark[i] && !dark[i - 1] {
            let mut j = i;
            while j < n && dark[j] {
                j += 1;
            }
            if j >= n {
                break;
            }
            let (mut p, mut q) = (i, j);
            while p - 1 > 0 && i - p < second && s[p - 1].luma > s[p].luma + 0.004 {
                p -= 1;
            }
            while q + 1 < n && q - j < second && s[q + 1].luma > s[q].luma + 0.004 {
                q += 1;
            }
            marks.push((s[p].t, "fade", s[q].t));
            i = q + 1;
            continue;
        }
        // a dissolve: k samples of steady change adding up to a different picture whose middle is the blend of its ends
        let mut found = false;
        let mut k = 15.min(n - i);
        while k >= 3 {
            let (a, b) = (i - 1, i - 1 + k);
            let total = pixel_distance(&s[a].grid, &s[b].grid);
            if total >= 0.12 && (i..=b).all(|x| step[x] < total * 0.7) {
                let m = (a + b) / 2;
                let mut blend = 0f32;
                for p in 0..s[m].grid.len() {
                    blend += (s[m].grid[p] - (s[a].grid[p] + s[b].grid[p]) / 2.0).abs();
                }
                blend /= s[m].grid.len() as f32;
                if blend <= total * 0.2 {
                    // the widest window that passes also holds steady frames either side: trim them off
                    let (mut lo, mut hi) = (a, b);
                    while lo + 1 < hi && step[lo + 1] < total * 0.06 {
                        lo += 1;
                    }
                    while hi - 1 > lo && step[hi] < total * 0.06 {
                        hi -= 1;
                    }
                    marks.push((s[lo].t, "dissolve", s[hi].t));
                    i = b + 1;
                    found = true;
                    break;
                }
            }
            k -= 1;
        }
        if found {
            continue;
        }
        let around: Vec<f32> = (i.saturating_sub(5).max(1)..(i + 6).min(n)).filter(|&x| x != i).map(|x| change[x]).collect();
        if change[i] >= 0.3 && change[i] >= median(&around) * 2.5 + 0.05 {
            marks.push((s[i].t, "cut", s[i].t));
        }
        i += 1;
    }
    let mut bounds = Vec::new();
    let (mut start, mut how) = (0.0, "start");
    for (t, kind, until) in marks {
        if t - start >= 0.4 {
            bounds.push(Bound { start, end: t, transition: how.into() });
        }
        start = until;
        how = kind;
    }
    if duration - start >= 0.2 || bounds.is_empty() {
        bounds.push(Bound { start, end: duration, transition: how.into() });
    }
    bounds
}

/// Each sample of a video at the Swift's rate: the first frame at or after each sample time.
pub fn samples(path: &std::path::Path, backend: &str) -> Result<(f64, Vec<Sample>), String> {
    let mut d = raea_media::backend(backend)?.open_video(path)?;
    let info = d.info().clone();
    let (duration, fps) = (info.duration, sample_rate(info.duration));
    let frame = 1.0 / info.fps.max(1.0);
    let mut out: Vec<Sample> = Vec::new();
    let mut next = 0.0;
    while next < duration {
        // frame_at gives the latest frame at or before t (within 1 us); a frame on, less 1 ms, is the first at or after
        let f = d.frame_at(next + frame - 1e-3)?;
        if out.last().is_none_or(|l| f.pts > l.t) {
            out.push(sample(&f.rgba, f.width as usize, f.height as usize, f.pts));
        }
        next += 1.0 / fps;
    }
    Ok((duration, out))
}

/// Motion and brightness inside a shot, as the Swift reads them.
pub fn motion_and_brightness(s: &[Sample], b: &Bound) -> (&'static str, f32) {
    let inside: Vec<&Sample> = s.iter().filter(|x| x.t >= b.start && x.t < b.end).collect();
    let change: Vec<f32> = inside.windows(2).map(|w| pixel_distance(&w[1].grid, &w[0].grid)).collect();
    let mean = if change.is_empty() { 0.0 } else { change.iter().sum::<f32>() / change.len() as f32 };
    let brightness = if inside.is_empty() { 0.0 } else { inside.iter().map(|x| x.luma).sum::<f32>() / inside.len() as f32 };
    (if mean < 0.025 { "still" } else { "moving" }, brightness)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(t: f64, v: f32) -> Sample {
        let grid = vec![v; GRID_W * GRID_H * 3];
        let mut hist = vec![0f32; 64];
        let q = ((v * 4.0) as usize).min(3);
        hist[q * 16 + q * 4 + q] = 1.0;
        Sample { t, grid, hist, luma: v }
    }

    #[test]
    fn a_hard_change_is_a_cut() {
        let s: Vec<Sample> = (0..40).map(|i| flat(i as f64 / 10.0, if i < 20 { 0.2 } else { 0.8 })).collect();
        let b = shot_bounds(&s, 4.0);
        assert_eq!(b.len(), 2);
        assert_eq!(b[1].transition, "cut");
        assert!((b[1].start - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_dark_stretch_is_a_fade() {
        let s: Vec<Sample> = (0..40).map(|i| flat(i as f64 / 10.0, if (18..23).contains(&i) { 0.01 } else { 0.5 })).collect();
        let b = shot_bounds(&s, 4.0);
        assert_eq!(b.iter().map(|x| x.transition.as_str()).collect::<Vec<_>>(), ["start", "fade"]);
    }

    #[test]
    fn one_picture_is_one_shot() {
        let s: Vec<Sample> = (0..30).map(|i| flat(i as f64 / 10.0, 0.5)).collect();
        assert_eq!(shot_bounds(&s, 3.0), vec![Bound { start: 0.0, end: 3.0, transition: "start".into() }]);
    }
}
