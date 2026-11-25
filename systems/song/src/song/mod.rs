//! A song's energy map: what a builder needs to pace cuts to music beyond its beats. Beats, downbeats and bars come
//! from Beat This! (beatthis.rs); on that grid, per beat and normalised 0..1 over the song: loudness (RMS), onsets
//! (spectral flux on the same log-mel frames Beat This! reads), brightness (spectral centroid), low end (the kick and
//! bass band, under 150 Hz), high (the band over 3 kHz), and a combined energy. From those:
//!
//!   sections  boundaries from novelty (a checkerboard kernel along the self-similarity of beat-synced mel bands and
//!             features) snapped to downbeats, or the file's own `[Section]` markers (a timed-text stream, or an .srt
//!             beside it) when it has them; each with its mean energy and a level (intro, low, mid, high, outro)
//!   events    build (energy and brightness rising over 2+ bars, start and end), drop (a big jump in energy and low end
//!             on a downbeat), stop (the full beat drops out for a few beats, leaving sparse hits, usually before a
//!             drop; its hits listed), hit (a strong isolated accent: a stab, a crash), dip / break (a fall that holds)
//!   pacing    per bar and per section, a suggested cut rate (cuts per bar: 0.25, 0.5, 1, 2 or 4) from energy and
//!             onset density
//!
//! The map is JSON (`raea-perceive song FILE`, `raea song FILE -o song.json`) that raea's song()
//! reads frame by frame (sdk/raea/src/song.tsx). Every time is in seconds from the file's start.

use std::path::Path;

use serde_json::{Value, json};

use crate::beatthis;

mod analyse;

pub use analyse::*;

const SR: f64 = 22_050.0;
const HOP: usize = 441;
const FPS: f64 = beatthis::FPS;
const MELS: usize = 128;

/// The ladder of cut rates (cuts per bar) pacing suggests.
pub const RATES: [f64; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
}

fn percentile(v: &[f64], p: f64) -> f64 {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if s.is_empty() {
        return 0.0;
    }
    s.sort_by(f64::total_cmp);
    let i = (p / 100.0 * (s.len() - 1) as f64).round() as usize;
    s[i.min(s.len() - 1)]
}

/// v scaled so its lo-th percentile is 0 and hi-th is 1, clamped.
fn normalise(v: &[f64], lo: f64, hi: f64) -> Vec<f64> {
    let (a, b) = (percentile(v, lo), percentile(v, hi));
    let span = (b - a).max(1e-9);
    v.iter().map(|x| ((x - a) / span).clamp(0.0, 1.0)).collect()
}

