//! Parakeet TDT 0.6b v3 (NVIDIA, CC-BY-4.0) on ONNX: what was said. The export is istupakov/parakeet-tdt-0.6b-v3-onnx
//! (int8 encoder and decoder, NeMo's 128-bin front end as a model of its own), decoded as onnx-asr's
//! NemoConformerTdt decodes it: greedy, each step's token and how many encoder frames to skip.

use std::path::Path;

use anyhow::{anyhow, Context};
use ort::session::Session;
use ort::value::Tensor;
use raea_sys_ml::models::Spec;

const REPO: &str = "istupakov/parakeet-tdt-0.6b-v3-onnx";
pub const SPEC: Spec = Spec {
    name: "parakeet-tdt-0.6b-v3",
    files: &[
        (REPO, "nemo128.onnx", "nemo128.onnx"),
        (REPO, "encoder-model.int8.onnx", "encoder-model.int8.onnx"),
        (REPO, "decoder_joint-model.int8.onnx", "decoder_joint-model.int8.onnx"),
        (REPO, "vocab.txt", "vocab.txt"),
    ],
};

/// The prediction network's state: two layers of 640.
const STATE: usize = 2 * 640;
/// Tokens one encoder frame may give before the decoder moves on (onnx-asr's max_tokens_per_step).
const PER_STEP: usize = 10;

pub struct Asr {
    front: Session,
    encoder: Session,
    decoder: Session,
    vocab: Vec<String>,
}

fn err(e: ort::Error) -> anyhow::Error {
    anyhow!("parakeet: {e}")
}

impl Asr {
    pub fn load(dir: &Path) -> anyhow::Result<Asr> {
        let vocab = std::fs::read_to_string(dir.join("vocab.txt")).context("parakeet's vocab.txt")?;
        // "token id" a line; the token itself can hold a space's mark but never a plain space
        let vocab: Vec<String> = vocab.lines().filter_map(|l| l.rsplit_once(' ').map(|(t, _)| t.to_string())).collect();
        anyhow::ensure!(vocab.last().map(String::as_str) == Some("<blk>"), "parakeet's vocab.txt doesn't end in its blank");
        Ok(Asr {
            front: raea_sys_ml::session(&dir.join("nemo128.onnx"))?,
            encoder: raea_sys_ml::session(&dir.join("encoder-model.int8.onnx"))?,
            decoder: raea_sys_ml::session(&dir.join("decoder_joint-model.int8.onnx"))?,
            vocab,
        })
    }

    /// The words in `x` (16 kHz mono), as one line.
    pub fn hear(&mut self, x: &[f32]) -> anyhow::Result<String> {
        if x.len() < 1600 {
            return Ok(String::new());
        }
        let wave = Tensor::from_array(([1usize, x.len()], x.to_vec())).map_err(err)?;
        let len = Tensor::from_array(([1usize], vec![x.len() as i64])).map_err(err)?;
        let (shape, feats, flen) = {
            let o = self.front.run(ort::inputs!["waveforms" => wave, "waveforms_lens" => len]).map_err(err)?;
            let (shape, f) = o["features"].try_extract_tensor::<f32>().map_err(err)?;
            let (_, l) = o["features_lens"].try_extract_tensor::<i64>().map_err(err)?;
            (shape.iter().map(|d| *d as usize).collect::<Vec<_>>(), f.to_vec(), l[0])
        };
        let feats = Tensor::from_array((shape, feats)).map_err(err)?;
        let flen = Tensor::from_array(([1usize], vec![flen])).map_err(err)?;
        let (dim, frames, enc) = {
            let o = self.encoder.run(ort::inputs!["audio_signal" => feats, "length" => flen]).map_err(err)?;
            let (shape, e) = o["outputs"].try_extract_tensor::<f32>().map_err(err)?;
            let (_, l) = o["encoded_lengths"].try_extract_tensor::<i64>().map_err(err)?;
            (shape[1] as usize, (shape[2] as usize, l[0] as usize), e.to_vec())
        };
        let (width, frames) = frames;
        let blank = self.vocab.len() - 1;
        let (mut s1, mut s2) = (vec![0f32; STATE], vec![0f32; STATE]);
        let mut tokens: Vec<usize> = Vec::new();
        let (mut t, mut emitted) = (0usize, 0usize);
        while t < frames.min(width) {
            let frame: Vec<f32> = (0..dim).map(|d| enc[d * width + t]).collect();
            let last = tokens.last().copied().unwrap_or(blank) as i32;
            let o = self
                .decoder
                .run(ort::inputs![
                    "encoder_outputs" => Tensor::from_array(([1usize, dim, 1], frame)).map_err(err)?,
                    "targets" => Tensor::from_array(([1usize, 1], vec![last])).map_err(err)?,
                    "target_length" => Tensor::from_array(([1usize], vec![1i32])).map_err(err)?,
                    "input_states_1" => Tensor::from_array(([2usize, 1, 640], s1.clone())).map_err(err)?,
                    "input_states_2" => Tensor::from_array(([2usize, 1, 640], s2.clone())).map_err(err)?,
                ])
                .map_err(err)?;
            let (_, logits) = o["outputs"].try_extract_tensor::<f32>().map_err(err)?;
            let best = |v: &[f32]| v.iter().enumerate().fold((0usize, f32::NEG_INFINITY), |b, (i, x)| if *x > b.1 { (i, *x) } else { b }).0;
            let token = best(&logits[..self.vocab.len()]);
            let skip = best(&logits[self.vocab.len()..]);
            if token != blank {
                s1.copy_from_slice(&o["output_states_1"].try_extract_tensor::<f32>().map_err(err)?.1[..STATE]);
                s2.copy_from_slice(&o["output_states_2"].try_extract_tensor::<f32>().map_err(err)?.1[..STATE]);
                tokens.push(token);
                emitted += 1;
            }
            if skip > 0 {
                t += skip;
                emitted = 0;
            } else if token == blank || emitted == PER_STEP {
                t += 1;
                emitted = 0;
            }
        }
        Ok(self.text(&tokens))
    }

    /// Tokens as a line: a token's leading mark is the space before a word.
    fn text(&self, tokens: &[usize]) -> String {
        let joined: String = tokens.iter().map(|t| self.vocab[*t].as_str()).filter(|t| !t.starts_with("<|")).collect();
        joined.replace('\u{2581}', " ").split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
