//! Optical flow without Apple Vision: pyramidal Lucas-Kanade on a grid of points (plain Rust, no model), reduced per
//! frame pair exactly as flow.rs reduces Vision's dense flow (docs/the roadmap Track L, the FLOW slot): the 3x3 grid's
//! mean magnitude, dx dy, zoom and mag, each as a share of the frame's width.

#[derive(Clone, Debug, Default)]
pub struct Flow {
    pub grid: [f32; 9],
    pub dx: f32,
    pub dy: f32,
    pub zoom: f32,
    pub mag: f32,
}

struct Img {
    w: usize,
    h: usize,
    v: Vec<f32>,
}

impl Img {
    fn at(&self, x: f32, y: f32) -> f32 {
        let x = x.clamp(0.0, (self.w - 1) as f32);
        let y = y.clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let r = |x: usize, y: usize| self.v[y * self.w + x];
        (r(x0, y0) * (1.0 - tx) + r(x1, y0) * tx) * (1.0 - ty) + (r(x0, y1) * (1.0 - tx) + r(x1, y1) * tx) * ty
    }

    fn half(&self) -> Img {
        let (w, h) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let mut v = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = ((2 * x).min(self.w - 1), (2 * y).min(self.h - 1));
                let (sx1, sy1) = ((sx + 1).min(self.w - 1), (sy + 1).min(self.h - 1));
                v[y * w + x] = (self.v[sy * self.w + sx] + self.v[sy * self.w + sx1] + self.v[sy1 * self.w + sx] + self.v[sy1 * self.w + sx1]) / 4.0;
            }
        }
        Img { w, h, v }
    }
}

fn grey(bgra: &[u8], w: usize, h: usize) -> Img {
    Img { w, h, v: bgra.chunks_exact(4).map(|p| 0.114 * p[0] as f32 + 0.587 * p[1] as f32 + 0.299 * p[2] as f32).collect() }
}

/// One point's flow from `a` to `b` by Lucas-Kanade over a (2r+1)^2 window, starting from `guess`, and the mean
/// absolute difference left between the windows (grey levels 0..255).
fn lk(a: &Img, b: &Img, px: f32, py: f32, guess: (f32, f32), r: i32) -> ((f32, f32), f32) {
    let (mut gxx, mut gxy, mut gyy) = (0f32, 0f32, 0f32);
    let mut grads = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
    for j in -r..=r {
        for i in -r..=r {
            let (x, y) = (px + i as f32, py + j as f32);
            let gx = (a.at(x + 1.0, y) - a.at(x - 1.0, y)) / 2.0;
            let gy = (a.at(x, y + 1.0) - a.at(x, y - 1.0)) / 2.0;
            gxx += gx * gx;
            gxy += gx * gy;
            gyy += gy * gy;
            grads.push((x, y, gx, gy, a.at(x, y)));
        }
    }
    let det = gxx * gyy - gxy * gxy;
    let resid = |u: f32, v: f32| grads.iter().map(|&(x, y, _, _, ia)| (ia - b.at(x + u, y + v)).abs()).sum::<f32>() / grads.len() as f32;
    if det < 1e-3 * (gxx + gyy).max(1.0) {
        return (guess, resid(guess.0, guess.1));
    }
    let (mut u, mut v) = guess;
    for _ in 0..6 {
        let (mut bx, mut by) = (0f32, 0f32);
        for &(x, y, gx, gy, ia) in &grads {
            let e = ia - b.at(x + u, y + v);
            bx += gx * e;
            by += gy * e;
        }
        let du = (gyy * bx - gxy * by) / det;
        let dv = (gxx * by - gxy * bx) / det;
        u += du;
        v += dv;
        if du.abs() + dv.abs() < 0.01 {
            break;
        }
    }
    ((u, v), resid(u, v))
}

/// The flow from `a` to `b`, both BGRA8 `w` x `h`: Lucas-Kanade on every 8th pixel over a 3-level pyramid.
pub fn flow(a: &[u8], b: &[u8], w: usize, h: usize) -> Flow {
    let (mut pa, mut pb) = (vec![grey(a, w, h)], vec![grey(b, w, h)]);
    for _ in 0..2 {
        let (na, nb) = (pa.last().unwrap().half(), pb.last().unwrap().half());
        pa.push(na);
        pb.push(nb);
    }
    let step = 8usize;
    let (gw, gh) = (w / step, h / step);
    // a point is kept only where the windows still match after it moved (a mean difference of 24 grey levels or
    // less) and it moved under a fifth of the frame: across a cut or an occlusion Lucas-Kanade runs away
    let mut field: Vec<Option<(f32, f32)>> = vec![None; gw * gh];
    for (k, f) in field.iter_mut().enumerate() {
        let (px, py) = (((k % gw) * step + step / 2) as f32, ((k / gw) * step + step / 2) as f32);
        let (mut g, mut e) = ((0f32, 0f32), 0f32);
        for lvl in (0..pa.len()).rev() {
            let s = (1 << lvl) as f32;
            (g, e) = lk(&pa[lvl], &pb[lvl], px / s, py / s, g, 7);
            if lvl > 0 {
                g = (g.0 * 2.0, g.1 * 2.0);
            }
        }
        let m = (g.0 * g.0 + g.1 * g.1).sqrt();
        if e <= 24.0 && m < 0.2 * w as f32 {
            // under half a pixel is the codec's noise, not motion (Vision's field is still there)
            *f = Some(if m < 0.5 { (0.0, 0.0) } else { g });
        }
    }
    // reduced as flow.rs reduces Vision's field
    let mut out = Flow::default();
    let (mut cells, mut counts) = ([0f64; 9], [0usize; 9]);
    let (mut sx, mut sy, mut sz, mut nz) = (0f64, 0f64, 0f64, 0usize);
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let kept = field.iter().filter(|f| f.is_some()).count();
    for (k, f) in field.iter().enumerate() {
        let Some((dx, dy)) = *f else { continue };
        let (x, y) = ((k % gw) * step + step / 2, (k / gw) * step + step / 2);
        let (dx, dy) = (dx as f64, dy as f64);
        let c = (y * 3 / h) * 3 + x * 3 / w;
        cells[c] += (dx * dx + dy * dy).sqrt();
        counts[c] += 1;
        sx += dx;
        sy += dy;
        let (rx, ry) = (x as f64 - cx, y as f64 - cy);
        let d = (rx * rx + ry * ry).sqrt();
        if d > 0.1 * w as f64 {
            sz += (dx * rx + dy * ry) / (d * d);
            nz += 1;
        }
    }
    let n = kept.max(1) as f64;
    for i in 0..9 {
        out.grid[i] = (cells[i] / counts[i].max(1) as f64 / w as f64) as f32;
    }
    out.dx = (sx / n / w as f64) as f32;
    out.dy = (sy / n / w as f64) as f32;
    out.zoom = (sz / nz.max(1) as f64) as f32;
    out.mag = out.grid.iter().cloned().fold(0.0, f32::max);
    out
}
