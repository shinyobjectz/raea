//! The camera's post-processing frame by frame (docs/rerender.feature RR1): the sky, bloom, fog, the grade and
//! vignette, AO and anti-aliasing, shadow quality, volumetric fog, tone mapping and exposure, each frame's as its own
//! parts ask, where every frame had the first frame's that asked for each. So a piece whose shots each have a look of
//! their own renders as one composition, and a frame is the same whichever others the render holds.

use super::*;

/// The camera row's post-processing (`sets`). A frame with no 3D parts holds the set of the 3D frame before it (the
/// first 3D frame's, before any), so a title card doesn't take the effects off the camera and put them back.
pub fn post(all: &[Vec<Item>], p: &mut Program, row: &mut BTreeMap<String, V>, id: &str) {
    let mut held: Option<BTreeMap<String, V>> = all.iter().find(|its| !its.is_empty()).map(|its| frame_post(its));
    let per: Vec<BTreeMap<String, V>> = all
        .iter()
        .map(|its| {
            if !its.is_empty() {
                held = Some(frame_post(its));
            }
            held.clone().unwrap_or_default()
        })
        .collect();
    sets(per, p, row, id);
}

/// A light's colour, shadows and shafts (a volumetric light), frame by frame (`sets`): the night shot's lamps glow in
/// the air and cast shadows, the day's don't, and each shot's sun has its own colour.
pub fn light_sets(its: &[Item], p: &mut Program, row: &mut BTreeMap<String, V>, id: &str) {
    let per = its
        .iter()
        .map(|i| {
            let mut set = BTreeMap::new();
            if let Some(c) = hex(i.props.get("color")).filter(|c| c != "#ffffff") {
                set.insert("color".into(), V::S(c));
            }
            if let Some(Value::Bool(s)) = i.props.get("castShadow") {
                set.insert("shadows".into(), V::N(*s as u8 as f64));
            }
            if i.props.get("volumetric") == Some(&Value::Bool(true)) {
                set.insert("volumetric".into(), V::N(1.0));
            }
            set
        })
        .collect();
    sets(per, p, row, id);
}

/// A row's columns that change as a set, one set a frame (RR1). One set for the whole piece: its columns on the row, as
/// always. Sets that change (a sky in one shot, fog in the next, a grade that ramps): each different one in
/// `frame_sets` (a JSON list of objects) and the frame's index into it in `frame_set_i` (keyed, stepped), which
/// raea-render lays onto the row each frame (raea-sys-frame tables.rs) for the systems that follow it (raea-sys-post's
/// sync for the camera, sync_light_sets for a light).
pub fn sets(per: Vec<BTreeMap<String, V>>, p: &mut Program, row: &mut BTreeMap<String, V>, id: &str) {
    let mut list: Vec<BTreeMap<String, V>> = Vec::new();
    let idx: Vec<f64> = per
        .into_iter()
        .map(|s| {
            (list.iter().position(|x| *x == s).unwrap_or_else(|| {
                list.push(s);
                list.len() - 1
            })) as f64
        })
        .collect();
    if list.len() <= 1 {
        row.extend(list.pop().unwrap_or_default());
        return;
    }
    let json: Vec<Value> = list.iter().map(|s| Value::Object(s.iter().map(|(c, v)| (c.clone(), json_of(v))).collect())).collect();
    row.insert("frame_sets".into(), V::S(Value::Array(json).to_string()));
    row.insert("frame_set_i".into(), V::N(idx[0]));
    p.keys.insert((id.to_string(), "frame_set_i".to_string()), step_keys(&idx));
}

/// Live play's sets (raea-runtime-scene live.rs, a frame at a time): the row's columns `set` names taken off it and put
/// back as a one-set table, so the next frame's set replaces this one whole. A live row keeps every column it was last
/// sent, so a sky, fog or light colour a later frame drops would otherwise stay on (RR1: render = play).
pub fn as_table(row: &mut BTreeMap<String, V>, set: impl Fn(&str) -> bool) {
    let cols: BTreeMap<String, V> = row.iter().filter(|(k, _)| set(k)).map(|(k, v)| (k.clone(), v.clone())).collect();
    row.retain(|k, _| !set(k));
    let json = Value::Array(vec![Value::Object(cols.iter().map(|(c, v)| (c.clone(), json_of(v))).collect())]);
    row.insert("frame_sets".into(), V::S(json.to_string()));
    row.insert("frame_set_i".into(), V::N(0.0));
}

