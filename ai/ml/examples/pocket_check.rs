//! Pocket TTS on raea-ml, checked: one line said twice with the same seed (are the samples identical?), first-audio
//! latency and real-time factor, and the word clock's starts for it. Writes OUT.wav.
//!
//!   cargo run --release --example pocket_check [-- OUT ["text"] [voice] [threads]]

use std::time::Instant;

use raea_ml::pocket::{Out, Pocket};
use raea_ml::words::Clock;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().cloned().unwrap_or_else(|| "pocket_check".into());
    let text = args.get(1).cloned().unwrap_or_else(|| "Every journey begins with the strange business of standing up. Nobody tells you how. You simply find that you are doing it.".into());
    let voice = args.get(2).cloned().unwrap_or_else(Pocket::default_voice);
    let t = Instant::now();
    let dir = raea_ml::models::ensure(&Pocket::SPEC, &mut |_, _| {})?;
    let mut pocket = Pocket::load(&dir)?;
    eprintln!("loaded in {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    pocket.speak("Okay.", &voice, &std::sync::atomic::AtomicBool::new(false), &mut |_| {})?;
    eprintln!("{voice}: voice + warm-up in {:.0} ms", t.elapsed().as_secs_f64() * 1e3);

    let mut runs: Vec<Vec<f32>> = Vec::new();
    for run in 0..2 {
        let t = Instant::now();
        let mut g = pocket.begin(&text, &voice, 42, 0.7)?;
        let mut audio: Vec<f32> = Vec::new();
        let mut first = None;
        let mut clock = Clock::new(pocket.sample_rate());
        let mut sentence_first = Vec::new();
        while let Some(o) = pocket.step(&mut g)? {
            match o {
                Out::Begin(s) => {
                    sentence_first.push((s.clone(), t.elapsed()));
                    clock.begin_sentence(&s, audio.len());
                }
                Out::Audio(a) => {
                    first.get_or_insert(t.elapsed());
                    audio.extend_from_slice(&a);
                    clock.advance(&audio, false);
                }
                Out::End => clock.end_sentence(audio.len()),
            }
        }
        clock.advance(&audio, true);
        let took = t.elapsed().as_secs_f64();
        let len = audio.len() as f64 / pocket.sample_rate() as f64;
        println!("run {run}: first audio {:.0} ms, {len:.2} s of audio in {:.0} ms, RTF {:.3}", first.unwrap().as_secs_f64() * 1e3, took * 1e3, took / len);
        if run == 0 {
            for (w, s) in clock.words.iter().zip(&clock.starts) {
                print!("{w}@{s:.2} ");
            }
            println!();
            write_wav(&format!("{out}.wav"), &audio, pocket.sample_rate())?;
        }
        runs.push(audio);
    }
    let same = runs[0] == runs[1];
    let diff = runs[0].iter().zip(&runs[1]).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
    println!("same seed, same samples: {same} (lengths {} {}, max diff {diff:e})", runs[0].len(), runs[1].len());
    Ok(())
}

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
