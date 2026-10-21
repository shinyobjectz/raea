//! 3D shapes as values (docs/media.feature M2, engine/html/docs/bevy.feature B5c): a `mesh` row's `shape` column is a CSG
//! expression, built by Manifold (manifold-rust, a pure-Rust port of Manifold 3.5) and cached by the expression, so it
//! is re-meshed only when the expression changes. Units are the scene's (a cube row of size 1 is one unit).
//!
//!   box(w, h, d)                     centred                          sphere(r)          cylinder(r, h) / cone(r1, r2, h)
//!   rounded_box(w, h, d, r)          its edges rounded by r           torus(R, r)        text3d("SALE", size, depth [, "Inter", weight])
//!   union(a, b, ...)  minus(a, b, ...)  intersect(a, b, ...)  hull(a, b, ...)
//!   move(a, x, y, z)  rotate(a, x°, y°, z°)  scale(a, s) / scale(a, x, y, z)
//!   mirror(a, nx, ny, nz)  transform(a, 12 numbers by columns)  trim(a, nx, ny, nz, offset)  minkowski(a, b)  minkowski_diff(a, b)
//!   refine(a, n)  refine_length(a, len)  refine_tolerance(a, tol)  smooth(a, angle°, smoothness, n)  simplify(a, tol)
//!   extrude("x y, ...; ...", h [, divisions, twist°, scale_x, scale_y])  revolve("x y, ...", [degrees, segments])
//!   obj("path") / glb("path")       a mesh file, closed into a solid
//!
//! `minus(rounded_box(2, 1, 0.2, 0.1), text3d("SALE", 0.45, 0.3))` is a sign with the word cut through it.
//!
//! Space and time (docs/media.feature M3): a clip row's `mask` column is a solid in (x, y, frame), in the edit's pixels
//! (y down), sliced at each frame into the region the clip shows. With the shapes above, and:
//!
//!   frame()                          the whole picture, all the time        rect(x, y, w, h) / disc(cx, cy, r): all the time
//!   wipe("right"|"left"|"down"|"up", f0, f1)   what a straight wipe has revealed: nothing before f0, all after f1
//!   during(a, f0, f1)                a from frame f0 to f1                   slide(a, dx, dy, f0, f1): a moving by (dx, dy) over f0..f1
//!
//! `minus(frame(), slide(disc(200, 960, 150), 680, 0, 0, 150))` is the clip with a hole moving across it.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use manifold_rust::cross_section::CrossSection;
use manifold_rust::linalg::{Vec2, Vec3};
use manifold_rust::manifold::Manifold;

const SEGMENTS: i32 = 48;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Num(f64),
    Str(String),
    Open,
    Close,
    Comma,
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let (mut i, mut out) = (0, Vec::new());
    while i < c.len() {
        let ch = c[i];
        match ch {
            ' ' | '\t' | '\n' => i += 1,
            '(' => {
                out.push(Tok::Open);
                i += 1
            }
            ')' => {
                out.push(Tok::Close);
                i += 1
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1
            }
            '"' | '\'' => {
                let end = c[i + 1..].iter().position(|x| *x == ch).ok_or("a string with no closing quote")? + i + 1;
                out.push(Tok::Str(c[i + 1..end].iter().collect()));
                i = end + 1;
            }
            d if d.is_ascii_digit() || d == '-' || d == '.' => {
                let start = i;
                i += 1;
                while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.' || c[i] == 'e') {
                    i += 1;
                }
                let t: String = c[start..i].iter().collect();
                out.push(Tok::Num(t.parse().map_err(|_| format!("not a number: {t}"))?));
            }
            a if a.is_alphabetic() || a == '_' => {
                let start = i;
                while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '3') {
                    i += 1;
                }
                out.push(Tok::Name(c[start..i].iter().collect()));
            }
            o => return Err(format!("unexpected {o:?} in a shape")),
        }
    }
    Ok(out)
}

#[derive(Clone)]
enum Arg {
    Num(f64),
    Str(String),
    Shape(Manifold),
}

struct P {
    t: Vec<Tok>,
    i: usize,
}

impl P {
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }

    fn expr(&mut self) -> Result<Arg, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Arg::Num(n)),
            Some(Tok::Str(s)) => Ok(Arg::Str(s)),
            Some(Tok::Name(f)) => {
                if self.next() != Some(Tok::Open) {
                    return Err(format!("{f} takes arguments: {f}(...)"));
                }
                let mut args = Vec::new();
                if self.t.get(self.i) == Some(&Tok::Close) {
                    self.i += 1;
                } else {
                    loop {
                        args.push(self.expr()?);
                        match self.next() {
                            Some(Tok::Comma) => continue,
                            Some(Tok::Close) => break,
                            _ => return Err(format!("{f}(...): a comma or a closing parenthesis")),
                        }
                    }
                }
                call(&f, args).map(Arg::Shape)
            }
            t => Err(format!("a shape, a number or a string, not {t:?}")),
        }
    }
}

fn nums(f: &str, a: &[Arg], n: usize) -> Result<Vec<f64>, String> {
    let v: Vec<f64> = a.iter().filter_map(|x| if let Arg::Num(n) = x { Some(*n) } else { None }).collect();
    if v.len() < n {
        return Err(format!("{f} needs {n} numbers"));
    }
    Ok(v)
}

