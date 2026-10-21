//! Beats in a sound (docs/the roadmap G's catalogue: cuts on beats), model-free: Ellis's dynamic-programming beat tracker
//! (2007). Onset strength is the spectral flux of a log-magnitude spectrogram 100 times a second; the tempo is the
//! envelope's autocorrelation peak between 60 and 180 BPM, weighted toward 120; the beats are the path through the
//! envelope that best keeps that period. A learned tracker (Beat This!) reads harder music better; this one is exact on
//! steady pulse and needs no weights.

use raea_media::Audio;

/// Frames of the onset envelope a second.
pub const RATE: f64 = 100.0;

fn fft(re: &mut [f64], im: &mut [f64]) {
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

/// The onset envelope: positive spectral flux of log(1 + 1000 |X|) over 1024-sample Hann windows (on the channels'
/// mean), RATE a second, high-passed (its 1 s moving mean removed, floored at 0) and scaled to unit standard deviation.
pub fn onsets(a: &Audio) -> Vec<f64> {
    let n = a.len();
    if n == 0 || a.rate == 0 {
        return Vec::new();
    }
    let mono: Vec<f64> = (0..n).map(|i| a.channels.iter().map(|c| c[i] as f64).sum::<f64>() / a.channels.len() as f64).collect();
    // a 46 ms window whatever the rate (a power of two)
    let win = ((0.046 * a.rate as f64).log2().round().exp2() as usize).max(256);
    let hop = a.rate as f64 / RATE;
    let hann: Vec<f64> = (0..win).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / win as f64).cos()).collect();
    let frames = (n as f64 / hop) as usize;
    let mut prev = vec![0.0; win / 2];
    let mut env = Vec::with_capacity(frames);
    for f in 0..frames {
        // the window sits so a frame's flux peaks when an onset reaches the frame's time: Hann-weighted flux rises
        // before the onset reaches the centre (by 0.43 of the window, measured on clicks), so the window lags by that
        let c = (f as f64 * hop) as i64 - win as i64 / 2 - (0.43 * win as f64) as i64;
        let mut re: Vec<f64> = (0..win).map(|i| { let k = c + i as i64; if k >= 0 && (k as usize) < n { mono[k as usize] * hann[i] } else { 0.0 } }).collect();
        let mut im = vec![0.0; win];
        fft(&mut re, &mut im);
        let mag: Vec<f64> = (0..win / 2).map(|k| (1.0 + 1000.0 * (re[k] * re[k] + im[k] * im[k]).sqrt()).ln()).collect();
        env.push(if f == 0 { 0.0 } else { mag.iter().zip(&prev).map(|(m, p)| (m - p).max(0.0)).sum::<f64>() });
        prev = mag;
    }
    let w = RATE as usize;
    let mut out: Vec<f64> = (0..env.len()).map(|i| {
        let (a, b) = (i.saturating_sub(w / 2), (i + w / 2).min(env.len()));
        (env[i] - env[a..b].iter().sum::<f64>() / (b - a) as f64).max(0.0)
    }).collect();
    let sd = (out.iter().map(|x| x * x).sum::<f64>() / out.len().max(1) as f64).sqrt();
    // a steady sound's flux is rounding noise: below 1e-6 there's no onset to scale up
    if sd > 1e-6 {
        out.iter_mut().for_each(|x| *x /= sd);
    } else {
        out.iter_mut().for_each(|x| *x = 0.0);
    }
    out
}

/// The beat period in envelope frames: the autocorrelation's peak between 60 and 180 BPM, weighted by a log-Gaussian
/// around 120 BPM (one octave wide), so a half or double tempo wins only when it is clearly stronger; None when no
/// period stands out.
pub fn period(env: &[f64]) -> Option<f64> {
    let (lo, hi) = ((RATE * 60.0 / 180.0) as usize, (RATE * 60.0 / 60.0) as usize);
    if env.len() < hi * 2 {
        return None;
    }
    let ac = |lag: usize| env.iter().zip(&env[lag..]).map(|(a, b)| a * b).sum::<f64>() / (env.len() - lag) as f64;
    let weight = |lag: usize| (-0.5 * ((lag as f64 / (RATE * 0.5)).log2()).powi(2)).exp();
    let best = (lo..=hi).max_by(|a, b| (ac(*a) * weight(*a)).total_cmp(&(ac(*b) * weight(*b))))?;
    // a pulse stands out of the autocorrelation: its peak at least 3 times the range's mean (click tracks read 17-37,
    // steady noise near 1)
    let mean = (lo..=hi).map(ac).sum::<f64>() / (hi - lo + 1) as f64;
    if mean <= 0.0 || ac(best) < 3.0 * mean {
        return None;
    }
    // refine to a fraction of a frame by a parabola through the neighbours
    let (y0, y1, y2) = (ac(best - 1) * weight(best - 1), ac(best) * weight(best), ac(best + 1) * weight(best + 1));
    let d = y0 - 2.0 * y1 + y2;
    Some(best as f64 + if d.abs() > 1e-12 { 0.5 * (y0 - y2) / d } else { 0.0 })
}

