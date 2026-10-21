//! Audio as columns (G7 part 3): every source decodes once into f32 columns at 48 kHz; the mix, gains, fades and
//! envelopes are whole-column kernels; loudness, peaks and onsets are scans. Deterministic and sample-exact: the same
//! rows give the same samples on every platform.

use crate::Audio;

pub const RATE: u32 = 48000;

/// One audio row of the edit: a source's columns placed on the edit's timeline.
#[derive(Debug, Clone)]
pub struct Lane {
    pub audio: Audio,
    /// where it starts in the edit (s), where in the source (s), how long (s)
    pub at: f64,
    pub from: f64,
    pub len: f64,
    /// gain in dB, as (edit time s, dB) keys (linear between keys; one key = constant)
    pub gain_db: Vec<(f64, f64)>,
    /// fade in / out (s)
    pub fade_in: f64,
    pub fade_out: f64,
}

fn db(x: f64) -> f64 {
    10f64.powf(x / 20.0)
}

fn key_at(keys: &[(f64, f64)], t: f64) -> f64 {
    if keys.is_empty() {
        return 0.0;
    }
    let i = keys.partition_point(|k| k.0 <= t);
    if i == 0 {
        keys[0].1
    } else if i == keys.len() {
        keys[i - 1].1
    } else {
        let ((t0, v0), (t1, v1)) = (keys[i - 1], keys[i]);
        v0 + (v1 - v0) * (t - t0) / (t1 - t0).max(1e-9)
    }
}

/// Mix lanes into `channels` columns of `seconds` at 48 kHz: sum of (source column x gain envelope x fades).
pub fn mix(lanes: &[Lane], seconds: f64, channels: usize) -> Audio {
    let n = (seconds * RATE as f64).round() as usize;
    let mut out = vec![vec![0f32; n]; channels];
    for l in lanes {
        let a = crate::soft_resample(&l.audio, RATE);
        let start = (l.at * RATE as f64).round() as usize;
        let src0 = (l.from * RATE as f64).round() as usize;
        let len = ((l.len * RATE as f64).round() as usize).min(a.len().saturating_sub(src0));
        let (fi, fo) = (
            (l.fade_in * RATE as f64) as usize,
            (l.fade_out * RATE as f64) as usize,
        );
        for k in 0..len {
            let o = start + k;
            if o >= n {
                break;
            }
            let t = o as f64 / RATE as f64;
            let mut g = db(key_at(&l.gain_db, t)) as f32;
            if fi > 0 && k < fi {
                g *= k as f32 / fi as f32;
            }
            if fo > 0 && k + fo > len {
                g *= (len - k) as f32 / fo as f32;
            }
            for (c, col) in out.iter_mut().enumerate() {
                let s = a.channels[c.min(a.channels.len() - 1)][src0 + k];
                col[o] += s * g;
            }
        }
    }
    Audio {
        rate: RATE,
        channels: out,
    }
}

/// Per-block RMS level in dBFS (block = `block` samples): the column a ducking rule reads.
pub fn block_db(a: &Audio, block: usize) -> Vec<f64> {
    let n = a.len().div_ceil(block);
    (0..n)
        .map(|b| {
            let (lo, hi) = (b * block, ((b + 1) * block).min(a.len()));
            let mut s = 0.0f64;
            for c in &a.channels {
                for x in &c[lo..hi] {
                    s += (*x as f64) * (*x as f64);
                }
            }
            let ms = s / ((hi - lo).max(1) * a.channels.len()) as f64;
            10.0 * ms.max(1e-12).log10()
        })
        .collect()
}

/// Sample peak in dBFS.
pub fn peak_db(a: &Audio) -> f64 {
    let p = a
        .channels
        .iter()
        .flat_map(|c| c.iter())
        .fold(0f32, |m, x| m.max(x.abs()));
    20.0 * (p as f64).max(1e-12).log10()
}

