//! 3D shapes as values (docs/media.feature M2, docs/bevy.feature B5c): a `mesh` row's `shape` column is a CSG
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
//!   sphere(r, n) / uv_sphere(r, sectors, stacks)   Bevy's icosphere of n subdivisions, or its UV sphere
//!   sdf("field" [, step])  tree("k=v ..." [, part])  loft("y: outline; ..." [, n])  terrain("k=v ...")
//!   boulder("k=v ...")  rosette("k=v ...")   blended fields and generated growth (see grown.rs)
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


mod buffer;
mod build;
mod glyphs;
mod grown;
mod kept;
mod mesh;
mod parse;
mod sphere;
pub use sphere::{SphereOf, cuboid_of, cylinder_of, plane_of, sphere_of};

pub use buffer::{buffer_tris, export_tris};
pub use mesh::{export, from_mesh, read_glb, read_obj, to_mesh};

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub use manifold_rust::manifold::Manifold;

use crate::build::PICTURE;
use crate::parse::{Arg, P, lex};

const SEGMENTS: i32 = 48;

/// The solid an expression describes.
pub fn solid(expr: &str) -> Result<Manifold, String> {
    match solid_or_empty(expr)? {
        m if !m.is_empty() => Ok(m),
        _ => Err(format!("shape {expr}: empty (a boolean left nothing)")),
    }
}

