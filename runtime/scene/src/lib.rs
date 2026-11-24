//! A composition's 3D (the data-3d elements of raea/3d, read by read3d) as raea-render rows: camera, lights, meshes,
//! glTF models and pictures in space, with every value that changes from frame to frame keyed on each frame. Rows
//! are matched across frames by identity, not by position in the tree: a part's `name` if it has one, else its
//! kind and shape (or source), counted in tree order. A part missing from some frames gets a `visible` column, so one
//! that appears later (inside a Sequence) shows in play exactly when it shows in a render. Groups compose their
//! transforms into their children's, in world space. The columns are raea-render's (runtime/scene3d/src/lib.rs).

use std::collections::BTreeMap;

use bevy::math::{Mat4, Vec3};
use raea_core_program::ir::{Program, V};
use serde_json::{Map, Value};

use raea_core_scene::*;
pub use raea_runtime_look::{camera, column, rgba_of};
use raea_runtime_look::*;

mod parts;
use parts::*;
mod models;
use models::*;
mod live;
pub use live::{LiveScene, Shot};
pub use raea_core_scene::{LIVE_GROUPS, cam_keyed, live_dynamic};
mod fast;

/// Gaussian splats' rows (<Splat>).
mod splat;
/// Baked simulations' and glitches' rows (<MeshSequence>, <Goo>, glitch props).
mod meshseq;
/// Fields of grass' rows (<Grass>).
mod grass;

/// The scene-wide parts placed by their parent alone, a camera column or a post effect each: node kind, item kind. An
/// <Atmosphere> is Bevy's procedural sky round the scene, a <Skybox> Bevy's cubemap sky on the camera.
const BY_PARENT: [(&str, &str); 19] = [("AmbientLight", "ambient"), ("Bloom", "bloom"), ("Atmosphere", "atmosphere"), ("Fog", "fog"), ("ToneMapping", "tonemapping"), ("DepthOfField", "dof"), ("Grade", "grade"), ("VolumetricFog", "volfog"), ("Environment", "environment"), ("Skybox", "skybox"),
    ("AmbientOcclusion", "ssao"), ("AntiAliasing", "aa"), ("Shadows", "shadows"), ("Look", "framelook"), ("Outline", "outline"), ("Fisheye", "fisheye"), ("LensDistortion", "distortion"), ("MotionBlur", "mblur"), ("BevyResource", "resource")];

