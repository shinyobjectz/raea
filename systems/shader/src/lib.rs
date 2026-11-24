//! A shader written in TypeScript over the lit standard surface (docs/shaders.feature SH1-SH4): raea/3d's
//! `<MeshStandardMaterial shader={…}>` (on a Mesh, a Model's glTF materials, a Character), `<SurfaceShader>` over every
//! lit surface in a scene, and raea/pbr's `<ExtendedMaterial extension={<ShaderExtension/>}>`. One Rust type covers them
//! all, as ShaderMaterial does for unlit ones: ExtendedMaterial<StandardMaterial, ShaderExt>, whose extension holds 16
//! vec4 uniform slots and four optional textures at bindings 100 and up (the standard material's are below), and whose
//! fragment shader is chosen per material in `specialize` from its bind-group data (a uuid handle hashed from the source).
//!
//! The source (`wesl` or `wgsl`) defines either hook, or both, or its own @fragment:
//!   fn surface(in: VertexOutput, pbr: ptr<function, PbrInput>)              before lighting (base colour, normal, ...)
//!   fn lit(in: VertexOutput, pbr: PbrInput, color: vec4<f32>) -> vec4<f32>  after lighting, before fog and post
//! and reads its uniforms as `u_NAME()` and its textures as `tex_NAME` / `samp_NAME`, with `view` and `globals` imported.
//! The textures share one sampler, the first texture's (the fragment stage has 16, and the standard surface uses 13).
//!
//! Only the main pass runs it: ExtendedMaterial falls back to the standard material's prepass, shadow and deferred
//! shaders, and `specialize` swaps a fragment shader only where it is PLACEHOLDER, so those pipelines are untouched.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::asset::uuid_handle;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline, MaterialPlugin};
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::{Shader, ShaderDefVal, ShaderRef};
use serde_json::Value;
use std::hash::{Hash, Hasher};

use raea_sys_material::shader_material::{defs, ident, uniform, Uniform, SLOTS, TEXTURES};

mod ext_bind;
pub mod bindings;
use raea_sys_shader_wgsl::{layout, storage_layout};
/// Uniforms packed in their declared types' WGSL layout (a compute's as written: raea-sys-compute written.rs).
pub use layout::slots as uniform_slots;
/// Each uniform's bytes by name, packed in its declared type's WGSL layout (a compute as written's, a frame pass as
/// written's, three's kernels').
pub fn uniform_bytes(src: &str, declared: &[(String, String)], given: &serde_json::Map<String, Value>, defs: &[ShaderDefVal]) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
    let slots = uniform_slots(src, declared, given, defs)?;
    Ok(declared.iter().zip(slots).map(|((n, _), s)| (n.clone(), s.iter().flat_map(|v| v.to_array()).flat_map(f32::to_ne_bytes).collect())).collect())
}
/// A storage buffer's words in its WGSL type's storage layout (a compute as written's own: raea-sys-compute slab.rs).
pub use storage_layout::words as storage_words;
pub mod own;
pub mod own_array;
mod own_images;
pub mod own_live;
/// Storage buffers shared by key between three's computes and its node materials (docs/r3f.feature R3F22).
pub mod shared;
mod failed;
pub mod ui;
use raea_sys_shader_wgsl::ui_bind;

/// The fragment shader every ShaderExt pipeline starts from, swapped in `specialize` for the material's own. It is a
/// whole lit surface by itself (no hooks), so a material whose module is missing still draws as standard.
const PLACEHOLDER: Handle<Shader> = uuid_handle!("2f6b1c4e-8d3a-4e7f-b9a2-6c1d5e8f0a73");

pub type ShadedMaterial = ExtendedMaterial<StandardMaterial, ShaderExt>;

pub fn install(app: &mut App) {
    let stub = module("", &[], &[], None);
    let _ = app.world_mut().resource_mut::<Assets<Shader>>().insert(&PLACEHOLDER, Shader::from_wesl(stub, "embedded://raea_sys_shader/placeholder.wesl"));
    app.add_plugins(MaterialPlugin::<ShadedMaterial>::default());
    own::install(app);
    ui::install(app);
    failed::install(app);
}

