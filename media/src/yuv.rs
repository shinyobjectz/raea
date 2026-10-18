//! YUV <-> RGBA, BT.709 limited range (what phones and our encoders use), for the backends that hand us planes.

/// I420 planes to straight RGBA8.
#[allow(clippy::too_many_arguments)]
pub fn i420_to_rgba(
    y: &[u8],
    u: &[u8],
    v: &[u8],
    ys: usize,
    us: usize,
    vs: usize,
    w: usize,
    h: usize,
    out: &mut Vec<u8>,
) {
    out.resize(w * h * 4, 0);
    for row in 0..h {
        for col in 0..w {
            let yy = y[row * ys + col] as f32 - 16.0;
            let uu = u[(row / 2) * us + col / 2] as f32 - 128.0;
            let vv = v[(row / 2) * vs + col / 2] as f32 - 128.0;
            let c = 1.164_383 * yy;
            let r = c + 1.792_741 * vv;
            let g = c - 0.213_249 * uu - 0.532_909 * vv;
            let b = c + 2.112_402 * uu;
            let i = (row * w + col) * 4;
            out[i] = r.round().clamp(0.0, 255.0) as u8;
            out[i + 1] = g.round().clamp(0.0, 255.0) as u8;
            out[i + 2] = b.round().clamp(0.0, 255.0) as u8;
            out[i + 3] = 255;
        }
    }
}

/// Straight RGBA8 to I420 planes (y, u, v), BT.709 limited range; w and h even.
pub fn rgba_to_i420(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    crate::encode_convert_timed(|| rgba_to_i420_inner(rgba, w, h))
}

fn rgba_to_i420_inner(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut y = vec![0u8; w * h];
    let mut u = vec![0u8; (w / 2) * (h / 2)];
    let mut v = vec![0u8; (w / 2) * (h / 2)];
    let px = |i: usize| {
        (
            rgba[i * 4] as f32,
            rgba[i * 4 + 1] as f32,
            rgba[i * 4 + 2] as f32,
        )
    };
    for row in 0..h {
        for col in 0..w {
            let (r, g, b) = px(row * w + col);
            y[row * w + col] = (16.0 + 0.182_586 * r + 0.614_231 * g + 0.062_007 * b)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    for row in 0..h / 2 {
        for col in 0..w / 2 {
            let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let (pr, pg, pb) = px((row * 2 + dy) * w + col * 2 + dx);
                r += pr;
                g += pg;
                b += pb;
            }
            let (r, g, b) = (r / 4.0, g / 4.0, b / 4.0);
            u[row * (w / 2) + col] = (128.0 - 0.100_644 * r - 0.338_572 * g + 0.439_216 * b)
                .round()
                .clamp(0.0, 255.0) as u8;
            v[row * (w / 2) + col] = (128.0 + 0.439_216 * r - 0.398_942 * g - 0.040_274 * b)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    (y, u, v)
}

/// RGBA rotated clockwise by `deg` (0, 90, 180 or 270), as a container's display matrix asks.
pub fn rotate(rgba: &[u8], w: usize, h: usize, deg: i32) -> (Vec<u8>, usize, usize) {
    crate::convert_timed(|| rotate_inner(rgba, w, h, deg))
}

fn rotate_inner(rgba: &[u8], w: usize, h: usize, deg: i32) -> (Vec<u8>, usize, usize) {
    let deg = deg.rem_euclid(360);
    if deg == 0 {
        return (rgba.to_vec(), w, h);
    }
    let (ow, oh) = if deg == 180 { (w, h) } else { (h, w) };
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match deg {
                90 => (h - 1 - y, x),
                180 => (w - 1 - x, h - 1 - y),
                _ => (y, w - 1 - x),
            };
            let (s, d) = ((y * w + x) * 4, (ny * ow + nx) * 4);
            out[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
        }
    }
    (out, ow, oh)
}

/// NV12 to I420 planes: the luma plane as it is, the interleaved chroma split in two.
pub fn nv12_to_i420(nv12: &[u8], w: usize, h: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    crate::encode_convert_timed(|| {
        let (y, uv) = nv12.split_at(w * h);
        let n = (w / 2) * (h / 2);
        let (mut u, mut v) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for p in uv[..2 * n].chunks_exact(2) {
            u.push(p[0]);
            v.push(p[1]);
        }
        (y.to_vec(), u, v)
    })
}

/// NV12 to straight RGBA (for PNGs and quality checks; the encoders take NV12 as it is).
pub fn nv12_to_rgba(nv12: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (y, u, v) = nv12_to_i420(nv12, w, h);
    let mut out = Vec::new();
    i420_to_rgba(&y, &u, &v, w, w / 2, w / 2, w, h, &mut out);
    out
}

/// Strided I420 planes to tight NV12 (luma, then interleaved Cb Cr).
#[allow(clippy::too_many_arguments)]
pub fn i420_to_nv12(y: &[u8], u: &[u8], v: &[u8], ys: usize, us: usize, vs: usize, w: usize, h: usize, out: &mut Vec<u8>) {
    crate::convert_timed(|| {
        out.resize(w * h * 3 / 2, 0);
        for r in 0..h {
            out[r * w..(r + 1) * w].copy_from_slice(&y[r * ys..r * ys + w]);
        }
        let uv = &mut out[w * h..];
        for r in 0..h / 2 {
            for c in 0..w / 2 {
                uv[r * w + 2 * c] = u[r * us + c];
                uv[r * w + 2 * c + 1] = v[r * vs + c];
            }
        }
    })
}

/// NV12 rotated clockwise by `deg` (0, 90, 180, 270): the luma plane by pixels, the chroma plane by Cb Cr pairs.
pub fn rotate_nv12(nv12: &[u8], w: usize, h: usize, deg: i32) -> (Vec<u8>, usize, usize) {
    let deg = deg.rem_euclid(360);
    if deg == 0 {
        return (nv12.to_vec(), w, h);
    }
    crate::convert_timed(|| {
        let (ow, oh) = if deg == 180 { (w, h) } else { (h, w) };
        let mut out = vec![0u8; w * h * 3 / 2];
        let to = |x: usize, y: usize, w: usize, h: usize| match deg {
            90 => (h - 1 - y, x),
            180 => (w - 1 - x, h - 1 - y),
            _ => (y, w - 1 - x),
        };
        for y in 0..h {
            for x in 0..w {
                let (nx, ny) = to(x, y, w, h);
                out[ny * ow + nx] = nv12[y * w + x];
            }
        }
        let (cw, ch) = (w / 2, h / 2);
        let ocw = ow / 2;
        for y in 0..ch {
            for x in 0..cw {
                let (nx, ny) = to(x, y, cw, ch);
                let (s, d) = (w * h + y * w + 2 * x, ow * oh + ny * ow + 2 * nx);
                out[d] = nv12[s];
                out[d + 1] = nv12[s + 1];
                let _ = ocw;
            }
        }
        (out, ow, oh)
    })
}

/// Straight RGBA to NV12 (BT.709 limited range), through the I420 planes.
pub fn rgba_to_nv12(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (y, u, v) = rgba_to_i420(rgba, w, h);
    let mut out = Vec::new();
    i420_to_nv12(&y, &u, &v, w, w / 2, w / 2, w, h, &mut out);
    out
}
