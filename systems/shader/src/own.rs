//! Your own Material (docs/control.feature FC2a, Bevy's shader/shader_material): raea/pbr's `<Material>` is Bevy's
//! `impl Material`, not an extension of StandardMaterial. Its fragment shader (and vertex shader, when it names one) is
//! the author's file compiled as written but for the numbers of its `@group(MATERIAL_BIND_GROUP)` bindings, which go to
//! this one Rust type's fixed slots in the order the source declares them (ui_bind.rs, as raea's UiMaterial does):
//! uniforms of any type up to 256 bytes at 0 to 7, packed in their declared types (layout.rs), textures at 8, 10, 12,
//! 14 and their samplers at 9, 11, 13, 15, each past ui_bind's OWN_BASE (a sprite's own bindings are below it). A
//! Rust Material's bind group is its type's, so raea's one type takes every author's layout this way. Its alpha mode,
//! depth bias and faces are the material's, as Bevy's methods give them.
//!
//! A file's `import super::NAME` reads NAME.wesl beside it: the file is registered as embedded://raea_sys_shader/
//! d{folder}/m{hash}.wesl and each module it imports from its folder as embedded://raea_sys_shader/d{folder}/NAME.wesl,
//! which is the module path WESL resolves `super::NAME` to.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bevy::asset::uuid_handle;
use bevy::mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::{Shader, ShaderRef};
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin, MaterialExtension2d, SpriteMaterialPlugin};
use serde_json::Value;

use crate::own_live::{storage, Packing};
use crate::ui_bind::{rebind_in, Rebound};
use crate::{layout, register, Inputs, OwnSurface, Shaded};

/// The fragment shader every pipeline starts from, swapped in `specialize` for the material's own (its uuid).
const PLACEHOLDER: Handle<Shader> = uuid_handle!("5d1e7a3c-2b9f-4e6d-8a1c-3f7b9d2e6c41");
const STUB: &str = "import bevy_pbr::render::forward_io::VertexOutput;\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> { return vec4(1.0, 0.0, 1.0, 1.0); }\n";
/// The same for a 2D one (Material2d's), whose vertex output is mesh2d's.
const PLACEHOLDER_2D: Handle<Shader> = uuid_handle!("5d1e7a3c-2b9f-4e6d-8a1c-3f7b9d2e6c42");
const STUB_2D: &str = "import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> { return vec4(1.0, 0.0, 1.0, 1.0); }\n";

/// How a source may write the material's group: 3 in 3D, 2 in 2D.
pub(crate) const GROUPS: &[&str] = &["constants::MATERIAL_BIND_GROUP", "#{MATERIAL_BIND_GROUP}", "3"];
const GROUPS_2D: &[&str] = &["constants::MATERIAL_BIND_GROUP", "#{MATERIAL_BIND_GROUP}", "2"];

