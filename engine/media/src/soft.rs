//! (b) Portable software, no ffmpeg: re_mp4 demuxes, openh264 decodes and encodes H.264, Symphonia decodes audio,
//! libopus encodes Opus, muxide writes the MP4. Builds wherever a C compiler does (macOS, iOS, Linux, Android, wasm).
//!
//! Decoding wants an edit mezzanine: H.264 without B-frames (decode order = display order), keyframes every second,
//! so the frame at any t is at most one GOP of decodes away. Phone HEVC goes through a platform decoder (or is
//! transcoded once at ingest): there is no permissive portable HEVC decoder.
//!
//! The H.264 half (openh264) is the `h264` feature; without it this backend decodes audio and muxes, and its video
//! calls say so (an exported game's player: lightspeed R7).
#![cfg_attr(not(feature = "h264"), allow(dead_code, unused_imports))]

use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(feature = "h264")]
use openh264::OpenH264API;
#[cfg(feature = "h264")]
use openh264::decoder::Decoder as H264Decoder;
#[cfg(feature = "h264")]
use openh264::encoder::{
    BitRate, Encoder as H264Encoder, EncoderConfig, FrameRate, RateControlMode,
};
#[cfg(feature = "h264")]
use openh264::formats::{YUVSlices, YUVSource};

use crate::{
    Audio, Backend, EncodeOpts, EncodeStats, Frame, Info, R, VideoDecoder, VideoEncoder, as_of,
};

pub struct Soft;

/// A demuxed video track: samples in decode order, their display times, and the codec config.
pub(crate) struct Track {
    pub bytes: Vec<u8>,
    pub samples: Vec<(usize, usize, bool, f64)>, // (offset, size, is_sync, pts seconds)
    pub config: Vec<u8>,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    /// clockwise degrees to display it (the video track's tkhd matrix)
    pub rotation: i32,
}

/// A file's first video track's display rotation (clockwise degrees; 0 when it has none or isn't an MP4/MOV).
pub fn rotation_of(path: &Path) -> i32 {
    std::fs::read(path).map(|b| mp4_rotation(&b)).unwrap_or(0)
}

/// The first video track's display rotation, from its tkhd matrix (QuickTime's 16.16 a, b, c, d).
fn mp4_rotation(b: &[u8]) -> i32 {
    fn boxes(b: &[u8]) -> Vec<(&[u8], &[u8])> {
        let mut out = Vec::new();
        let mut i = 0;
        while i + 8 <= b.len() {
            let mut size = u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
            let mut head = 8;
            if size == 1 && i + 16 <= b.len() {
                size = u64::from_be_bytes(b[i + 8..i + 16].try_into().unwrap()) as usize;
                head = 16;
            } else if size == 0 {
                size = b.len() - i;
            }
            if size < head || i + size > b.len() {
                break;
            }
            out.push((&b[i + 4..i + 8], &b[i + head..i + size]));
            i += size;
        }
        out
    }
    let Some((_, moov)) = boxes(b).into_iter().find(|(k, _)| *k == b"moov") else { return 0 };
    for (k, trak) in boxes(moov) {
        if k != b"trak" {
            continue;
        }
        let kids = boxes(trak);
        let is_video = kids
            .iter()
            .find(|(k, _)| *k == b"mdia")
            .and_then(|(_, m)| boxes(m).into_iter().find(|(k, _)| *k == b"hdlr"))
            .is_some_and(|(_, h)| h.get(8..12) == Some(b"vide"));
        if !is_video {
            continue;
        }
        let Some((_, tkhd)) = kids.iter().find(|(k, _)| *k == b"tkhd") else { return 0 };
        let m = if tkhd[0] == 1 { 4 + 8 + 8 + 4 + 4 + 8 + 8 + 2 + 2 + 2 + 2 } else { 4 + 4 + 4 + 4 + 4 + 4 + 8 + 2 + 2 + 2 + 2 };
        let fx = |o: usize| tkhd.get(m + o..m + o + 4).map(|x| i32::from_be_bytes(x.try_into().unwrap()) as f64 / 65536.0).unwrap_or(0.0);
        let (a, bb) = (fx(0), fx(4));
        return (bb.atan2(a).to_degrees().round() as i32).rem_euclid(360);
    }
    0
}

