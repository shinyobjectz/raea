//! The decoder: AVAssetReader frames as an as-of lookup on their times, watched for stalls, recycled every few hundred
//! frames.

use super::*;

pub(crate) struct Decoder {
    pub(crate) asset: Retained<AVURLAsset>,
    pub(crate) info: Info,
    pub(crate) reader: Option<(
        Retained<AVAssetReader>,
        Retained<AVAssetReaderOutput>,
    )>,
    pub(crate) cur: Option<Slot>,
    pub(crate) next: Option<Slot>,
    pub(crate) since_restart: usize,
    /// VideoToolbox hands over NV12 planes (no conversion) instead of BGRA
    pub(crate) nv12: bool,
    /// the track's frames carry alpha (ProRes 4444, HEVC with alpha): BGRA's alpha kept, not made 255 (#113)
    pub(crate) alpha: bool,
    /// surface mode: NV12 left in VideoToolbox's IOSurface-backed, Metal-compatible buffers (no copy)
    pub(crate) surface: bool,
    /// the presentation times, read once from the compressed samples (for index_at)
    pub(crate) sample_pts: Option<Vec<f64>>,
    /// the edit list's shift from the track's sample times to the reader's (as read_pts and vtseek have it), read once
    pub(crate) shift: Option<f64>,
    pub(crate) watch: Watch,
    /// the file, for a seek on VideoToolbox directly
    pub(crate) path: std::path::PathBuf,
    /// that seeker, made at the first seek (None inside: this track can't seek directly), and whether it gives BGRA
    pub(crate) seeker: Option<(Option<crate::vtseek::Seeker>, bool)>,
    /// the time asked for last: a request just after it is play going on, anything else a seek
    pub(crate) last_t: Option<f64>,
}

/// How long one read may block before the watch cancels it.
pub(crate) const STALL_MS: u64 = 15_000;
pub(crate) const STALLED: &str = "the video decoder stalled (AVAssetReader stopped answering)";

