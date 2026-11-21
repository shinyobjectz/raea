//! A clip's facts (Perception.swift › analyze, video): its shots and how each begins, each shot's framing, faces,
//! subject, motion, brightness, look, scene labels and layout, the words on screen over time, and the clip's scene
//! labels; a still per shot. Written as facts.json in the Swift's format, so everything that reads facts reads these.
//! What's heard comes from YAMNet (sound.rs); performance is a later step.

use std::path::Path;

use serde::Serialize;

use raea_sys_layout::layout::Layout;
use crate::shots::{self, Bound};
use raea_sys_vision::vision::{self, Vision};

mod tracks;

pub use tracks::*;

#[derive(Serialize)]
pub struct Shot {
    pub start: f64,
    pub end: f64,
    pub transition: String,
    pub framing: String,
    pub faces: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<[f64; 4]>,
    pub motion: String,
    pub brightness: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<f64>,
    pub scene: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub still: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
}

#[derive(Serialize)]
pub struct Words {
    pub text: String,
    pub start: f64,
    pub end: f64,
    #[serde(rename = "box")]
    pub bbox: [f64; 4],
}

#[derive(Serialize)]
pub struct Facts {
    pub version: u32,
    pub file: String,
    pub kind: String,
    pub width: u32,
    pub height: u32,
    pub seconds: f64,
    pub shots: Vec<Shot>,
    pub text: Vec<Words>,
    /// what's heard: speech, music, singing, laughter, applause, silence or noise, in spans
    pub sound: Vec<raea_sys_vision::sound::Span>,
    pub scene: Vec<String>,
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// The same words read at nearby moments become one span.
fn spans(mut reads: Vec<(String, f64, [f64; 4])>) -> Vec<Words> {
    reads.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut out: Vec<Words> = Vec::new();
    for (text, t, b) in reads {
        let key = text.to_lowercase();
        if let Some(w) = out.iter_mut().rev().find(|w| w.text.to_lowercase() == key && t - w.end <= 2.6) {
            w.end = r2(w.end.max(t));
        } else {
            out.push(Words { text, start: r2(t), end: r2(t), bbox: b });
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// The commonest labels, ties by name.
fn commonest(labels: &[String], top: usize) -> Vec<String> {
    let mut count: Vec<(String, usize)> = Vec::new();
    for l in labels {
        match count.iter_mut().find(|c| &c.0 == l) {
            Some(c) => c.1 += 1,
            None => count.push((l.clone(), 1)),
        }
    }
    count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    count.into_iter().take(top).map(|c| c.0).collect()
}

/// RGBA8 shrunk to fit within `max` on its long side (the Swift's still-frame generator's maximumSize).
fn fit(f: &raea_io_media::Frame, max: u32) -> (Vec<u8>, usize, usize) {
    let (w, h) = (f.width, f.height);
    if w.max(h) <= max {
        return (f.rgba.clone(), w as usize, h as usize);
    }
    let s = max as f64 / w.max(h) as f64;
    let (nw, nh) = (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1));
    let img = image::RgbaImage::from_raw(w, h, f.rgba.clone()).expect("rgba size");
    (image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle).into_raw(), nw as usize, nh as usize)
}

/// Reads a video and writes facts.json and a still per shot into `dir`; on the shots `given` when they're known
/// already: an edited video's cuts found frame by frame (a videoform's reference, docs/videoform.feature VF3) include
/// the cuts inside one scene, a push-in or a jump between two angles on one person, which the rule here leaves alone
/// in raw footage
pub fn video(path: &Path, dir: &Path, backend: &str, v: &dyn Vision, given: Option<Vec<Bound>>) -> Result<Facts, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let (duration, samples) = shots::samples(path, backend)?;
    if samples.is_empty() {
        return Err(format!("{}: no frames could be read", path.display()));
    }
    let bounds: Vec<Bound> = given.filter(|b| !b.is_empty()).unwrap_or_else(|| shots::shot_bounds(&samples, duration));
    let mut d = raea_io_media::backend(backend)?.open_video(path)?;
    let info = d.info().clone();
    let mut frame_at = |t: f64| -> Result<(Vec<u8>, usize, usize), String> { Ok(fit(&d.frame_at(t.clamp(0.0, (duration - 1e-3).max(0.0)))?, 960)) };
    let (mut out, mut reads) = (Vec::new(), Vec::new());
    // a text read every 2 s of the video, 30 at least, 300 at most: at 30 a reel's captions stopped a minute in (11-21)
    let (mut ocr_budget, mut layout_budget) = (((duration / 2.0).ceil() as usize).clamp(30, 300), 40);
    for (i, b) in bounds.iter().enumerate() {
        let mid = (b.start + b.end) / 2.0;
        let (motion, brightness) = shots::motion_and_brightness(&samples, b);
        let (frame, w, h) = frame_at(mid)?;
        let seen = v.read(&frame, w, h, ocr_budget > 0)?;
        if ocr_budget > 0 {
            ocr_budget -= 1;
            reads.extend(seen.words.iter().map(|(t, bx)| (t.clone(), mid, *bx)));
        }
        // words that come and go inside a long shot: read every 2 s too
        let mut t = b.start + 1.0;
        while t < b.end - 0.5 && ocr_budget > 0 && b.end - b.start > 3.0 {
            if (t - mid).abs() > 0.6 {
                let (f, fw, fh) = frame_at(t)?;
                reads.extend(v.read(&f, fw, fh, true)?.words.into_iter().map(|(x, bx)| (x, t, bx)));
                ocr_budget -= 1;
            }
            t += 2.0;
        }
        let still = format!("shot-{:02}.jpg", i + 1);
        let img = image::RgbaImage::from_raw(w as u32, h as u32, frame.clone()).ok_or("frame size")?;
        image::DynamicImage::ImageRgba8(img).to_rgb8().save(dir.join(&still)).map_err(|e| e.to_string())?;
        // the layout, with two frames just after the middle so a split has to hold still to count
        let layout = if layout_budget > 0 {
            layout_budget -= 1;
            let mut next = Vec::new();
            for dt in [0.4, 0.8] {
                if mid + dt < b.end - 0.05 {
                    next.push(frame_at(mid + dt)?);
                }
            }
            let refs: Vec<(&[u8], usize, usize)> = next.iter().map(|(p, nw, nh)| (p.as_slice(), *nw, *nh)).collect();
            Some(vision::layout(v, &frame, w, h, &refs)?)
        } else {
            None
        };
        out.push(Shot {
            start: r2(b.start),
            end: r2(b.end),
            transition: b.transition.clone(),
            framing: seen.framing,
            faces: seen.faces,
            subject: seen.subject,
            motion: motion.into(),
            brightness: r2(brightness as f64),
            look: seen.look,
            scene: seen.scene,
            still: Some(still),
            layout,
        });
    }
    let scene = commonest(&out.iter().flat_map(|s| s.scene.clone()).collect::<Vec<_>>(), 4);
    let facts = Facts {
        version: 2,
        file: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        kind: "video".into(),
        width: info.width,
        height: info.height,
        seconds: r2(duration),
        shots: out,
        text: spans(reads),
        // what's heard needs YAMNet's Core ML package, which a release doesn't ship yet: without it the facts go
        // without, rather than every shot's reading being lost at the last step
        sound: match raea_sys_vision::sound::model() {
            Ok(m) => raea_sys_vision::sound::spans(&m, path, backend)?,
            Err(_) => Vec::new(),
        },
        scene,
    };
    std::fs::write(dir.join("facts.json"), serde_json::to_string_pretty(&facts).unwrap()).map_err(|e| e.to_string())?;
    Ok(facts)
}

/// The creator's performance second by second (Perception.swift › performance): frames at up to `fps` a second
/// through the face models (the face followed from its last points, found again when lost), hands every third frame
/// (gestures, and their movement toward energy), the sound's level every 10 ms; only derived numbers come out.
pub fn performance(path: &Path, backend: &str, fps_wanted: f64) -> Result<crate::performance::Performance, String> {
    use raea_sys_vision::face;
    use raea_sys_vision::hands;
    use crate::performance as perf;
    let models = face::models()?;
    let hand_models = hands::models().ok();
    let mut d = raea_io_media::backend(backend)?.open_video(path)?;
    let info = d.info().clone();
    let fps = fps_wanted.min(if info.fps > 0.0 { info.fps } else { 30.0 });
    let frame_len = 1.0 / info.fps.max(1.0);
    let (mut looks, mut times, mut gestures) = (Vec::new(), Vec::new(), Vec::new());
    let (mut crop, mut last, mut last_hands): (Option<face::Crop>, Option<Vec<f32>>, Vec<hands::Hand>) = (None, None, Vec::new());
    let mut frames = 0usize;
    let mut prev_pts = f64::NEG_INFINITY;
    loop {
        let target = frames as f64 / fps;
        if target >= info.duration {
            break;
        }
        let f = d.frame_at(target + frame_len - 1e-3)?;
        if f.pts <= prev_pts {
            break;
        }
        prev_pts = f.pts;
        frames += 1;
        let t = f.pts;
        let (w, h) = (f.width as usize, f.height as usize);
        let long = w.max(h) as f64;
        let (pic, pw, ph) = if long > 1280.0 { raea_sys_vision::picture::scaled(&f.rgba, w, h, 1280.0 / long) } else { (f.rgba.clone(), w, h) };
        let mut found = match crop { Some(c) => face::mesh(&models, &pic, pw, ph, c)?, None => None };
        if found.is_none() {
            if let Some(c) = face::detect(&models, &pic, pw, ph, true)? {
                found = face::mesh(&models, &pic, pw, ph, c)?;
            }
        }
        crop = found.as_ref().map(|m| face::crop_around(&m.points));
        times.push(t);
        let mut hand_motion = 0.0;
        if let Some(hm) = &hand_models {
            if frames % 3 == 1 {
                let read = hands::hands(hm, &pic, pw, ph, 2).unwrap_or_default();
                for r in &read {
                    if let Some(g) = &r.gesture {
                        gestures.push((t, g.clone()));
                    }
                }
                for (a, b) in last_hands.iter().zip(&read) {
                    let size = (b.points[9][0] - b.points[0][0]).hypot(b.points[9][1] - b.points[0][1]).max(1.0);
                    hand_motion += a.points.iter().zip(&b.points).fold(0.0, |s, (p, q)| s + (p[0] - q[0]).hypot(p[1] - q[1])) / 21.0 / size / 3.0;
                }
                last_hands = read;
            }
        }
        match found {
            Some(m) => {
                let pts: Vec<Vec<f64>> = m.points.iter().map(|p| p.to_vec()).collect();
                let mut l = perf::look(&pts, &m.blendshapes, t, last.as_deref());
                l.motion += 2.0 * hand_motion;
                looks.push(l);
                last = Some(m.blendshapes);
            }
            None => last = None,
        }
    }
    // the sound's level every 10 ms (RMS of 16 kHz mono)
    let pcm = raea_sys_vision::sound::pcm16k(path, backend);
    let loudness: Vec<f32> = pcm.chunks(160).map(|w| (w.iter().map(|v| v * v).sum::<f32>() / w.len().max(1) as f32).sqrt()).collect();
    let mut p = perf::summarize(&looks, &times, fps, info.duration, &loudness);
    perf::add_gestures(&mut p, &gestures, hand_models.is_some());
    Ok(p)
}

/// Everything ingest wrote for one clip, and what each step cost.
#[derive(Serialize, serde::Deserialize, Default)]
pub struct Manifest {
    pub version: u32,
    pub source: String,
    /// sha-256 of the clip's bytes: the key (the same footage under another name or path is ingested once)
    pub hash: String,
    /// step -> seconds it took when it ran
    pub steps: std::collections::BTreeMap<String, f64>,
}

/// What ingest writes and in what shape; bumped when a step's output changes, which makes every clip run again.
pub const INGEST_VERSION: u32 = 1;

// (the store and a clip's content hash, store.rs, here as before)
pub use crate::store::{content_hash, store};

/// Ingest a clip once: facts (with what's heard) and the performance when a face is on camera, into `store/<hash>/`; a
/// step whose output is already there for this version is skipped.
pub fn ingest(path: &Path, store: &Path, backend: &str, v: &dyn Vision) -> Result<(std::path::PathBuf, Manifest), String> {
    let hash = content_hash(path)?;
    let dir = store.join(&hash[..16]);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mpath = dir.join("manifest.json");
    let mut m: Manifest = std::fs::read_to_string(&mpath).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if m.version != INGEST_VERSION {
        m = Manifest { version: INGEST_VERSION, ..Default::default() };
    }
    m.source = dunce::canonicalize(path).unwrap_or(path.to_path_buf()).display().to_string();
    m.hash = hash;
    let save = |m: &Manifest| std::fs::write(&mpath, serde_json::to_string_pretty(m).unwrap()).map_err(|e| e.to_string());
    let timed = |f: &mut dyn FnMut() -> Result<(), String>| -> Result<f64, String> {
        let t = std::time::Instant::now();
        f()?;
        Ok(t.elapsed().as_secs_f64())
    };
    // a file with no picture (a voice, a music bed) gets only the sound steps
    if raea_io_media::backend(backend).and_then(|b| b.open_video(path)).is_err() {
        ingest_sound(path, &dir, backend, &mut m)?;
        save(&m)?;
        return Ok((dir, m));
    }
    if !m.steps.contains_key("facts") {
        let s = timed(&mut || video(path, &dir, backend, v, None).map(|_| ()))?;
        m.steps.insert("facts".into(), s);
        save(&m)?;
    }
    let facts: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("facts.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let faces = facts["shots"].as_array().is_some_and(|a| a.iter().any(|s| s["faces"].as_u64().unwrap_or(0) > 0));
    let seconds = facts["seconds"].as_f64().unwrap_or(0.0);
    if !m.steps.contains_key("performance") && faces && seconds <= 600.0 {
        let fps = if seconds > 180.0 { 8.0 } else { 15.0 };
        let s = timed(&mut || {
            let p = performance(path, backend, fps)?;
            std::fs::write(dir.join("performance.json"), serde_json::to_string_pretty(&p).unwrap()).map_err(|e| e.to_string())
        })?;
        m.steps.insert("performance".into(), s);
        save(&m)?;
    }
    // tracks written before faces were confirmed (2026-10-15) are redone
    let confirmed = std::fs::read_to_string(dir.join("faces.json")).map(|t| t.contains("\"tracks\"")).unwrap_or(false);
    if !m.steps.contains_key("faces") || !confirmed {
        let s = timed(&mut || faces_track(path, &dir, backend, v, 4.0))?;
        m.steps.insert("faces".into(), s);
        save(&m)?;
    }
    // tracks written before the rival peak (2026-10-15) are redone
    let rival = std::fs::read_to_string(dir.join("attention.json")).map(|t| t.contains("\"rival_floor\"")).unwrap_or(false);
    if !m.steps.contains_key("attention") || !rival {
        let s = timed(&mut || attention_track(path, &dir, backend, v, 4.0))?;
        m.steps.insert("attention".into(), s);
        save(&m)?;
    }
    if !m.steps.contains_key("matte") && faces {
        let s = timed(&mut || matte_track(path, &dir, backend, v, 4.0))?;
        m.steps.insert("matte".into(), s);
        save(&m)?;
    }
    if !m.steps.contains_key("light") {
        let s = timed(&mut || light_track(path, &dir, backend, v, 2.0))?;
        m.steps.insert("light".into(), s);
        save(&m)?;
    }
    if !m.steps.contains_key("mesh") && faces {
        let s = timed(&mut || mesh_track(path, &dir, backend, 8.0))?;
        m.steps.insert("mesh".into(), s);
        save(&m)?;
    }
    ingest_sound(path, &dir, backend, &mut m)?;
    save(&m)?;
    Ok((dir, m))
}

/// The faces and the person's box through a clip, `fps` times a second, as faces.json: {"fps", "frames": [{"t",
/// "faces": [[x, y, w, h]...], "person": [x, y, w, h] | null}]}, boxes 0..1 of the picture from the top left.
pub fn faces_track(path: &Path, dir: &Path, backend: &str, v: &dyn Vision, fps: f64) -> Result<(), String> {
    let mut d = raea_io_media::backend(backend)?.open_video(path)?;
    let info = d.info().clone();
    let mut frames = Vec::new();
    // (time, the faces seen: box and Vision's confidence)
    let mut raw: Vec<(f64, Vec<([f64; 4], f64)>)> = Vec::new();
    let mut t = 0.0;
    let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
    while t < info.duration {
        let (rgba, w, h) = fit(&d.frame_at(t)?, 640);
        let p = v.people(&rgba, w, h, true)?;
        let b = |r: &raea_sys_layout::layout::Rect| [r3(r.x), r3(r.y), r3(r.w), r3(r.h)];
        let seen: Vec<([f64; 4], f64)> = p.faces.iter().zip(p.face_conf.iter().chain(std::iter::repeat(&1.0))).map(|(r, c)| (b(r), r3(*c as f64))).collect();
        frames.push(serde_json::json!({"t": r2(t), "faces": seen.iter().map(|f| f.0).collect::<Vec<_>>(), "conf": seen.iter().map(|f| f.1).collect::<Vec<_>>(),
            "person": p.person.as_ref().map(b)}));
        raw.push((r2(t), seen));
        t += 1.0 / fps;
    }
    let tracks = confirm_faces(&raw, fps);
    let out = serde_json::json!({"fps": fps, "frames": frames, "tracks": tracks, "confirm": CONFIRM_NOTE});
    std::fs::write(dir.join("faces.json"), out.to_string()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