pub(crate) fn demux(path: &Path, want: &str) -> R<Track> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mp4 = re_mp4::Mp4::read_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let (_, t) = mp4
        .tracks()
        .iter()
        .find(|(_, t)| t.kind == Some(re_mp4::TrackKind::Video))
        .ok_or(format!("{}: no video track", path.display()))?;
    let codec = t.codec_string(&mp4).unwrap_or_default();
    if !codec.starts_with(want) {
        return Err(format!(
            "{}: {codec}, not {want} (this backend reads {want} mezzanines)",
            path.display()
        ));
    }
    let config = t.raw_codec_config(&mp4).ok_or("no codec config")?;
    let samples = t
        .samples
        .iter()
        .map(|s| {
            (
                s.offset as usize,
                s.size as usize,
                s.is_sync,
                s.composition_timestamp as f64 / s.timescale as f64,
            )
        })
        .collect();
    let rotation = mp4_rotation(&bytes);
    Ok(Track {
        rotation,
        codec,
        width: t.width as u32,
        height: t.height as u32,
        duration: t.duration as f64 / t.timescale as f64,
        config,
        samples,
        bytes,
    })
}

/// AVCC (length-prefixed NALs) to Annex B; the avcC record's SPS and PPS as Annex B NALs.
fn annexb(sample: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(sample.len() + 16);
    let mut i = 0;
    while i + 4 <= sample.len() {
        let n =
            u32::from_be_bytes([sample[i], sample[i + 1], sample[i + 2], sample[i + 3]]) as usize;
        i += 4;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&sample[i..(i + n).min(sample.len())]);
        i += n;
    }
    out
}

fn avcc_params(avcc: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 5;
    let nsps = (avcc[i] & 0x1f) as usize;
    i += 1;
    for _ in 0..nsps {
        let n = u16::from_be_bytes([avcc[i], avcc[i + 1]]) as usize;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&avcc[i + 2..i + 2 + n]);
        i += 2 + n;
    }
    let npps = avcc[i] as usize;
    i += 1;
    for _ in 0..npps {
        let n = u16::from_be_bytes([avcc[i], avcc[i + 1]]) as usize;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&avcc[i + 2..i + 2 + n]);
        i += 2 + n;
    }
    out
}

#[cfg(feature = "h264")]
struct Decoder {
    track: Track,
    info: Info,
    pts: Vec<f64>,
    params: Vec<u8>,
    dec: Option<H264Decoder>,
    /// the index (decode order = display order) of the last frame decoded
    last: Option<usize>,
    /// RGBA, or NV12 when `nv12`
    out: Vec<u8>,
    nv12: bool,
}

#[cfg(feature = "h264")]
impl Decoder {
    /// The decoded picture at display orientation.
    fn display(&self) -> Vec<u8> {
        let (w, h, r) = (self.track.width as usize, self.track.height as usize, self.track.rotation);
        if self.nv12 { crate::yuv::rotate_nv12(&self.out, w, h, r).0 } else { crate::yuv::rotate(&self.out, w, h, r).0 }
    }

    /// Switch between RGBA and NV12 output (the next frame decodes afresh).
    fn mode(&mut self, nv12: bool) {
        if self.nv12 != nv12 {
            self.nv12 = nv12;
            self.last = None;
        }
    }

