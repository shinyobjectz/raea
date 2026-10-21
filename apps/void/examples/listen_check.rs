//! The listening models on a recorded turn: Silero's speech segments, Smart Turn at each pause (as voice mode would ask
//! it), at the clip's end and at any --cuts, and Parakeet's words, with timings.
//!
//!   cargo run --release --example listen_check -- TURN.wav [--cuts 1.6,2.4]
//!
//! TURN.wav: 16 kHz mono, 16-bit PCM or float. A TURN.json beside it ({"text", "speech_end"}, as in
//! ~/host/tests/voice/fixtures) adds the expected words and a judgement 0.2 s after the speech ends.

use std::path::Path;
use std::time::Instant;

use raea_void::voice::{models, vad::CHUNK, Parakeet, SmartTurn, Vad, LISTEN_RATE};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wav = args.first().ok_or_else(|| anyhow::anyhow!("usage: listen_check TURN.wav [--cuts S,S]"))?;
    let cuts: Vec<f64> = args
        .iter()
        .position(|a| a == "--cuts")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_default();
    let audio = read_wav(Path::new(wav))?;
    let secs = |n: usize| n as f64 / LISTEN_RATE as f64;
    println!("{wav}: {:.2} s", secs(audio.len()));
    let fixture: Option<serde_json::Value> =
        std::fs::read_to_string(Path::new(wav).with_extension("json")).ok().and_then(|s| serde_json::from_str(&s).ok());

    let quiet = &mut |_: &str, _: u64| {};
    let t = Instant::now();
    let mut vad = Vad::load(&models::ensure(&Vad::SPEC, quiet)?)?;
    let mut turn = SmartTurn::load(&models::ensure(&SmartTurn::SPEC, quiet)?)?;
    println!("load vad+turn {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    let mut stt = Parakeet::load(&models::ensure(&Parakeet::SPEC, quiet)?)?;
    println!("load parakeet {:.0} ms", t.elapsed().as_secs_f64() * 1e3);

    // warm-up (first runs pay for ORT's setup)
    vad.prob(&[0.0; CHUNK])?;
    vad.reset();
    turn.complete(&vec![0.0; LISTEN_RATE as usize])?;

    // Silero over the clip with VoiceTurns' rules: start at 0.5 for 3 chunks, a pause is 6 chunks below 0.35
    let (mut speaking, mut speech_run, mut silence_run) = (false, 0, 0);
    let mut seg_start = 0usize;
    let mut vad_ms = Vec::new();
    let mut turn_ms = Vec::new();
    for (i, chunk) in audio.chunks_exact(CHUNK).enumerate() {
        let t = Instant::now();
        let p = vad.prob(chunk)?;
        vad_ms.push(t.elapsed().as_secs_f64() * 1e3);
        let end = (i + 1) * CHUNK;
        if p >= 0.5 {
            speech_run += 1;
            silence_run = 0;
            if speech_run >= 3 && !speaking {
                speaking = true;
                seg_start = end - 3 * CHUNK;
            }
        } else if p < 0.35 {
            speech_run = 0;
            silence_run += 1;
            if speaking && silence_run >= 6 {
                speaking = false;
                let speech_end = end - 6 * CHUNK;
                let t = Instant::now();
                let c = turn.complete(&audio[..end])?;
                turn_ms.push(t.elapsed().as_secs_f64() * 1e3);
                println!(
                    "speech {:.2}-{:.2} s; pause at {:.2} s: Smart Turn {c:.3} ({:.0} ms)",
                    secs(seg_start), secs(speech_end), secs(end), turn_ms.last().unwrap()
                );
            }
        }
    }
    if speaking {
        println!("speech {:.2} s - end of clip", secs(seg_start));
    }
    vad_ms.sort_by(f64::total_cmp);
    println!(
        "vad: {} chunks, p50 {:.3} ms, p90 {:.3} ms per 32 ms chunk",
        vad_ms.len(), vad_ms[vad_ms.len() / 2], vad_ms[vad_ms.len() * 9 / 10]
    );

    let mut judge = |label: String, at: f64| -> anyhow::Result<()> {
        let n = ((at * LISTEN_RATE as f64) as usize).min(audio.len());
        let t = Instant::now();
        let c = turn.complete(&audio[..n])?;
        println!("smart turn {label} ({at:.2} s): {c:.4} ({:.0} ms)", t.elapsed().as_secs_f64() * 1e3);
        Ok(())
    };
    judge("at the clip's end".into(), secs(audio.len()))?;
    if let Some(end) = fixture.as_ref().and_then(|f| f["speech_end"].as_f64()) {
        judge("0.2 s after the speech ends".into(), end + 0.2)?;
    }
    for c in &cuts {
        judge("at a cut".into(), *c)?;
    }

    // LISTEN_DUMP=SECS:FILE writes Smart Turn's input at SECS as raw f32 [80, 800] (a parity check against transformers)
    if let Some((at, file)) = std::env::var("LISTEN_DUMP").ok().as_deref().and_then(|s| s.split_once(':')) {
        let n = ((at.parse::<f64>()? * LISTEN_RATE as f64) as usize).min(audio.len());
        let tail = &audio[n.saturating_sub(8 * LISTEN_RATE as usize)..n];
        let feats = raea_void::voice::mel::whisper(&raea_void::voice::mel::normalised_padded(tail, 8 * LISTEN_RATE as usize));
        std::fs::write(file, feats.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>())?;
    }

    let t = Instant::now();
    let text = stt.transcribe(&audio)?;
    let took = t.elapsed().as_secs_f64();
    println!("parakeet: {text:?} in {:.0} ms (RTF {:.3})", took * 1e3, took / secs(audio.len()));
    let t = Instant::now();
    stt.transcribe(&audio)?;
    let took = t.elapsed().as_secs_f64();
    println!("parakeet again: {:.0} ms (RTF {:.3})", took * 1e3, took / secs(audio.len()));
    if let Some(want) = fixture.as_ref().and_then(|f| f["text"].as_str()) {
        println!("expected: {want:?}");
    }
    Ok(())
}

/// A RIFF WAVE file's first channel as floats: 16-bit PCM (format 1) or 32-bit float (format 3), 16 kHz.
fn read_wav(path: &Path) -> anyhow::Result<Vec<f32>> {
    let b = std::fs::read(path)?;
    anyhow::ensure!(b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WAVE", "{}: not a WAV file", path.display());
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let (mut fmt, mut channels, mut rate, mut bits) = (0, 1, 0, 0);
    let mut i = 12;
    while i + 8 <= b.len() {
        let (id, size) = (&b[i..i + 4], u32_at(i + 4) as usize);
        let body = i + 8;
        if id == b"fmt " {
            fmt = u16_at(body);
            channels = u16_at(body + 2) as usize;
            rate = u32_at(body + 4);
            bits = u16_at(body + 14);
            if fmt == 0xFFFE {
                fmt = u16_at(body + 24); // WAVE_FORMAT_EXTENSIBLE: the subformat's first two bytes
            }
        } else if id == b"data" {
            anyhow::ensure!(rate == LISTEN_RATE, "{}: {rate} Hz, want {LISTEN_RATE}", path.display());
            let data = &b[body..(body + size).min(b.len())];
            return match (fmt, bits) {
                (1, 16) => Ok(data.chunks_exact(2 * channels).map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0).collect()),
                (3, 32) => Ok(data.chunks_exact(4 * channels).map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]])).collect()),
                _ => anyhow::bail!("{}: format {fmt}, {bits} bits not read", path.display()),
            };
        }
        i = body + size + (size & 1);
    }
    anyhow::bail!("{}: no data chunk", path.display())
}