/// The beats (seconds), by dynamic programming over the envelope: each frame's best score is its onset plus the best
/// predecessor half to twice a period back, penalised by how far that gap is from the period (log-squared, weight
/// `tight`); the best last beat is traced back.
pub fn track(env: &[f64], period: f64, tight: f64) -> Vec<f64> {
    let n = env.len();
    let mut score = env.to_vec();
    let mut back = vec![usize::MAX; n];
    for t in 0..n {
        let (a, b) = ((t as f64 - 2.0 * period).round().max(0.0) as usize, (t as f64 - period / 2.0).round());
        if b < 0.0 {
            continue;
        }
        let b = b as usize;
        let mut best = (f64::MIN, usize::MAX);
        for p in a..=b.min(t.saturating_sub(1)) {
            let s = score[p] - tight * ((t - p) as f64 / period).ln().powi(2);
            if s > best.0 {
                best = (s, p);
            }
        }
        if best.1 != usize::MAX && best.0 > 0.0 {
            score[t] = env[t] + best.0;
            back[t] = best.1;
        }
    }
    // the last beat: the best score in the final period
    let tail = n.saturating_sub(period.ceil() as usize);
    let Some(mut t) = (tail..n).max_by(|a, b| score[*a].total_cmp(&score[*b])) else { return Vec::new() };
    let mut beats = vec![t];
    while back[t] != usize::MAX {
        t = back[t];
        beats.push(t);
    }
    beats.reverse();
    beats.into_iter().map(|f| f as f64 / RATE).collect()
}

/// How strongly a pulse stands out: the autocorrelation's best (weighted) lag over the 60-180 BPM range's mean; click
/// tracks read 17-37, steady noise near 1, the UGC example's music bed 3.6 (librosa's onset autocorrelation: 3.4, peaking at the same 0.627 s).
pub fn strength(env: &[f64]) -> Option<f64> {
    let (lo, hi) = ((RATE * 60.0 / 180.0) as usize, (RATE * 60.0 / 60.0) as usize);
    if env.len() < hi * 2 {
        return None;
    }
    let ac = |lag: usize| env.iter().zip(&env[lag..]).map(|(a, b)| a * b).sum::<f64>() / (env.len() - lag) as f64;
    let mean = (lo..=hi).map(ac).sum::<f64>() / (hi - lo + 1) as f64;
    let p = period(env)?.round() as usize;
    (mean > 0.0).then(|| ac(p) / mean)
}

/// The beats of a sound, its tempo (BPM) and its pulse strength; empty when it is too short or has no pulse.
pub fn beats_with_strength(a: &Audio) -> (Vec<f64>, Option<f64>, Option<f64>) {
    let env = onsets(a);
    let Some(p) = period(&env) else { return (Vec::new(), None, strength(&env)) };
    (track(&env, p, 100.0), Some(60.0 * RATE / p), strength(&env))
}

/// The beats of a sound and its tempo (BPM); empty when it is too short or has no pulse.
pub fn beats(a: &Audio) -> (Vec<f64>, Option<f64>) {
    let (b, t, _) = beats_with_strength(a);
    (b, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a click track: 10 ms decaying noise bursts at `bpm`, starting at `first` s, over a quiet hum
    fn clicks(bpm: f64, first: f64, secs: f64) -> Audio {
        let rate = 22050u32;
        let mut s = vec![0f32; (secs * rate as f64) as usize];
        let mut seed = 1u32;
        for (i, x) in s.iter_mut().enumerate() {
            *x = 0.01 * (2.0 * std::f32::consts::PI * 110.0 * i as f32 / rate as f32).sin();
        }
        let mut t = first;
        while t < secs {
            let at = (t * rate as f64) as usize;
            for k in 0..(0.01 * rate as f64) as usize {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                if at + k < s.len() {
                    s[at + k] += 0.8 * ((seed >> 9) as f32 / (1u32 << 23) as f32 - 0.5) * (-(k as f32) / 60.0).exp();
                }
            }
            t += 60.0 / bpm;
        }
        Audio { rate, channels: vec![s] }
    }

    #[test]
    fn a_click_track_at_120_is_found() {
        let (b, bpm) = beats(&clicks(120.0, 0.25, 12.0));
        assert!((bpm.unwrap() - 120.0).abs() < 2.0, "{bpm:?}");
        // every beat within 20 ms of a click
        for t in &b {
            let k = ((t - 0.25) / 0.5).round();
            assert!((t - (0.25 + 0.5 * k)).abs() < 0.02, "{t}");
        }
        assert!(b.len() >= 22, "{}", b.len());
    }

    #[test]
    fn a_click_track_at_95_is_found() {
        let (_, bpm) = beats(&clicks(95.0, 0.1, 15.0));
        assert!((bpm.unwrap() - 95.0).abs() < 2.0, "{bpm:?}");
    }

    #[test]
    fn steady_noise_has_no_beat() {
        let rate = 22050u32;
        let mut seed = 7u32;
        let s: Vec<f32> = (0..rate as usize * 10).map(|_| { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); 0.1 * ((seed >> 9) as f32 / (1u32 << 23) as f32 - 0.5) }).collect();
        let (b, _) = beats(&Audio { rate, channels: vec![s] });
        // no clicks: no period stands out, so no beats
        assert!(b.is_empty(), "{}", b.len());
    }
}
