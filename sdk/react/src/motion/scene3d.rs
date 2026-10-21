//! A composition's 3D (the data-3d elements of @raea/motion/3d, read by read3d) as rdb rows: camera, lights, meshes,
//! glTF models and pictures in space, with every value that changes from frame to frame keyed on each frame. Rows
//! are matched across frames by identity, not by position in the tree: a part's `name` if it has one, else its
//! kind and shape (or source), counted in tree order. A part missing from some frames gets a `visible` column, so one
//! that appears later (inside a Sequence) shows in play exactly when it shows in a render. Groups compose their
//! transforms into their children's, in world space. The columns are rdb's (engine/rdb/src/engine/scene3d.rs).

use std::collections::BTreeMap;

use bevy::math::{EulerRot, Mat4, Quat, Vec3};
use rdb::ir::{Program, V};
use serde_json::{Map, Value};

use super::{hex_of, num, Frame, Node3};

/// Parts that pass through each other on screen (a check on the frames, not drawn).
#[path = "clip3d.rs"]
pub(crate) mod clip3d;
/// Gaussian splats' rows (<Splat>).
#[path = "splat3d.rs"]
mod splat3d;
/// Baked simulations' and glitches' rows (<MeshSequence>, <Goo>, glitch props).
#[path = "meshseq3d.rs"]
mod meshseq3d;

/// Light intensity: a DirectionalLight of intensity 1 is 8000 lux (as before); a point or spot light of intensity 1
/// lights a surface 3 units away about as brightly (8000 lux x 9 m² x 4π ≈ 900,000 lumens).
const LUX: f64 = 8000.0;
const LUMENS: f64 = 9.0e5;
/// An AmbientLight of intensity 1, in cd/m² (as before).
const AMBIENT: f64 = 400.0;
/// An <Environment> of intensity 1: the panorama's values (a Poly Haven HDR's sky is about 1) times this, in cd/m².
const ENV: f64 = 1000.0;
/// Bloom of intensity 1: Bevy's natural preset.
const BLOOM: f64 = 0.15;
/// Bevy's default exposure (EV100): exposure 0; +1 is a stop brighter.
const EV100: f64 = 9.7;

#[derive(Clone)]
struct Item {
    kind: &'static str,
    props: std::sync::Arc<Map<String, Value>>,
    shape: Option<std::sync::Arc<str>>,
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
    // a rotation as a quaternion [x, y, z, w] (a rigid body's, physics.tsx) wins over the Euler angles
    let q = p.get("quaternion").and_then(Value::as_array).filter(|q| q.len() == 4).map(|q| Quat::from_xyzw(q[0].as_f64().unwrap_or(0.0) as f32, q[1].as_f64().unwrap_or(0.0) as f32, q[2].as_f64().unwrap_or(0.0) as f32, q[3].as_f64().unwrap_or(1.0) as f32).normalize());
    Mat4::from_scale_rotation_translation(
        Vec3::new(s[0] as f32, s[1] as f32, s[2] as f32),
        q.unwrap_or_else(|| Quat::from_euler(EulerRot::XYZ, r[0] as f32, r[1] as f32, r[2] as f32)),
        Vec3::new(pos[0] as f32, pos[1] as f32, pos[2] as f32),
    )
}

fn walk(n: &Node3, parent: Mat4, out: &mut Vec<Item>) {
    let item = |kind: &'static str, world: Mat4, material: Option<Node3>| Item { kind, props: n.props.clone(), shape: n.shape.clone(), material, world };
    match &*n.kind {
        "Group" => {
            let w = parent * local(&n.props);
            for k in &n.kids {
                walk(k, w, out);
            }
        }
        // <Refinish>: every model inside re-covered (after the model's own MeshStandardMaterial children, which name
        // theirs more closely)
        "Refinish" => {
            let start = out.len();
            for k in &n.kids {
                walk(k, parent, out);
            }
            let Some(Value::Array(given)) = n.props.get("materials") else { return };
            let specs: Vec<Value> = given.iter().filter_map(Value::as_object).map(surface_spec).collect();
            for it in out[start..].iter_mut().filter(|it| it.kind == "model") {
                let p = std::sync::Arc::make_mut(&mut it.props);
                let mut all = match p.remove("surfaces") {
                    Some(Value::Array(own)) => own,
                    _ => Vec::new(),
                };
                all.extend(specs.iter().cloned());
                p.insert("surfaces".into(), Value::Array(all));
            }
        }
        "Mesh" => {
            let w = parent * local(&n.props);
            let mat = n.kids.iter().find(|k| k.kind.ends_with("Material")).cloned();
            let mut it = item("mesh", w, mat);
            // a compiled-in Bevy plugin's components on it (<BevyComponent>), applied through the TypeRegistry
            let comps: Vec<Value> = n.kids.iter().filter(|k| &*k.kind == "BevyComponent").map(|k| Value::Object((*k.props).clone())).collect();
            if !comps.is_empty() {
                std::sync::Arc::make_mut(&mut it.props).insert("components".into(), Value::Array(comps));
            }
            out.push(it);
            // a mesh's children (three.js parents meshes to meshes) move with it
            for k in &n.kids {
                walk(k, w, out);
            }
        }
        "Model" => {
            let mut it = item("model", parent * local(&n.props), None);
            // MeshToonMaterial children: toon surfaces over its glTF materials (all, or the one each names)
            let toons: Vec<Value> = n.kids.iter().filter(|k| &*k.kind == "MeshToonMaterial").map(|k| toon_spec(&k.props, true)).collect();
            if !toons.is_empty() {
                std::sync::Arc::make_mut(&mut it.props).insert("toon".into(), Value::Array(toons));
            }
            // MeshStandardMaterial children: its glTF materials (all, or the one each names) given another colour,
            // texture or finish, the rest of each kept
            let surfaces: Vec<Value> = n.kids.iter().filter(|k| matches!(&*k.kind, "MeshStandardMaterial" | "MeshPhysicalMaterial")).map(|k| surface_spec(&k.props)).collect();
            if !surfaces.is_empty() {
                std::sync::Arc::make_mut(&mut it.props).insert("surfaces".into(), Value::Array(surfaces));
            }
            out.push(it)
        }
        "ImagePlane" => out.push(item("panel", parent * local(&n.props), None)),
        // a picture pressed onto whatever surface its box crosses (<Decal>: rdb's forward decal, its +y out of the surface)
        "Decal" => out.push(item("decal", parent * local(&n.props), None)),
        // footage behind the 3D, filling the camera's view (<Plate>, <Matchmove>): not moved by groups
        "Plate" => out.push(item("plate", Mat4::IDENTITY, None)),
        "PerspectiveCamera" => out.push(item("camera", parent, None)),
        "AmbientLight" => out.push(item("ambient", parent, None)),
        "DirectionalLight" => out.push(item("directional", parent, None)),
        "PointLight" => out.push(item("point", parent, None)),
        "SpotLight" => out.push(item("spot", parent, None)),
        "Bloom" => out.push(item("bloom", parent, None)),
        "Fog" => out.push(item("fog", parent, None)),
        "ToneMapping" => out.push(item("tonemapping", parent, None)),
        "DepthOfField" => out.push(item("dof", parent, None)),
        "Grade" => out.push(item("grade", parent, None)),
        "VolumetricFog" => out.push(item("volfog", parent, None)),
        "Environment" => out.push(item("environment", parent, None)),
        "AmbientOcclusion" => out.push(item("ssao", parent, None)),
        "AntiAliasing" => out.push(item("aa", parent, None)),
        "Shadows" => out.push(item("shadows", parent, None)),
        "FogVolume" => out.push(item("fogvolume", parent * local(&n.props), None)),
        "Fire" => out.push(item("fire", parent * local(&n.props), None)),
        "Splat" => out.push(item("splat", parent * local(&n.props), None)),
        "MeshSequence" => out.push(item("meshseq", parent * local(&n.props), None)),
        "Look" => out.push(item("framelook", parent, None)),
        "Outline" => out.push(item("outline", parent, None)),
        "Fisheye" => out.push(item("fisheye", parent, None)),
        "LensDistortion" => out.push(item("distortion", parent, None)),
        "MotionBlur" => out.push(item("mblur", parent, None)),
        "BevyResource" => out.push(item("resource", parent, None)),
        _ => {
            for k in &n.kids {
                walk(k, parent, out);
            }
        }
    }
}

