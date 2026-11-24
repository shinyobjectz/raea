//! Adaptive geometry (docs/lightbench.feature LB2): detail chosen by how many pixels its error covers,
//! projected_px = focal_px * error * scale / depth, where depth is the distance to the bounds' near side (so a camera
//! inside them asks for full detail).
//!
//! Two ways, the second built on the first's checks: discrete LODs (the whole mesh simplified by halves, the coarsest
//! whose error stays under the threshold drawn), and a cluster hierarchy in the manner of Nanite. Its clusters of up to
//! 128 triangles are grouped by meshopt, each group simplified to half with its border locked, and split again into
//! the next level's clusters. Every cluster keeps its own error and bounds and its parent group's, both monotonic up
//! the hierarchy, so a cluster is drawn exactly when its own error projects within the threshold and its parent's
//! doesn't: a parent or its children, never both, and the locked borders keep the cut crack-free.

use bevy::math::Vec3;
use meshopt::{SimplifyOptions, VertexDataAdapter};

fn adapter(pos: &[Vec3]) -> VertexDataAdapter<'_> {
    VertexDataAdapter::new(bytemuck::cast_slice(pos), 12, 0).expect("positions")
}

/// The pixels an error of `error` metres covers at bounds (`center`, `radius`) scaled by `scale`, from `eye`.
pub fn projected_px(error: f32, center: Vec3, radius: f32, scale: f32, eye: Vec3, focal_px: f32) -> f32 {
    if !error.is_finite() {
        return f32::INFINITY;
    }
    let depth = (center.distance(eye) - radius * scale).max(1e-3);
    focal_px * error * scale / depth
}

/// The focal length in pixels of a vertical field of view over `height` pixels.
pub fn focal_px(fov_y: f32, height: f32) -> f32 {
    height * 0.5 / (fov_y * 0.5).tan()
}