fn shapes(f: &str, a: Vec<Arg>) -> Result<Vec<Manifold>, String> {
    let v: Vec<Manifold> = a.into_iter().filter_map(|x| if let Arg::Shape(m) = x { Some(m) } else { None }).collect();
    if v.is_empty() {
        return Err(format!("{f} needs shapes"));
    }
    Ok(v)
}

thread_local! {
    /// The edit's (width, height, frames) while a space-time mask is built.
    static PICTURE: std::cell::Cell<Option<(f64, f64, f64)>> = const { std::cell::Cell::new(None) };
}

/// A prism over all of the edit's time (a frame before the first to one after the last).
fn always(section: CrossSection) -> Result<Manifold, String> {
    let (_, _, n) = PICTURE.with(|p| p.get()).ok_or("frame(), rect, disc, wipe, during and slide are for a clip's mask")?;
    Ok(Manifold::extrude(&section.to_polygons(), n + 2.0, 1, 0.0, Vec2::new(1.0, 1.0)).translate(Vec3::new(0.0, 0.0, -1.0)))
}

fn space_time(f: &str, a: &[Arg]) -> Option<Result<Manifold, String>> {
    if !matches!(f, "frame" | "rect" | "disc" | "wipe") {
        return None;
    }
    let pic = PICTURE.with(|p| p.get());
    let rect = |x: f64, y: f64, w: f64, h: f64| CrossSection::from_rect(&manifold_rust::types::Rect { min: Vec2::new(x, y), max: Vec2::new(x + w, y + h) });
    Some((|| -> Result<Manifold, String> {
        let (w, h, _) = pic.ok_or_else(|| format!("{f}() is for a clip's mask (space and time)"))?;
        Ok(match f {
            "frame" => always(rect(0.0, 0.0, w, h))?,
            "rect" => {
                let n = nums(f, a, 4)?;
                always(rect(n[0], n[1], n[2], n[3]))?
            }
            "disc" => {
                let n = nums(f, a, 3)?;
                always(CrossSection::circle(n[2], 96).translate(Vec2::new(n[0], n[1])))?
            }
            "wipe" => {
                let dir = a.iter().find_map(|x| if let Arg::Str(s) = x { Some(s.clone()) } else { None }).unwrap_or_else(|| "right".into());
                let n = nums(f, a, 2)?;
                let (f0, f1) = (n[0], n[1].max(n[0] + 1e-6));
                // revealed where the edge has passed: a plane tilted in (x or y, t); trim keeps n . p >= offset
                let (k, along) = match dir.as_str() { "right" | "left" => (w / (f1 - f0), w), _ => (h / (f1 - f0), h) };
                let (nrm, off) = match dir.as_str() {
                    "right" => (Vec3::new(-1.0, 0.0, k), k * f0),
                    "left" => (Vec3::new(1.0, 0.0, -k), along - k * f0),
                    "down" => (Vec3::new(0.0, -1.0, k), k * f0),
                    "up" => (Vec3::new(0.0, 1.0, -k), along - k * f0),
                    o => return Err(format!("wipe({o:?}): right, left, down or up")),
                };
                let l = (nrm.x * nrm.x + nrm.y * nrm.y + nrm.z * nrm.z).sqrt();
                always(rect(0.0, 0.0, w, h))?.trim_by_plane(Vec3::new(nrm.x / l, nrm.y / l, nrm.z / l), off / l)
            }
            _ => unreachable!(),
        })
    })())
}

