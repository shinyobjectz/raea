//! A scene's parts as triangles in the world: a glTF's mesh nodes or a shape's solid, read once a process, and what
//! the contact and light checks share over them (a part's name, whether it's excused, a ray against a triangle).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use bevy::math::{Mat4, Vec3};
use serde_json::Value;

use super::{Item, base_ident, src_path};

/// A glTF file's triangles in its own space, a list per mesh node (node transforms applied) with the node's name, and
/// whether it moves (skins or animations: its pose isn't known here).
pub fn gltf_tris(path: &str) -> Result<(Vec<(String, Vec<[Vec3; 3]>)>, bool), String> {
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

/// Whether a part is left out: marked, see-through, unlit, or a model playing a clip.
pub fn excused(it: &Item) -> bool {
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
pub fn label(it: &Item, piece: &str) -> String {
    let base = match it.props.get("name").and_then(Value::as_str) {
        Some(n) => n.to_string(),
        None if it.kind == "model" => base_ident(it).rsplit('/').next().unwrap_or("a model").to_string(),
        None => format!("the mesh {}", base_ident(it).chars().take(40).collect::<String>()),
    };
    if piece.is_empty() { base } else { format!("{base} (its \"{piece}\")") }
}

/// A part's triangles in the world, with the node each came from.
#[derive(Clone)]
pub struct Part {
    pub label: String,
    pub tris: Vec<[Vec3; 3]>,
    pub node: Vec<u32>,
    pub nodes: Vec<String>,
    pub lo: Vec3,
    pub hi: Vec3,
    /// excused from the checks (see-through, unlit, allowOverlap, animating), but still a surface others rest on
    pub excused: bool,
    /// it moves between this frame and the next (in flight, keyed, baked): where it is now isn't where it rests
    pub moving: bool,
}

/// A glTF file's or a shape's triangles in its own space, read once a process.
pub fn own_tris(it: &Item) -> Result<Arc<(Vec<(String, Vec<[Vec3; 3]>)>, bool)>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Result<Arc<(Vec<(String, Vec<[Vec3; 3]>)>, bool)>, String>>>> = OnceLock::new();
    let key = match it.kind {
        "model" => format!("model:{}", src_path(it.props.get("src").and_then(Value::as_str).unwrap_or(""))),
        _ => format!("mesh:{}", it.shape.as_deref().unwrap_or("")),
    };
    if let Some(t) = CACHE.get_or_init(Default::default).lock().unwrap().get(&key) {
        return t.clone();
    }
    let made = if it.kind == "model" {
        gltf_tris(&key["model:".len()..]).map(Arc::new)
    } else {
        it.shape.as_deref().ok_or_else(|| "no shape".to_string()).and_then(raea_core_shape::cached).map(|t| {
            Arc::new((vec![(String::new(), t.positions.chunks_exact(3).map(|c| [Vec3::from(c[0]), Vec3::from(c[1]), Vec3::from(c[2])]).collect())], false))
        })
    };
    CACHE.get_or_init(Default::default).lock().unwrap().insert(key, made.clone());
    made
}

pub fn parts_of(its: &[Item], moving: &[bool]) -> (Vec<Part>, Vec<Item>, Vec<String>) {
    let (mut parts, mut items, mut skipped) = (Vec::new(), Vec::new(), Vec::new());
    for (it, &moves) in its.iter().zip(moving).filter(|(it, _)| (it.kind == "mesh" || it.kind == "model") && !it.props.contains_key("at")) {
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

/// A ray against one triangle (Möller–Trumbore): the distance along `d` (unit), if it hits ahead.
pub fn ray_tri(o: Vec3, d: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
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
