//! The structure part of an ad card (docs/ad-card.feature): what an edit looks like in numbers, measured the same way
//! for Kelly's ads, market ads and Buddy's own renders so they can be compared.
//!
//! A card holds the raw measurements (cuts, loudness, silence, and per second: on-screen text and faces). From it come
//! a summary (the editing profile's numbers: pacing, first cut, text share…), a 43-number structure vector, and a
//! comparison with the norms measured over thousands of ads (knowledge/editing-profile.json, ugc-edit-styles.json):
//! plain-words notes where the edit falls outside most ads of its kind, and the nearest editing style. The notes are
//! advice, never rules: nothing here blocks an edit.
//!
//! The numbers match knowledge/drive/bin/edit_anatomy.py, anatomy_aggregate.py and edit_vectors.py, which built the
//! corpus; tests pin the shared parts.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// One second of the ad: on-screen text (lines at least 2% of the frame tall) and the largest face.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Second {
    pub t: u32,
    #[serde(default)]
    pub text: bool,
    #[serde(default)]
    pub words: u32,
    /// centre of each text line, 0 = top of the frame, 1 = bottom
    #[serde(default)]
    pub text_y: Vec<f64>,
    /// height of each text line as a share of the frame's height
    #[serde(default)]
    pub text_h: Vec<f64>,
    /// the largest face's box as a share of the frame (0 = no face)
    #[serde(default)]
    pub face: f64,
}

/// An ad's measurements. `source` says how they were taken: "video" (decoded and detected) or "timeline" (read from an
/// edit's composition: exact cuts and text, no faces unless a render was measured too).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Card {
    #[serde(default)]
    pub source: String,
    pub duration: f64,
    #[serde(default)]
    pub w: Option<f64>,
    #[serde(default)]
    pub h: Option<f64>,
    #[serde(default)]
    pub cuts: Vec<f64>,
    #[serde(default)]
    pub lufs: Option<f64>,
    #[serde(default)]
    pub lra: Option<f64>,
    #[serde(default)]
    pub silence_s: Option<f64>,
    #[serde(default)]
    pub has_audio: bool,
    #[serde(default)]
    pub seconds: Vec<Second>,
    /// false when faces weren't looked for (a timeline without a render): face numbers are left out
    #[serde(default = "yes")]
    pub faces_known: bool,
    /// Shot changes from the sensitive detector (every cut plus the jump cuts a fixed scene threshold misses: a spike
    /// well above the frames around it). `cuts` stays the fixed-threshold list the editing profile was built on.
    #[serde(default)]
    pub shot_changes: Vec<f64>,
    /// Where the picture holds still for 1 s or more (frame-to-frame luma change under 0.5 at 480 px): freezes, held
    /// end cards, a locked-off product shot. Measured, not judged: the judge watching at about a frame a second missed
    /// two 1.5 s freezes that this finds to the frame (2026-10-12).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stills: Vec<[f64; 2]>,
    /// Still and silent at once for 1 s or more (silence under -40 dB): dead air a viewer scrolls past.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dead_air: Vec<[f64; 2]>,
    /// The creator's performance each second (eye contact, blinks, energy, speaking, reading) and the lip-sync offset,
    /// from the app's own Perception.swift (`ugc-perceive performance`), when measure is given it (T-332).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance: Option<Performance>,
}

/// One second of the creator's performance, as the app's Perception.swift measures it (docs/perception-measures.feature
/// › The creator's performance). Derived numbers only: no face points or expression scores.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PerformanceSecond {
    pub t: u32,
    /// share of the second's frames with a face
    pub face: f64,
    /// "lens", "near" (the screen just beside the lens) or "away"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eye_contact: Option<String>,
    /// share of the second's face frames inside the lens cone
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens_share: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blink: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaking: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gesture: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Performance {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub fps: f64,
    /// the sound against the mouth, ms: positive when the sound comes late
    #[serde(default)]
    pub lip_sync_ms: Option<f64>,
    #[serde(default)]
    pub lip_sync_confidence: Option<f64>,
    #[serde(default)]
    pub seconds: Vec<PerformanceSecond>,
}

impl Performance {
    /// Share of the hook's face seconds (the first 3 s) toward the camera (lens or near): the judge's "eye contact in the hook",
    /// measured instead of judged.
    pub fn hook_eye_contact(&self) -> Option<f64> {
        let hook: Vec<&str> = self.seconds.iter().filter(|s| s.t < 3).filter_map(|s| s.eye_contact.as_deref()).collect();
        (!hook.is_empty()).then(|| round3(hook.iter().filter(|e| **e != "away").count() as f64 / hook.len() as f64))
    }
}

