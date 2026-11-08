use super::*;

/// the scene's <SurfaceShader>, from the first frame: its spec as JSON (raea-runtime-scene3d's shaded.rs)
pub(crate) fn surface_shader_rows(all: &[Vec<Item>], p: &mut Program) {
    let Some(f0) = all.first() else { return };
    let Some(it) = f0.iter().find(|it| matches!(&*it.kind, "surface_shader")) else { return };
    p.spawns.push(("surface_shader".into(), vec![shader_row("surface_shader0".into(), it)]));
}

/// the scene's <FramePass>es or <Compute>s (`kind` frame_pass or compute), from the first frame, in the order written:
/// frame_pass0, frame_pass1, …; a compute or pass as written's `entry`, `bind` and `uniforms` keyed (strings that change
/// frame by frame)
pub(crate) fn ordered_shader_rows(all: &[Vec<Item>], p: &mut Program, kind: &str) {
    let Some(f0) = all.first() else { return };
    let mut rows: Vec<_> = f0.iter().filter(|it| *it.kind == *kind).enumerate().map(|(i, it)| shader_row(format!("{kind}{i}"), it)).collect();
    for (i, row) in rows.iter_mut().enumerate() {
        let id = format!("{kind}{i}");
        for prop in ["entry", "bind", "uniforms"].into_iter().filter(|k| f0.iter().filter(|it| *it.kind == *kind).nth(i).is_some_and(|it| it.props.contains_key(*k))) {
            let vals: Vec<String> = all.iter().map(|f| f.iter().filter(|it| *it.kind == *kind).nth(i).and_then(|it| it.props.get(prop)?.as_str()).unwrap_or("").to_string()).collect();
            keyed_strings(p, row, &id, prop, &vals);
        }
    }
    if !rows.is_empty() {
        p.spawns.push((kind.into(), rows));
    }
}

/// A row holding an item's shader spec as JSON.
fn shader_row(id: String, it: &Item) -> BTreeMap<String, V> {
    let mut row = BTreeMap::new();
    row.insert("id".into(), V::S(id));
    row.insert("shader".into(), V::S(it.props.get("shader").cloned().unwrap_or(Value::Null).to_string()));
    row
}

