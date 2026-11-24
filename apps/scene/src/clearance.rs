//! Camera clearance (docs/make.feature MK16, from AnyShot's clearance pass): every frame, how near the camera passes to
//! each solid part, and where it comes closer than a person carrying it could (0.45 m by default). A camera that
//! grazes a door frame or pushes into a desk draws the inside of the part, or a wall filling the frame; the boxes the
//! stage keeps say it without drawing. Only solids count: volumes, decals, footage, 2D, see-through parts (glass) and
//! anything under 5 cm across are passed over. A part's box answers first; a model whose box comes close is measured
//! on its triangles (a room model round the camera, a tree's box that's mostly air).

use std::collections::HashMap;

use bevy::math::{Mat4, Vec3};
use raea_core_facts::{Finding, Level};
use raea_core_scene::Item;
use serde_json::Value;

use raea_sys_stage::{View, called, model_box, own_box};

/// Parts smaller than this across (metres) are passed over: a pen the camera brushes isn't a collision.
const SMALL: f32 = 0.05;
/// Clearance the shot language asks of a carried camera, metres.
pub const DEFAULT: f32 = 0.45;

fn see_through(it: &Item) -> bool {
    let props = it.material.as_ref().map(|m| &m.props);
    let n = |k: &str| props.and_then(|p| p.get(k)).or(it.props.get(k)).and_then(Value::as_f64);
    n("specular_transmission").or(n("transmission")).is_some_and(|t| t > 0.3) || n("opacity").is_some_and(|o| o < 0.5)
}

/// The nearest distance from `p` (world) to a box [lo, hi] in a part's own space under `world`, in world metres.
pub fn to_box(world: &Mat4, lo: Vec3, hi: Vec3, p: Vec3) -> f32 {
    let local = world.inverse().transform_point3(p);
    let near = local.clamp(lo, hi);
    world.transform_point3(near).distance(p)
}

