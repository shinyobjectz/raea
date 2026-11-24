//! glTF to triangles with a class each, and back with TEXCOORD_1. Read: every primitive of every node's mesh (a mesh
//! two nodes use is baked twice, as two meshes, since its two places weather apart), its positions put in the world
//! by the node's transform. Written: each primitive's vertices split where its charts meet (charts.rs), every
//! attribute copied element by element in the new order (whatever its type), TEXCOORD_1 added (and as TEXCOORD_0 where
//! there's none, since Bevy's standard surface reads the first UV set wherever the second is), and the binary
//! rebuilt with only what is still used. Refused: skins, morph targets, .gltf with outside files, sparse accessors,
//! modes other than triangles, and a material that already reads TEXCOORD_1 (a lightmapped model).

use parry3d::glamx::{Mat4, Quat, Vec3};
use serde_json::{Map, Value, json};

use super::R;
use crate::charts::Atlas;
use raea_sys_glb::{Glb, arr, compact, floats, indices, parse, raw};

/// A surface's material, which decides how each cause marks it (raea/wear's recipes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Plaster,
    Brick,
    Sandstone,
    Granite,
    Wood,
    Iron,
    Zinc,
    Copper,
    Slate,
    Felt,
    Glass,
    Enamel,
    Brass,
    Ground,
}

const NAMES: [(&str, Class); 14] = [
    ("plaster", Class::Plaster),
    ("brick", Class::Brick),
    ("sandstone", Class::Sandstone),
    ("granite", Class::Granite),
    ("wood", Class::Wood),
    ("iron", Class::Iron),
    ("zinc", Class::Zinc),
    ("copper", Class::Copper),
    ("slate", Class::Slate),
    ("felt", Class::Felt),
    ("glass", Class::Glass),
    ("enamel", Class::Enamel),
    ("brass", Class::Brass),
    ("ground", Class::Ground),
];

impl Class {
    pub fn of(s: &str) -> Option<Class> {
        NAMES.iter().find(|(n, _)| *n == s).map(|(_, c)| *c)
    }
    pub fn name(self) -> &'static str {
        NAMES.iter().find(|(_, c)| *c == self).map(|(n, _)| *n).unwrap_or("plaster")
    }
    /// causes2's alpha (raea/wear reads it with textureLoad): 16 x (its place in NAMES + 1), so 0 is a texel no class
    /// covered and a byte off by a few still names the class.
    pub fn code(self) -> u8 {
        16 * (NAMES.iter().position(|(_, c)| *c == self).unwrap_or(0) as u8 + 1)
    }
}

/// By material name, a class or {alpha: {CODE: class}, default}: COLOR_0's alpha (0-255) picks the class, the
/// nearest code within 5. "*" is any other material; with no table everything is plaster.
#[derive(Clone, Debug, Default)]
pub struct Classes(Vec<(String, Pick)>);

#[derive(Clone, Debug)]
enum Pick {
    One(Class),
    Alpha(Vec<(u8, Class)>, Class),
}

impl Classes {
    pub fn of(v: &Value) -> R<Classes> {
        let Some(o) = v.as_object() else { return Ok(Classes::default()) };
        let class = |v: &Value, k: &str| v.as_str().and_then(Class::of).ok_or_else(|| format!("classes.{k}: a class name, got {v}"));
        let mut out = Vec::new();
        for (k, v) in o {
            let p = match v {
                Value::Object(a) => {
                    let mut codes = Vec::new();
                    for (c, cl) in a.get("alpha").and_then(Value::as_object).ok_or_else(|| format!("classes.{k}: {{alpha, default}}"))? {
                        let code: u8 = c.parse().map_err(|_| format!("classes.{k}.alpha: a code 0-255, got {c}"))?;
                        codes.push((code, class(cl, k)?));
                    }
                    Pick::Alpha(codes, a.get("default").map(|d| class(d, k)).transpose()?.unwrap_or(Class::Plaster))
                }
                v => Pick::One(class(v, k)?),
            };
            out.push((k.clone(), p));
        }
        Ok(Classes(out))
    }

    fn pick(&self, material: &str, alpha: Option<u8>) -> Class {
        let p = self.0.iter().find(|(k, _)| k == material).or_else(|| self.0.iter().find(|(k, _)| k == "*"));
        match p.map(|(_, p)| p) {
            None => Class::Plaster,
            Some(Pick::One(c)) => *c,
            Some(Pick::Alpha(codes, d)) => {
                let Some(a) = alpha else { return *d };
                codes.iter().filter(|(c, _)| c.abs_diff(a) <= 5).min_by_key(|(c, _)| c.abs_diff(a)).map(|(_, k)| *k).unwrap_or(*d)
            }
        }
    }
}


/// One primitive as baked: which mesh and primitive of the source, its triangles' range in Scene::tri, and its own
/// vertex count (Scene::tri indexes `base + local`).
pub struct Prim {
    pub mesh: usize,
    pub prim: usize,
    pub first_tri: usize,
    pub tris: usize,
    pub base: u32,
}