/// lights: directional (towards its target, the origin by default), point, spot
pub(crate) fn lights_rows(all: &[Vec<Item>], tr: &Tracks, p: &mut Program) {
    let mut li = 0;
    // a <Shadows>, found once: every light takes its cascades and biases (looking for it per light scanned every
    // frame's items once a light, 100 000 lights by 100 000 items)
    let shadows = all.iter().flatten().find(|i| i.kind == "shadows");
    for (_, track) in &tr.lights {
        let its = filled(&track);
        let kind = its[0].kind;
        let id = if kind == "directional" { format!("sun{li}") } else { format!("{kind}{li}") };
        li += 1;
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        if kind != "directional" {
            row.insert("type".into(), V::S(kind.into()));
        }
        // a missing frame is a dark one
        let on: Vec<f64> = track.iter().map(|t| t.is_some() as u8 as f64).collect();
        let power: Vec<f64> = its.iter().zip(&on).map(|(i, o)| light_power(kind, &i.props) * o).collect();
        column(p, &mut row, &id, if kind == "directional" { "lux" } else { "lumens" }, &power);
        let pos: Vec<[f64; 3]> = its.iter().map(|i| {
            let v = vec3(&i.props, "position", if kind == "directional" { [0.0, 1.0, 0.0] } else { [0.0; 3] });
            let w = i.world.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
            [w.x as f64, w.y as f64, w.z as f64]
        }).collect();
        let target: Vec<[f64; 3]> = its.iter().map(|i| vec3(&i.props, "target", [0.0; 3])).collect();
        if kind != "directional" {
            for (k, name) in ["x", "y", "z"].iter().enumerate() {
                column(p, &mut row, &id, name, &pos.iter().map(|v| v[k]).collect::<Vec<_>>());
            }
        }
        if kind != "point" {
            for (k, name) in ["dir_x", "dir_y", "dir_z"].iter().enumerate() {
                column(p, &mut row, &id, name, &pos.iter().zip(&target).map(|(a, b)| b[k] - a[k]).collect::<Vec<_>>());
            }
        }
        let first = &its[0].props;
        if let Some(c) = hex(first.get("color")).filter(|c| c != "#ffffff") {
            row.insert("color".into(), V::S(c));
        }
        if let Some(Value::Bool(s)) = first.get("castShadow") {
            row.insert("shadows".into(), V::N(*s as u8 as f64));
        }
        if first.get("volumetric") == Some(&Value::Bool(true)) {
            row.insert("volumetric".into(), V::N(1.0));
        }
        // a point or spot light the paint look draws (paint.rs)
        if kind != "directional" && first.get("paint") == Some(&Value::Bool(true)) {
            row.insert("paint".into(), V::N(1.0));
        }
        // soft shadows (PCSS): `true` for the default size (a sun's 0.5, a bulb's 0.25 m), or the size
        let soft = match first.get("softShadow") {
            Some(Value::Bool(true)) => Some(if kind == "directional" { 0.5 } else { 0.25 }),
            Some(Value::Number(x)) => x.as_f64(),
            _ => num(first, "softness"),
        };
        if let Some(r) = soft.filter(|r| *r > 0.0) {
            row.insert("soft".into(), V::N(r));
        }
        // a <Shadows>' cascades (the sun's) and biases (every light's)
        if let Some(sh) = shadows {
            for (prop, col) in [("distance", "shadow_dist"), ("firstCascade", "shadow_first"), ("cascades", "shadow_cascades"), ("overlap", "shadow_overlap"), ("bias", "shadow_bias"), ("normalBias", "shadow_nbias")] {
                if let Some(x) = num(&sh.props, prop) {
                    row.insert(col.into(), V::N(x));
                }
            }
        }
        // the light's own biases (Bevy's shadow_depth_bias, shadow_normal_bias), over a <Shadows>'
        for (prop, col) in [("shadow_depth_bias", "shadow_bias"), ("shadow_normal_bias", "shadow_nbias")] {
            if let Some(x) = num(first, prop) {
                row.insert(col.into(), V::N(x));
            }
        }
        // a bulb's or spot's reach, and its size (Bevy's radius: a spherical area light's highlight, 3d/spherical_area_lights)
        for (prop, col) in [("distance", "range"), ("radius", "radius")].iter().filter(|_| kind != "directional") {
            if let Some(d) = num(first, prop).filter(|d| *d > 0.0) {
                row.insert(col.to_string(), V::N(d));
            }
        }
        // a rectangle of light: its face's size
        if kind == "rect" {
            row.insert("width".into(), V::N(r6(n(first, "width", 1.0))));
            row.insert("height".into(), V::N(r6(n(first, "height", 1.0))));
        }
        if kind == "spot" { // its cone, keyed: spotlight.rs's light_sway widens it every frame
            let outer: Vec<f64> = its.iter().map(|i| n(&i.props, "angle", std::f64::consts::FRAC_PI_3).clamp(0.0, std::f64::consts::FRAC_PI_2)).collect();
            column(p, &mut row, &id, "inner", &its.iter().zip(&outer).map(|(i, a)| a * (1.0 - n(&i.props, "penumbra", 0.0).clamp(0.0, 1.0))).collect::<Vec<_>>());
            column(p, &mut row, &id, "outer", &outer);
        }
        layers_column(&mut row, first);
        p.spawns.push(("light".into(), vec![row]));
        components(p, &id, &its);
    }
}

