//! The bench's numbers (docs/denoise.feature DN0), each a frame against its truth, both as delivered (8-bit sRGB PNGs
//! after tone mapping):
//! - FLIP: LDR-FLIP's mean, written from the paper (Andersson et al., "FLIP: A Difference Evaluator for Alternating
//!   Images", HPG 2020) at its default viewing (0.7 m from a 0.7 m wide, 3840 px monitor: 67 pixels a degree);
//! - RMSE of the sRGB values, 0 to 1;
//! - bias: the frame's mean luminance against the truth's, in stops (brighter is positive): a denoiser's floor;
//! - blotches: the residual's mid band, in stops: log luminance between a 2 px and a width/64 Gaussian (features up to
//!   about 1/16 of the frame's width), the frame's band against the truth's, mean absolute;
//! - lag: frames after an event (a disocclusion, a light going out) until a frame's error settles back.
//! Flicker is raea light check's (raea_light::metric::error_flicker): the error's second difference in time against
//! the truth's own shot, so motion and fades cancel and what's left is light that changes when the truth doesn't.

use std::path::Path;

/// A frame as linear RGB, row by row, with its 8-bit sRGB values kept for RMSE.
pub struct Rgb {
    pub w: usize,
    pub h: usize,
    pub lin: Vec<[f32; 3]>,
    pub srgb: Vec<[f32; 3]>,
}

impl Rgb {
    /// A frame from its 8-bit sRGB pixels
    pub fn of(w: usize, h: usize, px: impl Iterator<Item = [u8; 3]>) -> Rgb {
        let lut: Vec<f32> = (0..=255u8).map(raea_light::metric::linear).collect();
        let (lin, srgb) = px.map(|c| (c.map(|v| lut[v as usize]), c.map(|v| v as f32 / 255.0))).unzip();
        Rgb { w, h, lin, srgb }
    }
}

pub fn load(p: &Path) -> Result<Rgb, String> {
    let img = image::open(p).map_err(|e| format!("{}: {e}", p.display()))?.to_rgb8();
    Ok(Rgb::of(img.width() as usize, img.height() as usize, img.pixels().map(|px| px.0)))
}

fn same(a: &Rgb, b: &Rgb) -> Result<(), String> {
    if (a.w, a.h) != (b.w, b.h) {
        return Err(format!("sizes differ: {}x{} against the truth's {}x{}", a.w, a.h, b.w, b.h));
    }
    Ok(())
}

/// Rec. 709's luminance weights
const Y709: [f32; 3] = [0.2126, 0.7152, 0.0722];

fn luma(c: &[f32; 3]) -> f32 {
    c.iter().zip(Y709).map(|(c, w)| c * w).sum()
}

pub fn rmse(a: &Rgb, truth: &Rgb) -> Result<f32, String> {
    same(a, truth)?;
    let s: f64 = a.srgb.iter().zip(&truth.srgb).map(|(p, q)| (0..3).map(|k| ((p[k] - q[k]) as f64).powi(2)).sum::<f64>()).sum();
    Ok((s / (3 * a.srgb.len()).max(1) as f64).sqrt() as f32)
}

/// The darkest luminance a stop is measured from (raea light's floor, about sRGB 8/255).
const FLOOR: f32 = 0.0025;

pub fn bias(a: &Rgb, truth: &Rgb) -> Result<f32, String> {
    same(a, truth)?;
    let mean = |x: &Rgb| x.lin.iter().map(|c| luma(c) as f64).sum::<f64>() / x.lin.len().max(1) as f64;
    Ok(((mean(a) as f32).max(FLOOR) / (mean(truth) as f32).max(FLOOR)).log2())
}

