//! `raea-motion ci ...`: the sound measurements the CI's gates make (bench/games/audio.sh, bench/tone/check.sh,
//! bench/speech/check.sh), where bash can't do the sample math:
//!   ci bounce-vs-live BOUNCE.wav TAP.wav [--skip SECONDS] [--json OUT]
//!       render equals play, for sound: the mixer core's bounce (RAEA_AUDIO_BOUNCE=OUT.wav on a render) held against
//!       live play's tap of the same frames (RAEA_AUDIO_TAP=TAP.wav, with TAP.json beside it: where the timeline sat
//!       on the device's clock). The bounce's sample t is the tap's origin + t. Prints, as JSON, the largest
//!       difference over the stretch both cover, how many samples differ by more than 1e-6, and, as a check on the
//!       alignment, the offset (within +-50 ms) where the two agree best.
//!   ci clicks FILE...
//!       clicks in a sound file: a click is a sample whose second difference (the curvature a step or a cut leaves)
//!       stands far above the sound around it: over 40 times the median curvature of the 10 ms around it and over an
//!       absolute floor (-40 dBFS), so a loud attack in loud sound isn't one and a step in a quiet passage is. Prints
//!       the integrated loudness and true peak (ffmpeg's ebur128) and the clicks found, with their times, a line a file.
//!   ci duck DUCK.wav NODUCK.wav NOVOICE.wav FROM-TO...
//!       a music bus's level under a voice: the music alone is DUCK - NODUCK + NOVOICE; prints, a line a window, its
//!       energy against NOVOICE's over that window (seconds), in dB.
//! These replace the Python the gates ran before (bounce_vs_live.py, clicks.py, tone/check.sh's ducking): each does
//! the same arithmetic in the same order, so their numbers are the same to the last bit.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::process::Command;

