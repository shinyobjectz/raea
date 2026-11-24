//! Parts that pass through each other where the camera sees it: a statue sunk into a column, a bust half inside its
//! plinth. Each mesh and glTF model is voxelized once in its own space (its solid: the cells the outside can't reach,
//! with how deep each one is), and on every frame each pair whose world boxes meet is tested by carrying the smaller
//! part's cells within their common box into the other's grid. A pair is reported when the overlap is deep for the smaller part's size,
//! a real share of it, and where the two surfaces meet (the seam) can be seen: inside the frame, nearer than the fog's
//! far edge, and not hidden behind any part (a ray from each seam point to the camera, marched through every part's
//! solid). Once per pair over each stretch of frames it lasts, with how far and which way to move it. A model is split
//! into its meshes (each node its own solid), so a ruin's thin column is resolved as finely as a statue.
//!
//! Not every overlap is a fault. Resting contact is shallow (a paw on the floor, a bust on its plinth); a part seated
//! into what's below it (a column's foot in the ground, the overlap confined to the bottom of the smaller part) is how
//! models are placed; a flat plane has no depth; see-through, unlit and `allowOverlap` parts (a blade into a body, a
//! ghost) are left out, and so are models that animate (their pose isn't known here) and files that can't be read.
//! The camera is the frame's first PerspectiveCamera (fov vertical); a letterbox drawn over the 3D isn't known here.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use bevy::math::{Mat4, Vec3};
use serde_json::Value;

use raea_core_scene::{Item, excused, gltf_tris, src_path, vec3};

// what each part rests on, and the parts that float, sink or clip (`raea scene lint|explain`)
pub mod lint;
pub use lint::lint;
// a model's box and a seat's facing (scene.bounds, scene.facing)
mod assets;
pub use assets::assets;

mod found;
pub use found::{clip_findings, lint_findings};

// the check a frame at a time, as a render's frames come (SG4a)
mod clip;
pub use clip::Clipping;

pub use raea_core_modelbox::{bounds, facing, seat_facing};

/// Lets go of every file's triangles and every part's solids this process made, which a long render's clipping check
/// held to its end: on a 7-minute film, gigabytes still held while frame purity rendered its stills (SG4a).
pub fn forget() {
    raea_core_modelbox::forget();
    solid::forget_solids();
}

mod solid;
use solid::*;
mod seen;
#[cfg(test)]
use seen::*;

/// Cells along a part's longest side.
const RES: f32 = 56.0;
/// A thin part (a sill, a shelf, a panel: a cell or two thick at RES) also gets a fine solid, this many cells across
/// its thinnest side, for the pierce test's depths, at most THIN_MOST times RES along its longest.
const THIN: f32 = 12.0;
const THIN_MOST: f32 = 8.0;
/// The pierce test's points on a part's real surface: one per this share of a cell.
const SURF_GRAIN: f32 = 1.0 / 3.0;
/// The most of the smaller part's cells scanned per test, within the two parts' common box (a stride over the rest).
const SAMPLES: usize = 160_000;
/// Deep enough to see: this share of the smaller part's diagonal ...
const DEPTH_SHARE: f32 = 0.04;
/// ... and this share of its inside cells in the other.
const VOLUME_SHARE: f32 = 0.02;
/// More of it inside than this, and it's hidden in the other: nothing of the overlap shows.
const ENCLOSED: f32 = 0.97;
/// A surface in a part (the pierce test): this deep, as a share of the part's thinnest side, and at least PIERCE_MIN
/// metres, at PIERCE_POINTS points or more. A tap through a 3 cm sill is ~1.5 cm in (the middle reads a little less: its
/// surface was thickened a cell); a cup set on it, ~0.
const PIERCE: f32 = 0.15;
const PIERCE_MIN: f32 = 0.004;
const PIERCE_POINTS: usize = 4;
/// A part smaller than this across (metres) is a particle: a spark, an ember, a mote.
const PARTICLE: f32 = 0.08;
/// An overlap within the bottom of the smaller part (this share of its height) is it seated in what's below.
const SEATED: f32 = 0.3;