fn walk(n: &Node3, parent: Mat4, out: &mut Vec<Item>) {
    let item = |kind: &'static str, world: Mat4, material: Option<Node3>| Item { kind, props: n.props.clone(), shape: n.shape.clone(), at: n.at.clone(), material, world };
    match &*n.kind {
        "Group" => {
            let w = parent * local(&n.props);
            // a parent entity's own components (<BevyComponent> children of <Entity transform>): an entity at its place
            // carrying them, as a bare <Entity>'s. Its children are placed in it here, not Bevy ChildOf children of it
            if n.kids.iter().any(|k| &*k.kind == "BevyComponent") {
                let mut it = item("entity", w, None);
                bevy_parts(n, &mut it);
                out.push(it);
            }
            let start = out.len();
            for k in n.kids.iter() {
                walk(k, w, out);
            }
            // a group's composition pass (`pass`) is every part's inside it that doesn't name its own
            if let Some(pass) = n.props.get("pass").filter(|v| v.is_string()) {
                for it in out[start..].iter_mut().filter(|it| matches!(it.kind, "mesh" | "model" | "panel" | "text") && !it.props.contains_key("pass")) {
                    std::sync::Arc::make_mut(&mut it.props).insert("pass".into(), pass.clone());
                }
            }
        }
        // <Refinish>: every model inside re-covered (after the model's own MeshStandardMaterial children, which name
        // theirs more closely)
        "Refinish" => {
            let start = out.len();
            for k in n.kids.iter() {
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
            bevy_parts(n, &mut it);
            out.push(it);
            // a mesh's children (three.js parents meshes to meshes) move with it
            for k in n.kids.iter() {
                walk(k, w, out);
            }
        }
        "Model" => {
            let mut it = item("model", parent * local(&n.props), None);
            bevy_parts(n, &mut it);
            // MeshToonMaterial children: toon surfaces over its glTF materials (all, or the one each names)
            let toons: Vec<Value> = n.kids.iter().filter(|k| &*k.kind == "MeshToonMaterial").map(|k| toon_spec(&k.props, true)).collect();
            if !toons.is_empty() {
                std::sync::Arc::make_mut(&mut it.props).insert("toon".into(), Value::Array(toons));
            }
            // MeshStandardMaterial children: its glTF materials (all, or the one each names) given another colour,
            // texture or finish, the rest of each kept
            // (and its <Lightmap> children, a bake on each glTF mesh one names: raea-sys-material's lightmap.rs)
            let mats: Vec<_> = n.kids.iter().filter(|k| matches!(&*k.kind, "MeshStandardMaterial" | "MeshPhysicalMaterial" | "Lightmap")).collect();
            let surfaces: Vec<Value> = mats.iter().map(|k| surface_spec(&k.props)).collect();
            if !surfaces.is_empty() {
                std::sync::Arc::make_mut(&mut it.props).insert("surfaces".into(), Value::Array(surfaces));
            }
            // their shaders (docs/shaders.feature SH2): each over the glTF material it names, or the rest
            let shaders: Vec<Value> = mats.iter().filter_map(|k| {
                let Some(Value::Object(sh)) = k.props.get("shader") else { return None };
                let mut sh = sh.clone();
                if let Some(m) = k.props.get("material") {
                    sh.insert("material".into(), m.clone());
                }
                Some(Value::Object(sh))
            }).collect();
            if !shaders.is_empty() {
                std::sync::Arc::make_mut(&mut it.props).insert("shader".into(), Value::Array(shaders));
            }
            out.push(it)
        }
        // a bare Bevy entity (<Entity>): a Transform, and its <BevyComponent> children through the TypeRegistry
        "Entity" => {
            let mut it = item("entity", parent * local(&n.props), None);
            bevy_parts(n, &mut it);
            out.push(it)
        }
        // a picture in space or a live picture on a quad (<Screen>, raea-render screen.rs: a panel with a `screen`
        // column), with its layer key (bevy_parts)
        "ImagePlane" | "Screen" => {
            let mut it = item("panel", parent * local(&n.props), None);
            bevy_parts(n, &mut it);
            out.push(it)
        }
        // a picture pressed onto whatever surface its box crosses (<Decal>: raea-render's forward decal, its +y out of the surface)
        "Decal" => out.push(item("decal", parent * local(&n.props), None)),
        // footage behind the 3D, filling the camera's view (<Plate>, <Matchmove>): not moved by groups
        "Plate" => out.push(item("plate", Mat4::IDENTITY, None)),
        "PerspectiveCamera" | "DirectionalLight" | "PointLight" | "SpotLight" | "RectAreaLight" => {
            let kind = match &*n.kind {
                "PerspectiveCamera" if n.props.get("target").is_some_and(Value::is_string) => "rendercam", // into an image (R3F23a)
                "PerspectiveCamera" => "camera",
                "DirectionalLight" => "directional",
                "PointLight" => "point",
                "RectAreaLight" => "rect",
                _ => "spot",
            };
            let mut it = item(kind, parent, None);
            bevy_parts(n, &mut it);
            out.push(it)
        }
        k if BY_PARENT.iter().any(|(n, _)| *n == k) => out.push(item(BY_PARENT.iter().find(|(n, _)| *n == k).map_or("", |&(_, i)| i), parent, None)),
        // text in the 3D world (<RichText>): a block and its glyphs (raea-render's text3d.rs)
        "RichText" => out.push(item("text", parent * local(&n.props), None)),
        "FogVolume" => out.push(item("fogvolume", parent * local(&n.props), None)),
        "Fire" => out.push(item("fire", parent * local(&n.props), None)),
        // a field of grass made by the engine from its grids (<Grass>): grass.rs
        "Grass" => out.push(item("grass", Mat4::IDENTITY, None)),
        "Splat" => out.push(item("splat", parent * local(&n.props), None)),
        // 2D over the 3D (<Sprite>, <Shape2D>): placed in composition px by their own props, not by groups
        "Sprite" => out.push(item("sprite", Mat4::IDENTITY, None)),
        "Shape2D" => out.push(item("shape2d", Mat4::IDENTITY, None)),
        "MeshSequence" => out.push(item("meshseq", parent * local(&n.props), None)),
        // one shader over every lit standard surface left (<SurfaceShader>, docs/shaders.feature SH4)
        "SurfaceShader" => out.push(item("surface_shader", parent, None)),
        // a shader over the frame (<FramePass>, docs/shaders.feature SH6)
        "FramePass" => out.push(item("frame_pass", parent, None)),
        // a compute shader run before the cameras draw (<Compute>, docs/shaders.feature SH10)
        "Compute" => out.push(item("compute", parent, None)),
        _ => {
            for k in n.kids.iter() {
                walk(k, parent, out);
            }
        }
    }
}

/// A part's SDK name as its row's `name` column: what a Bevy Entity field (a ChildOf's parent) names it by.
fn name_column(row: &mut BTreeMap<String, V>, props: &Map<String, Value>) {
    if let Some(Value::String(n)) = props.get("name") {
        row.insert("name".into(), V::S(n.clone()));
    }
}

/// The <BevyComponent> children of a part (a Mesh, a Model, a light, the camera) onto its item as `components`: Bevy's
/// own reflected components, or a compiled-in plugin's, applied by raea-render through the TypeRegistry (plugins.rs).
fn bevy_parts(n: &Node3, it: &mut Item) {
    let mut comps: Vec<Value> = n.kids.iter().filter(|k| &*k.kind == "BevyComponent").map(|k| Value::Object((*k.props).clone())).collect();
    // its source stamp as its layer key (docs/review.feature R2a), a component on the part, not a column its kind reads
    if let Some(key) = raea_sys_frame::layer::key_from_props(&n.props) {
        // RAEA_HIDE_LAYERS names it: drawn hidden, for the layer pass's oracle (bench/layers; a page element's: page_css)
        if std::env::var("RAEA_HIDE_LAYERS").is_ok_and(|h| h.split([',', '\n']).any(|k| k.trim() == key)) {
            comps.push(serde_json::json!({ "type": "bevy_camera::visibility::Visibility", "value": "Hidden" }));
        }
        comps.push(serde_json::json!({ "type": "raea_sys_frame::layer::LayerKey", "value": key }));
    }
    if !comps.is_empty() {
        std::sync::Arc::make_mut(&mut it.props).insert("components".into(), Value::Array(comps));
    }
}

/// A scene with a <Compute> whose state carries from frame to frame (a file's images, or buffers of its own), or a camera
/// into a render image (R3F23c: what it drew lasts, a ping-pong's start drawn at frame 0), so a still steps every frame before it (raea-runtime-export's scene_frames).
pub fn carries_state(scene: &[Node3]) -> bool {
    scene.iter().any(|n| (&*n.kind == "Compute" && n.props.get("shader").is_some_and(|s| s.get("file").is_some() || s.get("buffers").and_then(Value::as_object).is_some_and(|b| !b.is_empty()))) || (&*n.kind == "PerspectiveCamera" && n.props.get("target").is_some_and(Value::is_string)) || carries_state(&n.kids))
}

/// A frame's scene walked into its parts, each placed in the world.
pub fn items_of(scene: &[Node3]) -> Vec<Item> {
    let mut out = Vec::new();
    for n in scene {
        walk(n, Mat4::IDENTITY, &mut out);
    }
    // the cameras not drawing this frame (<Camera3d is_active={false}>) out, the piece's stamped with its shot (RR2)
    active_cameras(&mut out);
    // a camera's own AmbientLight (<Camera3d>'s <AmbientLight> child, Bevy 0.20's component) lights its view in place of
    // the scene's others (GlobalAmbientLight)
    let own = |i: &Item| i.kind == "ambient" && i.props.get("camera") == Some(&Value::Bool(true));
    if out.iter().any(own) { out.retain(|i| i.kind != "ambient" || own(i)); }
    raea_core_review::camera_override(&mut out); // a review's fixed camera in place of the scene's own (RAEA_CAMERA)
    out.retain(|i| raea_sys_layers::draws_part(std::env::var("RAEA_LAYER").ok().as_deref(), i.kind)); // an export's layer (LA2a)
    out
}


/// World position, rotation (Bevy's quaternion, q_x q_y q_z q_w: no Euler round trip, docs/control.feature FC1d) and
/// scale of each frame; each frame's quaternion takes the sign nearer the frame before's, so keys turn the short way.
fn placement(p: &mut Program, row: &mut BTreeMap<String, V>, id: &str, its: &[Item]) {
    let mut prev = bevy::math::Quat::IDENTITY;
    let per: Vec<[f64; 10]> = its.iter().map(|it| {
        let (s, q, t) = it.world.to_scale_rotation_translation();
        // a zero scale has no rotation (it comes out NaN, keyed on every frame since NaN never equals itself): the one before
        let q = if q.is_finite() { q } else { prev };
        prev = if q.dot(prev) < 0.0 { -q } else { q };
        [t.x as f64, t.y as f64, t.z as f64, prev.x as f64, prev.y as f64, prev.z as f64, prev.w as f64, s.x as f64, s.y as f64, s.z as f64]
    }).collect();
    for (k, name) in ["x", "y", "z", "q_x", "q_y", "q_z", "q_w"].iter().enumerate() {
        exact(p, row, id, name, &per.iter().map(|t| t[k]).collect::<Vec<_>>());
    }
    let sc: Vec<[f64; 3]> = per.iter().map(|t| [t[7], t[8], t[9]]).collect();
    if sc.iter().any(|s| s.iter().any(|v| (v - 1.0).abs() > 1e-9)) {
        for (k, name) in ["scale_x", "scale_y", "scale_z"].iter().enumerate() {
            exact(p, row, id, name, &sc.iter().map(|s| s[k]).collect::<Vec<_>>());
        }
    }
}

/// The lights, meshes, models and pictures in space, as rows, from each frame's parts.
pub fn rows(all: &[Vec<Item>], p: &mut Program) {
    let before = p.spawns.len();
    rows_into_of(all, &Tracks::of(all), p);
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
    grasses: Vec<Track>,
    splats: Vec<Track>,
    sprites: Vec<Track>,
    shapes2d: Vec<Track>,
    plates: Vec<Track>,
    meshseqs: Vec<Track>,
    entities: Vec<Track>,
    texts: Vec<Track>,
}

impl Tracks {
    fn of(all: &[Vec<Item>]) -> Tracks {
        Tracks {
            lights: tracks_of(all, &["directional", "point", "spot", "rect"]),
            // cut where a part's shape, words, file or size change (SG1): a row keeps its first frame's
            meshes: split_kept(tracks_of(all, &["mesh"])),
            models: split_kept(tracks_of(all, &["model"])),
            panels: split_kept(tracks_of(all, &["panel"])),
            decals: split_kept(tracks_of(all, &["decal"])),
            fogs: split_kept(tracks_of(all, &["fogvolume"])),
            fires: split_kept(tracks_of(all, &["fire"])),
            grasses: split_kept(tracks_of(all, &["grass"])),
            splats: split_kept(tracks_of(all, &["splat"])),
            sprites: tracks_of(all, &["sprite"]),
            shapes2d: tracks_of(all, &["shape2d"]),
            plates: split_kept(tracks_of(all, &["plate"])),
            meshseqs: split_kept(tracks_of(all, &["meshseq"])),
            entities: tracks_of(all, &["entity"]),
            texts: split_kept(tracks_of(all, &["text"])),
        }
    }
}

fn rows_into_of(all: &[Vec<Item>], tr: &Tracks, p: &mut Program) {
    let names = pass_names(all);
    resources(p, all);
    surface_shader_rows(all, p);
    ordered_shader_rows(all, p, "frame_pass");
    ordered_shader_rows(all, p, "compute");
    lights_rows(all, tr, p);
    meshes_rows(tr, &names, p);
    entities_rows(tr, p);
    texts_rows(tr, &names, p);
    models_rows(tr, &names, p);
    panels_rows(tr, &names, p);
    decals_rows(tr, p);
    plates_rows(tr, p);
    fogs_rows(tr, p);
    fires_rows(tr, p);
    // fields of grass (<Grass>): grass.rs
    grass::rows(&tr.grasses, p);
    // gaussian splats (<Splat>): splat.rs
    splat::rows(&tr.splats, p);
    // 2D over the 3D (<Sprite>, <Shape2D>): sprite2d.rs
    raea_runtime_scene2d::sprites(&tr.sprites, p);
    raea_runtime_scene2d::shapes(&tr.shapes2d, p);
    // baked simulations (<MeshSequence>, <Goo>): meshseq.rs
    meshseq::rows(&tr.meshseqs, p);
    render_cameras(all, &tracks_of(all, &["rendercam"]), p); // cameras into images (R3F23a): raea-runtime-look rendercam.rs
}
