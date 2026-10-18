//! The 3D kinds @raea/motion's 3D SDK writes (motion/docs/bevy-coverage.md), drawn with Bevy: lights of three types,
//! materials past colour, roughness and metalness, glTF models with their animation clips, the camera's
//! post-processing, and the systems that move all of it frame by frame from the rows. Every value a frame draws is a
//! column of the frame's rows (keys, rules), so a frame is a function of the rows, as everywhere in the engine.
//!
//! Columns (all optional; the defaults are what the engine drew before these existed):
//!   camera: x y z look_x look_y look_z fov near far, ambient ambient_color, bloom (intensity, 0 = none)
//!           bloom_threshold, fog_color fog_start fog_end | fog_density, tonemap ('aces' 'agx' 'tony' 'reinhard'
//!           'none'), exposure (EV100)
//!   light:  type ('directional' default, 'point', 'spot'), color, shadows (1/0);
//!           directional: lux dir_x dir_y dir_z;  point and spot: x y z lumens range radius;
//!           spot: dir_x dir_y dir_z outer inner (radians)
//!   mesh:   shape color rough metal, emissive emissive_k, opacity, unlit, clearcoat clearcoat_rough, reflectance,
//!           map (a picture for the base colour), x y z rot_x rot_y rot_z (radians, YXZ), scale | scale_x scale_y
//!           scale_z, visible, col_r col_g col_b (an animated colour, sRGB 0..1, wins over color)
//!   model:  src (a .glb or .gltf), scene (index), x y z rot_x rot_y rot_z scale | scale_x.., visible, anim (a clip's
//!           name, or its index as a number), anim_t (seconds into the clip), anim_loop (1 = wrap, 0 = hold the end)

use super::*;
use bevy::animation::{AnimationPlayer, graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex}};
use bevy::gltf::Gltf;
use bevy::world_serialization::WorldAssetRoot;

/// A model row's glTF: loaded by the asset server, then its scene spawned under the row's entity and its clip set up.
#[derive(Component)]
pub(super) struct Model3d {
    gltf: Handle<Gltf>,
    scene: usize,
    anim: Option<V>,
    state: ModelState,
    src: String,
}

#[derive(PartialEq)]
enum ModelState {
    Loading,
    Spawned(u32),
    Ready { player: Option<(Entity, AnimationNodeIndex, Handle<bevy::animation::AnimationClip>)> },
}

fn flag(vals: &BTreeMap<String, V>, k: &str, d: bool) -> bool {
    vals.get(k).map(V::truthy).unwrap_or(d)
}

fn scale_of(vals: &BTreeMap<String, V>) -> Vec3 {
    let s = num(vals, "scale", 1.0);
    Vec3::new(num(vals, "scale_x", s), num(vals, "scale_y", s), num(vals, "scale_z", s))
}

fn rot_of(vals: &BTreeMap<String, V>) -> Quat {
    Quat::from_euler(EulerRot::YXZ, num(vals, "rot_y", 0.0), num(vals, "rot_x", 0.0), num(vals, "rot_z", 0.0))
}

/// A mesh or model row's transform.
pub(super) fn transform_of(vals: &BTreeMap<String, V>) -> Transform {
    Transform::from_xyz(num(vals, "x", 0.0), num(vals, "y", 0.0), num(vals, "z", 0.0)).with_rotation(rot_of(vals)).with_scale(scale_of(vals))
}

/// Texture coordinates for a Manifold mesh (it has none): each vertex projected along its face's main axis onto the
/// mesh's bounds, so a picture map covers each side of a box once, the right way round seen from outside.
pub(super) fn box_uvs(pos: &[[f32; 3]], nor: &[[f32; 3]]) -> Vec<[f32; 2]> {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = |k: usize| (hi[k] - lo[k]).max(1e-6);
    let a = |p: &[f32; 3], k: usize| (p[k] - lo[k]) / span(k);
    let b = |p: &[f32; 3], k: usize| (hi[k] - p[k]) / span(k);
    pos.iter().zip(nor).map(|(p, n)| {
        let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
        if ax >= ay && ax >= az {
            if n[0] > 0.0 { [b(p, 2), b(p, 1)] } else { [a(p, 2), b(p, 1)] }
        } else if ay >= az {
            if n[1] > 0.0 { [a(p, 0), a(p, 2)] } else { [a(p, 0), b(p, 2)] }
        } else if n[2] > 0.0 {
            [a(p, 0), b(p, 1)]
        } else {
            [b(p, 0), b(p, 1)]
        }
    }).collect()
}