/// One Rust type for every author's Material: eight uniform buffers of 256 bytes, four textures with their samplers,
/// the shaders that read them, and how it blends and culls.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
#[bind_group_data(OwnKey)]
pub struct OwnMaterial {
    #[uniform(30)]
    u0: [Vec4; 16],
    #[uniform(31)]
    u1: [Vec4; 16],
    #[uniform(32)]
    u2: [Vec4; 16],
    #[uniform(33)]
    u3: [Vec4; 16],
    #[uniform(34)]
    u4: [Vec4; 16],
    #[uniform(35)]
    u5: [Vec4; 16],
    #[uniform(36)]
    u6: [Vec4; 16],
    #[uniform(37)]
    u7: [Vec4; 16],
    #[texture(38)]
    #[sampler(39)]
    t0: Option<Handle<Image>>,
    #[texture(40)]
    #[sampler(41)]
    t1: Option<Handle<Image>>,
    #[texture(42)]
    #[sampler(43)]
    t2: Option<Handle<Image>>,
    #[texture(44)]
    #[sampler(45)]
    t3: Option<Handle<Image>>,
    // one texture of each other dimension (ui_bind.rs DIMS), the 1d's and 2d_array's samplers the 5th and 6th
    #[texture(46, dimension = "1d")]
    #[sampler(47)]
    d1: Option<Handle<Image>>,
    #[texture(48, dimension = "2d_array")]
    #[sampler(49)]
    d2a: Option<Handle<Image>>,
    #[texture(50, dimension = "cube")]
    cube: Option<Handle<Image>>,
    #[texture(51, dimension = "cube_array")]
    cube_array: Option<Handle<Image>>,
    #[texture(52, dimension = "3d")]
    d3: Option<Handle<Image>>,
    // its read-only storage buffers (ui_bind.rs STORAGE); one it doesn't bind holds 16 zero bytes
    #[storage(53, read_only)]
    b0: Handle<ShaderBuffer>,
    #[storage(54, read_only)]
    b1: Handle<ShaderBuffer>,
    /// what its uniforms and buffers are packed from, for a frame's new values (own_live.rs)
    pub(crate) packing: Arc<Packing>,
    shader: u128,
    /// the vertex shader's uuid, 0 for Bevy's mesh vertex shader
    vertex: u128,
    /// None when the spec gives none: Opaque as a Material's, a sprite's own as a SpriteMaterial's
    alpha: Option<AlphaMode>,
    depth_bias: f32,
    /// the faces it culls (the pipeline's primitive.cull_mode); None keeps the pipeline's (3D culls Back, 2D none)
    cull: Option<Option<Face>>,
    /// its fragment stage's pipeline-overridable constants (WGSL `override`): their hash in CONSTANTS, 0 for none
    constants: u64,
    /// its vertex buffer's attributes and their shader locations (Material::specialize's get_layout): their hash in
    /// LAYOUTS, 0 for the pipeline's own
    layout: u64,
}

/// Each Material's constants by their hash: its key holds the hash, as a key is Copy (SpriteMaterial's combined key,
/// a packed struct, needs it).
static CONSTANTS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<u64, Vec<(String, f64)>>>> = std::sync::LazyLock::new(Default::default);

/// Constants kept in CONSTANTS under their hash (0 for none).
/// Each Material's vertex buffer layout by its hash, as CONSTANTS.
static LAYOUTS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<u64, Vec<(MeshVertexAttribute, u32)>>>> = std::sync::LazyLock::new(Default::default);

/// What `table` (CONSTANTS, LAYOUTS) keeps under `id`.
fn kept<T: Clone>(table: &std::sync::Mutex<std::collections::HashMap<u64, T>>, id: u64) -> Option<T> {
    table.lock().ok()?.get(&id).cloned()
}

/// A layout's [[{ name, id, format }, location], ...] kept in LAYOUTS under its hash (0 for none).
fn layout_of(v: Option<&Value>) -> Result<u64, String> {
    let Some(list) = v.and_then(Value::as_array).filter(|l| !l.is_empty()) else { return Ok(0) };
    let mut out = Vec::new();
    for e in list {
        let (a, at) = (e.get(0).ok_or("vertex_attributes are [attribute, location] pairs")?, e.get(1).and_then(Value::as_u64).ok_or("a vertex attribute's location is a whole number")?);
        out.push((raea_sys_material::mesh::attribute(a)?, at as u32));
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    out.iter().for_each(|(a, at)| (a.name, a.id, at).hash(&mut h));
    let id = h.finish().max(1);
    LAYOUTS.lock().map(|mut m| m.insert(id, out)).ok();
    Ok(id)
}

fn intern(c: Vec<(String, f64)>) -> u64 {
    if c.is_empty() {
        return 0;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.iter().for_each(|(n, v)| (n, v.to_bits()).hash(&mut h));
    let id = h.finish().max(1);
    CONSTANTS.lock().map(|mut m| m.insert(id, c)).ok();
    id
}

impl OwnMaterial {
    /// Its storage buffers, in slot order.
    pub(crate) fn buffers(&self) -> [Handle<ShaderBuffer>; 2] {
        [self.b0.clone(), self.b1.clone()]
    }
    /// Its uniforms' slots, packed again (own_live.rs patch).
    pub(crate) fn set_slots(&mut self, s: [[Vec4; 16]; 8]) {
        [self.u0, self.u1, self.u2, self.u3, self.u4, self.u5, self.u6, self.u7] = s;
    }
}

/// What picks a pipeline: the material's shaders, the faces it culls and its constants (opaque to reflection, as
/// SpriteMaterial's extension reflects its material's key).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
#[reflect(opaque)]
pub struct OwnKey {
    shader: u128,
    vertex: u128,
    cull: Option<Option<Face>>,
    constants: u64,
    layout: u64,
}

impl From<&OwnMaterial> for OwnKey {
    fn from(m: &OwnMaterial) -> Self {
        let &OwnMaterial { shader, vertex, cull, constants, layout, .. } = m;
        OwnKey { shader, vertex, cull, constants, layout }
    }
}

/// Material's and Material2d's specialize, one body for both (each trait takes its own pipeline and key types): the
/// material's shaders put where that trait's placeholder is (`swap`).
macro_rules! specialize {
    ($pipeline:ty, $key:ident, $placeholder:expr) => {
        fn specialize(_: &$pipeline, descriptor: &mut RenderPipelineDescriptor, layout: &MeshVertexBufferLayoutRef, key: $key<Self>) -> Result<(), SpecializedMeshPipelineError> {
            swap(descriptor, layout, &$placeholder, key.bind_group_data)
        }
    };
}

impl Material for OwnMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(PLACEHOLDER)
    }
    fn alpha_mode(&self) -> AlphaMode {
        self.alpha.unwrap_or(AlphaMode::Opaque)
    }
    fn depth_bias(&self) -> f32 {
        self.depth_bias
    }
    specialize!(MaterialPipeline, MaterialPipelineKey, PLACEHOLDER);
}

