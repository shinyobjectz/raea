//! The readers a reference is read by (docs/videoform.feature VF3), each what raea already runs, on the files a pass
//! holds: footage perception (raea-perceive analyze: shots, framing, faces, the camera, brightness and look, scene
//! labels, words on screen), Parakeet's words (parakeet-mlx, as `raea cut` hears a take), Beat This!'s beats
//! (raea-perceive beats), loudness, and Fingerprint's columns (SigLIP 2, CLAP, YAMNet, levels), cached by content hash.
//! Each shot's layers (raea-perceive layers, VF8e). Not read yet: stems (HTDemucs) and voice activity (Silero) aren't shipped in a release, nor depth; the words'
//! gaps give the pauses, and perception's framing the shot scale.

use raea_sys_columns::{Columns, Encoders, FPS, cache, media};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A word heard, in the reference's seconds
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// A raea verb's JSON, run as its own process
pub fn raea(args: &[&str]) -> Result<Value, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    json_of(Command::new(exe).args(args), &format!("raea {}", args.join(" ")))
}

fn json_of(cmd: &mut Command, what: &str) -> Result<Value, String> {
    let out = cmd.output().map_err(|e| format!("{what}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{what}: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim()));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("{what}: {e}"))
}

/// The content hash of what's read: the picture file's sha256, or of both streams' when the sound is its own
pub fn hash(picture: &Path, sound: Option<&Path>) -> Result<String, String> {
    let p = media::sha256(picture).map_err(|e| e.to_string())?;
    match sound {
        None => Ok(p),
        Some(s) => {
            use sha2::{Digest, Sha256};
            let s = media::sha256(s).map_err(|e| e.to_string())?;
            Ok(Sha256::digest(format!("{p}+{s}")).iter().map(|b| format!("{b:02x}")).collect())
        }
    }
}

/// Footage perception's facts on the shots given: each one's framing, faces, camera, brightness, look and scene
/// labels, and the words on screen with their boxes. `dir` gets its stills, and goes with the pass's folder.
pub fn perception(picture: &Path, dir: &Path, shots: &[[f64; 2]]) -> Result<Value, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let given: Vec<Value> = shots.iter().enumerate().map(|(i, s)| serde_json::json!({"start": s[0], "end": s[1], "transition": if i == 0 { "start" } else { "cut" }})).collect();
    let file = dir.join("shots.json");
    std::fs::write(&file, Value::from(given).to_string()).map_err(|e| e.to_string())?;
    let bin = raea_io_assets::perceive_bin();
    json_of(Command::new(&bin).arg("analyze").arg(picture).arg(dir).arg("--shots").arg(&file), "raea-perceive analyze")
}

/// Each shot's layers (VF8e: a person cut out, a picture inset, a card, a backdrop, a split, a screen), by
/// raea-perceive layers on the shots given; `dir` holds the shots' file
pub fn layers(picture: &Path, dir: &Path, shots: &[[f64; 2]]) -> Result<Vec<Vec<raea_sys_videoform::rows::Layer>>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join("layer-shots.json");
    let given: Vec<Value> = shots.iter().map(|s| serde_json::json!({"start": s[0], "end": s[1]})).collect();
    std::fs::write(&file, Value::from(given).to_string()).map_err(|e| e.to_string())?;
    let v = json_of(Command::new(raea_io_assets::perceive_bin()).arg("layers").arg(picture).arg(&file), "raea-perceive layers")?;
    let out: Vec<Vec<raea_sys_videoform::rows::Layer>> = serde_json::from_value(v).map_err(|e| format!("raea-perceive layers: {e}"))?;
    if out.len() != shots.len() {
        return Err(format!("raea-perceive layers: {} shots read of {}", out.len(), shots.len()));
    }
    Ok(out)
}

/// The shots, cut to cut, from every frame's change (raea media cuts)
pub fn shots(picture: &Path, dur: f64) -> Result<Vec<[f64; 2]>, String> {
    let cuts = raea(&["media", "cuts", &picture.to_string_lossy()])?;
    let at: Vec<f64> = cuts["cuts"].as_array().map(|a| a.iter().filter_map(|c| c["at"].as_f64()).collect()).unwrap_or_default();
    Ok(crate::pass::shots_from_cuts(&at, dur))
}

/// Beat This!'s model where raea-perceive reads it, copied from the install's share once (as `raea song` does)
fn beat_model() -> Result<(), String> {
    let model = raea_io_home::models().join("beat-this/beat_this_small0.onnx");
    let shipped = raea_io_home::home().join("models/beat-this/beat_this_small0.onnx");
    if !model.is_file() {
        let missing = |_| format!("beats: Beat This! isn't at {} (a release ships it)", model.display());
        std::fs::create_dir_all(model.parent().unwrap()).map_err(missing)?;
        std::fs::copy(&shipped, &model).map_err(missing)?;
    }
    Ok(())
}

