//! Planar charts and their shelf packing, and each texel's point on the surface. A chart is a primitive's triangles
//! joined across shared edges (positions welded to a millimetre, so flat-shaded faces with their own vertices still
//! join) while each stays within CHART_DEG of the first one's plane, projected on that plane: on a wall the atlas's
//! rows run down the wall (s along it, t down it), so the fall line is the same way in every chart. Charts are packed
//! on shelves at `density` texels a metre with a GUTTER each side, after merge.rs has joined the coplanar ones that
//! touch and folded the details too small for texels of their own into the charts they sit on; when the atlas would
//! pass `max_side`, the density is lowered until it fits (the result says which). Every texel a chart covers gets the world point and triangle
//! under its centre; texels within 0.75 of a texel of the chart's edge get the nearest point on it, so a bilinear
//! read at an edge reads the surface.

use std::collections::HashMap;

use parry3d::glamx::Vec3;

use super::{Ask, R};
use parry3d::shape::TriMesh;

use crate::scene::Scene;
use crate::{merge, rays};

/// One texel round each chart: enough for a bilinear read with no mips (a mipped read needs 2^k for k levels, WR8).
pub const GUTTER: u32 = 1;
const CHART_DEG: f32 = 15.0;
const NONE: u32 = u32::MAX;

pub struct Texel {
    pub x: u16,
    pub y: u16,
    pub p: Vec3,
    pub n: Vec3,
    pub tri: u32,
}

pub struct Atlas {
    pub w: u32,
    pub h: u32,
    /// the density the whole-model class got, and each chart's
    pub density: f32,
    pub chart_dens: Vec<f32>,
    pub charts: usize,
    /// triangles inside a closed shell, given no texels
    pub hidden: usize,
    pub texels: Vec<Texel>,
    /// per triangle, its chart
    pub chart_of: Vec<u32>,
    /// per primitive: each new vertex's source vertex (the primitive's own numbering), the new indices and TEXCOORD_1
    pub verts: Vec<Vec<u32>>,
    pub index: Vec<Vec<u32>>,
    pub uv: Vec<Vec<[f32; 2]>>,
    /// each chart's rectangle in the atlas, gutters included: x, y, w, h (the tests read it)
    pub rects: Vec<[u32; 4]>,
}

impl Atlas {
    /// A texel's index in a map over the atlas.
    pub fn px(&self, x: u16, y: u16) -> usize {
        y as usize * self.w as usize + x as usize
    }

    pub fn covered(&self) -> Vec<bool> {
        let mut c = vec![false; (self.w * self.h) as usize];
        for t in &self.texels {
            c[self.px(t.x, t.y)] = true;
        }
        c
    }
}

pub struct Chart {
    pub tris: Vec<u32>,
    /// the plane: its normal, its offset (n.p), and the basis across it
    pub n: Vec3,
    pub off: f32,
    pub u: Vec3,
    pub b: Vec3,
    pub s0: f32,
    pub t0: f32,
    pub s1: f32,
    pub t1: f32,
    /// texels a metre asked for it (the highest its own surfaces ask), before the atlas's scale
    pub dens: f32,
    w: u32,
    h: u32,
    ox: u32,
    oy: u32,
}

