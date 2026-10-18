//! The creator's performance second by second (Perception.swift › look, summarize, lipSync): eye contact in three
//! levels, blinks, energy against the clip's own median, speaking (a voice and a moving mouth), reading off camera,
//! the lip-sync offset, and the gesture held. Each frame's face mesh (478 points, 52 expression scores) is reduced to
//! a Look and dropped; only derived numbers leave. The mesh model and the audio decode run elsewhere.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// MediaPipe's 52 expression scores, in the model's order.
pub const BLENDSHAPES: [&str; 52] = [
    "_neutral", "browDownLeft", "browDownRight", "browInnerUp", "browOuterUpLeft", "browOuterUpRight", "cheekPuff",
    "cheekSquintLeft", "cheekSquintRight", "eyeBlinkLeft", "eyeBlinkRight", "eyeLookDownLeft", "eyeLookDownRight",
    "eyeLookInLeft", "eyeLookInRight", "eyeLookOutLeft", "eyeLookOutRight", "eyeLookUpLeft", "eyeLookUpRight",
    "eyeSquintLeft", "eyeSquintRight", "eyeWideLeft", "eyeWideRight", "jawForward", "jawLeft", "jawOpen", "jawRight",
    "mouthClose", "mouthDimpleLeft", "mouthDimpleRight", "mouthFrownLeft", "mouthFrownRight", "mouthFunnel", "mouthLeft",
    "mouthLowerDownLeft", "mouthLowerDownRight", "mouthPressLeft", "mouthPressRight", "mouthPucker", "mouthRight",
    "mouthRollLower", "mouthRollUpper", "mouthShrugLower", "mouthShrugUpper", "mouthSmileLeft", "mouthSmileRight",
    "mouthStretchLeft", "mouthStretchRight", "mouthUpperUpLeft", "mouthUpperUpRight", "noseSneerLeft", "noseSneerRight",
];

const EYE_YAW_DEGREES: f64 = 25.0;
const EYE_PITCH_DEGREES: f64 = 20.0;
const GAZE_YAW_OFFSET: f64 = -6.0;
const GAZE_PITCH_OFFSET: f64 = -6.0;
const TOWARD_YAW_CONE: f64 = 16.0;
const TOWARD_PITCH_CONE: f64 = 20.0;
const LENS_YAW_CONE: f64 = 13.0;
const LENS_PITCH_CONE: f64 = 13.0;
const READING_YAW_OFFSET: f64 = -6.0;
const READING_PITCH_OFFSET: f64 = -6.0;
const READING_YAW_CONE: f64 = 13.0;
const READING_PITCH_CONE: f64 = 13.0;
const LIP_SYNC_BIAS_MS: f64 = 0.0;

/// One frame's read of the face, already reduced.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub t: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub eyes: f64,
    pub blink: bool,
    pub mouth: f64,
    pub motion: f64,
    pub head_yaw: f64,
    pub head_pitch: f64,
    pub up: f64,
    pub iris_x: f64,
    pub iris_y: f64,
    pub nose_x: f64,
    pub nose_y: f64,
}

fn idx(name: &str) -> usize {
    BLENDSHAPES.iter().position(|n| *n == name).unwrap_or(0)
}

