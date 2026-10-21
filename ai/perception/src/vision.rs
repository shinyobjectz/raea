//! What Apple Vision sees in a frame, behind one trait (docs/the roadmap A3): faces, text lines, rectangles, the person
//! mask and whether the picture is a screenshot. AppleVision runs Vision's requests through objc2 with the settings
//! Perception.swift uses; another implementation (models elsewhere) can stand in on other platforms.

use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{
    VNCalculateImageAestheticsScoresRequest, VNClassifyImageRequest, VNDetectFaceRectanglesRequest, VNDetectHumanRectanglesRequest,
    VNGenerateAttentionBasedSaliencyImageRequest, VNGenerateImageFeaturePrintRequest, VNDetectRectanglesRequest, VNGeneratePersonSegmentationRequest,
    VNGeneratePersonSegmentationRequestQualityLevel, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};

use crate::layout::Rect;

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

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// A box as the Swift's facts round it (two decimals).
fn rounded_box(r: Rect) -> [f64; 4] {
    [r2(r.x), r2(r.y), r2(r.w), r2(r.h)]
}

/// RGBA8 as an sRGB CGImage, as the Swift hands Vision a decoded picture.
fn cg_image(rgba: &[u8], w: usize, h: usize) -> Result<objc2_core_foundation::CFRetained<objc2_core_graphics::CGImage>, String> {
    use objc2_core_graphics::{CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo, kCGColorSpaceSRGB};
    let data = objc2_core_foundation::CFData::from_bytes(rgba);
    let provider = CGDataProvider::with_cf_data(Some(&data)).ok_or("no data provider")?;
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or("no sRGB")?;
    unsafe {
        CGImage::new(w, h, 8, 32, w * 4, Some(&space), CGBitmapInfo(CGImageAlphaInfo::NoneSkipLast.0), Some(&provider), std::ptr::null(), false,
                     CGColorRenderingIntent::RenderingIntentDefault)
    }
    .ok_or_else(|| "CGImageCreate failed".to_string())
}

pub struct AppleVision {
    /// the person mask's quality: balanced for layout, fast for the live coach
    pub quality: VNGeneratePersonSegmentationRequestQualityLevel,
}

impl Default for AppleVision {
    fn default() -> Self {
        AppleVision { quality: VNGeneratePersonSegmentationRequestQualityLevel::Balanced }
    }
}

/// Vision's box (origin bottom left) as one from the top left.
fn rect(b: objc2_core_foundation::CGRect) -> Rect {
    Rect::new(b.origin.x, 1.0 - (b.origin.y + b.size.height), b.size.width, b.size.height)
}

