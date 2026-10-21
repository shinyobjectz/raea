//! (c) Platform hardware on Apple: AVFoundation, with VideoToolbox underneath, decodes HEVC and H.264 straight from
//! phone files (rotation applied by the asset's video composition) and encodes H.264/HEVC + AAC. Android (MediaCodec)
//! and Linux GPU servers (VA-API, NVDEC) implement the same trait in their own modules.

use std::path::Path;

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::*;
use objc2_core_foundation::CFString;
use objc2_core_media::{CMTime, CMTimeRange, kCMTimePositiveInfinity};
use objc2_core_video::*;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::{Audio, Backend, EncodeOpts, EncodeStats, Frame, Info, R, VideoDecoder, VideoEncoder};

pub struct Apple;

fn cf_key(k: &CFString) -> &NSString {
    // CFString and NSString are toll-free bridged
    unsafe { &*(k as *const CFString as *const NSString) }
}

fn asset(path: &Path) -> Retained<AVURLAsset> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.display().to_string()));
    unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) }
}

struct Decoder {
    asset: Retained<AVURLAsset>,
    info: Info,
    reader: Option<(
        Retained<AVAssetReader>,
        Retained<AVAssetReaderOutput>,
    )>,
    cur: Option<Slot>,
    next: Option<Slot>,
    since_restart: usize,
    /// VideoToolbox hands over NV12 planes (no conversion) instead of BGRA
    nv12: bool,
    /// surface mode: NV12 left in VideoToolbox's IOSurface-backed, Metal-compatible buffers (no copy)
    surface: bool,
    /// the presentation times, read once from the compressed samples (for index_at)
    sample_pts: Option<Vec<f64>>,
    watch: Watch,
    /// the file, for a seek on VideoToolbox directly
    path: std::path::PathBuf,
    /// that seeker, made at the first seek (None inside: this track can't seek directly), and whether it gives BGRA
    seeker: Option<(Option<crate::vtseek::Seeker>, bool)>,
    /// the time asked for last: a request just after it is play going on, anything else a seek
    last_t: Option<f64>,
}

/// How long one read may block before the watch cancels it.
const STALL_MS: u64 = 15_000;
const STALLED: &str = "the video decoder stalled (AVAssetReader stopped answering)";

/// A stalled read's way out: copyNextSampleBuffer on a composition output sometimes blocks forever on long iPhone takes
/// (2026-10-17: a render 2 h, scene detection 37 min, both at 0% CPU in FigSemaphoreWaitRelative). Nothing on the
/// reading thread can end it, but cancelReading from another thread does (Apple: callable from any thread), and the
/// read returns nil. One thread per decoder watches a deadline the read arms; the reader is a raw pointer read and
/// cancelled only under the lock, and the decoder clears it under the same lock before it lets the reader go.
struct Watch {
    deadline: std::sync::Arc<std::sync::atomic::AtomicU64>,
    reader: std::sync::Arc<std::sync::Mutex<usize>>,
    fired: std::sync::Arc<std::sync::atomic::AtomicBool>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Watch {
    fn new() -> Watch {
        use std::sync::atomic::Ordering::SeqCst;
        let w = Watch { deadline: Default::default(), reader: Default::default(), fired: Default::default(), stop: Default::default() };
        let (deadline, reader, fired, stop) = (w.deadline.clone(), w.reader.clone(), w.fired.clone(), w.stop.clone());
        std::thread::spawn(move || {
            while !stop.load(SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let d = deadline.load(SeqCst);
                if d != 0 && now_ms() > d {
                    let ptr = reader.lock().unwrap();
                    if *ptr != 0 && deadline.load(SeqCst) == d {
                        objc2::rc::autoreleasepool(|_| unsafe { (*(*ptr as *const AVAssetReader)).cancelReading() });
                        fired.store(true, SeqCst);
                        deadline.store(0, SeqCst);
                    }
                }
            }
        });
        w
    }
    fn set(&self, r: Option<&AVAssetReader>) {
        *self.reader.lock().unwrap() = r.map(|r| r as *const AVAssetReader as usize).unwrap_or(0);
    }
}

/// A decoded frame, and in surface mode the buffer it still lives in.
type Slot = (Frame, Option<objc2_core_foundation::CFRetained<CVPixelBuffer>>);

/// A decoded frame still in VideoToolbox's buffer: NV12 (video range), IOSurface-backed and Metal-compatible, retained
/// (frame_surface_at). Nothing was copied; a GPU can sample its two planes as textures.
pub struct SurfaceFrame {
    pub pts: f64,
    pub width: u32,
    pub height: u32,
    pub buffer: objc2_core_foundation::CFRetained<CVPixelBuffer>,
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.watch.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.watch.set(None);
        if let Some((r, _)) = self.reader.take() {
            unsafe { r.cancelReading() };
        }
    }
}

impl Decoder {
    fn restart(&mut self, t: f64) -> R<()> {
        // the old reader's VideoToolbox session goes now, not whenever its last reference does (and the watch lets go
        // of it first)
        self.watch.set(None);
        if let Some((old, _)) = self.reader.take() {
            unsafe { old.cancelReading() };
        }
        self.since_restart = 0;
        unsafe {
            let media = AVMediaTypeVideo.ok_or("AVMediaTypeVideo")?;
            #[allow(deprecated)]
            let tracks = self.asset.tracksWithMediaType(media);
            #[allow(deprecated)]
            let comp = AVVideoComposition::videoCompositionWithPropertiesOfAsset(&self.asset);
            let fmt = NSNumber::new_u32(if self.nv12 { kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange } else { kCVPixelFormatType_32BGRA });
            let key = cf_key(kCVPixelBufferPixelFormatTypeKey);
            let settings: Retained<NSDictionary<NSString, AnyObject>> = if self.surface {
                // buffers a Metal texture can wrap: IOSurface-backed and Metal-compatible
                let io: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::new();
                NSDictionary::from_slices(
                    &[key, cf_key(kCVPixelBufferMetalCompatibilityKey), cf_key(kCVPixelBufferIOSurfacePropertiesKey)],
                    &[&*fmt as &AnyObject, &*NSNumber::new_bool(true) as &AnyObject, &*io as &AnyObject],
                )
            } else {
                NSDictionary::from_slices(&[key], &[&*fmt as &AnyObject])
            };
            // a track whose frames are shown as decoded (no rotation, no edit list composition: what vtseek also
            // requires) is read straight from the decoder; a video composition output keeps its own pool of
            // composited frames as well, which for 4K ProRes was ~500 MB of footprint (lightspeed R3)
            let identity = tracks.firstObject().is_some_and(|tr| {
                let x = tr.preferredTransform();
                (x.a - 1.0).abs() < 1e-6 && x.b.abs() < 1e-6 && x.c.abs() < 1e-6 && (x.d - 1.0).abs() < 1e-6
            });
            let direct = identity && std::env::var("RAEA_READER_TRACK").as_deref() != Ok("0");
            let out: Retained<AVAssetReaderOutput> = if direct {
                let track = tracks.firstObject().unwrap();
                let o = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, Some(&settings));
                o.setAlwaysCopiesSampleData(false);
                Retained::into_super(o)
            } else {
                let o = AVAssetReaderVideoCompositionOutput::assetReaderVideoCompositionOutputWithVideoTracks_videoSettings(&tracks, Some(&settings));
                o.setVideoComposition(Some(&comp));
                o.setAlwaysCopiesSampleData(false);
                Retained::into_super(o)
            };
            let reader = AVAssetReader::assetReaderWithAsset_error(&self.asset)
                .map_err(|e| format!("AVAssetReader: {}", e.localizedDescription()))?;
            let start = (t - 0.5 / self.info.fps).max(0.0);
            reader.setTimeRange(CMTimeRange {
                start: CMTime::with_seconds(start, 600),
                duration: kCMTimePositiveInfinity,
            });
            reader.addOutput(&out);
            if !reader.startReading() {
                return Err("AVAssetReader didn't start".into());
            }
            self.watch.set(Some(&reader));
            self.reader = Some((reader, out));
        }
        self.cur = None;
        self.next = None;
        Ok(())
    }

    fn read(&mut self) -> R<Option<Slot>> {
        let Some((_, out)) = &self.reader else {
            return Ok(None);
        };
        unsafe {
            use std::sync::atomic::Ordering::SeqCst;
            self.watch.deadline.store(now_ms() + STALL_MS, SeqCst);
            let sb = out.copyNextSampleBuffer();
            self.watch.deadline.store(0, SeqCst);
            let Some(sb) = sb else {
                if self.watch.fired.swap(false, SeqCst) {
                    return Err(STALLED.into());
                }
                return Ok(None);
            };
            self.since_restart += 1;
            let pts = sb.presentation_time_stamp().seconds();
            let ib = sb.image_buffer().ok_or("no image buffer")?;
            if self.surface {
                // kept as it is: no lock, no copy
                let (w, h) = (CVPixelBufferGetWidth(&ib), CVPixelBufferGetHeight(&ib));
                return Ok(Some((Frame { pts, width: w as u32, height: h as u32, rgba: Vec::new() }, Some(ib))));
            }
            let pb: &CVPixelBuffer = &ib;
            if CVPixelBufferLockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly) != 0 {
                return Err("lock failed".into());
            }
            let (w, h, bpr) = (
                CVPixelBufferGetWidth(pb),
                CVPixelBufferGetHeight(pb),
                CVPixelBufferGetBytesPerRow(pb),
            );
            if self.nv12 {
                // the planes as they are: two row-by-row copies (rows may be padded), no conversion
                let t_copy = std::time::Instant::now();
                let mut out = vec![0u8; w * h * 3 / 2];
                for (plane, rows, off) in [(0usize, h, 0usize), (1, h / 2, w * h)] {
                    let base = CVPixelBufferGetBaseAddressOfPlane(pb, plane) as *const u8;
                    let pbpr = CVPixelBufferGetBytesPerRowOfPlane(pb, plane);
                    for r in 0..rows {
                        std::ptr::copy_nonoverlapping(base.add(r * pbpr), out.as_mut_ptr().add(off + r * w), w);
                    }
                }
                crate::NV12_COPY_NS.fetch_add(t_copy.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
                CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
                return Ok(Some((Frame { pts, width: w as u32, height: h as u32, rgba: out }, None)));
            }
            let base = CVPixelBufferGetBaseAddress(pb) as *const u8;
            let rgba = crate::convert_timed(|| {
                let mut rgba = vec![0u8; w * h * 4];
                for y in 0..h {
                    let row = std::slice::from_raw_parts(base.add(y * bpr), w * 4);
                    let dst = &mut rgba[y * w * 4..(y + 1) * w * 4];
                    for x in 0..w {
                        dst[x * 4] = row[x * 4 + 2];
                        dst[x * 4 + 1] = row[x * 4 + 1];
                        dst[x * 4 + 2] = row[x * 4];
                        dst[x * 4 + 3] = 255;
                    }
                }
                rgba
            });
            CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
            Ok(Some((
                Frame {
                    pts,
                    width: w as u32,
                    height: h as u32,
                    rgba,
                },
                None,
            )))
        }
    }
}

