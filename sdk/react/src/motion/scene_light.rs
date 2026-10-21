//! `raea-motion scene light EDIT.tsx`: the light on a shot's subject, metered as a cinematographer's incident meter
//! reads it, from the scene's own lights and geometry on one frame (CPU, no engine, as the lint steps it). The subject is
//! what the camera's centre looks at (or `--part NAME`); the light on it is said light by light in lux, each light
//! seen or shadowed by rays against the scene's triangles (a light without castShadow lights through everything, as
//! the engine draws it), with its colour temperature, the key's share, the EV100 a meter sets for it and how far the
//! shot's own exposure is from that. Given several compositions (`--composition b,d,e`), each is metered at its first
//! and last frames and each cut between them is checked: the subject's brightness on screen and the light's colour
//! either side (docs/scene.feature, "A light plan").
//!
//! Light as the engine draws it (scene3d.rs): a DirectionalLight of intensity 1 is 8000 lux; point and spot lights
//! take lumens (intensity x 9e5), a candela each lumen over 4π, faded by Bevy's range window, a spot's cone by its
//! outer and inner angles; an AmbientLight of 1 is 400 cd/m², an <Environment> of 1 its panorama's values x 1000
//! cd/m², neither shadowed (the engine's ambient occlusion darkens them only in the picture).

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::sync::{Arc, Mutex, OnceLock};

use bevy::math::{Quat, Vec3};
use serde_json::{json, Value};

use super::super::super::{num, Frame};
use super::super::{all_items, hex, rgb, src_path, vec3, Item, AMBIENT, ENV, EV100, LUMENS, LUX};
use super::lint::{parts_of, ray_tri, Part};

/// An incident meter's calibration (ISO 2720, a flat receptor): EV100 = log2(E x 100 / C).
const C: f32 = 250.0;
/// Rays over the hemisphere for an <Environment>'s light.
const SKY_RAYS: usize = 512;
/// Off a surface before a shadow ray leaves it (metres).
const LIFT: f32 = 0.002;
/// A cut whose subject is brighter or darker on screen by more than this (stops) is a jump.
const JUMP_STOPS: f32 = 1.0 / 3.0;
/// A cut whose light differs in colour by more than this (mireds) is a white-balance drift.
const DRIFT_MIRED: f32 = 10.0;
/// The matrix meter's grid of rays across the picture.
const MATRIX: [usize; 2] = [7, 5];

/// A part that stops light: not see-through (transmission, opacity) nor unlit.
fn blocks(it: &Item) -> bool {
    match &it.material {
        Some(m) => {
            let p = &m.props;
            !(&*m.kind == "MeshBasicMaterial"
                || p.get("opacity").and_then(Value::as_f64).is_some_and(|o| o < 0.95)
                || p.get("transmission").and_then(Value::as_f64).is_some_and(|t| t > 0.05))
        }
        None => true,
    }
}

/// A ray against a box: whether it enters it before `tmax`.
fn slab(o: Vec3, inv: Vec3, lo: Vec3, hi: Vec3, tmax: f32) -> bool {
    let (a, b) = ((lo - o) * inv, (hi - o) * inv);
    let (near, far) = (a.min(b).max_element(), a.max(b).min_element());
    far >= near.max(0.0) && near <= tmax
}

/// The nearest triangle a ray meets before `tmax`: (distance, part, triangle).
fn nearest(parts: &[&Part], o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, usize, usize)> {
    let inv = Vec3::ONE / d;
    let mut best: Option<(f32, usize, usize)> = None;
    for (i, p) in parts.iter().enumerate() {
        let lim = best.map_or(tmax, |b| b.0);
        if !slab(o, inv, p.lo, p.hi, lim) {
            continue;
        }
        for (k, t) in p.tris.iter().enumerate() {
            if let Some(s) = ray_tri(o, d, *t).filter(|s| *s < best.map_or(tmax, |b| b.0)) {
                best = Some((s, i, k));
            }
        }
    }
    best
}

