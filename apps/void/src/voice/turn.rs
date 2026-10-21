//! Smart Turn v3 (pipecat-ai, BSD-2) on ONNX: at a pause, how sure it is the speaker has finished (0-1), from the
//! last 8 s of the utterance as Whisper log-mel features (mel.rs).
//!
//! As pipecat's inference.py: keep the last 8 s, normalise the samples (zero mean, unit variance), pad with zeros on the
//! right to 8 s, Whisper features [1, 80, 800]; the graph ends in its sigmoid, so the output is the probability.
//!
//! The fp32 graph ("-gpu", 32 MB), not the int8 "-cpu" one: on 2026-10-15 the int8 graph moved by up to 0.13 when its
//! input moved by 2e-5 (midpause.wav at 4.81 s: 0.18 or 0.30), while fp32 didn't move in the third decimal, and fp32
//! ran faster on this Mac's ORT (14 ms against 37 ms a call).

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

const WINDOW: usize = 8 * super::LISTEN_RATE as usize;

pub struct SmartTurn {
    session: Session,
}

impl SmartTurn {
    pub const SPEC: super::models::Spec = super::models::Spec {
        name: "smart-turn-v3",
        files: &[("pipecat-ai/smart-turn-v3", "smart-turn-v3.2-gpu.onnx", "smart-turn-v3.2-gpu.onnx")],
    };

    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        Ok(SmartTurn { session: super::session(&dir.join("smart-turn-v3.2-gpu.onnx"))? })
    }

    /// The utterance so far (16 kHz mono; only its last 8 s are read). Probability the turn is complete.
    pub fn complete(&mut self, audio: &[f32]) -> anyhow::Result<f32> {
        let tail = &audio[audio.len().saturating_sub(WINDOW)..];
        let feats = super::mel::whisper(&super::mel::normalised_padded(tail, WINDOW));
        let frames = feats.len() / super::mel::N_MELS;
        let x = Tensor::from_array(([1usize, super::mel::N_MELS, frames], feats))?;
        let out = self.session.run(ort::inputs!["input_features" => x])?;
        let (_, p) = out[0].try_extract_tensor::<f32>()?;
        Ok(p[0])
    }
}
