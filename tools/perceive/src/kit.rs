//! raea-perceive kit FILE...: what each of a brand's assets is (docs/videoform.feature VF9b), a still or a clip's middle
//! frame: its size, how much is see-through, the faces, person and words Vision sees in it, whether it's a screenshot,
//! and its commonest colours. One JSON line a file, {"file", "seen"} (raea_sys_videoform_kit::Seen), or {"file", "error"}.

use raea_sys_videoform_kit::Seen;
use raea_sys_vision::vision::Vision;
use serde_json::json;
use std::collections::HashMap;

/// The longest side Vision is shown
const SIDE: u32 = 1600;
/// The colours are counted on a copy this wide, in bins this many levels a channel apart
const COUNT: u32 = 64;
const BIN: u8 = 32;
/// The commonest colours kept
const COLOURS: usize = 5;

/// To three places
fn three(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn picture(path: &str) -> Result<(image::RgbaImage, Option<f64>), String> {
    if let Ok(img) = image::open(path) {
        return Ok((img.to_rgba8(), None));
    }
    let mut d = raea_io_media::backend("apple")?.open_video(std::path::Path::new(path))?;
    let len = d.info().duration;
    let f = d.frame_at(len / 2.0)?;
    let img = image::RgbaImage::from_raw(f.width, f.height, f.rgba).ok_or("a frame of the wrong size")?;
    Ok((img, Some(len)))
}

fn look(v: &dyn Vision, path: &str) -> Result<Seen, String> {
    let (img, seconds) = picture(path)?;
    seen(v, &img, seconds)
}

/// What a picture is, as a kit's asset is read: a file's, or a layer's crop of a reference's frame (VF9d)
pub(crate) fn seen(v: &dyn Vision, img: &image::RgbaImage, seconds: Option<f64>) -> Result<Seen, String> {
    let (w, h) = img.dimensions();
    let scale = |side: u32| {
        let k = side as f64 / w.max(h) as f64;
        ((w as f64 * k).round().max(1.0) as u32, (h as f64 * k).round().max(1.0) as u32)
    };
    let shown = if w.max(h) > SIDE {
        let (a, b) = scale(SIDE);
        image::imageops::resize(img, a, b, image::imageops::FilterType::Triangle)
    } else {
        img.clone()
    };
    let x = v.see(shown.as_raw(), shown.width() as usize, shown.height() as usize)?;
    let (a, b) = scale(COUNT);
    let small = image::imageops::resize(img, a, b, image::imageops::FilterType::Triangle);
    let mut bins: HashMap<[u8; 3], usize> = HashMap::new();
    let (mut solid, mut clear) = (0usize, 0usize);
    for p in small.pixels() {
        if p[3] < 128 {
            clear += 1;
            continue;
        }
        solid += 1;
        *bins.entry([p[0] / BIN, p[1] / BIN, p[2] / BIN]).or_default() += 1;
    }
    let mut colours: Vec<([u8; 3], usize)> = bins.into_iter().collect();
    colours.sort_by_key(|&(c, n)| (std::cmp::Reverse(n), c));
    let b4 = |q: &raea_sys_layout::layout::Rect| crate::layers::b4(q).map(three);
    Ok(Seen {
        w,
        h,
        clear: three(clear as f64 / (solid + clear).max(1) as f64),
        faces: x.faces.iter().map(b4).collect(),
        person: three(if x.mask.is_empty() { 0.0 } else { x.mask.iter().filter(|&&m| m > 127).count() as f64 / x.mask.len() as f64 }),
        text: x.lines.iter().map(|(q, t)| (t.clone(), b4(q))).collect(),
        screen: x.screenshot,
        colours: colours.into_iter().take(COLOURS).map(|(c, n)| (format!("#{:02x}{:02x}{:02x}", c[0] * BIN + BIN / 2, c[1] * BIN + BIN / 2, c[2] * BIN + BIN / 2), three(n as f64 / solid.max(1) as f64))).collect(),
        seconds,
    })
}

pub(crate) fn run(files: &[&String]) -> Result<(), String> {
    let v = raea_sys_vision::vision::AppleVision::default();
    for f in files {
        match look(&v, f) {
            Ok(s) => println!("{}", json!({"file": f, "seen": s})),
            Err(e) => println!("{}", json!({"file": f, "error": e})),
        }
    }
    Ok(())
}
