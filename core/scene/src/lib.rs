//! A composition's 3D as data: the elements read from its React tree (Node3), each part placed in the world (Item),
//! parts matched across frames by identity (tracks), the props they carry, and the files they name. The scene's rows,
//! its contact and its light are all read from these.

use std::collections::BTreeMap;

use bevy::math::Mat4;
use serde_json::{Map, Value};

mod kept;
mod media;
mod parts;
mod props;
pub use kept::*;
pub use media::*;
pub use parts::*;
pub use props::*;

/// A 3D element and its children.
#[derive(Clone, Debug)]
pub struct Node3 {
    /// kind and shape shared too, kept per part with its props (ZeroHeap: each was copied into a String a frame)
    pub kind: std::sync::Arc<str>,
    /// shared: a part's props are parsed once per change of its data-props and handed to each frame's scene, walk and
    /// rows by reference (ZeroHeap: a live 3D frame parsed and copied every part's props)
    pub props: std::sync::Arc<Map<String, Value>>,
    pub shape: Option<std::sync::Arc<str>>,
    /// an <Instances>' placements, its data-at text (9 numbers a copy, JSON): shared across frames while it's the same
    /// text, outside props, so a set whose other props change each frame keeps one list, not one a frame (#84)
    pub at: Option<std::sync::Arc<str>>,
    /// shared: a subtree no commit touched is the last frame's, handed on for an Arc's count (docs/control.feature
    /// FC1b: many_cubes' 10,000 parts were re-read each frame for a camera's move). Build from a Vec with `.into()`
    pub kids: std::sync::Arc<[Node3]>,
}

/// A part of a frame's 3D, placed: its kind, props and shape, a material child, and its world transform.
#[derive(Clone)]
pub struct Item {
    pub kind: &'static str,
    pub props: std::sync::Arc<Map<String, Value>>,
    pub shape: Option<std::sync::Arc<str>>,
    /// its Node3's `at`
    pub at: Option<std::sync::Arc<str>>,
    pub material: Option<Node3>,
    pub world: Mat4,
}

/// What a part is called across frames: its name, else what it draws, counted in tree order.
pub fn base_ident(it: &Item) -> &str {
    if let Some(Value::String(n)) = it.props.get("name") {
        return n;
    }
    match it.kind {
        "mesh" => it.shape.as_deref().unwrap_or_default(),
        "model" | "panel" | "plate" | "splat" | "meshseq" => it.props.get("src").and_then(Value::as_str).unwrap_or(""),
        "text" => it.props.get("text").and_then(Value::as_str).unwrap_or(""),
        // a camera drawing into an image (R3F23a): by the image, so two passes into one image a frame are #0 and #1
        "rendercam" => it.props.get("target").and_then(Value::as_str).unwrap_or(""),
        _ => "",
    }
}

/// A mesh's surface as part of what it is across frames: its material's kind and its shader's source, hashed (not the
/// uniforms, which animate). A mesh row takes its surface from its track's first frame, so a later shot's mesh of the
/// same shape on an earlier shot's track drew the earlier shot's shader (a set's street in its own colours, the
/// ShaderExtension that reads them lost: gambit previz v4, act 3); two shaders are now two parts.
fn surface_ident(it: &Item) -> String {
    use std::hash::{Hash, Hasher};
    let Some(m) = it.material.as_ref().filter(|_| it.kind == "mesh") else { return String::new() };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut any = false;
    if let Some(sh) = m.props.get("shader").and_then(Value::as_object) {
        for k in ["wgsl", "wesl", "fragment"] {
            if let Some(s) = sh.get(k).and_then(Value::as_str) {
                (k, s).hash(&mut h);
                any = true;
            }
        }
    }
    for k in ["vertexShader", "fragmentShader"] {
        if let Some(s) = m.props.get(k).and_then(Value::as_str) {
            (k, s).hash(&mut h);
            any = true;
        }
    }
    match (&*m.kind, any) {
        ("MeshStandardMaterial", false) => String::new(),
        (kind, false) => format!("|{kind}"),
        (kind, true) => format!("|{kind}:{:016x}", h.finish()),
    }
}

/// A part's track over only the frames it's in: from its first to its last, None where it's out between.
#[derive(Clone)]
pub struct Span {
    pub first: usize,
    pub items: Vec<Option<Item>>,
}

impl Span {
    /// Its item at frame `k`, when it's in that frame.
    pub fn at(&self, k: usize) -> Option<&Item> {
        self.items.get(k.checked_sub(self.first)?)?.as_ref()
    }
}

/// `tracks_of`'s tracks as spans: each part's slots from its first frame to its last, not one for every frame of the
/// render. A long film's parts each live in a scene; a slot a frame for every one of them made the clipping check of a
/// 13,175-frame film ask for 28 GB at its end (docs/stage-guards.feature SG4a).
pub fn spans_of(all: &[Vec<Item>], kinds: &[&str]) -> Vec<(String, Span)> {
    let mut order: Vec<String> = Vec::new();
    let mut by: BTreeMap<String, Span> = BTreeMap::new();
    for (i, its) in all.iter().enumerate() {
        for (id, it) in ids_of(its, kinds) {
            let span = by.entry(id.clone()).or_insert_with(|| {
                order.push(id.clone());
                Span { first: i, items: Vec::new() }
            });
            span.items.resize(i - span.first, None);
            span.items.push(Some(it.clone()));
        }
    }
    order.into_iter().map(|id| {
        let s = by.remove(&id).unwrap();
        (id, s)
    }).collect()
}

