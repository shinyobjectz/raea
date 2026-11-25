//! `raea scene export EDIT.tsx -o DIR [--composition ID] [--frame N | --at A-B]` (docs/blender.feature BL37): the
//! composition's 3D at a frame, or on every frame of a span, as DIR/scene.json, with each mesh that isn't a primitive in
//! DIR/meshes/ and each model's file in DIR/models/. Every part is placed in the world (y up, metres; a quaternion
//! x y z w), so Blender (sdk/blender/src/scene.ts) and Godot (godot.rs) each build it with their own parts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use raea_core_scene::{Item, hex, light_power, n, num, tracks_of, vec3};
use serde_json::{Value, json};

use crate::R;

pub const USAGE: &str = "raea scene export EDIT.tsx -o DIR [--composition ID] [--frame N | --at A-B]";

/// The frames asked for: --frame N, every frame of --at A-B, else 0.
fn frames_of(args: &[String]) -> R<Vec<i64>> {
    let fl = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1));
    if let Some(span) = fl("--at") {
        let (a, b) = span.split_once('-').ok_or("--at takes A-B, the first and last frame")?;
        let (a, b): (i64, i64) = (a.trim().parse().map_err(|_| "--at A-B: A is a frame")?, b.trim().parse().map_err(|_| "--at A-B: B is a frame")?);
        if b < a || b - a > 10_000 {
            return Err("--at A-B: B at or after A, at most 10 000 frames".into());
        }
        return Ok((a..=b).collect());
    }
    Ok(vec![fl("--frame").map(|f| f.parse().map_err(|_| "--frame takes a frame number")).transpose()?.unwrap_or(0)])
}

fn r6(x: f32) -> f64 {
    (x as f64 * 1e6).round() / 1e6
}

/// A rotation turning -Z (the way a camera and a light face, in glTF, Blender and Godot alike) to `dir`, +Y towards
/// `up`: x y z w.
pub fn look_quat(dir: [f64; 3], up: [f64; 3]) -> [f64; 4] {
    let norm = |v: [f64; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l < 1e-12 { None } else { Some([v[0] / l, v[1] / l, v[2] / l]) }
    };
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let Some(f) = norm(dir) else { return [0.0, 0.0, 0.0, 1.0] };
    let z = [-f[0], -f[1], -f[2]];
    let x = norm(cross(up, z)).or_else(|| norm(cross([0.0, 0.0, 1.0], z))).unwrap_or([1.0, 0.0, 0.0]);
    let y = cross(z, x);
    // the rotation matrix's columns x, y, z as a quaternion
    let (m00, m11, m22) = (x[0], y[1], z[2]);
    let t = m00 + m11 + m22;
    let q = if t > 0.0 {
        let s = (t + 1.0).sqrt() * 2.0;
        [(y[2] - z[1]) / s, (z[0] - x[2]) / s, (x[1] - y[0]) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (y[0] + x[1]) / s, (z[0] + x[2]) / s, (y[2] - z[1]) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(y[0] + x[1]) / s, 0.25 * s, (z[1] + y[2]) / s, (z[0] - x[2]) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(z[0] + x[2]) / s, (z[1] + y[2]) / s, 0.25 * s, (x[1] - y[0]) / s]
    };
    q.map(|v| (v * 1e6).round() / 1e6)
}

/// A mesh or model's placement: position, rotation (x y z w), scale.
fn placed(it: &Item) -> Value {
    let (s, q, t) = it.world.to_scale_rotation_translation();
    let q = if q.is_finite() { [q.x, q.y, q.z, q.w] } else { [0.0, 0.0, 0.0, 1.0] };
    json!([r6(t.x), r6(t.y), r6(t.z), r6(q[0]), r6(q[1]), r6(q[2]), r6(q[3]), r6(s.x), r6(s.y), r6(s.z)])
}

