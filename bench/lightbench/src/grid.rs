//! The light grid (docs/lightbench.feature LB3c): world cells over everything the lights reach, each listing the lights
//! whose range touches it, so a pixel weighs only its cell's lights rather than all of them. A light that doesn't touch
//! a cell can't reach any point in it, so choosing among the cell's lights is still unbiased; outside the grid no light
//! reaches at all. Built on the CPU each frame as the lights move (a count, then the lists, cell by cell).

use crate::scene::GpuLight;
use bevy::math::{UVec3, Vec3};

pub struct Grid {
    pub min: Vec3,
    pub cell: f32,
    pub dims: UVec3,
    /// cell c's lights are idx[start[c]..start[c + 1]]
    pub start: Vec<u32>,
    pub idx: Vec<u32>,
}

/// At most this many cells along an axis; the cell grows past `cell` to keep to it.
const MAX_DIM: f32 = 96.0;

pub fn build(lights: &[GpuLight], cell: f32) -> Grid {
    let (mut lo, mut hi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for l in lights {
        let p = Vec3::from(l.pos);
        lo = lo.min(p - l.range);
        hi = hi.max(p + l.range);
    }
    if lights.is_empty() {
        (lo, hi) = (Vec3::ZERO, Vec3::ONE);
    }
    let cell = cell.max((hi - lo).max_element() / MAX_DIM);
    let dims = ((hi - lo) / cell).ceil().max(Vec3::ONE).as_uvec3();
    let n = (dims.x * dims.y * dims.z) as usize;
    let at = |x: u32, y: u32, z: u32| ((z * dims.y + y) * dims.x + x) as usize;
    // each light's cells: those its range sphere touches
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    for (i, l) in lights.iter().enumerate() {
        let p = Vec3::from(l.pos);
        let a = ((p - l.range - lo) / cell).floor().max(Vec3::ZERO).as_uvec3().min(dims - 1);
        let b = ((p + l.range - lo) / cell).floor().max(Vec3::ZERO).as_uvec3().min(dims - 1);
        for z in a.z..=b.z {
            for y in a.y..=b.y {
                for x in a.x..=b.x {
                    let c0 = lo + Vec3::new(x as f32, y as f32, z as f32) * cell;
                    let near = p.clamp(c0, c0 + cell);
                    if near.distance_squared(p) <= l.range * l.range {
                        pairs.push((at(x, y, z) as u32, i as u32));
                    }
                }
            }
        }
    }
    let mut start = vec![0u32; n + 1];
    pairs.iter().for_each(|(c, _)| start[*c as usize + 1] += 1);
    for c in 0..n {
        start[c + 1] += start[c];
    }
    let mut fill = start.clone();
    let mut idx = vec![0u32; pairs.len().max(1)];
    for (c, i) in pairs {
        idx[fill[c as usize] as usize] = i;
        fill[c as usize] += 1;
    }
    Grid { min: lo, cell, dims, start, idx }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_light_that_reaches_a_point_is_in_its_cell() {
        let lights: Vec<GpuLight> = (0..300)
            .map(|i| {
                let f = i as f32;
                GpuLight { pos: [(f * 7.3).sin() * 20.0, 1.0 + (f * 1.1).cos(), (f * 3.7).cos() * 30.0 - 20.0], range: 2.0 + (f * 0.37).sin().abs() * 5.0, power: [1.0; 3], radius: 0.05 }
            })
            .collect();
        let g = build(&lights, 2.0);
        let mut checked = 0;
        for k in 0..20000 {
            let f = k as f32;
            let p = Vec3::new((f * 0.731).sin() * 24.0, (f * 0.217).cos() * 3.0 + 1.0, (f * 0.393).cos() * 36.0 - 20.0);
            let c = ((p - g.min) / g.cell).floor();
            let inside = c.cmpge(Vec3::ZERO).all() && c.cmplt(g.dims.as_vec3()).all();
            for (i, l) in lights.iter().enumerate() {
                if Vec3::from(l.pos).distance(p) < l.range {
                    assert!(inside, "{p} is reached by light {i} but is outside the grid");
                    let c = c.as_uvec3();
                    let cell = ((c.z * g.dims.y + c.y) * g.dims.x + c.x) as usize;
                    assert!(g.idx[g.start[cell] as usize..g.start[cell + 1] as usize].contains(&(i as u32)), "light {i} reaches {p} but isn't in its cell");
                    checked += 1;
                }
            }
        }
        assert!(checked > 1000, "{checked}");
        // and the lists are far shorter than all the lights
        let mean = g.idx.len() as f32 / (g.start.len() - 1) as f32;
        assert!(mean < 30.0, "{mean} lights a cell");
    }
}
