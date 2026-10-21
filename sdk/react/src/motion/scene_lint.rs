//! `raea-motion scene lint|explain EDIT.tsx`: where each part of a 3D scene meets the others, measured from their real
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
use std::sync::{Arc, Mutex, OnceLock};

use bevy::math::{Mat4, Vec3};
use serde_json::{json, Value};

use super::super::{all_items, Item};
use super::super::super::Frame;
use super::{excused, gltf_tris, label, solid_of, test};

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

/// A part's triangles in the world, with the node each came from.
pub(super) struct Part {
    pub(super) label: String,
    pub(super) tris: Vec<[Vec3; 3]>,
    node: Vec<u32>,
    nodes: Vec<String>,
    pub(super) lo: Vec3,
    pub(super) hi: Vec3,
    /// excused from the checks (see-through, unlit, allowOverlap, animating), but still a surface others rest on
    excused: bool,
    /// it moves between this frame and the next (in flight, keyed, baked): where it is now isn't where it rests
    moving: bool,
}

/// A glTF file's or a shape's triangles in its own space, read once a process.
fn own_tris(it: &Item) -> Result<Arc<(Vec<(String, Vec<[Vec3; 3]>)>, bool)>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Result<Arc<(Vec<(String, Vec<[Vec3; 3]>)>, bool)>, String>>>> = OnceLock::new();
    let key = match it.kind {
        "model" => format!("model:{}", super::super::src_path(it.props.get("src").and_then(Value::as_str).unwrap_or(""))),
        _ => format!("mesh:{}", it.shape.as_deref().unwrap_or("")),
    };
    if let Some(t) = CACHE.get_or_init(Default::default).lock().unwrap().get(&key) {
        return t.clone();
    }
    let made = if it.kind == "model" {
        gltf_tris(&key["model:".len()..]).map(Arc::new)
    } else {
        it.shape.as_deref().ok_or_else(|| "no shape".to_string()).and_then(rdb::shape::cached).map(|t| {
            Arc::new((vec![(String::new(), t.positions.chunks_exact(3).map(|c| [Vec3::from(c[0]), Vec3::from(c[1]), Vec3::from(c[2])]).collect())], false))
        })
    };
    CACHE.get_or_init(Default::default).lock().unwrap().insert(key, made.clone());
    made
}

pub(super) fn parts_of(its: &[Item], moving: &[bool]) -> (Vec<Part>, Vec<Item>, Vec<String>) {
    let (mut parts, mut items, mut skipped) = (Vec::new(), Vec::new(), Vec::new());
    for (it, &moves) in its.iter().zip(moving).filter(|(it, _)| it.kind == "mesh" || it.kind == "model") {
        let name = label(it, "");
        let own = match own_tris(it) {
            Ok(t) => t,
            Err(e) => {
                skipped.push(format!("{name}: {e}"));
                continue;
            }
        };
        let animates = own.1 && it.kind == "model";
        let (mut tris, mut node, mut nodes) = (Vec::new(), Vec::new(), Vec::new());
        for (n, (nn, ts)) in own.0.iter().enumerate() {
            nodes.push(nn.clone());
            for t in ts {
                tris.push(t.map(|p| it.world.transform_point3(p)));
                node.push(n as u32);
            }
        }
        if tris.is_empty() {
            continue;
        }
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for t in &tris {
            for p in t {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
        parts.push(Part { label: name, tris, node, nodes, lo, hi, excused: excused(it) || animates, moving: moves });
        items.push(it.clone());
    }
    (parts, items, skipped)
}

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

/// A ray against one triangle (Möller–Trumbore): the distance along `d` (unit), if it hits ahead.
pub(super) fn ray_tri(o: Vec3, d: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let s = o - a;
    let u = s.dot(p) / det;
    if !(-1e-6..=1.0 + 1e-6).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) / det;
    if v < -1e-6 || u + v > 1.0 + 1e-6 {
        return None;
    }
    let t = e2.dot(q) / det;
    (t >= 0.0).then_some(t)
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
            let base = format!("{}|{}", it.kind, super::super::base_ident(it));
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            format!("{base}#{n}")
        }).collect()
    };
    let there: HashMap<String, Mat4> = keys(other).into_iter().zip(other.iter().map(|it| it.world)).collect();
    keys(now).iter().zip(now).map(|(k, it)| there.get(k).is_some_and(|w| !w.abs_diff_eq(it.world, 1e-6))).collect()
}

