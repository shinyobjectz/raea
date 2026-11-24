//! The kit's sheets and its landing (docs/make.feature MK11, MK13): the pick sheet (each object's reference beside
//! both generators' models, each in colour and in clay, drawn in raea), and the kit landed as a component set (one
//! component per object, its model and LOD imported beside it, so it travels as one folder), a `<Lineup>` of them all
//! with a scale bar, a thumb each, and kit.json.
//!
//!   raea make pick-sheet [DIR] [--only ID,...]
//!   raea make kit [DIR] [--name KIT] [--to FOLDER]

use std::path::{Path, PathBuf};

use raea_core_brief::make::{Make, Object};
use serde_json::{Value, json};

use crate::kit::{gen_dir, objects, picks};
use crate::sheet::{Tile, sheet};
use crate::shots::raea;

/// The tallest side an object will have once fitted, metres (for framing it): its height, its box, or its longest side.
pub fn tall(o: &Object) -> f64 {
    o.h.or(o.dims.map(|d| d[2])).or(o.max).unwrap_or(0.5).max(0.02)
}

/// The size a model is framed to: its biggest side (a stapler is framed by its length, not its height).
pub fn frame_size(o: &Object) -> f64 {
    o.dims.map(|[w, d, h]| w.max(d).max(h)).unwrap_or_else(|| tall(o))
}

/// A composition showing one model alone, fitted (bottom-centred at the origin, metres), on a pale floor in a
/// three-quarter view from slightly above, framed to its height.
pub fn viewer(model: &Path, h: f64) -> String {
    let d = h * 3.2 + 0.06;
    let m = model.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    format!(
        "// written by raea make (pick sheet, kit thumbs): one model, framed to its height\nimport {{ AbsoluteFill, Composition, registerRoot }} from \"raea\";\nimport {{ Model }} from \"raea/3d\";\nimport {{ StandardMaterial }} from \"raea/pbr\";\nimport {{ Cuboid, Mesh3d }} from \"raea/mesh\";\nimport {{ AmbientLight, DirectionalLight }} from \"raea/light\";\nimport {{ Transform }} from \"raea/transform\";\nimport {{ Camera3d }} from \"raea/camera\";\nimport model from \"./{m}\";\n\nfunction View() {{\n  return (\n    <AbsoluteFill style={{{{ background: \"#d9d6d0\" }}}}>\n      <Camera3d transform={{Transform.fromXyz({x:.3}, {y:.3}, {z:.3}).lookingAt([0, {cy:.3}, 0])}} projection={{{{ Perspective: {{ fov: (30 * Math.PI) / 180 }} }}}} />\n      <AmbientLight brightness={{300}} />\n      <DirectionalLight transform={{Transform.fromXyz(2, 5, 3).lookingAt([0, 0, 0])}} illuminance={{9000}} shadow_maps_enabled />\n      <Mesh3d transform={{Transform.fromXyz(0, -0.005, 0)}}>\n        <Cuboid half_size={{[{g:.2}, 0.005, {g:.2}]}} />\n        <StandardMaterial base_color=\"#cfccc6\" perceptual_roughness={{0.95}} />\n      </Mesh3d>\n      <Model src={{model}} />\n    </AbsoluteFill>\n  );\n}}\n\nregisterRoot(() => <Composition id=\"view\" component={{View}} width={{768}} height={{768}} fps={{30}} durationInFrames={{1}} />);\n",
        x = d * 0.62,
        y = h * 0.55 + d * 0.32,
        z = d * 0.78,
        cy = h * 0.45,
        g = (h * 4.0).max(1.0),
    )
}

/// One model drawn in colour and in clay: (colour.png, clay.png) beside it.
pub fn draw(model: &Path, h: f64) -> Result<(PathBuf, PathBuf), String> {
    let dir = model.parent().ok_or("a model's folder")?;
    let stem = model.file_stem().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    let tsx = dir.join(format!("{stem}.view.tsx"));
    std::fs::write(&tsx, viewer(model, h)).map_err(|e| format!("{}: {e}", tsx.display()))?;
    let (colour, clay) = (dir.join(format!("{stem}.colour.png")), dir.join(format!("{stem}.clay.png")));
    raea(&["render".into(), tsx.display().to_string(), "-o".into(), colour.display().to_string(), "--frame".into(), "0".into()], &[])?;
    raea(&["render".into(), tsx.display().to_string(), "-o".into(), clay.display().to_string(), "--frame".into(), "0".into(), "--clay".into()], &[])?;
    Ok((colour, clay))
}