impl Chart {
    pub fn new(tris: Vec<u32>, n: Vec3, off: f32, (u, b): (Vec3, Vec3), sc: &Scene, up: Vec3) -> Chart {
        let mut c = Chart { tris, n, off, u, b, s0: 0.0, t0: 0.0, s1: 0.0, t1: 0.0, dens: 0.0, w: 0, h: 0, ox: 0, oy: 0 };
        c.orient(sc, up);
        c
    }
    /// A level chart turned in its plane to its smallest rectangle (a sliver of street running across the tile, a
    /// wire's run); a wall keeps its rows down the fall line. Then fitted.
    pub fn orient(&mut self, sc: &Scene, up: Vec3) {
        if self.n == Vec3::ZERO || self.n.dot(up).abs() < 0.95 {
            return self.fit(sc);
        }
        let pts: Vec<(f32, f32)> = self.tris.iter().flat_map(|&t| sc.tri[t as usize]).map(|v| (sc.pos[v as usize].dot(self.u), sc.pos[v as usize].dot(self.b))).collect();
        let h = hull(pts);
        let mut best = (f32::MAX, 0.0f32);
        for i in 0..h.len() {
            let (a, b) = (h[i], h[(i + 1) % h.len()]);
            let th = (b.1 - a.1).atan2(b.0 - a.0);
            let (c, s) = (th.cos(), th.sin());
            let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
            for p in &h {
                let (x, y) = (p.0 * c + p.1 * s, p.1 * c - p.0 * s);
                lo = (lo.0.min(x), lo.1.min(y));
                hi = (hi.0.max(x), hi.1.max(y));
            }
            let area = (hi.0 - lo.0) * (hi.1 - lo.1);
            if area < best.0 * (1.0 - 1e-5) {
                best = (area, th);
            }
        }
        let (c, s) = (best.1.cos(), best.1.sin());
        (self.u, self.b) = (self.u * c + self.b * s, self.b * c - self.u * s);
        self.fit(sc);
    }
    /// Its rectangle in the plane, from its triangles.
    pub fn fit(&mut self, sc: &Scene) {
        (self.s0, self.t0, self.s1, self.t1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &t in &self.tris {
            for v in sc.tri[t as usize] {
                let (s, tt) = self.st(sc.pos[v as usize]);
                self.s0 = self.s0.min(s);
                self.s1 = self.s1.max(s);
                self.t0 = self.t0.min(tt);
                self.t1 = self.t1.max(tt);
            }
        }
    }
    /// The density its surfaces ask: the highest of their classes'.
    pub fn asked(&self, sc: &Scene, env: &Ask) -> f32 {
        self.tris.iter().map(|&t| env.density_of(sc.class[t as usize])).fold(0.0, f32::max).max(0.25)
    }
    pub fn st(&self, p: Vec3) -> (f32, f32) {
        (p.dot(self.u), -p.dot(self.b))
    }
    /// atlas texel coordinates (continuous: texel i covers [i, i+1)) of a world point on this chart
    fn xy(&self, p: Vec3, k: f32) -> (f32, f32) {
        let (s, t, d) = (self.st(p).0, self.st(p).1, self.dens * k);
        (self.ox as f32 + GUTTER as f32 + (s - self.s0) * d, self.oy as f32 + GUTTER as f32 + (t - self.t0) * d)
    }
}

fn weld(p: Vec3) -> (i64, i64, i64) {
    ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64, (p.z * 1000.0).round() as i64)
}

/// The basis of a plane with normal n: u along it horizontally (east on a level face), b up it.
fn basis(n: Vec3, env: &Ask) -> (Vec3, Vec3) {
    let u = if n.dot(env.up).abs() < 0.95 {
        env.up.cross(n).normalize()
    } else {
        let east = env.north.cross(env.up).normalize();
        (east - n * east.dot(n)).normalize()
    };
    (u, n.cross(u).normalize())
}

