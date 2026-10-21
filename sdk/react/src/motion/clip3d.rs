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
use serde_json::{json, Value};

use super::{all_items, base_ident, filled, src_path, tracks_of, vec3, Item};
use super::super::Frame;

// `raea-motion scene lint|explain`: what each part rests on, and the parts that float, sink or clip (motion/scene_lint.rs)
#[path = "scene_lint.rs"]
pub(crate) mod lint;
// `raea-motion scene light`: the light on a shot's subject, metered (motion/scene_light.rs; docs/scene.feature, "A light plan")
#[path = "scene_light.rs"]
pub(crate) mod light;

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

/// A part's solid in its own space: each cell's depth inside it (0 outside, 1 at the surface).
struct Solid {
    /// the glTF node it was (empty for a mesh or a one-mesh model)
    name: String,
    min: Vec3,
    cell: f32,
    n: [usize; 3],
    depth: Vec<u16>,
    /// the cells at least one cell under the surface
    inside: Vec<u32>,
    /// points on its real surface (from its triangles), SURF_GRAIN of a cell apart: what the pierce test puts in
    /// another part
    surf: Vec<Vec3>,
    /// a thin part's finer solid, the pierce test's depths in it
    fine: Option<Box<Solid>>,
    lo: Vec3,
    hi: Vec3,
}

impl Solid {
    fn at(&self, p: Vec3) -> u16 {
        let q = (p - self.min) / self.cell;
        if q.x < 0.0 || q.y < 0.0 || q.z < 0.0 {
            return 0;
        }
        let (x, y, z) = (q.x as usize, q.y as usize, q.z as usize);
        if x >= self.n[0] || y >= self.n[1] || z >= self.n[2] {
            return 0;
        }
        self.depth[(z * self.n[1] + y) * self.n[0] + x]
    }
    fn centre(&self, i: u32) -> Vec3 {
        let i = i as usize;
        let (x, y, z) = (i % self.n[0], (i / self.n[0]) % self.n[1], i / (self.n[0] * self.n[1]));
        self.min + Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5) * self.cell
    }
    /// whether it has an inside at all: at RES, or in its fine solid (a sill a cell thick at RES)
    fn solid(&self) -> bool {
        !self.inside.is_empty() || self.fine.as_ref().is_some_and(|f| !f.inside.is_empty())
    }
    /// its volume, from the finer of its solids
    fn volume(&self) -> f32 {
        let s = self.fine.as_deref().filter(|f| !f.inside.is_empty()).unwrap_or(self);
        s.inside.len() as f32 * s.cell.powi(3)
    }
    /// how far under the surface a cell of this depth is, in the part's own units (the surface was thickened a cell
    /// to close pinholes)
    fn metres(&self, d: u16) -> f32 {
        (d as f32 - 1.5).max(0.0) * self.cell
    }
}