/// The solid for an expression, an empty one too: a script that holds solids by their expressions (raea-runtime-calls'
/// `/3d` manifold) asks whether a boolean left nothing, which a mesh to draw can't be.
pub fn solid_or_empty(expr: &str) -> Result<Manifold, String> {
    let mut p = P { t: lex(expr)?, i: 0 };
    let a = p.expr()?;
    if p.i < p.t.len() {
        return Err(format!("shape: unexpected {:?} after the shape", p.t[p.i]));
    }
    match a {
        Arg::Shape(m) => Ok(m),
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

/// A shape made faster than this isn't kept on disk: a box is made in microseconds, and a part whose size changed each
/// frame wrote a file a frame (2.4 million in 12 hours of long renders, 11-21); the disk is for the cuts that cost
const KEEP_MS: f64 = 2.0;
/// The bytes of triangles one generation of held shapes takes: past it, the generation before is let go and this one
/// takes its place. A shape asked for again moves to the newest, so what a scene keeps asking for stays, and two
/// generations at most are held. A long render's shapes that change each frame (a bar that grows, 3D words that count)
/// are let go as it goes: held to 1 GB at once, a one-scene control grew by 100 MB in 4,000 frames (SG4a). Anything
/// let go is made again, or read from disk when it took KEEP_MS to make: a part asks for its shape when it's spawned,
/// and the end checks once more.
const GEN_BYTES: usize = 16 << 20;

/// The shapes held, the newest generation and the one before, and the newest's bytes.
#[derive(Default)]
struct Held {
    now: HashMap<String, std::sync::Arc<Tris>>,
    old: HashMap<String, std::sync::Arc<Tris>>,
    bytes: usize,
}

impl Held {
    fn get(&mut self, expr: &str) -> Option<std::sync::Arc<Tris>> {
        if let Some(t) = self.now.get(expr) {
            return Some(t.clone());
        }
        let t = self.old.remove(expr)?;
        self.hold(expr, &t);
        Some(t)
    }

    /// `t` held for `expr` in the newest generation, which becomes the one before when it would pass GEN_BYTES.
    fn hold(&mut self, expr: &str, t: &std::sync::Arc<Tris>) {
        let bytes = (t.positions.len() + t.normals.len()) * 12 + expr.len();
        if self.bytes + bytes > GEN_BYTES && !self.now.is_empty() {
            self.old = std::mem::take(&mut self.now);
            self.bytes = 0;
        }
        self.bytes += bytes;
        self.now.insert(expr.to_string(), t.clone());
    }
}

/// The mesh for an expression, built once per expression (a re-cut or a key that doesn't change the shape costs nothing).
pub fn cached(expr: &str) -> Result<std::sync::Arc<Tris>, String> {
    static CACHE: OnceLock<Mutex<Held>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(t) = cache.lock().unwrap().get(expr) {
        return Ok(t);
    }
    // a buffer mesh's own triangles (buffer.rs): data already, so nothing is kept on disk for it
    if let Some(b) = expr.strip_prefix("buffer:") {
        let t = std::sync::Arc::new(buffer_tris(b)?.0);
        cache.lock().unwrap().hold(expr, &t);
        return Ok(t);
    }
    // made by this build before, on disk (kept.rs): read back as it was written
    let kept = kept::place(expr);
    let t = match kept.as_deref().and_then(kept::read) {
        Some(t) => std::sync::Arc::new(t),
        None => {
            let made = std::time::Instant::now();
            let t = std::sync::Arc::new(tris(&solid(expr)?));
            if let Some(p) = kept.as_ref().filter(|_| made.elapsed().as_secs_f64() * 1e3 >= KEEP_MS) {
                kept::write(p, &t);
            }
            t
        }
    };
    cache.lock().unwrap().hold(expr, &t);
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_shapes_go_a_generation_at_a_time_and_what_is_asked_again_stays() {
        let mut h = Held::default();
        // a shape of a quarter of a generation
        let t = std::sync::Arc::new(Tris { positions: vec![[0.0; 3]; GEN_BYTES / 96], normals: vec![[0.0; 3]; GEN_BYTES / 96] });
        h.hold("kept", &t);
        for i in 0..12 {
            h.hold(&format!("bar {i}"), &t);
            assert!(h.get("kept").is_some(), "asked each time, it stays (after bar {i})");
        }
        assert!(h.get("bar 0").is_none(), "made once and never asked again, it went");
        assert!(h.now.len() + h.old.len() <= 8, "two generations at most: {} + {}", h.now.len(), h.old.len());
    }

    #[test]
    fn a_kept_mesh_reads_back_as_it_was_written() {
        let dir = std::env::temp_dir().join(format!("raea-shape-kept-{}", std::process::id()));
        let p = dir.join("a.tris");
        let t = tris(&solid("minus(box(2, 2, 2), cylinder(0.5, 3))").unwrap());
        assert!(kept::read(&p).is_none());
        kept::write(&p, &t);
        let back = kept::read(&p).unwrap();
        assert_eq!(back.positions.iter().flatten().map(|x| x.to_bits()).collect::<Vec<_>>(), t.positions.iter().flatten().map(|x| x.to_bits()).collect::<Vec<_>>());
        assert_eq!(back.normals.iter().flatten().map(|x| x.to_bits()).collect::<Vec<_>>(), t.normals.iter().flatten().map(|x| x.to_bits()).collect::<Vec<_>>());
        // a file cut short isn't a mesh
        let whole = std::fs::read(&p).unwrap();
        std::fs::write(&p, &whole[..whole.len() - 5]).unwrap();
        assert!(kept::read(&p).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_mesh_file_written_again_is_kept_under_a_new_key() {
        let dir = std::env::temp_dir().join(format!("raea-shape-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("scan.obj");
        std::fs::write(&p, "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nf 1 3 2\nf 1 2 4\nf 1 4 3\nf 2 3 4\n").unwrap();
        let expr = format!("minus(obj({:?}), box(0.1, 0.1, 0.1))", p.to_string_lossy());
        let first = kept::files_read(&expr);
        assert_eq!(first.len(), 24, "one file read: its length and modified time");
        // the same file written again with other content (a larger mesh) reads as another file
        std::fs::write(&p, "v 0 0 0\nv 2 0 0\nv 0 2 0\nv 0 0 2\nf 1 3 2\nf 1 2 4\nf 1 4 3\nf 2 3 4\n# remeshed\n").unwrap();
        assert_ne!(kept::files_read(&expr), first);
        // an expression that reads no file adds nothing to its key
        assert!(kept::files_read("box(1, 1, 1)").is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
