//! Pocket TTS (Kyutai, 100M, CC-BY-4.0) on ONNX, streamed: each few 80 ms Mimi frames go out as soon as they're decoded.
//!
//! The export is KevinAHM/pocket-tts-onnx's english_2026-04 bundle (int8 flow LM, flow head and Mimi decoder; the text
//! conditioner in fp32), and this is its pocket_tts_onnx.py loop in Rust: the voice's flow-LM state (a predefined voice
//! is a saved KV cache, kyutai/pocket-tts-without-voice-cloning) restored once per voice; per sentence, the text
//! embeddings through the flow LM, then one latent per frame (the flow head's LSD step from Gaussian noise at
//! temperature 0.7) until the EOS logit passes -4 and a few frames more; the Mimi decoder turns each two latents into
//! 3840 samples at 24 kHz, its own streaming state carried across the sentence. The tokenizer is mlx-community/pocket-tts's
//! tokenizer.json, which encodes exactly as the bundle's SentencePiece model.
//!
//! A voice can also be a clip: any WAV in ~/.raea-void/voices is a voice named after its file. Its audio goes through
//! the bundle's Mimi encoder (speaker projection included) behind the BOS embedding, and the flow LM's state after that
//! prompt is the voice, computed once per voice (pocket_tts get_state_for_audio_prompt; its 30 s cap, plus a trim of
//! the silence at either end).

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, bail, Context};
use ort::session::{Session, SessionInputValue};
use ort::value::{DynValue, Tensor};
use serde_json::Value;

const BUNDLE: &str = "KevinAHM/pocket-tts-onnx";
const VOICE_REPO: &str = "kyutai/pocket-tts-without-voice-cloning";

/// Frames (80 ms each) decoded per chunk: small, so the first sound comes after 160 ms of audio is generated.
const CHUNK_FRAMES: usize = 2;
const TEMPERATURE: f32 = 0.7;
/// The flow head's decode steps (the export's default).
const LSD_STEPS: usize = 1;
const EOS_LOGIT: f32 = -4.0;
/// The reference's cap on an audio prompt (get_state_for_audio_prompt's truncate).
const MAX_PROMPT_SECONDS: usize = 30;

/// One state tensor's initial value, kept in Rust so each sentence can start from a fresh copy.
#[derive(Clone)]
enum Data {
    F32(Vec<f32>),
    I64(Vec<i64>),
    Bool(Vec<bool>),
}

#[derive(Clone)]
struct Init {
    name: String,
    shape: Vec<i64>,
    data: Data,
}

impl Init {
    /// A state output kept as a fresh starting value.
    fn from_value(name: String, v: &DynValue) -> anyhow::Result<Init> {
        if let Ok((shape, d)) = v.try_extract_tensor::<f32>() {
            return Ok(Init { name, shape: shape.iter().copied().collect(), data: Data::F32(d.to_vec()) });
        }
        if let Ok((shape, d)) = v.try_extract_tensor::<i64>() {
            return Ok(Init { name, shape: shape.iter().copied().collect(), data: Data::I64(d.to_vec()) });
        }
        let (shape, d) = v.try_extract_tensor::<bool>()?;
        Ok(Init { name, shape: shape.iter().copied().collect(), data: Data::Bool(d.to_vec()) })
    }

    fn value(&self) -> anyhow::Result<DynValue> {
        let shape = self.shape.clone();
        Ok(match &self.data {
            Data::F32(v) => Tensor::from_array((shape, v.clone()))?.into_dyn(),
            Data::I64(v) => Tensor::from_array((shape, v.clone()))?.into_dyn(),
            Data::Bool(v) => Tensor::from_array((shape, v.clone()))?.into_dyn(),
        })
    }
}

/// A bundle.json state manifest as fresh tensors (its fills: nan, zeros, ones, empty).
fn manifest(entries: &Value) -> anyhow::Result<Vec<(Init, String, String)>> {
    let mut out = Vec::new();
    for e in entries.as_array().context("manifest")? {
        let shape: Vec<i64> = e["shape"].as_array().context("shape")?.iter().filter_map(Value::as_i64).collect();
        let n: usize = shape.iter().map(|&d| d as usize).product();
        let fill = e["fill"].as_str().unwrap_or("zeros");
        let data = match e["dtype"].as_str().unwrap_or("") {
            "float32" => Data::F32(vec![
                match fill {
                    "nan" => f32::NAN,
                    "ones" => 1.0,
                    _ => 0.0,
                };
                n
            ]),
            "int64" => Data::I64(vec![if fill == "ones" { 1 } else { 0 }; n]),
            "bool" => Data::Bool(vec![fill == "ones"; n]),
            other => bail!("state dtype {other}"),
        };
        let name = e["input_name"].as_str().context("input_name")?.to_string();
        let module = e["module"].as_str().unwrap_or("").to_string();
        let key = e["key"].as_str().unwrap_or("").to_string();
        out.push((Init { name, shape, data }, module, key));
    }
    Ok(out)
}