/// Whether anything stops a ray before `tmax`, and what.
fn shadowed(parts: &[&Part], o: Vec3, d: Vec3, tmax: f32) -> Option<usize> {
    let inv = Vec3::ONE / d;
    parts.iter().position(|p| slab(o, inv, p.lo, p.hi, tmax) && p.tris.iter().any(|t| ray_tri(o, d, *t).is_some_and(|s| s < tmax)))
}

/// An sRGB hex colour as linear light.
fn linear(c: Option<&Value>) -> Vec3 {
    let [r, g, b] = hex(c).map(|h| rgb(&h)).unwrap_or([1.0; 3]);
    let f = |c: f64| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } as f32;
    Vec3::new(f(r), f(g), f(b))
}

/// A light's colour temperature (K, McCamy's fit from its chromaticity), for light near white.
fn cct(c: Vec3) -> Option<f32> {
    let x = 0.4124 * c.x + 0.3576 * c.y + 0.1805 * c.z;
    let y = 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
    let z = 0.0193 * c.x + 0.1192 * c.y + 0.9505 * c.z;
    let s = x + y + z;
    if s <= 0.0 {
        return None;
    }
    let n = (x / s - 0.3320) / (0.1858 - y / s);
    Some(449.0 * n.powi(3) + 3525.0 * n.powi(2) + 6823.3 * n + 5520.33).filter(|k| (1000.0..=25000.0).contains(k))
}

/// A panorama's pixels (linear), read once a process.
struct Pano {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}

fn pano(path: &str) -> Result<Arc<Pano>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Result<Arc<Pano>, String>>>> = OnceLock::new();
    if let Some(p) = CACHE.get_or_init(Default::default).lock().unwrap().get(path) {
        return p.clone();
    }
    let made = image::open(path).map_err(|e| format!("{path}: {e}")).map(|im| {
        let im = im.to_rgb32f();
        let (w, h) = (im.width() as usize, im.height() as usize);
        // an 8-bit panorama is sRGB; an .hdr is linear already
        let srgb = !path.to_ascii_lowercase().ends_with(".hdr");
        let f = |c: f32| if !srgb { c } else if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        Arc::new(Pano { w, h, px: im.pixels().map(|p| [f(p[0]), f(p[1]), f(p[2])]).collect() })
    });
    CACHE.get_or_init(Default::default).lock().unwrap().insert(path.to_string(), made.clone());
    made
}

impl Pano {
    /// Its light along a world direction, as rdb's environment.rs maps it (+Y up, -Z the picture's middle).
    fn at(&self, d: Vec3) -> Vec3 {
        let u = 0.5 + d.x.atan2(-d.z) / TAU;
        let v = d.y.clamp(-1.0, 1.0).acos() / PI;
        let x = ((u * self.w as f32) as usize).min(self.w - 1);
        let y = ((v * self.h as f32) as usize).min(self.h - 1);
        Vec3::from(self.px[y * self.w + x])
    }
}