impl VideoDecoder for Decoder {
    fn info(&self) -> &Info {
        &self.info
    }
    fn frame_at(&mut self, t: f64) -> R<Frame> {
        // every Objective-C object made while decoding is released here (this thread has no run loop to drain a pool)
        objc2::rc::autoreleasepool(|_| {
            self.mode(false, false);
            self.frame_at_pooled(t).map(|s| s.0)
        })
    }
    fn frame_nv12_at(&mut self, t: f64) -> R<Frame> {
        objc2::rc::autoreleasepool(|_| {
            self.mode(true, false);
            self.frame_at_pooled(t).map(|s| s.0)
        })
    }
    fn frame_surface_at(&mut self, t: f64) -> R<Option<SurfaceFrame>> {
        objc2::rc::autoreleasepool(|_| {
            self.mode(true, true);
            // a seek (not the next frames of play, not what the reader holds) on VideoToolbox directly: only the frames
            // the target depends on decoded, only it output. Play resuming after it restarts the reader once.
            if let Some(f) = self.seek_direct(t)? {
                return Ok(Some(f));
            }
            let (f, pb) = self.frame_at_pooled(t)?;
            Ok(pb.map(|buffer| SurfaceFrame { pts: f.pts, width: f.width, height: f.height, buffer }))
        })
    }

    fn frame_surface_rgb_at(&mut self, t: f64) -> R<Option<SurfaceFrame>> {
        objc2::rc::autoreleasepool(|_| {
            self.mode(false, true);
            if let Some(f) = self.seek_direct(t)? {
                return Ok(Some(f));
            }
            let (f, pb) = self.frame_at_pooled(t)?;
            Ok(pb.map(|buffer| SurfaceFrame { pts: f.pts, width: f.width, height: f.height, buffer }))
        })
    }
    fn index_at(&mut self, t: f64) -> Option<usize> {
        if self.sample_pts.is_none() {
            self.sample_pts = objc2::rc::autoreleasepool(|_| self.read_pts()).ok();
        }
        // the same rule as frame_at: the latest frame with pts <= t + 1e-4 (as_of adds its own 1e-6)
        self.sample_pts.as_ref().filter(|p| !p.is_empty()).map(|p| crate::as_of(p, t + 1e-4 - 1e-6))
    }
}

/// Frames read before the reader (and its VideoToolbox session) is replaced: bounds the decoder service's memory.
const RECYCLE_FRAMES: usize = 300;

impl Decoder {
    /// The frame at `t` from the direct seeker, when `t` is a seek the reader can't serve from what it holds.
    fn seek_direct(&mut self, t: f64) -> R<Option<SurfaceFrame>> {
        let eps = 1e-4;
        let fps = self.info.fps.max(1.0);
        let following = self.last_t.is_some_and(|l| t >= l - eps && t - l < 3.0 / fps);
        self.last_t = Some(t);
        let held = matches!((&self.cur, &self.reader), (Some(c), Some(_)) if c.0.pts <= t + eps && t < c.0.pts + 2.0);
        if following || held || std::env::var("RAEA_VT_SEEK").as_deref() == Ok("0") {
            return Ok(None);
        }
        let bgra = !self.nv12;
        let trace = std::env::var_os("RAEA_VT_TRACE").is_some();
        let t0 = std::time::Instant::now();
        if self.seeker.as_ref().is_none_or(|s| s.1 != bgra) {
            let made = unsafe {
                let media = AVMediaTypeVideo.ok_or("AVMediaTypeVideo")?;
                #[allow(deprecated)]
                let tracks = self.asset.tracksWithMediaType(media);
                match tracks.firstObject() {
                    Some(track) => crate::vtseek::Seeker::new(&self.asset, &track, &self.path, bgra).unwrap_or(None),
                    None => None,
                }
            };
            self.seeker = Some((made, bgra));
            if trace {
                eprintln!("vtseek: made for {} in {:.1} ms", self.path.display(), t0.elapsed().as_secs_f64() * 1e3);
            }
        }
        let Some((Some(s), _)) = self.seeker.as_mut() else { return Ok(None) };
        let t1 = std::time::Instant::now();
        let (buffer, pts) = match s.frame_at(t) {
            Ok(f) => f,
            Err(e) => {
                raea_native::trace::event("video_error", serde_json::json!({"source": "video", "file": self.path, "at_s": t, "error": e}));
                return Err(e);
            }
        };
        let ms = t1.elapsed().as_secs_f64() * 1e3;
        if trace {
            eprintln!("vtseek: {t:.3} decoded {} in {ms:.1} ms", s.decoded);
        }
        // a seek over the scrub budget, in the run's trace with what it decoded
        if ms > 50.0 {
            raea_native::trace::event("video_slow", serde_json::json!({"source": "video", "file": self.path, "at_s": t, "ms": (ms * 10.0).round() / 10.0, "decoded": s.decoded}));
        }
        let (width, height) = (s.width, s.height);
        // the reader's frames are behind now: play going on from here restarts it
        self.cur = None;
        self.next = None;
        Ok(Some(SurfaceFrame { pts, width, height, buffer }))
    }

