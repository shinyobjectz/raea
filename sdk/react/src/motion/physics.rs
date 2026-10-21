//! Physics for compositions (sdk/motion/src/physics.tsx): rigid bodies baked with Rapier, parts of a glTF cut out as
//! bodies, and goo (tools/goo: MLS-MPM on Metal) baked to a mesh sequence. Every bake is a pure function of what it's
//! asked (the request, and the files it names by content), kept in the composition's asset store under that key, so a
//! render, a `render --at` and a play read the same bytes, and nothing is simulated at a frame.
//!
//!   native: physics.part  {src, nodes, material?}            -> {file, centre, lo, hi, tris}
//!   native: physics.bake  {fps, frames, gravity?, substeps?, bodies: [...]}  -> {file, bodies: {name: {p, q}}}
//!   native: physics.goo   {spec}                              -> {file, stats}
//!   raea-motion physics bake REQUEST.json [-o OUT.json]       (the same bake from the command line)
//!
//! Rapier (Apache-2.0) because it's a pure Rust library raea calls in process: no Bevy app, no Python; its
//! `enhanced-determinism` build steps the same inputs to the same bits on every run (and every platform), and the same
//! world type can step a live game frame by frame (physics.tsx, "Games").

use std::path::{Path, PathBuf};

use rapier3d::glamx::glam;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

type R<T> = Result<T, String>;

fn hex_sha(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}

/// A file's identity for a cache key: its path, size and modification time (a bake names big models; hashing 86 MB
/// on every call would cost more than the bake).
fn file_id(p: &Path) -> R<Value> {
    let m = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos() as u64).unwrap_or(0);
    Ok(json!([p.display().to_string(), m.len(), t]))
}

fn store_dir() -> PathBuf {
    let dir = crate::native::composition_dir().unwrap_or_else(|| PathBuf::from("."));
    rdb::assets::Store::beside(&dir.join("edit.tsx")).dir
}

fn media(src: &str) -> PathBuf {
    let dir = crate::native::composition_dir().unwrap_or_else(|| PathBuf::from("."));
    super::media_path(&dir, src)
}

// ---- glTF ------------------------------------------------------------------------------------------------------

struct Glb {
    j: Value,
    bin: Vec<u8>,
}

impl Glb {
    fn read(p: &Path) -> R<Glb> {
        let b = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
        if b.len() < 20 || &b[0..4] != b"glTF" {
            return Err(format!("{}: not a .glb", p.display()));
        }
        let jl = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
        let j: Value = serde_json::from_slice(&b[20..20 + jl]).map_err(|e| format!("{}: {e}", p.display()))?;
        let mut bin = Vec::new();
        let at = 20 + jl;
        if b.len() >= at + 8 {
            let bl = u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
            bin = b[at + 8..at + 8 + bl].to_vec();
        }
        Ok(Glb { j, bin })
    }

    fn arr(&self, k: &str) -> Vec<Value> {
        self.j.get(k).and_then(Value::as_array).cloned().unwrap_or_default()
    }

