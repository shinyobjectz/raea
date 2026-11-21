//! YAMNet (Apache-2.0, AudioSet's 521 classes) on ONNX Runtime: its 1024-d embedding per 0.96 s patch every 0.48 s of
//! 16 kHz mono, the front end inside the graph. Perception's ONNX copy only (models.rs).

use crate::siglip::{Input, run};
use anyhow::Result;
use ort::session::Session;

/// The shortest waveform the graph takes (one patch)
const MIN: usize = 15_600;

/// YAMNet's session
pub struct Yamnet(pub(crate) Session);

impl Yamnet {
    /// [patches][1024]; a waveform under one patch is padded with silence
    pub fn embed(&mut self, pcm16: &[f32]) -> Result<Vec<f32>> {
        let mut w = pcm16.to_vec();
        if w.len() < MIN {
            w.resize(MIN, 0.0);
        }
        Ok(run(&mut self.0, ("waveform", vec![w.len() as i64], Input::F32(w)), &["embeddings"])?.remove(0))
    }

    /// [patches][521]: each AudioSet class's score (0 Speech, 132 Music, ...), from the same graph's head, padded the
    /// same way
    pub fn classes(&mut self, pcm16: &[f32]) -> Result<Vec<f32>> {
        let padded: Vec<f32> = pcm16.iter().copied().chain(std::iter::repeat(0.0)).take(pcm16.len().max(MIN)).collect();
        let shape = vec![padded.len() as i64];
        let mut scores = run(&mut self.0, ("waveform", shape, Input::F32(padded)), &["scores"])?;
        Ok(scores.swap_remove(0))
    }
}
