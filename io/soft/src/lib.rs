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

mod audio;
#[cfg(feature = "h264")]
mod h264;
/// packet-level audio decoding (Symphonia), for coders handed packets
pub mod packets;
mod probe;

pub use audio::*;
#[cfg(feature = "h264")]
pub use h264::*;
pub use probe::probe;

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

use raea_io_codec::{
    Audio, Backend, EncodeOpts, EncodeStats, Frame, Info, R, VideoDecoder, VideoEncoder, as_of,
};

pub struct Soft;

/// A demuxed video track: samples in decode order, their display times, and the codec config.
pub struct Track {
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

pub fn demux(path: &Path, want: &str) -> R<Track> {
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


impl Backend for Soft {
    fn name(&self) -> &'static str {
        "soft"
    }
    #[cfg(not(feature = "h264"))]
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>> {
        Err(format!("{}: this build's soft backend has no H.264 (the h264 feature)", path.display()))
    }
    /// through the source's optimized copies (raea_io_codec::mezz::open): all-intra H.264 that this decoder reads
    #[cfg(feature = "h264")]
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>> {
        let d = raea_io_codec::mezz::open(path, open_h264, || probe(path).ok().map(|p| p.frame_secs()))?;
        Ok(Box::new(d))
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
        h264_encoder(out, o, None)
    }
}

/// An H.264 MP4 (no B-frames) as a decoder.
#[cfg(feature = "h264")]
fn open_h264(path: &Path) -> R<Decoder> {
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
    Ok(Decoder {
        info,
        pts,
        params,
        dec: None,
        last: None,
        out: Vec::new(),
        nv12: false,
        track,
    })
}

/// The bitrate openh264 is given: the options', or with --quality (o.quality, 0..1) that bitrate scaled by it, 2x a
/// step of 0.1 from high (0.85, the default bitrate): master (0.95) twice it, draft (0.6) under a fifth. openh264 has
/// no constant-quality mode the crate reaches, and it ignored the quality, so draft and master wrote the same file (#28).
pub fn quality_kbps(o: &EncodeOpts) -> u32 {
    match o.quality {
        Some(q) => ((o.kbps as f64 * 2f64.powf((q.clamp(0.0, 1.0) as f64 - 0.85) * 10.0)).round() as u32).clamp(300, 240_000),
        None => o.kbps,
    }
}

/// openh264 at a fixed quantiser `qp`: its Quality mode with the QP range pinned (#28, tests/quality.rs). With rate control
/// off it encodes at its default QP 26 whatever the range says (the crate never sets the layer's iDLayerQp). With it
/// on, it refuses a bitrate of 0 and a pinned QP below 14 (Native:3; 13 passes all-intra but not with P-frames), and
/// the bitrate moves nothing once the QP is pinned. A frame too big for its buffer at that QP is Native:3 too.
#[cfg(feature = "h264")]
pub(crate) fn fixed_qp(config: EncoderConfig, qp: u8) -> EncoderConfig {
    let qp = qp.clamp(14, 51);
    config.rate_control_mode(RateControlMode::Quality).bitrate(BitRate::from_bps(50_000_000)).qp(openh264::encoder::QpRange::new(qp, qp))
}

/// openh264's encoder at the options' bitrate (quality_kbps), or all-intra at a fixed quantiser when `intra_qp` is set (every frame an IDR: an optimized copy, raea_io_media::mezz).
#[cfg(feature = "h264")]
pub fn h264_encoder(out: &Path, o: &EncodeOpts, intra_qp: Option<u8>) -> R<Box<dyn VideoEncoder>> {
    let config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(quality_kbps(o) * 1000))
        .max_frame_rate(FrameRate::from_hz(o.fps as f32))
        .rate_control_mode(RateControlMode::Bitrate)
        // every frame is a sample: rate control may not drop one (the edit has a frame for every tick)
        .skip_frames(false)
        // BT.709 limited range, as raea_io_codec::yuv converts the frames and as the Mac's encoder tags them (vt.rs):
        // untagged, ffmpeg and other players read it as BT.601 and shift reds and greens
        .vui(openh264::encoder::VuiConfig::bt709());
    let config = match intra_qp {
        Some(qp) => fixed_qp(config, qp).intra_frame_period(openh264::encoder::IntraFramePeriod::from_num_frames(1)),
        None => config,
    };
    let enc = H264Encoder::with_api_config(OpenH264API::from_source(), config).map_err(|e| format!("openh264: {e}"))?;
    let _ = std::io::stdout().flush();
    Ok(Box::new(Encoder { enc, opts: o.clone(), out: out.to_path_buf(), frames: Vec::new() }))
}