    /// Each node's world matrix (column-major), parents first.
    fn world(&self) -> Vec<glam::Mat4> {
        let nodes = self.arr("nodes");
        let mut w = vec![glam::Mat4::IDENTITY; nodes.len()];
        let local = |n: &Value| -> glam::Mat4 {
            if let Some(m) = n.get("matrix").and_then(Value::as_array) {
                let a: Vec<f32> = m.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
                if a.len() == 16 {
                    return glam::Mat4::from_cols_slice(&a);
                }
            }
            let v = |k: &str, d: &[f32]| -> Vec<f32> { n.get(k).and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or(d.to_vec()) };
            let (t, r, s) = (v("translation", &[0.0; 3]), v("rotation", &[0.0, 0.0, 0.0, 1.0]), v("scale", &[1.0; 3]));
            glam::Mat4::from_scale_rotation_translation(glam::Vec3::new(s[0], s[1], s[2]), glam::Quat::from_xyzw(r[0], r[1], r[2], r[3]), glam::Vec3::new(t[0], t[1], t[2]))
        };
        let scenes = self.arr("scenes");
        let roots: Vec<usize> = scenes.first().and_then(|s| s.get("nodes")).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as usize)).collect()).unwrap_or_else(|| (0..nodes.len()).collect());
        let mut stack: Vec<(usize, glam::Mat4)> = roots.into_iter().map(|r| (r, glam::Mat4::IDENTITY)).collect();
        while let Some((i, parent)) = stack.pop() {
            let Some(n) = nodes.get(i) else { continue };
            w[i] = parent * local(n);
            for c in n.get("children").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64) {
                stack.push((c as usize, w[i]));
            }
        }
        w
    }

    fn nodes_named(&self, names: &[String]) -> R<Vec<usize>> {
        let nodes = self.arr("nodes");
        let mut out = Vec::new();
        for name in names {
            let i = nodes.iter().position(|n| n.get("name").and_then(Value::as_str) == Some(name)).ok_or(format!("no node named {name:?}"))?;
            out.push(i);
        }
        Ok(out)
    }

    fn view(&self, accessor: usize) -> R<(Vec<u8>, usize, String, String, usize)> {
        let a = self.arr("accessors").get(accessor).cloned().ok_or("no such accessor")?;
        let count = a["count"].as_u64().unwrap_or(0) as usize;
        let ty = a["type"].as_str().unwrap_or("").to_string();
        let ct = a["componentType"].as_u64().unwrap_or(0);
        let comp = match ct { 5126 | 5125 => 4, 5123 | 5122 => 2, _ => 1 };
        let n = match ty.as_str() { "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, "MAT4" => 16, _ => 1 };
        let bv = self.arr("bufferViews").get(a["bufferView"].as_u64().unwrap_or(0) as usize).cloned().ok_or("no bufferView")?;
        let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let stride = bv["byteStride"].as_u64().map(|s| s as usize).unwrap_or(comp * n);
        let mut out = Vec::with_capacity(count * comp * n);
        for i in 0..count {
            let s = off + i * stride;
            out.extend_from_slice(self.bin.get(s..s + comp * n).ok_or("accessor past the buffer")?);
        }
        Ok((out, count, ty, ct.to_string(), comp * n))
    }

    /// The nodes' triangles in the world: positions and indices.
    fn triangles(&self, nodes: &[usize]) -> R<(Vec<glam::Vec3>, Vec<[u32; 3]>)> {
        let w = self.world();
        let all = self.arr("nodes");
        let meshes = self.arr("meshes");
        let (mut pos, mut tri) = (Vec::new(), Vec::new());
        for &i in nodes {
            let Some(m) = all[i].get("mesh").and_then(Value::as_u64) else { continue };
            for prim in meshes[m as usize]["primitives"].as_array().into_iter().flatten() {
                let base = pos.len() as u32;
                let (b, count, _, _, _) = self.view(prim["attributes"]["POSITION"].as_u64().ok_or("a primitive without positions")? as usize)?;
                for k in 0..count {
                    let f = |o: usize| f32::from_le_bytes(b[k * 12 + o..k * 12 + o + 4].try_into().unwrap());
                    pos.push(w[i].transform_point3(glam::Vec3::new(f(0), f(4), f(8))));
                }
                let idx: Vec<u32> = match prim.get("indices").and_then(Value::as_u64) {
                    Some(ia) => {
                        let (b, count, _, ct, _) = self.view(ia as usize)?;
                        (0..count).map(|k| match ct.as_str() {
                            "5125" => u32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().unwrap()),
                            "5123" => u16::from_le_bytes(b[k * 2..k * 2 + 2].try_into().unwrap()) as u32,
                            _ => b[k] as u32,
                        }).collect()
                    }
                    None => (0..count as u32).collect(),
                };
                for t in idx.chunks_exact(3) {
                    tri.push([base + t[0], base + t[1], base + t[2]]);
                }
            }
        }
        Ok((pos, tri))
    }

    /// A new .glb holding only these nodes (each with its world matrix, so it stands where it stood), their meshes,
    /// materials and pictures; `material` replaces every material (a glitch's echo or missing texture).
    fn subset(&self, nodes: &[usize], material: Option<&Value>) -> R<Vec<u8>> {
        let w = self.world();
        let all = self.arr("nodes");
        let (accessors, views, meshes) = (self.arr("accessors"), self.arr("bufferViews"), self.arr("meshes"));
        let (materials, textures, images, samplers) = (self.arr("materials"), self.arr("textures"), self.arr("images"), self.arr("samplers"));
        let mut bin: Vec<u8> = Vec::new();
        let mut new_views: Vec<Value> = Vec::new();
        let mut view_map = std::collections::HashMap::new();
        let mut copy_view = |v: usize, bin: &mut Vec<u8>, new_views: &mut Vec<Value>| -> usize {
            *view_map.entry(v).or_insert_with(|| {
                let bv = &views[v];
                let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize;
                let len = bv["byteLength"].as_u64().unwrap_or(0) as usize;
                while bin.len() % 4 != 0 {
                    bin.push(0);
                }
                let at = bin.len();
                bin.extend_from_slice(&self.bin[off..off + len]);
                let mut nv = bv.clone();
                nv["buffer"] = json!(0);
                nv["byteOffset"] = json!(at);
                new_views.push(nv);
                new_views.len() - 1
            })
        };
        let mut acc_map = std::collections::HashMap::new();
        let mut new_acc: Vec<Value> = Vec::new();
        let mut mat_map = std::collections::HashMap::new();
        let mut new_mats: Vec<Value> = Vec::new();
        let (mut tex_map, mut img_map, mut smp_map) = (std::collections::HashMap::new(), std::collections::HashMap::new(), std::collections::HashMap::new());
        let (mut new_tex, mut new_img, mut new_smp): (Vec<Value>, Vec<Value>, Vec<Value>) = (Vec::new(), Vec::new(), Vec::new());
        let mut new_meshes = Vec::new();
        let mut new_nodes = Vec::new();
        let override_mat = material.map(|m| {
            new_mats.push(m.clone());
            0usize
        });
        for &i in nodes {
            let n = &all[i];
            let mut nn = json!({"name": n.get("name").cloned().unwrap_or(Value::Null), "matrix": w[i].to_cols_array()});
            if let Some(m) = n.get("mesh").and_then(Value::as_u64) {
                let mut mesh = meshes[m as usize].clone();
                for prim in mesh["primitives"].as_array_mut().into_iter().flatten() {
                    let mut remap = |a: u64, bin: &mut Vec<u8>, new_views: &mut Vec<Value>| -> u64 {
                        *acc_map.entry(a).or_insert_with(|| {
                            let mut acc = accessors[a as usize].clone();
                            if let Some(v) = acc.get("bufferView").and_then(Value::as_u64) {
                                acc["bufferView"] = json!(copy_view(v as usize, bin, new_views));
                            }
                            new_acc.push(acc);
                            (new_acc.len() - 1) as u64
                        })
                    };
                    if let Some(attrs) = prim["attributes"].as_object_mut() {
                        for v in attrs.values_mut() {
                            *v = json!(remap(v.as_u64().unwrap_or(0), &mut bin, &mut new_views));
                        }
                    }
                    if let Some(ix) = prim.get("indices").and_then(Value::as_u64) {
                        prim["indices"] = json!(remap(ix, &mut bin, &mut new_views));
                    }
                    // morph targets aren't carried
                    if let Some(o) = prim.as_object_mut() {
                        o.remove("targets");
                    }
                    let mi = prim.get("material").and_then(Value::as_u64);
                    let nm = match (override_mat, mi) {
                        (Some(o), _) => Some(o),
                        (None, Some(mi)) => Some(*mat_map.entry(mi).or_insert_with(|| {
                            let mut mat = materials[mi as usize].clone();
                            // every texture reference in the material, wherever it sits (pbr, normal, extensions)
                            fn walk(v: &mut Value, f: &mut dyn FnMut(u64) -> u64) {
                                match v {
                                    Value::Object(o) => {
                                        for (k, x) in o.iter_mut() {
                                            if k == "index" && x.is_u64() {
                                                *x = json!(f(x.as_u64().unwrap()));
                                            } else {
                                                walk(x, f);
                                            }
                                        }
                                    }
                                    Value::Array(a) => a.iter_mut().for_each(|x| walk(x, f)),
                                    _ => {}
                                }
                            }
                            walk(&mut mat, &mut |t| {
                                *tex_map.entry(t).or_insert_with(|| {
                                    let mut tex = textures[t as usize].clone();
                                    if let Some(s) = tex.get("source").and_then(Value::as_u64) {
                                        let ni = *img_map.entry(s).or_insert_with(|| {
                                            let mut img = images[s as usize].clone();
                                            if let Some(v) = img.get("bufferView").and_then(Value::as_u64) {
                                                img["bufferView"] = json!(copy_view(v as usize, &mut bin, &mut new_views));
                                            }
                                            new_img.push(img);
                                            (new_img.len() - 1) as u64
                                        });
                                        tex["source"] = json!(ni);
                                    }
                                    if let Some(s) = tex.get("sampler").and_then(Value::as_u64) {
                                        let ns = *smp_map.entry(s).or_insert_with(|| {
                                            new_smp.push(samplers[s as usize].clone());
                                            (new_smp.len() - 1) as u64
                                        });
                                        tex["sampler"] = json!(ns);
                                    }
                                    new_tex.push(tex);
                                    (new_tex.len() - 1) as u64
                                })
                            });
                            new_mats.push(mat);
                            new_mats.len() - 1
                        })),
                        (None, None) => None,
                    };
                    if let Some(nm) = nm {
                        prim["material"] = json!(nm);
                    }
                }
                new_meshes.push(mesh);
                nn["mesh"] = json!(new_meshes.len() - 1);
            }
            new_nodes.push(nn);
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let mut j = json!({
            "asset": {"version": "2.0", "generator": "raea physics.part"},
            "scene": 0, "scenes": [{"nodes": (0..new_nodes.len()).collect::<Vec<_>>()}],
            "nodes": new_nodes, "meshes": new_meshes, "accessors": new_acc, "bufferViews": new_views,
            "buffers": [{"byteLength": bin.len()}], "materials": new_mats,
        });
        if !new_tex.is_empty() {
            j["textures"] = json!(new_tex);
            j["images"] = json!(new_img);
        }
        if !new_smp.is_empty() {
            j["samplers"] = json!(new_smp);
        }
        let mut used: Vec<Value> = self.j.get("extensionsUsed").and_then(Value::as_array).cloned().unwrap_or_default();
        if material.is_some_and(|m| m.pointer("/extensions/KHR_materials_unlit").is_some()) && !used.contains(&json!("KHR_materials_unlit")) {
            used.push(json!("KHR_materials_unlit"));
        }
        if !used.is_empty() {
            j["extensionsUsed"] = json!(used);
        }
        Ok(glb_bytes(&j, &bin))
    }
}