/// A picture file as a texture (sRGB, mipmapped by the GPU's sampler only: linear, anisotropic).
fn picture(world: &mut World, path: &Path) -> Result<Handle<Image>, String> {
    let pic = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?.to_rgba8();
    let (w, h) = pic.dimensions();
    let mut img = Image::new(
        bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        pic.into_raw(),
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    img.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor { anisotropy_clamp: 16, ..bevy::image::ImageSamplerDescriptor::linear() });
    Ok(world.resource_mut::<Assets<Image>>().add(img))
}

/// A mesh row's surface: the standard material from its columns.
pub(super) fn material(world: &mut World, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<StandardMaterial, String> {
    let vals = &t.vals;
    let g = |k: &str, d: f32| num(vals, k, d);
    let mut base = colour(vals.get("color"), [0.8, 0.8, 0.8, 1.0]);
    if vals.contains_key("col_r") {
        base = Color::srgb(g("col_r", 0.8), g("col_g", 0.8), g("col_b", 0.8));
    }
    let opacity = g("opacity", 1.0).clamp(0.0, 1.0);
    base.set_alpha(opacity);
    let em = colour(vals.get("emissive"), [0.0, 0.0, 0.0, 1.0]).to_linear() * g("emissive_k", 1.0);
    let mut m = StandardMaterial {
        base_color: base,
        perceptual_roughness: g("rough", 0.45),
        metallic: g("metal", 0.0),
        emissive: LinearRgba::rgb(em.red, em.green, em.blue),
        unlit: flag(vals, "unlit", false),
        clearcoat: g("clearcoat", 0.0),
        clearcoat_perceptual_roughness: g("clearcoat_rough", 0.5),
        reflectance: g("reflectance", 0.5),
        // see-through only when asked (an opacity column, even at 1, may animate to less)
        alpha_mode: if vals.contains_key("opacity") { AlphaMode::Blend } else { AlphaMode::Opaque },
        ..default()
    };
    if let Some(V::S(src)) = vals.get("map").filter(|v| v.truthy()) {
        let path = resolve_src(inserted, src, dir).map_err(|e| format!("mesh {}: map: {e}", t.id))?;
        m.base_color_texture = Some(picture(world, &path).map_err(|e| format!("mesh {}: map: {e}", t.id))?);
    }
    Ok(m)
}

/// A light row: directional (the default, as before), point or spot.
pub(super) fn light(world: &mut World, e: Entity, vals: &BTreeMap<String, V>) {
    let g = |k: &str, d: f32| num(vals, k, d);
    let color = colour(vals.get("color"), [1.0, 1.0, 1.0, 1.0]);
    let kind = match vals.get("type") { Some(V::S(s)) => s.as_str(), _ => "directional" };
    let pos = Vec3::new(g("x", 0.0), g("y", 0.0), g("z", 0.0));
    let dir = Vec3::new(g("dir_x", -0.5), g("dir_y", -1.0), g("dir_z", -0.4));
    match kind {
        "point" => {
            world.entity_mut(e).insert((
                PointLight { color, intensity: g("lumens", 1.0e6), range: g("range", 20.0), radius: g("radius", 0.0), shadow_maps_enabled: flag(vals, "shadows", false) && !preview(), ..default() },
                Transform::from_translation(pos),
            ));
        }
        "spot" => {
            let outer = g("outer", std::f32::consts::FRAC_PI_4);
            world.entity_mut(e).insert((
                SpotLight { color, intensity: g("lumens", 1.0e6), range: g("range", 20.0), radius: g("radius", 0.0), outer_angle: outer, inner_angle: g("inner", 0.0).min(outer), shadow_maps_enabled: flag(vals, "shadows", false) && !preview(), ..default() },
                Transform::from_translation(pos).looking_to(dir, up_for(dir)),
            ));
        }
        _ => {
            world.entity_mut(e).insert((
                DirectionalLight { color, illuminance: g("lux", 8000.0), shadow_maps_enabled: flag(vals, "shadows", true) && !preview(), ..default() },
                Transform::default().looking_to(dir, up_for(dir)),
            ));
        }
    }
}

/// An up vector that isn't along `dir` (a light pointing straight down).
fn up_for(dir: Vec3) -> Vec3 {
    if dir.normalize_or_zero().cross(Vec3::Y).length_squared() < 1e-6 { Vec3::Z } else { Vec3::Y }
}

/// The camera row's lens and post-processing.
pub(super) fn camera(world: &mut World, e: Entity, vals: &BTreeMap<String, V>) {
    let g = |k: &str, d: f32| num(vals, k, d);
    if vals.contains_key("near") || vals.contains_key("far") {
        let fov = g("fov", 45.0).to_radians();
        world.entity_mut(e).insert(Projection::Perspective(PerspectiveProjection { fov, near: g("near", 0.1), far: g("far", 1000.0), ..default() }));
    }
    let bloom = g("bloom", 0.0);
    if bloom > 0.0 {
        let mut b = bevy::post_process::bloom::Bloom::NATURAL;
        b.intensity = bloom;
        if let Some(th) = vals.get("bloom_threshold").map(V::num) {
            b.prefilter.threshold = th as f32;
            b.prefilter.threshold_softness = 0.2;
        }
        world.entity_mut(e).insert(b);
    }
    if vals.contains_key("fog_color") || vals.contains_key("fog_end") || vals.contains_key("fog_density") {
        let falloff = match vals.get("fog_density") {
            Some(d) => FogFalloff::Exponential { density: d.num() as f32 },
            None => FogFalloff::Linear { start: g("fog_start", 5.0), end: g("fog_end", 20.0) },
        };
        world.entity_mut(e).insert(DistanceFog { color: colour(vals.get("fog_color"), [0.5, 0.5, 0.55, 1.0]), falloff, ..default() });
    }
    if let Some(V::S(tm)) = vals.get("tonemap") {
        use bevy::core_pipeline::tonemapping::Tonemapping as T;
        let t = match tm.as_str() {
            "none" => T::None,
            "reinhard" => T::Reinhard,
            "aces" => T::AcesFitted,
            "agx" => T::AgX,
            "neutral" | "blender" => T::BlenderFilmic,
            _ => T::TonyMcMapface,
        };
        world.entity_mut(e).insert(t);
    }
    if let Some(ev) = vals.get("exposure") {
        world.entity_mut(e).insert(bevy::camera::Exposure { ev100: ev.num() as f32 });
    }
}

/// A model row: its glTF asked for (the engine waits for it before drawing, `await_models`).
pub(super) fn model(world: &mut World, e: Entity, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<(), String> {
    let src = match t.vals.get("src") { Some(V::S(s)) => s.clone(), _ => return Err(format!("model {}: needs src (a .glb or .gltf file)", t.id)) };
    let path = resolve_src(inserted, &src, dir)?;
    if !path.exists() {
        return Err(format!("model {}: {} doesn't exist", t.id, path.display()));
    }
    let gltf: Handle<Gltf> = world.resource::<AssetServer>().load(path.to_string_lossy().to_string());
    let anim = t.vals.get("anim").cloned().filter(|v| v.truthy() || matches!(v, V::N(_)));
    world.entity_mut(e).insert((Model3d { gltf, scene: num(&t.vals, "scene", 0.0) as usize, anim, state: ModelState::Loading, src }, transform_of(&t.vals), Visibility::default()));
    Ok(())
}

/// Models along: loaded, then their scene spawned, then (a few updates later, once the scene's entities exist) their
/// clip found and set playing, paused: `sync_models` seeks it to each frame's time.
pub(super) fn model_setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut models: Query<(Entity, &mut Model3d)>,
    kids: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    mut err: ResMut<Error>,
) {
    for (e, mut m) in &mut models {
        match m.state {
            ModelState::Loading => {
                if let Some(bevy::asset::LoadState::Failed(why)) = server.get_load_state(&m.gltf) {
                    err.0 = Some(format!("model {}: {why}", m.src));
                    m.state = ModelState::Ready { player: None };
                    continue;
                }
                if !server.is_loaded_with_dependencies(&m.gltf) {
                    continue;
                }
                let Some(g) = gltfs.get(&m.gltf) else { continue };
                let Some(scene) = g.scenes.get(m.scene).or(g.default_scene.as_ref()).cloned() else {
                    err.0 = Some(format!("model {}: has no scene {}", m.src, m.scene));
                    m.state = ModelState::Ready { player: None };
                    continue;
                };
                let child = commands.spawn((WorldAssetRoot(scene), Transform::default(), Visibility::Inherited)).id();
                commands.entity(e).add_child(child);
                m.state = ModelState::Spawned(0);
            }
            ModelState::Spawned(n) => {
                // the scene's entities, then the player among them (the glTF puts one on the animated root)
                let found = descendants(e, &kids).into_iter().find(|d| players.contains(*d));
                let Some(anim) = m.anim.clone() else {
                    if n > 2 {
                        m.state = ModelState::Ready { player: None };
                    } else {
                        m.state = ModelState::Spawned(n + 1);
                    }
                    continue;
                };
                let Some(p) = found else {
                    if n > 120 {
                        err.0 = Some(format!("model {}: asked for clip {anim:?}, but it has no animated parts", m.src));
                        m.state = ModelState::Ready { player: None };
                    } else {
                        m.state = ModelState::Spawned(n + 1);
                    }
                    continue;
                };
                let Some(g) = gltfs.get(&m.gltf) else { continue };
                let clip = match &anim {
                    V::S(name) => g.named_animations.get(name.as_str()).cloned().ok_or_else(|| {
                        let mut names: Vec<&str> = g.named_animations.keys().map(|k| k.as_ref()).collect();
                        names.sort();
                        format!("model {}: no clip \"{name}\" (it has {})", m.src, names.join(", "))
                    }),
                    V::N(i) => g.animations.get(*i as usize).cloned().ok_or_else(|| format!("model {}: no clip {i} (it has {})", m.src, g.animations.len())),
                };
                let clip = match clip {
                    Ok(c) => c,
                    Err(why) => {
                        err.0 = Some(why);
                        m.state = ModelState::Ready { player: None };
                        continue;
                    }
                };
                let (graph, node) = AnimationGraph::from_clip(clip.clone());
                commands.entity(p).insert(AnimationGraphHandle(graphs.add(graph)));
                if let Ok(mut pl) = players.get_mut(p) {
                    pl.play(node).repeat().pause();
                }
                m.state = ModelState::Ready { player: Some((p, node, clip)) };
            }
            ModelState::Ready { .. } => {}
        }
    }
}

fn descendants(e: Entity, kids: &Query<&Children>) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack = vec![e];
    while let Some(x) = stack.pop() {
        if let Ok(c) = kids.get(x) {
            for k in c.iter() {
                out.push(k);
                stack.push(k);
            }
        }
    }
    out
}