impl Vision for AppleVision {
    fn see(&self, rgba: &[u8], pw: usize, ph: usize) -> Result<Seen, String> {
        let image = cg_image(rgba, pw, ph)?;
        let (w, h) = (pw as f64, ph as f64);
        let handler = unsafe { VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &image, &NSDictionary::new()) };
        let faces = unsafe { VNDetectFaceRectanglesRequest::new() };
        let text = VNRecognizeTextRequest::new();
        text.setRecognitionLevel(VNRequestTextRecognitionLevel::Fast);
        text.setUsesLanguageCorrection(false);
        let person = unsafe { VNGeneratePersonSegmentationRequest::new() };
        unsafe { person.setQualityLevel(self.quality) };
        let rects = unsafe { VNDetectRectanglesRequest::new() };
        unsafe {
            rects.setMaximumObservations(8);
            rects.setMinimumSize(0.08);
            rects.setMinimumAspectRatio(0.2);
            rects.setQuadratureTolerance(20.0); // screens shown at an angle are quads, not rectangles
        }
        let classify = unsafe { VNClassifyImageRequest::new() };
        // one request at a time, as the Swift performs them
        let requests: [&VNRequest; 5] = [&faces, &text, &person, &rects, &classify];
        for r in requests {
            let _ = handler.performRequests_error(&NSArray::from_slice(&[r]));
        }
        let mut seen = Seen::default();
        if let Some(rs) = unsafe { faces.results() } {
            seen.faces = rs.iter().map(|o| rect(unsafe { o.boundingBox() })).filter(|r| r.area() > 0.0015).collect();
        }
        if let Some(rs) = text.results() {
            for o in rs.iter() {
                let Some(top) = o.topCandidates(1).firstObject() else { continue };
                if top.confidence() <= 0.3 {
                    continue;
                }
                let (tl, tr) = (unsafe { o.topLeft() }, unsafe { o.topRight() });
                let tilt = ((tr.y - tl.y) * h).atan2((tr.x - tl.x) * w).abs().to_degrees();
                if tilt < 2.5 {
                    seen.lines.push((rect(unsafe { o.boundingBox() }), top.string().to_string()));
                }
            }
        }
        if let Some(rs) = unsafe { rects.results() } {
            seen.rects = rs.iter().map(|o| rect(unsafe { o.boundingBox() })).collect();
        }
        if let Some(rs) = unsafe { classify.results() } {
            seen.screenshot = rs.iter().any(|o| unsafe { o.identifier() }.to_string() == "screenshot" && unsafe { o.confidence() } >= 0.3);
        }
        if let Some(m) = unsafe { person.results() }.and_then(|r| r.firstObject()) {
            let buf: Retained<_> = unsafe { m.pixelBuffer() };
            unsafe { CVPixelBufferLockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly) };
            let (mw, mh, row) = (CVPixelBufferGetWidth(&buf), CVPixelBufferGetHeight(&buf), CVPixelBufferGetBytesPerRow(&buf));
            let base = CVPixelBufferGetBaseAddress(&buf) as *const u8;
            if !base.is_null() {
                seen.mask = (0..mh).flat_map(|y| (0..mw).map(move |x| unsafe { *base.add(y * row + x) })).collect();
                (seen.mw, seen.mh) = (mw, mh);
            }
            unsafe { CVPixelBufferUnlockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly) };
        }
        Ok(seen)
    }

    fn attention(&self, rgba: &[u8], w: usize, h: usize) -> Result<Option<(Vec<f32>, usize, usize)>, String> {
        let image = cg_image(rgba, w, h)?;
        let handler = unsafe { VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &image, &NSDictionary::new()) };
        let req = unsafe { VNGenerateAttentionBasedSaliencyImageRequest::new() };
        let _ = handler.performRequests_error(&NSArray::from_slice(&[&*req as &VNRequest]));
        let Some(o) = unsafe { req.results() }.and_then(|r| r.firstObject()) else { return Ok(None) };
        let buf: Retained<_> = unsafe { o.pixelBuffer() };
        unsafe { CVPixelBufferLockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly) };
        let (mw, mh, row) = (CVPixelBufferGetWidth(&buf), CVPixelBufferGetHeight(&buf), CVPixelBufferGetBytesPerRow(&buf));
        let base = CVPixelBufferGetBaseAddress(&buf) as *const u8;
        let map = if base.is_null() {
            None
        } else {
            // one float a pixel (OneComponent32Float)
            Some((0..mh).flat_map(|y| (0..mw).map(move |x| unsafe { *(base.add(y * row) as *const f32).add(x) })).collect::<Vec<f32>>())
        };
        unsafe { CVPixelBufferUnlockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly) };
        Ok(map.map(|m| (m, mw, mh)))
    }

    fn room(&self, rgba: &[u8], w: usize, h: usize) -> Result<Option<Vec<f32>>, String> {
        let p = self.people(rgba, w, h, false)?;
        let mut masked = rgba.to_vec();
        if p.mw > 0 {
            // the mask scaled to the frame (bilinear at pixel centres), the person blended to black
            let m = |x: f64, y: f64| -> f64 {
                let (fx, fy) = ((x + 0.5) * p.mw as f64 / w as f64 - 0.5, (y + 0.5) * p.mh as f64 / h as f64 - 0.5);
                let (x0, y0) = (fx.floor().max(0.0) as usize, fy.floor().max(0.0) as usize);
                let (x1, y1) = ((x0 + 1).min(p.mw - 1), (y0 + 1).min(p.mh - 1));
                let (tx, ty) = ((fx - x0 as f64).clamp(0.0, 1.0), (fy - y0 as f64).clamp(0.0, 1.0));
                let at = |x: usize, y: usize| p.mask[y * p.mw + x] as f64 / 255.0;
                (at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx) * (1.0 - ty) + (at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx) * ty
            };
            for y in 0..h {
                for x in 0..w {
                    let k = 1.0 - m(x as f64, y as f64);
                    let o = (y * w + x) * 4;
                    for c in 0..3 {
                        masked[o + c] = (masked[o + c] as f64 * k).round() as u8;
                    }
                }
            }
        }
        let image = cg_image(&masked, w, h)?;
        let handler = unsafe { VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &image, &NSDictionary::new()) };
        let print = unsafe { VNGenerateImageFeaturePrintRequest::new() };
        let _ = handler.performRequests_error(&NSArray::from_slice(&[&*print as &VNRequest]));
        let Some(fp) = unsafe { print.results() }.and_then(|r| r.firstObject()) else { return Ok(None) };
        let n = unsafe { fp.elementCount() };
        let data = unsafe { fp.data() };
        let bytes = data.to_vec();
        if bytes.len() < n * 4 {
            return Ok(None);
        }
        Ok(Some((0..n).map(|i| (f32::from_le_bytes([bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3]]) * 1000.0).round() / 1000.0).collect()))
    }

    fn people(&self, rgba: &[u8], w: usize, h: usize, fast: bool) -> Result<People, String> {
        let image = cg_image(rgba, w, h)?;
        let handler = unsafe { VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &image, &NSDictionary::new()) };
        let faces = unsafe { VNDetectFaceRectanglesRequest::new() };
        let person = unsafe { VNGeneratePersonSegmentationRequest::new() };
        unsafe {
            person.setQualityLevel(if fast { VNGeneratePersonSegmentationRequestQualityLevel::Fast } else { VNGeneratePersonSegmentationRequestQualityLevel::Balanced })
        };
        for r in [&*faces as &VNRequest, &*person as &VNRequest] {
            let _ = handler.performRequests_error(&NSArray::from_slice(&[r]));
        }
        let mut out = People::default();
        let found: Vec<(Rect, f32)> = unsafe { faces.results() }
            .map(|r| r.iter().map(|o| (rect(unsafe { o.boundingBox() }), unsafe { o.confidence() })).filter(|(r, _)| r.area() > 0.0015).collect())
            .unwrap_or_default();
        out.face_conf = found.iter().map(|f| f.1).collect();
        out.faces = found.into_iter().map(|f| f.0).collect();
        if let Some(m) = unsafe { person.results() }.and_then(|r| r.firstObject()) {
            let buf: Retained<_> = unsafe { m.pixelBuffer() };
            unsafe { CVPixelBufferLockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly) };
            let (mw, mh, row) = (CVPixelBufferGetWidth(&buf), CVPixelBufferGetHeight(&buf), CVPixelBufferGetBytesPerRow(&buf));
            let base = CVPixelBufferGetBaseAddress(&buf) as *const u8;
            if !base.is_null() {
                out.mask = (0..mh).flat_map(|y| (0..mw).map(move |x| unsafe { *base.add(y * row + x) })).collect();
                (out.mw, out.mh) = (mw, mh);
                let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
                for y in 0..mh {
                    for x in 0..mw {
                        if out.mask[y * mw + x] > 127 {
                            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                        }
                    }
                }
                if x0 != usize::MAX {
                    out.person = Some(Rect::new(x0 as f64 / mw as f64, y0 as f64 / mh as f64, (x1 - x0 + 1) as f64 / mw as f64, (y1 - y0 + 1) as f64 / mh as f64));
                }
            }
            unsafe { CVPixelBufferUnlockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly) };
        }
        Ok(out)
    }

    fn read(&self, rgba: &[u8], w: usize, h: usize, ocr: bool) -> Result<Read, String> {
        let image = cg_image(rgba, w, h)?;
        let handler = unsafe { VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &image, &NSDictionary::new()) };
        let faces = unsafe { VNDetectFaceRectanglesRequest::new() };
        let bodies = unsafe { VNDetectHumanRectanglesRequest::new() };
        let attention = unsafe { VNGenerateAttentionBasedSaliencyImageRequest::new() };
        let classify = unsafe { VNClassifyImageRequest::new() };
        let aesthetics = unsafe { VNCalculateImageAestheticsScoresRequest::new() };
        // one at a time: a request the device can't run mustn't take the others with it
        let requests: [&VNRequest; 5] = [&faces, &bodies, &attention, &classify, &aesthetics];
        for r in requests {
            let _ = handler.performRequests_error(&NSArray::from_slice(&[r]));
        }
        let mut out = Read { framing: "no people".into(), ..Default::default() };
        let found: Vec<Rect> = unsafe { faces.results() }.map(|r| r.iter().map(|o| rect(unsafe { o.boundingBox() })).collect()).unwrap_or_default();
        out.faces = found.len();
        if let Some(biggest) = found.iter().map(|f| f.h).fold(None::<f64>, |b, v| Some(b.map_or(v, |b| b.max(v)))) {
            out.framing = if biggest >= 0.22 { "close-up" } else if biggest >= 0.09 { "medium" } else { "wide" }.into();
        } else if unsafe { bodies.results() }.is_some_and(|r| r.count() > 0) {
            out.framing = "wide".into();
        }
        if let Some(objs) = unsafe { attention.results() }.and_then(|r| r.firstObject()).and_then(|o| unsafe { o.salientObjects() }) {
            // Swift's max(by:): the first of the most confident
            let best = objs.iter().fold(None::<Retained<objc2_vision::VNRectangleObservation>>, |b, o| match b {
                Some(b) if unsafe { b.confidence() } >= unsafe { o.confidence() } => Some(b),
                _ => Some(o),
            });
            out.subject = best.map(|o| rounded_box(rect(unsafe { o.boundingBox() })));
        }
        out.look = unsafe { aesthetics.results() }.and_then(|r| r.firstObject()).map(|o| r2(unsafe { o.overallScore() } as f64));
        if let Some(rs) = unsafe { classify.results() } {
            out.scene = rs.iter().filter(|o| unsafe { o.confidence() } >= 0.3).take(4).map(|o| unsafe { o.identifier() }.to_string().replace('_', " ")).collect();
        }
        if ocr {
            let text = VNRecognizeTextRequest::new();
            text.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
            text.setUsesLanguageCorrection(true);
            text.setMinimumTextHeight(0.015);
            let handler = unsafe { VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &image, &NSDictionary::new()) };
            let _ = handler.performRequests_error(&NSArray::from_slice(&[&*text as &VNRequest]));
            if let Some(rs) = text.results() {
                for o in rs.iter() {
                    let Some(c) = o.topCandidates(1).firstObject() else { continue };
                    if c.confidence() < 0.4 {
                        continue;
                    }
                    let t = c.string().to_string().trim().to_string();
                    if t.chars().filter(|ch| ch.is_alphanumeric()).count() >= 2 {
                        out.words.push((t, rounded_box(rect(unsafe { o.boundingBox() }))));
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Every line of text Apple Vision reads accurately in a frame (docs/the roadmap J1), with its confidence and its box (0..1
/// of the frame, from the top left, unrounded): no language correction (captions aren't prose), lines at least 1.5% of
/// the frame high.
pub fn text_lines(rgba: &[u8], w: usize, h: usize) -> Result<Vec<(String, f32, [f64; 4])>, String> {
    let image = cg_image(rgba, w, h)?;
    let text = VNRecognizeTextRequest::new();
    text.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    text.setUsesLanguageCorrection(false);
    text.setMinimumTextHeight(0.015);
    let handler = unsafe { VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &image, &NSDictionary::new()) };
    let _ = handler.performRequests_error(&NSArray::from_slice(&[&*text as &VNRequest]));
    let mut out = Vec::new();
    if let Some(rs) = text.results() {
        for o in rs.iter() {
            let Some(c) = o.topCandidates(1).firstObject() else { continue };
            let r = rect(unsafe { o.boundingBox() });
            out.push((c.string().to_string(), c.confidence(), [r.x, r.y, r.w, r.h]));
        }
    }
    Ok(out)
}

/// The layout reader's grey copy: the picture 240 px wide, luma, averaged down then sampled bilinearly.
pub fn grey(rgba: &[u8], w: usize, h: usize) -> crate::layout::Grey {
    let gw = 240usize;
    let gh = ((gw as f64 * h as f64 / w as f64) as usize).max(1);
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba.to_vec()).expect("rgba size");
    let small = image::imageops::resize(&img, gw as u32, gh as u32, image::imageops::FilterType::Triangle);
    let v = small.pixels().map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).round() / 255.0).collect();
    crate::layout::Grey { w: gw, h: gh, v }
}