    fn decode_sample(&mut self, i: usize) -> R<bool> {
        let (off, size, _, _) = self.track.samples[i];
        let nal = annexb(&self.track.bytes[off..off + size]);
        let dec = self.dec.as_mut().unwrap();
        match dec.decode(&nal).map_err(|e| format!("openh264: {e}"))? {
            Some(yuv) => {
                let (w, h) = yuv.dimensions();
                let (ys, us, vs) = yuv.strides();
                if self.nv12 {
                    crate::yuv::i420_to_nv12(yuv.y(), yuv.u(), yuv.v(), ys, us, vs, w, h, &mut self.out);
                } else {
                    // our BT.709 conversion (openh264's write_rgba8 is BT.601, which shifts colour against every
                    // other decoder on HD sources)
                    crate::yuv::i420_to_rgba(yuv.y(), yuv.u(), yuv.v(), ys, us, vs, w, h, &mut self.out);
                }
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[cfg(feature = "h264")]
impl VideoDecoder for Decoder {
    fn info(&self) -> &Info {
        &self.info
    }
    fn index_at(&mut self, t: f64) -> Option<usize> {
        (!self.pts.is_empty()).then(|| crate::as_of(&self.pts, t))
    }
    fn keyframes(&self) -> Option<Vec<usize>> {
        // samples are in presentation order here (openh264 streams without B-frames)
        Some(self.track.samples.iter().enumerate().filter(|(_, s)| s.2).map(|(i, _)| i).collect())
    }
    fn frame_nv12_at(&mut self, t: f64) -> R<Frame> {
        self.mode(true);
        self.frame_inner(t)
    }
    fn frame_at(&mut self, t: f64) -> R<Frame> {
        self.mode(false);
        self.frame_inner(t)
    }
}

#[cfg(feature = "h264")]
impl Decoder {
    fn frame_inner(&mut self, t: f64) -> R<Frame> {
        let want = as_of(&self.pts, t);
        // forward within the GOP: keep decoding; otherwise restart from the keyframe at or before `want`
        let from = match self.last {
            Some(l) if l <= want && !self.track.samples[l + 1..=want].iter().any(|s| s.2) => l + 1,
            Some(l) if l == want => {
                return Ok(Frame {
                    pts: self.pts[want],
                    width: self.info.width,
                    height: self.info.height,
                    rgba: self.display(),
                });
            }
            _ => {
                let key = (0..=want)
                    .rev()
                    .find(|&i| self.track.samples[i].2)
                    .unwrap_or(0);
                self.dec = Some(H264Decoder::new().map_err(|e| format!("openh264: {e}"))?);
                let params = self.params.clone();
                self.dec
                    .as_mut()
                    .unwrap()
                    .decode(&params)
                    .map_err(|e| format!("openh264: {e}"))?;
                key
            }
        };
        for i in from..=want {
            self.decode_sample(i)?;
            self.last = Some(i);
        }
        Ok(Frame {
            pts: self.pts[want],
            width: self.info.width,
            height: self.info.height,
            rgba: self.display(),
        })
    }
}

/// The sound track's codec, as Symphonia names it (aac, mp3, opus, pcm_s16le, ...), with its rate and channels.
pub fn audio_facts(path: &Path) -> R<(String, u32, usize)> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("symphonia: {e}"))?;
    let t = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some())
        .ok_or("no audio track")?
        .clone();
    let name = symphonia::default::get_codecs()
        .get_codec(t.codec_params.codec)
        .map(|d| d.short_name.to_string())
        .unwrap_or_else(|| format!("{:?}", t.codec_params.codec));
    Ok((
        name,
        t.codec_params.sample_rate.unwrap_or(0),
        t.codec_params.channels.map(|c| c.count()).unwrap_or(0),
    ))
}

pub(crate) fn decode_audio_symphonia(path: &Path) -> R<Audio> {
    use symphonia::core::audio::{AudioBufferRef, Signal};
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("symphonia: {e}"))?;
    let mut format = probed.format;
    // the first track with a sample rate is the sound (a .mov's default track is often the picture)
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some())
        .ok_or("no audio track")?
        .clone();
    let mut dec = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("symphonia: {e}"))?;
    let mut rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut cols: Vec<Vec<f32>> = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track.id {
            continue;
        }
        let buf = match dec.decode(&packet) {
            Ok(b) => b,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("symphonia: {e}")),
        };
        // the track can say 0 (Symphonia's MP4 reader on a 96 kHz AAC track, void's colitt take09): the decoded
        // buffer knows its rate
        if rate == 0 {
            rate = buf.spec().rate;
        }
        let mut push = |planes: &[&[f32]]| {
            if cols.is_empty() {
                cols = vec![Vec::new(); planes.len()];
            }
            for (c, p) in cols.iter_mut().zip(planes) {
                c.extend_from_slice(p);
            }
        };
        match buf {
            AudioBufferRef::F32(b) => {
                let planes: Vec<&[f32]> =
                    (0..b.spec().channels.count()).map(|c| b.chan(c)).collect();
                push(&planes);
            }
            other => {
                let mut f = symphonia::core::audio::AudioBuffer::<f32>::new(
                    other.capacity() as u64,
                    *other.spec(),
                );
                other.convert(&mut f);
                let planes: Vec<&[f32]> =
                    (0..f.spec().channels.count()).map(|c| f.chan(c)).collect();
                push(&planes);
            }
        }
    }
    // MP4 priming: the audio track's edit list says where the sound starts (AAC encoders put 1024-2112 silent
    // samples first); Symphonia 0.5 doesn't apply it, ffmpeg and AVFoundation do
    // (Apple's encoders write it as an iTunSMPB tag instead: priming, padding, and the true length)
    if let Some((skip, len)) = itunsmpb(path) {
        for c in &mut cols {
            c.drain(..skip.min(c.len()));
            c.truncate(len);
        }
    } else if let Some(skip) = mp4_audio_skip(path) {
        for c in &mut cols {
            c.drain(..skip.min(c.len()));
        }
    }
    if rate == 0 {
        return Err(format!("{}: the sound track has no sample rate", path.display()));
    }
    Ok(Audio {
        rate,
        channels: cols,
    })
}