pub struct Scene {
    pub glb: Glb,
    /// every vertex in the world; tri indexes it
    pub pos: Vec<Vec3>,
    pub tri: Vec<[u32; 3]>,
    /// per triangle: its plane's normal (zero for a degenerate one) and its class
    pub nrm: Vec<Vec3>,
    pub class: Vec<Class>,
    pub prims: Vec<Prim>,
    /// per node: the mesh it draws after the bake (a mesh used twice is copied)
    node_mesh: Vec<(usize, usize)>,
}


fn local(n: &Value) -> Mat4 {
    if let Some(m) = n.get("matrix").and_then(Value::as_array).filter(|m| m.len() == 16) {
        let a: Vec<f32> = m.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
        return Mat4::from_cols_slice(&a);
    }
    let v = |k: &str, d: &[f32]| -> Vec<f32> {
        n.get(k).and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_else(|| d.to_vec())
    };
    let (t, r, s) = (v("translation", &[0.0; 3]), v("rotation", &[0.0, 0.0, 0.0, 1.0]), v("scale", &[1.0; 3]));
    Mat4::from_scale_rotation_translation(Vec3::new(s[0], s[1], s[2]), Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize(), Vec3::new(t[0], t[1], t[2]))
}

pub fn read(bytes: &[u8], classes: &Classes) -> R<Scene> {
    let glb = parse(bytes)?;
    let j = &glb.json;
    for (i, m) in arr(j, "materials").iter().enumerate() {
        if m.to_string().contains("\"texCoord\":1") {
            return Err(format!("material {i} reads TEXCOORD_1 already (a lightmapped model): the bake would replace it"));
        }
    }
    let nodes = arr(j, "nodes");
    let mut world = vec![None; nodes.len()];
    let roots: Vec<usize> = match j.get("scenes").and_then(|s| s.get(j.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize)) {
        Some(s) => arr(s, "nodes").iter().filter_map(|n| n.as_u64().map(|n| n as usize)).collect(),
        None => (0..nodes.len()).collect(),
    };
    let mut stack: Vec<(usize, Mat4)> = roots.iter().map(|&r| (r, Mat4::IDENTITY)).collect();
    while let Some((n, parent)) = stack.pop() {
        let nd = nodes.get(n).ok_or_else(|| format!("node {n} missing"))?;
        if world[n].is_some() {
            continue;
        }
        let m = parent * local(nd);
        world[n] = Some(m);
        for c in arr(nd, "children").iter().filter_map(Value::as_u64) {
            stack.push((c as usize, m));
        }
    }
    let mut sc = Scene { glb: Glb { json: Value::Null, bin: Vec::new() }, pos: Vec::new(), tri: Vec::new(), nrm: Vec::new(), class: Vec::new(), prims: Vec::new(), node_mesh: Vec::new() };
    let meshes = arr(j, "meshes");
    let mats = arr(j, "materials");
    let mut used = vec![false; meshes.len()];
    for (n, nd) in nodes.iter().enumerate() {
        let (Some(mi), Some(m)) = (nd.get("mesh").and_then(Value::as_u64).map(|m| m as usize), world[n]) else { continue };
        if nd.get("skin").is_some() {
            return Err(format!("node {n} is skinned: a bake is of a still model"));
        }
        let mesh = meshes.get(mi).ok_or_else(|| format!("mesh {mi} missing"))?;
        // a mesh drawn by a second node is baked again as a copy (written as a new mesh)
        let baked_as = if used[mi] { meshes.len() + sc.node_mesh.iter().filter(|nb| nb.1 >= meshes.len()).count() } else { mi };
        used[mi] = true;
        sc.node_mesh.push((n, baked_as));
        for (pi, p) in arr(mesh, "primitives").iter().enumerate() {
            if p.get("mode").and_then(Value::as_u64).unwrap_or(4) != 4 {
                return Err(format!("mesh {mi} primitive {pi}: only triangles are taken"));
            }
            if p.get("targets").is_some() {
                return Err(format!("mesh {mi} primitive {pi}: morph targets aren't taken"));
            }
            let at = &p["attributes"];
            let (pos, _) = floats(&glb, at["POSITION"].as_u64().ok_or_else(|| format!("mesh {mi} primitive {pi}: no POSITION"))? as usize)?;
            let verts = (pos.len() / 3) as u32;
            let idx = match p.get("indices").and_then(Value::as_u64) {
                Some(i) => indices(&glb, i as usize)?,
                None => (0..verts).collect(),
            };
            let alpha = match at.get("COLOR_0").and_then(Value::as_u64) {
                Some(c) => {
                    let (f, comps) = floats(&glb, c as usize)?;
                    Some((f, comps))
                }
                None => None,
            };
            let mname = p.get("material").and_then(Value::as_u64).and_then(|i| mats.get(i as usize)).and_then(|m| m.get("name")).and_then(Value::as_str).unwrap_or("");
            let base = sc.pos.len() as u32;
            for v in pos.chunks_exact(3) {
                sc.pos.push(m.transform_point3(Vec3::new(v[0], v[1], v[2])));
            }
            let first_tri = sc.tri.len();
            for t in idx.chunks_exact(3) {
                if t.iter().any(|&i| i >= verts) {
                    return Err(format!("mesh {mi} primitive {pi}: an index past its vertices"));
                }
                let w = [base + t[0], base + t[1], base + t[2]];
                let (a, b, c) = (sc.pos[w[0] as usize], sc.pos[w[1] as usize], sc.pos[w[2] as usize]);
                sc.nrm.push((b - a).cross(c - a).try_normalize().unwrap_or(Vec3::ZERO));
                let a8 = alpha.as_ref().filter(|(_, k)| *k == 4).map(|(f, _)| (f[t[0] as usize * 4 + 3] * 255.0).round().clamp(0.0, 255.0) as u8);
                sc.class.push(classes.pick(mname, a8));
                sc.tri.push(w);
            }
            sc.prims.push(Prim { mesh: baked_as, prim: pi, first_tri, tris: sc.tri.len() - first_tri, base });
        }
    }
    if sc.tri.is_empty() {
        return Err("no triangles in the scene".into());
    }
    sc.glb = glb;
    Ok(sc)
}

