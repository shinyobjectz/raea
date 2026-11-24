//! A take's face track, for <Video framing="face"> (motion.feature, "How a Video fills its box"): where the largest
//! face's eyes are, sampled a few times a second by raea-perceive (found as `raea cut` and `raea frame` find it), kept
//! as TAKE.faces.json beside the take (as TAKE.words.json is), made on the first render that frames the take.

use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Samples a second.
const RATE: f64 = 4.0;
/// The longest side a sample is read at (the eyes are in shares of the picture, so its size doesn't matter)
const SIDE: u32 = 640;

/// The eyes at each sample, in shares of the picture (None: no face in that sample).
pub(crate) struct Track {
    every: f64,
    eyes: Vec<Option<(f64, f64)>>,
}

impl Track {
    /// The eyes at `t` seconds: between the samples around it, or the nearest sample with a face.
    pub(crate) fn eyes_at(&self, t: f64) -> Option<(f32, f32)> {
        let i = (t / self.every).max(0.0);
        let (a, b) = (i.floor() as usize, i.ceil() as usize);
        let got = match (self.eyes.get(a).copied().flatten(), self.eyes.get(b).copied().flatten()) {
            (Some(p), Some(q)) => {
                let k = i - a as f64;
                Some((p.0 + (q.0 - p.0) * k, p.1 + (q.1 - p.1) * k))
            }
            (one, other) => one.or(other).or_else(|| {
                let at = a.min(self.eyes.len().saturating_sub(1));
                (0..self.eyes.len()).filter(|&k| self.eyes[k].is_some()).min_by_key(|&k| k.abs_diff(at)).and_then(|k| self.eyes[k])
            }),
        };
        got.map(|(x, y)| (x as f32, y as f32))
    }
}

thread_local! {
    static TRACKS: RefCell<HashMap<PathBuf, Result<Rc<Track>, String>>> = RefCell::new(HashMap::new());
}

/// `take`'s face track: TAKE.faces.json when it's there and made from the take as it is now, else made (and kept).
/// An error says why there's none (no raea-perceive, no face in the take), once per take.
pub(crate) fn track(take: &Path) -> Result<Rc<Track>, String> {
    if let Some(t) = TRACKS.with_borrow(|m| m.get(take).cloned()) {
        return t;
    }
    let got = load_or_make(take).map(Rc::new);
    TRACKS.with_borrow_mut(|m| m.insert(take.to_path_buf(), got.clone()));
    got
}

/// The take as it is now (its size and modified time): a track made from another take of the same name is made again
fn stamp(take: &Path) -> Value {
    let m = std::fs::metadata(take).ok();
    let modified = m.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
    json!({"bytes": m.map(|m| m.len()), "modified": modified})
}

fn parse(v: &Value) -> Option<Track> {
    let every = v["every"].as_f64()?;
    // each sample [x, y], or null where no face was found
    let eyes = serde_json::from_value(v["eyes"].clone()).ok()?;
    Some(Track { every, eyes })
}

fn load_or_make(take: &Path) -> Result<Track, String> {
    // TAKE.faces.json, beside TAKE.words.json
    let file = take.with_extension("faces.json");
    if let Some(t) = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).filter(|v| v["take"] == stamp(take)).and_then(|v| parse(&v)) {
        return Ok(t);
    }
    let t = make(take)?;
    if t.eyes.iter().all(Option::is_none) {
        return Err(format!("framing=\"face\": raea-perceive found no face in {}, so there's nothing to frame on (give the Video objectPosition instead)", take.display()));
    }
    let eyes: Vec<Value> = t.eyes.iter().map(|e| e.map_or(Value::Null, |(x, y)| json!([(x * 1e4).round() / 1e4, (y * 1e4).round() / 1e4]))).collect();
    let out = json!({"take": stamp(take), "every": t.every, "eyes": eyes});
    if let Err(e) = std::fs::write(&file, serde_json::to_string(&out).unwrap_or_default() + "\n") {
        eprintln!("raea: {}: the face track wasn't kept ({e}); it's made again next time", file.display());
    }
    Ok(t)
}

/// The take sampled at RATE a second, each sample's largest face found by raea-perceive
fn make(take: &Path) -> Result<Track, String> {
    let perceive = raea_io_assets::perceive_bin();
    let be = raea_io_media::backend(&raea_sys_frame::media_backend())?;
    let mut d = be.open_video(take).map_err(|e| format!("{}: {e}", take.display()))?;
    let (dur, fps) = (d.info().duration, d.info().fps);
    let every = 1.0 / RATE;
    let n = ((dur / every).floor() as usize + 1).max(1);
    let dir = std::env::temp_dir().join(format!("raea-faces-{}-{}", std::process::id(), take.file_stem().and_then(|s| s.to_str()).unwrap_or("take")));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut files = Vec::with_capacity(n);
        for k in 0..n {
            let t = (k as f64 * every).min((dur - 0.5 / fps.max(1.0)).max(0.0));
            let f = d.frame_at(t).map_err(|e| format!("{} at {t:.2} s: {e}", take.display()))?;
            let img = image::RgbaImage::from_raw(f.width, f.height, f.rgba).ok_or("a frame of the wrong size")?;
            let k_side = f.width.max(f.height);
            let img = if k_side > SIDE {
                let s = SIDE as f64 / k_side as f64;
                image::imageops::resize(&img, (f.width as f64 * s).round() as u32, (f.height as f64 * s).round() as u32, image::imageops::FilterType::Triangle)
            } else {
                img
            };
            let p = dir.join(format!("{k:06}.png"));
            img.save(&p).map_err(|e| e.to_string())?;
            files.push(p);
        }
        let out = std::process::Command::new(&perceive).arg("eyes").args(&files).output().map_err(|e| {
            format!("framing=\"face\" needs raea-perceive to find the face, and it couldn't be run ({}: {e}). Update raea (`curl -fsSL https://raea.io/install | sh`: releases ship it in ~/.raea/bin), or point RAEA_PERCEIVE at one; or frame the Video yourself with objectPosition", perceive.display())
        })?;
        if !out.status.success() {
            return Err(format!("framing=\"face\": raea-perceive eyes failed (an older raea-perceive has no eyes: update it): {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        // the largest face's eyes (their midpoint), in shares of the picture from the top left
        let mut by: HashMap<String, Option<(f64, f64)>> = HashMap::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            let Some(file) = v["file"].as_str() else { continue };
            by.insert(file.to_string(), v["eyes"][0].as_f64().zip(v["eyes"][1].as_f64()));
        }
        Ok(Track { every, eyes: files.iter().map(|p| by.get(&p.display().to_string()).copied().flatten()).collect() })
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