/// The pick sheet: a row an object, its reference, then each generator's model fitted to size, in colour and in clay.
pub fn pick_sheet(dir: &Path, m: &Make, only: &[String]) -> Result<String, String> {
    let mut rows = Vec::new();
    for o in objects(m, only) {
        let g = gen_dir(dir, &o.id);
        let mut row = vec![Tile { img: crate::kit::refs_dir(dir).join(format!("{}.png", o.id)), label: format!("{} reference", o.id) }];
        for generator in ["tripo", "meshy"] {
            let raw = g.join(format!("{generator}.glb"));
            if !raw.is_file() {
                row.push(Tile { img: g.join("none.png"), label: format!("{generator}: not made") });
                continue;
            }
            // fitted to its real size first, so both are framed alike
            let fit = g.join(format!("{generator}.fit.glb"));
            let mut args = vec!["gen".into(), "check".into(), raw.display().to_string()];
            args.extend(crate::kit::fit_args(o, crate::kit::measured_size(&raw)));
            args.extend(["-o".into(), fit.display().to_string()]);
            let _ = raea(&args, &[]);
            let shown = if fit.is_file() { fit } else { raw };
            let (colour, clay) = draw(&shown, frame_size(o))?;
            row.push(Tile { img: colour, label: generator.to_string() });
            row.push(Tile { img: clay, label: format!("{generator} clay") });
        }
        rows.push(row);
    }
    if rows.is_empty() {
        return Err("raea make pick-sheet: no objects in the plan (or none named)".into());
    }
    let file = dir.join("review/picks.png");
    sheet("Pick sheet: reference | Tripo | Tripo clay | Meshy | Meshy clay", &rows, 300, &file)?;
    Ok(format!("wrote {}: judge each on its clay (fused parts, blobs, fidelity to the reference), then: raea make pick ID tripo|meshy \"why\"", file.display()))
}

/// A component's name from an object id: rotary_phone -> RotaryPhone.
pub fn component_name(id: &str) -> String {
    let mut out = String::new();
    for w in id.split(['_', '-']) {
        let mut c = w.chars();
        out.extend(c.next().into_iter().flat_map(char::to_uppercase).chain(c));
    }
    out
}

/// One object's component: its model imported beside it, its LOD for far away, placed by the props a composition gives.
pub fn component(o: &Object, why: &str) -> String {
    let n = component_name(&o.id);
    format!(
        "// raea component add: {label}, {h} m, from the kit's style (raea make kit; {why})\nimport {{ Model }} from \"raea/3d\";\nimport full from \"./{id}.glb\";\nimport far from \"./{id}_lod1.glb\";\n\ntype Props = {{\n  position?: [number, number, number];\n  rotation?: [number, number, number];\n  /** the quarter-triangle model, for when it's small on screen */\n  lod?: boolean;\n}};\n\n/** {label}: fitted to its real size, its bottom at the origin, its front toward +z. */\nexport default function {n}({{ position = [0, 0, 0], rotation = [0, 0, 0], lod = false }}: Props) {{\n  return <Model src={{lod ? far : full}} position={{position}} rotation={{rotation}} />;\n}}\n",
        label = o.label,
        h = tall(o),
        id = o.id,
        why = why.replace('\n', " "),
    )
}