/// `secs` seconds of a file's sound from `start` (seconds, in the same time as decode_audio_symphonia's: priming
/// already taken off): a seek and a short decode, for a player that streams a long take instead of decoding it whole
/// (a ten-minute take decoded whole cost a 1.9 s stall and 1.2 GB in live play).
pub(crate) fn decode_audio_window_symphonia(path: &Path, start: f64, secs: f64) -> R<Audio> {
    use symphonia::core::audio::{AudioBufferRef, Signal};
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    use symphonia::core::units::Time;
    static SKIPS: std::sync::Mutex<Option<std::collections::HashMap<std::path::PathBuf, usize>>> = std::sync::Mutex::new(None);
    let skip = {
        let mut g = SKIPS.lock().unwrap();
        *g.get_or_insert_with(Default::default).entry(path.to_path_buf()).or_insert_with(|| itunsmpb(path).map(|(s, _)| s).or_else(|| mp4_audio_skip(path)).unwrap_or(0))
    };
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
        .map_err(|e| format!("symphonia: {e}"))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.sample_rate.is_some()).ok_or("no audio track")?.clone();
    let mut dec = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| format!("symphonia: {e}"))?;
    let rate = track.codec_params.sample_rate.unwrap_or(0);
    if rate == 0 {
        return Err(format!("{}: the sound track has no sample rate", path.display()));
    }
    // samples in the file's own count (priming included), where the window starts and how many it holds
    let first = (start.max(0.0) * rate as f64).round() as u64 + skip as u64;
    let want = (secs * rate as f64).round() as usize;
    // seek a little before: AAC's first packet after a seek decodes without its overlap, so the decoder warms up on
    // packets that are then thrown away
    let t = first.saturating_sub(4096) as f64 / rate as f64;
    let seeked = format.seek(SeekMode::Accurate, SeekTo::Time { time: Time::new(t as u64, t.fract()), track_id: Some(track.id) });
    let mut at = seeked.map(|s| s.actual_ts).unwrap_or(0);
    dec.reset();
    let mut cols: Vec<Vec<f32>> = Vec::new();
    let mut have = 0usize;
    while have < want {
        let Ok(packet) = format.next_packet() else { break };
        if packet.track_id() != track.id {
            continue;
        }
        let buf = match dec.decode(&packet) {
            Ok(b) => b,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("symphonia: {e}")),
        };
        let mut f = symphonia::core::audio::AudioBuffer::<f32>::new(buf.capacity() as u64, *buf.spec());
        match buf {
            AudioBufferRef::F32(b) => b.convert(&mut f),
            other => other.convert(&mut f),
        }
        let n = f.frames();
        let ts = packet.ts().max(at);
        // the part of this packet inside the window
        let from = first.saturating_sub(ts) as usize;
        if from < n {
            if cols.is_empty() {
                cols = vec![Vec::new(); f.spec().channels.count()];
            }
            let take = (n - from).min(want - have);
            for (c, col) in cols.iter_mut().enumerate() {
                col.extend_from_slice(&f.chan(c)[from..from + take]);
            }
            have += take;
        }
        at = ts + n as u64;
    }
    Ok(Audio { rate, channels: cols })
}

