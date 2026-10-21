//! HTDemucs (demucs v4, MIT; its training data includes MUSDB, research-licensed: noted) as four stems: drums, bass,
//! other, vocals. The network between its STFT and its inverse is an ONNX graph (~/.local/share/raea-models/htdemucs/
//! htdemucs_core.onnx, exported from demucs 4.0.1's htdemucs: 3e-4 from PyTorch on the spectral branch, 5e-6 on the
//! waveform), because ONNX can't hold complex tensors; the STFT, the inverse and the chunking are done here as demucs
//! does them: 44.1 kHz stereo, 7.8 s segments (343 980 samples, centre-padded), overlapping by a quarter and
//! cross-faded with a triangle (apply_model, shifts=0); per segment a 4096-point STFT every 1024 samples (periodic Hann,
//! normalised, reflect padding of 1536 on the left as htdemucs._spec does), its real and imaginary parts as channels.

use std::path::PathBuf;

use ort::session::Session;
use ort::value::Tensor;

pub const SR: u32 = 44_100;
pub const SOURCES: [&str; 4] = ["drums", "bass", "other", "vocals"];
const L: usize = 343_980;
const NFFT: usize = 4096;
const HOP: usize = 1024;
const FQ: usize = NFFT / 2;

pub fn model_path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/raea-models/htdemucs/htdemucs_core.onnx")
}

fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
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
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let ang = sign * 2.0 * std::f64::consts::PI / len as f64;
        let (wr0, wi0) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut wr, mut wi) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
                let t = wr * wr0 - wi * wi0;
                wi = wr * wi0 + wi * wr0;
                wr = t;
            }
        }
        len <<= 1;
    }
}

fn hann() -> Vec<f64> {
    (0..NFFT).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / NFFT as f64).cos()).collect()
}

/// Reflect padding (no edge repeat), as torch's.
fn reflect(x: &[f64], left: usize, right: usize) -> Vec<f64> {
    let n = x.len() as i64;
    (-(left as i64)..n + right as i64)
        .map(|i| {
            let mut k = i;
            while k < 0 || k >= n {
                k = if k < 0 { -k } else { 2 * (n - 1) - k };
            }
            x[k as usize]
        })
        .collect()
}

/// htdemucs._spec for one channel of a segment: (re, im) as [freq][frame], FQ x LE.
fn spec(x: &[f64]) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let le = L.div_ceil(HOP);
    let pad = HOP / 2 * 3;
    let xp = reflect(x, pad, pad + le * HOP - x.len());
    // torch.stft(center=True, reflect): another NFFT/2 each side
    let xs = reflect(&xp, NFFT / 2, NFFT / 2);
    let w = hann();
    let scale = 1.0 / (NFFT as f64).sqrt();
    let frames = 1 + (xs.len() - NFFT) / HOP;
    let (mut re, mut im) = (vec![vec![0.0; le]; FQ], vec![vec![0.0; le]; FQ]);
    for f in 2..2 + le {
        debug_assert!(f < frames);
        let mut r: Vec<f64> = (0..NFFT).map(|i| xs[f * HOP + i] * w[i]).collect();
        let mut q = vec![0.0; NFFT];
        fft(&mut r, &mut q, false);
        for k in 0..FQ {
            re[k][f - 2] = r[k] * scale;
            im[k][f - 2] = q[k] * scale;
        }
    }
    (re, im)
}

/// htdemucs._ispec for one channel: frames padded back (a zero Nyquist bin, two zero frames each side), the normalised
/// inverse STFT with overlap-add over the squared window, centre and padding cut, L samples out.
fn ispec(re: &[Vec<f64>], im: &[Vec<f64>]) -> Vec<f64> {
    let le = re[0].len();
    let frames = le + 4;
    let pad = HOP / 2 * 3;
    let total = HOP * L.div_ceil(HOP) + 2 * pad;
    let w = hann();
    let scale = (NFFT as f64).sqrt();
    let full = NFFT + HOP * (frames - 1);
    let (mut y, mut wsum) = (vec![0.0; full], vec![0.0; full]);
    for f in 0..frames {
        let (mut r, mut q) = (vec![0.0; NFFT], vec![0.0; NFFT]);
        if (2..2 + le).contains(&f) {
            for k in 0..FQ {
                r[k] = re[k][f - 2];
                q[k] = im[k][f - 2];
            }
            // the conjugate-symmetric half (the Nyquist bin is the zero pad)
            for k in 1..FQ {
                r[NFFT - k] = r[k];
                q[NFFT - k] = -q[k];
            }
        }
        fft(&mut r, &mut q, true);
        for i in 0..NFFT {
            y[f * HOP + i] += r[i] / NFFT as f64 * scale * w[i];
            wsum[f * HOP + i] += w[i] * w[i];
        }
    }
    let start = NFFT / 2;
    (0..total)
        .map(|i| {
            let j = start + i;
            if j < full && wsum[j] > 1e-11 { y[j] / wsum[j] } else { 0.0 }
        })
        .skip(pad)
        .take(L)
        .collect()
}