fn glb_bytes(j: &Value, bin: &[u8]) -> Vec<u8> {
    let mut js = j.to_string().into_bytes();
    while js.len() % 4 != 0 {
        js.push(b' ');
    }
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((js.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(&js);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}

/// The material a part's variant asks for: `{missing: true}` (the magenta and black checker of a texture that didn't
/// load), or `{color, emissive, opacity, unlit}`.
fn variant_material(m: &Value, bin_images: &mut Vec<Vec<u8>>) -> Value {
    let rgb = |k: &str, d: [f64; 3]| -> [f64; 3] {
        m.get(k).and_then(Value::as_str).map(|h| {
            let h = h.trim_start_matches('#');
            let b = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("ff"), 16).unwrap_or(255) as f64 / 255.0;
            // sRGB to linear (glTF's factors are linear)
            [b(0), b(2), b(4)].map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) })
        }).unwrap_or(d)
    };
    if m.get("missing").and_then(Value::as_bool) == Some(true) {
        bin_images.push(checker_png());
        return json!({"name": "missing", "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicFactor": 0.0, "roughnessFactor": 0.6},
            "emissiveFactor": [0.25, 0.0, 0.25]});
    }
    let c = rgb("color", [1.0, 1.0, 1.0]);
    let op = m.get("opacity").and_then(Value::as_f64).unwrap_or(1.0);
    let mut mat = json!({"name": "variant", "pbrMetallicRoughness": {"baseColorFactor": [c[0], c[1], c[2], op], "metallicFactor": 0.0, "roughnessFactor": m.get("roughness").and_then(Value::as_f64).unwrap_or(0.5)},
        "emissiveFactor": rgb("emissive", [0.0, 0.0, 0.0]), "doubleSided": true});
    if op < 1.0 {
        mat["alphaMode"] = json!("BLEND");
    }
    if m.get("unlit").and_then(Value::as_bool) == Some(true) {
        mat["extensions"] = json!({"KHR_materials_unlit": {}});
    }
    mat
}

