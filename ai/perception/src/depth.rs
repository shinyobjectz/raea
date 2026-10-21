//! Depth (Perception.swift › inverseDepth): Depth Anything 3 Small (Apache-2.0) converted to Core ML, the picture
//! letterboxed into the model's 504 x 504 square on black, 1/depth (bigger = nearer) cropped back to the picture's
//! own area, one pixel in from a padded edge (the model smears the bars into the border).

use crate::coreml::{Bgra, Model, Units};

pub const SIDE: usize = 504;

pub fn model() -> Result<Model, String> {
    // `All` (the GPU): depth agrees on every unit (cosine 0.99985 to the CPU's) and the GPU is the fastest for it
    Model::load("depth-anything-3-small", "DepthAnything3Small", Units::All)
}

/// RGBA8 letterboxed into SIDE x SIDE on black, as the Swift's Core Image places it (oy up from the bottom); the
/// picture's size and place inside it (from the top).
pub fn letterbox(rgba: &[u8], w: usize, h: usize) -> (Bgra, usize, usize, usize, usize) {
    let l = crate::picture::letterbox(rgba, w, h, SIDE, |ch| (SIDE - ch) / 2);
    let top = SIDE - (SIDE - l.ch) / 2 - l.ch;
    (l.image, l.cw, l.ch, l.ox, top)
}

/// 1/depth for the picture's own area: (values row by row, width, height).
pub fn inverse_depth(model: &Model, rgba: &[u8], w: usize, h: usize) -> Result<(Vec<f32>, usize, usize), String> {
    let (input, cw, ch, ox, top) = letterbox(rgba, w, h);
    let oy = SIDE - top - ch;
    let out = model.predict(&[("image", &input)])?;
    let (d, shape) = out.floats("depth")?;
    let row = *shape.last().unwrap_or(&SIDE);
    let (ix, iy) = (if ox > 0 { 1 } else { 0 }, if oy > 0 { 1 } else { 0 });
    let (ww, hh) = (cw - 2 * ix, ch - 2 * iy);
    let mut values = vec![0f32; ww * hh];
    for y in 0..hh {
        for x in 0..ww {
            let v = d[(top + iy + y) * row + ox + ix + x];
            values[y * ww + x] = if v > 1e-6 { 1.0 / v } else { 0.0 };
        }
    }
    Ok((values, ww, hh))
}

/// The camera package (T-334): four frames of one shot in, each frame's [R t], intrinsics and depth out.
pub fn camera_model() -> Result<Model, String> {
    Model::load("depth-anything-3-small", "DepthAnything3SmallCamera", Units::CpuAndGpu)
}

/// One window of four frames (RGBA8 each, letterboxed like inverse_depth): each frame's rotation, camera centre and
/// fx, and frame 0's person/background depth separation (`person` reads the mask on the letterboxed frame 0).
pub fn camera_window(m: &Model, frames: &[(Vec<u8>, usize, usize)], person: &dyn Fn(&[u8], usize, usize) -> Result<(Vec<u8>, usize, usize), String>)
    -> Result<crate::camera::Window, String> {
    let boxed: Vec<(Bgra, usize, usize, usize, usize)> = frames.iter().take(4).map(|(p, w, h)| letterbox(p, *w, *h)).collect();
    let names = ["frame0", "frame1", "frame2", "frame3"];
    let inputs: Vec<(&str, &Bgra)> = boxed.iter().enumerate().map(|(i, b)| (names[i], &b.0)).collect();
    let out = m.predict(&inputs)?;
    let (ex, _) = out.floats("extrinsics")?;
    let (ki, _) = out.floats("intrinsics")?;
    let (mut rotations, mut centres, mut fx) = (Vec::new(), Vec::new(), Vec::new());
    for v in 0..4 {
        let mm: Vec<f64> = (0..12).map(|i| ex[v * 12 + i] as f64).collect();
        let r = vec![mm[0], mm[1], mm[2], mm[4], mm[5], mm[6], mm[8], mm[9], mm[10]];
        let t = [mm[3], mm[7], mm[11]];
        // C = -R^T t
        let c: Vec<f64> = (0..3).map(|j| -((r[j] * t[0]) + (r[3 + j] * t[1]) + (r[6 + j] * t[2]))).collect();
        centres.push(c);
        rotations.push(r);
        fx.push(ki[v * 9] as f64);
    }
    // separation from frame 0: the depth under the person against the rest (the larger median over the smaller)
    let mut separation = None;
    if let Ok((depth, shape)) = out.floats("depth") {
        let row = *shape.last().unwrap_or(&SIDE);
        let sq = &boxed[0].0;
        let mut rgba = sq.bytes.clone();
        for p in rgba.chunks_exact_mut(4) {
            p.swap(0, 2);
        }
        if let Ok((mask, mw, mh)) = person(&rgba, SIDE, SIDE) {
            if mw > 0 {
                let (mut fg, mut bg) = (Vec::new(), Vec::new());
                for y in (0..SIDE).step_by(4) {
                    for x in (0..SIDE).step_by(4) {
                        let v = depth[y * row + x];
                        if v <= 1e-6 {
                            continue;
                        }
                        if mask[(y * mh / SIDE) * mw + x * mw / SIDE] > 127 { fg.push(v) } else { bg.push(v) }
                    }
                }
                if fg.len() > 50 && bg.len() > 50 {
                    let med = |a: &mut Vec<f32>| { a.sort_by(f32::total_cmp); a[a.len() / 2] as f64 };
                    let (a, b) = (med(&mut bg), med(&mut fg));
                    separation = Some(((a.max(b) / a.min(b).max(1e-6)) * 100.0).round() / 100.0);
                }
            }
        }
    }
    Ok(crate::camera::Window { rotations, centres, fx, separation })
}
