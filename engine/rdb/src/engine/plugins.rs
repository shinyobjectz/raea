//! Bevy plugins inside raea's engine (sdk/motion/docs/bevy-plugins.md). A plugin is compiled in by a crate that registers
//! it here (bevy-plugins/, behind cargo features); TSX drives it with generic rows, never a hand-written system per
//! component:
//!
//!   component  {target: a row's id, type: a reflected type path, value: JSON, from?: frame}
//!              the value deserialized through Bevy's TypeRegistry (TypedReflectDeserializer) and inserted with
//!              ReflectComponent::insert on the target's entity: at load, or once frame `from` is drawn
//!   resource   {type, value}   the same for a reflected resource, at load
//!   mesh ... driven            the mesh's Transform is the plugin's (a rigid body's), not the rows'
//!
//! Render equals play: with a plugin installed, Bevy's clock is the frame index. Every update that draws a new frame f
//! from frame f - 1 advances Time by exactly 1/fps (TimeUpdateStrategy::ManualDuration, Time<Fixed> at the same step,
//! so FixedUpdate runs once); every other update (the same frame drawn again while its readback comes back) advances it
//! by zero. A frame reached by a jump is stepped to through the frames between, and a frame before the last stepped one
//! reloads the edit and steps from 0, so a plugin's state at frame f is always f steps from the load, however f was
//! reached (a render of one frame, a play, a seek).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use bevy::app::PluginGroupBuilder;
use bevy::ecs::reflect::{AppTypeRegistry, ReflectComponent, ReflectFromWorld, ReflectResource};
use bevy::prelude::*;
use bevy::reflect::serde::{TypedReflectDeserializer, TypedReflectSerializer};
use bevy::reflect::std_traits::ReflectDefault;
use bevy::reflect::{PartialReflect, ReflectDeserialize, ReflectFromReflect, ReflectSerialize, TypeInfo, TypeRegistration, TypeRegistry, enums::VariantInfo};
use bevy::time::TimeUpdateStrategy;
use serde::de::DeserializeSeed;
use serde_json::{Value, json};

use crate::eval::Tup;
use crate::ir::V;

/// What a plugin crate registers: its name, what it adds to the App, and optionally a change to DefaultPlugins (a
/// plugin that switches on bevy_winit must turn WinitPlugin off: rdb drives its own updates and has no window).
pub struct Registered {
    pub name: &'static str,
    pub install: fn(&mut App),
    pub group: Option<fn(PluginGroupBuilder) -> PluginGroupBuilder>,
}

static PLUGINS: Mutex<Vec<Registered>> = Mutex::new(Vec::new());

/// Registers a plugin for every Engine made from now on (once per name).
pub fn register(r: Registered) {
    let mut p = PLUGINS.lock().unwrap();
    if !p.iter().any(|x| x.name == r.name) {
        p.push(r);
    }
}

/// The plugins registered, by name.
pub fn names() -> Vec<&'static str> {
    PLUGINS.lock().unwrap().iter().map(|r| r.name).collect()
}

pub(super) fn group(mut b: PluginGroupBuilder) -> PluginGroupBuilder {
    for r in PLUGINS.lock().unwrap().iter() {
        if let Some(g) = r.group {
            b = g(b);
        }
    }
    b
}

/// The frame clock plugins step by: the last frame stepped to, the step, and whether the coming update steps.
#[derive(Resource)]
pub struct PluginClock {
    pub stepped: i64,
    pub dt: Duration,
    pub advancing: bool,
    pub steps: u64,
}

/// A mesh whose Transform a plugin owns (sync_meshes leaves it alone).
#[derive(Component)]
pub struct Driven;

/// Component rows that wait for a frame: (entity, type, value, from).
#[derive(Resource, Default)]
struct TimedRows(Vec<(Entity, String, Value, i64)>);

/// Adds the registered plugins and the frame clock, when there are any (an App without plugins is left as it was).
pub(super) fn install(app: &mut App, fps: f64) {
    let list: Vec<fn(&mut App)> = PLUGINS.lock().unwrap().iter().map(|r| r.install).collect();
    if list.is_empty() {
        return;
    }
    let dt = Duration::from_secs_f64(1.0 / fps.max(1.0));
    for f in list {
        f(app);
    }
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.world_mut().resource_mut::<Time<Fixed>>().set_timestep(dt);
    app.insert_resource(PluginClock { stepped: 0, dt, advancing: false, steps: 0 });
    app.init_resource::<TimedRows>();
    app.add_systems(Update, timed_rows);
}

