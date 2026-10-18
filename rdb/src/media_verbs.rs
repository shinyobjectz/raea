//! `rdb media VERB ...`: the agent's media work, native and the same on the Mac and the iPhone (docs/media.feature M1).
//! What agents used ffmpeg and ffprobe for, on raea-media (the platform's decoders and encoders, Symphonia's audio):
//!
//!   rdb media probe FILE                                   duration, size, fps, codec, audio (JSON)
//!   rdb media still FILE --at S -o OUT.png|.jpg            the frame showing at S
//!   rdb media sheet FILE... [--n 12] [--width 240] -o OUT  a contact sheet (n frames of one file, or each file's middle)
//!   rdb media audio FILE [--rate 16000] [--mono] [--from S --to S] -o OUT.wav|-   (- is f32le on stdout)
//!   rdb media loudness FILE [--from S --to S]              integrated LUFS, mean and peak dB, true peak
//!   rdb media motion FILE [--from S --to S] [--fps 6]      how much the picture changes (mean luma difference, YDIF)
//!   rdb media cuts FILE [--threshold 0.08]                 shot changes (ffmpeg's scene score)
//!   rdb media freezes FILE [--noise 0.001] [--min 2]       held pictures (ffmpeg's freezedetect)
//!   rdb media proxy FILE [--height 720] -o OUT.mp4         a smaller copy
//!   rdb media trim FILE --from S --to S -o OUT.mp4         a stretch, frame-exact (re-encoded)
//!   rdb media concat A B ... -o OUT.mp4                    one after another (same aspect)
//!
//! `rdb ffmpeg ...` and `rdb ffprobe ...` read the command lines models write by reflex and run them as these verbs; what
//! they can't read is refused with the verb or edit that does it. No ffmpeg binary is involved.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use raea_media::{Audio, Backend, EncodeOpts, Frame, Info};
use crate::image;
use serde_json::{Value, json};

type R<T> = Result<T, String>;

fn backend() -> R<Box<dyn Backend>> {
    raea_media::backend(&crate::engine::media_backend())
}

fn soft() -> R<Box<dyn Backend>> {
    raea_media::backend("soft")
}

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];
const SWITCHES: &[&str] = &["mono"];

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// `--name value` flags (and `-o`), switches, and positional arguments.
fn parse(args: &[String]) -> (Vec<String>, BTreeMap<String, String>) {
    let (mut pos, mut fl) = (Vec::new(), BTreeMap::new());
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-o" || (a.starts_with("--") && a.len() > 2) {
            let name = if a == "-o" { "o" } else { &a[2..] };
            if SWITCHES.contains(&name) {
                fl.insert(name.to_string(), "1".into());
            } else {
                fl.insert(name.to_string(), args.get(i + 1).cloned().unwrap_or_default());
                i += 1;
            }
        } else {
            pos.push(a.clone());
        }
        i += 1;
    }
    (pos, fl)
}

fn num(fl: &BTreeMap<String, String>, k: &str) -> R<Option<f64>> {
    fl.get(k).map(|v| v.parse::<f64>().map_err(|_| format!("--{k} takes a number (seconds), not {v:?}"))).transpose()
}

/// The verbs `rdb media` answers here (anything else after `rdb media` is the media table's rows for files).
pub const VERBS: &[&str] = &["probe", "still", "sheet", "audio", "loudness", "motion", "cuts", "freezes", "proxy", "trim", "concat", "help"];

pub fn cli(args: &[String]) -> i32 {
    let Some(verb) = args.first() else {
        println!("usage: rdb media <probe|still|sheet|audio|loudness|motion|cuts|proxy|trim|concat> FILE ... (rdb media help)");
        return 2;
    };
    let (pos, fl) = parse(&args[1..]);
    let r = run(verb, &pos, &fl);
    match r {
        Ok(Some(v)) => {
            println!("{v}");
            0
        }
        Ok(None) => 0,
        Err(e) => {
            println!("REFUSED  {e}");
            1
        }
    }
}