#[derive(Asset, AsBindGroup, Reflect, Clone, Debug)]
#[bind_group_data(ExtKey)]
pub struct ShaderExt {
    #[uniform(100)]
    pub(crate) slots: [Vec4; SLOTS],
    // one sampler for all four (the first texture's): the standard material and the view already bind 13 of the
    // fragment stage's 16 samplers, so four more failed the pipeline ("Too many bindings of type Samplers", 17)
    #[texture(101)]
    #[sampler(102)]
    tex0: Option<Handle<Image>>,
    #[texture(103)]
    tex1: Option<Handle<Image>>,
    #[texture(105)]
    tex2: Option<Handle<Image>>,
    #[texture(107)]
    tex3: Option<Handle<Image>>,
    /// the one texture array (docs/shaders.feature SH9), read with the same sampler
    #[texture(109, dimension = "2d_array")]
    arr: Option<Handle<Image>>,
    shader: u128,
    /// how a frame's uniforms repack `slots` (own_live.rs: a ShaderExtension's uniforms that change over the frames)
    #[reflect(ignore)]
    repack: Option<std::sync::Arc<Repack>>,
}

/// What a ShaderExt's slots are packed from: its uniforms' names in slot order with their first values, and its own
/// @binding(100) struct's fields when the source declares one (layout.rs)
#[derive(Debug)]
pub struct Repack {
    names: Vec<String>,
    first: Vec<Vec4>,
    fields: Option<Vec<(String, String)>>,
}

impl ShaderExt {
    /// The slots a frame's uniforms ({name: value}) pack into, None when they're the ones it holds; a uniform the
    /// frame doesn't give takes its first value
    pub fn slots_for(&self, given: &serde_json::Map<String, Value>) -> Result<Option<[Vec4; SLOTS]>, String> {
        let Some(r) = &self.repack else { return Ok(None) };
        let mut vals = Vec::with_capacity(r.names.len());
        for (n, f) in r.names.iter().zip(&r.first) {
            vals.push((n.clone(), given.get(n).map(|x| uniform(n, x).map(|u| u.value)).transpose()?.unwrap_or(*f)));
        }
        let slots = match &r.fields {
            Some(fields) => layout::pack(fields, &vals)?,
            None => std::array::from_fn(|i| vals.get(i).map_or(Vec4::ZERO, |v| v.1)),
        };
        Ok((slots != self.slots).then_some(slots))
    }
}

/// What picks a ShaderExt's pipeline: its shader's uuid.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ExtKey {
    shader: u128,
}

impl From<&ShaderExt> for ExtKey {
    fn from(m: &ShaderExt) -> Self {
        ExtKey { shader: m.shader }
    }
}

impl MaterialExtension for ShaderExt {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(PLACEHOLDER)
    }
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        if let Some(f) = descriptor.fragment.as_mut() {
            if f.shader.id() == PLACEHOLDER.id() {
                f.shader = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(key.bind_group_data.shader), core::marker::PhantomData);
            }
        }
        Ok(())
    }
}

