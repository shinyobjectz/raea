//! raea's media plane: one seam (`Backend`) and several implementations, so the edit's clips, voice and music
//! never depend on one tool. A decoder answers "the frame showing at t" as an as-of lookup on presentation times
//! (the latest frame with pts <= t), so any order of requests gives the same frames. Every backend hands back
//! elementary streams; muxing is pure Rust (muxide) everywhere.
//!
//!   ffmpeg  the ffmpeg CLI (the baseline)
//!   soft    portable software: openh264 (H.264), re_mp4 (demux), muxide (mux), Symphonia (audio), Opus
//!   apple   platform hardware on Apple: AVFoundation / VideoToolbox (decode HEVC and H.264, encode H.264/HEVC + AAC)
//!   av1     royalty-free AV1: dav1d decode, rav1e encode
//!
//! Other platforms (Android MediaCodec, Linux VA-API / NVDEC) slot in behind the same trait.

#[cfg(all(feature = "apple", target_vendor = "apple"))]
pub mod apple;
/// packet-level VideoToolbox coding (one compressed packet at a time), for coders handed packets
#[cfg(all(feature = "apple", target_vendor = "apple"))]
pub mod packets;
/// seeking on VideoToolbox driven directly (the frame at a time, decoding only what it depends on)
#[cfg(all(feature = "apple", target_vendor = "apple"))]
pub mod vtseek;
/// packet-level AAC encoding (AudioToolbox), for coders handing PCM in and taking packets out
#[cfg(all(feature = "apple", target_vendor = "apple"))]
pub mod aenc;
pub mod audio;
/// packet-level audio decoding (Symphonia), for coders handed packets
#[cfg(feature = "soft")]
pub mod apackets;
#[cfg(feature = "av1")]
pub mod av1;
pub mod ffmpeg;
#[cfg(any(feature = "soft", feature = "apple"))]
pub mod remux;
#[cfg(feature = "soft")]
pub mod soft;
pub mod yuv;

use std::path::Path;

pub type R<T> = Result<T, String>;

/// Nanoseconds spent converting decoded pictures to RGBA (YUV or BGRA to RGBA, rotation), across all decoders: the
/// stage G8 moves to the GPU, counted apart from decoding itself.
pub static CONVERT_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Nanoseconds spent copying decoded NV12 planes out of the decoder's buffers (what an IOSurface zero-copy would save on
/// the CPU side of decoding).
pub static NV12_COPY_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Nanoseconds encoders spend converting RGBA to what they take (BGRA for VideoToolbox, I420 for openh264 and rav1e).
pub static ENCODE_CONVERT_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Time an encoder-side conversion into ENCODE_CONVERT_NS.
pub fn encode_convert_timed<T>(f: impl FnOnce() -> T) -> T {
    let t = std::time::Instant::now();
    let r = f();
    ENCODE_CONVERT_NS.fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    r
}

/// Time a conversion into CONVERT_NS.
pub fn convert_timed<T>(f: impl FnOnce() -> T) -> T {
    let t = std::time::Instant::now();
    let r = f();
    CONVERT_NS.fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    r
}

/// What a source is (facts the edit's media table stores).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Info {
    pub codec: String,
    /// display size (rotation applied)
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
    pub frames: usize,
}

