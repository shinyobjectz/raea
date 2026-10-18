//! Seeking on VideoToolbox driven directly: the frame at any time, decoded from the keyframe before it with nothing
//! wasted. AVAssetReader, restarted at a time, decodes and outputs every frame from that keyframe on, as many as a GOP
//! holds (50 4K frames at 25 fps with keyframes every 2 s). Here the track's samples are indexed once (AVSampleCursor:
//! times, sync flags, where each lies in the file; nothing decoded); a seek reads the bytes it needs, skips the frames no
//! other frame references (H.264's nal_ref_idc 0: most B-frames), decodes the rest without output, and outputs only the
//! target, into the same IOSurface-backed, Metal-compatible buffers (NV12 or BGRA) the reader gives. A seek forward inside what's
//! decoded carries on from there.
//!
//! Only for a track shown as stored (no rotation in its transform): the reader's video composition applies one.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2_av_foundation::{AVAssetTrack, AVURLAsset};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMSampleTimingInfo, CMTime};
use objc2_core_video::*;
use objc2_video_toolbox::*;

use crate::R;

struct Sample {
    /// presentation time (the edit list's shift applied), seconds
    pts: f64,
    /// the sample's own times, as stored, for its sample buffer
    raw_pts: CMTime,
    raw_dts: CMTime,
    offset: u64,
    len: usize,
    sync: bool,
}

struct Shared {
    frame: Mutex<Option<(CFRetained<CVPixelBuffer>, f64)>>,
    err: Mutex<Option<String>>,
}

unsafe extern "C-unwind" fn output(refcon: *mut std::ffi::c_void, _frame: *mut std::ffi::c_void, status: i32, _flags: VTDecodeInfoFlags, image: *mut CVImageBuffer, pts: CMTime, _dur: CMTime) {
    let sh = unsafe { &*(refcon as *const Shared) };
    if status != 0 {
        *sh.err.lock().unwrap() = Some(format!("VideoToolbox couldn't decode a frame (status {status})"));
        return;
    }
    // a frame decoded without output comes back empty
    if let Some(img) = std::ptr::NonNull::new(image) {
        let pb = unsafe { CFRetained::retain(img.cast::<CVPixelBuffer>()) };
        let t = if pts.timescale == 0 { 0.0 } else { pts.value as f64 / pts.timescale as f64 };
        *sh.frame.lock().unwrap() = Some((pb, t));
    }
}

pub struct Seeker {
    session: CFRetained<VTDecompressionSession>,
    fmt: CFRetained<CMFormatDescription>,
    file: File,
    /// decode order
    samples: Vec<Sample>,
    /// (presentation time, decode index), by presentation time
    order: Vec<(f64, usize)>,
    h264: bool,
    /// the last sample decoded (every frame it may reference decoded too): a seek past it carries on
    pos: Option<usize>,
    shared: Box<Shared>,
    pub width: u32,
    pub height: u32,
    /// frames decoded by the last seek (for measuring)
    pub decoded: usize,
}

