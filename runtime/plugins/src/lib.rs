//! Bevy plugins inside raea's engine (sdk/raea/docs/bevy-plugins.md). A plugin is compiled in by a crate that registers
//! it here (bevy-plugins/, behind cargo features); TSX drives it with generic rows, never a hand-written system per
//! component:
//!
//!   component  {target: a row's id, type: a reflected type path, value: JSON, from?: frame}
//!              the value deserialized through Bevy's TypeRegistry (TypedReflectDeserializer) and inserted with
//!              ReflectComponent::insert on the target's entity: at load, or once frame `from` is drawn (each frame
//!              takes the last row from at or before it, in every build: docs/control.feature FC1c)
//!   resource   {type, value, from?}   the same for a reflected resource
//!   mesh ... driven            the mesh's Transform is the plugin's (a rigid body's), not the rows'
//!
//! Render equals play: with a plugin installed, Bevy's clock is the frame index. Every update that draws a new frame f
//! from frame f - 1 advances Time by exactly 1/fps (TimeUpdateStrategy::ManualDuration, Time<Fixed> at the same step,
//! so FixedUpdate runs once); every other update (the same frame drawn again while its readback comes back) advances it
//! by zero. A frame reached by a jump is stepped to through the frames between, and a frame before the last stepped one
//! reloads the edit and steps from 0, so a plugin's state at frame f is always f steps from the load, however f was
//! reached (a render of one frame, a play, a seek).
//!
//! A load whose rows give the plugins nothing to carry from frame to frame (`steps` false) isn't stepped (#77): its
//! clock is `pure`, and Time and the keyed rows are the frame drawn's, f / fps from 0, however it was reached (#162).

use std::any::TypeId;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use bevy::app::PluginGroupBuilder;
use bevy::ecs::reflect::{AppTypeRegistry, ReflectComponent, ReflectEvent, ReflectFromWorld, ReflectMessage, ReflectResource};
use bevy::prelude::*;
use bevy::asset::{ReflectAsset, ReflectHandle, RecursiveDependencyLoadState, UntypedHandle};
use bevy::reflect::serde::{ReflectDeserializerProcessor, TypedReflectDeserializer, TypedReflectSerializer};
use bevy::reflect::std_traits::ReflectDefault;
use bevy::state::reflect::{ReflectFreelyMutableState, ReflectState};
use bevy::reflect::{PartialReflect, ReflectDeserialize, ReflectFromReflect, ReflectSerialize, TypeInfo, TypeRegistration, TypeRegistry, enums::VariantInfo};
use bevy::time::TimeUpdateStrategy;
use serde::de::DeserializeSeed;
use serde_json::{Value, json};

use raea_core_program::eval::Tup;
use raea_core_program::ir::V;

use raea_sys_frame::*;
use raea_sys_material::{mesh, shader_material};

mod world;
pub use world::*;
mod fields;
pub use fields::*;
mod choose;
pub use choose::*;
mod reflect;
pub use reflect::*;
pub mod character;
#[cfg(test)]
mod tests;