fn call(f: &str, a: Vec<Arg>) -> Result<Manifold, String> {
    if let Some(r) = space_time(f, &a) {
        return r;
    }
    let fold = |v: Vec<Manifold>, op: fn(&Manifold, &Manifold) -> Manifold| v.into_iter().reduce(|x, y| op(&x, &y)).unwrap();
    Ok(match f {
        "box" | "cube" => {
            let n = nums(f, &a, 1)?;
            let (w, h, d) = if n.len() >= 3 { (n[0], n[1], n[2]) } else { (n[0], n[0], n[0]) };
            Manifold::cube(Vec3::new(w, h, d), true)
        }
        "rounded_box" => {
            let n = nums(f, &a, 4)?;
            let r = n[3].clamp(0.0, n[0].min(n[1]).min(n[2]) / 2.0 - 1e-6);
            // a box shrunk by r, swept by a sphere of r: the hull of eight spheres at its corners
            let (hx, hy, hz) = (n[0] / 2.0 - r, n[1] / 2.0 - r, n[2] / 2.0 - r);
            let ball = Manifold::sphere(r, SEGMENTS / 2);
            let corners: Vec<Manifold> = [-1.0, 1.0]
                .iter()
                .flat_map(|x| [-1.0, 1.0].iter().flat_map(move |y| [-1.0, 1.0].iter().map(move |z| (*x, *y, *z))))
                .map(|(x, y, z)| ball.translate(Vec3::new(x * hx, y * hy, z * hz)))
                .collect();
            Manifold::hull_manifolds(&corners)
        }
        "sphere" => Manifold::sphere(nums(f, &a, 1)?[0], SEGMENTS),
        "cylinder" => {
            let n = nums(f, &a, 2)?;
            Manifold::cylinder_centered(n[1], n[0], n[0], SEGMENTS, true)
        }
        "cone" => {
            let n = nums(f, &a, 3)?;
            Manifold::cylinder_centered(n[2], n[0], n[1], SEGMENTS, true)
        }
        "torus" => {
            let n = nums(f, &a, 2)?;
            let ring = CrossSection::circle(n[1], SEGMENTS / 2).translate(Vec2::new(n[0], 0.0));
            Manifold::revolve(&ring.to_polygons(), SEGMENTS, 360.0)
        }
        "text3d" => {
            let text = a.iter().find_map(|x| if let Arg::Str(s) = x { Some(s.clone()) } else { None }).ok_or("text3d(\"WORD\", size, depth)")?;
            let n = nums(f, &a, 2)?;
            let family = a.iter().filter_map(|x| if let Arg::Str(s) = x { Some(s.clone()) } else { None }).nth(1).unwrap_or_else(|| "Inter".into());
            let weight = n.get(2).copied().unwrap_or(800.0) as u16;
            text3d(&text, n[0], n[1], &family, weight)?
        }
        "union" => fold(shapes(f, a)?, Manifold::union),
        "minus" | "difference" => fold(shapes(f, a)?, Manifold::difference),
        "intersect" | "intersection" => fold(shapes(f, a)?, Manifold::intersection),
        "hull" => Manifold::hull_manifolds(&shapes(f, a)?),
        "during" => {
            let n = nums(f, &a, 2)?;
            let m = shapes(f, a)?.remove(0);
            let b = m.bounding_box();
            let slab = Manifold::cube(Vec3::new(b.max.x - b.min.x + 2.0, b.max.y - b.min.y + 2.0, n[1] - n[0]), false).translate(Vec3::new(b.min.x - 1.0, b.min.y - 1.0, n[0]));
            m.intersection(&slab)
        }
        "slide" => {
            // the shape from f0 to f1, moved by (dx, dy) in proportion: a shear of its prism through time
            let n = nums(f, &a, 4)?;
            let (dx, dy, f0, f1) = (n[0], n[1], n[2], n[3].max(n[2] + 1e-6));
            let m = shapes(f, a)?.remove(0);
            let b = m.bounding_box();
            let slab = Manifold::cube(Vec3::new(b.max.x - b.min.x + 2.0, b.max.y - b.min.y + 2.0, f1 - f0), false).translate(Vec3::new(b.min.x - 1.0, b.min.y - 1.0, f0));
            m.intersection(&slab).warp(move |v| {
                let u = (v.z - f0) / (f1 - f0);
                v.x += u * dx;
                v.y += u * dy;
            })
        }
        "move" | "translate" => {
            let n = nums(f, &a, 3)?;
            shapes(f, a)?.remove(0).translate(Vec3::new(n[0], n[1], n[2]))
        }
        "rotate" => {
            let n = nums(f, &a, 3)?;
            shapes(f, a)?.remove(0).rotate(n[0], n[1], n[2])
        }
        "scale" => {
            let n = nums(f, &a, 1)?;
            let v = if n.len() >= 3 { Vec3::new(n[0], n[1], n[2]) } else { Vec3::new(n[0], n[0], n[0]) };
            shapes(f, a)?.remove(0).scale(v)
        }
        // cross-sections made solid: a polygon string is "x y, x y, ...; x y, ..." (contours by ';', points by ',')
        "extrude" => {
            let poly = polygons_arg(f, &a)?;
            let n = nums(f, &a, 1)?;
            let g = |i: usize, d: f64| n.get(i).copied().unwrap_or(d);
            Manifold::extrude(&poly, n[0], g(1, 1.0) as i32, g(2, 0.0), Vec2::new(g(3, 1.0), g(4, 1.0)))
        }
        "revolve" => {
            let poly = polygons_arg(f, &a)?;
            let n: Vec<f64> = a.iter().filter_map(|x| if let Arg::Num(v) = x { Some(*v) } else { None }).collect();
            Manifold::revolve(&poly, n.get(1).map(|s| *s as i32).unwrap_or(SEGMENTS), n.first().copied().unwrap_or(360.0))
        }
        "mirror" => {
            let n = nums(f, &a, 3)?;
            shapes(f, a)?.remove(0).mirror(Vec3::new(n[0], n[1], n[2]))
        }
        "transform" => {
            // a 3x4 matrix, column by column (x, y, z axes, then the translation)
            let n = nums(f, &a, 12)?;
            let c = |i: usize| Vec3::new(n[i], n[i + 1], n[i + 2]);
            shapes(f, a)?.remove(0).transform(&manifold_rust::linalg::Mat3x4::from_cols(c(0), c(3), c(6), c(9)))
        }
        "trim" => {
            // what's on the normal's side of the plane n . p = offset
            let n = nums(f, &a, 4)?;
            shapes(f, a)?.remove(0).trim_by_plane(Vec3::new(n[0], n[1], n[2]), n[3])
        }
        "minkowski" => {
            let mut v = shapes(f, a)?;
            if v.len() < 2 {
                return Err("minkowski(a, b): two shapes".into());
            }
            let b = v.remove(1);
            v.remove(0).minkowski_sum(&b)
        }
        "minkowski_diff" => {
            let mut v = shapes(f, a)?;
            if v.len() < 2 {
                return Err("minkowski_diff(a, b): two shapes".into());
            }
            let b = v.remove(1);
            v.remove(0).minkowski_difference(&b)
        }
        "refine" => {
            let n = nums(f, &a, 1)?;
            shapes(f, a)?.remove(0).refine(n[0].max(1.0) as i32)
        }
        "smooth" => {
            // sharp edges kept above min_sharp_angle (degrees), the rest rounded, then refined n times
            let n: Vec<f64> = a.iter().filter_map(|x| if let Arg::Num(v) = x { Some(*v) } else { None }).collect();
            let m = shapes(f, a)?.remove(0);
            m.smooth_out(n.first().copied().unwrap_or(60.0), n.get(1).copied().unwrap_or(0.0)).refine(n.get(2).copied().unwrap_or(4.0).max(1.0) as i32)
        }
        // the mesh coarsened (vertices closer than tol merged) or refined until edges are under a length, or the
        // smoothed surface is within a tolerance
        "simplify" => {
            let n = nums(f, &a, 1)?;
            shapes(f, a)?.remove(0).simplify(n[0])
        }
        "refine_length" => {
            let n = nums(f, &a, 1)?;
            shapes(f, a)?.remove(0).refine_to_length(n[0])
        }
        "refine_tolerance" => {
            let n = nums(f, &a, 1)?;
            shapes(f, a)?.remove(0).refine_to_tolerance(n[0])
        }
        "obj" | "glb" => {
            let path = a.iter().find_map(|x| if let Arg::Str(s) = x { Some(s.clone()) } else { None }).ok_or_else(|| format!("{f}(\"path\")"))?;
            let (verts, tris) = if f == "obj" { read_obj(&path)? } else { read_glb(&path)? };
            from_mesh(&verts, &tris)?
        }
        o => return Err(format!("no shape {o}: box, rounded_box, sphere, cylinder, cone, torus, text3d, extrude, revolve, union, minus, intersect, hull, minkowski, minkowski_diff, move, rotate, scale, mirror, transform, trim, refine, smooth, simplify, refine_length, refine_tolerance, obj, glb; in a mask also frame, rect, disc, wipe, during, slide")),
    })
}

