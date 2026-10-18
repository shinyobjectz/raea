//! A composition's 3D (the data-3d elements of @raea/motion/3d, read by read3d) as rdb rows: camera, lights, meshes,
//! glTF models and pictures in space, with every value that changes from frame to frame keyed on each frame. Rows
//! are matched across frames by identity, not by position in the tree: a part's `name` if it has one, else its
//! kind and shape (or source), counted in tree order. A part missing from some frames gets a `visible` column, so one
//! that appears later (inside a Sequence) shows in play exactly when it shows in a render. Groups compose their
//! transforms into their children's, in world space. The columns are rdb's (rdb/src/engine/scene3d.rs).

use std::collections::BTreeMap;

use bevy::math::{EulerRot, Mat4, Quat, Vec3};
use rdb::ir::{Program, V};
use serde_json::{Map, Value};

use super::{hex_of, num, Frame, Node3};

/// Light intensity: a DirectionalLight of intensity 1 is 8000 lux (as before); a point or spot light of intensity 1
/// lights a surface 3 units away about as brightly (8000 lux x 9 m² x 4π ≈ 900,000 lumens).
const LUX: f64 = 8000.0;
const LUMENS: f64 = 9.0e5;
/// An AmbientLight of intensity 1, in cd/m² (as before).
const AMBIENT: f64 = 400.0;
/// Bloom of intensity 1: Bevy's natural preset.
const BLOOM: f64 = 0.15;
/// Bevy's default exposure (EV100): exposure 0; +1 is a stop brighter.
const EV100: f64 = 9.7;

#[derive(Clone)]
struct Item {
    kind: &'static str,
    props: Map<String, Value>,
    shape: Option<String>,
    material: Option<Node3>,
    world: Mat4,
}

fn vec3(p: &Map<String, Value>, k: &str, d: [f64; 3]) -> [f64; 3] {
    super::vec3(p, k, d)
}

fn scale3(p: &Map<String, Value>) -> [f64; 3] {
    match p.get("scale") {
        Some(Value::Number(n)) => {
            let s = n.as_f64().unwrap_or(1.0);
            [s, s, s]
        }
        _ => vec3(p, "scale", [1.0; 3]),
    }
}

/// A part's own transform, as three.js composes it: scale, then rotation (Euler XYZ, radians), then position.
fn local(p: &Map<String, Value>) -> Mat4 {
    let pos = vec3(p, "position", [0.0; 3]);
    let r = vec3(p, "rotation", [0.0; 3]);
    let s = scale3(p);
    Mat4::from_scale_rotation_translation(
        Vec3::new(s[0] as f32, s[1] as f32, s[2] as f32),
        Quat::from_euler(EulerRot::XYZ, r[0] as f32, r[1] as f32, r[2] as f32),
        Vec3::new(pos[0] as f32, pos[1] as f32, pos[2] as f32),
    )
}

fn walk(n: &Node3, parent: Mat4, out: &mut Vec<Item>) {
    let item = |kind: &'static str, world: Mat4, material: Option<Node3>| Item { kind, props: n.props.clone(), shape: n.shape.clone(), material, world };
    match n.kind.as_str() {
        "Group" => {
            let w = parent * local(&n.props);
            for k in &n.kids {
                walk(k, w, out);
            }
        }
        "Mesh" => {
            let w = parent * local(&n.props);
            let mat = n.kids.iter().find(|k| k.kind.ends_with("Material")).cloned();
            out.push(item("mesh", w, mat));
            // a mesh's children (three.js parents meshes to meshes) move with it
            for k in &n.kids {
                walk(k, w, out);
            }
        }
        "Model" => out.push(item("model", parent * local(&n.props), None)),
        "ImagePlane" => out.push(item("panel", parent * local(&n.props), None)),
        "PerspectiveCamera" => out.push(item("camera", parent, None)),
        "AmbientLight" => out.push(item("ambient", parent, None)),
        "DirectionalLight" => out.push(item("directional", parent, None)),
        "PointLight" => out.push(item("point", parent, None)),
        "SpotLight" => out.push(item("spot", parent, None)),
        "Bloom" => out.push(item("bloom", parent, None)),
        "Fog" => out.push(item("fog", parent, None)),
        "ToneMapping" => out.push(item("tonemapping", parent, None)),
        _ => {
            for k in &n.kids {
                walk(k, parent, out);
            }
        }
    }
}

fn items(f: &Frame) -> Vec<Item> {
    let mut out = Vec::new();
    for n in &f.scene {
        walk(n, Mat4::IDENTITY, &mut out);
    }
    out
}

