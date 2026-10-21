//! @raea/motion's native calls (`__raea_call(name, json) -> json`), on the JS thread: Manifold solids held as handles
//! (`/3d`'s `manifold`), and ECS worlds on Bevy's own ECS (`/ecs`, ecs.rs). TypeScript gets typed objects; the geometry,
//! the storage and the filtering are Rust.
//!
//!   solid.make    {fn, args}            -> {id, expr}        box, rounded_box, sphere, cylinder, cone, torus, text3d
//!   solid.combine {fn, ids}             -> {id, expr}        union, minus, intersect, hull
//!   solid.move    {id, fn, args}        -> {id, expr}        move, rotate, scale
//!   solid.query   {id, what, z?}        -> ...               volume, bounds, triangles, slice (area, contours, polygons)
//!   solid.op      {id, fn, args, other?} -> {id, expr}        mirror, transform, trim, refine(_length/_tolerance), smooth, simplify, minkowski(_diff)
//!   solid.section {id, z?}             -> section             a slice at z, or the silhouette (project)
//!   solid.decompose / solid.compose / solid.hull_points; solid.extrude, revolve, level_set, from_mesh, import, export
//!   section.op    {fn, polygons, others?, args} -> section    2D cross-sections by value: booleans, offset, hull, moves
//!   solid.free    {id}
//!   ecs.new {} -> {world}; ecs.spawn {world, kind, cols} -> {id}; ecs.set {world, id, cols}; ecs.get {world, id};
//!   ecs.despawn {world, id}; ecs.query {world, kind, where: [[col, op, value]]} -> {ids}; ecs.column {world, kind, col};
//!   ecs.rows {world}; ecs.count {world, kind}; ecs.clone {world} -> {world}; ecs.free {world}
//!   print {text}                                              (a headless run's output, to stdout)

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;

use manifold_rust::manifold::Manifold;
use serde_json::{Value, json};

type R<T> = Result<T, String>;

/// ml.* and speech.* (raea-ml), registered with the rest
#[cfg(feature = "ml")]
#[path = "ml.rs"]
pub mod ml;

/// The open composition's folder (where "/media/..." resolves), set with ml::set_dir.
pub fn composition_dir() -> Option<std::path::PathBuf> {
    #[cfg(feature = "ml")]
    return ml::dir();
    #[cfg(not(feature = "ml"))]
    None
}

struct Solid {
    m: Manifold,
    expr: String,
}

/// A decoded picture held natively: NV12 at its size (what mediabunny's samples point to by handle).
struct Picture {
    width: u32,
    height: u32,
    nv12: Vec<u8>,
}

#[derive(Default)]
struct State {
    solids: HashMap<u64, Solid>,
    pictures: HashMap<u64, Picture>,
    #[cfg(target_vendor = "apple")]
    vdecs: HashMap<u64, raea_media::packets::PacketDecoder>,
    #[cfg(target_vendor = "apple")]
    vencs: HashMap<u64, raea_media::packets::PacketEncoder>,
    adecs: HashMap<u64, raea_media::apackets::AudioPacketDecoder>,
    #[cfg(target_vendor = "apple")]
    aencs: HashMap<u64, raea_media::aenc::AacEncoder>,
    next: u64,
}

fn b64(v: &Value, k: &str) -> R<Vec<u8>> {
    use base64::Engine as _;
    match v.get(k).and_then(Value::as_str) {
        Some(s) => base64::engine::general_purpose::STANDARD.decode(s).map_err(|e| format!("{k}: {e}")),
        None => Ok(Vec::new()),
    }
}

fn enc64(b: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// mediabunny's transformation of a frame (VideoSampleTransformationDescription), on NV12: rotated clockwise, flipped
/// about the vertical axis, cropped (in the rotated picture), then resized to w x h by `fit` (fill stretches, contain
/// letterboxes in black, cover crops the middle). Each output pixel is mapped back to its source pixel.
fn transform_nv12(src: &Picture, rotation: u32, flip: bool, crop: Option<(f64, f64, f64, f64)>, w: u32, h: u32, fit: &str) -> Picture {
    let (sw, sh) = (src.width as f64, src.height as f64);
    // the rotated picture's size
    let (rw, rh) = if rotation % 180 == 0 { (sw, sh) } else { (sh, sw) };
    let (cx, cy, cw, ch) = crop.unwrap_or((0.0, 0.0, rw, rh));
    let (ow, oh) = (w as f64, h as f64);
    // the cropped picture placed in the output box
    let (sx, sy) = (ow / cw, oh / ch);
    let (kx, ky, ox, oy) = match fit {
        "contain" => {
            let k = sx.min(sy);
            (k, k, (ow - cw * k) / 2.0, (oh - ch * k) / 2.0)
        }
        "cover" => {
            let k = sx.max(sy);
            (k, k, (ow - cw * k) / 2.0, (oh - ch * k) / 2.0)
        }
        _ => (sx, sy, 0.0, 0.0),
    };
    // a point of the output (pixel centre) to a source point; None outside (letterbox)
    let back = |x: f64, y: f64| -> Option<(f64, f64)> {
        let (mut u, v) = ((x - ox) / kx + cx, (y - oy) / ky + cy);
        if u < cx || v < cy || u >= cx + cw || v >= cy + ch {
            return None;
        }
        if flip {
            u = rw - u;
        }
        // undo a clockwise rotation
        // (a source point (x, y) turned 90 clockwise lands at (sh - y, x) in the rotated picture, which is rw x rh = sh x sw)
        let (px, py) = match rotation % 360 {
            90 => (v, rw - u),
            180 => (rw - u, rh - v),
            270 => (rh - v, u),
            _ => (u, v),
        };
        Some((px.clamp(0.0, sw - 1e-6), py.clamp(0.0, sh - 1e-6)))
    };
    let (dw, dh) = (w as usize, h as usize);
    let (sw_i, sh_i) = (src.width as usize, src.height as usize);
    let mut out = vec![0u8; dw * dh + dw * (dh / 2)];
    for y in 0..dh {
        for x in 0..dw {
            out[y * dw + x] = match back(x as f64 + 0.5, y as f64 + 0.5) {
                Some((px, py)) => src.nv12[py as usize * sw_i + px as usize],
                None => 16,
            };
        }
    }
    let uv = dw * dh;
    for y in 0..dh / 2 {
        for x in 0..dw / 2 {
            let (u, v) = match back(x as f64 * 2.0 + 1.0, y as f64 * 2.0 + 1.0) {
                Some((px, py)) => {
                    let at = sw_i * sh_i + (py as usize / 2) * sw_i + (px as usize / 2) * 2;
                    (src.nv12[at], src.nv12[at + 1])
                }
                None => (128, 128),
            };
            out[uv + y * dw + x * 2] = u;
            out[uv + y * dw + x * 2 + 1] = v;
        }
    }
    Picture { width: w, height: h, nv12: out }
}

/// NV12 scaled (box-averaged when shrinking, nearest when growing), each plane on its own.
fn scale_nv12(src: &Picture, w: u32, h: u32) -> Picture {
    let (sw, sh, dw, dh) = (src.width as usize, src.height as usize, w as usize, h as usize);
    let mut out = vec![0u8; dw * dh + dw * (dh / 2)];
    let sample = |plane: &[u8], pw: usize, ph: usize, step: usize, ch: usize, x0: f64, x1: f64, y0: f64, y1: f64| -> u8 {
        let (xa, xb) = (x0.floor() as usize, (x1.ceil() as usize).max(x0.floor() as usize + 1).min(pw));
        let (ya, yb) = (y0.floor() as usize, (y1.ceil() as usize).max(y0.floor() as usize + 1).min(ph));
        let (mut s, mut n) = (0u32, 0u32);
        for y in ya..yb {
            for x in xa..xb {
                s += plane[y * pw * step + x * step + ch] as u32;
                n += 1;
            }
        }
        (s / n.max(1)) as u8
    };
    let (sy, suv) = src.nv12.split_at(sw * sh);
    for y in 0..dh {
        for x in 0..dw {
            let (x0, x1) = (x as f64 * sw as f64 / dw as f64, (x + 1) as f64 * sw as f64 / dw as f64);
            let (y0, y1) = (y as f64 * sh as f64 / dh as f64, (y + 1) as f64 * sh as f64 / dh as f64);
            out[y * dw + x] = sample(sy, sw, sh, 1, 0, x0, x1, y0, y1);
        }
    }
    let (cw, chh, scw, sch) = (dw / 2, dh / 2, sw / 2, sh / 2);
    for y in 0..chh {
        for x in 0..cw {
            let (x0, x1) = (x as f64 * scw as f64 / cw as f64, (x + 1) as f64 * scw as f64 / cw as f64);
            let (y0, y1) = (y as f64 * sch as f64 / chh as f64, (y + 1) as f64 * sch as f64 / chh as f64);
            for c in 0..2 {
                out[dw * dh + y * dw + x * 2 + c] = sample(suv, scw, sch, 2, c, x0, x1, y0, y1);
            }
        }
    }
    Picture { width: w, height: h, nv12: out }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// A headless run's end (`done {code}`): -1 while it runs.
pub static DONE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// Whether print also writes to stdout (the CLI; not an app).
pub static ECHO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// What a headless run printed (an app shows it; the CLI also writes it to stdout as it comes).
pub static PRINTED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

fn f(v: &Value, k: &str) -> R<f64> {
    v.get(k).and_then(Value::as_f64).ok_or_else(|| format!("{k}: a number"))
}

fn u(v: &Value, k: &str) -> R<u64> {
    v.get(k).and_then(Value::as_u64).ok_or_else(|| format!("{k}: an id"))
}

fn s<'a>(v: &'a Value, k: &str) -> R<&'a str> {
    v.get(k).and_then(Value::as_str).ok_or_else(|| format!("{k}: a string"))
}

