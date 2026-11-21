//! The kept rows (docs/videoform.feature VF3, VF3b): for each reference, one row a shot, with its measures and two
//! vectors, its SigLIP 2 picture vector (its frames' columns pooled) and its CLAP sound vector (the windows inside it
//! pooled). A reference's rows are two files in the videoform's videoform/rows/: HASH.json (the measures, with the
//! encoders' pins) and HASH.f16 (the vectors, a shot's picture then sound, little-endian half floats). Following and
//! checking read these; the reference itself is never opened again.

use half::f16;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Bumped when a row's measures or vectors change shape, which has every reference read again
/// (4: HE-AAC sound decoded at its true rate; before, a reel's sound read at twice its rate lasted half as long)
pub const VERSION: u32 = 4;

/// One shot of a reference, as it was read
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    pub start: f64,
    pub end: f64,
    /// how it begins: cut, dissolve, fade
    pub transition: String,
    /// its scale: close-up, medium, wide (footage perception)
    pub framing: String,
    pub faces: usize,
    /// the camera: still or moving
    pub motion: String,
    pub brightness: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<f64>,
    /// what's in it, Apple Vision's scene labels
    pub scene: Vec<String>,
    /// words on screen while it shows (captions, titles, signs), and where: [x, y, w, h] of the frame
    pub text: Vec<(String, [f64; 4])>,
    /// the words said while it shows
    pub words: String,
    pub words_per_minute: f64,
    /// how long the voice is silent at its start and its end, seconds (None: no voice near that edge)
    pub pause_before: Option<f64>,
    pub pause_after: Option<f64>,
    /// what's heard most: speech, music, speech and music, quiet (from the two scores below)
    pub heard: String,
    /// how strongly its sound is speech and music: YAMNet's AudioSet scores, the shot's mean (0 to 1)
    pub speech: f64,
    pub music: f64,
    /// its sound's mean level, dB
    pub level_db: f64,
    /// how far its cut is from the nearest beat, seconds (None: no beat near, or the first shot)
    pub off_beat: Option<f64>,
}

impl Shot {
    pub fn seconds(&self) -> f64 {
        self.end - self.start
    }
}

/// A reference's rows
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rows {
    pub version: u32,
    /// the reference's content hash
    pub media: String,
    /// each encoder's id and its file's sha256, as fingerprints pin them: rows made by others are read again
    pub pins: BTreeMap<String, String>,
    pub shots: Vec<Shot>,
    /// [shots][SIG] picture vectors and [shots][CLAP] sound vectors, each unit length (zeros where there was none)
    #[serde(skip)]
    pub sig: Vec<f32>,
    #[serde(skip)]
    pub clap: Vec<f32>,
}

/// The mean of a column's rows in `rows` (each `dim` wide), made unit length; zeros when there are none
fn mean(col: &[f32], dim: usize, rows: impl Iterator<Item = usize>) -> Vec<f32> {
    let mut out = vec![0f32; dim];
    let mut n = 0;
    for r in rows {
        if let Some(v) = col.get(r * dim..(r + 1) * dim) {
            out.iter_mut().zip(v).for_each(|(o, x)| *o += x);
            n += 1;
        }
    }
    let norm = out.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0 && norm > 0.0 {
        out.iter_mut().for_each(|o| *o /= norm);
    }
    out
}

/// A shot's picture vector: the frames sampled at `fps` that fall in [start, end), else the one nearest its middle
pub fn pool_frames(col: &[f32], dim: usize, fps: f64, start: f64, end: f64) -> Vec<f32> {
    let total = col.len() / dim.max(1);
    let inside: Vec<usize> = (0..total).filter(|&i| (start..end).contains(&(i as f64 / fps))).collect();
    if inside.is_empty() {
        let mid = (((start + end) / 2.0) * fps).round() as usize;
        return mean(col, dim, std::iter::once(mid.min(total.saturating_sub(1))));
    }
    mean(col, dim, inside.into_iter())
}

/// A shot's sound vector: the windows (`win` long every `hop`) whose middles fall in [start, end), else the nearest
pub fn pool_windows(col: &[f32], dim: usize, win: f64, hop: f64, start: f64, end: f64) -> Vec<f32> {
    let total = col.len() / dim.max(1);
    let middle = |i: usize| i as f64 * hop + win / 2.0;
    let inside: Vec<usize> = (0..total).filter(|&i| (start..end).contains(&middle(i))).collect();
    if inside.is_empty() {
        let near = (0..total).min_by(|&a, &b| (middle(a) - (start + end) / 2.0).abs().total_cmp(&(middle(b) - (start + end) / 2.0).abs()));
        return mean(col, dim, near.into_iter());
    }
    mean(col, dim, inside.into_iter())
}

