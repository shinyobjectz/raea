//! `raea scene lint|explain EDIT.tsx`: where each part of a 3D scene meets the others, measured from their real
//! geometry on one frame. A part sits on what's under it (rays straight down from its lowest points find the surface
//! and the gap, to the tenth of a millimetre), hangs from or is mounted on what touches its top or sides, or is in the
//! air. A part a few millimetres above its surface floats, one whose foot is below the surface it stands on is sunk,
//! and two parts passing deep into each other clip (the clipping check's own test, wherever it is and not only where
//! the camera sees it). `lint` says the faults and fails on them; `explain` says what every part rests on.
//!
//! Every mesh and glTF model is a part (a model whole, its nodes named in what it's said to rest on); see-through,
//! unlit, `allowOverlap` and animating parts are left out, as the clipping check leaves them. The ground is the part
//! that has nothing under it and is big (a floor, a terrain): it rests on nothing and that's no fault.

use std::collections::HashMap;

use bevy::math::{Mat4, Vec3};
use serde_json::{json, Value};

use raea_core_scene::{Item, Part, base_ident, label, parts_of, ray_tri};

use super::solid_of;

/// A gap under this (metres) is contact: a part sitting on its surface.
const TOUCH: f32 = 0.002;
/// Within this of a surface below, a part is meant to sit on it: further, and it's in the air on purpose (a lamp, a
/// bird) or placed wrong, said but not failed.
const NEAR: f32 = 0.05;
/// Within this of what's beside or over it, a part is mounted on it (a door on its carcass, a lamp on the ceiling).
const MOUNT: f32 = 0.005;
/// A tall part (half a metre or more) sunk no more than this share of its height is seated in what's under or around
/// it: a door frame in its floor, a window in its wall, walls meeting at a corner.
const SEATED: f32 = 0.03;
/// Sunk only when at least this share of its feet are under the surface.
const SUNK_SHARE: f32 = 0.1;
/// A door this far in front of the carcass behind it, the carcass covering it, is hung on it (doors stand off their
/// cupboard's frame by a reveal; the kitchen pack's by up to 2 cm).
const CARCASS: f32 = 0.025;
/// A wall, or a piece of one (the strip of wall over a window, the window in it): no thicker than WALL_THICK, at least
/// WALL_WIDE across and WALL_TALL high (metres; the kitchen's strip over its back window is 9.6 cm).
const WALL_THICK: f32 = 0.35;
const WALL_WIDE: f32 = 0.8;
const WALL_TALL: f32 = 0.05;
/// A part this big across its footprint (m²) with nothing under it is the ground.
const GROUND: f32 = 1.0;
/// The xz grid the scene's triangles are binned in, metres.
const BIN: f32 = 0.05;
/// The most lowest points a part is tested from.
const FEET: usize = 96;
/// A part resting on a surface whose up leans more than this (degrees) is tilted, unless it's posed (`posed` on it).
const LEVEL: f32 = 2.0;

/// Every part's triangles binned by the xz cells their boxes cover, for rays straight up and down.
struct Grid {
    lo: [f32; 2],
    n: [usize; 2],
    cells: Vec<Vec<(u32, u32)>>,
}

impl Grid {
    fn new(parts: &[Part]) -> Grid {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in parts {
            lo = [lo[0].min(p.lo.x), lo[1].min(p.lo.z)];
            hi = [hi[0].max(p.hi.x), hi[1].max(p.hi.z)];
        }
        let n = [(((hi[0] - lo[0]) / BIN) as usize + 1).min(4096), (((hi[1] - lo[1]) / BIN) as usize + 1).min(4096)];
        let mut g = Grid { lo, n, cells: vec![Vec::new(); n[0] * n[1]] };
        for (i, p) in parts.iter().enumerate() {
            for (k, t) in p.tris.iter().enumerate() {
                let (x0, z0) = g.cell(t[0].x.min(t[1].x).min(t[2].x), t[0].z.min(t[1].z).min(t[2].z));
                let (x1, z1) = g.cell(t[0].x.max(t[1].x).max(t[2].x), t[0].z.max(t[1].z).max(t[2].z));
                for z in z0..=z1 {
                    for x in x0..=x1 {
                        g.cells[z * n[0] + x].push((i as u32, k as u32));
                    }
                }
            }
        }
        g
    }
    fn cell(&self, x: f32, z: f32) -> (usize, usize) {
        let c = |v: f32, k: usize| (((v - self.lo[k]) / BIN).floor().max(0.0) as usize).min(self.n[k] - 1);
        (c(x, 0), c(z, 1))
    }
    /// Where the vertical line through (x, z) crosses each triangle there but the part `skip`'s: (y, part, triangle).
    fn column(&self, parts: &[Part], x: f32, z: f32, skip: usize) -> Vec<(f32, usize, usize)> {
        let (cx, cz) = self.cell(x, z);
        let mut out = Vec::new();
        for &(i, k) in &self.cells[cz * self.n[0] + cx] {
            if i as usize == skip {
                continue;
            }
            let [a, b, c] = parts[i as usize].tris[k as usize];
            // barycentric in the xz plane
            let d = (b.x - a.x) * (c.z - a.z) - (c.x - a.x) * (b.z - a.z);
            if d.abs() < 1e-12 {
                continue;
            }
            let u = ((x - a.x) * (c.z - a.z) - (c.x - a.x) * (z - a.z)) / d;
            let v = ((b.x - a.x) * (z - a.z) - (x - a.x) * (b.z - a.z)) / d;
            if u < -1e-6 || v < -1e-6 || u + v > 1.0 + 1e-6 {
                continue;
            }
            out.push((a.y + u * (b.y - a.y) + v * (c.y - a.y), i as usize, k as usize));
        }
        out
    }
}

