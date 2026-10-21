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
//! The map is JSON (`raea-perceive song FILE`, `raea-motion song FILE -o song.json`) that @raea/motion's song()
//! reads frame by frame (sdk/motion/src/song.tsx). Every time is in seconds from the file's start.

use std::path::Path;

use serde_json::{Value, json};

use crate::beatthis;

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

/// The map from a song's 22 050 Hz mono samples, its log-mel frames, beats and downbeats (seconds) and any
/// `[Section]` markers.
pub fn analyse(x: &[f32], spect: &[[f32; MELS]], beats: &[f64], downs: &[f64], marks: &[(f64, String)]) -> Value {
    let duration = x.len() as f64 / SR;
    let n = beats.len();
    if n < 8 {
        return json!({"version": 1, "duration": r3(duration), "tempo": null, "beatsPerBar": 4, "beats": beats, "downbeats": downs,
            "features": {}, "sections": [], "events": [], "bars": [], "note": "too few beats for a map"});
    }
    let mut gaps: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_by(f64::total_cmp);
    let median = gaps[gaps.len() / 2];
    // a beat closer than 0.6 of the usual gap to the one before is a stray (sparse intros): dropped
    let mut grid: Vec<f64> = Vec::with_capacity(n);
    for &t in beats {
        if grid.last().is_none_or(|l| t - l >= 0.6 * median) {
            grid.push(t);
        }
    }
    let beats = &grid[..];
    let n = beats.len();

    // each downbeat's beat index, and the bar length most bars have
    let mut down_i: Vec<usize> = downs.iter().map(|d| nearest(beats, *d)).collect();
    down_i.dedup();
    // a downbeat closer than two beats to the one before is a stray too
    let mut kept_d: Vec<usize> = Vec::new();
    for d in down_i {
        if kept_d.last().is_none_or(|l| d >= l + 2) {
            kept_d.push(d);
        }
    }
    let down_i = kept_d;
    // the tempo: a line fitted through the downbeats against their bar count (a missed downbeat counted by its gap),
    // over the beats per bar; never a median of single gaps, which 20 ms frames quantise (0.46 / 0.44 s read as one
    // tempo or the other)
    let fit = |pts: &[(f64, f64)]| -> Option<f64> {
        if pts.len() < 3 {
            return None;
        }
        let (mx, my) = (mean(&pts.iter().map(|p| p.0).collect::<Vec<_>>()), mean(&pts.iter().map(|p| p.1).collect::<Vec<_>>()));
        let (sxy, sxx) = pts.iter().fold((0.0, 0.0), |(a, c), (x, y)| (a + (x - mx) * (y - my), c + (x - mx) * (x - mx)));
        (sxx > 0.0).then(|| sxy / sxx)
    };
    let gap = {
        let mut bar_gaps: Vec<f64> = down_i.windows(2).map(|w| beats[w[1]] - beats[w[0]]).collect();
        bar_gaps.sort_by(f64::total_cmp);
        let bar_median = bar_gaps.get(bar_gaps.len() / 2).copied().unwrap_or(4.0 * median);
        let mut k = 0.0;
        let pts: Vec<(f64, f64)> = down_i
            .iter()
            .enumerate()
            .map(|(j, d)| {
                if j > 0 {
                    k += ((beats[*d] - beats[down_i[j - 1]]) / bar_median).round().max(1.0);
                }
                (k, beats[*d])
            })
            .collect();
        let beats_in_bar = (bar_median / median).round().max(1.0);
        fit(&pts).map(|s| s / beats_in_bar).unwrap_or_else(|| {
            let mut k = 0.0;
            let pts: Vec<(f64, f64)> = (0..n).map(|i| { if i > 0 { k += ((beats[i] - beats[i - 1]) / median).round().max(1.0); } (k, beats[i]) }).collect();
            fit(&pts).unwrap_or(median)
        })
    };
    let tempo = 60.0 / gap;
    let mut counts = [0usize; 9];
    for w in down_i.windows(2) {
        let c = w[1] - w[0];
        if c < counts.len() {
            counts[c] += 1;
        }
    }
    let bpb = (2..counts.len()).max_by_key(|c| counts[*c]).filter(|c| counts[*c] > 0).unwrap_or(4);
    // bars: one per downbeat, and a pickup before the first
    let mut bar_starts: Vec<usize> = Vec::new();
    if let Some(&first) = down_i.first() {
        if first > 0 {
            bar_starts.push(0);
        }
    }
    bar_starts.extend(&down_i);
    if bar_starts.is_empty() {
        bar_starts = (0..n).step_by(bpb).collect();
    }

    let f = frames(x, spect);
    let b = per_beat(&f, beats, duration);
    let time = |i: usize| if i < n { beats[i] } else { duration };
    let bar_of = |i: usize| bar_starts.iter().rposition(|s| *s <= i).unwrap_or(0);

    // ---- bars and pacing
    let bar_range = |j: usize| (bar_starts[j], if j + 1 < bar_starts.len() { bar_starts[j + 1] } else { n });
    let bars: Vec<(usize, usize, f64, f64, f64)> = (0..bar_starts.len())
        .map(|j| {
            let (s, e) = bar_range(j);
            let (en, on) = (mean(&b.energy[s..e]), mean(&b.onsets[s..e]));
            (s, e, en, on, rate(en, on))
        })
        .collect();

    // ---- sections: novelty boundaries, and the markers when there are any
    let nov = novelty(spect, beats, &b, 16);
    let dn: Vec<f64> = down_i.iter().map(|i| nov[*i]).collect();
    let thr = mean(&dn) + 0.25 * (dn.iter().map(|v| (v - mean(&dn)).powi(2)).sum::<f64>() / dn.len().max(1) as f64).sqrt();
    let mut cand: Vec<(usize, f64)> = down_i
        .iter()
        .enumerate()
        .filter(|(k, i)| {
            let v = nov[**i];
            v > thr && (k.saturating_sub(2)..(k + 3).min(down_i.len())).all(|q| nov[down_i[q]] <= v)
        })
        .map(|(_, i)| (*i, nov[*i]))
        .collect();
    cand.sort_by(|a, c| c.1.total_cmp(&a.1));
    let mut cuts: Vec<(usize, f64)> = Vec::new();
    for (i, v) in cand {
        if i >= 2 * bpb && i + 2 * bpb < n && cuts.iter().all(|(c, _)| c.abs_diff(i) >= 4 * bpb) {
            cuts.push((i, v));
        }
    }
    cuts.sort_by_key(|c| c.0);
    let nmax = nov.iter().copied().fold(1e-9, f64::max);
    let boundaries: Vec<Value> = cuts.iter().map(|(i, v)| json!({"time": r3(beats[*i]), "beat": i, "bar": bar_of(*i), "strength": r3(v / nmax)})).collect();

    // a section's start beat: a marker snaps to the nearest downbeat within half a bar, else the nearest beat
    let snap_start = |t: f64| -> usize {
        let bi = nearest(beats, t);
        let di = down_i.iter().copied().min_by(|a, c| (beats[*a] - t).abs().total_cmp(&(beats[*c] - t).abs()));
        match di {
            Some(d) if (beats[d] - t).abs() <= gap * bpb as f64 / 2.0 => d,
            _ => bi,
        }
    };
    let mut starts: Vec<(usize, Option<String>)> = if marks.is_empty() {
        std::iter::once((0, None)).chain(cuts.iter().map(|(i, _)| (*i, None))).collect()
    } else {
        let mut s: Vec<(usize, Option<String>)> = marks.iter().map(|(t, name)| (snap_start(*t), Some(name.clone()))).collect();
        if s[0].0 > bpb {
            s.insert(0, (0, Some("Intro".into())));
        } else {
            s[0].0 = 0;
        }
        s
    };
    starts.dedup_by(|a, c| a.0 == c.0);
    let song_mean = mean(&b.energy);
    let sections: Vec<(usize, usize, Option<String>, f64, &str, f64)> = (0..starts.len())
        .map(|k| {
            let (s, e) = (starts[k].0, if k + 1 < starts.len() { starts[k + 1].0 } else { n });
            let en = mean(&b.energy[s..e.max(s + 1).min(n)]);
            let level = if k == 0 && en < song_mean && starts.len() > 2 {
                "intro"
            } else if k + 1 == starts.len() && en < song_mean && starts.len() > 2 {
                "outro"
            } else if en < 0.35 {
                "low"
            } else if en < 0.6 {
                "mid"
            } else {
                "high"
            };
            // the section's pace: the median of its bars' rates
            let mut rs: Vec<f64> = bars.iter().filter(|bar| bar.0 >= s && bar.0 < e).map(|bar| bar.4).collect();
            rs.sort_by(f64::total_cmp);
            let pace = rs.get(rs.len() / 2).copied().unwrap_or(rate(en, 0.5));
            (s, e, starts[k].1.clone(), en, level, pace)
        })
        .collect();

    // ---- events
    let raw = &b.raw;
    let low = &b.low;
    let win = |v: &[f64], a: i64, c: i64| -> f64 {
        let (a, c) = (a.clamp(0, n as i64) as usize, c.clamp(0, n as i64) as usize);
        if c > a { mean(&v[a..c]) } else { 0.0 }
    };
    let bp = bpb as i64;
    let mut events: Vec<Value> = Vec::new();

    // the frames' onsets: peaks (the maximum within 80 ms either side)
    let nf = f.flux.len();
    let flux_hi = percentile(&f.flux, 99.0).max(1e-9);
    let flux_p95 = percentile(&f.flux, 95.0);
    let is_peak = |t: usize| {
        let (lo, hi) = (t.saturating_sub(4), (t + 5).min(nf));
        f.flux[t] > 0.0 && f.flux[lo..hi].iter().all(|v| *v <= f.flux[t])
    };
    // the strongest onset within 3 frames of a time
    let flux_near = |t: f64| -> f64 {
        let c = (t * FPS).round() as i64;
        (c - 3..=c + 3).filter(|k| *k >= 0 && (*k as usize) < nf).map(|k| f.flux[k as usize]).fold(0.0, f64::max)
    };
    // a time on the beat grid: the nearest beat, or the half beat when that's nearer
    let snap = |t: f64| -> f64 {
        let i = nearest(beats, t);
        let half = if beats[i] <= t && i + 1 < n { (beats[i] + beats[i + 1]) / 2.0 } else if i > 0 { (beats[i - 1] + beats[i]) / 2.0 } else { beats[i] };
        if (half - t).abs() < (beats[i] - t).abs() { half } else { beats[i] }
    };
    let hit_json = |t: f64, v: f64| json!({"time": r3(snap(t)), "at": r3(t), "strength": r3((v / flux_hi).min(1.0))});

    // stops: the full beat drops out (the low end, kick and bass, gone; the energy down) for one to eight beats and
    // comes back, leaving sparse hits
    let mut stops: Vec<(usize, usize, f64)> = Vec::new();
    let mut i = bpb;
    while i < n {
        let (rl, re) = (win(low, i as i64 - bp, i as i64), win(raw, i as i64 - bp, i as i64));
        let out = |k: usize| low[k] < 0.45 * rl || raw[k] < 0.5 * re;
        if rl > 0.35 && re > 0.25 && out(i) && !out(i - 1) {
            let mut e = i;
            while e < n && e - i < 2 * bpb && out(e) {
                e += 1;
            }
            let back = e < n && (low[e] >= 0.6 * rl || raw[e] >= 0.7 * re);
            let lands = down_i.contains(&e) && (raw[e] > 1.2 * re || low[e] > 0.9 * rl);
            if back && e - i <= 2 * bpb && (e - i >= 2 || lands) {
                stops.push((i, e, (re - win(raw, i as i64, e as i64)).max(0.0)));
                i = e;
                continue;
            }
        }
        i += 1;
    }

    // drops: on a downbeat, the bar after against the bar before (a slam), and the downbeat's own impact: the low end's
    // and the loudness's peak in its first 160 ms against the beats' peaks in the bar before (a chorus that lands on
    // one big hit and then cuts the band is a drop too: it doesn't hold)
    let db = |v: f64| 10.0 * (v + 1e-10).log10();
    let peak_at = |v: &[f64], t: f64| -> f64 {
        let c = (t * FPS).round() as i64;
        (c - 2..=c + 8).filter(|k| *k >= 0 && (*k as usize) < nf).map(|k| db(v[k as usize])).fold(-200.0, f64::max)
    };
    // against the beats of the bar before `from` (a drop after a stop is measured against the bar before the stop:
    // the beat only coming back isn't a drop)
    let impact_at = |p: usize, from: usize| -> f64 {
        // over the loudest beat of that bar: a verse's downbeat kick over its other beats isn't an impact
        let refs = |v: &[f64]| (from.saturating_sub(bpb)..from).map(|i| peak_at(v, beats[i])).fold(-200.0, f64::max);
        let (dl, dr) = (peak_at(&f.low, beats[p]) - refs(&f.low), peak_at(&f.rms, beats[p]) - refs(&f.rms));
        0.6 * (dl / 9.0).clamp(0.0, 1.0) + 0.4 * (dr / 4.0).clamp(0.0, 1.0)
    };
    let stop_into = |p: usize| stops.iter().find(|s| s.1 == p || s.1 + 1 == p).copied();
    // the breath before a drop: the quietest 40 ms of the 200 ms before the downbeat under the bar's usual level, 0..1
    // over 6 dB
    let gap_before = |p: usize| -> f64 {
        let c = (beats[p] * FPS).round() as usize;
        if c < 10 {
            return 0.0;
        }
        let quiet = (c - 10..c - 1).map(|k| db(mean(&f.rms[k..k + 2]))).fold(f64::INFINITY, f64::min);
        let a = ((beats[p.saturating_sub(bpb)] * FPS).round() as usize).min(c - 10);
        let usual = percentile(&f.rms[a..c - 10].iter().map(|v| db(*v)).collect::<Vec<_>>(), 50.0);
        ((usual - quiet) / 6.0).clamp(0.0, 1.0)
    };
    // a downbeat where the music changes (novelty, 0..1) lands harder
    let drop_base: Vec<(usize, f64, f64, f64)> = down_i
        .iter()
        .filter(|p| **p >= bpb && **p + 1 < n)
        .map(|&p| {
            let from = stop_into(p).map(|s| s.0).unwrap_or(p);
            let fi = from as i64;
            let jump = win(raw, p as i64, p as i64 + bp) - win(raw, fi - bp, fi);
            let impact = impact_at(p, from);
            (p, 0.45 * jump.max(0.0) + 0.3 * impact + 0.15 * nov[p] / nmax + 0.1 * gap_before(p), jump, impact)
        })
        .collect();

    // builds: what leads into a drop, a section or a stop, rising for two bars or more: the energy with the high band
    // and brightness (a riser's sweep), bar by bar
    let combo: Vec<f64> = (0..n).map(|i| 0.45 * raw[i] + 0.3 * b.high[i] + 0.25 * b.bright[i]).collect();
    let c = smooth(&combo, 1.5);
    let mut arrivals: Vec<usize> = drop_base.iter().filter(|d| d.1 > 0.06).map(|d| d.0).chain(sections.iter().map(|s| s.0)).chain(stops.iter().map(|s| s.0)).collect();
    arrivals.sort();
    arrivals.dedup();
    // every arrival's best build, then the strongest kept where they overlap (one that ends on a section's start or a
    // likely drop first)
    let bar_c = |s: usize| win(&c, s as i64, s as i64 + bp);
    let mut cands: Vec<(usize, usize, f64, f64)> = Vec::new();
    for &a in &arrivals {
        if a < 2 * bpb {
            continue;
        }
        // a build runs into the stop before its drop, not through it
        let e = stop_into(a).map(|s| s.0).unwrap_or(a);
        let end_c = win(&c, e as i64 - bp, e as i64);
        // a build lives in the section it leads out of (a pre-chorus, a bridge's second half)
        let floor = sections.iter().map(|x| x.0).filter(|x| *x < e).max().unwrap_or(0);
        // a gentle ramp counts when it lands on a new section (a loud pre-chorus has little headroom left)
        let min_rise = if sections.iter().any(|x| x.0 == a) { 0.05 } else { 0.1 };
        let mut best: Option<(usize, f64)> = None;
        for &s in down_i.iter().filter(|d| **d >= floor && **d + 2 * bpb <= e && **d + 16 * bpb >= e) {
            let rise = end_c - bar_c(s);
            // most bar-to-bar steps rise, and the build ends at its top
            let steps: Vec<f64> = (s..e.saturating_sub(bpb)).step_by(bpb).map(|k| bar_c(k + bpb) - bar_c(k)).collect();
            let up = steps.iter().filter(|d| **d > -0.02).count() as f64 / steps.len().max(1) as f64;
            let top = (s..e.saturating_sub(bpb)).step_by(bpb).map(bar_c).fold(0.0, f64::max);
            if up >= 0.7 && rise > min_rise && end_c >= top - 0.03 && best.is_none_or(|b| rise > b.1 + 0.02) {
                best = Some((s, rise));
            }
        }
        // a build doesn't run through a drop
        if let Some((s, rise)) = best
            && drop_base.iter().all(|d| d.0 <= s || d.0 >= e || d.1 < 0.4)
        {
            let lands = if sections.iter().any(|x| x.0 == a) { 0.15 } else if drop_base.iter().any(|d| d.0 == a && d.1 > 0.15) { 0.05 } else { 0.0 };
            cands.push((s, e, rise, rise + lands));
        }
    }
    cands.sort_by(|a, c| c.3.total_cmp(&a.3));
    let mut builds: Vec<(usize, usize, f64)> = Vec::new();
    for (s, e, rise, _) in cands {
        if builds.iter().all(|b| e <= b.0 || s >= b.1) {
            builds.push((s, e, rise));
        }
    }
    builds.sort_by_key(|b| b.0);
    if std::env::var_os("RAEA_SONG_DEBUG").is_some() {
        for &(p, base, jump, impact) in &drop_base {
            eprintln!("drop? {:7.2} base {base:.2} jump {jump:.2} impact {impact:.2} nov {:.2}", beats[p], nov[p] / nmax);
        }
    }
    // the drops: the base score, and what it ends (a build's rise, a stop)
    let mut drops: Vec<(usize, f64, f64)> = Vec::new();
    for &(p, base, jump, impact) in &drop_base {
        let build = builds.iter().filter(|b| b.1 <= p && b.1 + bpb >= p).map(|b| b.2).fold(0.0, f64::max);
        let section = sections.iter().any(|x| x.0 == p);
        let score = base + 0.25 * (2.0 * build).min(1.0) + if section { 0.1 } else { 0.0 };
        if score > 0.25 && raw[p] > 0.35 && (jump > 0.05 || impact > 0.1) {
            drops.push((p, score, jump));
        }
    }
    drops.sort_by(|a, c| c.1.total_cmp(&a.1));
    let mut kept: Vec<(usize, f64, f64)> = Vec::new();
    for d in drops {
        if kept.iter().all(|k| k.0.abs_diff(d.0) >= 4 * bpb) {
            kept.push(d);
        }
    }
    kept.sort_by_key(|d| d.0);
    let drops = kept;
    for &(s, e, rise) in &builds {
        let into = drops.iter().find(|d| d.0 >= e && d.0 <= e + 2 * bpb).map(|d| r3(beats[d.0]));
        events.push(json!({"kind": "build", "time": r3(beats[s]), "end": r3(time(e)), "beat": s, "bar": bar_of(s), "beats": e - s,
            "strength": r3(rise.min(1.0)), "into": into}));
    }
    for &(s, e, depth) in &stops {
        let mut hits: Vec<Value> = Vec::new();
        for t in ((time(s) - 0.05) * FPS) as usize..(((time(e) - 0.05) * FPS) as usize).min(nf) {
            if is_peak(t) && f.flux[t] > 0.7 * flux_p95 {
                let h = hit_json(t as f64 / FPS, f.flux[t]);
                match hits.last_mut() {
                    Some(l) if l["time"] == h["time"] => {
                        if h["strength"].as_f64() > l["strength"].as_f64() {
                            *l = h;
                        }
                    }
                    _ => hits.push(h),
                }
            }
        }
        let into = drops.iter().find(|d| d.0 == e || d.0 == e + 1).map(|d| r3(beats[d.0]));
        events.push(json!({"kind": "stop", "time": r3(time(s)), "end": r3(time(e)), "beat": s, "bar": bar_of(s), "beats": e - s,
            "strength": r3(depth.min(1.0)), "into": into, "hits": hits}));
    }
    for &(p, score, jump) in &drops {
        let after_stop = stops.iter().any(|s| s.1 == p || s.1 + 1 == p);
        let after_build = builds.iter().any(|b| b.1 <= p && b.1 + 2 * bpb >= p);
        // it holds when the bar after stays up; a drop that doesn't is one hit and then the band cuts
        let holds = jump > -0.05 && stops.iter().all(|s| s.0 > p + 1 || s.1 <= p);
        events.push(json!({"kind": "drop", "time": r3(beats[p]), "beat": p, "bar": bar_of(p), "strength": r3(score.min(1.0)),
            "rise": r3(jump), "holds": holds, "afterStop": after_stop, "afterBuild": after_build, "gapBefore": gap_before(p) > 0.5}));
    }

    // dips and breaks: a fall on a downbeat that holds two bars or more
    let mut dips: Vec<(usize, f64, f64)> = Vec::new();
    for &p in &down_i {
        let pi = p as i64;
        if pi < 2 * bp || p + 2 * bpb > n {
            continue;
        }
        let before = win(raw, pi - 2 * bp, pi);
        let after = win(raw, pi, pi + 2 * bp);
        let held = win(raw, pi, pi + 4 * bp);
        let fall = before - after;
        if fall > 0.18 && before - held > 0.12 && stops.iter().all(|s| p < s.0 || p >= s.1) {
            dips.push((p, fall, after));
        }
    }
    dips.sort_by(|a, c| c.1.total_cmp(&a.1));
    let mut dk: Vec<(usize, f64, f64)> = Vec::new();
    for d in dips {
        if dk.iter().all(|k| k.0.abs_diff(d.0) >= 4 * bpb) && drops.iter().all(|p| p.0.abs_diff(d.0) >= 2 * bpb) {
            dk.push(d);
        }
    }
    for (p, fall, after) in dk {
        // how long the fall holds: until a bar is back halfway, the next drop, or the next section
        let before = win(raw, p as i64 - 2 * bp, p as i64);
        let next = drops.iter().map(|d| d.0).chain(sections.iter().map(|x| x.0)).filter(|x| *x >= p + 2 * bpb).min().unwrap_or(n);
        let mut e = p + bpb;
        while e < next && win(raw, e as i64, e as i64 + bp) <= after + (before - after) / 2.0 {
            e += bpb;
        }
        let e = e.min(next).min(n);
        let kind = if after < 0.35 { "break" } else { "dip" };
        events.push(json!({"kind": kind, "time": r3(beats[p]), "end": r3(time(e)), "beat": p, "bar": bar_of(p), "beats": e - p, "strength": r3(fall.min(1.0))}));
    }

    // hits: accents that break the groove (a stab, a crash, a fill's last hit): an onset well over the same place in
    // the bars either side, a beat apart at least
    let bar_s = gap * bpb as f64;
    let mut hits: Vec<(f64, f64, f64)> = Vec::new();
    for t in 0..nf {
        if !is_peak(t) || f.flux[t] < flux_p95 {
            continue;
        }
        let ts = t as f64 / FPS;
        let mut around: Vec<f64> = [-2.0, -1.0, 1.0, 2.0].iter().map(|k| ts + k * bar_s).filter(|x| *x >= 0.0 && *x < duration).map(flux_near).collect();
        if around.len() < 2 {
            continue;
        }
        around.sort_by(f64::total_cmp);
        let usual = around[around.len() / 2].max(1e-9);
        let over = f.flux[t] / usual;
        let in_stop = stops.iter().any(|s| ts >= time(s.0) - 0.05 && ts < time(s.1) - 0.05);
        if over > 2.5 && !in_stop {
            hits.push((ts, f.flux[t], over));
        }
    }
    hits.sort_by(|a, c| (c.1 * c.2).total_cmp(&(a.1 * a.2)));
    let mut hk: Vec<(f64, f64, f64)> = Vec::new();
    for h in hits {
        if hk.iter().all(|k| (k.0 - h.0).abs() >= gap) {
            hk.push(h);
        }
    }
    hk.sort_by(|a, c| a.0.total_cmp(&c.0));
    for (t, v, over) in hk {
        let i = nearest(beats, t);
        let mut h = hit_json(t, v);
        h["kind"] = json!("hit");
        h["beat"] = json!(i);
        h["bar"] = json!(bar_of(i));
        h["over"] = json!(r3(over));
        events.push(h);
    }
    events.sort_by(|a, c| a["time"].as_f64().unwrap_or(0.0).total_cmp(&c["time"].as_f64().unwrap_or(0.0)));

    if let Some(p) = std::env::var_os("RAEA_SONG_FRAMES") {
        let mut csv = String::from("t,rms,low,high,centroid,flux\n");
        for t in 0..nf {
            csv += &format!("{:.2},{:.6},{:.6},{:.6},{:.3},{:.3}\n", t as f64 / FPS, f.rms[t], f.low[t], f.high[t], f.centroid[t], f.flux[t]);
        }
        let _ = std::fs::write(p, csv);
    }
    if std::env::var_os("RAEA_SONG_DEBUG").is_some() {
        for (j, bar) in bars.iter().enumerate() {
            let (s, e) = (bar.0, bar.1);
            eprintln!("bar {j:3} {:7.2}s  E {:.2} raw {:.2} loud {:.2} low {:.2} on {:.2} br {:.2} hi {:.2} nov {:.2}  rate {}", beats[s],
                bar.2, mean(&raw[s..e]), mean(&b.loud[s..e]), mean(&b.low[s..e]), bar.3, mean(&b.bright[s..e]), mean(&b.high[s..e]), nov[s] / nmax, bar.4);
        }
    }

    let q = |v: &[f64]| v.iter().map(|x| r3(*x)).collect::<Vec<_>>();
    json!({
        "version": 1,
        "duration": r3(duration),
        "tempo": (tempo * 100.0).round() / 100.0,
        "beatsPerBar": bpb,
        "beats": q(beats),
        "downbeats": down_i.iter().map(|i| r3(beats[*i])).collect::<Vec<_>>(),
        "features": {"loudness": q(&b.loud), "onsets": q(&b.onsets), "brightness": q(&b.bright), "low": q(&b.low), "high": q(&b.high), "energy": q(&b.energy)},
        "bars": bars.iter().map(|bar| json!({"time": r3(beats[bar.0]), "beat": bar.0, "beats": bar.1 - bar.0, "energy": r3(bar.2), "cuts": bar.4})).collect::<Vec<_>>(),
        "sections": sections.iter().map(|s| json!({"name": s.2.clone().unwrap_or_else(|| s.4.to_string()), "level": s.4, "time": r3(time(s.0)), "end": r3(time(s.1)),
            "beat": s.0, "bar": bar_of(s.0), "energy": r3(s.3), "cuts": s.5})).collect::<Vec<_>>(),
        "sectionsFrom": if marks.is_empty() { "novelty" } else { "markers" },
        "boundaries": boundaries,
        "events": events,
    })
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
}