/// The mean of a level column (one value every `step` seconds) over [start, end), dB
pub fn level(col: &[f32], step: f64, start: f64, end: f64) -> f64 {
    let v: Vec<f32> = col.iter().enumerate().filter(|(i, _)| (start..end).contains(&(*i as f64 * step))).map(|(_, x)| *x).collect();
    if v.is_empty() { -100.0 } else { (v.iter().sum::<f32>() / v.len() as f32) as f64 }
}

/// The folder of a reference's stills, one a shot from its middle (0.jpg, 1.jpg ...), for the contact sheets the
/// person confirms rules on (VF8): a still is a reading, small, never the video
pub fn stills(dir: &Path, media: &str) -> PathBuf {
    dir.join("videoform/stills").join(media)
}

fn paths(dir: &Path, media: &str) -> (PathBuf, PathBuf) {
    let d = dir.join("videoform/rows");
    (d.join(format!("{media}.json")), d.join(format!("{media}.f16")))
}

impl Rows {
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let (json, f16s) = paths(dir, &self.media);
        std::fs::create_dir_all(json.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&json, serde_json::to_vec(self).unwrap()).map_err(|e| format!("{}: {e}", json.display()))?;
        let bytes: Vec<u8> = self.sig.iter().chain(&self.clap).flat_map(|x| f16::from_f32(*x).to_le_bytes()).collect();
        std::fs::write(&f16s, bytes).map_err(|e| format!("{}: {e}", f16s.display()))
    }

    /// A reference's rows, when they were read at this VERSION with these vector widths
    pub fn load(dir: &Path, media: &str, sig: usize, clap: usize) -> Option<Rows> {
        let (json, f16s) = paths(dir, media);
        let mut r: Rows = serde_json::from_slice(&std::fs::read(json).ok()?).ok()?;
        let v: Vec<f32> = std::fs::read(f16s).ok()?.as_chunks::<2>().0.iter().map(|p| f16::from_le_bytes(*p).to_f32()).collect();
        let n = r.shots.len();
        if r.version != VERSION || v.len() != n * (sig + clap) {
            return None;
        }
        r.clap = v[n * sig..].to_vec();
        r.sig = v[..n * sig].to_vec();
        Some(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shots_pool_their_own_frames_and_windows() {
        // 4 frames a second, 2-wide: frames 0-3 point one way, 4-7 the other
        let col: Vec<f32> = (0..8).flat_map(|i| if i < 4 { [1.0, 0.0] } else { [0.0, 1.0] }).collect();
        assert_eq!(pool_frames(&col, 2, 4.0, 0.0, 1.0), vec![1.0, 0.0]);
        assert_eq!(pool_frames(&col, 2, 4.0, 1.0, 2.0), vec![0.0, 1.0]);
        // a shot shorter than a frame's step takes the nearest
        assert_eq!(pool_frames(&col, 2, 4.0, 1.05, 1.1), vec![0.0, 1.0]);
        // windows 2 s long every 1 s: middles at 1, 2, 3, 4
        let w: Vec<f32> = [[1.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, 1.0]].concat();
        assert_eq!(pool_windows(&w, 2, 2.0, 1.0, 2.5, 5.0), vec![0.0, 1.0]);
        assert_eq!(level(&[-20.0, -20.0, -40.0], 0.1, 0.0, 0.2), -20.0);
    }

    #[test]
    fn rows_come_back_as_they_were_kept() {
        let dir = std::env::temp_dir().join(format!("raea-vf-rows-{}", std::process::id()));
        let shot = Shot { start: 0.0, end: 1.5, framing: "close-up".into(), heard: "speech".into(), ..Default::default() };
        let r = Rows { version: VERSION, media: "abc".into(), shots: vec![shot.clone(), shot], sig: vec![0.5; 6], clap: vec![0.25; 4], ..Default::default() };
        r.save(&dir).unwrap();
        assert_eq!(Rows::load(&dir, "abc", 3, 2), Some(r));
        assert_eq!(Rows::load(&dir, "abc", 4, 2), None, "other widths are read again");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
