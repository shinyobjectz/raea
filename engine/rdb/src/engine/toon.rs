//! Toon (cel) surfaces and ink outlines: @raea/motion/3d's MeshToonMaterial, on a Mesh or over a glTF Model's own
//! materials (sdk/motion/docs/bevy-coverage.md). The surface is Bevy's StandardMaterial extended with a fragment shader that
//! lights it in hard bands (toon.wgsl); the outline is an inverted hull, a second draw of the mesh with smoothed normals,
//! front faces culled and pushed out a fixed number of pixels in clip space (toon_outline.wgsl).
//!
//! Columns (mesh and model rows):
//!   toon: the material as JSON. On a mesh one object, on a model a list (each may name the glTF `material` it's
//!         for; one without a name covers the rest):
//!         {steps, bands: [[start, "#hex" | null], ...], shadow: "#hex", rim: {k, color, width},
//!          spec: {k, color, size}, outline: {width (px), color, depth}, color: "#hex" (models only), material}
//!   keyed on a mesh, as the standard material's: col_r col_g col_b, emissive emissive_k, opacity; and the toon's own
//!   sh_r sh_g sh_b (an animated shadow colour), rim_k, spec_k, outline_w

use super::*;
use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{MeshVertexBufferLayoutRef, VertexAttributeValues};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::render::render_resource::{AsBindGroup, Face, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};

const TOON_SHADER: Handle<Shader> = uuid_handle!("6a0b6c1e-3f0e-4b8e-9d4f-5c2f1a7e0b31");
const OUTLINE_SHADER: Handle<Shader> = uuid_handle!("9c3d2e7a-81b4-4f6a-a0c5-2d9e6b4f7a12");

/// The toon surface: the standard material's inputs (base colour, its texture, emissive, opacity, fog) lit in bands.
pub type ToonMaterial = ExtendedMaterial<StandardMaterial, ToonExt>;

/// The toon shader's uniform (toon.wgsl's `Toon`).
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect, PartialEq)]
pub struct ToonParams {
    pub shadow: Vec4,
    pub rim: Vec4,
    pub spec: Vec4,
    pub shape: Vec4,
    pub starts: [Vec4; 2],
    pub colors: [Vec4; 8],
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct ToonExt {
    #[uniform(100)]
    pub params: ToonParams,
}

impl MaterialExtension for ToonExt {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(TOON_SHADER)
    }
}

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect, PartialEq)]
pub struct OutlineParams {
    pub color: Vec4,
    pub shape: Vec4,
}

/// The ink outline's hull material.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct OutlineMaterial {
    #[uniform(0)]
    pub params: OutlineParams,
    blend: bool,
}

impl Material for OutlineMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(OUTLINE_SHADER)
    }
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(OUTLINE_SHADER)
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.blend { AlphaMode::Blend } else { AlphaMode::Opaque }
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        // the inverted hull: only its back faces draw
        descriptor.primitive.cull_mode = Some(Face::Front);
        Ok(())
    }
}

/// The shaders and the two materials, on every engine (nothing is compiled until a toon part draws).
pub(super) fn install(app: &mut App) {
    load_internal_asset!(app, TOON_SHADER, "toon.wgsl", Shader::from_wgsl);
    load_internal_asset!(app, OUTLINE_SHADER, "toon_outline.wgsl", Shader::from_wgsl);
    app.add_plugins((MaterialPlugin::<ToonMaterial>::default(), MaterialPlugin::<OutlineMaterial>::default()));
}

/// One MeshToonMaterial as the row's `toon` column gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ToonSpec {
    /// a glTF material's name (a model's), or every material
    material: Option<String>,
    /// a model's base colour override
    color: Option<Color>,
    steps: u32,
    bands: Vec<(f32, Option<Color>)>,
    shadow: Option<Color>,
    rim: Option<(f32, Color, f32)>,
    spec: Option<(f32, Color, f32)>,
    outline: Option<(f32, Option<Color>, f32)>,
}