/// A Parakeet token from its JSON
fn token(t: &Value) -> Word {
    let at = |k: &str| t[k].as_f64().unwrap_or(0.0);
    Word { text: t["text"].as_str().unwrap_or("").to_string(), start: at("start"), end: at("end") }
}

/// The beats in a file's sound, seconds
pub fn beats(sound: &Path) -> Result<Vec<f64>, String> {
    beat_model()?;
    let v = json_of(Command::new(raea_io_assets::perceive_bin()).arg("beats").arg(sound), "raea-perceive beats")?;
    Ok(v["beats"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default())
}

/// parakeet-mlx, found or installed once by raea's tools/ensure.sh (as `raea cut` finds it)
fn parakeet() -> Result<PathBuf, String> {
    let script = raea_io_home::home().join("tools/ensure.sh");
    let out = Command::new("bash").arg(&script).arg("parakeet-mlx").output().map_err(|e| format!("{}: {e}", script.display()))?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !p.is_empty() { Ok(PathBuf::from(p)) } else { Err("words: parakeet-mlx couldn't be found or installed (tools/ensure.sh parakeet-mlx)".into()) }
}

/// Parakeet's tokens as words: a token starting with a space starts one, and so does a digit after a letter (heard.ts)
pub fn words_of(tokens: &[Word]) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    for t in tokens {
        let digit_after_letter = t.text.starts_with(|c: char| c.is_ascii_digit()) && out.last().is_some_and(|l| l.text.ends_with(|c: char| c.is_alphabetic()));
        match out.last_mut() {
            Some(last) if !t.text.starts_with(' ') && !digit_after_letter => {
                last.text.push_str(&t.text);
                last.end = t.end;
            }
            _ if !t.text.trim().is_empty() => out.push(Word { text: t.text.trim().to_string(), start: t.start, end: t.end }),
            _ => {}
        }
    }
    out
}

/// The words said, by Parakeet on Apple silicon; none where it can't run (Parakeet is MLX's)
pub fn words(sound: &Path, dir: &Path) -> Result<Vec<Word>, String> {
    if !cfg!(target_vendor = "apple") {
        return Ok(Vec::new());
    }
    let wav = dir.join("heard.wav");
    raea(&["media", "audio", &sound.to_string_lossy(), "--rate", "16000", "--mono", "-o", &wav.to_string_lossy()]).or_else(|e| if wav.is_file() { Ok(Value::Null) } else { Err(e) })?;
    let out = Command::new(parakeet()?).arg(&wav).args(["--output-format", "json", "--output-dir"]).arg(dir).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("words: parakeet-mlx: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("")));
    }
    let heard: Value = serde_json::from_slice(&std::fs::read(dir.join("heard.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut tokens = Vec::new();
    for sentence in heard["sentences"].as_array().map(Vec::as_slice).unwrap_or_default() {
        tokens.extend(sentence["tokens"].as_array().map(Vec::as_slice).unwrap_or_default().iter().map(token));
    }
    Ok(words_of(&tokens))
}

/// Fingerprint's columns for this content: from the cache, else read now and cached
pub fn columns(picture: &Path, sound: Option<&Path>, hash: &str, dur: f64, enc: &mut Encoders) -> Result<Columns, String> {
    // refresh reads a reference again with this raea's encoders, past what's kept (VF8c)
    if std::env::var_os("RAEA_COLUMNS_FRESH").is_none() && let Some(c) = cache::load(hash) {
        return Ok(c);
    }
    let frames = media::frames(picture, FPS, 0.0, dur).map_err(|e| e.to_string())?;
    let heard = sound.unwrap_or(picture);
    let pcm16 = media::sound(heard, 16_000).map_err(|e| e.to_string())?;
    let pcm48 = media::sound(heard, 48_000).map_err(|e| e.to_string())?;
    let mut c = enc.of(&frames, &pcm16, &pcm48, dur).map_err(|e| format!("{}: embedding: {e:#}", picture.display()))?;
    c.media = hash.to_string();
    cache::save(&c).map_err(|e| e.to_string())?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, start: f64, end: f64) -> Word {
        Word { text: text.into(), start, end }
    }

    #[test]
    fn tokens_join_into_words_as_heard_ts_joins_them() {
        let t = [w(" just", 0.0, 0.2), w("2", 0.2, 0.3), w("%", 0.3, 0.35), w(" over", 0.4, 0.5), w("time", 0.5, 0.7), w(" ", 0.7, 0.7)];
        assert_eq!(words_of(&t), vec![w("just", 0.0, 0.2), w("2%", 0.2, 0.35), w("overtime", 0.4, 0.7)]);
    }
}