/// A light's placement: where it is, turned to face its target (a sun's from its position to its target too).
fn light_placed(it: &Item) -> Value {
    let p = vec3(&it.props, "position", if it.kind == "directional" { [0.0, 1.0, 0.0] } else { [0.0; 3] });
    let w = it.world.transform_point3([p[0] as f32, p[1] as f32, p[2] as f32].into());
    let t = vec3(&it.props, "target", [0.0; 3]);
    let w = [w.x as f64, w.y as f64, w.z as f64];
    let q = look_quat([t[0] - w[0], t[1] - w[1], t[2] - w[2]], [0.0, 1.0, 0.0]);
    let r = |x: f64| (x * 1e6).round() / 1e6;
    json!([r(w[0]), r(w[1]), r(w[2]), q[0], q[1], q[2], q[3], 1, 1, 1])
}

/// What a mesh is made of, as the parts another tool has: a primitive with its sizes, else its Manifold mesh as a .glb.
fn shape(expr: &str, out: &Path, meshes: &mut BTreeMap<String, String>) -> R<Value> {
    if let Some(s) = raea_core_shape::cuboid_of(expr) {
        return Ok(json!({ "type": "cuboid", "size": s }));
    }
    if let Some(raea_core_shape::SphereOf::Ico { radius, .. } | raea_core_shape::SphereOf::Uv { radius, .. }) = raea_core_shape::sphere_of(expr) {
        return Ok(json!({ "type": "sphere", "radius": radius }));
    }
    if let Some((radius, height)) = raea_core_shape::cylinder_of(expr) {
        return Ok(json!({ "type": "cylinder", "radius": radius, "height": height }));
    }
    if let Some((_, size)) = raea_core_shape::plane_of(expr).filter(|(n, _)| *n == [0.0, 1.0, 0.0]) {
        return Ok(json!({ "type": "plane", "size": size }));
    }
    let k = meshes.len();
    let file = match meshes.get(expr) {
        Some(f) => f.clone(),
        None => {
            let f = format!("meshes/mesh{k}.glb");
            // export() empties meshes/ before the parts are walked, so the folder is made here, where a file first needs it
            let dir = out.join("meshes");
            if let Err(e) = std::fs::create_dir_all(&dir) {
                return Err(format!("{}: {e}", dir.display()));
            }
            // and as an .obj, which Godot imports as a mesh of its own (a .glb comes in as a scene)
            let (glb, obj) = (out.join(&f).to_string_lossy().into_owned(), out.join(f.replace(".glb", ".obj")).to_string_lossy().into_owned());
            match expr.strip_prefix("buffer:") {
                // a mesh given by its own vertices (a <BufferMesh>, a three.js geometry through raea/r3f): written as it
                // is, with its normals and texture coordinates, since it needn't be a closed solid
                Some(b) => {
                    let (t, uv) = raea_core_shape::buffer_tris(b)?;
                    raea_core_shape::export_tris(&t, uv.as_deref(), &glb, "glb")?;
                    raea_core_shape::export_tris(&t, uv.as_deref(), &obj, "obj")?;
                }
                None => {
                    let m = raea_core_shape::solid(expr)?;
                    raea_core_shape::export(&m, &glb, "glb")?;
                    raea_core_shape::export(&m, &obj, "obj")?;
                }
            }
            meshes.insert(expr.to_string(), f.clone());
            f
        }
    };
    Ok(json!({ "type": "mesh", "file": file, "obj": file.replace(".glb", ".obj") }))
}

/// A file the scene names (a model, a texture) copied into `sub` of the export with its licence sidecar: its path there.
fn copied(dir: &Path, src: &str, out: &Path, sub: &str, files: &mut BTreeMap<PathBuf, String>) -> R<String> {
    let from = raea_core_scene::media_path(dir, if src.starts_with('/') || src.contains("://") { src.to_string() } else { format!("/media/{src}") }.as_str());
    let from = if from.exists() { from } else { raea_core_scene::media_path(dir, src) };
    if let Some(f) = files.get(&from) {
        return Ok(f.clone());
    }
    let name = from.file_name().and_then(|n| n.to_str()).ok_or_else(|| format!("{src}: no file"))?;
    let mut rel = format!("{sub}/{name}");
    if files.values().any(|v| *v == rel) {
        rel = format!("{sub}/{}-{name}", files.len());
    }
    std::fs::create_dir_all(out.join(sub)).map_err(|e| e.to_string())?;
    std::fs::copy(&from, out.join(&rel)).map_err(|e| format!("{}: {e}", from.display()))?;
    for side in [format!("{}.json", from.display()), format!("{}.json", from.with_extension("").display())] {
        if Path::new(&side).is_file() {
            let _ = std::fs::copy(&side, out.join(format!("{rel}.json")));
            break;
        }
    }
    files.insert(from, rel.clone());
    Ok(rel)
}