fn run(verb: &str, pos: &[String], fl: &BTreeMap<String, String>) -> R<Option<Value>> {
    let file = || pos.first().map(PathBuf::from).ok_or_else(|| format!("rdb media {verb} FILE: which file?"));
    let out = || fl.get("o").filter(|o| !o.is_empty()).map(PathBuf::from).ok_or_else(|| format!("rdb media {verb} needs -o OUT"));
    let range = || -> R<(Option<f64>, Option<f64>)> { Ok((num(fl, "from")?, num(fl, "to")?)) };
    match verb {
        "probe" => probe(&file()?).map(Some),
        "still" => {
            let at = num(fl, "at")?.unwrap_or(0.0);
            still(&file()?, at, &out()?).map(Some)
        }
        "sheet" => {
            let files: Vec<PathBuf> = pos.iter().map(PathBuf::from).collect();
            let n = fl.get("n").and_then(|n| n.parse().ok()).unwrap_or(12);
            let w = fl.get("width").and_then(|n| n.parse().ok()).unwrap_or(240);
            sheet(&files, n, w, &out()?).map(Some)
        }
        "audio" => {
            let rate = fl.get("rate").and_then(|r| r.parse().ok()).unwrap_or(raea_media::audio::RATE);
            let (from, to) = range()?;
            // a WAV is 16-bit (the integer fold); `-o -` is f32le (the float fold)
            let wav = fl.get("o").is_some_and(|o| o != "-");
            let a = audio_folded(&file()?, rate, fl.contains_key("mono"), from, to, wav)?;
            let o = out()?;
            if o.as_os_str() == "-" {
                write_f32le(&a, &mut std::io::stdout().lock())?;
                return Ok(None);
            }
            write_wav(&a, &o)?;
            Ok(Some(json!({"wrote": o, "rate": a.rate, "channels": a.channels.len(), "seconds": round3(a.duration())})))
        }
        "loudness" => {
            let (from, to) = range()?;
            loudness(&file()?, from, to).map(Some)
        }
        "motion" => {
            let (from, to) = range()?;
            let fps = num(fl, "fps")?.unwrap_or(6.0);
            motion(&file()?, from, to, fps).map(|(v, _)| Some(v))
        }
        "freezes" => {
            let n = num(fl, "noise")?.unwrap_or(0.001);
            let min = num(fl, "min")?.unwrap_or(2.0);
            let f = freezes(&file()?, n, min)?;
            Ok(Some(json!({"file": file()?, "noise": n, "min": min, "freezes": f.iter().map(|(s, e)| json!({"start": round3(*s), "duration": e.map(|e| round3(e.0)), "end": e.map(|e| round3(e.1))})).collect::<Vec<_>>()})))
        }
        "cuts" => {
            let th = num(fl, "threshold")?.unwrap_or(0.08);
            let (from, to) = range()?;
            cuts(&file()?, th, from, to).map(|(v, _)| Some(v))
        }
        "proxy" => {
            let h = fl.get("height").and_then(|h| h.parse().ok()).unwrap_or(720);
            encode(&[(file()?, None, None)], Some(h), &out()?, fl.get("kbps").and_then(|k| k.parse().ok())).map(Some)
        }
        "trim" => {
            let (from, to) = range()?;
            if from.is_none() && to.is_none() {
                return Err("rdb media trim FILE --from S --to S -o OUT.mp4".into());
            }
            encode(&[(file()?, from, to)], None, &out()?, fl.get("kbps").and_then(|k| k.parse().ok())).map(Some)
        }
        "concat" => {
            if pos.len() < 2 {
                return Err("rdb media concat A B ... -o OUT.mp4: two files or more".into());
            }
            let srcs: Vec<_> = pos.iter().map(|p| (PathBuf::from(p), None, None)).collect();
            encode(&srcs, None, &out()?, fl.get("kbps").and_then(|k| k.parse().ok())).map(Some)
        }
        "help" => {
            println!("{}", include_str!("media_verbs.rs").lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").strip_prefix(' ').unwrap_or("")).collect::<Vec<_>>().join("\n"));
            Ok(None)
        }
        v => Err(format!("no media verb {v:?}: probe, still, sheet, audio, loudness, motion, cuts, freezes, proxy, trim, concat")),
    }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

// ---- probe

/// What a file is: video (with its audio), audio, or an image.
pub struct Probe {
    pub kind: &'static str,
    pub video: Option<Info>,
    pub audio: Option<(u32, usize, f64)>,
    pub image: Option<(u32, u32)>,
    pub duration: f64,
    pub bytes: u64,
    /// the video track's own timing, when the file states it (MP4, MOV)
    pub track: Option<Track>,
}

pub fn probe_of(path: &Path) -> R<Probe> {
    let bytes = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if IMAGE_EXTS.contains(&ext(path).as_str()) {
        let (w, h) = image::image_dimensions(path).map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(Probe { kind: "image", video: None, audio: None, image: Some((w, h)), duration: 0.0, bytes, track: None });
    }
    let track = video_track(path);
    let video = backend()?.open_video(path).ok().map(|d| d.info().clone()).filter(|i| i.width > 0 && i.frames > 0).map(|mut i| {
        // the file's own timing over what the decoder counted
        if let Some(t) = &track {
            i.duration = t.duration;
            i.frames = t.pts.len();
            i.fps = t.avg_fps();
        }
        i
    });
    let audio = soft()?.decode_audio(path).ok().filter(|a| a.len() > 0).map(|a| (a.rate, a.channels.len(), a.duration()));
    if video.is_none() && audio.is_none() {
        return Err(format!("{}: no picture or sound raea can read", path.display()));
    }
    // the container's own duration when it says one (ffprobe's format duration), else the longest stream
    let duration = container_duration(path).unwrap_or_else(|| video.as_ref().map(|v| v.duration).unwrap_or(0.0).max(audio.map(|a| a.2).unwrap_or(0.0)));
    Ok(Probe { kind: if video.is_some() { "video" } else { "audio" }, video, audio, image: None, duration, bytes, track })
}

/// An MP4 or QuickTime file's duration from its movie header (moov/mvhd): timescale and duration.
fn container_duration(path: &Path) -> Option<f64> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut head = [0u8; 16];
    // the top-level boxes, to moov
    let mut at = 0u64;
    let (moov, moov_len) = loop {
        if at + 8 > len {
            return None;
        }
        f.seek(SeekFrom::Start(at)).ok()?;
        f.read_exact(&mut head[..8]).ok()?;
        let mut size = u32::from_be_bytes(head[..4].try_into().ok()?) as u64;
        let mut hdr = 8;
        if size == 1 {
            f.read_exact(&mut head[8..16]).ok()?;
            size = u64::from_be_bytes(head[8..16].try_into().ok()?);
            hdr = 16;
        } else if size == 0 {
            size = len - at;
        }
        if size < hdr {
            return None;
        }
        if &head[4..8] == b"moov" {
            break (at + hdr, size - hdr);
        }
        at += size;
    };
    // moov is small: read it and find mvhd
    let mut m = vec![0u8; moov_len.min(64 << 20) as usize];
    f.seek(SeekFrom::Start(moov)).ok()?;
    f.read_exact(&mut m).ok()?;
    let mut i = 0usize;
    while i + 8 <= m.len() {
        let size = u32::from_be_bytes(m[i..i + 4].try_into().ok()?) as usize;
        if &m[i + 4..i + 8] == b"mvhd" {
            let b = &m[i + 8..];
            let (scale, dur) = if b.first() == Some(&1) {
                (u32::from_be_bytes(b[20..24].try_into().ok()?) as f64, u64::from_be_bytes(b[24..32].try_into().ok()?) as f64)
            } else {
                (u32::from_be_bytes(b[12..16].try_into().ok()?) as f64, u32::from_be_bytes(b[16..20].try_into().ok()?) as f64)
            };
            return (scale > 0.0).then_some(dur / scale);
        }
        if size < 8 {
            return None;
        }
        i += size;
    }
    None
}

/// A video track's timing as the file states it (MP4 / QuickTime sample tables): what ffprobe reports and what ffmpeg's
/// filters step through. A phone's variable-rate take (24 frames a second on average, on a 30 fps grid) needs it: frames
/// at a fixed rate miss or repeat some.
#[derive(Debug, Clone)]
pub struct Track {
    /// presentation times of every frame, sorted, from 0 (the edit list applied)
    pub pts: Vec<f64>,
    /// the track's own duration (mdhd)
    pub duration: f64,
    /// the rate the timestamps sit on (the most common frame duration): ffprobe's r_frame_rate
    pub grid_fps: f64,
}

impl Track {
    /// frames over the track's duration: ffprobe's avg_frame_rate
    pub fn avg_fps(&self) -> f64 {
        self.pts.len() as f64 / self.duration.max(1e-9)
    }
}

/// The boxes inside `b` (a box's payload): (type, payload).
fn boxes(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 8 <= b.len() {
        let mut size = u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = b[i + 4..i + 8].try_into().unwrap();
        let mut hdr = 8;
        if size == 1 && i + 16 <= b.len() {
            size = u64::from_be_bytes(b[i + 8..i + 16].try_into().unwrap()) as usize;
            hdr = 16;
        } else if size == 0 {
            size = b.len() - i;
        }
        if size < hdr || i + size > b.len() {
            break;
        }
        out.push((kind, &b[i + hdr..i + size]));
        i += size;
    }
    out
}

fn child<'a>(b: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(b).into_iter().find(|(k, _)| k == kind).map(|(_, p)| p)
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The first video track's timing, for MP4 and QuickTime files.
pub fn video_track(path: &Path) -> Option<Track> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    // the top-level boxes, to moov (read alone: mdat can be gigabytes)
    let mut at = 0u64;
    let moov = loop {
        if at + 8 > len {
            return None;
        }
        let mut h = [0u8; 16];
        f.seek(SeekFrom::Start(at)).ok()?;
        f.read_exact(&mut h[..8]).ok()?;
        let mut size = u32::from_be_bytes(h[..4].try_into().ok()?) as u64;
        let mut hdr = 8u64;
        if size == 1 {
            f.read_exact(&mut h[8..]).ok()?;
            size = u64::from_be_bytes(h[8..].try_into().ok()?);
            hdr = 16;
        } else if size == 0 {
            size = len - at;
        }
        if size < hdr {
            return None;
        }
        if &h[4..8] == b"moov" {
            let mut m = vec![0u8; (size - hdr).min(256 << 20) as usize];
            f.read_exact(&mut m).ok()?;
            break m;
        }
        at += size;
    };
    for (k, trak) in boxes(&moov) {
        if &k != b"trak" {
            continue;
        }
        let mdia = child(trak, b"mdia")?;
        let hdlr = child(mdia, b"hdlr")?;
        if hdlr.get(8..12) != Some(b"vide") {
            continue;
        }
        let mdhd = child(mdia, b"mdhd")?;
        let (scale, dur) = if mdhd.first() == Some(&1) { (be32(mdhd, 20)? as f64, be64(mdhd, 24)? as f64) } else { (be32(mdhd, 12)? as f64, be32(mdhd, 16)? as f64) };
        if scale <= 0.0 {
            return None;
        }
        let stbl = child(child(mdia, b"minf")?, b"stbl")?;
        // decode times from stts; composition offsets from ctts
        let stts = child(stbl, b"stts")?;
        let mut deltas: Vec<u32> = Vec::new();
        for e in 0..be32(stts, 4)? as usize {
            let (n, d) = (be32(stts, 8 + e * 8)?, be32(stts, 12 + e * 8)?);
            deltas.extend(std::iter::repeat_n(d, n as usize));
        }
        let mut dts = 0i64;
        let mut times: Vec<i64> = deltas.iter().map(|d| {
            let t = dts;
            dts += *d as i64;
            t
        }).collect();
        if let Some(ctts) = child(stbl, b"ctts") {
            let mut i = 0usize;
            for e in 0..be32(ctts, 4)? as usize {
                let n = be32(ctts, 8 + e * 8)? as usize;
                // signed whatever the version says: QuickTime writes negative offsets in version 0 boxes, and ffmpeg
                // reads them signed (take04.mov's HEVC: read unsigned, its frames sat 7 million seconds out)
                let off = be32(ctts, 12 + e * 8)? as i32 as i64;
                for _ in 0..n {
                    if let Some(t) = times.get_mut(i) {
                        *t += off;
                    }
                    i += 1;
                }
            }
        }
        // the edit list's start in media time (the first non-empty edit)
        let start = child(trak, b"edts").and_then(|e| child(e, b"elst")).and_then(|el| {
            let v1 = el.first() == Some(&1);
            let w = if v1 { 20 } else { 12 };
            (0..be32(el, 4)? as usize).find_map(|e| {
                let mt = if v1 { be64(el, 8 + e * w + 8)? as i64 } else { be32(el, 8 + e * w + 4)? as i32 as i64 };
                (mt >= 0).then_some(mt)
            })
        }).unwrap_or_else(|| times.iter().copied().min().unwrap_or(0));
        times.sort_unstable();
        let pts: Vec<f64> = times.iter().map(|t| (*t - start) as f64 / scale).collect();
        // the grid: the most common frame duration
        let mut count: BTreeMap<u32, usize> = BTreeMap::new();
        for d in &deltas {
            *count.entry(*d).or_default() += 1;
        }
        let common = count.into_iter().max_by_key(|(_, n)| *n).map(|(d, _)| d).unwrap_or(1).max(1);
        return Some(Track { pts, duration: dur / scale, grid_fps: scale / common as f64 });
    }
    None
}

