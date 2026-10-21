//! What a viewer can't read, measured on the render alone: a picture graded flat, and text the frame's edge cuts
//! through. The reader is perception's text recognition (Vision), so it checks what's drawn, whatever drew it. The
//! judge's commonest defects on the first run of the goal-only scenarios (2026-10-16 smoke: unreadable contrast, text
//! over text, missing captions, a washed-out grade) after the face; text over the face and off the frame are read from
//! the render's own manifest (host.rs), not here.

use serde_json::Value;
use std::path::Path;

/// What the render alone shows, with no edit to read (a TypeScript project's `raea check`): a flat picture, and text the
/// frame's edge cuts through.
pub async fn video_faults(video: &Path, frames: i64, fps: f64) -> Vec<String> {
    let (flat, edge) = tokio::join!(flat_picture(video), edge_text(video, frames, fps.max(1.0)));
    flat.into_iter().chain(edge).collect()
}

/// Text the frame's edge cuts through, read at 8 moments across the render: a line whose box runs to the left or right
/// edge. The judge's commonest app-promo defect on 2026-10-16 was a landscape screen recording centre-cropped to 9:16,
/// "clipping UI options, buttons and handwritten callouts off both edges". Two moments or more, so one passing word
/// doesn't count.
async fn edge_text(video: &Path, frames: i64, fps: f64) -> Option<String> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("raea-edge-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::create_dir_all(&dir);
    let at: Vec<i64> = (0..8).map(|i| ((i as f64 + 0.5) / 8.0 * frames as f64) as i64).collect();
    // the eight frames pulled at once, not one after another
    let pulled = futures_util::future::join_all(at.iter().map(|&f| {
        let png = dir.join(format!("{f}.png"));
        async move {
            let ok = tokio::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-ss", &format!("{:.3}", (f as f64 + 0.5) / fps), "-i"]).arg(video)
                .args(["-frames:v", "1"]).arg(&png).status().await.map(|s| s.success()).unwrap_or(false);
            (ok && png.exists()).then_some((f, png))
        }
    })).await;
    let pngs: Vec<(i64, std::path::PathBuf)> = pulled.into_iter().flatten().collect();
    let perceive = std::env::var("RAEA_PERCEIVE").unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ai/perception/target/release/raea-perceive").to_string_lossy().into());
    let o = if pngs.is_empty() { None } else {
        tokio::process::Command::new(&perceive).arg("read-lines").args(pngs.iter().map(|p| &p.1)).args(backend_args()).output().await.ok()
    };
    let _ = std::fs::remove_dir_all(&dir);
    let mut hits: Vec<(i64, String, &str)> = Vec::new();
    for line in String::from_utf8_lossy(&o?.stdout).lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let Some(f) = pngs.iter().find(|p| Some(p.1.to_string_lossy().as_ref()) == v["file"].as_str()).map(|p| p.0) else { continue };
        for l in v["lines"].as_array().into_iter().flatten() {
            let (Some(t), Some(b)) = (l["text"].as_str(), l["box"].as_array()) else { continue };
            let (x, w) = (b[0].as_f64().unwrap_or(0.5), b[2].as_f64().unwrap_or(0.0));
            if t.chars().filter(|c| c.is_alphabetic()).count() < 3 { continue }
            let side = if x <= 0.012 { "left" } else if x + w >= 0.988 { "right" } else { continue };
            if !hits.iter().any(|h| h.0 == f) { hits.push((f, t.to_string(), side)) }
        }
    }
    if hits.len() < 2 {
        return None;
    }
    let said: Vec<String> = hits.iter().take(4).map(|(f, t, side)| format!("{} \"{}\" off the {side}", clock(*f, fps), short(t))).collect();
    Some(format!("text runs off the frame's edge at {} of 8 moments ({}): the crop cuts through words, in the footage or in your own rows. Reframe so whole lines sit inside the frame (zoom out, pan, or crop to a region where they fit; a wide screen recording is best shown whole: <ScreenFit>, `raea frame TAKE`), or move the text in", hits.len(), said.join("; ")))
}