fn charts_of(sc: &Scene, env: &Ask, chart_of: &mut [u32]) -> Vec<Chart> {
    let cos = CHART_DEG.to_radians().cos();
    let mut out = Vec::new();
    for pr in &sc.prims {
        let tris = pr.first_tri..pr.first_tri + pr.tris;
        let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let mut edges: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        let welded: Vec<[u32; 3]> = tris
            .clone()
            .map(|t| {
                sc.tri[t].map(|v| {
                    let n = ids.len() as u32;
                    *ids.entry(weld(sc.pos[v as usize])).or_insert(n)
                })
            })
            .collect();
        for (k, w) in welded.iter().enumerate() {
            for e in 0..3 {
                let (a, b) = (w[e], w[(e + 1) % 3]);
                edges.entry((a.min(b), a.max(b))).or_default().push((pr.first_tri + k) as u32);
            }
        }
        for seed in tris.clone() {
            if chart_of[seed] != NONE {
                continue;
            }
            let id = out.len() as u32;
            chart_of[seed] = id;
            let n = sc.nrm[seed];
            let mut list = vec![seed as u32];
            let mut q = vec![seed];
            while let Some(t) = q.pop() {
                if n == Vec3::ZERO {
                    break;
                }
                let w = welded[t - pr.first_tri];
                for e in 0..3 {
                    let (a, b) = (w[e], w[(e + 1) % 3]);
                    for &o in &edges[&(a.min(b), a.max(b))] {
                        let o = o as usize;
                        if chart_of[o] == NONE && sc.nrm[o].dot(n) >= cos {
                            chart_of[o] = id;
                            list.push(o as u32);
                            q.push(o);
                        }
                    }
                }
            }
            list.sort_unstable();
            let (u, b) = if n == Vec3::ZERO { (Vec3::X, Vec3::Y) } else { basis(n, env) };
            let off = if n == Vec3::ZERO { 0.0 } else { n.dot(sc.pos[sc.tri[seed][0] as usize]) };
            out.push(Chart::new(list, n, off, (u, b), sc, env.up));
        }
    }
    out
}

/// Shelves, tallest charts first; the atlas's height, or None when a chart is wider than the side.
fn pack(cs: &mut [Chart], k: f32, side: u32) -> Option<(u32, u32)> {
    for c in cs.iter_mut() {
        let d = c.dens * k;
        c.w = (((c.s1 - c.s0) * d).ceil() as u32).max(1) + 2 * GUTTER;
        c.h = (((c.t1 - c.t0) * d).ceil() as u32).max(1) + 2 * GUTTER;
    }
    let area: f64 = cs.iter().map(|c| c.w as f64 * c.h as f64).sum();
    let widest = cs.iter().map(|c| c.w).max().unwrap_or(1);
    if widest > side {
        return None;
    }
    let w = ((area * 1.2).sqrt().ceil() as u32).next_power_of_two().clamp(64, side).max(widest);
    let mut order: Vec<usize> = (0..cs.len()).collect();
    order.sort_by(|&a, &b| cs[b].h.cmp(&cs[a].h).then(cs[b].w.cmp(&cs[a].w)).then(a.cmp(&b)));
    let (mut x, mut y, mut shelf) = (0u32, 0u32, 0u32);
    for i in order {
        if x + cs[i].w > w {
            y += shelf;
            x = 0;
            shelf = 0;
        }
        cs[i].ox = x;
        cs[i].oy = y;
        x += cs[i].w;
        shelf = shelf.max(cs[i].h);
    }
    Some((w, (y + shelf).div_ceil(4) * 4))
}