impl Material2d for OwnMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(PLACEHOLDER_2D)
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        alpha2d(self.alpha.unwrap_or(AlphaMode::Opaque))
    }
    fn depth_bias(&self) -> f32 {
        self.depth_bias
    }
    specialize!(Material2dPipeline, Material2dKey, PLACEHOLDER_2D);
}

/// A sprite's SpriteMaterial<M> (Bevy's shader/sprite_material): the same material extending the sprite's own, its
/// fragment where Material2d's is, the sprite's alpha mode unless the spec gives one.
impl MaterialExtension2d for OwnMaterial {
    fn fragment_shader() -> Option<ShaderRef> {
        Some(ShaderRef::Handle(PLACEHOLDER_2D))
    }
    fn alpha_mode(&self) -> Option<AlphaMode2d> {
        self.alpha.map(alpha2d)
    }
    fn depth_bias(&self) -> Option<f32> {
        Some(self.depth_bias)
    }
    specialize!(Material2dPipeline, Material2dKey, PLACEHOLDER_2D);
}

fn alpha2d(a: AlphaMode) -> AlphaMode2d {
    match a {
        AlphaMode::Opaque => AlphaMode2d::Opaque,
        AlphaMode::Mask(c) => AlphaMode2d::Mask(c),
        _ => AlphaMode2d::Blend,
    }
}

/// A pipeline given the material's own shaders: its fragment where it is `placeholder`, its vertex shader (if any) in
/// that same pipeline (the main pass's: the prepass and shadow pipelines keep Bevy's), the faces it culls, and its
/// fragment's pipeline-overridable constants (as Bevy's pipeline_constants pushes LEVELS in its specialize).
fn swap(descriptor: &mut RenderPipelineDescriptor, layout: &MeshVertexBufferLayoutRef, placeholder: &Handle<Shader>, k: OwnKey) -> Result<(), SpecializedMeshPipelineError> {
    let Some(f) = descriptor.fragment.as_mut().filter(|f| f.shader.id() == placeholder.id()) else { return Ok(()) };
    let shader = |id: u128| Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(id), core::marker::PhantomData);
    f.shader = shader(k.shader);
    if let Some(c) = kept(&CONSTANTS, k.constants) {
        f.constants.extend(c.into_iter().map(|(n, v)| (n.into(), v)));
    }
    if k.vertex != 0 {
        descriptor.vertex.shader = shader(k.vertex);
    }
    if let Some(l) = kept(&LAYOUTS, k.layout) {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&l.iter().map(|(a, at)| a.at_shader_location(*at)).collect::<Vec<_>>())?];
    }
    if let Some(c) = k.cull {
        descriptor.primitive.cull_mode = c;
    }
    Ok(())
}