    /// Every sample's presentation time, sorted, from a pass over the compressed samples (no decoding).
    fn read_pts(&self) -> R<Vec<f64>> {
        unsafe {
            let media = AVMediaTypeVideo.ok_or("AVMediaTypeVideo")?;
            #[allow(deprecated)]
            let tracks = self.asset.tracksWithMediaType(media);
            let track = tracks.firstObject().ok_or("no video track")?;
            let reader = AVAssetReader::assetReaderWithAsset_error(&self.asset).map_err(|e| format!("AVAssetReader: {}", e.localizedDescription()))?;
            let out = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, None);
            out.setAlwaysCopiesSampleData(false);
            reader.addOutput(&out);
            if !reader.startReading() {
                return Err("AVAssetReader didn't start".into());
            }
            // samples carry media-timeline times; the first non-empty edit maps them to presentation (with B-frames
            // the media timeline starts late by the reorder delay, which the edit list takes out)
            let mut shift = 0.0;
            for seg in track.segments().iter() {
                if !seg.isEmpty() {
                    let m = seg.timeMapping();
                    shift = m.target.start.seconds() - m.source.start.seconds();
                    break;
                }
            }
            let mut pts = Vec::new();
            while let Some(sb) = objc2::rc::autoreleasepool(|_| out.copyNextSampleBuffer()) {
                if sb.num_samples() > 0 {
                    pts.push(sb.presentation_time_stamp().seconds() + shift);
                }
            }
            pts.sort_by(f64::total_cmp);
            Ok(pts)
        }
    }

    /// Switch between BGRA (converted to RGBA), NV12 output and NV12 left in its buffers (surface): the reader starts
    /// again in the new format.
    fn mode(&mut self, nv12: bool, surface: bool) {
        if self.nv12 != nv12 || self.surface != surface {
            self.nv12 = nv12;
            self.surface = surface;
            if let Some((r, _)) = self.reader.take() {
                unsafe { r.cancelReading() };
            }
            self.cur = None;
            self.next = None;
        }
    }

    /// The frame at `t`; a stalled reader is cancelled by the watch, and the frame is tried once more on a fresh one.
    fn frame_at_pooled(&mut self, t: f64) -> R<Slot> {
        match self.frame_at_once(t) {
            Err(e) if e == STALLED => {
                self.watch.set(None);
                self.reader = None;
                self.cur = None;
                self.next = None;
                self.frame_at_once(t).map_err(|e| if e == STALLED { format!("{STALLED} twice at {t:.3} s") } else { e })
            }
            r => r,
        }
    }

    fn frame_at_once(&mut self, t: f64) -> R<Slot> {
        let eps = 1e-4;
        if self.since_restart >= RECYCLE_FRAMES {
            self.restart(t)?;
            self.cur = self.read()?;
        }
        let usable = match (&self.cur, &self.reader) {
            (Some(c), Some(_)) => c.0.pts <= t + eps && t < c.0.pts + 2.0,
            _ => false,
        };
        if !usable {
            self.restart(t)?;
            self.cur = self.read()?;
        }
        loop {
            if self.next.is_none() {
                self.next = self.read()?;
            }
            match &self.next {
                Some(n) if n.0.pts <= t + eps => {
                    self.cur = self.next.take();
                }
                _ => break,
            }
        }
        self.cur.clone().ok_or(format!("no frame at {t}"))
    }
}

impl Backend for Apple {
    fn name(&self) -> &'static str {
        "apple"
    }
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>> {
        objc2::rc::autoreleasepool(|_| open_video_pooled(path))
    }
    fn decode_audio(&self, path: &Path) -> R<Audio> {
        objc2::rc::autoreleasepool(|_| decode_audio_pooled(path))
    }
    fn encoder(&self, out: &Path, o: &EncodeOpts) -> R<Box<dyn VideoEncoder>> {
        // VideoToolbox driven directly (bounded frames in flight), unless RAEA_APPLE_ENC=writer asks for AVAssetWriter
        if std::env::var("RAEA_APPLE_ENC").map(|v| v == "writer").unwrap_or(false) {
            return objc2::rc::autoreleasepool(|_| Encoder::new(out, o).map(|e| Box::new(e) as Box<dyn VideoEncoder>));
        }
        objc2::rc::autoreleasepool(|_| VtEncoder::new(out, o, false).map(|e| Box::new(e) as Box<dyn VideoEncoder>))
    }
    fn encoder_10bit(&self, out: &Path, o: &EncodeOpts) -> R<Box<dyn VideoEncoder>> {
        objc2::rc::autoreleasepool(|_| VtEncoder::new(out, o, true).map(|e| Box::new(e) as Box<dyn VideoEncoder>))
    }
}

// A decoder moves to another thread whole and is used by one thread at a time (open_video_send): AVURLAsset and
// VTDecompressionSession are thread-safe, and the reader, the slots and the seeker are only ever touched by whoever holds
// the decoder.
unsafe impl Send for Decoder {}

/// A decoder that can be opened (and seeked) on a worker and handed over: a clip about to show, made ready ahead of its
/// cut so the frame that cuts to it doesn't pay the open and the seek (react's VideoFrames::prefetch).
pub fn open_video_send(path: &Path) -> R<Box<dyn VideoDecoder + Send>> {
    objc2::rc::autoreleasepool(|_| open_any(path))
}

fn open_video_pooled(path: &Path) -> R<Box<dyn VideoDecoder>> {
    open_any(path).map(|d| d as Box<dyn VideoDecoder>)
}

/// The decoder for `path` (mezz, lightspeed F4): its complete intra-only copy when there is one (every frame a
/// keyframe, so a seek decodes one frame); else, when copies of ranges of it exist, the source with those ranges read
/// from their copies (`Ranged`); else the source. Every decoder opened on a source decides the same way, so a render
/// and a play opened at the same moment read the same pixels.
fn open_any(path: &Path) -> R<Box<dyn VideoDecoder + Send>> {
    let whole = crate::mezz::resolve(path);
    if whole.as_path() != path {
        return open_decoder(&whole).map(|d| Box::new(d) as Box<dyn VideoDecoder + Send>);
    }
    let ranges = crate::mezz::ranges(path);
    let src = open_decoder(path)?;
    if ranges.is_empty() {
        return Ok(Box::new(src));
    }
    Ok(Box::new(Ranged { src, ranges: ranges.into_iter().map(|r| (r, None)).collect() }))
}

/// A source read through copies of the ranges an edit uses (mezz::make_range): a time inside a range is the copy's
/// frame at the time less the range's start (the copy's time t is the source's start + t), anything else the
/// source's own. Times, presentation times and frame indices stay the source's.
struct Ranged {
    src: Decoder,
    ranges: Vec<(crate::mezz::RangeCopy, Option<Decoder>)>,
}

unsafe impl Send for Ranged {}

impl Ranged {
    /// The range copy covering `t`, opened if it wasn't: its start, first frame and decoder.
    fn copy_at(&mut self, t: f64) -> R<Option<(f64, usize, &mut Decoder)>> {
        let eps = 1e-6;
        let Some(i) = self.ranges.iter().position(|(r, _)| t >= r.start - eps && t < r.end - eps) else { return Ok(None) };
        let (r, dec) = &mut self.ranges[i];
        if dec.is_none() {
            *dec = Some(objc2::rc::autoreleasepool(|_| open_decoder(&r.path))?);
        }
        Ok(Some((r.start, r.first, dec.as_mut().unwrap())))
    }
}

impl VideoDecoder for Ranged {
    fn info(&self) -> &Info {
        self.src.info()
    }
    fn frame_at(&mut self, t: f64) -> R<Frame> {
        match self.copy_at(t)? {
            Some((s, _, d)) => d.frame_at(t - s).map(|f| Frame { pts: f.pts + s, ..f }),
            None => self.src.frame_at(t),
        }
    }
    fn frame_nv12_at(&mut self, t: f64) -> R<Frame> {
        match self.copy_at(t)? {
            Some((s, _, d)) => d.frame_nv12_at(t - s).map(|f| Frame { pts: f.pts + s, ..f }),
            None => self.src.frame_nv12_at(t),
        }
    }
    fn frame_surface_at(&mut self, t: f64) -> R<Option<SurfaceFrame>> {
        match self.copy_at(t)? {
            Some((s, _, d)) => d.frame_surface_at(t - s).map(|f| f.map(|f| SurfaceFrame { pts: f.pts + s, ..f })),
            None => self.src.frame_surface_at(t),
        }
    }
    fn frame_surface_rgb_at(&mut self, t: f64) -> R<Option<SurfaceFrame>> {
        match self.copy_at(t)? {
            Some((s, _, d)) => d.frame_surface_rgb_at(t - s).map(|f| f.map(|f| SurfaceFrame { pts: f.pts + s, ..f })),
            None => self.src.frame_surface_rgb_at(t),
        }
    }
    // the source's own frame index (its presentation times, nothing decoded): a range's first and last count the
    // track's minimum frame duration, which a variable-rate phone clip runs finer than its frames (1/92 s at 30 fps)
    fn index_at(&mut self, t: f64) -> Option<usize> {
        self.src.index_at(t)
    }
}