fn args(v: &Value) -> Vec<Value> {
    v.get("args").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn arg_text(a: &Value) -> String {
    match a {
        Value::String(s) => format!("{s:?}"),
        Value::Number(n) => n.to_string(),
        o => o.to_string(),
    }
}

impl State {
    fn put(&mut self, m: Manifold, expr: String) -> Value {
        self.next += 1;
        let id = self.next;
        self.solids.insert(id, Solid { m, expr: expr.clone() });
        json!({"id": id, "expr": expr})
    }

    fn solid(&self, id: u64) -> R<&Solid> {
        self.solids.get(&id).ok_or_else(|| format!("solid {id} is gone (freed?)"))
    }

}



/// A solid no expression describes (made from a mesh, a level set, a warp), written as OBJ under its content's hash; its
/// expression loads it (`obj("...")`), so the renderer rebuilds it like any other.
fn bake(m: &Manifold) -> R<String> {
    use std::hash::{Hash, Hasher};
    let (verts, tris) = rdb::shape::to_mesh(m);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    verts.iter().for_each(|x| x.to_bits().hash(&mut h));
    tris.hash(&mut h);
    let dir = std::env::temp_dir().join("raea-solids");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{:016x}.obj", h.finish()));
    if !path.exists() {
        rdb::shape::export(m, &path.display().to_string(), "obj")?;
    }
    Ok(format!("obj({:?})", path.display().to_string()))
}

/// Polygons given as [[[x, y], ...], ...] in the polygon string rdb's shape language reads.
fn polygons_text(v: &Value) -> R<String> {
    let polys = v.get("polygons").and_then(Value::as_array).ok_or("polygons: [[[x, y], ...], ...]")?;
    let mut out = Vec::new();
    for c in polys {
        let pts: Vec<String> = c.as_array().ok_or("a contour is [[x, y], ...]")?.iter().map(|p| {
            let x = p.get(0).and_then(Value::as_f64).unwrap_or(f64::NAN);
            let y = p.get(1).and_then(Value::as_f64).unwrap_or(f64::NAN);
            format!("{x} {y}")
        }).collect();
        out.push(pts.join(", "));
    }
    Ok(out.join("; "))
}

fn vec3_of(v: &Value, k: &str) -> R<[f64; 3]> {
    let a = v.get(k).and_then(Value::as_array).ok_or_else(|| format!("{k}: [x, y, z]"))?;
    Ok([a.first().and_then(Value::as_f64).unwrap_or(0.0), a.get(1).and_then(Value::as_f64).unwrap_or(0.0), a.get(2).and_then(Value::as_f64).unwrap_or(0.0)])
}

/// A 2D cross-section given as [[[x, y], ...], ...] (`key`), unioned by Manifold's positive fill rule.
fn section_of(v: &Value, key: &str) -> R<manifold_rust::cross_section::CrossSection> {
    use manifold_rust::linalg::Vec2;
    let polys = v.get(key).and_then(Value::as_array).ok_or_else(|| format!("{key}: [[[x, y], ...], ...]"))?;
    let mut out: Vec<Vec<Vec2>> = Vec::new();
    for c in polys {
        let pts: Vec<Vec2> = c.as_array().ok_or("a contour is [[x, y], ...]")?.iter().map(|p| Vec2::new(p.get(0).and_then(Value::as_f64).unwrap_or(f64::NAN), p.get(1).and_then(Value::as_f64).unwrap_or(f64::NAN))).collect();
        if pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return Err("a cross-section's points must be finite [x, y]".into());
        }
        out.push(pts);
    }
    Ok(manifold_rust::cross_section::CrossSection::new(out))
}

/// A cross-section as TypeScript's CrossSection holds it: its contours, area, contour count and 2D bounds.
fn section_json(cs: &manifold_rust::cross_section::CrossSection) -> Value {
    let polys: Vec<Vec<[f64; 2]>> = cs.to_polygons().iter().map(|p| p.iter().map(|q| [q.x, q.y]).collect()).collect();
    let (lo, hi) = if cs.is_empty() { ([0.0, 0.0], [0.0, 0.0]) } else { let b = cs.bounds(); ([b.min.x, b.min.y], [b.max.x, b.max.y]) };
    json!({"polygons": polys, "area": cs.area(), "contours": cs.num_contour(), "bounds": {"min": lo, "max": hi}})
}