/// The polygon string argument: "x y, x y, ...; x y, ..." (contours by ';', points by ',', each counter-clockwise for
/// solid and clockwise for a hole, as Manifold winds them).
fn polygons_arg(f: &str, a: &[Arg]) -> Result<manifold_rust::types::Polygons, String> {
    let text = a.iter().find_map(|x| if let Arg::Str(s) = x { Some(s.clone()) } else { None }).ok_or_else(|| format!("{f}(\"x y, x y, ...\", ...): give the outline"))?;
    let mut out = Vec::new();
    for contour in text.split(';').filter(|c| !c.trim().is_empty()) {
        let mut pts = Vec::new();
        for pt in contour.split(',') {
            let xy: Vec<f64> = pt.split_whitespace().map(|v| v.parse::<f64>().map_err(|_| format!("{f}: bad number {v:?}"))).collect::<Result<_, _>>()?;
            if xy.len() != 2 {
                return Err(format!("{f}: a point is \"x y\", got {pt:?}"));
            }
            pts.push(Vec2::new(xy[0], xy[1]));
        }
        if pts.len() < 3 {
            return Err(format!("{f}: a contour needs 3 points"));
        }
        out.push(pts);
    }
    Ok(out)
}

/// A solid from triangles (positions flat x y z, indices by threes); merged by position so it closes.
pub fn from_mesh(verts: &[f32], tris: &[u32]) -> Result<Manifold, String> {
    let mut mesh = manifold_rust::types::MeshGL::default();
    mesh.num_prop = 3;
    mesh.vert_properties = verts.to_vec();
    mesh.tri_verts = tris.to_vec();
    mesh.merge();
    let m = Manifold::from_mesh_gl(&mesh);
    if m.is_empty() {
        return Err("the mesh isn't a closed solid (a manifold)".into());
    }
    Ok(m)
}

/// A solid's triangles: positions flat (x y z) and indices by threes.
pub fn to_mesh(m: &Manifold) -> (Vec<f32>, Vec<u32>) {
    let g = m.get_mesh_gl(-1);
    let np = g.num_prop as usize;
    let verts: Vec<f32> = g.vert_properties.chunks(np).flat_map(|c| [c[0], c[1], c[2]]).collect();
    (verts, g.tri_verts.clone())
}

/// Wavefront OBJ: v lines and f lines (polygons fanned, "i/t/n" indices, negatives from the end).
pub fn read_obj(path: &str) -> Result<(Vec<f32>, Vec<u32>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let (mut v, mut t) = (Vec::new(), Vec::new());
    for line in text.lines() {
        let mut w = line.split_whitespace();
        match w.next() {
            Some("v") => {
                for _ in 0..3 {
                    v.push(w.next().and_then(|x| x.parse::<f32>().ok()).ok_or_else(|| format!("{path}: a bad v line"))?);
                }
            }
            Some("f") => {
                let n = (v.len() / 3) as i64;
                let idx: Vec<u32> = w.map(|x| x.split('/').next().unwrap_or("").parse::<i64>().map(|i| if i < 0 { (n + i) as u32 } else { (i - 1) as u32 })).collect::<Result<_, _>>().map_err(|_| format!("{path}: a bad f line"))?;
                for k in 1..idx.len().saturating_sub(1) {
                    t.extend([idx[0], idx[k], idx[k + 1]]);
                }
            }
            _ => {}
        }
    }
    Ok((v, t))
}

