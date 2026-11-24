//! The camera row: where it is and looks (keyed), its lens and motion blur.

use super::*;

/// The camera row's 3D columns: where it is and looks (keyed), its lens, the ambient light, the post-processing.
/// A frame without depth of field: an f-stop so large no circle of confusion reaches a pixel
pub const NO_DOF: f64 = 1.0e6;

/// A live frame's camera row (runtime/scene's LiveScene) with its <Environment>'s panoramas as every one shown so far:
/// a frame names only its own, so the row lists those `sent` last, then this frame's if it's new, with env_i pointing
/// into the list, as a render's row lists them all at load; the engine bakes one it hasn't seen when the row brings it
/// (raea-sys-environment's live_rows, #38).
pub fn env_shown(sent: Option<&V>, cam: &mut BTreeMap<String, V>) {
    let Some(V::S(now)) = cam.get("env_srcs").cloned() else { return };
    let mut list: Vec<String> = match sent {
        Some(V::S(s)) => serde_json::from_str(s).unwrap_or_default(),
        _ => Vec::new(),
    };
    let mine: Vec<String> = serde_json::from_str(&now).unwrap_or_default();
    let i = cam.get("env_i").map(V::num).unwrap_or(-1.0);
    if let Some(src) = mine.get(i.max(0.0) as usize).filter(|_| i > -0.5) {
        let at = list.iter().position(|s| s == src).unwrap_or_else(|| {
            list.push(src.clone());
            list.len() - 1
        });
        cam.insert("env_i".into(), V::N(at as f64));
    }
    cam.insert("env_srcs".into(), V::S(serde_json::to_string(&list).unwrap_or_default()));
}

/// <Environment portals>' openings, each four corners in order round it, as a corner and its two neighbours
fn portal_corners(list: &[Value]) -> Vec<[[f64; 3]; 3]> {
    let point = |p: &Value| -> Option<[f64; 3]> {
        let a = p.as_array()?;
        Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
    };
    list.iter()
        .filter_map(|q| match q.as_array()?.iter().filter_map(point).collect::<Vec<_>>()[..] {
            [a, b, _, d] => Some([a, b, d]),
            _ => None,
        })
        .collect()
}

/// An <Environment>'s source in env_srcs: its panorama's path, or for <Environment room> `room:` and its blur (three's
/// RoomEnvironment, which raea-sys-environment's room.rs makes, #165).
pub fn env_src(e: &Item) -> Option<String> {
    if e.props.get("room") == Some(&Value::Bool(true)) {
        return Some(format!("room:{}", n(&e.props, "blur", 0.0)));
    }
    e.props.get("src").and_then(Value::as_str).map(src_path)
}

/// The camera row (`row`, id "cam": the scene's first camera) and a row for each camera after it (cam1, cam2, ...), each
/// drawn in its `order` over the ones before, cleared or not by its `clear` (<Camera3d order clear_color>, Bevy's
/// Camera: two_passes, split_screen), clearing to the first's colour when it clears.
pub fn camera(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>) -> Vec<BTreeMap<String, V>> {
    one(all, p, row, 0, "cam");
    let n = all.iter().map(|its| its.iter().filter(|i| i.kind == "camera").count()).max().unwrap_or(0);
    (1..n)
        .map(|k| {
            let id = format!("cam{k}");
            let mut r: BTreeMap<String, V> = row.get("bg").map(|b| ("bg".to_string(), b.clone())).into_iter().collect();
            r.insert("id".into(), V::S(id.clone()));
            one(all, p, &mut r, k, &id);
            r
        })
        .collect()
}

/// The `k`th camera's row: where it is and looks frame by frame, its lens, the scene's ambient light and looks.
fn one(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>, k: usize, id: &str) {
    let cams: Vec<Option<Item>> = all.iter().map(|its| its.iter().filter(|i| i.kind == "camera").nth(k).cloned()).collect();
    if cams.iter().any(Option::is_some) {
        let its = filled(&cams);
        // which of the scene's cameras is the piece's, where that's not the first: a cut where it changes (active.rs)
        if k == 0 {
            shot_column(p, row, id, &its);
        }
        placed(all, p, row, id, &its);
    } else {
        for (name, v) in [("x", 0.0), ("y", 0.0), ("z", 5.0), ("look_x", 0.0), ("look_y", 0.0), ("look_z", 0.0), ("fov", 50.0)] {
            row.insert(name.into(), V::N(v));
        }
    }
    lit(all, p, row, id);
}

