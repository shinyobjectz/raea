//! Vision without Apple (goal: perception off Apple Vision, one use at a time, each switched only where it matches
//! Vision on our measurements): the `Vision` trait over open models on RTen, each fetched once into ~/.cache/raea.
//!
//!   faces         YuNet (OpenCV Zoo, MIT), face_portable.rs
//!   person mask   MODNet (Apache-2.0, ZHKKKe/MODNet; the Xenova ONNX export)
//!   saliency      U^2-Net-p (Apache-2.0, xuebinqin/U-2-Net; rembg's ONNX export): the attention map and the subject
//!   room          MobileNetV2 (Apache-2.0, ONNX Model Zoo): ImageNet logits of the frame with the person blacked out
//!   text lines    ocrs (MIT/Apache), ocr_portable.rs
//!
//! Left out because nothing open answers the same question: Apple's scene labels (its own taxonomy) and its
//! aesthetics score; `read` leaves them empty. Rectangles and "is it a screenshot" are measured separately.

use crate::layout::Rect;
use crate::vision::{People, Read, Seen, Vision};
use rten_tensor::prelude::*;
use rten_tensor::NdTensor;
use std::path::PathBuf;

const MODNET: (&str, &str, &str) = ("modnet", "model.onnx", "https://huggingface.co/Xenova/modnet/resolve/main/onnx/model.onnx");
const U2NETP: (&str, &str, &str) = ("u2net", "u2netp.onnx", "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx");
const MOBILENET: (&str, &str, &str) = (
    "mobilenet",
    "mobilenetv2-12.onnx",
    "https://media.githubusercontent.com/media/onnx/models/main/validated/vision/classification/mobilenet/model/mobilenetv2-12.onnx",
);

pub fn fetch((dir, name, url): (&str, &str, &str)) -> Result<PathBuf, String> {
    let dir = PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache/raea").join(dir);
    let p = dir.join(name);
    if !p.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tmp = p.with_extension("part");
        let ok = std::process::Command::new("curl").args(["-fsSL", "-o"]).arg(&tmp).arg(url).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return Err(format!("couldn't fetch {url}"));
        }
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

fn load(m: (&str, &str, &str)) -> Result<rten::Model, String> {
    rten::Model::load_file(fetch(m)?).map_err(|e| e.to_string())
}

/// RGBA8 scaled to `tw` x `th` (triangle filter) as a 1x3xHxW tensor, each channel through `norm(value 0..1, channel)`.
fn tensor(rgba: &[u8], w: usize, h: usize, tw: usize, th: usize, norm: impl Fn(f32, usize) -> f32) -> NdTensor<f32, 4> {
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba.to_vec()).expect("rgba size");
    let small = image::imageops::resize(&img, tw as u32, th as u32, image::imageops::FilterType::Triangle);
    let mut t = NdTensor::<f32, 4>::zeros([1, 3, th, tw]);
    for (x, y, p) in small.enumerate_pixels() {
        for c in 0..3 {
            t[[0, c, y as usize, x as usize]] = norm(p[c] as f32 / 255.0, c);
        }
    }
    t
}

fn run1(m: &rten::Model, input: NdTensor<f32, 4>) -> Result<NdTensor<f32, 4>, String> {
    let inp = *m.input_ids().first().ok_or("no input")?;
    let out = *m.output_ids().first().ok_or("no output")?;
    let v = m.run(vec![(inp, input.view().into())], &[out], None).map_err(|e| e.to_string())?;
    v.into_iter().next().ok_or("no result")?.try_into().map_err(|e: rten::TryFromValueError| e.to_string())
}

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

pub struct PortableVision {
    faces: crate::face_portable::Detector,
    ocr: crate::ocr_portable::Reader,
    modnet: rten::Model,
    u2netp: rten::Model,
    mobilenet: rten::Model,
}