/// Triangles to a solid. The surface's cells are marked (each triangle sampled at half a cell) and thickened by one;
/// the outside is flooded in from the grid's top and sides, moving sideways and down but never up, so a model open
/// underneath (a column with no bottom face, a scanned bust) is still solid, while a cup or an arch stays open.
fn voxelize(tris: &[[Vec3; 3]]) -> Option<Solid> {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for t in tris {
        for p in t {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    let ext = hi - lo;
    if !ext.is_finite() || ext.max_element() <= 0.0 {
        return None;
    }
    let cell = ext.max_element() / RES;
    let thin = (ext.min_element() / THIN).max(ext.max_element() / (RES * THIN_MOST));
    let mut solid = grid(tris, lo, hi, cell);
    solid.surf = surface(tris, cell * SURF_GRAIN);
    if thin < 0.5 * cell {
        solid.fine = Some(Box::new(grid(tris, lo, hi, thin)));
    }
    Some(solid)
}

/// Points on the triangles, one per `grain` cube they cross.
fn surface(tris: &[[Vec3; 3]], grain: f32) -> Vec<Vec3> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for [a, b, c] in tris {
        let long = (*b - *a).length().max((*c - *b).length()).max((*a - *c).length());
        let m = ((long / (0.5 * grain)).ceil() as usize).clamp(1, 256);
        for i in 0..=m {
            for j in 0..=(m - i) {
                let (u, v) = (i as f32 / m as f32, j as f32 / m as f32);
                let p = *a + (*b - *a) * u + (*c - *a) * v;
                if seen.insert((p / grain).floor().as_ivec3().to_array()) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// The solid of triangles in the box lo..hi at this cell size.
fn grid(tris: &[[Vec3; 3]], lo: Vec3, hi: Vec3, cell: f32) -> Solid {
    let ext = hi - lo;
    // three cells clear all round: a face rounding into the second, thickened, still leaves the outside a way under
    let min = lo - Vec3::splat(3.0 * cell);
    let n = [((ext.x / cell) as usize) + 7, ((ext.y / cell) as usize) + 7, ((ext.z / cell) as usize) + 7];
    let idx = |x: usize, y: usize, z: usize| (z * n[1] + y) * n[0] + x;
    let mut wall = vec![false; n[0] * n[1] * n[2]];
    let mark = |wall: &mut Vec<bool>, p: Vec3| {
        let q = (p - min) / cell;
        let (x, y, z) = (q.x as usize, q.y as usize, q.z as usize);
        if x < n[0] && y < n[1] && z < n[2] {
            wall[idx(x, y, z)] = true;
        }
    };
    for [a, b, c] in tris {
        let long = (*b - *a).length().max((*c - *b).length()).max((*a - *c).length());
        let m = ((long / (0.5 * cell)).ceil() as usize).clamp(1, 1024);
        for i in 0..=m {
            for j in 0..=(m - i) {
                let (u, v) = (i as f32 / m as f32, j as f32 / m as f32);
                mark(&mut wall, *a + (*b - *a) * u + (*c - *a) * v);
            }
        }
    }
    // thickened a cell, so a pinhole doesn't let the outside in
    let mut thick = wall.clone();
    for z in 1..n[2] - 1 {
        for y in 1..n[1] - 1 {
            for x in 1..n[0] - 1 {
                if wall[idx(x, y, z)] {
                    for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                        thick[idx((x as i64 + dx) as usize, (y as i64 + dy) as usize, (z as i64 + dz) as usize)] = true;
                    }
                }
            }
        }
    }
    // the outside: in from the top and the sides, then sideways and down
    let mut out = vec![false; wall.len()];
    let mut q = VecDeque::new();
    for z in 0..n[2] {
        for y in 0..n[1] {
            for x in 0..n[0] {
                let edge = x == 0 || x == n[0] - 1 || z == 0 || z == n[2] - 1 || y == n[1] - 1;
                if edge && !thick[idx(x, y, z)] {
                    out[idx(x, y, z)] = true;
                    q.push_back((x, y, z));
                }
            }
        }
    }
    while let Some((x, y, z)) = q.pop_front() {
        for (dx, dy, dz) in [(1i64, 0i64, 0i64), (-1, 0, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            let (a, b, c) = (x as i64 + dx, y as i64 + dy, z as i64 + dz);
            if a < 0 || b < 0 || c < 0 || a >= n[0] as i64 || b >= n[1] as i64 || c >= n[2] as i64 {
                continue;
            }
            let k = idx(a as usize, b as usize, c as usize);
            if !out[k] && !thick[k] {
                out[k] = true;
                q.push_back((a as usize, b as usize, c as usize));
            }
        }
    }
    // each inside cell's depth: its steps from the outside
    let mut depth = vec![0u16; wall.len()];
    for z in 0..n[2] {
        for y in 0..n[1] {
            for x in 0..n[0] {
                if out[idx(x, y, z)] {
                    q.push_back((x, y, z));
                }
            }
        }
    }
    let mut seen = out;
    while let Some((x, y, z)) = q.pop_front() {
        let d = depth[idx(x, y, z)];
        for (dx, dy, dz) in [(1i64, 0i64, 0i64), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            let (a, b, c) = (x as i64 + dx, y as i64 + dy, z as i64 + dz);
            if a < 0 || b < 0 || c < 0 || a >= n[0] as i64 || b >= n[1] as i64 || c >= n[2] as i64 {
                continue;
            }
            let k = idx(a as usize, b as usize, c as usize);
            if !seen[k] {
                seen[k] = true;
                depth[k] = d + 1;
                q.push_back((a as usize, b as usize, c as usize));
            }
        }
    }
    let inside: Vec<u32> = (0..depth.len() as u32).filter(|&i| depth[i as usize] >= 3).collect();
    Solid { name: String::new(), min, cell, n, depth, inside, surf: Vec::new(), fine: None, lo, hi }
}


/// A glTF file's triangles in its own space, a list per mesh node (node transforms applied) with the node's name, and
/// whether it moves (skins or animations: its pose isn't known here).
fn gltf_tris(path: &str) -> Result<(Vec<(String, Vec<[Vec3; 3]>)>, bool), String> {
    let b = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let u32le = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
    let (json, bin): (Value, Vec<u8>) = if b.len() >= 20 && &b[0..4] == b"glTF" {
        let jl = u32le(12);
        let j = serde_json::from_slice(&b[20..20 + jl]).map_err(|e| format!("{path}: {e}"))?;
        let at = 20 + jl;
        (j, if b.len() > at + 8 { b[at + 8..at + 8 + u32le(at)].to_vec() } else { Vec::new() })
    } else {
        let j: Value = serde_json::from_slice(&b).map_err(|e| format!("{path}: {e}"))?;
        let uri = j["buffers"][0]["uri"].as_str().unwrap_or("");
        let file = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new(".")).join(uri);
        let bin = if uri.is_empty() || uri.starts_with("data:") { Vec::new() } else { std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))? };
        (j, bin)
    };
    let moves = json["skins"].as_array().is_some_and(|a| !a.is_empty()) || json["animations"].as_array().is_some_and(|a| !a.is_empty());
    let read = |i: u64| -> Result<(Vec<f32>, Vec<u32>), String> {
        let a = &json["accessors"][i as usize];
        let view = &json["bufferViews"][a["bufferView"].as_u64().ok_or("an accessor without a view (compressed?)")? as usize];
        let off = view["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = a["count"].as_u64().unwrap_or(0) as usize;
        let ct = a["componentType"].as_u64().unwrap_or(0);
        let k = match a["type"].as_str().unwrap_or("") { "VEC3" => 3, "SCALAR" => 1, t => return Err(format!("an accessor of {t}")) };
        let size = match ct { 5126 | 5125 => 4, 5123 => 2, 5121 => 1, _ => return Err(format!("component type {ct}")) };
        if ct != 5126 && k == 3 {
            return Err("quantized positions".into());
        }
        let stride = match view["byteStride"].as_u64().unwrap_or(0) as usize { 0 => size * k, s => s };
        let (mut f, mut u) = (Vec::new(), Vec::new());
        for e in 0..count {
            for c in 0..k {
                let o = off + e * stride + c * size;
                let s = bin.get(o..o + size).ok_or("an accessor past the buffer")?;
                match ct {
                    5126 => f.push(f32::from_le_bytes([s[0], s[1], s[2], s[3]])),
                    5125 => u.push(u32::from_le_bytes([s[0], s[1], s[2], s[3]])),
                    5123 => u.push(u16::from_le_bytes([s[0], s[1]]) as u32),
                    _ => u.push(s[0] as u32),
                }
            }
        }
        Ok((f, u))
    };
    let local = |n: &Value| -> Mat4 {
        if let Some(m) = n["matrix"].as_array().filter(|m| m.len() == 16) {
            let a: Vec<f32> = m.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
            return Mat4::from_cols_slice(&a);
        }
        let v = |k: &str, d: [f32; 4]| -> [f32; 4] {
            let mut o = d;
            for (i, x) in n[k].as_array().into_iter().flatten().enumerate().take(4) {
                o[i] = x.as_f64().unwrap_or(d[i] as f64) as f32;
            }
            o
        };
        let (t, r, s) = (v("translation", [0.0; 4]), v("rotation", [0.0, 0.0, 0.0, 1.0]), v("scale", [1.0; 4]));
        Mat4::from_scale_rotation_translation(Vec3::new(s[0], s[1], s[2]), bevy::math::Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize(), Vec3::new(t[0], t[1], t[2]))
    };
    let mut out = Vec::new();
    let scene = json["scene"].as_u64().unwrap_or(0) as usize;
    let roots: Vec<u64> = json["scenes"][scene]["nodes"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_else(|| (0..json["nodes"].as_array().map_or(0, |a| a.len() as u64)).collect());
    let mut stack: Vec<(u64, Mat4)> = roots.into_iter().map(|r| (r, Mat4::IDENTITY)).collect();
    while let Some((i, parent)) = stack.pop() {
        let node = &json["nodes"][i as usize];
        let w = parent * local(node);
        if let Some(m) = node["mesh"].as_u64() {
            let mut tris = Vec::new();
            for prim in json["meshes"][m as usize]["primitives"].as_array().into_iter().flatten() {
                if prim["mode"].as_u64().unwrap_or(4) != 4 {
                    continue;
                }
                let (p, _) = read(prim["attributes"]["POSITION"].as_u64().ok_or("no POSITION")?)?;
                let pts: Vec<Vec3> = p.chunks(3).map(|c| w.transform_point3(Vec3::new(c[0], c[1], c[2]))).collect();
                let ix = match prim["indices"].as_u64() { Some(a) => read(a)?.1, None => (0..pts.len() as u32).collect() };
                for t in ix.chunks_exact(3) {
                    if let (Some(a), Some(b), Some(c)) = (pts.get(t[0] as usize), pts.get(t[1] as usize), pts.get(t[2] as usize)) {
                        tris.push([*a, *b, *c]);
                    }
                }
            }
            let name = node["name"].as_str().or(json["meshes"][m as usize]["name"].as_str()).map_or_else(|| format!("mesh {m}"), String::from);
            if !tris.is_empty() {
                out.push((name, tris));
            }
        }
        for k in node["children"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
            stack.push((k, w));
        }
    }
    Ok((out, moves))
}

/// A part's solids (one per mesh node of a model), made once per file or shape expression this process (an Err: not
/// checked, and why).
fn solid_of(it: &Item) -> Result<Arc<Vec<Solid>>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Result<Arc<Vec<Solid>>, String>>>> = OnceLock::new();
    let key = match it.kind {
        "model" => format!("model:{}", src_path(it.props.get("src").and_then(Value::as_str).unwrap_or(""))),
        _ => format!("mesh:{}", it.shape.as_deref().unwrap_or("")),
    };
    if let Some(s) = CACHE.get_or_init(Default::default).lock().unwrap().get(&key) {
        return s.clone();
    }
    let made = (|| {
        let groups: Vec<(String, Vec<[Vec3; 3]>)> = if it.kind == "model" {
            let (g, moves) = gltf_tris(&key["model:".len()..])?;
            if moves {
                return Err("it animates".into());
            }
            g
        } else {
            let t = rdb::shape::cached(it.shape.as_deref().ok_or("no shape")?)?;
            vec![(String::new(), t.positions.chunks_exact(3).map(|c| [Vec3::from(c[0]), Vec3::from(c[1]), Vec3::from(c[2])]).collect())]
        };
        // the whole model, and each mesh on its own when there are several: a statue in open pieces is only solid
        // whole, a ruin's thin column only resolved alone
        let mut solids: Vec<Solid> = Vec::new();
        if groups.len() > 1 {
            let whole: Vec<[Vec3; 3]> = groups.iter().flat_map(|(_, t)| t.iter().copied()).collect();
            solids.extend(voxelize(&whole));
            solids.extend(groups.into_iter().filter_map(|(name, t)| voxelize(&t).map(|mut s| {
                s.name = name;
                s
            })));
        } else {
            solids.extend(groups.into_iter().filter_map(|(_, t)| voxelize(&t)));
        }
        solids.retain(Solid::solid);
        if solids.is_empty() { Err("it has no volume".to_string()) } else { Ok(Arc::new(solids)) }
    })();
    CACHE.get_or_init(Default::default).lock().unwrap().insert(key, made.clone());
    made
}

/// Whether a part is left out: marked, see-through, unlit, or a model playing a clip.
fn excused(it: &Item) -> bool {
    let p = &it.props;
    if p.get("allowOverlap").and_then(Value::as_bool) == Some(true) || p.get("animation").is_some_and(|v| !v.is_null()) {
        return true;
    }
    match &it.material {
        Some(m) => {
            let mp = &m.props;
            &*m.kind == "MeshBasicMaterial"
                || mp.get("opacity").and_then(Value::as_f64).is_some_and(|o| o < 0.95)
                || mp.get("transmission").and_then(Value::as_f64).is_some_and(|t| t > 0.05)
                || mp.get("unlit").and_then(Value::as_bool) == Some(true)
        }
        None => false,
    }
}

/// A part's name for a sentence: its own, else its file's or its shape's; and the piece of it, for a model of several.
fn label(it: &Item, piece: &str) -> String {
    let base = match it.props.get("name").and_then(Value::as_str) {
        Some(n) => n.to_string(),
        None if it.kind == "model" => base_ident(it).rsplit('/').next().unwrap_or("a model").to_string(),
        None => format!("the mesh {}", base_ident(it).chars().take(40).collect::<String>()),
    };
    if piece.is_empty() { base } else { format!("{base} (its \"{piece}\")") }
}

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

/// The frame's camera as a world-to-view matrix, its eye, its vertical focal scale, and the fog's far edge.
fn camera(its: &[Item]) -> Option<(Mat4, Vec3, f32, f32)> {
    let c = its.iter().find(|i| i.kind == "camera")?;
    let p = vec3(&c.props, "position", [0.0, 0.0, 5.0]);
    let l = vec3(&c.props, "lookAt", [0.0, 0.0, 0.0]);
    let fov = c.props.get("fov").and_then(Value::as_f64).unwrap_or(50.0) as f32;
    let eye = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32);
    let far = its.iter().find(|i| i.kind == "fog").and_then(|f| f.props.get("far").and_then(Value::as_f64)).unwrap_or(f64::MAX) as f32;
    Some((Mat4::look_at_rh(eye, Vec3::new(l[0] as f32, l[1] as f32, l[2] as f32), Vec3::Y), eye, 1.0 / (fov.to_radians() * 0.5).tan(), far))
}

/// Whether a ray from `p` to the eye passes through a solid (the first `skip` metres left out: the seam's own
/// surfaces).
fn blocked(p: Vec3, eye: Vec3, skip: f32, solids: &[(&Solid, Mat4)]) -> bool {
    let len = (eye - p).length();
    if len <= skip {
        return false;
    }
    let dir = (eye - p) / len;
    let from = p + dir * skip;
    for (s, w) in solids {
        let inv = w.inverse();
        let (a, b) = (inv.transform_point3(from), inv.transform_point3(eye));
        let d = b - a;
        // the segment against the solid's box, in its own space
        let (mut t0, mut t1) = (0f32, 1f32);
        for k in 0..3 {
            if d[k].abs() < 1e-9 {
                if a[k] < s.lo[k] || a[k] > s.hi[k] {
                    t1 = -1.0;
                }
                continue;
            }
            let (u, v) = ((s.lo[k] - a[k]) / d[k], (s.hi[k] - a[k]) / d[k]);
            t0 = t0.max(u.min(v));
            t1 = t1.min(u.max(v));
        }
        if t0 > t1 {
            continue;
        }
        let steps = ((t1 - t0) * d.length() / (0.7 * s.cell)).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let q = a + d * (t0 + (t1 - t0) * k as f32 / steps as f32);
            if s.at(q) >= 3 {
                return true;
            }
        }
    }
    false
}

/// Whether enough of a seam shows on this frame: inside the frame, in front, nearer than the fog's far edge, not hidden,
/// and at least a few pixels across.
fn shows(its: &[Item], seam: &[Vec3], solids: &[(&Solid, Mat4)], skip: f32, w: f32, h: f32) -> bool {
    let Some((view, eye, f, far)) = camera(its) else { return true };
    let (mut lo, mut hi, mut seen) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), 0);
    for p in seam {
        let q = view.transform_point3(*p);
        if q.z >= -0.05 || -q.z >= far {
            continue;
        }
        let s = Vec3::new(q.x / -q.z * f * h / w, q.y / -q.z * f, 0.0);
        if s.x.abs() > 1.0 || s.y.abs() > 1.0 || blocked(*p, eye, skip, solids) {
            continue;
        }
        seen += 1;
        lo = lo.min(s);
        hi = hi.max(s);
    }
    seen >= 3.min(seam.len()).max(1) && ((hi.x - lo.x) * w * 0.5).max((hi.y - lo.y) * h * 0.5) >= 6.0
}