/// What a plugin crate registers: its name, what it adds to the App, and optionally a change to DefaultPlugins (a
/// plugin that switches on bevy_winit must turn WinitPlugin off: raea-render drives its own updates and has no window).
/// `on_demand` is for a plugin that only an App which names it gets (a network peer's server or client half): raea-render's
/// Engine leaves it out, and `install_named` adds it.
pub struct Registered {
    pub name: &'static str,
    pub install: fn(&mut App),
    pub group: Option<fn(PluginGroupBuilder) -> PluginGroupBuilder>,
    pub on_demand: bool,
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

/// Bevy's plugin group for one of raea's Engines (raea-render's, the page's BlitzPage): each registered plugin's change
/// to it, and Bevy's window plugin left out. The Engines are headless, and what shows one in a window makes that window
/// itself; WinitPlugin would make an event loop in every App, and winit allows one a process, so a process's second App
/// panicked "Failed to build event loop: RecreationAttempt" (#182: Linux, a build cargo had unified bevy_winit into).
/// It's compiled in on the desktop (Cargo.toml), so it's always here to leave out, however the build brought it.
pub fn group(b: PluginGroupBuilder) -> PluginGroupBuilder {
    group_of(b, None)
}

/// `group` for an Engine whose project chose its plugins (choose.rs): only the chosen ones' changes.
pub fn group_chosen(b: PluginGroupBuilder, chosen: &Chosen) -> PluginGroupBuilder {
    group_of(b, Some(chosen))
}

fn group_of(mut b: PluginGroupBuilder, chosen: Option<&Chosen>) -> PluginGroupBuilder {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if b.contains::<bevy::winit::WinitPlugin>() {
        b = b.disable::<bevy::winit::WinitPlugin>();
    }
    for r in PLUGINS.lock().unwrap().iter().filter(|r| !r.on_demand && chosen.is_none_or(|c| c.adds(r.name))) {
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
    /// the load gives the plugins nothing to step (`steps` false): nothing is stepped, and Time and the keyed rows are
    /// the frame drawn's however it was reached (pure_time, keyed_rows). Stepping only on the frame after the last
    /// stepped one froze them: a still at frame 200 sampled globals.time 0, and a batch's second frame drew the
    /// first's <Material> uniforms (#162)
    pub pure: bool,
}

/// A mesh whose Transform a plugin owns (sync_meshes leaves it alone).
#[derive(Component)]
pub struct Driven;

/// The edit's component and resource rows keyed to frames (docs/control.feature FC1c): each one's target (None: a
/// resource), type, value and first frame, and the frame of the row last applied for each target and type. A frame
/// takes, for each, the last row from at or before it, so a seek (a still, live play's jump) lands on its value.
#[derive(Resource, Default)]
struct Keyed {
    /// in order of the frame each is from (load sorts them), so a frame reads only the rows due since the last
    rows: Vec<(Option<Entity>, String, Value, i64)>,
    applied: std::collections::HashMap<(Option<Entity>, String), i64>,
    /// the rows before this one are due by `last`, the frame last applied (a frame before it reads them all again)
    next: usize,
    last: i64,
}

/// Adds the registered plugins (all but the on-demand ones, and of the catalog's only those in the App's Chosen when it
/// has one) and the frame clock, when there are any. An App without plugins is left as it was.
pub fn install(app: &mut App, fps: f64) {
    // the keyboard's and mouse buttons' state as resources TypeScript can read and write (Bevy's InputPlugin adds
    // them without registering them for reflection; the gamepad's ButtonInput it registers itself)
    app.register_type::<ButtonInput<KeyCode>>();
    app.register_type::<ButtonInput<MouseButton>>();
    // a drawn part's layer key, a component row from its source stamp (docs/review.feature R2a)
    app.register_type::<raea_sys_frame::layer::LayerKey>();
    // a Range field (a Viewport's depth) is reflected opaque without serde's data: given it, it reads as {start, end}
    app.register_type_data::<core::ops::Range<f32>, ReflectDeserialize>();
    app.register_type_data::<core::ops::Range<f32>, ReflectSerialize>();
    // a live game's world calls (sdk/raea/src/world.ts): queued by its TypeScript, run here at the update's start
    // (engine/bevy-world, the layer BevyApp programs and pages share), entities named as the edit names them
    raea_bevy_world::register_natives();
    // the queue is the process's: React's App in this process leaves it to this one (its names resolve here)
    raea_bevy_world::claim();
    app.add_systems(First, raea_bevy_world::drain);
    world_methods(app);
    character::register(app);
    // materials written in TypeScript (<ShaderMaterial>, shader_material.rs): the render Engine only
    shader_material::install(app);
    keyed(app);
    let chosen = app.world().get_resource::<Chosen>().cloned();
    let list: Vec<fn(&mut App)> = PLUGINS.lock().unwrap().iter().filter(|r| !r.on_demand && chosen.as_ref().is_none_or(|c| c.adds(r.name))).map(|r| r.install).collect();
    install_list(app, fps, list);
}

/// Adds the plugins named, in that order, and the frame clock: a host's own headless App (raea-bevy-plugins' bridge,
/// which TypeScript drives) gets exactly the plugins it asks for. An unregistered name is an error that lists the
/// registered ones.
pub fn install_named(app: &mut App, fps: f64, names: &[&str]) -> Result<(), String> {
    let mut list = Vec::new();
    {
        let p = PLUGINS.lock().unwrap();
        for n in names {
            let r = p.iter().find(|r| r.name == *n).ok_or_else(|| format!("no plugin {n:?} in this build (it has: {})", p.iter().map(|r| r.name).collect::<Vec<_>>().join(", ")))?;
            list.push(r.install);
        }
    }
    world_methods(app);
    character::register(app);
    keyed(app);
    install_list(app, fps, list);
    Ok(())
}

/// Keyed component and resource rows, in every build: with a plugin or without one (docs/control.feature FC1c)
fn keyed(app: &mut App) {
    app.init_resource::<Keyed>();
    app.add_systems(Update, keyed_rows);
}

/// A live game's component and resource rows (Engine::set_rows), each as its live values now stand: a component's
/// value set on the row it targets (patched over what the part has), a resource's on the World. A live frame's
/// <BevyComponent value> may change every frame, as a render's are set at load.
pub fn apply_live_rows(world: &mut World, rows: &[(String, String)]) -> Result<(), String> {
    for (kind, id) in rows {
        let Some(e) = raea_sys_frame::store::find(world, kind, id) else { continue };
        let vals = {
            let fr = world.resource::<FrameRows>();
            match fr.ix.get(&e).and_then(|r| fr.live.as_ref().and_then(|l| l.get(*r))) {
                Some(v) => v.clone(),
                None => continue,
            }
        };
        let ty = s(&vals, "type").ok_or_else(|| format!("{kind} {id}: needs type"))?;
        let v: Value = serde_json::from_str(&s(&vals, "value").unwrap_or_else(|| "null".into())).map_err(|e| format!("{kind} {id}: value isn't JSON: {e}"))?;
        if kind == "resource" {
            insert_reflect_resource(world, &ty, &v).map_err(|err| format!("resource {id} ({ty}): {err}"))?;
            continue;
        }
        let target = s(&vals, "target").ok_or_else(|| format!("component {id}: needs target (a row's id)"))?;
        let found = raea_sys_frame::store::rows(world).iter().copied().find(|r| raea_sys_frame::store::key(world, *r).is_some_and(|k| k.id == target));
        let Some(t) = found else { return Err(format!("component {id}: no row {target}")) };
        insert_reflect(world, t, &ty, &v).map_err(|err| format!("component {id} ({ty}): {err}"))?;
    }
    Ok(())
}

fn install_list(app: &mut App, fps: f64, list: Vec<fn(&mut App)>) {
    if list.is_empty() {
        return;
    }
    let dt = Duration::from_secs_f64(1.0 / fps.max(1.0));
    for f in list {
        f(app);
    }
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.world_mut().resource_mut::<Time<Fixed>>().set_timestep(dt);
    app.insert_resource(PluginClock { stepped: 0, dt, advancing: false, steps: 0, pure: false });
    app.add_systems(First, pure_time.after(bevy::time::TimeSystems));
}

/// A pure clock's Time: the frame drawn's, f / fps from 0, whatever was drawn before it (a still alone, a batch's
/// next frame, a seek back in live play); Time<Virtual> too, which the fixed loop copies back into Time. It moves by
/// one step on an update that draws another frame than the last, and by none on the same frame drawn again.
fn pure_time(clock: Option<Res<Clock>>, pc: Option<Res<PluginClock>>, mut virt: ResMut<Time<Virtual>>, mut time: ResMut<Time>, mut last: Local<Option<i64>>) {
    let (Some(f), Some(c)) = (clock, pc) else { return };
    if !c.pure {
        *last = None;
        return;
    }
    let f = f.0.max(0);
    let at = c.dt * u32::try_from(f).unwrap_or(u32::MAX);
    let step = if *last == Some(f) { Duration::ZERO } else { c.dt.min(at) };
    *last = Some(f);
    let mut v = Time::<Virtual>::default();
    v.advance_by(at - step);
    v.advance_by(step);
    *time = v.as_generic();
    *virt = v;
}

pub fn active(world: &World) -> bool {
    world.contains_resource::<PluginClock>()
}

/// Whether an edit's rows give a plugin anything to carry from frame to frame: a driven mesh, a kind not known to be
/// frame-pure (a compute's buffers), or a component or resource row of a type from outside Bevy's own
/// scene and render crates and raea's (a plugin's: a rigid body, a particle effect; Bevy's animation, time or states).
/// Keyed rows land on their frame however it's reached, so without one a frame is the same however it was reached,
/// and a still at frame F needn't draw the F frames before it: a film's --at 2448 drew all of them, 217 s (#77).
pub fn steps(inserted: &[Tup]) -> bool {
    const PURE: &[&str] = &["camera", "light", "mesh", "model", "panel", "image", "audio", "master", "media", "text", "glyph", "decal", "fogvolume", "plate", "screenpage", "shape2d", "splat", "meshseq", "surface_shader", "entity"];
    const BEVY: &[&str] = &["bevy_camera", "bevy_ecs", "bevy_light", "bevy_mesh", "bevy_pbr", "bevy_post_process", "bevy_core_pipeline", "bevy_anti_alias", "bevy_render", "bevy_transform", "bevy_color", "bevy_image", "bevy_sprite", "bevy_text", "bevy_ui", "bevy_solari", "bevy_window"];
    // a short name could be anyone's: it steps
    let pure_type = |ty: &str| ["material:", "own:", "three:", "raea_"].iter().any(|p| ty.starts_with(p)) || (ty.contains("::") && BEVY.contains(&ty.split("::").next().unwrap_or("")));
    inserted.iter().any(|t| match t.kind.as_str() {
        "component" | "resource" => !s(&t.vals, "type").is_some_and(|ty| pure_type(&ty)),
        "mesh" => t.vals.get("driven").is_some_and(V::truthy),
        k => !PURE.contains(&k),
    })
}

/// Before an update: Time advances one frame when this update draws the frame after the last stepped, else not at all.
pub fn arm(world: &mut World) {
    let f = world.resource::<Clock>().0;
    let Some(mut c) = world.get_resource_mut::<PluginClock>() else { return };
    if c.pure {
        // nothing stepped: Time is the frame's (pure_time), and its real clock doesn't move
        c.advancing = false;
        world.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        return;
    }
    let step = f == c.stepped + 1;
    c.advancing = step;
    if raea_io_trace::stat("RAEA_PLUGIN_TRACE") {
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

/// Keyed rows whose frame has come, set on the update that draws it (so a plugin's steps after it see them): with a
/// plugin that steps, on the frame its clock steps to; without one, or with a pure clock, on the frame drawn, whichever
/// way it was reached.
/// A live engine (a fed render's, live play's) takes them too: its frames' rows set the values that change
/// (apply_live_rows), but a row held for its `from` never changes after the load, so nothing else sets it (a render to
/// MP4 left a rigid body from frame 36 asleep for good). Each lands once, on its frame.
fn keyed_rows(world: &mut World) {
    let f = match world.get_resource::<PluginClock>() {
        // a pure clock steps nothing: the frame drawn's rows, as without a plugin (#162)
        Some(c) if c.pure => match world.get_resource::<Clock>() {
            Some(c) => c.0,
            None => return,
        },
        Some(c) if !c.advancing => return,
        Some(c) => c.stepped,
        None => match world.get_resource::<Clock>() {
            Some(c) => c.0,
            None => return,
        },
    };
    let due: Vec<(Option<Entity>, String, Value, i64)> = {
        let mut k = world.resource_mut::<Keyed>();
        // forwards, only the rows that came due since the last frame (mesh_ray_cast's 66 gizmos keyed every frame were
        // 59 000 rows read each update by frame 900); a frame before the last reads them all again
        let from = if f >= k.last { k.next } else { 0 };
        let upto = from + k.rows[from..].partition_point(|r| r.3 <= f);
        (k.next, k.last) = (upto, f);
        let k = &*k;
        let mut last: std::collections::HashMap<(Option<Entity>, &str), &(Option<Entity>, String, Value, i64)> = Default::default();
        for r in &k.rows[from..upto] {
            let at = last.entry((r.0, r.1.as_str())).or_insert(r);
            if r.3 >= at.3 {
                *at = r;
            }
        }
        last.into_values().filter(|r| k.applied.get(&(r.0, r.1.clone())) != Some(&r.3)).cloned().collect()
    };
    for (e, ty, v, from) in due {
        let done = match e {
            Some(e) => insert_reflect(world, e, &ty, &v).map_err(|err| format!("component {ty}: {err}")),
            None => insert_reflect_resource(world, &ty, &v).map_err(|err| format!("resource {ty}: {err}")),
        };
        if let Err(err) = done {
            world.resource_mut::<Error>().0 = Some(err);
        }
        world.resource_mut::<Keyed>().applied.insert((e, ty), from);
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
pub fn apply_rows(world: &mut World, inserted: &[Tup], ents: &[Entity], dir: &Path) -> Result<(), String> {
    // what a Handle or an Entity field resolves against: the edit's folder, its media rows, and each row by id and name
    let mut by_name: BTreeMap<String, Entity> = BTreeMap::new();
    for (t, &e) in inserted.iter().zip(ents) {
        if let Some(n) = s(&t.vals, "name") {
            by_name.entry(n).or_insert(e);
        }
    }
    for (t, &e) in inserted.iter().zip(ents) {
        by_name.insert(t.id.clone(), e);
    }
    let media = inserted.iter().filter(|t| t.kind == "media").cloned().collect();
    world.insert_resource(Refs { dir: dir.to_path_buf(), names: by_name, media });
    world.insert_resource(raea_bevy_world::EntityNames(row_names));
    world.insert_resource(Loading::default());
    if active(world) {
        let mut c = world.resource_mut::<PluginClock>();
        c.stepped = 0;
        c.advancing = false;
    }
    let by_id: BTreeMap<&str, Entity> = inserted.iter().zip(ents).map(|(t, e)| (t.id.as_str(), *e)).collect();
    for (t, &e) in inserted.iter().zip(ents) {
        if t.kind == "mesh" && t.vals.get("driven").is_some_and(V::truthy) {
            world.entity_mut(e).insert(Driven);
        }
    }
    // Bevy's own reflected components and resources (a camera's Projection, a mesh's NotShadowReceiver) and a plugin's:
    // those from frame 0 now, the rest kept for their frame (keyed_rows), in every build
    let mut keyed = Keyed::default();
    for t in inserted.iter().filter(|t| t.kind == "component" || t.kind == "resource") {
        let ty = s(&t.vals, "type").ok_or_else(|| format!("{} {}: needs type", t.kind, t.id))?;
        let v: Value = serde_json::from_str(&s(&t.vals, "value").unwrap_or_else(|| "null".into())).map_err(|e| format!("{} {}: value isn't JSON: {e}", t.kind, t.id))?;
        let target = if t.kind == "resource" {
            None
        } else {
            let target = s(&t.vals, "target").ok_or_else(|| format!("component {}: needs target (a row's id)", t.id))?;
            Some(*by_id.get(target.as_str()).ok_or_else(|| format!("component {}: no row {target}", t.id))?)
        };
        let from = t.vals.get("from").map(V::num).unwrap_or(0.0) as i64;
        if from <= 0 {
            match target {
                Some(e) => insert_reflect(world, e, &ty, &v).map_err(|err| format!("component {} ({ty}): {err}", t.id))?,
                None => insert_reflect_resource(world, &ty, &v).map_err(|err| format!("resource {} ({ty}): {err}", t.id))?,
            }
            keyed.applied.insert((target, ty.clone()), 0);
        }
        keyed.rows.push((target, ty, v, from.max(0)));
    }
    // kept only when a row waits for a frame (a still edit's rows are all applied now); in frame order for keyed_rows
    keyed.rows.sort_by_key(|r| r.3);
    if !keyed.rows.iter().any(|r| r.3 > 0) {
        keyed = Keyed::default();
    }
    world.insert_resource(keyed);
    Ok(())
}