/// A light row's columns that change as a set: its colour, shadows and shafts (light_sets) and its <Shadows>
/// (FrameShadows::light).
pub fn light_set_col(c: &str) -> bool {
    matches!(c, "color" | "shadows" | "volumetric") || c.starts_with("shadow_")
}

/// One value a frame as stepped keys: a key only where it changes (each held until the next key's own frame).
pub fn step_keys(vals: &[f64]) -> Vec<(i64, f64, String)> {
    vals.iter().enumerate().filter(|(f, v)| *f == 0 || **v != vals[f - 1]).map(|(f, v)| (f as i64, *v, "step".to_string())).collect()
}

fn json_of(v: &V) -> Value {
    match v {
        V::N(x) => serde_json::json!(x),
        V::S(s) => Value::String(s.clone()),
    }
}

/// One frame's post-processing columns, from its own parts (the first of each kind).
pub fn frame_post(its: &[Item]) -> BTreeMap<String, V> {
    let mut row = BTreeMap::new();
    let first = |kind: &str| its.iter().find(|i| i.kind == kind).map(|i| &*i.props);
    // depth of field's kind, on the frames that have one (its aperture and focus are keyed columns: camera.rs), so a shot
    // without it draws no blur pass, and each shot's has its own size and mode, not the first one's
    if let Some(d) = first("dof") {
        row.insert("dof_max".into(), V::N(n(d, "maxBlur", 64.0)));
        if d.get("mode").and_then(Value::as_str) == Some("gaussian") {
            row.insert("dof_mode".into(), V::S("gaussian".into()));
        }
    }
    // a procedural sky (<Atmosphere>): Earth's air, lighting the scene too when `light`
    if let Some(a) = first("atmosphere") {
        row.insert("atmosphere".into(), V::N(1.0));
        if a.get("light") == Some(&Value::Bool(true)) {
            row.insert("atmo_light".into(), V::N(1.0));
        }
        if let Some(x) = num(a, "groundAlbedo") {
            row.insert("atmo_albedo".into(), V::N(x));
        }
    }
    if let Some(b) = first("bloom") {
        row.insert("bloom".into(), V::N(r6(n(b, "intensity", 1.0) * BLOOM)));
        if let Some(t) = num(b, "threshold") {
            row.insert("bloom_threshold".into(), V::N(t));
        }
        if let Some(s) = shares(b) {
            row.insert("bloom_passes".into(), V::S(s));
        }
        // three.js's and postprocessing's bloom: the glow added on top, not traded for the light around it
        if b.get("additive") == Some(&Value::Bool(true)) {
            row.insert("bloom_additive".into(), V::N(1.0));
        }
    }
    if let Some(f) = first("fog") {
        row.insert("fog_color".into(), V::S(hex(f.get("color")).unwrap_or("#808080".into())));
        match num(f, "density") {
            Some(d) => {
                row.insert("fog_density".into(), V::N(d));
            }
            None => {
                row.insert("fog_start".into(), V::N(n(f, "near", 5.0)));
                row.insert("fog_end".into(), V::N(n(f, "far", 20.0)));
            }
        }
    }
    if let Some(g) = first("grade") {
        grade(g, &mut row);
    }
    // ambient occlusion (no multisampling with it: SMAA unless asked otherwise) and the anti-aliasing
    let ssao = first("ssao");
    if let Some(a) = ssao {
        row.insert("ssao".into(), V::S(a.get("quality").and_then(Value::as_str).unwrap_or("medium").to_string()));
        if let Some(t) = num(a, "thickness") {
            row.insert("ssao_thickness".into(), V::N(t));
        }
    }
    let aa = first("aa").and_then(|a| a.get("mode")).and_then(Value::as_str);
    // steady's rates (systems/post steady.rs)
    if let Some(s) = first("aa").and_then(|a| a.get("steady")).and_then(Value::as_object) {
        for (prop, col) in [("still", "aa_steady_still"), ("moving", "aa_steady_moving"), ("px", "aa_steady_px")] {
            if let Some(x) = num(s, prop) {
                row.insert(col.into(), V::N(x));
            }
        }
    }
    match (aa, ssao.is_some()) {
        (Some("msaa") | None, true) => {
            row.insert("aa".into(), V::S("smaa".into()));
        }
        (Some(m), _) if m != "msaa" => {
            row.insert("aa".into(), V::S(m.to_string()));
        }
        _ => {}
    }
    // shadow quality: the map size and the filter on the camera; the cascades and biases go on each light's row
    if let Some(sh) = first("shadows") {
        if let Some(m) = num(sh, "mapSize") {
            row.insert("shadow_map".into(), V::N(m));
        }
        if let Some(f) = sh.get("filter").and_then(Value::as_str) {
            row.insert("shadow_filter".into(), V::S(f.to_string()));
        }
    }
    if let Some(v) = first("volfog") {
        volfog(v, &mut row);
    }
    if let Some(t) = first("tonemapping") {
        if let Some(m) = t.get("mode").and_then(Value::as_str) {
            row.insert("tonemap".into(), V::S(m.to_lowercase()));
        }
        if let Some(e) = num(t, "exposure") {
            row.insert("exposure".into(), V::N(r6(EV100 - e)));
        }
    }
    row
}