/// A plane filtered by the separable kernel `k` (odd length, centred), its edges clamped.
fn convolve(p: &[f32], w: usize, h: usize, kx: &[f32], ky: &[f32]) -> Vec<f32> {
    let (rx, ry) = ((kx.len() / 2) as isize, (ky.len() / 2) as isize);
    let at = |v: isize, n: usize| v.clamp(0, n as isize - 1) as usize;
    let mut t = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            t[y * w + x] = kx.iter().enumerate().map(|(i, k)| k * p[y * w + at(x as isize + i as isize - rx, w)]).sum();
        }
    }
    let mut o = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            o[y * w + x] = ky.iter().enumerate().map(|(i, k)| k * t[at(y as isize + i as isize - ry, h) * w + x]).sum();
        }
    }
    o
}

/// A kernel scaled to sum to 1
fn unit(k: Vec<f32>) -> Vec<f32> {
    let s: f32 = k.iter().sum();
    k.into_iter().map(|v| v / s).collect()
}

/// A Gaussian's weights out to 3 sigma, at least one pixel either side, not normalized
fn bell(sigma: f32) -> Vec<f32> {
    let r = (3.0 * sigma).ceil().max(1.0) as isize;
    let k = -0.5 / (sigma * sigma);
    (-r..=r).map(|x| (k * (x * x) as f32).exp()).collect()
}

fn gaussian(sigma: f32) -> Vec<f32> {
    unit(bell(sigma))
}

pub fn blotch(a: &Rgb, truth: &Rgb) -> Result<f32, String> {
    same(a, truth)?;
    let (w, h) = (a.w, a.h);
    let (fine, coarse) = (gaussian(2.0), gaussian((w as f32 / 64.0).max(3.0)));
    let band = |x: &Rgb| {
        let l: Vec<f32> = x.lin.iter().map(|c| luma(c).max(FLOOR).log2()).collect();
        let (f, c) = (convolve(&l, w, h, &fine, &fine), convolve(&l, w, h, &coarse, &coarse));
        f.into_iter().zip(c).map(|(f, c)| f - c).collect::<Vec<f32>>()
    };
    let (p, q) = (band(a), band(truth));
    Ok((p.iter().zip(&q).map(|(p, q)| (p - q).abs() as f64).sum::<f64>() / p.len().max(1) as f64) as f32)
}

/// Frames after `event` until the error settles: the first frame from which every later one is within 25% (and 0.02)
/// of the error before the event (the median of those frames). None when it never settles in the shot or there's no
/// frame before the event to settle to.
pub fn lag(errors: &[f32], event: usize) -> Option<usize> {
    let mut before: Vec<f32> = errors.get(..event)?.to_vec();
    if before.is_empty() || event >= errors.len() {
        return None;
    }
    before.sort_by(f32::total_cmp);
    let settled = before[before.len() / 2];
    let ok = |e: f32| e <= (settled * 1.25).max(settled + 0.02);
    let after = &errors[event..];
    let last_bad = after.iter().rposition(|&e| !ok(e));
    match last_bad {
        None => Some(0),
        Some(k) if k + 1 < after.len() => Some(k + 1),
        Some(_) => None,
    }
}

/// The worst whole-frame spike in a shot, in stops: each frame's mean light against the mean of the frames either side
/// (m[k] - (m[k-1] + m[k+1]) / 2, in log2), less the truth's own at that frame, so a fade or a cut in the truth counts
/// for nothing and a frame the denoiser lifts or drops alone counts in full. Per-pixel flicker averages such a flash
/// away. Fewer than three frames measure nothing.
pub fn spike(frames: &[raea_light::metric::Luma], truth: &[raea_light::metric::Luma]) -> Option<f32> {
    let m = |f: &raea_light::metric::Luma| ((f.y.iter().map(|&v| v as f64).sum::<f64>() / f.y.len().max(1) as f64) as f32).max(FLOOR).log2();
    let (a, t): (Vec<f32>, Vec<f32>) = (frames.iter().map(m).collect(), truth.iter().map(m).collect());
    let d2 = |v: &[f32], k: usize| v[k] - 0.5 * (v[k - 1] + v[k + 1]);
    (a.len() >= 3 && a.len() == t.len()).then(|| (1..a.len() - 1).map(|k| (d2(&a, k) - d2(&t, k)).abs()).fold(0.0, f32::max))
}

