//! The face (Perception.swift › detectFace, faceMesh, crop(around:)): MediaPipe's face landmarker (Apache-2.0)
//! converted to Core ML as three models: BlazeFace finds the face (letterboxed to 128, MediaPipe's weighted
//! suppression), the landmark model reads 478 points from a 256 x 256 crop turned upright, and the blendshape model
//! scores 52 expressions from 146 of the points.

use crate::coreml::{Input, Model, Units};
use crate::picture;

pub struct Models {
    pub detector: Model,
    pub landmarks: Model,
    pub blendshapes: Model,
}

pub fn models() -> Result<Models, String> {
    let f = "mediapipe-face-landmarker";
    Ok(Models {
        // CPU and Neural Engine: on the M5 Pro's GPU the points and blendshapes drift (max 5.29 px against 4.67, blendshape
        // mean 0.016 against 0.010 vs MediaPipe); `All` chose the Neural Engine already, so this only keeps it there
        detector: Model::load(f, "FaceDetector", Units::CpuAndNe)?,
        landmarks: Model::load(f, "FaceLandmarks", Units::CpuAndNe)?,
        blendshapes: Model::load(f, "FaceBlendshapes", Units::CpuAndNe)?,
    })
}

/// The square a face is read from: centre and side in the frame's pixels (y down), turned `rotation` radians
/// clockwise. Crop pixel (u, v) is centre + R(rotation)·((u, v) − 128)·side/256.
#[derive(Clone, Copy, Debug)]
pub struct Crop {
    pub cx: f64,
    pub cy: f64,
    pub side: f64,
    pub rotation: f64,
}

/// 478 points [x, y, z] in the frame's pixels (z at x's scale, smaller = nearer) and 52 blendshapes.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub points: Vec<[f64; 3]>,
    pub blendshapes: Vec<f32>,
}

pub const BLENDSHAPE_POINTS: [usize; 146] = [
    0, 1, 4, 5, 6, 7, 8, 10, 13, 14, 17, 21, 33, 37, 39, 40, 46, 52, 53, 54, 55, 58, 61, 63, 65, 66, 67, 70, 78, 80, 81, 82, 84, 87,
    88, 91, 93, 95, 103, 105, 107, 109, 127, 132, 133, 136, 144, 145, 146, 148, 149, 150, 152, 153, 154, 155, 157, 158, 159, 160, 161,
    162, 163, 168, 172, 173, 176, 178, 181, 185, 191, 195, 197, 234, 246, 249, 251, 263, 267, 269, 270, 276, 282, 283, 284, 285, 288,
    291, 293, 295, 296, 297, 300, 308, 310, 311, 312, 314, 317, 318, 321, 323, 324, 332, 334, 336, 338, 356, 361, 362, 365, 373, 374,
    375, 377, 378, 379, 380, 381, 382, 384, 385, 386, 387, 388, 389, 390, 397, 398, 400, 402, 405, 409, 415, 454, 466, 468, 469, 470,
    471, 472, 473, 474, 475, 476, 477,
];