/// A run condition for a plugin's systems that must change state only when the frame steps (not on every update):
/// true on an update that draws the next frame, and always when no plugin clock runs.
pub fn advancing(c: Option<Res<PluginClock>>) -> bool {
    c.is_none_or(|c| c.advancing)
}

pub fn active(world: &World) -> bool {
    world.contains_resource::<PluginClock>()
}

/// Before an update: Time advances one frame when this update draws the frame after the last stepped, else not at all.
pub(super) fn arm(world: &mut World) {
    let f = world.resource::<super::Clock>().0;
    let Some(mut c) = world.get_resource_mut::<PluginClock>() else { return };
    let step = f == c.stepped + 1;
    c.advancing = step;
    if std::env::var_os("RAEA_PLUGIN_TRACE").is_some() {
        eprintln!("plugin clock: frame {f}, stepped {}, {}", c.stepped, if step { "step" } else { "hold" });
    }
    let dt = if step {
        c.stepped = f;
        c.steps += 1;
        c.dt
    } else {
        Duration::ZERO
    };
    world.insert_resource(TimeUpdateStrategy::ManualDuration(dt));
}

/// Component rows whose frame has come, inserted on the update that draws it (so the steps after it see them).
fn timed_rows(world: &mut World) {
    let (advancing, f) = match world.get_resource::<PluginClock>() {
        Some(c) => (c.advancing, c.stepped),
        None => return,
    };
    if !advancing {
        return;
    }
    let due: Vec<(Entity, String, Value)> = world.resource::<TimedRows>().0.iter().filter(|r| r.3 == f).map(|r| (r.0, r.1.clone(), r.2.clone())).collect();
    for (e, ty, v) in due {
        if let Err(err) = insert_reflect(world, e, &ty, &v) {
            world.resource_mut::<super::Error>().0 = Some(format!("component {ty}: {err}"));
        }
    }
}

fn s(vals: &BTreeMap<String, V>, k: &str) -> Option<String> {
    match vals.get(k) {
        Some(V::S(x)) => Some(x.clone()),
        _ => None,
    }
}

/// The edit's component and resource rows applied at load (the rows' entities are `ents`, in `inserted`'s order), the
/// timed ones kept for their frame, and the plugin clock back at frame 0.
pub(super) fn apply_rows(world: &mut World, inserted: &[Tup], ents: &[Entity]) -> Result<(), String> {
    if !active(world) {
        if inserted.iter().any(|t| t.kind == "component" || t.kind == "resource") {
            return Err(format!("the edit drives a Bevy plugin (component rows), and this build has none compiled in (registered: {:?})", names()));
        }
        return Ok(());
    }
    {
        let mut c = world.resource_mut::<PluginClock>();
        c.stepped = 0;
        c.advancing = false;
    }
    world.resource_mut::<TimedRows>().0.clear();
    let by_id: BTreeMap<&str, Entity> = inserted.iter().zip(ents).map(|(t, e)| (t.id.as_str(), *e)).collect();
    for (t, &e) in inserted.iter().zip(ents) {
        if t.kind == "mesh" && t.vals.get("driven").is_some_and(V::truthy) {
            world.entity_mut(e).insert(Driven);
        }
    }
    for t in inserted {
        let value = |t: &Tup| -> Result<Value, String> { serde_json::from_str(&s(&t.vals, "value").unwrap_or_else(|| "null".into())).map_err(|e| format!("{} {}: value isn't JSON: {e}", t.kind, t.id)) };
        match t.kind.as_str() {
            "component" => {
                let ty = s(&t.vals, "type").ok_or_else(|| format!("component {}: needs type", t.id))?;
                let target = s(&t.vals, "target").ok_or_else(|| format!("component {}: needs target (a row's id)", t.id))?;
                let &e = by_id.get(target.as_str()).ok_or_else(|| format!("component {}: no row {target}", t.id))?;
                let v = value(t)?;
                let from = t.vals.get("from").map(V::num).unwrap_or(0.0) as i64;
                if from > 0 {
                    world.resource_mut::<TimedRows>().0.push((e, ty, v, from));
                } else {
                    insert_reflect(world, e, &ty, &v).map_err(|err| format!("component {} ({ty}): {err}", t.id))?;
                }
            }
            "resource" => {
                let ty = s(&t.vals, "type").ok_or_else(|| format!("resource {}: needs type", t.id))?;
                insert_reflect_resource(world, &ty, &value(t)?).map_err(|err| format!("resource {} ({ty}): {err}", t.id))?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// A reflected value from JSON in the shape Bevy's reflect serde reads (a struct an object, a newtype its inner value,
/// an enum "Unit" or {"Variant": ...}, glam vectors [x, y, z]), checked to be buildable before ReflectComponent::insert
/// (which panics on a partial value with no Default).
fn value_of(reg: &TypeRegistry, r: &TypeRegistration, path: &str, json: &Value) -> Result<Box<dyn PartialReflect>, String> {
    let v = TypedReflectDeserializer::new(r, reg).deserialize(json.clone()).map_err(|e| format!("{path}: {e}"))?;
    let whole = r.data::<ReflectFromReflect>().and_then(|f| f.from_reflect(v.as_partial_reflect())).is_some();
    if !whole && r.data::<ReflectDefault>().is_none() && r.data::<ReflectFromWorld>().is_none() {
        return Err(format!("{path} has no default, so give every field"));
    }
    Ok(v)
}

fn registration<'a>(reg: &'a TypeRegistry, path: &str) -> Result<&'a TypeRegistration, String> {
    reg.get_with_type_path(path).or_else(|| reg.get_with_short_type_path(path)).ok_or_else(|| format!("no reflected type {path} (is its plugin compiled in?)"))
}

/// One component from JSON onto an entity, through the TypeRegistry: the generic path every plugin component takes.
pub fn insert_reflect(world: &mut World, e: Entity, path: &str, json: &Value) -> Result<(), String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let reg = registry.read();
    let r = registration(&reg, path)?;
    let rc = r.data::<ReflectComponent>().ok_or_else(|| format!("{path} isn't a component"))?;
    let v = value_of(&reg, r, path, json)?;
    rc.insert(&mut world.entity_mut(e), v.as_partial_reflect(), &reg);
    Ok(())
}

/// One resource from JSON, the same way.
pub fn insert_reflect_resource(world: &mut World, path: &str, json: &Value) -> Result<(), String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let reg = registry.read();
    let r = registration(&reg, path)?;
    if r.data::<ReflectResource>().is_none() {
        return Err(format!("{path} isn't a resource"));
    }
    let rc = r.data::<ReflectComponent>().ok_or_else(|| format!("{path} isn't reflected as a resource"))?;
    let v = value_of(&reg, r, path, json)?;
    let id = rc.register_component(world);
    let e = match world.resource_entities().get(id) {
        Some(e) => e,
        None => world.spawn_empty().id(),
    };
    rc.insert(&mut world.entity_mut(e), v.as_partial_reflect(), &reg);
    Ok(())
}