/// A MeshToonMaterial's static values as rdb's `toon` column reads them (engine/rdb/src/engine/toon.rs): steps or bands, the
/// shadow colour, rim, highlight and outline. A model's also carries its colour override and the glTF material it's for.
fn toon_spec(p: &Map<String, Value>, model: bool) -> Value {
    let mut o = Map::new();
    if let Some(s) = num(p, "steps") {
        o.insert("steps".into(), Value::from(s));
    }
    if let Some(Value::Array(bs)) = p.get("bands") {
        let bands: Vec<Value> = bs.iter().map(|b| serde_json::json!([b.get("at").and_then(Value::as_f64).unwrap_or(0.0), hex(b.get("color"))])).collect();
        o.insert("bands".into(), Value::Array(bands));
    }
    if let Some(c) = hex(p.get("shadowColor")) {
        o.insert("shadow".into(), Value::String(c));
    }
    let part = |k: &str, flat: &str, size: &str| -> Option<Value> {
        let x = p.get(k).and_then(Value::as_object)?;
        let mut v = Map::new();
        v.insert("k".into(), Value::from(num(p, flat).or_else(|| num(x, "intensity")).unwrap_or(1.0)));
        if let Some(c) = hex(x.get("color")) {
            v.insert("color".into(), Value::String(c));
        }
        if let Some(w) = num(x, size) {
            v.insert(size.into(), Value::from(w));
        }
        Some(Value::Object(v))
    };
    if let Some(r) = part("rim", "rimIntensity", "width") {
        o.insert("rim".into(), r);
    }
    if let Some(h) = part("highlight", "highlightIntensity", "size") {
        o.insert("spec".into(), h);
    }
    if let Some(x) = p.get("outline").and_then(Value::as_object) {
        let mut v = Map::new();
        v.insert("width".into(), Value::from(num(p, "outlineWidth").or_else(|| num(x, "width")).unwrap_or(2.0)));
        if let Some(c) = hex(x.get("color")) {
            v.insert("color".into(), Value::String(c));
        }
        o.insert("outline".into(), Value::Object(v));
    }
    if model {
        for k in ["color", "material"] {
            if let Some(Value::String(c)) = p.get(k) {
                o.insert(k.into(), Value::String(c.clone()));
            }
        }
    }
    Value::Object(o)
}

/// A model's surface override (rdb's surface.rs): the material's props as a mesh row's columns, and the glTF material
/// it names, if one.
fn surface_spec(p: &Map<String, Value>) -> Value {
    let mut o = Map::new();
    if let Some(Value::String(m)) = p.get("material") {
        o.insert("material".into(), Value::String(m.clone()));
    }
    for k in ["color", "emissive"] {
        if let Some(c) = hex(p.get(k)) {
            o.insert(k.into(), Value::String(c));
        }
    }
    for (prop, col) in [("roughness", "rough"), ("metalness", "metal"), ("emissiveIntensity", "emissive_k"), ("opacity", "opacity"), ("clearcoat", "clearcoat"), ("clearcoatRoughness", "clearcoat_rough"), ("reflectance", "reflectance")] {
        if let Some(v) = num(p, prop) {
            o.insert(col.into(), Value::from(v));
        }
    }
    for (prop, col) in [("map", "map"), ("normalMap", "normal_map"), ("roughnessMap", "rough_map"), ("metalnessMap", "metal_map"), ("metallicRoughnessMap", "mr_map"), ("aoMap", "ao_map")] {
        if let Some(src) = p.get(prop).and_then(Value::as_str) {
            o.insert(col.into(), Value::String(src_path(src)));
        }
    }
    // map: null, the model's own picture taken off
    if p.get("map") == Some(&Value::Null) {
        o.insert("map".into(), Value::String(String::new()));
    }
    if let Some(Value::Array(r)) = p.get("repeat") {
        if let [u, v] = r.as_slice() {
            o.insert("repeat_u".into(), Value::from(u.as_f64().unwrap_or(1.0)));
            o.insert("repeat_v".into(), Value::from(v.as_f64().unwrap_or(1.0)));
        }
    }
    Value::Object(o)
}

fn items(f: &Frame) -> Vec<Item> {
    let mut out = Vec::new();
    for n in &f.scene {
        walk(n, Mat4::IDENTITY, &mut out);
    }
    out
}

/// What a part is called across frames: its name, else what it draws, counted in tree order.
fn base_ident(it: &Item) -> &str {
    if let Some(Value::String(n)) = it.props.get("name") {
        return n;
    }
    match it.kind {
        "mesh" => it.shape.as_deref().unwrap_or_default(),
        "model" | "panel" | "plate" | "splat" | "meshseq" => it.props.get("src").and_then(Value::as_str).unwrap_or(""),
        _ => "",
    }
}

/// Every part's identity and its item on each frame (None where it's missing), in order of first appearance.
fn tracks(frames: &[(i64, Frame)], kinds: &[&str]) -> Vec<(String, Vec<Option<Item>>)> {
    tracks_of(&all_items(frames), kinds)
}

/// Each frame's items, walked once (a frame's tree was walked again for each kind of part: 11 times a live frame)
fn all_items(frames: &[(i64, Frame)]) -> Vec<Vec<Item>> {
    frames.iter().map(|(_, f)| items(f)).collect()
}