/// glTF binary: the first mesh's primitives (POSITION and indices), node transforms not applied.
pub fn read_glb(path: &str) -> Result<(Vec<f32>, Vec<u32>), String> {
    let b = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if b.len() < 20 || &b[0..4] != b"glTF" {
        return Err(format!("{path}: not a GLB"));
    }
    let u32le = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
    let jl = u32le(12);
    let json: serde_json::Value = serde_json::from_slice(&b[20..20 + jl]).map_err(|e| format!("{path}: {e}"))?;
    let bin_at = 20 + jl;
    let bin = if b.len() > bin_at + 8 { &b[bin_at + 8..bin_at + 8 + u32le(bin_at)] } else { &[][..] };
    let acc = |i: usize| -> Result<(Vec<u8>, usize, u64, usize), String> {
        let a = &json["accessors"][i];
        let view = &json["bufferViews"][a["bufferView"].as_u64().ok_or("accessor without a view")? as usize];
        let off = view["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = a["count"].as_u64().unwrap_or(0) as usize;
        let ct = a["componentType"].as_u64().unwrap_or(0);
        let n = match a["type"].as_str().unwrap_or("") { "VEC3" => 3, "SCALAR" => 1, _ => 0 };
        let size = match ct { 5126 | 5125 => 4, 5123 => 2, 5121 => 1, _ => 4 };
        let stride = view["byteStride"].as_u64().unwrap_or(0) as usize;
        let stride = if stride == 0 { size * n } else { stride };
        let mut out = Vec::with_capacity(count * size * n);
        for k in 0..count {
            let at = off + k * stride;
            out.extend_from_slice(bin.get(at..at + size * n).ok_or("an accessor past the buffer")?);
        }
        Ok((out, count, ct, n))
    };
    let (mut v, mut t) = (Vec::new(), Vec::new());
    for prim in json["meshes"][0]["primitives"].as_array().ok_or_else(|| format!("{path}: no mesh"))? {
        let base = (v.len() / 3) as u32;
        let (p, _, _, _) = acc(prim["attributes"]["POSITION"].as_u64().ok_or("no POSITION")? as usize)?;
        v.extend(p.chunks(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])));
        let (ix, count, ct, _) = acc(prim["indices"].as_u64().ok_or("no indices")? as usize)?;
        for k in 0..count {
            let i = match ct {
                5125 => u32::from_le_bytes([ix[k * 4], ix[k * 4 + 1], ix[k * 4 + 2], ix[k * 4 + 3]]),
                5123 => u16::from_le_bytes([ix[k * 2], ix[k * 2 + 1]]) as u32,
                _ => ix[k] as u32,
            };
            t.push(base + i);
        }
    }
    Ok((v, t))
}

/// A solid written as OBJ (format "obj") or glTF binary ("glb").
pub fn export(m: &Manifold, path: &str, format: &str) -> Result<usize, String> {
    let (v, t) = to_mesh(m);
    let bytes = match format {
        "obj" => {
            let mut s = String::from("# raea solid\n");
            for c in v.chunks(3) {
                s += &format!("v {} {} {}\n", c[0], c[1], c[2]);
            }
            for c in t.chunks(3) {
                s += &format!("f {} {} {}\n", c[0] + 1, c[1] + 1, c[2] + 1);
            }
            s.into_bytes()
        }
        "glb" => {
            let mut bin = Vec::new();
            for x in &v {
                bin.extend_from_slice(&x.to_le_bytes());
            }
            let ilen = bin.len();
            for i in &t {
                bin.extend_from_slice(&i.to_le_bytes());
            }
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for c in v.chunks(3) {
                for k in 0..3 {
                    lo[k] = lo[k].min(c[k]);
                    hi[k] = hi[k].max(c[k]);
                }
            }
            let json = serde_json::json!({
                "asset": {"version": "2.0", "generator": "raea"},
                "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
                "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],
                "buffers": [{"byteLength": bin.len()}],
                "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": ilen, "target": 34962}, {"buffer": 0, "byteOffset": ilen, "byteLength": bin.len() - ilen, "target": 34963}],
                "accessors": [
                    {"bufferView": 0, "componentType": 5126, "count": v.len() / 3, "type": "VEC3", "min": lo, "max": hi},
                    {"bufferView": 1, "componentType": 5125, "count": t.len(), "type": "SCALAR"}
                ]
            });
            let mut js = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
            while js.len() % 4 != 0 {
                js.push(b' ');
            }
            while bin.len() % 4 != 0 {
                bin.push(0);
            }
            let total = 12 + 8 + js.len() + 8 + bin.len();
            let mut out = Vec::with_capacity(total);
            out.extend_from_slice(b"glTF");
            out.extend_from_slice(&2u32.to_le_bytes());
            out.extend_from_slice(&(total as u32).to_le_bytes());
            out.extend_from_slice(&(js.len() as u32).to_le_bytes());
            out.extend_from_slice(b"JSON");
            out.extend_from_slice(&js);
            out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
            out.extend_from_slice(b"BIN\0");
            out.extend_from_slice(&bin);
            out
        }
        f => return Err(format!("export: {f}? (obj, glb)")),
    };
    std::fs::write(path, &bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok(bytes.len())
}

/// Glyph outlines as closed polylines (curves flattened), in the face's units scaled to `size` per em.
#[cfg(feature = "native")]
struct Pen {
    polys: Vec<Vec<Vec2>>,
    cur: Vec<Vec2>,
    at: (f64, f64),
    dx: f64,
    k: f64,
}

