//! Silero VAD v5 (MIT; github.com/snakers4/silero-vad) through its ONNX model (~/.local/share/raea-models/silero-vad/
//! silero_vad.onnx, the one the silero-vad 5.1.2 package carries): speech spans. As the package runs it: 16 kHz mono,
//! 512-sample windows each with the previous window's last 64 samples in front, the recurrent state carried; then its
//! get_speech_timestamps with the defaults (speech at 0.5, out under 0.35, 250 ms shortest speech, 100 ms of silence
//! to end one, 30 ms of padding, shared halfway when two spans are closer than 60 ms).

use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::value::Tensor;

const SR: usize = 16_000;
const WIN: usize = 512;
const CTX: usize = 64;

pub fn model_path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/raea-models/silero-vad/silero_vad.onnx")
}

pub struct Vad(Session);

impl Vad {
    pub fn load() -> Result<Vad, String> {
        let s = (|| -> ort::Result<Session> { crate::onnx::session("silero-vad", model_path()) })().map_err(|e| format!("silero vad: {e}"))?;
        Ok(Vad(s))
    }

    /// The speech probability of each 512-sample window (32 ms) of 16 kHz mono.
    pub fn probs(&mut self, x: &[f32]) -> Result<Vec<f32>, String> {
        let mut state = vec![0f32; 2 * 128];
        let mut ctx = vec![0f32; CTX];
        let mut out = Vec::with_capacity(x.len() / WIN + 1);
        for start in (0..x.len()).step_by(WIN) {
            let mut input = ctx.clone();
            input.extend((0..WIN).map(|i| x.get(start + i).copied().unwrap_or(0.0)));
            ctx.copy_from_slice(&input[input.len() - CTX..]);
            let ti = Tensor::from_array(([1usize, CTX + WIN], input)).map_err(|e| e.to_string())?;
            let ts = Tensor::from_array(([2usize, 1, 128], state.clone())).map_err(|e| e.to_string())?;
            let sr = Tensor::from_array(((), vec![SR as i64])).map_err(|e| e.to_string())?;
            let o = self.0.run(ort::inputs!["input" => ti, "state" => ts, "sr" => sr]).map_err(|e| format!("silero vad run: {e}"))?;
            let (_, p) = o["output"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let (_, s) = o["stateN"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            out.push(p[0]);
            state.copy_from_slice(&s[..256]);
        }
        Ok(out)
    }
}

/// get_speech_timestamps's defaults over the window probabilities: speech spans in samples (no maximum length).
pub fn spans(probs: &[f32], len: usize) -> Vec<(usize, usize)> {
    let (thr, neg) = (0.5f32, 0.35f32);
    let (min_speech, min_silence, pad) = (SR as f64 * 0.25, SR as f64 * 0.1, SR as f64 * 0.03);
    let mut speeches: Vec<(usize, usize)> = Vec::new();
    let (mut triggered, mut start, mut temp_end) = (false, 0usize, 0usize);
    for (i, &p) in probs.iter().enumerate() {
        let at = WIN * i;
        if p >= thr && temp_end != 0 {
            temp_end = 0;
        }
        if p >= thr && !triggered {
            triggered = true;
            start = at;
            continue;
        }
        if p < neg && triggered {
            if temp_end == 0 {
                temp_end = at;
            }
            if ((at - temp_end) as f64) < min_silence {
                continue;
            }
            if ((temp_end - start) as f64) > min_speech {
                speeches.push((start, temp_end));
            }
            temp_end = 0;
            triggered = false;
        }
    }
    if triggered && ((len - start) as f64) > min_speech {
        speeches.push((start, len));
    }
    // padding, shared halfway between spans closer than twice it
    let n = speeches.len();
    for i in 0..n {
        if i == 0 {
            speeches[i].0 = (speeches[i].0 as f64 - pad).max(0.0) as usize;
        }
        if i + 1 < n {
            let gap = speeches[i + 1].0 - speeches[i].1;
            if (gap as f64) < 2.0 * pad {
                speeches[i].1 += gap / 2;
                speeches[i + 1].0 = speeches[i + 1].0.saturating_sub(gap / 2);
            } else {
                speeches[i].1 = ((speeches[i].1 as f64 + pad) as usize).min(len);
                speeches[i + 1].0 = (speeches[i + 1].0 as f64 - pad).max(0.0) as usize;
            }
        } else {
            speeches[i].1 = ((speeches[i].1 as f64 + pad) as usize).min(len);
        }
    }
    speeches
}

/// A file's speech spans (seconds), with the time it took.
pub fn file(path: &Path, backend: &str) -> Result<serde_json::Value, String> {
    let t = std::time::Instant::now();
    // a `.f32` file is raw 16 kHz mono already (the parity bench hands over the package's own samples)
    let x = if path.extension().is_some_and(|e| e == "f32") {
        std::fs::read(path).map_err(|e| e.to_string())?.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
    } else {
        crate::sound::pcm16k(path, backend)
    };
    let probs = Vad::load()?.probs(&x)?;
    let s = spans(&probs, x.len());
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    Ok(serde_json::json!({"model": "silero-vad v5", "spans": s.iter().map(|(a, b)| [r3(*a as f64 / SR as f64), r3(*b as f64 / SR as f64)]).collect::<Vec<_>>(),
        "speech_s": r3(s.iter().map(|(a, b)| (b - a) as f64).sum::<f64>() / SR as f64), "seconds": r3(t.elapsed().as_secs_f64())}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_follow_the_package_rules() {
        // 0.5 s silence, 1 s speech, 64 ms dip (under the 100 ms that ends speech), 0.5 s speech, 1 s silence, 0.1 s speech
        let w = |s: f64| (s * SR as f64 / WIN as f64).round() as usize;
        let mut p = vec![0.0f32; w(0.5)];
        p.extend(vec![0.9; w(1.0)]);
        p.extend(vec![0.1; 2]);
        p.extend(vec![0.9; w(0.5)]);
        p.extend(vec![0.0; w(1.0)]);
        p.extend(vec![0.9; w(0.1)]);
        p.extend(vec![0.0; w(0.5)]);
        let s = spans(&p, p.len() * WIN);
        assert_eq!(s.len(), 1, "{s:?}"); // the dip doesn't split it; the 0.1 s blip is too short
        let (a, b) = (s[0].0 as f64 / SR as f64, s[0].1 as f64 / SR as f64);
        assert!((a - 0.47).abs() < 0.04 && (b - 2.09).abs() < 0.05, "{a} {b}");
    }
}
