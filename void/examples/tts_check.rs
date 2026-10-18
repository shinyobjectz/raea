//! Pocket TTS on ort, checked: speaks one line in alba and in the founder's clip voice (watts), writes it as a 24 kHz WAV, prints first-audio latency and the
//! real-time factor.
//!
//!   cargo run --release --example tts_check [-- OUT_PREFIX ["text"] [voice]]   (default voices: alba, then the clip watts)

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use raea_void::voice::{models, Pocket};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().cloned().unwrap_or_else(|| "tts_check".into());
    let text = args.get(1).cloned().unwrap_or_else(|| "Hello from the void. Everything here is made by talking.".into());
    let voices: Vec<String> = match args.get(2) {
        Some(v) => vec![v.clone()],
        None => vec!["alba".into(), "watts".into()],
    };

    let t = Instant::now();
    let dir = models::ensure(&Pocket::SPEC, &mut |f, n| {
        if n % (8 << 20) < (1 << 16) {
            eprintln!("downloading {f}: {} MB", n >> 20);
        }
    })?;
    let mut pocket = Pocket::load(&dir)?;
    eprintln!("loaded in {:.0} ms; voices: {}", t.elapsed().as_secs_f64() * 1e3, Pocket::voices().join(", "));

    for voice in &voices {
        // a warm-up line (first-run allocations, the voice's state; a clip's encoding), as the app does at launch
        let t = Instant::now();
        pocket.speak("Okay.", voice, &AtomicBool::new(false), &mut |_| {})?;
        eprintln!("{voice}: voice + warm-up in {:.0} ms", t.elapsed().as_secs_f64() * 1e3);

        let mut samples: Vec<f32> = Vec::new();
        let mut first = None;
        let t = Instant::now();
        pocket.speak(&text, voice, &AtomicBool::new(false), &mut |chunk| {
            first.get_or_insert(t.elapsed());
            samples.extend_from_slice(chunk);
        })?;
        let took = t.elapsed().as_secs_f64();
        let length = samples.len() as f64 / pocket.sample_rate() as f64;
        let path = format!("{out}-{voice}.wav");
        write_wav(&path, &samples, pocket.sample_rate())?;
        println!(
            "{voice}: first audio {:.0} ms, {:.2} s of audio in {:.0} ms, RTF {:.3}, wrote {path}",
            first.map(|d| d.as_secs_f64() * 1e3).unwrap_or(f64::NAN),
            length,
            took * 1e3,
            took / length.max(1e-3)
        );
    }
    Ok(())
}

/// 16-bit PCM mono WAV.
fn write_wav(path: &str, samples: &[f32], rate: u32) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b)
}