// LDR-FLIP (the paper's section 4, its constants)
const PPD: f32 = 0.7 * 3840.0 / 0.7 * std::f32::consts::PI / 180.0;
const QC: f32 = 0.7;
const QF: f32 = 0.5;
const PC: f32 = 0.4;
const PT: f32 = 0.95;
/// sRGB's linear RGB to XYZ (D65), and back
const TO_XYZ: [[f32; 3]; 3] = [[0.4124564, 0.3575761, 0.1804375], [0.2126729, 0.7151522, 0.0721750], [0.0193339, 0.1191920, 0.9503041]];
const TO_RGB: [[f32; 3]; 3] = [[3.2404542, -1.5371385, -0.4985314], [-0.9692660, 1.8760108, 0.0415560], [0.0556434, -0.2040259, 1.0572252]];
const WHITE: [f32; 3] = [0.95047, 1.0, 1.08883];

fn mul(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0; 3];
    for (o, row) in o.iter_mut().zip(m) {
        *o = row[0] * v[0] + row[1] * v[1] + row[2] * v[2];
    }
    o
}

fn ycxcz(rgb: [f32; 3]) -> [f32; 3] {
    let x = mul(&TO_XYZ, rgb);
    let (xn, yn, zn) = (x[0] / WHITE[0], x[1] / WHITE[1], x[2] / WHITE[2]);
    [116.0 * yn - 16.0, 500.0 * (xn - yn), 200.0 * (yn - zn)]
}

fn ycxcz_to_rgb(c: [f32; 3]) -> [f32; 3] {
    let yn = (c[0] + 16.0) / 116.0;
    let xyz = [(c[1] / 500.0 + yn) * WHITE[0], yn * WHITE[1], (yn - c[2] / 200.0) * WHITE[2]];
    mul(&TO_RGB, xyz).map(|v| v.clamp(0.0, 1.0))
}

/// Hunt-adjusted L*a*b* of a linear RGB colour
fn hunt_lab(rgb: [f32; 3]) -> [f32; 3] {
    let x = mul(&TO_XYZ, rgb);
    let f = |t: f32| if t > (6.0f32 / 29.0).powi(3) { t.cbrt() } else { t / (3.0 * (6.0f32 / 29.0).powi(2)) + 4.0 / 29.0 };
    let (fx, fy, fz) = (f(x[0] / WHITE[0]), f(x[1] / WHITE[1]), f(x[2] / WHITE[2]));
    let l = 116.0 * fy - 16.0;
    [l, 0.01 * l * 500.0 * (fx - fy), 0.01 * l * 200.0 * (fy - fz)]
}