/// A sky panorama read for daylight (`scene.sky`), in the panorama's own units: its sun (the brightest point over the
/// horizon and the disc 2.5 degrees round it) as a world direction toward it once the panorama is turned by `rotation`
/// (radians about +Y, as <Environment rotation> turns it), and the light the sky puts on a level surface split into the
/// sun's and the rest's (the cosine-weighted integral over the upper hemisphere), the rest's mean luminance, and each
/// one's colour temperature. Daylight's units come from a sky model; these say the panorama's shares of it.
pub(crate) fn sky_of(path: &str, rotation: f32) -> Result<Value, String> {
    let p = pano(path)?;
    let (w, h) = (p.w, p.h);
    let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let dir = |x: usize, y: usize| {
        // the inverse of Pano::at: u = 0.5 + atan2(x, -z) / TAU, v = acos(y) / PI
        let (phi, th) = (((x as f32 + 0.5) / w as f32 - 0.5) * TAU, (y as f32 + 0.5) / h as f32 * PI);
        Vec3::new(th.sin() * phi.sin(), th.cos(), -th.sin() * phi.cos())
    };
    let (mut best, mut at) = (0.0, (0, 0));
    for y in 0..h / 2 {
        for x in 0..w {
            let l = lum(p.px[y * w + x]);
            if l > best {
                best = l;
                at = (x, y);
            }
        }
    }
    let sun_dir = dir(at.0, at.1);
    let disc = (2.5f32).to_radians().cos();
    let d_omega = (TAU / w as f32) * (PI / h as f32);
    let (mut e_sun, mut e_sky, mut l_sky, mut omega) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let (mut c_sun, mut c_sky) = (Vec3::ZERO, Vec3::ZERO);
    for y in 0..h / 2 {
        for x in 0..w {
            let d = dir(x, y);
            let c = Vec3::from(p.px[y * w + x]);
            let dw = d_omega * (((y as f32 + 0.5) / h as f32) * PI).sin();
            let e = lum(p.px[y * w + x]) * d.y.max(0.0) * dw;
            if d.dot(sun_dir) >= disc {
                e_sun += e;
                c_sun += c * dw;
            } else {
                e_sky += e;
                l_sky += lum(p.px[y * w + x]) * dw;
                omega += dw;
                c_sky += c * dw;
            }
        }
    }
    // a sun this faint against the sky is no sun: an overcast panorama
    let sunny = e_sun > 0.05 * e_sky;
    let turned = Quat::from_rotation_y(rotation) * sun_dir;
    Ok(json!({
        "sun": if sunny { json!({"dir": [turned.x, turned.y, turned.z], "elevation": sun_dir.y.asin().to_degrees(), "horizontal": e_sun, "cct": cct(c_sun)}) } else { Value::Null },
        "sky": {"horizontal": e_sky, "luminance": l_sky / omega.max(1e-6), "cct": cct(c_sky)},
    }))
}

/// The light from one source on the subject.
struct Got {
    name: String,
    kind: &'static str,
    lux: f32,
    colour: Vec3,
    /// what stops it, if something does
    blocked: Option<String>,
    shadows: bool,
}

/// What the camera's centre sees (or the part named), and the surface's normal there, facing the camera.
fn subject(its: &[Item], parts: &[&Part], all: &[Part], part: Option<&str>) -> Result<(Vec3, Vec3, String), String> {
    let cam = its.iter().find(|i| i.kind == "camera").ok_or("no camera")?;
    let p = |k: &str| {
        let v = vec3(&cam.props, k, [0.0, 0.0, -1.0]);
        cam.world.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32))
    };
    let eye = p("position");
    let aim = match part {
        Some(name) => {
            let q = all.iter().find(|q| q.label == name || q.label.ends_with(&format!(" {name}"))).ok_or_else(|| format!("no part {name}"))?;
            (q.lo + q.hi) / 2.0
        }
        None => p("lookAt"),
    };
    let mut d = (aim - eye).normalize();
    // a centre that looks out at the sky (through a window) meters the surface nearest it in the picture: rings of rays
    // out from the centre, 2 degrees apart, to 30
    let (u, v) = d.any_orthonormal_pair();
    let mut hit = nearest(parts, eye, d, 1e4);
    'rings: for ring in 1..=15 {
        if hit.is_some() {
            break;
        }
        let r = (2.0 * ring as f32).to_radians();
        for s in 0..16 {
            let a = s as f32 / 16.0 * TAU;
            let e = (d * r.cos() + (u * a.cos() + v * a.sin()) * r.sin()).normalize();
            if let Some(h) = nearest(parts, eye, e, 1e4) {
                hit = Some(h);
                d = e;
                break 'rings;
            }
        }
    }
    let (t, i, k) = hit.ok_or("the camera sees nothing within 30 degrees of its centre")?;
    let [a, b, c] = parts[i].tris[k];
    let mut n = (b - a).cross(c - a).normalize();
    if n.dot(d) > 0.0 {
        n = -n;
    }
    Ok((eye + d * t, n, parts[i].label.clone()))
}

