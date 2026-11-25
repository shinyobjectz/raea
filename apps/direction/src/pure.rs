//! SG9 (docs/stage-guards.feature): a frame looks the same however it is reached. When a video render with 3D in it ends,
//! a sample of its frames is rendered again by one more raea (`render --at`: each frame reached by a jump, none by the
//! frames before it) and held to the same frame of the video. A patch that differs beyond the encoder's noise means a
//! part kept state from an earlier frame: a shape, words, a colour or another part's row held over (SG1–SG4's faults,
//! and any later one of their kind). That frame is rendered alone (`--frame N`, in its own process) to be sure, and the
//! error names the frame, the patch, the parts under it (the stills' manifest, raea-sys-stage's on_screen) and where
//! the two pictures are kept. A frame that differs only when reached by a jump is the engine's fault, not the
//! composition's: a warning. RAEA_FRAME_PURE=0 skips the check (the stills it renders set it).

use std::path::Path;
use std::process::Command;

use raea_core_facts::{Finding, Level};
use serde_json::Value;

/// Sampled: every this many frames, the last frame, and the frame after each remount (more than 20 parts mounting or
/// unmounting at once, as the render's manifest lists them), for a render of more than MIN_FRAMES
const EVERY: u64 = 300;
const MIN_FRAMES: u64 = 60;
/// at most this many samples, spread evenly
const MOST: usize = 64;
/// A pixel differs when a channel differs by more than this (of 255) through a 3x3 erosion, which drops lone pixels (a
/// glyph's edge noise, the encoder's, its colour bleeding along an edge) and keeps a patch: tools/frame-pure-3d.sh's
/// bar, where a cut letter was 318. The largest channel, not luma: an orange part over a grey floor differs by 68 in
/// luma and 116 in blue, and a part that kept another frame's colour is often as bright as its own
const LEVELS: f32 = 96.0;
/// A frame differs when more of its pixels than this share differ (207 at 1080p), and at least LEAST
const SHARE: f64 = 1e-4;
const LEAST: usize = 64;
/// The flags of the render the stills repeat (each takes a value, then the bare ones): what it draws and at what size
const KEEP: [&str; 12] = ["--composition", "--w", "--h", "--fps", "--frames", "--words", "--perception", "--input", "--stage", "--props", "--scale", "--hold"];
const KEEP_BARE: [&str; 2] = ["--clay", "--lite"];
const BY: &str = "raea render (frame-pure)";

/// The frames to sample of a render `length` frames long, given its remounts.
pub(crate) fn samples(length: u64, remounts: &[u64]) -> Vec<u64> {
    if length <= MIN_FRAMES {
        return Vec::new();
    }
    let mut s: Vec<u64> = (1..).map(|k| k * EVERY).take_while(|f| *f < length).collect();
    s.push(length - 1);
    s.extend(remounts.iter().map(|f| f + 1).filter(|f| *f < length));
    s.sort_unstable();
    s.dedup();
    // more than MOST: the last of each of MOST even runs, so the last frame stays
    let n = s.len();
    if n > MOST {
        s = s.into_iter().enumerate().filter(|(i, _)| (i + 1) * MOST / n > i * MOST / n).map(|(_, f)| f).collect();
    }
    s
}

/// Where two RGBA frames of one size differ: the count of pixels with a channel off by more than LEVELS through a 3x3
/// erosion (every pixel round it differs too), and their box [x0, y0, x1, y1].
pub(crate) fn patch(a: &[u8], b: &[u8], w: usize, h: usize) -> (usize, [usize; 4]) {
    let d: Vec<f32> = (0..w * h)
        .map(|i| {
            let c = |k: usize| (f32::from(a[i * 4 + k]) - f32::from(b[i * 4 + k])).abs();
            c(0).max(c(1)).max(c(2))
        })
        .collect();
    let (mut n, mut bx) = (0, [usize::MAX, usize::MAX, 0, 0]);
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            if (y - 1..=y + 1).all(|yy| (x - 1..=x + 1).all(|xx| d[yy * w + xx] > LEVELS)) {
                n += 1;
                bx = [bx[0].min(x), bx[1].min(y), bx[2].max(x + 1), bx[3].max(y + 1)];
            }
        }
    }
    (n, if n == 0 { [0; 4] } else { bx })
}

fn differs(n: usize, w: usize, h: usize) -> bool {
    n > LEAST.max((SHARE * (w * h) as f64) as usize)
}

/// The render's own flags the stills repeat.
fn kept(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, a) in args.iter().enumerate() {
        if KEEP.contains(&a.as_str()) {
            out.extend(args.get(i..i + 2).into_iter().flatten().cloned());
        } else if KEEP_BARE.contains(&a.as_str()) {
            out.push(a.clone());
        }
    }
    out
}

