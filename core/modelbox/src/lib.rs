//! A glTF model's own geometry for the scene.* native calls: its box turned and scaled, and which way a seat faces.
//! raea-sys-contact reads its triangles from here, and its `forget` lets these go with its solids.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use bevy::math::Vec3;
use serde_json::{Value, json};

use raea_core_scene::{gltf_tris, local};

/// Each glTF file's triangles, read once until [`forget`]
static CACHE: OnceLock<Mutex<HashMap<String, Result<Arc<Vec<Vec3>>, String>>>> = OnceLock::new();

/// Lets go of every file's triangles this process read (raea-sys-contact's `forget` calls it with its solids).
pub fn forget() {
    if let Some(mut files) = CACHE.get().and_then(|c| c.lock().ok()) {
        files.clear();
    }
}

/// A glTF file's triangles in its own space (its node transforms applied), read once until [`forget`].
pub fn file_tris(path: &str) -> Result<Arc<Vec<Vec3>>, String> {
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
pub fn bounds(path: &std::path::Path, v: &Value) -> Result<Value, String> {
    let tris = file_tris(&path.to_string_lossy())?;
    let mut p = serde_json::Map::new();
    for k in ["rotation", "scale"] {
        if let Some(x) = v.get(k) {
            p.insert(k.into(), x.clone());
        }
    }
    let m = local(&p);
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
pub fn seat_facing(tris: &[Vec3]) -> Option<([f32; 2], f32, f32)> {
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
pub fn facing(path: &std::path::Path) -> Result<Value, String> {
    let tris = file_tris(&path.to_string_lossy())?;
    Ok(match seat_facing(&tris) {
        Some((f, seat, lean)) => json!({"faces": f, "seat": seat, "lean": lean}),
        None => json!({"faces": null}),
    })
}