/// `tracks` over items already walked: only the parts of these kinds are copied
fn tracks_of(all: &[Vec<Item>], kinds: &[&str]) -> Vec<(String, Vec<Option<Item>>)> {
    let mut order: Vec<String> = Vec::new();
    let mut by: BTreeMap<String, Vec<Option<Item>>> = BTreeMap::new();
    let frames = all;
    for (i, its) in all.iter().enumerate() {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for it in its.iter().filter(|it| kinds.contains(&it.kind)).cloned() {
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

/// A model's look as rdb's look column has it: 0 solid, 1 pixel, 2 site.
fn look_of(props: &Map<String, Value>) -> f64 {
    match props.get("look").and_then(Value::as_str) {
        Some("pixel") => 1.0,
        Some("site") => 2.0,
        _ => 0.0,
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
    // a picture inline (data:image/png;base64,..., data:image/svg+xml;utf8,<svg>): rdb decodes it in memory, as written
    if src.starts_with("data:") {
        return src.to_string();
    }
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

/// The distance the camera focuses at: its `focus`, else how far its lookAt point is.
fn focus_of(c: &Item) -> f64 {
    num(&c.props, "focus").unwrap_or_else(|| {
        let (p, l) = (vec3(&c.props, "position", [0.0, 0.0, 5.0]), vec3(&c.props, "lookAt", [0.0, 0.0, 0.0]));
        ((p[0] - l[0]).powi(2) + (p[1] - l[1]).powi(2) + (p[2] - l[2]).powi(2)).sqrt().max(0.01)
    })
}

/// The paint look's keyed values on one frame (rdb's look.rs and paint.rs read them): its tone bands, paper grain,
/// how far edges wander, and the wander's seed (the SDK folds a boil's beat into it).
fn paint_keyed(l: &Map<String, Value>) -> [(&'static str, f64); 4] {
    [("paint_bands", n(l, "bands", 3.0)), ("paint_grain", n(l, "grain", 0.3)), ("paint_rag", n(l, "rag", 2.0)), ("paint_seed", n(l, "seed", 0.0))]
}

/// The paint look's columns on the camera row: the keyed ones per frame (a frame without the <Look> holds the first's),
/// and its colours, sky and palette from the first frame.
fn paint_rows(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>, first: &Map<String, Value>) {
    let per: Vec<[(&str, f64); 4]> = all.iter().map(|its| paint_keyed(its.iter().find(|i| i.kind == "framelook").map(|i| &*i.props).unwrap_or(first))).collect();
    for k in 0..4 {
        column(p, row, "cam", per[0][k].0, &per.iter().map(|v| v[k].1).collect::<Vec<_>>());
    }
    row.insert("paint_rag_scale".into(), V::N(n(first, "ragScale", 24.0)));
    for (k, col) in [("shadow", "paint_shadow"), ("mid", "paint_mid")] {
        if let Some(h) = hex(first.get(k)) {
            row.insert(col.into(), V::S(h));
        }
    }
    // the sky: one colour, or [top, bottom]
    match first.get("sky") {
        Some(Value::Array(a)) => {
            if let Some(h) = hex(a.first()) {
                row.insert("paint_sky_top".into(), V::S(h));
            }
            if let Some(h) = hex(a.get(1)) {
                row.insert("paint_sky_bottom".into(), V::S(h));
            }
        }
        Some(v) => {
            if let Some(h) = hex(Some(v)) {
                row.insert("paint_sky_top".into(), V::S(h));
            }
        }
        None => {}
    }
    if let Some(Value::Array(a)) = first.get("palette") {
        let hs: Vec<String> = a.iter().filter_map(|c| hex(Some(c))).collect();
        if !hs.is_empty() {
            row.insert("paint_palette".into(), V::S(hs.join(",")));
        }
    }
}

/// The camera row's 3D columns: where it is and looks (keyed), its lens, the ambient light, the post-processing.
pub(super) fn camera(frames: &[(i64, Frame)], p: &mut Program, row: &mut BTreeMap<String, V>) {
    camera_of(&all_items(frames), p, row)
}

fn camera_of(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>) {
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
        // which way is up (`up`; +Y by default): a camera that rolls, as a solved one does (rdb's up_x/y/z, Matchmove)
        if its.iter().any(|c| c.props.get("up").is_some()) {
            let up: Vec<[f64; 3]> = its.iter().map(|c| vec3(&c.props, "up", [0.0, 1.0, 0.0])).collect();
            for (k, name) in ["up_x", "up_y", "up_z"].iter().enumerate() {
                column(p, row, id, name, &up.iter().map(|v| v[k]).collect::<Vec<_>>());
            }
        }
        // a window on a larger view (`view`: its full size and this frame's offset, in frame pixels): a crop of
        // footage keeps its lens's centre (rdb's camera view_*, Matchmove's 9:16 from 16:9)
        if its.iter().any(|c| c.props.get("view").is_some_and(Value::is_object)) {
            let view = |c: &Item, k: &str| c.props.get("view").and_then(|v| v.get(k)).and_then(Value::as_f64).unwrap_or(0.0);
            for (k, col) in [("width", "view_w"), ("height", "view_h"), ("x", "view_x"), ("y", "view_y")] {
                column(p, row, id, col, &its.iter().map(|c| view(c, k)).collect::<Vec<_>>());
            }
        }
        if all.iter().flatten().any(|i| i.kind == "dof") {
            column(p, row, id, "focus", &its.iter().map(focus_of).collect::<Vec<_>>());
        }
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
    // an <Environment>: each frame's panorama (an index into env_srcs, -1 for none), its light, turn and sky, keyed, so
    // a cut from a hall to a street changes the light on that frame
    let envs: Vec<Option<&Item>> = all.iter().map(|its| its.iter().find(|i| i.kind == "environment")).collect();
    if envs.iter().any(Option::is_some) {
        let mut srcs: Vec<String> = Vec::new();
        let idx: Vec<f64> = envs.iter().map(|e| match e.and_then(|e| e.props.get("src")).and_then(Value::as_str) {
            Some(s) => {
                let s = src_path(s);
                (srcs.iter().position(|x| *x == s).unwrap_or_else(|| { srcs.push(s); srcs.len() - 1 })) as f64
            }
            None => -1.0,
        }).collect();
        row.insert("env_srcs".into(), V::S(serde_json::to_string(&srcs).unwrap_or_default()));
        column(p, row, id, "env_i", &idx);
        let k = |e: &Option<&Item>| e.map(|e| n(&e.props, "intensity", 1.0)).unwrap_or(0.0);
        column(p, row, id, "env_k", &envs.iter().map(|e| k(e) * ENV).collect::<Vec<_>>());
        column(p, row, id, "env_rot", &envs.iter().map(|e| e.map(|e| n(&e.props, "rotation", 0.0)).unwrap_or(0.0)).collect::<Vec<_>>());
        let sky: Vec<f64> = envs.iter().map(|e| match e {
            Some(e) if e.props.get("background") == Some(&Value::Bool(true)) => num(&e.props, "backgroundIntensity").unwrap_or(k(&Some(e))) * ENV,
            _ => 0.0,
        }).collect();
        if sky.iter().any(|s| *s > 0.0) {
            column(p, row, id, "sky_k", &sky);
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
    if let Some(d) = first("dof") {
        row.insert("dof_aperture".into(), V::N(n(&d, "aperture", 2.8)));
        row.insert("dof_max".into(), V::N(n(&d, "maxBlur", 64.0)));
        if d.get("mode").and_then(Value::as_str) == Some("gaussian") {
            row.insert("dof_mode".into(), V::S("gaussian".into()));
        }
    }
    if let Some(o) = first("outline") {
        row.insert("outline".into(), V::S(hex(o.get("color")).unwrap_or("#000000".into())));
        row.insert("outline_depth".into(), V::N(n(&o, "depth", 0.04)));
        row.insert("outline_normal".into(), V::N(n(&o, "normal", 0.3)));
        if let Some(w) = num(&o, "width") {
            row.insert("outline_width".into(), V::N(w));
        }
    }
    if let Some(l) = first("framelook") {
        let mode = l.get("mode").and_then(Value::as_str).unwrap_or("site");
        row.insert("frame_look".into(), V::S(mode.into()));
        row.insert("frame_look_px".into(), V::N(n(&l, "pixelSize", if mode == "paint" { 1.0 } else { 6.0 })));
        if mode == "paint" {
            paint_rows(all, p, row, &l);
        }
        row.insert("frame_look_color".into(), V::N(n(&l, "color", 1.0)));
        row.insert("frame_look_lift".into(), V::N(n(&l, "lift", 0.0)));
        row.insert("frame_look_gain".into(), V::N(n(&l, "gain", 1.0)));
        if l.get("theme").and_then(Value::as_str) == Some("light") {
            row.insert("frame_look_theme".into(), V::S("light".into()));
        }
        // the ink look's two colours and its line
        for (k, col) in [("ink", "frame_look_ink"), ("paper", "frame_look_paper")] {
            if let Some(h) = hex(l.get(k)) {
                row.insert(col.into(), V::S(h));
            }
        }
        row.insert("frame_look_line".into(), V::N(n(&l, "line", 0.0)));
        row.insert("frame_look_threshold".into(), V::N(n(&l, "threshold", 0.5)));
        row.insert("frame_look_band".into(), V::N(n(&l, "band", 1.0)));
    }
    if let Some(g) = first("grade") {
        for (k, col) in [("temperature", "grade_temperature"), ("tint", "grade_tint"), ("saturation", "grade_saturation"), ("aberration", "aberration")] {
            if let Some(x) = num(&g, k) {
                row.insert(col.into(), V::N(x));
            }
        }
        for (sec, pre) in [("shadows", "grade_s_"), ("midtones", "grade_m_"), ("highlights", "grade_h_")] {
            let Some(o) = g.get(sec).and_then(Value::as_object) else { continue };
            for k in ["saturation", "contrast", "gamma", "gain", "lift"] {
                if let Some(x) = o.get(k).and_then(Value::as_f64) {
                    row.insert(format!("{pre}{k}"), V::N(x));
                }
            }
        }
        match g.get("vignette") {
            Some(Value::Number(x)) => {
                row.insert("vignette".into(), V::N(x.as_f64().unwrap_or(0.0)));
            }
            Some(Value::Object(o)) => {
                row.insert("vignette".into(), V::N(o.get("intensity").and_then(Value::as_f64).unwrap_or(1.0)));
                for (k, col) in [("radius", "vignette_radius"), ("smoothness", "vignette_smooth")] {
                    if let Some(x) = o.get(k).and_then(Value::as_f64) {
                        row.insert(col.into(), V::N(x));
                    }
                }
                if let Some(c) = hex(o.get("color")) {
                    row.insert("vignette_color".into(), V::S(c));
                }
            }
            _ => {}
        }
    }
    // ambient occlusion (no multisampling with it: SMAA unless asked otherwise) and the anti-aliasing
    let ssao = first("ssao");
    if let Some(a) = &ssao {
        row.insert("ssao".into(), V::S(a.get("quality").and_then(Value::as_str).unwrap_or("medium").to_string()));
        if let Some(t) = num(a, "thickness") {
            row.insert("ssao_thickness".into(), V::N(t));
        }
    }
    let aa = first("aa").and_then(|a| a.get("mode").and_then(Value::as_str).map(str::to_string));
    match (aa.as_deref(), ssao.is_some()) {
        (Some("msaa") | None, true) => {
            row.insert("aa".into(), V::S("smaa".into()));
        }
        (Some(m), _) if m != "msaa" => {
            row.insert("aa".into(), V::S(m.to_string()));
        }
        _ => {}
    }
    // shadow quality: the map size and the filter on the camera; the cascades and biases go on each light's row
    if let Some(sh) = first("shadows") {
        if let Some(m) = num(&sh, "mapSize") {
            row.insert("shadow_map".into(), V::N(m));
        }
        if let Some(f) = sh.get("filter").and_then(Value::as_str) {
            row.insert("shadow_filter".into(), V::S(f.to_string()));
        }
    }
    if let Some(v) = first("volfog") {
        row.insert("vfog".into(), V::N(1.0));
        row.insert("vfog_steps".into(), V::N(n(&v, "steps", 64.0)));
        if v.get("dither").and_then(Value::as_bool) == Some(true) {
            row.insert("vfog_dither".into(), V::N(1.0));
        }
        if let Some(r) = num(&v, "resolution") {
            row.insert("vfog_resolution".into(), V::N(r));
        }
        if let Some(m) = v.get("mode").and_then(Value::as_str).filter(|m| matches!(*m, "froxel" | "march")) {
            row.insert("vfog_mode".into(), V::S(m.into()));
        }
        if v.get("hero").and_then(Value::as_bool) == Some(false) {
            row.insert("vfog_hero".into(), V::N(0.0));
        }
        for (prop, col) in [("slices", "vfog_slices"), ("tile", "vfog_tile"), ("samples", "vfog_samples")] {
            if let Some(x) = num(&v, prop) {
                row.insert(col.into(), V::N(x));
            }
        }
        row.insert("vfog_ambient".into(), V::N(n(&v, "ambient", 0.0)));
        if let Some(c) = hex(v.get("ambientColor")) {
            row.insert("vfog_ambient_color".into(), V::S(c));
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
    lens(all, p, row);
    motion_blur(all, p, row);
}

/// The camera's lens (Fisheye or LensDistortion; engine/rdb/src/engine/lens.rs): its kind and edges from the first frame that
/// has one, every value keyed per frame. A frame without it is no lens (strength 0, no k, zoom 1), so a lens inside a
/// Sequence bends only its frames. A barrel's zoom, when not given, is worked out here (enough to keep its corners
/// inside the picture), so it's keyed like the rest.
fn lens(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>) {
    let per: Vec<Option<&Item>> = all.iter().map(|its| its.iter().find(|i| i.kind == "fisheye" || i.kind == "distortion")).collect();
    let Some(first) = per.iter().flatten().next() else { return };
    let id = "cam";
    let fisheye = first.kind == "fisheye";
    let mode = if !fisheye { 3.0 } else if first.props.get("mode").and_then(Value::as_str) == Some("stereographic") { 2.0 } else { 1.0 };
    row.insert("lens_mode".into(), V::N(mode));
    if first.props.get("edges").and_then(Value::as_str) == Some("black") {
        row.insert("lens_black".into(), V::N(1.0));
    }
    let same = |i: &&Item| (i.kind == "fisheye") == fisheye;
    let at = |k: &str, d: f64| -> Vec<f64> { per.iter().map(|i| i.filter(same).map(|i| n(&i.props, k, d)).unwrap_or(0.0)).collect() };
    if fisheye {
        column(p, row, id, "lens_s", &at("strength", 1.0));
        if per.iter().flatten().any(|i| i.props.contains_key("fov")) {
            let cams: Vec<f64> = all.iter().map(|its| its.iter().find(|i| i.kind == "camera").map(|c| n(&c.props, "fov", 50.0)).unwrap_or(50.0)).collect();
            let fovs: Vec<f64> = per.iter().zip(&cams).map(|(i, c)| i.filter(same).and_then(|i| num(&i.props, "fov")).unwrap_or(*c)).collect();
            column(p, row, id, "lens_fov", &fovs);
        }
    } else {
        column(p, row, id, "lens_k1", &at("k1", 0.0));
        column(p, row, id, "lens_k2", &at("k2", 0.0));
    }
    let zoom: Vec<f64> = per.iter().map(|i| match i.filter(same) {
        Some(i) => num(&i.props, "zoom").unwrap_or(if fisheye { 1.0 } else { (1.0 + n(&i.props, "k1", 0.0) + n(&i.props, "k2", 0.0)).max(1.0) }),
        None => 1.0,
    }).collect();
    column(p, row, id, "lens_zoom", &zoom);
}

/// The camera's motion blur (engine/rdb/src/engine/motion_blur.rs): mb_on keyed per frame (0 where a frame has none), its
/// shutter keyed (a frame without it holds the nearest frame's, which an off frame doesn't draw), its samples the first
/// frame's.
fn motion_blur(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>) {
    let per: Vec<Option<Item>> = all.iter().map(|its| its.iter().find(|i| i.kind == "mblur").cloned()).collect();
    if !per.iter().any(Option::is_some) {
        return;
    }
    let id = "cam";
    column(p, row, id, "mb_on", &per.iter().map(|i| i.is_some() as u8 as f64).collect::<Vec<_>>());
    let its = filled(&per);
    column(p, row, id, "mb_shutter", &its.iter().map(|i| n(&i.props, "shutterAngle", 0.5)).collect::<Vec<_>>());
    if let Some(s) = num(&its[0].props, "samples") {
        row.insert("mb_samples".into(), V::N(s));
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

/// A mesh's Bevy plugin components (engine/rdb/src/engine/plugins.rs): each type's value from the first frame it's there, and
/// again from each frame its value changes (or from the frame its `from` names), as `component` rows on the mesh's row.
fn components(p: &mut Program, id: &str, its: &[Item]) {
    let mut last: BTreeMap<String, String> = BTreeMap::new();
    let mut n = 0;
    for (f, it) in its.iter().enumerate() {
        for c in it.props.get("components").and_then(Value::as_array).into_iter().flatten() {
            let Some(ty) = c.get("type").and_then(Value::as_str) else { continue };
            let value = c.get("value").cloned().unwrap_or(Value::Null).to_string();
            if last.get(ty) == Some(&value) {
                continue;
            }
            last.insert(ty.to_string(), value.clone());
            let from = c.get("from").and_then(Value::as_f64).unwrap_or(f as f64);
            let mut row = BTreeMap::new();
            row.insert("id".into(), V::S(format!("{id}-c{n}")));
            row.insert("target".into(), V::S(id.to_string()));
            row.insert("type".into(), V::S(ty.to_string()));
            row.insert("value".into(), V::S(value));
            if from > 0.0 {
                row.insert("from".into(), V::N(from));
            }
            p.spawns.push(("component".into(), vec![row]));
            n += 1;
        }
    }
}

fn rows_into(frames: &[(i64, Frame)], p: &mut Program) {
    let all = all_items(frames);
    rows_into_of(&all, &Tracks::of(&all), p)
}

type Track = (String, Vec<Option<Item>>);

/// Each kind group's tracks, worked out once (a live frame worked them out twice: in rows_into and again for its ids)
struct Tracks {
    lights: Vec<Track>,
    meshes: Vec<Track>,
    models: Vec<Track>,
    panels: Vec<Track>,
    decals: Vec<Track>,
    fogs: Vec<Track>,
    fires: Vec<Track>,
    splats: Vec<Track>,
    plates: Vec<Track>,
    meshseqs: Vec<Track>,
}

impl Tracks {
    fn of(all: &[Vec<Item>]) -> Tracks {
        Tracks {
            lights: tracks_of(all, &["directional", "point", "spot"]),
            meshes: tracks_of(all, &["mesh"]),
            models: tracks_of(all, &["model"]),
            panels: tracks_of(all, &["panel"]),
            decals: tracks_of(all, &["decal"]),
            fogs: tracks_of(all, &["fogvolume"]),
            fires: tracks_of(all, &["fire"]),
            splats: tracks_of(all, &["splat"]),
            plates: tracks_of(all, &["plate"]),
            meshseqs: tracks_of(all, &["meshseq"]),
        }
    }
}

fn rows_into_of(all: &[Vec<Item>], tr: &Tracks, p: &mut Program) {
    // a compiled-in Bevy plugin's resources (<BevyResource>), from the first frame
    if let Some(f0) = all.first() {
        for (k, it) in f0.iter().filter(|it| it.kind == "resource").enumerate() {
            let mut row = BTreeMap::new();
            row.insert("id".into(), V::S(format!("res{k}")));
            row.insert("type".into(), V::S(it.props.get("type").and_then(Value::as_str).unwrap_or("").to_string()));
            row.insert("value".into(), V::S(it.props.get("value").cloned().unwrap_or(Value::Null).to_string()));
            p.spawns.push(("resource".into(), vec![row]));
        }
    }
    // lights: directional (towards its target, the origin by default), point, spot
    let mut li = 0;
    for (_, track) in &tr.lights {
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
        if first.get("volumetric") == Some(&Value::Bool(true)) {
            row.insert("volumetric".into(), V::N(1.0));
        }
        // soft shadows (PCSS): `true` for the default size (a sun's 0.5, a bulb's 0.25 m), or the size
        let soft = match first.get("softShadow") {
            Some(Value::Bool(true)) => Some(if kind == "directional" { 0.5 } else { 0.25 }),
            Some(Value::Number(x)) => x.as_f64(),
            _ => None,
        };
        if let Some(r) = soft.filter(|r| *r > 0.0) {
            row.insert("soft".into(), V::N(r));
        }
        // a <Shadows>' cascades (the sun's) and biases (every light's)
        if let Some(sh) = all.iter().flatten().find(|i| i.kind == "shadows") {
            for (prop, col) in [("distance", "shadow_dist"), ("firstCascade", "shadow_first"), ("cascades", "shadow_cascades"), ("overlap", "shadow_overlap"), ("bias", "shadow_bias"), ("normalBias", "shadow_nbias")] {
                if let Some(x) = num(&sh.props, prop) {
                    row.insert(col.into(), V::N(x));
                }
            }
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
    for (m, (_, track)) in tr.meshes.iter().enumerate() {
        let its = filled(&track);
        let Some(shape) = its[0].shape.clone() else { continue };
        let id = format!("mesh{m}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        row.insert("shape".into(), V::S(shape.to_string()));
        let mat = |i: &Item| i.material.as_ref().map(|m| m.props.clone()).unwrap_or_default();
        let mats: Vec<std::sync::Arc<Map<String, Value>>> = its.iter().map(mat).collect();
        let basic = its[0].material.as_ref().is_some_and(|m| &*m.kind == "MeshBasicMaterial");
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
        let m0 = &mats[0];
        // a roughness or metalness map is scaled by its factor, which is 1 unless given (three.js and glTF's rule)
        let has = |k: &str| m0.get(k).and_then(Value::as_str).is_some();
        let rough0 = if has("roughnessMap") || has("metallicRoughnessMap") { 1.0 } else { 0.5 };
        let metal0 = if has("metalnessMap") || has("metallicRoughnessMap") { 1.0 } else { 0.0 };
        column(p, &mut row, &id, "rough", &mats.iter().map(|m| n(m, "roughness", rough0)).collect::<Vec<_>>());
        column(p, &mut row, &id, "metal", &mats.iter().map(|m| n(m, "metalness", metal0)).collect::<Vec<_>>());
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
        for (prop, col) in [("map", "map"), ("normalMap", "normal_map"), ("roughnessMap", "rough_map"), ("metalnessMap", "metal_map"), ("metallicRoughnessMap", "mr_map"), ("aoMap", "ao_map")] {
            if let Some(src) = m0.get(prop).and_then(Value::as_str) {
                row.insert(col.into(), V::S(src_path(src)));
            }
        }
        if let Some(Value::Array(r)) = m0.get("repeat") {
            if let [u, v] = r.as_slice() {
                row.insert("repeat_u".into(), V::N(u.as_f64().unwrap_or(1.0)));
                row.insert("repeat_v".into(), V::N(v.as_f64().unwrap_or(1.0)));
            }
        }
        if its[0].material.as_ref().is_some_and(|m| &*m.kind == "MeshToonMaterial") {
            // a toon surface: its bands, rim, highlight and outline from the first frame; what animates, keyed
            row.insert("toon".into(), V::S(toon_spec(m0, false).to_string()));
            let shadows: Vec<Option<String>> = mats.iter().map(|m| hex(m.get("shadowColor"))).collect();
            // (always columns, so a live game's frames can change them as a render's keys do)
            if shadows[0].is_some() {
                let c: Vec<[f64; 3]> = shadows.iter().map(|c| rgb(c.as_deref().unwrap_or("#000000"))).collect();
                for (k, name) in ["sh_r", "sh_g", "sh_b"].iter().enumerate() {
                    column(p, &mut row, &id, name, &c.iter().map(|v| v[k]).collect::<Vec<_>>());
                }
            }
            for (col, part, flat, key, d) in [("rim_k", "rim", "rimIntensity", "intensity", 1.0), ("spec_k", "highlight", "highlightIntensity", "intensity", 1.0), ("outline_w", "outline", "outlineWidth", "width", 2.0)] {
                if m0.get(part).is_some_and(Value::is_object) {
                    let vals: Vec<f64> = mats.iter().map(|m| num(m, flat).or_else(|| m.get(part).and_then(Value::as_object).and_then(|x| num(x, key))).unwrap_or(d)).collect();
                    column(p, &mut row, &id, col, &vals);
                }
            }
        }
        grid_columns(&mut row, m0);
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, &track);
        if its[0].props.get("driven") == Some(&Value::Bool(true)) {
            row.insert("driven".into(), V::N(1.0));
        }
        if its[0].props.get("shadow") == Some(&Value::Bool(false)) {
            row.insert("shadow".into(), V::N(0.0));
        }
        p.spawns.push(("mesh".into(), vec![row]));
        components(p, &id, &its);
    }
    // glTF models: the file, its clip and the clip's time (keyed: the component works it out from the frame)
    for (k, (_, track)) in tr.models.iter().enumerate() {
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
        // attached to another model's bone: that model's row by its name
        if let Some(a) = first.get("attach") {
            let to = a.get("to").and_then(Value::as_str).unwrap_or("");
            if let Some(j) = tr.models.iter().position(|(_, t)| t.iter().flatten().next().and_then(|i| i.props.get("name")).and_then(Value::as_str) == Some(to)) {
                row.insert("attach_to".into(), V::S(format!("model{j}")));
                row.insert("attach_bone".into(), V::S(a.get("bone").and_then(Value::as_str).unwrap_or("").into()));
            } else {
                eprintln!("WARN  model {}: attach.to {to:?} names no model", first.get("name").and_then(Value::as_str).unwrap_or("?"));
            }
        }
        // a skin's joints, each placed where another model is: the joints' names and those models' rows, in turn
        if let Some(Value::Object(b)) = first.get("bones") {
            let (mut joints, mut rows) = (Vec::new(), Vec::new());
            for (joint, to) in b {
                let to = to.as_str().unwrap_or("");
                match tr.models.iter().position(|(_, t)| t.iter().flatten().next().and_then(|i| i.props.get("name")).and_then(Value::as_str) == Some(to)) {
                    Some(j) => {
                        joints.push(joint.clone());
                        rows.push(format!("model{j}"));
                    }
                    None => eprintln!("WARN  model {}: bones.{joint} names no model ({to:?})", first.get("name").and_then(Value::as_str).unwrap_or("?")),
                }
            }
            row.insert("bone_joints".into(), V::S(joints.join(",")));
            row.insert("bone_rows".into(), V::S(rows.join(",")));
        }
        if first.get("flat") == Some(&Value::Bool(true)) {
            row.insert("flat".into(), V::N(1.0));
        }
        if let Some(t) = first.get("toon") {
            row.insert("toon".into(), V::S(t.to_string()));
        }
        grid_columns(&mut row, first);
        // a look (rdb's look.rs): which one keyed by the frame, the rest the first frame's
        if its.iter().any(|i| i.props.contains_key("look")) {
            column(p, &mut row, &id, "look", &its.iter().map(|i| look_of(&i.props)).collect::<Vec<_>>());
            for (prop, col) in [("pixelSize", "look_px"), ("lift", "look_lift"), ("gain", "look_gain")] {
                if let Some(x) = num(first, prop) {
                    row.insert(col.into(), V::N(x));
                }
            }
            match first.get("edgeColor") {
                Some(Value::String(c)) => {
                    row.insert("look_edge".into(), V::S(if c == "none" { String::new() } else { hex_of(c).unwrap_or_default() }));
                }
                Some(Value::Null) | Some(Value::Bool(false)) => {
                    row.insert("look_edge".into(), V::S(String::new()));
                }
                _ => {}
            }
            if let Some(t) = first.get("theme").and_then(Value::as_str) {
                row.insert("look_theme".into(), V::S(t.to_string()));
            }
        }
        meshseq3d::glitch_columns(p, &mut row, &id, &its);
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, &track);
        p.spawns.push(("model".into(), vec![row]));
    }
    // pictures in space (rdb's panel: unlit, painted once, moved by the GPU)
    for (k, (_, track)) in tr.panels.iter().enumerate() {
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
    // decals: the picture and its surface from the first frame; placement, colour and opacity keyed (a stain that
    // spreads, a label that fades in)
    for (k, (_, track)) in tr.decals.iter().enumerate() {
        let its = filled(&track);
        let id = format!("decal{k}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        let first = &its[0].props;
        for (prop, col) in [("src", "map"), ("normalMap", "normal_map")] {
            if let Some(src) = first.get(prop).and_then(Value::as_str) {
                row.insert(col.into(), V::S(src_path(src)));
            }
        }
        let colors: Vec<[f64; 3]> = its.iter().map(|i| rgb(&hex(i.props.get("color")).unwrap_or("#ffffff".into()))).collect();
        for (c, name) in ["col_r", "col_g", "col_b"].iter().enumerate() {
            column(p, &mut row, &id, name, &colors.iter().map(|v| v[c]).collect::<Vec<_>>());
        }
        column(p, &mut row, &id, "opacity", &its.iter().map(|i| n(&i.props, "opacity", 1.0)).collect::<Vec<_>>());
        column(p, &mut row, &id, "rough", &its.iter().map(|i| n(&i.props, "roughness", 0.6)).collect::<Vec<_>>());
        column(p, &mut row, &id, "metal", &its.iter().map(|i| n(&i.props, "metalness", 0.0)).collect::<Vec<_>>());
        row.insert("fade".into(), V::N(n(first, "fade", 0.05)));
        if first.get("unlit") == Some(&Value::Bool(true)) {
            row.insert("unlit".into(), V::N(1.0));
        }
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, &track);
        p.spawns.push(("decal".into(), vec![row]));
    }
    // footage behind the 3D (rdb's plate: a video frame per frame on the camera's far plane, unlit): its source time
    // keyed, the clip and its lens's distortion the first frame's
    for (k, (_, track)) in tr.plates.iter().enumerate() {
        let its = filled(&track);
        let id = format!("plate{k}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        let first = &its[0].props;
        row.insert("src".into(), V::S(src_path(first.get("src").and_then(Value::as_str).unwrap_or(""))));
        column(p, &mut row, &id, "t", &its.iter().map(|i| n(&i.props, "time", 0.0)).collect::<Vec<_>>());
        if let Some(k1) = num(first, "k1").filter(|k| *k != 0.0) {
            row.insert("k1".into(), V::N(k1));
        }
        visible(p, &mut row, &id, &track);
        p.spawns.push(("plate".into(), vec![row]));
    }
    // fog volumes (a <VolumetricFog>'s boxes of air): placement, density and colour keyed, the rest the first frame's
    for (k, (_, track)) in tr.fogs.iter().enumerate() {
        let its = filled(&track);
        let id = format!("fog{k}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        column(p, &mut row, &id, "density", &its.iter().map(|i| n(&i.props, "density", 0.1)).collect::<Vec<_>>());
        let colors: Vec<String> = its.iter().map(|i| hex(i.props.get("color")).unwrap_or("#ffffff".into())).collect();
        row.insert("color".into(), V::S(colors[0].clone()));
        if colors.iter().any(|c| *c != colors[0]) {
            let c: Vec<[f64; 3]> = colors.iter().map(|c| rgb(c)).collect();
            for (k, name) in ["col_r", "col_g", "col_b"].iter().enumerate() {
                p.keys.insert((id.clone(), name.to_string()), c.iter().enumerate().map(|(f, v)| (f as i64, r6(v[k]), "linear".to_string())).collect());
                row.insert(name.to_string(), V::N(r6(c[0][k])));
            }
        }
        let first = &its[0].props;
        for (prop, col) in [("absorption", "absorption"), ("scattering", "scattering"), ("anisotropy", "anisotropy"), ("falloff", "falloff")] {
            if let Some(x) = num(first, prop) {
                row.insert(col.into(), V::N(x));
            }
        }
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, &track);
        p.spawns.push(("fogvolume".into(), vec![row]));
    }
    // fires (<Fire>: a flame drawn as a volume): its foot, its box's size, its clock, brightness and colour keyed
    for (k, (_, track)) in tr.fires.iter().enumerate() {
        let its = filled(&track);
        let id = format!("fire{k}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        for (prop, d) in [("time", 0.0), ("intensity", 6.0)] {
            column(p, &mut row, &id, prop, &its.iter().map(|i| n(&i.props, prop, d)).collect::<Vec<_>>());
        }
        let colors: Vec<String> = its.iter().map(|i| hex(i.props.get("color")).unwrap_or("#ff7a1f".into())).collect();
        row.insert("color".into(), V::S(colors[0].clone()));
        if colors.iter().any(|c| *c != colors[0]) {
            let c: Vec<[f64; 3]> = colors.iter().map(|c| rgb(c)).collect();
            for (k, name) in ["col_r", "col_g", "col_b"].iter().enumerate() {
                column(p, &mut row, &id, name, &c.iter().map(|v| v[k]).collect::<Vec<_>>());
            }
        }
        let first = &its[0].props;
        for prop in ["seed", "speed", "steps", "opacity"] {
            if let Some(x) = num(first, prop) {
                row.insert(prop.into(), V::N(x));
            }
        }
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, &track);
        p.spawns.push(("fire".into(), vec![row]));
    }
    // gaussian splats (<Splat>): splat3d.rs
    splat3d::rows(&tr.splats, p);
    // baked simulations (<MeshSequence>, <Goo>): meshseq3d.rs
    meshseq3d::rows(&tr.meshseqs, p);
}

/// A live game's 3D (live.rs): its rows made frame by frame from that frame's own scene, as play computed it, instead
/// of from a scripted run stepped ahead (scene_program). Parts are matched across frames by the identity `tracks`
/// gives them (a name, else kind and shape counted in tree order), so a part keeps its row as a render's does. The
/// columns a render takes from a part's first frame (shape, surface, light colour, a model's file) are taken from the
/// frame it first shows here; the columns a render keys come from each frame. A part missing from a frame is hidden (a
/// light goes dark), as a render's `visible` column hides it. Each frame only what changed goes to the engine.
#[derive(Default, Clone)]
pub(crate) struct LiveScene {
    /// identity (as `tracks` keys it) -> row id
    ids: std::collections::HashMap<String, String>,
    /// per row id: its kind, its first values, what was last sent, whether its colour has changed (a render keys
    /// col_r/g/b on every frame once it does), and the frames it has been missing
    rows: std::collections::HashMap<String, LiveRow>,
    /// rows made per group (lights, meshes, models, panels, decals): the next one's number
    made: [usize; 5],
    cam: BTreeMap<String, V>,
    /// frame 0 was stepped when play opened (live.rs), and nothing since
    pub(crate) warm: bool,
}

#[derive(Clone)]
struct LiveRow {
    kind: String,
    first: BTreeMap<String, V>,
    sent: BTreeMap<String, V>,
    recoloured: bool,
    missing: u32,
    /// seen this frame (frame_fast's mark)
    seen: bool,
}

/// A row missing this many frames is dropped from the engine (its entity, mesh and surface freed) and forgotten: a game
/// that spawns entities with fresh names (`rock-${id}`) would otherwise keep every one it ever made, hidden. Coming back
/// later, it's a new row with the same values, so nothing drawn changes.
const LIVE_FORGET: u32 = 240;

/// The columns a frame may change, per kind (what `rows_into` and `camera` key); everything else is the first frame's.
fn live_dynamic(kind: &str) -> &'static [&'static str] {
    match kind {
        "light" => &["lux", "lumens", "x", "y", "z", "dir_x", "dir_y", "dir_z"],
        "mesh" => &["x", "y", "z", "rot_x", "rot_y", "rot_z", "scale_x", "scale_y", "scale_z", "visible", "col_r", "col_g", "col_b", "rough", "metal", "emissive_k", "opacity", "sh_r", "sh_g", "sh_b", "rim_k", "spec_k", "outline_w"],
        "model" => &["x", "y", "z", "rot_x", "rot_y", "rot_z", "scale_x", "scale_y", "scale_z", "visible", "anim_t", "look", "glitch_explode", "glitch_scramble", "glitch_jitter", "glitch_snap", "glitch_smear_x", "glitch_smear_y", "glitch_smear_z", "glitch_seed"],
        "panel" => &["x", "y", "z", "rot_x", "rot_y", "rot_z", "scale_x", "scale_y", "scale_z", "visible"],
        "decal" => &["x", "y", "z", "rot_x", "rot_y", "rot_z", "scale_x", "scale_y", "scale_z", "visible", "col_r", "col_g", "col_b", "opacity", "rough", "metal"],
        _ => &[],
    }
}

impl LiveScene {
    /// Start again (the engine was dropped: a new size).
    pub(crate) fn reset(&mut self) {
        *self = LiveScene::default();
    }

    /// `diff(current(f))` for a frame whose parts are all known, without the rows: each known part's changing values worked out into a fixed array and held
    /// against what was sent in place, as diff holds a row (the same rules: recoloured meshes keyed, the emissive strength
    /// only where the first frame glows, opacity once seen-through, visible and scale back to 1, lights without visible),
    /// so a frame allocates only for what it changes. None, with nothing changed, when a part is new.
    fn frame_fast(&mut self, f: &(i64, Frame)) -> Option<(Vec<rdb::eval::Tup>, Vec<(String, String)>)> {
        if self.rows.is_empty() {
            return None;
        }
        // a lens, motion blur or a toon surface's keyed values: the full path, which keys them (camera_of, rows_into_of)
        if self.cam.keys().any(|k| k.starts_with("lens_") || k.starts_with("mb_")) || self.rows.values().any(|r| ["sh_r", "rim_k", "spec_k", "outline_w"].iter().any(|k| r.first.contains_key(*k))) {
            return None;
        }
        let its = items(&f.1);
        // pass 1: every part's id (as tracks_of names it on one frame), or None before anything changes
        let mut parts: Vec<(usize, &String)> = Vec::with_capacity(its.len());
        // each group's (kind, name) counts and the key, borrowed and written in place (ZeroHeap)
        let mut seen: Vec<(&str, &str, usize)> = Vec::new();
        let mut key = String::new();
        for kinds in [&["directional", "point", "spot"][..], &["mesh"][..], &["model"][..], &["panel"][..], &["decal"][..]] {
            seen.clear();
            for (i, it) in its.iter().enumerate().filter(|(_, it)| kinds.contains(&it.kind)) {
                let base = base_ident(it);
                let k = match seen.iter_mut().find(|s| s.0 == it.kind && s.1 == base) {
                    Some(s) => {
                        s.2 += 1;
                        s.2 - 1
                    }
                    None => {
                        seen.push((it.kind, base, 1));
                        0
                    }
                };
                key.clear();
                {
                    use std::fmt::Write as _;
                    let _ = write!(key, "{}|{base}#{k}", it.kind);
                }
                let id = self.ids.get(key.as_str())?;
                if it.kind == "mesh" && it.shape.is_none() {
                    continue;
                }
                if !self.rows.contains_key(id) {
                    return None;
                }
                parts.push((i, id));
            }
        }
        let mut out: Vec<rdb::eval::Tup> = Vec::new();
        // the camera, as diff holds it
        let mut cam: [(&str, Option<f64>); 9] = [("x", None), ("y", None), ("z", None), ("look_x", None), ("look_y", None), ("look_z", None), ("fov", None), ("ambient", None), ("focus", None)];
        match its.iter().find(|i| i.kind == "camera") {
            Some(c) => {
                let pos = vec3(&c.props, "position", [0.0, 0.0, 5.0]);
                let look = vec3(&c.props, "lookAt", [0.0, 0.0, 0.0]);
                for k in 0..3 {
                    cam[k].1 = Some(r6(pos[k]));
                    cam[3 + k].1 = Some(r6(look[k]));
                }
                cam[6].1 = Some(r6(n(&c.props, "fov", 50.0)));
                if self.cam.contains_key("focus") {
                    cam[8].1 = Some(r6(focus_of(c)));
                }
            }
            None => {
                for (k, d) in [0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 50.0].into_iter().enumerate() {
                    cam[k].1 = Some(match self.cam.get(cam[k].0) {
                        Some(V::N(v)) => *v,
                        _ => d,
                    });
                }
            }
        }
        let amb = its.iter().filter(|i| i.kind == "ambient").map(|i| n(&i.props, "intensity", 1.0)).sum::<f64>() * AMBIENT;
        cam[7].1 = if amb > 0.0 { Some(r6(amb)) } else { self.cam.contains_key("ambient").then_some(0.0) };
        let mut vals = BTreeMap::new();
        for (k, v) in cam {
            if let Some(v) = v.map(V::N) {
                if self.cam.get(k) != Some(&v) {
                    self.cam.insert(k.into(), v.clone());
                    vals.insert(k.to_string(), v);
                }
            }
        }
        // the paint look's keyed values, as camera_of keys them
        if self.cam.contains_key("paint_bands") {
            if let Some(l) = its.iter().find(|i| i.kind == "framelook") {
                for (k, v) in paint_keyed(&l.props) {
                    let v = V::N(r6(v));
                    if self.cam.get(k) != Some(&v) {
                        self.cam.insert(k.into(), v.clone());
                        vals.insert(k.to_string(), v);
                    }
                }
            }
        }
        if !vals.is_empty() {
            out.push(rdb::eval::Tup { kind: "camera".into(), id: "cam".into(), vals });
        }
        for r in self.rows.values_mut() {
            r.seen = false;
        }
        // pass 2: each part's values, then held against what was sent
        for (i, id) in parts {
            let it = &its[i];
            let mut want: [(&str, Option<f64>); 24] = [("", None); 24];
            let mut nw = 0usize;
            let mut put = |k: &'static str, v: f64| {
                want[nw] = (k, Some(r6(v)));
                nw += 1;
            };
            let mut color: Option<std::borrow::Cow<str>> = None;
            let place = |put: &mut dyn FnMut(&'static str, f64)| {
                let (s, q, t) = it.world.to_scale_rotation_translation();
                let (ry, rx, rz) = q.to_euler(EulerRot::YXZ);
                for (name, v) in [("x", t.x), ("y", t.y), ("z", t.z), ("rot_x", rx), ("rot_y", ry), ("rot_z", rz)] {
                    put(name, v as f64);
                }
                let sc = [s.x as f64, s.y as f64, s.z as f64];
                if sc.iter().any(|v| (v - 1.0).abs() > 1e-9) {
                    for (name, v) in [("scale_x", sc[0]), ("scale_y", sc[1]), ("scale_z", sc[2])] {
                        put(name, v);
                    }
                }
            };
            let kind: &'static str = match it.kind {
                "directional" | "point" | "spot" => {
                    let dir = it.kind == "directional";
                    put(if dir { "lux" } else { "lumens" }, n(&it.props, "intensity", 1.0) * if dir { LUX } else { LUMENS } * 1.0);
                    let v = vec3(&it.props, "position", if dir { [0.0, 1.0, 0.0] } else { [0.0; 3] });
                    let w = it.world.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
                    let pos = [w.x as f64, w.y as f64, w.z as f64];
                    let target = vec3(&it.props, "target", [0.0; 3]);
                    if !dir {
                        put("x", pos[0]);
                        put("y", pos[1]);
                        put("z", pos[2]);
                    }
                    if it.kind != "point" {
                        put("dir_x", target[0] - pos[0]);
                        put("dir_y", target[1] - pos[1]);
                        put("dir_z", target[2] - pos[2]);
                    }
                    "light"
                }
                "mesh" => {
                    static EMPTY: std::sync::LazyLock<Map<String, Value>> = std::sync::LazyLock::new(Map::new);
                    let m: &Map<String, Value> = it.material.as_ref().map(|m| &*m.props).unwrap_or(&EMPTY);
                    color = Some(m.get("color").and_then(Value::as_str).and_then(super::hex_cow).unwrap_or(std::borrow::Cow::Borrowed("#cccccc")));
                    put("rough", n(m, "roughness", 0.5));
                    put("metal", n(m, "metalness", 0.0));
                    if m.get("emissive").and_then(Value::as_str).and_then(super::hex_cow).is_some_and(|e| e != "#000000") {
                        put("emissive_k", n(m, "emissiveIntensity", 1.0));
                    }
                    if m.get("transparent") == Some(&Value::Bool(true)) || num(m, "opacity").is_some_and(|o| o < 1.0) {
                        put("opacity", n(m, "opacity", 1.0));
                    }
                    place(&mut put);
                    "mesh"
                }
                "model" => {
                    if it.props.contains_key("animation") {
                        put("anim_t", n(&it.props, "time", 0.0));
                    }
                    if it.props.contains_key("look") {
                        put("look", look_of(&it.props));
                    }
                    place(&mut put);
                    "model"
                }
                "decal" => {
                    let c = rgb(&it.props.get("color").and_then(Value::as_str).and_then(super::hex_cow).unwrap_or(std::borrow::Cow::Borrowed("#ffffff")));
                    for (k, name) in ["col_r", "col_g", "col_b"].into_iter().enumerate() {
                        put(name, c[k]);
                    }
                    put("opacity", n(&it.props, "opacity", 1.0));
                    put("rough", n(&it.props, "roughness", 0.6));
                    put("metal", n(&it.props, "metalness", 0.0));
                    place(&mut put);
                    "decal"
                }
                _ => {
                    place(&mut put);
                    "panel"
                }
            };
            let r = self.rows.get_mut(id).expect("checked in pass 1");
            r.seen = true;
            r.missing = 0;
            let set = |want: &mut [(&str, Option<f64>); 24], nw: &mut usize, k: &'static str, v: Option<f64>, only_if_absent: bool| {
                match want[..*nw].iter_mut().find(|w| w.0 == k) {
                    Some(w) => {
                        if !only_if_absent {
                            w.1 = v;
                        }
                    }
                    None => {
                        want[*nw] = (k, v);
                        *nw += 1;
                    }
                }
            };
            if kind == "mesh" {
                let c = color.as_deref().unwrap_or("#cccccc");
                if !matches!(r.first.get("color"), Some(V::S(s)) if s == c) {
                    r.recoloured = true;
                }
                if r.recoloured {
                    let c = rgb(c);
                    for (k, name) in ["col_r", "col_g", "col_b"].into_iter().enumerate() {
                        set(&mut want, &mut nw, name, Some(r6(c[k])), false);
                    }
                }
                if !r.first.contains_key("emissive") {
                    set(&mut want, &mut nw, "emissive_k", None, false);
                } else {
                    set(&mut want, &mut nw, "emissive_k", Some(1.0), true);
                }
                if r.sent.contains_key("opacity") {
                    set(&mut want, &mut nw, "opacity", Some(1.0), true);
                }
            }
            if r.sent.contains_key("visible") || kind != "light" {
                set(&mut want, &mut nw, "visible", Some(1.0), true);
            }
            for k in ["scale_x", "scale_y", "scale_z"] {
                if r.sent.contains_key(k) {
                    set(&mut want, &mut nw, k, Some(1.0), true);
                }
            }
            if kind == "light" {
                set(&mut want, &mut nw, "visible", None, false);
            }
            let mut changed: BTreeMap<String, V> = BTreeMap::new();
            for (k, v) in &want[..nw] {
                let Some(v) = v.map(V::N) else { continue };
                if r.sent.get(*k) != Some(&v) {
                    match r.sent.get_mut(*k) {
                        Some(slot) => *slot = v.clone(),
                        None => {
                            r.sent.insert(k.to_string(), v.clone());
                        }
                    }
                    changed.insert(k.to_string(), v);
                }
            }
            if !changed.is_empty() {
                out.push(rdb::eval::Tup { kind: kind.to_string(), id: id.clone(), vals: changed });
            }
        }
        // missing this frame: hidden (a light dark); dropped once it's been gone long enough
        let mut gone = Vec::new();
        for (id, r) in self.rows.iter_mut() {
            if r.seen {
                continue;
            }
            r.missing += 1;
            if r.missing > LIVE_FORGET {
                gone.push((r.kind.clone(), id.clone()));
                continue;
            }
            let (k, v) = match r.kind.as_str() {
                "light" => (if r.first.contains_key("lux") { "lux" } else { "lumens" }, V::N(0.0)),
                _ => ("visible", V::N(0.0)),
            };
            if r.sent.get(k) != Some(&v) {
                r.sent.insert(k.to_string(), v.clone());
                let mut vals = BTreeMap::new();
                vals.insert(k.to_string(), v);
                out.push(rdb::eval::Tup { kind: r.kind.clone(), id: id.clone(), vals });
            }
        }
        for (_, id) in &gone {
            self.rows.remove(id);
            self.ids.retain(|_, v| v != id);
        }
        Some((out, gone))
    }

    /// The frame's rows as a render's program would hold them at this frame: the camera row and each part's full row,
    /// by identity, with the row ids this scene keeps.
    fn current(&mut self, f: &(i64, Frame)) -> (BTreeMap<String, V>, Vec<(String, String, BTreeMap<String, V>)>) {
        let one = std::slice::from_ref(f);
        let all = all_items(one);
        let mut p = Program::default();
        let mut cam = BTreeMap::new();
        cam.insert("id".into(), V::S("cam".into()));
        cam.insert("bg".into(), V::S(f.1.clear.as_deref().and_then(hex_of).unwrap_or("#000000".into())));
        camera_of(&all, &mut p, &mut cam);
        if !all[0].iter().any(|i| i.kind == "camera") {
            // no camera this frame: a render holds the one before (filled)
            for k in ["x", "y", "z", "look_x", "look_y", "look_z", "fov", "focus"] {
                match self.cam.get(k) {
                    Some(v) => {
                        cam.insert(k.into(), v.clone());
                    }
                    None => {}
                }
            }
        }
        let tr = Tracks::of(&all);
        rows_into_of(&all, &tr, &mut p);
        // each local row (this frame's tracks, numbered as rows_into numbers them) by its identity
        let mut local: std::collections::HashMap<String, &String> = std::collections::HashMap::new();
        for (li, (key, track)) in tr.lights.iter().enumerate() {
            let kind = track[0].as_ref().map(|i| i.kind).unwrap_or("directional");
            local.insert(if kind == "directional" { format!("sun{li}") } else { format!("{kind}{li}") }, key);
        }
        for (prefix, group) in [("mesh", &tr.meshes), ("model", &tr.models), ("plane", &tr.panels), ("decal", &tr.decals)] {
            for (k, (key, _)) in group.iter().enumerate() {
                local.insert(format!("{prefix}{k}"), key);
            }
        }
        let mut out = Vec::new();
        for (kind, rows) in p.spawns {
            let group = match kind.as_str() {
                "light" => 0,
                "mesh" => 1,
                "model" => 2,
                "panel" => 3,
                "decal" => 4,
                _ => continue,
            };
            for mut row in rows {
                let Some(V::S(lid)) = row.get("id").cloned() else { continue };
                let Some(key) = local.get(&lid) else { continue };
                let id = match self.ids.get(key.as_str()) {
                    Some(id) => id.clone(),
                    None => {
                        let prefix: String = lid.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
                        let id = format!("{prefix}{}", self.made[group]);
                        self.made[group] += 1;
                        self.ids.insert((*key).clone(), id.clone());
                        id
                    }
                };
                row.insert("id".into(), V::S(id.clone()));
                out.push((kind.clone(), id, row));
            }
        }
        (cam, out)
    }

    /// The program the live engine opens with: this frame's camera and parts (a render's rows at this frame).
    pub(super) fn open(&mut self, f: &(i64, Frame), w: u32, h: u32, fps: f64) -> Program {
        self.reset();
        let (cam, rows) = self.current(f);
        let mut p = Program { fps, width: w, height: h, frames: 1, ..Default::default() };
        self.cam = cam.clone();
        p.spawns.push(("camera".into(), vec![cam]));
        for (kind, id, row) in rows {
            let first = row.clone();
            self.rows.insert(id, LiveRow { kind: kind.clone(), first, sent: row.clone(), recoloured: false, missing: 0, seen: false });
            p.spawns.push((kind, vec![row]));
        }
        p
    }

    /// What this frame changes: the rows to set (a new part's whole row; for one seen before, only the columns whose
    /// values changed) and the rows to drop (missing LIVE_FORGET frames).
    pub(super) fn frame(&mut self, f: &(i64, Frame)) -> (Vec<rdb::eval::Tup>, Vec<(String, String)>) {
        // RAEA_LIVE3D_CHECK=1: each frame also diffed from a render's full rows on a copy, and the two must agree
        let check = std::env::var_os("RAEA_LIVE3D_CHECK").is_some();
        let before = check.then(|| self.clone());
        let fast = std::env::var("RAEA_LIVE3D_FAST").as_deref() != Ok("0");
        let (out, used_fast) = match fast.then(|| self.frame_fast(f)).flatten() {
            Some(out) => (out, true),
            None => {
                let (cam, rows) = self.current(f);
                (self.diff(cam, rows), false)
            }
        };
        if let (Some(mut slow), true) = (before, used_fast) {
            let (cam, rows) = slow.current(f);
            let want = slow.diff(cam, rows);
            assert!(want == out, "RAEA_LIVE3D_CHECK frame {}: fast rows {:?} != full rows {:?}", f.0, out, want);
            assert!(slow.cam == self.cam, "RAEA_LIVE3D_CHECK frame {}: camera {:?} != {:?}", f.0, self.cam, slow.cam);
            assert!(slow.rows.len() == self.rows.len(), "RAEA_LIVE3D_CHECK frame {}: {} rows kept, {} on the full path", f.0, self.rows.len(), slow.rows.len());
            for (id, r) in &slow.rows {
                assert!(self.rows.get(id).is_some_and(|m| m.sent == r.sent && m.recoloured == r.recoloured && m.missing == r.missing), "RAEA_LIVE3D_CHECK frame {}: row {id}", f.0);
            }
        }
        out
    }

    /// What this frame changes, from its camera row and parts' rows (`current`'s)
    fn diff(&mut self, cam: BTreeMap<String, V>, rows: Vec<(String, String, BTreeMap<String, V>)>) -> (Vec<rdb::eval::Tup>, Vec<(String, String)>) {
        let mut out: Vec<rdb::eval::Tup> = Vec::new();
        // the camera: where it is and looks, its lens and the ambient light (0 once there's none)
        let mut vals = BTreeMap::new();
        // and the lens's and motion blur's, whatever of theirs the frame keys
        let keyed: Vec<String> = cam.keys().filter(|k| k.starts_with("lens_") || k.starts_with("mb_")).cloned().collect();
        for k in ["x", "y", "z", "look_x", "look_y", "look_z", "fov", "ambient", "focus", "paint_bands", "paint_grain", "paint_rag", "paint_seed"].into_iter().chain(keyed.iter().map(String::as_str)) {
            let v = cam.get(k).cloned().or_else(|| (k == "ambient" && self.cam.contains_key("ambient")).then_some(V::N(0.0)));
            if let Some(v) = v {
                if self.cam.get(k) != Some(&v) {
                    self.cam.insert(k.into(), v.clone());
                    vals.insert(k.to_string(), v);
                }
            }
        }
        if !vals.is_empty() {
            out.push(rdb::eval::Tup { kind: "camera".into(), id: "cam".into(), vals });
        }
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (kind, id, row) in rows {
            seen.insert(id.clone());
            let Some(r) = self.rows.get_mut(&id) else {
                // first seen: its whole row, spawned with these values
                self.rows.insert(id.clone(), LiveRow { kind: kind.clone(), first: row.clone(), sent: row.clone(), recoloured: false, missing: 0, seen: false });
                out.push(rdb::eval::Tup { kind, id, vals: row.into_iter().filter(|(k, _)| k != "id").collect() });
                continue;
            };
            r.missing = 0;
            let mut want: BTreeMap<String, V> = BTreeMap::new();
            for k in live_dynamic(&kind) {
                if let Some(v) = row.get(*k) {
                    want.insert(k.to_string(), v.clone());
                }
            }
            if kind == "mesh" {
                // a colour other than its first: keyed from now on, as a render keys it once a part's colour changes
                if row.get("color") != r.first.get("color") {
                    r.recoloured = true;
                }
                if r.recoloured {
                    if let Some(V::S(c)) = row.get("color") {
                        let c = rgb(c);
                        for (k, name) in ["col_r", "col_g", "col_b"].iter().enumerate() {
                            want.insert(name.to_string(), V::N(r6(c[k])));
                        }
                    }
                }
                // the emissive strength only where the first frame glows (a render's column), opacity once it's been
                // see-through, scale back to 1: what a render's keyed column says on a frame this one leaves it out
                if !r.first.contains_key("emissive") {
                    want.remove("emissive_k");
                } else {
                    want.entry("emissive_k".into()).or_insert(V::N(1.0));
                }
                if r.sent.contains_key("opacity") {
                    want.entry("opacity".into()).or_insert(V::N(1.0));
                }
            }
            if r.sent.contains_key("visible") || kind != "light" {
                want.entry("visible".into()).or_insert(V::N(1.0));
            }
            for k in ["scale_x", "scale_y", "scale_z"] {
                if r.sent.contains_key(k) {
                    want.entry(k.into()).or_insert(V::N(1.0));
                }
            }
            if kind == "light" {
                want.retain(|k, _| k != "visible");
            }
            let changed: BTreeMap<String, V> = want.into_iter().filter(|(k, v)| r.sent.get(k) != Some(v)).collect();
            if !changed.is_empty() {
                for (k, v) in &changed {
                    r.sent.insert(k.clone(), v.clone());
                }
                out.push(rdb::eval::Tup { kind, id, vals: changed });
            }
        }
        // missing this frame: hidden (a light dark); dropped once it's been gone long enough
        let mut gone = Vec::new();
        for (id, r) in self.rows.iter_mut() {
            if seen.contains(id) {
                continue;
            }
            r.missing += 1;
            if r.missing > LIVE_FORGET {
                gone.push((r.kind.clone(), id.clone()));
                continue;
            }
            let off: Vec<(&str, V)> = match r.kind.as_str() {
                "light" => vec![(if r.first.contains_key("lux") { "lux" } else { "lumens" }, V::N(0.0))],
                _ => vec![("visible", V::N(0.0))],
            };
            let mut vals = BTreeMap::new();
            for (k, v) in off {
                if r.sent.get(k) != Some(&v) {
                    r.sent.insert(k.to_string(), v.clone());
                    vals.insert(k.to_string(), v);
                }
            }
            if !vals.is_empty() {
                out.push(rdb::eval::Tup { kind: r.kind.clone(), id: id.clone(), vals });
            }
        }
        for (_, id) in &gone {
            self.rows.remove(id);
            self.ids.retain(|_, v| v != id);
        }
        (out, gone)
    }
}

/** A `grid` prop (a material's or a model's) as the row's columns (rdb's grid.rs): the first frame's. */
fn grid_columns(row: &mut BTreeMap<String, V>, props: &Map<String, Value>) {
    let Some(g) = props.get("grid").and_then(Value::as_object) else { return };
    let Some(cell) = g.get("cell").and_then(Value::as_f64).filter(|c| *c > 0.0) else { return };
    row.insert("grid_cell".into(), V::N(cell));
    if let Some(m) = g.get("major").and_then(Value::as_f64) {
        row.insert("grid_major".into(), V::N(m));
    }
    for (k, col) in [("color", "grid_color"), ("edge", "grid_edge")] {
        if let Some(c) = g.get(k).and_then(Value::as_str) {
            row.insert(col.into(), V::S(c.into()));
        }
    }
    if g.get("toon") == Some(&Value::Bool(true)) {
        row.insert("grid_toon".into(), V::N(1.0));
    }
}