/// A camera's place, lens and frame, from its items frame by frame (`its`, filled): where it is and looks, its field
/// of view or orthographic height, up, rotation, view window and focus, keyed; its order, clear, colour, viewport, near
/// and far, layers and Bevy components as its first frame has them. A screen camera's (one) and an image's (rendercam.rs).
pub fn placed(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>, id: &str, its: &[Item]) {
    let pos: Vec<[f64; 3]> = its.iter().map(|c| vec3(&c.props, "position", [0.0, 0.0, 5.0])).collect();
    let look: Vec<[f64; 3]> = its.iter().map(|c| vec3(&c.props, "lookAt", [0.0, 0.0, 0.0])).collect();
    for (k, name) in ["x", "y", "z"].iter().enumerate() {
        column(p, row, id, name, &pos.iter().map(|v| v[k]).collect::<Vec<_>>());
    }
    for (k, name) in ["look_x", "look_y", "look_z"].iter().enumerate() {
        column(p, row, id, name, &look.iter().map(|v| v[k]).collect::<Vec<_>>());
    }
    column(p, row, id, "fov", &its.iter().map(|c| n(&c.props, "fov", 50.0)).collect::<Vec<_>>());
    // an orthographic camera (<OrthographicCamera height>): the view's height in metres, keyed; 0 is perspective
    if its.iter().any(|c| c.props.contains_key("ortho")) {
        column(p, row, id, "ortho", &its.iter().map(|c| n(&c.props, "ortho", 0.0)).collect::<Vec<_>>());
    }
    // which way is up (`up`; +Y by default): a camera that rolls, as a solved one does (raea-render's up_x/y/z, Matchmove)
    keyed_vec(p, row, id, &its, "up", &["up_x", "up_y", "up_z"], &[0.0, 1.0, 0.0]);
    // a Camera3d's Transform rotation (`rotation`, a quaternion on every frame), exact, which the engine places it by over
    // its look-at point
    if its.iter().all(|c| c.props.get("rotation").and_then(Value::as_array).is_some_and(|q| q.len() == 4)) {
        quat_columns(p, row, id, "rot", &its.iter().map(Some).collect::<Vec<_>>());
    }
    // a window on a larger view (`view`: its full size and this frame's offset, in frame pixels): a crop of
    // footage keeps its lens's centre (raea-render's camera view_*, Matchmove's 9:16 from 16:9)
    if its.iter().any(|c| c.props.get("view").is_some_and(Value::is_object)) {
        let view = |c: &Item, k: &str| c.props.get("view").and_then(|v| v.get(k)).and_then(Value::as_f64).unwrap_or(0.0);
        for (k, col) in [("width", "view_w"), ("height", "view_h"), ("x", "view_x"), ("y", "view_y")] {
            column(p, row, id, col, &its.iter().map(|c| view(c, k)).collect::<Vec<_>>());
        }
    }
    if all.iter().flatten().any(|i| i.kind == "dof") {
        column(p, row, id, "focus", &its.iter().map(focus_of).collect::<Vec<_>>());
    }
    let first = &its[0].props;
    if let Some(o) = num(first, "order") {
        row.insert("order".into(), V::N(o));
    }
    if let Some(c) = first.get("clear").and_then(Value::as_str) {
        row.insert("clear".into(), V::S(c.into()));
    }
    if let Some(b) = first.get("bg").and_then(Value::as_str) {
        row.insert("bg".into(), V::S(b.into()));
    }
    // its viewport (Bevy's Viewport: physical px from the target's top left)
    let vp = |k: &str, i: usize| first.get("viewport").and_then(|v| v.get(k)).and_then(|v| v.get(i)).and_then(Value::as_f64);
    for (col, k, i) in [("vp_x", "physical_position", 0), ("vp_y", "physical_position", 1), ("vp_w", "physical_size", 0), ("vp_h", "physical_size", 1)] {
        if let Some(x) = vp(k, i) {
            row.insert(col.into(), V::N(x));
        }
    }
    for (k, col) in [("near", "near"), ("far", "far")] {
        if let Some(v) = num(first, k) {
            row.insert(col.into(), V::N(v));
        }
    }
    layers_column(row, first);
    // Bevy's own components on the camera (<BevyComponent> inside it)
    components(p, id, its);
}