pub fn call(name: &str, v: &Value) -> R<Value> {
    STATE.with(|st| {
        let mut st = st.borrow_mut();
        match name {
            "done" => {
                DONE.store(v.get("code").and_then(Value::as_i64).unwrap_or(0) as i32, std::sync::atomic::Ordering::SeqCst);
                Ok(Value::Null)
            }
            "print" => {
                let text = s(v, "text")?;
                if let Ok(mut p) = PRINTED.lock() {
                    p.push_str(text);
                    p.push('\n');
                }
                if ECHO.load(std::sync::atomic::Ordering::Relaxed) {
                    let mut o = std::io::stdout().lock();
                    let _ = writeln!(o, "{text}");
                    let _ = o.flush();
                }
                Ok(Value::Null)
            }
            "solid.expr" => {
                let expr = s(v, "expr")?.to_string();
                let m = rdb::shape::solid(&expr)?;
                Ok(st.put(m, expr))
            }
            "solid.make" => {
                let fun = s(v, "fn")?;
                let expr = format!("{fun}({})", args(v).iter().map(arg_text).collect::<Vec<_>>().join(", "));
                let m = rdb::shape::solid(&expr)?;
                Ok(st.put(m, expr))
            }
            "solid.combine" => {
                let fun = s(v, "fn")?;
                let ids: Vec<u64> = v.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
                if ids.len() < 2 && fun != "hull" {
                    return Err(format!("{fun} takes two solids or more"));
                }
                let parts: Vec<&Solid> = ids.iter().map(|i| st.solid(*i)).collect::<R<_>>()?;
                let expr = format!("{fun}({})", parts.iter().map(|p| p.expr.as_str()).collect::<Vec<_>>().join(", "));
                let ms: Vec<Manifold> = parts.iter().map(|p| p.m.clone()).collect();
                let m = match fun {
                    "union" => ms.into_iter().reduce(|a, b| a.union(&b)).unwrap(),
                    "minus" => ms.into_iter().reduce(|a, b| a.difference(&b)).unwrap(),
                    "intersect" => ms.into_iter().reduce(|a, b| a.intersection(&b)).unwrap(),
                    "hull" => Manifold::hull_manifolds(&ms),
                    o => return Err(format!("no boolean {o}: union, minus, intersect, hull")),
                };
                Ok(st.put(m, expr))
            }
            "solid.move" => {
                let base = st.solid(u(v, "id")?)?;
                let fun = s(v, "fn")?;
                let a: Vec<f64> = args(v).iter().filter_map(Value::as_f64).collect();
                let need = if fun == "scale" { 1 } else { 3 };
                if a.len() < need {
                    return Err(format!("{fun} takes {need} numbers"));
                }
                let (m, expr) = match fun {
                    "move" => (base.m.translate(manifold_rust::linalg::Vec3::new(a[0], a[1], a[2])), format!("move({}, {}, {}, {})", base.expr, a[0], a[1], a[2])),
                    "rotate" => (base.m.rotate(a[0], a[1], a[2]), format!("rotate({}, {}, {}, {})", base.expr, a[0], a[1], a[2])),
                    "scale" => {
                        let (x, y, z) = if a.len() >= 3 { (a[0], a[1], a[2]) } else { (a[0], a[0], a[0]) };
                        (base.m.scale(manifold_rust::linalg::Vec3::new(x, y, z)), format!("scale({}, {x}, {y}, {z})", base.expr))
                    }
                    o => return Err(format!("no transform {o}: move, rotate, scale")),
                };
                Ok(st.put(m, expr))
            }
            "solid.query" => {
                let sd = st.solid(u(v, "id")?)?;
                match s(v, "what")? {
                    "volume" => Ok(json!(sd.m.volume())),
                    "bounds" => {
                        let b = sd.m.bounding_box();
                        Ok(json!({"min": [b.min.x, b.min.y, b.min.z], "max": [b.max.x, b.max.y, b.max.z]}))
                    }
                    "triangles" => Ok(json!(sd.m.num_tri())),
                    "empty" => Ok(json!(sd.m.is_empty())),
                    "slice" => {
                        let cs = sd.m.slice(f(v, "z")?);
                        let polys: Vec<Vec<[f64; 2]>> = cs.to_polygons().iter().map(|p| p.iter().map(|q| [q.x, q.y]).collect()).collect();
                        Ok(json!({"area": cs.area(), "contours": cs.num_contour(), "polygons": polys}))
                    }
                    "area" => Ok(json!(sd.m.surface_area())),
                    "genus" => Ok(json!(sd.m.genus())),
                    "min_gap" => {
                        let o = st.solid(u(v, "other")?)?;
                        Ok(json!(sd.m.min_gap(&o.m, f(v, "z")?)))
                    }
                    "ray" => {
                        let a: Vec<f64> = args(v).iter().filter_map(Value::as_f64).collect();
                        if a.len() < 6 {
                            return Err("ray takes 6 numbers: from x y z, to x y z".into());
                        }
                        let hits = sd.m.ray_cast(manifold_rust::linalg::Vec3::new(a[0], a[1], a[2]), manifold_rust::linalg::Vec3::new(a[3], a[4], a[5]));
                        Ok(json!(hits.iter().map(|h| json!({"distance": h.distance, "position": [h.position.x, h.position.y, h.position.z], "normal": [h.normal.x, h.normal.y, h.normal.z]})).collect::<Vec<_>>()))
                    }
                    o => Err(format!("no query {o}: volume, bounds, triangles, empty, slice, area, genus, min_gap, ray")),
                }
            }
            // the rest of Manifold: each a new solid with the expression that rebuilds it (rdb's shape language)
            "solid.op" => {
                let base = st.solid(u(v, "id")?)?;
                let fun = s(v, "fn")?;
                let a: Vec<f64> = args(v).iter().filter_map(Value::as_f64).collect();
                let need = |n: usize| if a.len() < n { Err(format!("{fun} takes {n} numbers")) } else { Ok(()) };
                let other = match v.get("other").and_then(Value::as_u64) {
                    Some(o) => Some(st.solid(o)?),
                    None => None,
                };
                let v3 = |i: usize| manifold_rust::linalg::Vec3::new(a[i], a[i + 1], a[i + 2]);
                let (m, expr) = match fun {
                    "mirror" => {
                        need(3)?;
                        (base.m.mirror(v3(0)), format!("mirror({}, {}, {}, {})", base.expr, a[0], a[1], a[2]))
                    }
                    "transform" => {
                        need(12)?;
                        let mat = manifold_rust::linalg::Mat3x4::from_cols(v3(0), v3(3), v3(6), v3(9));
                        (base.m.transform(&mat), format!("transform({}, {})", base.expr, a.iter().take(12).map(|x| x.to_string()).collect::<Vec<_>>().join(", ")))
                    }
                    "trim" => {
                        need(4)?;
                        (base.m.trim_by_plane(v3(0), a[3]), format!("trim({}, {}, {}, {}, {})", base.expr, a[0], a[1], a[2], a[3]))
                    }
                    "refine" => {
                        need(1)?;
                        let n = a[0].max(1.0) as i32;
                        (base.m.refine(n), format!("refine({}, {n})", base.expr))
                    }
                    "smooth" => {
                        let (ang, sm, n) = (a.first().copied().unwrap_or(60.0), a.get(1).copied().unwrap_or(0.0), a.get(2).copied().unwrap_or(4.0).max(1.0) as i32);
                        (base.m.smooth_out(ang, sm).refine(n), format!("smooth({}, {ang}, {sm}, {n})", base.expr))
                    }
                    "minkowski" | "minkowski_diff" => {
                        let o = other.ok_or_else(|| format!("{fun} takes another solid"))?;
                        let m = if fun == "minkowski" { base.m.minkowski_sum(&o.m) } else { base.m.minkowski_difference(&o.m) };
                        (m, format!("{fun}({}, {})", base.expr, o.expr))
                    }
                    "simplify" | "refine_length" | "refine_tolerance" => {
                        need(1)?;
                        let m = match fun { "simplify" => base.m.simplify(a[0]), "refine_length" => base.m.refine_to_length(a[0]), _ => base.m.refine_to_tolerance(a[0]) };
                        (m, format!("{fun}({}, {})", base.expr, a[0]))
                    }
                    o => return Err(format!("no operation {o}: mirror, transform, trim, refine, refine_length, refine_tolerance, smooth, simplify, minkowski, minkowski_diff")),
                };
                Ok(st.put(m, expr))
            }
            "solid.extrude" | "solid.revolve" => {
                let polys = polygons_text(v)?;
                let a: Vec<f64> = args(v).iter().filter_map(Value::as_f64).collect();
                let expr = if name == "solid.extrude" {
                    format!("extrude({polys:?}, {})", a.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
                } else if a.is_empty() {
                    format!("revolve({polys:?})")
                } else {
                    format!("revolve({polys:?}, {})", a.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
                };
                let m = rdb::shape::solid(&expr)?;
                Ok(st.put(m, expr))
            }
            "solid.mesh" => {
                let sd = st.solid(u(v, "id")?)?;
                let (verts, tris) = rdb::shape::to_mesh(&sd.m);
                Ok(json!({"verts": verts, "tris": tris}))
            }
            "solid.from_mesh" => {
                let verts: Vec<f32> = v.get("verts").and_then(Value::as_array).ok_or("verts: x y z ...")?.iter().filter_map(Value::as_f64).map(|x| x as f32).collect();
                let tris: Vec<u32> = v.get("tris").and_then(Value::as_array).ok_or("tris: i j k ...")?.iter().filter_map(Value::as_u64).map(|x| x as u32).collect();
                let m = rdb::shape::from_mesh(&verts, &tris)?;
                let expr = bake(&m)?;
                Ok(st.put(m, expr))
            }
            "solid.level_set" => {
                // an SDF sampled on a grid (nx*ny*nz values over [min, max], x fastest), surfaced at `level`
                let dims: Vec<usize> = v.get("dims").and_then(Value::as_array).ok_or("dims: [nx, ny, nz]")?.iter().filter_map(Value::as_u64).map(|x| x as usize).collect();
                let grid: Vec<f64> = v.get("grid").and_then(Value::as_array).ok_or("grid: values")?.iter().filter_map(Value::as_f64).collect();
                let lo = vec3_of(v, "min")?;
                let hi = vec3_of(v, "max")?;
                if dims.len() != 3 || dims.iter().any(|d| *d < 2) || grid.len() != dims[0] * dims[1] * dims[2] {
                    return Err("level_set: dims [nx, ny, nz] (each 2 or more) and nx*ny*nz grid values".into());
                }
                let level = v.get("level").and_then(Value::as_f64).unwrap_or(0.0);
                let edge = v.get("edge").and_then(Value::as_f64).unwrap_or(((hi[0] - lo[0]) / (dims[0] - 1) as f64).min((hi[1] - lo[1]) / (dims[1] - 1) as f64).min((hi[2] - lo[2]) / (dims[2] - 1) as f64));
                let (nx, ny, nz) = (dims[0], dims[1], dims[2]);
                let sample = move |p: manifold_rust::linalg::Vec3| -> f64 {
                    let at = |c: f64, l: f64, h: f64, n: usize| ((c - l) / (h - l) * (n - 1) as f64).clamp(0.0, (n - 1) as f64);
                    let (fx, fy, fz) = (at(p.x, lo[0], hi[0], nx), at(p.y, lo[1], hi[1], ny), at(p.z, lo[2], hi[2], nz));
                    let (x0, y0, z0) = (fx.floor() as usize, fy.floor() as usize, fz.floor() as usize);
                    let (x1, y1, z1) = ((x0 + 1).min(nx - 1), (y0 + 1).min(ny - 1), (z0 + 1).min(nz - 1));
                    let (tx, ty, tz) = (fx - x0 as f64, fy - y0 as f64, fz - z0 as f64);
                    let g = |x: usize, y: usize, z: usize| grid[x + nx * (y + ny * z)];
                    let l = |a: f64, b: f64, t: f64| a + (b - a) * t;
                    // inside is negative in an SDF; Manifold's level set keeps where the function is above the level
                    -l(l(l(g(x0, y0, z0), g(x1, y0, z0), tx), l(g(x0, y1, z0), g(x1, y1, z0), tx), ty), l(l(g(x0, y0, z1), g(x1, y0, z1), tx), l(g(x0, y1, z1), g(x1, y1, z1), tx), ty), tz)
                };
                let bounds = manifold_rust::types::Box { min: manifold_rust::linalg::Vec3::new(lo[0], lo[1], lo[2]), max: manifold_rust::linalg::Vec3::new(hi[0], hi[1], hi[2]) };
                let m = Manifold::level_set_with_level(sample, bounds, edge, -level);
                if m.is_empty() {
                    return Err("level_set: no surface at that level inside the bounds".into());
                }
                let expr = bake(&m)?;
                Ok(st.put(m, expr))
            }
            "solid.export" => {
                let sd = st.solid(u(v, "id")?)?;
                let path = s(v, "path")?;
                let format = v.get("format").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| if path.ends_with(".glb") { "glb".into() } else { "obj".into() });
                Ok(json!({"bytes": rdb::shape::export(&sd.m, path, &format)?}))
            }
            "solid.import" => {
                let path = s(v, "path")?;
                let f = if path.to_ascii_lowercase().ends_with(".glb") { "glb" } else { "obj" };
                let expr = format!("{f}({path:?})");
                let m = rdb::shape::solid(&expr)?;
                Ok(st.put(m, expr))
            }
            // a solid's cross-section at z, or its silhouette on the x-y plane, as a 2D cross-section
            "solid.section" => {
                let sd = st.solid(u(v, "id")?)?;
                let cs = match v.get("z").and_then(Value::as_f64) { Some(z) => sd.m.slice(z), None => sd.m.project() };
                Ok(section_json(&cs))
            }
            // its disconnected pieces, each a solid (written under its hash, as a mesh's solid is)
            "solid.decompose" => {
                let parts = st.solid(u(v, "id")?)?.m.decompose();
                let mut out = Vec::new();
                for m in parts {
                    let expr = bake(&m)?;
                    out.push(st.put(m, expr));
                }
                Ok(Value::Array(out))
            }
            // solids that don't overlap, held as one (Manifold's compose: no boolean, so the union is exact)
            "solid.compose" => {
                let ids: Vec<u64> = v.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
                if ids.is_empty() {
                    return Err("compose takes one solid or more".into());
                }
                let parts: Vec<&Solid> = ids.iter().map(|i| st.solid(*i)).collect::<R<_>>()?;
                let expr = if parts.len() == 1 { parts[0].expr.clone() } else { format!("union({})", parts.iter().map(|p| p.expr.as_str()).collect::<Vec<_>>().join(", ")) };
                let m = Manifold::compose(&parts.iter().map(|p| p.m.clone()).collect::<Vec<_>>());
                Ok(st.put(m, expr))
            }
            // the convex hull of points
            "solid.hull_points" => {
                let pts: Vec<manifold_rust::linalg::Vec3> = v.get("points").and_then(Value::as_array).ok_or("points: [[x, y, z], ...]")?.iter().map(|p| manifold_rust::linalg::Vec3::new(p.get(0).and_then(Value::as_f64).unwrap_or(0.0), p.get(1).and_then(Value::as_f64).unwrap_or(0.0), p.get(2).and_then(Value::as_f64).unwrap_or(0.0))).collect();
                if pts.len() < 4 {
                    return Err("hull of points takes 4 points or more".into());
                }
                let m = Manifold::hull(&pts);
                let expr = bake(&m)?;
                Ok(st.put(m, expr))
            }
            // 2D cross-sections (Manifold's CrossSection), held by value: each answer is the new section
            "section.op" => {
                use manifold_rust::cross_section::CrossSection;
                use manifold_rust::linalg::Vec2;
                use manifold_rust::types::OpType;
                let fun = s(v, "fn")?;
                let a: Vec<f64> = args(v).iter().filter_map(Value::as_f64).collect();
                let need = |n: usize| if a.len() < n { Err(format!("section {fun} takes {n} numbers")) } else { Ok(()) };
                let others = || -> R<Vec<CrossSection>> {
                    v.get("others").and_then(Value::as_array).map(|list| list.iter().map(|o| section_of(&json!({"p": o}), "p")).collect::<R<Vec<_>>>()).unwrap_or_else(|| Ok(Vec::new()))
                };
                let cs = match fun {
                    "circle" => {
                        need(1)?;
                        CrossSection::circle(a[0], a.get(1).copied().unwrap_or(0.0) as i32)
                    }
                    "square" => {
                        need(2)?;
                        CrossSection::square_vec2(Vec2::new(a[0], a[1]), a.get(2).copied().unwrap_or(1.0) != 0.0)
                    }
                    "hull_points" => {
                        // every point of every contour, as given (no union first)
                        let raw: Vec<Vec2> = v.get("polygons").and_then(Value::as_array).into_iter().flatten().flat_map(|c| c.as_array().cloned().unwrap_or_default()).map(|p| Vec2::new(p.get(0).and_then(Value::as_f64).unwrap_or(0.0), p.get(1).and_then(Value::as_f64).unwrap_or(0.0))).collect();
                        if raw.len() < 3 {
                            return Err("section hull_points takes 3 points or more".into());
                        }
                        CrossSection::hull_points(&raw)
                    }
                    _ => {
                        let me = section_of(v, "polygons")?;
                        match fun {
                            "make" => me,
                            "union" | "minus" | "intersect" => {
                                let mut all = vec![me];
                                all.extend(others()?);
                                if all.len() < 2 {
                                    return Err(format!("section {fun} takes another section"));
                                }
                                CrossSection::batch_boolean(&all, match fun { "union" => OpType::Add, "minus" => OpType::Subtract, _ => OpType::Intersect })
                            }
                            "hull" => {
                                let mut all = vec![me];
                                all.extend(others()?);
                                CrossSection::hull_cross_sections(&all)
                            }
                            "minkowski" => {
                                // Clipper's sum sweeps the outline, leaving the inside out: the inside comes back as this
                                // shape moved by one point of the other (the sum of a shape and a connected one)
                                let o = others()?.into_iter().next().ok_or("section minkowski takes another section")?;
                                let p0 = o.to_polygons().first().and_then(|c| c.first().copied()).ok_or("section minkowski: the other section is empty")?;
                                me.minkowski_sum(&o).union(&me.translate(p0))
                            }
                            "offset" => {
                                // delta, join (0 square, 1 round, 2 miter, 3 bevel), miter limit, round segments
                                need(1)?;
                                me.offset_with_params(a[0], a.get(1).copied().unwrap_or(1.0) as i32, a.get(2).copied().unwrap_or(2.0), a.get(3).copied().unwrap_or(0.0) as i32)
                            }
                            "simplify" => {
                                need(1)?;
                                me.simplify(a[0])
                            }
                            "translate" => {
                                need(2)?;
                                me.translate(Vec2::new(a[0], a[1]))
                            }
                            "rotate" => {
                                need(1)?;
                                me.rotate(a[0])
                            }
                            "scale" => {
                                need(1)?;
                                me.scale(Vec2::new(a[0], a.get(1).copied().unwrap_or(a[0])))
                            }
                            "mirror" => {
                                need(2)?;
                                me.mirror(Vec2::new(a[0], a[1]))
                            }
                            "decompose" => return Ok(Value::Array(me.decompose().iter().map(section_json).collect())),
                            o => return Err(format!("no section operation {o}: make, circle, square, hull_points, union, minus, intersect, hull, minkowski, offset, simplify, translate, rotate, scale, mirror, decompose")),
                        }
                    }
                };
                Ok(section_json(&cs))
            }
            "solid.free" => {
                st.solids.remove(&u(v, "id")?);
                Ok(Value::Null)
            }
            n if n.starts_with("ecs.") => crate::ecs::call(n, v),
            // ---- media (mediabunny's coders, @raea/motion/media)
            "media.live" => {
                #[cfg(target_vendor = "apple")]
                let (vd, ve) = (st.vdecs.len(), st.vencs.len() + st.aencs.len());
                #[cfg(not(target_vendor = "apple"))]
                let (vd, ve) = (0usize, 0usize);
                Ok(json!({"vdecs": vd, "vencs": ve, "adecs": st.adecs.len(), "frames": st.pictures.len(), "solids": st.solids.len(), "worlds": crate::ecs::live()}))
            }
            "media.size" => Ok(json!(std::fs::metadata(s(v, "path")?).map_err(|e| e.to_string())?.len())),
            "media.read" => {
                use std::io::{Read, Seek, SeekFrom};
                let (start, end) = (u(v, "start")?, u(v, "end")?);
                let mut f = std::fs::File::open(s(v, "path")?).map_err(|e| e.to_string())?;
                f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
                let mut buf = vec![0u8; end.saturating_sub(start) as usize];
                let n = f.read(&mut buf).map_err(|e| e.to_string())?;
                buf.truncate(n);
                let mut rest = buf.len();
                while rest < (end - start) as usize {
                    let mut more = vec![0u8; (end - start) as usize - rest];
                    let k = f.read(&mut more).map_err(|e| e.to_string())?;
                    if k == 0 {
                        break;
                    }
                    buf.extend_from_slice(&more[..k]);
                    rest = buf.len();
                }
                Ok(json!(enc64(&buf)))
            }
            "media.write" => {
                use std::io::{Seek, SeekFrom, Write as _};
                let path = s(v, "path")?;
                let data = b64(v, "data")?;
                let mut f = std::fs::OpenOptions::new().create(true).write(true).truncate(v.get("truncate").and_then(Value::as_bool).unwrap_or(false)).open(path).map_err(|e| format!("{path}: {e}"))?;
                f.seek(SeekFrom::Start(v.get("at").and_then(Value::as_u64).unwrap_or(0))).map_err(|e| e.to_string())?;
                f.write_all(&data).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            #[cfg(target_vendor = "apple")]
            "vdec.new" => {
                let d = raea_media::packets::PacketDecoder::new(s(v, "codec")?, &b64(v, "description")?)?;
                st.next += 1;
                let id = st.next;
                st.vdecs.insert(id, d);
                Ok(json!({"id": id}))
            }
            #[cfg(target_vendor = "apple")]
            "vdec.decode" | "vdec.flush" => {
                let id = u(v, "id")?;
                let frames = {
                    let d = st.vdecs.get_mut(&id).ok_or("no such decoder")?;
                    if name == "vdec.flush" { d.flush()? } else { d.decode(&b64(v, "data")?, f(v, "pts")?, f(v, "dur")?)? }
                };
                let mut out = Vec::new();
                for fr in frames {
                    st.next += 1;
                    let pid = st.next;
                    out.push(json!({"frame": pid, "pts": fr.pts, "dur": fr.dur, "width": fr.width, "height": fr.height}));
                    st.pictures.insert(pid, Picture { width: fr.width, height: fr.height, nv12: fr.nv12 });
                }
                Ok(Value::Array(out))
            }
            #[cfg(target_vendor = "apple")]
            "vdec.free" => {
                st.vdecs.remove(&u(v, "id")?);
                Ok(Value::Null)
            }
            #[cfg(target_vendor = "apple")]
            "venc.new" => {
                let e = raea_media::packets::PacketEncoder::new(u(v, "width")? as u32, u(v, "height")? as u32, f(v, "fps")?, u(v, "kbps")? as u32)?;
                st.next += 1;
                let id = st.next;
                st.vencs.insert(id, e);
                Ok(json!({"id": id}))
            }
            #[cfg(target_vendor = "apple")]
            "venc.encode" | "venc.finish" => {
                let id = u(v, "id")?;
                let packets = if name == "venc.finish" {
                    st.vencs.get_mut(&id).ok_or("no such encoder")?.finish()?
                } else {
                    let pic = st.pictures.get(&u(v, "frame")?).ok_or("that frame is gone")?;
                    let nv12 = pic.nv12.clone();
                    st.vencs.get_mut(&id).ok_or("no such encoder")?.encode(&nv12, f(v, "pts")?, f(v, "dur")?)?
                };
                let e = st.vencs.get(&id).unwrap();
                let desc = e.description().map(|d| enc64(&d));
                if name == "venc.finish" {
                    st.vencs.remove(&id);
                }
                Ok(json!({"packets": packets.iter().map(|p| json!({"data": enc64(&p.data), "key": p.key, "pts": p.pts, "dur": p.dur})).collect::<Vec<_>>(), "description": desc}))
            }
            // a heavy job in one native call: decode, scale, encode with sound (what `rdb media trim|proxy` does, here
            // in-process so it runs the same in an app); {path, out, from?, to?, height?, kbps?}
            "media.transcode" => {
                let (path, out) = (s(v, "path")?, s(v, "out")?);
                let be = raea_media::backend(if cfg!(target_vendor = "apple") { "apple" } else { "soft" })?;
                let mut dec = be.open_video(std::path::Path::new(path))?;
                let info = dec.info().clone();
                let (from, to) = (v.get("from").and_then(Value::as_f64).unwrap_or(0.0).max(0.0), v.get("to").and_then(Value::as_f64).unwrap_or(info.duration).min(info.duration));
                let h = v.get("height").and_then(Value::as_u64).map(|h| (h as u32).min(info.height)).unwrap_or(info.height);
                let (w, h) = (((info.width as f64 * h as f64 / info.height as f64 / 2.0).round() as u32 * 2).max(2), (h / 2 * 2).max(2));
                let fps = info.fps;
                let kbps = v.get("kbps").and_then(Value::as_u64).map(|k| k as u32).unwrap_or(((w as f64 * h as f64 * fps * if h < info.height { 0.08 } else { 0.1 }) / 1000.0).clamp(300.0, 20000.0) as u32);
                let mut enc = be.encoder(std::path::Path::new(out), &raea_media::EncodeOpts { width: w, height: h, fps, kbps, nv12: true, realtime: false })?;
                let n = ((to - from) * fps).round().max(0.0) as usize;
                for i in 0..n {
                    let fr = dec.frame_nv12_at(from + (i as f64 + 0.5) / fps - 1e-6)?;
                    let pic = Picture { width: fr.width, height: fr.height, nv12: fr.rgba };
                    let pic = if pic.width != w || pic.height != h { scale_nv12(&pic, w, h) } else { pic };
                    enc.push_nv12(&pic.nv12, w as usize, h as usize)?;
                }
                // the stretch's sound at the mix rate, in the source's channels (mono stays mono, as ffmpeg keeps it)
                let sound = raea_media::backend("soft").and_then(|b| b.decode_audio(std::path::Path::new(path))).ok().filter(|a| a.len() > 0).map(|a| {
                    let a = raea_media::soft_resample(&a, raea_media::audio::RATE);
                    let (lo, hi) = (((from * a.rate as f64) as usize).min(a.len()), (((from + n as f64 / fps) * a.rate as f64) as usize).min(a.len()));
                    let chans: Vec<Vec<f32>> = (0..a.channels.len().clamp(1, 2)).map(|c| a.channels[c.min(a.channels.len() - 1)][lo..hi].to_vec()).collect();
                    raea_media::Audio { rate: a.rate, channels: chans }
                });
                let stats = enc.finish(sound.as_ref())?;
                Ok(json!({"frames": n, "width": w, "height": h, "fps": fps, "kbps": kbps, "bytes": stats.bytes}))
            }
            "frame.info" => {
                let p = st.pictures.get(&u(v, "frame")?).ok_or("that frame is gone")?;
                Ok(json!({"width": p.width, "height": p.height}))
            }
            "frame.png" => {
                let p = st.pictures.get(&u(v, "frame")?).ok_or("that frame is gone")?;
                let rgba = raea_media::yuv::nv12_to_rgba(&p.nv12, p.width as usize, p.height as usize);
                image::save_buffer(s(v, "path")?, &rgba, p.width, p.height, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "frame.scale" => {
                let p = st.pictures.get(&u(v, "frame")?).ok_or("that frame is gone")?;
                let (w, h) = ((u(v, "width")? / 2 * 2).max(2) as u32, (u(v, "height")? / 2 * 2).max(2) as u32);
                let q = scale_nv12(p, w, h);
                st.next += 1;
                let id = st.next;
                st.pictures.insert(id, q);
                Ok(json!({"frame": id, "width": w, "height": h}))
            }
            "frame.transform" => {
                let p = st.pictures.get(&u(v, "frame")?).ok_or("that frame is gone")?;
                let (w, h) = ((u(v, "width")? / 2 * 2).max(2) as u32, (u(v, "height")? / 2 * 2).max(2) as u32);
                let crop = v.get("crop").filter(|c| c.is_object()).map(|c| (c["left"].as_f64().unwrap_or(0.0), c["top"].as_f64().unwrap_or(0.0), c["width"].as_f64().unwrap_or(1.0), c["height"].as_f64().unwrap_or(1.0)));
                let q = transform_nv12(p, v.get("rotation").and_then(Value::as_u64).unwrap_or(0) as u32, v.get("flip").and_then(Value::as_bool).unwrap_or(false), crop, w, h, v.get("fit").and_then(Value::as_str).unwrap_or("fill"));
                st.next += 1;
                let id = st.next;
                st.pictures.insert(id, q);
                Ok(json!({"frame": id, "width": w, "height": h}))
            }
            "frame.free" => {
                st.pictures.remove(&u(v, "frame")?);
                Ok(Value::Null)
            }
            "adec.new" => {
                let desc = b64(v, "description")?;
                let d = raea_media::apackets::AudioPacketDecoder::new(s(v, "codec")?, Some(&desc), u(v, "rate")? as u32, u(v, "channels")? as u32)?;
                st.next += 1;
                let id = st.next;
                st.adecs.insert(id, d);
                Ok(json!({"id": id}))
            }
            "adec.decode" => {
                let (rate, planes) = st.adecs.get_mut(&u(v, "id")?).ok_or("no such decoder")?.decode(&b64(v, "data")?)?;
                let mut bytes = Vec::with_capacity(planes.iter().map(Vec::len).sum::<usize>() * 4);
                for p in &planes {
                    for x in p {
                        bytes.extend_from_slice(&x.to_le_bytes());
                    }
                }
                Ok(json!({"rate": rate, "channels": planes.len(), "frames": planes.first().map(Vec::len).unwrap_or(0), "data": enc64(&bytes)}))
            }
            "venc.codecs" => {
                #[cfg(target_vendor = "apple")]
                let hw = raea_media::packets::hardware_encoders();
                #[cfg(not(target_vendor = "apple"))]
                let hw: Vec<String> = Vec::new();
                // what the engine's encoder writes today (avc), and what the hardware could (av01, vp09 only where present)
                Ok(json!({"hardware": hw, "avc": hw.iter().any(|c| c == "avc1"), "hevc": hw.iter().any(|c| c == "hvc1"), "av1": hw.iter().any(|c| c == "av01"), "vp9": hw.iter().any(|c| c == "vp09")}))
            }
            #[cfg(target_vendor = "apple")]
            "aenc.new" => {
                let e = raea_media::aenc::AacEncoder::new(u(v, "rate")? as u32, u(v, "channels")? as u32, v.get("kbps").and_then(Value::as_u64).unwrap_or(128) as u32)?;
                let desc = enc64(&e.description());
                st.next += 1;
                let id = st.next;
                st.aencs.insert(id, e);
                Ok(json!({"id": id, "description": desc, "priming": raea_media::aenc::AAC_PRIMING}))
            }
            #[cfg(target_vendor = "apple")]
            "aenc.encode" | "aenc.finish" => {
                let id = u(v, "id")?;
                let e = st.aencs.get_mut(&id).ok_or("no such encoder")?;
                let packets = if name == "aenc.finish" {
                    e.finish()?
                } else {
                    let bytes = b64(v, "data")?;
                    let ch = u(v, "channels")? as usize;
                    let floats: Vec<f32> = bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                    let n = floats.len() / ch.max(1);
                    let planes: Vec<&[f32]> = (0..ch).map(|c| &floats[c * n..(c + 1) * n]).collect();
                    e.encode(&planes)?
                };
                Ok(json!({"packets": packets.iter().map(|p| json!({"data": enc64(&p.data), "pts": p.pts, "dur": p.dur})).collect::<Vec<_>>()}))
            }
            #[cfg(target_vendor = "apple")]
            "aenc.free" => {
                st.aencs.remove(&u(v, "id")?);
                Ok(Value::Null)
            }
            "adec.free" => {
                st.adecs.remove(&u(v, "id")?);
                Ok(Value::Null)
            }
            o => Err(format!("no native call {o}")),
        }
    })
}

/// The calls above in raea-native's registry, with their TypeScript types (what `raea-motion types` writes into
/// sdk/motion/src/native-calls.ts). Idempotent; every host running @raea/motion calls it before its first bundle.
pub fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        const BOUNDS: &str = "{ min: [number, number, number]; max: [number, number, number] }";
        const SOLID: &str = "{ id: number; expr: string }";
        const SECTION: &str = "{ polygons: [number, number][][]; area: number; contours: number; bounds: { min: [number, number]; max: [number, number] } }";
        const COLS: &str = "Record<string, number | string | boolean>";
        const ROW: &str = "{ id: number; kind: string } & Record<string, number | string | boolean>";
        let calls: &[(&str, &str, &str, &str)] = &[
            ("done", "{ code: number }", "null", "a headless run ends with this exit code"),
            ("print", "{ text: string }", "null", "one line of a headless run's output"),
            ("solid.make", "{ fn: \"box\" | \"rounded_box\" | \"sphere\" | \"cylinder\" | \"cone\" | \"torus\" | \"text3d\"; args: (number | string)[] }", SOLID, "a Manifold solid from a primitive"),
            ("solid.expr", "{ expr: string }", SOLID, "a Manifold solid from an expression (rdb's mesh shape language)"),
            ("solid.combine", "{ fn: \"union\" | \"minus\" | \"intersect\" | \"hull\"; ids: number[] }", SOLID, "a boolean or hull of solids"),
            ("solid.move", "{ id: number; fn: \"move\" | \"rotate\" | \"scale\"; args: number[] }", SOLID, "a solid translated, turned (degrees) or scaled"),
            ("solid.query", "{ id: number; what: \"volume\" | \"bounds\" | \"triangles\" | \"empty\" | \"slice\" | \"area\" | \"genus\" | \"min_gap\" | \"ray\"; z?: number; other?: number; args?: number[] }", &format!("number | boolean | {BOUNDS} | {{ area: number; contours: number; polygons: [number, number][][] }} | {{ distance: number; position: [number, number, number]; normal: [number, number, number] }}[]"), "a solid's volume, bounds, triangle count, emptiness, cross-section at z, area, genus, gap to another (searching z far) or ray hits (args from, to)"),
            ("solid.op", "{ id: number; fn: \"mirror\" | \"transform\" | \"trim\" | \"refine\" | \"refine_length\" | \"refine_tolerance\" | \"smooth\" | \"simplify\" | \"minkowski\" | \"minkowski_diff\"; args?: number[]; other?: number }", SOLID, "a solid mirrored, transformed (3x4, by columns), trimmed by a plane, refined (by count, edge length or tolerance), smoothed, simplified, or Minkowski-summed/differenced with another"),
            ("solid.extrude", "{ polygons: [number, number][][]; args: number[] }", SOLID, "a cross-section extruded: args height, divisions, twist degrees, top scale x, y"),
            ("solid.revolve", "{ polygons: [number, number][][]; args?: number[] }", SOLID, "a cross-section revolved about y: args degrees, segments"),
            ("solid.mesh", "{ id: number }", "{ verts: number[]; tris: number[] }", "a solid's triangles (positions x y z, indices by threes)"),
            ("solid.from_mesh", "{ verts: number[]; tris: number[] }", SOLID, "a solid from closed triangles"),
            ("solid.level_set", "{ grid: number[]; dims: [number, number, number]; min: [number, number, number]; max: [number, number, number]; level?: number; edge?: number }", SOLID, "the surface of an SDF sampled on a grid (negative inside)"),
            ("solid.export", "{ id: number; path: string; format?: \"obj\" | \"glb\" }", "{ bytes: number }", "a solid written as OBJ or glTF binary"),
            ("solid.import", "{ path: string }", SOLID, "a solid read from OBJ or glTF binary"),
            ("solid.section", "{ id: number; z?: number }", SECTION, "a solid's cross-section at z, or with no z its silhouette on the x-y plane (project)"),
            ("solid.decompose", "{ id: number }", &format!("{SOLID}[]"), "a solid's disconnected pieces"),
            ("solid.compose", "{ ids: number[] }", SOLID, "solids that don't overlap held as one"),
            ("solid.hull_points", "{ points: [number, number, number][] }", SOLID, "the convex hull of points"),
            ("section.op", "{ fn: \"make\" | \"circle\" | \"square\" | \"hull_points\" | \"union\" | \"minus\" | \"intersect\" | \"hull\" | \"minkowski\" | \"offset\" | \"simplify\" | \"translate\" | \"rotate\" | \"scale\" | \"mirror\" | \"decompose\"; polygons?: [number, number][][]; others?: [number, number][][][]; args?: number[] }", &format!("{SECTION} | {SECTION}[]"), "a 2D cross-section made, combined, offset or moved (Manifold's CrossSection), held by value"),
            ("solid.free", "{ id: number }", "null", "drop a solid"),
            ("media.live", "{}", "{ vdecs: number; vencs: number; adecs: number; frames: number; solids: number; worlds: number }", "how many native coders, frames, solids and worlds are alive (for leak checks)"),
            ("media.size", "{ path: string }", "number", "a file's size in bytes"),
            ("media.read", "{ path: string; start: number; end: number }", "string", "bytes start..end of a file, base64"),
            ("media.write", "{ path: string; data: string; truncate?: boolean; at?: number }", "null", "base64 bytes written to a file"),
            ("media.transcode", "{ path: string; out: string; from?: number; to?: number; height?: number; kbps?: number }", "{ frames: number; width: number; height: number; fps: number; kbps: number; bytes: number }", "trim, scale and encode with sound to MP4, natively end to end"),
            ("vdec.new", "{ codec: string; description: string }", "{ id: number }", "a VideoToolbox decoder for avc or hevc"),
            ("vdec.decode", "{ id: number; data: string; pts: number; dur: number }", "{ frame: number; pts: number; dur: number; width: number; height: number }[]", "a packet decoded; the frames it completes"),
            ("vdec.flush", "{ id: number }", "{ frame: number; pts: number; dur: number; width: number; height: number }[]", "the frames still held"),
            ("vdec.free", "{ id: number }", "null", "drop a decoder"),
            ("venc.new", "{ width: number; height: number; fps: number; kbps: number }", "{ id: number }", "a VideoToolbox H.264 encoder"),
            ("venc.encode", "{ id: number; frame: number; pts: number; dur: number }", "{ packets: { data: string; key: boolean; pts: number; dur: number }[]; description: string | null }", "an engine frame encoded"),
            ("venc.finish", "{ id: number }", "{ packets: { data: string; key: boolean; pts: number; dur: number }[]; description: string | null }", "the encoder flushed"),
            ("frame.info", "{ frame: number }", "{ width: number; height: number }", "an engine frame's size"),
            ("frame.png", "{ frame: number; path: string }", "null", "an engine frame written as PNG"),
            ("frame.scale", "{ frame: number; width: number; height: number }", "{ frame: number; width: number; height: number }", "an engine frame scaled (a new frame)"),
            ("frame.transform", "{ frame: number; rotation?: number; flip?: boolean; crop?: { left: number; top: number; width: number; height: number }; width: number; height: number; fit?: \"fill\" | \"contain\" | \"cover\" }", "{ frame: number; width: number; height: number }", "an engine frame rotated, flipped, cropped and resized (a new frame): mediabunny's transformations, natively"),
            ("frame.free", "{ frame: number }", "null", "drop an engine frame"),
            ("adec.new", "{ codec: string; description: string; rate: number; channels: number }", "{ id: number }", "a Symphonia decoder for aac, mp3 or flac"),
            ("adec.decode", "{ id: number; data: string }", "{ rate: number; channels: number; frames: number; data: string }", "a packet decoded to planar f32, base64"),
            ("venc.codecs", "{}", "{ hardware: string[]; avc: boolean; hevc: boolean; av1: boolean; vp9: boolean }", "the video codecs this machine encodes in hardware (VideoToolbox's list)"),
            ("aenc.new", "{ rate: number; channels: number; kbps?: number }", "{ id: number; description: string; priming: number }", "an AAC-LC encoder (AudioToolbox): its AudioSpecificConfig (base64) and its priming in frames"),
            ("aenc.encode", "{ id: number; data: string; channels: number }", "{ packets: { data: string; pts: number; dur: number }[] }", "planar f32 samples (base64, channel after channel) encoded; the packets they complete"),
            ("aenc.finish", "{ id: number }", "{ packets: { data: string; pts: number; dur: number }[] }", "the encoder flushed"),
            ("aenc.free", "{ id: number }", "null", "drop an encoder"),
            ("adec.free", "{ id: number }", "null", "drop a decoder"),
        ];
        // raea's command lines, in this process (the shell's rdb and raea-motion commands): run in `cwd` (a real folder)
        // with their output captured
        for (tool, doc) in [("rdb", "rdb's command line (media verbs too: rdb media, rdb ffmpeg)"), ("raea-motion", "raea-motion's command line (render, bundle, types)")] {
            raea_native::register(
                &format!("cli.{tool}"),
                "{ args: string[]; cwd: string }",
                "{ exit: number; stdout: string; stderr: string }",
                &format!("{doc}, run in process in cwd"),
                move |v| {
                    let args: Vec<String> = v.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                    let cwd = std::path::PathBuf::from(s(v, "cwd")?);
                    let run = move || match tool {
                        "rdb" => rdb::cli::cli(&args),
                        _ => crate::motion::cli(&args),
                    };
                    #[cfg(unix)]
                    {
                        let (exit, stdout, stderr) = raea_native::capture(&cwd, run)?;
                        Ok(json!({"exit": exit, "stdout": stdout, "stderr": stderr}))
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = cwd;
                        Ok(json!({"exit": run(), "stdout": "", "stderr": ""}))
                    }
                },
            );
        }
        for (name, args, answer, doc) in calls.iter().copied().chain(crate::ecs::CALLS.iter().copied()) {
            let n = name.to_string();
            raea_native::register(name, args, answer, doc, move |v| call(&n, v));
        }
        #[cfg(feature = "ml")]
        ml::register();
        // matchmove.solve: a clip's camera solve for <Matchmove> (motion/track.rs)
        crate::motion::track::register();
        // physics.part, physics.bake, physics.goo: rigid bodies and goo for <Physics> and <Goo> (motion/physics.rs)
        crate::motion::physics::register();
        // scene.bounds: a model's box turned and scaled, for place() (motion/scene_lint.rs)
        crate::motion::register_scene();
    });
}
