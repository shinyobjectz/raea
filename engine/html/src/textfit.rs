//! Text that doesn't fit its component (the founder, 2026-10-15: text running past a card, and layouts where most of a row
//! of small cards wraps). Reads a frame's text report (Scene::render_text_boxes: each text's box, its lines, its
//! component and how far it spills past that component's inner edge) and returns findings, each with the region of the
//! frame to show (a crop the agent or a person can look at):
//!   - error: text spills past its component (a card, a pill, a button) by 2 px or more (up or down: beyond a quarter
//!     of a line, the font's ascent and descent)
//!   - warning: crowded wrapping: in a row of three or more like components (same size, same top), half or more break
//!     a short label up (two lines or more at two words a line or fewer; a paragraph wraps by design)
//! The fix is a redesign (wider components, fewer to a row, shorter words), not a nudge, so they're reported together.

use serde_json::{json, Value};

fn num(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

fn rect(v: &Value) -> [f64; 4] {
    [num(v, "x"), num(v, "y"), num(v, "w"), num(v, "h")]
}

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1]));
    let (x1, y1) = ((a[0] + a[2]).max(b[0] + b[2]), (a[1] + a[3]).max(b[1] + b[3]));
    [x0, y0, x1 - x0, y1 - y0]
}

fn name(c: &Value) -> String {
    let id = c.get("id").and_then(Value::as_str).unwrap_or("");
    let class = c.get("class").and_then(Value::as_str).unwrap_or("");
    let tag = c.get("tag").and_then(Value::as_str).unwrap_or("div");
    if !id.is_empty() {
        format!("{tag}#{id}")
    } else if !class.is_empty() {
        format!("{tag}.{}", class.split_whitespace().next().unwrap_or(""))
    } else {
        tag.to_string()
    }
}

