//! The second UV set raea's bakes share (docs/wear.feature WR1, docs/bake.feature BK1): planar charts packed into
//! one atlas over a glTF's static meshes, and each texel's point on the surface. raea/wear's cause maps
//! (raea-sys-weathering) and the light bake's lightmaps (raea-sys-lightbake) both lie over it, so a model has one
//! TEXCOORD_1. Split out of raea-sys-weathering on 11-23 so both bakes can build on it; its bytes are the same.
//!
//!   scene.rs   glTF to triangles with a material class each, and back with TEXCOORD_1
//!   charts.rs  the charts, their shelf packing, each texel's point (merge.rs joins and folds them)
//!   rays.rs    the triangles shut inside a closed shell (no texels), and the samples the bakes' rays share

pub mod charts;
pub mod merge;
pub mod rays;
pub mod scene;

use parry3d::glamx::Vec3;
use serde_json::Value;

pub use scene::{Class, Classes};

pub type R<T> = Result<T, String>;

/// What an atlas is laid out for, from a bake request's `env`: the world's up and north (a wall's rows run down the
/// fall line, a level chart turns to its smallest rectangle), texels a metre (any class, and by class where a class
/// asks otherwise) and the atlas's longest side.
#[derive(Clone, Debug)]
pub struct Ask {
    pub up: Vec3,
    pub north: Vec3,
    pub density: f32,
    pub densities: Vec<(Class, f32)>,
    pub max_side: u32,
}

impl Ask {
    /// The atlas's fields of a request's `env`: {up, north, density (or {"*": d, CLASS: d}), max_side}, each optional.
    pub fn of(v: &Value) -> R<Ask> {
        let num = |k: &str, d: f64| v.pointer(k).and_then(Value::as_f64).unwrap_or(d) as f32;
        let ask = Ask {
            up: dir(v, "/up", Vec3::Y)?,
            north: dir(v, "/north", Vec3::NEG_Z)?,
            density: match v.get("density") { Some(Value::Object(o)) => o.get("*").and_then(Value::as_f64).unwrap_or(8.0) as f32, _ => num("/density", 8.0) },
            densities: match v.get("density") {
                Some(Value::Object(o)) => o
                    .iter()
                    .filter(|(k, _)| k.as_str() != "*")
                    .map(|(k, d)| Ok((Class::of(k).ok_or_else(|| format!("env.density.{k}: a class name"))?, d.as_f64().ok_or_else(|| format!("env.density.{k}: a number"))? as f32)))
                    .collect::<R<_>>()?,
                _ => Vec::new(),
            },
            max_side: num("/max_side", 4096.0) as u32,
        };
        for d in std::iter::once(ask.density).chain(ask.densities.iter().map(|(_, d)| *d)) {
            if !(0.25..=256.0).contains(&d) {
                return Err(format!("env.density: 0.25-256 texels a metre, got {d}"));
            }
        }
        if !(64..=16384).contains(&ask.max_side) {
            return Err(format!("env.max_side: 64-16384, got {}", ask.max_side));
        }
        Ok(ask)
    }

    /// Texels a metre for a surface of this class.
    pub fn density_of(&self, c: Class) -> f32 {
        self.densities.iter().find(|(k, _)| *k == c).map(|(_, d)| *d).unwrap_or(self.density)
    }
}

/// A direction in the request at pointer `k` (three numbers, normalized), or `d` when it isn't given.
pub fn dir(v: &Value, k: &str, d: Vec3) -> R<Vec3> {
    match v.pointer(k) {
        None => Ok(d),
        Some(a) => {
            let a = a.as_array().filter(|a| a.len() == 3).ok_or_else(|| format!("env{k}: three numbers"))?;
            let f = |i: usize| a[i].as_f64().map(|x| x as f32).ok_or_else(|| format!("env{k}: numbers"));
            Vec3::new(f(0)?, f(1)?, f(2)?).try_normalize().ok_or_else(|| format!("env{k}: not a direction"))
        }
    }
}

/// A bake request's model laid out: its triangles, their ray mesh, the atlas, and when the charts began (both bakes
/// start here, so the same request gives them the same atlas).
pub fn lay(bytes: &[u8], req: &Value) -> R<(scene::Scene, parry3d::shape::TriMesh, charts::Atlas, std::time::Instant)> {
    let ask = Ask::of(req.get("env").unwrap_or(&Value::Null))?;
    let sc = scene::read(bytes, &Classes::of(req.get("classes").unwrap_or(&Value::Null))?)?;
    let mesh = rays::mesh_of(&sc);
    let t = std::time::Instant::now();
    let at = charts::build(&sc, &ask, &mesh)?;
    Ok((sc, mesh, at, t))
}

/// A texel of the atlas and the surface point under it in world units (charts.rs: its centre's point when that's in a
/// triangle, else the nearest point within 0.75 of a texel of its chart's edge), the triangle's normal, the triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Texel {
    pub x: u16,
    pub y: u16,
    pub p: [f32; 3],
    pub n: [f32; 3],
    pub tri: u32,
}

/// The atlas alone, with no causes traced (docs/bake.feature BK1): the model again with its TEXCOORD_1, the atlas's
/// size and density, and every texel's point. Given a wear bake's request it is that bake's atlas (raea-sys-weathering's
/// test says so, byte for byte), so a light bake laid over it and the cause maps read one second UV set.
#[derive(Debug, PartialEq)]
pub struct Layout {
    pub glb: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub density: f32,
    pub charts: usize,
    pub hidden: usize,
    pub texels: Vec<Texel>,
}

pub fn layout(bytes: &[u8], req: &Value) -> R<Layout> {
    Ok(laid(bytes, req)?.0)
}

/// The layout with the model's triangles in world space, numbered as its texels number them (`Texel::tri`).
pub fn laid(bytes: &[u8], req: &Value) -> R<(Layout, parry3d::shape::TriMesh)> {
    let (sc, mesh, at, _) = lay(bytes, req)?;
    let texels = at.texels.iter().map(|t| Texel { x: t.x, y: t.y, p: t.p.to_array(), n: t.n.to_array(), tri: t.tri }).collect();
    Ok((Layout { glb: scene::write(&sc, &at)?, w: at.w, h: at.h, density: at.density, charts: at.charts, hidden: at.hidden, texels }, mesh))
}
