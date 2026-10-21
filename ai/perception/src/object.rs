//! Masks of any object, tracked through a clip (Perception.swift › ObjectTracker): EdgeTAM (Apache-2.0) as four Core
//! ML models: the image encoder on the frame stretched to 1024 x 1024, memory attention over a bank (the prompted
//! frame and the 6 frames before this one; object pointers of the prompted frame and the 15 before), the mask
//! decoder from a click, a box or (tracking) a dummy point, and the memory encoder that writes this frame's memory.

use crate::coreml::{Input, Model, Units};
use crate::hair;

pub struct Models {
    pub encoder: Model,
    pub decoder: Model,
    pub attention: Model,
    pub memory: Model,
}

pub fn models() -> Result<Models, String> {
    Ok(Models {
        encoder: Model::load("edgetam", "EdgeTAMImageEncoder", Units::All)?,
        decoder: Model::load("edgetam", "EdgeTAMMaskDecoder", Units::All)?,
        attention: Model::load("edgetam", "EdgeTAMMemoryAttention", Units::All)?,
        // fp32: it can't use the Neural Engine, and is fastest on the GPU
        memory: Model::load("edgetam", "EdgeTAMMemoryEncoder", Units::CpuAndGpu)?,
    })
}

/// What the object is: a box [x, y, w, h] or a point [x, y], 0..1 of the frame.
#[derive(Clone, Debug)]
pub enum Prompt {
    Box([f64; 4]),
    Point([f64; 2]),
}

const SIDE: usize = 1024;
const MEMORY_SLOTS: usize = 7;
const POINTER_SLOTS: usize = 16;
const MEMORY_SIZE: usize = 512 * 64;
const POINTER_SIZE: usize = 256;

type Kept = (Vec<f32>, Vec<f32>);

pub struct Tracker<'a> {
    m: &'a Models,
    prompted: Option<Kept>,
    recent: Vec<Kept>,
}