/// meshes: shape and surface from their first frame; placement and the surface values that move, keyed
pub(crate) fn meshes_rows(tr: &Tracks, names: &[String], p: &mut Program) {
    for (m, (_, track)) in tr.meshes.iter().enumerate() {
        let its = filled(&track);
        let Some(shape) = its[0].shape.clone() else { continue };
        let id = format!("mesh{m}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        row.insert("shape".into(), V::S(shape.to_string()));
        let mat = |i: &Item| i.material.as_ref().map(|m| m.props.clone()).unwrap_or_default();
        let mats: Vec<std::sync::Arc<Map<String, Value>>> = its.iter().map(mat).collect();
        let basic = its[0].material.as_ref().is_some_and(|m| &*m.kind == "MeshBasicMaterial");
        let colors: Vec<String> = mats.iter().map(|m| hex(m.get("color")).unwrap_or("#cccccc".into())).collect();
        row.insert("color".into(), V::S(colors[0].clone()));
        if colors.iter().any(|c| *c != colors[0]) {
            let c: Vec<[f64; 3]> = colors.iter().map(|c| rgb(c)).collect();
            for (k, name) in ["col_r", "col_g", "col_b"].iter().enumerate() {
                let vals: Vec<f64> = c.iter().map(|v| v[k]).collect();
                // exact, as the hex's byte over 255 (the engine reads either into the same f32: #26)
                p.keys.insert((id.clone(), name.to_string()), vals.iter().enumerate().map(|(f, v)| (f as i64, *v, "linear".to_string())).collect());
                row.insert(name.to_string(), V::N(vals[0]));
            }
        }
        let m0 = &mats[0];
        // a roughness or metalness map is scaled by its factor, which is 1 unless given (three.js and glTF's rule)
        let has = |k: &str| m0.get(k).and_then(Value::as_str).is_some();
        let rough0 = if has("roughnessMap") || has("metallicRoughnessMap") { 1.0 } else { 0.5 };
        let metal0 = if has("metalnessMap") || has("metallicRoughnessMap") { 1.0 } else { 0.0 };
        column(p, &mut row, &id, "rough", &mats.iter().map(|m| n(m, "roughness", rough0)).collect::<Vec<_>>());
        column(p, &mut row, &id, "metal", &mats.iter().map(|m| n(m, "metalness", metal0)).collect::<Vec<_>>());
        if let Some(e) = hex(m0.get("emissive")).filter(|e| e != "#000000") {
            row.insert("emissive".into(), V::S(e));
            column(p, &mut row, &id, "emissive_k", &mats.iter().map(|m| emissive_power(m)).collect::<Vec<_>>());
        }
        let see_through = mats.iter().any(|m| m.get("transparent") == Some(&Value::Bool(true)) || num(m, "opacity").is_some_and(|o| o < 1.0));
        if see_through {
            column(p, &mut row, &id, "opacity", &mats.iter().map(|m| n(m, "opacity", 1.0)).collect::<Vec<_>>());
        }
        if basic || m0.get("unlit") == Some(&Value::Bool(true)) {
            row.insert("unlit".into(), V::N(1.0));
        }
        for (k, col) in [("clearcoat", "clearcoat"), ("clearcoatRoughness", "clearcoat_rough"), ("reflectance", "reflectance")] {
            if let Some(v) = num(m0, k) {
                row.insert(col.into(), V::N(v));
            }
        }
        for (prop, col) in [("map", "map"), ("normalMap", "normal_map"), ("roughnessMap", "rough_map"), ("metalnessMap", "metal_map"), ("metallicRoughnessMap", "mr_map"), ("aoMap", "ao_map"), ("emissiveMap", "emissive_map"), ("depthMap", "depth_map"), ("clearcoatNormalMap", "clearcoat_normal_map")] {
            if let Some(src) = m0.get(prop).and_then(Value::as_str) {
                row.insert(col.into(), V::S(src_path(src)));
            }
        }
        if let Some(Value::Array(r)) = m0.get("repeat") {
            if let [u, v] = r.as_slice() {
                row.insert("repeat_u".into(), V::N(u.as_f64().unwrap_or(1.0)));
                row.insert("repeat_v".into(), V::N(v.as_f64().unwrap_or(1.0)));
            }
        }
        // how its alpha and faces draw, and real transmission (StandardMaterial's fields), from the first frame
        for (prop, col) in [("alphaMode", "alpha"), ("side", "side")] {
            if let Some(Value::String(x)) = m0.get(prop) {
                row.insert(col.into(), V::S(x.clone()));
            }
        }
        for (prop, col) in [("alphaCutoff", "alpha_cutoff"), ("transmission", "transmission"), ("ior", "ior"), ("thickness", "thickness")] {
            if let Some(x) = num(m0, prop) {
                row.insert(col.into(), V::N(x));
            }
        }
        // StandardMaterial's other fields (docs/shaders.feature SH7), from the first frame
        if let Some(f) = fields_of(m0) {
            row.insert("fields".into(), V::S(f));
        }
        // how its maps are read (docs/shaders.feature SH5): all of them, and per map by its prop name
        for k in ["sampler", "samplers"] {
            if let Some(v) = m0.get(k).filter(|v| v.is_object()) {
                row.insert(k.into(), V::S(v.to_string()));
            }
        }
        // a shader over its lit surface (docs/shaders.feature SH1), from the first frame
        if let Some(sh) = m0.get("shader").filter(|v| v.is_object()) {
            row.insert("shader".into(), V::S(sh.to_string()));
        }
        // a three.js shader (<ThreeShaderMaterial>, docs/r3f.feature R3F11): its spec from the first frame, unlit in its
        // colour where it can't run; its uniforms keyed frame by frame (three_uniforms)
        if its[0].material.as_ref().is_some_and(|m| &*m.kind == "ThreeShaderMaterial") {
            row.insert("three".into(), V::S(Value::Object((**m0).clone()).to_string()));
            row.insert("unlit".into(), V::N(1.0));
        }
        if its[0].material.as_ref().is_some_and(|m| &*m.kind == "MeshToonMaterial") {
            // a toon surface: its bands, rim, highlight and outline from the first frame; what animates, keyed
            row.insert("toon".into(), V::S(toon_spec(m0, false).to_string()));
            let shadows: Vec<Option<String>> = mats.iter().map(|m| hex(m.get("shadowColor"))).collect();
            // (always columns, so a live game's frames can change them as a render's keys do)
            if shadows[0].is_some() {
                let c: Vec<[f64; 3]> = shadows.iter().map(|c| rgb(c.as_deref().unwrap_or("#000000"))).collect();
                for (k, name) in ["sh_r", "sh_g", "sh_b"].iter().enumerate() {
                    column(p, &mut row, &id, name, &c.iter().map(|v| v[k]).collect::<Vec<_>>());
                }
            }
            for (col, part, flat, key, d) in [("rim_k", "rim", "rimIntensity", "intensity", 1.0), ("spec_k", "highlight", "highlightIntensity", "intensity", 1.0), ("outline_w", "outline", "outlineWidth", "width", 2.0)] {
                if m0.get(part).is_some_and(Value::is_object) {
                    let vals: Vec<f64> = mats.iter().map(|m| num(m, flat).or_else(|| m.get(part).and_then(Value::as_object).and_then(|x| num(x, key))).unwrap_or(d)).collect();
                    column(p, &mut row, &id, col, &vals);
                }
            }
        }
        grid_columns(&mut row, m0);
        paint_columns(&mut row, m0);
        raea_sys_wear::columns(&mut row, m0);
        raea_sys_procmat::columns(&mut row, m0);
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, &track);
        pass_column(p, &mut row, &id, &its, &names);
        if its[0].props.get("driven") == Some(&Value::Bool(true)) {
            row.insert("driven".into(), V::N(1.0));
        }
        // a page on it over its UVs (<Mesh3d>'s layer="surface" element, S3a): its screen spec, JSON (screen.rs's
        // mesh_screens)
        if let Some(v) = its[0].props.get("surface").filter(|v| v.is_object()) {
            row.insert("surface".into(), V::S(v.to_string()));
        }
        // `castShadow: false`, three.js's `shadow: false` (`shadow` wins)
        if its[0].props.get("shadow").or(its[0].props.get("castShadow")) == Some(&Value::Bool(false)) {
            row.insert("shadow".into(), V::N(0.0));
        }
        // `shadow: "only"`: it casts shadows but isn't drawn (raea-render's shadow_only.rs)
        if its[0].props.get("shadow").and_then(Value::as_str) == Some("only") {
            row.insert("shadow_only".into(), V::N(1.0));
        }
        // `shadow: "catch"`: only the shadows falling on it are drawn (raea-render's shadow_catch.rs)
        if its[0].props.get("shadow").and_then(Value::as_str) == Some("catch") {
            row.insert("shadow_catch".into(), V::N(1.0));
        }
        // `receiveShadow: false`: no shadow falls on it (NotShadowReceiver)
        if its[0].props.get("receiveShadow") == Some(&Value::Bool(false)) {
            row.insert("receive".into(), V::N(0.0));
        }
        layers_column(&mut row, &its[0].props);
        instances_column(&mut row, &its);
        // its geometry's own vertex attributes (JSON, Mesh3d `attributes`), from the first frame: raea-sys-mesh with_attributes
        if let Some(Value::String(a)) = its[0].props.get("attributes") {
            row.insert("attributes".into(), V::S(a.clone()));
        }
        name_column(&mut row, &its[0].props);
        p.spawns.push(("mesh".into(), vec![row]));
        components(p, &id, &its);
        material_fields(p, &id, &its);
        three_uniforms(p, &id, &its);
        own_uniforms(p, &id, &its, |it| it.material.as_ref()?.props.get("shader"));
    }
}