/// A frame's layout end to end in Rust: Vision behind the trait, the grey copies, the layout logic.
pub fn layout(vision: &dyn Vision, rgba: &[u8], w: usize, h: usize, next: &[(&[u8], usize, usize)]) -> Result<crate::layout::Layout, String> {
    let seen = vision.see(rgba, w, h)?;
    let inputs = crate::layout::Inputs {
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
    Ok(crate::layout::layout(&inputs))
}

/// The framing coach end to end in Rust: Vision behind the trait (the tallest face, the fast person mask), then the
/// coach's logic over the frame's pixels.
pub fn coach(vision: &dyn Vision, rgba: &[u8], w: usize, h: usize, iso: f32, fov: f32) -> Result<crate::coach::Coach, String> {
    let p = vision.people(rgba, w, h, true)?;
    // Swift's max(by:) on the box height: the first of the tallest
    let face = p.faces.iter().copied().fold(None::<Rect>, |b, f| match b { Some(b) if b.h >= f.h => Some(b), _ => Some(f) });
    let inputs = crate::coach::Inputs { w, h, face, mask: if face.is_some() { p.mask } else { vec![] }, mw: p.mw, mh: p.mh };
    let read = |x: usize, y: usize| { let o = (y * w + x) * 4; (rgba[o], rgba[o + 1], rgba[o + 2]) };
    Ok(crate::coach::coach(&inputs, iso, fov, Some(&read)))
}
