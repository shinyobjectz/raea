//! The fixed cameras of `raea render EDIT.tsx --review DIR` (runtime/export's review.rs renders them), and the camera a
//! review's child render puts in place of the scene's own (RAEA_CAMERA). The cameras come from the frame's parts (scene
//! lint's boxes; ground slabs and other flat, wide parts left out): hero (the scene's own camera), front (+z looking
//! -z), right, top, iso, and low (0.3 m off the floor, for feet and floaters); an interior (a ceiling over the plan)
//! gets the same views from inside, under the ceiling.

use std::sync::{Arc, OnceLock};

use bevy::math::Mat4;
use raea_core_scene::Item;
use serde_json::{Map, Value, json};

pub const VIEWS: [&str; 6] = ["hero", "front", "right", "top", "iso", "low"];
pub const TILE_W: u32 = 640;

/// A fixed camera: (view, eye, target, fov degrees).
pub type Cam = (&'static str, [f64; 3], [f64; 3], f64);
/// A part's box: (min, max).
pub type Box3 = ([f64; 3], [f64; 3]);

/// The fixed cameras, each (view, eye, target, fov degrees), from the parts' box and the picture's aspect.
pub fn cameras(lo: [f64; 3], hi: [f64; 3], aspect: f64) -> Vec<Cam> {
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let r = (((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() / 2.0).max(0.05);
    let fov: f64 = 40.0;
    let half_v = (fov / 2.0).to_radians();
    let half_h = (half_v.tan() * aspect).atan();
    let d = r / half_v.min(half_h).sin() * 1.08;
    let k = 1.0 / 3f64.sqrt();
    let low_y = 0.3f64.max(lo[1] + 0.05).min(hi[1]);
    vec![
        ("front", [c[0], c[1] + 0.15 * r, c[2] + d], c, fov),
        ("right", [c[0] + d, c[1] + 0.15 * r, c[2]], c, fov),
        ("top", [c[0], c[1] + d, c[2] + 0.001 * d], c, fov),
        ("iso", [c[0] + d * k, c[1] + d * k, c[2] + d * k], c, fov),
        ("low", [c[0], low_y, c[2] + d], [c[0], low_y, c[2]], fov),
    ]
}

/// An interior's cameras: inside the room, under its ceiling (`ceil`, the ceiling's underside), each looking across it,
/// since from outside an enclosed room every view is its walls and roof.
pub fn inside(lo: [f64; 3], hi: [f64; 3], ceil: f64) -> Vec<Cam> {
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    // well in from the box's sides, which can run past a room's walls (columns or a porch outside it)
    let m = |k: usize| ((hi[k] - lo[k]) * 0.12).clamp(0.5, 6.0);
    let top = ceil.min(hi[1]);
    let eye = (lo[1] + 1.6).min(top - 0.2).max(lo[1] + 0.3);
    // a corner view from about a tall man's reach, under a dropped ceiling too
    let up = (lo[1] + ((top - lo[1]) * 0.8).min(3.0)).max(eye);
    let fov = 70.0;
    vec![
        ("front", [c[0], eye, hi[2] - m(2)], [c[0], eye - 0.3, lo[2]], fov),
        ("right", [hi[0] - m(0), eye, c[2]], [lo[0], eye - 0.3, c[2]], fov),
        ("top", [c[0], top - 0.1, c[2] + 0.001], [c[0], lo[1], c[2]], 90.0),
        ("iso", [hi[0] - m(0), up, hi[2] - m(2)], [c[0], lo[1] + 0.5, c[2]], fov),
        ("low", [c[0], (lo[1] + 0.3).min(top), hi[2] - m(2)], [c[0], lo[1] + 0.3, lo[2]], fov),
    ]
}

/// The underside of a ceiling over the parts' box, when there is one (a part over most of the plan in the box's top
/// third): the scene is then an interior, seen from inside.
pub fn ceiling(boxes: &[Box3], lo: [f64; 3], hi: [f64; 3]) -> Option<f64> {
    let (w, d, h) = (hi[0] - lo[0], hi[2] - lo[2], hi[1] - lo[1]);
    boxes.iter().filter(|(a, b)| b[0] - a[0] >= 0.8 * w && b[2] - a[2] >= 0.8 * d && a[1] > lo[1] + h * 0.66 && h > 1.5).map(|(a, _)| a[1]).reduce(f64::min)
}

/// The box round every part (slabs and ceilings too).
pub fn whole(boxes: &[Box3]) -> Option<Box3> {
    boxes.iter().copied().reduce(|(a, b), (c, d)| ([a[0].min(c[0]), a[1].min(c[1]), a[2].min(c[2])], [b[0].max(d[0]), b[1].max(d[1]), b[2].max(d[2])]))
}

/// Every part's box from scene lint's findings.
pub fn boxes_of(found: &[Value]) -> Vec<Box3> {
    found.iter().filter_map(|v| {
        let b = v.get("box")?;
        let p = |k: &str| -> Option<[f64; 3]> { let a = b.get(k)?.as_array()?; Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?]) };
        Some((p("min")?, p("max")?))
    }).filter(|(lo, hi)| (0..3).all(|k| hi[k].is_finite() && lo[k].is_finite() && hi[k] >= lo[k])).collect()
}

/// The parts' box from scene lint's findings, ground slabs left out when anything else stands: a flat part wider
/// than 5 m, or a thin one (under a twentieth of its span) lying at the bottom of the scene, the floor a few props
/// stand on. A raised slab (a table top, a shelf) stays.
pub fn parts_box(found: &[Value]) -> Option<Box3> {
    let boxes = boxes_of(found);
    let bottom = boxes.iter().map(|(lo, _)| lo[1]).fold(f64::INFINITY, f64::min);
    let flat_wide = |(lo, hi): &([f64; 3], [f64; 3])| {
        let (h, span) = (hi[1] - lo[1], (hi[0] - lo[0]).max(hi[2] - lo[2]));
        h < 0.25 && (span > 5.0 || (h < 0.05 * span && hi[1] <= bottom + 0.05))
    };
    let standing: Vec<Box3> = boxes.iter().copied().filter(|b| !flat_wide(b)).collect();
    whole(if standing.is_empty() { &boxes } else { &standing })
}

/// The arguments a child render keeps: everything but this render's own output, frame and review flags.
pub fn passed(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--review" | "--frame" | "--at" => i += 2,
            "--clay" => i += 1,
            a => {
                out.push(a.to_string());
                i += 1;
            }
        }
    }
    out
}

