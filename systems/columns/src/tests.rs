//! FP1a's parity bar: the Rust columns against the 11-12 spike's references (~/films/fingerprint-spike, or
//! $FP_SPIKE), at cosine 0.999 or better. The references are on Shane's machine, not in git (client footage), so
//! these tests are ignored by default: `cargo test -p raea-sys-columns -- --ignored`.

use super::*;
use std::path::PathBuf;

fn spike() -> PathBuf {
    std::env::var_os("FP_SPIKE").map(PathBuf::from).unwrap_or_else(|| raea_io_home::user_home().unwrap().join("films/fingerprint-spike"))
}

fn cos(a: &[f32], b: &[f32]) -> f32 {
    let d: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    d / (a.iter().map(|x| x * x).sum::<f32>().sqrt() * b.iter().map(|x| x * x).sum::<f32>().sqrt()).max(1e-9)
}

fn f32s(p: &std::path::Path) -> Vec<f32> {
    std::fs::read(p).unwrap().as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect()
}

fn reference(dtype: &str) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(spike().join(format!("clap/vectors-{dtype}.jsonl"))).unwrap();
    text.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect()
}

/// The spike's CLAP vectors to compare with: RAEA_CLAP_DTYPE's (fp32 unless q8)
fn dtype() -> String {
    std::env::var("RAEA_CLAP_DTYPE").unwrap_or_else(|_| "fp32".into())
}

fn vec_of(v: &serde_json::Value) -> Vec<f32> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect()
}

#[test]
#[ignore]
fn clap_audio_matches_the_spike() {
    let mut e = Encoders::load().unwrap();
    let items: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(spike().join("clap/items.json")).unwrap()).unwrap();
    let refs = reference(&dtype());
    let by: std::collections::HashMap<String, Vec<f32>> = refs.iter().skip(1).map(|r| (r["id"].as_str().unwrap().to_string(), vec_of(&r["vec"]))).collect();
    let mut worst = (1.0f32, String::new());
    let mut sum = 0.0;
    // every 5th clip (all five conditions of each window keyed in turn): the dev build's FFT is slow
    for (n, it) in items.iter().enumerate().filter(|(i, _)| (i / 5) % 5 == 0) {
        if n % 100 == 0 {
            eprintln!("clip {n} of {}", items.len());
        }
        let id = format!("{}/{}", it["key"].as_str().unwrap(), it["cond"].as_str().unwrap());
        let pcm = f32s(std::path::Path::new(it["pcm"].as_str().unwrap()));
        let v = e.clap_clips(&[&pcm]).unwrap();
        let c = cos(&v, &by[&id]);
        sum += c;
        if c < worst.0 {
            worst = (c, id);
        }
    }
    let seen = items.len().div_ceil(25) * 5;
    eprintln!("CLAP audio, Rust against transformers.js ({}): {seen} clips, mean cosine {:.5}, worst {:.5} ({})", dtype(), sum / seen as f32, worst.0, worst.1);
    assert!(worst.0 >= 0.999);
}

#[test]
#[ignore]
fn clap_text_matches_the_spike() {
    let mut e = Encoders::load().unwrap();
    let t = &reference(&dtype())[0];
    let names: Vec<String> = t["names"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_string()).collect();
    let texts: Vec<String> = names
        .iter()
        .map(|n| n.split(':').collect::<Vec<_>>())
        .map(|p| p.last().unwrap().to_string())
        .collect();
    // the spike's prompts: its axis words are in its script, so only the vocabulary's words are rebuilt here
    let vocab: Vec<(usize, String)> = names.iter().enumerate().filter(|(_, n)| n.starts_with("vocab:")).map(|(i, _)| (i, format!("This is a sound of {}.", texts[i]))).collect();
    // transformers.js embedded the spike's 46 prompts as one batch padded to its longest (the export takes no mask, so
    // the pads reach the tower): the same here shows the tower and tokenizer match; raea embeds each text unpadded
    let axis_prompts = ["hiss and static noise over everything", "a noisy recording full of white noise", "the sound suddenly cutting out to silence", "audio dropping out for a moment", "harsh clipped distorted audio", "loud crackling digital distortion"];
    let tok = tokenizers::Tokenizer::from_file(models::locate().unwrap().clap_tokenizer).unwrap();
    let longest = axis_prompts.iter().map(|w| format!("This is a sound of {w}.")).chain(vocab.iter().map(|(_, s)| s.clone())).map(|s| tok.encode(s, true).unwrap().get_ids().len()).max().unwrap();
    let v = e.words_sound_padded(&vocab.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>(), longest).unwrap();
    let worst = vocab.iter().enumerate().map(|(k, (i, _))| cos(&v[k * CLAP..(k + 1) * CLAP], &vec_of(&t["vecs"][*i]))).fold(1.0f32, f32::min);
    eprintln!("CLAP text, Rust against transformers.js ({}): {} descriptions, worst cosine {worst:.5}", dtype(), vocab.len());
    assert!(worst >= 0.999);
}