/// What a part is called across frames: its name, else what it draws, counted in tree order.
fn base_ident(it: &Item) -> String {
    if let Some(Value::String(n)) = it.props.get("name") {
        return n.clone();
    }
    match it.kind {
        "mesh" => it.shape.clone().unwrap_or_default(),
        "model" | "panel" => it.props.get("src").and_then(Value::as_str).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

/// Every part's identity and its item on each frame (None where it's missing), in order of first appearance.
fn tracks(frames: &[(i64, Frame)], kinds: &[&str]) -> Vec<(String, Vec<Option<Item>>)> {
    let mut order: Vec<String> = Vec::new();
    let mut by: BTreeMap<String, Vec<Option<Item>>> = BTreeMap::new();
    for (i, (_, f)) in frames.iter().enumerate() {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for it in items(f).into_iter().filter(|it| kinds.contains(&it.kind)) {
            let base = format!("{}|{}", it.kind, base_ident(&it));
            let k = seen.entry(base.clone()).or_insert(0);
            let id = format!("{base}#{k}");
            *k += 1;
            let slot = by.entry(id.clone()).or_insert_with(|| {
                order.push(id.clone());
                vec![None; frames.len()]
            });
            slot[i] = Some(it);
        }
    }
    order.into_iter().map(|id| {
        let v = by.remove(&id).unwrap();
        (id, v)
    }).collect()
}

/// Each frame's item, a missing frame holding the nearest present one (a hidden part stays where it was).
fn filled(track: &[Option<Item>]) -> Vec<Item> {
    let first = track.iter().flatten().next().cloned().expect("a track has a frame");
    let mut last = first;
    track.iter().map(|t| {
        if let Some(it) = t {
            last = it.clone();
        }
        last.clone()
    }).collect()
}

fn r6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

/// A row's numeric column: its first frame's value, keyed on every frame when it changes.
fn column(p: &mut Program, row: &mut BTreeMap<String, V>, id: &str, name: &str, vals: &[f64]) {
    let vals: Vec<f64> = vals.iter().map(|v| r6(*v)).collect();
    row.insert(name.into(), V::N(vals[0]));
    if !vals.iter().all(|v| (*v - vals[0]).abs() < 1e-9) {
        p.keys.insert((id.to_string(), name.to_string()), vals.iter().enumerate().map(|(f, v)| (f as i64, *v, "linear".to_string())).collect());
    }
}

fn visible(p: &mut Program, row: &mut BTreeMap<String, V>, id: &str, track: &[Option<Item>]) {
    if track.iter().any(Option::is_none) {
        let on: Vec<f64> = track.iter().map(|t| t.is_some() as u8 as f64).collect();
        column(p, row, id, "visible", &on);
    }
}

/// World position, rotation (rdb's rot_y, rot_x, rot_z: YXZ) and scale of each frame.
fn placement(p: &mut Program, row: &mut BTreeMap<String, V>, id: &str, its: &[Item]) {
    let per: Vec<[f64; 9]> = its.iter().map(|it| {
        let (s, q, t) = it.world.to_scale_rotation_translation();
        let (ry, rx, rz) = q.to_euler(EulerRot::YXZ);
        [t.x as f64, t.y as f64, t.z as f64, rx as f64, ry as f64, rz as f64, s.x as f64, s.y as f64, s.z as f64]
    }).collect();
    for (k, name) in ["x", "y", "z", "rot_x", "rot_y", "rot_z"].iter().enumerate() {
        column(p, row, id, name, &per.iter().map(|t| t[k]).collect::<Vec<_>>());
    }
    let sc: Vec<[f64; 3]> = per.iter().map(|t| [t[6], t[7], t[8]]).collect();
    if sc.iter().any(|s| s.iter().any(|v| (v - 1.0).abs() > 1e-9)) {
        for (k, name) in ["scale_x", "scale_y", "scale_z"].iter().enumerate() {
            column(p, row, id, name, &sc.iter().map(|s| s[k]).collect::<Vec<_>>());
        }
    }
}

fn hex(c: Option<&Value>) -> Option<String> {
    c.and_then(Value::as_str).and_then(hex_of)
}

fn rgb(h: &str) -> [f64; 3] {
    let h = h.trim_start_matches('#');
    let h = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect::<String>() } else { h.to_string() };
    let b = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("cc"), 16).unwrap_or(204) as f64 / 255.0;
    [b(0), b(2), b(4)]
}

