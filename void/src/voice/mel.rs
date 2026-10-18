//! Log-mel features for the listening models: Whisper's, for Smart Turn.
//!
//! The same numbers as transformers' WhisperFeatureExtractor: a periodic Hann window of 400, hop 160, centred with
//! reflect padding, the power spectrum through 80 Slaney-normalised mel filters (0-8 kHz), log10 floored at 1e-10, the
//! last frame dropped, everything within 8 of the loudest value, then (x + 4) / 4.

use std::sync::OnceLock;

use rustfft::num_complex::Complex32;

pub const N_FFT: usize = 400;
pub const HOP: usize = 160;
pub const N_MELS: usize = 80;
const BINS: usize = N_FFT / 2 + 1;

fn hz_to_mel(f: f64) -> f64 {
    // Slaney: linear below 1 kHz, logarithmic above
    if f < 1000.0 { 3.0 * f / 200.0 } else { 15.0 + (f / 1000.0).ln() * 27.0 / 6.4f64.ln() }
}

fn mel_to_hz(m: f64) -> f64 {
    if m < 15.0 { 200.0 * m / 3.0 } else { 1000.0 * ((m - 15.0) * 6.4f64.ln() / 27.0).exp() }
}

/// [N_MELS][BINS], as transformers' mel_filter_bank(201, 80, 0, 8000, 16000, norm="slaney", mel_scale="slaney").
fn filters() -> &'static Vec<[f32; BINS]> {
    static F: OnceLock<Vec<[f32; BINS]>> = OnceLock::new();
    F.get_or_init(|| {
        let (lo, hi) = (hz_to_mel(0.0), hz_to_mel(8000.0));
        let pts: Vec<f64> = (0..N_MELS + 2).map(|i| mel_to_hz(lo + (hi - lo) * i as f64 / (N_MELS + 1) as f64)).collect();
        let fft: Vec<f64> = (0..BINS).map(|i| 8000.0 * i as f64 / (BINS - 1) as f64).collect();
        (0..N_MELS)
            .map(|m| {
                let mut row = [0f32; BINS];
                let enorm = 2.0 / (pts[m + 2] - pts[m]);
                for (k, &f) in fft.iter().enumerate() {
                    let down = (f - pts[m]) / (pts[m + 1] - pts[m]);
                    let up = (pts[m + 2] - f) / (pts[m + 2] - pts[m + 1]);
                    row[k] = (down.min(up).max(0.0) * enorm) as f32;
                }
                row
            })
            .collect()
    })
}

/// Whisper log-mel of `audio` (16 kHz): [N_MELS][frames] row-major, frames = len / HOP (the extractor drops the last).
pub fn whisper(audio: &[f32]) -> Vec<f32> {
    let pad = N_FFT / 2;
    let n = audio.len();
    // reflect padding (numpy's "reflect": the edge sample isn't repeated)
    let at = |i: isize| -> f32 {
        let mut i = i;
        if n == 1 {
            return audio[0];
        }
        let last = n as isize - 1;
        while i < 0 || i > last {
            if i < 0 {
                i = -i;
            }
            if i > last {
                i = 2 * last - i;
            }
        }
        audio[i as usize]
    };
    let frames = n / HOP; // 1 + n / HOP centred frames, the last dropped
    let window: Vec<f32> = (0..N_FFT).map(|i| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / N_FFT as f64).cos()) as f32).collect();
    let fft = rustfft::FftPlanner::<f32>::new().plan_fft_forward(N_FFT);
    let fb = filters();
    let mut out = vec![0f32; N_MELS * frames];
    let mut buf = vec![Complex32::new(0.0, 0.0); N_FFT];
    let mut power = [0f32; BINS];
    for t in 0..frames {
        let start = (t * HOP) as isize - pad as isize;
        for i in 0..N_FFT {
            buf[i] = Complex32::new(at(start + i as isize) * window[i], 0.0);
        }
        fft.process(&mut buf);
        for k in 0..BINS {
            power[k] = buf[k].norm_sqr();
        }
        for (m, row) in fb.iter().enumerate() {
            let e: f32 = row.iter().zip(&power).map(|(a, b)| a * b).sum();
            out[m * frames + t] = e.max(1e-10).log10();
        }
    }
    let top = out.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    for v in &mut out {
        *v = ((*v).max(top - 8.0) + 4.0) / 4.0;
    }
    out
}

/// transformers' zero_mean_unit_var_norm over the real samples, then zeros to `len` (padding on the right).
pub fn normalised_padded(audio: &[f32], len: usize) -> Vec<f32> {
    let n = audio.len().min(len);
    let a = &audio[..n];
    let mean = a.iter().map(|&x| x as f64).sum::<f64>() / n.max(1) as f64;
    let var = a.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / n.max(1) as f64;
    let sd = (var + 1e-7).sqrt();
    let mut out: Vec<f32> = a.iter().map(|&x| ((x as f64 - mean) / sd) as f32).collect();
    out.resize(len, 0.0);
    out
}