pub fn build(sc: &Scene, env: &Ask, mesh: &TriMesh) -> R<Atlas> {
    let mut chart_of = vec![NONE; sc.tri.len()];
    // triangles inside a closed shell are set aside first, so no chart starts from or runs over them
    let inside = rays::hidden(sc, mesh);
    for (t, &h) in inside.iter().enumerate() {
        if h {
            chart_of[t] = NONE - 1;
        }
    }
    let cs = charts_of(sc, env, &mut chart_of);
    let mut cs = merge::absorb(merge::split(merge::merge(cs, sc, env.up), sc, env), sc, env);
    // the hidden triangles share one texel, which stays empty (zero in every cause)
    let hidden: Vec<u32> = (0..sc.tri.len() as u32).filter(|&t| inside[t as usize]).collect();
    let hid = (!hidden.is_empty()).then(|| {
        let mut c = Chart::new(hidden.clone(), Vec3::ZERO, 0.0, (Vec3::X, Vec3::Y), sc, env.up);
        (c.s0, c.t0, c.s1, c.t1, c.dens) = (0.0, 0.0, 0.0, 0.0, env.density);
        cs.push(c);
        cs.len() - 1
    });
    for (i, c) in cs.iter().enumerate() {
        for &t in &c.tris {
            chart_of[t as usize] = i as u32;
        }
    }
    // the scale on every chart's density, lowered until the atlas fits
    let mut d: f32 = 1.0;
    let (w, h) = loop {
        match pack(&mut cs, d, env.max_side) {
            Some((w, h)) if h <= env.max_side => break (w, h),
            Some((_, h)) => d *= (env.max_side as f32 / h as f32).sqrt() * 0.98,
            None => d *= 0.9,
        }
        if d * env.density < 0.01 {
            return Err(format!("the model doesn't fit a {} atlas at any density", env.max_side));
        }
    };
    // texels under each chart, centre in a triangle first, then the edge band
    let mut owner = vec![NONE; (w * h) as usize];
    let mut texels: Vec<Texel> = Vec::new();
    for (ci, c) in cs.iter().enumerate() {
        if Some(ci) == hid {
            continue;
        }
        for &t in &c.tris {
            let tri = sc.tri[t as usize];
            if sc.nrm[t as usize] == Vec3::ZERO {
                continue;
            }
            let wp = tri.map(|v| sc.pos[v as usize]);
            let q = wp.map(|p| c.xy(p, d));
            let (x0, x1) = (q.iter().map(|p| p.0).fold(f32::MAX, f32::min), q.iter().map(|p| p.0).fold(f32::MIN, f32::max));
            let (y0, y1) = (q.iter().map(|p| p.1).fold(f32::MAX, f32::min), q.iter().map(|p| p.1).fold(f32::MIN, f32::max));
            let xr = (x0.floor().max(c.ox as f32) as u32)..=(x1.ceil().min((c.ox + c.w - 1) as f32) as u32);
            let yr = (y0.floor().max(c.oy as f32) as u32)..=(y1.ceil().min((c.oy + c.h - 1) as f32) as u32);
            for y in yr {
                for x in xr.clone() {
                    let i = (y * w + x) as usize;
                    if owner[i] != NONE {
                        continue;
                    }
                    if let Some(bc) = bary(q, (x as f32 + 0.5, y as f32 + 0.5)).filter(|b| b.iter().all(|&v| v >= -1e-5)) {
                        owner[i] = texels.len() as u32;
                        texels.push(Texel { x: x as u16, y: y as u16, p: wp[0] * bc[0] + wp[1] * bc[1] + wp[2] * bc[2], n: sc.nrm[t as usize], tri: t });
                    }
                }
            }
        }
        // the edge band: an empty texel within 0.75 of a texel of a triangle takes its nearest point
        let mut best: HashMap<(u32, u32), (f32, u32, [f32; 3])> = HashMap::new();
        for &t in &c.tris {
            if sc.nrm[t as usize] == Vec3::ZERO {
                continue;
            }
            let wp = sc.tri[t as usize].map(|v| sc.pos[v as usize]);
            let q = wp.map(|p| c.xy(p, d));
            let lo = |a: f32| (a - 1.0).floor().max(0.0) as u32;
            let (x0, x1) = (lo(q.iter().map(|p| p.0).fold(f32::MAX, f32::min)).max(c.ox), (q.iter().map(|p| p.0).fold(f32::MIN, f32::max) + 1.0) as u32);
            let (y0, y1) = (lo(q.iter().map(|p| p.1).fold(f32::MAX, f32::min)).max(c.oy), (q.iter().map(|p| p.1).fold(f32::MIN, f32::max) + 1.0) as u32);
            for y in y0..=y1.min(c.oy + c.h - 1) {
                for x in x0..=x1.min(c.ox + c.w - 1) {
                    if owner[(y * w + x) as usize] != NONE {
                        continue;
                    }
                    let (dist, bc) = nearest(q, (x as f32 + 0.5, y as f32 + 0.5));
                    if dist <= 0.75 && best.get(&(x, y)).is_none_or(|b| dist < b.0) {
                        best.insert((x, y), (dist, t, bc));
                    }
                }
            }
        }
        let mut band: Vec<_> = best.into_iter().collect();
        band.sort_by_key(|((x, y), _)| (*y, *x));
        for ((x, y), (_, t, bc)) in band {
            let wp = sc.tri[t as usize].map(|v| sc.pos[v as usize]);
            owner[(y * w + x) as usize] = texels.len() as u32;
            texels.push(Texel { x: x as u16, y: y as u16, p: wp[0] * bc[0] + wp[1] * bc[1] + wp[2] * bc[2], n: sc.nrm[t as usize], tri: t });
        }
    }
    // each primitive's vertices split by chart, with their TEXCOORD_1
    let (mut verts, mut index, mut uv) = (Vec::new(), Vec::new(), Vec::new());
    for pr in &sc.prims {
        let mut seen: HashMap<(u32, u32), u32> = HashMap::new();
        let (mut vs, mut ix, mut us) = (Vec::new(), Vec::new(), Vec::new());
        for t in pr.first_tri..pr.first_tri + pr.tris {
            let c = &cs[chart_of[t] as usize];
            for v in sc.tri[t] {
                let local = v - pr.base;
                let id = *seen.entry((local, chart_of[t])).or_insert_with(|| {
                    let (x, y) = if Some(chart_of[t] as usize) == hid { ((c.ox + GUTTER) as f32 + 0.5, (c.oy + GUTTER) as f32 + 0.5) } else { c.xy(sc.pos[v as usize], d) };
                    vs.push(local);
                    us.push([x / w as f32, y / h as f32]);
                    (vs.len() - 1) as u32
                });
                ix.push(id);
            }
        }
        verts.push(vs);
        index.push(ix);
        uv.push(us);
    }
    let mut rects = Vec::with_capacity(cs.len());
    for c in cs.iter() {
        rects.push([c.ox, c.oy, c.w, c.h]);
    }
    let chart_dens = cs.iter().map(|c| c.dens * d).collect();
    Ok(Atlas { w, h, density: env.density * d, chart_dens, charts: cs.len(), hidden: hidden.len(), texels, chart_of, verts, index, uv, rects })
}

