//! The plan stop (docs/make.feature MK18, from AnyScene's endpoint/floorplan.py, MIT): before anything is generated,
//! the plan drawn so a person can approve it. A floor plan from above (every object's footprint at its `at`, turned by
//! its `rot`, labelled; invented ones dashed, so what came from the reference and what was added are told apart), a
//! grey-box blockout of the same plan rendered from the entrance and through `raea render --review`, and the
//! reference beside the blockout on one comparison sheet.
//!
//!   raea make plan [DIR]   -> review/plan/floor-plan.png, blockout.tsx, blockout.png, comparison.png, colour/clay sheets

use std::path::{Path, PathBuf};

use raea_core_brief::make::{Make, Object, Origin};

use crate::sheet::{Tile, esc, sheet, svg_png};
use crate::shots::raea;

/// An object's footprint, metres: [width, depth] (its box, else a square about as wide as it's tall).
pub fn footprint(o: &Object) -> [f64; 2] {
    if let Some([w, d, _]) = o.dims {
        return [w, d];
    }
    let s = o.max.unwrap_or_else(|| o.h.unwrap_or(0.5) * 0.6).max(0.05);
    [s, s]
}

fn height(o: &Object) -> f64 {
    o.dims.map(|d| d[2]).or(o.h).unwrap_or(0.05).max(0.01)
}

/// The placed objects and the floor they span: (objects with an `at`, [x0, z0, x1, z1] with half a metre around).
pub fn extent(m: &Make) -> (Vec<&Object>, [f64; 4]) {
    let placed: Vec<&Object> = m.objects.iter().filter(|o| o.at.is_some()).collect();
    let mut b = [0.0f64, 0.0, 1.0, 1.0];
    for o in &placed {
        let at = o.at.unwrap();
        let [w, d] = footprint(o);
        let r = 0.5 * (w * w + d * d).sqrt();
        b = [b[0].min(at[0] - r), b[1].min(at[2] - r), b[2].max(at[0] + r), b[3].max(at[2] + r)];
    }
    (placed, [b[0] - 0.5, b[1] - 0.5, b[2] + 0.5, b[3] + 0.5])
}

/// The floor plan as SVG: the entrance (z = 0) at the bottom, a metre grid, each footprint turned and labelled.
pub fn floor_svg(m: &Make) -> Result<String, String> {
    let (placed, [x0, z0, x1, z1]) = extent(m);
    if placed.is_empty() {
        return Err("raea make plan: no object has an `at` (where it stands, in metres: x right, z away from the entrance)".into());
    }
    let px = (1400.0 / (x1 - x0).max(z1 - z0)).min(260.0);
    let (w, h) = (((x1 - x0) * px).ceil() as u32 + 40, ((z1 - z0) * px).ceil() as u32 + 130);
    // text sized to the plan, so it still reads once the plan is a third of a comparison sheet
    let t = (w as f64 * 0.02).max(16.0);
    // plan x right, z away from the entrance: up the page
    let sx = |x: f64| (x - x0).mul_add(px, 20.0);
    let sy = |z: f64| 70.0 + (z1 - z) * px;
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\"><rect width=\"100%\" height=\"100%\" fill=\"#f4f2ee\"/>");
    s += &format!("<text x=\"20\" y=\"{:.0}\" font-size=\"{:.0}\" font-weight=\"bold\" fill=\"#222\">Floor plan: {} objects (solid: from the reference; dashed: invented)</text>", t * 1.6, t * 1.1, placed.len());
    let mut g = x0.ceil();
    while g <= x1 {
        s += &format!("<line x1=\"{0:.1}\" y1=\"{1:.1}\" x2=\"{0:.1}\" y2=\"{2:.1}\" stroke=\"#dedad2\" stroke-width=\"1\"/>", sx(g), sy(z1), sy(z0));
        g += 1.0;
    }
    let mut g = z0.ceil();
    while g <= z1 {
        s += &format!("<line x1=\"{1:.1}\" y1=\"{0:.1}\" x2=\"{2:.1}\" y2=\"{0:.1}\" stroke=\"#dedad2\" stroke-width=\"1\"/>", sy(g), sx(x0), sx(x1));
        g += 1.0;
    }
    // the entrance: the front edge, and where the camera looks in from
    s += &format!("<line x1=\"{0:.1}\" y1=\"{1:.1}\" x2=\"{2:.1}\" y2=\"{1:.1}\" stroke=\"#3a7bd5\" stroke-width=\"4\"/>", sx(x0), sy(z0), sx(x1));
    s += &format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.0}\" fill=\"#3a7bd5\" text-anchor=\"middle\">entrance: the camera looks in from here</text>", (sx(x0) + sx(x1)) / 2.0, sy(z0) + t * 1.3, t * 0.9);
    for o in &placed {
        let at = o.at.unwrap();
        let [fw, fd] = footprint(o);
        let invented = o.source == Some(Origin::Invented);
        let dash = if invented { " stroke-dasharray=\"8 5\"" } else { "" };
        let fill = if o.hero == Some(true) { "#f2c46d" } else if invented { "#ece8f4" } else { "#d9e4d2" };
        s += &format!(
            "<g transform=\"translate({:.1} {:.1}) rotate({:.1})\"><rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{fill}\" stroke=\"#333\" stroke-width=\"2\"{dash}/><line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"{:.1}\" stroke=\"#333\" stroke-width=\"2\"/></g>",
            sx(at[0]),
            sy(at[2]),
            -o.rot.unwrap_or(0.0),
            -fw * px / 2.0,
            -fd * px / 2.0,
            fw * px,
            fd * px,
            fd * px / 2.0
        );
        s += &format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.0}\" fill=\"#111\" text-anchor=\"middle\">{}</text>", sx(at[0]), sy(at[2]) - t * 0.25, t, esc(&o.label));
        s += &format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.0}\" fill=\"#555\" text-anchor=\"middle\">{:.2} m</text>", sx(at[0]), sy(at[2]) + t * 0.9, t * 0.8, height(o));
    }
    s += &format!("<line x1=\"20\" y1=\"{0}\" x2=\"{1:.1}\" y2=\"{0}\" stroke=\"#111\" stroke-width=\"5\"/><text x=\"20\" y=\"{2:.0}\" font-size=\"{3:.0}\" fill=\"#111\">1 m</text>", h - 18, 20.0 + px, h as f64 - 26.0, t * 0.9);
    s += "</svg>";
    Ok(s)
}