/// The .glb again, each primitive re-indexed over its charts with TEXCOORD_1, and the binary holding only what's used.
pub fn write(sc: &Scene, at: &Atlas) -> R<Vec<u8>> {
    let g = &sc.glb;
    let mut j = g.json.clone();
    let src_meshes: Vec<Value> = arr(&j, "meshes").to_vec();
    let mut accessors: Vec<Value> = arr(&j, "accessors").to_vec();
    let mut views: Vec<Value> = arr(&j, "bufferViews").to_vec();
    let mut bin = g.bin.clone();
    let mut push = |bytes: &[u8], target: Option<u32>, acc: Value| -> usize {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let mut v = json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len()});
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        bin.extend_from_slice(bytes);
        views.push(v);
        let mut a = acc;
        a["bufferView"] = json!(views.len() - 1);
        a.as_object_mut().unwrap().remove("byteOffset");
        accessors.push(a);
        accessors.len() - 1
    };
    let mut meshes: Vec<Value> = src_meshes.clone();
    for (k, pr) in sc.prims.iter().enumerate() {
        while meshes.len() <= pr.mesh {
            let copy_of = sc.node_mesh.iter().find(|(_, b)| *b == meshes.len()).map(|(n, _)| arr(&j, "nodes")[*n]["mesh"].as_u64().unwrap() as usize).unwrap_or(0);
            meshes.push(src_meshes[copy_of].clone());
        }
        let (order, idx, uv) = (&at.verts[k], &at.index[k], &at.uv[k]);
        let p = meshes[pr.mesh]["primitives"][pr.prim].clone();
        let mut attrs = Map::new();
        for (name, a) in p["attributes"].as_object().ok_or("a primitive without attributes")? {
            if name == "TEXCOORD_1" {
                continue;
            }
            let (b, comps, csize, src) = raw(g, a.as_u64().ok_or("an attribute that isn't an accessor")? as usize)?;
            let el = comps * csize;
            let mut nb = Vec::with_capacity(order.len() * el);
            for &v in order {
                nb.extend_from_slice(&b[v as usize * el..v as usize * el + el]);
            }
            let mut acc = json!({"componentType": src["componentType"], "type": src["type"], "count": order.len()});
            if let Some(nz) = src.get("normalized") {
                acc["normalized"] = nz.clone();
            }
            if name == "POSITION" {
                let f: Vec<f32> = nb.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for v in f.chunks_exact(3) {
                    for i in 0..3 {
                        lo[i] = lo[i].min(v[i]);
                        hi[i] = hi[i].max(v[i]);
                    }
                }
                acc["min"] = json!(lo);
                acc["max"] = json!(hi);
            }
            attrs.insert(name.clone(), json!(push(&nb, Some(34962), acc)));
        }
        let uvb: Vec<u8> = uv.iter().flat_map(|q| q.iter().flat_map(|f| f.to_le_bytes())).collect();
        let tc1 = push(&uvb, Some(34962), json!({"componentType": 5126, "type": "VEC2", "count": uv.len()}));
        attrs.insert("TEXCOORD_1".into(), json!(tc1));
        // Bevy's standard surface reads the first UV set wherever there's a second, so a primitive with none gets the
        // atlas's as its first too (no texture of its own reads it)
        if !attrs.contains_key("TEXCOORD_0") {
            attrs.insert("TEXCOORD_0".into(), json!(tc1));
        }
        let ib: Vec<u8> = idx.iter().flat_map(|i| i.to_le_bytes()).collect();
        let ia = push(&ib, Some(34963), json!({"componentType": 5125, "type": "SCALAR", "count": idx.len()}));
        let mut np = p.clone();
        np["attributes"] = Value::Object(attrs);
        np["indices"] = json!(ia);
        meshes[pr.mesh]["primitives"][pr.prim] = np;
    }
    j["meshes"] = json!(meshes);
    if let Some(nodes) = j.get_mut("nodes").and_then(Value::as_array_mut) {
        for (n, m) in &sc.node_mesh {
            nodes[*n]["mesh"] = json!(m);
        }
    }
    j["accessors"] = json!(accessors);
    j["bufferViews"] = json!(views);
    compact(&mut j, &bin)
}

