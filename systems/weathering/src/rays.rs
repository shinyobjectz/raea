//! Sky, sun, cavity, edge and ground over every texel, by rays through the model's own triangles (parry3d's BVH).
//!   sky     rain reach: rays up the rain's cone (its axis the way rain comes from, `Env::spread` wide), each seen one
//!           counting n.d, over what an open level roof gets; a wall facing the wind gets some, one under a deep
//!           cornice little, one facing away almost none
//!   sun     the share of the whole year's sun path the point sees, weighted by n.s (raea-sys-exposure)
//!   cavity  the share of short rays (CAVITY_REACH, cosine-weighted) that hit something: hollows, joints, corners
//!   edge    nearness to a convex arris, sharper edges counting more; a free edge (a panel's border with nothing
//!           continuing past it) counts as an arris, one running into another surface (a wall's foot) doesn't
//!   ground  the drop to the walking surface below: a ray down, through anything that isn't a walking class
//!   open    the share of the whole sky seen (cosine-weighted, any distance): where the air reaches whatever the rain
//!           does, so soot settles on a sheltered east front and a court's floor stays dark and damp
//! and causes3 (raea-sys-exposure, docs/wear.feature WR9): December's sun, a storm's driven snow and the litter traps.
//! Samples are a rotated R2 sequence per texel, so the result is the same on every run.

use std::collections::HashMap;

use raea_sys_exposure::cone;

use parry3d::glamx::Vec3;
use parry3d::query::{Ray, RayCast};
use parry3d::shape::TriMesh;
use rayon::prelude::*;

use raea_sys_exposure::{Driven, Exposer, Frame};

use super::{Env, Walk};
use crate::charts::Atlas;
use crate::scene::Scene;
pub(crate) use raea_sys_atlas::rays::{OFF, SKY_REACH, hash, hemi, sample};

const CAVITY_REACH: f32 = 0.5;
pub(crate) const GROUND_RANGE: f32 = 4.0;

struct Edges {
    segs: Vec<(Vec3, Vec3, f32)>,
    cells: HashMap<(i32, i32, i32), Vec<u32>>,
    cell: f32,
}

impl Edges {
    fn of(sc: &Scene, mesh: &TriMesh, reach: f32) -> Edges {
        let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let mut at: Vec<Vec3> = Vec::new();
        let mut id = |p: Vec3| {
            let k = ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64, (p.z * 1000.0).round() as i64);
            *ids.entry(k).or_insert_with(|| {
                at.push(p);
                (at.len() - 1) as u32
            })
        };
        let mut by: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (t, tri) in sc.tri.iter().enumerate() {
            if sc.nrm[t] == Vec3::ZERO {
                continue;
            }
            let w = tri.map(|v| id(sc.pos[v as usize]));
            for e in 0..3 {
                let (a, b) = (w[e], w[(e + 1) % 3]);
                if a != b {
                    by.entry((a.min(b), a.max(b))).or_default().push(t as u32);
                }
            }
        }
        let centre = |t: u32| {
            let v = sc.tri[t as usize];
            (sc.pos[v[0] as usize] + sc.pos[v[1] as usize] + sc.pos[v[2] as usize]) / 3.0
        };
        let mut keys: Vec<_> = by.keys().copied().collect();
        keys.sort_unstable();
        let mut segs = Vec::new();
        for k in keys {
            let ts = &by[&k];
            let (a, b) = (at[k.0 as usize], at[k.1 as usize]);
            let m = (a + b) / 2.0;
            let sharp = if ts.len() >= 2 {
                let (na, nb) = (sc.nrm[ts[0] as usize], sc.nrm[ts[1] as usize]);
                let convex = (centre(ts[1]) - m).dot(na) < -1e-4 || (centre(ts[0]) - m).dot(nb) < -1e-4;
                if !convex || na.dot(nb) > 20f32.to_radians().cos() { 0.0 } else { (1.0 - na.dot(nb)).clamp(0.0, 1.0) }
            } else {
                let n = sc.nrm[ts[0] as usize];
                let out = m - centre(ts[0]);
                let o = (out - n * out.dot(n)).normalize_or_zero();
                // a free edge, unless another surface carries on past it: from 3 cm past the edge and 3 cm off the
                // face, a ray back toward the edge's line meets it (a wall's foot meets the ground there)
                let back = Ray::new(m + o * 0.03 + n * 0.03, -o);
                if mesh.cast_local_ray(&back, 0.06, false).is_some() { 0.0 } else { 0.6 }
            };
            if sharp > 0.0 {
                segs.push((a, b, sharp));
            }
        }
        let cell = reach * 2.0;
        let mut cells: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
        for (i, (a, b, _)) in segs.iter().enumerate() {
            let (lo, hi) = (a.min(*b) - Vec3::splat(reach), a.max(*b) + Vec3::splat(reach));
            let c = |v: f32| (v / cell).floor() as i32;
            for x in c(lo.x)..=c(hi.x) {
                for y in c(lo.y)..=c(hi.y) {
                    for z in c(lo.z)..=c(hi.z) {
                        cells.entry((x, y, z)).or_default().push(i as u32);
                    }
                }
            }
        }
        Edges { segs, cells, cell }
    }

    fn near(&self, p: Vec3, reach: f32) -> f32 {
        let k = ((p.x / self.cell).floor() as i32, (p.y / self.cell).floor() as i32, (p.z / self.cell).floor() as i32);
        let Some(list) = self.cells.get(&k) else { return 0.0 };
        let mut best: f32 = 0.0;
        for &i in list {
            let (a, b, s) = self.segs[i as usize];
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
            let d = (p - (a + ab * t)).length();
            best = best.max(s * (1.0 - d / reach).max(0.0));
        }
        best
    }
}