/// The K-weighted channels (BS.1770's shelf, then its high-pass), at 48 kHz.
fn k_weighted(a: &Audio) -> Vec<Vec<f64>> {
    let a = crate::soft_resample(a, RATE);
    let fs = RATE as f64;
    // the two K-weighting biquads (BS.1770 coefficients, derived for fs)
    let shelf = {
        let (f0, g, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
        let k = (std::f64::consts::PI * f0 / fs).tan();
        let vh = 10f64.powf(g / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
            2.0 * (k * k - 1.0) / a0,
            (1.0 - k / q + k * k) / a0,
        ]
    };
    let hp = {
        let (f0, q) = (38.13547087602444, 0.5003270373238773);
        let k = (std::f64::consts::PI * f0 / fs).tan();
        let a0 = 1.0 + k / q + k * k;
        [
            1.0,
            -2.0,
            1.0,
            2.0 * (k * k - 1.0) / a0,
            (1.0 - k / q + k * k) / a0,
        ]
    };
    let biquad = |x: &[f64], c: [f64; 5]| -> Vec<f64> {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        x.iter()
            .map(|&v| {
                let y = c[0] * v + c[1] * x1 + c[2] * x2 - c[3] * y1 - c[4] * y2;
                x2 = x1;
                x1 = v;
                y2 = y1;
                y1 = y;
                y
            })
            .collect()
    };
    a.channels
        .iter()
        .map(|c| {
            biquad(
                &biquad(&c.iter().map(|x| *x as f64).collect::<Vec<_>>(), shelf),
                hp,
            )
        })
        .collect()
}

/// Momentary loudness (LUFS, 400 ms trailing window) at each of `n` times `i / fps`: the column a brief reads.
pub fn momentary(a: &Audio, fps: f64, n: usize) -> Vec<f64> {
    let w = k_weighted(a);
    let len = w.first().map(Vec::len).unwrap_or(0);
    let mut prefix = vec![0f64; len + 1];
    for c in &w {
        for i in 0..len {
            prefix[i + 1] += c[i] * c[i];
        }
    }
    for i in 0..len {
        prefix[i + 1] += prefix[i];
    }
    let block = (0.4 * RATE as f64) as usize;
    (0..n)
        .map(|f| {
            let end = (((f as f64 + 1.0) / fps) * RATE as f64).round() as usize;
            let end = end.min(len);
            let start = end.saturating_sub(block);
            let e = (prefix[end] - prefix[start]) / block as f64;
            (-0.691 + 10.0 * e.max(1e-15).log10()).max(-120.0)
        })
        .collect()
}

/// Integrated loudness (EBU R128 / ITU-R BS.1770-4): K-weighting, 400 ms blocks with 75% overlap, absolute gate at
/// -70 LUFS, relative gate at -10 LU. Written as column scans.
pub fn lufs(a: &Audio) -> f64 {
    let fs = RATE as f64;
    let weighted = k_weighted(a);
    let len = weighted.first().map(Vec::len).unwrap_or(0);
    let (block, step) = ((0.4 * fs) as usize, (0.1 * fs) as usize);
    let mut z = Vec::new();
    let mut s = 0;
    while s + block <= len {
        let mut e = 0.0;
        for c in &weighted {
            e += c[s..s + block].iter().map(|v| v * v).sum::<f64>() / block as f64;
        }
        z.push(e);
        s += step;
    }
    let l = |e: f64| -0.691 + 10.0 * e.max(1e-15).log10();
    let abs: Vec<f64> = z.iter().copied().filter(|&e| l(e) > -70.0).collect();
    if abs.is_empty() {
        return -70.0;
    }
    let rel = l(abs.iter().sum::<f64>() / abs.len() as f64) - 10.0;
    let gated: Vec<f64> = abs.into_iter().filter(|&e| l(e) > rel).collect();
    l(gated.iter().sum::<f64>() / gated.len().max(1) as f64)
}

/// The residual between two mixes in dB relative to the reference's RMS (how close one mix is to another).
pub fn residual_db(a: &Audio, reference: &Audio) -> f64 {
    let n = a.len().min(reference.len());
    let (mut e, mut r) = (0.0f64, 0.0f64);
    for c in 0..a.channels.len().min(reference.channels.len()) {
        for i in 0..n {
            let d = a.channels[c][i] as f64 - reference.channels[c][i] as f64;
            e += d * d;
            r += (reference.channels[c][i] as f64).powi(2);
        }
    }
    10.0 * (e.max(1e-20) / r.max(1e-20)).log10()
}

/// Loudness to `target` LUFS, then a look-ahead peak limiter to `ceiling` dBFS (5 ms look-ahead: the gain reaches
/// its floor before the peak arrives; 80 ms release). Column scans, deterministic.
/// The ceiling is a true-peak ceiling (dBTP, BS.1770): the limiter works from the 4x-oversampled peaks, not the
/// samples, since a master whose samples stopped at -1 dBFS still reached -0.9 dBTP between them.
pub fn master(a: &Audio, target: f64, ceiling: f64) -> Audio {
    master_gained(a, target, ceiling).0
}

/// `master`, with the gain it puts on the sound before the limiter (dB): what live play gives a game's voices ahead
/// of its own limiter at the same ceiling, so the limiter's reduction counts once, as in the render (the loudness
/// change after limiting, given instead, counted it twice: a game played 1 LU under its render).
pub fn master_gained(a: &Audio, target: f64, ceiling: f64) -> (Audio, f64) {
    let tp = true_envelope(a);
    // limiting takes loudness off: measure again and make it up (converges in two or three passes)
    let mut gain = target - lufs(a);
    let mut out = limit(a, gain, ceiling, &tp);
    for _ in 0..3 {
        let miss = target - lufs(&out);
        if miss.abs() < 0.1 {
            break;
        }
        gain += miss;
        out = limit(a, gain, ceiling, &tp);
    }
    // the gain ramps reshape the waveform a little: if a peak between samples still gets over, aim that much lower
    for _ in 0..2 {
        let over = true_peak_db(&out) - ceiling;
        if over <= 0.0 {
            break;
        }
        out = limit(a, gain, ceiling - over - 0.02, &tp);
    }
    (out, gain)
}

/// `master`, held to its ceiling after the encoder: the mastered sound is put through `encode` (an AAC round trip) and,
/// if its true peak comes back over the ceiling, mastered again with the ceiling lowered by the overshoot (twice at
/// most). AAC adds a few tenths of a dB to a limited true peak (an agent session's -1.5 ceiling gave -1.1 to -1.4 in the file).
pub fn master_for(a: &Audio, target: f64, ceiling: f64, encode: &dyn Fn(&Audio) -> crate::R<Audio>) -> Audio {
    let mut c = ceiling;
    let mut out = master(a, target, c);
    for _ in 0..2 {
        let Ok(rt) = encode(&out) else { break };
        let over = true_peak_db(&rt) - ceiling;
        if over <= 0.0 {
            break;
        }
        c -= over + 0.05;
        out = master(a, target, c);
    }
    out
}

/// The true peak (dBTP): the peak of the signal oversampled 4x.
pub fn true_peak_db(a: &Audio) -> f64 {
    peak_db(&crate::soft_resample(a, a.rate * 4))
}

/// Each sample's true peak across channels: the largest 4x-oversampled magnitude from half a sample before it to half
/// a sample after.
fn true_envelope(a: &Audio) -> Vec<f32> {
    let n = a.len();
    let mut env = vec![0f32; n];
    if n == 0 {
        return env;
    }
    let up = crate::soft_resample(a, a.rate * 4);
    for ch in &up.channels {
        for (j, y) in ch.iter().enumerate() {
            let v = y.abs();
            for i in [(j + 2) / 4, (j + 1) / 4] {
                if i < n && v > env[i] {
                    env[i] = v;
                }
            }
        }
    }
    for ch in &a.channels {
        for (i, x) in ch.iter().enumerate() {
            env[i] = env[i].max(x.abs());
        }
    }
    env
}

fn limit(a: &Audio, gain_db: f64, ceiling: f64, tp: &[f32]) -> Audio {
    let g = db(gain_db) as f32;
    let c = db(ceiling) as f32;
    let n = a.len();
    // the gain each sample needs (its true peak, across channels)
    let mut need = vec![1f32; n];
    for (i, p) in tp.iter().enumerate().take(n) {
        let v = p * g;
        if v > c {
            need[i] = c / v;
        }
    }
    // look-ahead: a sample's gain is the least needed over the next `la` samples, ramped linearly toward it
    let la = (0.005 * RATE as f64) as usize;
    let mut ahead = vec![1f32; n];
    let mut dq: std::collections::VecDeque<usize> = Default::default();
    for i in (0..n).rev() {
        while dq.back().is_some_and(|&j| need[j] >= need[i]) {
            dq.pop_back();
        }
        dq.push_back(i);
        while dq.front().is_some_and(|&j| j > i + la) {
            dq.pop_front();
        }
        ahead[i] = need[*dq.front().unwrap()];
    }
    let release = (-1.0 / (0.08 * RATE as f64)).exp() as f32;
    let attack = 1.0 / la.max(1) as f32;
    let mut env = 1f32;
    let mut gains = vec![1f32; n];
    for i in 0..n {
        let t = ahead[i];
        env = if t < env {
            (env - attack).max(t)
        } else {
            t + (env - t) * release
        };
        gains[i] = env.min(need[i]);
    }
    Audio {
        rate: a.rate,
        channels: a
            .channels
            .iter()
            .map(|ch| ch.iter().zip(&gains).map(|(x, k)| x * g * k).collect())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone at a quarter of the rate, 45 degrees off the samples: every sample sits at 0.707 of the wave's peak,
    /// so a sample-peak limiter lets the true peak through 3 dB over its ceiling. The target is louder than the
    /// ceiling allows, so the limiter has to work.
    #[test]
    fn the_master_ceiling_holds_between_samples() {
        let n = 3 * RATE as usize;
        let ch: Vec<f32> = (0..n).map(|i| 0.5 * (std::f32::consts::FRAC_PI_2 * i as f32 + std::f32::consts::FRAC_PI_4).sin()).collect();
        let a = Audio { rate: RATE, channels: vec![ch.clone(), ch] };
        let out = master(&a, 6.0, -1.0);
        let (tp, sp) = (true_peak_db(&out), peak_db(&out));
        assert!(tp <= -1.0, "true peak {tp:.2} dBTP over the -1 ceiling (sample peak {sp:.2})");
        assert!(tp > -2.0, "limited far below the ceiling: {tp:.2} dBTP");
    }
}
