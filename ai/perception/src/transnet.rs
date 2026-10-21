//! TransNetV2 (MIT; github.com/soCzech/TransNetV2) through its ONNX export (~/host/models' sibling
//! ~/.local/share/raea-models/transnetv2/transnetv2.onnx, checked against the official TF model at 5.2e-7): each frame's
//! probability that a shot ends there. The official windowing: frames at 48 x 27 RGB, 100-frame windows stepping by
//! 50, padded with the first and last frame, the middle 50 predictions of each kept (ai/meaning/bench/transnetv2/tn.py).

use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::value::Tensor;

pub fn model_path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/raea-models/transnetv2/transnetv2.onnx")
}

pub struct TransNet(Session);

impl TransNet {
    pub fn load() -> Result<TransNet, String> {
        let s = (|| -> ort::Result<Session> { crate::onnx::session("transnet", model_path()) })().map_err(|e| format!("transnetv2: {e}"))?;
        Ok(TransNet(s))
    }

    /// Per-frame probabilities (single: a shot ends at this frame; many: within a transition) for 48 x 27 RGB frames.
    pub fn predict(&mut self, frames: &[Vec<u8>]) -> Result<(Vec<f32>, Vec<f32>), String> {
        let n = frames.len();
        if n == 0 {
            return Ok((vec![], vec![]));
        }
        let end = 25 + 50 - if n % 50 != 0 { n % 50 } else { 50 };
        let mut padded: Vec<&Vec<u8>> = Vec::new();
        padded.extend(std::iter::repeat_n(&frames[0], 25));
        padded.extend(frames.iter());
        padded.extend(std::iter::repeat_n(&frames[n - 1], end));
        let sig = |x: f32| 1.0 / (1.0 + (-x as f64).exp()) as f32;
        let (mut single, mut many) = (Vec::new(), Vec::new());
        let mut i = 0;
        while i + 100 <= padded.len() {
            let data: Vec<u8> = padded[i..i + 100].iter().flat_map(|f| f.iter().copied()).collect();
            let t = Tensor::from_array(([1usize, 100, 27, 48, 3], data)).map_err(|e| e.to_string())?;
            let out = self.0.run(ort::inputs!["frames" => t]).map_err(|e| format!("transnetv2 run: {e}"))?;
            let (_, a) = out[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let (_, b) = out[1].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            single.extend(a[25..75].iter().map(|x| sig(*x)));
            many.extend(b[25..75].iter().map(|x| sig(*x)));
            i += 50;
        }
        single.truncate(n);
        many.truncate(n);
        Ok((single, many))
    }
}

/// Every frame of a video at 48 x 27 RGB (each output pixel the mean of the source pixels it covers).
pub fn frames(path: &Path, backend: &str) -> Result<(f64, Vec<Vec<u8>>), String> {
    let mut d = raea_media::backend(backend)?.open_video(path)?;
    let info = d.info().clone();
    let frame = 1.0 / info.fps.max(1.0);
    let (mut out, mut prev) = (Vec::new(), f64::NEG_INFINITY);
    let mut k = 0usize;
    loop {
        let t = k as f64 * frame;
        if t >= info.duration {
            break;
        }
        k += 1;
        let f = d.frame_at(t + frame - 1e-3)?;
        if f.pts <= prev {
            continue;
        }
        prev = f.pts;
        let (w, h) = (f.width as usize, f.height as usize);
        let mut px = vec![0u8; 48 * 27 * 3];
        for y in 0..27 {
            let (y0, y1) = (y * h / 27, ((y + 1) * h / 27).max(y * h / 27 + 1));
            for x in 0..48 {
                let (x0, x1) = (x * w / 48, ((x + 1) * w / 48).max(x * w / 48 + 1));
                let mut acc = [0u32; 3];
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let i = (yy * w + xx) * 4;
                        for c in 0..3 {
                            acc[c] += f.rgba[i + c] as u32;
                        }
                    }
                }
                let cnt = ((y1 - y0) * (x1 - x0)) as u32;
                for c in 0..3 {
                    px[(y * 48 + x) * 3 + c] = ((acc[c] + cnt / 2) / cnt) as u8;
                }
            }
        }
        out.push(px);
    }
    Ok((info.fps, out))
}