pub fn probe(path: &Path) -> R<Value> {
    let p = probe_of(path)?;
    let mut v = json!({"file": path, "kind": p.kind, "duration": round3(p.duration), "bytes": p.bytes});
    if let Some(i) = &p.video {
        v["video"] = json!({"codec": i.codec, "width": i.width, "height": i.height, "fps": round3(standard_fps(i.fps)), "frame_rate": rate_fraction(p.track.as_ref().map(|t| t.grid_fps).unwrap_or(i.fps)), "frames": i.frames, "duration": round3(i.duration)});
    }
    if let Some((rate, ch, d)) = p.audio {
        v["audio"] = json!({"rate": rate, "channels": ch, "duration": round3(d)});
    }
    if let Some((w, h)) = p.image {
        v["image"] = json!({"width": w, "height": h});
    }
    Ok(v)
}

/// A measured frame rate (frames over duration) snapped to the standard rate it is within 1% of: a phone's variable-rate
/// take that averages 23.92 is 24 (ffprobe's avg_frame_rate says 24/1).
fn standard_fps(fps: f64) -> f64 {
    let rates = [24000.0 / 1001.0, 24.0, 25.0, 30000.0 / 1001.0, 30.0, 48.0, 50.0, 60000.0 / 1001.0, 60.0, 120.0];
    // the nearest, within 1% (30 is 30, not 29.97)
    rates.into_iter().filter(|r| (fps - r).abs() / r < 0.01).min_by(|a, b| (fps - a).abs().total_cmp(&(fps - b).abs())).unwrap_or(fps)
}

/// A frame rate as ffprobe writes it: 30/1, 30000/1001.
fn rate_fraction(fps: f64) -> String {
    let fps = standard_fps(fps);
    let n = fps.round();
    if (fps - n).abs() < 0.005 {
        return format!("{}/1", n as i64);
    }
    let ntsc = (fps * 1.001).round();
    if (fps - ntsc * 1000.0 / 1001.0).abs() < 0.005 {
        return format!("{}/1001", ntsc as i64 * 1000);
    }
    format!("{}/1000", (fps * 1000.0).round() as i64)
}

// ---- pictures

fn to_image(f: Frame) -> R<image::RgbaImage> {
    image::RgbaImage::from_raw(f.width, f.height, f.rgba).ok_or_else(|| "a decoded frame had the wrong size".to_string())
}

fn save(img: &image::RgbaImage, out: &Path) -> R<()> {
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let r = match ext(out).as_str() {
        "jpg" | "jpeg" => image::DynamicImage::ImageRgba8(img.clone()).to_rgb8().save(out),
        _ => img.save(out),
    };
    r.map_err(|e| format!("{}: {e}", out.display()))
}

/// The frame showing at `t` (the last frame when `t` is past the end).
fn frame(path: &Path, t: f64) -> R<image::RgbaImage> {
    let mut d = backend()?.open_video(path)?;
    let dur = d.info().duration;
    let t = t.clamp(0.0, (dur - 0.5 / d.info().fps.max(1.0)).max(0.0));
    to_image(d.frame_at(t)?)
}

pub fn still(path: &Path, at: f64, out: &Path) -> R<Value> {
    let img = if IMAGE_EXTS.contains(&ext(path).as_str()) {
        image::open(path).map_err(|e| format!("{}: {e}", path.display()))?.to_rgba8()
    } else {
        frame(path, at)?
    };
    save(&img, out)?;
    Ok(json!({"wrote": out, "at": at, "width": img.width(), "height": img.height()}))
}

pub fn sheet(files: &[PathBuf], n: usize, w: u32, out: &Path) -> R<Value> {
    if files.is_empty() {
        return Err("rdb media sheet FILE... -o OUT: which files?".into());
    }
    let mut tiles = Vec::new();
    if files.len() == 1 {
        let p = probe_of(&files[0])?;
        for i in 0..n.max(1) {
            let t = (i as f64 + 0.5) / n.max(1) as f64 * p.duration;
            tiles.push((t, frame(&files[0], t)?));
        }
    } else {
        for f in files {
            let p = probe_of(f)?;
            let img = if p.kind == "image" { image::open(f).map_err(|e| format!("{}: {e}", f.display()))?.to_rgba8() } else { frame(f, p.duration / 2.0)? };
            tiles.push((p.duration / 2.0, img));
        }
    }
    let th = tiles.iter().map(|(_, i)| (i.height() as f64 * w as f64 / i.width() as f64).round() as u32).max().unwrap_or(w);
    let cols = tiles.len().min(4) as u32;
    let rows = (tiles.len() as u32).div_ceil(cols);
    let mut sheet = image::RgbaImage::from_pixel(cols * w, rows * th, image::Rgba([0, 0, 0, 255]));
    for (k, (_, img)) in tiles.iter().enumerate() {
        let h = (img.height() as f64 * w as f64 / img.width() as f64).round() as u32;
        let small = image::imageops::resize(img, w, h.max(1), image::imageops::FilterType::Triangle);
        image::imageops::overlay(&mut sheet, &small, ((k as u32 % cols) * w) as i64, ((k as u32 / cols) * th) as i64);
    }
    save(&sheet, out)?;
    Ok(json!({"wrote": out, "tiles": tiles.len(), "at": tiles.iter().map(|(t, _)| round3(*t)).collect::<Vec<_>>(), "width": sheet.width(), "height": sheet.height()}))
}