/// A frame's parts of these kinds, each with its id as spans_of names it across frames: its kind, what it is and draws,
/// and which of the frame's parts alike it is (#0, #1, ...). A check taking frames as they come (raea-sys-contact's
/// Clipping) follows a part from frame to frame by it.
pub fn ids_of<'a>(its: &'a [Item], kinds: &[&str]) -> Vec<(String, &'a Item)> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    its.iter()
        .filter(|it| kinds.contains(&it.kind))
        .map(|it| {
            let base = format!("{}|{}{}", it.kind, base_ident(it), surface_ident(it));
            let k = seen.entry(base.clone()).or_insert(0);
            let id = format!("{base}#{k}");
            *k += 1;
            (id, it)
        })
        .collect()
}

/// `tracks` over items already walked: only the parts of these kinds are copied, a slot for every frame
pub fn tracks_of(all: &[Vec<Item>], kinds: &[&str]) -> Vec<(String, Vec<Option<Item>>)> {
    spans_of(all, kinds)
        .into_iter()
        .map(|(id, s)| {
            let mut v = vec![None; s.first];
            v.extend(s.items);
            v.resize(all.len(), None);
            (id, v)
        })
        .collect()
}

/// Each frame's item, a missing frame holding the nearest present one (a hidden part stays where it was).
pub fn filled(track: &[Option<Item>]) -> Vec<Item> {
    let first = track.iter().flatten().next().cloned().expect("a track has a frame");
    let mut last = first;
    track.iter().map(|t| {
        if let Some(it) = t {
            last = it.clone();
        }
        last.clone()
    }).collect()
}

/// An <Entities>' scene (a world's, already in world space) as the meshes it holds, each with its material.
pub fn entities_nodes(props: &Map<String, Value>) -> Vec<Node3> {
    let nodes = props.get("scene").and_then(Value::as_array).into_iter().flatten().filter(|n| n["kind"] == "Mesh");
    nodes
        .map(|n| {
            let mesh = n["mesh"].as_object().cloned().unwrap_or_default();
            let p: Map<String, Value> = ["position", "rotation", "scale"].iter().map(|k| (k.to_string(), n[*k].clone())).collect();
            let mat: Map<String, Value> = ["color", "roughness", "metalness"].iter().filter_map(|k| Some((k.to_string(), mesh.get(*k)?.clone()))).collect();
            let material = Node3 { kind: "MeshStandardMaterial".into(), props: std::sync::Arc::new(mat), shape: None, at: None, kids: Vec::new().into() };
            Node3 { kind: "Mesh".into(), props: std::sync::Arc::new(p), shape: mesh.get("shape").and_then(Value::as_str).map(Into::into), at: None, kids: std::sync::Arc::new([material]) }
        })
        .collect()
}

#[cfg(test)]
mod surface_tracks {
    use super::*;

    fn mesh(shape: &str, material: &str, shader: Option<(&str, f64)>) -> Item {
        let mut props = Map::new();
        if let Some((src, k)) = shader {
            props.insert("shader".into(), serde_json::json!({ "wgsl": src, "uniforms": { "k": k } }));
        }
        let material = Node3 { kind: material.into(), props: std::sync::Arc::new(props), shape: None, at: None, kids: Vec::new().into() };
        Item { kind: "mesh", props: std::sync::Arc::new(Map::new()), shape: Some(shape.into()), at: None, material: Some(material), world: Mat4::IDENTITY }
    }

    #[test]
    fn one_shape_under_two_shaders_is_two_parts() {
        // a shot's box under shader A, then the next shot's box of the same shape under shader B
        let a = mesh("box", "MeshStandardMaterial", Some(("fn surface(in: VertexOutput, pbr: ptr<function, PbrInput>) { }", 1.0)));
        let b = mesh("box", "MeshStandardMaterial", Some(("fn surface(in: VertexOutput, pbr: ptr<function, PbrInput>) { (*pbr).material.base_color = in.color; }", 1.0)));
        let t = tracks_of(&[vec![a], vec![b]], &["mesh"]);
        assert_eq!(t.len(), 2, "{:?}", t.iter().map(|x| &x.0).collect::<Vec<_>>());
    }

    #[test]
    fn a_span_covers_only_the_frames_a_part_is_in() {
        let m = || mesh("box", "MeshStandardMaterial", None);
        // in frames 2 and 4 of 6, out in 3
        let all = vec![vec![], vec![], vec![m()], vec![], vec![m()], vec![]];
        let s = &spans_of(&all, &["mesh"])[0].1;
        assert_eq!((s.first, s.items.len()), (2, 3));
        assert!(s.at(1).is_none() && s.at(2).is_some() && s.at(3).is_none() && s.at(4).is_some() && s.at(5).is_none());
        // the dense tracks are the same parts, a slot a frame
        let t = &tracks_of(&all, &["mesh"])[0].1;
        assert_eq!(t.iter().map(Option::is_some).collect::<Vec<_>>(), [false, false, true, false, true, false]);
    }

    #[test]
    fn a_shader_whose_uniforms_move_stays_one_part() {
        let src = "fn surface(in: VertexOutput, pbr: ptr<function, PbrInput>) { }";
        let t = tracks_of(&[vec![mesh("box", "MeshStandardMaterial", Some((src, 0.0)))], vec![mesh("box", "MeshStandardMaterial", Some((src, 1.0)))]], &["mesh"]);
        assert_eq!(t.len(), 1);
        assert!(t[0].1.iter().all(Option::is_some));
    }

    #[test]
    fn plain_standard_meshes_keep_their_old_identity() {
        // no shader: the key is what it was before, so a scene without shaders matches its parts as it did
        let t = tracks_of(&[vec![mesh("box", "MeshStandardMaterial", None)], vec![mesh("box", "MeshStandardMaterial", None)]], &["mesh"]);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].0, "mesh|box#0");
    }
}