/// How long a file's sound is, from its container (frames over rate), without decoding it.
pub(crate) fn probe_seconds_symphonia(path: &Path) -> Option<f64> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).ok()?;
    let t = probed.format.tracks().iter().find(|t| t.codec_params.sample_rate.is_some())?.codec_params.clone();
    Some(t.n_frames? as f64 / t.sample_rate? as f64)
}

/// An MP4's `moov` box (header included), read by walking the top-level boxes with seeks: the tags and edit lists
/// live there, and reading the whole file for them (a ten-minute take is over a gigabyte) stalled a player for seconds.
fn moov(path: &Path) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut at = 0u64;
    while at + 8 <= len {
        f.seek(SeekFrom::Start(at)).ok()?;
        let mut h = [0u8; 16];
        f.read_exact(&mut h[..8]).ok()?;
        let mut size = u32::from_be_bytes(h[..4].try_into().unwrap()) as u64;
        if size == 1 {
            f.read_exact(&mut h[8..16]).ok()?;
            size = u64::from_be_bytes(h[8..16].try_into().unwrap());
        } else if size == 0 {
            size = len - at;
        }
        if size < 8 {
            return None;
        }
        if &h[4..8] == b"moov" {
            let mut b = vec![0u8; size.min(64 << 20) as usize];
            f.seek(SeekFrom::Start(at)).ok()?;
            f.read_exact(&mut b).ok()?;
            return Some(b);
        }
        at += size;
    }
    None
}

/// iTunes gapless info: " 00000000 PRIMING PADDING LENGTH ..." (hex), stored as text after the tag name.
fn itunsmpb(path: &Path) -> Option<(usize, usize)> {
    let b = moov(path)?;
    let at = b.windows(8).position(|w| w == b"iTunSMPB")?;
    let tail = &b[at + 8..(at + 8 + 200).min(b.len())];
    let start = tail.windows(9).position(|w| w == b" 00000000")?;
    let text: String = tail[start..]
        .iter()
        .take_while(|c| c.is_ascii_hexdigit() || **c == b' ')
        .map(|c| *c as char)
        .collect();
    let f: Vec<u64> = text
        .split_whitespace()
        .filter_map(|x| u64::from_str_radix(x, 16).ok())
        .collect();
    (f.len() >= 4 && f[3] > 0).then(|| (f[1] as usize, f[3] as usize))
}

/// The first sound track's edit-list start (`elst` media_time, in its own sample rate's units), from the MP4 boxes.
fn mp4_audio_skip(path: &Path) -> Option<usize> {
    let b = moov(path)?;
    fn boxes(b: &[u8]) -> Vec<(&[u8], &[u8])> {
        let mut out = Vec::new();
        let mut i = 0;
        while i + 8 <= b.len() {
            let mut size = u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
            let kind = &b[i + 4..i + 8];
            let mut head = 8;
            if size == 1 && i + 16 <= b.len() {
                size = u64::from_be_bytes(b[i + 8..i + 16].try_into().unwrap()) as usize;
                head = 16;
            } else if size == 0 {
                size = b.len() - i;
            }
            if size < head || i + size > b.len() {
                break;
            }
            out.push((kind, &b[i + head..i + size]));
            i += size;
        }
        out
    }
    let moov = boxes(&b).into_iter().find(|(k, _)| *k == b"moov")?.1;
    for (k, trak) in boxes(moov) {
        if k != b"trak" {
            continue;
        }
        let kids = boxes(trak);
        let mdia = kids.iter().find(|(k, _)| *k == b"mdia")?.1;
        let hdlr = boxes(mdia).into_iter().find(|(k, _)| *k == b"hdlr")?.1;
        if hdlr.get(8..12) != Some(b"soun") {
            continue;
        }
        let edts = kids.iter().find(|(k, _)| *k == b"edts")?.1;
        let elst = boxes(edts).into_iter().find(|(k, _)| *k == b"elst")?.1;
        let version = elst[0];
        let count = u32::from_be_bytes(elst[4..8].try_into().ok()?);
        if count == 0 {
            return None;
        }
        // entry: segment duration, media time (signed), rate; an empty edit (media_time -1) first means a delay instead
        let media_time = if version == 1 {
            i64::from_be_bytes(elst[16..24].try_into().ok()?)
        } else {
            i32::from_be_bytes(elst[12..16].try_into().ok()?) as i64
        };
        return (media_time > 0).then_some(media_time as usize);
    }
    None
}