pub fn install(app: &mut App) {
    let _ = app.world_mut().resource_mut::<Assets<Shader>>().insert(&PLACEHOLDER, Shader::from_wesl(STUB, "embedded://raea_sys_shader/own_placeholder.wesl"));
    let _ = app.world_mut().resource_mut::<Assets<Shader>>().insert(&PLACEHOLDER_2D, Shader::from_wesl(STUB_2D, "embedded://raea_sys_shader/own_placeholder_2d.wesl"));
    app.add_plugins((MaterialPlugin::<OwnMaterial>::default(), Material2dPlugin::<OwnMaterial>::default(), SpriteMaterialPlugin::<OwnMaterial>::default()));
}

/// The material a `<Material>` spec asks for: {fragment, vertex?, uniforms, textures, defs, own: {alpha_mode,
/// depth_bias, cull_mode}}, its shaders registered as written (bindings renumbered, imports beside them); a 2D one's
/// (`two_d`, a Material2d's) binds group 2.
pub fn build(world: &mut World, v: &Value, image: &mut impl FnMut(&mut World, &Value) -> Result<Handle<Image>, String>, two_d: bool) -> Result<OwnMaterial, String> {
    let groups = if two_d { GROUPS_2D } else { GROUPS };
    let what = "a Material";
    let o = v.as_object().ok_or_else(|| format!("{what} is {{fragment, uniforms, textures}}, got {v}"))?;
    let path = o.get("fragment").and_then(Value::as_str).ok_or_else(|| format!("{what}'s fragment is its shader file"))?;
    // the uniforms are packed in the source's own types below, so Inputs reads the rest
    let mut rest = o.clone();
    let given = rest.remove("uniforms").and_then(|u| u.as_object().cloned()).unwrap_or_default();
    let buffers = rest.remove("buffers").and_then(|u| u.as_object().cloned()).unwrap_or_default();
    let ins = Inputs::read(world, &rest, what, image)?;
    let Rebound { src, uniforms, textures, others, storages, .. } = rebind_in(&ins.src, groups, what, true).map_err(|e| format!("{path}: {e}"))?;
    let [u0, u1, u2, u3, u4, u5, u6, u7] = layout::slots(&src, &uniforms, &given, &ins.defs).map_err(|e| format!("{path}: {e}"))?;
    if let Some((k, _)) = ins.textures.iter().find(|(k, _)| !textures.contains(k)) {
        return Err(format!("{path} has no texture_2d {k}"));
    }
    // a texture the source declares and the spec doesn't give is Bevy's fallback image, as an Option<Handle<Image>>
    // field left None binds
    let mut t: [Option<Handle<Image>>; 4] = Default::default();
    for (i, name) in textures.iter().enumerate() {
        t[i] = ins.textures.iter().find(|(k, _)| k == name).map(|x| x.1.clone());
    }
    // the other dimensions: a 2d_array from the spec's one texture array ({layers}); the rest are fallbacks for now
    let mut dims: [Option<Handle<Image>>; 5] = Default::default();
    if let Some((name, h)) = &ins.array {
        if !others.iter().any(|(n, d)| n == name && *d == "2d_array") {
            return Err(format!("{path} has no texture_2d_array {name}"));
        }
        dims[1] = Some(h.clone());
    }
    // its storage buffers from the spec's `buffers`, each in its declared type's layout
    if let Some(k) = layout::undeclared(&buffers, &storages) {
        return Err(format!("{path} has no storage buffer {k}"));
    }
    let mut b = Vec::new();
    for (name, ty) in &storages {
        let v = buffers.get(name).ok_or_else(|| format!("{path}'s storage buffer {name} has no value"))?;
        b.push(world.resource_mut::<Assets<ShaderBuffer>>().add(storage(&src, &ins.defs, name, ty, v).map_err(|e| format!("{path}: {e}"))?));
    }
    while b.len() < 2 {
        b.push(world.resource_mut::<Assets<ShaderBuffer>>().add(ShaderBuffer::from(vec![0u32; 4])));
    }
    let packing = Arc::new(Packing { src: src.clone(), uniforms, storages, defs: ins.defs.clone() });
    let krate = folder(world, path, &src)?;
    let shader = register(world, src, &ins.defs, "raea own material", &krate);
    let vertex = match o.get("vertex").and_then(Value::as_str) {
        None => 0,
        Some(p) => {
            let s = std::fs::read_to_string(p).map_err(|e| format!("{what}'s vertex {p}: {e}"))?;
            let s = rebind_in(&s, groups, what, true).map_err(|e| format!("{p}: {e}"))?.src;
            let krate = folder(world, p, &s)?;
            register(world, s, &ins.defs, "raea own material vertex", &krate)
        }
    };
    let own = o.get("own").and_then(Value::as_object).cloned().unwrap_or_default();
    let alpha = alpha_mode(own.get("alpha_mode"))?;
    let depth_bias = own.get("depth_bias").and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let cull = match own.get("cull_mode") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) if v == "Front" => Some(Some(Face::Front)),
        Some(v) if v == "Back" => Some(Some(Face::Back)),
        Some(v) => return Err(format!("a Material's cull_mode is \"Front\", \"Back\" or null (none), got {v}")),
    };
    let mut constants = Vec::new();
    for (k, v) in own.get("constants").and_then(Value::as_object).into_iter().flatten() {
        let n = v.as_f64().ok_or_else(|| format!("a Material's constant {k} is a number, got {v}"))?;
        constants.push((k.clone(), n));
    }
    let constants = intern(constants);
    let [t0, t1, t2, t3] = t;
    let [d1, d2a, cube, cube_array, d3] = dims;
    Ok(OwnMaterial { u0, u1, u2, u3, u4, u5, u6, u7, t0, t1, t2, t3, d1, d2a, cube, cube_array, d3, b0: b[0].clone(), b1: b[1].clone(), packing, shader, vertex, alpha, depth_bias, cull, constants, layout: layout_of(own.get("vertex_attributes"))? })
}