#[cfg(feature = "native")]
impl Pen {
    fn pt(&self, x: f32, y: f32) -> (f64, f64) {
        (self.dx + x as f64 * self.k, y as f64 * self.k)
    }
    fn push(&mut self, p: (f64, f64)) {
        self.cur.push(Vec2::new(p.0, p.1));
        self.at = p;
    }
}

#[cfg(feature = "native")]
impl skrifa::outline::OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        let p = self.pt(x, y);
        self.push(p);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.push(p);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (a, c, b) = (self.at, self.pt(cx, cy), self.pt(x, y));
        for i in 1..=8 {
            let t = i as f64 / 8.0;
            let u = 1.0 - t;
            self.push((u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1));
        }
    }
    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let (a, c0, c1, b) = (self.at, self.pt(c0x, c0y), self.pt(c1x, c1y), self.pt(x, y));
        for i in 1..=10 {
            let t = i as f64 / 10.0;
            let u = 1.0 - t;
            let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.push((w0 * a.0 + w1 * c0.0 + w2 * c1.0 + w3 * b.0, w0 * a.1 + w1 * c0.1 + w2 * c1.1 + w3 * b.1));
        }
    }
    fn close(&mut self) {
        if self.cur.len() >= 3 {
            self.polys.push(std::mem::take(&mut self.cur));
        }
        self.cur.clear();
    }
}

#[cfg(feature = "native")]
fn area(p: &[Vec2]) -> f64 {
    (0..p.len()).map(|i| {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        a.x * b.y - b.x * a.y
    }).sum::<f64>() / 2.0
}

/// A word as a solid: the bundled face's outlines (the faces the renderer loads), `size` units an em, `depth` deep,
/// centred on the origin.
#[cfg(feature = "native")]
fn text3d(text: &str, size: f64, depth: f64, family: &str, weight: u16) -> Result<Manifold, String> {
    use skrifa::MetadataProvider;
    use skrifa::raw::TableProvider;
    let bytes = raea::fonts::face_ttf(family, weight, false).or_else(|| raea::fonts::face_ttf("Inter", weight, false)).ok_or("no bundled face for text3d")?;
    let font = skrifa::FontRef::new(&bytes).map_err(|e| format!("text3d's face: {e}"))?;
    let upem = font.head().map(|h| h.units_per_em()).unwrap_or(1000) as f64;
    let (charmap, outlines) = (font.charmap(), font.outline_glyphs());
    let metrics = font.glyph_metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
    let mut pen = Pen { polys: Vec::new(), cur: Vec::new(), at: (0.0, 0.0), dx: 0.0, k: size / upem };
    for ch in text.chars() {
        let Some(gid) = charmap.map(ch) else { continue };
        if let Some(g) = outlines.get(gid) {
            let settings = skrifa::outline::DrawSettings::unhinted(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
            g.draw(settings, &mut pen).map_err(|e| format!("text3d: {e}"))?;
            skrifa::outline::OutlinePen::close(&mut pen);
        }
        pen.dx += metrics.advance_width(gid).unwrap_or(0.0) as f64 * size / upem;
    }
    if pen.polys.is_empty() {
        return Err(format!("text3d({text:?}): nothing to draw"));
    }
    // Manifold fills counter-clockwise contours: turn every glyph so its outer contours run that way (TrueType draws
    // them clockwise, CFF counter-clockwise), and its holes run against them
    let mut polys = pen.polys;
    let biggest = polys.iter().map(|p| area(p)).fold(0.0f64, |m, a| if a.abs() > m.abs() { a } else { m });
    if biggest < 0.0 {
        for p in &mut polys {
            p.reverse();
        }
    }
    let section = CrossSection::new(polys);
    let solid = Manifold::extrude(&section.to_polygons(), depth, 1, 0.0, Vec2::new(1.0, 1.0));
    let b = solid.bounding_box();
    Ok(solid.translate(Vec3::new(-(b.min.x + b.max.x) / 2.0, -(b.min.y + b.max.y) / 2.0, -(b.min.z + b.max.z) / 2.0)))
}

/// Without native the bundled faces (the page engine's) aren't linked, so a word has no outlines to extrude.
#[cfg(not(feature = "native"))]
fn text3d(text: &str, _size: f64, _depth: f64, _family: &str, _weight: u16) -> Result<Manifold, String> {
    Err(format!("text3d({text:?}): the bundled faces are only in the native build"))
}

/// The solid an expression describes.
pub fn solid(expr: &str) -> Result<Manifold, String> {
    let mut p = P { t: lex(expr)?, i: 0 };
    let a = p.expr()?;
    if p.i < p.t.len() {
        return Err(format!("shape: unexpected {:?} after the shape", p.t[p.i]));
    }
    match a {
        Arg::Shape(m) if !m.is_empty() => Ok(m),
        Arg::Shape(_) => Err(format!("shape {expr}: empty (a boolean left nothing)")),
        _ => Err(format!("shape {expr}: not a shape")),
    }
}

/// A clip's mask as a solid in (x, y, frame) over a w x h picture of n frames, built once per expression.
pub fn mask_solid(expr: &str, w: f64, h: f64, n: f64) -> Result<std::sync::Arc<Manifold>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, std::sync::Arc<Manifold>>>> = OnceLock::new();
    let key = format!("{w}x{h}x{n}:{expr}");
    let cache = CACHE.get_or_init(Default::default);
    if let Some(m) = cache.lock().unwrap().get(&key) {
        return Ok(m.clone());
    }
    PICTURE.with(|p| p.set(Some((w, h, n))));
    let r = solid(expr);
    PICTURE.with(|p| p.set(None));
    let m = std::sync::Arc::new(r?);
    cache.lock().unwrap().insert(key, m.clone());
    Ok(m)
}

