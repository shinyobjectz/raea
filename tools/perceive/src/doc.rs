//! `raea-perceive read-doc FILE...`: a document's words, page by page (docs/videoform.feature VF8f). A PDF's pages are
//! drawn by CoreGraphics and read by Vision's text recognition, as a picture is, so a page that is a picture reads as
//! well as one set in type; a still (a brief sent as a screenshot) is read the same. Each line keeps its box and its
//! height, which says a heading from body text. One JSON line a file: {file, pages: [{lines: [{text, at, h}]}]}.

use serde_json::{Value, json};
use std::path::Path;

/// A page this wide, pixels: body type at 10 pt is about 25 px high, which Vision reads well
const WIDTH: usize = 1700;
/// The least a line is of the page's height: a footnote's type
const LEAST: f32 = 0.004;

fn page(rgba: &[u8], w: usize, h: usize) -> Result<Value, String> {
    let lines = raea_sys_vision::vision::text_lines_from(rgba, w, h, false, LEAST)?;
    // top to bottom, then left to right; a line's height in pixels of a WIDTH-wide page, so pages compare
    let mut ls = lines;
    ls.retain(|(t, ..)| !t.trim().is_empty());
    ls.sort_by(|a, b| (a.2[1] + a.2[3] / 2.0).total_cmp(&(b.2[1] + b.2[3] / 2.0)).then(a.2[0].total_cmp(&b.2[0])));
    let px = h as f64 * WIDTH as f64 / w as f64;
    Ok(json!({"lines": ls.iter().map(|(t, _, b)| json!({"text": t.trim(), "at": b, "h": (b[3] * px * 10.0).round() / 10.0})).collect::<Vec<_>>()}))
}

pub fn run(files: &[&String]) -> Result<(), String> {
    if files.is_empty() {
        return Err("read-doc FILE...".into());
    }
    for f in files {
        let p = Path::new(f.as_str());
        let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let pages: Result<Vec<Value>, String> = if ext == "pdf" {
            let mut out = Vec::new();
            for (px, w, h) in raea_sys_vision::pdf::pages(p, WIDTH)? {
                out.push(page(&px, w, h)?);
            }
            Ok(out)
        } else {
            let img = image::open(p).map_err(|e| format!("{f}: {e}"))?.to_rgba8();
            let (w, h) = (img.width() as usize, img.height() as usize);
            page(img.as_raw(), w, h).map(|v| vec![v])
        };
        println!("{}", json!({"file": f, "pages": pages?}));
    }
    Ok(())
}
