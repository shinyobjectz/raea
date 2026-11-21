//! Fingerprint's columns (docs/fingerprint.feature FP1a, docs/fingerprint.md "The stack"): a file is read once by
//! frozen encoders that run on the person's own machine, and every fingerprint reads the same columns.
//!
//! - picture: SigLIP 2 base p16-224 per frame at 4 fps, its pooled vector (L2-normalised); the 14 x 14 patch tokens
//!   come with FP12's heatmaps
//! - change: from the picture column (window.rs)
//! - look: luminance histogram, contrast, saturation, local contrast and sharpness per frame (look.rs)
//! - sound: CLAP htsat-unfused per 2 s window, 1 s hop (L2-normalised); YAMNet per 0.48 s when its model is here
//! - level: the sound's RMS in dBFS per 0.1 s, for checks, the sheet and the timeline
//! - text: EmbeddingGemma 2 per passage of a script, brief or page (FP1b), a passage a step; a still is one frame
//!
//! Every encoder is ONNX on ONNX Runtime's CPU kernels (raea-sys-ml): never Core ML's GPU, which computes SigLIP wrong
//! on this Mac. The text towers (CLAP's, SigLIP's) only turn an axis's words into vectors (FP2e). Columns are cached by
//! the file's sha256 and STEP (cache.rs).

pub mod cache;
mod clap;
mod clap_text;
pub mod damage;
mod gemma;
pub mod pieces;
pub mod look;
pub mod media;
pub mod models;
mod siglip;
pub mod window;
mod yamnet;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Frames a second in the picture column
pub const FPS: f64 = 4.0;
/// CLAP's window and hop, seconds
pub const CLAP_WIN: f64 = 2.0;
pub const CLAP_HOP: f64 = 1.0;
/// YAMNet's hop, seconds
pub const YAM_HOP: f64 = 0.48;
pub const SIG: usize = 768;
pub const CLAP: usize = 512;
pub const YAM: usize = 1024;
/// AudioSet's classes, as YAMNet scores them
pub const YAM_CLASSES: usize = 521;
pub const TEXT: usize = gemma::DIM;
/// The column version: a change to how any column is made bumps it, and the cache makes them again
pub const STEP: u32 = 1;

/// A file's columns, row-major, each row one step of its column.
#[derive(Clone, Debug, Default)]
pub struct Columns {
    /// the file's sha256
    pub media: String,
    pub dur: f64,
    pub sig: Vec<f32>,
    pub look: Vec<f32>,
    pub clap: Vec<f32>,
    pub yam: Vec<f32>,
    pub level: Vec<f32>,
    /// one EmbeddingGemma 2 row per passage of a text piece (FP1b); empty for pictures and sound
    pub text: Vec<f32>,
}

impl Columns {
    pub fn clap_windows(&self) -> usize {
        self.clap.len() / CLAP
    }
}

/// The encoders, loaded once (the resident service keeps one): the text towers load on first use.
pub struct Encoders {
    files: models::Files,
    vision: siglip::Vision,
    clap: clap::Audio,
    yam: Option<yamnet::Yamnet>,
    vision_text: Option<siglip::Text>,
    clap_text: Option<clap_text::Text>,
    gemma: Option<gemma::Text>,
}

impl Encoders {
    pub fn load() -> Result<Encoders> {
        let files = models::locate()?;
        let vision = siglip::Vision(raea_sys_ml::session(&files.sig_vision).context("SigLIP 2's image tower")?);
        let clap = clap::Audio::load(&files.clap_audio)?;
        let yam = match files.yam.as_deref() {
            Some(f) => Some(yamnet::Yamnet(raea_sys_ml::session(f).context("YAMNet")?)),
            None => None,
        };
        Ok(Encoders { files, vision, clap, yam, vision_text: None, clap_text: None, gemma: None })
    }