/// A part's surface: three.js-style props as the renderer reads them (runtime/scene's meshes_rows).
fn material(it: &Item, dir: &Path, out: &Path, files: &mut BTreeMap<PathBuf, String>) -> Value {
    let Some(m) = &it.material else { return json!({ "color": "#cccccc", "roughness": 0.5, "metallic": 0.0 }) };
    let p = &m.props;
    let mut v = json!({ "color": hex(p.get("color")).unwrap_or("#cccccc".into()), "roughness": n(p, "roughness", 0.5), "metallic": n(p, "metalness", 0.0) });
    if let Some(e) = hex(p.get("emissive")).filter(|e| e != "#000000") {
        v["emissive"] = json!(e);
        v["emissive_strength"] = json!(raea_core_scene::emissive_power(p));
    }
    if let Some(o) = num(p, "opacity").filter(|o| *o < 1.0) {
        v["opacity"] = json!(o);
    }
    if &*m.kind == "MeshBasicMaterial" || p.get("unlit") == Some(&Value::Bool(true)) {
        v["unlit"] = json!(true);
    }
    for (prop, key) in [("map", "color_map"), ("normalMap", "normal_map"), ("roughnessMap", "roughness_map"), ("metalnessMap", "metallic_map"), ("emissiveMap", "emissive_map")] {
        if let Some(src) = p.get(prop).and_then(Value::as_str)
            && let Ok(f) = copied(dir, src, out, "textures", files)
        {
            v[key] = json!(f);
        }
    }
    v
}

/// The light another tool sets: its kind, colour, intensity as raea reads it (1 is a 3d.tsx light's default) and in
/// Bevy's units (lux for a sun, lumens else), reach and cone.
fn light(it: &Item) -> Value {
    let p = &it.props;
    let power = light_power(it.kind, p);
    let unit = if it.kind == "directional" { raea_core_scene::LUX } else { raea_core_scene::LUMENS };
    let mut v = json!({ "type": it.kind, "color": hex(p.get("color")).unwrap_or("#ffffff".into()), "intensity": power / unit });
    v[if it.kind == "directional" { "lux" } else { "lumens" }] = json!(power);
    if let Some(d) = num(p, "distance").filter(|d| *d > 0.0) {
        v["range"] = json!(d);
    }
    if it.kind == "spot" {
        v["angle"] = json!(n(p, "angle", std::f64::consts::FRAC_PI_3));
        v["penumbra"] = json!(n(p, "penumbra", 0.0));
    }
    if it.kind == "rect" {
        v["width"] = json!(n(p, "width", 1.0));
        v["height"] = json!(n(p, "height", 1.0));
    }
    v
}

/// Where the part is written in the TSX, when the bundle stamped it, and its name.
fn source(it: &Item) -> Value {
    let s = |k: &str| it.props.get(k).and_then(Value::as_str).map(str::to_string);
    json!({ "name": s("name"), "loc": s("data-raea-loc"), "use": s("data-raea-use") })
}