/// A <Grade>'s columns: its passes, its colour grading by section and its vignette.
fn grade(g: &Map<String, Value>, row: &mut BTreeMap<String, V>) {
    if let Some(s) = shares(g) {
        row.insert("grade_passes".into(), V::S(s));
    }
    for (k, col) in [("temperature", "grade_temperature"), ("tint", "grade_tint"), ("saturation", "grade_saturation"), ("exposure", "grade_exposure")] {
        if let Some(x) = num(g, k) {
            row.insert(col.into(), V::N(x));
        }
    }
    for (sec, pre) in [("shadows", "grade_s_"), ("midtones", "grade_m_"), ("highlights", "grade_h_")] {
        let Some(o) = g.get(sec).and_then(Value::as_object) else { continue };
        for k in ["saturation", "contrast", "gamma", "gain", "lift"] {
            if let Some(x) = o.get(k).and_then(Value::as_f64) {
                row.insert(format!("{pre}{k}"), V::N(x));
            }
        }
    }
    match g.get("vignette") {
        Some(Value::Number(x)) => {
            row.insert("vignette".into(), V::N(x.as_f64().unwrap_or(0.0)));
        }
        Some(Value::Object(o)) => {
            row.insert("vignette".into(), V::N(o.get("intensity").and_then(Value::as_f64).unwrap_or(1.0)));
            for (k, col) in [("radius", "vignette_radius"), ("smoothness", "vignette_smooth")] {
                if let Some(x) = o.get(k).and_then(Value::as_f64) {
                    row.insert(col.into(), V::N(x));
                }
            }
            if let Some(c) = hex(o.get("color")) {
                row.insert("vignette_color".into(), V::S(c));
            }
        }
        _ => {}
    }
}

