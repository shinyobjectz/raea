//! raea's media seam: what a decoder and an encoder are (`VideoDecoder`, `VideoEncoder`, `Backend`), frames, audio as
//! columns, and what every backend shares: YUV conversion, resampling, loudness, and muxing elementary streams in pure
//! Rust (muxide). The backends are their own crates (raea-io-soft, raea-io-apple, raea-io-av1, raea-io-ffmpeg);
//! raea-io-media puts them behind one name.

pub mod audio;
pub mod mezz;
#[cfg(feature = "mux")]
pub mod remux;
pub mod samples;
mod resample;
pub mod yuv;

pub use resample::{soft_resample, soft_resample_blocks};

/// A decoded frame still in VideoToolbox's buffer: NV12 (video range), IOSurface-backed and Metal-compatible, retained
/// (frame_surface_at). Nothing was copied; a GPU can sample its two planes as textures.
#[cfg(target_vendor = "apple")]
pub struct SurfaceFrame {
    pub pts: f64,
    pub width: u32,
    pub height: u32,
    pub buffer: objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>,
}

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
    #[cfg(target_vendor = "apple")]
    fn frame_surface_at(&mut self, _t: f64) -> R<Option<SurfaceFrame>> {
        Ok(None)
    }
    /// The frame at `t` as VideoToolbox's BGRA in an IOSurface-backed buffer (its colour conversion, the pictures the
    /// RGBA path gives), left in its buffer for a GPU to wrap. None where the backend has no surfaces.
    #[cfg(target_vendor = "apple")]
    fn frame_surface_rgb_at(&mut self, _t: f64) -> R<Option<SurfaceFrame>> {
        Ok(None)
    }
    fn index_at(&mut self, _t: f64) -> Option<usize> {
        None
    }
    /// The keyframes' indices (presentation order), where a decoder can start without anything before it: lets a
    /// render decode groups of pictures in parallel (raea-render cpuview). None where the backend doesn't know them.
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

/// The average bitrate (kbps) a render takes when it isn't given one: 0.32 bits a pixel a frame for H.264 (1080p30 about
/// 20 Mbit/s, 4K30 about 80), 0.6 of that for HEVC, never under the old flat 10 Mbit/s and never past 240 Mbit/s (H.264
/// High level 5.2). A flat 10 Mbit/s left 4K text and vector edges visibly rough; VideoToolbox spends well under the
/// target on flat graphics anyway (80 Mbit/s asked came out 13 on sdk/raea/bench/encode/flat4k.tsx), so easy
/// pictures stay small.
pub fn default_kbps(width: u32, height: u32, fps: f64, hevc: bool) -> u32 {
    let bpp = if hevc { 0.32 * 0.6 } else { 0.32 };
    let kbps = width as f64 * height as f64 * fps.max(1.0) * bpp / 1000.0;
    (kbps.round() as u32).clamp(10_000, 240_000)
}

/// `--quality` as given: a number 0..1, or a preset (draft 0.6, high 0.85, master 0.95).
pub fn parse_quality(v: &str) -> R<f32> {
    match v {
        "draft" => Ok(0.6),
        "high" => Ok(0.85),
        "master" => Ok(0.95),
        _ => v.parse::<f32>().ok().filter(|q| (0.0..=1.0).contains(q)).ok_or_else(|| format!("--quality takes 0..1 or draft, high, master (not {v})")),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EncodeOpts {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// target video bitrate in kbps (an average; VideoToolbox spends less on easy pictures). Ignored where `quality` is
    /// set and the backend has a constant quality; openh264 scales it by the quality.
    pub kbps: u32,
    /// constant quality, 0..1 (VideoToolbox's kVTCompressionPropertyKey_Quality; libx264's CRF through the ffmpeg
    /// backend), in place of `kbps`: each frame gets the bits its picture needs. None: the average bitrate.
    /// openh264 (Windows, Linux) has no such mode: it scales `kbps` by the quality instead (raea_io_soft::quality_kbps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<f32>,
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
    /// One frame as P010 (the luma plane, then interleaved Cb Cr at half size; each sample a little-endian u16 with
    /// its 10-bit BT.709 limited-range value in the top 10 bits), for an encoder made by `Backend::encoder_10bit`.
    fn push_p010(&mut self, _p010: &[u8], _w: usize, _h: usize) -> R<()> {
        Err("this encoder takes no 10-bit frames".into())
    }
    /// Finish the stream, mux it with `audio` (if any) into the MP4.
    fn finish(self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats>;
}

pub trait Backend {
    fn name(&self) -> &'static str;
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>>;
    fn decode_audio(&self, path: &Path) -> R<Audio>;
    fn encoder(&self, out: &Path, opts: &EncodeOpts) -> R<Box<dyn VideoEncoder>>;
    /// A 10-bit encoder (frames by push_p010; HEVC Main10 on Apple), whose decoded colours can match 8-bit RGB
    /// exactly: 8-bit limited-range BT.709's own round trip errs up to 2/255. `opts.nv12` is ignored.
    fn encoder_10bit(&self, _out: &Path, _opts: &EncodeOpts) -> R<Box<dyn VideoEncoder>> {
        Err(format!("the {} backend has no 10-bit encoder", self.name()))
    }
    /// `a` encoded as this backend's encoders lay sound under their video (VideoEncoder::finish), for a video put
    /// together from parts encoded apart (raea-runtime-segments' join).
    fn sound(&self, _a: &Audio) -> R<SoundTrack> {
        Err(format!("the {} backend lays no sound under a joined video", self.name()))
    }
}

/// A sound encoded for a muxer, as a backend lays it under its video: AAC (each packet an ADTS frame, after `priming`
/// samples of the encoder's own silence) or Opus, each packet with its time in seconds.
#[derive(Debug, Clone, Default)]
pub struct SoundTrack {
    pub aac: bool,
    pub rate: u32,
    pub channels: u16,
    pub priming: u32,
    pub packets: Vec<(f64, Vec<u8>)>,
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