pub use crate::soft_resample as resample;

/// Opus packets (20 ms) for the audio columns at 48 kHz: (pts seconds, packet).
pub(crate) fn opus_packets(a: &Audio) -> R<Vec<(f64, Vec<u8>)>> {
    let a = resample(a, 48000);
    let ch = a.channels.len().clamp(1, 2);
    let mut enc = opus::Encoder::new(
        48000,
        if ch == 2 {
            opus::Channels::Stereo
        } else {
            opus::Channels::Mono
        },
        opus::Application::Audio,
    )
    .map_err(|e| format!("opus: {e}"))?;
    enc.set_bitrate(opus::Bitrate::Bits(128_000))
        .map_err(|e| format!("opus: {e}"))?;
    let frame = 960;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 4000];
    let mut i = 0;
    while i < a.len() {
        let mut pcm = Vec::with_capacity(frame * ch);
        for k in 0..frame {
            for c in 0..ch {
                pcm.push(a.channels[c].get(i + k).copied().unwrap_or(0.0));
            }
        }
        let n = enc
            .encode_float(&pcm, &mut buf)
            .map_err(|e| format!("opus: {e}"))?;
        out.push((i as f64 / 48000.0, buf[..n].to_vec()));
        i += frame;
    }
    Ok(out)
}

#[cfg(feature = "h264")]
struct Encoder {
    enc: H264Encoder,
    opts: EncodeOpts,
    out: PathBuf,
    frames: Vec<(Vec<u8>, bool)>,
}

#[cfg(feature = "h264")]
impl VideoEncoder for Encoder {
    fn push(&mut self, rgba: &[u8]) -> R<()> {
        let (w, h) = (self.opts.width as usize, self.opts.height as usize);
        let (y, u, v) = crate::yuv::rgba_to_i420(rgba, w, h);
        self.push_i420(&y, &u, &v)
    }
    fn push_nv12(&mut self, nv12: &[u8], w: usize, h: usize) -> R<()> {
        let (y, u, v) = crate::yuv::nv12_to_i420(nv12, w, h);
        self.push_i420(&y, &u, &v)
    }
    fn finish(self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats> {
        mux_h264(&self.out, &self.opts, &self.frames, audio)
    }
}

#[cfg(feature = "h264")]
impl Encoder {
    fn push_i420(&mut self, y: &[u8], u: &[u8], v: &[u8]) -> R<()> {
        let (w, h) = (self.opts.width as usize, self.opts.height as usize);
        let src = YUVSlices::new((y, u, v), (w, h), (w, w / 2, w / 2));
        let bs = self
            .enc
            .encode(&src)
            .map_err(|e| format!("openh264: {e}"))?;
        let key = matches!(
            bs.frame_type(),
            openh264::encoder::FrameType::IDR | openh264::encoder::FrameType::I
        );
        self.frames.push((bs.to_vec(), key));
        Ok(())
    }
}

/// H.264 samples (Annex B, one per frame, display order, with key flags) and the audio columns (as Opus) into an MP4.
pub fn mux_h264(
    out: &Path,
    opts: &EncodeOpts,
    frames: &[(Vec<u8>, bool)],
    audio: Option<&Audio>,
) -> R<EncodeStats> {
    let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let mut b = muxide::api::MuxerBuilder::new(std::io::BufWriter::new(file))
        .video(
            muxide::api::VideoCodec::H264,
            opts.width,
            opts.height,
            opts.fps,
        )
        .with_fast_start(true);
    let packets = match audio {
        Some(a) => Some(opus_packets(a)?),
        None => None,
    };
    if let Some(a) = audio {
        b = b.audio(
            muxide::api::AudioCodec::Opus,
            48000,
            a.channels.len().clamp(1, 2) as u16,
        );
    }
    let mut m = b.build().map_err(|e| format!("muxide: {e}"))?;
    // interleave by time
    let mut ai = 0;
    for (k, (data, key)) in frames.iter().enumerate() {
        let pts = k as f64 / opts.fps;
        if let Some(p) = &packets {
            while ai < p.len() && p[ai].0 < pts {
                m.write_audio(p[ai].0, &p[ai].1)
                    .map_err(|e| format!("muxide audio: {e}"))?;
                ai += 1;
            }
        }
        m.write_video(pts, data, *key)
            .map_err(|e| format!("muxide video: {e}"))?;
    }
    let end = frames.len() as f64 / opts.fps;
    if let Some(p) = &packets {
        for (pts, d) in p[ai..].iter().filter(|x| x.0 < end) {
            m.write_audio(*pts, d)
                .map_err(|e| format!("muxide audio: {e}"))?;
        }
    }
    m.finish().map_err(|e| format!("muxide: {e}"))?;
    Ok(EncodeStats {
        frames: frames.len(),
        bytes: std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0),
        video_codec: "h264 (openh264)".into(),
        audio_codec: if audio.is_some() {
            "opus (libopus)".into()
        } else {
            String::new()
        },
    })
}