/// One more raea rendering `file` at `at` into `dir` (its stills, frame-00000.png on, in `at`'s order, and its
/// manifest); its own errors are let through, since these are pictures to compare, not the render. Stills past 3,600
/// frames make their 3D live at each (render_bundle's LIVE_PAST): 64 of a 13,175-frame film reached by rows went past
/// 30 GB and were killed.
fn stills(file: &str, args: &[String], at: &[u64], dir: &Path) -> Result<Value, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let list: Vec<String> = at.iter().map(u64::to_string).collect();
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let manifest = dir.join("stills.json");
    let (flag, out) = if at.len() == 1 { ("--frame", dir.join("frame-00000.png").display().to_string()) } else { ("--at", format!("{}/", dir.display())) };
    let o = Command::new(me).arg("render").arg(file).args(kept(args)).args([flag, &list.join(","), "-o", &out, "--manifest"]).arg(&manifest).env("RAEA_RECORD", "off").env("RAEA_FRAME_PURE", "0").env("RAEA_DESPITE", "all: frame-pure's own stills")
        // nothing here reads the stills' clipping or staging, only their pictures and parts on screen: the checks cost
        // the film's 44 stills 1.6 GB, beside the render that's still held (SG4a: its 8.6 GB peak)
        .env("RAEA_CLIP_CHECK", "0").env("RAEA_STAGE_CHECK", "0").output().map_err(|e| e.to_string())?;
    let m = std::fs::read(&manifest).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    m.ok_or_else(|| format!("the stills didn't render: {}", String::from_utf8_lossy(&o.stderr).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message")))
}

fn png(p: &Path) -> Option<(Vec<u8>, usize, usize)> {
    let img = image::open(p).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w as usize, h as usize))
}

/// The parts on screen at `frame` whose boxes meet `bx`, most overlap first (the manifest of the frame rendered alone:
/// its `parts` hold that one frame).
fn under(m: &Value, frame: u64, bx: [usize; 4]) -> Vec<String> {
    let at = &m["parts"][0];
    if at["frame"] != frame {
        return Vec::new();
    }
    let b = bx.map(|v| v as f64);
    let mut hit: Vec<(f64, String)> = at["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let r: Vec<f64> = p[1].as_array()?.iter().filter_map(Value::as_f64).collect();
            let r: [f64; 4] = r.try_into().ok()?;
            let over = (b[2].min(r[2]) - b[0].max(r[0])).max(0.0) * (b[3].min(r[3]) - b[1].max(r[1])).max(0.0);
            (over > 0.0).then(|| (over, p[0].as_str().unwrap_or("a part").to_string()))
        })
        .collect();
    hit.sort_by(|a, b| b.0.total_cmp(&a.0));
    hit.into_iter().map(|h| h.1).take(3).collect()
}