/// A stalled read's way out: copyNextSampleBuffer on a composition output sometimes blocks forever on long iPhone takes
/// (2026-10-17: a render 2 h, scene detection 37 min, both at 0% CPU in FigSemaphoreWaitRelative). Nothing on the
/// reading thread can end it, but cancelReading from another thread does (Apple: callable from any thread), and the
/// read returns nil. One thread per decoder watches a deadline the read arms; the reader is a raw pointer read and
/// cancelled only under the lock, and the decoder clears it under the same lock before it lets the reader go.
pub(crate) struct Watch {
    pub(crate) deadline: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub(crate) reader: std::sync::Arc<std::sync::Mutex<usize>>,
    pub(crate) fired: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Watch {
    pub(crate) fn new() -> Watch {
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
    pub(crate) fn set(&self, r: Option<&AVAssetReader>) {
        *self.reader.lock().unwrap() = r.map(|r| r as *const AVAssetReader as usize).unwrap_or(0);
    }
}

/// A decoded frame, and in surface mode the buffer it still lives in.
pub(crate) type Slot = (Frame, Option<objc2_core_foundation::CFRetained<CVPixelBuffer>>);

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
    /// A fresh reader for the frame at `t`, started half a frame before it.
    pub(crate) fn restart(&mut self, t: f64) -> R<()> {
        self.restart_from((t - 0.5 / self.info.fps).max(0.0))
    }

    /// A fresh reader whose time range starts at `start`: it decodes from the keyframe at or before it.
    pub(crate) fn restart_from(&mut self, start: f64) -> R<()> {
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
            reader.setTimeRange(CMTimeRange {
                start: CMTime::with_seconds(start, 1_000_000),
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

    pub(crate) fn read(&mut self) -> R<Option<Slot>> {
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
                raea_io_codec::NV12_COPY_NS.fetch_add(t_copy.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
                CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
                return Ok(Some((Frame { pts, width: w as u32, height: h as u32, rgba: out }, None)));
            }
            let base = CVPixelBufferGetBaseAddress(pb) as *const u8;
            let rgba = raea_io_codec::convert_timed(|| {
                let mut rgba = vec![0u8; w * h * 4];
                for y in 0..h {
                    let row = std::slice::from_raw_parts(base.add(y * bpr), w * 4);
                    crate::convert::bgra_to_rgba(row, &mut rgba[y * w * 4..(y + 1) * w * 4], self.alpha);
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
        self.sample_pts.as_ref().filter(|p| !p.is_empty()).map(|p| raea_io_codec::as_of(p, t + 1e-4 - 1e-6))
    }
}

/// Frames read before the reader (and its VideoToolbox session) is replaced: bounds the decoder service's memory.
pub(crate) const RECYCLE_FRAMES: usize = 300;

impl Decoder {
    /// The frame at `t` from the direct seeker, when `t` is a seek the reader can't serve from what it holds.
    pub(crate) fn seek_direct(&mut self, t: f64) -> R<Option<SurfaceFrame>> {
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
                raea_io_trace::event("video_error", serde_json::json!({"source": "video", "file": self.path, "at_s": t, "error": e}));
                return Err(e);
            }
        };
        let ms = t1.elapsed().as_secs_f64() * 1e3;
        if trace {
            eprintln!("vtseek: {t:.3} decoded {} in {ms:.1} ms", s.decoded);
        }
        // a seek over the scrub budget, in the run's trace with what it decoded
        if ms > 50.0 {
            raea_io_trace::event("video_slow", serde_json::json!({"source": "video", "file": self.path, "at_s": t, "ms": (ms * 10.0).round() / 10.0, "decoded": s.decoded}));
        }
        let (width, height) = (s.width, s.height);
        // the reader's frames are behind now: play going on from here restarts it
        self.cur = None;
        self.next = None;
        Ok(Some(SurfaceFrame { pts, width, height, buffer }))
    }

    /// Every sample's presentation time, sorted, from a pass over the compressed samples (no decoding).
    pub(crate) fn read_pts(&self) -> R<Vec<f64>> {
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
    pub(crate) fn mode(&mut self, nv12: bool, surface: bool) {
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
    pub(crate) fn frame_at_pooled(&mut self, t: f64) -> R<Slot> {
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

    /// The latest keyframe at or before `t`, in the reader's times: a cursor at `t` stepped back in decode order to a
    /// sync sample shown by then (a GOP's steps or two, nothing decoded; an open GOP's keyframe is decoded before the
    /// leading frames shown ahead of it, so one found first may not be due yet); None when the track has no cursor.
    pub(crate) fn key_before(&mut self, t: f64) -> Option<f64> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let media = AVMediaTypeVideo?;
            #[allow(deprecated)]
            let tracks = self.asset.tracksWithMediaType(media);
            let track = tracks.firstObject()?;
            let shift = *self.shift.get_or_insert_with(|| {
                track.segments().iter().find(|s| !s.isEmpty()).map(|s| s.timeMapping()).map(|m| m.target.start.seconds() - m.source.start.seconds()).unwrap_or(0.0)
            });
            let cur = track.makeSampleCursorWithPresentationTimeStamp(CMTime::with_seconds(t - shift, 1_000_000))?;
            for _ in 0..2000 {
                let pts = cur.presentationTimeStamp().seconds() + shift;
                if cur.currentSampleSyncInfo().sampleIsFullSync.as_bool() && pts <= t {
                    return Some(pts);
                }
                if cur.stepInDecodeOrderByCount(-1) != -1 {
                    return None;
                }
            }
            None
        })
    }

    pub(crate) fn frame_at_once(&mut self, t: f64) -> R<Slot> {
        let eps = 1e-4;
        if self.since_restart >= RECYCLE_FRAMES {
            // a fresh reader (it bounds VideoToolbox's memory) started at a keyframe play has reached, so it decodes
            // nothing it won't show. Started between keyframes, it first decoded the GOP up to there: in play a hitch
            // every 300 frames (46 ms at 1080p60, 163 ms at 4K30 with 250-frame GOPs; tests/play_timing.rs). Until a
            // keyframe comes the old reader goes on, up to twice as long.
            let held = self.cur.as_ref().map(|c| c.0.pts);
            let key = held.and_then(|h| self.key_before(t + eps).filter(|&k| k > h + eps));
            if let Some(k) = key {
                self.restart_from(k + 1e-4)?;
                self.cur = self.read()?;
            } else if self.since_restart >= 2 * RECYCLE_FRAMES {
                self.restart(t)?;
                self.cur = self.read()?;
            }
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