/// A <VolumetricFog>'s columns.
fn volfog(v: &Map<String, Value>, row: &mut BTreeMap<String, V>) {
    row.insert("vfog".into(), V::N(1.0));
    row.insert("vfog_steps".into(), V::N(n(v, "steps", 64.0)));
    if v.get("dither").and_then(Value::as_bool) == Some(true) {
        row.insert("vfog_dither".into(), V::N(1.0));
    }
    if let Some(r) = num(v, "resolution") {
        row.insert("vfog_resolution".into(), V::N(r));
    }
    if let Some(m) = v.get("mode").and_then(Value::as_str).filter(|m| matches!(*m, "froxel" | "march")) {
        row.insert("vfog_mode".into(), V::S(m.into()));
    }
    if v.get("hero").and_then(Value::as_bool) == Some(false) {
        row.insert("vfog_hero".into(), V::N(0.0));
    }
    for (prop, col) in [("slices", "vfog_slices"), ("tile", "vfog_tile"), ("samples", "vfog_samples")] {
        if let Some(x) = num(v, prop) {
            row.insert(col.into(), V::N(x));
        }
    }
    row.insert("vfog_ambient".into(), V::N(n(v, "ambient", 0.0)));
    if let Some(c) = hex(v.get("ambientColor")) {
        row.insert("vfog_ambient_color".into(), V::S(c));
    }
    // what it tunes beyond its look (raea_sys_fog::froxel::tuning, docs/nobuild.feature NB1a)
    if let Some(s) = v.get("shadow").and_then(Value::as_str) {
        row.insert("vfog_shadow".into(), V::S(s.into()));
    }
    let flag = |o: &Map<String, Value>, k: &str| o.get(k).and_then(Value::as_bool).map(|b| V::N(b as u8 as f64));
    for (prop, col) in [("heroExact", "vfog_hero_exact"), ("tricubic", "vfog_tricubic")] {
        if let Some(b) = flag(v, prop) {
            row.insert(col.into(), b);
        }
    }
    if let Some(o) = v.get("epipolar").and_then(Value::as_object) {
        for (prop, col) in [("lineGap", "vfog_epi_line"), ("spacing", "vfog_epi_spacing"), ("every", "vfog_epi_every"), ("relTolerance", "vfog_epi_rel"), ("absTolerance", "vfog_epi_abs"), ("depthTolerance", "vfog_epi_depth")] {
            if let Some(x) = num(o, prop) {
                row.insert(col.into(), V::N(x));
            }
        }
    }
    match v.get("solari") {
        Some(Value::Bool(b)) => {
            row.insert("vfog_solari".into(), V::N(*b as u8 as f64));
        }
        Some(Value::Object(o)) => {
            for (prop, col) in [("candidates", "vfog_solari_candidates"), ("frames", "vfog_solari_frames"), ("skyRays", "vfog_solari_sky_rays"), ("guide", "vfog_solari_guide")] {
                if let Some(x) = num(o, prop) {
                    row.insert(col.into(), V::N(x));
                }
            }
            for (prop, col) in [("lamps", "vfog_solari_lamps"), ("sky", "vfog_solari_sky"), ("filter", "vfog_solari_filter")] {
                if let Some(b) = flag(o, prop) {
                    row.insert(col.into(), b);
                }
            }
        }
        _ => {}
    }
}

/// Each frame's <Shadows> (found once for every light): its props, and whether they change from frame to frame.
pub struct FrameShadows<'a> {
    per: Vec<Option<&'a Map<String, Value>>>,
    varies: bool,
}