/// RAEA_CAMERA="ex,ey,ez,tx,ty,tz[,fov°]", the camera a review's child render uses.
fn wanted() -> Option<&'static ([f64; 3], [f64; 3], Option<f64>)> {
    static CAM: OnceLock<Option<([f64; 3], [f64; 3], Option<f64>)>> = OnceLock::new();
    CAM.get_or_init(|| {
        let v: Vec<f64> = std::env::var("RAEA_CAMERA").ok()?.split(',').map(|s| s.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
        (v.len() >= 6).then(|| ([v[0], v[1], v[2]], [v[3], v[4], v[5]], v.get(6).copied()))
    })
    .as_ref()
}

/// RAEA_LENS="mm": the scene's own camera kept where it is, with that focal length's field of view on a full-frame
/// sensor (a lens chart, docs/make.feature MK15).
fn lens() -> Option<f64> {
    static LENS: OnceLock<Option<f64>> = OnceLock::new();
    *LENS.get_or_init(|| std::env::var("RAEA_LENS").ok()?.trim().parse::<f64>().ok().filter(|mm| *mm > 0.0).map(fov_of_lens))
}

/// The vertical field of view (degrees) of a focal length (mm) on a 36 x 24 mm sensor.
pub fn fov_of_lens(mm: f64) -> f64 {
    2.0 * (12.0 / mm).atan().to_degrees()
}

/// A review's fixed camera in place of the scene's own: every frame's camera item is moved there, or one is added when
/// the scene has none; or, with RAEA_LENS, the scene's own camera given that lens. With both unset, nothing changes.
pub fn camera_override(out: &mut Vec<Item>) {
    if let Some((eye, at, fov)) = wanted() {
        place_camera(out, *eye, *at, *fov);
    } else if let Some(fov) = lens()
        && let Some(c) = out.iter_mut().find(|i| i.kind == "camera")
    {
        Arc::make_mut(&mut c.props).insert("fov".into(), json!(fov));
    }
}