/// The creator's performance in a video, from the app's Perception.swift as a Mac command (`ugc-perceive performance
/// <video> 15`, agent/tools/perceive), so measure, the evals and the app read the same numbers.
pub fn performance(path: &std::path::Path, perceive: &std::path::Path) -> Result<Performance, String> {
    let out = std::process::Command::new(perceive).arg("performance").arg(path).arg("15").output()
        .map_err(|e| format!("{}: {e}", perceive.display()))?;
    if !out.status.success() {
        return Err(format!("{}: {}", perceive.display(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("performance: {e}"))
}

fn yes() -> bool {
    true
}

/// Names of the 43 structure-vector numbers, in order (edit_vectors.py NAMES).
pub const FEATURES: [&str; 43] = [
    "cut_rate_s0", "cut_rate_s1", "cut_rate_s2", "cut_rate_s3", "cut_rate_s4", "cut_rate_s5", "cut_rate_s6", "cut_rate_s7", "cut_rate_s8", "cut_rate_s9",
    "text_s0", "text_s1", "text_s2", "text_s3", "text_s4", "text_s5", "text_s6", "text_s7", "text_s8", "text_s9",
    "face_s0", "face_s1", "face_s2", "face_s3", "face_s4", "face_s5", "face_s6", "face_s7", "face_s8", "face_s9",
    "log_duration", "cuts_per_10s", "log_median_shot", "first_cut", "text_share", "text_y", "text_h", "log_words",
    "face_share", "face_size", "lufs", "silence_share", "aspect_hw",
];

/// Python's statistics.median: the middle value, or the mean of the two middle values.
pub fn median(xs: &[f64]) -> Option<f64> {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

impl Card {
    /// Cuts inside the ad (a cut in the first or last 0.2 s is the detector seeing the start or the end).
    pub fn inner_cuts(&self) -> Vec<f64> {
        let d = self.duration;
        self.cuts.iter().copied().filter(|c| *c > 0.2 && *c < d - 0.2).collect()
    }

    fn shots(&self) -> Vec<f64> {
        let d = self.duration;
        if d <= 0.0 {
            return vec![];
        }
        let cuts = self.inner_cuts();
        let starts = std::iter::once(0.0).chain(cuts.iter().copied());
        let ends = cuts.iter().copied().chain(std::iter::once(d));
        starts.zip(ends).map(|(a, b)| b - a).collect()
    }

    /// The editing profile's numbers for this ad (anatomy_aggregate.py `features`). None where it can't be known.
    pub fn summary(&self) -> BTreeMap<&'static str, Option<f64>> {
        let d = self.duration;
        let cuts = self.inner_cuts();
        let shots = self.shots();
        let secs = &self.seconds;
        let n = secs.len().max(1) as f64;
        let has = !secs.is_empty();
        let text_y: Vec<f64> = secs.iter().flat_map(|s| s.text_y.iter().copied()).collect();
        let text_h: Vec<f64> = secs.iter().flat_map(|s| s.text_h.iter().copied()).collect();
        let mut by_t: Vec<&Second> = secs.iter().collect();
        by_t.sort_by_key(|s| s.t);
        let words: Vec<f64> = secs.iter().filter(|s| s.text).map(|s| s.words as f64).collect();
        let faces: Vec<f64> = secs.iter().filter(|s| s.face > 0.0).map(|s| s.face).collect();
        let face_known = self.faces_known && has;
        let mut m = BTreeMap::new();
        m.insert("duration", Some(d));
        m.insert("cuts_per_10s", (d > 0.0).then(|| cuts.len() as f64 / d * 10.0));
        m.insert("first_cut_s", cuts.first().copied());
        m.insert("median_shot_s", median(&shots));
        m.insert("cuts_in_first_3s", Some(cuts.iter().filter(|c| **c <= 3.0).count() as f64));
        m.insert("text_share", has.then(|| secs.iter().filter(|s| s.text).count() as f64 / n));
        m.insert("first_text_s", by_t.iter().find(|s| s.text).map(|s| s.t as f64));
        m.insert("text_center_y", median(&text_y));
        m.insert("text_height", median(&text_h));
        m.insert("words_on_screen", median(&words));
        m.insert("face_share", face_known.then(|| faces.len() as f64 / n));
        m.insert("face_size", if face_known { median(&faces) } else { None });
        m.insert("lufs", self.lufs);
        m.insert("silence_share", match (self.silence_s, d > 0.0) {
            (Some(s), true) => Some(s / d),
            _ => None,
        });
        m
    }

    /// The 43-number structure vector (edit_vectors.py `vector`): the ad in ten equal slices (cuts a second, share of
    /// seconds with text, share with a face), then whole-ad numbers. None for ads under 3 s or with no seconds measured.
    pub fn vector(&self) -> Option<Vec<f64>> {
        let d = self.duration;
        if d < 3.0 || self.seconds.is_empty() {
            return None;
        }
        let cuts = self.inner_cuts();
        let bin = |t: f64| ((10.0 * t / d) as usize).min(9);
        let mut cut_rate = [0.0f64; 10];
        for c in &cuts {
            cut_rate[bin(*c)] += 1.0;
        }
        // Flashing or strobing edits read as dozens of cuts a second to the scene detector: cap a slice at 3 a second.
        let cut_rate: Vec<f64> = cut_rate.iter().map(|x| (x / (d / 10.0)).min(3.0)).collect();
        let (mut text, mut face, mut cnt) = ([0.0f64; 10], [0.0f64; 10], [0.0f64; 10]);
        for s in &self.seconds {
            let b = bin(s.t as f64 + 0.5);
            cnt[b] += 1.0;
            text[b] += if s.text { 1.0 } else { 0.0 };
            face[b] += if s.face > 0.0 { 1.0 } else { 0.0 };
        }
        let text: Vec<f64> = text.iter().zip(&cnt).map(|(t, c)| if *c > 0.0 { t / c } else { 0.0 }).collect();
        let face: Vec<f64> = face.iter().zip(&cnt).map(|(f, c)| if *c > 0.0 { f / c } else { 0.0 }).collect();
        let shots = self.shots();
        let ty: Vec<f64> = self.seconds.iter().flat_map(|s| s.text_y.iter().copied()).collect();
        let th: Vec<f64> = self.seconds.iter().flat_map(|s| s.text_h.iter().copied()).collect();
        let words: Vec<f64> = self.seconds.iter().filter(|s| s.text).map(|s| s.words as f64).collect();
        let fs: Vec<f64> = self.seconds.iter().filter(|s| s.face > 0.0).map(|s| s.face).collect();
        let n = self.seconds.len() as f64;
        let w = self.w.filter(|w| *w > 0.0).unwrap_or(1.0).max(1.0);
        let h = self.h.filter(|h| *h > 0.0).unwrap_or(1.0);
        let g = [
            d.ln(),
            (cuts.len() as f64 / d * 10.0).min(15.0),
            median(&shots).unwrap_or(d).max(0.2).ln(),
            cuts.first().map(|c| c.min(10.0)).unwrap_or(10.0),
            self.seconds.iter().filter(|s| s.text).count() as f64 / n,
            median(&ty).unwrap_or(0.5),
            median(&th).unwrap_or(0.0),
            median(&words).map(|x| x.ln_1p()).unwrap_or(0.0),
            self.seconds.iter().filter(|s| s.face > 0.0).count() as f64 / n,
            median(&fs).unwrap_or(0.0),
            self.lufs.unwrap_or(-24.0),
            self.silence_s.unwrap_or(0.0) / d,
            h / w,
        ];
        Some(cut_rate.into_iter().chain(text).chain(face).chain(g).collect())
    }

    /// "vertical", "horizontal" or "square" (anatomy_aggregate.py's shape groups).
    pub fn shape(&self) -> &'static str {
        let (w, h) = (self.w.unwrap_or(0.0), self.h.unwrap_or(0.0));
        if h > w {
            "vertical"
        } else if (h - w).abs() < 0.5 {
            "square"
        } else {
            "horizontal"
        }
    }

    /// The length band the profile groups by.
    pub fn band(&self) -> &'static str {
        match self.duration {
            d if d < 15.0 => "5-15s",
            d if d < 30.0 => "15-30s",
            d if d < 60.0 => "30-60s",
            _ => "60-180s",
        }
    }
}

/// Visible cuts in a timeline: times where the top-most video clip changes (the eval's `video_cuts`). Clips are
/// (start, end) in document order, later clips on top; cuts a few milliseconds apart are one cut.
pub fn timeline_cuts(clips: &[(f64, f64)]) -> Vec<f64> {
    let top = |t: f64| clips.iter().enumerate().filter(|(_, (s, e))| *s <= t && t < *e).map(|(i, _)| i).max();
    let mut edges: Vec<f64> = clips.iter().flat_map(|(s, e)| [*s, *e]).filter(|t| t.is_finite()).collect();
    edges.sort_by(|a, b| a.partial_cmp(b).unwrap());
    edges.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let first = clips.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let last = clips.iter().map(|c| c.1).filter(|e| e.is_finite()).fold(0.0, f64::max);
    let mut cuts: Vec<f64> = edges
        .into_iter()
        .filter(|t| *t > first + 1e-6 && *t < last - 1e-6)
        .filter(|t| {
            let (before, after) = (top(t - 1e-4), top(t + 1e-4));
            before != after && after.is_some()
        })
        .collect();
    cuts.dedup_by(|a, b| (*a - *b).abs() < 0.02);
    cuts
}

/// One second of a timeline from the renderer's text boxes at that time (`render_text_boxes` JSON): visible text
/// elements at least 2% of the frame tall, not nested inside another listed one.
pub fn second_from_boxes(t: u32, boxes: &Value) -> Second {
    let height = boxes["height"].as_f64().unwrap_or(1920.0).max(1.0);
    let list = boxes["boxes"].as_array().cloned().unwrap_or_default();
    let mut s = Second { t, ..Default::default() };
    let shown: Vec<&Value> = list
        .iter()
        .filter(|b| b["opacity"].as_f64().unwrap_or(1.0) >= 0.1 && b["visibleFraction"].as_f64().unwrap_or(1.0) >= 0.5)
        .filter(|b| b["h"].as_f64().unwrap_or(0.0) / height >= 0.02)
        .collect();
    // A highlighted word inside a caption is listed as well as the caption: count the outermost box only (of two
    // boxes that fill the same rectangle, the first).
    let inside = |a: &Value, b: &Value| {
        let (ax, ay, aw, ah) = (a["x"].as_f64().unwrap_or(0.0), a["y"].as_f64().unwrap_or(0.0), a["w"].as_f64().unwrap_or(0.0), a["h"].as_f64().unwrap_or(0.0));
        let (bx, by, bw, bh) = (b["x"].as_f64().unwrap_or(0.0), b["y"].as_f64().unwrap_or(0.0), b["w"].as_f64().unwrap_or(0.0), b["h"].as_f64().unwrap_or(0.0));
        ax >= bx - 0.5 && ay >= by - 0.5 && ax + aw <= bx + bw + 0.5 && ay + ah <= by + bh + 0.5
    };
    let outer = |i: usize| !(0..shown.len()).any(|j| j != i && inside(shown[i], shown[j]) && (j < i || !inside(shown[j], shown[i])));
    for b in (0..shown.len()).filter(|i| outer(*i)).map(|i| shown[i]) {
        let (y, h) = (b["y"].as_f64().unwrap_or(0.0), b["h"].as_f64().unwrap_or(0.0));
        s.text = true;
        s.words += b["words"].as_u64().unwrap_or_else(|| b["text"].as_str().unwrap_or("").split_whitespace().count() as u64) as u32;
        s.text_y.push(round3((y + h / 2.0) / height));
        s.text_h.push(round3(h / height));
    }
    s
}

/// Measures a video file the way edit_anatomy.py does: ffmpeg's scene detector for cuts (at 240 px wide), EBU R128
/// loudness and silences under -40 dB for 0.4 s, and one frame a second (at most 90, 360 px) read by a Vision helper
/// for on-screen text and faces (`vision`: knowledge/drive/tools/frames-vision.swift built; without it text and faces
/// are left unknown). `keyframes`: also keep the hook frame and five more spread over the ad there (for the open
/// image embedding, docs/ad-card.feature).
pub fn from_video(path: &std::path::Path, ffmpeg: &std::path::Path, vision: Option<&std::path::Path>, keyframes: Option<&std::path::Path>) -> Result<Card, String> {
    use std::process::Command;
    let run = |args: &[&str]| -> String {
        Command::new(ffmpeg).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string() + String::from_utf8_lossy(&o.stderr).as_ref()).unwrap_or_default()
    };
    let f = path.to_str().ok_or("path isn't UTF-8")?;
    if !path.exists() {
        return Err(format!("{f}: not found"));
    }
    let info = run(&["-hide_banner", "-i", f]);
    let duration = info.split("Duration: ").nth(1).and_then(|r| {
        let hms: Vec<f64> = r.split(',').next()?.split(':').map(|x| x.trim().parse::<f64>().ok()).collect::<Option<_>>()?;
        (hms.len() == 3).then(|| hms[0] * 3600.0 + hms[1] * 60.0 + hms[2])
    }).ok_or(format!("{f}: no duration (not a video?)"))?;
    // "Stream #0:0: Video: h264 (High), yuv420p(tv, bt709), 1080x1920 [SAR 1:1 DAR 9:16], …"
    let size = info.lines().find(|l| l.contains("Video:")).and_then(|l| {
        l.split(|c: char| c == ',' || c == ' ').find_map(|tok| {
            let (a, b) = tok.split_once('x')?;
            Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))
        })
    });
    let rotated = info.lines().any(|l| (l.contains("rotation of") || l.trim_start().starts_with("rotate")) && l.contains("90"));
    let (mut w, mut h) = size.map(|(a, b)| (Some(a), Some(b))).unwrap_or((None, None));
    if rotated {
        std::mem::swap(&mut w, &mut h);
    }
    // One decode, two readings: every frame's scene score at 240 px (the fixed 0.32 threshold, the same cuts the editing
    // profile was built on), and at 480 px with the frame-to-frame brightness change for the sensitive detector.
    let dir = std::env::temp_dir().join(format!("scenes-{}-{}", std::process::id(), crate::media::hash(f)));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (fa, fb) = (dir.join("a.txt"), dir.join("b.txt"));
    let graph = format!("[0:v]split[a][b];[a]scale=240:-2,select='gte(scene\\,0)',metadata=print:file={}[oa];[b]scale=480:-2,select='gte(scene\\,0)',signalstats,metadata=print:file={}[ob]",
                        fa.display(), fb.display());
    run(&["-hide_banner", "-hwaccel", "videotoolbox", "-i", f, "-an", "-filter_complex", &graph, "-map", "[oa]", "-f", "null", "-", "-map", "[ob]", "-f", "null", "-"]);
    let scores = scene_scores(&std::fs::read_to_string(&fa).unwrap_or_default());
    let mut cuts: Vec<f64> = scores.iter().filter(|(_, s)| *s > 0.32).map(|(t, _)| (t * 100.0).round() / 100.0).collect();
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    cuts.dedup();
    let signals = frame_signals(&std::fs::read_to_string(&fb).unwrap_or_default());
    let shot_changes = sensitive_shot_changes(&signals);
    let stills = still_spans(&signals, 0.5, 1.0);
    let _ = std::fs::remove_dir_all(&dir);
    let audio = run(&["-hide_banner", "-i", f, "-vn", "-af", "ebur128=framelog=quiet,silencedetect=n=-40dB:d=0.4", "-f", "null", "-"]);
    let lufs = audio.lines().find(|l| l.trim_start().starts_with("I:") && l.trim_end().ends_with("LUFS")).and_then(|l| numbers_after(l, "I:").first().copied());
    let lra = audio.lines().find(|l| l.trim_start().starts_with("LRA:") && l.trim_end().ends_with(" LU")).and_then(|l| numbers_after(l, "LRA:").first().copied());
    let silence: f64 = numbers_after(&audio, "silence_duration:").iter().sum();
    let dead_air = overlaps(&stills, &silence_spans(&audio, duration), 1.0);
    let mut card = Card {
        source: "video".into(),
        duration,
        w,
        h,
        cuts,
        lufs,
        lra,
        silence_s: Some((silence * 100.0).round() / 100.0),
        has_audio: audio.contains("Audio:"),
        seconds: vec![],
        faces_known: false,
        shot_changes,
        stills,
        dead_air,
        performance: None,
    };
    if vision.is_none() && keyframes.is_none() {
        return Ok(card);
    }
    if let (None, Some(dir)) = (vision, keyframes) {
        // keyframes only: seek straight to the hook and five more spread over the ad (named by their second, as the
        // one-a-second pass names them) instead of decoding a frame every second
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let last = (duration.floor() as usize).saturating_sub(1).min(89);
        let mut secs: Vec<usize> = std::iter::once(0).chain((1..6).map(|i| ((i * last) as f64 / 5.0).round() as usize)).collect();
        secs.sort();
        secs.dedup();
        for sec in secs {
            let out = dir.join(format!("{sec:03}.jpg"));
            run(&["-hide_banner", "-v", "error", "-ss", &format!("{}", sec as f64 + 0.5), "-i", f, "-frames:v", "1", "-vf", "scale=360:-2", "-q:v", "5", out.to_str().unwrap_or_default()]);
        }
        return Ok(card);
    }
    let tmp = std::env::temp_dir().join(format!("measure-{}-{}", std::process::id(), crate::media::hash(f)));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let pattern = tmp.join("%04d.jpg");
    run(&["-hide_banner", "-v", "error", "-i", f, "-vf", "fps=1,scale=360:-2", "-frames:v", "90", "-q:v", "5", pattern.to_str().unwrap()]);
    let mut frames: Vec<std::path::PathBuf> = std::fs::read_dir(&tmp).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    frames.sort();
    if let Some(vision) = vision {
        for chunk in frames.chunks(30) {
            let out = Command::new(vision).args(chunk).env_remove("VISION_PRINT").output().map_err(|e| format!("{}: {e}", vision.display()))?;
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let Ok(j) = serde_json::from_str::<Value>(line) else { continue };
                let t = j["frame"].as_str().and_then(|n| n.trim_end_matches(".jpg").parse::<u32>().ok()).unwrap_or(1).saturating_sub(1);
                let lines = j["text"].as_array().cloned().unwrap_or_default();
                let big: Vec<&Value> = lines.iter().filter(|x| x["h"].as_f64().unwrap_or(0.0) >= 0.02).collect();
                card.seconds.push(Second {
                    t,
                    text: !big.is_empty(),
                    words: lines.iter().map(|x| x["s"].as_str().unwrap_or("").split_whitespace().count() as u32).sum(),
                    text_y: big.iter().map(|x| round3(x["y"].as_f64().unwrap_or(0.0) + x["h"].as_f64().unwrap_or(0.0) / 2.0)).collect(),
                    text_h: big.iter().map(|x| x["h"].as_f64().unwrap_or(0.0)).collect(),
                    face: j["faces"].as_array().map(|fs| fs.iter().map(|x| x["w"].as_f64().unwrap_or(0.0) * x["h"].as_f64().unwrap_or(0.0)).fold(0.0, f64::max)).unwrap_or(0.0),
                });
            }
        }
        card.faces_known = true;
    }
    if let Some(dir) = keyframes {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let n = frames.len();
        let mut pick: Vec<usize> = std::iter::once(0).chain((1..6).map(|i| ((i * (n.max(1) - 1)) as f64 / 5.0).round() as usize)).collect();
        pick.sort();
        pick.dedup();
        for i in pick.into_iter().filter(|i| *i < n) {
            std::fs::copy(&frames[i], dir.join(format!("{i:03}.jpg"))).map_err(|e| e.to_string())?;
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(card)
}

/// (time, scene score) for every frame of ffmpeg's `metadata=print` output (the 240 px reading).
fn scene_scores(text: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut t = None;
    for line in text.lines() {
        if let Some(x) = numbers_after(line, "pts_time:").first() {
            t = Some(*x);
        } else if let (Some(tt), Some(sc)) = (t, line.trim().strip_prefix("lavfi.scene_score=").and_then(|v| v.parse::<f64>().ok())) {
            out.push((tt, sc));
            t = None;
        }
    }
    out
}

/// Spans where the luma change stays under `max_ydif` for `min_len` seconds or more, from per-frame signals.
pub fn still_spans(frames: &[(f64, f64, f64)], max_ydif: f64, min_len: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let mut cur: Option<[f64; 2]> = None;
    for &(t, _, yd) in frames {
        if yd < max_ydif {
            cur = Some(cur.map(|c| [c[0], t]).unwrap_or([t, t]));
        } else if let Some(c) = cur.take() {
            if c[1] - c[0] >= min_len { out.push(round2(c)); }
        }
    }
    if let Some(c) = cur { if c[1] - c[0] >= min_len { out.push(round2(c)); } }
    out
}

/// silencedetect's spans (silence_start … silence_end); one still open at the end runs to the end.
fn silence_spans(log: &str, duration: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let mut start: Option<f64> = None;
    for line in log.lines() {
        if let Some(s) = numbers_after(line, "silence_start:").first() {
            start = Some(*s);
        } else if let (Some(e), Some(s)) = (numbers_after(line, "silence_end:").first(), start) {
            out.push([s, *e]);
            start = None;
        }
    }
    if let Some(s) = start { out.push([s, duration]); }
    out
}

/// Where two lists of spans overlap for `min_len` seconds or more.
pub fn overlaps(a: &[[f64; 2]], b: &[[f64; 2]], min_len: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for x in a {
        for y in b {
            let (s, e) = (x[0].max(y[0]), x[1].min(y[1]));
            if e - s >= min_len { out.push(round2([s, e])); }
        }
    }
    out
}

fn round2(c: [f64; 2]) -> [f64; 2] {
    [(c[0] * 100.0).round() / 100.0, (c[1] * 100.0).round() / 100.0]
}

/// (time, scene score, luma change) for every frame of a `select…,signalstats,metadata=print` reading.
fn frame_signals(text: &str) -> Vec<(f64, f64, f64)> {
    let mut out: Vec<(f64, f64, f64)> = Vec::new();
    for line in text.lines() {
        if let Some(x) = numbers_after(line, "pts_time:").first() {
            out.push((*x, 0.0, 0.0));
        } else if let Some(last) = out.last_mut() {
            let l = line.trim();
            if let Some(v) = l.strip_prefix("lavfi.scene_score=").and_then(|v| v.parse().ok()) {
                last.1 = v;
            } else if let Some(v) = l.strip_prefix("lavfi.signalstats.YDIF=").and_then(|v| v.parse().ok()) {
                last.2 = v;
            }
        }
    }
    out
}

/// Shot changes, sensitively, from (time, scene score at 480 px, luma change) per frame: any frame over 0.32; a scene
/// score of 0.12+ standing 6 times over the median of the twelve frames either side; or a luma change of 8+ standing
/// 8 times over its median. The strongest is kept within a quarter second. Fitted on 40 of Buddy's renders whose cut
/// times are known (2026-10-12): on the 20 held out, precision 0.90 and recall 0.65 for visible cuts, against 0.86 and
/// 0.40 for the fixed threshold alone (knowledge/drive/bin/shot_detector_fit.py).
pub fn sensitive_shot_changes(frames: &[(f64, f64, f64)]) -> Vec<f64> {
    let mut picked: Vec<(f64, f64)> = Vec::new();
    for (i, &(t, sc, yd)) in frames.iter().enumerate() {
        let lo = i.saturating_sub(12);
        let hi = (i + 13).min(frames.len());
        let others = |k: usize| frames[lo..hi].iter().enumerate().filter(move |(j, _)| lo + j != i).map(move |(_, x)| if k == 1 { x.1 } else { x.2 }).collect::<Vec<f64>>();
        let (ms, my) = (median(&others(1)).unwrap_or(0.0), median(&others(2)).unwrap_or(0.0));
        if sc > 0.32 || (sc >= 0.12 && sc >= 6.0 * ms.max(1e-4)) || (yd >= 8.0 && yd >= 8.0 * my.max(1e-3)) {
            let strength = sc + yd / 100.0;
            match picked.last_mut() {
                Some(last) if t - last.0 < 0.25 => {
                    if strength > last.1 {
                        *last = (t, strength);
                    }
                }
                _ => picked.push((t, strength)),
            }
        }
    }
    picked.into_iter().map(|(t, _)| (t * 100.0).round() / 100.0).collect()
}

/// Every number that follows `key` in `text` ("pts_time:12.3" → 12.3).
fn numbers_after(text: &str, key: &str) -> Vec<f64> {
    text.split(key).skip(1).filter_map(|r| {
        let r = r.trim_start();
        let end = r.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(r.len());
        r[..end].parse::<f64>().ok()
    }).collect()
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Measured norms: the editing profile (percentiles by length band and shape) and the editing styles (centroids of
/// the structure vectors). Both are aggregates; neither holds an ad.
#[derive(Clone, Debug, Default)]
pub struct Norms {
    pub profile: Value,
    pub styles: Value,
}

/// What each summary number is called in a note, its unit, and whether it's worth a note at all.
const NOTED: [(&str, &str, &str); 8] = [
    ("first_cut_s", "The first shot change comes at", "s"),
    ("cuts_per_10s", "Pacing is", " shot changes every 10 s"),
    ("median_shot_s", "The typical shot lasts", "s"),
    ("text_share", "On-screen text shows for", "% of the ad"),
    ("text_center_y", "Text sits at", "% down the frame"),
    ("words_on_screen", "Text on screen runs to", " words at a time"),
    ("face_share", "A face is on screen for", "% of the ad"),
    ("lufs", "Loudness is", " LUFS"),
];

impl Norms {
    pub fn from_json(profile: &str, styles: &str) -> Result<Norms, String> {
        Ok(Norms {
            profile: serde_json::from_str(profile).map_err(|e| format!("editing profile: {e}"))?,
            styles: serde_json::from_str(styles).map_err(|e| format!("editing styles: {e}"))?,
        })
    }

    /// The most specific group with at least 30 ads: length band and shape, then band, then shape, then all.
    fn group(&self, card: &Card) -> Option<(String, &Value)> {
        let (band, shape) = (card.band(), card.shape());
        let groups = &self.profile["groups"];
        [format!("band:{band}|shape:{shape}"), format!("band:{band}"), format!("shape:{shape}"), "all".to_string()]
            .into_iter()
            .find(|k| groups[k]["n"].as_u64().unwrap_or(0) >= 30)
            .map(|k| {
                let v = &groups[&k];
                (k, v)
            })
    }

    /// Advisory notes where the card falls outside the middle 80% (p10–p90) of ads of its kind, in plain words.
    pub fn notes(&self, card: &Card) -> Vec<Value> {
        let Some((key, group)) = self.group(card) else { return vec![] };
        let kind = describe_group(&key);
        let summary = card.summary();
        let mut out = Vec::new();
        for (name, label, unit) in NOTED {
            let (Some(Some(v)), Some(p10), Some(p50), Some(p90)) =
                (summary.get(name), group[name]["p10"].as_f64(), group[name]["p50"].as_f64(), group[name]["p90"].as_f64())
            else {
                continue;
            };
            if *v >= p10 && *v <= p90 {
                continue;
            }
            let pct = unit.starts_with('%');
            let f = |x: f64| if pct { format!("{:.0}", x * 100.0) } else if name == "lufs" { format!("{x:.0}") } else { format!("{x:.1}") };
            let side = if *v > p90 { "higher" } else { "lower" };
            out.push(json!({
                "measure": name,
                "value": v,
                "usual": p50,
                "range": [p10, p90],
                "side": side,
                "group": key,
                "note": format!("{label} {}{unit}; in {kind} it's usually {}{unit} (most between {} and {}).", f(*v), f(p50), f(p10), f(p90)),
            }));
        }
        out
    }

    /// The nearest editing styles to the card's structure vector, and how far it is from Kelly's centre.
    pub fn style(&self, card: &Card) -> Option<Value> {
        let v = card.vector()?;
        let mean: Vec<f64> = self.styles["mean"].as_array()?.iter().filter_map(|x| x.as_f64()).collect();
        let std: Vec<f64> = self.styles["std"].as_array()?.iter().filter_map(|x| x.as_f64()).collect();
        if mean.len() != v.len() || std.len() != v.len() {
            return None;
        }
        let z = |x: &[f64]| -> Vec<f64> { x.iter().zip(&mean).zip(&std).map(|((x, m), s)| (x - m) / s.max(1e-9)).collect() };
        let dist = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt();
        let me = z(&v);
        let mut near: Vec<(f64, &Value)> = self.styles["styles"]
            .as_array()?
            .iter()
            .filter_map(|s| {
                let c: Vec<f64> = FEATURES.iter().map(|n| s["centroid"][*n].as_f64()).collect::<Option<_>>()?;
                Some((dist(&me, &z(&c)), s))
            })
            .collect();
        near.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let kelly: Option<Vec<f64>> = self.styles["kelly_centre_z"].as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).collect());
        let pick = |(d, s): &(f64, &Value)| json!({"style": s["style"], "describe": s["describe"], "distance": round3(*d), "ads": s["ads"],
                                                     "median_engagement_rate": s["median_engagement_rate"]});
        Some(json!({
            "nearest": near.first().map(pick),
            "next": near.get(1).map(pick),
            "distance_to_kelly_centre": kelly.filter(|k| k.len() == me.len()).map(|k| round3(dist(&me, &k))),
        }))
    }
}

fn describe_group(key: &str) -> String {
    let mut band = None;
    let mut shape = None;
    for part in key.split('|') {
        if let Some(b) = part.strip_prefix("band:") {
            band = Some(b.replace("-", "–").replace("s", " s"));
        }
        if let Some(s) = part.strip_prefix("shape:") {
            shape = Some(s.to_string());
        }
    }
    match (shape, band) {
        (Some(s), Some(b)) => format!("{s} {b} ads"),
        (None, Some(b)) => format!("{b} ads"),
        (Some(s), None) => format!("{s} ads"),
        _ => "ads like this".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> Card {
        Card {
            duration: 20.0,
            w: Some(1080.0),
            h: Some(1920.0),
            cuts: vec![0.1, 3.0, 6.5, 12.0, 19.9],
            lufs: Some(-18.0),
            silence_s: Some(2.0),
            seconds: (0..20).map(|t| Second { t, text: t % 2 == 0, words: 4, text_y: vec![0.5], text_h: vec![0.04], face: if t < 10 { 0.1 } else { 0.0 } }).collect(),
            faces_known: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_creators_performance_reads_from_perception_and_gives_the_hooks_eye_contact() {
        // What `ugc-perceive performance` prints (Perception.swift): derived numbers only.
        let json = r#"{"model": "mediapipe-face-landmarker-v2+mediapipe-hands", "fps": 15, "lip_sync_ms": 10, "lip_sync_confidence": 0.24,
            "seconds": [{"t": 0, "face": 1, "eye_contact": "lens", "lens_share": 0.9, "blink": false, "energy": 1.1, "speaking": 0.4, "reading": false},
                        {"t": 1, "face": 1, "eye_contact": "away", "gesture": "Thumb_Up"},
                        {"t": 2, "face": 0}, {"t": 3, "face": 1, "eye_contact": "near"}]}"#;
        let p: Performance = serde_json::from_str(json).unwrap();
        assert_eq!(p.lip_sync_ms, Some(10.0));
        assert_eq!(p.seconds[1].gesture.as_deref(), Some("Thumb_Up"));
        assert_eq!(p.hook_eye_contact(), Some(0.5), "the first 3 s with a face: lens, away");
        let card = Card { performance: Some(p), ..Default::default() };
        let back: Card = serde_json::from_str(&serde_json::to_string(&card).unwrap()).unwrap();
        assert_eq!(back.performance.unwrap().seconds.len(), 4);
        assert!(!serde_json::to_string(&Card::default()).unwrap().contains("performance"), "left out when not measured");
    }

    #[test]
    fn a_freeze_held_with_silence_is_dead_air() {
        // 30 frames a second: moving, then 1.5 s frozen (luma change 0), then moving
        let frames: Vec<(f64, f64, f64)> = (0..150).map(|i| {
            let t = i as f64 / 30.0;
            (t, 0.0, if (2.0..3.5).contains(&t) { 0.0 } else { 2.0 })
        }).collect();
        let stills = still_spans(&frames, 0.5, 1.0);
        assert_eq!(stills.len(), 1, "{stills:?}");
        assert!((stills[0][0] - 2.0).abs() < 0.05 && (stills[0][1] - 3.47).abs() < 0.05, "{stills:?}");
        let log = "[silencedetect @ 0x1] silence_start: 1.9\n[silencedetect @ 0x1] silence_end: 3.6 | silence_duration: 1.7\n";
        let silences = silence_spans(log, 5.0);
        assert_eq!(silences, vec![[1.9, 3.6]]);
        assert_eq!(overlaps(&stills, &silences, 1.0).len(), 1);
        // still but not silent (a held end card over music) isn't dead air
        assert!(overlaps(&stills, &[[0.0, 1.0]], 1.0).is_empty());
        // a short hold isn't a still
        assert!(still_spans(&frames, 0.5, 2.0).is_empty());
    }

    #[test]
    fn summary_matches_the_profile_script() {
        let s = card().summary();
        // cuts at 0.1 and 19.9 are the start and end, not cuts
        assert_eq!(s["cuts_per_10s"], Some(1.5));
        assert_eq!(s["first_cut_s"], Some(3.0));
        // shots 3, 3.5, 5.5, 8 → median 4.5
        assert_eq!(s["median_shot_s"], Some(4.5));
        assert_eq!(s["cuts_in_first_3s"], Some(1.0));
        assert_eq!(s["text_share"], Some(0.5));
        assert_eq!(s["face_share"], Some(0.5));
        assert_eq!(s["silence_share"], Some(0.1));
    }

    #[test]
    fn vector_matches_the_styles_script() {
        let v = card().vector().unwrap();
        assert_eq!(v.len(), 43);
        // slice 1 (2–4 s) holds the cut at 3.0: one cut in 2 s = 0.5 a second
        assert_eq!(v[1], 0.5);
        assert_eq!(v[10], 1.0 - 0.5);
        assert!((v[30] - 20f64.ln()).abs() < 1e-12);
        assert_eq!(v[33], 3.0);
        assert!((v[42] - 1920.0 / 1080.0).abs() < 1e-12);
        let mut no_secs = card();
        no_secs.seconds.clear();
        assert!(no_secs.vector().is_none());
    }

    #[test]
    fn timeline_cuts_are_where_the_top_clip_changes() {
        // a take under the whole ad, b-roll over it from 3 to 5, a second take from 8
        let cuts = timeline_cuts(&[(0.0, 8.0), (3.0, 5.0), (8.0, 12.0)]);
        assert_eq!(cuts, vec![3.0, 5.0, 8.0]);
        // two clips a millisecond apart are one cut
        assert_eq!(timeline_cuts(&[(0.0, 4.0), (4.0, 6.0), (4.001, 9.0)]), vec![4.0]);
    }

    #[test]
    fn nested_caption_words_count_once() {
        let boxes = json!({"height": 1920.0, "boxes": [
            {"x": 100.0, "y": 900.0, "w": 800.0, "h": 120.0, "opacity": 1.0, "visibleFraction": 1.0, "words": 5, "text": "this is the whole caption"},
            {"x": 300.0, "y": 910.0, "w": 200.0, "h": 100.0, "opacity": 1.0, "visibleFraction": 1.0, "words": 1, "text": "whole"},
            {"x": 0.0, "y": 0.0, "w": 50.0, "h": 10.0, "opacity": 1.0, "visibleFraction": 1.0, "words": 1, "text": "tiny"},
            {"x": 0.0, "y": 1500.0, "w": 500.0, "h": 100.0, "opacity": 0.0, "visibleFraction": 1.0, "words": 2, "text": "faded out"}
        ]});
        let s = second_from_boxes(3, &boxes);
        assert!(s.text);
        assert_eq!(s.words, 5);
        assert_eq!(s.text_y, vec![round3(960.0 / 1920.0)]);
    }

    #[test]
    fn notes_flag_only_what_falls_outside_most_ads() {
        let profile = json!({"groups": {"band:15-30s|shape:vertical": {"n": 100,
            "first_cut_s": {"p10": 0.8, "p50": 2.0, "p90": 2.5}, "cuts_per_10s": {"p10": 1.0, "p50": 2.8, "p90": 5.0}}}});
        let n = Norms { profile, styles: json!({}) };
        let notes = n.notes(&card());
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert_eq!(notes[0]["measure"], "first_cut_s");
        assert_eq!(notes[0]["note"], "The first shot change comes at 3.0s; in vertical 15–30 s ads it's usually 2.0s (most between 0.8 and 2.5).");
    }

    #[test]
    fn ffmpeg_numbers_parse() {
        let log = "[Parsed_metadata_2] frame:3 pts:90 pts_time:3.003\nlavfi.scene_score=0.5\n frame:9 pts:270 pts_time:9.01\n";
        assert_eq!(numbers_after(log, "pts_time:"), vec![3.003, 9.01]);
        assert_eq!(numbers_after("    I:         -21.3 LUFS", "I:"), vec![-21.3]);
    }

    #[test]
    fn a_jump_cut_spike_counts_as_a_shot_change() {
        // steady noise; a jump cut at 2.0 s (scene 0.15, under the fixed 0.32), a hard cut at 4.0 s, and a jump cut at
        // 5.0 s seen only in the luma change (scene 0.03, luma 12 against ~1)
        let mut f: Vec<(f64, f64, f64)> = (0..240).map(|i| (i as f64 / 30.0, 0.01 + (i % 3) as f64 * 0.005, 1.0 + (i % 2) as f64 * 0.3)).collect();
        f[60].1 = 0.15;
        f[120].1 = 0.6;
        f[150] = (5.0, 0.03, 12.0);
        // a pan: raised but steady for a second never spikes
        for x in &mut f[180..210] { x.1 = 0.09; x.2 = 6.0; }
        assert_eq!(sensitive_shot_changes(&f), vec![2.0, 4.0, 5.0]);
    }

    #[test]
    fn corpus_rows_read_as_cards() {
        let row = r#"{"id": "ab", "drive": "x", "client": "own", "duration": 12.5, "w": 1080, "h": 1920, "cuts": [2.0],
                      "lufs": null, "lra": null, "silence_s": 0.0, "has_audio": true,
                      "seconds": [{"t": 0, "text": true, "words": 3, "text_y": [0.4], "text_h": [0.03], "face": 0.2}]}"#;
        let c: Card = serde_json::from_str(row).unwrap();
        assert_eq!(c.summary()["first_cut_s"], Some(2.0));
        assert!(c.faces_known);
    }
}