impl<'a> FrameShadows<'a> {
    pub fn of(all: &'a [Vec<Item>]) -> FrameShadows<'a> {
        let per: Vec<Option<&Map<String, Value>>> = all.iter().map(|its| its.iter().find(|i| i.kind == "shadows").map(|i| &*i.props)).collect();
        let varies = per.windows(2).any(|w| w[0] != w[1]);
        FrameShadows { per, varies }
    }

    /// A light's shadow cascades (the sun's: light.rs reads them for it alone) and biases (every light's) from the
    /// frame's <Shadows> (RR1). One <Shadows> for the whole piece: its values on the row, as always. Ones that change:
    /// keyed, a frame without one at raea's cascades and Bevy's biases for the light's kind (0.02 / 1.8, a bulb's
    /// 0.08 / 0.6). A bias the light gives itself stays over them, unkeyed.
    pub fn light(&self, p: &mut Program, row: &mut BTreeMap<String, V>, id: &str, kind: &str, own: &Map<String, Value>) {
        let bulb = kind == "point";
        let cols = [
            ("distance", "shadow_dist", 150.0, None),
            ("firstCascade", "shadow_first", 8.0, None),
            ("cascades", "shadow_cascades", 4.0, None),
            ("overlap", "shadow_overlap", 0.3, None),
            ("bias", "shadow_bias", if bulb { 0.08 } else { 0.02 }, Some("shadow_depth_bias")),
            ("normalBias", "shadow_nbias", if bulb { 0.6 } else { 1.8 }, Some("shadow_normal_bias")),
        ];
        for (prop, col, d, mine) in cols {
            if mine.is_some_and(|m| own.contains_key(m)) {
                continue;
            }
            if !self.varies {
                if let Some(x) = self.per.iter().flatten().next().and_then(|sh| num(sh, prop)) {
                    row.insert(col.into(), V::N(x));
                }
                continue;
            }
            column(p, row, id, col, &self.per.iter().map(|sh| sh.and_then(|sh| num(sh, prop)).unwrap_or(d)).collect::<Vec<_>>());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &'static str, props: Value) -> Item {
        Item { kind, props: std::sync::Arc::new(props.as_object().cloned().unwrap_or_default()), shape: None, material: None, at: None, world: Default::default() }
    }

    #[test]
    fn the_scene_sig_changes_with_what_component_rows_are_made_from() {
        let with = |m: Option<(&str, Value)>| {
            let mut it = item("mesh", serde_json::json!({"components": [{"type": "Detail", "value": {"near": 10}}]}));
            it.material = m.map(|(kind, props)| Node3 { kind: kind.into(), props: std::sync::Arc::new(props.as_object().cloned().unwrap_or_default()), shape: None, at: None, kids: Vec::new().into() });
            vec![it]
        };
        let field = |w: f64| with(Some(("StandardMaterial", serde_json::json!({"color": "#000000", "field:emissive_exposure_weight": w}))));
        let shader = |t: f64| with(Some(("ThreeShaderMaterial", serde_json::json!({"uniforms": {"u_k": 1}, "time": t}))));
        // a material's animated field (FC1c) and a shader's time each change it: the fast path can't keep their rows
        assert_ne!(scene_sig(&field(0.0)), scene_sig(&field(1.0)));
        assert_ne!(scene_sig(&shader(0.0)), scene_sig(&shader(0.5)));
        // still components the same and nothing animated: the same sig, so LT21's fast path holds
        assert_eq!(scene_sig(&field(0.5)), scene_sig(&field(0.5)));
        assert_eq!(scene_sig(&with(None)), scene_sig(&with(None)));
    }

    #[test]
    fn each_frame_takes_its_own_post_processing() {
        let sun = item("light", serde_json::json!({}));
        let sky = vec![sun.clone(), item("atmosphere", serde_json::json!({"light": true})), item("bloom", serde_json::json!({"intensity": 0.35}))];
        let fog = vec![sun.clone(), item("fog", serde_json::json!({"color": "#c9ced4", "near": 10, "far": 90}))];
        // shot 1 (sky, bloom) for 3 frames, a title card with no 3D, shot 2 (fog) for 2 frames
        let all = vec![sky.clone(), sky.clone(), sky, vec![], fog.clone(), fog];
        let (mut p, mut row) = (Program::default(), BTreeMap::new());
        post(&all, &mut p, &mut row, "cam");
        let list: Vec<Value> = match row.get("frame_sets") {
            Some(V::S(s)) => serde_json::from_str(s).unwrap(),
            _ => panic!("no frame_sets: {row:?}"),
        };
        assert_eq!(list.len(), 2);
        assert!(list[0].get("atmosphere").is_some() && list[0].get("fog_color").is_none());
        assert!(list[1].get("fog_color").is_some() && list[1].get("bloom").is_none());
        // the title card holds shot 1's set; the key steps to shot 2's on its first frame
        assert_eq!(p.keys[&("cam".to_string(), "frame_set_i".to_string())], vec![(0, 0.0, "step".to_string()), (4, 1.0, "step".to_string())]);
        assert!(!row.contains_key("bloom"), "a changing set's columns are in the list, not on the row");
    }

    #[test]
    fn a_light_takes_each_frames_shadow_cascades() {
        // the interior shot's 20 m cascades between two 120 m shots, and a frame with no <Shadows>
        let sh = |d: f64| vec![item("shadows", serde_json::json!({"distance": d, "bias": 0.01}))];
        let all = vec![sh(120.0), sh(20.0), sh(120.0), vec![]];
        let (mut p, mut row) = (Program::default(), BTreeMap::new());
        let own = serde_json::json!({"shadow_normal_bias": 1.0}).as_object().cloned().unwrap();
        FrameShadows::of(&all).light(&mut p, &mut row, "sun0", "directional", &own);
        let keys = |c: &str| p.keys.get(&("sun0".to_string(), c.to_string())).map(|k| k.iter().map(|(f, v, _)| (*f, *v)).collect::<Vec<_>>());
        assert_eq!(keys("shadow_dist"), Some(vec![(0, 120.0), (1, 20.0), (2, 120.0), (3, 150.0)]));
        assert_eq!(keys("shadow_bias").map(|k| k.last().copied()), Some(Some((3, 0.02))));
        // the light's own normal bias is its own on every frame
        assert!(keys("shadow_nbias").is_none() && !row.contains_key("shadow_nbias"));
        // one <Shadows> throughout: on the row, unkeyed
        let (mut p, mut row) = (Program::default(), BTreeMap::new());
        FrameShadows::of(&[sh(20.0), sh(20.0)]).light(&mut p, &mut row, "sun0", "directional", &Map::new());
        assert_eq!(row.get("shadow_dist"), Some(&V::N(20.0)));
        assert!(p.keys.is_empty());
    }

    #[test]
    fn one_set_for_the_whole_piece_stays_on_the_row() {
        let shot = vec![item("bloom", serde_json::json!({"intensity": 0.5})), item("tonemapping", serde_json::json!({"mode": "AgX"}))];
        let (mut p, mut row) = (Program::default(), BTreeMap::new());
        post(&[shot.clone(), vec![], shot], &mut p, &mut row, "cam");
        assert!(row.contains_key("bloom") && row.get("tonemap") == Some(&V::S("agx".into())));
        assert!(!row.contains_key("frame_sets") && p.keys.is_empty());
    }

    #[test]
    fn an_additive_bloom_says_so_and_a_natural_one_doesnt() {
        let row = frame_post(&[item("bloom", serde_json::json!({"intensity": 2.0, "additive": true}))]);
        assert_eq!(row.get("bloom_additive"), Some(&V::N(1.0)));
        assert!(!frame_post(&[item("bloom", serde_json::json!({"intensity": 2.0}))]).contains_key("bloom_additive"));
    }
}

/// What a live frame's light sets (as_table) are made from: each light's colour, shadows, shafts and own biases, and the
/// frame's <Shadows>.
pub fn light_sig(its: &[Item]) -> String {
    let mut s = String::new();
    for it in its.iter().filter(|i| matches!(i.kind, "directional" | "point" | "spot" | "rect" | "shadows")) {
        s.push_str(it.kind);
        for k in ["color", "castShadow", "volumetric", "shadow_depth_bias", "shadow_normal_bias"] {
            if let Some(v) = it.props.get(k) {
                s.push_str(&format!("|{k}={v}"));
            }
        }
        if it.kind == "shadows" {
            s.push_str(&Value::Object((*it.props).clone()).to_string());
        }
        s.push(';');
    }
    s
}

/// light_sig, and everything a part's component rows are made from: its Bevy components (its <BevyComponent>s, a
/// Detail, its layer key), its material's animated fields (material_fields, docs/control.feature FC1c) and a three.js
/// shader's uniforms, time and textures (three_uniforms). A live frame's fast path (raea-runtime-scene fast.rs) doesn't
/// diff component rows, so the frame where any of these changes goes the full way, and a frame where none did leaves
/// the rows as they were sent. (11-18, v0.1.8-rc1: with the components alone, a material's animated field and a shader's
/// time stayed at the first frame in live play; bevy-ts's material-field-live failed on Windows and the Mac.)
pub fn scene_sig(its: &[Item]) -> String {
    let mut s = light_sig(its);
    for it in its {
        if let Some(c) = it.props.get("components") {
            s.push_str(&c.to_string());
        }
        let Some(m) = it.material.as_ref() else { continue };
        for (k, v) in m.props.iter().filter(|(k, _)| k.starts_with("field:")) {
            s.push_str(k);
            s.push_str(&v.to_string());
        }
        if &*m.kind == "ThreeShaderMaterial" {
            for k in ["uniforms", "time", "textures"] {
                if let Some(v) = m.props.get(k) {
                    s.push_str(&v.to_string());
                }
            }
        }
    }
    s
}