/// The parts of every frame asked for (`all`, one list of items each), written into `out`: scene.json's value.
pub fn scene(all: &[Vec<Item>], frames: &[i64], dir: &Path, out: &Path) -> R<Value> {
    let (mut meshes, mut files) = (BTreeMap::new(), BTreeMap::new());
    let mut parts = Vec::new();
    for (id, track) in tracks_of(all, &["mesh", "model", "directional", "point", "spot", "rect"]) {
        let Some(first) = track.iter().flatten().next() else { continue };
        let mut part = json!({ "id": id, "kind": if first.kind == "mesh" || first.kind == "model" { first.kind } else { "light" }, "source": source(first) });
        match first.kind {
            // an <Instances> set is many copies of one mesh: left out (counted in `left`)
            "mesh" if first.at.is_some() => continue,
            "mesh" => {
                let Some(expr) = first.shape.as_deref() else { continue };
                part["shape"] = shape(expr, out, &mut meshes)?;
                part["material"] = material(first, dir, out, &mut files);
            }
            "model" => {
                let src = first.props.get("src").and_then(Value::as_str).unwrap_or_default();
                part["model"] = json!(copied(dir, src, out, "models", &mut files)?);
            }
            _ => part["light"] = light(first),
        }
        let at: Vec<Value> = track.iter().map(|t| t.as_ref().map_or(Value::Null, |it| if part["kind"] == "light" { light_placed(it) } else { placed(it) })).collect();
        part["at"] = json!(at);
        parts.push(part);
    }
    // the camera: the first drawing one each frame, from its position to what it looks at
    let cam: Vec<Value> = all.iter().map(|its| {
        its.iter().find(|i| i.kind == "camera").map_or(Value::Null, |c| {
            let (p, l, up) = (vec3(&c.props, "position", [0.0, 0.0, 5.0]), vec3(&c.props, "lookAt", [0.0; 3]), vec3(&c.props, "up", [0.0, 1.0, 0.0]));
            let q = look_quat([l[0] - p[0], l[1] - p[1], l[2] - p[2]], up);
            json!({ "at": [p[0], p[1], p[2], q[0], q[1], q[2], q[3], 1, 1, 1], "look": l, "fov": n(&c.props, "fov", 50.0) })
        })
    }).collect();
    let ambient = all[0].iter().find(|i| i.kind == "ambient").map(|a| json!({ "color": hex(a.props.get("color")).unwrap_or("#ffffff".into()), "nits": light_power("ambient", &a.props) }));
    // what doesn't cross, by kind, counted on the first frame
    let mut left: BTreeMap<&str, usize> = BTreeMap::new();
    for i in &all[0] {
        match i.kind {
            // a mesh drawn at many places (<Instances>) crosses as its first copy only
            "mesh" if i.at.is_some() => *left.entry("instances").or_default() += 1,
            "mesh" | "model" | "directional" | "point" | "spot" | "rect" | "camera" | "ambient" => {}
            k => *left.entry(k).or_default() += 1,
        }
    }
    Ok(json!({ "raea": "scene/1", "frames": frames, "parts": parts, "camera": cam, "ambient": ambient, "left": left }))
}

