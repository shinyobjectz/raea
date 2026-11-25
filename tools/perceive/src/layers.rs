//! raea-perceive layers VIDEO SHOTS.json: each shot's layers (docs/videoform.feature VF8e), from small copies of its
//! frames (FPS a second, at least MIN) and what Vision sees in LOOKS of them (the person mask, faces, text, whether
//! it's a screen). SHOTS.json is [{"start": S, "end": E}, ...]; prints [[layer, ...], ...], a list a shot.

use raea_sys_videoform_layers::{Frame, Sighting, layers};
use raea_sys_vision::vision::Vision;
use serde_json::Value;

/// Frames a second read for what moves and holds still, and the fewest a shot gets
const FPS: f64 = 4.0;
const MIN: usize = 4;
/// Frames Vision looks at in each shot
const LOOKS: usize = 3;
/// How far outside the shot its neighbours' frames are read
const NEAR: f64 = 0.05;
/// The small copies' width
const SMALL: u32 = 90;

fn small(rgba: &[u8], w: u32, h: u32) -> Frame {
    let Some(img) = image::RgbaImage::from_raw(w, h, rgba.to_vec()) else { return Frame::default() };
    let sh = ((SMALL as f64 * h as f64 / w as f64).round() as u32).max(1);
    let s = image::imageops::resize(&img, SMALL, sh, image::imageops::FilterType::Triangle);
    Frame { w: SMALL as usize, h: sh as usize, rgb: s.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect() }
}

/// The small copy of the frame at `t`, when there is one
fn small_at(d: &mut dyn raea_io_media::VideoDecoder, t: f64) -> Option<Frame> {
    let f = d.frame_at(t).ok()?;
    Some(small(&f.rgba, f.width, f.height))
}

/// The fewest pixels a side a layer's crop is read at
const CROP: u32 = 24;

/// What a layer shows (VF9d), told as a kit's asset is: its crop of the shot's middle frame, read as an asset is; a
/// person cut out is a person; "" (once kept) when it's too small to tell
fn shows(v: &dyn Vision, l: &raea_sys_videoform::rows::Layer, frame: Option<&image::RgbaImage>) -> Option<String> {
    if l.form == "cutout" {
        return Some("person".into());
    }
    let f = frame?;
    let (fw, fh) = (f.width() as f64, f.height() as f64);
    let [x, y, w, h] = l.at;
    let (cx, cy) = ((x.clamp(0.0, 1.0) * fw) as u32, (y.clamp(0.0, 1.0) * fh) as u32);
    let (cw, ch) = (((w * fw) as u32).min(f.width() - cx.min(f.width())), ((h * fh) as u32).min(f.height() - cy.min(f.height())));
    if cw < CROP || ch < CROP {
        return None;
    }
    let crop = image::imageops::crop_imm(f, cx, cy, cw, ch).to_image();
    let s = crate::kit::seen(v, &crop, None).ok()?;
    let kind = raea_sys_videoform_kit::tag(&s, "");
    // a screen or page is a screenshot, whatever photo is on it
    Some(if l.form == "screen" && ["person", "product"].contains(&kind) { "screenshot" } else { kind }.to_string())
}

pub(crate) fn b4(r: &raea_sys_layout::layout::Rect) -> [f64; 4] {
    [r.x, r.y, r.w, r.h]
}

