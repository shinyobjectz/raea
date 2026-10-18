//! Open, portable embeddings for ad cards (docs/ad-card.feature). The founder, 2026-10-11: "ship embeddings that are not
//! apple native… an embedding we can actually take with us", then "siglip + yamnet".
//!
//! SigLIP 2 base (google/siglip2-base-patch16-256, Apache-2.0) as ONNX, run with ONNX Runtime: the same vectors on the
//! Mac, the iPhone and any server. Frames become 768 numbers; short label phrases ("hands holding a product") become
//! 768 numbers in the same space, so a frame can be scored against labels with no model call (zero-shot). The phrases
//! are embedded once on the Mac and shipped as a table, so only the vision model needs to ship.
//!
//! Model folder (RAEA_MODELS, else UGC_MODELS, default ~/.local/share/raea-models/siglip2-base-p16-256): onnx/vision_model.onnx (fp32,
//! the reference the server runs), onnx/text_model.onnx (or text_model_fp16.onnx) and tokenizer.json (label phrases,
//! Mac only). The phone runs coreml/ImageEncoder.mlpackage (8-bit palettized, Apache-2.0), which matches the
//! reference at cosine 0.9956 mean / 0.987 worst on our frames (T-410, 2026-10-13). 256, not 224: the 224 model's
//! int8 builds broke on the phone (cosine 0.60 ONNX int8, 0.86 Core ML int8), and the 256 Core ML build is proven.

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::{Path, PathBuf};
use tokenizers::{PaddingDirection, PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

/// What a stored vector was made with: a vector is only ever compared with one of the same id.
pub const MODEL_ID: &str = "siglip2-base-p16-256";
pub const DIM: usize = 768;
const SIZE: u32 = 256;
/// SigLIP 2's text side reads 64 tokens, padded to that length (it was trained that way).
const TEXT_LEN: usize = 64;

pub fn default_dir() -> PathBuf {
    std::env::var_os("RAEA_MODELS")
        .or_else(|| std::env::var_os("UGC_MODELS"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/raea-models"))
        .join(MODEL_ID)
}

fn session(file: &Path) -> Result<Session, String> {
    let threads = std::thread::available_parallelism().map(|n| n.get().min(4)).unwrap_or(2);
    (|| -> ort::Result<Session> {
        Session::builder()?.with_optimization_level(GraphOptimizationLevel::Level3)?.with_intra_threads(threads)?.commit_from_file(file)
    })()
    .map_err(|e| format!("{}: {e}", file.display()))
}

/// The output that holds the pooled embedding: by name when the export names it, else the 2-D one.
fn pooled(s: &Session) -> usize {
    let names: Vec<&str> = s.outputs().iter().map(|o| o.name()).collect();
    for want in ["image_embeds", "text_embeds", "pooler_output"] {
        if let Some(i) = names.iter().position(|n| *n == want) {
            return i;
        }
    }
    names.len().saturating_sub(1)
}

pub struct Vision {
    session: Session,
    input: String,
    output: usize,
}

impl Vision {
    /// `int8`: the shipping model; otherwise the fp32 reference.
    pub fn load(dir: &Path, int8: bool) -> Result<Vision, String> {
        let file = dir.join("onnx").join(if int8 { "vision_model_int8.onnx" } else { "vision_model.onnx" });
        let session = session(&file)?;
        let input = session.inputs().first().map(|i| i.name().to_string()).ok_or("the vision model has no input")?;
        let output = pooled(&session);
        Ok(Vision { session, input, output })
    }

    /// An RGB image (any size) to its L2-normalised vector. Resized straight to 256x256 (no crop, as SigLIP's
    /// processor does), scaled to -1..1.
    pub fn embed_rgb(&mut self, rgb: &image::RgbImage) -> Result<Vec<f32>, String> {
        let img = image::imageops::resize(rgb, SIZE, SIZE, image::imageops::FilterType::Triangle);
        let n = (SIZE * SIZE) as usize;
        let mut px = vec![0f32; 3 * n];
        for (i, p) in img.pixels().enumerate() {
            for c in 0..3 {
                px[c * n + i] = (p[c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
        let t = Tensor::from_array(([1usize, 3, SIZE as usize, SIZE as usize], px)).map_err(|e| e.to_string())?;
        let out = self.session.run(ort::inputs![self.input.as_str() => t]).map_err(|e| format!("vision run: {e}"))?;
        let (_, data) = out[self.output].try_extract_tensor::<f32>().map_err(|e| format!("vision output: {e}"))?;
        if data.len() < DIM {
            return Err(format!("vision output has {} numbers, not {DIM}", data.len()));
        }
        Ok(normalised(&data[..DIM]))
    }

    pub fn embed_file(&mut self, path: &Path) -> Result<Vec<f32>, String> {
        let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?.to_rgb8();
        self.embed_rgb(&img)
    }
}

pub struct Text {
    session: Session,
    tokenizer: Tokenizer,
    input: String,
    output: usize,
}

impl Text {
    pub fn load(dir: &Path) -> Result<Text, String> {
        let onnx = dir.join("onnx");
        let file = ["text_model_fp16.onnx", "text_model.onnx"].iter().map(|f| onnx.join(f)).find(|f| f.exists()).unwrap_or_else(|| onnx.join("text_model.onnx"));
        let session = session(&file)?;
        let mut tokenizer = Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| format!("tokenizer.json: {e}"))?;
        let pad_id = tokenizer.token_to_id("<pad>").unwrap_or(0);
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(TEXT_LEN),
            direction: PaddingDirection::Right,
            pad_id,
            pad_token: "<pad>".into(),
            ..Default::default()
        }));
        tokenizer.with_truncation(Some(TruncationParams { max_length: TEXT_LEN, ..Default::default() })).map_err(|e| e.to_string())?;
        let input = session.inputs().first().map(|i| i.name().to_string()).ok_or("the text model has no input")?;
        let output = pooled(&session);
        Ok(Text { session, tokenizer, input, output })
    }

    /// A short phrase to its L2-normalised vector (lowercased, as SigLIP was trained).
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let enc = self.tokenizer.encode(text.to_lowercase(), true).map_err(|e| e.to_string())?;
        let ids: Vec<i64> = enc.get_ids().iter().map(|x| *x as i64).collect();
        let t = Tensor::from_array(([1usize, ids.len()], ids)).map_err(|e| e.to_string())?;
        let out = self.session.run(ort::inputs![self.input.as_str() => t]).map_err(|e| format!("text run: {e}"))?;
        let (_, data) = out[self.output].try_extract_tensor::<f32>().map_err(|e| format!("text output: {e}"))?;
        if data.len() < DIM {
            return Err(format!("text output has {} numbers, not {DIM}", data.len()));
        }
        Ok(normalised(&data[..DIM]))
    }
}

pub fn normalised(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter().map(|x| x / n).collect()
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// A label table: each label's phrases embedded and averaged (prompt ensembling), grouped by what they classify
/// ("format", "scene"…). Built once on the Mac, shipped as JSON.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LabelTable {
    pub model: String,
    pub groups: Vec<LabelGroup>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LabelGroup {
    pub name: String,
    pub labels: Vec<Label>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Label {
    pub name: String,
    pub phrases: Vec<String>,
    #[serde(default)]
    pub v: Vec<f32>,
}

impl LabelTable {
    /// Each group's labels ranked for one image vector: (label, cosine), best first.
    pub fn score(&self, image: &[f32]) -> Vec<(String, Vec<(String, f32)>)> {
        self.groups
            .iter()
            .map(|g| {
                let mut s: Vec<(String, f32)> = g.labels.iter().map(|l| (l.name.clone(), cosine(image, &l.v))).collect();
                s.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                (g.name.clone(), s)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectors_are_unit_length_and_cosine_is_their_dot() {
        let v = normalised(&[3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn labels_rank_by_cosine() {
        let t = LabelTable {
            model: MODEL_ID.into(),
            groups: vec![LabelGroup {
                name: "format".into(),
                labels: vec![
                    Label { name: "a".into(), phrases: vec![], v: vec![1.0, 0.0] },
                    Label { name: "b".into(), phrases: vec![], v: vec![0.0, 1.0] },
                ],
            }],
        };
        let s = t.score(&normalised(&[0.2, 0.9]));
        assert_eq!(s[0].1[0].0, "b");
    }
}

/// YAMNet (google/yamnet, Apache-2.0) as ONNX for sound: a 16 kHz mono waveform to 521 AudioSet class scores and a
/// 1,024-number embedding per 0.48 s frame, averaged over the clip. Used on the stem without the voice (music and
/// effects); the voice itself is read by Parakeet in the app. Model: <models>/yamnet/yamnet.onnx, labels beside it.
pub const AUDIO_MODEL_ID: &str = "yamnet-audioset";
pub const AUDIO_DIM: usize = 1024;
pub const AUDIO_CLASSES: usize = 521;
pub const AUDIO_RATE: u32 = 16_000;

pub struct Audio {
    session: Session,
    input: String,
    scores: usize,
    embeddings: usize,
    pub labels: Vec<String>,
}

/// YAMNet's hop between frames, in seconds (each frame reads 0.96 s).
pub const AUDIO_HOP_S: f64 = 0.48;

/// A clip's sound frame by frame: class scores (0..1, AudioSet order) and embeddings, one row per 0.48 s hop.
pub struct SoundFrames {
    pub scores: Vec<Vec<f32>>,
    pub embeddings: Vec<Vec<f32>>,
}

/// One clip's sound: mean class scores (0..1, AudioSet order) and the L2-normalised mean embedding.
pub struct Sound {
    pub scores: Vec<f32>,
    pub embedding: Vec<f32>,
}

impl Audio {
    /// `dir`: the yamnet model folder (yamnet.onnx and yamnet_class_map.csv).
    pub fn load(dir: &Path) -> Result<Audio, String> {
        let session = session(&dir.join("yamnet.onnx"))?;
        let input = session.inputs().first().map(|i| i.name().to_string()).ok_or("yamnet has no input")?;
        let names: Vec<String> = session.outputs().iter().map(|o| o.name().to_string()).collect();
        let at = |w: &str| names.iter().position(|n| n == w).ok_or(format!("yamnet has no '{w}' output"));
        let (scores, embeddings) = (at("scores")?, at("embeddings")?);
        let csv = std::fs::read_to_string(dir.join("yamnet_class_map.csv")).unwrap_or_default();
        let labels = csv.lines().skip(1).filter_map(|l| l.splitn(3, ',').nth(2)).map(|s| s.trim_matches('"').to_string()).collect();
        Ok(Audio { session, input, scores, embeddings, labels })
    }

    /// 16 kHz mono samples in -1..1. Clips shorter than one frame (0.975 s) are padded with silence.
    pub fn embed_pcm(&mut self, pcm: &[f32]) -> Result<Sound, String> {
        let f = self.frames_pcm(pcm)?;
        let mean = |rows: &[Vec<f32>], width: usize| {
            let mut m = vec![0f32; width];
            for r in rows {
                for (v, x) in m.iter_mut().zip(r) {
                    *v += x / rows.len().max(1) as f32;
                }
            }
            m
        };
        Ok(Sound { scores: mean(&f.scores, AUDIO_CLASSES), embedding: normalised(&mean(&f.embeddings, AUDIO_DIM)) })
    }

    /// Per frame, not averaged: YAMNet's frames are 0.96 s windows every `AUDIO_HOP_S` (0.48 s), frame i starting at
    /// i * 0.48 s. Embeddings are as YAMNet gives them (not normalised), so a caller can tell quiet from loud.
    pub fn frames_pcm(&mut self, pcm: &[f32]) -> Result<SoundFrames, String> {
        let mut w = pcm.to_vec();
        if w.len() < 15_600 {
            w.resize(15_600, 0.0);
        }
        let n = w.len();
        let t = Tensor::from_array(([n], w)).map_err(|e| e.to_string())?;
        let out = self.session.run(ort::inputs![self.input.as_str() => t]).map_err(|e| format!("yamnet run: {e}"))?;
        let rows = |i: usize, width: usize| -> Result<Vec<Vec<f32>>, String> {
            let (_, d) = out[i].try_extract_tensor::<f32>().map_err(|e| format!("yamnet output: {e}"))?;
            Ok(d.chunks_exact(width).map(|r| r.to_vec()).collect())
        };
        Ok(SoundFrames { scores: rows(self.scores, AUDIO_CLASSES)?, embeddings: rows(self.embeddings, AUDIO_DIM)? })
    }

    /// The top `k` AudioSet labels of a clip, highest first.
    pub fn top(&self, sound: &Sound, k: usize) -> Vec<(String, f32)> {
        let mut ix: Vec<usize> = (0..sound.scores.len()).collect();
        ix.sort_by(|a, b| sound.scores[*b].total_cmp(&sound.scores[*a]));
        ix.into_iter().take(k).map(|i| (self.labels.get(i).cloned().unwrap_or_default(), sound.scores[i])).collect()
    }
}

/// A 16 kHz mono WAV (16-bit PCM or 32-bit float) to samples. Anything else should be converted first
/// (`ffmpeg -i in -ac 1 -ar 16000 out.wav`); the app hands PCM straight to `embed_pcm`.
pub fn read_wav_16k(path: &Path) -> Result<Vec<f32>, String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err(format!("{}: not a WAV file", path.display()));
    }
    let (mut fmt, mut i) = (None, 12usize);
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let body = &b[i + 8..(i + 8 + len).min(b.len())];
        if id == b"fmt " && body.len() >= 16 {
            let u16at = |o: usize| u16::from_le_bytes([body[o], body[o + 1]]);
            fmt = Some((u16at(0), u16at(2), u32::from_le_bytes([body[4], body[5], body[6], body[7]]), u16at(14)));
        } else if id == b"data" {
            let (format, channels, rate, bits) = fmt.ok_or("WAV data before its format")?;
            if channels != 1 || rate != AUDIO_RATE {
                return Err(format!("{}: needs 16 kHz mono, has {rate} Hz × {channels}", path.display()));
            }
            return match (format, bits) {
                (1, 16) => Ok(body.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0).collect()),
                (3, 32) => Ok(body.chunks_exact(4).map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]])).collect()),
                _ => Err(format!("{}: unsupported WAV encoding {format}/{bits}-bit", path.display())),
            };
        }
        i += 8 + len + (len & 1);
    }
    Err(format!("{}: no audio data", path.display()))
}
