//! The frame's camera as the checks read it: where it is, where it looks, and a point's place on screen.

use bevy::math::{Mat4, Quat, Vec3, Vec4};
use raea_core_scene::{Item, vec3};
use serde_json::Value;

/// The camera a frame draws through (its first PerspectiveCamera that draws), in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub eye: Vec3,
    /// where it looks and its up, unit vectors
    pub fwd: Vec3,
    pub up: Vec3,
    /// pixels a metre at one metre's depth (the vertical focal length in px)
    pub focal: f32,
    pub w: f32,
    pub h: f32,
    view: Mat4,
}

impl View {
    /// The frame's camera, None when it has none. Camera3d gives its rotation as a quaternion; a three.js-style camera
    /// gives where it looks (lookAt). Its parent's transform is the item's matrix.
    pub fn of(its: &[Item], w: u32, h: u32) -> Option<View> {
        let c = its.iter().find(|i| i.kind == "camera" && i.props.get("active").and_then(Value::as_bool) != Some(false))?;
        let (_, parent_rot, _) = c.world.to_scale_rotation_translation();
        let p = vec3(&c.props, "position", [0.0, 0.0, 5.0]);
        let eye = c.world.transform_point3(Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32));
        let q = c.props.get("rotation").and_then(Value::as_array).filter(|q| q.len() == 4).map(|q| {
            let n = |i: usize| q[i].as_f64().unwrap_or(0.0) as f32;
            Quat::from_xyzw(n(0), n(1), n(2), n(3)).normalize()
        });
        let (fwd, up) = match q {
            Some(q) => (parent_rot * (q * Vec3::NEG_Z), parent_rot * (q * Vec3::Y)),
            None => {
                let l = vec3(&c.props, "lookAt", [0.0, 0.0, 0.0]);
                let at = c.world.transform_point3(Vec3::new(l[0] as f32, l[1] as f32, l[2] as f32));
                ((at - eye).normalize_or(Vec3::NEG_Z), parent_rot * Vec3::Y)
            }
        };
        let fov = c.props.get("fov").and_then(Value::as_f64).unwrap_or(50.0).clamp(1.0, 179.0) as f32;
        let focal = (h as f32 / 2.0) / (fov.to_radians() / 2.0).tan();
        Some(View::new(eye, fwd, up, focal, w as f32, h as f32))
    }

    pub fn new(eye: Vec3, fwd: Vec3, up: Vec3, focal: f32, w: f32, h: f32) -> View {
        let f = fwd.normalize_or(Vec3::NEG_Z);
        let s = f.cross(up).normalize_or(Vec3::X);
        let u = s.cross(f);
        let view = Mat4::from_cols(Vec4::new(s.x, u.x, -f.x, 0.0), Vec4::new(s.y, u.y, -f.y, 0.0), Vec4::new(s.z, u.z, -f.z, 0.0), Vec4::new(-eye.dot(s), -eye.dot(u), eye.dot(f), 1.0));
        View { eye, fwd: f, up: u, focal, w, h, view }
    }

    /// A point on screen: x and y in px from the frame's top left, and its depth in front of the camera; None behind it.
    pub fn project(&self, p: Vec3) -> Option<[f32; 3]> {
        let v = self.view.transform_point3(p);
        let depth = -v.z;
        (depth > 1e-3).then(|| [self.w / 2.0 + v.x * self.focal / depth, self.h / 2.0 - v.y * self.focal / depth, depth])
    }

    /// The box on screen of world points (x0, y0, x1, y1 in px), None when any is behind the camera.
    pub fn bounds(&self, pts: impl IntoIterator<Item = Vec3>) -> Option<[f32; 4]> {
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in pts {
            let [x, y, _] = self.project(p)?;
            b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
        }
        (b[0] <= b[2]).then_some(b)
    }

    /// The share of the frame a box on screen covers, the part inside the frame only.
    pub fn share(&self, b: [f32; 4]) -> f32 {
        let (x0, y0, x1, y1) = (b[0].max(0.0), b[1].max(0.0), b[2].min(self.w), b[3].min(self.h));
        if x1 <= x0 || y1 <= y0 { 0.0 } else { (x1 - x0) * (y1 - y0) / (self.w * self.h) }
    }

    /// The angle between two views' directions, in degrees.
    pub fn turn(&self, o: &View) -> f32 {
        // atan2, not acos: near zero acos of an f32 dot reads 0 one frame and 0.02 degrees the next
        self.fwd.cross(o.fwd).length().atan2(self.fwd.dot(o.fwd)).to_degrees()
    }
}

/// The eight corners of a box in a part's own space, carried to the world by its matrix.
pub fn corners(world: &Mat4, lo: Vec3, hi: Vec3) -> [Vec3; 8] {
    let mut out = [Vec3::ZERO; 8];
    for (k, c) in out.iter_mut().enumerate() {
        let local = Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z });
        *c = world.transform_point3(local);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    pub(crate) fn camera(props: Value) -> Item {
        Item { kind: "camera", props: Arc::new(props.as_object().unwrap().clone()), shape: None, at: None, material: None, world: Mat4::IDENTITY }
    }

    #[test]
    fn a_point_in_front_lands_where_the_lens_puts_it() {
        let v = View::of(&[camera(json!({"position": [0, 0, 5], "lookAt": [0, 0, 0], "fov": 90}))], 1920, 1080).unwrap();
        assert_eq!(v.project(Vec3::ZERO), Some([960.0, 540.0, 5.0]));
        // at 90 degrees the frame's top edge is as far up as the point is deep
        let [_, y, _] = v.project(Vec3::new(0.0, 5.0, 0.0)).unwrap();
        assert!(y.abs() < 1e-3, "{y}");
        assert_eq!(v.project(Vec3::new(0.0, 0.0, 6.0)), None, "behind the camera");
        // Camera3d's quaternion: turned a quarter about y, it looks down -x
        let q = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let v = View::of(&[camera(json!({"position": [0, 0, 0], "rotation": [q.x, q.y, q.z, q.w], "fov": 50}))], 100, 100).unwrap();
        assert!(v.fwd.abs_diff_eq(Vec3::NEG_X, 1e-5), "{:?}", v.fwd);
        assert!(v.project(Vec3::new(-3.0, 0.0, 0.0)).is_some());
    }
}