pub struct Demucs(Session);

impl Demucs {
    pub fn load() -> Result<Demucs, String> {
        let s = (|| -> ort::Result<Session> { crate::onnx::session("htdemucs", model_path()) })().map_err(|e| format!("htdemucs: {e}"))?;
        Ok(Demucs(s))
    }

    /// One segment of exactly L samples per channel: the four stems, [source][channel][sample].
    fn segment(&mut self, mix: &[Vec<f64>; 2]) -> Result<Vec<[Vec<f64>; 2]>, String> {
        let le = L.div_ceil(HOP);
        let specs = [spec(&mix[0]), spec(&mix[1])];
        // channels: c0 re, c0 im, c1 re, c1 im
        let mut mag = Vec::with_capacity(4 * FQ * le);
        for (re, im) in &specs {
            for part in [re, im] {
                for row in part {
                    mag.extend(row.iter().map(|v| *v as f32));
                }
            }
        }
        let mixf: Vec<f32> = mix.iter().flat_map(|c| c.iter().map(|v| *v as f32)).collect();
        let tm = Tensor::from_array(([1usize, 2, L], mixf)).map_err(|e| e.to_string())?;
        let tg = Tensor::from_array(([1usize, 4, FQ, le], mag)).map_err(|e| e.to_string())?;
        let o = self.0.run(ort::inputs!["mix" => tm, "mag" => tg]).map_err(|e| format!("htdemucs run: {e}"))?;
        let (_, sp) = o["spec"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let (_, wv) = o["wave"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for s in 0..SOURCES.len() {
            let mut chans: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
            for (c, ch) in chans.iter_mut().enumerate() {
                let at = |part: usize, k: usize, f: usize| sp[(((s * 4) + c * 2 + part) * FQ + k) * le + f] as f64;
                let re: Vec<Vec<f64>> = (0..FQ).map(|k| (0..le).map(|f| at(0, k, f)).collect()).collect();
                let im: Vec<Vec<f64>> = (0..FQ).map(|k| (0..le).map(|f| at(1, k, f)).collect()).collect();
                let x = ispec(&re, &im);
                *ch = x.iter().enumerate().map(|(i, v)| v + wv[((s * 2) + c) * L + i] as f64).collect();
            }
            out.push(chans);
        }
        Ok(out)
    }

    /// The stems of a whole stereo mix, as apply_model(split=True, overlap=0.25, shifts=0): [source][channel][sample].
    pub fn separate(&mut self, mix: &[Vec<f32>; 2]) -> Result<Vec<[Vec<f32>; 2]>, String> {
        let n = mix[0].len();
        let stride = (0.75 * L as f64) as usize;
        // the triangle, scaled to a peak of 1
        let half = L / 2;
        let mut weight: Vec<f64> = (1..=half).map(|i| i as f64).chain((1..=L - half).rev().map(|i| i as f64)).collect();
        let peak = weight.iter().cloned().fold(0.0, f64::max);
        weight.iter_mut().for_each(|w| *w /= peak);
        let mut out = vec![[vec![0.0f64; n], vec![0.0f64; n]]; SOURCES.len()];
        let mut sum = vec![0.0f64; n];
        let mut offset = 0;
        while offset < n {
            let len = L.min(n - offset);
            // centre-padded to L (TensorChunk.padded): zeros, taken from the mix around it where it has samples
            let delta = L - len;
            let start = offset as i64 - (delta / 2) as i64;
            let seg: [Vec<f64>; 2] = [0, 1].map(|c| (0..L as i64).map(|i| { let k = start + i; if k >= 0 && (k as usize) < n { mix[c][k as usize] as f64 } else { 0.0 } }).collect());
            let stems = self.segment(&seg)?;
            // the chunk's own part (centre-trimmed), weighted
            let trim = (delta / 2) as usize;
            for (s, st) in stems.iter().enumerate() {
                for c in 0..2 {
                    for i in 0..len {
                        out[s][c][offset + i] += weight[i] * st[c][trim + i];
                    }
                }
            }
            for i in 0..len {
                sum[offset + i] += weight[i];
            }
            offset += stride;
        }
        Ok(out.into_iter().map(|st| st.map(|c| c.iter().zip(&sum).map(|(v, w)| (v / w) as f32).collect())).collect())
    }
}