/// The incident light at `at` (normal `n`) from each light the frame has.
fn sources(its: &[Item], parts: &[&Part], at: Vec3, n: Vec3) -> Vec<Got> {
    let o = at + n * LIFT;
    let mut out = Vec::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for it in its {
        let k = seen.entry(it.kind).or_insert(0);
        let name = it.props.get("name").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{}{}", it.kind, k));
        *k += 1;
        let pr = &it.props;
        let shadows = pr.get("castShadow").and_then(Value::as_bool) == Some(true);
        let colour = linear(pr.get("color"));
        let power = num(pr, "intensity").unwrap_or(1.0) as f32;
        let world = |key: &str, d: [f64; 3]| {
            let v = vec3(pr, key, d);
            it.world.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32))
        };
        let seen_by = |l: Vec3, far: f32| -> Option<String> { shadowed(parts, o, l, far).map(|j| parts[j].label.clone()) };
        match it.kind {
            "directional" => {
                // toward the light: from its target (the origin unless given) to its position
                let t = vec3(pr, "target", [0.0; 3]);
                let l = (world("position", [0.0, 1.0, 0.0]) - Vec3::new(t[0] as f32, t[1] as f32, t[2] as f32)).normalize();
                let cos = n.dot(l).max(0.0);
                let blocked = if shadows && cos > 0.0 { seen_by(l, 1e4) } else { None };
                let lux = if blocked.is_some() { 0.0 } else { power * LUX as f32 * cos };
                out.push(Got { name, kind: "sun", lux, colour, blocked, shadows });
            }
            "point" | "spot" => {
                let pos = world("position", [0.0; 3]);
                let to = pos - at;
                let r = to.length().max(1e-4);
                let l = to / r;
                let cos = n.dot(l).max(0.0);
                let candela = power * LUMENS as f32 / (4.0 * PI);
                // Bevy's range window: (1 - (d/range)^4)^2, the light gone at its range
                let range = num(pr, "distance").filter(|d| *d > 0.0).unwrap_or(20.0) as f32;
                let window = (1.0 - (r / range).powi(4)).clamp(0.0, 1.0).powi(2);
                let mut lux = candela * cos * window / (r * r);
                if it.kind == "spot" {
                    let t = vec3(pr, "target", [0.0; 3]);
                    let axis = (Vec3::new(t[0] as f32, t[1] as f32, t[2] as f32) - pos).normalize();
                    let outer = (num(pr, "angle").unwrap_or(std::f64::consts::FRAC_PI_3) as f32).clamp(0.0, PI / 2.0);
                    let inner = outer * (1.0 - (num(pr, "penumbra").unwrap_or(0.0) as f32).clamp(0.0, 1.0));
                    let (co, ci) = (outer.cos(), inner.cos());
                    let scale = 1.0 / (ci - co).max(1e-4);
                    lux *= ((-l).dot(axis) * scale - co * scale).clamp(0.0, 1.0).powi(2);
                }
                let blocked = if shadows && lux > 0.0 { seen_by(l, r - LIFT) } else { None };
                if blocked.is_some() {
                    lux = 0.0;
                }
                out.push(Got { name, kind: if it.kind == "spot" { "spot" } else { "point" }, lux, colour, blocked, shadows });
            }
            "ambient" => out.push(Got { name, kind: "ambient", lux: PI * power * AMBIENT as f32, colour, blocked: None, shadows: false }),
            "environment" => {
                let Some(src) = pr.get("src").and_then(Value::as_str) else { continue };
                let Ok(pano) = pano(&src_path(src)) else { continue };
                let k = power * ENV as f32;
                let rot = Quat::from_rotation_y(num(pr, "rotation").unwrap_or(0.0) as f32).inverse();
                // cosine-weighted directions about the normal (a Fibonacci spiral): irradiance = π x their mean
                let (t1, t2) = n.any_orthonormal_pair();
                let mut sum = Vec3::ZERO;
                for s in 0..SKY_RAYS {
                    let u = (s as f32 + 0.5) / SKY_RAYS as f32;
                    let phi = s as f32 * 2.399_963;
                    let (r, z) = (u.sqrt(), (1.0 - u).sqrt());
                    let d = t1 * (r * phi.cos()) + t2 * (r * phi.sin()) + n * z;
                    sum += pano.at(rot * d);
                }
                let e = sum / SKY_RAYS as f32 * PI * k;
                let y = 0.2126 * e.x + 0.7152 * e.y + 0.0722 * e.z;
                out.push(Got { name, kind: "environment", lux: y, colour: if y > 0.0 { e / y } else { Vec3::ONE }, blocked: None, shadows: false });
            }
            _ => {}
        }
    }
    out
}