/// `raea scene export ...`: scene.json written, and its value.
pub fn export(args: &[String]) -> R<(PathBuf, Value)> {
    let edit = args.get(2).ok_or(USAGE)?;
    let tsx = dunce::canonicalize(&PathBuf::from(edit)).map_err(|e| format!("{edit}: {e}"))?;
    let dir = tsx.parent().ok_or("the edit's folder")?.to_path_buf();
    let out = args.iter().position(|a| a == "-o").and_then(|i| args.get(i + 1)).ok_or(USAGE)?;
    let out = std::path::absolute(out).map_err(|e| e.to_string())?;
    let frames = frames_of(args)?;
    let js = std::env::temp_dir().join(format!("raea-scene-export-{}.js", std::process::id()));
    // each part's place in the TSX (data-raea-loc), which raea blender back and raea godot back say their edits at
    // SAFETY: the CLI sets it before the bundle starts any thread that reads the environment
    unsafe { std::env::set_var("RAEA_SOURCE_STAMPS", "1") };
    raea_runtime_bundle::bundle_composition(&tsx, &js, &raea_runtime_bundle::comp_opts(args)?, None)?;
    let got = raea_runtime_export::scene_frames(&js, &dir, frames[frames.len() - 1] + 1, false, &frames);
    let _ = std::fs::remove_file(&js);
    let (got, w, h, fps) = got?.ok_or("the composition has no 3D")?;
    let all: Vec<Vec<Item>> = frames.iter().map(|f| got.iter().find(|(k, _)| k == f).map(|(_, fr)| raea_runtime_scene::items_of(&fr.scene)).unwrap_or_default()).collect();
    if all.iter().all(Vec::is_empty) {
        return Err(format!("{edit}: no 3D on frame {}", frames[0]));
    }
    let _ = std::fs::remove_dir_all(out.join("meshes"));
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut v = scene(&all, &frames, &dir, &out)?;
    v["from"] = json!(tsx.to_string_lossy());
    v["fps"] = json!(fps);
    v["size"] = json!([w, h]);
    std::fs::write(out.join("scene.json"), serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok((out, v))
}

/// The summary line of an export.
pub fn said(out: &Path, v: &Value) -> String {
    let parts = v["parts"].as_array().map_or(0, Vec::len);
    let frames = v["frames"].as_array().map_or(0, Vec::len);
    let left: Vec<String> = v["left"].as_object().into_iter().flatten().map(|(k, n)| format!("{n} {k}")).collect();
    let left = if left.is_empty() { String::new() } else { format!("; not carried: {}", left.join(", ")) };
    format!("{parts} parts on {frames} frame{} in {}{left}", if frames == 1 { "" } else { "s" }, out.join("scene.json").display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rotation_faces_where_it_is_turned() {
        // looking down -Z is no turn; looking down +X turns -90 degrees about Y
        assert_eq!(look_quat([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]), [0.0, 0.0, 0.0, 1.0]);
        let q = look_quat([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!((q[1] + std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6 && (q[3] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6, "{q:?}");
        // straight down still has a way up
        let q = look_quat([0.0, -1.0, 0.0], [0.0, 1.0, 0.0]);
        assert!(q.iter().all(|v| v.is_finite()) && (q.iter().map(|v| v * v).sum::<f64>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn frames_are_one_or_a_span() {
        let a = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        assert_eq!(frames_of(&a("scene export e.tsx")).unwrap(), vec![0]);
        assert_eq!(frames_of(&a("scene export e.tsx --frame 12")).unwrap(), vec![12]);
        assert_eq!(frames_of(&a("scene export e.tsx --at 3-5")).unwrap(), vec![3, 4, 5]);
        assert!(frames_of(&a("scene export e.tsx --at 5-3")).is_err());
    }

    #[test]
    fn primitives_cross_as_themselves_and_the_rest_as_a_mesh() {
        let out = std::env::temp_dir().join(format!("raea-interchange-shape-{}", std::process::id()));
        // no meshes/ beforehand, as export() leaves it: shape() makes it (it once failed every solid in a fresh export)
        std::fs::create_dir_all(&out).unwrap();
        let mut m = BTreeMap::new();
        assert_eq!(shape("box(1, 2, 3)", &out, &mut m).unwrap(), json!({ "type": "cuboid", "size": [1.0, 2.0, 3.0] }));
        assert_eq!(shape("sphere(0.5)", &out, &mut m).unwrap()["type"], "sphere");
        assert_eq!(shape("rotate(cone(0.5, 0.5, 2), -90, 0, 0)", &out, &mut m).unwrap()["height"], 2.0);
        let v = shape("difference(box(1, 1, 1), sphere(0.6))", &out, &mut m).unwrap();
        assert_eq!(v["type"], "mesh");
        assert!(out.join(v["file"].as_str().unwrap()).is_file());
        // the same mesh twice is one file
        assert_eq!(shape("difference(box(1, 1, 1), sphere(0.6))", &out, &mut m).unwrap(), v);
        // a buffer mesh (an open quad, as raea/r3f gives a three.js PlaneGeometry) is a mesh file too, not refused
        let q = shape(r#"buffer:{"positions": [0,0,0, 1,0,0, 1,1,0, 0,1,0], "indices": [0,1,2, 0,2,3]}"#, &out, &mut m).unwrap();
        assert_eq!(q["type"], "mesh");
        assert!(out.join(q["file"].as_str().unwrap()).is_file() && out.join(q["obj"].as_str().unwrap()).is_file());
        let _ = std::fs::remove_dir_all(&out);
    }
}
