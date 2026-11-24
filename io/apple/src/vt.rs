//! The VideoToolbox encoder: a VTCompressionSession driven directly, a bounded number of frames in flight, muxed by
//! muxide.

use super::*;

/// Frames the VideoToolbox session may hold at once (RAEA_VT_INFLIGHT overrides): a push waits for the oldest to come
/// out, so memory stays at a few frames whatever the encoder's speed. 8 measured the same as 3 on ts1 (258-264 fps
/// both, paired x3, lightspeed T1/G6): the media engine is the bound, not the queue.
pub(crate) const VT_INFLIGHT: usize = 3;

/// Why a full queue doesn't just wait (#83): above 4096x2304 (H.264 Level 5.2's ceiling) VideoToolbox gives an H.264
/// session its software encoder, though VTCompressionSessionCreate succeeds, and that one emits nothing until it holds
/// 13 frames. With VT_INFLIGHT out, a push then waited forever (and the watchdog blamed the GPU). So a queue that's full
/// with nothing out for VT_STUCK has its frames completed (what RAEA_VT_FLUSH does), and every full queue after that;
/// nothing out for VT_DEADLINE fails the render instead of hanging it. Hardware encoders never go quiet that long.
const VT_STUCK: std::time::Duration = std::time::Duration::from_millis(300);
const VT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

/// The render's error when the encoder gave nothing back for VT_DEADLINE.
fn vt_stuck(o: &EncodeOpts, hevc10: bool) -> String {
    let codec = if hevc10 { "HEVC" } else { "H.264" };
    format!("VideoToolbox: no frame came out of the {codec} encoder for {} s at {}x{}; --10bit encodes HEVC on the hardware up to 8K", VT_DEADLINE.as_secs(), o.width, o.height)
}

pub(crate) struct VtState {
    pub(crate) inflight: usize,
    /// RAEA_MEM_MARKS diagnostics: the most frames ever in flight, bytes of samples kept
    pub(crate) max_inflight: usize,
    pub(crate) kept: usize,
    pub(crate) discard: bool,
    /// each frame's sample on disk as it comes out, not in memory (SG4a: a 7-minute film's 1 GB)
    pub(crate) samples: Option<raea_io_codec::samples::Samples>,
    /// SPS and PPS, Annex B, from the first sample's format description
    pub(crate) params: Option<Vec<u8>>,
    pub(crate) err: Option<String>,
}

pub(crate) struct VtShared {
    pub(crate) st: std::sync::Mutex<VtState>,
    pub(crate) cv: std::sync::Condvar,
    /// HEVC (Main10, Backend::encoder_10bit): parameter sets VPS, SPS, PPS and IRAP keyframes
    pub(crate) hevc: bool,
}

/// Hardware H.264 through a VTCompressionSession of our own: NV12 (or BGRA) pixel buffers from the session's pool, at
/// most VT_INFLIGHT frames in flight, the samples kept as Annex B and muxed by muxide (with AAC from AudioToolbox).
pub(crate) struct VtEncoder {
    pub(crate) session: objc2_core_foundation::CFRetained<objc2_video_toolbox::VTCompressionSession>,
    pub(crate) shared: Box<VtShared>,
    pub(crate) out: std::path::PathBuf,
    pub(crate) opts: EncodeOpts,
    pub(crate) cap: usize,
    pub(crate) n: i64,
    /// RAEA_VT_FLUSH=N: complete the frames every N pushes (the encoder emits and lets go of what it holds)
    pub(crate) flush: Option<i64>,
    /// RAEA_VT_OWNPOOL=N: pixel buffers from a pool of ours, at most N out, instead of the session's pool
    pub(crate) own_pool: Option<(objc2_core_foundation::CFRetained<CVPixelBufferPool>, i32)>,
    /// HEVC Main10 from P010 (Backend::encoder_10bit)
    pub(crate) hevc10: bool,
    /// a full queue went quiet once (VT_STUCK): every full queue since completes its frames at once
    pub(crate) rescued: bool,
}

