//! Beat This! (MIT; github.com/CPJKU/beat_this, Foscarin et al. 2024), the small checkpoint ("small0", 2.1M
//! parameters) through its ONNX export (~/.local/share/raea-models/beat-this/beat_this_small0.onnx, 1e-5 from PyTorch):
//! beats and downbeats. The front end is the package's LogMelSpect done here: 22 050 Hz mono, a 1024-point STFT every
//! 441 samples (50 frames a second; centred, reflect-padded, periodic Hann, magnitude over sqrt(1024)), 128 Slaney mel
//! bands from 30 Hz to 11 kHz (unnormalised triangles), log1p(1000 x). The model runs on 1500-frame chunks that
//! overlap by 6 frames at each border, earlier chunks winning the overlap; the minimal postprocessing (the package's
//! default; its DBN option needs madmom, whose models are non-commercial): a peak is a logit over 0 that is the
//! maximum within 3 frames either side, adjacent peaks are averaged, and each downbeat moves to its nearest beat.

use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::value::Tensor;

pub const FPS: f64 = 50.0;
const SR: u32 = 22_050;
const NFFT: usize = 1024;
const HOP: usize = 441;
const MELS: usize = 128;
const CHUNK: usize = 1500;
const BORDER: usize = 6;

pub fn model_path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/raea-models/beat-this/beat_this_small0.onnx")
}

pub(crate) fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((ang * k as f64).cos(), (ang * k as f64).sin());
                let (a, b) = (start + k, start + k + len / 2);
                let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
        }
        len <<= 1;
    }
}

pub(crate) fn hz_to_mel(f: f64) -> f64 {
    let (f_sp, min_log_hz) = (200.0 / 3.0, 1000.0);
    if f >= min_log_hz {
        min_log_hz / f_sp + (f / min_log_hz).ln() / (6.4f64.ln() / 27.0)
    } else {
        f / f_sp
    }
}

pub(crate) fn mel_to_hz(m: f64) -> f64 {
    let (f_sp, min_log_hz) = (200.0 / 3.0, 1000.0);
    let min_log_mel = min_log_hz / f_sp;
    if m >= min_log_mel {
        min_log_hz * ((6.4f64.ln() / 27.0) * (m - min_log_mel)).exp()
    } else {
        f_sp * m
    }
}

/// torchaudio's melscale_fbanks(513, 30, 11000, 128, 22050, norm=None, mel_scale="slaney"), as [freq][mel].
fn filterbank() -> Vec<[f64; MELS]> {
    let nf = NFFT / 2 + 1;
    let freqs: Vec<f64> = (0..nf).map(|i| (SR / 2) as f64 * i as f64 / (nf - 1) as f64).collect();
    let (m0, m1) = (hz_to_mel(30.0), hz_to_mel(11_000.0));
    let pts: Vec<f64> = (0..MELS + 2).map(|i| mel_to_hz(m0 + (m1 - m0) * i as f64 / (MELS + 1) as f64)).collect();
    freqs
        .iter()
        .map(|&f| {
            let mut row = [0.0; MELS];
            for (m, r) in row.iter_mut().enumerate() {
                let down = (f - pts[m]) / (pts[m + 1] - pts[m]);
                let up = (pts[m + 2] - f) / (pts[m + 2] - pts[m + 1]);
                *r = down.min(up).max(0.0);
            }
            row
        })
        .collect()
}