/// One frame's face (points x, y, z; the 52 scores) reduced to a Look; `last` is the previous frame's scores.
pub fn look(p: &[Vec<f64>], s: &[f32], t: f64, last: Option<&[f32]>) -> Look {
    let v = |name: &str| s[idx(name)] as f64;
    let deg = 180.0 / std::f64::consts::PI;
    let yaw_head = (p[454][2] - p[234][2]).atan2(p[454][0] - p[234][0]) * deg;
    let pitch_head = (p[152][2] - p[10][2]).atan2(p[152][1] - p[10][1]) * deg;
    let eyes = (v("eyeLookOutLeft") + v("eyeLookInRight") - v("eyeLookInLeft") - v("eyeLookOutRight")) / 2.0;
    let up = (v("eyeLookUpLeft") + v("eyeLookUpRight") - v("eyeLookDownLeft") - v("eyeLookDownRight")) / 2.0;
    let blinks = [idx("eyeBlinkLeft"), idx("eyeBlinkRight")];
    let motion = last.map_or(0.0, |l| (0..s.len()).fold(0.0, |acc, i| if blinks.contains(&i) { acc } else { acc + ((s[i] - l[i]) as f64).abs() }));
    let gap = |a: usize, b: usize| (p[a][0] - p[b][0]).hypot(p[a][1] - p[b][1]);
    let eyes_width = gap(33, 263).max(1.0);
    let along = |iris: usize, a: usize, b: usize| {
        let (dx, dy) = (p[b][0] - p[a][0], p[b][1] - p[a][1]);
        ((p[iris][0] - p[a][0]) * dx + (p[iris][1] - p[a][1]) * dy) / (dx * dx + dy * dy).max(1.0) - 0.5
    };
    let between = |iris: usize, upper: usize, lower: usize| (p[iris][1] - p[upper][1]) / (p[lower][1] - p[upper][1]).max(0.5) - 0.5;
    let (ex, ey) = ((p[33][0] + p[133][0] + p[362][0] + p[263][0]) / 4.0, (p[33][1] + p[133][1] + p[362][1] + p[263][1]) / 4.0);
    Look {
        t,
        yaw: yaw_head + eyes * EYE_YAW_DEGREES,
        pitch: -pitch_head + up * EYE_PITCH_DEGREES,
        eyes,
        blink: v("eyeBlinkLeft") > 0.5 && v("eyeBlinkRight") > 0.5,
        mouth: gap(13, 14) / eyes_width,
        motion,
        head_yaw: yaw_head,
        head_pitch: -pitch_head,
        up,
        iris_x: (along(468, 33, 133) + along(473, 362, 263)) / 2.0,
        iris_y: (between(468, 159, 145) + between(473, 386, 374)) / 2.0,
        nose_x: (p[1][0] - ex) / eyes_width,
        nose_y: (p[1][1] - ey) / (p[152][1] - ey).max(1.0),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Second {
    pub t: i64,
    pub face: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eye_contact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens_share: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blink: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub energy: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaking: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gesture: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Performance {
    pub model: String,
    pub fps: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lip_sync_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lip_sync_confidence: Option<f64>,
    pub seconds: Vec<Second>,
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

pub fn median(x: &[f64]) -> f64 {
    let mut s = x.to_vec();
    s.sort_by(f64::total_cmp);
    if s.is_empty() { 0.0 } else { s[s.len() / 2] }
}

/// Seconds from the frames' Looks and the sound's level every 10 ms.
pub fn summarize(looks: &[Look], frame_times: &[f64], fps: f64, duration: f64, loudness: &[f32]) -> Performance {
    let seconds_count = ((duration - 1e-6).ceil() as i64).max(1);
    // voice: louder than the quiet part of the clip by 10 dB, and not silence
    let mut sorted = loudness.to_vec();
    sorted.sort_by(f32::total_cmp);
    let floor = if sorted.is_empty() { 0.0 } else { sorted[sorted.len() / 5] };
    let voiced: Vec<bool> = loudness.iter().map(|v| *v > (floor * 3.16f32).max(0.003)).collect();
    let voice_at = |t: f64| {
        let i = (t * 100.0).round() as i64;
        ((i - 5).max(0)..=(i + 5).max(0)).any(|k| (k as usize) < voiced.len() && voiced[k as usize])
    };
    // the mouth moving: its change against the frame before, over a small threshold
    let mut mouth_moving: HashMap<u64, bool> = HashMap::new();
    for w in looks.windows(2) {
        if w[1].t - w[0].t < 2.5 / fps {
            mouth_moving.insert(w[1].t.to_bits(), (w[1].mouth - w[0].mouth).abs() > 0.015);
        }
    }
    let per_second_motion: Vec<f64> = (0..seconds_count)
        .map(|s| {
            let m: Vec<f64> = looks.iter().filter(|l| l.t as i64 == s && l.motion > 0.0).map(|l| l.motion).collect();
            if m.is_empty() { 0.0 } else { m.iter().fold(0.0, |a, b| a + b) / m.len() as f64 }
        })
        .collect();
    let typical = median(&per_second_motion.iter().copied().filter(|m| *m > 0.0).collect::<Vec<_>>());
    let mut seconds = Vec::new();
    for s in 0..seconds_count {
        let in_second: Vec<&Look> = looks.iter().filter(|l| l.t as i64 == s).collect();
        let frames_in_second = frame_times.iter().filter(|t| **t as i64 == s).count();
        let mut second = Second { t: s, face: if frames_in_second == 0 { 0.0 } else { r2(in_second.len() as f64 / frames_in_second as f64) }, ..Default::default() };
        if !in_second.is_empty() {
            let mut yaws: Vec<f64> = in_second.iter().map(|l| l.yaw).collect();
            let mut pitches: Vec<f64> = in_second.iter().map(|l| l.pitch).collect();
            yaws.sort_by(f64::total_cmp);
            pitches.sort_by(f64::total_cmp);
            let (dy, dp) = ((yaws[yaws.len() / 2] - GAZE_YAW_OFFSET).abs(), (pitches[pitches.len() / 2] - GAZE_PITCH_OFFSET).abs());
            second.eye_contact = Some(
                if dy <= LENS_YAW_CONE && dp <= LENS_PITCH_CONE {
                    "lens"
                } else if dy <= TOWARD_YAW_CONE && dp <= TOWARD_PITCH_CONE {
                    "near"
                } else {
                    "away"
                }
                .into(),
            );
            let in_lens = in_second.iter().filter(|l| (l.yaw - GAZE_YAW_OFFSET).abs() <= LENS_YAW_CONE && (l.pitch - GAZE_PITCH_OFFSET).abs() <= LENS_PITCH_CONE).count();
            second.lens_share = Some(r2(in_lens as f64 / in_second.len() as f64));
            let at_camera = in_second
                .iter()
                .filter(|l| (l.yaw - READING_YAW_OFFSET).abs() <= READING_YAW_CONE && (l.pitch - READING_PITCH_OFFSET).abs() <= READING_PITCH_CONE)
                .count();
            second.blink = Some(in_second.iter().any(|l| l.blink));
            if typical > 0.0 && per_second_motion[s as usize] > 0.0 {
                second.energy = Some(r2(per_second_motion[s as usize] / typical));
            }
            if !loudness.is_empty() {
                let talking = in_second.iter().filter(|l| voice_at(l.t) && *mouth_moving.get(&l.t.to_bits()).unwrap_or(&false)).count();
                second.speaking = Some(r2(talking as f64 / in_second.len() as f64));
            }
            // reading: mostly off the lens, the eyes sweeping across, turning back at least three times
            let eyes: Vec<f64> = in_second.iter().map(|l| l.eyes).collect();
            let turns = eyes.windows(3).filter(|w| (w[1] - w[0]) * (w[2] - w[1]) < 0.0 && (w[1] - w[0]).abs() > 0.03).count();
            // in_second isn't empty here, so neither is eyes
            let range = eyes.iter().copied().fold(f64::MIN, f64::max) - eyes.iter().copied().fold(f64::MAX, f64::min);
            second.reading = Some((at_camera as f64 / in_second.len() as f64) < 0.2 && range > 0.5 && turns >= 3);
        }
        seconds.push(second);
    }
    let (lag, confidence) = lip_sync(looks, loudness, fps);
    Performance {
        model: "mediapipe-face-landmarker-v2".into(),
        fps,
        lip_sync_ms: lag.map(|l| (l - LIP_SYNC_BIAS_MS).round()),
        lip_sync_confidence: confidence.map(r2),
        seconds,
    }
}

/// The lag (ms, -500..500) at which the sound's loudness best follows the mouth's opening, and the correlation there.
pub fn lip_sync(looks: &[Look], loudness: &[f32], fps: f64) -> (Option<f64>, Option<f64>) {
    if looks.len() < (fps * 3.0) as usize || loudness.len() < 300 {
        return (None, None);
    }
    let end = (loudness.len() as f64 / 100.0).min(looks.last().map_or(0.0, |l| l.t));
    let n = (end * 100.0) as i64;
    if n <= 300 {
        return (None, None);
    }
    let n = n as usize;
    // the mouth at 100 a second, linear between frames (gaps over 0.3 s left out)
    let mut jaw = vec![f64::NAN; n];
    for w in looks.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if b.t - a.t >= 0.3 {
            continue;
        }
        let i0 = (a.t * 100.0).round() as i64;
        let i1 = ((b.t * 100.0).round() as i64).min(n as i64 - 1);
        if !(i0 < n as i64 && i1 > i0) {
            continue;
        }
        for i in i0..=i1 {
            jaw[i as usize] = a.mouth + (b.mouth - a.mouth) * (i - i0) as f64 / (i1 - i0) as f64;
        }
    }
    let level: Vec<f64> = (0..n).map(|i| (loudness[i] as f64 + 1e-4).ln()).collect();
    let detrend = |x: &[f64]| -> Vec<f64> {
        let mut out = x.to_vec();
        for i in 0..x.len() {
            let w: Vec<f64> = x[i.saturating_sub(50)..(i + 51).min(x.len())].iter().copied().filter(|v| !v.is_nan()).collect();
            out[i] = if x[i].is_nan() || w.is_empty() { f64::NAN } else { x[i] - w.iter().fold(0.0, |a, b| a + b) / w.len() as f64 };
        }
        out
    };
    let (j, l) = (detrend(&jaw), detrend(&level));
    let mut best: Option<(i64, f64)> = None;
    for lag in -50i64..=50 {
        let (mut sxy, mut sxx, mut syy, mut count) = (0.0, 0.0, 0.0, 0usize);
        for i in 0..n as i64 {
            let k = i + lag;
            if k < 0 || k >= n as i64 || j[i as usize].is_nan() || l[k as usize].is_nan() {
                continue;
            }
            let (x, y) = (j[i as usize], l[k as usize]);
            sxy += x * y;
            sxx += x * x;
            syy += y * y;
            count += 1;
        }
        if !(count > 200 && sxx > 0.0 && syy > 0.0) {
            continue;
        }
        let r = sxy / (sxx * syy).sqrt();
        if best.is_none_or(|b| r > b.1) {
            best = Some((lag, r));
        }
    }
    match best {
        Some((lag, r)) if r > 0.0 => (Some((lag * 10) as f64), Some(r)),
        _ => (None, None),
    }
}

/// A gesture counts in a second when it's read in at least two of that second's hand reads.
pub fn add_gestures(p: &mut Performance, gestures: &[(f64, String)], hands: bool) {
    for second in p.seconds.iter_mut() {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for (t, g) in gestures {
            if *t as i64 == second.t {
                match counts.iter_mut().find(|c| c.0 == g) {
                    Some(c) => c.1 += 1,
                    None => counts.push((g, 1)),
                }
            }
        }
        if let Some(top) = counts.iter().fold(None::<&(&str, usize)>, |b, c| if b.is_none_or(|b| c.1 > b.1) { Some(c) } else { b }) {
            if top.1 >= 2 {
                second.gesture = Some(top.0.into());
            }
        }
    }
    if hands {
        p.model += "+mediapipe-hands";
    }
}
