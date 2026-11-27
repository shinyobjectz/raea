//! Canvas 2D (docs/ts-native.feature P15): a `canvas` element's bitmap, drawn with tiny-skia from the display list
//! its CanvasRenderingContext2D records (sdk/ui/src/canvas.ts, the `__canvas2d` prop: {gen, w, h, ops}), and shown in
//! the element's box as an image (scaled to the box, smoothed, as Chrome shows a canvas whose box isn't its size).
//!
//! The bitmap is `width` x `height` (300x150 unless the element says), and so is the element's box when its classes
//! and style give it none. The list is replayed onto the kept bitmap from the first call it hasn't drawn (the same
//! `gen`), or from a cleared bitmap (a new `gen`: the JS cleared it whole, or its size changed).
//!
//! Each call carries its state, resolved by the JS: paths in the bitmap's pixels, the transform in force (strokes and
//! gradients are drawn in its space), globalAlpha, globalCompositeOperation, the clip (paths intersected), a shadow
//! (drawn below the shape: its alpha, blurred, offset, in the shadow's colour). Text is shaped and outlined by usvg in
//! the page's fonts (`page_fonts`) and filled or stroked with the call's paint. Images are data: URLs, decoded by
//! bevy_image. A video drawn is its frame at the time the call gives, decoded on the CPU by raea-media (its file found
//! by `videos`: a Blitz page's own roots), so nothing is read back from a GPU.
//!
//! getImageData is `read`, the synchronous host call `canvas.read` a host registers (raea-page, among its native
//! calls): the list replayed on a bitmap of its own on the JS's thread and the rectangle's pixels sent back, not
//! premultiplied, as base64.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use serde_json::Value;
use tiny_skia as sk;

mod draw;
mod measure;
pub use measure::{advances, metrics};
mod svgtext;
pub use svgtext::svg_text;

/// A text native: its answer to `args` in the given fonts.
pub type TextNative = fn(&usvg::Options, &Value) -> Value;

/// A text native's answer (`canvas.measure` is [`metrics`], `canvas.advances` [`advances`], `canvas.svgText`
/// [`svg_text`]), laid out in the page's fonts as they are now.
pub fn with_page_fonts(answer: TextNative, args: &Value) -> Result<Value, String> {
    Ok(answer(&page_options(), args))
}
mod filter;
mod paint;
mod subsets;
pub mod svg;

use paint::*;

/// A canvas's bitmap as its list drew it, apart from any page (raea-page's Blitz page keeps one per canvas too).
pub struct Replay {
    pm: sk::Pixmap,
    generation: u64,
    drawn: usize,
}

impl Replay {
    /// The bitmap a canvas's props describe, drawn on from where `old` left off when it's the same list and size.
    pub fn update(old: Option<Replay>, props: &serde_json::Map<String, Value>, opt: &usvg::Options<'static>) -> Replay {
        let (w, h) = (dim(props, "width", 300).max(1), dim(props, "height", 150).max(1));
        let list = props.get("__canvas2d").cloned().unwrap_or(Value::Null);
        let generation = list["gen"].as_u64().unwrap_or(0);
        let ops = list["ops"].as_array().cloned().unwrap_or_default();
        let mut r = match old {
            Some(r) if r.pm.width() == w && r.pm.height() == h && r.generation == generation && r.drawn <= ops.len() => r,
            _ => Replay { pm: sk::Pixmap::new(w, h).unwrap(), generation, drawn: 0 },
        };
        *FONTS.lock().unwrap() = Some(opt.fontdb.clone());
        let mut cx = Ctx { opt, images: HashMap::new() };
        for op in &ops[r.drawn..] {
            cx.op(&mut r.pm, op);
        }
        r.drawn = ops.len();
        r
    }

