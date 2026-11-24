//! The Blender steps a scene's models call for (docs/blender.feature BL10, BL16): a model in a format raea can't load
//! (an error: it draws nothing), and one carrying more triangles than its distance from the camera can show (a
//! warning: 150 000 at a metre or closer, falling with the square of the distance, never under 2 000). Each names its
//! fix, `raea blender run blender/X.py ...`, in its words and in `fix`.

use raea_core_meshcheck::blender;
use raea_core_scene::{Item, src_path};
use serde_json::{Value, json};

use raea_core_modelbox::file_tris;
use super::seen::camera;

/// The frame's models measured against the camera: (findings, the error count).
pub fn assets(its: &[Item]) -> (Vec<Value>, usize) {
    let eye = camera(its).map(|c| c.1);
    let (mut out, mut errors) = (Vec::new(), 0);
    let mut said = std::collections::HashSet::new();
    for it in its.iter().filter(|i| i.kind == "model") {
        let Some(src) = it.props.get("src").and_then(Value::as_str) else { continue };
        if let Some(mut f) = blender::unloadable(src) {
            if said.insert(src.to_string()) {
                f["part"] = json!(src);
                out.push(f);
                errors += 1;
            }
            continue;
        }
        let (Some(eye), Ok(tris)) = (eye, file_tris(&src_path(src))) else { continue };
        let n = tris.len() / 3;
        // the nearest point of its box, placed, to the eye
        let (mut lo, mut hi) = (bevy::math::Vec3::splat(f32::MAX), bevy::math::Vec3::splat(f32::MIN));
        for q in tris.iter() {
            let w = it.world.transform_point3(*q);
            lo = lo.min(w);
            hi = hi.max(w);
        }
        if n == 0 {
            continue;
        }
        let seen = (eye.clamp(lo, hi) - eye).length() as f64;
        let budget = blender::budget(seen);
        if n > budget && said.insert(format!("{src}@tris")) {
            let k = (budget / 1000).max(1);
            let f = blender::fix("decimate.py", &format!("{src} {} {budget}", blender::beside(src, &format!("{k}k"))));
            out.push(json!({"part": src, "check": "triangles", "level": "warning", "says": blender::says(&format!("{src} carries {n} triangles, {seen:.1} m from the camera: over the {budget} it can show there"), &f), "fix": f, "seen": (seen * 100.0).round() / 100.0}));
        }
    }
    (out, errors)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bevy::math::Mat4;
    use raea_core_scene::Item;
    use serde_json::json;

    fn item(kind: &'static str, props: serde_json::Value) -> Item {
        let Some(m) = props.as_object().cloned() else { unreachable!() };
        Item { kind, props: Arc::new(m), shape: None, material: None, at: None, world: Mat4::IDENTITY }
    }

    #[test]
    fn a_model_raea_cant_load_names_convert() {
        let its = vec![item("camera", json!({"position": [0, 1, 4]})), item("model", json!({"src": "media/src/chair.fbx"})), item("model", json!({"src": "media/src/chair.fbx"}))];
        let (found, errors) = super::assets(&its);
        assert_eq!(errors, 1, "{found:?}");
        assert_eq!(found[0]["fix"]["run"], "raea blender run blender/convert.py media/src/chair.fbx media/src/chair.glb");
    }
}