/// What the mask shows at frame f: its slice through the frame's middle, as coverage (0..255) over w x h pixels.
/// Each pixel row is sampled on four sub-rows; along a row, a span's ends cover their pixels by the fraction they
/// reach (an edge on a pixel boundary is exactly 0 or 255). Contours wind as Manifold's do (non-zero).
pub fn mask_coverage(m: &Manifold, f: i64, w: usize, h: usize) -> Vec<u8> {
    let polys = m.slice(f as f64 + 0.5).to_polygons();
    let mut acc = vec![0f32; w * h];
    let edges: Vec<(f64, f64, f64, f64)> = polys.iter().flat_map(|p| (0..p.len()).map(move |i| (p[i].x, p[i].y, p[(i + 1) % p.len()].x, p[(i + 1) % p.len()].y))).collect();
    let (ymin, ymax) = edges.iter().fold((f64::MAX, f64::MIN), |(lo, hi), e| (lo.min(e.1.min(e.3)), hi.max(e.1.max(e.3))));
    if edges.is_empty() {
        return vec![0; w * h];
    }
    let (r0, r1) = ((ymin.floor().max(0.0)) as usize, (ymax.ceil().min(h as f64)) as usize);
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for row in r0..r1 {
        for sub in 0..4 {
            let sy = row as f64 + (sub as f64 + 0.5) / 4.0;
            xs.clear();
            for &(x0, y0, x1, y1) in &edges {
                if (y0 <= sy && y1 > sy) || (y1 <= sy && y0 > sy) {
                    let x = x0 + (sy - y0) / (y1 - y0) * (x1 - x0);
                    xs.push((x, if y1 > y0 { 1 } else { -1 }));
                }
            }
            xs.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut wind = 0;
            for k in 0..xs.len() {
                let before = wind;
                wind += xs[k].1;
                if before == 0 && wind != 0 {
                    // a span opens at xs[k]; it closes where the winding returns to 0
                    let mut wk = wind;
                    let mut end = w as f64;
                    for &(x, d) in &xs[k + 1..] {
                        wk += d;
                        if wk == 0 {
                            end = x;
                            break;
                        }
                    }
                    let (a, b) = (xs[k].0.clamp(0.0, w as f64), end.clamp(0.0, w as f64));
                    if b <= a {
                        continue;
                    }
                    let line = &mut acc[row * w..(row + 1) * w];
                    let (ia, ib) = (a.floor() as usize, b.floor() as usize);
                    if ia == ib {
                        line[ia.min(w - 1)] += (b - a) as f32 * 0.25;
                    } else {
                        line[ia] += (ia as f64 + 1.0 - a) as f32 * 0.25;
                        for px in &mut line[ia + 1..ib.min(w)] {
                            *px += 0.25;
                        }
                        if ib < w {
                            line[ib] += (b - ib as f64) as f32 * 0.25;
                        }
                    }
                }
            }
        }
    }
    acc.into_iter().map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8).collect()
}

/// The masked clips' coverage at frame `f`, one byte a slot in a word a pixel (kgpu's mask M), marking those slots
/// masked (on = 2). `masks[s]` is slot s's `mask` expression. None when no slot has one.
#[cfg(feature = "native")]
pub fn slot_masks(masks: &[Option<String>], slots: &mut [raea_kgpu::Slot], f: i64, w: usize, h: usize, frames: i64) -> Result<Option<Vec<u32>>, String> {
    if masks.iter().all(Option::is_none) {
        return Ok(None);
    }
    let mut words = vec![0u32; w * h];
    for (s, m) in masks.iter().enumerate().take(4) {
        let Some(expr) = m else { continue };
        let solid = mask_solid(expr, w as f64, h as f64, frames as f64).map_err(|e| format!("mask {expr}: {e}"))?;
        for (wd, c) in words.iter_mut().zip(mask_coverage(&solid, f, w, h)) {
            *wd |= (c as u32) << (8 * s);
        }
        slots[s].on = 2;
    }
    Ok(Some(words))
}