/// The blockout: a grey box for each placed object at its place and turn on a floor, seen from the entrance.
pub fn blockout_tsx(m: &Make) -> String {
    let (placed, [x0, z0, x1, z1]) = extent(m);
    let mut boxes = String::new();
    for o in &placed {
        let at = o.at.unwrap();
        let [w, d] = footprint(o);
        let h = height(o);
        let colour = if o.source == Some(Origin::Invented) { "#b4b0bd" } else { "#bdbdb8" };
        let a = -o.rot.unwrap_or(0.0).to_radians() / 2.0;
        boxes += &format!(
            "      <Mesh3d transform={{Transform.fromXyz({:.3}, {:.3}, {:.3}).withRotation([0, {:.5}, 0, {:.5}])}}>\n        <Cuboid half_size={{[{:.3}, {:.3}, {:.3}]}} />\n        <StandardMaterial base_color=\"{colour}\" perceptual_roughness={{0.9}} />\n      </Mesh3d>\n",
            at[0],
            at[1] + h / 2.0,
            at[2],
            a.sin(),
            a.cos(),
            w / 2.0,
            h / 2.0,
            d / 2.0
        );
    }
    let (cx, cz) = ((x0 + x1) / 2.0, (z0 + z1) / 2.0);
    let (fw, fd) = ((x1 - x0) / 2.0, (z1 - z0) / 2.0);
    format!(
        "// written by raea make plan: the plan as grey boxes, seen from the entrance (approve it before anything is made)\nimport {{ AbsoluteFill, Composition, registerRoot }} from \"raea\";\nimport {{ StandardMaterial }} from \"raea/pbr\";\nimport {{ Cuboid, Mesh3d }} from \"raea/mesh\";\nimport {{ AmbientLight, DirectionalLight }} from \"raea/light\";\nimport {{ Transform }} from \"raea/transform\";\nimport {{ Camera3d }} from \"raea/camera\";\n\nfunction Blockout() {{\n  return (\n    <AbsoluteFill style={{{{ background: \"#cfd3d6\" }}}}>\n      <Camera3d transform={{Transform.fromXyz({cx:.3}, 1.6, {ez:.3}).lookingAt([{cx:.3}, 0.9, {cz:.3}])}} projection={{{{ Perspective: {{ fov: (50 * Math.PI) / 180 }} }}}} />\n      <AmbientLight brightness={{350}} />\n      <DirectionalLight transform={{Transform.fromXyz({lx:.3}, 6, -2).lookingAt([{cx:.3}, 0, {cz:.3}])}} illuminance={{8000}} shadow_maps_enabled />\n      <Mesh3d transform={{Transform.fromXyz({cx:.3}, -0.01, {cz:.3})}}>\n        <Cuboid half_size={{[{fw:.3}, 0.01, {fd:.3}]}} />\n        <StandardMaterial base_color=\"#e6e3dc\" perceptual_roughness={{0.95}} />\n      </Mesh3d>\n{boxes}    </AbsoluteFill>\n  );\n}}\n\nregisterRoot(() => <Composition id=\"blockout\" component={{Blockout}} width={{1280}} height={{720}} fps={{30}} durationInFrames={{1}} />);\n",
        ez = z0 - 0.4,
        lx = cx + 2.0,
    )
}

pub fn plan_dir(dir: &Path) -> PathBuf {
    dir.join("review/plan")
}

/// The plan stop's pictures: floor plan, blockout (and its review sheets), and the comparison with reference 0.
pub fn draw_plan(dir: &Path, m: &Make, review: bool) -> Result<String, String> {
    let out = plan_dir(dir);
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let floor = out.join("floor-plan.png");
    svg_png(&floor_svg(m)?)?.save(&floor).map_err(|e| format!("{}: {e}", floor.display()))?;
    let tsx = out.join("blockout.tsx");
    std::fs::write(&tsx, blockout_tsx(m)).map_err(|e| format!("{}: {e}", tsx.display()))?;
    let shot = out.join("blockout.png");
    raea(&["render".into(), tsx.display().to_string(), "-o".into(), shot.display().to_string(), "--frame".into(), "0".into()], &[])?;
    let mut said = vec![format!("wrote {}", floor.display()), format!("wrote {}", shot.display())];
    if review {
        raea(&["render".into(), tsx.display().to_string(), "--review".into(), out.display().to_string()], &[])?;
        said.push(format!("review sheets of the blockout in {}", out.display()));
    }
    let mut row = Vec::new();
    if let Some(r) = m.references.first() {
        row.push(Tile { img: dir.join(r), label: "reference".into() });
    }
    row.push(Tile { img: shot, label: "blockout from the entrance".into() });
    row.push(Tile { img: floor, label: "floor plan".into() });
    let cmp = out.join("comparison.png");
    sheet("Plan: the reference, the blockout from the entrance, the floor plan", &[row], 640, &cmp)?;
    said.push(format!("wrote {}: approve the plan (positions, sizes, what's invented) before any generation", cmp.display()));
    Ok(said.join("\n"))
}