/// The shot's camera exposure (EV100), as scene3d.rs sets it from <ToneMapping exposure>.
fn exposure(its: &[Item]) -> f32 {
    let e = its.iter().find(|i| i.kind == "tonemapping").and_then(|t| num(&t.props, "exposure")).unwrap_or(0.0);
    (EV100 - e) as f32
}

/// The frame as a camera's matrix meter reads it: the light on what each of a grid of rays across the picture meets,
/// averaged in stops (the geometric mean), so a cut is judged on the whole picture, not on two different subjects.
fn matrix(its: &[Item], parts: &[&Part], aspect: f32) -> Option<f32> {
    let cam = its.iter().find(|i| i.kind == "camera")?;
    let p = |k: &str, d: [f64; 3]| {
        let v = vec3(&cam.props, k, d);
        cam.world.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32))
    };
    let eye = p("position", [0.0; 3]);
    let fwd = (p("lookAt", [0.0, 0.0, -1.0]) - eye).normalize();
    let up0 = vec3(&cam.props, "up", [0.0, 1.0, 0.0]);
    let right = fwd.cross(Vec3::new(up0[0] as f32, up0[1] as f32, up0[2] as f32)).normalize();
    let up = right.cross(fwd);
    let ty = (num(&cam.props, "fov").unwrap_or(50.0) as f32).to_radians() / 2.0;
    let (ty, tx) = (ty.tan(), ty.tan() * aspect);
    let (mut sum, mut k) = (0.0f32, 0);
    for j in 0..MATRIX[1] {
        for i in 0..MATRIX[0] {
            let x = ((i as f32 + 0.5) / MATRIX[0] as f32) * 2.0 - 1.0;
            let y = ((j as f32 + 0.5) / MATRIX[1] as f32) * 2.0 - 1.0;
            let d = (fwd + right * (x * tx) + up * (y * ty)).normalize();
            let Some((t, pi, ki)) = nearest(parts, eye, d, 1e4) else { continue };
            let [a, b, c] = parts[pi].tris[ki];
            let mut n = (b - a).cross(c - a).normalize();
            if n.dot(d) > 0.0 {
                n = -n;
            }
            let e: f32 = sources(its, parts, eye + d * t, n).iter().map(|g| g.lux).sum();
            sum += e.max(0.01).log2();
            k += 1;
        }
    }
    (k > 0).then(|| 2f32.powf(sum / k as f32))
}

/// One frame metered.
fn meter(its: &[Item], part: Option<&str>, aspect: f32) -> Result<Value, String> {
    let (all, items, _) = parts_of(its, &vec![false; its.len()]);
    let parts: Vec<&Part> = all.iter().zip(&items).filter(|(_, it)| blocks(it)).map(|(p, _)| p).collect();
    let (at, n, what) = subject(its, &parts, &all, part)?;
    let frame = matrix(its, &parts, aspect);
    let got = sources(its, &parts, at, n);
    let total: f32 = got.iter().map(|g| g.lux).sum();
    let mix = got.iter().fold(Vec3::ZERO, |a, g| a + g.colour * g.lux);
    let key = got.iter().max_by(|a, b| a.lux.total_cmp(&b.lux));
    let set = exposure(its);
    let metered = (total.max(1e-6) * 100.0 / C).log2();
    Ok(json!({
        "subject": what, "at": [at.x, at.y, at.z], "normal": [n.x, n.y, n.z],
        "lux": total,
        "kelvin": cct(mix).map(|k| k.round()),
        "key": key.map(|k| json!({"light": k.name, "share": if total > 0.0 { k.lux / total } else { 0.0 }})),
        "ev100": {"metered": metered, "set": set, "over": metered - set},
        // the subject's brightness on screen, in stops: its light over the camera's exposure
        "screen": total.max(1e-6).log2() - set,
        // the whole picture's, as a matrix meter reads it (what a cut is judged on)
        "frame": frame.map(|f| json!({"lux": f, "screen": f.log2() - set, "over": (f * 100.0 / C).log2() - set})),
        "lights": got.iter().map(|g| json!({
            "name": g.name, "kind": g.kind, "lux": g.lux, "share": if total > 0.0 { g.lux / total } else { 0.0 },
            "kelvin": cct(g.colour).map(|k| k.round()), "shadows": g.shadows, "blockedBy": g.blocked,
        })).collect::<Vec<_>>(),
    }))
}