fn open_decoder(path: &Path) -> R<Decoder> {
    // the process's first VideoToolbox session costs ~50 ms (later ones ~1): paid now, off this thread, so no seek
    // pays it
    static WARM: std::sync::Once = std::sync::Once::new();
    WARM.call_once(|| {
        let p = path.to_path_buf();
        let _ = std::thread::Builder::new().name("raea vt warm".into()).spawn(move || {
            objc2::rc::autoreleasepool(|_| unsafe {
                let a = asset(&p);
                let Some(media) = AVMediaTypeVideo else { return };
                #[allow(deprecated)]
                let tracks = a.tracksWithMediaType(media);
                if let Some(track) = tracks.firstObject() {
                    let _ = crate::vtseek::Seeker::new(&a, &track, &p, false);
                }
            })
        });
    });
    let a = asset(path);
    let info = unsafe {
        let media = AVMediaTypeVideo.ok_or("AVMediaTypeVideo")?;
        #[allow(deprecated)]
        let tracks = a.tracksWithMediaType(media);
        let track = tracks
            .firstObject()
            .ok_or(format!("{}: no video track", path.display()))?;
        #[allow(deprecated)]
        let comp = AVVideoComposition::videoCompositionWithPropertiesOfAsset(&a);
        let size = comp.renderSize();
        let fps = track.nominalFrameRate() as f64;
        #[allow(deprecated)]
        let duration = a.duration().seconds();
        // the codec, from the track's format description (a CMFormatDescription in the array)
        let fourcc = {
            let descs = track.formatDescriptions();
            match descs.firstObject() {
                Some(d) => {
                    let fd = &*(objc2::rc::Retained::as_ptr(&d)
                        as *const objc2_core_media::CMFormatDescription);
                    let st = fd.media_sub_type().to_be_bytes();
                    match &st {
                        b"avc1" | b"avc3" => "h264".to_string(),
                        b"hvc1" | b"hev1" => "hevc".to_string(),
                        b"av01" => "av1".to_string(),
                        other => String::from_utf8_lossy(other).into_owned(),
                    }
                }
                None => "unknown".to_string(),
            }
        };
        Info {
            codec: fourcc,
            width: size.width as u32,
            height: size.height as u32,
            fps,
            duration,
            frames: (duration * fps).round() as usize,
        }
    };
    Ok((Decoder {
        asset: a,
        info,
        reader: None,
        cur: None,
        next: None,
        since_restart: 0,
        nv12: false,
        surface: false,
        sample_pts: None,
        watch: Watch::new(),
        path: path.to_path_buf(),
        seeker: None,
        last_t: None,
    }))
}

/// AudioToolbox through AVAudioFile: float columns at the file's rate, encoder priming and padding removed.
fn decode_audio_pooled(path: &Path) -> R<Audio> {
    use objc2_avf_audio::{AVAudioFile, AVAudioPCMBuffer};
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.display().to_string()));
        let file = AVAudioFile::initForReading_error(AVAudioFile::alloc(), &url)
            .map_err(|e| format!("AVAudioFile: {}", e.localizedDescription()))?;
        let fmt = file.processingFormat();
        let (rate, ch) = (fmt.sampleRate(), fmt.channelCount() as usize);
        let total = file.length().max(0) as usize;
        let mut channels = vec![Vec::with_capacity(total); ch];
        let chunk = 1 << 16;
        loop {
            let buf = AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                &fmt,
                chunk,
            )
            .ok_or("AVAudioPCMBuffer")?;
            file.readIntoBuffer_error(&buf)
                .map_err(|e| format!("AVAudioFile read: {}", e.localizedDescription()))?;
            let n = buf.frameLength() as usize;
            if n == 0 {
                break;
            }
            let data = buf.floatChannelData();
            for (c, col) in channels.iter_mut().enumerate() {
                col.extend_from_slice(std::slice::from_raw_parts((*data.add(c)).as_ptr(), n));
            }
            if n < chunk as usize {
                break;
            }
        }
        Ok(Audio {
            rate: rate.round() as u32,
            channels,
        })
    }
}

/// Hardware H.264 through AVAssetWriter (VideoToolbox), BGRA pixel buffers from the writer's pool.
struct Encoder {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    video: std::path::PathBuf,
    out: std::path::PathBuf,
    opts: EncodeOpts,
    n: i64,
}

fn num(v: f64) -> Retained<NSNumber> {
    NSNumber::new_f64(v)
}

impl Encoder {
    fn new(out: &Path, o: &EncodeOpts) -> R<Encoder> {
        let video = out.with_extension("video.mp4");
        let _ = std::fs::remove_file(&video);
        unsafe {
            let url = NSURL::fileURLWithPath(&NSString::from_str(&video.display().to_string()));
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(
                &url,
                AVFileTypeMPEG4.ok_or("AVFileTypeMPEG4")?,
            )
            .map_err(|e| format!("AVAssetWriter: {}", e.localizedDescription()))?;
            let compression: Retained<NSDictionary<NSString, AnyObject>> =
                NSDictionary::from_slices(
                    &[
                        AVVideoAverageBitRateKey.ok_or("key")?,
                        AVVideoMaxKeyFrameIntervalKey.ok_or("key")?,
                        AVVideoAllowFrameReorderingKey.ok_or("key")?,
                    ],
                    // no B-frames: display order = decode order, like the edit's mezzanines
                    &[
                        &*num((o.kbps * 1000) as f64) as &AnyObject,
                        &*num((o.fps * 2.0).round()) as &AnyObject,
                        &*NSNumber::new_bool(false) as &AnyObject,
                    ],
                );
            let codec: &NSString = AVVideoCodecTypeH264.ok_or("h264")?;
            let settings: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
                &[
                    AVVideoCodecKey.ok_or("key")?,
                    AVVideoWidthKey.ok_or("key")?,
                    AVVideoHeightKey.ok_or("key")?,
                    AVVideoCompressionPropertiesKey.ok_or("key")?,
                ],
                &[
                    codec as &AnyObject,
                    &*num(o.width as f64) as &AnyObject,
                    &*num(o.height as f64) as &AnyObject,
                    &*compression as &AnyObject,
                ],
            );
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                AVMediaTypeVideo.ok_or("video")?,
                Some(&settings),
            );
            input.setExpectsMediaDataInRealTime(false);
            let attrs: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
                &[
                    cf_key(kCVPixelBufferPixelFormatTypeKey),
                    cf_key(kCVPixelBufferWidthKey),
                    cf_key(kCVPixelBufferHeightKey),
                ],
                &[
                    // NV12 from the engine's GPU readback goes in as it is; RGBA is swizzled to BGRA
                    &*NSNumber::new_u32(if o.nv12 { kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange } else { kCVPixelFormatType_32BGRA }) as &AnyObject,
                    &*num(o.width as f64) as &AnyObject,
                    &*num(o.height as f64) as &AnyObject,
                ],
            );
            let adaptor = AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(&input, Some(&attrs));
            writer.addInput(&input);
            if !writer.startWriting() {
                return Err(format!(
                    "AVAssetWriter didn't start: {:?}",
                    writer.error().map(|e| e.localizedDescription().to_string())
                ));
            }
            writer.startSessionAtSourceTime(CMTime::with_seconds(0.0, 600));
            Ok(Encoder {
                writer,
                input,
                adaptor,
                video,
                out: out.to_path_buf(),
                opts: o.clone(),
                n: 0,
            })
        }
    }
}

impl VideoEncoder for Encoder {
    fn push(&mut self, rgba: &[u8]) -> R<()> {
        objc2::rc::autoreleasepool(|_| self.push_pooled(rgba))
    }
    fn push_nv12(&mut self, nv12: &[u8], w: usize, h: usize) -> R<()> {
        if !self.opts.nv12 {
            return Err("apple encoder: made for RGBA (EncodeOpts.nv12 false)".into());
        }
        objc2::rc::autoreleasepool(|_| self.push_nv12_pooled(nv12, w, h))
    }
    fn finish(self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats> {
        objc2::rc::autoreleasepool(|_| self.finish_pooled(audio))
    }
}

impl Encoder {
    /// NV12 straight into a pool buffer of the same format: two plane copies, row by row (the pool's rows may be
    /// padded), no conversion.
    fn push_nv12_pooled(&mut self, nv12: &[u8], w: usize, h: usize) -> R<()> {
        unsafe {
            while !self.input.isReadyForMoreMediaData() {
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
            let pool = self.adaptor.pixelBufferPool().ok_or("no pixel buffer pool")?;
            let pb = pool_buffer(&pool)?;
            CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags(0));
            crate::encode_convert_timed(|| {
                for (plane, rows, off) in [(0usize, h, 0usize), (1, h / 2, w * h)] {
                    let base = CVPixelBufferGetBaseAddressOfPlane(&pb, plane) as *mut u8;
                    let bpr = CVPixelBufferGetBytesPerRowOfPlane(&pb, plane);
                    for r in 0..rows {
                        std::ptr::copy_nonoverlapping(nv12.as_ptr().add(off + r * w), base.add(r * bpr), w);
                    }
                }
            });
            CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags(0));
            let t = CMTime::with_seconds(self.n as f64 / self.opts.fps, 6000);
            if !self.adaptor.appendPixelBuffer_withPresentationTime(&pb, t) {
                return Err(format!("append failed: {:?}", self.writer.error().map(|e| e.localizedDescription().to_string())));
            }
        }
        self.n += 1;
        Ok(())
    }