fn hex(v: Option<&serde_json::Value>) -> Option<Color> {
    let s = v?.as_str()?;
    s.starts_with('#').then(|| colour(Some(&V::S(s.to_string())), [0.0, 0.0, 0.0, 1.0]))
}

fn f(v: &serde_json::Value, k: &str, d: f32) -> f32 {
    v.get(k).and_then(serde_json::Value::as_f64).map(|x| x as f32).unwrap_or(d)
}

impl ToonSpec {
    fn parse(v: &serde_json::Value) -> ToonSpec {
        let bands = v.get("bands").and_then(serde_json::Value::as_array).map(|bs| {
            bs.iter().filter_map(|b| {
                let b = b.as_array()?;
                Some((b.first()?.as_f64()? as f32, hex(b.get(1))))
            }).collect()
        }).unwrap_or_default();
        let part = |k: &str, size_key: &str, size: f32| {
            v.get(k).filter(|x| x.is_object()).map(|x| (f(x, "k", 1.0), hex(x.get("color")).unwrap_or(Color::WHITE), f(x, size_key, size)))
        };
        ToonSpec {
            material: v.get("material").and_then(serde_json::Value::as_str).map(str::to_string),
            color: hex(v.get("color")),
            steps: f(v, "steps", 2.0).round().clamp(1.0, 8.0) as u32,
            bands,
            shadow: hex(v.get("shadow")),
            rim: part("rim", "width", 0.3),
            spec: part("spec", "size", 0.2),
            outline: v.get("outline").filter(|x| x.is_object()).map(|o| (f(o, "width", 2.0), hex(o.get("color")), f(o, "depth", 0.002))),
        }
    }

    /// The row's `toon` column: one spec (a mesh's) or a list (a model's).
    pub(super) fn of(vals: &BTreeMap<String, V>) -> Vec<ToonSpec> {
        let Some(V::S(s)) = vals.get("toon") else { return Vec::new() };
        match serde_json::from_str::<serde_json::Value>(s) {
            Ok(serde_json::Value::Array(a)) => a.iter().map(ToonSpec::parse).collect(),
            Ok(v @ serde_json::Value::Object(_)) => vec![ToonSpec::parse(&v)],
            _ => Vec::new(),
        }
    }
}

/// A mesh with a toon surface: its spec, for the frame's columns.
#[derive(Component, Clone)]
pub(super) struct Toon(pub ToonSpec);

/// The hull drawing a toon part's outline (a child of the part, or of a model's mesh).
#[derive(Component)]
pub(super) struct OutlineHull {
    of: Entity,
}

fn lin(c: Color) -> Vec4 {
    let l = c.to_linear();
    Vec4::new(l.red, l.green, l.blue, 1.0)
}

/// sRGB-linear to Oklab and back (the ink's default colour).
fn oklab(c: LinearRgba) -> Vec3 {
    let l = 0.4122214708 * c.red + 0.5363325363 * c.green + 0.0514459929 * c.blue;
    let m = 0.2119034982 * c.red + 0.6806995451 * c.green + 0.1073969566 * c.blue;
    let s = 0.0883024619 * c.red + 0.2817188376 * c.green + 0.6299787005 * c.blue;
    let (l, m, s) = (l.max(0.0).cbrt(), m.max(0.0).cbrt(), s.max(0.0).cbrt());
    Vec3::new(
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    )
}

fn from_oklab(c: Vec3) -> LinearRgba {
    let l = c.x + 0.3963377774 * c.y + 0.2158037573 * c.z;
    let m = c.x - 0.1055613458 * c.y - 0.0638541728 * c.z;
    let s = c.x - 0.0894841775 * c.y - 1.2914855480 * c.z;
    let (l, m, s) = (l * l * l, m * m * m, s * s * s);
    LinearRgba::rgb(
        (4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s).max(0.0),
        (-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s).max(0.0),
        (-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s).max(0.0),
    )
}