/// The text lines a reader finds in the render at these seconds (perception's read-lines), for a check's context.
pub async fn read_text(video: &Path, at_s: &[f64]) -> Vec<String> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("raea-read-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::create_dir_all(&dir);
    let mut pngs = Vec::new();
    for (i, t) in at_s.iter().enumerate() {
        let png = dir.join(format!("{i}.png"));
        let ok = tokio::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-ss", &format!("{t:.2}"), "-i"]).arg(video)
            .args(["-frames:v", "1"]).arg(&png).status().await.map(|s| s.success()).unwrap_or(false);
        if ok && png.exists() { pngs.push(png) }
    }
    let perceive = std::env::var("RAEA_PERCEIVE").unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ai/perception/target/release/raea-perceive").to_string_lossy().into());
    let mut out: Vec<String> = Vec::new();
    if !pngs.is_empty() {
        if let Ok(o) = tokio::process::Command::new(&perceive).arg("read-lines").args(&pngs).args(backend_args()).output().await {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
                for t in v["lines"].as_array().into_iter().flatten().filter_map(|l| l["text"].as_str()) {
                    if !out.iter().any(|o| o == t) { out.push(t.to_string()) }
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// The text reader perception uses: $RAEA_OCR_BACKEND (apple, ocrs), else its own default.
fn backend_args() -> Vec<String> {
    std::env::var("RAEA_OCR_BACKEND").map(|b| vec!["--backend".to_string(), b]).unwrap_or_default()
}

fn short(t: &str) -> String {
    let s: String = t.chars().take(40).collect();
    if t.chars().count() > 40 { format!("{s}…") } else { s }
}

fn clock(f: i64, fps: f64) -> String {
    let s = f as f64 / fps;
    format!("{}:{:04.1}", (s / 60.0) as i64, s % 60.0)
}

/// A picture with almost nothing in it: three seconds or more in a row whose whole brightness range (darkest to
/// brightest pixel) is under 70 of 255: a grey wash over everything, an empty screen or a blank card. The first smoke
/// run's explainer sat at 59 for half its length under a grey wash; a clean white page with type on it spans 240+.
async fn flat_picture(video: &Path) -> Option<String> {
    // decoded in hardware where there is one: software decoded every frame of a 1080x1920 render to sample one a
    // second, most of a check's picture step
    let mut cmd = tokio::process::Command::new("ffmpeg");
    cmd.arg("-hide_banner");
    if cfg!(target_vendor = "apple") {
        cmd.args(["-hwaccel", "videotoolbox"]);
    }
    let o = cmd.arg("-i").arg(video)
        .args(["-vf", "fps=1,scale=270:-2,signalstats,metadata=print:file=-", "-an", "-f", "null", "-"]).output().await.ok()?;
    let text = String::from_utf8_lossy(&o.stdout).to_string();
    let grab = |k: &str| -> Vec<f64> { text.split(&format!("lavfi.signalstats.{k}=")).skip(1).filter_map(|s| s.split_whitespace().next()?.parse().ok()).collect() };
    let (lo, hi) = (grab("YMIN"), grab("YMAX"));
    let n = lo.len().min(hi.len());
    let flat: Vec<bool> = (0..n).map(|i| hi[i] - lo[i] < 70.0).collect();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if flat[i] {
            let j = (i..n).find(|&j| !flat[j]).unwrap_or(n);
            if j - i >= 3 { runs.push((i, j)); }
            i = j;
        } else {
            i += 1;
        }
    }
    let (a, b) = *runs.first()?;
    let total: usize = runs.iter().map(|r| r.1 - r.0).sum();
    Some(format!("the picture has almost nothing in it from 0:{a:02} to 0:{b:02} (its brightness spans only {:.0}-{:.0} of 255{}): a wash over the footage, an empty screen or a blank card; give those seconds something to see", lo[a], hi[a], if runs.len() > 1 { format!("; {total} s in all") } else { String::new() }))
}