/// Bevy's AlphaMode as the SDK writes it: "Opaque", "Blend", {"Mask": cutoff}, "Premultiplied", "AlphaToCoverage",
/// "Add" or "Multiply".
fn alpha_mode(v: Option<&Value>) -> Result<Option<AlphaMode>, String> {
    Ok(Some(match v {
        None => return Ok(None),
        Some(Value::String(s)) => match s.as_str() {
            "Opaque" => AlphaMode::Opaque,
            "Blend" => AlphaMode::Blend,
            "Premultiplied" => AlphaMode::Premultiplied,
            "AlphaToCoverage" => AlphaMode::AlphaToCoverage,
            "Add" => AlphaMode::Add,
            "Multiply" => AlphaMode::Multiply,
            _ => return Err(format!("a Material's alpha_mode is Opaque, Blend, Mask, Premultiplied, AlphaToCoverage, Add or Multiply, got {s:?}")),
        },
        Some(v) => match v.get("Mask").and_then(Value::as_f64) {
            Some(c) => AlphaMode::Mask(c as f32),
            None => return Err(format!("a Material's alpha_mode is a name or {{\"Mask\": cutoff}}, got {v}")),
        },
    }))
}

/// A picture file decoded now as an Image (sRGB, the default sampler, as asset_server.load reads a colour texture), so
/// the first frame that draws it has it.
pub fn picture(world: &mut World, path: &str) -> Result<Handle<Image>, String> {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("png");
    let image = Image::from_buffer(&bytes, ImageType::Extension(ext), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default()).map_err(|e| format!("{path}: {e}"))?;
    Ok(world.resource_mut::<Assets<Image>>().add(image))
}

