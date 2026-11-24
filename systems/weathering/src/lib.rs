//! Cause maps baked from a glTF's geometry (docs/wear.feature WR1-WR3): for each texel of a second UV set this bake
//! adds, what weathers that point of the surface, traced from the causes as Dorsey, Pedersen and Hanrahan (1996)
//! trace rain and Chen et al. (2005) trace their gamma-tons: where rain reaches and runs, what the sun and the sky
//! see, edges and hollows, the height over the ground, where hands and feet touch, what the water carries and leaves.
//! raea/wear's recipes read the maps, one WESL function per material class.
//!
//!   in:  a .glb (static meshes), the environment (`Env::of`), the material classes (`scene::Classes::of`)
//!   out: in the store under the request's content key: model.glb (TEXCOORD_1 added, charts.rs), causes0-3.png,
//!        meta.json; the same request gives the same bytes
//!
//! causes0 = (sky: rain reach, sun: share of the sun's path seen, cavity: occlusion within CAVITY_REACH, edge:
//! nearness to a convex arris); causes1 = (flow, deposit, deposit kind, touch); causes2 = (ground: height over the
//! walking surface, 0-4 m; element: the chart's own number; open: the share of the whole sky seen; class: the
//! triangle's material class as `Class::code`); causes3 = (winter sun, drift, litter, crack: 1 - the distance to the
//! nearest crack from an opening's corner over raea-sys-damage's REACH).

mod flow;
mod maps;
mod rays;
mod touch;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::time::Instant;

use parry3d::glamx::Vec3;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub use raea_sys_atlas::{Ask, Class, Classes};
pub(crate) use raea_sys_atlas::{charts, scene};

type R<T> = Result<T, String>;

/// Bumped when a bake's bytes would change for the same request.
const VERSION: &str = "raea-sys-weathering 7";

/// The world the bake's causes come from.
#[derive(Clone, Debug)]
pub struct Env {
    pub up: Vec3,
    pub north: Vec3,
    /// the way rain falls (unit), and its cone's half-angle in radians
    pub rain: Vec3,
    pub spread: f32,
    /// degrees north (the sun's path is the whole year's, and December's for causes3)
    pub lat: f32,
    /// the way driven snow travels in a storm (unit), and its cone's half-angle in radians
    pub snow: Vec3,
    pub snow_spread: f32,
    /// the walking surfaces: triangles of these classes, or a level at this height
    pub walk: Walk,
    /// texels a metre asked (any class), by class where a class asks otherwise, and the atlas's longest side
    pub density: f32,
    pub densities: Vec<(Class, f32)>,
    pub max_side: u32,
    pub sky_rays: u32,
    pub cavity_rays: u32,
}

/// Driven snow in a Berlin winter storm, ASSUMED until the DWD's records are read: from the east-north-east (cold
/// continental air), falling 60 degrees off the vertical.
fn storm(up: Vec3, north: Vec3) -> Vec3 {
    let east = north.cross(up).normalize();
    let from = (east * 22.5f32.to_radians().cos() + north * 22.5f32.to_radians().sin()).normalize();
    (-from * 60f32.to_radians().sin() - up * 60f32.to_radians().cos()).normalize()
}

#[derive(Clone, Debug)]
pub enum Walk {
    Classes(Vec<Class>),
    Level(f32),
}

impl Env {
    /// The request's `env` (every field optional): {up, north, rain: {dir, spread (degrees)}, sun: {lat}, snow: {dir,
    /// spread} (a storm's driven snow; the way it travels), walk: [class...] | {y}, density (or {"*": d, CLASS: d}),
    /// max_side, rays: {sky, cavity}}
    pub fn of(v: &Value) -> R<Env> {
        let ask = Ask::of(v)?;
        let v3 = |k: &str, d: Vec3| raea_sys_atlas::dir(v, k, d);
        let num = |k: &str, d: f64| v.pointer(k).and_then(Value::as_f64).unwrap_or(d) as f32;
        let walk = match v.get("walk") {
            None => Walk::Classes(vec![Class::Ground]),
            Some(Value::Array(a)) => Walk::Classes(
                a.iter()
                    .map(|c| c.as_str().and_then(Class::of).ok_or_else(|| format!("env.walk: a class name, got {c}")))
                    .collect::<R<_>>()?,
            ),
            Some(w) => Walk::Level(w.get("y").and_then(Value::as_f64).ok_or("env.walk: [class...] or {y}")? as f32),
        };
        let env = Env {
            up: ask.up,
            north: ask.north,
            rain: v3("/rain/dir", Vec3::NEG_Y)?,
            spread: num("/rain/spread", 30.0).clamp(0.0, 80.0).to_radians(),
            lat: num("/sun/lat", 52.5).clamp(-89.0, 89.0),
            snow: v3("/snow/dir", storm(ask.up, ask.north))?,
            snow_spread: num("/snow/spread", 25.0).clamp(0.0, 80.0).to_radians(),
            walk,
            density: ask.density,
            densities: ask.densities,
            max_side: ask.max_side,
            sky_rays: num("/rays/sky", 32.0).clamp(4.0, 512.0) as u32,
            cavity_rays: num("/rays/cavity", 8.0).clamp(2.0, 128.0) as u32,
        };
        Ok(env)
    }

