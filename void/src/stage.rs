//! What void asks of the machine for its stage and tools: rdb's media verbs (`rdb media`, `rdb ffmpeg`), the render's
//! shape, the preview files the TypeScript stage plays (stage_ts.rs), and stills as JPEG for a model to see. The stage
//! itself is stage_ts.rs: the project's render through raea-motion; the rrl stage (a resident `rrl serve`) is gone
//! with rrl (2026-10-17).

use std::path::{Path, PathBuf};

use anyhow::Context;

/// The rdb binary: RAEA_RDB, else raea/rdb's release build. Its media verbs (`rdb media probe|still|sheet|...`,
/// `rdb ffmpeg`, `rdb ffprobe`: raea-media, no ffmpeg binary) are the media work void asks of the machine.
pub fn rdb_bin() -> PathBuf {
    std::env::var_os("RAEA_RDB")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../rdb/target/release/rdb"))
}

/// One rdb command (`rdb media sheet …`): its whole output either way, stdout first, at most 10 minutes.
pub async fn rdb(args: Vec<String>) -> anyhow::Result<String> {
    let run = tokio::process::Command::new(rdb_bin()).args(&args).kill_on_drop(true).output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(600), run)
        .await
        .map_err(|_| anyhow::anyhow!("rdb {} gave no answer in 10 minutes and was stopped", args.first().map(String::as_str).unwrap_or("")))?
        .with_context(|| format!("{}", rdb_bin().display()))?;
    let mut text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() && text.is_empty() {
        text = err.trim().to_string();
    }
    anyhow::ensure!(out.status.success(), "rdb {}: {text}", args.iter().take(2).cloned().collect::<Vec<_>>().join(" "));
    Ok(text)
}

/// What a render is: its size, length and rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub frames: i64,
}

/// Previews: the program rendered to H.264 with its sound by rdb's own pipeline (`rdb render --codec apple`, ~24 fps
/// for the hook under load against ~13 fps from three serve workers), one file per edit version under
/// ~/.raea-void/previews, the newest 8 kept. Playback plays this file in the webview's video element, so buffering,
/// timing and audio sync are the platform's.
pub fn previews_dir() -> PathBuf {
    crate::spaces::home().join("previews")
}

pub fn preview_path(version: u64) -> PathBuf {
    previews_dir().join(format!("{version:x}.mp4"))
}

/// A PNG as a base64 JPEG at most `max` px on its longest side (sips), for a model to see.
pub fn jpeg_b64(png: &Path, max: u32) -> anyhow::Result<String> {
    use base64::Engine as _;
    let jpg = png.with_extension("jpg");
    let ok = std::process::Command::new("sips")
        .args(["-s", "format", "jpeg", "-s", "formatOptions", "80", "-Z", &max.to_string()])
        .arg(png).arg("--out").arg(&jpg).output()?.status.success();
    anyhow::ensure!(ok, "sips couldn't convert {}", png.display());
    Ok(base64::engine::general_purpose::STANDARD.encode(std::fs::read(&jpg)?))
}