// ---- the registry as a schema (what bevy-plugins' generator turns into TypeScript)

fn field_list<'a>(it: impl Iterator<Item = (Option<&'static str>, &'static str)> + 'a) -> Vec<Value> {
    it.map(|(n, t)| match n {
        Some(n) => json!({"name": n, "type": t}),
        None => json!({"type": t}),
    }).collect()
}

fn shape_of(ti: &TypeInfo) -> Value {
    match ti {
        TypeInfo::Struct(s) => json!({"kind": "struct", "fields": field_list(s.iter().map(|f| (Some(f.name()), f.type_path())))}),
        TypeInfo::TupleStruct(s) => json!({"kind": "tuple_struct", "fields": field_list(s.iter().map(|f| (None, f.type_path())))}),
        TypeInfo::Tuple(s) => json!({"kind": "tuple", "fields": field_list(s.iter().map(|f| (None, f.type_path())))}),
        TypeInfo::List(l) => json!({"kind": "list", "item": l.item_ty().path()}),
        TypeInfo::Array(a) => json!({"kind": "array", "item": a.item_ty().path(), "len": a.capacity()}),
        TypeInfo::Map(m) => json!({"kind": "map", "key": m.key_ty().path(), "value": m.value_ty().path()}),
        TypeInfo::Set(s) => json!({"kind": "set", "item": s.value_ty().path()}),
        TypeInfo::Enum(e) => json!({"kind": "enum", "variants": e.iter().map(|v| match v {
            VariantInfo::Unit(u) => json!({"name": u.name(), "kind": "unit"}),
            VariantInfo::Tuple(t) => json!({"name": t.name(), "kind": "tuple", "fields": field_list(t.iter().map(|f| (None, f.type_path())))}),
            VariantInfo::Struct(s) => json!({"name": s.name(), "kind": "struct", "fields": field_list(s.iter().map(|f| (Some(f.name()), f.type_path())))}),
        }).collect::<Vec<_>>()}),
        TypeInfo::Opaque(_) => json!({"kind": "opaque"}),
    }
}