/// The rest of a screen camera's row: the scene's ambient light, environment, sky, post-processing, composition
/// passes, lens and motion blur, frame by frame.
fn lit(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>, id: &str) {
    // ambient: the AmbientLights on the camera's layers (an image's pass lights its own scene with its own): rendercam.rs
    let layers: Vec<u64> = match row.get("layers") {
        Some(V::S(l)) => l.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
        _ => vec![0],
    };
    ambient_columns(all, p, row, id, &layers);
    // an <Environment>: each frame's panorama (an index into env_srcs, -1 for none), its light, turn and sky, keyed, so
    // a cut from a hall to a street changes the light on that frame
    // Bevy's EnvironmentMapLight with its own prefiltered maps (<EnvironmentMapLight>: its diffuse and specular
    // cubemap files, as the first frame has them), in place of a panorama raea filters
    let maps: Vec<Option<&Item>> = all.iter().map(|its| its.iter().find(|i| i.kind == "environment" && i.props.contains_key("diffuse_map"))).collect();
    if let Some(m) = maps.iter().flatten().next() {
        for k in ["diffuse_map", "specular_map"] {
            row.insert(format!("envmap_{k}"), V::S(m.props.get(k).and_then(Value::as_str).unwrap_or_default().to_string()));
        }
        row.insert("envmap_k".into(), V::N(n(&m.props, "intensity", 1.0)));
        // its rotation (a Quat), keyed: rotate_environment_map turns it every frame
        quat_columns(p, row, id, "envmap", &maps);
    }
    // a <Skybox>: Bevy's cubemap sky on the camera, its image as the first frame has it, its brightness and rotation keyed
    if let Some(s) = all.iter().flatten().find(|i| i.kind == "skybox") {
        row.insert("skybox_image".into(), V::S(s.props.get("image").and_then(Value::as_str).unwrap_or_default().to_string()));
        let skies: Vec<Option<&Item>> = all.iter().map(|its| its.iter().find(|i| i.kind == "skybox")).collect();
        column(p, row, id, "skybox_k", &skies.iter().map(|s| s.map(|i| n(&i.props, "brightness", 0.0)).unwrap_or(0.0)).collect::<Vec<_>>());
        quat_columns(p, row, id, "skybox", &skies);
    }
    let envs: Vec<Option<&Item>> = all.iter().map(|its| its.iter().find(|i| i.kind == "environment" && !i.props.contains_key("diffuse_map"))).collect();
    if envs.iter().any(Option::is_some) {
        let mut srcs: Vec<String> = Vec::new();
        let idx: Vec<f64> = envs.iter().map(|e| match e.and_then(|e| env_src(e)) {
            Some(s) => (srcs.iter().position(|x| *x == s).unwrap_or_else(|| { srcs.push(s); srcs.len() - 1 })) as f64,
            None => -1.0,
        }).collect();
        row.insert("env_srcs".into(), V::S(serde_json::to_string(&srcs).unwrap_or_default()));
        column(p, row, id, "env_i", &idx);
        let k = |e: &Option<&Item>| e.map(|e| n(&e.props, "intensity", 1.0)).unwrap_or(0.0);
        column(p, row, id, "env_k", &envs.iter().map(|e| k(e) * ENV).collect::<Vec<_>>());
        column(p, row, id, "env_rot", &envs.iter().map(|e| e.map(|e| n(&e.props, "rotation", 0.0)).unwrap_or(0.0)).collect::<Vec<_>>());
        let sky: Vec<f64> = envs.iter().map(|e| match e {
            Some(e) if e.props.get("background") == Some(&Value::Bool(true)) => num(&e.props, "backgroundIntensity").unwrap_or(k(&Some(e))) * ENV,
            _ => 0.0,
        }).collect();
        if sky.iter().any(|s| *s > 0.0) {
            column(p, row, id, "sky_k", &sky);
        }
        // its sky portals (<Environment portals>), the first frame's: each a corner and its two neighbours
        if let Some(list) = envs.iter().flatten().find_map(|e| e.props.get("portals")).and_then(Value::as_array) {
            row.insert("env_portals".into(), V::S(serde_json::to_string(&portal_corners(list)).unwrap_or_default()));
        }
    }
    // a <Grade>'s colour fringes, keyed: each frame's own (0 where a frame has none), so a glitch's spike draws on its
    // frames and a frame is the same whichever others the render holds (it was the first frame's, for every frame)
    let ab: Vec<f64> = all.iter().map(|its| its.iter().find(|i| i.kind == "grade").and_then(|g| num(&g.props, "aberration")).unwrap_or(0.0)).collect();
    if all.iter().flatten().any(|i| i.kind == "grade" && num(&i.props, "aberration").is_some()) {
        column(p, row, id, "aberration", &ab);
    }
    // post-processing: each frame's own (post.rs, docs/rerender.feature RR1)
    post(all, p, row, id);
    if all.iter().flatten().any(|i| i.kind == "dof") {
        // keyed: a frame without a <DepthOfField> draws sharp (an f-stop past any blur), not the shot's first one; its
        // size and mode are each frame's post-processing (post.rs), which puts it on and takes it off
        let ap: Vec<f64> = all.iter().map(|its| its.iter().find(|i| i.kind == "dof").map_or(NO_DOF, |i| n(&i.props, "aperture", 2.8))).collect();
        column(p, row, "cam", "dof_aperture", &ap);
    }
    frame_looks(all, p, row);
    // the composition passes the parts name (engine/compose.rs)
    let passes = pass_names(all);
    if !passes.is_empty() {
        row.insert("passes".into(), V::S(passes.join(",")));
    }
    lens(all, p, row);
    motion_blur(all, p, row);
}