/// The overlaps a render shows: each with a sentence (for the agent) and its numbers (for the manifest).
pub(crate) fn check(frames: &[(i64, Frame)], w: u32, h: u32, fps: f64) -> Vec<Value> {
    if std::env::var("RAEA_CLIP_CHECK").as_deref() == Ok("0") || frames.iter().all(|(_, f)| f.scene.is_empty()) {
        return Vec::new();
    }
    let all = all_items(frames);
    let parts: Vec<(String, Vec<Option<Item>>)> = tracks_of(&all, &["mesh", "model"]);
    let solids: Vec<Option<Arc<Vec<Solid>>>> = parts.iter().map(|(_, tr)| solid_of(tr.iter().flatten().next()?).ok()).collect();
    let placed: Vec<Vec<Item>> = parts.iter().map(|(_, tr)| filled(tr)).collect();
    // the pieces that hide others on frame k: every checked part there, see-through ones left out
    let placed = &placed;
    let occluders = |k: usize| -> Vec<(&Solid, Mat4)> {
        parts.iter().enumerate().filter_map(|(i, (_, tr))| Some((i, tr[k].as_ref()?, solids[i].as_ref()?)))
            .filter(|(_, it, _)| !excused(it) || it.props.get("allowOverlap").and_then(Value::as_bool) == Some(true))
            .flat_map(|(i, _, ss)| ss.iter().map(move |s| (s, placed[i][k].world)))
            .collect()
    };
    // frame by frame: the parts' world boxes swept along x for the pairs that meet, each pair's test reused while
    // neither part moves (the camera may still have)
    let mut last: HashMap<(usize, usize), (Mat4, Mat4, Option<Hit>)> = HashMap::new();
    let mut found: HashMap<(usize, usize), Vec<(usize, Hit)>> = HashMap::new();
    for k in 0..frames.len() {
        let mut boxes: Vec<(usize, Vec3, Vec3)> = Vec::new();
        for (i, (_, tr)) in parts.iter().enumerate() {
            let (Some(it), Some(ss)) = (&tr[k], &solids[i]) else { continue };
            if excused(it) {
                continue;
            }
            let w = placed[i][k].world;
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for s in ss.iter() {
                let (l, h) = world_box(s, &w);
                lo = lo.min(l);
                hi = hi.max(h);
            }
            boxes.push((i, lo, hi));
        }
        boxes.sort_by(|x, y| x.1.x.total_cmp(&y.1.x));
        let mut occ: Option<Vec<(&Solid, Mat4)>> = None;
        for (n, &(i0, lo0, hi0)) in boxes.iter().enumerate() {
            for &(j0, lo1, hi1) in &boxes[n + 1..] {
                if lo1.x > hi0.x {
                    break;
                }
                if lo0.cmpgt(hi1).any() || lo1.cmpgt(hi0).any() {
                    continue;
                }
                let (i, j) = (i0.min(j0), i0.max(j0));
                let (a, b) = (solids[i].as_ref().unwrap(), solids[j].as_ref().unwrap());
                let (wa, wb) = (placed[i][k].world, placed[j][k].world);
                let r = match last.get(&(i, j)) {
                    Some((la, lb, r)) if *la == wa && *lb == wb => r.clone(),
                    _ => {
                        let mut best: Option<Hit> = None;
                        for (pa, sa) in a.iter().enumerate() {
                            for (pb, sb) in b.iter().enumerate() {
                                if let Some(mut hit) = test(sa, &wa, sb, &wb) {
                                    if best.as_ref().is_none_or(|b| hit.depth > b.depth) {
                                        hit.pieces = (pa, pb);
                                        best = Some(hit);
                                    }
                                }
                            }
                        }
                        best
                    }
                };
                last.insert((i, j), (wa, wb, r.clone()));
                if let Some(hit) = r {
                    let (sa, sb) = (&a[hit.pieces.0], &b[hit.pieces.1]);
                    let skip = 3.0 * (sa.cell * scale_of(&wa)).max(sb.cell * scale_of(&wb));
                    if shows(&all[k], &hit.seam, occ.get_or_insert_with(|| occluders(k)), skip, w as f32, h as f32) {
                        found.entry((i, j)).or_default().push((k, hit));
                    }
                }
            }
        }
    }
    let mut spans: Vec<((usize, usize), Vec<(usize, Hit)>)> = found.into_iter().collect();
    spans.sort_by_key(|(p, g)| (g[0].0, *p));
    let clock = |f: i64| {
        let s = f as f64 / fps;
        format!("{}:{:04.1}", (s / 60.0) as i64, s % 60.0)
    };
    let mut out = Vec::new();
    for ((i, j), got) in spans {
        // a stretch is rendered frames in a row
        let mut runs: Vec<Vec<&(usize, Hit)>> = Vec::new();
        for g in &got {
            match runs.last_mut() {
                Some(r) if r.last().unwrap().0 + 1 == g.0 => r.push(g),
                _ => runs.push(vec![g]),
            }
        }
        for run in runs {
            let &&(k, ref hit) = run.iter().max_by(|x, y| x.1.depth.total_cmp(&y.1.depth)).unwrap();
            let (pa, pb) = (parts[i].1[k].as_ref().unwrap(), parts[j].1[k].as_ref().unwrap());
            let (sa, sb) = (solids[i].as_ref().unwrap(), solids[j].as_ref().unwrap());
            let (la, lb) = (label(pa, &sa[hit.pieces.0].name), label(pb, &sb[hit.pieces.1].name));
            let (small, big) = if hit.small_first { (la, lb) } else { (lb, la) };
            // out the shortest way: the smallest move along a world axis (not down) after which no piece of the one
            // passes into any piece of the other
            let (wa, wb) = (placed[i][k].world, placed[j][k].world);
            let small_is_a = hit.small_first;
            let (axis, step) = clear_move(&sa[..], &wa, &sb[..], &wb, small_is_a);
            let axis_name = ["x", "y", "z"][axis];
            let (f0, f1) = (frames[run.first().unwrap().0].0, frames[run.last().unwrap().0].0);
            let when = if f0 == f1 { clock(f0) } else { format!("{}–{}", clock(f0), clock(f1)) };
            let says = format!(
                "{when} {small} passes {:.2} m into {big} ({:.0}% of it inside), and the seam shows: move {small} {step:+.2} along world {axis_name} (or move {big}), or give it allowOverlap if it's meant to",
                hit.depth, hit.share * 100.0
            );
            out.push(json!({"from": f0, "to": f1, "part": small, "into": big, "depth": (hit.depth * 1000.0).round() / 1000.0, "share": (hit.share * 1000.0).round() / 1000.0, "move": {"axis": axis_name, "by": (step * 1000.0).round() / 1000.0}, "says": says}));
        }
    }
    out
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

/// `check`, each overlap said on stderr (what an agent driving raea-motion reads) as it's returned for the manifest.
pub(crate) fn told(frames: &[(i64, Frame)], w: u32, h: u32, fps: f64) -> Vec<Value> {
    let t = std::time::Instant::now();
    let found = check(frames, w, h, fps);
    for c in &found {
        eprintln!("raea-motion: clipping: {}", c["says"].as_str().unwrap_or_default());
    }
    if std::env::var_os("RAEA_MOTION_TIMES").is_some() {
        eprintln!("raea-motion times: the clipping check {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(c: Vec3, s: Vec3) -> Vec<[Vec3; 3]> {
        let t = rdb::shape::cached(&format!("box({}, {}, {})", s.x, s.y, s.z)).unwrap();
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