/// What a part rests on, measured.
#[derive(Clone, Debug)]
enum Rest {
    /// on a surface below: the part and node under it, the gap (negative: sunk that far), and where
    On { under: usize, node: u32, gap: f32, at: Vec3, share: f32 },
    /// touching something at its top or sides, nothing close under it: hanging or mounted
    Mounted { on: usize, node: u32, gap: f32, below: Option<(usize, f32)> },
    /// nothing within reach under it or touching it; the nearest surface below, if any, and how far
    Air { below: Option<(usize, u32, f32)> },
    /// the ground: nothing under it, and big
    Ground,
}

/// The part's lowest points (its feet): its vertices within a millimetre (or 2% of its height) of its lowest, and
/// points across the faces lying there (a slab's corners overhang what holds it up; its middle doesn't), at most FEET.
fn feet(p: &Part) -> Vec<Vec3> {
    let tol = (0.02 * (p.hi.y - p.lo.y)).clamp(0.0005, 0.003);
    let low = |q: &Vec3| q.y <= p.lo.y + tol;
    let mut v: Vec<Vec3> = p.tris.iter().flatten().filter(|q| low(q)).copied().collect();
    let bases: Vec<&[Vec3; 3]> = p.tris.iter().filter(|t| t.iter().all(low)).collect();
    let area: f32 = bases.iter().map(|t| 0.5 * (t[1] - t[0]).cross(t[2] - t[0]).length()).sum();
    if area > 0.0 {
        // about FEET points over the base, spread by area
        let step = (area / FEET as f32).sqrt();
        for t in bases {
            let long = (t[1] - t[0]).length().max((t[2] - t[1]).length()).max((t[0] - t[2]).length());
            let m = ((long / step).ceil() as usize).clamp(1, 32);
            for i in 0..=m {
                for j in 0..=(m - i) {
                    let (a, b) = (i as f32 / m as f32, j as f32 / m as f32);
                    v.push(t[0] + (t[1] - t[0]) * a + (t[2] - t[0]) * b);
                }
            }
        }
    }
    v.sort_by(|a, b| (a.x, a.z, a.y).partial_cmp(&(b.x, b.z, b.y)).unwrap_or(std::cmp::Ordering::Equal));
    v.dedup_by(|a, b| (*a - *b).length() < 1e-4);
    if v.len() > 4 * FEET {
        let k = v.len() as f32 / (4 * FEET) as f32;
        v = (0..4 * FEET).map(|i| v[(i as f32 * k) as usize]).collect();
    }
    v
}

/// Whether a part is a wall or a piece of one: thin, wide and high.
fn wall(p: &Part) -> bool {
    let e = p.hi - p.lo;
    e.y >= WALL_TALL && e.x.min(e.z) <= WALL_THICK && e.x.max(e.z) >= WALL_WIDE
}

/// A wall's face area, for which of two is set into the other.
fn face(p: &Part) -> f32 {
    let e = p.hi - p.lo;
    e.y * e.x.max(e.z)
}