fn say(comp: &str, frame: i64, m: &Value) -> String {
    let f = |v: &Value| v.as_f64().unwrap_or(0.0);
    let mut t = vec![format!(
        "{comp} frame {frame}: {} at {:.0} lux{}, EV100 {:.2} metered, {:.2} set ({:+.2} stops over a meter), key {} {:.0}%; the picture {:.0} lux ({:+.2} stops over)",
        m["subject"].as_str().unwrap_or(""),
        f(&m["lux"]),
        m["kelvin"].as_f64().map(|k| format!(", {k:.0} K")).unwrap_or_default(),
        f(&m["ev100"]["metered"]),
        f(&m["ev100"]["set"]),
        f(&m["ev100"]["over"]),
        m["key"]["light"].as_str().unwrap_or("-"),
        f(&m["key"]["share"]) * 100.0,
        f(&m["frame"]["lux"]),
        f(&m["frame"]["over"])
    )];
    for l in m["lights"].as_array().into_iter().flatten() {
        let why = match (l["blockedBy"].as_str(), l["shadows"].as_bool()) {
            (Some(b), _) => format!("shadowed by {b}"),
            (None, Some(false)) if l["kind"] != "environment" && l["kind"] != "ambient" => "no shadows: lights through everything".into(),
            _ => String::new(),
        };
        t.push(format!(
            "  {:<12} {:<12} {:>9.1} lux {:>4.0}%{}  {why}",
            l["kind"].as_str().unwrap_or(""),
            l["name"].as_str().unwrap_or(""),
            f(&l["lux"]),
            f(&l["share"]) * 100.0,
            l["kelvin"].as_f64().map(|k| format!(" {k:>6.0} K")).unwrap_or_else(|| "         ".into())
        ));
    }
    t.join("\n")
}

/// One composition's frames, stepped.
fn frames_of(args: &[String], tsx: &std::path::Path, comp: &str) -> Result<(Vec<(i64, Frame)>, f32), String> {
    use super::super::super::{bundle_composition, comp_opts, scene_frames_to};
    let dir = tsx.parent().unwrap().to_path_buf();
    let mut opts = comp_opts(args)?;
    opts.composition = Some(comp.to_string()).filter(|c| !c.is_empty());
    let js = std::env::temp_dir().join(format!("raea-light-{}-{comp}.js", std::process::id()));
    bundle_composition(tsx, &js, &opts, None)?;
    let got = scene_frames_to(&js, &dir, 1);
    let _ = std::fs::remove_file(&js);
    let (frames, w, h, _) = got?.ok_or("the composition has no 3D")?;
    Ok((frames, w as f32 / h.max(1) as f32))
}