/// BlazeFace's best face (or the biggest, `largest`) after weighted suppression, as the crop to read it from.
pub fn detect(m: &Models, rgba: &[u8], w: usize, h: usize, largest: bool) -> Result<Option<Crop>, String> {
    let side = 128usize;
    // the Swift translates the scaled picture up by side - oy - ch from the bottom: its top sits oy from the top
    let l = picture::letterbox(rgba, w, h, side, |ch| side - (side - ch) / 2 - ch);
    let (ox, oy) = (l.ox as f64, ((side - l.ch) / 2) as f64);
    let out = m.detector.predict(&[("image", &l.image)])?;
    let (scores, _) = out.floats("scores")?;
    let (boxes, _) = out.floats("boxes")?;
    let (keys, _) = out.floats("keypoints")?;
    let mut found: Vec<usize> = (0..scores.len()).filter(|&i| scores[i] >= 0.5).collect();
    found.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]));
    let area = |i: usize| boxes[i * 4 + 2] * boxes[i * 4 + 3];
    let best = if largest {
        // Swift's max(by:): the first of the largest
        found.iter().copied().fold(None::<usize>, |b, i| if b.is_none_or(|b| area(b) < area(i)) { Some(i) } else { b })
    } else {
        found.first().copied()
    };
    let Some(best) = best else { return Ok(None) };
    let iou = |i: usize, j: usize| -> f32 {
        let (a, c) = (i * 4, j * 4);
        let ww = ((boxes[a] + boxes[a + 2]).min(boxes[c] + boxes[c + 2]) - boxes[a].max(boxes[c])).max(0.0);
        let hh = ((boxes[a + 1] + boxes[a + 3]).min(boxes[c + 1] + boxes[c + 3]) - boxes[a + 1].max(boxes[c + 1])).max(0.0);
        let union = boxes[a + 2] * boxes[a + 3] + boxes[c + 2] * boxes[c + 3] - ww * hh;
        if union > 0.0 { ww * hh / union } else { 0.0 }
    };
    // boxes overlapping the best are averaged with it by score, not dropped (MediaPipe's WEIGHTED suppression)
    let cluster: Vec<usize> = found.iter().copied().filter(|&j| iou(best, j) > 0.3).collect();
    let total = cluster.iter().fold(0f32, |s, &j| s + scores[j]);
    let mean = |values: &[f32], stride: usize, at: usize| (cluster.iter().fold(0f32, |s, &j| s + values[j * stride + at] * scores[j]) / total) as f64;
    let x = |v: f64| (v * side as f64 - ox) / l.scale;
    let y = |v: f64| (v * side as f64 - oy) / l.scale;
    let (left, top) = (x(mean(&boxes, 4, 0)), y(mean(&boxes, 4, 1)));
    let (bw, bh) = (mean(&boxes, 4, 2) * side as f64 / l.scale, mean(&boxes, 4, 3) * side as f64 / l.scale);
    // keypoints 0 and 1: the subject's right eye, then left
    let (rx, ry, lx, ly) = (x(mean(&keys, 12, 0)), y(mean(&keys, 12, 1)), x(mean(&keys, 12, 2)), y(mean(&keys, 12, 3)));
    Ok(Some(Crop { cx: left + bw / 2.0, cy: top + bh / 2.0, side: 1.5 * bw.max(bh), rotation: -(-(ly - ry)).atan2(lx - rx) }))
}

/// The face in `crop`: its points and blendshapes, or None when the landmark model sees no face there.
pub fn mesh(m: &Models, rgba: &[u8], w: usize, h: usize, crop: Crop) -> Result<Option<Mesh>, String> {
    let side = 256usize;
    let k = crop.side / side as f64;
    let (c, s) = (crop.rotation.cos(), crop.rotation.sin());
    let half = side as f64 / 2.0;
    let input = picture::warp(rgba, w, h, side, |bx, by| {
        let (a, b) = ((bx - half) * k, (by - half) * k);
        (crop.cx + (c * a - s * b), crop.cy + (s * a + c * b))
    });
    let out = m.landmarks.predict(&[("image", &input)])?;
    let (raw, _) = out.floats("landmarks")?;
    let (presence, _) = out.floats("presence")?;
    if presence.first().copied().unwrap_or(0.0) < 0.5 {
        return Ok(None);
    }
    let points: Vec<[f64; 3]> = (0..478)
        .map(|i| {
            let (u, v) = (raw[i * 3] as f64 - half, raw[i * 3 + 1] as f64 - half);
            [crop.cx + (c * u - s * v) * k, crop.cy + (s * u + c * v) * k, raw[i * 3 + 2] as f64 * k]
        })
        .collect();
    let subset: Vec<f32> = BLENDSHAPE_POINTS.iter().flat_map(|&i| [points[i][0] as f32, points[i][1] as f32]).collect();
    let shapes = m.blendshapes.predict_inputs(&[("points", Input::Array { shape: vec![1, 146, 2], data: subset })])?;
    let (blendshapes, _) = shapes.floats("blendshapes")?;
    Ok(Some(Mesh { points, blendshapes }))
}

/// The next frame's crop from this frame's points, as MediaPipe follows a face.
pub fn crop_around(points: &[[f64; 3]]) -> Crop {
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in points {
        x0 = x0.min(p[0]);
        x1 = x1.max(p[0]);
        y0 = y0.min(p[1]);
        y1 = y1.max(p[1]);
    }
    let (r, l) = (points[33], points[263]);
    Crop { cx: (x0 + x1) / 2.0, cy: (y0 + y1) / 2.0, side: 1.5 * (x1 - x0).max(y1 - y0), rotation: -(-(l[1] - r[1])).atan2(l[0] - r[0]) }
}