/// A row's mask expression, if it has one.
pub fn mask_of(v: &std::collections::BTreeMap<String, crate::ir::V>) -> Option<String> {
    match v.get("mask") {
        Some(crate::ir::V::S(s)) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// A triangle mesh: positions and normals, three vertices a triangle. Normals are smoothed across edges whose faces
/// meet within 30 degrees (a torus or a cup reads round) and kept apart across sharper ones (a box's edges stay crisp).
#[derive(Debug, Clone)]
pub struct Tris {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
}

pub fn tris(m: &Manifold) -> Tris {
    let gl = m.get_mesh_gl(-1);
    let np = gl.num_prop as usize;
    let v = |i: u32| {
        let o = i as usize * np;
        [gl.vert_properties[o], gl.vert_properties[o + 1], gl.vert_properties[o + 2]]
    };
    let (mut positions, mut normals) = (Vec::new(), Vec::new());
    for t in gl.tri_verts.chunks_exact(3) {
        let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
        let (u, w) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
        let n = [n[0] / l, n[1] / l, n[2] / l];
        positions.extend([a, b, c]);
        normals.extend([n, n, n]);
    }
    // each corner takes the mean of the face normals at its position that lie within 30 degrees of its own face's
    let key = |p: &[f32; 3]| ((p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64, (p[2] * 1e4).round() as i64);
    let mut at: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, p) in positions.iter().enumerate() {
        at.entry(key(p)).or_default().push(i);
    }
    let face = normals.clone();
    let cos = 30f32.to_radians().cos();
    for (i, p) in positions.iter().enumerate() {
        let f = face[i];
        let mut s = [0f32; 3];
        for &j in &at[&key(p)] {
            let g = face[j];
            if f[0] * g[0] + f[1] * g[1] + f[2] * g[2] >= cos {
                s = [s[0] + g[0], s[1] + g[1], s[2] + g[2]];
            }
        }
        let l = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
        if l > 1e-9 {
            normals[i] = [s[0] / l, s[1] / l, s[2] / l];
        }
    }
    Tris { positions, normals }
}

/// The mesh for an expression, built once per expression (a re-cut or a key that doesn't change the shape costs nothing).
pub fn cached(expr: &str) -> Result<std::sync::Arc<Tris>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, std::sync::Arc<Tris>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(t) = cache.lock().unwrap().get(expr) {
        return Ok(t.clone());
    }
    let t = std::sync::Arc::new(tris(&solid(expr)?));
    cache.lock().unwrap().insert(expr.to_string(), t.clone());
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csg_volumes_are_right() {
        let v = |e: &str| solid(e).unwrap().volume();
        assert!((v("box(2, 1, 0.5)") - 1.0).abs() < 1e-9);
        // a box minus a cylinder through it: 8 - pi * 0.5^2 * 2
        let hole = v("minus(box(2, 2, 2), cylinder(0.5, 3))");
        assert!((hole - (8.0 - std::f64::consts::PI * 0.25 * 2.0)).abs() < 0.02, "{hole}");
        // rounding takes the corners off: under the box, over the box minus its edges' worth
        let r = v("rounded_box(2, 1, 0.4, 0.1)");
        assert!(r < 0.8 && r > 0.75, "{r}");
        assert!(solid("move(box(1, 1, 1), 0, 0, 5)").unwrap().bounding_box().min.z > 4.4);
    }

    #[test]
    fn text_is_a_solid_with_holes() {
        let o = solid("text3d(\"O\", 1, 0.2)").unwrap();
        let b = o.bounding_box();
        assert!((b.max.z - b.min.z - 0.2).abs() < 1e-6);
        // an O is a ring: its volume is well under its bounding box's
        let boxed = (b.max.x - b.min.x) * (b.max.y - b.min.y) * 0.2;
        assert!(o.volume() < boxed * 0.6 && o.volume() > boxed * 0.15, "{} of {boxed}", o.volume());
        let sign = solid("minus(rounded_box(2, 1, 0.2, 0.05), text3d(\"SALE\", 0.45, 0.3))").unwrap();
        assert!(sign.volume() < solid("rounded_box(2, 1, 0.2, 0.05)").unwrap().volume());
        assert!(tris(&sign).positions.len() > 300);
    }

    #[test]
    fn masks_in_space_and_time() {
        let (w, h) = (100usize, 50usize);
        // a wipe to the right over frames 10..20: half revealed at frame 14.5's slice (the middle of frame 14)
        let m = mask_solid("wipe(\"right\", 10, 20)", w as f64, h as f64, 30.0).unwrap();
        let cov = |f| mask_coverage(&m, f, w, h);
        assert!(cov(5).iter().all(|c| *c == 0));
        assert!(cov(25).iter().all(|c| *c == 255));
        let mid = cov(14);
        let shown = mid.iter().take(w).map(|c| *c as f64 / 255.0).sum::<f64>();
        assert!((shown - 45.0).abs() < 0.6, "{shown}");
        // a split: the left 40 px, edges exact
        let s = mask_coverage(&mask_solid("rect(0, 0, 40, 50)", w as f64, h as f64, 30.0).unwrap(), 3, w, h);
        assert_eq!((s[39], s[40], s[w * 49 + 39]), (255, 0, 255));
        // a hole moving right by 50 over frames 0..30: its centre is at x 25 + 50 * 15.5 / 30 at frame 15
        let hole = mask_solid("minus(frame(), slide(disc(25, 25, 10), 50, 0, 0, 30))", w as f64, h as f64, 30.0).unwrap();
        let c = mask_coverage(&hole, 15, w, h);
        let cx = 25.0 + 50.0 * 15.5 / 30.0;
        assert_eq!(c[25 * w + cx as usize], 0);
        assert_eq!(c[25 * w + 5], 255);
        assert!(solid("wipe(\"right\", 0, 1)").is_err(), "space-time shapes need a picture");
    }

    #[test]
    fn bad_shapes_say_why() {
        assert!(solid("blob(1)").err().unwrap().contains("no shape blob"));
        assert!(solid("box(").is_err());
        assert!(solid("minus(box(1,1,1), box(2,2,2))").err().unwrap().contains("empty"));
    }
}