pub(crate) fn cli(args: &[String]) -> Result<String, String> {
    let usage = "raea-motion scene light EDIT.tsx [--composition ID[,ID...]] [--frame N|last] [--part NAME] [--json]";
    let Some(edit) = args.get(2) else { return Err(usage.into()) };
    let tsx = std::path::PathBuf::from(edit).canonicalize().map_err(|e| format!("{edit}: {e}"))?;
    let flag = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let comps: Vec<String> = flag("--composition").map(|c| c.split(',').map(str::to_string).collect()).unwrap_or_else(|| vec![String::new()]);
    let part = flag("--part");
    let json_out = args.iter().any(|a| a == "--json");
    let pick = |frames: &[(i64, Frame)], which: &str| -> Result<usize, String> {
        match which {
            "last" => Ok(frames.len() - 1),
            f => {
                let f: i64 = f.parse().map_err(|_| "--frame takes a frame number or last")?;
                frames.iter().position(|(n, _)| *n == f).ok_or_else(|| format!("no frame {f}"))
            }
        }
    };
    let mut shots: Vec<(String, Vec<(i64, Value)>)> = Vec::new();
    for c in &comps {
        let (frames, aspect) = frames_of(args, &tsx, c)?;
        let all = all_items(&frames);
        // one composition: the frame asked for; a sequence: each shot's first and last, for its cuts
        let at: Vec<usize> = if comps.len() == 1 { vec![pick(&frames, flag("--frame").as_deref().unwrap_or("0"))?] } else { vec![0, frames.len() - 1] };
        let mut m = Vec::new();
        for k in at {
            m.push((frames[k].0, meter(&all[k], part.as_deref(), aspect).map_err(|e| format!("{c} frame {}: {e}", frames[k].0))?));
        }
        shots.push((c.clone(), m));
    }
    // each cut: the last frame of one shot against the first of the next
    let mut cuts = Vec::new();
    for w in shots.windows(2) {
        let (a, b) = (&w[0].1.last().unwrap().1, &w[1].1[0].1);
        // judged on the whole picture (the matrix meter), the subject's when a picture meets nothing
        let screen = |m: &Value| m["frame"]["screen"].as_f64().or_else(|| m["screen"].as_f64()).unwrap_or(0.0);
        let stops = screen(b) - screen(a);
        let mired = match (a["kelvin"].as_f64(), b["kelvin"].as_f64()) {
            (Some(x), Some(y)) => Some(1e6 / y - 1e6 / x),
            _ => None,
        };
        let jump = stops.abs() > JUMP_STOPS as f64;
        let drift = mired.is_some_and(|m| m.abs() > DRIFT_MIRED as f64);
        let key_a = a["key"]["light"].as_str().unwrap_or("");
        let key_b = b["key"]["light"].as_str().unwrap_or("");
        cuts.push(json!({"from": w[0].0, "to": w[1].0, "stops": stops, "mired": mired, "jump": jump, "drift": drift,
            "keys": [key_a, key_b], "says": format!("{} to {}: the picture {:+.2} stops{}{}{}{}", w[0].0, w[1].0, stops,
                mired.map(|m| format!(", light {m:+.0} mired")).unwrap_or_default(),
                if jump { " -- a jump" } else { "" }, if drift { " -- a white-balance drift" } else { "" },
                if key_a != key_b { format!(" (key {key_a} to {key_b})") } else { String::new() })}));
    }
    if json_out {
        let shots: Vec<Value> = shots.iter().map(|(c, m)| json!({"composition": c, "frames": m.iter().map(|(f, v)| json!({"frame": f, "meter": v})).collect::<Vec<_>>()})).collect();
        return Ok(serde_json::to_string_pretty(&json!({"shots": shots, "cuts": cuts})).unwrap_or_default());
    }
    let mut t = Vec::new();
    for (c, m) in &shots {
        for (f, v) in m {
            t.push(say(if c.is_empty() { "the composition" } else { c }, *f, v));
        }
    }
    for c in &cuts {
        t.push(format!("cut {}", c["says"].as_str().unwrap_or("")));
    }
    Ok(t.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_constants() {
        // an incident reading of 2.5 lux is EV100 0; a sunny day's 100,000 lux about EV100 15.3
        assert!(((2.5f32 * 100.0 / C).log2()).abs() < 1e-6);
        assert!(((1.0e5f32 * 100.0 / C).log2() - 15.29).abs() < 0.01);
        // white light is about 6500 K, a warm bulb's colour lower
        let d65 = cct(Vec3::ONE).unwrap();
        assert!((d65 - 6500.0).abs() < 150.0, "{d65}");
        let warm = cct(linear(Some(&json!("#ffe9cc")))).unwrap();
        assert!(warm < 5500.0, "{warm}");
    }
}