/// The frame's camera put at `eye` looking at `at`. A Camera3d's Transform rotation goes: the engine places a camera
/// by its rotation over its look-at point, so a review camera keeping the scene's would face the scene camera's way.
fn place_camera(out: &mut Vec<Item>, eye: [f64; 3], at: [f64; 3], fov: Option<f64>) {
    let set = |props: &mut Map<String, Value>| {
        props.insert("position".into(), json!(eye));
        props.insert("lookAt".into(), json!(at));
        props.remove("rotation");
        if let Some(f) = fov {
            props.insert("fov".into(), json!(f));
        }
    };
    match out.iter_mut().find(|i| i.kind == "camera") {
        Some(c) => set(Arc::make_mut(&mut c.props)),
        None => {
            let mut props = Map::new();
            set(&mut props);
            out.push(Item { kind: "camera", props: Arc::new(props), shape: None, material: None, at: None, world: Mat4::IDENTITY });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cameras_frame_the_box_and_the_low_one_is_at_the_floor() {
        let cams = cameras([-1.0, 0.0, -1.0], [1.0, 2.0, 1.0], 16.0 / 9.0);
        assert_eq!(cams.iter().map(|c| c.0).collect::<Vec<_>>(), ["front", "right", "top", "iso", "low"]);
        let (_, eye, at, fov) = cams[0];
        // the box's bounding sphere fits the vertical field of view
        let d = ((eye[0] - at[0]).powi(2) + (eye[1] - at[1]).powi(2) + (eye[2] - at[2]).powi(2)).sqrt();
        // the box's half diagonal: sqrt(2^2 + 2^2 + 2^2) / 2
        let r = 3f64.sqrt();
        assert!(d * (fov / 2.0).to_radians().sin() >= r * 0.99, "{d} {r}");
        assert_eq!(cams[4].1[1], 0.3);
        // a 50 mm lens on full frame: about 27 degrees vertical (MK15's lens chart)
        assert!((fov_of_lens(50.0) - 26.99).abs() < 0.01);
        assert_eq!(passed(&["render", "e.tsx", "-o", "x", "--review", "d", "--clay", "--composition", "A"].map(String::from)), vec!["--composition", "A"]);
        let found = vec![json!({"box": {"min": [-50, -0.1, -50], "max": [50, 0, 50]}}), json!({"box": {"min": [0, 0, 0], "max": [1, 1, 1]}})];
        assert_eq!(parts_box(&found), Some(([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])));
        // a small floor under a few props (a kit's lineup) is a ground slab too; a raised table top isn't
        let kit = vec![json!({"box": {"min": [-0.6, -0.01, -0.6], "max": [0.6, 0, 0.6]}}), json!({"box": {"min": [-0.2, 0, -0.05], "max": [0.2, 0.14, 0.05]}})];
        assert_eq!(parts_box(&kit), Some(([-0.2, 0.0, -0.05], [0.2, 0.14, 0.05])));
        let table = vec![json!({"box": {"min": [-0.8, 0.72, -0.4], "max": [0.8, 0.75, 0.4]}}), json!({"box": {"min": [-0.7, 0, -0.3], "max": [-0.65, 0.72, -0.25]}})];
        assert_eq!(parts_box(&table), Some(([-0.8, 0.0, -0.4], [0.8, 0.75, 0.4])));
        // a room with a ceiling is seen from inside: every camera within its walls and under its ceiling
        let room = vec![json!({"box": {"min": [-5, 0, -5], "max": [5, 0.1, 5]}}), json!({"box": {"min": [-5, 2.9, -5], "max": [5, 3, 5]}}), json!({"box": {"min": [0, 0, 0], "max": [1, 1, 1]}})];
        let (lo, hi) = whole(&boxes_of(&room)).unwrap();
        let ceil = ceiling(&boxes_of(&room), lo, hi).expect("a ceiling");
        assert_eq!(ceil, 2.9);
        for (_, eye, _, _) in inside(lo, hi, ceil) {
            assert!((0..3).all(|k| eye[k] > lo[k] && eye[k] < hi[k]) && eye[1] < ceil, "{eye:?}");
        }
        assert!(ceiling(&boxes_of(&found), [0.0; 3], [1.0; 3]).is_none());
    }

    #[test]
    fn a_review_camera_drops_the_scene_cameras_rotation_and_one_is_added_when_there_is_none() {
        let props = json!({"position": [0, 2, 6], "lookAt": [0, 1, 0], "rotation": [0.0, 0.0, 0.0, 1.0]}).as_object().cloned().unwrap();
        let mut its = vec![Item { kind: "camera", props: Arc::new(props), shape: None, material: None, at: None, world: Mat4::IDENTITY }];
        place_camera(&mut its, [6.0, 1.7, 0.0], [0.5, 1.5, 0.0], Some(40.0));
        let p = &its[0].props;
        assert_eq!((p["position"].clone(), p["lookAt"].clone(), p["fov"].clone(), p.get("rotation")), (json!([6.0, 1.7, 0.0]), json!([0.5, 1.5, 0.0]), json!(40.0), None));
        let mut none = Vec::new();
        place_camera(&mut none, [0.0, 7.0, 0.0], [0.0; 3], None);
        assert_eq!((none.len(), none[0].kind), (1, "camera"));
    }
}