// ---- sound

fn slice(a: &Audio, from: Option<f64>, to: Option<f64>) -> Audio {
    let n = a.len();
    let at = |s: f64| ((s * a.rate as f64).round().max(0.0) as usize).min(n);
    let (lo, hi) = (from.map(at).unwrap_or(0), to.map(at).unwrap_or(n));
    Audio { rate: a.rate, channels: a.channels.iter().map(|c| c[lo..hi.max(lo)].to_vec()).collect() }
}

/// The sound at `rate`, folded to mono when asked. The fold follows ffmpeg's: to float, stereo at -3 dB a side
/// ((L + R) * 0.707); to 16-bit integers, whose downmix ffmpeg normalises against clipping, the average ((L + R) / 2).
pub fn audio(path: &Path, rate: u32, mono: bool, from: Option<f64>, to: Option<f64>) -> R<Audio> {
    audio_folded(path, rate, mono, from, to, false)
}

pub fn audio_folded(path: &Path, rate: u32, mono: bool, from: Option<f64>, to: Option<f64>, integer: bool) -> R<Audio> {
    let a = slice(&soft()?.decode_audio(path)?, from, to);
    let a = if mono && a.channels.len() > 1 {
        // stereo folds down as ffmpeg's -ac 1 does, each side at -3 dB (L + R) * 0.707; more channels average
        let k = if a.channels.len() == 2 && !integer { std::f32::consts::SQRT_2 } else { a.channels.len() as f32 };
        let m = (0..a.len()).map(|i| a.channels.iter().map(|c| c[i]).sum::<f32>() / k).collect();
        Audio { rate: a.rate, channels: vec![m] }
    } else {
        a
    };
    Ok(raea_media::soft_resample(&a, rate))
}

fn write_f32le(a: &Audio, w: &mut dyn Write) -> R<()> {
    let mut buf = Vec::with_capacity(a.len() * a.channels.len() * 4);
    for i in 0..a.len() {
        for c in &a.channels {
            buf.extend_from_slice(&c[i].to_le_bytes());
        }
    }
    w.write_all(&buf).map_err(|e| e.to_string())
}