    fn push_pooled(&mut self, rgba: &[u8]) -> R<()> {
        unsafe {
            while !self.input.isReadyForMoreMediaData() {
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
            let pool = self
                .adaptor
                .pixelBufferPool()
                .ok_or("no pixel buffer pool")?;
            let pb = pool_buffer(&pool)?;
            CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags(0));
            let (w, h, bpr) = (
                CVPixelBufferGetWidth(&pb),
                CVPixelBufferGetHeight(&pb),
                CVPixelBufferGetBytesPerRow(&pb),
            );
            let base = CVPixelBufferGetBaseAddress(&pb) as *mut u8;
            crate::encode_convert_timed(|| {
                for y in 0..h {
                    let row = std::slice::from_raw_parts_mut(base.add(y * bpr), w * 4);
                    let src = &rgba[y * w * 4..(y + 1) * w * 4];
                    for x in 0..w {
                        row[x * 4] = src[x * 4 + 2];
                        row[x * 4 + 1] = src[x * 4 + 1];
                        row[x * 4 + 2] = src[x * 4];
                        row[x * 4 + 3] = 255;
                    }
                }
            });
            CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags(0));
            let t = CMTime::with_seconds(self.n as f64 / self.opts.fps, 6000);
            if !self.adaptor.appendPixelBuffer_withPresentationTime(&pb, t) {
                return Err(format!(
                    "append failed: {:?}",
                    self.writer
                        .error()
                        .map(|e| e.localizedDescription().to_string())
                ));
            }
        }
        self.n += 1;
        Ok(())
    }
    fn finish_pooled(self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats> {
        unsafe {
            self.input.markAsFinished();
            let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let d2 = done.clone();
            let block =
                block2::RcBlock::new(move || d2.store(true, std::sync::atomic::Ordering::SeqCst));
            self.writer.finishWritingWithCompletionHandler(&block);
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            if self.writer.status() != AVAssetWriterStatus::Completed {
                return Err(format!(
                    "AVAssetWriter: {:?}",
                    self.writer
                        .error()
                        .map(|e| e.localizedDescription().to_string())
                ));
            }
        }
        let audio_codec = match audio {
            Some(a) => {
                let m4a = self.out.with_extension("aac.m4a");
                write_aac(&m4a, a)?;
                crate::remux::h264_aac(&self.video, &m4a, &self.out, self.opts.fps)?;
                let _ = std::fs::remove_file(&m4a);
                let _ = std::fs::remove_file(&self.video);
                "aac (AudioToolbox)".to_string()
            }
            None => {
                std::fs::rename(&self.video, &self.out).map_err(|e| e.to_string())?;
                String::new()
            }
        };
        Ok(EncodeStats {
            frames: self.n as usize,
            bytes: std::fs::metadata(&self.out).map(|m| m.len()).unwrap_or(0),
            video_codec: "h264 (VideoToolbox)".into(),
            audio_codec,
        })
    }
}

/// AAC (AudioToolbox, through AVAudioFile) from the audio columns.
/// The sound through AudioToolbox's AAC and back (what a player will decode from the file): for mastering to the
/// encoded result, as AAC can put the true peak over a limiter's ceiling.
pub fn aac_round_trip(a: &Audio) -> R<Audio> {
    let p = std::env::temp_dir().join(format!("raea-aac-{}-{:?}.m4a", std::process::id(), std::thread::current().id()));
    write_aac(&p, a)?;
    // Symphonia where it's built (what the mastering was measured with); else AudioToolbox's own reader
    #[cfg(feature = "soft-audio")]
    let back = crate::soft::decode_audio_symphonia(&p);
    #[cfg(not(feature = "soft-audio"))]
    let back = decode_audio_pooled(&p);
    let _ = std::fs::remove_file(&p);
    back
}

fn write_aac(path: &Path, a: &Audio) -> R<()> {
    use objc2_avf_audio::{
        AVAudioFile, AVAudioFormat, AVAudioPCMBuffer, AVEncoderBitRateKey, AVFormatIDKey,
        AVNumberOfChannelsKey, AVSampleRateKey,
    };
    let a = crate::soft_resample(a, 48000);
    let ch = a.channels.len().clamp(1, 2);
    let _ = std::fs::remove_file(path);
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.display().to_string()));
        let settings: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
            &[
                AVFormatIDKey.ok_or("key")?,
                AVSampleRateKey.ok_or("key")?,
                AVNumberOfChannelsKey.ok_or("key")?,
                AVEncoderBitRateKey.ok_or("key")?,
            ],
            &[
                &*NSNumber::new_u32(u32::from_be_bytes(*b"aac ")) as &AnyObject,
                &*num(48000.0) as &AnyObject,
                &*num(ch as f64) as &AnyObject,
                &*num(128000.0) as &AnyObject,
            ],
        );
        let file =
            AVAudioFile::initForWriting_settings_error(AVAudioFile::alloc(), &url, &settings)
                .map_err(|e| format!("AVAudioFile: {}", e.localizedDescription()))?;
        let fmt = AVAudioFormat::initStandardFormatWithSampleRate_channels(
            AVAudioFormat::alloc(),
            48000.0,
            ch as u32,
        )
        .ok_or("AVAudioFormat")?;
        let chunk = 48000usize;
        let mut i = 0;
        while i < a.len() {
            let n = chunk.min(a.len() - i);
            let buf = AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                &fmt,
                n as u32,
            )
            .ok_or("AVAudioPCMBuffer")?;
            buf.setFrameLength(n as u32);
            let data = buf.floatChannelData();
            for c in 0..ch {
                let dst = std::slice::from_raw_parts_mut((*data.add(c)).as_ptr(), n);
                dst.copy_from_slice(&a.channels[c][i..i + n]);
            }
            file.writeFromBuffer_error(&buf)
                .map_err(|e| format!("AVAudioFile write: {}", e.localizedDescription()))?;
            i += n;
        }
    }
    Ok(())
}


/// Playback as QuickTime does it, without an app: AVFoundation's verdict (playable, readable), then every video frame
/// and every audio sample decoded through AVAssetReader (video to BGRA, audio to linear PCM).
pub fn playable(path: &Path) -> R<serde_json::Value> {
    objc2::rc::autoreleasepool(|_| unsafe {
        let a = asset(path);
        #[allow(deprecated)]
        let (playable, readable) = (a.isPlayable(), a.isReadable());
        #[allow(deprecated)]
        let duration = a.duration().seconds();
        let read_track = |media: &AVMediaType, settings: Option<&NSDictionary<NSString, AnyObject>>| -> R<(usize, usize)> {
            #[allow(deprecated)]
            let tracks = a.tracksWithMediaType(media);
            let Some(track) = tracks.firstObject() else { return Ok((0, 0)) };
            let reader = AVAssetReader::assetReaderWithAsset_error(&a).map_err(|e| format!("AVAssetReader: {}", e.localizedDescription()))?;
            let out = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, settings);
            out.setAlwaysCopiesSampleData(false);
            reader.addOutput(&out);
            if !reader.startReading() {
                return Err(format!("AVAssetReader didn't start: {:?}", reader.error().map(|e| e.localizedDescription().to_string())));
            }
            let (mut buffers, mut samples) = (0usize, 0usize);
            while let Some(sb) = objc2::rc::autoreleasepool(|_| out.copyNextSampleBuffer()) {
                buffers += 1;
                samples += sb.num_samples() as usize;
            }
            if reader.status() == AVAssetReaderStatus::Failed {
                return Err(format!("decode failed: {:?}", reader.error().map(|e| e.localizedDescription().to_string())));
            }
            Ok((buffers, samples))
        };
        let bgra: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_slices(&[cf_key(kCVPixelBufferPixelFormatTypeKey)], &[&*NSNumber::new_u32(kCVPixelFormatType_32BGRA) as &AnyObject]);
        let pcm: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
            &[objc2_avf_audio::AVFormatIDKey.ok_or("key")?],
            &[&*NSNumber::new_u32(u32::from_be_bytes(*b"lpcm")) as &AnyObject],
        );
        let video = read_track(AVMediaTypeVideo.ok_or("video")?, Some(&bgra));
        let audio = read_track(AVMediaTypeAudio.ok_or("audio")?, Some(&pcm));
        Ok(serde_json::json!({
            "file": path.display().to_string(), "avfoundation_playable": playable, "avfoundation_readable": readable, "duration": duration,
            "video_frames_decoded": video.as_ref().map(|v| v.1).ok(), "video_error": video.err(),
            "audio_samples_decoded": audio.as_ref().map(|v| v.1).ok(), "audio_error": audio.err(),
        }))
    })
}

/// Frames the encoder may hold at once. AVAssetWriter's isReadyForMoreMediaData doesn't bound its queue when the input
/// isn't real-time (on the iPhone the hook rendered at 47 fps with 2.6 GB held, ~8 MB a frame), so the pixel buffer
/// pool does: past this many outstanding buffers a push waits for the encoder to give one back.
/// RAEA_ENC_INFLIGHT overrides it (0: unbounded, as before).
const FRAMES_IN_FLIGHT: i32 = 6;

