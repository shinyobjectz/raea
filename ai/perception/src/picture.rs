//! Pictures for the models, prepared the way Perception.swift's Core Image calls prepare them, so the same picture
//! gives the same model input: one scale for both axes, Core Image's bottom-left origin, bilinear at pixel centres,
//! transparent outside the picture, over black.

use crate::coreml::Bgra;

/// Bilinear sample of RGBA8 at (x, y) in pixel units (rows from the top, x = 0.0 the left edge of pixel 0's centre
/// minus a half): transparent (0) outside, as Core Image samples a transformed image.
fn bilinear(rgba: &[u8], w: usize, h: usize, fx: f64, fy: f64) -> [u8; 3] {
    let at = |x: i64, y: i64, c: usize| -> f64 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 { 0.0 } else { rgba[(y as usize * w + x as usize) * 4 + c] as f64 }
    };
    let (x0, y0) = (fx.floor() as i64, fy.floor() as i64);
    let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
    let mut out = [0u8; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let top = at(x0, y0, c) * (1.0 - tx) + at(x0 + 1, y0, c) * tx;
        let bottom = at(x0, y0 + 1, c) * (1.0 - tx) + at(x0 + 1, y0 + 1, c) * tx;
        *o = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// A side x side BGRA canvas whose pixel (bx, by) (rows from the top) shows the picture at `src(bx, by)` (a point in
/// the picture's pixels, rows from the top, centres at +0.5), black where that falls outside.
pub fn warp(rgba: &[u8], w: usize, h: usize, side: usize, src: impl Fn(f64, f64) -> (f64, f64)) -> Bgra {
    let mut bytes = vec![0u8; side * side * 4];
    for by in 0..side {
        for bx in 0..side {
            let (fx, fy) = src(bx as f64 + 0.5, by as f64 + 0.5);
            let s = bilinear(rgba, w, h, fx - 0.5, fy - 0.5);
            let o = (by * side + bx) * 4;
            bytes[o] = s[2];
            bytes[o + 1] = s[1];
            bytes[o + 2] = s[0];
            bytes[o + 3] = 255;
        }
    }
    Bgra { w: side, h: side, bytes }
}

/// The picture scaled by `s` (Core Image's affine scale: bilinear at pixel centres), as RGBA8 of its new size.
pub fn scaled(rgba: &[u8], w: usize, h: usize, s: f64) -> (Vec<u8>, usize, usize) {
    let (ow, oh) = (((w as f64 * s).round() as usize).max(1), ((h as f64 * s).round() as usize).max(1));
    let mut out = vec![0u8; ow * oh * 4];
    for y in 0..oh {
        for x in 0..ow {
            let p = bilinear(rgba, w, h, (x as f64 + 0.5) / s - 0.5, (y as f64 + 0.5) / s - 0.5);
            let o = (y * ow + x) * 4;
            out[o..o + 3].copy_from_slice(&p);
            out[o + 3] = 255;
        }
    }
    (out, ow, oh)
}

/// The picture scaled to fit side x side (one scale), its scaled size, and its place: left padding `ox`, and the
/// bottom edge `bottom` up from the canvas's bottom (Core Image's origin), as each Swift caller translates it.
pub struct Letterbox {
    pub image: Bgra,
    pub scale: f64,
    pub cw: usize,
    pub ch: usize,
    pub ox: usize,
}

/// RGBA8 averaged down by an integer factor (each output pixel the mean of an f x f block, edges clamped).
fn box_down(rgba: &[u8], w: usize, h: usize, f: usize) -> (Vec<u8>, usize, usize) {
    let (ow, oh) = (w.div_ceil(f), h.div_ceil(f));
    let mut out = vec![0u8; ow * oh * 4];
    for y in 0..oh {
        for x in 0..ow {
            let (mut acc, mut n) = ([0u32; 4], 0u32);
            for yy in y * f..((y + 1) * f).min(h) {
                for xx in x * f..((x + 1) * f).min(w) {
                    let i = (yy * w + xx) * 4;
                    for c in 0..4 {
                        acc[c] += rgba[i + c] as u32;
                    }
                    n += 1;
                }
            }
            for c in 0..4 {
                out[(y * ow + x) * 4 + c] = ((acc[c] + n / 2) / n) as u8;
            }
        }
    }
    (out, ow, oh)
}

pub fn letterbox(rgba: &[u8], w: usize, h: usize, side: usize, bottom: impl Fn(usize) -> usize) -> Letterbox {
    let scale = (side as f64 / w as f64).min(side as f64 / h as f64);
    let (cw, ch) = (((w as f64 * scale).round() as usize).max(1), ((h as f64 * scale).round() as usize).max(1));
    let ox = (side - cw) / 2;
    let oy = bottom(ch) as f64;
    // where each canvas pixel falls in the picture (pixels, rows from the top): Core Image's space is from the bottom
    let place = move |cx: f64, cy_top: f64| {
        let cy = side as f64 - cy_top;
        let (u, v) = ((cx - ox as f64) / scale, (cy - oy) / scale);
        (u, h as f64 - v)
    };
    // shrinking past half: sample a copy averaged down by the whole factor first, as Core Image's downscaling does
    // (measured on the face references: the detector's crop moves to within 0.5 degrees of the Swift's, from 1.6-3.5)
    let image = if scale < 0.5 {
        let f = (1.0 / scale).floor() as usize;
        let (small, sw, sh) = box_down(rgba, w, h, f);
        warp(&small, sw, sh, side, |cx, cy| {
            let (x, y) = place(cx, cy);
            (x / f as f64, y / f as f64)
        })
    } else {
        warp(rgba, w, h, side, place)
    };
    Letterbox { image, scale, cw, ch, ox }
}