/// SG9 on a finished render: its findings, none when it isn't a video with 3D in it of more than MIN_FRAMES.
pub fn check(args: &[String], out: &Path, manifest: &Value) -> Vec<Finding> {
    let (Some(file), true) = (args.get(1), manifest["has3d"].as_bool() == Some(true)) else { return Vec::new() };
    if std::env::var("RAEA_FRAME_PURE").as_deref() == Ok("0") || args.iter().any(|a| a == "--frame" || a == "--at") {
        return Vec::new();
    }
    let remounts: Vec<u64> = manifest["remounts"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
    let at = samples(manifest["length"].as_u64().unwrap_or(0), &remounts);
    let fps = manifest["fps"].as_f64().unwrap_or(30.0);
    if at.is_empty() {
        return Vec::new();
    }
    let dir = std::env::temp_dir().join(format!("raea-frame-pure-{}", std::process::id()));
    let found = run(file, args, out, &at, fps, &dir).unwrap_or_else(|e| vec![Finding::new("frame-pure", Level::Note, format!("frame purity wasn't checked: {e}"), BY)]);
    let _ = std::fs::remove_dir_all(&dir);
    found
}

fn run(file: &str, args: &[String], out: &Path, at: &[u64], fps: f64, dir: &Path) -> Result<Vec<Finding>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    stills(file, args, at, &dir.join("at"))?;
    let mut video = raea_io_media::preferred().into_iter().find_map(|b| b.open_video(out).ok()).ok_or_else(|| format!("{}: no decoder opens it", out.display()))?;
    let keep = out.with_extension("frame-pure");
    let mut found = Vec::new();
    for (i, &f) in at.iter().enumerate() {
        let Some((jump, w, h)) = png(&dir.join(format!("at/frame-{i:05}.png"))) else { continue };
        let v = video.frame_at((f as f64 + 0.5) / fps)?;
        if (v.width as usize, v.height as usize) != (w, h) {
            return Err(format!("the video is {}x{}, its stills {w}x{h}", v.width, v.height));
        }
        if !differs(patch(&v.rgba, &jump, w, h).0, w, h) {
            continue;
        }
        // reached by a jump it differs: rendered alone, in its own process, to be sure
        let one = dir.join(format!("alone-{f}"));
        let alone_m = stills(file, args, &[f], &one)?;
        let Some((alone, _, _)) = png(&one.join("frame-00000.png")) else { continue };
        let (n, bx) = patch(&v.rgba, &alone, w, h);
        let parts = under(&alone_m, f, bx);
        let named = if parts.is_empty() { String::new() } else { format!(" over {}", parts.join(", ")) };
        let finding = if differs(n, w, h) {
            std::fs::create_dir_all(&keep).map_err(|e| e.to_string())?;
            let (seq, own) = (keep.join(format!("frame-{f:05}-render.png")), keep.join(format!("frame-{f:05}-alone.png")));
            image::save_buffer(&seq, &v.rgba, w as u32, h as u32, image::ColorType::Rgba8).map_err(|e| e.to_string())?;
            std::fs::copy(one.join("frame-00000.png"), &own).map_err(|e| e.to_string())?;
            let mut x = Finding::new("frame-pure", Level::Error, format!("{n} pixels{named} (x {}-{}, y {}-{}) differ from the frame rendered alone: a part kept an earlier frame's shape, words, colour or row. The two are {} and {}", bx[0], bx[2], bx[1], bx[3], seq.display(), own.display()), BY).frames(f as i64, f as i64 + 1).measure("pixels", n as f64, "px", Some(LEAST.max((SHARE * (w * h) as f64) as usize) as f64));
            for p in &parts {
                x = x.about(p.clone(), None);
            }
            x.fix("find what that part takes from the frames before (a key it keeps, a prop read once), or `raea bug` with both pictures when it's raea's")
        } else {
            Finding::new("frame-pure-jump", Level::Warn, format!("frame {f}{named} differs when the render jumps to it from frame {}, not alone or in sequence: raea's, not the composition's (`raea bug` with this line)", if i > 0 { at[i - 1].to_string() } else { "0".into() }), BY).frames(f as i64, f as i64 + 1)
        };
        found.push(finding);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_every_300th_the_last_and_after_each_remount() {
        assert!(samples(60, &[]).is_empty());
        assert_eq!(samples(180, &[]), vec![179]);
        assert_eq!(samples(1000, &[40, 599, 999]), vec![41, 300, 600, 900, 999]);
        let many = samples(1_000_000, &[]);
        assert_eq!((many.len(), many.last()), (MOST, Some(&999_999)));
    }

    #[test]
    fn a_patch_counts_but_lone_pixels_and_noise_dont() {
        let (w, h) = (64, 32);
        let a = vec![40u8; w * h * 4];
        let mut b = a.clone();
        // noise: every pixel off by 20, and lone pixels off by 200
        b.iter_mut().for_each(|v| *v += 20);
        for i in (0..w * h).step_by(7) {
            b[i * 4..i * 4 + 3].copy_from_slice(&[240, 240, 240]);
        }
        assert_eq!(patch(&a, &b, w, h).0, 0);
        // a 10x6 block of words turned white at x 20, y 8: its eroded inside, 8x4
        for y in 8..14 {
            for x in 20..30 {
                b[(y * w + x) * 4..(y * w + x) * 4 + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let (n, bx) = patch(&a, &b, w, h);
        assert_eq!(n, 32);
        assert_eq!(bx, [21, 9, 29, 13]);
    }

    #[test]
    fn the_stills_repeat_what_the_render_draws_and_nothing_else() {
        let a: Vec<String> = ["render", "e.tsx", "-o", "out.mp4", "--composition", "k3", "--kbps", "8000", "--clay", "--despite", "type-small", "why", "--frames", "90"].iter().map(|s| s.to_string()).collect();
        assert_eq!(kept(&a), ["--composition", "k3", "--clay", "--frames", "90"]);
    }

    #[test]
    fn the_parts_under_a_patch_most_overlap_first() {
        let m = serde_json::json!({"parts": [{"frame": 600, "parts": [["floor", [0.0, 500.0, 1920.0, 1080.0]], ["counter", [800.0, 300.0, 1000.0, 400.0]], ["sky", [0.0, 0.0, 1920.0, 200.0]]]}]});
        assert_eq!(under(&m, 600, [850, 350, 1000, 520]), ["counter", "floor"]);
        assert!(under(&m, 300, [850, 350, 1000, 520]).is_empty());
    }
}