/// The lineup: every component side by side in the plan's order, gaps between, a label each lying in front of the pieces and a
/// scale bar (the longest round length the set covers), seen from a little right of centre and above.
pub fn lineup(m: &Make, kit: &str) -> String {
    let mut imports = String::new();
    let mut parts = String::new();
    let mut x = 0.0;
    let foot = |o: &Object| o.dims.map(|d| (d[0], d[1])).unwrap_or((tall(o) * 0.8, tall(o) * 0.8));
    let widths: Vec<f64> = m.objects.iter().map(|o| foot(o).0).collect();
    let gap = (widths.iter().cloned().fold(0.0, f64::max) * 0.6).clamp(0.03, 0.3);
    let total: f64 = widths.iter().sum::<f64>() + gap * (m.objects.len().saturating_sub(1)) as f64;
    let highest = m.objects.iter().map(tall).fold(0.02, f64::max);
    let deepest = m.objects.iter().map(|o| foot(o).1).fold(0.02, f64::max);
    // labels lie on the floor in front of every piece, so none hides one; the bar in front of them
    let fs = (total * 0.035).clamp(0.01, 0.06);
    let lz = deepest / 2.0 + fs * 1.9;
    let (bar, bar_label) = scale_bar(total);
    let bz = lz + fs * 2.8;
    for (o, w) in m.objects.iter().zip(&widths) {
        let n = component_name(&o.id);
        imports.push_str(&format!("import {n} from \"./{n}\";\n"));
        let cx = x + w / 2.0 - total / 2.0;
        parts.push_str(&format!("      <{n} position={{[{cx:.3}, 0, 0]}} />\n      <Mesh3d transform={{Transform.fromXyz({cx:.3}, 0.002, {lz:.3}).withRotation(FLAT)}}>\n        <Text3D size={{{fs:.3}}} depth={{0.001}} read={{false}}>{{{label:?}}}</Text3D>\n        <StandardMaterial base_color=\"#2a2a2a\" />\n      </Mesh3d>\n", label = o.label));
        x += w + gap;
    }
    // framed to the set: wide enough for every piece, tall enough for the highest and the bar in front, from a little
    // to the right and above, so a long piece shows its length
    let d = (total * 1.45).max(highest * 4.2).max(bz * 3.0).max(0.35);
    let (cx, cy, cz) = (d * 0.34, highest * 0.5 + d * 0.36, d * 0.88 + bz * 0.5);
    format!(
        "// raea make kit: the {kit} kit's lineup, every piece at its real size beside a {bar_label} bar (the kit's cover)\nimport {{ AbsoluteFill, Composition, registerRoot }} from \"raea\";\nimport {{ Text3D }} from \"raea/3d\";\nimport {{ StandardMaterial }} from \"raea/pbr\";\nimport {{ Cuboid, Mesh3d }} from \"raea/mesh\";\nimport {{ AmbientLight, DirectionalLight }} from \"raea/light\";\nimport {{ Transform }} from \"raea/transform\";\nimport {{ Camera3d }} from \"raea/camera\";\n{imports}\n// lying flat on the floor, reading toward the camera\nconst FLAT: [number, number, number, number] = [-0.7071068, 0, 0, 0.7071068];\n\nexport function Lineup() {{\n  return (\n    <AbsoluteFill style={{{{ background: \"#d6d2ca\" }}}}>\n      <Camera3d transform={{Transform.fromXyz({cx:.3}, {cy:.3}, {cz:.3}).lookingAt([0, {ty:.3}, {tz:.3}])}} projection={{{{ Perspective: {{ fov: (30 * Math.PI) / 180 }} }}}} />\n      <AmbientLight brightness={{300}} />\n      <DirectionalLight transform={{Transform.fromXyz(2, 6, 4).lookingAt([0, 0, 0])}} illuminance={{9000}} shadow_maps_enabled />\n      <Mesh3d transform={{Transform.fromXyz(0, -0.005, 0)}}>\n        <Cuboid half_size={{[{g:.2}, 0.005, {g:.2}]}} />\n        <StandardMaterial base_color=\"#d6d2ca\" perceptual_roughness={{0.95}} />\n      </Mesh3d>\n      <Mesh3d transform={{Transform.fromXyz({bx:.3}, 0.002, {bz:.3})}}>\n        <Cuboid half_size={{[{bh:.3}, 0.002, {bt:.4}]}} />\n        <StandardMaterial base_color=\"#202020\" />\n      </Mesh3d>\n      <Mesh3d transform={{Transform.fromXyz({bx:.3}, 0.002, {blz:.3}).withRotation(FLAT)}}>\n        <Text3D size={{{fs:.3}}} depth={{0.001}} read={{false}}>{bar_label}</Text3D>\n        <StandardMaterial base_color=\"#202020\" />\n      </Mesh3d>\n{parts}    </AbsoluteFill>\n  );\n}}\n\nregisterRoot(() => <Composition id=\"lineup\" component={{Lineup}} width={{1920}} height={{1080}} fps={{30}} durationInFrames={{1}} />);\n",
        ty = highest * 0.35,
        tz = bz * 0.45,
        // a floor past every edge of the frame (the review's views leave a ground slab out of their framing)
        g = (total * 3.0).max(2.0),
        bh = bar / 2.0,
        bt = fs * 0.25,
        bx = -total / 2.0 + bar / 2.0,
        blz = bz + fs * 1.3,
    )
}

/// The scale bar for a set this wide: the longest round length that fits under it, and its label.
pub fn scale_bar(width: f64) -> (f64, &'static str) {
    [(1.0, "1 m"), (0.5, "50 cm"), (0.25, "25 cm"), (0.1, "10 cm"), (0.05, "5 cm")].into_iter().find(|(l, _)| *l <= width.max(0.05)).unwrap_or((0.05, "5 cm"))
}