/// One frame's parts, each with what it rests on, and the faults: (findings as JSON, the error count).
pub(crate) fn lint(frames: &[(i64, Frame)], frame: usize) -> (Vec<Value>, usize) {
    // the frame, and the next (or the one before, at the end) to see what moves
    let other = if frame + 1 < frames.len() { frame + 1 } else { frame.saturating_sub(1) };
    let all = all_items(&frames[frame..frame + 1]);
    let next = all_items(&frames[other..other + 1]);
    let moving = moving(&all[0], &next[0]);
    let (parts, items, skipped) = parts_of(&all[0], &moving);
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
            let mut best: Option<super::Hit> = None;
            for (pa, sa) in a.iter().enumerate() {
                for (pb, sb) in b.iter().enumerate() {
                    if let Some(mut h) = test(sa, &items[i].world, sb, &items[j].world) {
                        if best.as_ref().is_none_or(|b| h.depth > b.depth) {
                            h.pieces = (pa, pb);
                            best = Some(h);
                        }
                    }
                }
            }
            if let Some(h) = best {
                let (la, lb) = (label(&items[i], &a[h.pieces.0].name), label(&items[j], &b[h.pieces.1].name));
                let (small, big) = if h.small_first { (la, lb) } else { (lb, la) };
                errors += 1;
                out.push(json!({"part": small, "into": big, "check": "clips", "level": "error", "depth": (h.depth * 1e4).round() / 1e4, "share": (h.share * 1e3).round() / 1e3,
                    "says": format!("{small} passes {} into {big} ({:.0}% of it inside)", mm(h.depth), h.share * 100.0)}));
            }
        }
    }
    (out, errors)
}

/// A glTF file's triangles in its own space (its node transforms applied), read once a process.
fn file_tris(path: &str) -> Result<Arc<Vec<Vec3>>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Result<Arc<Vec<Vec3>>, String>>>> = OnceLock::new();
    if let Some(t) = CACHE.get_or_init(Default::default).lock().unwrap().get(path) {
        return t.clone();
    }
    let made = gltf_tris(path).map(|(groups, _)| Arc::new(groups.into_iter().flat_map(|(_, t)| t.into_iter().flatten()).collect::<Vec<_>>()));
    CACHE.get_or_init(Default::default).lock().unwrap().insert(path.to_string(), made.clone());
    made
}

/// `scene.bounds`: a glTF model's box turned (Euler XYZ, radians, as Model takes it) and scaled about its origin. Its
/// min y is how far it reaches below its origin: set on a surface at height h, its origin goes at h - min y. Read from
/// every node's triangles with their transforms (Manifold's import drops them, and wants closed meshes).
fn bounds_call(v: &Value) -> Result<Value, String> {
    let src = v.get("src").and_then(Value::as_str).ok_or("scene.bounds: src, a .glb (staticFile(\"x.glb\"))")?;
    let dir = crate::native::composition_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let path = super::super::super::media_path(&dir, src);
    let tris = file_tris(&path.to_string_lossy())?;
    let mut p = serde_json::Map::new();
    for k in ["rotation", "scale"] {
        if let Some(x) = v.get(k) {
            p.insert(k.into(), x.clone());
        }
    }
    let m = super::super::local(&p);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for q in tris.iter() {
        let w = m.transform_point3(*q);
        lo = lo.min(w);
        hi = hi.max(w);
    }
    if tris.is_empty() {
        return Err(format!("scene.bounds: {} has no triangles", path.display()));
    }
    Ok(json!({"min": [lo.x, lo.y, lo.z], "max": [hi.x, hi.y, hi.z]}))
}

