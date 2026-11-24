//! The scene's triangles in one BVH, built once on the CPU (binned SAH) and flattened depth-first for the GPU's shadow
//! rays: an inner node's left child is the next node and its right child is at `a`; a leaf holds `count` triangles
//! from `a`. Every triangle is in it, the ones off screen too, so a caster the camera can't see still casts.

use bevy::math::Vec3;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug)]
pub struct Node {
    pub min: [f32; 3],
    pub a: u32,
    pub max: [f32; 3],
    pub count: u32,
}

/// A triangle as the GPU tests it: v0, v1 - v0 and v2 - v0, each padded to a vec4 whose w holds the triangle's
/// albedo (linear r, g, b), so a bounce ray knows the colour of what it hits.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug)]
pub struct Tri {
    pub v0: [f32; 4],
    pub e1: [f32; 4],
    pub e2: [f32; 4],
}

pub struct Bvh {
    pub nodes: Vec<Node>,
    pub tris: Vec<Tri>,
}

const LEAF: usize = 4;
const BINS: usize = 16;

#[derive(Clone, Copy)]
struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    const EMPTY: Self = Self { min: Vec3::INFINITY, max: Vec3::NEG_INFINITY };
    fn grow(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    /// The bin of BINS a centre falls in along an axis of this box (the centres' box)
    fn bin(&self, c: Vec3, axis: usize) -> usize {
        (((c[axis] - self.min[axis]) / (self.max[axis] - self.min[axis]) * BINS as f32) as usize).min(BINS - 1)
    }
    fn join(&mut self, b: &Aabb) {
        self.min = self.min.min(b.min);
        self.max = self.max.max(b.max);
    }
    fn area(&self) -> f32 {
        let d = (self.max - self.min).max(Vec3::ZERO);
        2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
    }
}

/// The BVH over `tris`, each with its albedo (`albedo` one a triangle, or empty for black).
pub fn build(tris: &[[Vec3; 3]], albedo: &[Vec3]) -> Bvh {
    let boxes: Vec<Aabb> = tris
        .iter()
        .map(|t| {
            let mut b = Aabb::EMPTY;
            t.iter().for_each(|p| b.grow(*p));
            b
        })
        .collect();
    let cents: Vec<Vec3> = boxes.iter().map(|b| (b.min + b.max) * 0.5).collect();
    let mut idx: Vec<u32> = (0..tris.len() as u32).collect();
    let mut nodes = Vec::with_capacity(tris.len() / 2 + 1);
    split(&boxes, &cents, &mut idx, 0, tris.len(), &mut nodes);
    let tris = idx
        .iter()
        .map(|&i| {
            let [a, b, c] = tris[i as usize];
            let k = albedo.get(i as usize).copied().unwrap_or(Vec3::ZERO);
            Tri { v0: a.extend(k.x).into(), e1: (b - a).extend(k.y).into(), e2: (c - a).extend(k.z).into() }
        })
        .collect();
    Bvh { nodes, tris }
}

