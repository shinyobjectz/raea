//! Parakeet TDT 0.6B v3 (NVIDIA, CC-BY-4.0) on ONNX: an utterance's words, punctuated.
//!
//! istupakov's export, run as onnx-asr runs it (models/nemo.py, asr.py): nemo128.onnx turns samples into 128 log-mel
//! features, the int8 encoder gives one 1024-d vector per 80 ms, and the decoder+joint is stepped greedily over them.
//! Its output is the 8193 token logits followed by 5 duration logits (skip 0-4 frames). The decoder's state moves on only
//! when a token is emitted; at most 10 tokens come out of one frame.

use std::collections::HashMap;
use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

const MAX_TOKENS_PER_STEP: usize = 10;

pub struct Parakeet {
    preprocessor: Session,
    encoder: Session,
    decoder: Session,
    vocab: Vec<String>,
    blank: usize,
}

impl Parakeet {
    pub const SPEC: super::models::Spec = super::models::Spec {
        name: "parakeet-tdt-0.6b-v3",
        files: &[
            ("istupakov/parakeet-tdt-0.6b-v3-onnx", "nemo128.onnx", "nemo128.onnx"),
            ("istupakov/parakeet-tdt-0.6b-v3-onnx", "encoder-model.int8.onnx", "encoder-model.int8.onnx"),
            ("istupakov/parakeet-tdt-0.6b-v3-onnx", "decoder_joint-model.int8.onnx", "decoder_joint-model.int8.onnx"),
            ("istupakov/parakeet-tdt-0.6b-v3-onnx", "vocab.txt", "vocab.txt"),
        ],
    };

    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let mut ids: HashMap<usize, String> = HashMap::new();
        for line in std::fs::read_to_string(dir.join("vocab.txt"))?.lines() {
            let Some((token, id)) = line.rsplit_once(' ') else { continue };
            ids.insert(id.parse()?, token.replace('\u{2581}', " "));
        }
        let mut vocab = vec![String::new(); ids.len()];
        for (id, token) in ids {
            anyhow::ensure!(id < vocab.len(), "vocab.txt: id {id} out of range");
            vocab[id] = token;
        }
        let blank = vocab.iter().position(|t| t == "<blk>").unwrap_or(vocab.len() - 1);
        Ok(Parakeet {
            preprocessor: super::session(&dir.join("nemo128.onnx"))?,
            encoder: super::session(&dir.join("encoder-model.int8.onnx"))?,
            decoder: super::session(&dir.join("decoder_joint-model.int8.onnx"))?,
            vocab,
            blank,
        })
    }

    /// 16 kHz mono samples of one utterance to text.
    pub fn transcribe(&mut self, audio: &[f32]) -> anyhow::Result<String> {
        let tokens = self.decode(audio)?;
        let joined: String = tokens.iter().map(|&(i, _)| self.vocab[i].as_str()).collect();
        Ok(spaces(&joined))
    }

    /// Any length of 16 kHz mono audio to words with times (seconds): cut into pieces of at most 20 s at their quietest
    /// 50 ms (so no word is split), each decoded alone. A word starts at a token beginning with a space; it spans from
    /// its first token's frame to the next word's (or its last token plus its frame step), frames being 80 ms.
    pub fn words(&mut self, audio: &[f32]) -> anyhow::Result<Vec<Word>> {
        let mut out = Vec::new();
        let mut at = 0usize;
        while at < audio.len() {
            let end = cut_point(audio, at, 20 * 16_000);
            let offset = at as f64 / 16_000.0;
            let tokens = self.decode(&audio[at..end])?;
            let mut piece: Vec<Word> = Vec::new();
            for &(tok, frame) in &tokens {
                let text = &self.vocab[tok];
                let t = offset + frame as f64 * 0.08;
                if text.starts_with(' ') || piece.is_empty() {
                    piece.push(Word { text: text.trim_start().to_string(), start: t, end: t + 0.08 });
                } else if let Some(last) = piece.last_mut() {
                    last.text.push_str(text);
                    last.end = t + 0.08;
                }
            }
            // a word ends where the next begins, if that's sooner than its own last token allows
            for i in 1..piece.len() {
                let next = piece[i].start;
                if piece[i - 1].end > next {
                    piece[i - 1].end = next;
                }
            }
            out.extend(piece.into_iter().filter(|w| !w.text.is_empty()));
            at = end;
        }
        Ok(out)
    }

    /// Greedy TDT over one piece of audio: each emitted token and the encoder frame (80 ms) it was emitted at.
    fn decode(&mut self, audio: &[f32]) -> anyhow::Result<Vec<(usize, usize)>> {
        if audio.is_empty() {
            return Ok(Vec::new());
        }
        // samples -> features [1, 128, T]
        let w = Tensor::from_array(([1usize, audio.len()], audio.to_vec()))?;
        let wl = Tensor::from_array(([1usize], vec![audio.len() as i64]))?;
        let out = self.preprocessor.run(ort::inputs!["waveforms" => w, "waveforms_lens" => wl])?;
        let (shape, feats) = out["features"].try_extract_tensor::<f32>()?;
        let shape: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
        let feats = feats.to_vec();
        let (_, flen) = out["features_lens"].try_extract_tensor::<i64>()?;
        let flen = flen[0];
        drop(out);

        // features -> encodings [1, 1024, T']
        let f = Tensor::from_array((shape, feats))?;
        let fl = Tensor::from_array(([1usize], vec![flen]))?;
        let out = self.encoder.run(ort::inputs!["audio_signal" => f, "length" => fl])?;
        let (eshape, enc) = out["outputs"].try_extract_tensor::<f32>()?;
        let (dim, frames) = (eshape[1] as usize, eshape[2] as usize);
        let enc = enc.to_vec();
        let (_, elen) = out["encoded_lengths"].try_extract_tensor::<i64>()?;
        let len = (elen[0].max(0) as usize).min(frames);
        drop(out);

        // greedy TDT
        let mut state = (vec![0f32; 2 * 640], vec![0f32; 2 * 640]);
        let mut tokens: Vec<(usize, usize)> = Vec::new();
        let mut column = vec![0f32; dim];
        let (mut t, mut emitted) = (0usize, 0usize);
        while t < len {
            for d in 0..dim {
                column[d] = enc[d * frames + t];
            }
            let prev = tokens.last().map(|t| t.0).unwrap_or(self.blank) as i32;
            let e = Tensor::from_array(([1usize, dim, 1], column.clone()))?;
            let tg = Tensor::from_array(([1usize, 1], vec![prev]))?;
            let tl = Tensor::from_array(([1usize], vec![1i32]))?;
            let s1 = Tensor::from_array(([2usize, 1, 640], state.0.clone()))?;
            let s2 = Tensor::from_array(([2usize, 1, 640], state.1.clone()))?;
            let out = self.decoder.run(ort::inputs![
                "encoder_outputs" => e, "targets" => tg, "target_length" => tl,
                "input_states_1" => s1, "input_states_2" => s2
            ])?;
            let (_, logits) = out["outputs"].try_extract_tensor::<f32>()?;
            let v = self.vocab.len();
            let token = argmax(&logits[..v]);
            let step = argmax(&logits[v..]);
            if token != self.blank {
                let (_, a) = out["output_states_1"].try_extract_tensor::<f32>()?;
                let (_, b) = out["output_states_2"].try_extract_tensor::<f32>()?;
                state = (a.to_vec(), b.to_vec());
                tokens.push((token, t));
                emitted += 1;
            }
            if step > 0 {
                t += step;
                emitted = 0;
            } else if token == self.blank || emitted == MAX_TOKENS_PER_STEP {
                t += 1;
                emitted = 0;
            }
        }
        Ok(tokens)
    }
}

