//! A model's surfaces re-covered (<Model> with <MeshStandardMaterial> children): each of its glTF materials (all, or
//! the one a spec names by `material`) given the colour, texture or finish the spec gives, the rest of it kept. A
//! cabinet's wood repainted, a bottle's label swapped, a chair's fabric changed without touching its file.
//!
//! The model row's `surfaces` column: a JSON list of objects of a mesh row's material columns (color, rough, metal,
//! map, normal_map, rough_map, metal_map, mr_map, ao_map, repeat_u/v, emissive, emissive_k, opacity, clearcoat, ...)
//! and `material`. Applied once the model's scene has its meshes, before the first frame is drawn (await_models), and
//! before its toon surfaces, which then go over these.

use super::*;

#[derive(Clone)]
pub(super) struct Surface {
    material: Option<String>,
    /// the spec's columns made into a material (its textures loaded); only the parts the spec gives are used
    made: StandardMaterial,
    given: Vec<String>,
}

/// The row's `surfaces` column, each made into a material now (its pictures read as a mesh's are).
pub(super) fn of(world: &mut World, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<Vec<Surface>, String> {
    let Some(V::S(s)) = t.vals.get("surfaces") else { return Ok(Vec::new()) };
    let list: Vec<serde_json::Map<String, serde_json::Value>> = serde_json::from_str(s).map_err(|e| format!("model {}: surfaces: {e}", t.id))?;
    let mut out = Vec::new();
    for spec in list {
        let mut vals = BTreeMap::new();
        for (k, v) in &spec {
            match v {
                serde_json::Value::String(s) => vals.insert(k.clone(), V::S(s.clone())),
                serde_json::Value::Number(n) => vals.insert(k.clone(), V::N(n.as_f64().unwrap_or(0.0))),
                _ => None,
            };
        }
        let given: Vec<String> = vals.keys().cloned().collect();
        let made = scene3d::material(world, &Tup { kind: "mesh".into(), id: t.id.clone(), vals }, inserted, dir)?;
        out.push(Surface { material: spec.get("material").and_then(serde_json::Value::as_str).map(str::to_string), made, given });
    }
    Ok(out)
}

/// The spec for a glTF material by name: the one naming it, else the one naming none.
fn spec_for<'a>(specs: &'a [Surface], name: Option<&str>) -> Option<(usize, &'a Surface)> {
    specs.iter().enumerate().find(|(_, s)| s.material.is_some() && s.material.as_deref() == name).or_else(|| specs.iter().enumerate().find(|(_, s)| s.material.is_none()))
}

/// A glTF material with what a spec gives laid over it.
fn over(base: &StandardMaterial, s: &Surface) -> StandardMaterial {
    let mut m = base.clone();
    let has = |k: &str| s.given.iter().any(|g| g == k);
    if has("color") {
        m.base_color = s.made.base_color.with_alpha(base.base_color.alpha());
    }
    if has("opacity") {
        m.base_color.set_alpha(s.made.base_color.alpha());
        m.alpha_mode = AlphaMode::Blend;
    }
    if has("map") {
        m.base_color_texture = s.made.base_color_texture.clone();
        // the picture's own colours, unless a colour is given too
        if !has("color") {
            m.base_color = Color::WHITE.with_alpha(m.base_color.alpha());
        }
    }
    if has("normal_map") {
        m.normal_map_texture = s.made.normal_map_texture.clone();
    }
    if has("rough") {
        m.perceptual_roughness = s.made.perceptual_roughness;
    }
    if has("metal") {
        m.metallic = s.made.metallic;
    }
    if has("rough_map") || has("metal_map") || has("mr_map") {
        m.metallic_roughness_texture = s.made.metallic_roughness_texture.clone();
        // a map's factor is 1 unless given (three.js and glTF's rule)
        if !has("rough") {
            m.perceptual_roughness = 1.0;
        }
        if !has("metal") {
            m.metallic = if has("metal_map") || has("mr_map") { 1.0 } else { 0.0 };
        }
    }
    if has("ao_map") {
        m.occlusion_texture = s.made.occlusion_texture.clone();
    }
    if has("emissive") || has("emissive_k") {
        m.emissive = s.made.emissive;
    }
    if has("repeat_u") || has("repeat_v") {
        m.uv_transform = s.made.uv_transform;
    }
    for (k, f) in [("clearcoat", 0), ("clearcoat_rough", 1), ("reflectance", 2)] {
        if has(k) {
            match f {
                0 => m.clearcoat = s.made.clearcoat,
                1 => m.clearcoat_perceptual_roughness = s.made.clearcoat_perceptual_roughness,
                _ => m.reflectance = s.made.reflectance,
            }
        }
    }
    m
}

/// A model's meshes re-covered once its scene is spawned, each shared glTF material swapped once per spec. Returns
/// whether it found the meshes.
pub(super) fn apply_to_model(world: &mut World, root: Entity, specs: &[Surface]) -> bool {
    let mut stack = vec![root];
    let mut found = Vec::new();
    while let Some(x) = stack.pop() {
        if let Some(c) = world.get::<Children>(x) {
            stack.extend(c.iter());
        }
        if let Some(mat) = world.get::<MeshMaterial3d<StandardMaterial>>(x) {
            let name = world.get::<bevy::gltf::GltfMaterialName>(x).map(|n| n.0.clone());
            found.push((x, mat.0.clone(), name));
        }
    }
    if found.is_empty() {
        return false;
    }
    let mut made: HashMap<(AssetId<StandardMaterial>, usize), Handle<StandardMaterial>> = HashMap::new();
    for (x, mat, name) in found {
        let Some((k, spec)) = spec_for(specs, name.as_deref()) else { continue };
        let h = match made.get(&(mat.id(), k)) {
            Some(h) => h.clone(),
            None => {
                let Some(base) = world.resource::<Assets<StandardMaterial>>().get(&mat).cloned() else { continue };
                let h = world.resource_mut::<Assets<StandardMaterial>>().add(over(&base, spec));
                made.insert((mat.id(), k), h.clone());
                h
            }
        };
        world.entity_mut(x).insert(MeshMaterial3d(h));
    }
    true
}