/// Where a kit lands: `--to`, else the workspace's shared/components/KIT (every project imports it as
/// @shared/components/KIT), else the project's components/KIT.
pub fn kit_dir(dir: &Path, kit: &str, to: Option<&str>) -> PathBuf {
    if let Some(t) = to {
        return PathBuf::from(t);
    }
    if let Some(ws) = raea_core_facts::record::up(dir, ".raea/workspace") {
        let shared = if !ws.join("shared").is_dir() && ws.join("components").is_dir() { ws.join("components") } else { ws.join("shared/components") };
        return shared.join(kit);
    }
    let project = raea_core_facts::record::up(dir, ".raea/project").unwrap_or_else(|| dir.to_path_buf());
    project.join("components").join(kit)
}

fn copy(a: &Path, b: &Path) -> Result<(), String> {
    match std::fs::copy(a, b) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("{} -> {}: {e}", a.display(), b.display())),
    }
}

/// The kit landed: each prepped object's model, LOD and sidecars, its component and thumb; the lineup and its cover
/// render; kit.json. Objects not prepped yet are named, not skipped silently.
pub fn land(dir: &Path, m: &Make, name: Option<&str>, to: Option<&str>) -> Result<String, String> {
    let kit = name.map(str::to_string).unwrap_or_else(|| dir.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or("kit".into()));
    let dest = kit_dir(dir, &kit, to);
    std::fs::create_dir_all(dest.join("thumbs")).map_err(|e| format!("{}: {e}", dest.display()))?;
    let p = picks(dir);
    let (mut items, mut missing) = (Vec::new(), Vec::new());
    for o in &m.objects {
        let from = dir.join("out/kit").join(&o.id);
        let (full, lod) = (from.join(format!("{}.glb", o.id)), from.join(format!("{}_lod1.glb", o.id)));
        if !full.is_file() || !lod.is_file() {
            missing.push(o.id.clone());
            continue;
        }
        for f in [&full, &lod] {
            let name = f.file_name().unwrap();
            copy(f, &dest.join(name))?;
            let side = f.with_extension("glb.json");
            if side.is_file() {
                copy(&side, &dest.join(format!("{}.json", name.to_string_lossy())))?;
            }
        }
        let why = p.get(&o.id).and_then(|v| v["why"].as_str()).unwrap_or("");
        let n = component_name(&o.id);
        std::fs::write(dest.join(format!("{n}.tsx")), component(o, why)).map_err(|e| e.to_string())?;
        let (colour, _) = draw(&full, frame_size(o))?;
        copy(&colour, &dest.join("thumbs").join(format!("{}.png", o.id)))?;
        std::fs::create_dir_all(dir.join("out/kit/thumbs")).map_err(|e| e.to_string())?;
        copy(&colour, &dir.join("out/kit/thumbs").join(format!("{}.png", o.id)))?;
        items.push(json!({"id": o.id, "label": o.label, "component": format!("{n}.tsx"), "model": format!("{}.glb", o.id), "lod1": format!("{}_lod1.glb", o.id),
            "thumb": format!("thumbs/{}.png", o.id), "h": tall(o), "pick": p.get(&o.id).cloned().unwrap_or(Value::Null)}));
    }
    if !missing.is_empty() {
        return Err(format!("raea make kit: not prepped yet: {} (raea make prep --only {})", missing.join(", "), missing.join(",")));
    }
    let lineup_tsx = dest.join("lineup.tsx");
    std::fs::write(&lineup_tsx, lineup(m, &kit)).map_err(|e| e.to_string())?;
    let cover = dest.join("cover.png");
    raea(&["render".into(), lineup_tsx.display().to_string(), "-o".into(), cover.display().to_string(), "--frame".into(), "0".into()], &[])?;
    let doc = json!({"kit": kit, "style": m.style, "references": m.references, "items": items, "lineup": "lineup.tsx", "cover": "cover.png", "landed": dest.display().to_string(), "made": {"by": "raea make kit (docs/make.feature MK13)"}});
    let text = serde_json::to_string_pretty(&doc).unwrap_or_default();
    std::fs::write(dest.join("kit.json"), &text).map_err(|e| e.to_string())?;
    // the job's own record of it, what `raea make status` counts (core/brief deliverables)
    std::fs::write(dir.join("out/kit/kit.json"), &text).map_err(|e| e.to_string())?;
    copy(&cover, &dir.join("out/kit/lineup.png"))?;
    Ok(format!("landed the {kit} kit ({} pieces) in {}: components, lineup.tsx, cover.png, thumbs/, kit.json", items.len(), dest.display()))
}
