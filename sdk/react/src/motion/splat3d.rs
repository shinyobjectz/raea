//! Gaussian splats' rows (`<Splat>`, sdk/motion/src/splat.tsx): a `splat` row each, its file, placement and every effect
//! keyed per frame (engine/rdb/src/engine/splat.rs reads them).

use super::*;

/// Numeric props and the columns they key (the prop's default when a frame leaves it out).
const KEYED: &[(&str, &str, f64)] = &[
    ("splatScale", "splat_scale", 1.0),
    ("opacity", "opacity", 1.0),
    ("brightness", "brightness", 1.0),
    ("saturation", "saturation", 1.0),
    ("contrast", "contrast", 1.0),
    ("revealSoft", "reveal_soft", 0.15),
    ("revealGlow", "reveal_glow", 0.0),
    ("disperse", "disperse", 0.0),
    ("curl", "curl", 1.0),
    ("explode", "explode", 0.0),
    ("waveAmplitude", "wave_amp", 0.0),
    ("wavelength", "wave_len", 1.0),
    ("wavePhase", "wave_phase", 0.0),
    ("twist", "twist", 0.0),
    ("morph", "morph", 0.0),
    ("pulse", "pulse", 0.0),
    ("pulseScale", "pulse_scale", 0.15),
    ("pulseGlow", "pulse_glow", 0.0),
    ("crop", "crop", 0.0),
    ("bands", "bands", 0.0),
    ("paint", "paint", 0.0),
    ("seed", "seed", 0.0),
];

/// Vector props: the prop, its three columns.
const VECS: &[(&str, [&str; 3])] = &[("wind", ["wind_x", "wind_y", "wind_z"]), ("revealFrom", ["reveal_x", "reveal_y", "reveal_z"]), ("revealAxis", ["reveal_ax", "reveal_ay", "reveal_az"])];

pub(super) fn rows(tracks: &[Track], p: &mut Program) {
    for (k, (_, track)) in tracks.iter().enumerate() {
        let its = filled(track);
        let id = format!("splat{k}");
        let mut row = BTreeMap::new();
        row.insert("id".into(), V::S(id.clone()));
        let first = &its[0].props;
        row.insert("src".into(), V::S(src_path(first.get("src").and_then(Value::as_str).unwrap_or(""))));
        if let Some(to) = its.iter().find_map(|i| i.props.get("morphTo").and_then(Value::as_str)) {
            row.insert("morph_to".into(), V::S(src_path(to)));
        }
        for (prop, col) in [("scatter", "disperse_mode"), ("revealMode", "reveal_mode")] {
            if let Some(s) = its.iter().find_map(|i| i.props.get(prop).and_then(Value::as_str)) {
                row.insert(col.into(), V::S(s.into()));
            }
        }
        // a reveal keyed only where a frame has one (a frame without is whole)
        if its.iter().any(|i| i.props.contains_key("reveal")) {
            column(p, &mut row, &id, "reveal", &its.iter().map(|i| n(&i.props, "reveal", 1.0)).collect::<Vec<_>>());
        }
        for (prop, col, d) in KEYED {
            if its.iter().any(|i| i.props.contains_key(*prop)) {
                column(p, &mut row, &id, col, &its.iter().map(|i| n(&i.props, prop, *d)).collect::<Vec<_>>());
            }
        }
        for (prop, cols) in VECS {
            if its.iter().any(|i| i.props.contains_key(*prop)) {
                let d = match *prop {
                    "wind" => [1.0, 0.15, 0.0],
                    "revealAxis" => [0.0, 1.0, 0.0],
                    _ => [0.0; 3],
                };
                let vs: Vec<[f64; 3]> = its.iter().map(|i| vec3(&i.props, prop, d)).collect();
                for (a, col) in cols.iter().enumerate() {
                    column(p, &mut row, &id, col, &vs.iter().map(|v| v[a]).collect::<Vec<_>>());
                }
            }
        }
        for (prop, col, d) in [("tint", "tint", "#ffffff"), ("glowColor", "glow", "#ffebbf")] {
            if !its.iter().any(|i| i.props.contains_key(prop)) {
                continue;
            }
            let colors: Vec<String> = its.iter().map(|i| hex(i.props.get(prop)).unwrap_or(d.into())).collect();
            row.insert(col.into(), V::S(colors[0].clone()));
            if col == "tint" && colors.iter().any(|c| *c != colors[0]) {
                let c: Vec<[f64; 3]> = colors.iter().map(|c| rgb(c)).collect();
                for (k, name) in ["tint_r", "tint_g", "tint_b"].iter().enumerate() {
                    column(p, &mut row, &id, name, &c.iter().map(|v| v[k]).collect::<Vec<_>>());
                }
            }
        }
        placement(p, &mut row, &id, &its);
        visible(p, &mut row, &id, track);
        p.spawns.push(("splat".into(), vec![row]));
    }
}