/// The log-mel spectrogram (frames x 128) of 22 050 Hz mono samples.
pub fn spectrogram(x: &[f32]) -> Vec<[f32; MELS]> {
    let pad = NFFT / 2;
    let n = x.len();
    if n <= pad {
        return Vec::new();
    }
    // reflect padding (no edge repeat), as torch.stft(center=True, pad_mode="reflect")
    let at = |i: i64| -> f64 {
        let k = if i < 0 { -i } else if i as usize >= n { 2 * (n as i64 - 1) - i } else { i };
        x[k as usize] as f64
    };
    let hann: Vec<f64> = (0..NFFT).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / NFFT as f64).cos()).collect();
    let fb = filterbank();
    let frames = 1 + n / HOP;
    let scale = 1.0 / (NFFT as f64).sqrt();
    (0..frames)
        .map(|f| {
            let s = (f * HOP) as i64 - pad as i64;
            let mut re: Vec<f64> = (0..NFFT).map(|i| at(s + i as i64) * hann[i]).collect();
            let mut im = vec![0.0; NFFT];
            fft(&mut re, &mut im);
            let mut mel = [0f32; MELS];
            for k in 0..=NFFT / 2 {
                let mag = (re[k] * re[k] + im[k] * im[k]).sqrt() * scale;
                if mag == 0.0 {
                    continue;
                }
                for (m, w) in fb[k].iter().enumerate() {
                    if *w > 0.0 {
                        mel[m] += (mag * w) as f32;
                    }
                }
            }
            mel.map(|v| (1000.0 * v).ln_1p())
        })
        .collect()
}

/// A file's sound as 22 050 Hz mono (the channels' mean, raea-media's resampler); a `.f32` file is taken as raw
/// 22 050 Hz mono floats already (the parity bench hands over the package's own samples).
pub fn pcm(path: &Path, backend: &str) -> Result<Vec<f32>, String> {
    if path.extension().is_some_and(|e| e == "f32") {
        let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect());
    }
    let a = raea_media::backend(backend)?.decode_audio(path)?;
    if a.channels.is_empty() {
        return Ok(Vec::new());
    }
    let n = a.len();
    let mono: Vec<f32> = (0..n).map(|i| a.channels.iter().map(|c| c[i]).sum::<f32>() / a.channels.len() as f32).collect();
    Ok(raea_media::soft_resample(&raea_media::Audio { rate: a.rate, channels: vec![mono] }, SR).channels.into_iter().next().unwrap_or_default())
}

pub struct BeatThis(Session);

impl BeatThis {
    pub fn load() -> Result<BeatThis, String> {
        let s = (|| -> ort::Result<Session> { crate::onnx::session("beatthis", model_path()) })().map_err(|e| format!("beat this: {e}"))?;
        Ok(BeatThis(s))
    }

