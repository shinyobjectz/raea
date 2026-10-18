//! The framing coach (Perception.swift › coach(from:)): the biggest face, where the eyes sit, and the light on the face
//! in stops (key to fill, top to bottom, the background against the face, mixed colour), and one warning only for
//! what Post can't fix. The inputs are Vision's face and person mask; the pixels are read where they're asked for.

use serde::{Deserialize, Serialize};

use crate::layout::{Boxed, Rect, r3};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Coach {
    pub aspect: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<Boxed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eye_line: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_of_view: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backlit_stops: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_fill_stops: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mixed: Option<f64>,
}

/// What the coach reads from Vision.
#[derive(Clone, Debug)]
pub struct Inputs {
    pub w: usize,
    pub h: usize,
    pub face: Option<Rect>,
    /// the person mask's bytes, row by row; empty when there's none
    pub mask: Vec<u8>,
    pub mw: usize,
    pub mh: usize,
}

type Rgb = (f64, f64, f64);

/// The coach from what Vision saw; `pixel(x, y)` gives the upright frame's (r, g, b) bytes, None when unreadable.
pub fn coach(i: &Inputs, iso: f32, field_of_view: f32, pixel: Option<&dyn Fn(usize, usize) -> (u8, u8, u8)>) -> Coach {
    let (ww, hh) = (i.w, i.h);
    let mut out = Coach { aspect: ww as f64 / hh.max(1) as f64, ..Default::default() };
    if field_of_view > 0.0 {
        let fov = field_of_view as f64;
        out.field_of_view = Some(r3(fov));
        out.lens = Some(if fov > 90.0 { "ultra wide" } else if fov > 60.0 { "wide" } else if fov > 40.0 { "normal" } else { "tele" }.into());
    }
    let Some(f) = i.face else { return out };
    out.face = Some(Boxed { x: r3(f.x), y: r3(f.y), w: r3(f.w), h: r3(f.h) });
    out.eye_line = Some(r3(f.y + 0.45 * f.h));

    let mask_at = |x: f64, y: f64| -> bool {
        if i.mask.is_empty() {
            return true;
        }
        i.mask[((y * i.mh as f64) as usize).min(i.mh - 1) * i.mw + ((x * i.mw as f64) as usize).min(i.mw - 1)] > 127
    };
    let Some(pixel) = pixel else { return out };
    let lin = |v: u8| {
        let c = v as f64 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let rgb = |x: f64, y: f64| -> Rgb {
        let px = ((x * ww as f64) as i64).max(0).min(ww as i64 - 1) as usize;
        let py = ((y * hh as f64) as i64).max(0).min(hh as i64 - 1) as usize;
        let p = pixel(px, py);
        (lin(p.0), lin(p.1), lin(p.2))
    };
    let luma = |c: Rgb| 0.2126 * c.0 + 0.7152 * c.1 + 0.0722 * c.2;
    let warmth = |c: Rgb| (c.0 - c.2) / (c.0 + c.1 + c.2).max(1e-4);

    // the skin of the face: the middle of the face box, inside the person
    let (mut left, mut right): (Vec<Rgb>, Vec<Rgb>) = (Vec::new(), Vec::new());
    let (mut upper, mut lower): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    let mut clipped = 0usize;
    let steps = 24usize;
    for j in 0..steps {
        for k in 0..steps {
            let x = f.x + f.w * (0.2 + 0.6 * (k as f64 + 0.5) / steps as f64);
            let y = f.y + f.h * (0.25 + 0.6 * (j as f64 + 0.5) / steps as f64);
            if !(x > 0.0 && x < 1.0 && y > 0.0 && y < 1.0 && mask_at(x, y)) {
                continue;
            }
            let c = rgb(x, y);
            if c.0.min(c.1).min(c.2) > 0.95 {
                clipped += 1;
            }
            if k < steps / 2 { left.push(c) } else { right.push(c) }
            if j < steps / 2 { upper.push(luma(c)) } else { lower.push(luma(c)) }
        }
    }
    let skin: Vec<Rgb> = left.iter().chain(right.iter()).copied().collect();
    if skin.len() <= 40 {
        return out;
    }
    let mean = |v: &[Rgb]| -> Rgb {
        let n = v.len().max(1) as f64;
        (v.iter().fold(0.0, |s, c| s + c.0) / n, v.iter().fold(0.0, |s, c| s + c.1) / n, v.iter().fold(0.0, |s, c| s + c.2) / n)
    };
    let stops = |a: f64, b: f64| (a.max(1e-4) / b.max(1e-4)).log2();
    let face = luma(mean(&skin));
    let clip_share = clipped as f64 / skin.len() as f64;
    let key_fill = stops(luma(mean(&left)), luma(mean(&right))).abs();
    let top_down = stops(upper.iter().fold(0.0, |s, v| s + v) / upper.len().max(1) as f64, lower.iter().fold(0.0, |s, v| s + v) / lower.len().max(1) as f64);
    let mixed = (warmth(mean(&left)) - warmth(mean(&right))).abs();

    // the background just around the person
    let mut around: Vec<f64> = Vec::new();
    for j in 0..steps {
        for k in 0..steps {
            let x = f.x - 1.5 * f.w + 4.0 * f.w * (k as f64 + 0.5) / steps as f64;
            let y = f.y - 0.8 * f.h + 3.0 * f.h * (j as f64 + 0.5) / steps as f64;
            if !(x > 0.0 && x < 1.0 && y > 0.0 && y < 1.0 && !mask_at(x, y)) {
                continue;
            }
            around.push(luma(rgb(x, y)));
        }
    }
    let backlit = if around.len() > 30 { Some(stops(around.iter().fold(0.0, |s, v| s + v) / around.len() as f64, face)) } else { None };

    // first match wins (the thresholds were checked on Kelly's finished ads and on made-bad copies, T-401)
    let light = if clip_share > 0.05 {
        "overexposed"
    } else if face < 0.03 || iso >= 1600.0 {
        "low light"
    } else if backlit.is_some_and(|b| b > 1.0) {
        "window behind (backlit)"
    } else if mixed > 0.15 {
        "mixed"
    } else if key_fill > 1.3 {
        "light to one side"
    } else if top_down > 1.0 {
        "overhead"
    } else if key_fill < 0.5 {
        "even front light"
    } else {
        "soft front light"
    };
    out.light = Some(light.into());
    out.warning = match light {
        "overexposed" => Some("Your face is blown out: turn a little away from the light, or soften it"),
        "low light" => Some("Too dark to clean up later: add light or move closer to a window"),
        "window behind (backlit)" => Some("Backlit: turn around to face the window"),
        "mixed" => Some("Two colours of light on your face: turn off the lamp or the overhead light"),
        _ => None,
    }
    .map(String::from);
    // a face close to a wide lens bulges, and no edit can undo it
    if out.warning.is_none() && out.field_of_view.is_some_and(|fov| fov > 60.0) && f.w > 0.55 {
        out.warning = Some("Too close for this lens: step back, or zoom in to about 2×".into());
    }
    out.face_level = Some(r3(face));
    out.backlit_stops = backlit.map(r3);
    out.key_fill_stops = Some(r3(key_fill));
    out.mixed = Some(r3(mixed));
    out
}

/// The inputs as `ugc-perceive coach-inputs` prints them.
pub fn inputs_from_json(j: &serde_json::Value) -> Inputs {
    let face: Vec<f64> = j["face"].as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
    Inputs {
        w: j["W"].as_u64().unwrap_or(0) as usize,
        h: j["H"].as_u64().unwrap_or(0) as usize,
        face: if face.len() == 4 { Some(Rect::new(face[0], face[1], face[2], face[3])) } else { None },
        mask: j["mask"].as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0) as u8).collect()).unwrap_or_default(),
        mw: j["mw"].as_u64().unwrap_or(0) as usize,
        mh: j["mh"].as_u64().unwrap_or(0) as usize,
    }
}