fn hyab(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).abs() + ((a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The contrast sensitivity kernel of one opponent channel (a sum of two Gaussians, in degrees), normalized
fn csf(a1: f32, b1: f32, a2: f32, b2: f32) -> Vec<f32> {
    let r = (3.0 * (b1.max(b2) / (2.0 * std::f32::consts::PI.powi(2))).sqrt() * PPD).ceil() as isize;
    let pi2 = std::f32::consts::PI.powi(2);
    unit((-r..=r).map(|i| (i as f32 / PPD).powi(2)).map(|x2| a1 * (std::f32::consts::PI / b1).sqrt() * (-pi2 * x2 / b1).exp() + a2 * (std::f32::consts::PI / b2).sqrt() * (-pi2 * x2 / b2).exp()).collect())
}

/// The feature detectors' 1-D parts: a Gaussian and its first and second derivatives at the paper's 0.082 degrees
fn feature_kernels() -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let sd = 0.5 * 0.082 * PPD;
    let g = bell(sd);
    let r = (g.len() / 2) as isize;
    let d1: Vec<f32> = (-r..=r).zip(&g).map(|(x, g)| -(x as f32) * g).collect();
    let d2: Vec<f32> = (-r..=r).zip(&g).map(|(x, g)| ((x * x) as f32 / (sd * sd) - 1.0) * g).collect();
    (g, d1, d2)
}

/// A 2-D kernel's separable parts scaled so its positive weights sum to 1 and its negative to -1 (the paper's
/// normalization), given the 2-D sums each sign would have
fn signed_norm(along: &[f32], across: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let (mut pos, mut neg) = (0.0f32, 0.0f32);
    for a in along {
        for b in across {
            let v = a * b;
            if v > 0.0 { pos += v } else { neg -= v }
        }
    }
    let gs: f32 = across.iter().sum();
    let across: Vec<f32> = across.iter().map(|v| v / gs).collect();
    let (p, n) = (pos / gs, neg / gs);
    (along.iter().map(|v| if *v > 0.0 { v / p } else { v / n.max(1e-12) }).collect(), across)
}

/// The edge and point strength at each pixel of a plane
fn features(y: &[f32], w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
    let (g, d1, d2) = feature_kernels();
    let (e1, eg) = signed_norm(&d1, &g);
    let (p1, pg) = signed_norm(&d2, &g);
    let (ex, ey) = (convolve(y, w, h, &e1, &eg), convolve(y, w, h, &eg, &e1));
    let (px, py) = (convolve(y, w, h, &p1, &pg), convolve(y, w, h, &pg, &p1));
    let m = |a: Vec<f32>, b: Vec<f32>| a.into_iter().zip(b).map(|(a, b)| a.hypot(b)).collect::<Vec<f32>>();
    (m(ex, ey), m(px, py))
}

/// LDR-FLIP's mean over the frame (0 the same, 1 the most different)
pub fn flip(a: &Rgb, truth: &Rgb) -> Result<f32, String> {
    same(a, truth)?;
    let (w, h) = (a.w, a.h);
    let kernels = [csf(1.0, 0.0047, 0.0, 1e-5), csf(1.0, 0.0053, 0.0, 1e-5), csf(34.1, 0.04, 13.5, 0.025)];
    // the colour pipeline: each opponent channel filtered as the eye sees it, back to RGB, then Hunt-adjusted Lab
    let filtered = |x: &Rgb| {
        let ch: Vec<[f32; 3]> = x.lin.iter().map(|c| ycxcz(*c)).collect();
        let planes: Vec<Vec<f32>> = (0..3).map(|k| convolve(&ch.iter().map(|c| c[k]).collect::<Vec<_>>(), w, h, &kernels[k], &kernels[k])).collect();
        (0..w * h).map(|i| hunt_lab(ycxcz_to_rgb([planes[0][i], planes[1][i], planes[2][i]]))).collect::<Vec<[f32; 3]>>()
    };
    let (fa, ft) = (filtered(a), filtered(truth));
    let cmax = hyab(hunt_lab([0.0, 1.0, 0.0]), hunt_lab([0.0, 0.0, 1.0])).powf(QC);
    // the feature pipeline, on the achromatic channel normalized to 0..1
    let ach = |x: &Rgb| x.lin.iter().map(|c| (ycxcz(*c)[0] + 16.0) / 116.0).collect::<Vec<f32>>();
    let ((ea, pa), (et, pt)) = (features(&ach(a), w, h), features(&ach(truth), w, h));
    let mut sum = 0.0f64;
    for i in 0..w * h {
        let d = hyab(fa[i], ft[i]).powf(QC);
        let ec = if d < PC * cmax { PT / (PC * cmax) * d } else { PT + (d - PC * cmax) / (cmax - PC * cmax) * (1.0 - PT) };
        let ef = (std::f32::consts::FRAC_1_SQRT_2 * (ea[i] - et[i]).abs().max((pa[i] - pt[i]).abs())).powf(QF);
        sum += ec.min(1.0).powf(1.0 - ef) as f64;
    }
    Ok((sum / (w * h).max(1) as f64) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(v: f32) -> u8 {
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    fn flat(w: usize, h: usize, c: [f32; 3]) -> Rgb {
        Rgb::of(w, h, std::iter::repeat_n(c.map(q), w * h))
    }

    fn noisy(base: &Rgb, amp: f32, seed: u32) -> Rgb {
        let mut s = seed;
        Rgb::of(base.w, base.h, base.srgb.iter().map(|c| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * amp;
            c.map(|v| q(v + n))
        }))
    }

    #[test]
    fn a_frame_against_itself_is_no_error() {
        let a = noisy(&flat(48, 32, [0.4, 0.5, 0.3]), 0.2, 7);
        assert!(flip(&a, &a).unwrap() < 1e-6);
        assert_eq!(rmse(&a, &a).unwrap(), 0.0);
        assert!(bias(&a, &a).unwrap().abs() < 1e-6 && blotch(&a, &a).unwrap() < 1e-6);
    }

    #[test]
    fn more_noise_is_more_flip_and_black_against_white_is_near_one() {
        let base = flat(48, 32, [0.5, 0.5, 0.5]);
        let (lo, hi) = (flip(&noisy(&base, 0.05, 1), &base).unwrap(), flip(&noisy(&base, 0.3, 1), &base).unwrap());
        assert!(lo < hi, "{lo} {hi}");
        let bw = flip(&flat(32, 32, [0.0; 3]), &flat(32, 32, [1.0; 3])).unwrap();
        assert!(bw > 0.9, "{bw}");
    }

    #[test]
    fn twice_the_light_is_one_stop_of_bias() {
        let (a, b) = (flat(8, 8, [0.4; 3]), flat(8, 8, [0.2; 3]));
        let (la, lb) = (luma(&a.lin[0]), luma(&b.lin[0]));
        let want = (la / lb).log2();
        assert!((bias(&a, &b).unwrap() - want).abs() < 1e-4);
    }

    #[test]
    fn a_soft_blob_is_a_blotch_and_fine_grain_less_so() {
        // 640 wide, so the band runs from 2 px to 10 px: a wall of soft blotches 12 px across against per-pixel grain
        let (w, h) = (640, 320);
        let base = flat(w, h, [0.5; 3]);
        let tau = std::f32::consts::TAU;
        let blob = Rgb::of(w, h, (0..w * h).map(|i| [q(0.5 + 0.05 * (tau * (i % w) as f32 / 24.0).sin() * (tau * (i / w) as f32 / 24.0).sin()); 3]));
        let grain = noisy(&base, 0.02, 3);
        let (b, g) = (blotch(&blob, &base).unwrap(), blotch(&grain, &base).unwrap());
        assert!(b > g, "blob {b} grain {g}");
    }

    #[test]
    fn a_frame_lifted_alone_is_a_spike_and_a_fade_is_not() {
        let f = |v: f32| raea_light::metric::Luma { w: 4, h: 4, y: vec![v; 16] };
        let fade: Vec<_> = (0..6).map(|k| f(0.1 * 1.2f32.powi(k))).collect();
        let mut flash: Vec<_> = (0..6).map(|_| f(0.1)).collect();
        flash[3] = f(0.2);
        assert!(spike(&fade, &fade).unwrap() < 1e-5);
        assert!((spike(&flash, &fade).unwrap() - 1.0).abs() < 0.05);
    }

    #[test]
    fn lag_counts_the_frames_to_settle() {
        // settled at 0.1, an event at frame 4, three frames to come back
        let e = [0.1, 0.1, 0.11, 0.1, 0.9, 0.5, 0.2, 0.11, 0.1, 0.1];
        assert_eq!(lag(&e, 4), Some(3));
        assert_eq!(lag(&[0.1, 0.1, 0.1, 0.1], 2), Some(0));
        assert_eq!(lag(&[0.1, 0.1, 0.9, 0.9], 2), None);
        assert_eq!(lag(&[0.9, 0.1], 0), None);
    }
}