pub fn cli(args: &[String]) -> i32 {
    let r = match args.first().map(String::as_str) {
        Some("bounce-vs-live") => bounce_vs_live(&args[1..]),
        Some("clicks") => clicks(&args[1..]),
        Some("duck") => duck(&args[1..]),
        _ => Err("usage: raea-motion ci bounce-vs-live BOUNCE.wav TAP.wav [--skip S] [--json OUT] | ci clicks FILE... | \
                  ci duck DUCK.wav NODUCK.wav NOVOICE.wav FROM-TO..."
            .into()),
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

const RATE: usize = 48000;

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// A WAV as the mixer core writes it (a 44-byte header, 32-bit float, interleaved): its format tag, channels, rate and
/// the samples after the "data" tag
fn wav(path: &str) -> Result<(u16, u16, u32, Vec<f64>), String> {
    let b = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if b.len() < 28 {
        return Err(format!("{path}: not a WAV"));
    }
    let fmt = u16::from_le_bytes([b[20], b[21]]);
    let ch = u16::from_le_bytes([b[22], b[23]]);
    let rate = u32::from_le_bytes([b[24], b[25], b[26], b[27]]);
    let i = b.windows(4).position(|w| w == b"data").ok_or(format!("{path}: no data chunk"))? + 8;
    let s = b.get(i..).unwrap_or(&[]).chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect();
    Ok((fmt, ch, rate, s))
}

#[derive(Deserialize)]
struct Side {
    anchors: Vec<(serde_json::Value, i64)>,
    report: Option<Box<RawValue>>,
}

#[derive(Serialize)]
struct Same {
    bounce_s: f64,
    tap_s: f64,
    compared_s: f64,
    origin: i64,
    anchors: usize,
    max_abs_diff: f64,
    /// null when the two are the same (-inf dB)
    max_abs_diff_db: Option<f64>,
    at_s: Option<f64>,
    #[serde(rename = "samples_over_1e-6")]
    samples_over: u64,
    best_lag_samples: Option<i64>,
    live: Option<Box<RawValue>>,
}

fn bounce_vs_live(args: &[String]) -> Result<(), String> {
    let (bounce, tap) = match args {
        [b, t, ..] => (b.as_str(), t.as_str()),
        _ => return Err("usage: raea-motion ci bounce-vs-live BOUNCE.wav TAP.wav [--skip SECONDS] [--json OUT]".into()),
    };
    let skip: f64 = flag(args, "--skip").map(|s| s.parse().map_err(|_| format!("--skip {s}: not a number"))).transpose()?.unwrap_or(0.0);
    let out = flag(args, "--json");
    let (fb, cb, rb, b) = wav(bounce)?;
    let (ft, ct, rt, t) = wav(tap)?;
    for (f, p) in [(fb, bounce), (ft, tap)] {
        if f != 3 {
            return Err(format!("{p}: not a float WAV"));
        }
    }
    let side_path = match tap.rfind('.') {
        Some(i) => format!("{}.json", &tap[..i]),
        None => format!("{tap}.json"),
    };
    let side: Side = serde_json::from_str(&std::fs::read_to_string(&side_path).map_err(|e| format!("{side_path}: {e}"))?)
        .map_err(|e| format!("{side_path}: {e}"))?;
    if rb != rt || cb != 2 || ct != 2 {
        return Err(format!("rates or channels differ: bounce {rb} Hz x{cb}, tap {rt} Hz x{ct}"));
    }
    let origin = side.anchors.first().ok_or(format!("{side_path}: no anchors"))?.1;
    // the first frame the play stepped: the bounce from that frame's sample (its timeline sample), the tap from origin on
    let (nb, nt) = ((b.len() / 2) as i64, (t.len() / 2) as i64);
    let start = ((skip * rb as f64) as i64).max(0);
    let end = nb.min(nt - origin);
    let (mut worst, mut worst_at, mut over) = (0f64, -1i64, 0u64);
    for i in start..end {
        let j = origin + i;
        if j < 0 {
            continue;
        }
        for c in 0..2 {
            let d = (b[(2 * i + c) as usize] - t[(2 * j + c) as usize]).abs();
            if d > 1e-6 {
                over += 1;
            }
            if d > worst {
                worst = d;
                worst_at = i;
            }
        }
    }
    // the offset where they agree best, over a 2 s window with sound in it (a check that the anchor is the alignment):
    // the window is the loudest 2 s (every 16th sample's magnitude summed) of those starting every 0.1 s
    let energy: Vec<f64> = (start..end.max(start)).map(|i| b[(2 * i) as usize].abs()).collect();
    let n = energy.len() as i64;
    let mut best_k: Option<(i64, f64)> = None;
    let mut k = 0;
    while k < 1.max(n - 96000) {
        let mut s = 0.0;
        let mut x = k;
        while x < (k + 96000).min(n) {
            s += energy[x as usize];
            x += 16;
        }
        if best_k.is_none_or(|(_, m)| s > m) {
            best_k = Some((k, s));
        }
        k += 4800;
    }
    let mid = start + best_k.map(|(k, _)| k).unwrap_or(0);
    let win_end = (mid + 96000).min(end);
    let mut best: Option<(i64, f64)> = None;
    for lag in -2400..=2400i64 {
        let mut s = 0.0;
        let mut i = mid;
        while i < win_end {
            let j = origin + i + lag;
            if 0 <= j && j < nt {
                let d = b[(2 * i) as usize] - t[(2 * j) as usize];
                s += d * d;
            }
            i += 8;
        }
        if best.is_none_or(|(_, m)| s < m) {
            best = Some((lag, s));
        }
    }
    let report = Same {
        bounce_s: nb as f64 / rb as f64,
        tap_s: nt as f64 / rt as f64,
        compared_s: (end - start).max(0) as f64 / rb as f64,
        origin,
        anchors: side.anchors.len(),
        max_abs_diff: worst,
        max_abs_diff_db: (worst > 0.0).then(|| 20.0 * worst.log10()),
        at_s: (worst_at >= 0).then(|| worst_at as f64 / rb as f64),
        samples_over: over,
        best_lag_samples: best.map(|(l, _)| l),
        live: side.report,
    };
    let json = serde_json::to_string(&report).map_err(|e| e.to_string())?;
    println!("{json}");
    if let Some(o) = out {
        std::fs::write(o, &json).map_err(|e| format!("{o}: {e}"))?;
    }
    Ok(())
}

/// A file's samples, both channels at 48 kHz, decoded by ffmpeg
fn decode(path: &str) -> Result<(Vec<f64>, Vec<f64>), String> {
    let o = Command::new("ffmpeg")
        .args(["-v", "error", "-i", path, "-f", "f32le", "-ac", "2", "-ar", &RATE.to_string(), "-"])
        .output()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    if !o.status.success() {
        return Err(format!("ffmpeg couldn't decode {path}: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    let s: Vec<f64> = o.stdout.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect();
    Ok((s.iter().step_by(2).copied().collect(), s.iter().skip(1).step_by(2).copied().collect()))
}

/// The times (s) of one channel's clicks, one a 5 ms at most
fn click_times(x: &[f64]) -> Vec<f64> {
    const RATIO: f64 = 40.0;
    const FLOOR: f64 = 0.01; // -40 dBFS
    let d2: Vec<f64> = (2..x.len()).map(|i| (x[i] - 2.0 * x[i - 1] + x[i - 2]).abs()).collect();
    let w = RATE / 100;
    let mut out = Vec::new();
    let mut block = Vec::with_capacity(3 * w);
    for start in (0..d2.len()).step_by(w) {
        block.clear();
        block.extend_from_slice(&d2[start.saturating_sub(w)..(start + 2 * w).min(d2.len())]);
        let mid = block.len() / 2;
        let med = *block.select_nth_unstable_by(mid, |a, b| a.total_cmp(b)).1 + 1e-9;
        for (i, &v) in d2.iter().enumerate().take((start + w).min(d2.len())).skip(start) {
            if v > FLOOR && v > RATIO * med {
                out.push((i + 2) as f64 / RATE as f64);
            }
        }
    }
    let mut merged: Vec<f64> = Vec::new();
    for t in out {
        if merged.last().is_none_or(|&m| t - m > 0.005) {
            merged.push(t);
        }
    }
    merged
}

/// Python's repr of a float: the shortest text that reads back the same, with ".0" on a whole number
fn py_float(v: f64) -> String {
    let s = format!("{v}");
    if s.contains(['.', 'e', 'i', 'N']) { s } else { format!("{s}.0") }
}

/// The last match of `prefix` NUMBER `suffix` in `text` (ffmpeg's ebur128 summary is the last one it prints)
fn last_number<'a>(text: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let mut found = None;
    let mut rest = text;
    while let Some(p) = rest.find(prefix) {
        let after = rest[p + prefix.len()..].trim_start();
        let len = after.char_indices().take_while(|&(i, c)| c.is_ascii_digit() || c == '.' || (c == '-' && i == 0)).count();
        if len > 0 && after[len..].starts_with(suffix) {
            found = Some(&after[..len]);
        }
        rest = &rest[p + prefix.len()..];
    }
    found
}

fn clicks(files: &[String]) -> Result<(), String> {
    if files.is_empty() {
        return Err("usage: raea-motion ci clicks FILE...".into());
    }
    for path in files {
        let meter = Command::new("ffmpeg")
            .args(["-nostats", "-i", path, "-af", "ebur128=peak=true", "-f", "null", "-"])
            .output()
            .map_err(|e| format!("ffmpeg: {e}"))?;
        let meter = String::from_utf8_lossy(&meter.stderr);
        let i = last_number(&meter, "I:", " LUFS").ok_or(format!("{path}: ffmpeg's ebur128 gave no integrated loudness"))?;
        let tp = last_number(&meter, "Peak:", " dBFS").ok_or(format!("{path}: ffmpeg's ebur128 gave no true peak"))?;
        let (l, r) = decode(path)?;
        // each time to the millisecond (Python's round(t, 3): half to even on the exact value), both channels as one set
        let mut times: Vec<f64> = [click_times(&l), click_times(&r)].concat().into_iter().map(|t| format!("{t:.3}").parse().unwrap()).collect();
        times.sort_by(f64::total_cmp);
        times.dedup();
        let at = if times.is_empty() {
            String::new()
        } else {
            format!(" at [{}] s", times.iter().take(8).map(|&t| py_float(t)).collect::<Vec<_>>().join(", "))
        };
        println!("{path}: {i} LUFS, true peak {tp} dBTP, clicks {}{at}", times.len());
    }
    Ok(())
}

fn duck(args: &[String]) -> Result<(), String> {
    let usage = "usage: raea-motion ci duck DUCK.wav NODUCK.wav NOVOICE.wav FROM-TO...";
    let [a, c, d, windows @ ..] = args else { return Err(usage.into()) };
    if windows.is_empty() {
        return Err(usage.into());
    }
    let (a, c, d) = (wav(a)?.3, wav(c)?.3, wav(d)?.3);
    // stereo samples: [i0, i1) of the interleaved floats
    let span = |t0: f64, t1: f64| ((t0 * RATE as f64) as usize * 2, (t1 * RATE as f64) as usize * 2);
    for w in windows {
        let (t0, t1) = w
            .split_once('-')
            .and_then(|(x, y)| Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?)))
            .ok_or(format!("{w}: not a window FROM-TO in seconds"))?;
        let (i0, i1) = span(t0, t1);
        if i1 > a.len().min(c.len()).min(d.len()) {
            return Err(format!("{w}: past the end of the shortest file"));
        }
        let n = (i1 - i0).max(1) as f64;
        let mut music = 0.0;
        for k in i0..i1 {
            let m = a[k] - c[k] + d[k];
            music += m * m;
        }
        let mut alone = 0.0;
        for v in &d[i0..i1] {
            alone += v * v;
        }
        println!("{}", 10.0 * ((music / n) / (alone / n)).log10());
    }
    Ok(())
}