/// A buffer from the encoder's pool, waiting while FRAMES_IN_FLIGHT are out.
unsafe fn pool_buffer(pool: &CVPixelBufferPool) -> R<objc2_core_foundation::CFRetained<CVPixelBuffer>> {
    use objc2_core_foundation::{CFDictionary, CFNumber, CFType};
    let key: &CFType = unsafe { objc2_core_video::kCVPixelBufferPoolAllocationThresholdKey };
    let cap = std::env::var("RAEA_ENC_INFLIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(FRAMES_IN_FLIGHT);
    if cap <= 0 {
        let mut pb: *mut CVPixelBuffer = std::ptr::null_mut();
        if unsafe { CVPixelBufferPool::create_pixel_buffer(None, pool, std::ptr::NonNull::new(&mut pb).unwrap()) } != 0 || pb.is_null() {
            return Err("CVPixelBufferPoolCreatePixelBuffer failed".into());
        }
        return Ok(unsafe { objc2_core_foundation::CFRetained::from_raw(std::ptr::NonNull::new(pb).unwrap()) });
    }
    let n = CFNumber::new_i32(cap);
    let aux = CFDictionary::<CFType, CFType>::from_slices(&[key], &[&n]);
    let t0 = std::time::Instant::now();
    loop {
        let mut pb: *mut CVPixelBuffer = std::ptr::null_mut();
        let r = unsafe { CVPixelBufferPool::create_pixel_buffer_with_aux_attributes(None, pool, Some(aux.as_opaque()), std::ptr::NonNull::new(&mut pb).unwrap()) };
        if r == objc2_core_video::kCVReturnWouldExceedAllocationThreshold {
            if t0.elapsed() > std::time::Duration::from_secs(10) {
                return Err("the encoder held its frames for 10 s".into());
            }
            std::thread::sleep(std::time::Duration::from_micros(200));
            continue;
        }
        if r != 0 || pb.is_null() {
            return Err(format!("CVPixelBufferPoolCreatePixelBufferWithAuxAttributes: {r}"));
        }
        return Ok(unsafe { objc2_core_foundation::CFRetained::from_raw(std::ptr::NonNull::new(pb).unwrap()) });
    }
}

/// Frames the VideoToolbox session may hold at once (RAEA_VT_INFLIGHT overrides): a push waits for the oldest to come
/// out, so memory stays at a few frames whatever the encoder's speed. 8 measured the same as 3 on ts1 (258-264 fps
/// both, paired x3, lightspeed T1/G6): the media engine is the bound, not the queue.
const VT_INFLIGHT: usize = 3;

struct VtState {
    inflight: usize,
    /// RAEA_MEM_MARKS diagnostics: the most frames ever in flight, bytes of samples kept
    max_inflight: usize,
    kept: usize,
    discard: bool,
    frames: Vec<Option<(Vec<u8>, bool)>>,
    /// SPS and PPS, Annex B, from the first sample's format description
    params: Option<Vec<u8>>,
    err: Option<String>,
}

struct VtShared {
    st: std::sync::Mutex<VtState>,
    cv: std::sync::Condvar,
    /// HEVC (Main10, Backend::encoder_10bit): parameter sets VPS, SPS, PPS and IRAP keyframes
    hevc: bool,
}

/// Hardware H.264 through a VTCompressionSession of our own: NV12 (or BGRA) pixel buffers from the session's pool, at
/// most VT_INFLIGHT frames in flight, the samples kept as Annex B and muxed by muxide (with AAC from AudioToolbox).
struct VtEncoder {
    session: objc2_core_foundation::CFRetained<objc2_video_toolbox::VTCompressionSession>,
    shared: Box<VtShared>,
    out: std::path::PathBuf,
    opts: EncodeOpts,
    cap: usize,
    n: i64,
    /// RAEA_VT_FLUSH=N: complete the frames every N pushes (the encoder emits and lets go of what it holds)
    flush: Option<i64>,
    /// RAEA_VT_OWNPOOL=N: pixel buffers from a pool of ours, at most N out, instead of the session's pool
    own_pool: Option<(objc2_core_foundation::CFRetained<CVPixelBufferPool>, i32)>,
    /// HEVC Main10 from P010 (Backend::encoder_10bit)
    hevc10: bool,
}

unsafe extern "C-unwind" fn vt_output(
    refcon: *mut std::ffi::c_void,
    frame: *mut std::ffi::c_void,
    status: i32,
    _flags: objc2_video_toolbox::VTEncodeInfoFlags,
    sb: *mut objc2_core_media::CMSampleBuffer,
) {
    // an autorelease pool of its own: VideoToolbox calls back on its own threads, and nothing autoreleased here may
    // wait for a pool that never drains (iOS)
    objc2::rc::autoreleasepool(|_| unsafe { vt_output_pooled(refcon, frame, status, sb) })
}

unsafe fn vt_output_pooled(refcon: *mut std::ffi::c_void, frame: *mut std::ffi::c_void, status: i32, sb: *mut objc2_core_media::CMSampleBuffer) {
    let sh = unsafe { &*(refcon as *const VtShared) };
    let idx = frame as usize;
    let mut st = sh.st.lock().unwrap();
    let got: Result<(Vec<u8>, bool), String> = (|| {
        if status != 0 || sb.is_null() {
            return Err(format!("VideoToolbox: frame {idx}: status {status}"));
        }
        let sb = unsafe { &*sb };
        if st.params.is_none() {
            if let Some(fd) = unsafe { sb.format_description() } {
                let mut p = Vec::new();
                let mut count = 0usize;
                let mut i = 0usize;
                loop {
                    let (mut ptr, mut len) = (std::ptr::null::<u8>(), 0usize);
                    let r = if sh.hevc {
                        unsafe { objc2_core_media::CMVideoFormatDescriptionGetHEVCParameterSetAtIndex(&fd, i, &mut ptr, &mut len, &mut count, std::ptr::null_mut()) }
                    } else {
                        unsafe { objc2_core_media::CMVideoFormatDescriptionGetH264ParameterSetAtIndex(&fd, i, &mut ptr, &mut len, &mut count, std::ptr::null_mut()) }
                    };
                    if r != 0 || ptr.is_null() {
                        break;
                    }
                    p.extend_from_slice(&[0, 0, 0, 1]);
                    p.extend_from_slice(unsafe { std::slice::from_raw_parts(ptr, len) });
                    i += 1;
                    if i >= count {
                        break;
                    }
                }
                st.params = Some(p);
            }
        }
        let bb = unsafe { sb.data_buffer() }.ok_or("no data buffer")?;
        let n = unsafe { bb.data_length() };
        let mut v = vec![0u8; n];
        if n > 0 && unsafe { bb.copy_data_bytes(0, n, std::ptr::NonNull::new(v.as_mut_ptr() as *mut std::ffi::c_void).unwrap()) } != 0 {
            return Err("CMBlockBufferCopyDataBytes failed".into());
        }
        // a keyframe has an IDR slice (NAL type 5; HEVC: an IRAP slice, types 16-21); the samples are AVCC, four-byte
        // lengths
        let mut key = false;
        let mut i = 0;
        while i + 4 < v.len() {
            let len = u32::from_be_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as usize;
            if if sh.hevc { (16..=21).contains(&((v[i + 4] >> 1) & 0x3f)) } else { v[i + 4] & 0x1f == 5 } {
                key = true;
            }
            i += 4 + len;
        }
        Ok((crate::remux::avcc_to_annexb(&v, if key { st.params.as_deref() } else { None }), key))
    })();
    match got {
        Ok(f) => {
            if st.frames.len() <= idx {
                st.frames.resize(idx + 1, None);
            }
            st.kept += f.0.len();
            // RAEA_VT_DISCARD (diagnostics): drop the samples, to tell our storage from the encoder's
            st.frames[idx] = Some(if st.discard { (Vec::new(), f.1) } else { f });
        }
        Err(e) => st.err = Some(e),
    }
    st.inflight -= 1;
    sh.cv.notify_all();
}

impl VtEncoder {
    /// `hevc10`: HEVC Main10 from P010 frames (push_p010) instead of H.264 High from NV12 or BGRA.
    fn new(out: &Path, o: &EncodeOpts, hevc10: bool) -> R<VtEncoder> {
        use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFType};
        use objc2_video_toolbox::*;
        let shared = Box::new(VtShared { st: std::sync::Mutex::new(VtState { inflight: 0, max_inflight: 0, kept: 0, discard: std::env::var_os("RAEA_VT_DISCARD").is_some(), frames: Vec::new(), params: None, err: None }), cv: std::sync::Condvar::new(), hevc: hevc10 });
        unsafe {
            let fmt = CFNumber::new_i32(if hevc10 {
                kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange
            } else if o.nv12 {
                kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
            } else {
                kCVPixelFormatType_32BGRA
            } as i32);
            let (wn, hn) = (CFNumber::new_i32(o.width as i32), CFNumber::new_i32(o.height as i32));
            fn k(x: &CFString) -> &CFType {
                x
            }
            let attrs = CFDictionary::<CFType, CFType>::from_slices(
                &[k(kCVPixelBufferPixelFormatTypeKey), k(kCVPixelBufferWidthKey), k(kCVPixelBufferHeightKey)],
                &[&*fmt as &CFType, &*wn as &CFType, &*hn as &CFType],
            );
            let mut sp: *mut VTCompressionSession = std::ptr::null_mut();
            let r = VTCompressionSession::create(
                None,
                o.width as i32,
                o.height as i32,
                if hevc10 { objc2_core_media::kCMVideoCodecType_HEVC } else { objc2_core_media::kCMVideoCodecType_H264 },
                None,
                Some(attrs.as_opaque()),
                None,
                Some(vt_output),
                &*shared as *const VtShared as *mut std::ffi::c_void,
                std::ptr::NonNull::new(&mut sp).unwrap(),
            );
            if r != 0 || sp.is_null() {
                return Err(format!("VTCompressionSessionCreate: {r}"));
            }
            let session = objc2_core_foundation::CFRetained::from_raw(std::ptr::NonNull::new(sp).unwrap());
            let vs: &VTSession = &*(sp as *const VTSession);
            let set = |key: &CFString, v: &CFType| -> R<()> {
                let r = VTSessionSetProperty(vs, key, Some(v));
                if r != 0 { Err(format!("VTSessionSetProperty {key}: {r}")) } else { Ok(()) }
            };
            // as the AVAssetWriter path: average bit rate, a keyframe every 2 s, no B-frames (display order = decode
            // order, like the edit's mezzanines); not real-time, so the encoder may take its time per frame
            set(kVTCompressionPropertyKey_AverageBitRate, &CFNumber::new_i32((o.kbps * 1000) as i32))?;
            set(kVTCompressionPropertyKey_MaxKeyFrameInterval, &CFNumber::new_i32((o.fps * 2.0).round() as i32))?;
            // RAEA_VT_FPS=N (diagnostics): the frame rate the encoder plans for (real-time mode paces to it)
            let expected = std::env::var("RAEA_VT_FPS").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(o.fps);
            set(kVTCompressionPropertyKey_ExpectedFrameRate, &CFNumber::new_f64(expected))?;
            // RAEA_VT_POWER=0 (diagnostics): don't trade speed for power
            if std::env::var("RAEA_VT_POWER").map(|v| v == "0").unwrap_or(false) {
                if let Err(e) = set(kVTCompressionPropertyKey_MaximizePowerEfficiency, CFBoolean::new(false)) {
                    eprintln!("RAEA_VT_POWER: {e} (ignored)");
                }
            }
            // RAEA_VT_SPEED=1 (diagnostics): prioritise encoding speed over quality
            if std::env::var_os("RAEA_VT_SPEED").is_some() {
                if let Err(e) = set(kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality, CFBoolean::new(true)) {
                    eprintln!("RAEA_VT_SPEED: {e} (ignored)");
                }
            }
            set(kVTCompressionPropertyKey_AllowFrameReordering, CFBoolean::new(false))?;
            // real-time for 3D content (EncodeOpts.realtime), else not: see EncodeOpts.realtime
            let realtime = match std::env::var("RAEA_VT_REALTIME").ok().as_deref() {
                Some("0") => false,
                Some(_) => true,
                None => o.realtime,
            };
            set(kVTCompressionPropertyKey_RealTime, CFBoolean::new(realtime))?;
            if std::env::var_os("RAEA_MEM_MARKS").is_some() {
                eprintln!("vt_mark session realtime {realtime}");
            }
            set(kVTCompressionPropertyKey_ProfileLevel, if hevc10 { kVTProfileLevel_HEVC_Main10_AutoLevel } else { kVTProfileLevel_H264_High_AutoLevel })?;
            // BT.709 in the stream's own colour description (the SPS VUI): untagged HD reads as BT.601 in some players,
            // which shifts skin tones (the frames are converted with BT.709's matrix)
            set(kVTCompressionPropertyKey_ColorPrimaries, kCVImageBufferColorPrimaries_ITU_R_709_2)?;
            set(kVTCompressionPropertyKey_TransferFunction, kCVImageBufferTransferFunction_ITU_R_709_2)?;
            set(kVTCompressionPropertyKey_YCbCrMatrix, kCVImageBufferYCbCrMatrix_ITU_R_709_2)?;
            // RAEA_VT_DELAY=N: at most N frames held for lookahead before output (diagnostics for the iPhone's growth)
            if let Some(d) = std::env::var("RAEA_VT_DELAY").ok().and_then(|v| v.parse::<i32>().ok()) {
                if let Err(e) = set(kVTCompressionPropertyKey_MaxFrameDelayCount, &CFNumber::new_i32(d)) {
                    eprintln!("RAEA_VT_DELAY: {e} (not supported by this encoder; ignored)");
                }
            }
            let cap = std::env::var("RAEA_VT_INFLIGHT").ok().and_then(|v| v.parse().ok()).filter(|c: &usize| *c > 0).unwrap_or(VT_INFLIGHT);
            let flush = std::env::var("RAEA_VT_FLUSH").ok().and_then(|v| v.parse::<i64>().ok()).filter(|n| *n > 0);
            let own_pool = match std::env::var("RAEA_VT_OWNPOOL").ok().and_then(|v| v.parse::<i32>().ok()).filter(|n| *n > 0) {
                Some(n) => {
                    let io = CFDictionary::<CFType, CFType>::from_slices(&[], &[]);
                    let pattrs = CFDictionary::<CFType, CFType>::from_slices(
                        &[k(kCVPixelBufferPixelFormatTypeKey), k(kCVPixelBufferWidthKey), k(kCVPixelBufferHeightKey), k(kCVPixelBufferIOSurfacePropertiesKey)],
                        &[&*fmt as &CFType, &*wn as &CFType, &*hn as &CFType, &*io as &CFType],
                    );
                    let mut pp: *mut CVPixelBufferPool = std::ptr::null_mut();
                    let r = CVPixelBufferPool::create(None, None, Some(pattrs.as_opaque()), std::ptr::NonNull::new(&mut pp).unwrap());
                    if r != 0 || pp.is_null() {
                        return Err(format!("CVPixelBufferPoolCreate: {r}"));
                    }
                    Some((objc2_core_foundation::CFRetained::from_raw(std::ptr::NonNull::new(pp).unwrap()), n))
                }
                None => None,
            };
            Ok(VtEncoder { session, shared, out: out.to_path_buf(), opts: o.clone(), cap, n: 0, flush, own_pool, hevc10 })
        }
    }

    /// Waits for a free slot, fills one of the session's pixel buffers, and submits it.
    fn submit(&mut self, fill: impl FnOnce(&CVPixelBuffer)) -> R<()> {
        let t0 = std::time::Instant::now();
        {
            let mut st = self.shared.st.lock().unwrap();
            while st.inflight >= self.cap && st.err.is_none() {
                st = self.shared.cv.wait(st).unwrap();
            }
            if let Some(e) = st.err.take() {
                return Err(e);
            }
            st.inflight += 1;
            st.max_inflight = st.max_inflight.max(st.inflight);
            if std::env::var_os("RAEA_MEM_MARKS").is_some() && self.n % 25 == 0 {
                eprintln!("vt_mark frame {} in_flight {} max_in_flight {} kept_bytes {}", self.n, st.inflight, st.max_inflight, st.kept);
            }
        }
        unsafe {
            let pb = match &self.own_pool {
                // ours, bounded: past N buffers out, wait for the encoder to give one back
                Some((pool, n)) => match threshold_buffer(pool, *n) {
                    Ok(pb) => pb,
                    Err(e) => {
                        self.shared.st.lock().unwrap().inflight -= 1;
                        return Err(e);
                    }
                },
                None => {
                    let pool = self.session.pixel_buffer_pool().ok_or("no pixel buffer pool")?;
                    let mut pb: *mut CVPixelBuffer = std::ptr::null_mut();
                    if CVPixelBufferPool::create_pixel_buffer(None, &pool, std::ptr::NonNull::new(&mut pb).unwrap()) != 0 || pb.is_null() {
                        self.shared.st.lock().unwrap().inflight -= 1;
                        return Err("CVPixelBufferPoolCreatePixelBuffer failed".into());
                    }
                    objc2_core_foundation::CFRetained::from_raw(std::ptr::NonNull::new(pb).unwrap())
                }
            };
            let t1 = std::time::Instant::now();
            CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags(0));
            crate::encode_convert_timed(|| fill(&pb));
            CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags(0));
            let t2 = std::time::Instant::now();
            let t = CMTime::with_seconds(self.n as f64 / self.opts.fps, 6000);
            let d = CMTime::with_seconds(1.0 / self.opts.fps, 6000);
            let r = self.session.encode_frame(&pb, t, d, None, self.n as usize as *mut std::ffi::c_void, std::ptr::null_mut());
            for (i, s) in [t1 - t0, t2 - t1, t2.elapsed()].into_iter().enumerate() {
                VT_PUSH_NS[i].fetch_add(s.as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
            }
            if r != 0 {
                self.shared.st.lock().unwrap().inflight -= 1;
                return Err(format!("VTCompressionSessionEncodeFrame: {r}"));
            }
        }
        self.n += 1;
        if let Some(every) = self.flush {
            if self.n % every == 0 {
                unsafe { self.session.complete_frames(objc2_core_media::kCMTimeInvalid) };
            }
        }
        Ok(())
    }
}

