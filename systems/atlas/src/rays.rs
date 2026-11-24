//! The rays the atlas needs, and the sampling weathering's causes share: a triangle shut inside a closed shell gets
//! no texels (`hidden`), found by rays through the model's own triangles (parry3d's BVH). Samples are a rotated R2
//! sequence per texel, so the result is the same on every run.

use std::f32::consts::TAU;

use parry3d::glamx::Vec3;
use parry3d::query::{Ray, RayCast};
use parry3d::shape::TriMesh;
use rayon::prelude::*;

use crate::scene::Scene;

/// How far a ray starts off its surface, and how far one looking for the sky goes.
pub const OFF: f32 = 0.01;
pub const SKY_REACH: f32 = 400.0;

pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}

/// The j-th of a texel's samples in the unit square.
pub fn sample(texel: u32, j: u32) -> (f32, f32) {
    let h = hash(texel.wrapping_mul(2654435761).wrapping_add(0x9e3779b9));
    let (r0, r1) = ((h & 0xffff) as f32 / 65536.0, (h >> 16) as f32 / 65536.0);
    ((0.5 + 0.754_877_7 * j as f32 + r0).fract(), (0.5 + 0.569_840_3 * j as f32 + r1).fract())
}

/// Two unit vectors across n.
pub fn across(n: Vec3) -> (Vec3, Vec3) {
    let a = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let t = n.cross(a).normalize();
    (t, n.cross(t))
}

/// A cosine-weighted direction about n from a sample in the unit square.
pub fn hemi(n: Vec3, (u1, u2): (f32, f32)) -> Vec3 {
    let r = u1.sqrt();
    let (t, b) = across(n);
    (n * (1.0 - u1).max(0.0).sqrt() + t * (r * (TAU * u2).cos()) + b * (r * (TAU * u2).sin())).normalize()
}

pub fn mesh_of(sc: &Scene) -> TriMesh {
    TriMesh::new(sc.pos.clone(), sc.tri.clone()).expect("a TriMesh from the scene's triangles")
}

/// Triangles inside a closed shell: at its centre and near each corner, nine in ten of the rays out of its front hit
/// the back of something (ground under a house, a party wall against its neighbour's, a slab's underside within its
/// walls). Nothing sees them. One point open keeps the triangle: a big one only partly covered (a street's triangle
/// running under its pavement) is still seen where it isn't.
pub fn hidden(sc: &Scene, mesh: &TriMesh) -> Vec<bool> {
    const K: u32 = 16;
    const AT: [[f32; 3]; 4] = [[1.0 / 3.0; 3], [0.8, 0.1, 0.1], [0.1, 0.8, 0.1], [0.1, 0.1, 0.8]];
    (0..sc.tri.len())
        .into_par_iter()
        .map(|t| {
            let n = sc.nrm[t];
            if n == Vec3::ZERO {
                return false;
            }
            let [a, b, c] = sc.tri[t].map(|v| sc.pos[v as usize]);
            AT.iter().enumerate().all(|(k, w)| {
                let o = a * w[0] + b * w[1] + c * w[2] + n * OFF;
                let back = (0..K)
                    .filter(|&j| {
                        let d = hemi(n, sample((t as u32 ^ 0x5bd1_e995).wrapping_add(k as u32 * 7919), j));
                        mesh.cast_local_ray_and_get_normal(&Ray::new(o, d), SKY_REACH, false).is_some_and(|h| sc.nrm[h.subshape as usize].dot(d) > 0.0)
                    })
                    .count();
                back as u32 * 10 >= K * 9
            })
        })
        .collect()
}