    /// Per-frame beat and downbeat logits for a spectrogram, chunked as the package does.
    pub fn logits(&mut self, spect: &[[f32; MELS]]) -> Result<(Vec<f32>, Vec<f32>), String> {
        let n = spect.len();
        let (mut beat, mut down) = (vec![-1000f32; n], vec![-1000f32; n]);
        if n == 0 {
            return Ok((beat, down));
        }
        let step = (CHUNK - 2 * BORDER) as i64;
        let mut starts: Vec<i64> = (0..).map(|i| -(BORDER as i64) + i * step).take_while(|s| *s < n as i64 - BORDER as i64).collect();
        if n > CHUNK - 2 * BORDER {
            *starts.last_mut().unwrap() = n as i64 - (CHUNK - BORDER) as i64;
        }
        // later chunks first, so earlier ones overwrite the overlap (keep_first)
        for &start in starts.iter().rev() {
            let (lo, hi) = (start.max(0) as usize, ((start + CHUNK as i64) as usize).min(n));
            let left = (-start).max(0) as usize;
            let right = (start + CHUNK as i64 - n as i64).clamp(0, BORDER as i64) as usize;
            let len = left + (hi - lo) + right;
            let mut data = vec![0f32; len * MELS];
            for (i, row) in spect[lo..hi].iter().enumerate() {
                data[(left + i) * MELS..(left + i + 1) * MELS].copy_from_slice(row);
            }
            let t = Tensor::from_array(([1usize, len, MELS], data)).map_err(|e| e.to_string())?;
            let out = self.0.run(ort::inputs!["spect" => t]).map_err(|e| format!("beat this run: {e}"))?;
            let (_, b) = out["beat"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let (_, d) = out["downbeat"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            // the chunk's prediction without its borders lands at start + BORDER
            for k in BORDER..len.saturating_sub(BORDER) {
                let at = start + k as i64;
                if at >= 0 && (at as usize) < n {
                    beat[at as usize] = b[k];
                    down[at as usize] = d[k];
                }
            }
        }
        Ok((beat, down))
    }
}

/// Peak frames: logits over 0 that are the maximum within 3 frames either side, adjacent ones (1 frame apart) averaged.
pub fn peaks(logits: &[f32]) -> Vec<f64> {
    let n = logits.len();
    let raw: Vec<usize> = (0..n)
        .filter(|&i| {
            let (a, b) = (i.saturating_sub(3), (i + 3).min(n - 1));
            logits[i] > 0.0 && logits[a..=b].iter().all(|x| *x <= logits[i])
        })
        .collect();
    let mut out = Vec::new();
    let mut it = raw.into_iter();
    let Some(first) = it.next() else { return out };
    let (mut p, mut c, mut last) = (first as f64, 1.0, first);
    for q in it {
        if q - last <= 1 {
            c += 1.0;
            p += (q as f64 - p) / c;
        } else {
            out.push(p);
            p = q as f64;
            c = 1.0;
        }
        last = q;
    }
    out.push(p);
    out
}

/// Beats and downbeats (seconds) from the logits; each downbeat moved to its nearest beat.
pub fn beats(beat: &[f32], down: &[f32]) -> (Vec<f64>, Vec<f64>) {
    let b: Vec<f64> = peaks(beat).into_iter().map(|f| f / FPS).collect();
    let mut d: Vec<f64> = peaks(down)
        .into_iter()
        .map(|f| f / FPS)
        .map(|t| b.iter().copied().min_by(|x, y| (x - t).abs().total_cmp(&(y - t).abs())).unwrap_or(t))
        .collect();
    d.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    (b, d)
}

/// A file's beats and downbeats, with the time it took.
pub fn file(path: &Path, backend: &str) -> Result<serde_json::Value, String> {
    let t = std::time::Instant::now();
    let x = pcm(path, backend)?;
    let spect = spectrogram(&x);
    let (b, d) = BeatThis::load()?.logits(&spect)?;
    let (beats, downbeats) = beats(&b, &d);
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    // the median gap between beats (on a weak pulse the model places few beats, so a tempo from it would mislead)
    let gap = if beats.len() > 2 {
        let mut gaps: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
        gaps.sort_by(f64::total_cmp);
        Some(r3(gaps[gaps.len() / 2]))
    } else {
        None
    };
    Ok(serde_json::json!({"model": "beat_this small0", "fps": FPS, "frames": spect.len(), "median_gap_s": gap,
        "beats": beats.into_iter().map(r3).collect::<Vec<_>>(), "downbeats": downbeats.into_iter().map(r3).collect::<Vec<_>>(),
        "seconds": r3(t.elapsed().as_secs_f64())}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_are_local_maxima_over_zero_with_neighbours_averaged() {
        let mut l = vec![-5f32; 40];
        l[10] = 2.0; // a peak
        l[12] = 1.0; // within 3 frames of a bigger one: not a peak
        l[20] = 1.5;
        l[21] = 1.5; // two equal adjacent maxima: one peak at 20.5
        l[30] = -0.5; // a maximum under 0: not a beat
        assert_eq!(peaks(&l), vec![10.0, 20.5]);
    }

    #[test]
    fn downbeats_move_to_the_nearest_beat() {
        let mut b = vec![-5f32; 200];
        let mut d = vec![-5f32; 200];
        for f in [25, 50, 75, 100, 125] {
            b[f] = 3.0;
        }
        d[52] = 3.0;
        let (beats, downs) = beats(&b, &d);
        assert_eq!(beats, vec![0.5, 1.0, 1.5, 2.0, 2.5]);
        assert_eq!(downs, vec![1.0]);
    }

    #[test]
    fn the_mel_bank_matches_torchaudio_at_its_edges() {
        let fb = filterbank();
        // no weight under 30 Hz or over 11 kHz, every band has some weight
        assert!(fb[0].iter().all(|w| *w == 0.0));
        assert!((0..MELS).all(|m| fb.iter().any(|r| r[m] > 0.0)));
        assert_eq!(spectrogram(&vec![0.0; 22_050]).len(), 1 + 22_050 / HOP);
    }
}