/// Builds the node for idx[lo..hi] and its subtree; returns its index.
fn split(boxes: &[Aabb], cents: &[Vec3], idx: &mut [u32], lo: usize, hi: usize, nodes: &mut Vec<Node>) -> usize {
    let me = nodes.len();
    nodes.push(Node::default());
    let (mut bounds, mut cb) = (Aabb::EMPTY, Aabb::EMPTY);
    for &i in &idx[lo..hi] {
        bounds.join(&boxes[i as usize]);
        cb.grow(cents[i as usize]);
    }
    nodes[me].min = bounds.min.into();
    nodes[me].max = bounds.max.into();
    let n = hi - lo;
    let leaf = |nodes: &mut Vec<Node>| {
        nodes[me].a = lo as u32;
        nodes[me].count = n as u32;
        me
    };
    if n <= LEAF {
        return leaf(nodes);
    }
    // the cheapest of BINS-1 planes on each axis by the surface area heuristic
    let ext = cb.max - cb.min;
    let mut best = (f32::INFINITY, 0usize, 0usize);
    for axis in 0..3 {
        if ext[axis] <= 1e-9 {
            continue;
        }
        let mut bins = [(Aabb::EMPTY, 0usize); BINS];
        for &i in &idx[lo..hi] {
            let b = &mut bins[cb.bin(cents[i as usize], axis)];
            b.0.join(&boxes[i as usize]);
            b.1 += 1;
        }
        let mut right = [(0.0f32, 0usize); BINS];
        let (mut acc, mut cnt) = (Aabb::EMPTY, 0);
        for s in (1..BINS).rev() {
            acc.join(&bins[s].0);
            cnt += bins[s].1;
            right[s] = (acc.area(), cnt);
        }
        let (mut acc, mut cnt) = (Aabb::EMPTY, 0);
        for s in 1..BINS {
            acc.join(&bins[s - 1].0);
            cnt += bins[s - 1].1;
            if cnt == 0 || right[s].1 == 0 {
                continue;
            }
            let cost = acc.area() * cnt as f32 + right[s].0 * right[s].1 as f32;
            if cost < best.0 {
                best = (cost, axis, s);
            }
        }
    }
    let mut mid = lo;
    if best.0.is_finite() {
        let (axis, s) = (best.1, best.2);
        for k in lo..hi {
            if cb.bin(cents[idx[k] as usize], axis) < s {
                idx.swap(k, mid);
                mid += 1;
            }
        }
    }
    if mid == lo || mid == hi {
        // centroids all in one place: halve by index
        mid = lo + n / 2;
    }
    split(boxes, cents, idx, lo, mid, nodes);
    let right = split(boxes, cents, idx, mid, hi, nodes);
    nodes[me].a = right as u32;
    nodes[me].count = 0;
    me
}

impl Bvh {
    /// Whether anything lies on the ray from `o` along unit `d` before `tmax`: the shader's `occluded`, on the CPU.
    pub fn occluded(&self, o: Vec3, d: Vec3, tmax: f32) -> bool {
        let inv = Vec3::ONE / d.map(|c| if c.abs() < 1e-12 { 1e-12 } else { c });
        let mut stack = Vec::with_capacity(64);
        let mut i = 0usize;
        loop {
            let n = &self.nodes[i];
            if hit_box(o, inv, n.min.into(), n.max.into(), tmax) {
                if n.count > 0 {
                    let t = &self.tris[n.a as usize..(n.a + n.count) as usize];
                    if t.iter().any(|t| hit_tri(o, d, t, tmax)) {
                        return true;
                    }
                } else {
                    stack.push(n.a as usize);
                    i += 1;
                    continue;
                }
            }
            match stack.pop() {
                Some(j) => i = j,
                None => return false,
            }
        }
    }

    /// The nearest triangle on the ray before `tmax`, and how far: the shader's `closest`, on the CPU.
    pub fn closest(&self, o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, usize)> {
        let inv = Vec3::ONE / d.map(|c| if c.abs() < 1e-12 { 1e-12 } else { c });
        let mut stack = Vec::with_capacity(64);
        let (mut best, mut i) = (None, 0usize);
        let mut far = tmax;
        loop {
            let n = &self.nodes[i];
            if hit_box(o, inv, n.min.into(), n.max.into(), far) {
                if n.count > 0 {
                    for k in n.a as usize..(n.a + n.count) as usize {
                        let t = tri_t(o, d, &self.tris[k]);
                        if t < far {
                            (far, best) = (t, Some((t, k)));
                        }
                    }
                } else {
                    stack.push(n.a as usize);
                    i += 1;
                    continue;
                }
            }
            match stack.pop() {
                Some(j) => i = j,
                None => return best,
            }
        }
    }