#[test]
#[ignore]
fn siglip_matches_onnx_fp32() {
    let mut e = Encoders::load().unwrap();
    let raw = std::fs::read(spike().join("ref/siglip_frames.u8")).unwrap();
    let frames: Vec<media::Rgb> = raw.chunks(media::SIDE * media::SIDE * 3).map(|c| c.to_vec()).collect();
    let mut want = f32s(&spike().join("ref/siglip_pooled.f32"));
    normalise(&mut want, SIG);
    let got = e.vision.embed(&frames).unwrap();
    let worst = (0..frames.len()).map(|i| cos(&got[i * SIG..(i + 1) * SIG], &want[i * SIG..(i + 1) * SIG])).fold(1.0f32, f32::min);
    eprintln!("SigLIP 2, Rust ort against Python onnxruntime (fp32, the same pixels): {} frames, worst cosine {worst:.6}", frames.len());
    assert!(worst >= 0.999);
}

#[test]
fn clap_bank_is_slaney() {
    let b = clap::bank();
    // every band has weight, and the bank is normalised so each band's area is about the same
    for m in 0..64 {
        assert!(b[m * 513..(m + 1) * 513].iter().any(|x| *x > 0.0), "band {m} empty");
    }
    let p = clap::padded(&[1.0; 96_000]);
    assert_eq!((p[383_999], p[384_000]), (1.0, 0.0), "four repeats of 2 s, then zeros (repeatpad)");
}

/// FP1b on the real models: a script's passages get EmbeddingGemma 2 rows (each nearest the query its words answer, and
/// the same vectors transformers.js gives for the first), a still gets one SigLIP 2 frame
#[test]
#[ignore]
fn a_script_and_a_still_get_rows() {
    let mut e = Encoders::load().unwrap();
    let dir = std::env::temp_dir().join(format!("raea-pieces-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("ad.md");
    std::fs::write(&script, "INT. KITCHEN - MORNING\n\nShe opens the fridge and it's empty.\n\nBuy Sunmelt today: 20% off your first box.\n").unwrap();
    let c = e.columns(&script).unwrap();
    assert_eq!((c.text.len() / crate::TEXT, c.dur, c.sig.len()), (3, 3.0, 0));
    let q = e.words_text(&["a call to action".into(), "a scene heading".into()]).unwrap();
    let dot = |r: usize, k: usize| c.text[r * crate::TEXT..(r + 1) * crate::TEXT].iter().zip(&q[k * crate::TEXT..(k + 1) * crate::TEXT]).map(|(a, b)| a * b).sum::<f32>();
    assert!(dot(2, 0) > dot(0, 0) && dot(0, 1) > dot(2, 1), "cta {} {} heading {} {}", dot(2, 0), dot(0, 0), dot(0, 1), dot(2, 1));
    let still = dir.join("board.png");
    image::RgbImage::from_fn(64, 48, |x, _| image::Rgb([x as u8 * 4, 90, 200])).save(&still).unwrap();
    let s = e.columns(&still).unwrap();
    assert_eq!((s.sig.len(), s.dur, s.text.len()), (crate::SIG, 1.0, 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_with_no_sound_track_has_no_sound_to_judge() {
    use crate::window::Modality;
    // what columns() gives a render with no audio stream: one empty clip's CLAP vector, and no levels
    let silent = Columns { sig: vec![0.0; SIG], clap: vec![0.0; CLAP], ..Default::default() };
    assert!(Modality::Picture.in_file(&silent) && !Modality::Sound.in_file(&silent) && !Modality::Events.in_file(&silent));
    let heard = Columns { level: vec![-30.0], ..silent };
    assert!(Modality::Sound.in_file(&heard));
}
