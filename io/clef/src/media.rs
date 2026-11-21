//! The media cut to what the decision model reads (docs/ask.feature AK1b), with raea's own FFmpeg: a window of a video as
//! 480 px H.264 of at most 60 s, its sound as mono 16 kHz MP3 of at most 300 s, a still as JPEG of at most 1,600 px.
//! What a file has (its length, a picture, a sound) comes from ffprobe first, so a question that can't be answered
//! isn't asked.

use std::path::{Path, PathBuf};
use std::process::Command;

use raea_core_ask::limits;

/// What a file holds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Probe {
    pub seconds: f64,
    /// a moving picture (a video stream with more than one frame)
    pub video: bool,
    /// a picture that doesn't move (an image, or a video of one frame)
    pub still: bool,
    pub sound: bool,
}

fn tool(name: &str, file: &Path) -> Result<Command, String> {
    let video = file.extension().is_some_and(|e| !matches!(e.to_string_lossy().to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp" | "mp3" | "wav" | "m4a"));
    raea_io_assets::ffmpeg_tool(name, video).map(Command::new)
}

fn run(mut c: Command, what: &str) -> Result<Vec<u8>, String> {
    let out = c.output().map_err(|e| format!("{what}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{what}: {}", err.lines().last().unwrap_or("FFmpeg failed").trim()));
    }
    Ok(out.stdout)
}

/// What `file` holds, from ffprobe.
pub fn probe(file: &Path) -> Result<Probe, String> {
    if !file.is_file() {
        return Err(format!("{}: no such file", file.display()));
    }
    let mut c = tool("ffprobe", file)?;
    c.args(["-v", "error", "-show_entries", "stream=codec_type,nb_frames,disposition:format=duration", "-of", "json"]).arg(file);
    let j: serde_json::Value = serde_json::from_slice(&run(c, "ffprobe")?).map_err(|e| format!("ffprobe: {e}"))?;
    let streams = j["streams"].as_array().cloned().unwrap_or_default();
    let seconds = j["format"]["duration"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let pictures: Vec<&serde_json::Value> = streams.iter().filter(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1).collect();
    let one_frame = |s: &&serde_json::Value| s["nb_frames"].as_str().is_some_and(|n| n == "1") || seconds == 0.0;
    Ok(Probe {
        seconds,
        video: pictures.iter().any(|s| !one_frame(s)),
        still: !pictures.is_empty() && pictures.iter().all(one_frame),
        sound: streams.iter().any(|s| s["codec_type"] == "audio"),
    })
}

/// A file in the temp folder, removed when dropped.
struct Temp(PathBuf);

impl Temp {
    fn new(ext: &str) -> Temp {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Temp(std::env::temp_dir().join(format!("raea-ask-{}-{n}.{ext}", std::process::id())))
    }
    fn read(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| format!("{}: {e}", self.0.display()))
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

fn window(from: f64, to: Option<f64>, most: f64) -> Vec<String> {
    let len = to.map(|t| (t - from).max(0.1)).unwrap_or(most).min(most);
    vec!["-ss".into(), format!("{from:.3}"), "-t".into(), format!("{len:.3}")]
}

/// The H.264 this FFmpeg has, small: libx264 at CRF 30, else the system's own at 600 kb/s (raea_io_ffmpeg's choice, which
/// for a clip a person plays is 8 Mb/s, too big for a minute under 16 MiB).
fn small_h264() -> Result<Vec<String>, String> {
    let mut args: Vec<String> = raea_io_ffmpeg::h264().ok_or("this FFmpeg has no H.264 encoder (libx264, VideoToolbox or Media Foundation)")?.into_iter().map(str::to_string).collect();
    match args.iter().position(|a| a == "8M") {
        Some(i) => args[i] = "600k".into(),
        None => args.extend(["-crf", "30", "-preset", "veryfast"].map(String::from)),
    }
    Ok(args)
}

/// The picture from `from` to `to` (at most 60 s), 480 px wide, no sound.
pub fn video(file: &Path, from: f64, to: Option<f64>) -> Result<Vec<u8>, String> {
    let out = Temp::new("mp4");
    let mut c = tool("ffmpeg", file)?;
    c.args(["-nostdin", "-v", "error", "-y"]).args(window(from, to, limits::VIDEO_SECONDS)).arg("-i").arg(file);
    c.args(["-an", "-vf", "scale=480:-2", "-r", "10"]).args(small_h264()?).args(["-movflags", "+faststart"]).arg(&out.0);
    run(c, "cutting the picture")?;
    out.read()
}

/// The sound from `from` to `to` (at most 300 s), mono 16 kHz MP3.
pub fn sound(file: &Path, from: f64, to: Option<f64>) -> Result<Vec<u8>, String> {
    let out = Temp::new("mp3");
    let mut c = tool("ffmpeg", file)?;
    c.args(["-nostdin", "-v", "error", "-y"]).args(window(from, to, limits::SOUND_SECONDS)).arg("-i").arg(file);
    c.args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "libmp3lame", "-b:a", "64k"]).arg(&out.0);
    run(c, "cutting the sound").map_err(|e| if e.contains("libmp3lame") { "this FFmpeg can't write MP3 (libmp3lame)".into() } else { e })?;
    out.read()
}

/// A still: an image file as it is, or a video's frame at `at` seconds, as JPEG at most 1,600 px wide.
pub fn still(file: &Path, at: Option<f64>) -> Result<Vec<u8>, String> {
    let out = Temp::new("jpg");
    let mut c = tool("ffmpeg", file)?;
    c.args(["-nostdin", "-v", "error", "-y"]);
    if let Some(t) = at {
        c.args(["-ss", &format!("{t:.3}")]);
    }
    c.arg("-i").arg(file).args(["-frames:v", "1", "-vf", "scale='min(1600,iw)':-2", "-q:v", "4"]).arg(&out.0);
    run(c, "taking the still")?;
    out.read()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_held_to_the_limit() {
        assert_eq!(window(5.0, Some(20.0), 60.0), ["-ss", "5.000", "-t", "15.000"]);
        assert_eq!(window(0.0, None, 60.0), ["-ss", "0.000", "-t", "60.000"]);
        assert_eq!(window(0.0, Some(400.0), 300.0)[3], "300.000");
    }
}