/// A Gaussian smoothing with sigma in samples (edges renormalised).
fn smooth(v: &[f64], sigma: f64) -> Vec<f64> {
    if sigma <= 0.0 {
        return v.to_vec();
    }
    let r = (3.0 * sigma).ceil() as i64;
    let w: Vec<f64> = (-r..=r).map(|k| (-(k * k) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    (0..v.len() as i64)
        .map(|i| {
            let (mut s, mut n) = (0.0, 0.0);
            for k in -r..=r {
                let j = i + k;
                if j >= 0 && (j as usize) < v.len() {
                    s += v[j as usize] * w[(k + r) as usize];
                    n += w[(k + r) as usize];
                }
            }
            s / n
        })
        .collect()
}

// ---- markers ----------------------------------------------------------------------------------------------------

/// `[Section]` cues of an SRT document: (start seconds, name).
pub fn srt_markers(srt: &str) -> Vec<(f64, String)> {
    let ts = |s: &str| -> Option<f64> {
        let s = s.trim().replace(',', ".");
        let p: Vec<&str> = s.split(':').collect();
        if p.len() != 3 {
            return None;
        }
        Some(p[0].parse::<f64>().ok()? * 3600.0 + p[1].parse::<f64>().ok()? * 60.0 + p[2].parse::<f64>().ok()?)
    };
    let mut out = Vec::new();
    for block in srt.replace("\r\n", "\n").split("\n\n") {
        let lines: Vec<&str> = block.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some(start) = lines[ti].split("-->").next().and_then(ts) else { continue };
        for l in &lines[ti + 1..] {
            if l.starts_with('[') && l.ends_with(']') && l.len() > 2 {
                out.push((start, l[1..l.len() - 1].trim().to_string()));
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// A song's section markers and where they came from: an explicit .srt, else `<stem>.srt` beside the file, else the
/// file's own first timed-text stream (an m4a's mov_text, through ffmpeg when it's installed).
pub fn markers(path: &Path, explicit: Option<&Path>) -> (Vec<(f64, String)>, Option<String>) {
    let from_srt = |p: &Path| std::fs::read_to_string(p).ok().map(|s| srt_markers(&s)).filter(|m| !m.is_empty());
    if let Some(p) = explicit {
        if p.as_os_str() == "none" {
            return (Vec::new(), None);
        }
        return (from_srt(p).unwrap_or_default(), Some(p.display().to_string()));
    }
    let side = path.with_extension("srt");
    if let Some(m) = from_srt(&side) {
        return (m, Some(side.display().to_string()));
    }
    let out = std::process::Command::new("ffmpeg").args(["-v", "error", "-nostdin", "-i"]).arg(path).args(["-map", "0:s:0", "-f", "srt", "-"]).output();
    if let Ok(o) = out
        && o.status.success()
    {
        let m = srt_markers(&String::from_utf8_lossy(&o.stdout));
        if !m.is_empty() {
            return (m, Some(format!("{} (timed text)", path.display())));
        }
    }
    (Vec::new(), None)
}

// ---- features ---------------------------------------------------------------------------------------------------

/// Per-frame (50 a second) measures from the samples and the log-mel frames.
struct Frames {
    rms: Vec<f64>,
    low: Vec<f64>,
    high: Vec<f64>,
    centroid: Vec<f64>,
    flux: Vec<f64>,
}

fn frames(x: &[f32], spect: &[[f32; MELS]]) -> Frames {
    // the mel bands' centres, as beatthis's filterbank places them (30 Hz to 11 kHz)
    let (m0, m1) = (beatthis::hz_to_mel(30.0), beatthis::hz_to_mel(11_000.0));
    let centre: Vec<f64> = (0..MELS).map(|m| beatthis::mel_to_hz(m0 + (m1 - m0) * (m + 1) as f64 / (MELS + 1) as f64)).collect();
    let n = spect.len();
    let mut f = Frames { rms: vec![0.0; n], low: vec![0.0; n], high: vec![0.0; n], centroid: vec![0.0; n], flux: vec![0.0; n] };
    for t in 0..n {
        let (a, b) = ((t * HOP).saturating_sub(HOP), ((t + 1) * HOP).min(x.len()));
        if b > a {
            f.rms[t] = x[a..b].iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / (b - a) as f64;
        }
        let (mut lo, mut hi, mut num, mut den) = (0.0, 0.0, 0.0, 0.0);
        for k in 0..MELS {
            // the band's magnitude back from log1p(1000 x)
            let m = ((spect[t][k] as f64).exp() - 1.0) / 1000.0;
            if centre[k] <= 150.0 {
                lo += m * m;
            }
            if centre[k] >= 3000.0 {
                hi += m * m;
            }
            num += centre[k].log2() * m;
            den += m;
        }
        f.low[t] = lo;
        f.high[t] = hi;
        f.centroid[t] = if den > 1e-9 { num / den } else { f64::NAN };
        if t > 0 {
            f.flux[t] = (0..MELS).map(|k| (spect[t][k] - spect[t - 1][k]).max(0.0) as f64).sum();
        }
    }
    f
}

/// What happens per beat, raw (dB, log2 Hz, flux) and normalised.
struct Beats {
    loud: Vec<f64>,
    low: Vec<f64>,
    high: Vec<f64>,
    bright: Vec<f64>,
    onsets: Vec<f64>,
    /// the combined energy before smoothing, and after
    raw: Vec<f64>,
    energy: Vec<f64>,
}

fn per_beat(f: &Frames, beats: &[f64], end: f64) -> Beats {
    let n = beats.len();
    let nf = f.rms.len();
    let span = |i: usize| -> (usize, usize) {
        let a = (beats[i] * FPS).round() as usize;
        let b = if i + 1 < n { (beats[i + 1] * FPS).round() as usize } else { ((end.min(beats[i] + 0.5)) * FPS).round() as usize };
        (a.min(nf.saturating_sub(1)), b.max(a + 1).min(nf))
    };
    let db = |v: &[f64], i: usize| {
        let (a, b) = span(i);
        10.0 * (mean(&v[a..b]) + 1e-10).log10()
    };
    let loud: Vec<f64> = (0..n).map(|i| db(&f.rms, i)).collect();
    let low: Vec<f64> = (0..n).map(|i| db(&f.low, i)).collect();
    let high: Vec<f64> = (0..n).map(|i| db(&f.high, i)).collect();
    let onsets: Vec<f64> = (0..n).map(|i| { let (a, b) = span(i); mean(&f.flux[a..b]) }).collect();
    let bright: Vec<f64> = (0..n)
        .map(|i| {
            let (a, b) = span(i);
            let v: Vec<f64> = f.centroid[a..b].iter().copied().filter(|c| c.is_finite()).collect();
            if v.is_empty() { f64::NAN } else { mean(&v) }
        })
        .collect();
    let fill = |v: Vec<f64>| -> Vec<f64> { let m = percentile(&v, 50.0); v.into_iter().map(|x| if x.is_finite() { x } else { m }).collect() };
    let (loud, low, high, onsets, bright) =
        (normalise(&loud, 3.0, 98.0), normalise(&low, 3.0, 98.0), normalise(&high, 3.0, 98.0), normalise(&onsets, 3.0, 98.0), normalise(&fill(bright), 3.0, 98.0));
    let raw: Vec<f64> = (0..n).map(|i| 0.4 * loud[i] + 0.25 * low[i] + 0.2 * onsets[i] + 0.15 * high[i]).collect();
    let raw = normalise(&raw, 2.0, 99.0);
    // a light smoothing (sigma 1 beat): a beat's wobble goes, a drop stays a step
    let energy = normalise(&smooth(&raw, 1.0), 2.0, 99.0);
    Beats { loud, low, high, bright, onsets, raw, energy }
}

// ---- sections ---------------------------------------------------------------------------------------------------

/// Novelty per beat: a Gaussian-tapered checkerboard kernel (half-width `l` beats) along the cosine self-similarity
/// of beat-synced mel bands (16 groups) and the beat features, each z-scored over the song.
fn novelty(spect: &[[f32; MELS]], beats: &[f64], b: &Beats, l: usize) -> Vec<f64> {
    let n = beats.len();
    let nf = spect.len();
    let mut rows: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let a = ((beats[i] * FPS).round() as usize).min(nf.saturating_sub(1));
            let e = if i + 1 < n { ((beats[i + 1] * FPS).round() as usize).min(nf) } else { (a + 25).min(nf) };
            let e = e.max(a + 1);
            let mut v = vec![0.0; 16];
            for t in a..e {
                for k in 0..MELS {
                    v[k / 8] += spect[t][k] as f64;
                }
            }
            v.iter_mut().for_each(|x| *x /= (e - a) as f64 * 8.0);
            v.extend([b.loud[i], b.low[i], b.onsets[i], b.bright[i], b.high[i]]);
            v
        })
        .collect();
    // the features weigh a little more than one mel group each
    let weight = |k: usize| [2.0, 2.0, 1.5, 1.5, 1.5].get(k.wrapping_sub(16)).copied().unwrap_or(1.0);
    let d = rows.first().map(Vec::len).unwrap_or(0);
    for k in 0..d {
        let col: Vec<f64> = rows.iter().map(|r| r[k]).collect();
        let m = mean(&col);
        let sd = (col.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / n.max(1) as f64).sqrt().max(1e-9);
        let w = weight(k);
        rows.iter_mut().for_each(|r| r[k] = (r[k] - m) / sd * w);
    }
    let norms: Vec<f64> = rows.iter().map(|r| r.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-9)).collect();
    let sim = |i: usize, j: usize| rows[i].iter().zip(&rows[j]).map(|(a, b)| a * b).sum::<f64>() / (norms[i] * norms[j]);
    let li = l as i64;
    let sigma = l as f64 / 2.0;
    (0..n as i64)
        .map(|c| {
            let mut s = 0.0;
            for a in -li..li {
                for bb in -li..li {
                    let (i, j) = (c + a, c + bb);
                    if i < 0 || j < 0 || i >= n as i64 || j >= n as i64 {
                        continue;
                    }
                    let sign = if (a < 0) == (bb < 0) { 1.0 } else { -1.0 };
                    let g = (-((a as f64 + 0.5).powi(2) + (bb as f64 + 0.5).powi(2)) / (2.0 * sigma * sigma)).exp();
                    s += sign * g * sim(i as usize, j as usize);
                }
            }
            s.max(0.0)
        })
        .collect()
}

// ---- the map ----------------------------------------------------------------------------------------------------

fn nearest(v: &[f64], t: f64) -> usize {
    let mut best = 0;
    for (i, x) in v.iter().enumerate() {
        if (x - t).abs() < (v[best] - t).abs() {
            best = i;
        }
    }
    best
}

/// The cut rate on the ladder for a bar's energy and onset density.
fn rate(energy: f64, onsets: f64) -> f64 {
    let c = 0.7 * energy + 0.3 * onsets;
    if c < 0.2 {
        0.25
    } else if c < 0.4 {
        0.5
    } else if c < 0.6 {
        1.0
    } else if c < 0.8 {
        2.0
    } else {
        4.0
    }
}

/// Times below the 20 ms frame grid: each peak moved to the vertex of the parabola through its logit and its two
/// neighbours (at most half a frame either way). A peak already between two frames (adjacent maxima averaged) stays.
pub fn refine(times: &[f64], logits: &[f32]) -> Vec<f64> {
    times
        .iter()
        .map(|&t| {
            let f = t * FPS;
            let c = f.round();
            if (f - c).abs() > 1e-6 || c < 1.0 || c as usize + 1 >= logits.len() {
                return t;
            }
            let i = c as usize;
            let (a, b, d) = (logits[i - 1] as f64, logits[i] as f64, logits[i + 1] as f64);
            let den = a - 2.0 * b + d;
            if den >= 0.0 {
                return t;
            }
            ((c + (0.5 * (a - d) / den).clamp(-0.5, 0.5)) / FPS * 1e4).round() / 1e4
        })
        .collect()
}

/// A file's map, with the time it took: Beat This! for the grid, then everything above. `srt` overrides where the
/// section markers come from.
pub fn file(path: &Path, backend: &str, srt: Option<&Path>) -> Result<Value, String> {
    let t = std::time::Instant::now();
    // AVFoundation reads no Opus in an m4a (a lyrics-tracked song often is one): ffmpeg then, when it's there
    let x = beatthis::pcm(path, backend).or_else(|e| if backend == "apple" { beatthis::pcm(path, "ffmpeg").map_err(|_| e) } else { Err(e) })?;
    let spect = beatthis::spectrogram(&x);
    let (bl, dl) = beatthis::BeatThis::load()?.logits(&spect)?;
    let (beats, downs) = beatthis::beats(&bl, &dl);
    let (beats, downs) = (refine(&beats, &bl), refine(&downs, &dl));
    let (marks, from) = markers(path, srt);
    let mut m = analyse(&x, &spect, &beats, &downs, &marks);
    m["source"] = json!(path.file_name().map(|n| n.to_string_lossy().to_string()));
    m["markers"] = json!(from);
    m["model"] = json!("beat_this small0");
    m["seconds"] = json!(r3(t.elapsed().as_secs_f64()));
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_section_markers_are_read_and_lyrics_ignored() {
        let s = "1\n00:00:13,404 --> 00:00:13,404\n[Verse 1]\n\n2\n00:00:13,404 --> 00:00:17,154\nYou drew a crooked sun\n\n3\n00:01:54,654 --> 00:01:54,654\n[Glitch Ska Break]\n";
        assert_eq!(srt_markers(s), vec![(13.404, "Verse 1".to_string()), (114.654, "Glitch Ska Break".to_string())]);
    }

    #[test]
    fn stray_downbeats_between_whole_bars_are_dropped_and_a_phase_that_holds_stays() {
        // #49: a downbeat marked mid-bar (51, 55, 59), and two in a row off the phase (67, 71) between 65 and 73
        let model = [1, 5, 9, 49, 51, 53, 55, 57, 59, 61, 65, 67, 71, 73, 77];
        assert_eq!(analyse::steady(&model, 4), vec![1, 5, 9, 49, 53, 57, 61, 65, 69, 73, 77]);
        // the other decode's downbeats there, with no strays: the same bars
        assert_eq!(analyse::steady(&[1, 5, 9, 49, 53, 57, 61, 65, 69, 73, 77], 4), vec![1, 5, 9, 49, 53, 57, 61, 65, 69, 73, 77]);
        // a dropped beat moves the phase for good (three downbeats hold it), and bars of three in a song of fours stay
        assert_eq!(analyse::steady(&[0, 4, 8, 11, 15, 19, 23], 4), vec![0, 4, 8, 11, 15, 19, 23]);
        assert_eq!(analyse::steady(&[0, 4, 8, 11, 14, 17, 20, 24], 4), vec![0, 4, 8, 11, 14, 17, 20, 24]);
    }

    #[test]
    fn beats_refine_to_the_parabola_vertex() {
        let mut l = vec![-5f32; 10];
        (l[3], l[4], l[5]) = (1.0, 3.0, 2.0);
        let t = refine(&[4.0 / FPS, 4.5 / FPS], &l);
        assert!((t[0] - (4.0 + 0.5 * (1.0 - 2.0) / (1.0 - 6.0 + 2.0)) / FPS).abs() < 1e-4);
        assert_eq!(t[1], 4.5 / FPS);
    }

    #[test]
    fn rates_climb_the_ladder_with_energy() {
        assert_eq!(rate(0.05, 0.0), 0.25);
        assert_eq!(rate(0.5, 0.5), 1.0);
        assert_eq!(rate(1.0, 1.0), 4.0);
        assert!(RATES.contains(&rate(0.7, 0.2)));
    }

    /// A synthetic song at 120 bpm: a quiet first half (a soft kick and hat), a hush of one bar with the kick gone, then
    /// a loud second half. The map finds the
    /// drop on the downbeat where the loud half starts, and a stop just before it.
    #[test]
    fn a_drop_after_a_stop_is_found_on_its_downbeat() {
        let secs = 48.0;
        let n = (secs * SR) as usize;
        let mut x = vec![0f32; n];
        let beat = 0.5;
        let drop_t = 24.0;
        for (i, v) in x.iter_mut().enumerate() {
            let t = i as f64 / SR;
            let ph = (t % beat) / beat;
            let hush = (drop_t - 2.0..drop_t).contains(&t);
            let loud = t >= drop_t;
            // a hat every beat (noise burst), louder and with a kick after the drop
            let noise = (((i as u64).wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0;
            let env = (-ph * 30.0).exp();
            let hat = noise * env * if hush { 0.02 } else if loud { 0.3 } else { 0.08 };
            let kick = (2.0 * std::f64::consts::PI * 55.0 * t).sin() * (-ph * 12.0).exp() * if hush { 0.0 } else if loud { 0.8 } else { 0.3 };
            *v = (hat + kick) as f32;
        }
        let beats: Vec<f64> = (0..(secs / beat) as usize).map(|i| i as f64 * beat).collect();
        let downs: Vec<f64> = beats.iter().copied().step_by(4).collect();
        let spect = beatthis::spectrogram(&x);
        let m = analyse(&x, &spect, &beats, &downs, &[]);
        let ev = m["events"].as_array().unwrap();
        let drop = ev.iter().find(|e| e["kind"] == "drop").expect("a drop");
        assert_eq!(drop["time"].as_f64().unwrap(), drop_t);
        let stop = ev.iter().find(|e| e["kind"] == "stop").expect("a stop");
        assert_eq!(stop["end"].as_f64().unwrap(), drop_t);
        assert!(stop["time"].as_f64().unwrap() >= drop_t - 2.5);
        assert_eq!(m["beatsPerBar"], 4);
    }

    /// #49: downbeats two beats apart in places (as Beat This! marks some songs) leave the tempo the beats' own
    #[test]
    fn half_bars_leave_the_tempo_the_beats() {
        let beat = 0.605;
        let x = vec![0f32; (40.0 * SR) as usize];
        let beats: Vec<f64> = (0..64).map(|i| 0.02 + i as f64 * beat).collect();
        // bars of 4, then 2, 4, 4, 2, ... (every third bar half)
        let (mut downs, mut i, mut k) = (Vec::new(), 0, 0);
        while i < beats.len() {
            downs.push(beats[i]);
            i += if k % 3 == 1 { 2 } else { 4 };
            k += 1;
        }
        let m = analyse(&x, &beatthis::spectrogram(&x), &beats, &downs, &[]);
        assert!((m["tempo"].as_f64().unwrap() - 60.0 / beat).abs() < 0.05, "tempo {} for beats {beat} s apart", m["tempo"]);
    }
}
