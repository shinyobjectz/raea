//! The hair matte (Perception.swift › hairMask): MediaPipe's hair segmenter (Apache-2.0) converted to Core ML, on the
//! whole picture stretched to 512 x 512 (Lanczos, as the Swift scales), its probabilities written as an 8-bit matte.

use crate::coreml::{Bgra, Model, Units};

pub const SIDE: usize = 512;

pub fn model() -> Result<Model, String> {
    // the Neural Engine, never the GPU: on the M5 Pro the GPU's matte fails the MediaPipe reference (IoU 0.96-0.97, up to
    // 182 levels off) where CPU and Neural Engine pass (0.998-0.999); `All` chose the Neural Engine, nothing kept it there
    Model::load("mediapipe-hair-segmenter", "HairSegmenter", Units::CpuAndNe)
}

/// RGBA8 stretched to side x side with a Lanczos filter, as BGRA.
pub fn stretched(rgba: &[u8], w: usize, h: usize, side: usize) -> Bgra {
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba.to_vec()).expect("rgba size");
    let small = if w == side && h == side { img } else { image::imageops::resize(&img, side as u32, side as u32, image::imageops::FilterType::Lanczos3) };
    let mut bytes = small.into_raw();
    for p in bytes.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
    Bgra { w: side, h: side, bytes }
}

/// The hair matte of a picture: SIDE x SIDE bytes, 255 = hair.
pub fn mask(model: &Model, rgba: &[u8], w: usize, h: usize) -> Result<Vec<u8>, String> {
    let input = stretched(rgba, w, h, SIDE);
    let out = model.predict(&[("image", &input)])?;
    let (p, _) = out.floats("hair")?;
    Ok(p.iter().map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8).collect())
}