/// The nearest distance from `p` to a triangle.
pub fn to_tri(p: Vec3, t: &[Vec3; 3]) -> f32 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return p.distance(a);
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return p.distance(b);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return p.distance(a + ab * (d1 / (d1 - d3)));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return p.distance(c);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return p.distance(a + ac * (d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return p.distance(b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    p.distance(a + ab * (vb * denom) + ac * (vc * denom))
}

/// A frame's nearest solid parts to its camera: (part, distance) for each closer than `min`.
pub fn near_parts(its: &[Item], eye: Vec3, min: f32, boxes: &mut HashMap<String, Option<(Vec3, Vec3)>>, tris: &mut HashMap<String, Option<Vec<[Vec3; 3]>>>) -> Vec<(String, f32)> {
    let mut out = Vec::new();
    for it in its {
        if !matches!(it.kind, "mesh" | "model") || it.at.is_some() || see_through(it) {
            continue;
        }
        let bx = if it.kind == "mesh" { own_box(it, boxes) } else { model_box(it, boxes) };
        let Some((lo, hi)) = bx else { continue };
        let ext = (hi - lo) * it.world.to_scale_rotation_translation().0.abs();
        if ext.max_element() < SMALL {
            continue;
        }
        let mut d = to_box(&it.world, lo, hi, eye);
        if d >= min {
            continue;
        }
        if it.kind == "model" {
            let Some(src) = it.props.get("src").and_then(Value::as_str).map(raea_core_scene::src_path) else { continue };
            let t = tris.entry(src.clone()).or_insert_with(|| raea_core_scene::gltf_tris(&src).ok().map(|(g, _)| g.into_iter().flat_map(|(_, t)| t).collect()));
            if let Some(t) = t {
                let local = it.world.inverse().transform_point3(eye);
                // nearest in the model's own space, then measured in the world's (a scaled model's metres)
                if let Some(best) = t.iter().min_by(|a, b| to_tri(local, a).total_cmp(&to_tri(local, b))) {
                    let near = nearest_on(local, best);
                    d = it.world.transform_point3(near).distance(eye);
                }
            }
        }
        if d < min {
            out.push((called(it), d));
        }
    }
    out
}

/// The point on a triangle nearest `p` (by projecting and clamping through `to_tri`'s regions, sampled finely enough
/// to measure a clearance to the millimetre).
fn nearest_on(p: Vec3, t: &[Vec3; 3]) -> Vec3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let n = (b - a).cross(c - a);
    let l2 = n.length_squared();
    if l2 > 1e-14 {
        let q = p - n * ((p - a).dot(n) / l2);
        // barycentric: inside, the projection is the nearest point
        let (v0, v1, v2) = (b - a, c - a, q - a);
        let (d00, d01, d11, d20, d21) = (v0.dot(v0), v0.dot(v1), v1.dot(v1), v2.dot(v0), v2.dot(v1));
        let den = d00 * d11 - d01 * d01;
        if den.abs() > 1e-14 {
            let v = (d11 * d20 - d01 * d21) / den;
            let w = (d00 * d21 - d01 * d20) / den;
            if v >= 0.0 && w >= 0.0 && v + w <= 1.0 {
                return q;
            }
        }
    }
    // outside: the nearest point of the three edges
    let seg = |x: Vec3, y: Vec3| {
        let d = y - x;
        x + d * ((p - x).dot(d) / d.length_squared().max(1e-14)).clamp(0.0, 1.0)
    };
    [seg(a, b), seg(b, c), seg(c, a)].into_iter().min_by(|u, v| u.distance(p).total_cmp(&v.distance(p))).unwrap_or(a)
}

/// Every frame's camera held `min` metres off every solid part: a finding for each part and run of frames it comes
/// closer, with the nearest it gets.
pub fn clearance(all: &[Vec<Item>], frames: &[i64], w: u32, h: u32, fps: f64, min: f32, by: &str) -> Vec<Finding> {
    let (mut boxes, mut tris) = (HashMap::new(), HashMap::new());
    // part -> runs of (first frame index, last, nearest)
    let mut runs: Vec<(String, usize, usize, f32)> = Vec::new();
    let mut open: HashMap<String, usize> = HashMap::new();
    for (k, its) in all.iter().enumerate() {
        let Some(v) = View::of(its, w, h) else { continue };
        let near = near_parts(its, v.eye, min, &mut boxes, &mut tris);
        let names: Vec<&String> = near.iter().map(|n| &n.0).collect();
        open.retain(|name, _| names.contains(&name));
        for (name, d) in near {
            match open.get(&name).copied() {
                Some(r) if runs[r].2 + 1 == k && frames[k] == frames[runs[r].2] + 1 => {
                    runs[r].2 = k;
                    runs[r].3 = runs[r].3.min(d);
                }
                _ => {
                    runs.push((name.clone(), k, k, d));
                    open.insert(name, runs.len() - 1);
                }
            }
        }
    }
    runs.into_iter()
        .map(|(name, a, b, d)| {
            let (fa, fb) = (frames[a], frames[b]);
            let when = if a == b { format!("at frame {fa} ({:.2} s)", fa as f64 / fps) } else { format!("over frames {fa}-{fb} ({:.2}-{:.2} s)", fa as f64 / fps, fb as f64 / fps) };
            let says = if d < 1e-3 {
                format!("the camera is inside `{name}` {when}: move the path out of it, or the part")
            } else {
                format!("the camera passes {d:.2} m from `{name}` {when}, under the {min:.2} m a carried camera keeps: move the path or the part, or pass a smaller --clearance for a camera on a rig")
            };
            Finding::new("camera-clearance", Level::Error, says, by).frames(fa, fb + 1)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::Arc;

    fn item(kind: &'static str, props: Value, world: Mat4) -> Item {
        Item { kind, props: Arc::new(props.as_object().cloned().unwrap_or_default()), shape: None, at: None, material: None, world }
    }

    /// A camera at `eye` looking at `at`, 50 degrees.
    fn cam(eye: [f32; 3], at: [f32; 3]) -> Item {
        item("camera", json!({"position": eye, "lookAt": at, "fov": 50}), Mat4::IDENTITY)
    }

    #[test]
    fn distances_to_a_box_and_a_triangle() {
        let w = Mat4::from_translation(Vec3::new(0.0, 0.5, 0.0));
        let (lo, hi) = (Vec3::splat(-0.5), Vec3::splat(0.5));
        assert!((to_box(&w, lo, hi, Vec3::new(0.0, 0.5, 1.5)) - 1.0).abs() < 1e-5);
        assert_eq!(to_box(&w, lo, hi, Vec3::new(0.0, 0.5, 0.0)), 0.0);
        let t = [Vec3::ZERO, Vec3::X, Vec3::Z];
        assert!((to_tri(Vec3::new(0.25, 0.3, 0.25), &t) - 0.3).abs() < 1e-5);
        assert!((to_tri(Vec3::new(-1.0, 0.0, 0.0), &t) - 1.0).abs() < 1e-5);
        assert!((nearest_on(Vec3::new(0.25, 0.3, 0.25), &t) - Vec3::new(0.25, 0.0, 0.25)).length() < 1e-5);
    }

    fn solid(name: &str, size: [f32; 3], at: [f32; 3]) -> Item {
        let mut it = item("mesh", json!({"name": name}), Mat4::from_translation(Vec3::from(at)));
        it.shape = Some(format!("box({}, {}, {})", size[0], size[1], size[2]).into());
        it
    }

    #[test]
    fn a_camera_pushing_into_a_desk_is_found_and_a_small_or_glass_part_is_not() {
        // a desk 1.6 x 0.75 x 0.8 m standing at the origin; the camera pushes in from 2 m to 0.1 m off its front face
        let desk = solid("desk", [1.6, 0.75, 0.8], [0.0, 0.375, 0.0]);
        let pen = solid("pen", [0.04, 0.01, 0.01], [0.0, 1.0, 0.45]);
        let mut glass = solid("pane", [1.0, 1.0, 0.01], [0.0, 1.0, 0.3]);
        glass.props = Arc::new(json!({"name": "pane", "specular_transmission": 0.9}).as_object().cloned().unwrap());
        let zs = [2.4, 1.6, 1.0, 0.7, 0.5, 2.4];
        let all: Vec<Vec<Item>> = zs.iter().map(|z| vec![cam([0.0, 0.6, *z], [0.0, 0.4, 0.0]), desk.clone(), pen.clone(), glass.clone()]).collect();
        let frames: Vec<i64> = (0..zs.len() as i64).collect();
        let f = clearance(&all, &frames, 640, 360, 30.0, DEFAULT, "t");
        // the desk's face is at z 0.4: frames 3 and 4 come within 0.3 and 0.1 m; one run, its nearest said
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].says.contains("0.10 m from `desk` over frames 3-4"), "{}", f[0].says);
        assert_eq!(f[0].level, Level::Error);
        // a rig's camera may come closer
        assert!(clearance(&all, &frames, 640, 360, 30.0, 0.05, "t").is_empty());
        // inside it, said so
        let inside = vec![vec![cam([0.0, 0.3, 0.0], [0.0, 0.3, -1.0]), desk.clone()]];
        assert!(clearance(&inside, &[0], 640, 360, 30.0, DEFAULT, "t")[0].says.contains("inside `desk`"));
    }
}
