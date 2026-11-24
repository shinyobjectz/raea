//! Charts joined where the flood fill can't join them. A wall cut into strips by its openings shares no edges between
//! its strips (T-junctions), so each strip would be a chart with gutters of its own: coplanar charts whose rectangles
//! touch become one, and the wall's streaks then run on without a seam. Details too small for texels of their own at
//! the bake's density (a reveal, a frame's face, a bracket) take the texels of the big chart they sit on, by
//! projection on its plane: a reveal then reads the wall at its opening's edge, a frame the wall under it.

use std::collections::HashMap;

use parry3d::glamx::Vec3;
use raea_core_meshcheck::find;

use crate::Ask;
use crate::charts::Chart;
use crate::scene::Scene;

/// rectangles closer than this (metres, in their plane) touch
const TOUCH: f32 = 0.02;
/// a small chart's host is a big chart whose plane is within this of the small chart's centre
const HOST_REACH: f32 = 0.35;
/// under this many texels across, or this area in texels, a chart is a detail
const THIN: f32 = 1.5;
const AREA: f32 = 6.0;
/// a sparse chart is cut into cells this many texels square
const SPLIT: f32 = 64.0;


/// Coplanar charts (the same plane to 1/64 in the normal and 2 cm in offset) whose rectangles touch, joined.
pub fn merge(cs: Vec<Chart>, sc: &Scene, up: Vec3) -> Vec<Chart> {
    let q = |v: f32| (v * 64.0).round() as i32;
    let mut groups: HashMap<(i32, i32, i32, i64), Vec<u32>> = HashMap::new();
    for (i, c) in cs.iter().enumerate() {
        if c.n != Vec3::ZERO {
            groups.entry((q(c.n.x), q(c.n.y), q(c.n.z), (c.off / 0.02).round() as i64)).or_default().push(i as u32);
        }
    }
    let mut parent: Vec<u32> = (0..cs.len() as u32).collect();
    let mut keys: Vec<_> = groups.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let mut g = groups.remove(&k).unwrap();
        g.sort_by(|&a, &b| cs[a as usize].s0.total_cmp(&cs[b as usize].s0).then(a.cmp(&b)));
        // a sweep along s: a chart can only touch those that start before it ends
        for (x, &a) in g.iter().enumerate() {
            let ca = &cs[a as usize];
            for &b in &g[x + 1..] {
                let cb = &cs[b as usize];
                if cb.s0 > ca.s1 + TOUCH {
                    break;
                }
                if cb.t0 <= ca.t1 + TOUCH && ca.t0 <= cb.t1 + TOUCH {
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    if ra != rb {
                        parent[ra.max(rb) as usize] = ra.min(rb);
                    }
                }
            }
        }
    }
    let mut members: Vec<Vec<u32>> = vec![Vec::new(); cs.len()];
    for i in 0..cs.len() as u32 {
        let r = find(&mut parent, i);
        members[r as usize].push(i);
    }
    let mut taken: Vec<Option<Chart>> = cs.into_iter().map(Some).collect();
    let mut out = Vec::new();
    for (r, m) in members.into_iter().enumerate() {
        if m.is_empty() {
            continue;
        }
        let mut c = taken[r].take().unwrap();
        for &o in &m[1..] {
            c.tris.extend(taken[o as usize].take().unwrap().tris);
        }
        c.tris.sort_unstable();
        c.orient(sc, up);
        out.push(c);
    }
    out
}

/// Charts whose triangles cover less than half their rectangle (a street network round its blocks, a quarter's gutters
/// at one eaves height joined where they cross), cut on a grid SPLIT texels square: each cell's triangles, by their
/// centres, a chart in the same plane.
pub fn split(cs: Vec<Chart>, sc: &Scene, env: &Ask) -> Vec<Chart> {
    let mut out = Vec::new();
    for c in cs {
        let d = c.asked(sc, env);
        let cell = SPLIT / d;
        let rect = ((c.s1 - c.s0) * d).max(1.0) * ((c.t1 - c.t0) * d).max(1.0);
        if c.n == Vec3::ZERO || rect < 4.0 * SPLIT * SPLIT {
            out.push(c);
            continue;
        }
        let st = |t: u32| sc.tri[t as usize].map(|v| c.st(sc.pos[v as usize]));
        let filled = c
            .tris
            .iter()
            .map(|&t| {
                let [a, b, e] = st(t);
                ((b.0 - a.0) * (e.1 - a.1) - (e.0 - a.0) * (b.1 - a.1)).abs() / 2.0
            })
            .sum::<f32>()
            * d
            * d;
        if filled >= 0.5 * rect {
            out.push(c);
            continue;
        }
        let mut cells: std::collections::BTreeMap<(i32, i32), Vec<u32>> = Default::default();
        for &t in &c.tris {
            let [a, b, e] = st(t);
            let (s, tt) = ((a.0 + b.0 + e.0) / 3.0, (a.1 + b.1 + e.1) / 3.0);
            cells.entry((((s - c.s0) / cell).floor() as i32, ((tt - c.t0) / cell).floor() as i32)).or_default().push(t);
        }
        for (_, tris) in cells {
            out.push(Chart::new(tris, c.n, c.off, (c.u, c.b), sc, env.up));
        }
    }
    out
}