/// One word heard, with its time in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Where to end a piece starting at `at` of at most `max` samples: the end if it's near, else the quietest 50 ms in
/// its last third.
fn cut_point(audio: &[f32], at: usize, max: usize) -> usize {
    if audio.len() - at <= max {
        return audio.len();
    }
    let win = 800;
    let (lo, hi) = (at + max * 2 / 3, at + max - win);
    (lo..hi)
        .step_by(160)
        .min_by(|&a, &b| {
            let e = |s: usize| audio[s..s + win].iter().map(|x| x * x).sum::<f32>();
            e(a).total_cmp(&e(b))
        })
        .map(|s| s + win / 2)
        .unwrap_or(at + max)
}

fn argmax(x: &[f32]) -> usize {
    x.iter().enumerate().fold((0, f32::NEG_INFINITY), |b, (i, &v)| if v > b.1 { (i, v) } else { b }).0
}

/// onnx-asr's DECODE_SPACE_PATTERN (`\A\s|\s\B|(\s)\b` -> " " only for the last): no leading space, and a space stays
/// only before a word character (none before punctuation).
fn spaces(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if c.is_whitespace() {
            let next_word = chars.get(i + 1).is_some_and(|n| n.is_alphanumeric() || *n == '_');
            if i == 0 || !next_word {
                continue;
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}