/// The wall each part is set into (a sill in the wall under it; a window, itself a wall's piece, in a bigger one), if
/// any: its box passes more than 3 mm into the wall's thickness. What's set into a wall and the wall are mounted on each
/// other, and neither is sunk into the other.
fn insets(parts: &[Part]) -> Vec<Option<usize>> {
    (0..parts.len())
        .map(|i| {
            let p = &parts[i];
            (0..parts.len()).filter(|&j| j != i && wall(&parts[j]) && (!wall(p) || face(&parts[j]) > face(p))).find(|&j| {
                let (lo, hi) = (p.lo.max(parts[j].lo), p.hi.min(parts[j].hi));
                let e = hi - lo;
                let w = &parts[j];
                // the wall's thin axis
                let thin = if (w.hi.x - w.lo.x) < (w.hi.z - w.lo.z) { e.x } else { e.z };
                e.min_element() > 0.0 && thin > 0.003
            })
        })
        .collect()
}

fn rest_of(parts: &[Part], grid: &Grid, insets: &[Option<usize>], i: usize) -> Rest {
    let p = &parts[i];
    let height = p.hi.y - p.lo.y;
    // how far above a foot a surface may be and still be the one it stands in (sunk): half its height, at most NEAR
    let up = (0.5 * height).min(NEAR);
    let feet = feet(p);
    // each foot: the highest surface of another part from `up` above it down
    let mut best: Option<(f32, usize, u32, Vec3)> = None;
    let mut nearest_below: Option<(usize, u32, f32)> = None;
    let mut deepest: Option<(f32, usize, u32, Vec3)> = None;
    let (mut tested, mut sunk) = (0usize, 0usize);
    for f in &feet {
        let mut col = grid.column(parts, f.x, f.z, i);
        // a part and the wall it's set into don't stand on each other, nor do a wall's pieces (a corner, the wall over
        // a window)
        col.retain(|&(y, j, _)| y <= f.y + up && insets[i] != Some(j) && insets[j] != Some(i) && !(wall(p) && wall(&parts[j])));
        let Some(&(y, j, k)) = col.iter().max_by(|a, b| a.0.total_cmp(&b.0)) else { continue };
        let gap = f.y - y;
        tested += 1;
        if gap < -TOUCH {
            sunk += 1;
        }
        if deepest.is_none_or(|(g, ..)| gap < g) {
            deepest = Some((gap, j, parts[j].node[k], *f));
        }
        if nearest_below.is_none_or(|(_, _, g)| gap < g) {
            nearest_below = Some((j, parts[j].node[k], gap));
        }
        if best.is_none_or(|(g, ..)| gap.abs() < g.abs()) {
            best = Some((gap, j, parts[j].node[k], *f));
        }
    }
    let share = if tested > 0 { sunk as f32 / tested as f32 } else { 0.0 };
    let mounted = touching(parts, i).or_else(|| insets[i].map(|w| (w, 0, 0.0)));
    // against the face of what it's sunk into, it's mounted on that face (a magnet on its paper, flush with the
    // fridge door: rays down meet the paper's top edge)
    if let (Some((on, node, gap)), Some(d)) = (mounted, deepest) {
        if d.1 == on && d.0 < -TOUCH && gap.abs() <= TOUCH {
            return Rest::Mounted { on, node, gap, below: None };
        }
    }
    // sunk: a real share of its feet under the surface (a few straddling a thin edge, a magnet over its paper's top,
    // aren't), the deepest saying how far
    if let Some((gap, under, node, at)) = deepest.filter(|d| d.0 < -TOUCH && share >= SUNK_SHARE) {
        return Rest::On { under, node, gap, at, share };
    }
    if let Some((gap, under, node, at)) = best {
        // resting; or a little above a surface and touching nothing else (touching a wall or its carcass, it's
        // mounted there: a cupboard's door over the one below it)
        if gap.abs() <= TOUCH || (gap > 0.0 && gap <= NEAR && mounted.is_none()) {
            return Rest::On { under, node, gap, at, share };
        }
    }
    if let Some((on, node, gap)) = mounted {
        return Rest::Mounted { on, node, gap, below: nearest_below.map(|(j, _, g)| (j, g)) };
    }
    let area = (p.hi.x - p.lo.x) * (p.hi.z - p.lo.z);
    if nearest_below.is_none() && area >= GROUND {
        return Rest::Ground;
    }
    Rest::Air { below: nearest_below }
}