    /// Its size and its pixels, RGBA8 not premultiplied.
    pub fn rgba(&self) -> (u32, u32, Vec<u8>) {
        let mut px = Vec::with_capacity(self.pm.data().len());
        for p in self.pm.pixels() {
            let c = p.demultiply();
            px.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        (self.pm.width(), self.pm.height(), px)
    }
}

/// Where a canvas's videos' and pictures' files are, for a host that draws canvases (raea-page's Blitz page: the page's
/// roots, so a /media/ src is the project's file); until one is given a src is read as a file path.
pub fn videos(find: fn(&str) -> Option<std::path::PathBuf>) {
    *VIDEO_PATHS.write().unwrap() = Some(find);
}

/// The fonts a canvas was last drawn in (`canvas.read` has no World to ask).
static FONTS: std::sync::Mutex<Option<std::sync::Arc<usvg::fontdb::Database>>> = std::sync::Mutex::new(None);

/// Where the page's fonts are now, for `canvas.read`, from a host that has them (raea-page's Blitz page: blitz-dom's
/// font database). Without it a read used the fonts the last shown canvas was drawn in, and a canvas read back just
/// after the page added a face (Excalidraw drawing each element onto a canvas of its own once Excalifont was in) came
/// back in the fallback, until some shown canvas was drawn again.
static FONT_SOURCE: std::sync::OnceLock<fn() -> std::sync::Arc<usvg::fontdb::Database>> = std::sync::OnceLock::new();

/// Gives `canvas.read` the page's fonts as they are when it's called. The first host's stands: there is one page font
/// database a process.
pub fn fonts(now: fn() -> std::sync::Arc<usvg::fontdb::Database>) {
    let _ = FONT_SOURCE.set(now);
}

/// `canvas.read` {width, height, __canvas2d, x, y, w, h} -> {data: base64 RGBA}: the canvas's list drawn on a bitmap of
/// its own and the rectangle read off it, outside the bitmap transparent black (getImageData's answer).
pub fn read(args: &Value) -> Result<Value, String> {
    use base64::Engine as _;
    let props = args.as_object().ok_or("no canvas")?;
    let opt = page_options();
    let (bw, bh, px) = Replay::update(None, props, &opt).rgba();
    let (x, y) = (paint::f(&args["x"]) as i64, paint::f(&args["y"]) as i64);
    let (w, h) = ((paint::f(&args["w"]) as i64).max(0), (paint::f(&args["h"]) as i64).max(0));
    if w * h > 1 << 28 {
        return Err("too large".into());
    }
    let mut out = vec![0u8; (w * h * 4) as usize];
    for row in 0..h {
        let sy = y + row;
        if sy < 0 || sy >= bh as i64 {
            continue;
        }
        let (x0, x1) = (x.max(0), (x + w).min(bw as i64));
        if x0 >= x1 {
            continue;
        }
        let src = ((sy * bw as i64 + x0) * 4) as usize..((sy * bw as i64 + x1) * 4) as usize;
        let dst = ((row * w + (x0 - x)) * 4) as usize;
        out[dst..dst + src.len()].copy_from_slice(&px[src]);
    }
    Ok(serde_json::json!({ "data": base64::engine::general_purpose::STANDARD.encode(out) }))
}

/// The text options a page's canvas draws with now: its fonts (`fonts`) as page_fonts makes them, else the defaults.
pub(crate) fn page_options() -> usvg::Options<'static> {
    let db = FONT_SOURCE.get().map(|now| now()).or_else(|| FONTS.lock().unwrap().clone());
    db.map_or_else(usvg::Options::default, |db| page_fonts(&db))
}

/// Where a video's or a picture's src is as a file, for drawing it on a canvas (`videos`).
static VIDEO_PATHS: std::sync::RwLock<Option<fn(&str) -> Option<std::path::PathBuf>>> = std::sync::RwLock::new(None);

/// A src as a file: by the host's roots (`videos`), else as a path (a file:// URL's too).
fn media_file(src: &str) -> Option<std::path::PathBuf> {
    match *VIDEO_PATHS.read().unwrap() {
        Some(f) => f(src),
        None => Some(std::path::PathBuf::from(src.strip_prefix("file://").unwrap_or(src))),
    }
}

/// A picture that isn't inline, read from its file, with its type by its name: drawImage of an <img> or new Image()
/// whose src is the project's /media/ file (raea #253: the card's screenshot drew nothing, as only data: URLs were
/// decoded).
pub(crate) fn image_file(src: &str) -> Option<(String, Vec<u8>)> {
    let path = media_file(src)?;
    let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    Some((mime.to_string(), std::fs::read(path).ok()?))
}