/// Whether the source defines `fn NAME(`.
pub fn defines(src: &str, name: &str) -> bool {
    src.match_indices("fn").any(|(i, _)| {
        let before = src[..i].chars().next_back();
        let rest = src[i + 2..].trim_start();
        before.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_')) && rest.len() < src.len() - i - 2 && rest.strip_prefix(name).is_some_and(|r| r.trim_start().starts_with('('))
    })
}

/// `src` with every whole-word `from` written `to`.
fn rename(src: &str, from: &str, to: &str) -> String {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find(from) {
        let before = rest[..i].chars().next_back().or_else(|| out.chars().next_back());
        let after = rest[i + from.len()..].chars().next();
        out += &rest[..i];
        out += if word(before) || word(after) { from } else { to };
        rest = &rest[i + from.len()..];
    }
    out + rest
}

/// The module a ShaderExt source is compiled as, in WESL: the standard surface's imports, the slots' and textures'
/// bindings and accessors, the source, and (unless it has its own) the entry point: the standard surface's inputs,
/// `surface` on them, Bevy's lighting, `lit` on its colour, then fog and the rest of the main pass's own processing.
pub fn module(source: &str, uniforms: &[(String, &'static str)], textures: &[String], array: Option<&str>) -> String {
    let group = "constants::MATERIAL_BIND_GROUP";
    let mut m = String::from(
        "import bevy_pbr::render::pbr_types::{PbrInput, STANDARD_MATERIAL_FLAGS_UNLIT_BIT};\n\
         import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;\n\
         import bevy_pbr::render::pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing};\n\
         import bevy_pbr::render::forward_io::{VertexOutput, FragmentOutput};\n\
         import bevy_pbr::render::mesh_view_bindings::{view, globals};\n\
         @if(VISIBILITY_RANGE_DITHER) import bevy_pbr::render::pbr_functions::visibility_range_dither;\n\n",
    );
    m += &format!("struct RaeaSlots {{ s: array<vec4<f32>, 16> }};\n@group({group}) @binding(100) var<uniform> raea_slots: RaeaSlots;\n");
    if !textures.is_empty() || array.is_some() {
        m += &format!("@group({group}) @binding(102) var raea_sampler: sampler;\n");
    }
    for (i, t) in textures.iter().enumerate() {
        m += &format!("@group({group}) @binding({}) var tex_{t}: texture_2d<f32>;\n", 101 + i * 2);
    }
    if let Some(a) = array {
        m += &format!("@group({group}) @binding(109) var tex_{a}: texture_2d_array<f32>;\n");
    }
    m += &accessors(uniforms, "raea_slots");
    m += "\n";
    // each texture's `samp_NAME` is the one shared sampler (WGSL has no alias for a sampler variable)
    let source = textures.iter().map(String::as_str).chain(array).fold(source.to_string(), |src, t| rename(&src, &format!("samp_{t}"), "raea_sampler"));
    let source = source.as_str();
    m += source;
    if source.contains("@fragment") {
        return m;
    }
    let surface = if defines(source, "surface") { "    surface(in, &pbr_input);\n" } else { "" };
    let lit = if defines(source, "lit") { "    color = lit(in, pbr_input, color);\n" } else { "" };
    m += &format!(
        "\n\n@fragment\n\
         fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {{\n\
         \x20   @if(VISIBILITY_RANGE_DITHER) visibility_range_dither(in.position, in.visibility_range_dither);\n\
         \x20   var pbr_input = pbr_input_from_standard_material(in, is_front);\n\
         \x20   pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);\n\
         {surface}\
         \x20   var color = pbr_input.material.base_color;\n\
         \x20   if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {{\n\
         \x20       color = apply_pbr_lighting(pbr_input);\n\
         \x20   }}\n\
         {lit}\
         \x20   var out: FragmentOutput;\n\
         \x20   out.color = main_pass_post_lighting_processing(pbr_input, color);\n\
         \x20   return out;\n\
         }}\n"
    );
    m
}

/// The `u_NAME()` accessor of each uniform, reading its slot of `var.s` (an array of 16 vec4).
pub fn accessors(uniforms: &[(String, &'static str)], var: &str) -> String {
    let mut m = String::new();
    for (i, (n, ty)) in uniforms.iter().enumerate() {
        let part = match *ty {
            "f32" => ".x",
            "vec2<f32>" => ".xy",
            "vec3<f32>" => ".xyz",
            _ => "",
        };
        m += &format!("fn u_{n}() -> {ty} {{ return {var}.s[{i}]{part}; }}\n");
    }
    m
}

/// A spec's source, uniforms and textures, checked (`what` names the spec in errors: "a surface shader").
pub struct Inputs {
    pub src: String,
    pub uniforms: Vec<Uniform>,
    pub textures: Vec<(String, Handle<Image>)>,
    /// the one texture array, `{layers}` (raea-sys-sampler's arrays)
    pub array: Option<(String, Handle<Image>)>,
    /// the one binding_array<texture_2d<f32>>, a list of pictures (own.rs, Bevy's texture_binding_array)
    pub bindings: Option<(String, Vec<Handle<Image>>)>,
    pub defs: Vec<ShaderDefVal>,
}

impl Inputs {
    pub fn read(world: &mut World, o: &serde_json::Map<String, Value>, what: &str, image: &mut impl FnMut(&mut World, &Value) -> Result<Handle<Image>, String>) -> Result<Inputs, String> {
        let src = match (o.get("wgsl").and_then(Value::as_str), o.get("wesl").and_then(Value::as_str), o.get("fragment").and_then(Value::as_str)) {
            (Some(s), None, None) | (None, Some(s), None) => s.to_string(),
            // an asset path, found by the SDK (asset.load), as Bevy's MaterialExtension::fragment_shader names one
            (None, None, Some(p)) => std::fs::read_to_string(p).map_err(|e| format!("{what}: its fragment {p}: {e}"))?,
            (None, None, None) => return Err(format!("{what} needs wesl, wgsl or a fragment path: its source")),
            _ => return Err(format!("{what} takes one of wesl, wgsl and fragment")),
        };
        let mut uniforms = vec![];
        if let Some(u) = o.get("uniforms") {
            for (k, x) in u.as_object().ok_or_else(|| format!("{what}'s uniforms are {{name: value}}"))? {
                uniforms.push(uniform(k, x)?);
            }
        }
        if uniforms.len() > SLOTS {
            return Err(format!("{what} has at most {SLOTS} uniforms, got {}", uniforms.len()));
        }
        let mut textures = vec![];
        let mut array = None;
        let mut bindings = None;
        if let Some(t) = o.get("textures") {
            for (k, x) in t.as_object().ok_or_else(|| format!("{what}'s textures are {{name: picture}}"))? {
                if !ident(k) {
                    return Err(format!("a texture's name is a WGSL identifier, got {k:?}"));
                }
                if let Some(list) = x.as_array() {
                    if bindings.is_some() {
                        return Err(format!("{what} has one binding_array, got a second ({k})"));
                    }
                    let pics = list.iter().enumerate().map(|(i, p)| raea_sys_sampler::texture(world, p, &format!("{what} texture {k}[{i}]"), image).map_err(|e| format!("texture {k}[{i}]: {e}")));
                    bindings = Some((k.clone(), pics.collect::<Result<Vec<_>, _>>()?));
                    continue;
                }
                if raea_sys_sampler::arrays::is_array(x) {
                    if array.is_some() {
                        return Err(format!("{what} has one texture array, got a second ({k})"));
                    }
                    array = Some((k.clone(), raea_sys_sampler::arrays::array(world, x, &format!("{what} texture {k}"), image)?));
                    continue;
                }
                textures.push((k.clone(), raea_sys_sampler::texture(world, x, &format!("{what} texture {k}"), image).map_err(|e| format!("texture {k}: {e}"))?));
            }
        }
        if textures.len() > TEXTURES {
            return Err(format!("{what} has at most {TEXTURES} textures, got {}", textures.len()));
        }
        Ok(Inputs { src, uniforms, textures, array, bindings, defs: defs(o, what)? })
    }

    /// The uniforms' names and WGSL types, and the textures' names.
    pub fn names(&self) -> (Vec<(String, &'static str)>, Vec<String>) {
        (self.uniforms.iter().map(|u| (u.name.clone(), u.ty)).collect(), self.textures.iter().map(|t| t.0.clone()).collect())
    }

    /// The uniforms' values, in their slots.
    pub fn slots(&self) -> [Vec4; SLOTS] {
        let mut slots = [Vec4::ZERO; SLOTS];
        for (i, u) in self.uniforms.iter().enumerate() {
            slots[i] = u.value;
        }
        slots
    }
}

/// `module` and its `defs` added to Assets<Shader> under the uuid of their hash salted with `kind`, as
/// embedded://`krate`/m{id}.wesl; returns the uuid.
pub fn register(world: &mut World, module: String, defs: &[ShaderDefVal], kind: &str, krate: &str) -> u128 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    module.hash(&mut h);
    defs.hash(&mut h);
    let hi = h.finish();
    kind.hash(&mut h);
    let id = ((hi as u128) << 64) | h.finish() as u128;
    failed::ours(id);
    let aid = AssetId::<Shader>::Uuid { uuid: bevy::asset::uuid::Uuid::from_u128(id) };
    let mut shaders = world.resource_mut::<Assets<Shader>>();
    if shaders.get(aid).is_none() {
        let mut shader = Shader::from_wesl(module, format!("embedded://{krate}/m{id:032x}.wesl"));
        shader.shader_defs = defs.to_vec();
        let _ = shaders.insert(aid, shader);
    }
    id
}

/// A ShaderExt from a `shader` spec, {wesl | wgsl, uniforms, textures}, its module added to Assets<Shader> under the
/// uuid of its hash. `image` resolves a texture (a path, an inline picture, a "render:NAME" image) as a Handle<Image>
/// (raea-runtime-plugins' fields::image).
pub fn build(world: &mut World, v: &Value, mut image: impl FnMut(&mut World, &Value) -> Result<Handle<Image>, String>) -> Result<ShaderExt, String> {
    let o = v.as_object().ok_or_else(|| format!("a surface shader is {{wesl, uniforms, textures}}, got {v}"))?;
    let ins = Inputs::read(world, o, "a surface shader", &mut image)?;
    if !(ins.src.contains("@fragment") || defines(&ins.src, "surface") || defines(&ins.src, "lit")) {
        return Err("a surface shader defines fn surface(in: VertexOutput, pbr: ptr<function, PbrInput>), fn lit(in: VertexOutput, pbr: PbrInput, color: vec4<f32>) -> vec4<f32>, or its own @fragment".into());
    }
    let (names, tnames) = ins.names();
    // its own @binding(100) struct: the source as written, the uniforms in that struct's layout (layout.rs, FC2b)
    // and a file (`fragment`) always as written, as Bevy's fragment_shader() names one: its imports first, its own
    // bindings or none (a uniform it doesn't declare is refused)
    let file = o.contains_key("fragment");
    // a file's own bindings in ShaderExt's slots, and its textures in their order (ext_bind.rs)
    let (own, tex) = match file {
        true => ext_bind::fit(&ins.src, &ins.textures)?,
        false => (ins.src.clone(), ins.textures.iter().map(|x| Some(x.1.clone())).collect()),
    };
    let (src, slots, fields) = match layout::own_struct(&own, &ins.defs).or_else(|| file.then(|| Ok(vec![]))) {
        Some(fields) => {
            let (fields, vals) = (fields?, ins.uniforms.iter().map(|u| (u.name.clone(), u.value)).collect::<Vec<_>>());
            (own, layout::pack(&fields, &vals)?, Some(fields))
        }
        None => (module(&ins.src, &names, &tnames, ins.array.as_ref().map(|a| a.0.as_str())), ins.slots(), None),
    };
    let (names, first) = ins.uniforms.iter().map(|u| (u.name.clone(), u.value)).unzip();
    let repack = (!ins.uniforms.is_empty()).then(|| std::sync::Arc::new(Repack { names, first, fields }));
    let id = register(world, src, &ins.defs, "raea surface shader", "raea_sys_shader");
    let mut t = tex.into_iter();
    Ok(ShaderExt { slots, tex0: t.next().flatten(), tex1: t.next().flatten(), tex2: t.next().flatten(), tex3: t.next().flatten(), arr: ins.array.map(|a| a.1), shader: id, repack })
}

/// A mesh row's shader put on its part: a <Material>'s own (own.rs) or a ShaderExtension over its standard material.
pub fn put(world: &mut World, e: Entity, v: &Value, mut image: impl FnMut(&mut World, &Value) -> Result<Handle<Image>, String>, made: &mut HashMap<(AssetId<StandardMaterial>, u128), Handle<ShadedMaterial>>) -> Result<(), String> {
    // a <Material>'s spec has its `own` (own.rs)
    if v.get("own").is_some() {
        let m = own::shared(world, v, &mut image, false)?;
        return own::put(world, e, m);
    }
    let ext = build(world, v, image)?;
    shade(world, e, &ext, made);
    // the frame's uniforms a row gave before it was on (own_live.rs)
    own_live::settle(world, e)
}

/// A shaded part: the standard material the rows animate (sync_meshes writes it), copied onto the shaded one's base
/// when it changes (sync_shaded).
#[derive(Component, Clone)]
pub struct Shaded {
    pub std: Handle<StandardMaterial>,
}

/// A part a scene's <SurfaceShader> leaves alone: it has its own shader, or isn't a lit standard surface.
#[derive(Component)]
pub struct OwnSurface;

/// A standard part given the shader over it: its MeshMaterial3d<StandardMaterial> swapped for a ShadedMaterial (one per
/// standard material and shader, `made` shared across parts), the standard one kept aside as `Shaded`.
pub fn shade(world: &mut World, e: Entity, ext: &ShaderExt, made: &mut HashMap<(AssetId<StandardMaterial>, u128), Handle<ShadedMaterial>>) -> bool {
    let Some(std) = world.get::<MeshMaterial3d<StandardMaterial>>(e).map(|m| m.0.clone()) else { return false };
    let h = match made.get(&(std.id(), ext.shader)) {
        Some(h) => h.clone(),
        None => {
            let Some(base) = world.resource::<Assets<StandardMaterial>>().get(&std).cloned() else { return false };
            let h = world.resource_mut::<Assets<ShadedMaterial>>().add(ShadedMaterial { base, extension: ext.clone() });
            made.insert((std.id(), ext.shader), h.clone());
            h
        }
    };
    world.entity_mut(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((MeshMaterial3d(h), Shaded { std }, OwnSurface));
    true
}

/// The frame's standard values onto each shaded material: its base follows the standard one the rows animate (colour,
/// opacity, emissive, roughness, metal, a sprite sheet's uv), as paint.rs's copies do.
pub fn sync_shaded(stds: Res<Assets<StandardMaterial>>, mut shaded: ResMut<Assets<ShadedMaterial>>, q: Query<(&Shaded, &MeshMaterial3d<ShadedMaterial>)>) {
    for (s, m) in &q {
        let (Some(std), Some(cur)) = (stds.get(&s.std), shaded.get(&m.0)) else { continue };
        let b = &cur.base;
        let moved = b.base_color != std.base_color || b.emissive != std.emissive || b.perceptual_roughness != std.perceptual_roughness || b.metallic != std.metallic || b.alpha_mode != std.alpha_mode || b.uv_transform != std.uv_transform;
        if moved {
            let std = std.clone();
            if let Some(mut cur) = shaded.get_mut(&m.0) {
                cur.base = std;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_frame_s_uniforms_repack_the_slots_they_change() {
        use bevy::math::Vec4;
        let repack = super::Repack { names: vec!["glow".into(), "tint".into()], first: vec![Vec4::ZERO, Vec4::new(1.0, 0.5, 0.0, 0.0)], fields: None };
        let mut ext = super::ShaderExt { slots: [Vec4::ZERO; super::SLOTS], tex0: None, tex1: None, tex2: None, tex3: None, arr: None, shader: 0, repack: Some(std::sync::Arc::new(repack)) };
        ext.slots[1] = Vec4::new(1.0, 0.5, 0.0, 0.0);
        // frame 0's values are the ones it holds: nothing to write
        assert_eq!(ext.slots_for(serde_json::json!({ "glow": 0.0 }).as_object().unwrap()).unwrap(), None);
        // a later frame's glow goes in its slot, the tint it didn't give keeps its first value
        let s = ext.slots_for(serde_json::json!({ "glow": 0.75 }).as_object().unwrap()).unwrap().expect("glow changed");
        assert_eq!((s[0].x, s[1]), (0.75, Vec4::new(1.0, 0.5, 0.0, 0.0)));
    }

    #[test]
    fn hooks_are_found_by_name() {
        assert!(super::defines("fn surface(in: VertexOutput, pbr: ptr<function, PbrInput>) {}", "surface"));
        assert!(super::defines("fn  lit (in: VertexOutput, p: PbrInput, c: vec4<f32>) -> vec4<f32> { return c; }", "lit"));
        assert!(!super::defines("fn lighting(x: f32) -> f32 { return x; }", "lit"));
        assert!(!super::defines("fn unlit(x: f32) -> f32 { return x; }", "lit"));
    }

    #[test]
    fn the_entry_calls_the_hooks_it_has() {
        let both = super::module("fn surface(in: VertexOutput, pbr: ptr<function, PbrInput>) {}\nfn lit(in: VertexOutput, pbr: PbrInput, color: vec4<f32>) -> vec4<f32> { return u_tint(); }", &[("tint".into(), "vec4<f32>")], &["pic".into()], None);
        assert!(both.contains("surface(in, &pbr_input);"));
        // a detail's levels cross-fade (docs/realism.feature LT21)
        assert!(both.contains("@if(VISIBILITY_RANGE_DITHER) visibility_range_dither(in.position, in.visibility_range_dither);"));
        assert!(both.contains("color = lit(in, pbr_input, color);"));
        assert!(both.contains("@binding(100) var<uniform> raea_slots"));
        assert!(both.contains("@binding(101) var tex_pic: texture_2d<f32>;"));
        assert!(both.contains("@binding(102) var raea_sampler: sampler;"));
        let tex = super::module("fn lit(in: VertexOutput, pbr: PbrInput, color: vec4<f32>) -> vec4<f32> { return textureSample(tex_pic, samp_pic, in.uv); }", &[], &["pic".into()], None);
        assert!(tex.contains("textureSample(tex_pic, raea_sampler, in.uv)"));
        assert_eq!(super::rename("samp_a + my_samp_a + samp_ab", "samp_a", "s"), "s + my_samp_a + samp_ab");
        assert!(both.contains("fn u_tint() -> vec4<f32> { return raea_slots.s[0]; }"));
        let lit = super::module("fn lit(in: VertexOutput, pbr: PbrInput, color: vec4<f32>) -> vec4<f32> { return color; }", &[], &[], None);
        assert!(!lit.contains("surface(in"));
        let own = super::module("@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> { return vec4(1.0); }", &[], &[], None);
        assert_eq!(own.matches("@fragment").count(), 1);
    }
}
