//! What Apple Vision sees in a frame, behind one trait (docs/goal.feature A3): faces, text lines, rectangles, the person
//! mask and whether the picture is a screenshot. AppleVision runs Vision's requests through objc2 with the settings
//! Perception.swift uses (apple_vision.rs); another implementation (raea-sys-perceive's portable models) stands in on
//! other platforms.

use raea_sys_layout::layout::Rect;

/// What Vision saw (boxes 0..1 from the top left).
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub faces: Vec<Rect>,
    /// text lines laid over the picture (tilted text, printed on things, is left out), with their words
    pub lines: Vec<(Rect, String)>,
    pub rects: Vec<Rect>,
    /// the person mask's bytes, row by row, and its size; empty without a person
    pub mask: Vec<u8>,
    pub mw: usize,
    pub mh: usize,
    pub screenshot: bool,
}

pub trait Vision {
    /// What Vision sees in an RGBA8 picture, for its layout.
    fn see(&self, rgba: &[u8], w: usize, h: usize) -> Result<Seen, String>;
    /// A shot's middle frame read the way an editor reads it (Perception.swift › see).
    fn read(&self, rgba: &[u8], w: usize, h: usize, ocr: bool) -> Result<Read, String>;
    /// The faces and the person's box (from the person mask), cheaply, for a track through a clip.
    fn faces(&self, rgba: &[u8], w: usize, h: usize) -> Result<(Vec<Rect>, Option<Rect>), String> {
        let p = self.people(rgba, w, h, true)?;
        Ok((p.faces, p.person))
    }
    /// The faces and the person mask (its bytes, and its box), `fast` for a live camera or a track.
    fn people(&self, rgba: &[u8], w: usize, h: usize, fast: bool) -> Result<People, String>;
    /// Where the eye goes: Vision's attention-based saliency map (about 68 x 68 floats, rows from the top, 0..1).
    fn attention(&self, rgba: &[u8], w: usize, h: usize) -> Result<Option<(Vec<f32>, usize, usize)>, String>;
    /// The place as a vector: the frame's feature print with the person blacked out (Perception.swift › roomVector),
    /// rounded to three decimals; None where it can't be read.
    fn room(&self, rgba: &[u8], w: usize, h: usize) -> Result<Option<Vec<f32>>, String>;
}

/// Faces and the person mask, boxes 0..1 from the top left.
#[derive(Clone, Debug, Default)]
pub struct People {
    pub faces: Vec<Rect>,
    /// each face's detection confidence, in the order of `faces` (raw, per frame: ingest confirms faces over time)
    pub face_conf: Vec<f32>,
    pub person: Option<Rect>,
    pub mask: Vec<u8>,
    pub mw: usize,
    pub mh: usize,
}

/// A shot's frame read (Perception.swift › Seen).
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Read {
    /// close-up | medium | wide | no people (from the biggest face, or a body without one)
    pub framing: String,
    pub faces: usize,
    /// the most attention-grabbing region [x, y, w, h], 0..1 from the top left
    pub subject: Option<[f64; 4]>,
    /// Apple's aesthetics score, -1..1
    pub look: Option<f64>,
    /// Apple's scene labels over 0.3, at most 4
    pub scene: Vec<String>,
    /// lines of text read accurately, with their boxes
    pub words: Vec<(String, [f64; 4])>,
}

// Apple Vision's implementation (apple_vision.rs), named here as before
#[cfg(target_vendor = "apple")]
pub use crate::apple_vision::{AppleVision, text_lines, text_lines_at, text_lines_from};

/// The layout reader's grey copy: the picture 240 px wide, luma, averaged down then sampled bilinearly.
pub fn grey(rgba: &[u8], w: usize, h: usize) -> raea_sys_layout::layout::Grey {
    let gw = 240usize;
    let gh = ((gw as f64 * h as f64 / w as f64) as usize).max(1);
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba.to_vec()).expect("rgba size");
    let small = image::imageops::resize(&img, gw as u32, gh as u32, image::imageops::FilterType::Triangle);
    let v = small.pixels().map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).round() / 255.0).collect();
    raea_sys_layout::layout::Grey { w: gw, h: gh, v }
}

/// A frame's layout end to end in Rust: Vision behind the trait, the grey copies, the layout logic.
pub fn layout(vision: &dyn Vision, rgba: &[u8], w: usize, h: usize, next: &[(&[u8], usize, usize)]) -> Result<raea_sys_layout::layout::Layout, String> {
    let seen = vision.see(rgba, w, h)?;
    let inputs = raea_sys_layout::layout::Inputs {
        w: w as f64,
        h: h as f64,
        screenshot: seen.screenshot,
        faces: seen.faces,
        lines: seen.lines,
        rects: seen.rects,
        mask: if seen.mask.is_empty() { None } else { Some(seen.mask.iter().map(|b| *b as f32 / 255.0).collect()) },
        mw: seen.mw,
        mh: seen.mh,
        g: grey(rgba, w, h),
        next: next.iter().map(|(p, nw, nh)| grey(p, *nw, *nh)).collect(),
    };
    Ok(raea_sys_layout::layout::layout(&inputs))
}

/// The framing coach end to end in Rust: Vision behind the trait (the tallest face, the fast person mask), then the
/// coach's logic over the frame's pixels.
pub fn coach(vision: &dyn Vision, rgba: &[u8], w: usize, h: usize, iso: f32, fov: f32) -> Result<raea_sys_layout::coach::Coach, String> {
    let p = vision.people(rgba, w, h, true)?;
    // Swift's max(by:) on the box height: the first of the tallest
    let face = p.faces.iter().copied().fold(None::<Rect>, |b, f| match b { Some(b) if b.h >= f.h => Some(b), _ => Some(f) });
    let inputs = raea_sys_layout::coach::Inputs { w, h, face, mask: if face.is_some() { p.mask } else { vec![] }, mw: p.mw, mh: p.mh };
    let read = |x: usize, y: usize| { let o = (y * w + x) * 4; (rgba[o], rgba[o + 1], rgba[o + 2]) };
    Ok(raea_sys_layout::coach::coach(&inputs, iso, fov, Some(&read)))
}