/// Ink for a colour: its hue, very dark (Oklab lightness 0.2, half its chroma); near-black for a grey.
pub(super) fn ink(c: Color) -> Color {
    let lab = oklab(c.to_linear());
    Color::LinearRgba(from_oklab(Vec3::new(0.2, lab.y * 0.5, lab.z * 0.5)))
}

/// The shader's values for a spec at this frame (an animated shadow colour or rim, highlight strength from the row).
fn params(s: &ToonSpec, vals: Option<&BTreeMap<String, V>>) -> ToonParams {
    let g = |k: &str| vals.and_then(|v| v.get(k)).map(|x| x.num() as f32);
    let mut p = ToonParams::default();
    p.shadow = match (g("sh_r"), s.shadow) {
        (Some(r), _) => lin(Color::srgb(r, g("sh_g").unwrap_or(0.0), g("sh_b").unwrap_or(0.0))),
        (None, Some(c)) => lin(c),
        (None, None) => Vec4::ZERO,
    };
    if let Some((k, c, w)) = s.rim {
        p.rim = lin(c).truncate().extend(g("rim_k").unwrap_or(k).max(0.0));
        p.shape.y = w.clamp(0.0, 1.0);
    }
    if let Some((k, c, size)) = s.spec {
        p.spec = lin(c).truncate().extend(g("spec_k").unwrap_or(k).max(0.0));
        // size 0..1: the highlight's angular radius, up to half a radian
        p.shape.z = (size.clamp(0.0, 1.0) * 0.5).cos();
    }
    let n = if s.bands.is_empty() { s.steps.clamp(1, 8) } else { (s.bands.len() as u32).clamp(1, 8) };
    p.shape.x = n as f32;
    for k in 1..n as usize {
        let start = if s.bands.is_empty() { k as f32 / n as f32 } else { s.bands[k].0 };
        p.starts[k / 4][k % 4] = start;
    }
    for (k, (_, c)) in s.bands.iter().enumerate().take(8) {
        if let Some(c) = c {
            p.colors[k] = lin(*c);
        }
    }
    p
}

/// A toon surface over a standard one (a mesh row's, or a glTF material's).
pub(super) fn material(base: StandardMaterial, s: &ToonSpec, vals: Option<&BTreeMap<String, V>>) -> ToonMaterial {
    ToonMaterial { base, extension: ToonExt { params: params(s, vals) } }
}

fn outline_params(s: &ToonSpec, base: Color, vals: Option<&BTreeMap<String, V>>, opacity: f32) -> Option<OutlineMaterial> {
    let (w, c, depth) = s.outline?;
    let w = vals.and_then(|v| v.get("outline_w")).map(|x| x.num() as f32).unwrap_or(w).max(0.0);
    let c = c.unwrap_or_else(|| ink(base));
    let mut col = lin(c);
    col.w = opacity;
    Some(OutlineMaterial { params: OutlineParams { color: col, shape: Vec4::new(w, depth.clamp(0.0, 0.05), 0.0, 0.0) }, blend: opacity < 1.0 })
}

/// Each vertex's normal replaced by the mean of the face normals at its position (weighted by the face's angle there),
/// so a hull pushed along them stays closed across hard edges (a box's, a flat-shaded solid's, a model's split normals).
pub(super) fn smoothed_normals(pos: &[[f32; 3]], tris: &[[usize; 3]]) -> Vec<[f32; 3]> {
    let key = |p: &[f32; 3]| [(p[0] * 1e5).round() as i64, (p[1] * 1e5).round() as i64, (p[2] * 1e5).round() as i64];
    let mut ids: HashMap<[i64; 3], usize> = HashMap::new();
    let slot: Vec<usize> = pos.iter().map(|p| {
        let n = ids.len();
        *ids.entry(key(p)).or_insert(n)
    }).collect();
    let mut acc = vec![Vec3::ZERO; ids.len()];
    for t in tris {
        let v = t.map(|i| Vec3::from(pos[i]));
        let n = (v[1] - v[0]).cross(v[2] - v[0]);
        if n.length_squared() < 1e-20 {
            continue;
        }
        let n = n.normalize();
        for c in 0..3 {
            let a = (v[(c + 1) % 3] - v[c]).normalize_or_zero();
            let b = (v[(c + 2) % 3] - v[c]).normalize_or_zero();
            acc[slot[t[c]]] += n * a.dot(b).clamp(-1.0, 1.0).acos();
        }
    }
    slot.iter().map(|&s| acc[s].normalize_or(Vec3::Y).to_array()).collect()
}