impl PortableVision {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            faces: crate::face_portable::Detector::new()?,
            ocr: crate::ocr_portable::Reader::new()?,
            modnet: load(MODNET)?,
            u2netp: load(U2NETP)?,
            mobilenet: load(MOBILENET)?,
        })
    }

    fn rgb(rgba: &[u8]) -> Vec<u8> {
        rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect()
    }

    fn face_rects(&self, rgba: &[u8], w: usize, h: usize) -> Result<Vec<(Rect, f32)>, String> {
        Ok(self.faces.faces(&Self::rgb(rgba), w as u32, h as u32, 0.6)?.into_iter().map(|(b, c)| (Rect::new(b[0], b[1], b[2], b[3]), c)).filter(|(r, _)| r.area() > 0.0015).collect())
    }

    /// MODNet's alpha as bytes, its shorter side `side` (a multiple of 32), and its size. A matting model always finds
    /// a foreground (a product, a cartoon), so it counts as a person only with a face or over 30% of the frame; else
    /// it's all zero. Measured against Vision's person on 354 eval-render frames: they agree on 97.5%; Vision alone
    /// finds faceless bodies (legs, hands pouring), this alone a person under a blanket.
    pub fn person_mask(&self, rgba: &[u8], w: usize, h: usize, side: usize, face: bool) -> Result<(Vec<u8>, usize, usize), String> {
        let (mut m, mw, mh) = self.mask(rgba, w, h, side)?;
        if !face && (m.iter().filter(|&&b| b > 127).count() as f64) < 0.3 * m.len() as f64 {
            m.iter_mut().for_each(|b| *b = 0);
        }
        Ok((m, mw, mh))
    }

    /// MODNet's alpha as bytes, its shorter side `side` (a multiple of 32), and its size.
    pub fn mask(&self, rgba: &[u8], w: usize, h: usize, side: usize) -> Result<(Vec<u8>, usize, usize), String> {
        let s = side as f64 / w.min(h) as f64;
        let r32 = |v: f64| (((v / 32.0).round() as usize).max(1)) * 32;
        let (tw, th) = (r32(w as f64 * s), r32(h as f64 * s));
        let a = run1(&self.modnet, tensor(rgba, w, h, tw, th, |v, _| (v - 0.5) / 0.5))?;
        let (mh, mw) = (a.size(2), a.size(3));
        Ok(((0..mh).flat_map(|y| (0..mw).map(move |x| (y, x))).map(|(y, x)| (a[[0, 0, y, x]].clamp(0.0, 1.0) * 255.0).round() as u8).collect(), mw, mh))
    }

    /// U^2-Net-p's saliency, 0..1 after its min-max, 320 x 320 rows from the top.
    pub fn saliency(&self, rgba: &[u8], w: usize, h: usize) -> Result<Vec<f32>, String> {
        let max = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).max().unwrap_or(255).max(1) as f32 / 255.0;
        let d = run1(&self.u2netp, tensor(rgba, w, h, 320, 320, |v, c| (v / max - MEAN[c]) / STD[c]))?;
        let m: Vec<f32> = d.iter().copied().collect();
        let (lo, hi) = m.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        Ok(m.iter().map(|v| if hi > lo { (v - lo) / (hi - lo) } else { 0.0 }).collect())
    }
}

impl PortableVision {
    /// MobileNetV2's ImageNet probabilities for the frame (softmax over its 1000 logits).
    pub fn imagenet(&self, rgba: &[u8], w: usize, h: usize) -> Result<Vec<f32>, String> {
        let inp = tensor(rgba, w, h, 224, 224, |v, c| (v - MEAN[c]) / STD[c]);
        let id = *self.mobilenet.input_ids().first().ok_or("no input")?;
        let out = *self.mobilenet.output_ids().first().ok_or("no output")?;
        let v = self.mobilenet.run(vec![(id, inp.view().into())], &[out], None).map_err(|e| e.to_string())?;
        let t: NdTensor<f32, 2> = v.into_iter().next().ok_or("no result")?.try_into().map_err(|e: rten::TryFromValueError| e.to_string())?;
        let max = t.iter().copied().fold(f32::MIN, f32::max);
        let e: Vec<f32> = t.iter().map(|x| (x - max).exp()).collect();
        let sum: f32 = e.iter().sum();
        Ok(e.iter().map(|x| x / sum).collect())
    }
}