    /// The deepest leaf's depth: the GPU's traversal stack must hold it.
    pub fn depth(&self) -> usize {
        fn go(b: &Bvh, i: usize) -> usize {
            let n = &b.nodes[i];
            if n.count > 0 { 1 } else { 1 + go(b, i + 1).max(go(b, n.a as usize)) }
        }
        go(self, 0)
    }
}

fn hit_box(o: Vec3, inv: Vec3, min: Vec3, max: Vec3, tmax: f32) -> bool {
    let (t0, t1) = ((min - o) * inv, (max - o) * inv);
    let near = t0.min(t1).max_element().max(0.0);
    let far = t0.max(t1).min_element().min(tmax);
    near <= far
}

pub fn hit_tri(o: Vec3, d: Vec3, t: &Tri, tmax: f32) -> bool {
    tri_t(o, d, t) < tmax
}

/// Möller-Trumbore, both sides: how far along the ray the triangle is, infinity if it isn't on it.
pub fn tri_t(o: Vec3, d: Vec3, t: &Tri) -> f32 {
    let (v0, e1, e2) = (Vec3::from_slice(&t.v0[..3]), Vec3::from_slice(&t.e1[..3]), Vec3::from_slice(&t.e2[..3]));
    let pv = d.cross(e2);
    let det = e1.dot(pv);
    if det.abs() < 1e-12 {
        return f32::INFINITY;
    }
    let inv = 1.0 / det;
    let tv = o - v0;
    let u = tv.dot(pv) * inv;
    if !(0.0..=1.0).contains(&u) {
        return f32::INFINITY;
    }
    let qv = tv.cross(e1);
    let v = d.dot(qv) * inv;
    if v < 0.0 || u + v > 1.0 {
        return f32::INFINITY;
    }
    let t = e2.dot(qv) * inv;
    if t > 0.0 { t } else { f32::INFINITY }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn f(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
        fn v(&mut self, s: f32) -> Vec3 {
            Vec3::new(self.f(), self.f(), self.f()) * 2.0 * s - s
        }
    }

    #[test]
    fn the_bvh_answers_as_every_triangle_does() {
        let mut r = Lcg(7);
        let soup: Vec<[Vec3; 3]> = (0..3000)
            .map(|_| {
                let c = r.v(5.0);
                [c + r.v(0.4), c + r.v(0.4), c + r.v(0.4)]
            })
            .collect();
        let bvh = build(&soup, &[]);
        let all: Vec<Tri> = build(&soup, &[]).tris;
        assert_eq!(all.len(), soup.len());
        let (mut hits, mut n) = (0, 0);
        for _ in 0..20000 {
            let o = r.v(6.0);
            let d = r.v(1.0).normalize_or(Vec3::X);
            let tmax = r.f() * 12.0;
            let brute = all.iter().any(|t| hit_tri(o, d, t, tmax));
            assert_eq!(bvh.occluded(o, d, tmax), brute, "ray {o} {d} {tmax}");
            // the nearest hit too, as LB5's bounce rays find it
            let near = all.iter().enumerate().map(|(k, t)| (tri_t(o, d, t), k)).filter(|h| h.0 < tmax).min_by(|a, b| a.0.total_cmp(&b.0));
            assert_eq!(bvh.closest(o, d, tmax).map(|h| h.1), near.map(|h| h.1), "nearest on ray {o} {d} {tmax}");
            hits += brute as u32;
            n += 1;
        }
        // both answers turn up, so the test means something
        assert!(hits > n / 10 && hits < n * 9 / 10, "{hits} of {n} rays hit");
        assert!(bvh.depth() < 48, "depth {}", bvh.depth());
    }

    #[test]
    fn coincident_triangles_still_split() {
        let t = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let bvh = build(&vec![t; 100], &[]);
        assert!(bvh.occluded(Vec3::new(0.2, 0.2, 1.0), Vec3::NEG_Z, 2.0));
        assert!(!bvh.occluded(Vec3::new(0.8, 0.8, 1.0), Vec3::NEG_Z, 2.0));
    }
}