/// A video's frame at `t`, as a pixmap: opened and decoded on the CPU when it's asked for (a histogram reads one frame
/// per seek), the last few kept.
fn video_frame(src: &str, t: f64) -> Option<sk::Pixmap> {
    static FRAMES: std::sync::Mutex<Vec<(String, u64, sk::Pixmap)>> = std::sync::Mutex::new(Vec::new());
    if let Some((.., pm)) = FRAMES.lock().unwrap().iter().find(|(s, b, _)| s == src && *b == t.to_bits()) {
        return Some(pm.clone());
    }
    let path = media_file(src)?;
    let frame = raea_io_media::preferred().into_iter().find_map(|b| b.open_video(&path).ok()?.frame_at(t).ok())?;
    let mut pm = sk::Pixmap::new(frame.width, frame.height)?;
    for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
        let c = &frame.rgba[i * 4..i * 4 + 4];
        *px = sk::ColorU8::from_rgba(c[0], c[1], c[2], c[3]).premultiply();
    }
    let mut frames = FRAMES.lock().unwrap();
    frames.push((src.to_string(), t.to_bits(), pm.clone()));
    if frames.len() > 8 {
        frames.remove(0);
    }
    Some(pm)
}

fn dim(p: &serde_json::Map<String, Value>, k: &str, d: u32) -> u32 {
    p.get(k).and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())).filter(|v| *v >= 0.0).map_or(d, |v| v as u32)
}