pub(crate) unsafe extern "C-unwind" fn vt_output(
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

pub(crate) unsafe fn vt_output_pooled(refcon: *mut std::ffi::c_void, frame: *mut std::ffi::c_void, status: i32, sb: *mut objc2_core_media::CMSampleBuffer) {
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
        Ok((raea_io_codec::remux::avcc_to_annexb(&v, if key { st.params.as_deref() } else { None }), key))
    })();
    match got {
        Ok(f) => {
            // RAEA_VT_DISCARD (diagnostics): drop the samples, to tell our storage from the encoder's
            let data: &[u8] = if st.discard { &[] } else { &f.0 };
            st.kept += data.len();
            if let Some(Err(e)) = st.samples.as_mut().map(|s| s.put(idx, data, f.1)) {
                st.err = Some(format!("VideoToolbox: {e}"));
            }
        }
        Err(e) => st.err = Some(e),
    }
    st.inflight -= 1;
    sh.cv.notify_all();
}

impl VtEncoder {
    /// `hevc10`: HEVC Main10 from P010 frames (push_p010) instead of H.264 High from NV12 or BGRA.
    pub(crate) fn new(out: &Path, o: &EncodeOpts, hevc10: bool) -> R<VtEncoder> {
        use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFType};
        use objc2_video_toolbox::*;
        let shared = Box::new(VtShared { st: std::sync::Mutex::new(VtState { inflight: 0, max_inflight: 0, kept: 0, discard: std::env::var_os("RAEA_VT_DISCARD").is_some(), samples: Some(raea_io_codec::samples::Samples::new()?), params: None, err: None }), cv: std::sync::Condvar::new(), hevc: hevc10 });
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
            // or constant quality (o.quality): no bitrate target, each frame the bits its picture needs
            match o.quality {
                Some(q) => set(kVTCompressionPropertyKey_Quality, &CFNumber::new_f32(q.clamp(0.0, 1.0)))?,
                None => set(kVTCompressionPropertyKey_AverageBitRate, &CFNumber::new_i32((o.kbps * 1000) as i32))?,
            }
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
            if raea_io_trace::stat("RAEA_MEM_MARKS") {
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
            Ok(VtEncoder { session, shared, out: out.to_path_buf(), opts: o.clone(), cap, n: 0, flush, own_pool, hevc10, rescued: false })
        }
    }

    /// Waits for a free slot, fills one of the session's pixel buffers, and submits it.
    pub(crate) fn submit(&mut self, fill: impl FnOnce(&CVPixelBuffer)) -> R<()> {
        let t0 = std::time::Instant::now();
        {
            let mut st = self.shared.st.lock().unwrap();
            let mut quiet = std::time::Instant::now();
            while st.inflight >= self.cap && st.err.is_none() {
                if quiet.elapsed() >= VT_DEADLINE {
                    return Err(vt_stuck(&self.opts, self.hevc10));
                }
                let before = st.inflight;
                if self.rescued || quiet.elapsed() >= VT_STUCK {
                    if !self.rescued {
                        self.rescued = true;
                        eprintln!("raea encode: VideoToolbox's encoder holds frames back at {}x{} (its software H.264 above 4096x2304), so they're completed as they go, slower; --10bit encodes HEVC on the hardware", self.opts.width, self.opts.height);
                    }
                    // unlocked: the output callback takes the lock for each frame complete_frames lets out
                    drop(st);
                    unsafe { self.session.complete_frames(objc2_core_media::kCMTimeInvalid) };
                    st = self.shared.st.lock().unwrap();
                }
                if st.inflight >= before && st.err.is_none() {
                    st = self.shared.cv.wait_timeout(st, VT_STUCK).unwrap().0;
                }
                if st.inflight < before {
                    quiet = std::time::Instant::now();
                }
            }
            if let Some(e) = st.err.take() {
                return Err(e);
            }
            st.inflight += 1;
            st.max_inflight = st.max_inflight.max(st.inflight);
            if raea_io_trace::stat("RAEA_MEM_MARKS") && self.n % 25 == 0 {
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
            raea_io_codec::encode_convert_timed(|| fill(&pb));
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

/// An encoder dropped on an error (a stuck one's VT_DEADLINE among them) still holding frames: the session is torn down
/// first, so no output callback reaches its shared state once that's freed. finish() has invalidated it already.
impl Drop for VtEncoder {
    fn drop(&mut self) {
        unsafe { self.session.invalidate() };
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
        if raea_io_trace::stat("RAEA_ENC_TRACE") && self.n > 0 {
            let ms = |i: usize| VT_PUSH_NS[i].load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6 / self.n as f64;
            eprintln!("raea encode: {} frames, a frame's submit: waiting for room and a buffer {:.3} ms, the copy into it {:.3} ms, encode_frame {:.3} ms (max in flight {})", self.n, ms(0), ms(1), ms(2), self.cap);
        }
        unsafe {
            self.session.complete_frames(objc2_core_media::kCMTimeInvalid);
        }
        {
            let mut st = self.shared.st.lock().unwrap();
            let mut quiet = std::time::Instant::now();
            while st.inflight > 0 && st.err.is_none() {
                if quiet.elapsed() >= VT_DEADLINE {
                    return Err(vt_stuck(&self.opts, self.hevc10));
                }
                let before = st.inflight;
                st = self.shared.cv.wait_timeout(st, VT_STUCK).unwrap().0;
                if st.inflight < before {
                    quiet = std::time::Instant::now();
                }
            }
        }
        unsafe { self.session.invalidate() };
        let mut st = self.shared.st.lock().unwrap();
        if let Some(e) = st.err.take() {
            return Err(e);
        }
        // each frame's sample on disk (samples::Samples), read back one at a time as the mux asks
        let frames = st.samples.take().ok_or("VideoToolbox: finished twice")?;
        if let Some(i) = frames.missing() {
            return Err(format!("VideoToolbox: frame {i} never came out"));
        }
        if frames.len() != self.n as usize {
            return Err(format!("VideoToolbox: {} of {} frames came out", frames.len(), self.n));
        }
        if raea_io_trace::stat("RAEA_MEM_MARKS") {
            eprintln!("vt_mark finish frames {} max_in_flight {} kept_bytes {}", frames.len(), st.max_inflight, st.kept);
        }
        if st.discard {
            return Ok(EncodeStats { frames: frames.len(), bytes: 0, video_codec: "h264 (VideoToolbox, discarded)".into(), audio_codec: String::new() });
        }
        let n = frames.len();
        let mut read = frames.reader()?;
        let codec = if self.hevc10 { muxide::api::VideoCodec::H265 } else { muxide::api::VideoCodec::H264 };
        let audio_codec = match audio {
            Some(a) => {
                let m4a = self.out.with_extension("aac.m4a");
                write_aac(&m4a, a)?;
                raea_io_codec::remux::annexb_mux_from(codec, n, &mut read, self.opts.width, self.opts.height, self.opts.fps, Some(&m4a), &self.out)?;
                let _ = std::fs::remove_file(&m4a);
                "aac (AudioToolbox)".to_string()
            }
            None => {
                raea_io_codec::remux::annexb_mux_from(codec, n, &mut read, self.opts.width, self.opts.height, self.opts.fps, None, &self.out)?;
                String::new()
            }
        };
        let video_codec = if self.hevc10 { "hevc main10 (VideoToolbox)" } else { "h264 (VideoToolbox)" };
        Ok(EncodeStats { frames: n, bytes: std::fs::metadata(&self.out).map(|m| m.len()).unwrap_or(0), video_codec: video_codec.into(), audio_codec })
    }
}

/// A VtEncoder submit's time (RAEA_ENC_TRACE): waiting for room and a buffer, the copy into it, encode_frame
pub(crate) static VT_PUSH_NS: [std::sync::atomic::AtomicU64; 3] = [const { std::sync::atomic::AtomicU64::new(0) }; 3];

/// A buffer from `pool` while fewer than `cap` are out; waits for one to come back otherwise (10 s at most).
pub(crate) unsafe fn threshold_buffer(pool: &CVPixelBufferPool, cap: i32) -> R<objc2_core_foundation::CFRetained<CVPixelBuffer>> {
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