fn sphere(pos: &[Vec3], idx: &[u32]) -> (Vec3, f32) {
    let (mut lo, mut hi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    idx.iter().for_each(|&i| {
        lo = lo.min(pos[i as usize]);
        hi = hi.max(pos[i as usize]);
    });
    let c = (lo + hi) * 0.5;
    (c, idx.iter().map(|&i| pos[i as usize].distance(c)).fold(0.0, f32::max))
}

/// One discrete level: its triangles and its error in metres (object space).
pub struct Level {
    pub indices: Vec<u32>,
    pub error: f32,
}

/// The mesh, then halves of it, until simplifying stops halving or `levels` are made.
pub fn chain(pos: &[Vec3], idx: &[u32], levels: usize) -> Vec<Level> {
    let a = adapter(pos);
    let mut out = vec![Level { indices: idx.to_vec(), error: 0.0 }];
    while out.len() < levels {
        let last = out.last().unwrap();
        let mut err = 0.0;
        let next = meshopt::simplify(&last.indices, &a, last.indices.len() / 2, f32::MAX, SimplifyOptions::ErrorAbsolute, Some(&mut err));
        if next.len() as f32 > last.indices.len() as f32 * 0.8 {
            break;
        }
        let error = err.max(last.error);
        out.push(Level { indices: next, error });
    }
    out
}

/// The coarsest level whose error projects within `tau` pixels.
pub fn pick(levels: &[Level], center: Vec3, radius: f32, scale: f32, eye: Vec3, focal: f32, tau: f32) -> usize {
    (0..levels.len()).rev().find(|&l| projected_px(levels[l].error, center, radius, scale, eye, focal) <= tau).unwrap_or(0)
}

#[derive(Clone, Debug)]
pub struct Cluster {
    pub indices: Vec<u32>,
    pub level: u32,
    /// its own error and bounds (the group it was simplified in; zero error at the finest level)
    pub error: f32,
    pub center: Vec3,
    pub radius: f32,
    /// its parent group's (infinite error at the roots)
    pub parent_error: f32,
    pub parent_center: Vec3,
    pub parent_radius: f32,
}

fn clusters_of(pos: &[Vec3], idx: &[u32]) -> Vec<Vec<u32>> {
    let m = meshopt::build_meshlets(idx, &adapter(pos), 128, 128, 0.0);
    m.iter().map(|c| c.triangles.iter().map(|&t| c.vertices[usize::from(t)]).collect()).collect()
}

/// The named clusters' triangles as one index list.
pub fn indices_of(all: &[Cluster], which: &[usize]) -> Vec<u32> {
    which.iter().flat_map(|&c| all[c].indices.iter().copied()).collect()
}

/// The cluster hierarchy of a closed mesh.
pub fn dag(pos: &[Vec3], idx: &[u32]) -> Vec<Cluster> {
    let a = adapter(pos);
    let mut all: Vec<Cluster> = Vec::new();
    let mut level: Vec<usize> = Vec::new();
    for c in clusters_of(pos, idx) {
        let (center, radius) = sphere(pos, &c);
        level.push(all.len());
        all.push(Cluster { indices: c, level: 0, error: 0.0, center, radius, parent_error: f32::INFINITY, parent_center: center, parent_radius: radius });
    }
    for depth in 1.. {
        if level.len() <= 1 {
            break;
        }
        // groups of about four clusters that share vertices
        let counts: Vec<u32> = level.iter().map(|&c| all[c].indices.len() as u32).collect();
        let flat = indices_of(&all, &level);
        let mut part = vec![0u32; level.len()];
        let groups = meshopt::partition_clusters(&mut part, &flat, &counts, pos.len(), 4);
        let mut next = Vec::new();
        for g in 0..groups {
            let members: Vec<usize> = level.iter().zip(&part).filter(|(_, p)| **p as usize == g).map(|(c, _)| *c).collect();
            let merged = indices_of(&all, &members);
            let mut err = 0.0;
            let simple = meshopt::simplify(&merged, &a, merged.len() / 2, f32::MAX, SimplifyOptions::LockBorder | SimplifyOptions::ErrorAbsolute, Some(&mut err));
            // a group that won't halve stays as it is: its clusters are roots
            if members.len() < 2 || simple.len() as f32 > merged.len() as f32 * 0.85 {
                continue;
            }
            // the group's bounds hold its children's, and its error is at least theirs: both only grow upward
            let (center, mut radius) = sphere(pos, &merged);
            for &c in &members {
                radius = radius.max(center.distance(all[c].center) + all[c].radius);
            }
            let error = members.iter().map(|&c| all[c].error).fold(err, f32::max);
            for &c in &members {
                (all[c].parent_error, all[c].parent_center, all[c].parent_radius) = (error, center, radius);
            }
            for c in clusters_of(pos, &simple) {
                next.push(all.len());
                all.push(Cluster { indices: c, level: depth, error, center, radius, parent_error: f32::INFINITY, parent_center: center, parent_radius: radius });
            }
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    all
}

/// The clusters to draw: each whose own error projects within `tau` and whose parent's doesn't.
pub fn select(dag: &[Cluster], scale: f32, eye: Vec3, focal: f32, tau: f32) -> Vec<usize> {
    (0..dag.len())
        .filter(|&i| {
            let c = &dag[i];
            projected_px(c.error, c.center, c.radius, scale, eye, focal) <= tau && projected_px(c.parent_error, c.parent_center, c.parent_radius, scale, eye, focal) > tau
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn knot() -> (Vec<Vec3>, Vec<u32>) {
        let (p, _, i) = crate::scene::knot_data(2, 3, 512, 16, 0.33);
        (p, i)
    }

    #[test]
    fn farther_is_coarser_and_a_narrower_view_finer() {
        let (p, i) = knot();
        let levels = chain(&p, &i, 8);
        assert!(levels.len() >= 5, "{} levels", levels.len());
        assert!(levels.windows(2).all(|w| w[1].error >= w[0].error && w[1].indices.len() < w[0].indices.len()));
        let f45 = focal_px(45f32.to_radians(), 720.0);
        let at = |d: f32, f: f32| pick(&levels, Vec3::ZERO, 3.3, 0.5, Vec3::new(0.0, 0.0, d), f, 1.0);
        let by_distance: Vec<usize> = [2.0, 5.0, 10.0, 20.0, 40.0, 80.0].iter().map(|&d| at(d, f45)).collect();
        assert!(by_distance.windows(2).all(|w| w[1] >= w[0]) && by_distance[5] > by_distance[0], "{by_distance:?}");
        let f15 = focal_px(15f32.to_radians(), 720.0);
        assert!(at(20.0, f15) < at(20.0, f45), "15 deg {} 45 deg {}", at(20.0, f15), at(20.0, f45));
        // a camera inside the bounds asks for full detail
        assert_eq!(at(0.5, f45), 0);
    }

    #[test]
    fn the_hierarchy_draws_a_parent_or_its_children_and_never_cracks() {
        let (p, i) = knot();
        let dag = dag(&p, &i);
        let top = dag.iter().map(|c| c.level).max().unwrap();
        assert!(top >= 3, "{top} levels");
        for c in &dag {
            assert!(c.parent_error >= c.error && c.parent_radius >= c.radius * 0.999, "{c:?}");
        }
        let f = focal_px(45f32.to_radians(), 720.0);
        let full = i.len() / 3;
        let mut drawn = Vec::new();
        for (d, tau) in [(1.0, 1.0), (4.0, 1.0), (12.0, 1.0), (30.0, 1.0), (90.0, 1.0), (12.0, 4.0), (12.0, 0.25), (12.0, 0.0)] {
            let eye = Vec3::new(0.3, 0.2, d);
            let pick = select(&dag, 0.5, eye, f, tau);
            // a closed mesh: every edge of what's drawn is shared by exactly two triangles
            let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
            let mut tris = 0;
            for &c in &pick {
                for t in dag[c].indices.chunks_exact(3) {
                    tris += 1;
                    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                        *edges.entry((a.min(b), a.max(b))).or_default() += 1;
                    }
                }
            }
            let open = edges.values().filter(|&&n| n != 2).count();
            assert_eq!(open, 0, "at {d} m, {tau} px: {open} edges not shared by two triangles");
            drawn.push(tris);
        }
        // a threshold of nothing draws every finest triangle; up close, the knot's near side is finest, its far side not
        assert_eq!(drawn[7], full);
        assert!(drawn[0] > full / 2, "{drawn:?}");
        assert!(drawn[0] >= drawn[1] && drawn[1] >= drawn[2] && drawn[2] >= drawn[3] && drawn[3] >= drawn[4] && drawn[4] < full / 8, "{drawn:?}");
        // a looser threshold draws fewer, a tighter one more
        assert!(drawn[5] <= drawn[2] && drawn[6] >= drawn[2], "{drawn:?}");
        println!("triangles drawn (full {full}): {drawn:?}");
    }
}