fn referenced(shape: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for k in ["item", "key", "value"] {
        if let Some(t) = shape.get(k).and_then(Value::as_str) {
            out.push(t.to_string());
        }
    }
    let mut fields = |fs: Option<&Value>| {
        for f in fs.and_then(Value::as_array).into_iter().flatten() {
            if let Some(t) = f.get("type").and_then(Value::as_str) {
                out.push(t.to_string());
            }
        }
    };
    fields(shape.get("fields"));
    for v in shape.get("variants").and_then(Value::as_array).into_iter().flatten() {
        fields(v.get("fields"));
    }
    out
}

/// The registry's components and resources whose type path `keep` accepts, and every type their fields reach: kind,
/// fields, enum variants, whether it's a component or resource, whether it has its own serde form, and its default
/// (serialized as the reflect deserializer reads it) when it has one.
pub fn schema(world: &World, keep: &dyn Fn(&str) -> bool) -> Value {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let reg = registry.read();
    let mut todo: Vec<String> = reg.iter().filter(|r| keep(r.type_info().type_path()) && (r.data::<ReflectComponent>().is_some() || r.data::<ReflectResource>().is_some())).map(|r| r.type_info().type_path().to_string()).collect();
    todo.sort();
    let roots = todo.clone();
    let mut types = serde_json::Map::new();
    while let Some(path) = todo.pop() {
        if types.contains_key(&path) {
            continue;
        }
        let Some(r) = reg.get_with_type_path(&path) else {
            types.insert(path.clone(), json!({"kind": "unregistered"}));
            continue;
        };
        let ti = r.type_info();
        let mut t = shape_of(ti);
        for p in referenced(&t) {
            if !types.contains_key(&p) {
                todo.push(p);
            }
        }
        let o = t.as_object_mut().unwrap();
        o.insert("short".into(), json!(ti.type_path_table().short_path()));
        o.insert("generic".into(), json!(!ti.generics().is_empty()));
        if r.data::<ReflectResource>().is_some() {
            o.insert("resource".into(), json!(true));
        } else if r.data::<ReflectComponent>().is_some() {
            o.insert("component".into(), json!(true));
        }
        if r.data::<ReflectSerialize>().is_some() && r.data::<ReflectDeserialize>().is_some() {
            o.insert("serde".into(), json!(true));
        }
        if let Some(d) = r.data::<ReflectDefault>() {
            let v = d.default();
            if let Ok(j) = serde_json::to_value(TypedReflectSerializer::new(v.as_partial_reflect(), &reg)) {
                o.insert("default".into(), j);
            }
        }
        types.insert(path, t);
    }
    json!({"roots": roots, "types": types})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.register_type::<Transform>().register_type::<Visibility>().register_type::<bevy::light::DirectionalLightShadowMap>();
        app
    }

    #[test]
    fn a_component_from_json_through_the_registry() {
        let mut app = app();
        let w = app.world_mut();
        let e = w.spawn_empty().id();
        // a struct with a default: the fields given, the rest default
        insert_reflect(w, e, "bevy_transform::components::transform::Transform", &json!({"translation": [1.0, 2.0, 3.0]})).unwrap();
        let t = w.get::<Transform>(e).unwrap();
        assert_eq!(t.translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(t.scale, Vec3::ONE);
        // a short path names it too; an enum is its variant's name
        insert_reflect(w, e, "Visibility", &json!("Hidden")).unwrap();
        assert_eq!(*w.get::<Visibility>(e).unwrap(), Visibility::Hidden);
        assert!(insert_reflect(w, e, "no::Such", &json!({})).unwrap_err().contains("no reflected type"));
        assert!(insert_reflect(w, e, "Transform", &json!({"translation": "up"})).is_err());
    }

    #[test]
    fn a_resource_from_json_and_the_schema() {
        let mut app = app();
        let w = app.world_mut();
        insert_reflect_resource(w, "bevy_light::directional_light::DirectionalLightShadowMap", &json!({"size": 512})).unwrap();
        assert_eq!(w.resource::<bevy::light::DirectionalLightShadowMap>().size, 512);
        let s = schema(w, &|p| p.starts_with("bevy_transform::components::transform::Transform"));
        let t = &s["types"]["bevy_transform::components::transform::Transform"];
        assert_eq!(t["kind"], "struct");
        assert_eq!(t["component"], true);
        assert_eq!(t["fields"][0]["name"], "translation");
        assert_eq!(t["default"]["scale"], json!([1.0, 1.0, 1.0]));
        // the types its fields reach come along
        assert!(s["types"].get("glam::Vec3").is_some());
    }
}