/// The closest other part within MOUNT of this one's top or sides: rays out from points on its box's faces, up and
/// along ±x and ±z, against the parts whose boxes come within reach.
fn touching(parts: &[Part], i: usize) -> Option<(usize, u32, f32)> {
    let p = &parts[i];
    let reach = CARCASS.max(2.0 * MOUNT);
    let near: Vec<usize> = (0..parts.len())
        .filter(|&j| j != i && !(parts[j].lo.cmpgt(p.hi + Vec3::splat(reach)).any() || p.lo.cmpgt(parts[j].hi + Vec3::splat(reach)).any()))
        .collect();
    if near.is_empty() {
        return None;
    }
    // the points of the part nearest each face of its box, out along that face's normal
    let dirs = [Vec3::Y, Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z];
    let mut best: Option<(usize, u32, f32)> = None;
    for d in dirs {
        let extent = |q: Vec3| q.dot(d);
        let far = p.tris.iter().flatten().map(|q| extent(*q)).fold(f32::MIN, f32::max);
        let tol = 0.003f32.max(0.01 * (p.hi - p.lo).max_element());
        let mut pts: Vec<Vec3> = p.tris.iter().flatten().filter(|q| extent(**q) >= far - tol).copied().collect();
        pts.dedup_by(|a, b| (*a - *b).length() < 1e-5);
        let step = (pts.len() / 24).max(1);
        for o in pts.into_iter().step_by(step) {
            for &j in &near {
                for (k, t) in parts[j].tris.iter().enumerate() {
                    // the ray starts a touch inside, so a face flush with it is found at ~0
                    if let Some(s) = ray_tri(o - d * 0.0005, d, *t) {
                        let gap = s - 0.0005;
                        // close, or a door a reveal in front of the carcass that covers it (not a shelf off its wall)
                        let ok = gap <= MOUNT || (gap <= CARCASS && !wall(&parts[j]) && covers(&parts[j], p, d));
                        if ok && best.is_none_or(|(_, _, g)| gap.abs() < g.abs()) {
                            best = Some((j, parts[j].node[k], gap));
                        }
                    }
                }
            }
        }
    }
    best
}

/// Whether `back`'s box covers `front`'s across the two axes square to `d` (within a centimetre): a carcass behind its
/// door, a ceiling over its lamp.
fn covers(back: &Part, front: &Part, d: Vec3) -> bool {
    (0..3).filter(|&k| d[k].abs() < 0.5).all(|k| back.lo[k] <= front.lo[k] + 0.01 && back.hi[k] >= front.hi[k] - 0.01)
}

fn named(parts: &[Part], j: usize, node: u32) -> String {
    let p = &parts[j];
    match p.nodes.get(node as usize).filter(|n| !n.is_empty() && p.nodes.len() > 1) {
        Some(n) => format!("{} (its \"{n}\")", p.label),
        None => p.label.clone(),
    }
}

fn mm(m: f32) -> String {
    format!("{:.1} mm", m * 1000.0)
}

/// Which of a frame's items move by the other frame: each matched by kind, what it is and its count among the same.
fn moving(now: &[Item], other: &[Item]) -> Vec<bool> {
    let keys = |its: &[Item]| -> Vec<String> {
        let mut seen: HashMap<String, usize> = HashMap::new();
        its.iter().map(|it| {
            let base = format!("{}|{}", it.kind, base_ident(it));
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            format!("{base}#{n}")
        }).collect()
    };
    let there: HashMap<String, Mat4> = keys(other).into_iter().zip(other.iter().map(|it| it.world)).collect();
    keys(now).iter().zip(now).map(|(k, it)| there.get(k).is_some_and(|w| !w.abs_diff_eq(it.world, 1e-6))).collect()
}