impl VideoEncoder for VtEncoder {
    fn push(&mut self, rgba: &[u8]) -> R<()> {
        if self.opts.nv12 || self.hevc10 {
            return Err("apple encoder: made for NV12 or P010, not RGBA".into());
        }
        objc2::rc::autoreleasepool(|_| {
            self.submit(|pb| unsafe {
                let (w, h, bpr) = (CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb), CVPixelBufferGetBytesPerRow(pb));
                let base = CVPixelBufferGetBaseAddress(pb) as *mut u8;
                for y in 0..h {
                    let row = std::slice::from_raw_parts_mut(base.add(y * bpr), w * 4);
                    let src = &rgba[y * w * 4..(y + 1) * w * 4];
                    for x in 0..w {
                        row[x * 4] = src[x * 4 + 2];
                        row[x * 4 + 1] = src[x * 4 + 1];
                        row[x * 4 + 2] = src[x * 4];
                        row[x * 4 + 3] = 255;
                    }
                }
            })
        })
    }
    fn push_p010(&mut self, p010: &[u8], w: usize, h: usize) -> R<()> {
        if !self.hevc10 {
            return Err("apple encoder: made for 8-bit frames (P010 needs Backend::encoder_10bit)".into());
        }
        if p010.len() < w * h * 3 {
            return Err(format!("push_p010: {} bytes for {w}x{h} (want {})", p010.len(), w * h * 3));
        }
        objc2::rc::autoreleasepool(|_| {
            self.submit(|pb| unsafe {
                // two bytes a sample: luma rows of w samples, then h/2 rows of w/2 Cb Cr pairs
                for (plane, rows, off) in [(0usize, h, 0usize), (1, h / 2, w * h * 2)] {
                    let base = CVPixelBufferGetBaseAddressOfPlane(pb, plane) as *mut u8;
                    let bpr = CVPixelBufferGetBytesPerRowOfPlane(pb, plane);
                    for r in 0..rows {
                        std::ptr::copy_nonoverlapping(p010.as_ptr().add(off + r * w * 2), base.add(r * bpr), w * 2);
                    }
                }
            })
        })
    }
    fn push_nv12(&mut self, nv12: &[u8], w: usize, h: usize) -> R<()> {
        if !self.opts.nv12 || self.hevc10 {
            return Err("apple encoder: made for RGBA or P010, not NV12".into());
        }
        objc2::rc::autoreleasepool(|_| {
            self.submit(|pb| unsafe {
                for (plane, rows, off) in [(0usize, h, 0usize), (1, h / 2, w * h)] {
                    let base = CVPixelBufferGetBaseAddressOfPlane(pb, plane) as *mut u8;
                    let bpr = CVPixelBufferGetBytesPerRowOfPlane(pb, plane);
                    for r in 0..rows {
                        std::ptr::copy_nonoverlapping(nv12.as_ptr().add(off + r * w), base.add(r * bpr), w);
                    }
                }
            })
        })
    }
    fn finish(self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats> {
        if std::env::var_os("RAEA_ENC_TRACE").is_some() && self.n > 0 {
            let ms = |i: usize| VT_PUSH_NS[i].load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6 / self.n as f64;
            eprintln!("raea encode: {} frames, a frame's submit: waiting for room and a buffer {:.3} ms, the copy into it {:.3} ms, encode_frame {:.3} ms (max in flight {})", self.n, ms(0), ms(1), ms(2), self.cap);
        }
        unsafe {
            self.session.complete_frames(objc2_core_media::kCMTimeInvalid);
        }
        {
            let mut st = self.shared.st.lock().unwrap();
            while st.inflight > 0 && st.err.is_none() {
                st = self.shared.cv.wait(st).unwrap();
            }
        }
        unsafe { self.session.invalidate() };
        let mut st = self.shared.st.lock().unwrap();
        if let Some(e) = st.err.take() {
            return Err(e);
        }
        let frames: Vec<(Vec<u8>, bool)> = std::mem::take(&mut st.frames).into_iter().enumerate().map(|(i, f)| f.ok_or(format!("VideoToolbox: frame {i} never came out"))).collect::<R<_>>()?;
        if frames.len() != self.n as usize {
            return Err(format!("VideoToolbox: {} of {} frames came out", frames.len(), self.n));
        }
        if std::env::var_os("RAEA_MEM_MARKS").is_some() {
            eprintln!("vt_mark finish frames {} max_in_flight {} kept_bytes {}", frames.len(), st.max_inflight, st.kept);
        }
        if st.discard {
            return Ok(EncodeStats { frames: frames.len(), bytes: 0, video_codec: "h264 (VideoToolbox, discarded)".into(), audio_codec: String::new() });
        }
        let codec = if self.hevc10 { muxide::api::VideoCodec::H265 } else { muxide::api::VideoCodec::H264 };
        let audio_codec = match audio {
            Some(a) => {
                let m4a = self.out.with_extension("aac.m4a");
                write_aac(&m4a, a)?;
                crate::remux::annexb_mux(codec, &frames, self.opts.width, self.opts.height, self.opts.fps, Some(&m4a), &self.out)?;
                let _ = std::fs::remove_file(&m4a);
                "aac (AudioToolbox)".to_string()
            }
            None => {
                crate::remux::annexb_mux(codec, &frames, self.opts.width, self.opts.height, self.opts.fps, None, &self.out)?;
                String::new()
            }
        };
        let video_codec = if self.hevc10 { "hevc main10 (VideoToolbox)" } else { "h264 (VideoToolbox)" };
        Ok(EncodeStats { frames: frames.len(), bytes: std::fs::metadata(&self.out).map(|m| m.len()).unwrap_or(0), video_codec: video_codec.into(), audio_codec })
    }
}