/// Every model ready (loaded, spawned, its clip set up) before the first frame is drawn: a frame never shows a model
/// half-loaded. At most 60 s.
pub(super) fn await_models(app: &mut App) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    loop {
        let world = app.world_mut();
        if let Some(e) = world.resource_mut::<Error>().0.take() {
            return Err(e);
        }
        let mut q = world.query::<&Model3d>();
        let waiting = q.iter(world).filter(|m| !matches!(m.state, ModelState::Ready { .. })).count();
        if waiting == 0 {
            return Ok(());
        }
        if t0.elapsed().as_secs_f64() > 60.0 {
            return Err(format!("{waiting} model(s) didn't load in 60 s"));
        }
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Meshes and models where the frame's rows put them: transform, whether they show, and the material values a row
/// animates. Only columns a row has are applied, so rows written before these columns draw as they did.
pub(super) fn sync_meshes(frame: Res<FrameRows>, mut mats: ResMut<Assets<StandardMaterial>>, mut q: Query<(Entity, &mut Transform, &mut Visibility, Option<&MeshMaterial3d<StandardMaterial>>), Or<(With<Mesh3d>, With<Model3d>)>>) {
    for (e, mut t, mut vis, mat) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        let nt = transform_of(v);
        if *t != nt {
            *t = nt;
        }
        if let Some(on) = v.get("visible") {
            let want = if on.truthy() { Visibility::Inherited } else { Visibility::Hidden };
            if *vis != want {
                *vis = want;
            }
        }
        let Some(h) = mat else { continue };
        let animated = ["col_r", "opacity", "emissive_k", "rough", "metal"].iter().any(|k| v.contains_key(*k));
        if !animated {
            continue;
        }
        let Some(cur) = mats.get(&h.0) else { continue };
        let mut base = cur.base_color;
        if v.contains_key("col_r") {
            base = Color::srgb(num(v, "col_r", 0.8), num(v, "col_g", 0.8), num(v, "col_b", 0.8));
        }
        if v.contains_key("opacity") {
            base.set_alpha(num(v, "opacity", 1.0).clamp(0.0, 1.0));
        } else {
            base.set_alpha(cur.base_color.alpha());
        }
        let rough = if v.contains_key("rough") { num(v, "rough", 0.45) } else { cur.perceptual_roughness };
        let metal = if v.contains_key("metal") { num(v, "metal", 0.0) } else { cur.metallic };
        let emissive = if v.contains_key("emissive_k") {
            let em = colour(v.get("emissive"), [0.0, 0.0, 0.0, 1.0]).to_linear() * num(v, "emissive_k", 1.0);
            LinearRgba::rgb(em.red, em.green, em.blue)
        } else {
            cur.emissive
        };
        if cur.base_color != base || cur.perceptual_roughness != rough || cur.metallic != metal || cur.emissive != emissive {
            if let Some(mut m) = mats.get_mut(&h.0) {
                m.base_color = base;
                m.perceptual_roughness = rough;
                m.metallic = metal;
                m.emissive = emissive;
            }
        }
    }
}