/// bare entities (<Entity>): placed (keyed like a mesh), named, and their Bevy components
pub(crate) fn entities_rows(tr: &Tracks, p: &mut Program) {
    for (k, (_, track)) in tr.entities.iter().enumerate() {
        let its = filled(track);
        let id = format!("ent{k}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, track);
        name_column(&mut row, &its[0].props);
        p.spawns.push(("entity".into(), vec![row]));
        components(p, &id, &its);
    }
}

/// text in the 3D world (<RichText>): the block (its runs, em, alignment and surface from its first frame; its
/// placement keyed), then a row for each of its glyphs, under it: each glyph's motion about its own centre keyed like
/// a mesh's (x, y, z, rot_*, scale_*, col_*, opacity, visible), its place in the block raea-render's to lay out (text3d.rs)
pub(crate) fn texts_rows(tr: &Tracks, names: &[String], p: &mut Program) {
    for (k, (_, track)) in tr.texts.iter().enumerate() {
        let its = filled(track);
        let id = format!("text{k}");
        let first = &its[0].props;
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        row.insert("spans".into(), V::S(first.get("spans").cloned().unwrap_or(Value::Array(vec![])).to_string()));
        for (prop, col, d) in [("size", "size", 0.5), ("align", "align", 0.5), ("lineHeight", "line_height", 1.2)] {
            row.insert(col.into(), V::N(r6(n(first, prop, d))));
        }
        if let Some(w) = num(first, "maxWidth").filter(|w| *w > 0.0) {
            row.insert("max_width".into(), V::N(r6(w)));
        }
        for (prop, col) in [("roughness", "rough"), ("metalness", "metal")] {
            if let Some(v) = num(first, prop) {
                row.insert(col.into(), V::N(r6(v)));
            }
        }
        if let Some(e) = hex(first.get("emissive")).filter(|e| e != "#000000") {
            row.extend([("emissive".into(), V::S(e)), ("emissive_k".into(), V::N(r6(emissive_power(first))))]);
        }
        for (prop, col) in [("unlit", "unlit"), ("castShadow", "shadow"), ("receiveShadow", "receive")] {
            if let Some(Value::Bool(b)) = first.get(prop) {
                row.insert(col.into(), V::N(*b as u8 as f64));
            }
        }
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, track);
        pass_column(p, &mut row, &id, &its, &names);
        name_column(&mut row, first);
        layers_column(&mut row, first);
        p.spawns.push(("text".into(), vec![row]));
        // its glyphs: as many as its first frame has (a block's text is fixed, as a mesh's shape is)
        let name = first.get("name").and_then(Value::as_str);
        let glyphs = |it: &Item| it.props.get("glyphs").and_then(Value::as_array).cloned().unwrap_or_default();
        let per: Vec<Vec<Value>> = its.iter().map(glyphs).collect();
        for (gi, g0) in per[0].iter().enumerate() {
            let gid = format!("{id}-g{gi}");
            let mut row = BTreeMap::new();
            row.insert("id".into(), V::S(gid.clone()));
            row.insert("target".into(), V::S(id.clone()));
            row.insert("at".into(), V::N(g0.get("at").and_then(Value::as_f64).unwrap_or(0.0)));
            row.insert("span".into(), V::N(g0.get("span").and_then(Value::as_f64).unwrap_or(0.0)));
            row.insert("ch".into(), V::S(g0.get("ch").and_then(Value::as_str).unwrap_or("").to_string()));
            if let Some(n) = name {
                row.insert("name".into(), V::S(format!("{n}:{gi}")));
            }
            let at = |f: usize| per[f].get(gi).and_then(Value::as_object).cloned().unwrap_or_default();
            let gs: Vec<Map<String, Value>> = (0..its.len()).map(at).collect();
            // each frame's motion about its centre, as an Item's world matrix, so placement keys it as a mesh's
            let moved: Vec<Item> = gs.iter().map(|g| {
                let mut m = Map::new();
                for (from, to) in [("p", "position"), ("r", "rotation"), ("s", "scale")] {
                    if let Some(v) = g.get(from) {
                        m.insert(to.into(), v.clone());
                    }
                }
                Item { kind: "glyph", props: std::sync::Arc::new(Map::new()), shape: None, material: None, world: local(&m) }
            }).collect();
            placement(p, &mut row, &gid, &moved);
            let colors: Vec<String> = gs.iter().map(|g| hex(g.get("c")).unwrap_or("#ffffff".into())).collect();
            row.insert("color".into(), V::S(colors[0].clone()));
            if colors.iter().any(|c| *c != colors[0]) {
                let c: Vec<[f64; 3]> = colors.iter().map(|c| rgb(c)).collect();
                for (ch, col) in ["col_r", "col_g", "col_b"].iter().enumerate() {
                    exact(p, &mut row, &gid, col, &c.iter().map(|v| v[ch]).collect::<Vec<_>>());
                }
            }
            let op: Vec<f64> = gs.iter().map(|g| g.get("o").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, 1.0)).collect();
            if op.iter().any(|o| *o < 1.0) {
                column(p, &mut row, &gid, "opacity", &op);
            }
            // shown while its block is in the frame and it isn't hidden (`visible: false`, or not yet revealed)
            let on: Vec<f64> = track.iter().zip(&gs).map(|(t, g)| (t.is_some() && g.get("v") != Some(&Value::Bool(false))) as u8 as f64).collect();
            if on.iter().any(|v| *v < 1.0) {
                column(p, &mut row, &gid, "visible", &on);
            }
            layers_column(&mut row, first);
            pass_column(p, &mut row, &gid, &its, &names);
            p.spawns.push(("glyph".into(), vec![row]));
        }
    }
}
