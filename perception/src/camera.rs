//! A shot's camera (Perception.swift › cameraSummary): the move (still, handheld, pan, tilt, push-in, pull-out),
//! the shake, zoom or dolly, the lens as a 35 mm focal length, and the person's depth separation, from Depth
//! Anything 3's camera head over windows of four frames 0.5 s apart (rotations, camera centres, fx). The model and
//! Vision (which camera, the room vector) run elsewhere; this is the arithmetic over what they gave.

use serde::{Deserialize, Serialize};

/// One window of four frames: each frame's rotation (3 x 3 row-major), camera centre and fx; frame 0's separation.
#[derive(Clone, Debug, Deserialize)]
pub struct Window {
    pub rotations: Vec<Vec<f64>>,
    pub centres: Vec<Vec<f64>>,
    pub fx: Vec<f64>,
    pub separation: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShotCamera {
    #[serde(rename = "move")]
    pub movement: String,
    pub shake: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom_or_dolly: Option<String>,
    pub focal_mm: f64,
    pub lens: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separation: Option<f64>,
    pub pan_dps: f64,
    pub tilt_dps: f64,
    pub push: f64,
    pub travel: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<Vec<f64>>,
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// R_a · R_bᵀ for row-major 3 x 3s.
pub fn relative(a: &[f64], b: &[f64]) -> [f64; 9] {
    let mut out = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            let mut v = 0.0;
            for k in 0..3 {
                v += a[i * 3 + k] * b[j * 3 + k];
            }
            out[i * 3 + j] = v;
        }
    }
    out
}

pub fn angle(r: &[f64]) -> f64 {
    ((r[0] + r[4] + r[8] - 1.0) / 2.0).clamp(-1.0, 1.0).acos() * 180.0 / std::f64::consts::PI
}

/// A shot's camera from its windows, how many frames were read, the first frame's size, which camera and the room.
pub fn summary(windows: &[Window], images: usize, width: usize, height: usize, side: &str, room: Option<Vec<f64>>) -> ShotCamera {
    let (mut yaw, mut pitch) = (0.0f64, 0.0f64);
    let (mut jitter, mut forward, mut fx_ratio, mut travels): (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) = (vec![], vec![], vec![], vec![]);
    let (mut fx0, mut sep): (Option<f64>, Option<f64>) = (None, None);
    let deg = 180.0 / std::f64::consts::PI;
    for w in windows {
        if sep.is_none() {
            sep = w.separation;
        }
        if fx0.is_none() {
            fx0 = Some(w.fx[0]);
        }
        let rel = relative(&w.rotations[3], &w.rotations[0]);
        // about y (pan) and x (tilt), small-angle, in frame 0's axes (x right, y down)
        yaw += rel[2].atan2(rel[8]) * deg;
        pitch += (-rel[5]).atan2(rel[8]) * deg;
        // the middle frames against a steady turn: the jitter
        for k in 1..=2 {
            let mid = angle(&relative(&w.rotations[k], &w.rotations[0]));
            jitter.push((mid - angle(&rel) * k as f64 / 3.0).abs());
        }
        // the camera centre's move in frame 0's axes: z > 0 is toward the subject
        let d: Vec<f64> = (0..3).map(|j| w.centres[3][j] - w.centres[0][j]).collect();
        let r0 = &w.rotations[0];
        let mut local = [0.0f64; 3];
        for j in 0..3 {
            for k in 0..3 {
                local[j] += r0[j * 3 + k] * d[k];
            }
        }
        let norm = local.iter().fold(0.0, |s, v| s + v * v).sqrt().max(1e-9);
        forward.push(local[2] / norm * (norm * 10.0).min(1.0));
        travels.push((norm * 10.0).min(1.0));
        fx_ratio.push(w.fx[3] / w.fx[0].max(1e-6));
    }
    let seconds = (images as f64 - 1.0) * 0.5;
    let shake = r2(if jitter.is_empty() { 0.0 } else { (jitter.iter().fold(0.0, |s, v| s + v * v) / jitter.len() as f64).sqrt() });
    let zoom = fx_ratio.iter().any(|r| (r - 1.0).abs() > 0.04);
    let push = forward.iter().fold(0.0, |s, v| s + v) / forward.len().max(1) as f64;
    let travel = travels.iter().fold(0.0, |s, v| s + v) / travels.len().max(1) as f64;
    let (pan, tilt) = (yaw / seconds, pitch / seconds);
    // thresholds fitted to Gemini's labels on 50 library shots (T-334)
    let movement = if pan.abs() >= 2.0 && pan.abs() >= tilt.abs() {
        "pan"
    } else if tilt.abs() >= 2.0 {
        "tilt"
    } else if !zoom && travel >= 0.3 && push.abs() >= 0.2 {
        if push > 0.0 { "push-in" } else { "pull-out" }
    } else if shake < 0.8 {
        "still"
    } else {
        "handheld"
    };
    // CGFloat on Apple's 64-bit platforms is a Double
    let scale = (504.0 / width as f64).min(504.0 / height as f64);
    let f_src = fx0.unwrap_or(400.0) / scale;
    let mm = 43.27 * f_src / (width as f64).hypot(height as f64);
    ShotCamera {
        movement: movement.into(),
        shake,
        zoom_or_dolly: if zoom { Some("zoom".into()) } else if push.abs() > 0.5 { Some("dolly".into()) } else { None },
        focal_mm: (mm * 10.0).round() / 10.0,
        lens: if mm < 20.0 { "wide" } else if mm > 40.0 { "tele" } else { "standard" }.into(),
        side: Some(side.into()),
        separation: sep,
        pan_dps: r2(pan),
        tilt_dps: r2(tilt),
        push: r2(push),
        travel: r2(travel),
        room,
    }
}
