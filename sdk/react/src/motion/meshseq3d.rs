//! Rows for baked simulations and glitches (sdk/motion/src/physics.tsx; engine/rdb/src/engine/meshseq.rs draws them):
//! `<MeshSequence>` (and `<Goo>`, which makes one) as a `meshseq` row, its frame and surface keyed per frame; and a
//! part's `glitch` prop (`<Model glitch>`, `<Mesh glitch>`, `<Body glitch>`) as glitch_* columns on its model or mesh
//! row.

use super::*;

/// The glitch columns, from a part's `glitch` prop on each frame ({explode, scramble, jitter, snap, smear, seed}).
pub(super) fn glitch_columns(p: &mut Program, row: &mut BTreeMap<String, V>, id: &str, its: &[Item]) {
    if !its.iter().any(|i| i.props.get("glitch").is_some_and(Value::is_object)) {
        return;
    }
    let get = |i: &Item, k: &str| i.props.get("glitch").and_then(|g| g.get(k)).and_then(Value::as_f64).unwrap_or(0.0);
    let smear = |i: &Item, a: usize| i.props.get("glitch").and_then(|g| g.get("smear")).and_then(Value::as_array).and_then(|v| v.get(a)).and_then(Value::as_f64).unwrap_or(0.0);
    for (k, col) in [("explode", "glitch_explode"), ("scramble", "glitch_scramble"), ("jitter", "glitch_jitter"), ("snap", "glitch_snap"), ("seed", "glitch_seed")] {
        column(p, row, id, col, &its.iter().map(|i| get(i, k)).collect::<Vec<_>>());
    }
    for (a, col) in ["glitch_smear_x", "glitch_smear_y", "glitch_smear_z"].iter().enumerate() {
        column(p, row, id, col, &its.iter().map(|i| smear(i, a)).collect::<Vec<_>>());
    }
}

/// Numeric props of a sequence and the columns they key (the default when a frame leaves one out).
const KEYED: &[(&str, &str, f64)] = &[("frame", "frame", 0.0), ("roughness", "rough", 0.3), ("metalness", "metal", 0.0)];

pub(super) fn rows(tracks: &[Track], p: &mut Program) {
    for (k, (_, track)) in tracks.iter().enumerate() {
        let its = filled(track);
        let id = format!("meshseq{k}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        let first = &its[0].props;
        row.insert("src".into(), V::S(src_path(first.get("src").and_then(Value::as_str).unwrap_or(""))));
        if let Some(m) = first.get("map").and_then(Value::as_str) {
            row.insert("map".into(), V::S(src_path(m)));
        }
        for (prop, col, d) in KEYED {
            if its.iter().any(|i| i.props.contains_key(*prop)) || *prop == "frame" {
                column(p, &mut row, &id, col, &its.iter().map(|i| n(&i.props, prop, *d)).collect::<Vec<_>>());
            }
        }
        // the first frame's surface
        for (prop, col) in [("clearcoat", "clearcoat"), ("clearcoatRoughness", "clearcoat_rough"), ("subsurface", "subsurface"), ("thickness", "thickness"), ("reflectance", "reflectance"),
            ("transmission", "transmission"), ("ior", "ior"), ("attenuationDistance", "atten_dist")] {
            if let Some(x) = num(first, prop) {
                row.insert(col.into(), V::N(x));
            }
        }
        if let Some(c) = hex(first.get("attenuationColor")) {
            row.insert("atten_color".into(), V::S(c));
        }
        if first.get("doubleSided") == Some(&Value::Bool(true)) {
            row.insert("double_sided".into(), V::N(1.0));
        }
        if its.iter().any(|i| i.props.contains_key("opacity")) {
            column(p, &mut row, &id, "opacity", &its.iter().map(|i| n(&i.props, "opacity", 1.0)).collect::<Vec<_>>());
        }
        let colors: Vec<String> = its.iter().map(|i| hex(i.props.get("color")).unwrap_or("#ffffff".into())).collect();
        row.insert("color".into(), V::S(colors[0].clone()));
        if colors.iter().any(|c| *c != colors[0]) {
            let c: Vec<[f64; 3]> = colors.iter().map(|c| rgb(c)).collect();
            for (k, name) in ["col_r", "col_g", "col_b"].iter().enumerate() {
                column(p, &mut row, &id, name, &c.iter().map(|v| v[k]).collect::<Vec<_>>());
            }
        }
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, track);
        p.spawns.push(("meshseq".into(), vec![row]));
    }
}