    /// Each encoder's id and its file's sha256: a fingerprint's heads are pinned to these
    pub fn pins(&self) -> std::collections::BTreeMap<String, String> {
        let mut p: std::collections::BTreeMap<String, String> = self.files.pins().into_iter().collect();
        // EmbeddingGemma 2 is pinned once it's on this machine: a fingerprint made before it keeps working (FP1b)
        if let Some((m, _)) = models::gemma_here() {
            p.insert("embeddinggemma-2/model_quantized.onnx_data".into(), models::pin(&m.with_extension("onnx_data")));
        }
        p
    }

    /// A file's columns, from the cache when it has them
    pub fn columns(&mut self, file: &Path) -> Result<Columns> {
        let media = media::sha256(file)?;
        if let Some(c) = cache::load(&media) {
            return Ok(c);
        }
        // a script, brief or page is read as passages; a still is one frame (FP1b)
        if let Some(passages) = pieces::passages(file)? {
            let title = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let texts: Vec<String> = passages.iter().map(|p| gemma::as_document(p, &title)).collect();
            let text = self.gemma()?.embed(&texts)?;
            let c = Columns { media, dur: passages.len().max(1) as f64, text, ..Default::default() };
            cache::save(&c)?;
            return Ok(c);
        }
        if let Some(still) = pieces::still(file)? {
            let (sig, look) = self.picture(&[still])?;
            let c = Columns { media, dur: 1.0, sig, look, ..Default::default() };
            cache::save(&c)?;
            return Ok(c);
        }
        let dur = media::duration(file)?;
        let frames = media::frames(file, FPS, 0.0, dur)?;
        let (pcm16, pcm48) = (media::sound(file, 16_000)?, media::sound(file, clap::RATE)?);
        let mut c = self.of(&frames, &pcm16, &pcm48, dur)?;
        c.media = media;
        cache::save(&c)?;
        Ok(c)
    }

    /// Columns of frames (at FPS) and sound given directly: a damaged window (damage.rs) or a span already decoded
    pub fn of(&mut self, frames: &[media::Rgb], pcm16: &[f32], pcm48: &[f32], dur: f64) -> Result<Columns> {
        let (sig, look) = self.picture(frames)?;
        let (clap, yam) = self.sound(pcm16, pcm48)?;
        Ok(Columns { media: String::new(), dur, sig, look, clap, yam, level: media::levels(pcm16, 16_000), text: Vec::new() })
    }

    /// The picture columns of frames at FPS: SigLIP 2's pooled vectors and each frame's look
    pub fn picture(&mut self, frames: &[media::Rgb]) -> Result<(Vec<f32>, Vec<f32>)> {
        let sig = self.vision.embed(frames)?;
        Ok((sig, frames.iter().flat_map(|f| look::look(f)).collect()))
    }

    /// Each frame's 196 patch tokens, L2-normalised (siglip.rs tower): FP12's patch heads read them. An export that
    /// gives another count (a class token first, another grid) is refused, since a patch's place would be wrong
    pub fn patches(&mut self, frames: &[media::Rgb]) -> Result<Vec<f32>> {
        let v = self.vision.tower(frames, "last_hidden_state")?;
        anyhow::ensure!(v.len() == frames.len() * 196 * SIG, "SigLIP 2 gave {} patch values for {} frames, not 196 x {SIG} each", v.len(), frames.len());
        Ok(v)
    }

    /// The sound columns of mono sound at 16 and 48 kHz: CLAP per CLAP_WIN window every CLAP_HOP (one window at
    /// least), and YAMNet's when its model is here
    pub fn sound(&mut self, pcm16: &[f32], pcm48: &[f32]) -> Result<(Vec<f32>, Vec<f32>)> {
        let mut clips: Vec<&[f32]> = Vec::new();
        let (win, hop) = ((CLAP_WIN * clap::RATE as f64) as usize, (CLAP_HOP * clap::RATE as f64) as usize);
        let mut s = 0;
        while s == 0 || s + win <= pcm48.len() {
            clips.push(&pcm48[s.min(pcm48.len())..(s + win).min(pcm48.len())]);
            s += hop;
        }
        let clap = self.clap.embed(&clips)?;
        let yam = match &mut self.yam {
            Some(y) => y.embed(pcm16)?,
            None => Vec::new(),
        };
        Ok((clap, yam))
    }