/// The convex hull of points in a plane, counter-clockwise (Andrew's monotone chain).
fn hull(mut p: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    p.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let cross = |o: (f32, f32), a: (f32, f32), b: (f32, f32)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut h: Vec<(f32, f32)> = Vec::with_capacity(p.len() * 2);
    for pass in 0..2 {
        let start = h.len();
        let pts: Box<dyn Iterator<Item = &(f32, f32)>> = if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in pts {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// Barycentric coordinates of p in the 2D triangle q, or None for a degenerate one.
fn bary(q: [(f32, f32); 3], p: (f32, f32)) -> Option<[f32; 3]> {
    let (a, b, c) = (q[0], q[1], q[2]);
    let den = (b.1 - c.1) * (a.0 - c.0) + (c.0 - b.0) * (a.1 - c.1);
    if den.abs() < 1e-12 {
        return None;
    }
    let l0 = ((b.1 - c.1) * (p.0 - c.0) + (c.0 - b.0) * (p.1 - c.1)) / den;
    let l1 = ((c.1 - a.1) * (p.0 - c.0) + (a.0 - c.0) * (p.1 - c.1)) / den;
    Some([l0, l1, 1.0 - l0 - l1])
}

/// The distance from p to the 2D triangle q, and the nearest point's barycentric coordinates.
fn nearest(q: [(f32, f32); 3], p: (f32, f32)) -> (f32, [f32; 3]) {
    if let Some(b) = bary(q, p).filter(|b| b.iter().all(|&v| v >= 0.0)) {
        return (0.0, b);
    }
    let mut best = (f32::MAX, [1.0, 0.0, 0.0]);
    for e in 0..3 {
        let (i, j) = (e, (e + 1) % 3);
        let (a, b) = (q[i], q[j]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = dx * dx + dy * dy;
        let s = if len2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let (nx, ny) = (a.0 + s * dx, a.1 + s * dy);
        let dist = ((p.0 - nx).powi(2) + (p.1 - ny).powi(2)).sqrt();
        if dist < best.0 {
            let mut bc = [0.0; 3];
            bc[i] = 1.0 - s;
            bc[j] = s;
            best = (dist, bc);
        }
    }
    best
}