/// One decoded picture: straight RGBA8, display size.
#[derive(Debug, Clone)]
pub struct Frame {
    pub pts: f64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Audio as columns: one f32 column per channel at `rate`.
#[derive(Debug, Clone, Default)]
pub struct Audio {
    pub rate: u32,
    pub channels: Vec<Vec<f32>>,
}

impl Audio {
    pub fn len(&self) -> usize {
        self.channels.first().map(Vec::len).unwrap_or(0)
    }
    pub fn duration(&self) -> f64 {
        self.len() as f64 / self.rate.max(1) as f64
    }
}

pub trait VideoDecoder {
    fn info(&self) -> &Info;
    /// The frame showing at `t` seconds: the latest frame with pts <= t (the first frame before it).
    fn frame_at(&mut self, t: f64) -> R<Frame>;
    /// Which frame (its index in presentation order) shows at `t`, without decoding: the as-of lookup over the
    /// presentation times. A cache keyed by it can skip decoding frames it already holds. None if not known.
    /// Apple: the frame showing at `t` still in VideoToolbox's buffer (NV12, IOSurface-backed, Metal-compatible),
    /// retained, nothing copied. None where the backend has no such buffer.
    #[cfg(all(feature = "apple", target_vendor = "apple"))]
    fn frame_surface_at(&mut self, _t: f64) -> R<Option<apple::SurfaceFrame>> {
        Ok(None)
    }
    /// The frame at `t` as VideoToolbox's BGRA in an IOSurface-backed buffer (its colour conversion, the pictures the
    /// RGBA path gives), left in its buffer for a GPU to wrap. None where the backend has no surfaces.
    #[cfg(all(feature = "apple", target_vendor = "apple"))]
    fn frame_surface_rgb_at(&mut self, _t: f64) -> R<Option<apple::SurfaceFrame>> {
        Ok(None)
    }
    fn index_at(&mut self, _t: f64) -> Option<usize> {
        None
    }
    /// The keyframes' indices (presentation order), where a decoder can start without anything before it: lets a
    /// render decode groups of pictures in parallel (rdb cpuview). None where the backend doesn't know them.
    fn keyframes(&self) -> Option<Vec<usize>> {
        None
    }
    /// The same frame as NV12 (luma, then interleaved Cb Cr; display size; BT.709 limited range) in `Frame.rgba`,
    /// for the GPU path: the platform's decoder hands its planes over, the portable ones interleave theirs. The
    /// default converts from RGBA on the CPU (the ffmpeg baseline).
    fn frame_nv12_at(&mut self, t: f64) -> R<Frame> {
        let f = self.frame_at(t)?;
        let rgba = yuv::rgba_to_nv12(&f.rgba, f.width as usize, f.height as usize);
        Ok(Frame { rgba, ..f })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EncodeOpts {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// target video bitrate in kbps
    pub kbps: u32,
    /// frames will arrive as NV12 (push_nv12): platform encoders set up for it, so nothing converts on the CPU
    pub nv12: bool,
    /// the content is 3D: the Apple encoder runs real-time there (its non-real-time look-ahead held hundreds of MB of
    /// work buffers on a 3D edit on the iPhone; on clips it's flat and real-time would cap it near 88 fps).
    /// RAEA_VT_REALTIME=0/1 overrides.
    pub realtime: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct EncodeStats {
    pub frames: usize,
    pub bytes: u64,
    pub video_codec: String,
    pub audio_codec: String,
}

pub trait VideoEncoder {
    /// One frame, straight RGBA8 at the encoder's size, in display order.
    fn push(&mut self, rgba: &[u8]) -> R<()>;
    /// One frame as NV12 (the luma plane, then interleaved Cb Cr at half size; BT.709 limited range), as the
    /// engine's GPU readback gives it. The default converts to RGBA and pushes that (only the ffmpeg baseline uses it).
    fn push_nv12(&mut self, nv12: &[u8], w: usize, h: usize) -> R<()> {
        let (y, u, v) = yuv::nv12_to_i420(nv12, w, h);
        let mut rgba = Vec::new();
        yuv::i420_to_rgba(&y, &u, &v, w, w / 2, w / 2, w, h, &mut rgba);
        self.push(&rgba)
    }
    /// Finish the stream, mux it with `audio` (if any) into the MP4.
    fn finish(self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats>;
}

pub trait Backend {
    fn name(&self) -> &'static str;
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>>;
    fn decode_audio(&self, path: &Path) -> R<Audio>;
    fn encoder(&self, out: &Path, opts: &EncodeOpts) -> R<Box<dyn VideoEncoder>>;
}

/// `secs` seconds of a file's sound from `start` (decode_audio's time): a seek and a short decode, for streaming a long
/// take as it plays.
#[cfg(feature = "soft")]
pub fn decode_audio_window(path: &Path, start: f64, secs: f64) -> R<Audio> {
    soft::decode_audio_window_symphonia(path, start, secs)
}

/// How long a file's sound is, from its container, without decoding it.
#[cfg(feature = "soft")]
pub fn probe_seconds(path: &Path) -> Option<f64> {
    soft::probe_seconds_symphonia(path)
}

/// The backends compiled into this build, by name.
pub fn backends() -> Vec<Box<dyn Backend>> {
    let mut v: Vec<Box<dyn Backend>> = vec![Box::new(ffmpeg::Ffmpeg)];
    #[cfg(feature = "soft")]
    v.push(Box::new(soft::Soft));
    #[cfg(all(feature = "apple", target_vendor = "apple"))]
    v.push(Box::new(apple::Apple));
    #[cfg(feature = "av1")]
    v.push(Box::new(av1::Av1));
    v
}

pub fn backend(name: &str) -> R<Box<dyn Backend>> {
    backends()
        .into_iter()
        .find(|b| b.name() == name)
        .ok_or(format!("no backend {name:?} in this build"))
}

/// Band-limited resampling: a Kaiser-windowed sinc (32 zero crossings each side, beta 9, cutoff just under the lower
/// Nyquist), the same family as ffmpeg's swresample, so mixes agree with ffmpeg's to well under -60 dB.
pub fn soft_resample(a: &Audio, to: u32) -> Audio {
    if a.rate == to || a.channels.is_empty() {
        return a.clone();
    }
    // a rate of 0 is no sound to speak of (and n would be infinite)
    if a.rate == 0 || to == 0 {
        return Audio { rate: to, channels: vec![Vec::new(); a.channels.len()] };
    }
    let (from, to_f) = (a.rate as f64, to as f64);
    let n = (a.len() as f64 * to_f / from).round() as usize;
    let cutoff = 0.97 * from.min(to_f) / from; // in cycles per input sample x2
    let zc = 32.0;
    let half = (zc / cutoff).ceil() as i64;
    let beta = 9.0f64;
    let i0 = |x: f64| {
        let (mut s, mut t, mut k) = (1.0, 1.0, 1.0);
        while t > 1e-12 * s {
            t *= (x / (2.0 * k)).powi(2);
            s += t;
            k += 1.0;
        }
        s
    };
    let i0b = i0(beta);
    // the kernel at 1024 phases between input samples
    const PH: usize = 1024;
    let taps = (2 * half + 1) as usize;
    let mut table = vec![0f32; (PH + 1) * taps];
    for p in 0..=PH {
        let frac = p as f64 / PH as f64;
        for k in 0..taps {
            let x = (k as i64 - half) as f64 - frac;
            let u = x * cutoff;
            let sinc = if u.abs() < 1e-12 {
                1.0
            } else {
                (std::f64::consts::PI * u).sin() / (std::f64::consts::PI * u)
            };
            let r = x / (half as f64 + 1.0);
            let w = if r.abs() >= 1.0 {
                0.0
            } else {
                i0(beta * (1.0 - r * r).sqrt()) / i0b
            };
            table[p * taps + k] = (cutoff * sinc * w) as f32;
        }
    }
    let step = from / to_f;
    let channels = a
        .channels
        .iter()
        .map(|c| {
            (0..n)
                .map(|i| {
                    let x = i as f64 * step;
                    let j = x.floor() as i64;
                    let p = (((x - j as f64) * PH as f64).round() as usize).min(PH);
                    let row = &table[p * taps..(p + 1) * taps];
                    let mut acc = 0f32;
                    for (k, w) in row.iter().enumerate() {
                        let idx = j + k as i64 - half;
                        if idx >= 0 && (idx as usize) < c.len() {
                            acc += c[idx as usize] * w;
                        }
                    }
                    acc
                })
                .collect()
        })
        .collect();
    Audio { rate: to, channels }
}

/// The as-of index: the latest pts <= t (or the first frame for t before it). `pts` sorted ascending.
pub fn as_of(pts: &[f64], t: f64) -> usize {
    let i = pts.partition_point(|p| *p <= t + 1e-6);
    i.saturating_sub(1)
}

/// Mean SSIM between two RGBA8 frames of the same size (luma, 8x8 windows): the parity measure.
pub fn ssim(a: &[u8], b: &[u8], w: usize, h: usize) -> f64 {
    let luma = |p: &[u8], i: usize| {
        0.299 * p[i * 4] as f64 + 0.587 * p[i * 4 + 1] as f64 + 0.114 * p[i * 4 + 2] as f64
    };
    let (c1, c2) = ((0.01 * 255.0f64).powi(2), (0.03 * 255.0f64).powi(2));
    let (mut total, mut n) = (0.0, 0);
    for by in (0..h.saturating_sub(7)).step_by(8) {
        for bx in (0..w.saturating_sub(7)).step_by(8) {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for y in by..by + 8 {
                for x in bx..bx + 8 {
                    let i = y * w + x;
                    let (va, vb) = (luma(a, i), luma(b, i));
                    sa += va;
                    sb += vb;
                    saa += va * va;
                    sbb += vb * vb;
                    sab += va * vb;
                }
            }
            let m = 64.0;
            let (ma, mb) = (sa / m, sb / m);
            let (va, vb, cov) = (saa / m - ma * ma, sbb / m - mb * mb, sab / m - ma * mb);
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            n += 1;
        }
    }
    total / n.max(1) as f64
}
