//! Silero VAD v5 (MIT) on ONNX: the speech probability of each 32 ms chunk (512 samples at 16 kHz).
//!
//! The v5 graph wants each chunk with the 64 samples before it (its context) and carries a [2, 1, 128] state from one
//! chunk to the next, as silero-vad's own OnnxWrapper and onnx-asr's SileroVad do.

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

pub const CHUNK: usize = 512;
const CONTEXT: usize = 64;

pub struct Vad {
    session: Session,
    state: Vec<f32>,
    context: [f32; CONTEXT],
}

impl Vad {
    pub const SPEC: super::models::Spec = super::models::Spec {
        name: "silero-vad",
        files: &[("onnx-community/silero-vad", "onnx/model.onnx", "silero_vad.onnx")],
    };

    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let session = super::session(&dir.join("silero_vad.onnx"))?;
        Ok(Vad { session, state: vec![0.0; 2 * 128], context: [0.0; CONTEXT] })
    }

    /// Forget the recurrent state (a new conversation).
    pub fn reset(&mut self) {
        self.state.iter_mut().for_each(|s| *s = 0.0);
        self.context = [0.0; CONTEXT];
    }

    /// Speech probability (0-1) of one 512-sample chunk at 16 kHz.
    pub fn prob(&mut self, chunk: &[f32]) -> anyhow::Result<f32> {
        anyhow::ensure!(chunk.len() == CHUNK, "silero wants {CHUNK} samples, got {}", chunk.len());
        let mut input = Vec::with_capacity(CONTEXT + CHUNK);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(chunk);
        self.context.copy_from_slice(&chunk[CHUNK - CONTEXT..]);
        let x = Tensor::from_array(([1usize, CONTEXT + CHUNK], input))?;
        let state = Tensor::from_array(([2usize, 1, 128], std::mem::take(&mut self.state)))?;
        let sr = Tensor::from_array(([0usize; 0], vec![super::LISTEN_RATE as i64]))?;
        let out = self.session.run(ort::inputs!["input" => x, "state" => state, "sr" => sr])?;
        let (_, p) = out["output"].try_extract_tensor::<f32>()?;
        let p = p[0];
        let (_, s) = out["stateN"].try_extract_tensor::<f32>()?;
        self.state = s.to_vec();
        Ok(p)
    }
}