impl<'a> Tracker<'a> {
    pub fn new(m: &'a Models) -> Tracker<'a> {
        Tracker { m, prompted: None, recent: Vec::new() }
    }

    /// The next frame's matte at width x height (255 = the object; all 0 when EdgeTAM says it's out of view): from the
    /// prompt on the prompted frame, else tracked from memory.
    pub fn step(&mut self, rgba: &[u8], w: usize, h: usize, prompt: Option<&Prompt>, width: usize, height: usize) -> Result<Vec<u8>, String> {
        let side = SIDE as f64;
        let input = hair::stretched(rgba, w, h, SIDE);
        let frame = self.m.encoder.predict(&[("image", &input)])?;
        let (vf, vf_shape) = frame.floats("vision_features")?;
        let (h0, h0_shape) = frame.floats("high_res_feat_0")?;
        let (h1, h1_shape) = frame.floats("high_res_feat_1")?;
        let mut points: Vec<(f64, f64, f32)> = vec![(0.0, 0.0, -1.0)]; // a tracking frame's dummy point
        let (mut pix, mut pix_shape) = (vf.clone(), vf_shape.clone());
        match prompt {
            Some(Prompt::Box(b)) => {
                points = vec![(b[0] * side, b[1] * side, 2.0), ((b[0] + b[2]) * side, (b[1] + b[3]) * side, 3.0)];
                self.recent.clear();
            }
            Some(Prompt::Point(p)) => {
                points = vec![(p[0] * side, p[1] * side, 1.0)];
                self.recent.clear();
            }
            None => {
                let Some(prompted) = &self.prompted else { return Err("EdgeTAM was asked to track before it was shown the object".into()) };
                // the bank: slot 0 the prompted frame, slot s the frame 7 - s back; pointers: slot d the frame d back
                let mut memory = vec![0f32; MEMORY_SLOTS * MEMORY_SIZE];
                let mut memory_valid = vec![0f32; MEMORY_SLOTS];
                let mut pointers = vec![0f32; POINTER_SLOTS * POINTER_SIZE];
                let mut pointer_valid = vec![0f32; POINTER_SLOTS];
                memory[..MEMORY_SIZE].copy_from_slice(&prompted.0[..MEMORY_SIZE]);
                memory_valid[0] = 1.0;
                pointers[..POINTER_SIZE].copy_from_slice(&prompted.1[..POINTER_SIZE]);
                pointer_valid[0] = 1.0;
                let n = self.recent.len();
                for s in 1..MEMORY_SLOTS {
                    if n >= MEMORY_SLOTS - s {
                        memory[s * MEMORY_SIZE..(s + 1) * MEMORY_SIZE].copy_from_slice(&self.recent[n - (MEMORY_SLOTS - s)].0[..MEMORY_SIZE]);
                        memory_valid[s] = 1.0;
                    }
                }
                for d in 1..POINTER_SLOTS {
                    if n >= d {
                        pointers[d * POINTER_SIZE..(d + 1) * POINTER_SIZE].copy_from_slice(&self.recent[n - d].1[..POINTER_SIZE]);
                        pointer_valid[d] = 1.0;
                    }
                }
                let attended = self.m.attention.predict_inputs(&[
                    ("vision_features", Input::Array { shape: vf_shape.clone(), data: vf.clone() }),
                    ("memory", Input::Array { shape: vec![1, MEMORY_SLOTS, 512, 64], data: memory }),
                    ("memory_valid", Input::Array { shape: vec![1, MEMORY_SLOTS], data: memory_valid }),
                    ("obj_ptrs", Input::Array { shape: vec![1, POINTER_SLOTS, 256], data: pointers }),
                    ("obj_ptr_valid", Input::Array { shape: vec![1, POINTER_SLOTS], data: pointer_valid }),
                ])?;
                (pix, pix_shape) = attended.floats("pix_feat")?;
            }
        }
        let scalar = |v: f32| Input::Array { shape: vec![1], data: vec![v] };
        let decoded = self.m.decoder.predict_inputs(&[
            ("pix_feat", Input::Array { shape: pix_shape, data: pix }),
            ("high_res_feat_0", Input::Array { shape: h0_shape, data: h0 }),
            ("high_res_feat_1", Input::Array { shape: h1_shape, data: h1 }),
            ("point_coords", Input::Array { shape: vec![1, points.len(), 2], data: points.iter().flat_map(|p| [p.0 as f32, p.1 as f32]).collect() }),
            ("point_labels", Input::Array { shape: vec![1, points.len()], data: points.iter().map(|p| p.2).collect() }),
            ("add_no_mem", scalar(if prompt.is_none() { 0.0 } else { 1.0 })),
            // one click, and every tracking frame, picks the best of three masks; a box takes the single mask
            ("multimask", scalar(if matches!(prompt, Some(Prompt::Box(_))) { 0.0 } else { 1.0 })),
        ])?;
        let (logits, logits_shape) = decoded.floats("mask_logits")?;
        let (pointer, _) = decoded.floats("obj_ptr")?;
        let (score, _) = decoded.floats("object_score_logit")?;
        let remembered = self.m.memory.predict_inputs(&[
            ("vision_features", Input::Array { shape: vf_shape, data: vf }),
            ("mask_logits", Input::Array { shape: logits_shape, data: logits.clone() }),
            ("binarize", scalar(if prompt.is_none() { 0.0 } else { 1.0 })),
        ])?;
        let (memory, _) = remembered.floats("memory")?;
        if prompt.is_some() {
            self.prompted = Some((memory, pointer));
        } else {
            self.recent.push((memory, pointer));
            if self.recent.len() > POINTER_SLOTS - 1 {
                let extra = self.recent.len() - (POINTER_SLOTS - 1);
                self.recent.drain(..extra);
            }
        }
        if score.first().copied().unwrap_or(0.0) <= 0.0 {
            return Ok(vec![0u8; width * height]);
        }
        // the logits resized bilinearly (as the official predictor's F.interpolate, align_corners=False), then the
        // sigmoid: resizing probabilities instead would move the edge
        let n = 256usize;
        let mut alpha = vec![0u8; width * height];
        for y in 0..height {
            let sy = ((y as f32 + 0.5) * n as f32 / height as f32 - 0.5).clamp(0.0, (n - 1) as f32);
            let (y0, fy) = (sy as usize, sy - (sy as usize) as f32);
            let y1 = (y0 + 1).min(n - 1);
            for x in 0..width {
                let sx = ((x as f32 + 0.5) * n as f32 / width as f32 - 0.5).clamp(0.0, (n - 1) as f32);
                let (x0, fx) = (sx as usize, sx - (sx as usize) as f32);
                let x1 = (x0 + 1).min(n - 1);
                let v = (logits[y0 * n + x0] * (1.0 - fx) + logits[y0 * n + x1] * fx) * (1.0 - fy)
                    + (logits[y1 * n + x0] * (1.0 - fx) + logits[y1 * n + x1] * fx) * fy;
                let p = 1.0 / (1.0 + (-v).exp());
                alpha[y * width + x] = (p * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
        Ok(alpha)
    }
}