/// An edit mezzanine: the source decoded by `decoder` (the platform's, or ffmpeg's where there is none) and encoded
/// as H.264 the portable decoders take: no B-frames, a keyframe every `gop` frames, fixed QP (16: visually lossless
/// for edit sources), at the source's frame rate and display size (rotation applied). Video only.
#[cfg(feature = "h264")]
pub fn mezzanine(input: &Path, out: &Path, decoder: &dyn crate::Backend, gop: usize, qp: u8) -> R<usize> {
    let mut d = decoder.open_video(input)?;
    let info = d.info().clone();
    let fps = info.fps;
    let mut frames: Vec<(Vec<u8>, bool)> = Vec::with_capacity(info.frames);
    let mut enc = GopEncoder::new(info.width, info.height, fps, qp)?;
    for i in 0..info.frames {
        if i % gop == 0 {
            enc = GopEncoder::new(info.width, info.height, fps, qp)?;
        }
        let f = d.frame_at(i as f64 / fps)?;
        frames.push(enc.push(&f.rgba)?);
    }
    let opts = EncodeOpts { width: info.width, height: info.height, fps, kbps: 0, nv12: false, realtime: false };
    mux_h264(out, &opts, &frames, None)?;
    Ok(frames.len())
}

/// An encoder for one GOP that depends on nothing before it: fixed quantiser (no rate control state carried
/// between frames), the first frame an IDR with its SPS/PPS, no other keyframes. Two GOPs made from the same frames
/// are the same bytes, so an export can be stitched from GOPs made at different times.
#[cfg(feature = "h264")]
pub struct GopEncoder {
    enc: H264Encoder,
    w: usize,
    h: usize,
}

#[cfg(feature = "h264")]
impl GopEncoder {
    pub fn new(w: u32, h: u32, fps: f64, qp: u8) -> R<GopEncoder> {
        let config = EncoderConfig::new()
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .rate_control_mode(RateControlMode::Off)
            .qp(openh264::encoder::QpRange::new(qp, qp))
            .skip_frames(false)
            .scene_change_detect(false)
            .adaptive_quantization(false)
            .background_detection(false)
            .intra_frame_period(openh264::encoder::IntraFramePeriod::from_num_frames(
                100_000,
            ))
            .num_threads(1);
        let enc = H264Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|e| format!("openh264: {e}"))?;
        Ok(GopEncoder {
            enc,
            w: w as usize,
            h: h as usize,
        })
    }
    pub fn push(&mut self, rgba: &[u8]) -> R<(Vec<u8>, bool)> {
        let (y, u, v) = crate::yuv::rgba_to_i420(rgba, self.w, self.h);
        let src = YUVSlices::new(
            (&y, &u, &v),
            (self.w, self.h),
            (self.w, self.w / 2, self.w / 2),
        );
        let bs = self
            .enc
            .encode(&src)
            .map_err(|e| format!("openh264: {e}"))?;
        let key = matches!(
            bs.frame_type(),
            openh264::encoder::FrameType::IDR | openh264::encoder::FrameType::I
        );
        Ok((bs.to_vec(), key))
    }
}