/// The camera where the frame's row puts it, looking where it says; its field of view and ambient light.
pub(super) fn sync_camera(frame: Res<FrameRows>, mut q: Query<(Entity, &mut Transform, Option<&mut Projection>, Option<&mut AmbientLight>), With<Camera3d>>) {
    for (e, mut t, proj, amb) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        let pos = Vec3::new(num(v, "x", 0.0), num(v, "y", 2.0), num(v, "z", 9.0));
        let look = Vec3::new(num(v, "look_x", 0.0), num(v, "look_y", 1.0), num(v, "look_z", 0.0));
        if (look - pos).length_squared() > 1e-12 {
            let nt = Transform::from_translation(pos).looking_at(look, up_for(look - pos));
            if *t != nt {
                *t = nt;
            }
        }
        if let (Some(fov), Some(mut p)) = (v.get("fov"), proj) {
            if let Projection::Perspective(pp) = p.as_mut() {
                let f = (fov.num() as f32).to_radians();
                if pp.fov != f {
                    pp.fov = f;
                }
            }
        }
        if let (Some(b), Some(mut a)) = (v.get("ambient"), amb) {
            if a.brightness != b.num() as f32 {
                a.brightness = b.num() as f32;
            }
        }
    }
}

/// Lights where the frame's rows put them, as bright as they say.
pub(super) fn sync_lights(frame: Res<FrameRows>, mut q: Query<(Entity, &mut Transform, Option<&mut DirectionalLight>, Option<&mut PointLight>, Option<&mut SpotLight>)>) {
    for (e, mut t, d, p, s) in &mut q {
        if d.is_none() && p.is_none() && s.is_none() {
            continue;
        }
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        let pos = Vec3::new(num(v, "x", 0.0), num(v, "y", 0.0), num(v, "z", 0.0));
        let dir = Vec3::new(num(v, "dir_x", -0.5), num(v, "dir_y", -1.0), num(v, "dir_z", -0.4));
        let nt = if p.is_some() {
            Transform::from_translation(pos)
        } else if s.is_some() {
            Transform::from_translation(pos).looking_to(dir, up_for(dir))
        } else {
            Transform::default().looking_to(dir, up_for(dir))
        };
        if *t != nt {
            *t = nt;
        }
        if let Some(mut d) = d {
            let lux = num(v, "lux", 8000.0);
            if d.illuminance != lux {
                d.illuminance = lux;
            }
        }
        if let Some(mut p) = p {
            let lm = num(v, "lumens", 1.0e6);
            if p.intensity != lm {
                p.intensity = lm;
            }
        }
        if let Some(mut s) = s {
            let lm = num(v, "lumens", 1.0e6);
            if s.intensity != lm {
                s.intensity = lm;
            }
        }
    }
}

/// Each model's clip at the frame's time (anim_t seconds): seeked, never advanced by the clock, so the pose is a
/// function of the frame.
pub(super) fn sync_models(frame: Res<FrameRows>, clips: Res<Assets<bevy::animation::AnimationClip>>, models: Query<(Entity, &Model3d)>, mut players: Query<&mut AnimationPlayer>) {
    for (e, m) in &models {
        let ModelState::Ready { player: Some((p, node, ref clip)) } = m.state else { continue };
        let Some(v) = frame.vals(e) else { continue };
        let mut at = num(&v, "anim_t", 0.0).max(0.0);
        // looped (anim_loop, the default) or held on its last pose
        let len = clips.get(clip).map(|c| c.duration()).unwrap_or(0.0);
        if len > 0.0 {
            at = if flag(&v, "anim_loop", true) { at % len } else { at.min(len) };
        }
        if let Ok(mut pl) = players.get_mut(p) {
            if let Some(a) = pl.animation_mut(node) {
                if a.seek_time() != at || !a.is_paused() {
                    a.seek_to(at).pause();
                }
            }
        }
    }
}