/// The findings for one frame's text report: {"errors": [...], "warnings": [...], "regions": [{x, y, w, h, finding}]}.
pub fn findings(report: &Value) -> Value {
    let boxes = report.get("boxes").and_then(Value::as_array).cloned().unwrap_or_default();
    let t = num(report, "t");
    let (mut errors, mut warnings, mut regions) = (Vec::new(), Vec::new(), Vec::new());
    for b in &boxes {
        let comp = &b["component"];
        let Some(spills) = b.get("spills").and_then(Value::as_object) else { continue };
        if comp.is_null() || spills.is_empty() {
            continue;
        }
        // a line box reaches past its letters by the font's ascent and descent: a big display number's box crossed its
        // card's top by 6 px while the ink sat well inside (a graded deck, 2026-10-15); so a spill up or down counts past
        // a quarter of a line's height, left or right as it is
        let line = num(b, "h") / num(b, "lines").max(1.0);
        let worst = spills
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_f64()? - if k == "top" || k == "bottom" { 0.25 * line } else { 0.0 })))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some((side, px)) = worst else { continue };
        // two pixels: under that it's antialiasing and subpixel layout (a 1.2 px "spill" is invisible)
        if px < 2.0 {
            continue;
        }
        let px = spills.get(&side).and_then(Value::as_f64).unwrap_or(px);
        let msg = format!(
            "at {t:.2}s \"{}\" spills {px:.0} px past the {side} edge of its {} ({:.0} x {:.0} px): give it room (a wider component, smaller or shorter text) or let it wrap inside",
            b["text"].as_str().unwrap_or(""),
            name(comp),
            num(comp, "w"),
            num(comp, "h")
        );
        regions.push(json!({"region": union(rect(comp), rect(b)), "finding": msg}));
        errors.push(msg);
    }
    // rows of like components: the same size and top (within 2 px), three or more
    let mut comps: Vec<(Value, Vec<&Value>)> = Vec::new();
    for b in &boxes {
        let c = &b["component"];
        if c.is_null() {
            continue;
        }
        match comps.iter_mut().find(|(k, _)| k == c) {
            Some((_, texts)) => texts.push(b),
            None => comps.push((c.clone(), vec![b])),
        }
    }
    let mut used = vec![false; comps.len()];
    for i in 0..comps.len() {
        if used[i] {
            continue;
        }
        let a = rect(&comps[i].0);
        let row: Vec<usize> = (0..comps.len())
            .filter(|&j| !used[j] && { let b = rect(&comps[j].0); (a[1] - b[1]).abs() <= 2.0 && (a[2] - b[2]).abs() <= 2.0 && (a[3] - b[3]).abs() <= 2.0 })
            .collect();
        for &j in &row {
            used[j] = true;
        }
        if row.len() < 3 {
            continue;
        }
        // a short label breaking up, two words a line or fewer (the cramped cards: one); a paragraph wraps by design (a
        // graded deck's concept cards: about seven)
        let wraps = |texts: &Vec<&Value>| texts.iter().any(|b| {
            let (lines, words) = (num(b, "lines"), num(b, "words"));
            lines >= 2.0 && words / lines <= 2.0
        });
        let wrapped: Vec<usize> = row.iter().copied().filter(|&j| wraps(&comps[j].1)).collect();
        if wrapped.len() * 2 < row.len() {
            continue;
        }
        let region = row.iter().map(|&j| rect(&comps[j].0)).reduce(union).unwrap();
        let msg = format!(
            "at {t:.2}s text wraps in {} of a row of {} {} ({:.0} px wide): the row is too crowded for its words; fewer per row, wider ones, or shorter text",
            wrapped.len(),
            row.len(),
            name(&comps[row[0]].0),
            a[2]
        );
        regions.push(json!({"region": region, "finding": msg}));
        warnings.push(msg);
    }
    json!({"errors": errors, "warnings": warnings, "regions": regions})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, x: f64) -> Value {
        json!({"id": id, "tag": "div", "x": x, "y": 100.0, "w": 150.0, "h": 105.0})
    }

    #[test]
    fn spills_are_errors_and_crowded_rows_warnings() {
        let mut boxes = vec![json!({"text": "FULFILLMENT", "words": 1, "lines": 1, "x": 180.0, "y": 160.0, "w": 145.0, "h": 20.0,
            "component": card("c1", 110.0), "spills": {"right": 67.2}})];
        for (i, (text, lines)) in [("Check the footage", 3), ("Edit the cuts", 3), ("Check the edit", 3), ("Film", 1)].iter().enumerate() {
            boxes.push(json!({"text": text, "words": text.split(' ').count(), "lines": lines, "x": 300.0 + 170.0 * i as f64, "y": 130.0,
                "w": 70.0, "h": 78.0, "component": card(&format!("c{}", i + 2), 280.0 + 170.0 * i as f64), "spills": {}}));
        }
        let f = findings(&json!({"t": 0.5, "boxes": boxes}));
        assert_eq!(f["errors"].as_array().unwrap().len(), 1, "{f}");
        assert!(f["errors"][0].as_str().unwrap().contains("67 px past the right edge of its div#c1"));
        assert_eq!(f["warnings"].as_array().unwrap().len(), 1, "{f}");
        assert!(f["warnings"][0].as_str().unwrap().contains("3 of a row of 5"));
        assert_eq!(f["regions"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_roomy_layout_is_clean() {
        let boxes = vec![json!({"text": "Check the footage, then the edit", "words": 6, "lines": 1, "x": 130.0, "y": 276.0, "w": 313.0, "h": 26.0,
            "component": {"id": "wide", "tag": "div", "x": 110.0, "y": 260.0, "w": 420.0, "h": 80.0}, "spills": {}})];
        let f = findings(&json!({"t": 0.5, "boxes": boxes}));
        assert!(f["errors"].as_array().unwrap().is_empty() && f["warnings"].as_array().unwrap().is_empty(), "{f}");
    }
}

/// Render each of `times` on `comp`, check its text against its components (`findings`) and crop each finding's region
/// (24 px around it) to `crop_dir` as PNG: {"errors", "warnings", "crops"}, findings repeated across frames kept once.
/// The one loop `raea lint --layout` and the C/Rust API (`api::layout_path`, the agent's render gate) both use.
pub fn check(stage: &mut crate::stage::Stage, times: &[f64], crop_dir: Option<&std::path::Path>) -> Result<Value, String> {
    let (w, h) = stage.comp().output_size();
    let scale = w as f64 / stage.comp().width.max(1) as f64;
    let (mut errors, mut warnings, mut crops) = (Vec::<String>::new(), Vec::<String>::new(), Vec::<String>::new());
    let mut seen = std::collections::HashSet::new();
    let mut buf = Vec::new();
    for &t in times {
        let report = stage.text_boxes(t, &mut buf, None);
        let tf = &report["textfit"];
        let key = |m: &str| m.split_once("s ").map(|x| x.1.to_string()).unwrap_or_else(|| m.to_string());
        for (list, kind) in [(&mut errors, "errors"), (&mut warnings, "warnings")] {
            for m in tf[kind].as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
                if !seen.insert(key(m)) {
                    continue;
                }
                list.push(m.to_string());
                let (Some(dir), Some(r)) = (crop_dir, tf["regions"].as_array().into_iter().flatten().find(|r| r["finding"].as_str() == Some(m))) else { continue };
                let g = |i: usize| r["region"][i].as_f64().unwrap_or(0.0) * scale;
                let pad = 24.0 * scale;
                let (x0, y0) = ((g(0) - pad).max(0.0) as u32, (g(1) - pad).max(0.0) as u32);
                let (x1, y1) = ((g(0) + g(2) + pad).min(w as f64) as u32, (g(1) + g(3) + pad).min(h as f64) as u32);
                if x1 <= x0 || y1 <= y0 || buf.len() < (w * h * 4) as usize {
                    continue;
                }
                let mut c = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
                for y in y0..y1 {
                    let row = ((y * w + x0) * 4) as usize;
                    c.extend_from_slice(&buf[row..row + ((x1 - x0) * 4) as usize]);
                }
                // premultiplied to straight alpha where the frame is see-through
                for p in c.chunks_exact_mut(4) {
                    let a = p[3] as u32;
                    if a != 0 && a != 255 {
                        for v in &mut p[..3] {
                            *v = ((*v as u32 * 255 + a / 2) / a).min(255) as u8;
                        }
                    }
                }
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                let path = dir.join(format!("textfit-{:02}-{t:.2}s.png", crops.len() + 1));
                image::save_buffer(&path, &c, x1 - x0, y1 - y0, image::ColorType::Rgba8).map_err(|e| format!("{}: {e}", path.display()))?;
                crops.push(path.display().to_string());
            }
        }
    }
    Ok(json!({"errors": errors, "warnings": warnings, "crops": crops}))
}

/// The times `check` looks at when none are given: every half second, at most 40 frames.
pub fn default_times(duration: f64) -> Vec<f64> {
    let d = duration.max(0.0);
    (0..((d / 0.5).ceil() as usize).clamp(1, 40)).map(|i| (i as f64 * 0.5).min((d - 0.01).max(0.0))).collect()
}