/// The hull's mesh: the part's own, its normals smoothed (everything else, skin joints and weights included, kept).
pub(super) fn hull_mesh(m: &Mesh) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(pos)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let tris: Vec<[usize; 3]> = match m.indices() {
        Some(ix) => {
            let v: Vec<usize> = ix.iter().collect();
            v.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()
        }
        None => (0..pos.len() / 3).map(|i| [3 * i, 3 * i + 1, 3 * i + 2]).collect(),
    };
    let normals = smoothed_normals(pos, &tris);
    let mut out = m.clone();
    out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    Some(out)
}

/// The hull drawn for `of`: a child entity with the hull mesh, never casting or receiving shadows. A skinned mesh's
/// skin goes with it, so the hull is posed as the mesh is.
fn spawn_hull(world: &mut World, of: Entity, mesh: Handle<Mesh>, mat: OutlineMaterial) {
    let mat = world.resource_mut::<Assets<OutlineMaterial>>().add(mat);
    let skin = world.get::<bevy::mesh::skinning::SkinnedMesh>(of).cloned();
    let mut hull = world.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Inherited, NotShadowCaster, NotShadowReceiver, OutlineHull { of }));
    if let Some(skin) = skin {
        // a posed skin's bounds aren't its rest bounds: never culled
        hull.insert((skin, bevy::camera::visibility::NoFrustumCulling));
    }
    let h = hull.id();
    world.entity_mut(of).add_child(h);
}

/// A mesh row's toon surface and outline (spawn_3d): `base` is the standard material its columns make.
pub(super) fn spawn_mesh(world: &mut World, e: Entity, vals: &BTreeMap<String, V>, mesh: &Handle<Mesh>, base: StandardMaterial) {
    let Some(spec) = ToonSpec::of(vals).into_iter().next() else { return };
    let color = base.base_color;
    let opacity = color.alpha();
    let mat = world.resource_mut::<Assets<ToonMaterial>>().add(material(base, &spec, Some(vals)));
    world.entity_mut(e).insert((MeshMaterial3d(mat), Toon(spec.clone())));
    if let Some(o) = outline_params(&spec, color.with_alpha(1.0), Some(vals), opacity) {
        let hull = world.resource::<Assets<Mesh>>().get(mesh).and_then(hull_mesh);
        if let Some(hull) = hull {
            let h = world.resource_mut::<Assets<Mesh>>().add(hull);
            spawn_hull(world, e, h, o);
        }
    }
}