/// The camera's lens (Fisheye or LensDistortion; engine/render/src/engine/lens.rs): its kind and edges from the first frame that
/// has one, every value keyed per frame. A frame without it is no lens (strength 0, no k, zoom 1), so a lens inside a
/// Sequence bends only its frames. A barrel's zoom, when not given, is worked out here (enough to keep its corners
/// inside the picture), so it's keyed like the rest.
pub fn lens(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>) {
    let per: Vec<Option<&Item>> = all.iter().map(|its| its.iter().find(|i| i.kind == "fisheye" || i.kind == "distortion")).collect();
    let Some(first) = per.iter().flatten().next() else { return };
    let id = "cam";
    let fisheye = first.kind == "fisheye";
    let mode = if !fisheye { 3.0 } else if first.props.get("mode").and_then(Value::as_str) == Some("stereographic") { 2.0 } else { 1.0 };
    row.insert("lens_mode".into(), V::N(mode));
    if first.props.get("edges").and_then(Value::as_str) == Some("black") {
        row.insert("lens_black".into(), V::N(1.0));
    }
    let same = |i: &&Item| (i.kind == "fisheye") == fisheye;
    let at = |k: &str, d: f64| -> Vec<f64> { per.iter().map(|i| i.filter(same).map(|i| n(&i.props, k, d)).unwrap_or(0.0)).collect() };
    if fisheye {
        column(p, row, id, "lens_s", &at("strength", 1.0));
        if per.iter().flatten().any(|i| i.props.contains_key("fov")) {
            let cams: Vec<f64> = all.iter().map(|its| its.iter().find(|i| i.kind == "camera").map(|c| n(&c.props, "fov", 50.0)).unwrap_or(50.0)).collect();
            let fovs: Vec<f64> = per.iter().zip(&cams).map(|(i, c)| i.filter(same).and_then(|i| num(&i.props, "fov")).unwrap_or(*c)).collect();
            column(p, row, id, "lens_fov", &fovs);
        }
    } else {
        column(p, row, id, "lens_k1", &at("k1", 0.0));
        column(p, row, id, "lens_k2", &at("k2", 0.0));
    }
    let zoom: Vec<f64> = per.iter().map(|i| match i.filter(same) {
        Some(i) => num(&i.props, "zoom").unwrap_or(if fisheye { 1.0 } else { (1.0 + n(&i.props, "k1", 0.0) + n(&i.props, "k2", 0.0)).max(1.0) }),
        None => 1.0,
    }).collect();
    column(p, row, id, "lens_zoom", &zoom);
}