impl Seeker {
    /// For the video track of `asset` (stored in `path`), or None where it can't seek directly (rotated, not H.264 or
    /// HEVC, samples in another file).
    /// `bgra`: VideoToolbox's BGRA (its colour conversion, as the reader's RGB frames) instead of NV12.
    pub fn new(asset: &AVURLAsset, track: &AVAssetTrack, path: &std::path::Path, bgra: bool) -> R<Option<Seeker>> {
        unsafe {
            let t = track.preferredTransform();
            if (t.a - 1.0).abs() > 1e-6 || t.b.abs() > 1e-6 || t.c.abs() > 1e-6 || (t.d - 1.0).abs() > 1e-6 {
                return Ok(None);
            }
            let descs = track.formatDescriptions();
            let Some(d) = descs.firstObject() else { return Ok(None) };
            let fd = &*(Retained::as_ptr(&d) as *const CMFormatDescription);
            let codec = fd.media_sub_type().to_be_bytes();
            let h264 = &codec == b"avc1" || &codec == b"avc3";
            if !(h264 || &codec == b"hvc1" || &codec == b"hev1") {
                return Ok(None);
            }
            let fmt = CFRetained::retain(std::ptr::NonNull::from(fd));
            let _ = asset;
            // the edit list's shift, as the reader's times have it (decoder.read_pts)
            let mut shift = 0.0;
            for seg in track.segments().iter() {
                if !seg.isEmpty() {
                    let m = seg.timeMapping();
                    shift = m.target.start.seconds() - m.source.start.seconds();
                    break;
                }
            }
            let t0 = std::time::Instant::now();
            let Some(cur) = track.makeSampleCursorAtFirstSampleInDecodeOrder() else { return Ok(None) };
            let mut samples = Vec::new();
            loop {
                let range = cur.currentSampleStorageRange();
                let pts = cur.presentationTimeStamp();
                let dts = cur.decodeTimeStamp();
                let sync = cur.currentSampleSyncInfo().sampleIsFullSync.as_bool();
                if range.length <= 0 {
                    return Ok(None);
                }
                samples.push(Sample { pts: pts.seconds() + shift, raw_pts: pts, raw_dts: dts, offset: range.offset as u64, len: range.length as usize, sync });
                if cur.stepInDecodeOrderByCount(1) != 1 {
                    break;
                }
            }
            if samples.is_empty() || !samples[0].sync {
                return Ok(None);
            }
            let t_index = t0.elapsed().as_secs_f64() * 1e3;
            let mut order: Vec<(f64, usize)> = samples.iter().enumerate().map(|(i, s)| (s.pts, i)).collect();
            order.sort_by(|a, b| a.0.total_cmp(&b.0));
            let dims = objc2_core_media::CMVideoFormatDescriptionGetDimensions(fd);
            let shared = Box::new(Shared { frame: Mutex::new(None), err: Mutex::new(None) });
            let record = VTDecompressionOutputCallbackRecord { decompressionOutputCallback: Some(output), decompressionOutputRefCon: &*shared as *const Shared as *mut std::ffi::c_void };
            fn k(x: &CFString) -> &CFType {
                x
            }
            let pix = CFNumber::new_i32(if bgra { kCVPixelFormatType_32BGRA } else { kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange } as i32);
            let io = CFDictionary::<CFType, CFType>::from_slices(&[], &[]);
            let attrs = CFDictionary::<CFType, CFType>::from_slices(
                &[k(kCVPixelBufferPixelFormatTypeKey), k(kCVPixelBufferMetalCompatibilityKey), k(kCVPixelBufferIOSurfacePropertiesKey)],
                &[&*pix as &CFType, CFBoolean::new(true) as &CFType, &*io as &CFType],
            );
            let mut sp: *mut VTDecompressionSession = std::ptr::null_mut();
            let r = VTDecompressionSession::create(None, &fmt, None, Some(attrs.as_opaque()), &record, std::ptr::NonNull::new(&mut sp).unwrap());
            if r != 0 || sp.is_null() {
                return Err(format!("VTDecompressionSessionCreate: {r}"));
            }
            let session = CFRetained::from_raw(std::ptr::NonNull::new(sp).unwrap());
            let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            if std::env::var_os("RAEA_VT_TRACE").is_some() {
                eprintln!("vtseek: {} samples indexed in {t_index:.1} ms, session in {:.1} ms", samples.len(), t0.elapsed().as_secs_f64() * 1e3 - t_index);
            }
            Ok(Some(Seeker { session, fmt, file, samples, order, h264, pos: None, shared, width: dims.width as u32, height: dims.height as u32, decoded: 0 }))
        }
    }