pub(crate) fn causes(sc: &Scene, at: &Atlas, env: &Env, mesh: &TriMesh, c0: &mut [[u8; 4]], c2: &mut [[u8; 4]], c3: &mut [[u8; 4]]) {
    // an arris reaches a texel past itself, and no less than 8 cm
    let reach_of = |tri: u32| (1.0 / at.chart_dens[at.chart_of[tri as usize] as usize]).max(0.08);
    let widest = at.chart_dens.iter().map(|d| (1.0 / d).max(0.08)).fold(0.08, f32::max);
    let edges = Edges::of(sc, mesh, widest);
    let frame = Frame { up: env.up, north: env.north, lat: env.lat };
    let ex = Exposer::new(mesh, &frame, Driven { dir: env.snow, spread: env.snow_spread }, env.sky_rays);
    let from = -env.rain;
    let k = env.sky_rays;
    let level: f32 = (0..k).map(|j| cone(from, env.spread, sample(0, j)).dot(env.up).max(0.0)).sum::<f32>() / k as f32;
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let hit = |o: Vec3, d: Vec3, far: f32| mesh.cast_local_ray(&Ray::new(o, d), far, false).is_some();
    let vals: Vec<([u8; 4], [u8; 4], [u8; 4])> = at
        .texels
        .par_iter()
        .enumerate()
        .map(|(i, t)| {
            let (p, n, i) = (t.p, t.n, i as u32);
            let o = p + n * OFF;
            let mut wet = 0.0;
            for j in 0..k {
                let d = cone(from, env.spread, sample(i, j));
                let c = n.dot(d);
                if c > 0.0 && !hit(o, d, SKY_REACH) {
                    wet += c;
                }
            }
            let sky = if level > 0.0 { wet / k as f32 / level } else { 0.0 };
            let x = ex.at(i, p, n);
            let kc = env.cavity_rays;
            let occl = (0..kc).filter(|&j| hit(o, hemi(n, sample(i, 2000 + j)), CAVITY_REACH)).count() as f32 / kc as f32;
            // open: the cosine-weighted share of the sky the point sees at any distance (a court's floor little, a
            // roof all), whatever the rain does
            let open = (0..kc).filter(|&j| !hit(o, hemi(n, sample(i, 3000 + j)), SKY_REACH)).count() as f32 / kc as f32;
            let edge = edges.near(p, reach_of(t.tri));
            let ground = ground(sc, mesh, env, t.tri, p, n);
            let element = (crate::rays::hash(at.chart_of[t.tri as usize].wrapping_add(17)) & 0xff) as u8;
            // a: the crack, raea-sys-damage's (touch.rs)
            let c3 = [byte(x.december), byte(x.drift), byte(x.litter), 0];
            ([byte(sky), byte(x.sun), byte(occl), byte(edge)], [byte(ground / GROUND_RANGE), element, byte(open), sc.class[t.tri as usize].code()], c3)
        })
        .collect();
    for (t, (a, b, c)) in at.texels.iter().zip(vals) {
        let i = t.y as usize * at.w as usize + t.x as usize;
        c0[i] = a;
        c2[i] = b;
        c3[i] = c;
    }
}

/// The drop from p to the walking surface below, in metres (GROUND_RANGE when there is none within reach).
fn ground(sc: &Scene, mesh: &TriMesh, env: &Env, tri: u32, p: Vec3, n: Vec3) -> f32 {
    match &env.walk {
        Walk::Level(y) => (p.dot(env.up) - y).max(0.0),
        Walk::Classes(walk) => {
            if walk.contains(&sc.class[tri as usize]) && n.dot(env.up) > 0.5 {
                return 0.0;
            }
            let mut o = p + n * OFF;
            for _ in 0..8 {
                let Some(h) = mesh.cast_local_ray_and_get_normal(&Ray::new(o, -env.up), 100.0, false) else { return GROUND_RANGE };
                let q = o - env.up * h.time_of_impact;
                if walk.contains(&sc.class[h.subshape as usize]) {
                    return (p - q).dot(env.up).max(0.0);
                }
                o = q - env.up * 0.01;
            }
            GROUND_RANGE
        }
    }
}