fn checker_png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(64, 64, |x, y| if ((x / 8) + (y / 8)) % 2 == 0 { image::Rgba([255, 0, 255, 255]) } else { image::Rgba([0, 0, 0, 255]) });
    let mut out = std::io::Cursor::new(Vec::new());
    let _ = image::DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

/// `physics.part`: the nodes cut out of a .glb (and a variant's material), kept by key; their centre and bounds.
pub fn part(v: &Value) -> R<Value> {
    let src = v.get("src").and_then(Value::as_str).ok_or("physics.part: src, a .glb (staticFile(\"x.glb\"))")?;
    let nodes: Vec<String> = v.get("nodes").and_then(Value::as_array).ok_or("physics.part: nodes, the glTF nodes' names")?.iter().filter_map(|x| x.as_str().map(String::from)).collect();
    let path = media(src);
    let material = v.get("material").filter(|m| m.is_object()).cloned();
    let key = hex_sha(json!(["part", 2, file_id(&path)?, nodes, material]).to_string().as_bytes());
    let dir = store_dir();
    let out = dir.join(format!("{key}.glb"));
    let meta = dir.join(format!("{key}.glb.json"));
    if let Ok(b) = std::fs::read(&meta) {
        if out.exists() {
            if let Ok(m) = serde_json::from_slice::<Value>(&b) {
                return Ok(m);
            }
        }
    }
    let g = Glb::read(&path)?;
    let ix = g.nodes_named(&nodes).map_err(|e| format!("physics.part {src}: {e}"))?;
    let (pos, tri) = g.triangles(&ix)?;
    let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
    for p in &pos {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let mut extra = Vec::new();
    let mat = material.as_ref().map(|m| variant_material(m, &mut extra));
    let mut bytes = g.subset(&ix, mat.as_ref())?;
    if let Some(png) = extra.pop() {
        // the checker as the file's one picture
        let gg = parse_glb(&bytes)?;
        let (mut j, mut bin) = gg;
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let at = bin.len();
        bin.extend(&png);
        let views = j["bufferViews"].as_array_mut().unwrap();
        views.push(json!({"buffer": 0, "byteOffset": at, "byteLength": png.len()}));
        let vi = views.len() - 1;
        j["images"] = json!([{"bufferView": vi, "mimeType": "image/png"}]);
        j["samplers"] = json!([{"magFilter": 9728, "minFilter": 9728}]);
        j["textures"] = json!([{"source": 0, "sampler": 0}]);
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        j["buffers"] = json!([{"byteLength": bin.len()}]);
        bytes = glb_bytes(&j, &bin);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    let m = json!({"file": out.display().to_string(), "centre": ((lo + hi) / 2.0).to_array(), "lo": lo.to_array(), "hi": hi.to_array(), "tris": tri.len()});
    std::fs::write(&meta, m.to_string()).map_err(|e| format!("{}: {e}", meta.display()))?;
    Ok(m)
}

fn parse_glb(b: &[u8]) -> R<(Value, Vec<u8>)> {
    let jl = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
    let j: Value = serde_json::from_slice(&b[20..20 + jl]).map_err(|e| e.to_string())?;
    let at = 20 + jl;
    let bl = u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    Ok((j, b[at + 8..at + 8 + bl].to_vec()))
}

// ---- rigid bodies ----------------------------------------------------------------------------------------------

fn f(v: &Value, k: &str, d: f64) -> f32 {
    v.get(k).and_then(Value::as_f64).unwrap_or(d) as f32
}

fn v3(v: &Value, k: &str, d: [f32; 3]) -> glam::Vec3 {
    v.get(k).and_then(Value::as_array).filter(|a| a.len() == 3).map(|a| glam::Vec3::new(a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32, a[2].as_f64().unwrap_or(0.0) as f32)).unwrap_or(glam::Vec3::from_array(d))
}

/// A body's shape and the point it turns about (its rest centre), from its request: a hull (or trimesh, for a fixed
/// body) of its glTF nodes' triangles, or a box, sphere or capsule given in metres.
fn shape_of(b: &Value, glbs: &mut std::collections::HashMap<PathBuf, Glb>) -> R<(ColliderBuilder, glam::Vec3)> {
    let name = b.get("name").and_then(Value::as_str).unwrap_or("?");
    let centre = v3(b, "centre", [0.0; 3]);
    let shape = b.get("shape").cloned().unwrap_or(json!("hull"));
    if let Some(s) = shape.as_object() {
        let c = if let Some(x) = s.get("box").and_then(Value::as_array) {
            ColliderBuilder::cuboid(x[0].as_f64().unwrap_or(0.1) as f32 / 2.0, x[1].as_f64().unwrap_or(0.1) as f32 / 2.0, x[2].as_f64().unwrap_or(0.1) as f32 / 2.0)
        } else if let Some(r) = s.get("sphere").and_then(Value::as_f64) {
            ColliderBuilder::ball(r as f32)
        } else if let Some(x) = s.get("capsule").and_then(Value::as_array) {
            // [radius, length]: along y
            ColliderBuilder::capsule_y(x[1].as_f64().unwrap_or(0.1) as f32 / 2.0, x[0].as_f64().unwrap_or(0.05) as f32)
        } else {
            return Err(format!("body {name}: shape is \"hull\", \"mesh\", {{box: [x, y, z]}}, {{sphere: r}} or {{capsule: [r, length]}}"));
        };
        return Ok((c, centre));
    }
    let src = b.get("src").and_then(Value::as_str).ok_or(format!("body {name}: a hull needs src and nodes (or give a shape)"))?;
    let path = media(src);
    if !glbs.contains_key(&path) {
        glbs.insert(path.clone(), Glb::read(&path)?);
    }
    let g = &glbs[&path];
    let nodes: Vec<String> = b.get("nodes").and_then(Value::as_array).into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
    let ix = g.nodes_named(&nodes).map_err(|e| format!("body {name}: {e}"))?;
    let (pos, tri) = g.triangles(&ix)?;
    if pos.len() < 4 {
        return Err(format!("body {name}: its nodes have {} vertices; a hull needs 4", pos.len()));
    }
    let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
    for p in &pos {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let c = if b.get("centre").is_some() { centre } else { (lo + hi) / 2.0 };
    let local: Vec<glam::Vec3> = pos.iter().map(|p| *p - c).collect();
    let col = match shape.as_str() {
        Some("mesh") => ColliderBuilder::trimesh(local, tri).map_err(|e| format!("body {name}: {e:?}"))?,
        Some("box") => {
            let h = (hi - lo) / 2.0;
            ColliderBuilder::cuboid(h.x.max(1e-3), h.y.max(1e-3), h.z.max(1e-3))
        }
        _ => ColliderBuilder::convex_hull(&local).ok_or(format!("body {name}: no hull (its vertices are flat?): give shape \"box\""))?,
    };
    Ok((col, c))
}

/// The bake: every body stepped at `substeps` x fps from its rest pose, its pose kept each frame as the rigid motion
/// from rest (`p` the translation of the rest-space points, `q` the rotation), so a part drawn where it stood at rest
/// is placed by a group with that position and quaternion.
pub fn simulate(req: &Value) -> R<Value> {
    let fps = req.get("fps").and_then(Value::as_f64).unwrap_or(30.0);
    let frames = req.get("frames").and_then(Value::as_u64).ok_or("physics.bake: frames")? as usize;
    let sub = req.get("substeps").and_then(Value::as_u64).unwrap_or(8).clamp(1, 64) as usize;
    let mut w = PhysicsWorld::new();
    w.gravity = v3(req, "gravity", [0.0, -9.81, 0.0]);
    w.integration_parameters.dt = (1.0 / (fps * sub as f64)) as f32;
    let bodies = req.get("bodies").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut glbs = std::collections::HashMap::new();
    struct B {
        name: String,
        h: RigidBodyHandle,
        c: glam::Vec3,
        release: Option<usize>,
        kin: Option<(Vec<f32>, Vec<f32>)>,
        impulses: Vec<Value>,
    }
    let mut list: Vec<B> = Vec::new();
    for b in &bodies {
        let name = b.get("name").and_then(Value::as_str).ok_or("physics.bake: every body needs a name")?.to_string();
        let (col, c) = shape_of(b, &mut glbs)?;
        let fixed = b.get("fixed").and_then(Value::as_bool) == Some(true);
        let kin = b.get("path").filter(|p| p.is_object()).map(|p| {
            let arr = |k: &str| p.get(k).and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>()).unwrap_or_default();
            (arr("p"), arr("q"))
        });
        let release = b.get("release").and_then(Value::as_f64).map(|r| r.max(0.0).round() as usize).filter(|r| *r > 0);
        let rb = if fixed {
            RigidBodyBuilder::fixed()
        } else if kin.is_some() || release.is_some() {
            RigidBodyBuilder::kinematic_position_based()
        } else {
            RigidBodyBuilder::dynamic()
        };
        let rb = rb
            .translation(c)
            .linvel(v3(b, "velocity", [0.0; 3]))
            .angvel(v3(b, "angularVelocity", [0.0; 3]))
            .linear_damping(f(b, "linearDamping", 0.0))
            .angular_damping(f(b, "angularDamping", 0.05))
            .ccd_enabled(b.get("ccd").and_then(Value::as_bool).unwrap_or(true))
            .can_sleep(false)
            .build();
        let h = w.bodies.insert(rb);
        let mut col = col.friction(f(b, "friction", 0.5)).restitution(f(b, "restitution", 0.2));
        col = match b.get("mass").and_then(Value::as_f64) {
            Some(m) => col.mass(m as f32),
            None => col.density(f(b, "density", 1000.0)),
        };
        w.colliders.insert_with_parent(col.build(), h, &mut w.bodies);
        let impulses = b.get("impulses").and_then(Value::as_array).cloned().unwrap_or_default();
        list.push(B { name, h, c, release, kin, impulses });
    }
    // static colliders: the floor and tables (boxes, world space)
    for c in req.get("colliders").and_then(Value::as_array).into_iter().flatten() {
        let lo = v3(c, "min", [-1.0, -1.0, -1.0]);
        let hi = v3(c, "max", [1.0, 0.0, 1.0]);
        let h = (hi - lo) / 2.0;
        w.colliders.insert(ColliderBuilder::cuboid(h.x, h.y, h.z).translation((lo + hi) / 2.0).friction(f(c, "friction", 0.6)).restitution(f(c, "restitution", 0.1)).build());
    }
    let mut out: Vec<(Vec<f32>, Vec<f32>)> = list.iter().map(|_| (Vec::with_capacity(frames * 3), Vec::with_capacity(frames * 4))).collect();
    let pose_of = |k: &(Vec<f32>, Vec<f32>), fr: usize, c: glam::Vec3| -> (glam::Vec3, glam::Quat) {
        let n = k.0.len() / 3;
        let i = fr.min(n.saturating_sub(1));
        let p = if n > 0 { glam::Vec3::new(k.0[i * 3], k.0[i * 3 + 1], k.0[i * 3 + 2]) } else { glam::Vec3::ZERO };
        let q = if k.1.len() >= (i + 1) * 4 { glam::Quat::from_xyzw(k.1[i * 4], k.1[i * 4 + 1], k.1[i * 4 + 2], k.1[i * 4 + 3]).normalize() } else { glam::Quat::IDENTITY };
        // the path is the part's rigid motion from rest: its centre goes to p + q c
        (p + q * c, q)
    };
    for fr in 0..frames {
        // this frame's events: releases, impulses, kinematic targets
        for b in &list {
            let body = &mut w.bodies[b.h];
            if b.release == Some(fr) && b.kin.is_none() {
                body.set_body_type(RigidBodyType::Dynamic, true);
            }
            for e in &b.impulses {
                if e.get("frame").and_then(Value::as_f64).map(|x| x.round() as usize) != Some(fr) {
                    continue;
                }
                let imp = v3(e, "impulse", [0.0; 3]);
                match e.get("at").filter(|a| a.is_array()) {
                    Some(_) => body.apply_impulse_at_point(imp, v3(e, "at", [0.0; 3]), true),
                    None => body.apply_impulse(imp, true),
                }
                let t = v3(e, "torque", [0.0; 3]);
                if t != glam::Vec3::ZERO {
                    body.apply_torque_impulse(t, true);
                }
            }
        }
        for s in 0..sub {
            for b in &list {
                if let Some(k) = &b.kin {
                    // between this frame's target and the next's
                    let (p0, q0) = pose_of(k, fr, b.c);
                    let (p1, q1) = pose_of(k, fr + 1, b.c);
                    let u = (s + 1) as f32 / sub as f32;
                    let body = &mut w.bodies[b.h];
                    body.set_next_kinematic_position(Pose::from_parts(p0.lerp(p1, u), q0.slerp(q1, u)));
                }
            }
            w.step();
        }
        for (b, o) in list.iter().zip(out.iter_mut()) {
            let body = &w.bodies[b.h];
            let q = *body.rotation();
            let t = body.translation();
            // rest point x goes to t + q (x - c)
            let p = t - q * b.c;
            o.0.extend([p.x, p.y, p.z]);
            o.1.extend([q.x, q.y, q.z, q.w]);
        }
    }
    let r6 = |x: f32| (x as f64 * 1e6).round() / 1e6;
    let mut res = serde_json::Map::new();
    for (b, o) in list.iter().zip(&out) {
        res.insert(b.name.clone(), json!({"p": o.0.iter().map(|x| r6(*x)).collect::<Vec<_>>(), "q": o.1.iter().map(|x| r6(*x)).collect::<Vec<_>>(), "centre": b.c.to_array()}));
    }
    Ok(json!({"fps": fps, "frames": frames, "bodies": res}))
}

/// `physics.bake`: the simulation for a request, made once (keyed by the request and its files) and kept.
pub fn bake(req: &Value) -> R<Value> {
    let mut files = Vec::new();
    for b in req.get("bodies").and_then(Value::as_array).into_iter().flatten() {
        if let Some(s) = b.get("src").and_then(Value::as_str) {
            files.push(file_id(&media(s))?);
        }
    }
    let key = hex_sha(json!(["physics", 1, req, files]).to_string().as_bytes());
    let dir = store_dir();
    let out = dir.join(format!("{key}.physics.json"));
    if let Ok(b) = std::fs::read(&out) {
        if let Ok(mut v) = serde_json::from_slice::<Value>(&b) {
            v["file"] = json!(out.display().to_string());
            return Ok(v);
        }
    }
    let t0 = std::time::Instant::now();
    let mut v = simulate(req)?;
    v["seconds"] = json!(t0.elapsed().as_secs_f64());
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(&out, v.to_string()).map_err(|e| format!("{}: {e}", out.display()))?;
    eprintln!("raea: physics baked: {} bodies, {} frames in {:.2} s -> {}", req["bodies"].as_array().map(|a| a.len()).unwrap_or(0), req["frames"], t0.elapsed().as_secs_f64(), out.display());
    v["file"] = json!(out.display().to_string());
    Ok(v)
}

// ---- goo -------------------------------------------------------------------------------------------------------

fn goo_dir() -> PathBuf {
    std::env::var_os("RAEA_GOO").filter(|v| !v.is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/raea-goo"))
}

/// the goo tool's scripts (tools/goo): run.py (MPM) and sph.py (SPlisHSPlasH, a spec's "solver": "sph"); a bake is
/// keyed on both
const GOO_CODE: [&str; 2] = ["run.py", "sph.py"];

fn ensure_goo() -> R<PathBuf> {
    let dir = goo_dir();
    let ready = |d: &Path| d.join(".venv/bin/python").exists() && GOO_CODE.iter().all(|f| d.join(f).exists()) && d.join(".ready").exists();
    let script = rdb::assets::setup_script("goo");
    // a newer script in the tree than the install's: set up again (it copies them)
    let stale = GOO_CODE.iter().any(|f| {
        std::fs::metadata(script.with_file_name(f)).and_then(|m| m.modified()).ok()
            .zip(std::fs::metadata(dir.join(f)).and_then(|m| m.modified()).ok())
            .is_some_and(|(tree, inst)| tree > inst)
    });
    if ready(&dir) && !stale {
        return Ok(dir);
    }
    if std::env::var("RAEA_AUDIO_AUTOSETUP").as_deref() == Ok("0") && !ready(&dir) {
        return Err(format!("the goo solver isn't installed: run {}", script.display()));
    }
    eprintln!("raea: setting up the goo solver ({} into {})", script.display(), dir.display());
    let st = std::process::Command::new("sh").arg(&script).env("RAEA_GOO", &dir).stdout(std::io::stderr()).status().map_err(|e| format!("goo setup: {e}"))?;
    if !st.success() || !ready(&dir) {
        return Err(format!("goo setup didn't finish ({st}): run {}", script.display()));
    }
    Ok(dir)
}

/// `physics.goo`: a goo spec (tools/goo/run.py's) baked to a mesh sequence, once per spec and its files.
pub fn goo(v: &Value) -> R<Value> {
    let mut spec = v.get("spec").cloned().ok_or("physics.goo: spec")?;
    let mut files = Vec::new();
    let obstacles = spec.get_mut("obstacles").and_then(Value::as_array_mut).map(std::mem::take).unwrap_or_default();
    let mut placed = Vec::new();
    for mut o in obstacles {
        if let Some(s) = o.get("src").and_then(Value::as_str) {
            let p = media(s);
            files.push(file_id(&p)?);
            o["src"] = json!(p.display().to_string());
        }
        placed.push(o);
    }
    if !placed.is_empty() {
        spec["obstacles"] = json!(placed);
    }
    for o in spec.get_mut("objects").and_then(Value::as_array_mut).into_iter().flatten() {
        if let Some(s) = o.get("src").and_then(Value::as_str) {
            let p = media(s);
            files.push(file_id(&p)?);
            o["src"] = json!(p.display().to_string());
        }
    }
    let setup = rdb::assets::setup_script("goo");
    let code: Vec<String> = GOO_CODE.iter().map(|f| std::fs::read(setup.with_file_name(f)).map(|b| hex_sha(&b)).unwrap_or_default()).collect();
    let key = hex_sha(json!(["goo", 1, spec, files, code]).to_string().as_bytes());
    let dir = store_dir();
    let out = dir.join(format!("{key}.rseq"));
    let stats = dir.join(format!("{key}.rseq.json"));
    let answer = || -> R<Value> {
        let s: Value = std::fs::read(&stats).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({}));
        Ok(json!({"file": out.display().to_string(), "stats": s}))
    };
    if out.exists() && stats.exists() {
        return answer();
    }
    if std::env::var("RAEA_PHYSICS_NO_BAKE").as_deref() == Ok("1") {
        return Err(format!("physics.goo: not baked yet ({key}) and RAEA_PHYSICS_NO_BAKE=1"));
    }
    let tool = ensure_goo()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let sp = dir.join(format!("{key}.goo.json"));
    std::fs::write(&sp, spec.to_string()).map_err(|e| format!("{}: {e}", sp.display()))?;
    let tmp = dir.join(format!("{key}.rseq.part"));
    let t0 = std::time::Instant::now();
    eprintln!("raea: baking goo ({} objects, {} frames) -> {}", spec["objects"].as_array().map(|a| a.len()).unwrap_or(0), spec["frames"], out.display());
    let st = std::process::Command::new(tool.join(".venv/bin/python"))
        .arg("-I").arg(tool.join("run.py")).arg(&sp).arg(&tmp)
        .stdout(std::io::stderr())
        .status()
        .map_err(|e| format!("goo: {e}"))?;
    if !st.success() {
        return Err(format!("goo bake failed ({st}); spec {}", sp.display()));
    }
    std::fs::rename(&tmp, &out).map_err(|e| format!("{}: {e}", out.display()))?;
    let _ = std::fs::rename(tmp.with_extension("part.json"), &stats).or_else(|_| std::fs::rename(dir.join(format!("{key}.rseq.part.json")), &stats));
    if !stats.exists() {
        let _ = std::fs::write(&stats, json!({"seconds": t0.elapsed().as_secs_f64()}).to_string());
    }
    eprintln!("raea: goo baked in {:.1} s", t0.elapsed().as_secs_f64());
    answer()
}

pub(crate) fn register() {
    raea_native::register(
        "physics.part",
        "{ src: string; nodes: string[]; material?: { missing?: boolean; color?: string; emissive?: string; opacity?: number; roughness?: number; unlit?: boolean } }",
        "{ file: string; centre: [number, number, number]; lo: [number, number, number]; hi: [number, number, number]; tris: number }",
        "glTF nodes cut out as their own .glb (where they stood), optionally with one material over them (kept in the asset store)",
        part,
    );
    raea_native::register(
        "physics.bake",
        "{ fps: number; frames: number; gravity?: [number, number, number]; substeps?: number; colliders?: { min: [number, number, number]; max: [number, number, number]; friction?: number; restitution?: number }[]; bodies: Record<string, unknown>[] }",
        "{ fps: number; frames: number; file: string; bodies: Record<string, { p: number[]; q: number[]; centre: [number, number, number] }> }",
        "rigid bodies stepped by Rapier, deterministic, made once per request and kept: each body's rigid motion from rest per frame",
        bake,
    );
    raea_native::register(
        "physics.goo",
        "{ spec: Record<string, unknown> }",
        "{ file: string; stats: Record<string, unknown> }",
        "goo (MLS-MPM, tools/goo) baked to a mesh sequence (.rseq), made once per spec and kept",
        goo,
    );
}

/// `raea-motion physics bake REQUEST.json [-o OUT.json] [--dir EDIT_DIR]`, `raea-motion physics goo SPEC.json`
pub fn cli(args: &[String]) -> R<String> {
    let usage = "raea-motion physics bake REQUEST.json [-o OUT.json] | physics goo SPEC.json | physics part SRC.glb NODE...";
    let verb = args.get(1).map(String::as_str).ok_or(usage)?;
    let fl = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let read = |p: &str| -> R<Value> { serde_json::from_slice(&std::fs::read(p).map_err(|e| format!("{p}: {e}"))?).map_err(|e| format!("{p}: {e}")) };
    let v = match verb {
        "bake" => bake(&read(args.get(2).ok_or(usage)?)?)?,
        "simulate" => simulate(&read(args.get(2).ok_or(usage)?)?)?,
        "goo" => goo(&json!({"spec": read(args.get(2).ok_or(usage)?)?}))?,
        "part" => part(&json!({"src": args.get(2).ok_or(usage)?, "nodes": args[3..].iter().filter(|a| !a.starts_with('-')).collect::<Vec<_>>()}))?,
        _ => return Err(usage.into()),
    };
    match fl("-o") {
        Some(o) => {
            std::fs::write(&o, v.to_string()).map_err(|e| format!("{o}: {e}"))?;
            Ok(o)
        }
        None => Ok(v.get("file").and_then(Value::as_str).map(String::from).unwrap_or_else(|| v.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drop_request() -> Value {
        json!({"fps": 30, "frames": 45, "colliders": [{"min": [-2, -1, -2], "max": [2, 0, 2]}],
            "bodies": [
                {"name": "ball", "shape": {"sphere": 0.1}, "centre": [0, 1, 0], "restitution": 0.6},
                {"name": "crate", "shape": {"box": [0.2, 0.2, 0.2]}, "centre": [0.5, 0.5, 0], "angularVelocity": [0, 0, 3]},
                {"name": "held", "shape": {"box": [0.2, 0.2, 0.2]}, "centre": [-0.5, 0.5, 0], "release": 20, "impulses": [{"frame": 20, "impulse": [2.0, 0, 0]}]}
            ]})
    }

    #[test]
    fn a_bake_is_the_same_every_time() {
        let a = simulate(&drop_request()).unwrap();
        let b = simulate(&drop_request()).unwrap();
        assert_eq!(a.to_string(), b.to_string());
    }

    #[test]
    fn bodies_fall_bounce_and_wait_to_be_released() {
        let v = simulate(&drop_request()).unwrap();
        let p = |n: &str, fr: usize, a: usize| v["bodies"][n]["p"][fr * 3 + a].as_f64().unwrap();
        // the ball falls from 1 m and lands on the floor (its centre 0.1 above it: moved down by ~0.9)
        assert!(p("ball", 0, 1) > -0.01);
        assert!((p("ball", 44, 1) + 0.9).abs() < 0.12, "{}", p("ball", 44, 1));
        // held still until frame 20, then falls and is pushed along x
        assert!(p("held", 19, 1).abs() < 1e-6 && p("held", 19, 0).abs() < 1e-6);
        assert!(p("held", 44, 1) < -0.1 && p("held", 44, 0) > 0.01, "held at 20..44: {:?}",
            (20..45).step_by(4).map(|fr| (p("held", fr, 0), p("held", fr, 1))).collect::<Vec<_>>());
    }
}