    /// A sample's picture as far as a seek cares, from its first coded slice (H.264; HEVC samples are all `Ref`).
    fn kind(&self, data: &[u8]) -> Kind {
        if !self.h264 {
            return Kind::Ref;
        }
        let mut at = 0;
        while at + 5 <= data.len() {
            let n = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize;
            let h = data[at + 4];
            let t = h & 0x1f;
            // a coded slice decides it; parameter sets, SEI and delimiters before it don't
            if (1..=5).contains(&t) {
                if h & 0x60 == 0 {
                    return Kind::Disposable;
                }
                // slice_type from the slice header (first_mb_in_slice ue(v), slice_type ue(v)), emulation-prevention
                // bytes taken out of the few bytes read
                let raw = &data[at + 5..(at + 4 + n).min(at + 5 + 16).min(data.len())];
                let mut rbsp = Vec::with_capacity(raw.len());
                for (k, b) in raw.iter().enumerate() {
                    if *b == 3 && k >= 2 && raw[k - 1] == 0 && raw[k - 2] == 0 {
                        continue;
                    }
                    rbsp.push(*b);
                }
                let mut bits = Bits { d: &rbsp, at: 0 };
                let _first_mb = bits.ue();
                return match bits.ue().map(|t| t % 5) {
                    Some(1) => Kind::RefB,
                    _ => Kind::Ref,
                };
            }
            at += 4 + n;
        }
        Kind::Ref
    }

