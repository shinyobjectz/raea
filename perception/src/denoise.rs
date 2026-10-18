//! DeepFilterNet3 (MIT/Apache-2.0; github.com/Rikorose/DeepFilterNet) through the authors' own Rust library, libDF
//! v0.5.6, with its built-in DFN3 model run by tract: a clip's voice with the noise taken out. 48 kHz mono, one hop
//! (480 samples) at a time, the STFT and lookahead delay removed so the output lines up with the input, as the
//! library's own deep-filter tool does with --compensate-delay. Behind the `denoise` feature.

use std::path::Path;

use df::tract::{DfParams, DfTract, RuntimeParams};
use ndarray::{Array2, Axis};

pub const SR: u32 = 48_000;

/// 48 kHz mono samples, denoised and lined up with the input (same length).
pub fn denoise(x: &[f32]) -> Result<Vec<f32>, String> {
    let mut m = DfTract::new(DfParams::default(), &RuntimeParams::default_with_ch(1)).map_err(|e| format!("deepfilternet: {e}"))?;
    let hop = m.hop_size;
    let delay = m.fft_size - m.hop_size + m.lookahead * m.hop_size;
    // the input followed by the delay's worth of silence, so the tail comes out too
    let n = x.len() + delay;
    let padded: Vec<f32> = x.iter().copied().chain(std::iter::repeat_n(0.0, delay + hop)).collect();
    let noisy = Array2::from_shape_vec((1, padded.len()), padded).map_err(|e| e.to_string())?;
    let mut enh = Array2::<f32>::zeros(noisy.raw_dim());
    for (a, b) in noisy.axis_chunks_iter(Axis(1), hop).zip(enh.axis_chunks_iter_mut(Axis(1), hop)) {
        if a.len_of(Axis(1)) < hop {
            break;
        }
        m.process(a, b).map_err(|e| format!("deepfilternet: {e}"))?;
    }
    Ok(enh.row(0).iter().skip(delay).take(n - delay).copied().collect())
}

/// A file's sound as 48 kHz mono (the channels' mean, raea-media's resampler).
pub fn pcm48k(path: &Path, backend: &str) -> Result<Vec<f32>, String> {
    let a = raea_media::backend(backend)?.decode_audio(path)?;
    if a.channels.is_empty() {
        return Ok(Vec::new());
    }
    let n = a.len();
    let mono: Vec<f32> = (0..n).map(|i| a.channels.iter().map(|c| c[i]).sum::<f32>() / a.channels.len() as f32).collect();
    Ok(raea_media::soft_resample(&raea_media::Audio { rate: a.rate, channels: vec![mono] }, SR).channels.into_iter().next().unwrap_or_default())
}
