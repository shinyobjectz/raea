//! `media probe`: what a file is (duration, size, fps, codec, audio), with the MP4 boxes read directly for a track's own timing.

use super::*;
use raea_io_mp4::{be32, be64, boxes, child};

pub(crate) fn round3(x: f64) -> f64 {
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
    let opened = open_video(path);
    let why = opened.as_ref().err().cloned();
    let video = opened.ok().map(|d| d.info().clone()).filter(|i| i.width > 0 && i.frames > 0).map(|mut i| {
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
        // what each backend said about the picture: a missing FFmpeg, a codec none of them reads
        let why = why.map(|w| format!(" ({})", w.split_once(": no backend reads it (").map_or(w.as_str(), |(_, r)| r.trim_end_matches(')')).to_string())).unwrap_or_default();
        return Err(format!("{}: no picture or sound raea can read{why}", path.display()));
    }
    // the container's own duration when it says one (ffprobe's format duration), else the longest stream
    // (a fragmented file's mvhd can say 0)
    let duration = container_duration(path).filter(|d| *d > 0.0).unwrap_or_else(|| video.as_ref().map(|v| v.duration).unwrap_or(0.0).max(audio.map(|a| a.2).unwrap_or(0.0)));
    Ok(Probe { kind: if video.is_some() { "video" } else { "audio" }, video, audio, image: None, duration, bytes, track })
}

/// An MP4 or QuickTime file's duration from its movie header (moov/mvhd): timescale and duration.
pub(crate) fn container_duration(path: &Path) -> Option<f64> {
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
        // a fragmented file's samples are in its moofs, not here
        let fragmented = deltas.is_empty();
        if fragmented {
            (deltas, times) = raea_io_mp4::fragments::samples(path, &moov, raea_io_mp4::fragments::track_id(trak)?)?;
        }
        if let (false, Some(ctts)) = (fragmented, child(stbl, b"ctts")) {
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
        // a fragmented file's mdhd can say 0: the samples' own span then
        let dur = if dur > 0.0 { dur } else { deltas.iter().map(|d| *d as f64).sum() };
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
pub fn standard_fps(fps: f64) -> f64 {
    let rates = [24000.0 / 1001.0, 24.0, 25.0, 30000.0 / 1001.0, 30.0, 48.0, 50.0, 60000.0 / 1001.0, 60.0, 120.0];
    // the nearest, within 1% (30 is 30, not 29.97)
    rates.into_iter().filter(|r| (fps - r).abs() / r < 0.01).min_by(|a, b| (fps - a).abs().total_cmp(&(fps - b).abs())).unwrap_or(fps)
}

/// A frame rate as ffprobe writes it: 30/1, 30000/1001.
pub(crate) fn rate_fraction(fps: f64) -> String {
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
