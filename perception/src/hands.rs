//! Hands (Perception.swift › hands): MediaPipe's hand landmarker and gesture recognizer (Apache-2.0) converted to Core
//! ML: palms found on a 192 x 192 letterbox (kept over 0.5, best first, overlaps by IoU 0.3 dropped), each hand read
//! from a 224 x 224 crop turned so it points up, its 21 points embedded and classified into one of 8 gestures.

use crate::coreml::{Input, Model, Units};
use crate::picture;

pub struct Models {
    pub detector: Model,
    pub landmarks: Model,
    pub embedder: Model,
    pub classifier: Model,
}

pub fn models() -> Result<Models, String> {
    Ok(Models {
        detector: Model::load("mediapipe-hand-landmarker", "HandDetector", Units::All)?,
        landmarks: Model::load("mediapipe-hand-landmarker", "HandLandmarks", Units::All)?,
        embedder: Model::load("mediapipe-gesture-recognizer", "GestureEmbedder", Units::All)?,
        classifier: Model::load("mediapipe-gesture-recognizer", "GestureClassifier", Units::All)?,
    })
}

pub const GESTURES: [&str; 8] = ["None", "Closed_Fist", "Open_Palm", "Pointing_Up", "Thumb_Down", "Thumb_Up", "Victory", "ILoveYou"];

/// A hand: 21 points [x, y, z] in the frame's pixels, its gesture (None under 0.5 or "None"), and its crop
/// (centre x, centre y, side, rotation in degrees).
#[derive(Clone, Debug, serde::Serialize)]
pub struct Hand {
    pub points: Vec<[f64; 3]>,
    pub gesture: Option<String>,
    pub crop: [f64; 4],
}

/// Up to `limit` hands in the frame, with their gestures.
pub fn hands(m: &Models, rgba: &[u8], w: usize, h: usize, limit: usize) -> Result<Vec<Hand>, String> {
    let side = 192usize;
    let l = picture::letterbox(rgba, w, h, side, |ch| side - (side - ch) / 2 - ch);
    let (ox, oy) = (l.ox as f64, ((side - l.ch) / 2) as f64);
    let out = m.detector.predict(&[("image", &l.image)])?;
    let (scores, _) = out.floats("scores")?;
    let (boxes, _) = out.floats("boxes")?;
    let (keys, _) = out.floats("keypoints")?;
    let iou = |p: usize, q: usize| -> f32 {
        let (p, q) = (p * 4, q * 4);
        let ww = ((boxes[p] + boxes[p + 2]).min(boxes[q] + boxes[q + 2]) - boxes[p].max(boxes[q])).max(0.0);
        let hh = ((boxes[p + 1] + boxes[p + 3]).min(boxes[q + 1] + boxes[q + 3]) - boxes[p + 1].max(boxes[q + 1])).max(0.0);
        let union = boxes[p + 2] * boxes[p + 3] + boxes[q + 2] * boxes[q + 3] - ww * hh;
        if union > 0.0 { ww * hh / union } else { 0.0 }
    };
    let mut order: Vec<usize> = (0..scores.len()).filter(|&i| scores[i] >= 0.5).collect();
    order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]));
    let mut kept: Vec<usize> = Vec::new();
    for i in order {
        if kept.len() >= limit {
            break;
        }
        if kept.iter().all(|&k| iou(i, k) < 0.3) {
            kept.push(i);
        }
    }
    let x = |v: f32| (v as f64 * side as f64 - ox) / l.scale;
    let y = |v: f32| (v as f64 * side as f64 - oy) / l.scale;
    let mut reads = Vec::new();
    for i in kept {
        let (left, top) = (x(boxes[i * 4]), y(boxes[i * 4 + 1]));
        let (bw, bh) = (boxes[i * 4 + 2] as f64 * side as f64 / l.scale, boxes[i * 4 + 3] as f64 * side as f64 / l.scale);
        // keypoint 0 is the wrist, 2 the middle finger's knuckle; the hand points up the crop
        let (x0, y0, x2, y2) = (x(keys[i * 14]), y(keys[i * 14 + 1]), x(keys[i * 14 + 4]), y(keys[i * 14 + 5]));
        let rot = std::f64::consts::FRAC_PI_2 - (-(y2 - y0)).atan2(x2 - x0);
        let (c, sn) = (rot.cos(), rot.sin());
        let (cx, cy) = (left + bw / 2.0 + 0.5 * bh * sn, top + bh / 2.0 - 0.5 * bh * c);
        let side_crop = 2.6 * bw.max(bh);
        let size = 224usize;
        let kk = side_crop / size as f64;
        let half = size as f64 / 2.0;
        let crop = picture::warp(rgba, w, h, size, |bx, by| {
            let (a, b) = ((bx - half) * kk, (by - half) * kk);
            (cx + (c * a - sn * b), cy + (sn * a + c * b))
        });
        let lo = m.landmarks.predict(&[("image", &crop)])?;
        let (presence, _) = lo.floats("presence")?;
        if presence.first().copied().unwrap_or(0.0) < 0.5 {
            continue;
        }
        let (raw, _) = lo.floats("landmarks")?;
        let points: Vec<[f64; 3]> = (0..21)
            .map(|j| {
                let (u, v) = (raw[j * 3] as f64 - half, raw[j * 3 + 1] as f64 - half);
                [cx + (c * u - sn * v) * kk, cy + (sn * u + c * v) * kk, raw[j * 3 + 2] as f64 * kk]
            })
            .collect();
        let flat: Vec<f32> = points.iter().flat_map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
        let e = m.embedder.predict_inputs(&[("hand", Input::Array { shape: vec![1, 21, 3], data: flat })])?;
        let (emb, shape) = e.floats("embedding")?;
        let g = m.classifier.predict_inputs(&[("embedding", Input::Array { shape, data: emb })])?;
        let (v, _) = g.floats("scores")?;
        // Swift's max(by:): the first of the largest
        let best = (0..v.len()).fold(None::<usize>, |b, i| if b.is_none_or(|b| v[b] < v[i]) { Some(i) } else { b });
        let gesture = best.filter(|&b| v[b] >= 0.5 && b > 0).map(|b| GESTURES[b].to_string());
        reads.push(Hand { points, gesture, crop: [cx, cy, side_crop, rot.to_degrees()] });
    }
    Ok(reads)
}