    fn decode(&mut self, i: usize, data: &[u8], out: bool) -> R<()> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let s = &self.samples[i];
            let mut bb: *mut CMBlockBuffer = std::ptr::null_mut();
            let r = CMBlockBuffer::create_with_memory_block(None, std::ptr::null_mut(), data.len(), None, std::ptr::null(), 0, data.len(), 0, std::ptr::NonNull::new(&mut bb).unwrap());
            if r != 0 || bb.is_null() {
                return Err(format!("CMBlockBufferCreateWithMemoryBlock: {r}"));
            }
            let bb = CFRetained::from_raw(std::ptr::NonNull::new(bb).unwrap());
            let r = CMBlockBuffer::replace_data_bytes(std::ptr::NonNull::new(data.as_ptr() as *mut std::ffi::c_void).unwrap(), &bb, 0, data.len());
            if r != 0 {
                return Err(format!("CMBlockBufferReplaceDataBytes: {r}"));
            }
            let timing = CMSampleTimingInfo { duration: CMTime::with_seconds(0.0, 600), presentationTimeStamp: s.raw_pts, decodeTimeStamp: s.raw_dts };
            let size = data.len();
            let mut sb: *mut CMSampleBuffer = std::ptr::null_mut();
            let r = CMSampleBuffer::create_ready(None, Some(&bb), Some(&self.fmt), 1, 1, &timing, 1, &size, std::ptr::NonNull::new(&mut sb).unwrap());
            if r != 0 || sb.is_null() {
                return Err(format!("CMSampleBufferCreateReady: {r}"));
            }
            let sb = CFRetained::from_raw(std::ptr::NonNull::new(sb).unwrap());
            // asynchronous, so the decoder pipelines frames (waited for after the target); a frame before the target
            // decoded without output
            let mut flags = VTDecodeFrameFlags::Frame_EnableAsynchronousDecompression;
            if !out {
                flags |= VTDecodeFrameFlags::Frame_DoNotOutputFrame;
            }
            if std::env::var("RAEA_VT_SYNC").is_ok() {
                flags = if out { VTDecodeFrameFlags(0) } else { VTDecodeFrameFlags::Frame_DoNotOutputFrame };
            }
            let mut info = VTDecodeInfoFlags(0);
            let r = self.session.decode_frame(&sb, flags, std::ptr::null_mut(), &mut info);
            if r != 0 {
                return Err(format!("VTDecompressionSessionDecodeFrame: {r}"));
            }
            Ok(())
        })
    }

    /// The frame showing at `t` (the latest whose presentation time is at or before it), and its presentation time.
    pub fn frame_at(&mut self, t: f64) -> R<(CFRetained<CVPixelBuffer>, f64)> {
        let eps = 1e-4;
        let p = self.order.partition_point(|o| o.0 <= t + eps).max(1) - 1;
        let target = self.order[p].1;
        // the keyframe it decodes from: the last before it in decode order, or the one before that for a frame shown
        // before its keyframe (an open GOP's leading pictures reference the GOP before)
        let mut key = (0..=target).rev().find(|&i| self.samples[i].sync).unwrap_or(0);
        if self.samples[target].pts < self.samples[key].pts - eps {
            key = (0..key).rev().find(|&i| self.samples[i].sync).unwrap_or(0);
        }
        // carry on from what's decoded when that's ahead of the keyframe and before the target
        let start = match self.pos {
            Some(p) if p >= key && p < target => p + 1,
            _ => key,
        };
        *self.shared.frame.lock().unwrap() = None;
        self.decoded = 0;
        // what each sample from the start to the target is (read once per sample: its first slice's header)
        let mut kinds = Vec::with_capacity(target + 1 - start);
        let mut buf = Vec::new();
        for i in start..=target {
            let s = &self.samples[i];
            let head = s.len.min(64);
            buf.resize(head, 0);
            self.file.read_exact_at(&mut buf, s.offset).map_err(|e| format!("reading a sample: {e}"))?;
            kinds.push(self.kind(&buf));
        }
        if std::env::var_os("RAEA_VT_TRACE").is_some() {
            let k: String = kinds.iter().map(|k| match k { Kind::Disposable => 'd', Kind::RefB => 'b', Kind::Ref => 'R' }).collect();
            eprintln!("vtseek: samples {start}..={target} kinds {k}");
        }
        // a seek that starts at an open GOP's keyframe (HEVC's CRA) skips its leading pictures (shown before it): they
        // reference the GOP before, which isn't decoded (VideoToolbox fails them, -17694 "reference missing"), and no
        // frame shown after the keyframe references them (a target that is one starts from the keyframe before)
        let lead = (start == key).then(|| self.samples[key].pts - eps);
        for i in start..=target {
            let out = i == target;
            let k = kinds[i - start];
            if !out && (k == Kind::Disposable || lead.is_some_and(|l| self.samples[i].pts < l)) {
                continue;
            }
            let s = &self.samples[i];
            buf.resize(s.len, 0);
            self.file.read_exact_at(&mut buf, s.offset).map_err(|e| format!("reading a sample: {e}"))?;
            let (td, kb) = (std::time::Instant::now(), buf.len() / 1024);
            self.decode(i, &buf, out)?;
            self.decoded += 1;
            if std::env::var_os("RAEA_VT_TRACE").is_some() {
                eprintln!("vtseek:   sample {i} ({k:?}, {kb} KB) submitted in {:.1} ms", td.elapsed().as_secs_f64() * 1e3);
            }
        }
        let tw = std::time::Instant::now();
        unsafe { self.session.wait_for_asynchronous_frames() };
        if std::env::var_os("RAEA_VT_TRACE").is_some() {
            eprintln!("vtseek:   waited {:.1} ms for the decoder", tw.elapsed().as_secs_f64() * 1e3);
        }
        if let Some(e) = self.shared.err.lock().unwrap().take() {
            self.pos = None;
            return Err(e);
        }
        self.pos = Some(target);
        let f = self.shared.frame.lock().unwrap().take().ok_or("VideoToolbox gave no frame")?;
        Ok((f.0, self.samples[target].pts))
    }
}

impl Drop for Seeker {
    fn drop(&mut self) {
        unsafe { self.session.invalidate() };
    }
}

/// What a sample is to a seek: nothing refers to it (skipped), a reference B-frame, or anything else. Reference
/// B-frames are decoded: skipping those of mini-GOPs before the target's broke frames in an x264 B-pyramid stream
/// (later P-frames refer to them; worst 244 levels off in examples/vtseek_check), so only the trace tells them apart.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Disposable,
    RefB,
    Ref,
}

/// Exp-Golomb reading over a slice header's first bytes.
struct Bits<'a> {
    d: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        let b = *self.d.get(self.at / 8)?;
        let v = (b >> (7 - self.at % 8)) & 1;
        self.at += 1;
        Some(v as u32)
    }

    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let mut v = 0u32;
        for _ in 0..zeros {
            v = (v << 1) | self.bit()?;
        }
        Some((1u32 << zeros) - 1 + v)
    }
}