/// 16-bit PCM WAV, as ffmpeg writes by default.
fn write_wav(a: &Audio, out: &Path) -> R<()> {
    let ch = a.channels.len().max(1) as u16;
    let data = (a.len() * ch as usize * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&ch.to_le_bytes());
    b.extend_from_slice(&a.rate.to_le_bytes());
    b.extend_from_slice(&(a.rate * ch as u32 * 2).to_le_bytes());
    b.extend_from_slice(&(ch * 2).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for i in 0..a.len() {
        for c in &a.channels {
            b.extend_from_slice(&((c[i].clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
        }
    }
    std::fs::write(out, b).map_err(|e| format!("{}: {e}", out.display()))
}

/// Mean and peak level (dBFS) over every sample of every channel, as volumedetect reports them.
fn levels(a: &Audio) -> (f64, f64) {
    let (mut sum, mut n, mut peak) = (0f64, 0usize, 0f32);
    for c in &a.channels {
        for x in c {
            sum += (*x as f64) * (*x as f64);
            peak = peak.max(x.abs());
            n += 1;
        }
    }
    let db = |p: f64| if p > 0.0 { 10.0 * p.log10() } else { -91.0 };
    (db(sum / n.max(1) as f64), db((peak as f64) * (peak as f64)))
}

pub fn loudness(path: &Path, from: Option<f64>, to: Option<f64>) -> R<Value> {
    let a = slice(&soft()?.decode_audio(path)?, from, to);
    if a.len() == 0 {
        return Err(format!("{}: no sound in that stretch", path.display()));
    }
    let (mean, max) = levels(&a);
    let r1 = |x: f64| (x * 10.0).round() / 10.0;
    Ok(json!({"file": path, "lufs": r1(raea_media::audio::lufs(&a)), "mean_db": r1(mean), "max_db": r1(max), "true_peak_db": r1(raea_media::audio::true_peak_db(&a)), "seconds": round3(a.duration())}))
}

// ---- measures over frames

/// The decoder's own planes (NV12: luma, then Cb Cr interleaved at half size), as ffmpeg's filters see a 4:2:0 frame.
fn yuv(f: Frame) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let n = (f.width * f.height) as usize;
    let mut buf = f.rgba;
    let uv = buf.split_off(n.min(buf.len()));
    let (u, v) = (uv.iter().step_by(2).copied().collect(), uv.iter().skip(1).step_by(2).copied().collect());
    (buf, u, v)
}

fn sad(a: &[u8], b: &[u8]) -> u64 {
    a.iter().zip(b).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64).sum()
}

/// Samples from `from` to `to` at `fps` (the source's frames when `fps` is its rate): each tick's time, and the time
/// to read, the last frame before the middle of the tick, as ffmpeg's fps filter picks it (at 6 fps from 30, tick 0 is
/// source frame 2 and tick 1 frame 7).
fn times(duration: f64, from: Option<f64>, to: Option<f64>, fps: f64) -> Vec<(f64, f64)> {
    let (a, b) = (from.unwrap_or(0.0).max(0.0), to.unwrap_or(duration).min(duration));
    let n = ((b - a) * fps).round().max(0.0) as usize;
    (0..n).map(|i| (a + i as f64 / fps, a + (i as f64 + 0.5) / fps - 1e-6)).collect()
}

/// The video's own duration: its track's (what ffmpeg's filters run to), else the decoder's.
fn video_span(path: &Path, info: &Info) -> f64 {
    video_track(path).map(|t| t.duration).unwrap_or(info.duration)
}

/// How much the picture changes: the mean absolute luma difference between samples `fps` apart (signalstats YDIF).
pub fn motion(path: &Path, from: Option<f64>, to: Option<f64>, fps: f64) -> R<(Value, Vec<(f64, f64)>)> {
    let mut d = backend()?.open_video(path)?;
    let ts = times(video_span(path, d.info()), from, to, fps);
    let mut prev: Option<Vec<u8>> = None;
    let mut out = Vec::new();
    for (t, at) in ts {
        let (y, ..) = yuv(d.frame_nv12_at(at)?);
        let ydif = prev.as_ref().map(|p| sad(p, &y) as f64 / y.len() as f64).unwrap_or(0.0);
        out.push((t, ydif));
        prev = Some(y);
    }
    let mean = if out.is_empty() { 0.0 } else { out.iter().map(|(_, v)| v).sum::<f64>() / out.len() as f64 };
    Ok((json!({"file": path, "ydif_mean": round3(mean), "samples": out.len(), "fps": fps}), out))
}

/// Shot changes: ffmpeg's scene score (select's get_scene_score: the mean absolute luma difference from the frame before,
/// against the change in it since the frame before that, over 100) above `threshold`, frame by frame.
pub fn cuts(path: &Path, threshold: f64, from: Option<f64>, to: Option<f64>) -> R<(Value, Vec<(usize, f64, f64)>)> {
    let mut d = backend()?.open_video(path)?;
    let info = d.info().clone();
    // every frame the file holds, at its own time (a variable-rate take has no fixed step)
    let ts = frames_of(path, &info, from, to);
    let mut prev: Option<(Vec<u8>, Vec<u8>, Vec<u8>)> = None;
    let mut prev_mafd = 0.0;
    let mut hits = Vec::new();
    for (n, (_, at)) in ts.into_iter().enumerate() {
        let f = d.frame_nv12_at(at)?;
        let pts = f.pts;
        let (y, u, v) = yuv(f);
        if let Some((py, ..)) = &prev {
            // a YUV frame is scored on its luma alone (f_select.c: nb_planes = is_yuv ? 1 : ...)
            let mafd = sad(py, &y) as f64 / y.len() as f64;
            let score = (mafd.min((mafd - prev_mafd).abs()) / 100.0).clamp(0.0, 1.0);
            prev_mafd = mafd;
            if score > threshold {
                hits.push((n, pts, score));
            }
        }
        prev = Some((y, u, v));
    }
    let v = json!({"file": path, "threshold": threshold, "cuts": hits.iter().map(|(_, t, s)| json!({"at": round3(*t), "score": round3(*s)})).collect::<Vec<_>>()});
    Ok((v, hits))
}

/// Every frame the file holds as (tick, time to read): its own times when the file states them, else a fixed step.
fn frames_of(path: &Path, info: &Info, from: Option<f64>, to: Option<f64>) -> Vec<(f64, f64)> {
    match video_track(path) {
        Some(t) => t.pts.iter().filter(|p| from.is_none_or(|a| **p >= a - 1e-6) && to.is_none_or(|b| **p < b)).map(|p| (*p, *p + 1e-4)).collect(),
        None => times(info.duration, from, to, info.fps),
    }
}

/// Held pictures: ffmpeg's freezedetect. Each frame is compared with the last one that moved (the mean absolute difference
/// over every plane, as a fraction of full scale); a stretch at or under `noise` lasting `min` seconds or more is a
/// freeze, reported as (start, Some((duration, end))) when it ends, or (start, None) when the file ends frozen.
pub fn freezes(path: &Path, noise: f64, min: f64) -> R<Vec<(f64, Option<(f64, f64)>)>> {
    let mut d = backend()?.open_video(path)?;
    let info = d.info().clone();
    let mut reference: Option<(f64, Vec<u8>, Vec<u8>, Vec<u8>)> = None;
    let mut frozen_now = false;
    let mut out: Vec<(f64, Option<(f64, f64)>)> = Vec::new();
    for (pts, at) in frames_of(path, &info, None, None) {
        let (y, u, v) = yuv(d.frame_nv12_at(at)?);
        let mut frozen = false;
        if let Some((rp, ry, ru, rv)) = &reference {
            let mafd = (sad(ry, &y) + sad(ru, &u) + sad(rv, &v)) as f64 / (y.len() + u.len() + v.len()) as f64 / 256.0;
            frozen = mafd <= noise;
            if pts - rp >= min - 1e-9 {
                if !frozen_now {
                    out.push((*rp, None));
                }
                if !frozen {
                    if let Some(last) = out.last_mut() {
                        last.1 = Some((pts - last.0, pts));
                    }
                }
                frozen_now = frozen;
            }
        }
        if !frozen {
            reference = Some((pts, y, u, v));
        }
    }
    Ok(out)
}

// ---- encoding (proxy, trim, concat)

/// A picture at w x h: scaled, and when its shape differs from the output's, fitted inside it on black (concat of a
/// 480x848 take after a 480x864 one; ffmpeg would join them as they are and leave the player to cope).
fn fit(img: image::RgbaImage, w: u32, h: u32) -> image::RgbaImage {
    if img.width() == w && img.height() == h {
        return img;
    }
    let k = (w as f64 / img.width() as f64).min(h as f64 / img.height() as f64);
    let (fw, fh) = (((img.width() as f64 * k).round() as u32).clamp(1, w), ((img.height() as f64 * k).round() as u32).clamp(1, h));
    // bicubic, as ffmpeg's scale filter
    let small = image::imageops::resize(&img, fw, fh, image::imageops::FilterType::CatmullRom);
    if fw == w && fh == h {
        return small;
    }
    let mut out = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
    image::imageops::overlay(&mut out, &small, ((w - fw) / 2) as i64, ((h - fh) / 2) as i64);
    out
}

fn even(x: f64) -> u32 {
    ((x / 2.0).round() as u32 * 2).max(2)
}

/// One stretch after another into OUT.mp4 at the first's frame rate and size (or `height`, never upscaled), with sound.
pub fn encode(srcs: &[(PathBuf, Option<f64>, Option<f64>)], height: Option<u32>, out: &Path, kbps: Option<u32>) -> R<Value> {
    // a smaller copy is for looking at (a judge, a phone preview): fewer bits a pixel than a full-size one (VideoToolbox
    // at 0.05 lost 0.03 SSIM to x264 -crf 28 on take04; 0.08 matches it)
    let bits = if height.is_some() { 0.08 } else { 0.1 };
    let be = backend()?;
    let mut decs = Vec::new();
    for (p, ..) in srcs {
        decs.push(be.open_video(p).map_err(|e| format!("{}: {e}", p.display()))?);
    }
    let first = decs[0].info().clone();
    let (sw, sh) = (first.width as f64, first.height as f64);
    let h = height.map(|h| (h as f64).min(sh)).unwrap_or(sh);
    let (w, h) = (even(sw * h / sh), even(h));
    // the output's rate: the first source's average (its track's), at the standard rate it's near
    let fps = standard_fps(video_track(&srcs[0].0).map(|t| t.avg_fps()).unwrap_or(first.fps));
    let kbps = kbps.unwrap_or(((w as f64 * h as f64 * fps * bits) / 1000.0).clamp(300.0, 20000.0) as u32);
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let mut enc = be.encoder(out, &EncodeOpts { width: w, height: h, fps, kbps, nv12: false, realtime: false })?;
    let mut frames = 0usize;
    let mut lanes: Vec<Audio> = Vec::new();
    let mut any_sound = false;
    let mut src_channels = 0usize;
    for (d, (p, from, to)) in decs.iter_mut().zip(srcs) {
        let info = d.info().clone();
        let ts = times(video_span(p, &info), *from, *to, fps);
        for (_, at) in &ts {
            let img = to_image(d.frame_at(*at)?)?;
            let img = fit(img, w, h);
            enc.push(img.as_raw())?;
        }
        frames += ts.len();
        // the stretch's sound (silence where there is none), stereo at the mix rate
        let secs = ts.len() as f64 / fps;
        let a = soft()?.decode_audio(p).ok().filter(|a| a.len() > 0);
        any_sound |= a.is_some();
        let a = a.map(|a| raea_media::soft_resample(&slice(&a, Some(from.unwrap_or(0.0)), None), raea_media::audio::RATE));
        let n = (secs * raea_media::audio::RATE as f64).round() as usize;
        let mut ch = vec![vec![0f32; n]; 2];
        if let Some(a) = &a {
            src_channels = src_channels.max(a.channels.len().min(2));
        }
        if let Some(a) = a {
            for (c, col) in ch.iter_mut().enumerate() {
                let src = &a.channels[c.min(a.channels.len() - 1)];
                let k = n.min(src.len());
                col[..k].copy_from_slice(&src[..k]);
            }
        }
        lanes.push(Audio { rate: raea_media::audio::RATE, channels: ch });
    }
    // as many channels as the sources have (mono stays mono, as ffmpeg keeps it: two copies of one channel would
    // measure 3 dB louder)
    let out_ch = src_channels.clamp(1, 2);
    let sound = Audio { rate: raea_media::audio::RATE, channels: (0..out_ch).map(|c| lanes.iter().flat_map(|l| l.channels[c].iter().copied()).collect()).collect() };
    let stats = enc.finish(if any_sound { Some(&sound) } else { None })?;
    Ok(json!({"wrote": out, "frames": frames, "seconds": round3(frames as f64 / fps), "width": w, "height": h, "fps": round3(fps), "kbps": kbps, "bytes": stats.bytes, "sound": any_sound}))
}

// ---- ffmpeg and ffprobe, read

/// `rdb ffmpeg ARGS`: an ffmpeg command line run as the media verbs.
pub fn ffmpeg_cli(args: &[String]) -> i32 {
    match ffmpeg(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("rdb ffmpeg: {e}");
            1
        }
    }
}

fn secs(s: &str) -> R<f64> {
    // 12.5, 00:01:02.5, 1:02
    let mut t = 0.0;
    for part in s.split(':') {
        t = t * 60.0 + part.parse::<f64>().map_err(|_| format!("not a time: {s}"))?;
    }
    Ok(t)
}

const VERBS_HINT: &str = "rdb media probe/still/sheet/audio/loudness/motion/cuts/proxy/trim/concat, or an edit (clip rows) rendered with raea render";

#[derive(Default)]
struct FfInput {
    path: String,
    ss: Option<f64>,
    t: Option<f64>,
    to: Option<f64>,
    concat: bool,
}

fn ffmpeg(args: &[String]) -> R<()> {
    let mut inputs: Vec<FfInput> = Vec::new();
    let mut pending = FfInput::default();
    let (mut ss, mut t, mut to) = (None, None, None);
    let (mut vf, mut af): (Option<String>, Option<String>) = (None, None);
    let (mut ar, mut ac, mut fmt, mut one_frame, mut outp) = (None, None, None, false, None::<String>);
    let mut i = 0;
    let next = |i: &mut usize| -> R<String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| format!("{} needs a value", args[*i - 1]))
    };
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-i" => {
                pending.path = next(&mut i)?;
                inputs.push(std::mem::take(&mut pending));
            }
            "-ss" => {
                let v = secs(&next(&mut i)?)?;
                if inputs.is_empty() { pending.ss = Some(v) } else { ss = Some(v) }
            }
            "-t" => {
                let v = secs(&next(&mut i)?)?;
                if inputs.is_empty() { pending.t = Some(v) } else { t = Some(v) }
            }
            "-to" => {
                let v = secs(&next(&mut i)?)?;
                if inputs.is_empty() { pending.to = Some(v) } else { to = Some(v) }
            }
            "-f" => {
                let f = next(&mut i)?;
                if f == "concat" && i + 1 < args.len() {
                    pending.concat = true;
                } else {
                    fmt = Some(f);
                }
            }
            "-vf" | "-filter:v" => vf = Some(next(&mut i)?),
            "-af" | "-filter:a" => af = Some(next(&mut i)?),
            "-ar" => ar = Some(next(&mut i)?.parse::<u32>().map_err(|_| "-ar takes a rate")?),
            "-ac" => ac = Some(next(&mut i)?.parse::<u32>().map_err(|_| "-ac takes a channel count")?),
            "-frames:v" | "-vframes" => one_frame = next(&mut i)? == "1",
            "-filter_complex" | "-lavfi" => {
                return Err(format!("-filter_complex isn't read here: overlays, wipes and layouts are an edit (clip rows with x, y, w, h, from, to; a transition row), rendered with raea render. Or {VERBS_HINT}"));
            }
            // codec, quality and container choices: the platform encoder decides (H.264 + AAC in MP4)
            "-c" | "-c:v" | "-c:a" | "-codec" | "-vcodec" | "-acodec" | "-crf" | "-preset" | "-b:v" | "-b:a" | "-q:v" | "-qscale:v" | "-pix_fmt" | "-movflags" | "-v" | "-loglevel" | "-r" | "-safe" | "-map" | "-tune" | "-profile:v" | "-g" | "-threads" => {
                next(&mut i)?;
            }
            "-y" | "-n" | "-hide_banner" | "-nostats" | "-nostdin" | "-an" | "-vn" | "-sn" | "-shortest" | "-accurate_seek" => {}
            o if o.starts_with('-') && o != "-" => return Err(format!("{o} isn't read here; {VERBS_HINT}")),
            o => outp = Some(o.to_string()),
        }
        i += 1;
    }
    let out = outp.ok_or("no output")?;
    let Some(inp) = inputs.first() else { return Err("no input (-i FILE)".into()) };
    let from = inp.ss.or(ss);
    let span_to = |from: Option<f64>| inp.to.or(to).or_else(|| inp.t.or(t).map(|d| from.unwrap_or(0.0) + d));
    let (from, until) = (from, span_to(from));
    let path = PathBuf::from(&inp.path);
    let to_null = out == "-" || fmt.as_deref() == Some("null");
    // measures: printed the way ffmpeg's filters print them, so a caller reading them keeps working
    if to_null && fmt.as_deref() != Some("f32le") && fmt.as_deref() != Some("s16le") {
        let mut said = false;
        if let Some(f) = vf.as_deref() {
            if f.contains("scene") {
                let th = f.split("gt(scene,").nth(1).and_then(|r| r.split(')').next()).and_then(|x| x.parse().ok()).unwrap_or(0.4);
                let (_, hits) = cuts(&path, th, from, until)?;
                for (n, pts, s) in hits {
                    eprintln!("[Parsed_showinfo_1 @ 0x0] n:{n} pts:{} pts_time:{pts:.6} scene:{s:.6}", (pts * 1000.0).round() as i64);
                }
                said = true;
            } else if let Some(spec) = f.strip_prefix("freezedetect") {
                let opt = |k: &str, dflt: f64| spec.trim_start_matches('=').split(':').find_map(|kv| kv.strip_prefix(&format!("{k}="))).map(|v| v.trim_end_matches("dB")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(dflt);
                let (n, dur) = (opt("n", opt("noise", 0.001)), opt("d", opt("duration", 2.0)));
                for (start, end) in freezes(&path, n, dur)? {
                    eprintln!("[freezedetect @ 0x0] lavfi.freezedetect.freeze_start: {}", ff_time(start));
                    if let Some((d, e)) = end {
                        eprintln!("[freezedetect @ 0x0] lavfi.freezedetect.freeze_duration: {}\n[freezedetect @ 0x0] lavfi.freezedetect.freeze_end: {}", ff_time(d), ff_time(e));
                    }
                }
                said = true;
            } else if f.contains("signalstats") {
                let fps = f.split("fps=").nth(1).and_then(|r| r.split(',').next()).and_then(|x| x.parse().ok()).unwrap_or(6.0);
                let (_, ys) = motion(&path, from, until, fps)?;
                for (k, (t, y)) in ys.iter().enumerate() {
                    eprintln!("frame:{k} pts:{} pts_time:{t:.6}\nlavfi.signalstats.YDIF={y:.6}", (t * 1000.0).round() as i64);
                }
                said = true;
            } else {
                return Err(format!("-vf {f} to null isn't read here; {VERBS_HINT}"));
            }
        }
        if let Some(f) = af.as_deref() {
            let a = slice(&soft()?.decode_audio(&path)?, from, until);
            if f.contains("volumedetect") {
                let (mean, max) = levels(&a);
                eprintln!("[Parsed_volumedetect_0 @ 0x0] n_samples: {}\n[Parsed_volumedetect_0 @ 0x0] mean_volume: {mean:.1} dB\n[Parsed_volumedetect_0 @ 0x0] max_volume: {max:.1} dB", a.len() * a.channels.len());
            } else if f.contains("ebur128") {
                eprintln!("[Parsed_ebur128_0 @ 0x0] Summary:\n\n  Integrated loudness:\n    I:         {:.1} LUFS\n\n  True peak:\n    Peak:      {:.1} dBFS", raea_media::audio::lufs(&a), raea_media::audio::true_peak_db(&a));
            } else {
                return Err(format!("-af {f} isn't read here; {VERBS_HINT}"));
            }
            said = true;
        }
        if !said {
            return Err(format!("an output to null measures something (-vf select=scene, signalstats; -af volumedetect, ebur128); {VERBS_HINT}"));
        }
        return Ok(());
    }
    // raw or WAV audio
    let oext = ext(Path::new(&out));
    if matches!(fmt.as_deref(), Some("f32le")) || oext == "wav" {
        if vf.is_some() || af.is_some() {
            return Err(format!("audio filters aren't read here; {VERBS_HINT}"));
        }
        let rate = ar.unwrap_or(soft()?.decode_audio(&path).map(|a| a.rate).unwrap_or(raea_media::audio::RATE));
        let a = audio_folded(&path, rate, ac == Some(1), from, until, fmt.as_deref() != Some("f32le"))?;
        if out == "-" {
            return write_f32le(&a, &mut std::io::stdout().lock());
        }
        if fmt.as_deref() == Some("f32le") {
            let mut f = std::fs::File::create(&out).map_err(|e| format!("{out}: {e}"))?;
            return write_f32le(&a, &mut f);
        }
        return write_wav(&a, Path::new(&out));
    }
    if fmt.is_some() && fmt.as_deref() != Some("mp4") && fmt.as_deref() != Some("image2") {
        return Err(format!("-f {} isn't read here; {VERBS_HINT}", fmt.unwrap_or_default()));
    }
    // a still
    if one_frame || IMAGE_EXTS.contains(&oext.as_str()) {
        if vf.is_some() {
            return Err(format!("-vf on a still isn't read here: take the still (rdb media still), then crop or scale it in an edit; {VERBS_HINT}"));
        }
        still(&path, from.unwrap_or(0.0), Path::new(&out))?;
        return Ok(());
    }
    // video: concat, trim, scale
    let height = match vf.as_deref() {
        None => None,
        Some(f) if f.starts_with("scale=") && !chained(f) => Some(scale_height(&f[6..], &path)?),
        Some(f) => return Err(format!("-vf {f} isn't read here (only scale=W:H); crops, pads and overlays are an edit: {VERBS_HINT}")),
    };
    if af.is_some() {
        return Err(format!("-af on a video isn't read here; {VERBS_HINT}"));
    }
    let srcs: Vec<(PathBuf, Option<f64>, Option<f64>)> = if inp.concat {
        let list = std::fs::read_to_string(&inp.path).map_err(|e| format!("{}: {e}", inp.path))?;
        let base = Path::new(&inp.path).parent().map(Path::to_path_buf).unwrap_or_default();
        list.lines()
            .filter_map(|l| l.trim().strip_prefix("file "))
            .map(|f| (base.join(f.trim().trim_matches('\'').trim_matches('"')), None, None))
            .collect()
    } else if inputs.len() > 1 {
        return Err(format!("several inputs: use -f concat with a list, or an edit for anything laid over; {VERBS_HINT}"));
    } else {
        vec![(path.clone(), from, until)]
    };
    encode(&srcs, height, Path::new(&out), None)?;
    Ok(())
}