/// A VtEncoder submit's time (RAEA_ENC_TRACE): waiting for room and a buffer, the copy into it, encode_frame
static VT_PUSH_NS: [std::sync::atomic::AtomicU64; 3] = [const { std::sync::atomic::AtomicU64::new(0) }; 3];

/// A buffer from `pool` while fewer than `cap` are out; waits for one to come back otherwise (10 s at most).
unsafe fn threshold_buffer(pool: &CVPixelBufferPool, cap: i32) -> R<objc2_core_foundation::CFRetained<CVPixelBuffer>> {
    use objc2_core_foundation::{CFDictionary, CFNumber, CFType};
    let key: &CFType = unsafe { objc2_core_video::kCVPixelBufferPoolAllocationThresholdKey };
    let n = CFNumber::new_i32(cap);
    let aux = CFDictionary::<CFType, CFType>::from_slices(&[key], &[&n]);
    let t0 = std::time::Instant::now();
    loop {
        let mut pb: *mut CVPixelBuffer = std::ptr::null_mut();
        let r = unsafe { CVPixelBufferPool::create_pixel_buffer_with_aux_attributes(None, pool, Some(aux.as_opaque()), std::ptr::NonNull::new(&mut pb).unwrap()) };
        if r == objc2_core_video::kCVReturnWouldExceedAllocationThreshold {
            if t0.elapsed() > std::time::Duration::from_secs(10) {
                return Err(format!("the encoder held {cap} buffers for 10 s"));
            }
            std::thread::sleep(std::time::Duration::from_micros(200));
            continue;
        }
        if r != 0 || pb.is_null() {
            return Err(format!("CVPixelBufferPoolCreatePixelBufferWithAuxAttributes: {r}"));
        }
        return Ok(unsafe { objc2_core_foundation::CFRetained::from_raw(std::ptr::NonNull::new(pb).unwrap()) });
    }
}