pub(crate) fn run(video: &str, shots: &str) -> Result<(), String> {
    let shots: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(shots).map_err(|e| format!("{shots}: {e}"))?).map_err(|e| e.to_string())?;
    let mut d = raea_io_media::backend("apple")?.open_video(std::path::Path::new(video))?;
    let end = d.info().duration;
    let v = raea_sys_vision::vision::AppleVision::default();
    let mut out = Vec::new();
    for s in &shots {
        let (a, b) = (s["start"].as_f64().unwrap_or(0.0), s["end"].as_f64().unwrap_or(0.0).min(end - 1e-3));
        let n = (((b - a) * FPS).floor() as usize).max(MIN);
        let mut frames = Vec::with_capacity(n);
        let mut seen = Vec::new();
        let mut middle = None;
        for i in 0..n {
            let t = a + (b - a) * (i as f64 + 0.5) / n as f64;
            let f = d.frame_at(t.max(0.0))?;
            frames.push(small(&f.rgba, f.width, f.height));
            // Vision on frames spread through the shot
            if (0..LOOKS).any(|k| i == (n * (2 * k + 1)) / (2 * LOOKS)) {
                let x = v.see(&f.rgba, f.width as usize, f.height as usize)?;
                seen.push(Sighting {
                    faces: x.faces.iter().map(b4).collect(),
                    mask: x.mask,
                    mw: x.mw,
                    mh: x.mh,
                    text: x.lines.iter().map(|(r, _)| b4(r)).collect(),
                    screen: x.screenshot,
                });
                if seen.len() == LOOKS.div_ceil(2) {
                    middle = image::RgbaImage::from_raw(f.width, f.height, f.rgba);
                }
            }
        }
        // the frames just either side of it, where a layer's own cut shows
        let before = if a > NEAR { small_at(d.as_mut(), a - NEAR) } else { None };
        let after = if b + NEAR < end { small_at(d.as_mut(), b + NEAR) } else { None };
        let mut l = layers(&frames, [before.as_ref(), after.as_ref()], &seen);
        l.iter_mut().for_each(|l| l.kind = Some(shows(&v, l, middle.as_ref()).unwrap_or_default()));
        out.push(l);
    }
    println!("{}", serde_json::to_string(&out).unwrap());
    Ok(())
}

/// The mattes' width
const MATTE: u32 = 512;

/// raea-perceive mattes VIDEO FROM TO OUTDIR [--every N]: the person cut out of a stretch of footage (VF8e: a layered
/// kind's cut-out), as a <Video>'s matte wears them: m00001.png the file's frame 0, one each N frames, MATTE px wide,
/// the person's alpha; frames before FROM get an empty one. Prints the matte as the SDK takes it, and the person's box
/// in the stretch's middle frame, [x, y, w, h] of the frame.
pub(crate) fn mattes(video: &str, from: f64, to: f64, out: &str, every: usize) -> Result<(), String> {
    let mut d = raea_io_media::backend("apple")?.open_video(std::path::Path::new(video))?;
    let info = d.info().clone();
    let v = raea_sys_vision::vision::AppleVision::default();
    std::fs::create_dir_all(out).map_err(|e| format!("{out}: {e}"))?;
    let every = every.max(1);
    let last = ((to.min(info.duration - 1e-3) * info.fps) as usize) / every;
    let mh = ((MATTE as f64 * info.height as f64 / info.width as f64).round() as u32).max(1);
    let empty = image::RgbaImage::new(1, 1);
    let mid = (((from + to) / 2.0 * info.fps) as usize) / every;
    let mut person = None;
    for i in 0..=last {
        let t = (i * every) as f64 / info.fps;
        let file = std::path::Path::new(out).join(format!("m{:05}.png", i + 1));
        if t < from - 0.2 {
            empty.save(&file).map_err(|e| e.to_string())?;
            continue;
        }
        let f = d.frame_at(t)?;
        let p = v.people(&f.rgba, f.width as usize, f.height as usize, false)?;
        if i == mid {
            person = p.person.as_ref().map(b4);
        }
        let mut m = image::RgbaImage::new(p.mw.max(1) as u32, p.mh.max(1) as u32);
        for (k, px) in m.pixels_mut().enumerate() {
            *px = image::Rgba([255, 255, 255, p.mask.get(k).copied().unwrap_or(0)]);
        }
        image::imageops::resize(&m, MATTE, mh, image::imageops::FilterType::Triangle).save(&file).map_err(|e| e.to_string())?;
    }
    println!("{}", serde_json::json!({"fps": info.fps, "every": every, "count": last + 1, "person": person}));
    Ok(())
}