/// The camera's motion blur (engine/render/src/engine/motion_blur.rs): mb_on keyed per frame (0 where a frame has none), its
/// shutter keyed (a frame without it holds the nearest frame's, which an off frame doesn't draw), its samples the first
/// frame's.
pub fn motion_blur(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>) {
    let per: Vec<Option<Item>> = all.iter().map(|its| its.iter().find(|i| i.kind == "mblur").cloned()).collect();
    if !per.iter().any(Option::is_some) {
        return;
    }
    let id = "cam";
    column(p, row, id, "mb_on", &per.iter().map(|i| i.is_some() as u8 as f64).collect::<Vec<_>>());
    let its = filled(&per);
    column(p, row, id, "mb_shutter", &its.iter().map(|i| n(&i.props, "shutterAngle", 0.5)).collect::<Vec<_>>());
    if let Some(s) = num(&its[0].props, "samples") {
        row.insert("mb_samples".into(), V::N(s));
    }
    if its[0].props.get("kind").and_then(Value::as_str) == Some("bevy") {
        row.insert("mb_bevy".into(), V::N(1.0));
    }
}

/// a part's `rotation` as glam's Quat [x, y, z, w] (Bevy's identity when it has none)
fn quat(props: &serde_json::Map<String, Value>) -> [f64; 4] {
    match props.get("rotation").and_then(Value::as_array) {
        Some(q) if q.len() == 4 => [0, 1, 2, 3].map(|k| q[k].as_f64().unwrap_or(0.0)),
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

/// each frame's part's `rotation` (the identity on a frame without one) as four exact columns, `<name>_qx` to
/// `<name>_qw` (keyed when it turns)
fn quat_columns(p: &mut Program, row: &mut BTreeMap<String, V>, id: &str, name: &str, parts: &[Option<&Item>]) {
    let qs: Vec<[f64; 4]> = parts.iter().map(|m| m.map_or([0.0, 0.0, 0.0, 1.0], |i| quat(&i.props))).collect();
    for (k, axis) in ["qx", "qy", "qz", "qw"].iter().enumerate() {
        exact(p, row, id, &format!("{name}_{axis}"), &qs.iter().map(|q| q[k]).collect::<Vec<_>>());
    }
}

/// A `#rgb`, `#rrggbb` or `#rrggbbaa` colour as sRGB 0..1, alpha last (a holder's background as the camera's clear,
/// keyed bg_r..bg_a)
pub fn rgba_of(c: &str) -> Option<[f64; 4]> {
    let h = c.strip_prefix('#')?;
    let h: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    if h.len() != 6 && h.len() != 8 {
        return None;
    }
    let at = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok().map(|v| v as f64 / 255.0);
    Some([at(0)?, at(2)?, at(4)?, if h.len() == 8 { at(6)? } else { 1.0 }])
}

/// A part's composition pass on each frame (`pass`: 0 "scene", from 1 the piece's passes, pass_names), stepped.
pub fn pass_column(p: &mut Program, row: &mut BTreeMap<String, V>, id: &str, its: &[Item], names: &[String]) {
    if names.is_empty() || !its.iter().any(|i| i.props.contains_key("pass")) {
        return;
    }
    let ids: Vec<f64> = its.iter().map(|i| i.props.get("pass").and_then(Value::as_str).and_then(|n| names.iter().position(|x| x == n)).map_or(0.0, |k| (k + 1) as f64)).collect();
    row.insert("pass".into(), V::N(ids[0]));
    if !ids.iter().all(|v| *v == ids[0]) {
        p.keys.insert((id.to_string(), "pass".to_string()), ids.iter().enumerate().map(|(f, v)| (f as i64, *v, "step".to_string())).collect());
    }
}

/// The distance the camera focuses at: its `focus`, else how far its lookAt point is.
pub fn focus_of(c: &Item) -> f64 {
    num(&c.props, "focus").unwrap_or_else(|| {
        let (p, l) = (vec3(&c.props, "position", [0.0, 0.0, 5.0]), vec3(&c.props, "lookAt", [0.0, 0.0, 0.0]));
        ((p[0] - l[0]).powi(2) + (p[1] - l[1]).powi(2) + (p[2] - l[2]).powi(2)).sqrt().max(0.01)
    })
}