#[derive(Clone)]
struct Hit {
    depth: f32,
    share: f32,
    /// whether the first of the pair is the smaller (carried into the other)
    small_first: bool,
    /// which piece of each part
    pieces: (usize, usize),
    /// where the two surfaces meet, world: what a viewer would see of it
    seam: Vec<Vec3>,
}

/// World box of a solid.
fn world_box(s: &Solid, w: &Mat4) -> (Vec3, Vec3) {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for k in 0..8 {
        let c = Vec3::new(if k & 1 == 0 { s.lo.x } else { s.hi.x }, if k & 2 == 0 { s.lo.y } else { s.hi.y }, if k & 4 == 0 { s.lo.z } else { s.hi.z });
        let p = w.transform_point3(c);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo, hi)
}

fn scale_of(w: &Mat4) -> f32 {
    let (s, _, _) = w.to_scale_rotation_translation();
    (s.x.abs() * s.y.abs() * s.z.abs()).cbrt()
}

/// One pair of solids on one frame: the overlap of the smaller one's inside with the other's (volume), else one's real
/// surface deep in the other (pierce: a thin piece of a big model, a tap, through a thin part, a sill, that the
/// volume test sees too coarsely), or None if it's contact, a seat, or nothing.
fn test(a: &Solid, wa: &Mat4, b: &Solid, wb: &Mat4) -> Option<Hit> {
    volume(a, wa, b, wb).or_else(|| {
        // whichever goes deeper into which, as a share of what it goes into
        let ab = pierce(a, wa, b, wb).map(|h| Hit { small_first: true, ..h });
        let ba = pierce(b, wb, a, wa).map(|h| Hit { small_first: false, ..h });
        match (ab, ba) {
            (Some(x), Some(y)) => Some(if x.depth >= y.depth { x } else { y }),
            (x, y) => x.or(y),
        }
    })
}