struct Ctx<'a> {
    opt: &'a usvg::Options<'static>,
    images: HashMap<String, sk::Pixmap>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_and_clears() {
        let mut pm = sk::Pixmap::new(20, 10).unwrap();
        let opt = usvg::Options::default();
        let mut cx = Ctx { opt: &opt, images: HashMap::new() };
        let st = serde_json::json!({"ts": [1, 0, 0, 1, 0, 0], "alpha": 1, "comp": "source-over", "clip": [], "shadow": null});
        cx.op(&mut pm, &serde_json::json!(["fill", [0, 0, 0, 1, 20, 0, 1, 20, 10, 1, 0, 10, 4], "nonzero", [255, 0, 0, 1], st]));
        assert_eq!(pm.pixel(5, 5).unwrap().red(), 255);
        cx.op(&mut pm, &serde_json::json!(["clear", [0, 0, 0, 1, 10, 0, 1, 10, 10, 1, 0, 10, 4], st]));
        assert_eq!(pm.pixel(5, 5).unwrap().alpha(), 0);
        assert_eq!(pm.pixel(15, 5).unwrap().alpha(), 255);
    }

    #[test]
    fn a_canvas_drawn_on_another_is_its_list_replayed() {
        let opt = usvg::Options::default();
        let mut pm = sk::Pixmap::new(8, 8).unwrap();
        let mut cx = Ctx { opt: &opt, images: HashMap::new() };
        let st = serde_json::json!({"ts": [1, 0, 0, 1, 0, 0], "alpha": 1, "comp": "source-over", "clip": []});
        // the source: a 2x2 canvas filled red; drawn at 4,4 scaled to 4x4
        let rect = serde_json::json!([0, 0, 0, 1, 2, 0, 1, 2, 2, 1, 0, 2, 4]);
        let src = serde_json::json!({"key": "test:1:1", "w": 2, "h": 2, "ops": [["fill", rect, "nonzero", [255, 0, 0, 1], st]]});
        cx.op(&mut pm, &serde_json::json!(["canvas", src, null, null, null, null, 4, 4, 4, 4, st]));
        let at = |x: u32, y: u32| pm.pixel(x, y).unwrap().demultiply();
        assert_eq!((at(5, 5).red(), at(5, 5).alpha()), (255, 255));
        assert_eq!(at(2, 2).alpha(), 0);
    }

    #[test]
    fn a_picture_from_a_file_is_drawn() {
        // raea #253: drawImage of a picture whose src is a file (a page's /media/, found by the host's roots) drew nothing
        let dir = std::env::temp_dir().join(format!("raea-canvas-pic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut pic = sk::Pixmap::new(3, 2).unwrap();
        pic.fill(sk::Color::from_rgba8(0, 0, 210, 255));
        let file = dir.join("shot.png");
        pic.save_png(&file).unwrap();
        let opt = usvg::Options::default();
        let mut pm = sk::Pixmap::new(10, 10).unwrap();
        let mut cx = Ctx { opt: &opt, images: HashMap::new() };
        let st = serde_json::json!({"ts": [1, 0, 0, 1, 0, 0], "alpha": 1, "comp": "source-over", "clip": [], "shadow": null, "smooth": false});
        let src = file.to_string_lossy().to_string();
        cx.op(&mut pm, &serde_json::json!(["image", src, null, null, null, null, 2, 2, 6, 4, st]));
        assert_eq!(pm.pixel(4, 4).unwrap().demultiply().blue(), 210, "the picture is drawn");
        assert_eq!(pm.pixel(1, 1).unwrap().alpha(), 0, "only where it was asked to go");
        // a file that isn't a picture, or isn't there, draws nothing and doesn't panic
        cx.op(&mut pm, &serde_json::json!(["image", dir.join("none.png").to_string_lossy(), null, null, null, null, 0, 0, 10, 10, st]));
        assert_eq!(pm.pixel(1, 1).unwrap().alpha(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn image_under_a_scale() {
        // Excalidraw's way: a 4x4 picture drawn at 2x size under a 0.5 transform covers 4x4 pixels at (2, 2)
        let mut pm = sk::Pixmap::new(12, 12).unwrap();
        let opt = usvg::Options::default();
        let mut img = sk::Pixmap::new(4, 4).unwrap();
        img.fill(sk::Color::from_rgba8(0, 200, 0, 255));
        let mut cx = Ctx { opt: &opt, images: HashMap::from([("pic".to_string(), img)]) };
        let st = serde_json::json!({"ts": [0.5, 0, 0, 0.5, 0, 0], "alpha": 1, "comp": "source-over", "clip": [], "shadow": null, "smooth": false});
        cx.op(&mut pm, &serde_json::json!(["image", "pic", null, null, null, null, 4, 4, 8, 8, st]));
        for (x, y) in [(2, 2), (5, 5), (3, 4)] {
            assert_eq!(pm.pixel(x, y).unwrap().green(), 200, "inside at {x},{y}");
        }
        for (x, y) in [(1, 1), (6, 6), (9, 3)] {
            assert_eq!(pm.pixel(x, y).unwrap().alpha(), 0, "outside at {x},{y}");
        }
    }
}

/// Text options for a canvas drawn without a Bevy world's fonts (raea-page's Blitz page): the faces in `db` (the
/// system's and the page's own @font-face faces, blitz-dom's SVG faces), with Chrome's generic families on macOS
pub fn page_fonts(db: &std::sync::Arc<usvg::fontdb::Database>) -> usvg::Options<'static> {
    let mut last = PAGE_FONTS.lock().unwrap();
    if !last.as_ref().is_some_and(|(from, _, _)| std::sync::Arc::ptr_eq(from, db)) {
        let mut faces = (**db).clone();
        faces.set_sans_serif_family("Helvetica");
        faces.set_serif_family("Times");
        faces.set_monospace_family("Courier");
        // a family the page split by unicode-range, each file under an alias of its own (subsets.rs)
        let split = subsets::alias(&mut faces);
        *last = Some((db.clone(), std::sync::Arc::new(faces), std::sync::Arc::new(split)));
    }
    usvg::Options { fontdb: last.as_ref().unwrap().1.clone(), ..Default::default() }
}

type Faces = std::sync::Arc<usvg::fontdb::Database>;
/// The last `db` page_fonts was given, its copy with the generic families set and the split families aliased, made
/// again when the page adds a face.
static PAGE_FONTS: std::sync::Mutex<Option<(Faces, Faces, std::sync::Arc<subsets::Subsets>)>> = std::sync::Mutex::new(None);

/// The split families of a database page_fonts made (none for any other: a Bevy world's fonts).
pub(crate) fn split_families(db: &Faces) -> Option<std::sync::Arc<subsets::Subsets>> {
    PAGE_FONTS.lock().unwrap().as_ref().filter(|(_, made, _)| std::sync::Arc::ptr_eq(made, db)).map(|(_, _, s)| s.clone())
}