/// Each chart too small for texels of its own at density d, moved into the nearest big chart within HOST_REACH of
/// its plane (and over its rectangle); a detail with no host keeps its own chart. Hosts are refitted.
pub fn absorb(cs: Vec<Chart>, sc: &Scene, env: &Ask) -> Vec<Chart> {
    let ds: Vec<f32> = cs.iter().map(|c| c.asked(sc, env)).collect();
    let small = |c: &Chart, d: f32| {
        let (w, h) = ((c.s1 - c.s0) * d, (c.t1 - c.t0) * d);
        c.n != Vec3::ZERO && (w.min(h) < THIN || w * h < AREA)
    };
    let bounds = |c: &Chart| {
        let ps = || c.tris.iter().flat_map(|&t| sc.tri[t as usize].map(|v| sc.pos[v as usize]));
        (ps().fold(Vec3::MAX, Vec3::min), ps().fold(Vec3::MIN, Vec3::max))
    };
    const CELL: f32 = 4.0;
    let cell = |v: Vec3| {
        let c = (v / CELL).floor().as_ivec3();
        (c.x, c.y, c.z)
    };
    let mut grid: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
    let is_small: Vec<bool> = cs.iter().zip(&ds).map(|(c, &d)| small(c, d)).collect();
    for (i, c) in cs.iter().enumerate() {
        if is_small[i] || c.n == Vec3::ZERO {
            continue;
        }
        let (lo, hi) = bounds(c);
        let (a, b) = (cell(lo - Vec3::splat(HOST_REACH)), cell(hi + Vec3::splat(HOST_REACH)));
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for z in a.2..=b.2 {
                    grid.entry((x, y, z)).or_default().push(i as u32);
                }
            }
        }
    }
    let mut host = vec![u32::MAX; cs.len()];
    for (i, c) in cs.iter().enumerate() {
        if !is_small[i] {
            continue;
        }
        let (lo, hi) = bounds(c);
        let mid = (lo + hi) / 2.0;
        let mut best: Option<(f32, f32, u32)> = None;
        for &h in grid.get(&cell(mid)).into_iter().flatten() {
            let hc = &cs[h as usize];
            let dist = (hc.n.dot(mid) - hc.off).abs();
            let (s, t) = hc.st(mid);
            let over = s >= hc.s0 - HOST_REACH && s <= hc.s1 + HOST_REACH && t >= hc.t0 - HOST_REACH && t <= hc.t1 + HOST_REACH;
            let area = (hc.s1 - hc.s0) * (hc.t1 - hc.t0);
            if over && dist <= HOST_REACH && best.is_none_or(|(bd, ba, _)| dist < bd - 1e-4 || ((dist - bd).abs() <= 1e-4 && area > ba)) {
                best = Some((dist, area, h));
            }
        }
        if let Some((_, _, h)) = best {
            host[i] = h;
        }
    }
    let mut taken: Vec<Option<Chart>> = cs.into_iter().zip(&ds).map(|(mut c, &d)| {
        c.dens = d;
        Some(c)
    }).collect();
    for i in 0..taken.len() {
        if host[i] != u32::MAX {
            let tris = taken[i].take().unwrap().tris;
            // a host's own triangles stay first, so its own surface wins a texel both cover
            taken[host[i] as usize].as_mut().unwrap().tris.extend(tris);
        }
    }
    let mut out: Vec<Chart> = taken.into_iter().flatten().collect();
    for c in &mut out {
        c.fit(sc);
    }
    out
}
