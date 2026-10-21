//! What's heard, in spans (Perception.swift › sound): speech, music, singing, laughter, applause, silence or noise.
//! The Swift asks Apple's built-in SoundAnalysis classifier; raea uses YAMNet (Apache-2.0, AudioSet's 521 classes,
//! the Core ML package the apps already carry), so the same read runs off Apple too. Patches of 0.96 s every 0.48 s,
//! each labelled by its top class, mapped to the seven labels; neighbours merge across gaps up to 0.6 s; spans under
//! a second are dropped, as the Swift does.

use std::path::Path;

use crate::coreml::{Input, Model, Units};

#[derive(Clone, Debug, serde::Serialize, PartialEq)]
pub struct Span {
    pub label: String,
    pub start: f64,
    pub end: f64,
}

pub fn model() -> Result<Model, String> {
    // small and fp32: the CPU keeps it exact (the log-mel front end is inside)
    Model::load("yamnet", "YAMNet", Units::CpuOnly)
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// An AudioSet class name as one of the seven labels.
pub fn label(name: &str) -> &'static str {
    let n = name.to_lowercase();
    let speech = ["speech", "conversation", "narration, monologue", "whispering", "shout", "yell", "babbling", "child speech, kid speaking",
        "male speech, man speaking", "female speech, woman speaking", "speech synthesizer"];
    let instruments = ["piano", "guitar", "drum", "drum kit", "synthesizer", "orchestra", "bass guitar", "keyboard (musical)", "musical instrument",
        "electric guitar", "acoustic guitar", "electronic organ", "violin, fiddle", "drum machine", "beat"];
    if speech.contains(&n.as_str()) {
        "speech"
    } else if n == "singing" || n == "choir" || n.contains("song") {
        "singing"
    } else if n.contains("music") || instruments.contains(&n.as_str()) || n == "techno" {
        "music"
    } else if n.contains("laugh") || n == "giggle" || n == "chuckle, chortle" {
        "laughter"
    } else if n == "applause" || n == "clapping" {
        "applause"
    } else if n == "silence" {
        "silence"
    } else {
        "noise"
    }
}

/// The clip's sound as 16 kHz mono floats; empty without a sound track.
pub fn pcm16k(path: &Path, backend: &str) -> Vec<f32> {
    let Ok(a) = raea_media::backend(backend).and_then(|b| b.decode_audio(path)) else { return Vec::new() };
    if a.channels.is_empty() {
        return Vec::new();
    }
    let n = a.len();
    let mono: Vec<f32> = (0..n).map(|i| a.channels.iter().map(|c| c[i]).sum::<f32>() / a.channels.len() as f32).collect();
    let m = raea_media::soft_resample(&raea_media::Audio { rate: a.rate, channels: vec![mono] }, 16_000);
    m.channels.into_iter().next().unwrap_or_default()
}

/// Each patch's top class name and its start time.
pub fn timeline(m: &Model, pcm: &[f32]) -> Result<Vec<(f64, String)>, String> {
    let classes: Vec<String> = m.user_metadata("classes").and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    let (window, hop, length) = (160_000usize, 7_680usize, 15_360usize);
    let mut out = Vec::new();
    let mut start = 0;
    while start < pcm.len() {
        let count = window.min(pcm.len() - start);
        let mut chunk = vec![0f32; window];
        chunk[..count].copy_from_slice(&pcm[start..start + count]);
        let o = m.predict_inputs(&[("waveform", Input::Array { shape: vec![window], data: chunk })])?;
        let (scores, _) = o.floats("scores")?;
        let patches = scores.len() / 521;
        // a patch counts while most of it is real sound
        let real = patches.min((count.saturating_sub(length / 2)) / hop + 1).max(1);
        for p in 0..real {
            let s = &scores[p * 521..(p + 1) * 521];
            let best = (0..521).fold(0, |b, i| if s[i] > s[b] { i } else { b });
            let t = (start + p * hop) as f64 / 16_000.0;
            out.push((t, classes.get(best).cloned().unwrap_or_else(|| format!("class {best}"))));
        }
        start += window;
    }
    Ok(out)
}

/// What's heard, in spans.
pub fn spans(m: &Model, path: &Path, backend: &str) -> Result<Vec<Span>, String> {
    let pcm = pcm16k(path, backend);
    if pcm.len() < 8_000 {
        return Ok(Vec::new());
    }
    let mut out: Vec<Span> = Vec::new();
    for (t, name) in timeline(m, &pcm)? {
        let (a, b) = (t, t + 0.96);
        let l = label(&name);
        match out.last_mut() {
            Some(last) if last.label == l && a - last.end <= 0.6 => last.end = r2(b),
            _ => out.push(Span { label: l.into(), start: r2(a), end: r2(b) }),
        }
    }
    Ok(out.into_iter().filter(|s| s.end - s.start >= 1.0).collect())
}
