//! Visible cuts (docs/the roadmap A4): the signals the current detector reads from ffmpeg, computed in Rust from raea-media's
//! NV12 frames, and the detector itself (engine/html/src/measure.rs › sensitive_shot_changes, fitted on Buddy's renders).
//!
//! The signals, per frame, at 480 pixels wide as the ffmpeg command scales them (`scale=480:-2`):
//! - scene: ffmpeg's `select` scene score: mafd = the mean absolute difference of the Y plane against the previous
//!   frame (measured: ffmpeg's score on these files is luma's alone); scene = clamp(min(mafd, |mafd - prev|) / 100, 0, 1)
//! - ydif: `signalstats`' YDIF, the mean absolute difference of the Y plane against the previous frame
//!
//! The planes are resized with a box (area) filter where ffmpeg uses bicubic; cuts_rust.py measures the difference.

use std::path::Path;

/// One frame's signals: (time, scene score, YDIF).
pub type Signal = (f64, f64, f64);

/// A plane resized to `ow` x `oh` by averaging the source pixels each output pixel covers.
fn area(src: &[u8], w: usize, h: usize, stride: usize, step: usize, ow: usize, oh: usize) -> Vec<u8> {
    let mut out = vec![0u8; ow * oh];
    for y in 0..oh {
        let (y0, y1) = (y * h / oh, ((y + 1) * h / oh).max(y * h / oh + 1).min(h));
        for x in 0..ow {
            let (x0, x1) = (x * w / ow, ((x + 1) * w / ow).max(x * w / ow + 1).min(w));
            let (mut s, mut n) = (0u32, 0u32);
            for yy in y0..y1 {
                for xx in x0..x1 {
                    s += src[yy * stride + xx * step] as u32;
                    n += 1;
                }
            }
            out[y * ow + x] = ((s + n / 2) / n) as u8;
        }
    }
    out
}

/// Every frame's signals.
pub fn signals(path: &Path, backend: &str) -> Result<Vec<Signal>, String> {
    let mut d = raea_media::backend(backend)?.open_video(path)?;
    let info = d.info().clone();
    let frame = 1.0 / info.fps.max(1.0);
    let (ow, oh) = (480usize, {
        // scale=480:-2: the height kept in proportion, rounded down to even
        let h = (480.0 * info.height as f64 / info.width as f64).round() as usize;
        h - h % 2
    });
    let mut out = Vec::new();
    let mut prev: Option<[Vec<u8>; 3]> = None;
    let mut prev_mafd = 0.0f64;
    let mut prev_pts = f64::NEG_INFINITY;
    let mut k = 0usize;
    loop {
        let t = k as f64 * frame;
        if t >= info.duration {
            break;
        }
        k += 1;
        let f = d.frame_nv12_at(t + frame - 1e-3)?;
        if f.pts <= prev_pts {
            continue;
        }
        prev_pts = f.pts;
        let (w, h) = (f.width as usize, f.height as usize);
        let y = area(&f.rgba[..w * h], w, h, w, 1, ow, oh);
        let uv = &f.rgba[w * h..];
        let (cw, ch) = (w / 2, h / 2);
        let u = area(uv, cw, ch, w, 2, ow / 2, oh / 2);
        let v = area(&uv[1..], cw, ch, w, 2, ow / 2, oh / 2);
        let planes = [y, u, v];
        let (mut scene, mut ydif) = (0.0, 0.0);
        if let Some(p) = &prev {
            // the luma plane only: on these files ffmpeg's score matches Y alone (0.0488 vs 0.0492 on kd-20s), not
            // Y, U and V over their pixel count, which reads 0.72 of it
            let sad: u64 = p[0].iter().zip(&planes[0]).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64).sum();
            let mafd = sad as f64 / planes[0].len() as f64;
            let diff = (mafd - prev_mafd).abs();
            scene = (mafd.min(diff) / 100.0).clamp(0.0, 1.0);
            prev_mafd = mafd;
            ydif = p[0].iter().zip(&planes[0]).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64).sum::<u64>() as f64 / planes[0].len() as f64;
        }
        out.push((f.pts, scene, ydif));
        prev = Some(planes);
    }
    Ok(out)
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// The visible cuts (engine/html/src/measure.rs › sensitive_shot_changes, the same rule and thresholds).
pub fn visible_cuts(frames: &[Signal]) -> Vec<f64> {
    let mut picked: Vec<(f64, f64)> = Vec::new();
    for (i, &(t, sc, yd)) in frames.iter().enumerate() {
        let (lo, hi) = (i.saturating_sub(12), (i + 13).min(frames.len()));
        let mut ms: Vec<f64> = (lo..hi).filter(|&j| j != i).map(|j| frames[j].1).collect();
        let mut my: Vec<f64> = (lo..hi).filter(|&j| j != i).map(|j| frames[j].2).collect();
        let (ms, my) = (median(&mut ms), median(&mut my));
        if sc > 0.32 || (sc >= 0.12 && sc >= 6.0 * ms.max(1e-4)) || (yd >= 8.0 && yd >= 8.0 * my.max(1e-3)) {
            let strength = sc + yd / 100.0;
            match picked.last_mut() {
                Some(last) if t - last.0 < 0.25 => {
                    if strength > last.1 {
                        *last = (t, strength);
                    }
                }
                _ => picked.push((t, strength)),
            }
        }
    }
    picked.into_iter().map(|p| p.0).collect()
}