    /// EmbeddingGemma 2's text model, loaded on first use (fetched once when the app's library hasn't it)
    fn gemma(&mut self) -> Result<&mut gemma::Text> {
        if self.gemma.is_none() {
            let (m, t) = models::gemma()?;
            self.gemma = Some(gemma::Text::load(&m, &t)?);
        }
        Ok(self.gemma.as_mut().unwrap())
    }

    /// Words as EmbeddingGemma 2 query vectors (text axes, FP1b), one row each
    pub fn words_text(&mut self, texts: &[String]) -> Result<Vec<f32>> {
        let q: Vec<String> = texts.iter().map(|t| gemma::as_query(t)).collect();
        self.gemma()?.embed(&q)
    }

    /// Words as CLAP text vectors (sound axes), L2-normalised, one row each
    /// YAMNet's AudioSet class scores every YAM_HOP s of 16 kHz mono, [patches][YAM_CLASSES]: what's heard by name
    /// (a videoform's speech and music, docs/videoform.feature VF3)
    pub fn sound_classes(&mut self, pcm16: &[f32]) -> Result<Vec<f32>> {
        self.yam.as_mut().ok_or_else(|| anyhow::anyhow!("YAMNet isn't installed"))?.classes(pcm16)
    }

    pub fn words_sound(&mut self, texts: &[String]) -> Result<Vec<f32>> {
        if self.clap_text.is_none() {
            self.clap_text = Some(clap_text::Text::load(&self.files.clap_text, &self.files.clap_tokenizer)?);
        }
        self.clap_text.as_mut().unwrap().embed(texts)
    }

    /// CLAP text vectors padded to `len` tokens: only for checking against a padded batch (tests)
    pub fn words_sound_padded(&mut self, texts: &[String], len: usize) -> Result<Vec<f32>> {
        self.words_sound(&[])?;
        self.clap_text.as_mut().unwrap().embed_padded(texts, len)
    }

    /// Words as SigLIP 2 text vectors (picture axes), L2-normalised, one row each
    pub fn words_picture(&mut self, texts: &[String]) -> Result<Vec<f32>> {
        if self.vision_text.is_none() {
            self.vision_text = Some(siglip::Text::load(&self.files.sig_text, &self.files.sig_tokenizer)?);
        }
        self.vision_text.as_mut().unwrap().embed(texts)
    }

    /// CLAP on clips of 48 kHz sound given directly (a test against the reference, or a damaged clip)
    pub fn clap_clips(&mut self, clips: &[&[f32]]) -> Result<Vec<f32>> {
        self.clap.embed(clips)
    }
}

/// `$var`, else `rel` under the home folder
/// A text model's session and its tokenizer
pub(crate) fn text_model(model: &Path, tokenizer: &Path, what: &str) -> Result<(ort::session::Session, tokenizers::Tokenizer)> {
    let tok = tokenizers::Tokenizer::from_file(tokenizer).map_err(|e| anyhow::anyhow!("{what}'s tokenizer: {e}"))?;
    Ok((raea_sys_ml::session(model).context(what.to_string())?, tok))
}

pub fn under(var: &str, rel: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(p) => p.into(),
        None => raea_io_home::user_home().unwrap_or_default().join(rel),
    }
}

/// Rows scaled to unit length in place
pub(crate) fn normalise(v: &mut [f32], dim: usize) {
    for row in v.chunks_mut(dim) {
        let n = row.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        row.iter_mut().for_each(|x| *x /= n);
    }
}

#[cfg(test)]
mod tests;