fn n(p: &Map<String, Value>, k: &str, d: f64) -> f64 {
    num(p, k).unwrap_or(d)
}

thread_local! {
    /// The edit's folder, for the files 3D parts name (set by whoever builds the rows on this thread: a render, or
    /// live play's thread building its 3D), so the rows carry paths that hold wherever rdb runs.
    static MEDIA_DIR: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Files named by 3D parts are found from `dir` (the edit's folder) on this thread from now.
pub(crate) fn set_media_dir(dir: &std::path::Path) {
    MEDIA_DIR.with(|d| *d.borrow_mut() = Some(dir.to_path_buf()));
}

/// A /media/... URL (staticFile) as a file: absolute from the edit's folder when that's known and the file is there,
/// else a path from the edit's folder, its segments decoded.
fn src_path(src: &str) -> String {
    let rel = decoded(src);
    MEDIA_DIR.with(|d| d.borrow().as_ref().map(|dir| super::media_path(dir, &format!("/{rel}"))).filter(|p| p.exists()).map(|p| p.to_string_lossy().to_string()))
        .unwrap_or(rel)
}

fn decoded(src: &str) -> String {
    let s = src.trim_start_matches('/');
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// The camera row's 3D columns: where it is and looks (keyed), its lens, the ambient light, the post-processing.
pub(super) fn camera(frames: &[(i64, Frame)], p: &mut Program, row: &mut BTreeMap<String, V>) {
    let all: Vec<Vec<Item>> = frames.iter().map(|(_, f)| items(f)).collect();
    let cams: Vec<Option<Item>> = all.iter().map(|its| its.iter().find(|i| i.kind == "camera").cloned()).collect();
    let id = "cam";
    if cams.iter().any(Option::is_some) {
        let its = filled(&cams);
        let pos: Vec<[f64; 3]> = its.iter().map(|c| vec3(&c.props, "position", [0.0, 0.0, 5.0])).collect();
        let look: Vec<[f64; 3]> = its.iter().map(|c| vec3(&c.props, "lookAt", [0.0, 0.0, 0.0])).collect();
        for (k, name) in ["x", "y", "z"].iter().enumerate() {
            column(p, row, id, name, &pos.iter().map(|v| v[k]).collect::<Vec<_>>());
        }
        for (k, name) in ["look_x", "look_y", "look_z"].iter().enumerate() {
            column(p, row, id, name, &look.iter().map(|v| v[k]).collect::<Vec<_>>());
        }
        column(p, row, id, "fov", &its.iter().map(|c| n(&c.props, "fov", 50.0)).collect::<Vec<_>>());
        let first = &its[0].props;
        for (k, col) in [("near", "near"), ("far", "far")] {
            if let Some(v) = num(first, k) {
                row.insert(col.into(), V::N(v));
            }
        }
    } else {
        for (name, v) in [("x", 0.0), ("y", 0.0), ("z", 5.0), ("look_x", 0.0), ("look_y", 0.0), ("look_z", 0.0), ("fov", 50.0)] {
            row.insert(name.into(), V::N(v));
        }
    }
    // ambient: every AmbientLight's intensity summed, per frame; its colour the first one's
    let amb: Vec<f64> = all.iter().map(|its| its.iter().filter(|i| i.kind == "ambient").map(|i| n(&i.props, "intensity", 1.0)).sum::<f64>() * AMBIENT).collect();
    if amb.iter().any(|a| *a > 0.0) {
        column(p, row, id, "ambient", &amb);
        if let Some(c) = all.iter().flatten().find(|i| i.kind == "ambient").and_then(|i| hex(i.props.get("color"))) {
            if c != "#ffffff" {
                row.insert("ambient_color".into(), V::S(c));
            }
        }
    }
    // post-processing: as the first frame that has each asks
    let first = |kind: &str| all.iter().flatten().find(|i| i.kind == kind).map(|i| i.props.clone());
    if let Some(b) = first("bloom") {
        row.insert("bloom".into(), V::N(r6(n(&b, "intensity", 1.0) * BLOOM)));
        if let Some(t) = num(&b, "threshold") {
            row.insert("bloom_threshold".into(), V::N(t));
        }
    }
    if let Some(f) = first("fog") {
        row.insert("fog_color".into(), V::S(hex(f.get("color")).unwrap_or("#808080".into())));
        match num(&f, "density") {
            Some(d) => {
                row.insert("fog_density".into(), V::N(d));
            }
            None => {
                row.insert("fog_start".into(), V::N(n(&f, "near", 5.0)));
                row.insert("fog_end".into(), V::N(n(&f, "far", 20.0)));
            }
        }
    }
    if let Some(t) = first("tonemapping") {
        if let Some(m) = t.get("mode").and_then(Value::as_str) {
            row.insert("tonemap".into(), V::S(m.to_lowercase()));
        }
        if let Some(e) = num(&t, "exposure") {
            row.insert("exposure".into(), V::N(r6(EV100 - e)));
        }
    }
}

/// The lights, meshes, models and pictures in space, as rows.
pub(super) fn rows(frames: &[(i64, Frame)], p: &mut Program) {
    let before = p.spawns.len();
    rows_into(frames, p);
    // RAEA_SCENE_ROWS=1: the 3D rows printed (what play and render draw), for checking a scene
    if std::env::var_os("RAEA_SCENE_ROWS").is_some() {
        for (kind, rows) in p.spawns.iter().filter(|(k, _)| k == "camera").chain(p.spawns[before..].iter()) {
            for r in rows {
                let keyed: Vec<&str> = p.keys.keys().filter(|(id, _)| Some(&V::S(id.clone())) == r.get("id")).map(|(_, c)| c.as_str()).collect();
                eprintln!("3d row {kind}: {r:?} keyed {keyed:?}");
            }
        }
    }
}

fn rows_into(frames: &[(i64, Frame)], p: &mut Program) {
    // lights: directional (towards its target, the origin by default), point, spot
    let mut li = 0;
    for (_, track) in tracks(frames, &["directional", "point", "spot"]) {
        let its = filled(&track);
        let kind = its[0].kind;
        let id = if kind == "directional" { format!("sun{li}") } else { format!("{kind}{li}") };
        li += 1;
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        if kind != "directional" {
            row.insert("type".into(), V::S(kind.into()));
        }
        // a missing frame is a dark one
        let on: Vec<f64> = track.iter().map(|t| t.is_some() as u8 as f64).collect();
        let k = if kind == "directional" { LUX } else { LUMENS };
        let power: Vec<f64> = its.iter().zip(&on).map(|(i, o)| n(&i.props, "intensity", 1.0) * k * o).collect();
        column(p, &mut row, &id, if kind == "directional" { "lux" } else { "lumens" }, &power);
        let pos: Vec<[f64; 3]> = its.iter().map(|i| {
            let v = vec3(&i.props, "position", if kind == "directional" { [0.0, 1.0, 0.0] } else { [0.0; 3] });
            let w = i.world.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
            [w.x as f64, w.y as f64, w.z as f64]
        }).collect();
        let target: Vec<[f64; 3]> = its.iter().map(|i| vec3(&i.props, "target", [0.0; 3])).collect();
        if kind != "directional" {
            for (k, name) in ["x", "y", "z"].iter().enumerate() {
                column(p, &mut row, &id, name, &pos.iter().map(|v| v[k]).collect::<Vec<_>>());
            }
        }
        if kind != "point" {
            for (k, name) in ["dir_x", "dir_y", "dir_z"].iter().enumerate() {
                column(p, &mut row, &id, name, &pos.iter().zip(&target).map(|(a, b)| b[k] - a[k]).collect::<Vec<_>>());
            }
        }
        let first = &its[0].props;
        if let Some(c) = hex(first.get("color")).filter(|c| c != "#ffffff") {
            row.insert("color".into(), V::S(c));
        }
        if let Some(Value::Bool(s)) = first.get("castShadow") {
            row.insert("shadows".into(), V::N(*s as u8 as f64));
        }
        if kind != "directional" {
            if let Some(d) = num(first, "distance").filter(|d| *d > 0.0) {
                row.insert("range".into(), V::N(d));
            }
        }
        if kind == "spot" {
            let angle = n(first, "angle", std::f64::consts::FRAC_PI_3).clamp(0.0, std::f64::consts::FRAC_PI_2);
            row.insert("outer".into(), V::N(r6(angle)));
            row.insert("inner".into(), V::N(r6(angle * (1.0 - n(first, "penumbra", 0.0).clamp(0.0, 1.0)))));
        }
        p.spawns.push(("light".into(), vec![row]));
    }
    // meshes: shape and surface from their first frame; placement and the surface values that move, keyed
    for (m, (_, track)) in tracks(frames, &["mesh"]).into_iter().enumerate() {
        let its = filled(&track);
        let Some(shape) = its[0].shape.clone() else { continue };
        let id = format!("mesh{m}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        row.insert("shape".into(), V::S(shape));
        let mat = |i: &Item| i.material.as_ref().map(|m| m.props.clone()).unwrap_or_default();
        let mats: Vec<Map<String, Value>> = its.iter().map(mat).collect();
        let basic = its[0].material.as_ref().is_some_and(|m| m.kind == "MeshBasicMaterial");
        let colors: Vec<String> = mats.iter().map(|m| hex(m.get("color")).unwrap_or("#cccccc".into())).collect();
        row.insert("color".into(), V::S(colors[0].clone()));
        if colors.iter().any(|c| *c != colors[0]) {
            let c: Vec<[f64; 3]> = colors.iter().map(|c| rgb(c)).collect();
            for (k, name) in ["col_r", "col_g", "col_b"].iter().enumerate() {
                let vals: Vec<f64> = c.iter().map(|v| v[k]).collect();
                p.keys.insert((id.clone(), name.to_string()), vals.iter().enumerate().map(|(f, v)| (f as i64, r6(*v), "linear".to_string())).collect());
                row.insert(name.to_string(), V::N(r6(vals[0])));
            }
        }
        column(p, &mut row, &id, "rough", &mats.iter().map(|m| n(m, "roughness", 0.5)).collect::<Vec<_>>());
        column(p, &mut row, &id, "metal", &mats.iter().map(|m| n(m, "metalness", 0.0)).collect::<Vec<_>>());
        let m0 = &mats[0];
        if let Some(e) = hex(m0.get("emissive")).filter(|e| e != "#000000") {
            row.insert("emissive".into(), V::S(e));
            column(p, &mut row, &id, "emissive_k", &mats.iter().map(|m| n(m, "emissiveIntensity", 1.0)).collect::<Vec<_>>());
        }
        let see_through = mats.iter().any(|m| m.get("transparent") == Some(&Value::Bool(true)) || num(m, "opacity").is_some_and(|o| o < 1.0));
        if see_through {
            column(p, &mut row, &id, "opacity", &mats.iter().map(|m| n(m, "opacity", 1.0)).collect::<Vec<_>>());
        }
        if basic || m0.get("unlit") == Some(&Value::Bool(true)) {
            row.insert("unlit".into(), V::N(1.0));
        }
        for (k, col) in [("clearcoat", "clearcoat"), ("clearcoatRoughness", "clearcoat_rough"), ("reflectance", "reflectance")] {
            if let Some(v) = num(m0, k) {
                row.insert(col.into(), V::N(v));
            }
        }
        if let Some(src) = m0.get("map").and_then(Value::as_str) {
            row.insert("map".into(), V::S(src_path(src)));
        }
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, &track);
        p.spawns.push(("mesh".into(), vec![row]));
    }
    // glTF models: the file, its clip and the clip's time (keyed: the component works it out from the frame)
    for (k, (_, track)) in tracks(frames, &["model"]).into_iter().enumerate() {
        let its = filled(&track);
        let id = format!("model{k}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        let first = &its[0].props;
        row.insert("src".into(), V::S(src_path(first.get("src").and_then(Value::as_str).unwrap_or(""))));
        match first.get("animation") {
            Some(Value::String(s)) => {
                row.insert("anim".into(), V::S(s.clone()));
            }
            Some(Value::Number(x)) => {
                row.insert("anim".into(), V::N(x.as_f64().unwrap_or(0.0)));
            }
            _ => {}
        }
        if first.contains_key("animation") {
            column(p, &mut row, &id, "anim_t", &its.iter().map(|i| n(&i.props, "time", 0.0)).collect::<Vec<_>>());
            if first.get("loop") == Some(&Value::Bool(false)) {
                row.insert("anim_loop".into(), V::N(0.0));
            }
        }
        if let Some(s) = num(first, "scene") {
            row.insert("scene".into(), V::N(s));
        }
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, &track);
        p.spawns.push(("model".into(), vec![row]));
    }
    // pictures in space (rdb's panel: unlit, painted once, moved by the GPU)
    for (k, (_, track)) in tracks(frames, &["panel"]).into_iter().enumerate() {
        let its = filled(&track);
        let id = format!("plane{k}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        let first = &its[0].props;
        row.insert("src".into(), V::S(src_path(first.get("src").and_then(Value::as_str).unwrap_or(""))));
        row.insert("w".into(), V::N(n(first, "width", 1.0)));
        if let Some(h) = num(first, "height") {
            row.insert("h".into(), V::N(h));
        }
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, &track);
        p.spawns.push(("panel".into(), vec![row]));
    }
}