/// Which way a seat faces, read from its triangles as it comes (unturned): the seat is the most up-facing area at one
/// height (2 cm bins) between a quarter of the model's height and its top; what stands more than 4 cm over the seat is
/// its back (and arms: a bar stool's back can be a 5 cm lip). A back stands at an edge of the seat and arms at its sides,
/// so the back's offset from the seat's centre is read against the seat's half-width and half-depth and the larger one
/// is the way a diner faces, away from it (a bench's one high end leans the raw offset 40 degrees). Plan (x, z) along
/// an axis of the model, or None for a seat with nothing over it (a backless stool faces any way). Also the seat's
/// height over the model's lowest point and how far off centre (in half-sizes) the back stands.
pub(crate) fn seat_facing(tris: &[Vec3]) -> Option<([f32; 2], f32, f32)> {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for q in tris {
        lo = lo.min(*q);
        hi = hi.max(*q);
    }
    let h = hi.y - lo.y;
    if !(h > 0.0) {
        return None;
    }
    let bin = 0.02;
    let mut up: HashMap<i32, f32> = HashMap::new();
    let mut parts = Vec::with_capacity(tris.len() / 3);
    for t in tris.chunks_exact(3) {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        let area = n.length() / 2.0;
        if !(area > 0.0) {
            continue;
        }
        let c = (t[0] + t[1] + t[2]) / 3.0;
        parts.push((c, area));
        if n.y / (2.0 * area) > 0.9 && c.y >= lo.y + 0.25 * h {
            *up.entry((c.y / bin).floor() as i32).or_default() += area;
        }
    }
    // the seat's height: the fullest bin (and its neighbours, a cushion's top is rarely flat)
    let fill = |k: i32| (k - 1..=k + 1).map(|j| up.get(&j).copied().unwrap_or(0.0)).sum::<f32>();
    let k = *up.keys().max_by(|a, b| fill(**a).total_cmp(&fill(**b)))?;
    let seat_y = (k as f32 + 0.5) * bin;
    let (mut seat, mut sa, mut back, mut ba) = (Vec3::ZERO, 0.0, Vec3::ZERO, 0.0);
    let (mut slo, mut shi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for &(c, a) in &parts {
        if (c.y - seat_y).abs() <= 1.5 * bin {
            seat += c * a;
            sa += a;
            slo = slo.min(c);
            shi = shi.max(c);
        } else if c.y > seat_y + 0.04 {
            back += c * a;
            ba += a;
        }
    }
    // against the seat's own area: legs and rails under it can be most of a model
    if sa <= 0.0 || ba < 0.02 * sa {
        return None;
    }
    let d = seat / sa - back / ba;
    let half = ((shi - slo) / 2.0).max(Vec3::splat(0.01));
    let (nx, nz) = (d.x / half.x, d.z / half.z);
    // a back centred over the seat (a stool's ring, a throne's arms all round) says no way
    if nx.abs().max(nz.abs()) < 0.1 {
        return None;
    }
    let f = if nx.abs() > nz.abs() { [nx.signum(), 0.0] } else { [0.0, nz.signum()] };
    Some((f, seat_y - lo.y, nx.abs().max(nz.abs())))
}

/// `scene.facing`: which way a seat faces as it comes, for room.tsx's faceToward and the layout's seat rule.
fn facing_call(v: &Value) -> Result<Value, String> {
    let src = v.get("src").and_then(Value::as_str).ok_or("scene.facing: src, a .glb (staticFile(\"x.glb\"))")?;
    let dir = crate::native::composition_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let path = super::super::super::media_path(&dir, src);
    let tris = file_tris(&path.to_string_lossy())?;
    Ok(match seat_facing(&tris) {
        Some((f, seat, lean)) => json!({"faces": f, "seat": seat, "lean": lean}),
        None => json!({"faces": null}),
    })
}

/// `scene.sky`: a sky panorama read for daylight (scene_light.rs sky_of).
fn sky_call(v: &Value) -> Result<Value, String> {
    let src = v.get("src").and_then(Value::as_str).ok_or("scene.sky: src, an .hdr panorama (staticFile(\"x.hdr\"))")?;
    let dir = crate::native::composition_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let path = super::super::super::media_path(&dir, src);
    super::light::sky_of(&path.to_string_lossy(), v.get("rotation").and_then(Value::as_f64).unwrap_or(0.0) as f32)
}

pub(crate) fn register() {
    raea_native::register(
        "scene.sky",
        "{ src: string; rotation?: number }",
        "{ sun: { dir: [number, number, number]; elevation: number; horizontal: number; cct: number | null } | null; sky: { horizontal: number; luminance: number; cct: number | null } }",
        "a sky panorama read for daylight: its sun's direction once turned by rotation (as <Environment rotation>), and the light on a level surface from the sun's disc and from the rest of the sky (panorama units), the sky's mean luminance, each one's colour temperature",
        sky_call,
    );
    raea_native::register(
        "scene.facing",
        "{ src: string }",
        "{ faces: [number, number] | null; seat?: number; lean?: number }",
        "which way a seat faces as it comes, on the plan (x, z), read from its triangles: from its back's centre across its seat's; null for a seat with nothing over it (a backless stool); seat is its height",
        facing_call,
    );
    raea_native::register(
        "scene.bounds",
        "{ src: string; rotation?: [number, number, number]; scale?: number | [number, number, number] }",
        "{ min: [number, number, number]; max: [number, number, number] }",
        "a glTF model's box turned (Euler XYZ radians) and scaled about its origin, from all its nodes: set on a surface at h, its origin goes at h - min y",
        bounds_call,
    );
}

/// `raea-motion scene lint|explain EDIT.tsx [--composition ID] [--frame N] [--json]`: the frame's parts measured
/// (frame 0 by default). `lint` says the faults (errors fail it: floating, sunk, clipping; warnings: in the air,
/// tilted); `explain` says what every part rests on. --json prints the findings (each part's support, gap and the y
/// that fixes it) instead.
pub(crate) fn cli(args: &[String]) -> Result<String, String> {
    use super::super::super::{bundle_composition, comp_opts, flag, scene_frames_to};
    let usage = "raea-motion scene lint|explain|light EDIT.tsx [--composition ID] [--frame N] [--json]";
    let (Some(mode), Some(edit)) = (args.get(1).map(String::as_str), args.get(2)) else { return Err(usage.into()) };
    if mode == "light" {
        return super::light::cli(args);
    }
    if mode != "lint" && mode != "explain" {
        return Err(usage.into());
    }
    let tsx = std::path::PathBuf::from(edit).canonicalize().map_err(|e| format!("{edit}: {e}"))?;
    let dir = tsx.parent().unwrap().to_path_buf();
    let frame: i64 = flag(args, "--frame").map(|f| f.parse().map_err(|_| "--frame takes a frame number")).transpose()?.unwrap_or(0);
    let js = std::env::temp_dir().join(format!("raea-scene-{}.js", std::process::id()));
    bundle_composition(&tsx, &js, &comp_opts(args)?, None)?;
    // and the frame after it, to see what moves
    let got = scene_frames_to(&js, &dir, frame + 2);
    let _ = std::fs::remove_file(&js);
    let (frames, ..) = got?.ok_or("the composition has no 3D")?;
    let k = frames.iter().position(|(f, _)| *f == frame).ok_or_else(|| format!("no frame {frame}"))?;
    let (found, errors) = lint(&frames, k);
    let shown: Vec<&Value> = found.iter().filter(|v| mode == "explain" || !matches!(v["level"].as_str(), Some("ok") | Some("note"))).collect();
    let text = if args.iter().any(|a| a == "--json") {
        serde_json::to_string_pretty(&json!({"frame": frame, "errors": errors, "found": shown})).unwrap_or_default()
    } else {
        let mut t: Vec<String> = shown.iter().map(|v| format!("{:<8} {}", v["level"].as_str().unwrap_or(""), v["says"].as_str().unwrap_or(""))).collect();
        let warnings = found.iter().filter(|v| v["level"] == "warning").count();
        t.push(format!("frame {frame}: {} parts, {errors} errors, {warnings} warnings", found.iter().filter(|v| v["check"] != "clips" && v["check"] != "tilted" && v["check"] != "skipped").count()));
        t.join("\n")
    };
    // errors fail a lint, with nothing after the findings (--json stays one document)
    if errors > 0 && mode == "lint" {
        println!("{text}");
        std::process::exit(1);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, c: Vec3, s: Vec3) -> Part {
        let t = rdb::shape::cached(&format!("box({}, {}, {})", s.x, s.y, s.z)).unwrap();
        let tris: Vec<[Vec3; 3]> = t.positions.chunks_exact(3).map(|p| [Vec3::from(p[0]) + c, Vec3::from(p[1]) + c, Vec3::from(p[2]) + c]).collect();
        let n = tris.len();
        Part { label: name.into(), tris, node: vec![0; n], nodes: vec![String::new()], lo: c - s / 2.0, hi: c + s / 2.0, excused: false, moving: false }
    }

    fn rest(parts: &[Part], i: usize) -> Rest {
        rest_of(parts, &Grid::new(parts), &insets(parts), i)
    }

    #[test]
    fn seat_faces_away_from_its_back() {
        let boxed = |c: Vec3, sz: Vec3| part("x", c, sz).tris.into_iter().flatten().collect::<Vec<_>>();
        let legs: Vec<Vec3> = [(-0.2, -0.2), (0.2, -0.2), (-0.2, 0.2), (0.2, 0.2)].iter().flat_map(|&(x, z)| boxed(Vec3::new(x, 0.22, z), Vec3::new(0.03, 0.44, 0.03))).collect();
        let seat = boxed(Vec3::new(0.0, 0.45, 0.0), Vec3::new(0.44, 0.03, 0.44));
        // a chair, its back at -z: faces +z, its seat 0.465 up
        let chair = [legs.clone(), seat.clone(), boxed(Vec3::new(0.0, 0.7, -0.21), Vec3::new(0.44, 0.45, 0.03))].concat();
        let (f, h, _) = seat_facing(&chair).unwrap();
        assert_eq!(f, [0.0, 1.0]);
        assert!((h - 0.465).abs() < 0.03, "{h}");
        // turned: its back at +x faces -x
        let turned: Vec<Vec3> = chair.iter().map(|p| Vec3::new(-p.z, p.y, p.x)).collect();
        assert!(seat_facing(&turned).unwrap().0[0] < -0.99);
        // a back with one high end (a settle's) still faces straight out
        let settle = [chair.clone(), boxed(Vec3::new(-0.2, 0.6, 0.05), Vec3::new(0.03, 0.25, 0.3))].concat();
        assert_eq!(seat_facing(&settle).unwrap().0, [0.0, 1.0]);
        // a backless stool faces any way
        assert!(seat_facing(&[legs, seat].concat()).is_none());
    }

    #[test]
    fn sits_floats_sinks_hangs() {
        let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
        let wall = part("wall", Vec3::new(0.0, 1.0, -1.05), Vec3::new(4.0, 2.0, 0.1));
        let mug = |y: f32| part("mug", Vec3::new(0.3, y + 0.05, 0.2), Vec3::new(0.08, 0.1, 0.08));
        // on it
        let r = rest(&[floor, mug(0.0)], 1);
        assert!(matches!(r, Rest::On { under: 0, gap, .. } if gap.abs() < 1e-4), "{r:?}");
        // 3.5 mm above it
        let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
        let r = rest(&[floor, mug(0.0035)], 1);
        assert!(matches!(r, Rest::On { gap, .. } if (gap - 0.0035).abs() < 1e-4), "{r:?}");
        // 6 mm in
        let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
        let r = rest(&[floor, mug(-0.006)], 1);
        assert!(matches!(r, Rest::On { gap, .. } if (gap + 0.006).abs() < 1e-4), "{r:?}");
        // the floor is the ground
        let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
        assert!(matches!(rest(&[floor], 0), Rest::Ground));
        // a shelf on the wall, 1.2 m up: mounted
        let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
        let shelf = part("shelf", Vec3::new(0.0, 1.2, -0.9), Vec3::new(0.6, 0.03, 0.2));
        let r = rest(&[floor, wall, shelf], 2);
        assert!(matches!(r, Rest::Mounted { on: 1, .. }), "{r:?}");
        // and moved 2 cm off the wall: in the air
        let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
        let wall = part("wall", Vec3::new(0.0, 1.0, -1.05), Vec3::new(4.0, 2.0, 0.1));
        let shelf = part("shelf", Vec3::new(0.0, 1.2, -0.88), Vec3::new(0.6, 0.03, 0.2));
        assert!(matches!(rest(&[floor, wall, shelf], 2), Rest::Air { below: Some((0, _, _)) }));
    }

    #[test]
    fn set_into_a_wall_and_hung_on_a_carcass() {
        let floor = || part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
        let wall = || part("wall", Vec3::new(0.0, 1.2, -1.0), Vec3::new(4.0, 2.4, 0.2));
        // a sill set 5 cm into the wall, 1 m up, 4 cm over a cupboard top: mounted on the wall, not floating
        let cupboard = part("cupboard", Vec3::new(0.0, 0.47, -0.6), Vec3::new(1.0, 0.94, 0.6));
        let sill = part("sill", Vec3::new(0.0, 0.99, -0.87), Vec3::new(1.0, 0.02, 0.16));
        let r = rest(&[floor(), wall(), cupboard, sill], 3);
        assert!(matches!(r, Rest::Mounted { on: 1, .. }), "{r:?}");
        // a lintel of the wall over a window set into it: neither sunk into the other
        let window = part("window", Vec3::new(0.0, 1.5, -1.0), Vec3::new(1.0, 1.0, 0.1));
        let lintel = part("lintel", Vec3::new(0.0, 2.2, -1.0), Vec3::new(1.2, 0.42, 0.2));
        let ps = [floor(), wall(), window, lintel];
        assert!(!matches!(rest(&ps, 3), Rest::On { gap, .. } if gap < -TOUCH), "{:?}", rest(&ps, 3));
        // a cupboard door 2 cm in front of its carcass, 10 cm off the floor: hung on it
        let carcass = part("carcass", Vec3::new(0.0, 0.45, 0.0), Vec3::new(0.6, 0.9, 0.5));
        let door = part("door", Vec3::new(0.0, 0.5, 0.26 + 0.02), Vec3::new(0.56, 0.7, 0.02));
        let r = rest(&[floor(), carcass, door], 2);
        assert!(matches!(r, Rest::Mounted { on: 1, .. }), "{r:?}");
    }

    #[test]
    fn a_magnet_on_its_paper_on_a_door() {
        // a door's face at z 0.3, a paper on it, a magnet on the paper straddling its top edge: rays straight down
        // meet the paper's top edge in the magnet's foot; it's mounted on the paper, not sunk into it
        let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
        let door = part("door", Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.7, 1.6, 0.6));
        let paper = part("paper", Vec3::new(0.0, 1.5, 0.3005), Vec3::new(0.2, 0.28, 0.001));
        let magnet = part("magnet", Vec3::new(0.0, 1.635, 0.3015), Vec3::new(0.04, 0.04, 0.002));
        let r = rest(&[floor, door, paper, magnet], 3);
        assert!(matches!(r, Rest::Mounted { on: 2, .. } | Rest::Mounted { on: 1, .. }), "{r:?}");
    }

    #[test]
    fn a_mug_on_its_side_rests_on_its_lowest_edge() {
        // a cylinder lying along x on the island
        let top = part("island", Vec3::new(0.0, 0.98, 0.0), Vec3::new(1.0, 0.04, 1.0));
        // manifold's cylinder stands along z: lying, once y is up; its lowest point set 1 mm above the stone
        let t = rdb::shape::cached("cylinder(0.04, 0.1)").unwrap();
        let low = t.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let c = Vec3::new(0.0, 1.001 - low, 0.0);
        let tris: Vec<[Vec3; 3]> = t.positions.chunks_exact(3).map(|p| [Vec3::from(p[0]) + c, Vec3::from(p[1]) + c, Vec3::from(p[2]) + c]).collect();
        let n = tris.len();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for q in tris.iter().flatten() {
            lo = lo.min(*q);
            hi = hi.max(*q);
        }
        let mug = Part { label: "mug".into(), tris, node: vec![0; n], nodes: vec![String::new()], lo, hi, excused: false, moving: false };
        let r = rest(&[top, mug], 1);
        assert!(matches!(r, Rest::On { under: 0, gap, .. } if (gap - 0.001).abs() < 3e-4), "{r:?}");
    }
}
