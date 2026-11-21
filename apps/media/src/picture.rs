//! `media still` and `media sheet`: frames as images.

use super::*;

pub(crate) fn to_image(f: Frame) -> R<image::RgbaImage> {
    image::RgbaImage::from_raw(f.width, f.height, f.rgba).ok_or_else(|| "a decoded frame had the wrong size".to_string())
}

pub(crate) fn save(img: &image::RgbaImage, out: &Path) -> R<()> {
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let r = match ext(out).as_str() {
        "jpg" | "jpeg" => image::DynamicImage::ImageRgba8(img.clone()).to_rgb8().save(out),
        _ => img.save(out),
    };
    r.map_err(|e| format!("{}: {e}", out.display()))
}

/// The frame showing at `t` (the last frame when `t` is past the end).
pub(crate) fn frame(path: &Path, t: f64) -> R<image::RgbaImage> {
    let mut d = open_video(path)?;
    let dur = d.info().duration;
    let t = t.clamp(0.0, (dur - 0.5 / d.info().fps.max(1.0)).max(0.0));
    to_image(d.frame_at(t)?)
}

/// The image made `width` wide (its height in proportion) when it's wider; never made bigger.
fn narrowed(img: image::RgbaImage, width: Option<u32>) -> image::RgbaImage {
    match width.filter(|w| *w > 0 && *w < img.width()) {
        Some(w) => image::imageops::resize(&img, w, ((img.height() as f64 * w as f64 / img.width() as f64).round() as u32).max(1), image::imageops::FilterType::CatmullRom),
        None => img,
    }
}

/// `width`: the still made that wide (its height in proportion) when the picture is wider; never made bigger.
pub fn still(path: &Path, at: f64, out: &Path, width: Option<u32>) -> R<Value> {
    let img = narrowed(if IMAGE_EXTS.contains(&ext(path).as_str()) { image::open(path).map_err(|e| format!("{}: {e}", path.display()))?.to_rgba8() } else { frame(path, at)? }, width);
    save(&img, out)?;
    Ok(json!({"wrote": out, "at": at, "width": img.width(), "height": img.height()}))
}

/// A still at each of `ats` (seconds, comma-separated), into the folder `out` as 0.jpg, 1.jpg ... (a videoform's shots)
pub fn stills(path: &Path, ats: &str, out: &Path, width: Option<u32>) -> R<Value> {
    let mut n = 0;
    for a in ats.split(',').map(str::trim).filter(|a| !a.is_empty()) {
        let at: f64 = a.parse().map_err(|_| format!("raea media still --at: {a} isn't a time"))?;
        save(&narrowed(frame(path, at)?, width), &out.join(format!("{n}.jpg")))?;
        n += 1;
    }
    Ok(json!({"wrote": out, "stills": n}))
}

pub fn sheet(files: &[PathBuf], n: usize, w: u32, out: &Path) -> R<Value> {
    if files.is_empty() {
        return Err("raea media sheet FILE... -o OUT: which files?".into());
    }
    let mut tiles = Vec::new();
    if files.len() == 1 {
        let p = probe_of(&files[0])?;
        for i in 0..n.max(1) {
            let t = (i as f64 + 0.5) / n.max(1) as f64 * p.duration;
            tiles.push((t, frame(&files[0], t)?));
        }
    } else {
        for f in files {
            let p = probe_of(f)?;
            let img = if p.kind == "image" { image::open(f).map_err(|e| format!("{}: {e}", f.display()))?.to_rgba8() } else { frame(f, p.duration / 2.0)? };
            tiles.push((p.duration / 2.0, img));
        }
    }
    let th = tiles.iter().map(|(_, i)| (i.height() as f64 * w as f64 / i.width() as f64).round() as u32).max().unwrap_or(w);
    let cols = tiles.len().min(4) as u32;
    let rows = (tiles.len() as u32).div_ceil(cols);
    let mut sheet = image::RgbaImage::from_pixel(cols * w, rows * th, image::Rgba([0, 0, 0, 255]));
    for (k, (_, img)) in tiles.iter().enumerate() {
        let h = (img.height() as f64 * w as f64 / img.width() as f64).round() as u32;
        let small = image::imageops::resize(img, w, h.max(1), image::imageops::FilterType::Triangle);
        image::imageops::overlay(&mut sheet, &small, ((k as u32 % cols) * w) as i64, ((k as u32 / cols) * th) as i64);
    }
    save(&sheet, out)?;
    // OUT.labels.json beside it (docs/callouts.feature CO2c): each cell says what it is, so raea show OUT.png labels them
    let heights: Vec<u32> = tiles.iter().map(|(_, i)| (i.height() as f64 * w as f64 / i.width() as f64).round().max(1.0) as u32).collect();
    let texts: Vec<String> = if files.len() == 1 { tiles.iter().map(|(t, _)| clock(*t)).collect() } else { files.iter().map(|f| f.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect() };
    let labels = out.with_extension("labels.json");
    std::fs::write(&labels, serde_json::to_string_pretty(&json!({"labels": cell_labels(&texts, &heights, w, th, cols, rows)})).unwrap_or_default()).map_err(|e| format!("{}: {e}", labels.display()))?;
    Ok(json!({"wrote": out, "labels": labels, "tiles": tiles.len(), "at": tiles.iter().map(|(t, _)| round3(*t)).collect::<Vec<_>>(), "width": sheet.width(), "height": sheet.height()}))
}

/// Each cell's label: its text and its box in the sheet's own 0-1 (a cell `w` wide, its picture `heights[k]` tall in a row
/// `th` tall), in the sheet's order, left to right then down.
fn cell_labels(texts: &[String], heights: &[u32], w: u32, th: u32, cols: u32, rows: u32) -> Vec<Value> {
    let (sw, sh) = ((cols * w) as f64, (rows * th) as f64);
    let r4 = |v: f64| (v * 1e4).round() / 1e4;
    texts.iter().zip(heights).enumerate().map(|(k, (text, h))| {
        let (x, y) = (((k as u32 % cols) * w) as f64, ((k as u32 / cols) * th) as f64);
        json!({"text": text, "box": [r4(x / sw), r4(y / sh), r4((x + w as f64) / sw), r4((y + *h as f64) / sh)]})
    }).collect()
}

/// A moment as a cell's tag: 0:03.2, 1:05.0
fn clock(t: f64) -> String {
    let d = (t.max(0.0) * 10.0).round() as u64;
    format!("{}:{:04.1}", d / 600, (d % 600) as f64 / 10.0)
}

#[cfg(test)]
mod sheet_tests {
    use super::*;

    #[test]
    fn a_sheets_cells_are_labelled_by_their_boxes() {
        // five cells, four across: the second row's one cell, a shorter picture than its row
        let texts: Vec<String> = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
        let l = cell_labels(&texts, &[135, 135, 135, 135, 90], 240, 135, 4, 2);
        assert_eq!(l[0], json!({"text": "a", "box": [0.0, 0.0, 0.25, 0.5]}));
        assert_eq!(l[3], json!({"text": "d", "box": [0.75, 0.0, 1.0, 0.5]}));
        assert_eq!(l[4], json!({"text": "e", "box": [0.0, 0.5, 0.25, 0.8333]}));
    }

    #[test]
    fn a_moment_reads_as_a_clock() {
        assert_eq!(clock(3.24), "0:03.2");
        assert_eq!(clock(59.96), "1:00.0");
        assert_eq!(clock(65.0), "1:05.0");
    }
}

// ---- sound