/// A safetensors file's tensors (F32 and I64 only, all a voice state holds): name -> (shape, data).
fn safetensors(file: &Path) -> anyhow::Result<HashMap<String, (Vec<i64>, Data)>> {
    let bytes = std::fs::read(file).with_context(|| file.display().to_string())?;
    let n = u64::from_le_bytes(bytes[..8].try_into()?) as usize;
    let header: Value = serde_json::from_slice(&bytes[8..8 + n])?;
    let body = &bytes[8 + n..];
    let mut out = HashMap::new();
    for (name, t) in header.as_object().context("safetensors header")? {
        if name == "__metadata__" {
            continue;
        }
        let shape: Vec<i64> = t["shape"].as_array().context("shape")?.iter().filter_map(Value::as_i64).collect();
        let off: Vec<usize> = t["data_offsets"].as_array().context("offsets")?.iter().filter_map(|v| v.as_u64().map(|x| x as usize)).collect();
        let raw = &body[off[0]..off[1]];
        let data = match t["dtype"].as_str().unwrap_or("") {
            "F32" => Data::F32(raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()),
            "I64" => Data::I64(raw.chunks_exact(8).map(|c| i64::from_le_bytes(c.try_into().unwrap())).collect()),
            other => bail!("{}: tensor {name} is {other}", file.display()),
        };
        out.insert(name.clone(), (shape, data));
    }
    Ok(out)
}

/// `src` (shape `ss`) copied into the leading corner of `dst` (shape `ds`), as pocket_tts_onnx.py's _adapt_state_tensor.
fn copy_corner<T: Copy>(src: &[T], ss: &[i64], dst: &mut [T], ds: &[i64]) {
    fn strides(s: &[i64]) -> Vec<usize> {
        let mut st = vec![1usize; s.len()];
        for i in (0..s.len().saturating_sub(1)).rev() {
            st[i] = st[i + 1] * s[i + 1] as usize;
        }
        st
    }
    let (sst, dst_st) = (strides(ss), strides(ds));
    let common: Vec<usize> = ss.iter().zip(ds).map(|(a, b)| (*a).min(*b) as usize).collect();
    fn rec<T: Copy>(d: usize, so: usize, doff: usize, c: &[usize], sst: &[usize], dstst: &[usize], src: &[T], dst: &mut [T]) {
        if d == c.len() - 1 {
            dst[doff..doff + c[d]].copy_from_slice(&src[so..so + c[d]]);
            return;
        }
        for i in 0..c[d] {
            rec(d + 1, so + i * sst[d], doff + i * dstst[d], c, sst, dstst, src, dst);
        }
    }
    if !common.is_empty() && common.iter().all(|&c| c > 0) {
        rec(0, 0, 0, &common, &sst, &dst_st, src, dst);
    }
}

/// Gaussian noise for the flow head: xorshift64* and Box-Muller.
struct Noise(u64);

impl Noise {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 + 0.5) / (1u64 << 24) as f32
    }

    fn normal(&mut self, std: f32) -> f32 {
        let (u, v) = (self.next_f32(), self.next_f32());
        (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos() * std
    }
}

struct Model {
    session: Session,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

impl Model {
    fn load(file: &Path) -> anyhow::Result<Self> {
        let session = super::session(file)?;
        let inputs = session.inputs().iter().map(|o| o.name().to_string()).collect();
        let outputs = session.outputs().iter().map(|o| o.name().to_string()).collect();
        Ok(Model { session, inputs, outputs })
    }