/// The frame's toon values onto each toon mesh (as sync_meshes does for standard ones): base colour, opacity,
/// emissive, the shadow colour, rim and highlight strength, and its outline's width and opacity.
pub(super) fn sync_toon(
    frame: Res<FrameRows>,
    mut mats: ResMut<Assets<ToonMaterial>>,
    mut inks: ResMut<Assets<OutlineMaterial>>,
    parts: Query<(Entity, &Toon, &MeshMaterial3d<ToonMaterial>)>,
    hulls: Query<(&OutlineHull, &MeshMaterial3d<OutlineMaterial>)>,
) {
    const KEYED: [&str; 9] = ["col_r", "opacity", "emissive_k", "rough", "sh_r", "rim_k", "spec_k", "outline_w", "metal"];
    let mut changed_opacity: HashMap<Entity, (f32, Color)> = HashMap::new();
    for (e, toon, h) in &parts {
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        if !KEYED.iter().any(|k| v.contains_key(*k)) {
            continue;
        }
        let Some(cur) = mats.get(&h.0) else { continue };
        let mut base = cur.base.base_color;
        if v.contains_key("col_r") {
            base = Color::srgb(num(v, "col_r", 0.8), num(v, "col_g", 0.8), num(v, "col_b", 0.8));
        }
        let opacity = if v.contains_key("opacity") { num(v, "opacity", 1.0).clamp(0.0, 1.0) } else { cur.base.base_color.alpha() };
        base.set_alpha(opacity);
        let emissive = if v.contains_key("emissive_k") {
            let em = colour(v.get("emissive"), [0.0, 0.0, 0.0, 1.0]).to_linear() * num(v, "emissive_k", 1.0);
            LinearRgba::rgb(em.red, em.green, em.blue)
        } else {
            cur.base.emissive
        };
        let p = params(&toon.0, Some(v));
        let blend = v.contains_key("opacity") && cur.base.alpha_mode == AlphaMode::Opaque;
        if cur.base.base_color != base || cur.base.emissive != emissive || cur.extension.params != p || blend {
            if let Some(mut m) = mats.get_mut(&h.0) {
                m.base.base_color = base;
                m.base.emissive = emissive;
                m.extension.params = p;
                if blend {
                    m.base.alpha_mode = AlphaMode::Blend;
                }
            }
        }
        changed_opacity.insert(e, (opacity, base.with_alpha(1.0)));
    }
    for (hull, h) in &hulls {
        let Some(&(opacity, base)) = changed_opacity.get(&hull.of) else { continue };
        let Ok((_, toon, _)) = parts.get(hull.of) else { continue };
        let v = frame.vals(hull.of);
        let Some(want) = outline_params(&toon.0, base, v.as_deref(), opacity) else { continue };
        let Some(cur) = inks.get(&h.0) else { continue };
        if cur.params != want.params || cur.blend != want.blend {
            if let Some(mut m) = inks.get_mut(&h.0) {
                m.params = want.params;
                m.blend = want.blend;
            }
        }
    }
}

/// The spec for a glTF material by name: the one naming it, else the one naming none.
fn spec_for<'a>(specs: &'a [ToonSpec], name: Option<&str>) -> Option<&'a ToonSpec> {
    specs.iter().find(|s| s.material.is_some() && s.material.as_deref() == name).or_else(|| specs.iter().find(|s| s.material.is_none()))
}

