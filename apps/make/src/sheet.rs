//! The contact sheets a gate looks at (docs/make.feature MK10, MK11, MK15): pictures in rows, each labelled under it,
//! a title over them, on one PNG a person, `raea ask` and the critic read at once. Labels are drawn by resvg in the
//! system's own fonts; an SVG drawn whole (the floor plan, MK18) goes through `svg_png`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use image::{Rgba, RgbaImage, imageops};
use resvg::{tiny_skia, usvg};

/// One picture on a sheet and what it is.
pub struct Tile {
    pub img: PathBuf,
    pub label: String,
}

const LABEL_H: u32 = 30;
const TITLE_H: u32 = 44;
const GAP: u32 = 6;

fn options() -> usvg::Options<'static> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let db = DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    });
    usvg::Options { fontdb: db.clone(), font_family: "Helvetica".into(), ..usvg::Options::default() }
}

/// SVG text escaped.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// An SVG document drawn at its own size.
pub fn svg_png(svg: &str) -> Result<RgbaImage, String> {
    let tree = usvg::Tree::from_str(svg, &options()).map_err(|e| format!("svg: {e}"))?;
    let size = tree.size().to_int_size();
    let mut pix = tiny_skia::Pixmap::new(size.width(), size.height()).ok_or("svg: empty size")?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pix.as_mut());
    // tiny-skia keeps premultiplied alpha; image wants it straight
    let mut img = RgbaImage::new(size.width(), size.height());
    for (p, c) in img.pixels_mut().zip(pix.pixels()) {
        let d = c.demultiply();
        *p = Rgba([d.red(), d.green(), d.blue(), d.alpha()]);
    }
    Ok(img)
}

/// Rows of tiles, each `tile_w` wide (its height from the first picture's shape; any other shape fitted inside it), a label under each and the title
/// over them all, written to `out`. A tile whose picture won't open is drawn grey, its label saying so.
pub fn sheet(title: &str, rows: &[Vec<Tile>], tile_w: u32, out: &Path) -> Result<(), String> {
    let first = rows.iter().flatten().find_map(|t| image::open(&t.img).ok()).ok_or_else(|| format!("{title}: no picture to put on the sheet"))?;
    let th = ((tile_w as f64) * first.height() as f64 / first.width().max(1) as f64).round().max(1.0) as u32;
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1) as u32;
    let (w, h) = (GAP + cols * (tile_w + GAP), TITLE_H + rows.len() as u32 * (th + LABEL_H + GAP));
    let mut img = RgbaImage::from_pixel(w, h, Rgba([18, 18, 20, 255]));
    let mut text = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\"><text x=\"{GAP}\" y=\"30\" font-size=\"22\" font-weight=\"bold\" fill=\"#ffffff\">{}</text>", esc(title));
    for (r, row) in rows.iter().enumerate() {
        for (c, t) in row.iter().enumerate() {
            let (x, y) = (GAP + c as u32 * (tile_w + GAP), TITLE_H + r as u32 * (th + LABEL_H + GAP));
            let label = match image::open(&t.img) {
                Ok(p) => {
                    // fitted inside the tile, its own shape kept, centred
                    let k = (tile_w as f64 / p.width().max(1) as f64).min(th as f64 / p.height().max(1) as f64);
                    let (pw, ph) = (((p.width() as f64 * k).round() as u32).max(1), ((p.height() as f64 * k).round() as u32).max(1));
                    let small = imageops::resize(&p.to_rgba8(), pw, ph, imageops::FilterType::Triangle);
                    imageops::overlay(&mut img, &small, (x + (tile_w - pw.min(tile_w)) / 2) as i64, (y + (th - ph.min(th)) / 2) as i64);
                    t.label.clone()
                }
                Err(_) => {
                    imageops::overlay(&mut img, &RgbaImage::from_pixel(tile_w, th, Rgba([70, 70, 70, 255])), x as i64, y as i64);
                    format!("{} (missing)", t.label)
                }
            };
            text.push_str(&format!("<text x=\"{}\" y=\"{}\" font-size=\"17\" fill=\"#e8e8e8\">{}</text>", x + 4, y + th + 21, esc(&label)));
        }
    }
    text.push_str("</svg>");
    imageops::overlay(&mut img, &svg_png(&text)?, 0, 0);
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    img.save(out).map_err(|e| format!("{}: {e}", out.display()))
}