    /// Runs with `named` inputs plus `state` (by input name); returns every output in order. The state's outputs
    /// replace it (the outputs from `offset` on are the state, in manifest order).
    fn run(&mut self, named: Vec<(&str, DynValue)>, state: &mut [(String, DynValue)], offset: usize) -> anyhow::Result<Vec<DynValue>> {
        let mut inputs: Vec<(Cow<str>, SessionInputValue)> = named.into_iter().map(|(k, v)| (Cow::Owned(k.to_string()), v.into())).collect();
        for (name, v) in state.iter() {
            inputs.push((Cow::Owned(name.clone()), SessionInputValue::from(v)));
        }
        let mut out = self.session.run(inputs)?;
        let mut values = Vec::with_capacity(self.outputs.len());
        for name in &self.outputs {
            values.push(out.remove(name).ok_or_else(|| anyhow!("output {name} missing"))?);
        }
        drop(out);
        let rest = values.split_off(offset);
        for ((_, slot), v) in state.iter_mut().zip(rest) {
            *slot = v;
        }
        Ok(values)
    }
}

/// The executable that encodes a clip voice in a child process (`EXE --encode-voice NAME`); unset, it runs in-process.
pub static ENCODER: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub struct Pocket {
    tokenizer: tokenizers::Tokenizer,
    text_conditioner: Model,
    flow_main: Model,
    flow_head: Model,
    mimi: Model,
    /// Loaded on the first clip voice.
    mimi_encoder: Option<Model>,
    dir: std::path::PathBuf,
    flow_manifest: Vec<(Init, String, String)>,
    mimi_manifest: Vec<Init>,
    /// Each voice's flow-LM state after its prompt, loaded once.
    voices: HashMap<String, Vec<Init>>,
    sample_rate: u32,
    samples_per_frame: usize,
    frame_rate: f64,
    latent_dim: usize,
    conditioning_dim: usize,
    max_token_per_chunk: usize,
    eos_tokens: Vec<u32>,
    fallback_tokens: Vec<u32>,
    noise: Noise,
}

impl Pocket {
    pub const SPEC: super::models::Spec = super::models::Spec {
        name: "pocket-tts",
        files: &[
            (BUNDLE, "onnx/english_2026-04/bundle.json", "bundle.json"),
            (BUNDLE, "onnx/english_2026-04/text_conditioner.onnx", "text_conditioner.onnx"),
            (BUNDLE, "onnx/english_2026-04/flow_lm_main_int8.onnx", "flow_lm_main_int8.onnx"),
            (BUNDLE, "onnx/english_2026-04/flow_lm_flow_int8.onnx", "flow_lm_flow_int8.onnx"),
            (BUNDLE, "onnx/english_2026-04/mimi_decoder_int8.onnx", "mimi_decoder_int8.onnx"),
            // voice cloning: the encoder for clips, and the BOS embedding that goes before a clip's prompt
            (BUNDLE, "onnx/english_2026-04/mimi_encoder.onnx", "mimi_encoder.onnx"),
            (BUNDLE, "onnx/english_2026-04/bos_before_voice.npy", "bos_before_voice.npy"),
            ("mlx-community/pocket-tts", "tokenizer.json", "tokenizer.json"),
            (VOICE_REPO, "languages/english_2026-04/embeddings/alba.safetensors", "voice-alba.safetensors"),
            (VOICE_REPO, "languages/english_2026-04/embeddings/marius.safetensors", "voice-marius.safetensors"),
            (VOICE_REPO, "languages/english_2026-04/embeddings/javert.safetensors", "voice-javert.safetensors"),
            (VOICE_REPO, "languages/english_2026-04/embeddings/jean.safetensors", "voice-jean.safetensors"),
            (VOICE_REPO, "languages/english_2026-04/embeddings/fantine.safetensors", "voice-fantine.safetensors"),
            (VOICE_REPO, "languages/english_2026-04/embeddings/cosette.safetensors", "voice-cosette.safetensors"),
            (VOICE_REPO, "languages/english_2026-04/embeddings/eponine.safetensors", "voice-eponine.safetensors"),
            (VOICE_REPO, "languages/english_2026-04/embeddings/azelma.safetensors", "voice-azelma.safetensors"),
        ],
    };
    pub const VOICES: &[&str] = &["alba", "marius", "javert", "jean", "fantine", "cosette", "eponine", "azelma"];

    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let bundle: Value = serde_json::from_slice(&std::fs::read(dir.join("bundle.json"))?)?;
        let tokenizer = tokenizers::Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| anyhow!("tokenizer: {e}"))?;
        let flow_manifest = manifest(&bundle["flow_lm_state_manifest"])?;
        let mimi_manifest = manifest(&bundle["mimi_state_manifest"])?.into_iter().map(|(i, _, _)| i).collect();
        let num = |k: &str| bundle[k].as_f64().ok_or_else(|| anyhow!("bundle.json: {k}?"));
        let encode = |s: &str| -> anyhow::Result<Vec<u32>> {
            Ok(tokenizer.encode(s, false).map_err(|e| anyhow!("tokenize: {e}"))?.get_ids().to_vec())
        };
        // the boundaries pocket_tts_onnx.py splits on: the tokens of ".!...?" and ",;:" after the leading "▁"
        let eos_tokens = encode(".!...?")?[1..].to_vec();
        let fallback_tokens = encode(",;:")?[1..].to_vec();
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
        Ok(Pocket {
            text_conditioner: Model::load(&dir.join("text_conditioner.onnx"))?,
            flow_main: Model::load(&dir.join("flow_lm_main_int8.onnx"))?,
            flow_head: Model::load(&dir.join("flow_lm_flow_int8.onnx"))?,
            mimi: Model::load(&dir.join("mimi_decoder_int8.onnx"))?,
            mimi_encoder: None,
            dir: dir.to_path_buf(),
            flow_manifest,
            mimi_manifest,
            voices: HashMap::new(),
            sample_rate: num("sample_rate")? as u32,
            samples_per_frame: num("samples_per_frame")? as usize,
            frame_rate: num("frame_rate")?,
            latent_dim: num("latent_dim")? as usize,
            conditioning_dim: num("conditioning_dim")? as usize,
            max_token_per_chunk: bundle["max_token_per_chunk"].as_u64().unwrap_or(50) as usize,
            eos_tokens,
            fallback_tokens,
            tokenizer,
            noise: Noise(seed),
        })
    }

    /// Where clip voices live: ~/.raea-void/voices/<name>.wav.
    pub fn voice_dir() -> std::path::PathBuf {
        std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default().join(".raea-void/voices")
    }

    /// Every voice: the built-ins, then each WAV in voice_dir() by file name.
    pub fn voices() -> Vec<String> {
        let mut out: Vec<String> = Self::VOICES.iter().map(|v| v.to_string()).collect();
        let mut clips: Vec<String> = std::fs::read_dir(Self::voice_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")))
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .filter(|n| !Self::VOICES.contains(&n.as_str()))
            .collect();
        clips.sort();
        out.extend(clips);
        out
    }

    /// The voice to use when none is chosen: the founder's clip "watts" when it's there, otherwise alba.
    pub fn default_voice() -> String {
        if Self::voice_dir().join("watts.wav").exists() { "watts".into() } else { "alba".into() }
    }

    /// Output rate (24 kHz).
    /// Encode a clip voice's state to disk (the child process's whole job: `void --encode-voice NAME`).
    pub fn encode_voice(&mut self, voice: &str) -> anyhow::Result<()> {
        self.voice_state(voice).map(|_| ())
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// `text` in `voice`, streamed: `on_audio` gets mono samples at sample_rate() as frames decode. Returns early,
    /// without error, once `cancel` is set (barge-in).
    pub fn speak(&mut self, text: &str, voice: &str, cancel: &AtomicBool, on_audio: &mut dyn FnMut(&[f32])) -> anyhow::Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        let base = self.voice_state(voice)?;
        for chunk in self.split_into_best_sentences(text)? {
            if cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            let (prepared, guess) = prepare_text_prompt(&chunk);
            if prepared.is_empty() {
                continue;
            }
            let ids: Vec<i64> = self.encode(&prepared)?.into_iter().map(i64::from).collect();
            self.sentence(&base, ids, guess + 2, cancel, on_audio)?;
        }
        Ok(())
    }

    fn encode(&self, s: &str) -> anyhow::Result<Vec<u32>> {
        Ok(self.tokenizer.encode(s, false).map_err(|e| anyhow!("tokenize: {e}"))?.get_ids().to_vec())
    }

    fn decode(&self, ids: &[u32]) -> anyhow::Result<String> {
        self.tokenizer.decode(ids, true).map_err(|e| anyhow!("detokenize: {e}"))
    }

    /// One sentence: text conditioning, then frames until EOS (+ frames_after_eos), decoded two at a time.
    fn sentence(&mut self, base: &[Init], ids: Vec<i64>, frames_after_eos: usize, cancel: &AtomicBool, on_audio: &mut dyn FnMut(&[f32])) -> anyhow::Result<()> {
        let (ld, cd) = (self.latent_dim, self.conditioning_dim);
        let mut state: Vec<(String, DynValue)> = base.iter().map(|i| Ok((i.name.clone(), i.value()?))).collect::<anyhow::Result<_>>()?;
        let mut mimi: Vec<(String, DynValue)> = self.mimi_manifest.iter().map(|i| Ok((i.name.clone(), i.value()?))).collect::<anyhow::Result<_>>()?;

        let n_tokens = ids.len();
        let tc_input = self.text_conditioner.inputs[0].clone();
        let text = self.text_conditioner.run(vec![(&tc_input, Tensor::from_array(([1usize, n_tokens], ids))?.into_dyn())], &mut [], 1)?;
        let (shape, emb) = text[0].try_extract_tensor::<f32>()?;
        let m = if shape.len() == 2 { shape[0] as usize } else { shape[1] as usize };
        let text_embeddings = Tensor::from_array(([1usize, m, cd], emb.to_vec()))?.into_dyn();
        self.flow_main.run(
            vec![("sequence", Tensor::from_array(([1usize, 0, ld], Vec::<f32>::new()))?.into_dyn()), ("text_embeddings", text_embeddings)],
            &mut state,
            2,
        )?;

        let limit = ((n_tokens as f64 / 3.0 + 2.0) * self.frame_rate).ceil() as usize;
        let mut curr = vec![f32::NAN; ld];
        let mut eos_step: Option<usize> = None;
        let mut pending: Vec<f32> = Vec::new();
        let std = TEMPERATURE.sqrt();
        let dt = 1.0 / LSD_STEPS as f32;
        for step in 0..limit {
            if cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            let out = self.flow_main.run(
                vec![
                    ("sequence", Tensor::from_array(([1usize, 1, ld], curr.clone()))?.into_dyn()),
                    ("text_embeddings", Tensor::from_array(([1usize, 0, cd], Vec::<f32>::new()))?.into_dyn()),
                ],
                &mut state,
                2,
            )?;
            let (_, eos) = out[1].try_extract_tensor::<f32>()?;
            if eos_step.is_none() && eos[0] > EOS_LOGIT {
                eos_step = Some(step);
            }
            if eos_step.is_some_and(|e| step >= e + frames_after_eos) {
                break;
            }
            let (cshape, c) = out[0].try_extract_tensor::<f32>()?;
            let cshape: Vec<i64> = cshape.iter().copied().collect();
            let c = c.to_vec();
            let mut x: Vec<f32> = (0..ld).map(|_| self.noise.normal(std)).collect();
            for j in 0..LSD_STEPS {
                let s = j as f32 / LSD_STEPS as f32;
                let flow = self.flow_head.run(
                    vec![
                        ("c", Tensor::from_array((cshape.clone(), c.clone()))?.into_dyn()),
                        ("s", Tensor::from_array(([1usize, 1], vec![s]))?.into_dyn()),
                        ("t", Tensor::from_array(([1usize, 1], vec![s + dt]))?.into_dyn()),
                        ("x", Tensor::from_array(([1usize, ld], x.clone()))?.into_dyn()),
                    ],
                    &mut [],
                    1,
                )?;
                let (_, f) = flow[0].try_extract_tensor::<f32>()?;
                for (xi, fi) in x.iter_mut().zip(f) {
                    *xi += fi * dt;
                }
            }
            pending.extend_from_slice(&x);
            curr = x;
            if pending.len() >= CHUNK_FRAMES * ld {
                self.decode_frames(std::mem::take(&mut pending), &mut mimi, on_audio)?;
            }
        }
        if !pending.is_empty() && !cancel.load(Ordering::Relaxed) {
            self.decode_frames(pending, &mut mimi, on_audio)?;
        }
        Ok(())
    }

    fn decode_frames(&mut self, latents: Vec<f32>, mimi: &mut [(String, DynValue)], on_audio: &mut dyn FnMut(&[f32])) -> anyhow::Result<()> {
        let frames = latents.len() / self.latent_dim;
        let out = self.mimi.run(vec![("latent", Tensor::from_array(([1usize, frames, self.latent_dim], latents))?.into_dyn())], mimi, 1)?;
        let (_, audio) = out[0].try_extract_tensor::<f32>()?;
        debug_assert_eq!(audio.len(), frames * self.samples_per_frame);
        on_audio(audio);
        Ok(())
    }

    /// The flow-LM state after `voice`'s prompt: its saved KV caches placed into the manifest's full-size tensors
    /// (pocket_tts_onnx.py's _state_from_model_state).
    fn voice_state(&mut self, voice: &str) -> anyhow::Result<Vec<Init>> {
        if let Some(s) = self.voices.get(voice) {
            return Ok(s.clone());
        }
        let file = self.dir.join(format!("voice-{voice}.safetensors"));
        if !Self::VOICES.contains(&voice) || !file.exists() {
            let clip = Self::voice_dir().join(format!("{voice}.wav"));
            if clip.exists() {
                // the clip's state is saved next to it after the first encoding (the encoder's activations over a
                // 30 s clip peak near 3 GB, so it runs once per clip, not once per launch)
                let meta = std::fs::metadata(&clip)?;
                let stamp = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                let saved = Self::voice_dir().join(".state").join(format!("{voice}-{}-{stamp}.safetensors", meta.len()));
                let state = match saved.exists().then(|| safetensors(&saved)).transpose()? {
                    Some(s) => self.state_from_saved(voice, &s)?,
                    // the encoding's ~3 GB peak runs in a child process when the app names one (ENCODER), so the
                    // app's own footprint never holds it (memguard kills a job over 6 GB)
                    None if ENCODER.get().is_some() => {
                        let exe = ENCODER.get().unwrap();
                        let st = std::process::Command::new(exe).args(["--encode-voice", voice]).status()?;
                        anyhow::ensure!(st.success() && saved.exists(), "encoding {voice}'s clip failed ({st})");
                        self.state_from_saved(voice, &safetensors(&saved)?)?
                    }
                    None => {
                        let state = self.clip_state(&clip)?;
                        if let Err(e) = self.save_state(&state, &saved) {
                            eprintln!("pocket: couldn't save {voice}'s state: {e}");
                        }
                        state
                    }
                };
                self.voices.insert(voice.to_string(), state.clone());
                return Ok(state);
            }
            bail!("no Pocket voice {voice:?} (voices: {})", Self::voices().join(", "));
        }
        let state = self.state_from_saved(voice, &safetensors(&file)?)?;
        self.voices.insert(voice.to_string(), state.clone());
        Ok(state)
    }

    /// A saved voice state (Kyutai's format: "<module>/cache" sliced to the prompt, "<module>/offset") placed into the
    /// manifest's full-size tensors.
    fn state_from_saved(&self, voice: &str, saved: &HashMap<String, (Vec<i64>, Data)>) -> anyhow::Result<Vec<Init>> {
        let mut state = Vec::new();
        for (init, module, key) in &self.flow_manifest {
            let mut init = init.clone();
            let found = saved.get(&format!("{module}/{key}")).cloned().or_else(|| {
                // a step the file doesn't hold is its offset (the prompt's length)
                (key == "step").then(|| saved.get(&format!("{module}/offset")).cloned()).flatten()
            });
            if let Some((shape, data)) = found {
                match (&mut init.data, data) {
                    (Data::F32(dst), Data::F32(src)) => {
                        if src.len() == dst.len() {
                            *dst = src;
                        } else {
                            copy_corner(&src, &shape, dst, &init.shape);
                        }
                    }
                    (Data::I64(dst), Data::I64(src)) if src.len() == dst.len() => *dst = src,
                    (Data::I64(dst), Data::I64(src)) => copy_corner(&src, &shape, dst, &init.shape),
                    _ => bail!("{voice}: {module}/{key} has the wrong type"),
                }
            }
            state.push(init);
        }
        Ok(state)
    }

    /// A clip's voice: its audio (24 kHz mono, silence trimmed, at most 30 s) through the Mimi encoder, the BOS
    /// embedding before it, and the flow LM run over that prompt from an empty state.
    fn clip_state(&mut self, clip: &Path) -> anyhow::Result<Vec<Init>> {
        let audio = trim_silence(&read_wav(clip, self.sample_rate)?, self.sample_rate as usize);
        let audio: Vec<f32> = audio.into_iter().take(MAX_PROMPT_SECONDS * self.sample_rate as usize).collect();
        anyhow::ensure!(audio.len() > self.samples_per_frame, "{}: no speech in the clip", clip.display());
        if self.mimi_encoder.is_none() {
            self.mimi_encoder = Some(Model::load(&self.dir.join("mimi_encoder.onnx"))?);
        }
        let encoder = self.mimi_encoder.as_mut().unwrap();
        let n = audio.len();
        let out = encoder.run(vec![("audio", Tensor::from_array(([1usize, 1, n], audio))?.into_dyn())], &mut [], 1)?;
        let (shape, emb) = out[0].try_extract_tensor::<f32>()?;
        let cd = self.conditioning_dim;
        anyhow::ensure!(shape.last() == Some(&(cd as i64)), "mimi encoder gave {shape:?}");
        let mut prompt = read_npy_f32(&self.dir.join("bos_before_voice.npy"))?;
        prompt.extend_from_slice(emb);
        let frames = prompt.len() / cd;
        let mut state: Vec<(String, DynValue)> =
            self.flow_manifest.iter().map(|(i, _, _)| Ok((i.name.clone(), i.value()?))).collect::<anyhow::Result<_>>()?;
        self.flow_main.run(
            vec![
                ("sequence", Tensor::from_array(([1usize, 0, self.latent_dim], Vec::<f32>::new()))?.into_dyn()),
                ("text_embeddings", Tensor::from_array(([1usize, frames, cd], prompt))?.into_dyn()),
            ],
            &mut state,
            2,
        )?;
        state.into_iter().map(|(name, v)| Init::from_value(name, &v)).collect()
    }

    /// A voice state written as Kyutai's voice files are: each layer's KV cache cut to the prompt's length, and its
    /// offset.
    fn save_state(&self, state: &[Init], file: &Path) -> anyhow::Result<()> {
        let mut tensors: Vec<(String, Vec<i64>, Data)> = Vec::new();
        for (init, (_, module, key)) in state.iter().zip(&self.flow_manifest) {
            if key != "step" {
                continue;
            }
            let Data::I64(step) = &init.data else { bail!("{module}/step isn't int64") };
            let frames = step[0];
            tensors.push((format!("{module}/offset"), vec![1], Data::I64(vec![frames])));
            let cache = state.iter().zip(&self.flow_manifest).find(|(_, (_, m, k))| m == module && k == "cache").map(|(i, _)| i);
            let Some(Init { shape, data: Data::F32(full), .. }) = cache else { bail!("{module}/cache missing") };
            let mut cut_shape = shape.clone();
            cut_shape[2] = frames.min(shape[2]);
            let mut cut = vec![0f32; cut_shape.iter().map(|&d| d as usize).product()];
            copy_corner(full, shape, &mut cut, &cut_shape);
            tensors.push((format!("{module}/cache"), cut_shape, Data::F32(cut)));
        }
        let mut header = serde_json::Map::new();
        let mut body: Vec<u8> = Vec::new();
        for (name, shape, data) in &tensors {
            let start = body.len();
            let dtype = match data {
                Data::F32(v) => {
                    v.iter().for_each(|x| body.extend_from_slice(&x.to_le_bytes()));
                    "F32"
                }
                Data::I64(v) => {
                    v.iter().for_each(|x| body.extend_from_slice(&x.to_le_bytes()));
                    "I64"
                }
                Data::Bool(_) => bail!("bool in a flow state"),
            };
            header.insert(name.clone(), serde_json::json!({"dtype": dtype, "shape": shape, "data_offsets": [start, body.len()]}));
        }
        let mut head = serde_json::to_vec(&Value::Object(header))?;
        while head.len() % 8 != 0 {
            head.push(b' ');
        }
        std::fs::create_dir_all(file.parent().context("state path")?)?;
        let mut bytes = (head.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(&head);
        bytes.extend_from_slice(&body);
        let part = file.with_extension("part");
        std::fs::write(&part, bytes)?;
        std::fs::rename(part, file)?;
        Ok(())
    }

    /// pocket_tts_onnx.py's _split_into_best_sentences: split at sentence ends (then at commas when a sentence is longer
    /// than max_token_per_chunk tokens), and pack the pieces into chunks of at most that many tokens.
    fn split_into_best_sentences(&self, text: &str) -> anyhow::Result<Vec<String>> {
        let (prepared, _) = prepare_text_prompt(text);
        let tokens = self.encode(prepared.trim())?;
        let segments = self.segments(&tokens, &self.eos_tokens)?;
        let mut refined = Vec::new();
        for (count, seg) in segments {
            if count <= self.max_token_per_chunk {
                refined.push((count, seg));
                continue;
            }
            let sub = self.encode(seg.trim())?;
            let subs = self.segments(&sub, &self.fallback_tokens)?;
            if subs.len() > 1 {
                refined.extend(subs);
            } else {
                refined.push((count, seg));
            }
        }
        let mut chunks = Vec::new();
        let (mut current, mut current_count) = (String::new(), 0usize);
        for (count, seg) in refined {
            if current.is_empty() {
                current = seg;
                current_count = count;
            } else if current_count + count > self.max_token_per_chunk {
                chunks.push(current.trim().to_string());
                current = seg;
                current_count = count;
            } else {
                current.push(' ');
                current.push_str(&seg);
                current_count += count;
            }
        }
        if !current.is_empty() {
            chunks.push(current.trim().to_string());
        }
        Ok(chunks)
    }

    fn segments(&self, tokens: &[u32], boundary: &[u32]) -> anyhow::Result<Vec<(usize, String)>> {
        let mut idx = vec![0usize];
        let mut previous = false;
        for (i, t) in tokens.iter().enumerate() {
            if boundary.contains(t) {
                previous = true;
            } else {
                if previous {
                    idx.push(i);
                }
                previous = false;
            }
        }
        idx.push(tokens.len());
        idx.windows(2).map(|w| Ok((w[1] - w[0], self.decode(&tokens[w[0]..w[1]])?))).collect()
    }
}

/// pocket_tts_onnx.py's _prepare_text_prompt: one line, a capital first letter, a final period after a word; and how
/// many frames to keep after EOS (3 for four words or fewer, else 1; the caller adds 2).
fn prepare_text_prompt(text: &str) -> (String, usize) {
    let mut t = text.trim().replace(['\n', '\r'], " ").replace("  ", " ");
    if t.is_empty() {
        return (t, 1);
    }
    let guess = if t.split_whitespace().count() <= 4 { 3 } else { 1 };
    let mut chars = t.chars();
    if let Some(first) = chars.next().filter(|c| !c.is_uppercase()) {
        t = first.to_uppercase().chain(chars).collect();
    }
    if t.chars().last().is_some_and(char::is_alphanumeric) {
        t.push('.');
    }
    (t, guess)
}

/// A WAV (16-bit PCM or 32-bit float, any channels and rate) as mono samples at `rate` (linear resampling).
fn read_wav(path: &Path, rate: u32) -> anyhow::Result<Vec<f32>> {
    let b = std::fs::read(path).with_context(|| path.display().to_string())?;
    anyhow::ensure!(b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WAVE", "{}: not a WAV", path.display());
    let (mut fmt, mut data) = (None, None);
    let mut i = 12;
    while i + 8 <= b.len() {
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into()?) as usize;
        let body = &b[i + 8..(i + 8 + len).min(b.len())];
        match &b[i..i + 4] {
            b"fmt " => fmt = Some(body),
            b"data" => data = Some(body),
            _ => {}
        }
        i += 8 + len + (len & 1);
    }
    let (fmt, data) = (fmt.context("WAV without fmt")?, data.context("WAV without data")?);
    let format = u16::from_le_bytes([fmt[0], fmt[1]]);
    let channels = u16::from_le_bytes([fmt[2], fmt[3]]).max(1) as usize;
    let src_rate = u32::from_le_bytes(fmt[4..8].try_into()?);
    let bits = u16::from_le_bytes([fmt[14], fmt[15]]);
    let samples: Vec<f32> = match (format, bits) {
        (1, 16) | (0xFFFE, 16) => data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(),
        (3, 32) | (0xFFFE, 32) => data.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect(),
        _ => bail!("{}: WAV format {format} with {bits} bits (want 16-bit PCM or 32-bit float)", path.display()),
    };
    let mono: Vec<f32> = samples.chunks_exact(channels).map(|c| c.iter().sum::<f32>() / channels as f32).collect();
    if src_rate == rate || mono.is_empty() {
        return Ok(mono);
    }
    let n = (mono.len() as u64 * rate as u64 / src_rate as u64) as usize;
    let step = src_rate as f64 / rate as f64;
    Ok((0..n)
        .map(|k| {
            let x = k as f64 * step;
            let (j, f) = (x as usize, (x.fract()) as f32);
            let a = mono[j.min(mono.len() - 1)];
            let b = mono[(j + 1).min(mono.len() - 1)];
            a + (b - a) * f
        })
        .collect())
}

/// The clip without the silence at either end: 20 ms windows quieter than 40 dB under the loudest are dropped, with
/// 100 ms kept on each side.
fn trim_silence(audio: &[f32], rate: usize) -> Vec<f32> {
    let w = rate / 50;
    let rms: Vec<f32> = audio.chunks(w).map(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt()).collect();
    let peak = rms.iter().cloned().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return Vec::new();
    }
    let floor = peak * 0.01;
    let (Some(first), Some(last)) = (rms.iter().position(|&r| r > floor), rms.iter().rposition(|&r| r > floor)) else {
        return Vec::new();
    };
    let pad = rate / 10;
    let start = (first * w).saturating_sub(pad);
    let end = ((last + 1) * w + pad).min(audio.len());
    audio[start..end].to_vec()
}

/// A float32 .npy's values (bos_before_voice.npy).
fn read_npy_f32(path: &Path) -> anyhow::Result<Vec<f32>> {
    let b = std::fs::read(path).with_context(|| path.display().to_string())?;
    anyhow::ensure!(b.len() > 10 && &b[1..6] == b"NUMPY", "{}: not a .npy", path.display());
    let (header_len, start) = if b[6] == 1 {
        (u16::from_le_bytes([b[8], b[9]]) as usize, 10)
    } else {
        (u32::from_le_bytes(b[8..12].try_into()?) as usize, 12)
    };
    let header = String::from_utf8_lossy(&b[start..start + header_len]);
    anyhow::ensure!(header.contains("<f4") && header.contains("'fortran_order': False"), "{}: want little-endian float32, C order ({header})", path.display());
    Ok(b[start + header_len..].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
}