/// A model's meshes given toon surfaces (and outlines) once its scene is spawned: each glTF material swapped for a toon
/// one over it (its colour and texture kept unless the spec gives a colour), shared materials swapped once. Returns
/// whether it found the meshes.
pub(super) fn apply_to_model(world: &mut World, root: Entity, specs: &[ToonSpec]) -> bool {
    let mut stack = vec![root];
    let mut found = Vec::new();
    while let Some(x) = stack.pop() {
        if let Some(c) = world.get::<Children>(x) {
            stack.extend(c.iter());
        }
        if let (Some(m), Some(mat)) = (world.get::<Mesh3d>(x), world.get::<MeshMaterial3d<StandardMaterial>>(x)) {
            let name = world.get::<bevy::gltf::GltfMaterialName>(x).map(|n| n.0.clone());
            found.push((x, m.0.clone(), mat.0.clone(), name));
        }
    }
    if found.is_empty() {
        return false;
    }
    let mut made: HashMap<(AssetId<StandardMaterial>, usize), Handle<ToonMaterial>> = HashMap::new();
    let mut hulls: HashMap<AssetId<Mesh>, Handle<Mesh>> = HashMap::new();
    for (x, mesh, mat, name) in found {
        let Some((k, spec)) = specs.iter().enumerate().find(|(_, s)| Some(*s) == spec_for(specs, name.as_deref())) else { continue };
        let Some(std) = world.resource::<Assets<StandardMaterial>>().get(&mat).cloned() else { continue };
        let mut base = std.clone();
        if let Some(c) = spec.color {
            base.base_color = c.with_alpha(std.base_color.alpha());
        }
        let colour_of = base.base_color;
        let h = match made.get(&(mat.id(), k)) {
            Some(h) => h.clone(),
            None => {
                let h = world.resource_mut::<Assets<ToonMaterial>>().add(material(base, spec, None));
                made.insert((mat.id(), k), h.clone());
                h
            }
        };
        world.entity_mut(x).remove::<MeshMaterial3d<StandardMaterial>>().insert(MeshMaterial3d(h));
        if let Some(o) = outline_params(spec, colour_of.with_alpha(1.0), None, 1.0) {
            let hull = match hulls.get(&mesh.id()) {
                Some(h) => Some(h.clone()),
                None => {
                    let m = world.resource::<Assets<Mesh>>().get(&mesh).and_then(hull_mesh);
                    m.map(|m| {
                        let h = world.resource_mut::<Assets<Mesh>>().add(m);
                        hulls.insert(mesh.id(), h.clone());
                        h
                    })
                }
            };
            if let Some(hull) = hull {
                spawn_hull(world, x, hull, o);
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cube's corners, each face with its own (flat) normal: the smoothed normals at a corner all agree, and point
    /// out along the diagonal, so the hull is closed.
    #[test]
    fn hull_normals_close_hard_edges() {
        let tris = crate::shape::cached("box(1, 1, 1)").unwrap();
        let t: Vec<[usize; 3]> = (0..tris.positions.len() / 3).map(|i| [3 * i, 3 * i + 1, 3 * i + 2]).collect();
        let n = smoothed_normals(&tris.positions, &t);
        for (p, n) in tris.positions.iter().zip(&n) {
            let want = Vec3::from(*p).signum().normalize();
            assert!(Vec3::from(*n).dot(want) > 0.999, "{p:?}: {n:?}");
        }
    }

    #[test]
    fn spec_parses_and_bands_order() {
        let mut vals = BTreeMap::new();
        vals.insert("toon".into(), V::S(r##"{"steps":3,"rim":{"k":0.8,"color":"#ffffff","width":0.25},"outline":{"width":3}}"##.into()));
        let s = ToonSpec::of(&vals).remove(0);
        let p = params(&s, Some(&vals));
        assert_eq!(p.shape.x, 3.0);
        assert!(p.starts[0][1] < p.starts[0][2]);
        assert!((p.rim.w - 0.8).abs() < 1e-6 && (p.shape.y - 0.25).abs() < 1e-6);
        assert_eq!(p.spec.w, 0.0);
        let o = outline_params(&s, Color::srgb(1.0, 0.85, 0.2), None, 1.0).unwrap();
        assert_eq!(o.params.shape.x, 3.0);
        // the ink is the colour's hue, dark
        assert!(o.params.color.x < 0.05 && o.params.color.x > o.params.color.z);
        let mut b = BTreeMap::new();
        b.insert("toon".into(), V::S(r##"{"bands":[[0,"#202040"],[0.3,null],[0.7,"#ffffff"]]}"##.into()));
        let p = params(&ToonSpec::of(&b)[0], None);
        assert_eq!((p.shape.x, p.starts[0][1], p.starts[0][2]), (3.0, 0.3, 0.7));
        assert_eq!((p.colors[0].w, p.colors[1].w, p.colors[2].w), (1.0, 0.0, 1.0));
    }

    #[test]
    fn model_specs_map_by_material_name() {
        let mut vals = BTreeMap::new();
        vals.insert("toon".into(), V::S(r##"[{"material":"Skin","color":"#ffd23f"},{"steps":3}]"##.into()));
        let s = ToonSpec::of(&vals);
        assert_eq!(spec_for(&s, Some("Skin")).unwrap().steps, 2);
        assert_eq!(spec_for(&s, Some("Eyes")).unwrap().steps, 3);
        assert_eq!(spec_for(&s, None).unwrap().steps, 3);
    }
}
