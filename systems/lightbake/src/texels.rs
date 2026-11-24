//! Each texel's place in the world (docs/bake.feature BK2). BK1's layout gives every texel the world point under its
//! centre (or, in a chart's edge band, the nearest point of the chart), its triangle's normal and the triangle, so the
//! texels aren't rasterized a second time. Here each point gets its ray origin, moved off its surface by an offset
//! from the model's size and off its triangle's edges, so the trace never hits the surface it starts on nor starts in
//! the plane of the next one, and the texels inside other geometry (a
//! table leg's foot in the floor, a wall's end inside the next wall) are found by rays and marked: they neither go
//! black nor leak light into the bake, and BK4 fills them from their chart's neighbours.

use parry3d::glamx::Vec3;
use parry3d::query::{Ray, RayCast};
use parry3d::shape::TriMesh;
use rayon::prelude::*;
use serde_json::{Value, json};

use raea_sys_atlas::rays::{hemi, sample};
pub use raea_sys_atlas::{Layout, Texel};

/// Rays each texel casts for the inside test: cosine-weighted, the atlas's rotated R2 sequence per texel.
const RAYS: u32 = 16;

pub struct Texels {
    /// BK1's layout: the model with its TEXCOORD_1, the atlas's size and density, the texels' surface points
    pub layout: Layout,
    /// each texel's ray origin: its surface point, four offsets in from its triangle's edges, moved `offset` along its normal
    pub origin: Vec<[f32; 3]>,
    /// the texels inside other geometry (their rays meet back faces), left for BK4 to fill
    pub inside: Vec<bool>,
    /// metres off the surface: 1e-5 of the model's size or of its farthest coordinate, at least 10 microns
    pub offset: f32,
    /// the model's triangles in world space, for the trace (BK3)
    pub mesh: TriMesh,
}

/// The request of a bake that only lights: texels a metre and the atlas's longest side. A model that is also worn
/// passes its wear bake's request instead, so its lightmap lies over that bake's atlas.
pub fn request(texels: f32, max_side: u32) -> Value {
    json!({ "env": { "density": texels, "max_side": max_side } })
}

/// Every texel of the model's atlas in the world: its origin and whether it's inside other geometry.
pub fn texels(glb: &[u8], req: &Value) -> Result<Texels, String> {
    let (layout, mesh) = raea_sys_atlas::laid(glb, req)?;
    let (mut lo, mut hi, mut far) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), 0.0f32);
    for &v in mesh.vertices() {
        (lo, hi, far) = (lo.min(v), hi.max(v), far.max(v.abs().max_element()));
    }
    let offset = (1e-5 * (hi - lo).length().max(far)).max(1e-5);
    let half = 0.5 / layout.density;
    let origin: Vec<[f32; 3]> = layout.texels.iter().map(|t| {
        let tri = mesh.indices()[t.tri as usize].map(|v| mesh.vertices()[v as usize]);
        (off_edges(tri, Vec3::from(t.p), 4.0 * offset, half) + Vec3::from(t.n) * offset).to_array()
    }).collect();
    let inside = layout.texels.par_iter().zip(&origin).enumerate().map(|(i, (t, &o))| inside(&mesh, Vec3::from(o), Vec3::from(t.n), i as u32)).collect();
    Ok(Texels { layout, origin, inside, offset, mesh })
}

/// Whether a texel is inside other geometry: of its rays that meet anything, at least half meet a back face and more
/// meet back faces than front ones. A foot inside a closed floor slab meets back faces every way; one under a floor
/// that is a single plane meets its underside upwards and nothing downwards, and is inside too.
fn inside(mesh: &TriMesh, o: Vec3, n: Vec3, seed: u32) -> bool {
    let (mut back, mut front) = (0, 0);
    for j in 0..RAYS {
        let d = hemi(n, sample(seed, j));
        if let Some(h) = mesh.cast_local_ray_and_get_normal(&Ray::new(o, d), f32::MAX, false) {
            let [a, b, c] = mesh.indices()[h.subshape as usize].map(|v| mesh.vertices()[v as usize]);
            if (b - a).cross(c - a).dot(d) > 0.0 { back += 1 } else { front += 1 }
        }
    }
    back * 2 >= RAYS && back > front
}

/// p moved along its triangle until it's `gap` from each edge, or by `most` (half a texel: its own footprint; a needle
/// narrower than the gap would send it to its far end). A chart's
/// edge band takes its texels from the edge itself, and a point on the edge of a floor is in the plane of the wall
/// standing on it: its rays started in that wall and half of them left the room.
fn off_edges([a, b, c]: [Vec3; 3], p: Vec3, gap: f32, most: f32) -> Vec3 {
    let n = (b - a).cross(c - a);
    let area = n.length_squared().max(f32::MIN_POSITIVE);
    // barycentric weights, each raised to what stands `gap` from the edge opposite its corner (its height: area / edge)
    let w = [(c - b, b), (a - c, c), (b - a, a)].map(|(edge, from)| {
        let least = (gap * edge.length() / area.sqrt()).min(1.0 / 3.0);
        (edge.cross(p - from).dot(n) / area).max(least)
    });
    p + ((a * w[0] + b * w[1] + c * w[2]) / (w[0] + w[1] + w[2]) - p).clamp_length_max(most)
}

#[cfg(test)]
pub(crate) mod tests;