/// The box around a byte mask's pixels over 127, 0..1.
fn mask_box(mask: &[u8], mw: usize, mh: usize) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for y in 0..mh {
        for x in 0..mw {
            if mask[y * mw + x] > 127 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    (x0 != usize::MAX).then(|| Rect::new(x0 as f64 / mw as f64, y0 as f64 / mh as f64, (x1 - x0 + 1) as f64 / mw as f64, (y1 - y0 + 1) as f64 / mh as f64))
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

impl Vision for PortableVision {
    fn see(&self, rgba: &[u8], w: usize, h: usize) -> Result<Seen, String> {
        let mut seen = Seen { faces: self.face_rects(rgba, w, h)?.into_iter().map(|f| f.0).collect(), ..Default::default() };
        let face = !seen.faces.is_empty();
        for (t, _, b) in self.ocr.lines(&Self::rgb(rgba), w as u32, h as u32)? {
            seen.lines.push((Rect::new(b[0], b[1], b[2], b[3]), t));
        }
        let (mask, mw, mh) = self.person_mask(rgba, w, h, 384, face)?;
        if mask.iter().any(|&b| b > 127) {
            (seen.mask, seen.mw, seen.mh) = (mask, mw, mh);
        }
        Ok(seen)
    }

    fn read(&self, rgba: &[u8], w: usize, h: usize, ocr: bool) -> Result<Read, String> {
        let faces = self.face_rects(rgba, w, h)?;
        let mut out = Read { framing: "no people".into(), faces: faces.len(), ..Default::default() };
        if let Some(biggest) = faces.iter().map(|f| f.0.h).reduce(f64::max) {
            out.framing = if biggest >= 0.22 { "close-up" } else if biggest >= 0.09 { "medium" } else { "wide" }.into();
        } else {
            let (m, mw, mh) = self.person_mask(rgba, w, h, 256, false)?;
            if mask_box(&m, mw, mh).is_some() {
                out.framing = "wide".into();
            }
        }
        let s = self.saliency(rgba, w, h)?;
        let bytes: Vec<u8> = s.iter().map(|v| (v * 255.0) as u8).collect();
        out.subject = mask_box(&bytes, 320, 320).map(|r| [r2(r.x), r2(r.y), r2(r.w), r2(r.h)]);
        if ocr {
            for (t, _, b) in self.ocr.lines(&Self::rgb(rgba), w as u32, h as u32)? {
                out.words.push((t, [r2(b[0]), r2(b[1]), r2(b[2]), r2(b[3])]));
            }
        }
        Ok(out)
    }

    fn people(&self, rgba: &[u8], w: usize, h: usize, fast: bool) -> Result<People, String> {
        let found = self.face_rects(rgba, w, h)?;
        let (mask, mw, mh) = self.person_mask(rgba, w, h, if fast { 256 } else { 384 }, !found.is_empty())?;
        let person = mask_box(&mask, mw, mh);
        Ok(People { face_conf: found.iter().map(|f| f.1).collect(), faces: found.into_iter().map(|f| f.0).collect(), person, mask, mw, mh })
    }

    fn attention(&self, rgba: &[u8], w: usize, h: usize) -> Result<Option<(Vec<f32>, usize, usize)>, String> {
        // averaged down to 64 x 64 (Vision's map is about 68 x 68)
        let s = self.saliency(rgba, w, h)?;
        let n = 64;
        let mut m = vec![0f32; n * n];
        for (i, v) in s.iter().enumerate() {
            let (x, y) = ((i % 320) * n / 320, (i / 320) * n / 320);
            m[y * n + x] += v / 25.0;
        }
        Ok(Some((m, n, n)))
    }

    fn room(&self, rgba: &[u8], w: usize, h: usize) -> Result<Option<Vec<f32>>, String> {
        let face = !self.face_rects(rgba, w, h)?.is_empty();
        let (mask, mw, mh) = self.person_mask(rgba, w, h, 256, face)?;
        let mut masked = rgba.to_vec();
        for y in 0..h {
            for x in 0..w {
                let k = 1.0 - mask[(y * mh / h) * mw + x * mw / w] as f32 / 255.0;
                let o = (y * w + x) * 4;
                for c in 0..3 {
                    masked[o + c] = (masked[o + c] as f32 * k).round() as u8;
                }
            }
        }
        let inp = tensor(&masked, w, h, 224, 224, |v, c| (v - MEAN[c]) / STD[c]);
        let id = *self.mobilenet.input_ids().first().ok_or("no input")?;
        let out = *self.mobilenet.output_ids().first().ok_or("no output")?;
        let v = self.mobilenet.run(vec![(id, inp.view().into())], &[out], None).map_err(|e| e.to_string())?;
        let t: NdTensor<f32, 2> = v.into_iter().next().ok_or("no result")?.try_into().map_err(|e: rten::TryFromValueError| e.to_string())?;
        let l: Vec<f32> = t.iter().copied().collect();
        let norm = l.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        Ok(Some(l.iter().map(|x| (x / norm * 1000.0).round() / 1000.0).collect()))
    }
}