/// One frame's parts, each with what it rests on, and the faults: (findings as JSON, the error count).
/// `now` is the frame's parts, `next` the next frame's (or the one before, at the end) to see what moves.
pub fn lint(now: &[Item], next: &[Item]) -> (Vec<Value>, usize) {
    let moving = moving(now, next);
    let (parts, items, skipped) = parts_of(now, &moving);
    let grid = Grid::new(&parts);
    let insets = insets(&parts);
    let mut out = Vec::new();
    let mut errors = 0;
    for s in skipped {
        out.push(json!({"check": "skipped", "level": "note", "says": format!("not checked: {s}")}));
    }
    for i in 0..parts.len() {
        let p = &parts[i];
        let r = rest_of(&parts, &grid, &insets, i);
        let (level, check, says) = match &r {
            Rest::On { under, node, gap, at, share } => {
                let on = named(&parts, *under, *node);
                let at = format!("[{:.3}, {:.3}, {:.3}]", at.x, at.y, at.z);
                if *gap < -TOUCH && -gap <= SEATED * (p.hi.y - p.lo.y) && p.hi.y - p.lo.y >= 0.5 {
                    ("note", "seated", format!("{} is seated {} into {on}, a little of its height (a frame in its wall, a post in the ground)", p.label, mm(-gap)))
                } else if *gap < -TOUCH {
                    ("error", "sunk", format!("{} is sunk {} into {on} ({:.0}% of its base; its lowest point at {at}): raise it {:+.4} m", p.label, mm(-gap), share * 100.0, -gap))
                } else if *gap > TOUCH {
                    ("error", "floats", format!("{} floats {} above {on} (its nearest point at {at}): lower it {:+.4} m", p.label, mm(*gap), -gap))
                } else {
                    ("ok", "rests", format!("{} rests on {on} ({})", p.label, mm(*gap)))
                }
            }
            Rest::Mounted { on, node, gap, below } => {
                let under = below.map(|(j, g)| format!("; {} above {}", mm(g), parts[j].label)).unwrap_or_default();
                ("ok", "mounted", format!("{} is mounted on {} ({}){under}", p.label, named(&parts, *on, *node), mm(*gap)))
            }
            Rest::Air { below } => match below {
                Some((j, node, g)) if *g < 0.0 => ("warning", "air", format!("{} touches nothing it could rest on, and {} passes {} into it", p.label, named(&parts, *j, *node), mm(-g))),
                Some((j, node, g)) => ("warning", "air", format!("{} is in the air, {} above {} and touching nothing", p.label, mm(*g), named(&parts, *j, *node))),
                None => ("warning", "air", format!("{} has nothing under it and touches nothing", p.label)),
            },
            Rest::Ground => ("ok", "ground", format!("{} is the ground", p.label)),
        };
        // a part left out of the checks, or on its way somewhere, is still a surface; its own faults aren't said
        let (level, check) = if p.moving && level != "ok" { ("note", "moving") } else { (level, check) };
        let level = if p.excused && level != "ok" { "note" } else { level };
        if level == "error" {
            errors += 1;
        }
        let mut v = json!({"part": p.label, "check": check, "level": level, "says": says, "box": {"min": [p.lo.x, p.lo.y, p.lo.z], "max": [p.hi.x, p.hi.y, p.hi.z]}});
        // its origin's world y, and where it goes to touch (what an agent sets its position's y to, when it has no
        // parent group moving it)
        let y = items[i].world.w_axis.y;
        match &r {
            Rest::On { under, node, gap, .. } => {
                v["on"] = json!(named(&parts, *under, *node));
                v["gap"] = json!((gap * 1e5).round() / 1e5);
                if level == "error" {
                    v["fix"] = json!({"y": ((y - gap) * 1e5).round() / 1e5, "dy": (-gap * 1e5).round() / 1e5});
                }
            }
            Rest::Mounted { on, node, .. } => v["on"] = json!(named(&parts, *on, *node)),
            _ => {}
        }
        out.push(v);
        // tilted: resting on a surface, its own up leaning off the world's
        if let Rest::On { under, node, gap, .. } = &r {
            let up = items[i].world.transform_vector3(Vec3::Y).normalize_or_zero();
            let lean = up.dot(Vec3::Y).clamp(-1.0, 1.0).acos().to_degrees();
            let posed = items[i].props.get("posed").and_then(Value::as_bool) == Some(true);
            if lean > LEVEL && *gap <= NEAR && !posed && !p.excused && !p.moving {
                out.push(json!({"part": p.label, "check": "tilted", "level": "warning", "on": named(&parts, *under, *node), "degrees": (lean * 10.0).round() / 10.0,
                    "says": format!("{} leans {lean:.1}° off level on {} (give it posed if it's meant to)", p.label, named(&parts, *under, *node))}));
            }
        }
    }
    // parts passing into each other, wherever (the clipping check's own test: deep for the smaller part's size, a
    // real share of it, not resting contact and not a foot seated in what's below)
    let solids: Vec<_> = items.iter().map(|it| solid_of(it).ok()).collect();
    for i in 0..parts.len() {
        for j in i + 1..parts.len() {
            if parts[i].excused || parts[j].excused || parts[i].lo.cmpgt(parts[j].hi).any() || parts[j].lo.cmpgt(parts[i].hi).any() {
                continue;
            }
            let (Some(a), Some(b)) = (&solids[i], &solids[j]) else { continue };
            if let Some(h) = super::deepest(a, &items[i].world, b, &items[j].world) {
                let (la, lb) = (label(&items[i], &a[h.pieces.0].name), label(&items[j], &b[h.pieces.1].name));
                let (small, big) = if h.small_first { (la, lb) } else { (lb, la) };
                errors += 1;
                // out the shortest way (not down), and when that's sideways, settled onto what's under its new place
                // in the same move (MK9): a stool pushed out from a table drops to the floor, not left at the top's height
                let (axis, by) = super::clear_move(a, &items[i].world, b, &items[j].world, h.small_first);
                let mover = if h.small_first { i } else { j };
                let mut v = Vec3::ZERO;
                v[axis] = by;
                let axis_name = ["x", "y", "z"][axis];
                let mut fix = json!({"part": small, "axis": axis_name, "by": (by * 1e4).round() / 1e4});
                let mut then = String::new();
                if axis != 1
                    && let Some((under, node, gap)) = landing(&rest_after(&parts, &insets, mover, v))
                    && gap.abs() > TOUCH
                {
                    fix["dy"] = json!((-gap * 1e4).round() / 1e4);
                    then = format!(" and {:+.3} m along y to rest on {}", -gap, named(&parts, under, node));
                }
                out.push(json!({"part": small, "into": big, "check": "clips", "level": "error", "depth": (h.depth * 1e4).round() / 1e4, "share": (h.share * 1e3).round() / 1e3, "fix": fix,
                    "says": format!("{small} passes {} into {big} ({:.0}% of it inside): move it {by:+.3} m along world {axis_name}{then}", mm(h.depth), h.share * 100.0)}));
            }
        }
    }
    // faces in one plane over each other (MK8): between two parts an error once it's a square centimetre or more,
    // inside one model a warning (a generated mesh's doubled faces flicker only where they show)
    for i in 0..parts.len() {
        if parts[i].excused || parts[i].moving {
            continue;
        }
        for j in i + 1..parts.len() {
            let near = |a: &Part, b: &Part| (a.lo - Vec3::splat(0.001)).cmple(b.hi).all() && (b.lo - Vec3::splat(0.001)).cmple(a.hi).all();
            if parts[j].excused || parts[j].moving || !near(&parts[i], &parts[j]) {
                continue;
            }
            if let Some((n, area, at)) = raea_core_zfight::overlaps(&parts[i].tris, Some(&parts[j].tris)) {
                let level = if area >= 1e-4 { "error" } else { "warning" };
                if level == "error" {
                    errors += 1;
                }
                out.push(json!({"part": parts[i].label, "into": parts[j].label, "check": "zfight", "level": level, "faces": n, "area": (area * 1e6).round() / 1e6, "at": [at.x, at.y, at.z],
                    "says": format!("{} and {} have faces in one plane over each other ({n} pair{}, up to {:.1} cm² at [{:.3}, {:.3}, {:.3}]): they flicker as the camera moves; lift one by a millimetre or cut the overlap", parts[i].label, parts[j].label, if n == 1 { "" } else { "s" }, area * 1e4, at.x, at.y, at.z)}));
            }
        }
        if let Some((n, area, at)) = raea_core_zfight::overlaps(&parts[i].tris, None) {
            out.push(json!({"part": parts[i].label, "check": "zfight", "level": "warning", "faces": n, "area": (area * 1e6).round() / 1e6, "at": [at.x, at.y, at.z],
                "says": format!("{} has faces doubled in one plane inside it ({n} pair{}, up to {:.1} cm²): they flicker where they show; delete the doubles (raea gen check -o)", parts[i].label, if n == 1 { "" } else { "s" }, area * 1e4)}));
        }
    }
    (out, errors)
}

/// The surface a part would land on and how far it is (negative: sunk): what it rests on, or the nearest surface
/// under it when it's in the air.
fn landing(r: &Rest) -> Option<(usize, u32, f32)> {
    match r {
        Rest::On { under, node, gap, .. } => Some((*under, *node, *gap)),
        Rest::Air { below } => *below,
        _ => None,
    }
}

/// What a part rests on after a move by `v` (MK9): the feet test again at its new place, against everything else.
fn rest_after(parts: &[Part], insets: &[Option<usize>], i: usize, v: Vec3) -> Rest {
    let mut moved: Vec<Part> = parts.to_vec();
    let p = &mut moved[i];
    for q in p.tris.iter_mut().flatten() {
        *q += v;
    }
    p.lo += v;
    p.hi += v;
    let grid = Grid::new(&moved);
    rest_of(&moved, &grid, insets, i)
}

#[cfg(test)]
#[path = "lint/tests.rs"]
mod tests;