impl Backend for Soft {
    fn name(&self) -> &'static str {
        "soft"
    }
    #[cfg(not(feature = "h264"))]
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>> {
        Err(format!("{}: this build's soft backend has no H.264 (the h264 feature)", path.display()))
    }
    #[cfg(feature = "h264")]
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>> {
        let track = demux(path, "avc1")?;
        let params = avcc_params(&track.config);
        let pts: Vec<f64> = track.samples.iter().map(|s| s.3).collect();
        if pts.windows(2).any(|w| w[1] < w[0]) {
            return Err(format!(
                "{}: has B-frames (display order differs from decode order); use an edit mezzanine",
                path.display()
            ));
        }
        let fps = if pts.len() > 1 {
            (pts.len() - 1) as f64 / (pts[pts.len() - 1] - pts[0]).max(1e-9)
        } else {
            30.0
        };
        // the display size: a quarter turn swaps the sides
        let (dw, dh) = if track.rotation % 180 == 90 { (track.height, track.width) } else { (track.width, track.height) };
        let info = Info {
            codec: track.codec.clone(),
            width: dw,
            height: dh,
            fps,
            duration: track.duration,
            frames: pts.len(),
        };
        Ok(Box::new(Decoder {
            info,
            pts,
            params,
            dec: None,
            last: None,
            out: Vec::new(),
            nv12: false,
            track,
        }))
    }
    fn decode_audio(&self, path: &Path) -> R<Audio> {
        decode_audio_symphonia(path)
    }
    #[cfg(not(feature = "h264"))]
    fn encoder(&self, out: &Path, _o: &EncodeOpts) -> R<Box<dyn VideoEncoder>> {
        Err(format!("{}: this build's soft backend has no H.264 (the h264 feature)", out.display()))
    }
    #[cfg(feature = "h264")]
    fn encoder(&self, out: &Path, o: &EncodeOpts) -> R<Box<dyn VideoEncoder>> {
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(o.kbps * 1000))
            .max_frame_rate(FrameRate::from_hz(o.fps as f32))
            .rate_control_mode(RateControlMode::Bitrate)
            // every frame is a sample: rate control may not drop one (the edit has a frame for every tick)
            .skip_frames(false);
        let enc = H264Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|e| format!("openh264: {e}"))?;
        let _ = std::io::stdout().flush();
        Ok(Box::new(Encoder {
            enc,
            opts: o.clone(),
            out: out.to_path_buf(),
            frames: Vec::new(),
        }))
    }
}

#[cfg(test)]
mod window_tests {
    /// A window of a file's sound equals that span of the whole decode (RAEA_TEST_AUDIO=FILE; skipped without it).
    #[test]
    fn a_window_matches_the_whole_decode() {
        let Some(f) = std::env::var_os("RAEA_TEST_AUDIO") else { return };
        let p = std::path::Path::new(&f);
        let whole = super::decode_audio_symphonia(p).unwrap();
        for start in [0.0, 10.0, 37.25] {
            let w = super::decode_audio_window_symphonia(p, start, 2.0).unwrap();
            let s0 = (start * whole.rate as f64).round() as usize;
            let n = w.channels[0].len().min(whole.channels[0].len() - s0);
            assert!(n > (whole.rate as usize) * 19 / 10, "window at {start} holds {n} samples");
            let err = (0..n).map(|i| (w.channels[0][i] - whole.channels[0][s0 + i]).abs()).fold(0.0f32, f32::max);
            assert!(err < 1e-4, "window at {start}: max difference {err}");
        }
    }
}