/// `x` through `into` across its thinnest side, its real surface deep inside (points at least PIERCE of that side under
/// its surface),
/// unless that's `x`'s foot seated in what it stands on, or `x` wholly inside (hidden). The Hit names `x` first.
fn pierce(x: &Solid, wx: &Mat4, into: &Solid, wi: &Mat4) -> Option<Hit> {
    let (xlo, xhi) = world_box(x, wx);
    let (ilo, ihi) = world_box(into, wi);
    let (lo, hi) = (xlo.max(ilo), xhi.min(ihi));
    if lo.cmpgt(hi).any() || x.surf.is_empty() {
        return None;
    }
    // across, through and round in `into`'s own space (a bay's angled face is no box in the world's)
    let ki = scale_of(wi);
    let to_i = wi.inverse();
    let ext = into.hi - into.lo;
    let need = (PIERCE * ext.min_element() * ki).max(PIERCE_MIN);
    // through it: out both sides across its thinnest (a tap through a sill), not an end stopping in it (a wall's in the
    // next at a corner, a sill's in the wall, hidden in the joint)
    let t = if ext.x <= ext.y && ext.x <= ext.z { 0 } else if ext.y <= ext.z { 1 } else { 2 };
    let (xl, xh) = world_box(x, &(to_i * *wx));
    if xl[t] > into.lo[t] - need / ki || xh[t] < into.hi[t] + need / ki {
        return None;
    }
    let deep = into.fine.as_deref().unwrap_or(into);
    let (mut n, mut depth, mut top, mut bottom) = (0usize, 0f32, f32::MIN, f32::MAX);
    let (mut dlo, mut dhi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut seam = Vec::new();
    for p in &x.surf {
        let w = wx.transform_point3(*p);
        if w.cmplt(lo).any() || w.cmpgt(hi).any() {
            continue;
        }
        let l = to_i.transform_point3(w);
        let d = deep.metres(deep.at(l)) * ki;
        if d >= need {
            n += 1;
            depth = depth.max(d);
            top = top.max(w.y);
            bottom = bottom.min(w.y);
            dlo = dlo.min(l);
            dhi = dhi.max(l);
            seam.push(w);
        }
    }
    // a sliver, or x hidden in it
    if n < PIERCE_POINTS || n as f32 > ENCLOSED * x.surf.len() as f32 {
        return None;
    }
    // and through its face, `into` all round it (a frame lapping the cut end of a wall, a sash on its post, is set in it)
    let m = ext[t].max(need / ki);
    if (0..3).any(|u| u != t && (dlo[u] < into.lo[u] + m || dhi[u] > into.hi[u] - m)) {
        return None;
    }
    // x's foot seated in what's under it
    if top <= xlo.y + SEATED * (xhi.y - xlo.y) && ihi.y <= xlo.y + SEATED * (xhi.y - xlo.y) + into.cell * ki * 2.0 {
        return None;
    }
    // or what's on x seated in it: x's top face up in the other's foot
    let foot = ilo.y + SEATED * (ihi.y - ilo.y);
    if bottom >= ilo.y - into.cell * ki * 2.0 && top <= foot && xhi.y <= foot + x.cell * scale_of(wx) * 2.0 {
        return None;
    }
    let share = n as f32 / x.surf.len() as f32;
    let keep = (seam.len() / 64).max(1);
    Some(Hit { depth, share, small_first: true, pieces: (0, 0), seam: seam.into_iter().step_by(keep).collect() })
}

/// The volume test: the smaller part's inside cells in the other's.
fn volume(a: &Solid, wa: &Mat4, b: &Solid, wb: &Mat4) -> Option<Hit> {
    let (alo, ahi) = world_box(a, wa);
    let (blo, bhi) = world_box(b, wb);
    if alo.cmpgt(bhi).any() || blo.cmpgt(ahi).any() {
        return None;
    }
    let (sa, sb) = (scale_of(wa), scale_of(wb));
    let vol = |s: &Solid, k: f32| s.volume() * k.powi(3);
    let small_first = vol(a, sa) <= vol(b, sb);
    let (s, ws, ks, o, wo, ko) = if small_first { (a, wa, sa, b, wb, sb) } else { (b, wb, sb, a, wa, sa) };
    if s.inside.is_empty() {
        return None;
    }
    let (slo, shi) = world_box(s, ws);
    let size = (shi - slo).length();
    if size < PARTICLE {
        return None;
    }
    // no point of the overlap is deeper than half the boxes' common part is thin: two parts side by side stop here
    let (ilo, ihi) = (alo.max(blo), ahi.min(bhi));
    if (ihi - ilo).min_element() < DEPTH_SHARE * size {
        return None;
    }
    // the smaller part's cells within the common part, in its own grid
    let to_s = ws.inverse();
    let (mut llo, mut lhi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for k in 0..8 {
        let c = to_s.transform_point3(Vec3::new(if k & 1 == 0 { ilo.x } else { ihi.x }, if k & 2 == 0 { ilo.y } else { ihi.y }, if k & 4 == 0 { ilo.z } else { ihi.z }));
        llo = llo.min(c);
        lhi = lhi.max(c);
    }
    let cell_of = |v: f32, k: usize| (((v - s.min[k]) / s.cell).floor().max(0.0) as usize).min(s.n[k] - 1);
    let (x0, x1, y0, y1, z0, z1) = (cell_of(llo.x, 0), cell_of(lhi.x, 0), cell_of(llo.y, 1), cell_of(lhi.y, 1), cell_of(llo.z, 2), cell_of(lhi.z, 2));
    let range = (x1 - x0 + 1) * (y1 - y0 + 1) * (z1 - z0 + 1);
    let stride = (range / SAMPLES).max(1);
    let to_o = wo.inverse();
    let (mut hits, mut depth, mut seen) = (0usize, 0f32, 0usize);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut seam = Vec::new();
    for z in z0..=z1 {
        for y in y0..=y1 {
            for x in x0..=x1 {
                seen += 1;
                if seen % stride != 0 {
                    continue;
                }
                let i = ((z * s.n[1] + y) * s.n[0] + x) as u32;
                let ds = s.depth[i as usize];
                if ds < 3 {
                    continue;
                }
                let p = ws.transform_point3(s.centre(i));
                let d = o.at(to_o.transform_point3(p));
                if d < 3 {
                    continue;
                }
                hits += 1;
                lo = lo.min(p);
                hi = hi.max(p);
                depth = depth.max((s.metres(ds) * ks).min(o.metres(d) * ko));
                // near both surfaces: the line where one goes into the other
                if ds <= 4 && d <= 4 {
                    seam.push(p);
                }
            }
        }
    }
    let share = (hits * stride) as f32 / s.inside.len() as f32;
    // shallow, a sliver, or wholly inside (no seam to see: a decal in a decal, a part hidden in another)
    if depth < DEPTH_SHARE * size || share < VOLUME_SHARE || share > ENCLOSED {
        return None;
    }
    // seated in what's below it: the overlap kept to the smaller part's foot, and the other part under it
    let (_, ohi) = world_box(o, wo);
    if hi.y <= slo.y + SEATED * (shi.y - slo.y) && ohi.y <= slo.y + SEATED * (shi.y - slo.y) + s.cell * ks * 2.0 {
        return None;
    }
    let keep = (seam.len() / 64).max(1);
    let seam: Vec<Vec3> = seam.into_iter().step_by(keep).collect();
    Some(Hit { depth, share, small_first, pieces: (0, 0), seam })
}

/// The deepest overlap of any piece of one part (`a`, its solids at `wa`) in any of the other's, and which two pieces.
fn deepest(a: &[Solid], wa: &Mat4, b: &[Solid], wb: &Mat4) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for (na, sa) in a.iter().enumerate() {
        for (nb, sb) in b.iter().enumerate() {
            if let Some(mut hit) = test(sa, wa, sb, wb)
                && best.as_ref().is_none_or(|b| hit.depth > b.depth)
            {
                hit.pieces = (na, nb);
                best = Some(hit);
            }
        }
    }
    best
}

/// The shortest move of one part (the first if `move_a`) along a world axis (+x, -x, +y, +z, -z) that leaves no piece
/// of it in a piece of the other: the axis and the signed distance (metres).
fn clear_move(a: &[Solid], wa: &Mat4, b: &[Solid], wb: &Mat4, move_a: bool) -> (usize, f32) {
    let span = |ss: &[Solid], w: &Mat4| ss.iter().map(|s| { let (lo, hi) = world_box(s, w); (hi - lo).length() }).fold(0f32, f32::max);
    let reach = span(a, wa) + span(b, wb);
    let step = (if move_a { span(a, wa) } else { span(b, wb) } / 48.0).max(0.005);
    let mut best = (0usize, f32::MAX);
    for (axis, sign) in [(0usize, 1f32), (0, -1.0), (1, 1.0), (2, 1.0), (2, -1.0)] {
        let mut d = step;
        while d < reach.min(best.1.abs()) {
            let mut v = Vec3::ZERO;
            v[axis] = sign * d;
            let t = Mat4::from_translation(v);
            let (ma, mb) = if move_a { (t * *wa, *wb) } else { (*wa, t * *wb) };
            if a.iter().all(|x| b.iter().all(|y| test(x, &ma, y, &mb).is_none())) {
                best = (axis, sign * (d + step));
                break;
            }
            d += step;
        }
    }
    if best.1 == f32::MAX { (1, reach) } else { best }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(c: Vec3, s: Vec3) -> Vec<[Vec3; 3]> {
        let t = raea_core_shape::cached(&format!("box({}, {}, {})", s.x, s.y, s.z)).unwrap();
        t.positions.chunks_exact(3).map(|p| [Vec3::from(p[0]) + c, Vec3::from(p[1]) + c, Vec3::from(p[2]) + c]).collect()
    }

    #[test]
    fn deep_overlap_is_found_and_contact_is_not() {
        let pillar = voxelize(&boxed(Vec3::ZERO, Vec3::new(0.6, 3.0, 0.6))).unwrap();
        let statue = voxelize(&boxed(Vec3::ZERO, Vec3::new(0.5, 1.0, 0.8))).unwrap();
        // the statue's middle in the pillar
        let hit = test(&statue, &Mat4::from_translation(Vec3::new(0.35, 0.0, 0.0)), &pillar, &Mat4::IDENTITY).expect("sunk in");
        assert!(hit.depth > 0.05, "{}", hit.depth);
        assert!(!hit.seam.is_empty());
        // beside it, touching
        assert!(test(&statue, &Mat4::from_translation(Vec3::new(0.56, 0.0, 0.0)), &pillar, &Mat4::IDENTITY).is_none());
        // standing on a floor slab, its foot a little in
        let floor = voxelize(&boxed(Vec3::new(0.0, -0.1, 0.0), Vec3::new(6.0, 0.2, 6.0))).unwrap();
        assert!(test(&statue, &Mat4::from_translation(Vec3::new(0.0, 0.46, 0.0)), &floor, &Mat4::IDENTITY).is_none());
        // a column seated well into the ground is placed, not clipped
        assert!(test(&pillar, &Mat4::from_translation(Vec3::new(0.0, 1.4, 0.0)), &floor, &Mat4::IDENTITY).is_none());
    }

    #[test]
    fn depths_grow_from_every_face() {
        // a box's foot is shallow: the outside gets under it
        let statue = voxelize(&boxed(Vec3::ZERO, Vec3::new(0.5, 1.0, 0.8))).unwrap();
        assert!(statue.at(Vec3::new(0.0, -0.5 + 2.0 * statue.cell, 0.0)) <= 4);
        // a 1.2 m slab 3 cm thick, its fine solid closed: deepest in the middle
        let sill = voxelize(&boxed(Vec3::ZERO, Vec3::new(1.2, 0.03, 0.22))).unwrap();
        let f = sill.fine.as_deref().expect("a thin part's fine solid");
        assert!(sill.inside.is_empty() && sill.solid(), "a cell thick at RES, solid in its fine one");
        assert!(f.at(Vec3::ZERO) >= 6, "{}", f.at(Vec3::ZERO));
    }

    #[test]
    fn a_thin_piece_through_a_thin_part_is_found() {
        // a sink: its basin and, behind, a tap 3 cm across rising to 1.45 m, one model
        let mut sink = boxed(Vec3::new(0.0, 0.85, 0.45), Vec3::new(1.0, 0.2, 0.7));
        sink.extend(boxed(Vec3::new(0.0, 1.2, 0.13), Vec3::new(0.03, 0.5, 0.03)));
        let sink = voxelize(&sink).unwrap();
        // a 3 cm sill at 1.27 m, 22 cm deep: through the tap
        let sill = voxelize(&boxed(Vec3::new(0.0, 1.27, 0.11), Vec3::new(1.2, 0.03, 0.22))).unwrap();
        let hit = test(&sink, &Mat4::IDENTITY, &sill, &Mat4::IDENTITY).expect("the tap through the sill");
        assert!(hit.small_first, "the sink passes into the sill");
        // 7 cm deep, behind the tap: clear
        let shallow = voxelize(&boxed(Vec3::new(0.0, 1.27, 0.035), Vec3::new(1.2, 0.03, 0.07))).unwrap();
        assert!(test(&sink, &Mat4::IDENTITY, &shallow, &Mat4::IDENTITY).is_none());
        // a cup set on the sill: contact
        let cup = voxelize(&boxed(Vec3::new(0.4, 1.33, 0.11), Vec3::new(0.08, 0.09, 0.08))).unwrap();
        assert!(test(&cup, &Mat4::IDENTITY, &sill, &Mat4::IDENTITY).is_none());
    }

    #[test]
    fn an_open_bottom_is_still_solid() {
        // a box with no bottom face, as scanned columns come
        let t: Vec<[Vec3; 3]> = boxed(Vec3::ZERO, Vec3::ONE).into_iter().filter(|t| !t.iter().all(|p| p.y < -0.49)).collect();
        let s = voxelize(&t).unwrap();
        assert!(s.at(Vec3::ZERO) >= 10, "{}", s.at(Vec3::ZERO));
    }

    #[test]
    fn a_wall_hides_a_seam() {
        let wall = voxelize(&boxed(Vec3::ZERO, Vec3::new(4.0, 4.0, 0.4))).unwrap();
        let at = Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0));
        let seen: [(&Solid, Mat4); 1] = [(&wall, at)];
        // a point behind the wall, the eye in front
        assert!(blocked(Vec3::ZERO, Vec3::new(0.0, 0.0, 6.0), 0.05, &seen));
        // beside it
        assert!(!blocked(Vec3::new(3.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 6.0), 0.05, &seen));
    }
}