/// A time as ffmpeg's av_ts2timestr writes it: up to 6 decimals, trailing zeros dropped (44.233333, 55).
fn ff_time(t: f64) -> String {
    let s = format!("{t:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A filter chain (a comma outside parentheses and quotes): scale=-2:'min(720,ih)' is one filter.
fn chained(f: &str) -> bool {
    let (mut depth, mut quoted) = (0i32, false);
    for c in f.chars() {
        match c {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// scale=W:H's height: -1/-2 keep the aspect, min(N,ih) caps it.
fn scale_height(spec: &str, path: &Path) -> R<u32> {
    let spec = spec.trim_matches(['\'', '"']);
    let (w, h) = spec.split_once(':').ok_or_else(|| format!("scale={spec}: write scale=W:H"))?;
    let (w, h) = (w.trim_matches(['\'', '"']), h.trim_matches(['\'', '"']));
    let p = probe_of(path)?;
    let i = p.video.ok_or("not a video")?;
    let n = |s: &str| -> Option<f64> {
        if let Some(r) = s.strip_prefix("min(") {
            let (a, b) = r.trim_end_matches(')').split_once(',')?;
            let v = |x: &str| if x == "ih" { Some(i.height as f64) } else if x == "iw" { Some(i.width as f64) } else { x.parse().ok() };
            return Some(v(a)?.min(v(b)?));
        }
        s.parse().ok()
    };
    match (n(w), n(h)) {
        (_, Some(h)) if h > 0.0 => Ok(h as u32),
        (Some(w), _) if w > 0.0 => Ok((i.height as f64 * w / i.width as f64).round() as u32),
        _ => Err(format!("scale={spec}: a width or a height in pixels")),
    }
}

/// `rdb ffprobe ARGS`: an ffprobe command line answered from rdb media probe, in the format it asks for.
pub fn ffprobe_cli(args: &[String]) -> i32 {
    match ffprobe(args) {
        Ok(s) => {
            print!("{s}");
            0
        }
        Err(e) => {
            eprintln!("rdb ffprobe: {e}");
            1
        }
    }
}

fn ffprobe(args: &[String]) -> R<String> {
    let (mut entries, mut of, mut sel, mut file) = (None::<String>, String::from("default"), None::<String>, None::<String>);
    let (mut show_format, mut show_streams) = (false, false);
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut val = || {
            i += 1;
            args.get(i).cloned().ok_or_else(|| format!("{a} needs a value"))
        };
        match a {
            "-show_entries" => entries = Some(val()?),
            "-of" | "-print_format" | "-output_format" => of = val()?,
            "-select_streams" => sel = Some(val()?),
            "-v" | "-loglevel" | "-i" => {
                let v = val()?;
                if a == "-i" {
                    file = Some(v);
                }
            }
            "-show_format" => show_format = true,
            "-show_streams" => show_streams = true,
            "-hide_banner" | "-count_frames" => {}
            o if o.starts_with('-') => return Err(format!("{o} isn't read here; rdb media probe FILE gives everything as JSON")),
            f => file = Some(f.to_string()),
        }
        i += 1;
    }
    let path = PathBuf::from(file.ok_or("which file?")?);
    let p = probe_of(&path)?;
    // the streams: video, then audio (an image is one video stream)
    let mut streams: Vec<(char, Vec<(&str, Value)>)> = Vec::new();
    if let Some(v) = &p.video {
        // ffprobe reports the stored size (a phone's portrait take is stored landscape and rotated on show)
        let turned = raea_media::soft::rotation_of(&path).rem_euclid(180) == 90;
        let (w, h) = if turned { (v.height, v.width) } else { (v.width, v.height) };
        streams.push(('v', vec![
            ("codec_name", json!(v.codec)), ("codec_type", json!("video")), ("width", json!(w)), ("height", json!(h)),
            ("r_frame_rate", json!(rate_fraction(p.track.as_ref().map(|t| t.grid_fps).unwrap_or(v.fps)))), ("avg_frame_rate", json!(rate_fraction(v.fps))),
            ("duration", json!(format!("{:.6}", v.duration))), ("nb_frames", json!(v.frames.to_string())),
        ]));
    }
    if let Some((w, h)) = p.image {
        streams.push(('v', vec![("codec_type", json!("video")), ("width", json!(w)), ("height", json!(h))]));
    }
    if let Some((rate, ch, d)) = p.audio {
        streams.push(('a', vec![("codec_type", json!("audio")), ("sample_rate", json!(rate.to_string())), ("channels", json!(ch)), ("duration", json!(format!("{d:.6}")))]));
    }
    if let Some(s) = &sel {
        let (kind, idx) = s.split_once(':').map(|(k, n)| (k, n.parse::<usize>().ok())).unwrap_or((s.as_str(), None));
        let kind = kind.chars().next().unwrap_or('v');
        let picked: Vec<_> = streams.into_iter().filter(|(k, _)| *k == kind).collect();
        streams = match idx {
            Some(n) => picked.into_iter().nth(n).into_iter().collect(),
            None => picked,
        };
    }
    let format: Vec<(&str, Value)> = vec![
        ("filename", json!(path)), ("nb_streams", json!(streams.len())), ("duration", json!(format!("{:.6}", p.duration))), ("size", json!(p.bytes.to_string())),
    ];
    // which fields of which section
    let mut want_s: Option<Vec<String>> = None;
    let mut want_f: Option<Vec<String>> = None;
    if let Some(e) = &entries {
        for sec in e.split(':') {
            let (name, fields) = sec.split_once('=').unwrap_or((sec, ""));
            let fields: Vec<String> = fields.split(',').filter(|f| !f.is_empty()).map(str::to_string).collect();
            match name {
                "stream" => want_s = Some(fields),
                "format" => want_f = Some(fields),
                o => return Err(format!("-show_entries {o} isn't read here (stream=..., format=...)")),
            }
        }
    } else {
        if show_streams {
            want_s = Some(Vec::new());
        }
        if show_format {
            want_f = Some(Vec::new());
        }
    }
    let pick = |all: &[(&str, Value)], want: &[String]| -> Vec<(String, Value)> {
        if want.is_empty() {
            return all.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        }
        want.iter().filter_map(|w| all.iter().find(|(k, _)| k == w).map(|(k, v)| (k.to_string(), v.clone()))).collect()
    };
    let mut sections: Vec<(&str, Vec<(String, Value)>)> = Vec::new();
    if let Some(w) = &want_s {
        for (_, s) in &streams {
            sections.push(("STREAM", pick(s, w)));
        }
    }
    if let Some(w) = &want_f {
        sections.push(("FORMAT", pick(&format, w)));
    }
    let plain = |v: &Value| match v {
        Value::String(s) => s.clone(),
        o => o.to_string(),
    };
    let (kind, opts) = of.split_once('=').unwrap_or((of.as_str(), ""));
    let mut s = String::new();
    match kind {
        "csv" | "compact" => {
            let no_prefix = opts.split(':').any(|o| o == "p=0" || o == "print_section=0");
            for (name, fields) in &sections {
                let vals: Vec<String> = fields.iter().map(|(k, v)| if kind == "compact" { format!("{k}={}", plain(v)) } else { plain(v) }).collect();
                let sep = if kind == "compact" { "|" } else { "," };
                if no_prefix {
                    s.push_str(&vals.join(sep));
                } else {
                    s.push_str(&format!("{}{sep}{}", name.to_lowercase(), vals.join(sep)));
                }
                s.push('\n');
            }
        }
        "json" => {
            let obj = |f: &Vec<(String, Value)>| Value::Object(f.iter().cloned().collect());
            let mut root = serde_json::Map::new();
            if want_s.is_some() {
                root.insert("streams".into(), Value::Array(sections.iter().filter(|(n, _)| *n == "STREAM").map(|(_, f)| obj(f)).collect()));
            }
            if let Some((_, f)) = sections.iter().find(|(n, _)| *n == "FORMAT") {
                root.insert("format".into(), obj(f));
            }
            s = serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default();
            s.push('\n');
        }
        "default" | "flat" | "ini" => {
            let nw = opts.split(':').any(|o| o == "nw=1" || o == "noprint_wrappers=1");
            let nk = opts.split(':').any(|o| o == "nk=1" || o == "nokey=1");
            for (name, fields) in &sections {
                if !nw {
                    s.push_str(&format!("[{name}]\n"));
                }
                for (k, v) in fields {
                    if nk {
                        s.push_str(&format!("{}\n", plain(v)));
                    } else {
                        s.push_str(&format!("{k}={}\n", plain(v)));
                    }
                }
                if !nw {
                    s.push_str(&format!("[/{name}]\n"));
                }
            }
        }
        o => return Err(format!("-of {o} isn't read here (csv, compact, json, default)")),
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_and_rates_read_as_ffmpeg_writes_them() {
        assert_eq!(secs("12.5").unwrap(), 12.5);
        assert_eq!(secs("00:01:02.5").unwrap(), 62.5);
        assert_eq!(rate_fraction(30.0), "30/1");
        assert_eq!(rate_fraction(29.97), "30000/1001");
        // a measured rate snaps to the nearest standard one within 1%
        assert_eq!(rate_fraction(24.03), "24/1");
        assert_eq!(rate_fraction(23.95), "24000/1001");
        assert_eq!(rate_fraction(12.5), "12500/1000");
    }

    #[test]
    fn one_scale_filter_is_not_a_chain() {
        assert!(!chained("scale=-2:'min(720,ih)'"));
        assert!(!chained("scale=-2:min(720\\,ih)"));
        assert!(chained("scale=-2:720,fps=30"));
    }

    #[test]
    fn ffmpeg_samples_the_middle_of_each_tick() {
        // fps=6 over 30 fps footage takes source frames 2, 7, 12 (the last before each tick's middle)
        let info = Info { codec: "h264".into(), width: 2, height: 2, fps: 30.0, duration: 1.0, frames: 30 };
        let t = times(info.duration, None, None, 6.0);
        assert_eq!(t.len(), 6);
        let frame = |s: f64| (s * 30.0).floor() as i64;
        assert_eq!(t.iter().map(|(_, at)| frame(*at)).collect::<Vec<_>>(), vec![2, 7, 12, 17, 22, 27]);
        assert_eq!(t[1].0, 1.0 / 6.0);
    }

    #[test]
    fn wav_is_16_bit_pcm() {
        let dir = std::env::temp_dir().join(format!("rdb-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.wav");
        write_wav(&Audio { rate: 16000, channels: vec![vec![0.0, 0.5, -1.0]] }, &p).unwrap();
        let b = std::fs::read(&p).unwrap();
        assert_eq!(&b[..4], b"RIFF");
        assert_eq!(b.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(b[24..28].try_into().unwrap()), 16000);
        assert_eq!(i16::from_le_bytes(b[46..48].try_into().unwrap()), 16384);
        assert_eq!(i16::from_le_bytes(b[48..50].try_into().unwrap()), -32767);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_isnt_read_is_refused_with_the_way_to_do_it() {
        let a = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
        let e = ffmpeg(&a("-i x.mp4 -filter_complex overlay out.mp4")).unwrap_err();
        assert!(e.contains("an edit"), "{e}");
        let e = ffmpeg(&a("-i x.mp4 -vf hue=s=0 out.mp4")).unwrap_err();
        assert!(e.contains("isn't read here"), "{e}");
    }
}
