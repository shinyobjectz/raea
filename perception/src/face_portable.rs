//! Faces found without Apple Vision: YuNet (OpenCV Zoo, MIT, Shiqi Yu) on RTen, its ONNX model fetched once into
//! ~/.cache/raea/yunet. The same shape as Vision's faces (box 0..1 from the top left, a confidence), so
//! `faces --backend yunet` can stand in for Vision wherever it matches it on our measurements.

use rten_tensor::prelude::*;
use rten_tensor::NdTensor;
use std::path::PathBuf;

const MODEL: (&str, &str) = (
    "face_detection_yunet_2023mar.onnx",
    "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
);
/// the network's square input; pictures are scaled to fit and padded
const SIDE: usize = 640;

fn model() -> Result<PathBuf, String> {
    let dir = PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache/raea/yunet");
    let p = dir.join(MODEL.0);
    if !p.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tmp = p.with_extension("part");
        let ok = std::process::Command::new("curl").args(["-fsSL", "-o"]).arg(&tmp).arg(MODEL.1).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return Err(format!("couldn't fetch {}", MODEL.1));
        }
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

pub struct Detector {
    model: rten::Model,
}

impl Detector {
    pub fn new() -> Result<Self, String> {
        Ok(Self { model: rten::Model::load_file(model()?).map_err(|e| e.to_string())? })
    }

    fn output(&self, name: &str) -> Result<rten::NodeId, String> {
        self.model.find_node(name).ok_or_else(|| {
            let names: Vec<_> = self.model.output_ids().iter().filter_map(|&id| self.model.node_info(id).and_then(|n| n.name().map(String::from))).collect();
            format!("yunet: no output {name} (has {names:?})")
        })
    }

    /// Every face in an RGB8 picture scoring `min` or more: (box 0..1 from the top left, confidence), biggest first.
    /// Boxes are drawn as Vision draws them (`vision_box`), so thresholds set on Vision's faces carry over.
    pub fn faces(&self, rgb: &[u8], w: u32, h: u32, min: f32) -> Result<Vec<([f64; 4], f32)>, String> {
        let (w, h) = (w as usize, h as usize);
        let scale = SIDE as f64 / w.max(h) as f64;
        let (sw, sh) = (((w as f64 * scale).round() as usize).max(1), ((h as f64 * scale).round() as usize).max(1));
        // BGR, 0..255, as OpenCV's FaceDetectorYN feeds it; nearest-neighbour scaling, the rest left black
        let mut input = NdTensor::<f32, 4>::zeros([1, 3, SIDE, SIDE]);
        for y in 0..sh {
            let sy = ((y as f64 / scale) as usize).min(h - 1);
            for x in 0..sw {
                let sx = ((x as f64 / scale) as usize).min(w - 1);
                let i = (sy * w + sx) * 3;
                input[[0, 0, y, x]] = rgb[i + 2] as f32;
                input[[0, 1, y, x]] = rgb[i + 1] as f32;
                input[[0, 2, y, x]] = rgb[i] as f32;
            }
        }
        let strides = [8usize, 16, 32];
        let mut ids = Vec::new();
        for s in strides {
            for k in ["cls", "obj", "bbox"] {
                ids.push(self.output(&format!("{k}_{s}"))?);
            }
        }
        let inp = *self.model.input_ids().first().ok_or("yunet: no input")?;
        let mut outs = self.model.run(vec![(inp, input.view().into())], &ids, None).map_err(|e| e.to_string())?.into_iter();
        let mut found: Vec<([f64; 4], f32)> = Vec::new();
        for s in strides {
            let mut next = || -> Result<NdTensor<f32, 3>, String> { outs.next().ok_or("yunet: missing output")?.try_into().map_err(|e: rten::TryFromValueError| e.to_string()) };
            let (cls, obj, bbox) = (next()?, next()?, next()?);
            let cols = SIDE / s;
            for i in 0..cls.size(1) {
                let score = (cls[[0, i, 0]].clamp(0.0, 1.0) * obj[[0, i, 0]].clamp(0.0, 1.0)).sqrt();
                if score < min {
                    continue;
                }
                let (r, c) = ((i / cols) as f64, (i % cols) as f64);
                let s = s as f64;
                let (cx, cy) = ((c + bbox[[0, i, 0]] as f64) * s, (r + bbox[[0, i, 1]] as f64) * s);
                let (bw, bh) = ((bbox[[0, i, 2]] as f64).exp() * s, (bbox[[0, i, 3]] as f64).exp() * s);
                // back to the picture, 0..1
                let (x0, y0) = ((cx - bw / 2.0) / scale, (cy - bh / 2.0) / scale);
                let b = [x0 / w as f64, y0 / h as f64, bw / scale / w as f64, bh / scale / h as f64];
                let x = b[0].clamp(0.0, 1.0);
                let y = b[1].clamp(0.0, 1.0);
                found.push(([x, y, (b[0] + b[2]).min(1.0) - x, (b[1] + b[3]).min(1.0) - y], score));
            }
        }
        // the best of each overlapping group (IoU 0.3, as FaceDetectorYN's default NMS)
        found.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut kept: Vec<([f64; 4], f32)> = Vec::new();
        for f in found {
            if kept.iter().all(|k| iou(&k.0, &f.0) < 0.3) {
                kept.push(f);
            }
        }
        kept.sort_by(|a, b| (b.0[2] * b.0[3]).total_cmp(&(a.0[2] * a.0[3])));
        Ok(kept.into_iter().map(|(b, c)| (vision_box(b), c)).collect())
    }
}

/// YuNet's box as Vision draws the same face: wider, shorter and a little lower (medians over 191 faces both found in
/// 354 eval-render frames, 2026-10-17; IoU with Vision's box 0.70 before, 0.86 after).
pub fn vision_box(b: [f64; 4]) -> [f64; 4] {
    let (cx, cy) = (b[0] + b[2] * (0.5 + 0.011), b[1] + b[3] * (0.5 + 0.05));
    let (w, h) = (b[2] * 1.175, b[3] * 0.827);
    let (x, y) = ((cx - w / 2.0).max(0.0), (cy - h / 2.0).max(0.0));
    [x, y, (cx + w / 2.0).min(1.0) - x, (cy + h / 2.0).min(1.0) - y]
}

pub fn iou(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let ix = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
    let iy = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
    if ix <= 0.0 || iy <= 0.0 {
        return 0.0;
    }
    let i = ix * iy;
    i / (a[2] * a[3] + b[2] * b[3] - i)
}