/// The embedded folder a file's module goes in (raea_sys_shader/d{hash of its folder}), with every module it imports
/// by `super::NAME` registered there from its own folder, and theirs in turn.
fn folder(world: &mut World, path: &str, src: &str) -> Result<String, String> {
    let dir = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new("."));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    dir.hash(&mut h);
    let krate = format!("raea_sys_shader/d{:016x}", h.finish());
    let mut todo: Vec<String> = siblings(src);
    let mut done: Vec<String> = Vec::new();
    while let Some(name) = todo.pop() {
        if done.contains(&name) {
            continue;
        }
        let file = ["wesl", "wgsl"].iter().map(|x| dir.join(format!("{name}.{x}"))).find(|f| f.exists()).ok_or_else(|| format!("{path} imports super::{name}, and {} has no {name}.wesl", dir.display()))?;
        let s = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        todo.extend(siblings(&s));
        let at = format!("embedded://{krate}/{name}.wesl");
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (&at, &s).hash(&mut h);
        let id = AssetId::<Shader>::Uuid { uuid: bevy::asset::uuid::Uuid::from_u64_pair(h.finish(), 0x5d1e_7a3c) };
        let _ = world.resource_mut::<Assets<Shader>>().insert(id, Shader::from_wesl(s, at));
        done.push(name);
    }
    Ok(krate)
}

/// The modules a source imports from its own folder: each `super::NAME` of its `import` statements.
fn siblings(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines().map(str::trim).filter(|l| l.starts_with("import ")) {
        for (i, _) in line.match_indices("super::") {
            // super::NAME::item, or super::{NAME::item, OTHER::item}
            let after = &line[i + 7..];
            let parts: Vec<&str> = match after.strip_prefix('{') {
                Some(r) => r.split('}').next().unwrap_or("").split(',').collect(),
                None => vec![after],
            };
            for p in parts {
                let name: String = p.trim().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// The Materials made this load, by their spec (its track of values included, raea-runtime-look own_uniforms): parts
/// with one spec share one, as Bevy's examples share a handle (shader/storage_buffer's 91 cubes draw with one).
#[derive(Resource, Default)]
pub struct OwnMade(pub std::collections::HashMap<String, Handle<OwnMaterial>>);

/// The Material for a spec: the one made already for the same spec (in 3D or 2D), or a new one (`build`).
pub fn shared(world: &mut World, v: &Value, image: &mut impl FnMut(&mut World, &Value) -> Result<Handle<Image>, String>, two_d: bool) -> Result<Handle<OwnMaterial>, String> {
    let key = format!("{two_d} {v}");
    if let Some(h) = world.get_resource::<OwnMade>().and_then(|m| m.0.get(&key)).cloned() {
        return Ok(h);
    }
    let m = build(world, v, image, two_d)?;
    let h = world.resource_mut::<Assets<OwnMaterial>>().add(m);
    world.get_resource_or_init::<OwnMade>().0.insert(key, h.clone());
    Ok(h)
}

/// A part given its own Material: its MeshMaterial3d<StandardMaterial> swapped for the OwnMaterial, the standard one
/// kept aside as `Shaded` (so the rows' sync finds it) and the scene's <SurfaceShader> kept off it.
pub fn put(world: &mut World, e: Entity, h: Handle<OwnMaterial>) -> Result<(), String> {
    let Some(std) = world.get::<MeshMaterial3d<StandardMaterial>>(e).map(|m| m.0.clone()) else { return Ok(()) };
    world.entity_mut(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((MeshMaterial3d(h), Shaded { std }, OwnSurface));
    crate::own_live::settle(world, e)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_files_own_folder_imports_are_found() {
        let src = "import bevy_pbr::forward_io::VertexOutput;\nimport super::custom_material_import::COLOR_MULTIPLIER;\nimport super::{a::X, b_2::Y};\nfn f() { super::not_an_import(); }\n";
        assert_eq!(super::siblings(src), vec!["custom_material_import".to_string(), "a".into(), "b_2".into()]);
        assert!(super::alpha_mode(Some(&serde_json::json!("Blend"))).unwrap() == Some(bevy::prelude::AlphaMode::Blend));
        assert!(super::alpha_mode(Some(&serde_json::json!({"Mask": 0.5}))).unwrap() == Some(bevy::prelude::AlphaMode::Mask(0.5)));
        assert!(super::alpha_mode(Some(&serde_json::json!("Clear"))).is_err());
    }
}