    /// What the atlas is laid out for (raea_sys_atlas::Ask): the same fields, so `layout` with this request agrees.
    pub fn ask(&self) -> Ask {
        Ask { up: self.up, north: self.north, density: self.density, densities: self.densities.clone(), max_side: self.max_side }
    }
}

/// The bake of `src` (a .glb) under the request {env, classes}, kept in `store` by content.
pub fn bake(store: &Path, src: &Path, req: &Value) -> R<Value> {
    let bytes = std::fs::read(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let mut key = Sha256::new();
    key.update(VERSION.as_bytes());
    key.update(raea_sys_runoff::VERSION.as_bytes());
    key.update(raea_sys_touch::VERSION.as_bytes());
    key.update(raea_sys_exposure::VERSION.as_bytes());
    key.update(raea_sys_damage::VERSION.as_bytes());
    key.update(canonical(&strip_src(req)).as_bytes());
    key.update(&bytes);
    let key: String = key.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect();
    let dir = store.join("wear").join(&key);
    let meta = dir.join("meta.json");
    if let Ok(s) = std::fs::read_to_string(&meta) {
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            return Ok(v);
        }
    }
    let t0 = Instant::now();
    let (env, sc, mesh, at, t1) = lay(&bytes, req).map_err(|e| format!("{}: {e}", src.display()))?;
    let t2 = Instant::now();
    let n = (at.w * at.h) as usize;
    let (mut c0, mut c1, mut c2, mut c3) = (vec![[0u8; 4]; n], vec![[0u8; 4]; n], vec![[0u8; 4]; n], vec![[0u8; 4]; n]);
    rays::causes(&sc, &at, &env, &mesh, &mut c0, &mut c2, &mut c3);
    flow::flow(&sc, &at, &env, &mesh, &c0, &mut c1);
    let (spots, crack) = touch::touch(&sc, &at, &env, &mesh, &c0, &mut c1, &c2);
    for (tx, b) in at.texels.iter().zip(crack) {
        c3[at.px(tx.x, tx.y)][3] = b;
    }
    let t3 = Instant::now();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let model = dir.join("model.glb");
    std::fs::write(&model, scene::write(&sc, &at)?).map_err(|e| format!("{}: {e}", model.display()))?;
    let covered = at.covered();
    let mut files = Map::new();
    for (name, mut m) in [("causes0", c0), ("causes1", c1), ("causes2", c2), ("causes3", c3)] {
        let exact: &[usize] = if name == "causes2" { &[1, 3] } else { &[] };
        maps::dilate(&mut m, &mut covered.clone(), at.w, at.h, charts::GUTTER, exact);
        let p = dir.join(format!("{name}.png"));
        maps::png(&p, &m, at.w, at.h)?;
        files.insert(name.into(), json!(p.display().to_string()));
    }
    let sp = dir.join("spots.json");
    std::fs::write(&sp, spots.to_string()).map_err(|e| format!("{}: {e}", sp.display()))?;
    let ms = |a: Instant, b: Instant| (b - a).as_millis() as u64;
    let out = json!({
        "key": key,
        "file": model.display().to_string(),
        "maps": files,
        "spots": sp.display().to_string(),
        "size": [at.w, at.h],
        "density": at.density,
        "asked_density": env.density,
        "charts": at.charts,
        "hidden_tris": at.hidden,
        "texels": at.texels.len(),
        "tris": sc.tri.len(),
        "ms": { "read": ms(t0, t1), "charts": ms(t1, t2), "causes": ms(t2, t3), "write": ms(t3, Instant::now()) },
    });
    std::fs::write(&meta, out.to_string()).map_err(|e| format!("{}: {e}", meta.display()))?;
    Ok(out)
}

/// The request's world, and its model laid out as raea_sys_atlas::layout lays it (the same atlas, by construction).
fn lay(bytes: &[u8], req: &Value) -> R<(Env, scene::Scene, parry3d::shape::TriMesh, charts::Atlas, Instant)> {
    let env = Env::of(req.get("env").unwrap_or(&Value::Null))?;
    let (sc, mesh, at, t) = raea_sys_atlas::lay(bytes, req)?;
    Ok((env, sc, mesh, at, t))
}

fn strip_src(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("src");
    }
    v
}

/// JSON with every object's keys sorted, so a key doesn't depend on how the request was written.
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let mut ks: Vec<&String> = o.keys().collect();
            ks.sort();
            let parts: Vec<String> = ks.iter().map(|k| format!("{}:{}", Value::String((*k).clone()), canonical(&o[*k]))).collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical).collect::<Vec<_>>().join(",")),
        v => v.to_string(),
    }
}
