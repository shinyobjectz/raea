//! Packet-level video on VideoToolbox, for coders that are handed compressed packets one at a time (@raea/motion's
//! mediabunny decoders and encoders): a decompression session made from the track's decoder configuration (avcC or
//! hvcC) that takes AVCC packets and gives NV12 frames in presentation order, and a compression session that takes
//! NV12 frames and gives AVCC packets with the avcC a muxer needs. The frames stay in Rust; what holds them decides
//! whether pixels ever move.

use std::sync::Mutex;

use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMSampleTimingInfo, CMTime, kCMTimeInvalid};
use objc2_core_video::*;
use objc2_video_toolbox::*;

use crate::R;

/// A decoded picture: NV12 (luma, then interleaved Cb Cr at half size), tightly packed.
#[derive(Clone, Debug)]
pub struct DecodedFrame {
    pub pts: f64,
    pub dur: f64,
    pub width: u32,
    pub height: u32,
    pub nv12: Vec<u8>,
}

/// An encoded packet: AVCC (four-byte lengths), whether it's a keyframe, its times.
#[derive(Clone, Debug)]
pub struct Packet {
    pub data: Vec<u8>,
    pub key: bool,
    pub pts: f64,
    pub dur: f64,
}

fn secs(t: CMTime) -> f64 {
    if t.timescale == 0 { 0.0 } else { t.value as f64 / t.timescale as f64 }
}

fn cmtime(s: f64) -> CMTime {
    unsafe { CMTime::with_seconds(s, 90000) }
}

fn be16(b: &[u8], at: usize) -> R<usize> {
    b.get(at..at + 2).map(|x| u16::from_be_bytes([x[0], x[1]]) as usize).ok_or_else(|| "a short decoder configuration".to_string())
}

/// The parameter-set NAL units and the length size of an avcC or hvcC record.
fn parameter_sets(codec: &str, d: &[u8]) -> R<(Vec<Vec<u8>>, i32)> {
    let mut sets = Vec::new();
    if codec.starts_with("avc") {
        if d.len() < 7 {
            return Err("avcC too short".into());
        }
        let len = (d[4] & 3) as i32 + 1;
        let mut i = 5;
        let nsps = (d[i] & 0x1f) as usize;
        i += 1;
        for _ in 0..nsps {
            let n = be16(d, i)?;
            sets.push(d.get(i + 2..i + 2 + n).ok_or("avcC: a short SPS")?.to_vec());
            i += 2 + n;
        }
        let npps = *d.get(i).ok_or("avcC: no PPS count")? as usize;
        i += 1;
        for _ in 0..npps {
            let n = be16(d, i)?;
            sets.push(d.get(i + 2..i + 2 + n).ok_or("avcC: a short PPS")?.to_vec());
            i += 2 + n;
        }
        Ok((sets, len))
    } else if codec.starts_with("hev") || codec.starts_with("hvc") {
        if d.len() < 23 {
            return Err("hvcC too short".into());
        }
        let len = (d[21] & 3) as i32 + 1;
        let arrays = d[22] as usize;
        let mut i = 23;
        for _ in 0..arrays {
            i += 1; // completeness and NAL unit type
            let n = be16(d, i)?;
            i += 2;
            for _ in 0..n {
                let l = be16(d, i)?;
                sets.push(d.get(i + 2..i + 2 + l).ok_or("hvcC: a short NAL unit")?.to_vec());
                i += 2 + l;
            }
        }
        Ok((sets, len))
    } else {
        Err(format!("{codec}: only H.264 (avc) and HEVC (hev/hvc) decode here"))
    }
}

struct DecShared {
    out: Mutex<Vec<DecodedFrame>>,
    err: Mutex<Option<String>>,
}

unsafe extern "C-unwind" fn dec_output(
    refcon: *mut std::ffi::c_void,
    _frame: *mut std::ffi::c_void,
    status: i32,
    _flags: VTDecodeInfoFlags,
    image: *mut CVImageBuffer,
    pts: CMTime,
    dur: CMTime,
) {
    objc2::rc::autoreleasepool(|_| {
        let sh = unsafe { &*(refcon as *const DecShared) };
        if status != 0 || image.is_null() {
            *sh.err.lock().unwrap() = Some(format!("VideoToolbox couldn't decode a packet (status {status})"));
            return;
        }
        let pb: &CVPixelBuffer = unsafe { &*image };
        unsafe {
            CVPixelBufferLockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
            let (w, h) = (CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb));
            let mut nv12 = vec![0u8; w * h + w * (h / 2)];
            for (plane, rows, off) in [(0usize, h, 0usize), (1, h / 2, w * h)] {
                let base = CVPixelBufferGetBaseAddressOfPlane(pb, plane) as *const u8;
                let bpr = CVPixelBufferGetBytesPerRowOfPlane(pb, plane);
                for r in 0..rows {
                    std::ptr::copy_nonoverlapping(base.add(r * bpr), nv12.as_mut_ptr().add(off + r * w), w);
                }
            }
            CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
            sh.out.lock().unwrap().push(DecodedFrame { pts: secs(pts), dur: secs(dur), width: w as u32, height: h as u32, nv12 });
        }
    });
}

/// Compressed packets in, NV12 frames out (presentation order).
pub struct PacketDecoder {
    session: CFRetained<VTDecompressionSession>,
    fmt: CFRetained<CMFormatDescription>,
    shared: Box<DecShared>,
    pending: Vec<DecodedFrame>,
}

/// How many frames the decoder holds back to put B-frames in order.
const REORDER: usize = 4;

impl PacketDecoder {
    /// From the track's codec string and decoder configuration (avcC or hvcC bytes).
    pub fn new(codec: &str, description: &[u8]) -> R<PacketDecoder> {
        let (sets, len) = parameter_sets(codec, description)?;
        if sets.len() < 2 {
            return Err(format!("{codec}: the decoder configuration has {} parameter sets", sets.len()));
        }
        let ptrs: Vec<std::ptr::NonNull<u8>> = sets.iter().map(|s| std::ptr::NonNull::new(s.as_ptr() as *mut u8).unwrap()).collect();
        let sizes: Vec<usize> = sets.iter().map(Vec::len).collect();
        let mut fd: *const CMFormatDescription = std::ptr::null();
        let r = unsafe {
            if codec.starts_with("avc") {
                objc2_core_media::CMVideoFormatDescriptionCreateFromH264ParameterSets(None, sets.len(), std::ptr::NonNull::new(ptrs.as_ptr() as *mut _).unwrap(), std::ptr::NonNull::new(sizes.as_ptr() as *mut _).unwrap(), len, std::ptr::NonNull::new(&mut fd).unwrap())
            } else {
                objc2_core_media::CMVideoFormatDescriptionCreateFromHEVCParameterSets(None, sets.len(), std::ptr::NonNull::new(ptrs.as_ptr() as *mut _).unwrap(), std::ptr::NonNull::new(sizes.as_ptr() as *mut _).unwrap(), len, None, std::ptr::NonNull::new(&mut fd).unwrap())
            }
        };
        if r != 0 || fd.is_null() {
            return Err(format!("{codec}: CMVideoFormatDescriptionCreateFrom…ParameterSets: {r}"));
        }
        let fmt = unsafe { CFRetained::from_raw(std::ptr::NonNull::new(fd as *mut CMFormatDescription).unwrap()) };
        let shared = Box::new(DecShared { out: Mutex::new(Vec::new()), err: Mutex::new(None) });
        let record = VTDecompressionOutputCallbackRecord { decompressionOutputCallback: Some(dec_output), decompressionOutputRefCon: &*shared as *const DecShared as *mut std::ffi::c_void };
        let fmtn = CFNumber::new_i32(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange as i32);
        fn k(x: &CFString) -> &CFType {
            x
        }
        let attrs = CFDictionary::<CFType, CFType>::from_slices(&[unsafe { k(kCVPixelBufferPixelFormatTypeKey) }], &[&*fmtn as &CFType]);
        let mut sp: *mut VTDecompressionSession = std::ptr::null_mut();
        let r = unsafe { VTDecompressionSession::create(None, &fmt, None, Some(attrs.as_opaque()), &record, std::ptr::NonNull::new(&mut sp).unwrap()) };
        if r != 0 || sp.is_null() {
            return Err(format!("VTDecompressionSessionCreate: {r}"));
        }
        let session = unsafe { CFRetained::from_raw(std::ptr::NonNull::new(sp).unwrap()) };
        Ok(PacketDecoder { session, fmt, shared, pending: Vec::new() })
    }

    fn collect(&mut self) -> R<()> {
        if let Some(e) = self.shared.err.lock().unwrap().take() {
            return Err(e);
        }
        let mut got = std::mem::take(&mut *self.shared.out.lock().unwrap());
        self.pending.append(&mut got);
        self.pending.sort_by(|a, b| a.pts.total_cmp(&b.pts));
        Ok(())
    }

    /// One AVCC packet; the frames now ready, in presentation order.
    pub fn decode(&mut self, data: &[u8], pts: f64, dur: f64) -> R<Vec<DecodedFrame>> {
        objc2::rc::autoreleasepool(|_| -> R<()> {
            unsafe {
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
                let timing = CMSampleTimingInfo { duration: cmtime(dur), presentationTimeStamp: cmtime(pts), decodeTimeStamp: kCMTimeInvalid };
                let size = data.len();
                let mut sb: *mut CMSampleBuffer = std::ptr::null_mut();
                let r = CMSampleBuffer::create_ready(None, Some(&bb), Some(&self.fmt), 1, 1, &timing, 1, &size, std::ptr::NonNull::new(&mut sb).unwrap());
                if r != 0 || sb.is_null() {
                    return Err(format!("CMSampleBufferCreateReady: {r}"));
                }
                let sb = CFRetained::from_raw(std::ptr::NonNull::new(sb).unwrap());
                let mut info = VTDecodeInfoFlags(0);
                let r = self.session.decode_frame(&sb, VTDecodeFrameFlags(0), std::ptr::null_mut(), &mut info);
                if r != 0 {
                    return Err(format!("VTDecompressionSessionDecodeFrame: {r}"));
                }
            }
            Ok(())
        })?;
        self.collect()?;
        let n = self.pending.len().saturating_sub(REORDER);
        Ok(self.pending.drain(..n).collect())
    }

    /// The frames still held, in presentation order.
    pub fn flush(&mut self) -> R<Vec<DecodedFrame>> {
        unsafe {
            self.session.finish_delayed_frames();
            self.session.wait_for_asynchronous_frames();
        }
        self.collect()?;
        Ok(std::mem::take(&mut self.pending))
    }
}

impl Drop for PacketDecoder {
    fn drop(&mut self) {
        unsafe { self.session.invalidate() };
    }
}

struct EncShared {
    st: Mutex<EncState>,
    cv: std::sync::Condvar,
}

#[derive(Default)]
struct EncState {
    /// by submission index: (AVCC, keyframe)
    done: std::collections::BTreeMap<usize, (Vec<u8>, bool)>,
    inflight: usize,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    err: Option<String>,
}

unsafe extern "C-unwind" fn enc_output(refcon: *mut std::ffi::c_void, frame: *mut std::ffi::c_void, status: i32, _flags: VTEncodeInfoFlags, sb: *mut CMSampleBuffer) {
    objc2::rc::autoreleasepool(|_| {
        let sh = unsafe { &*(refcon as *const EncShared) };
        let idx = frame as usize;
        let mut st = sh.st.lock().unwrap();
        let got: R<(Vec<u8>, bool)> = (|| {
            if status != 0 || sb.is_null() {
                return Err(format!("VideoToolbox: frame {idx}: status {status}"));
            }
            let sb = unsafe { &*sb };
            if st.sps.is_none() {
                if let Some(fd) = unsafe { sb.format_description() } {
                    let s = &mut *st;
                    for (i, slot) in [(0usize, &mut s.sps), (1, &mut s.pps)] {
                        let (mut ptr, mut len, mut count) = (std::ptr::null::<u8>(), 0usize, 0usize);
                        let r = unsafe { objc2_core_media::CMVideoFormatDescriptionGetH264ParameterSetAtIndex(&fd, i, &mut ptr, &mut len, &mut count, std::ptr::null_mut()) };
                        if r == 0 && !ptr.is_null() {
                            *slot = Some(unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec());
                        }
                    }
                }
            }
            let bb = unsafe { sb.data_buffer() }.ok_or("no data buffer")?;
            let n = unsafe { bb.data_length() };
            let mut v = vec![0u8; n];
            if n > 0 && unsafe { bb.copy_data_bytes(0, n, std::ptr::NonNull::new(v.as_mut_ptr() as *mut std::ffi::c_void).unwrap()) } != 0 {
                return Err("CMBlockBufferCopyDataBytes failed".into());
            }
            let mut key = false;
            let mut i = 0;
            while i + 4 < v.len() {
                let len = u32::from_be_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as usize;
                if v[i + 4] & 0x1f == 5 {
                    key = true;
                }
                i += 4 + len;
            }
            Ok((v, key))
        })();
        match got {
            Ok(p) => {
                st.done.insert(idx, p);
            }
            Err(e) => st.err = Some(e),
        }
        st.inflight -= 1;
        sh.cv.notify_all();
    });
}

/// NV12 frames in, H.264 AVCC packets out (no B-frames: decode order is presentation order).
pub struct PacketEncoder {
    session: CFRetained<VTCompressionSession>,
    shared: Box<EncShared>,
    times: Vec<(f64, f64)>,
    next_out: usize,
    width: u32,
    height: u32,
}

impl PacketEncoder {
    pub fn new(width: u32, height: u32, fps: f64, kbps: u32) -> R<PacketEncoder> {
        let shared = Box::new(EncShared { st: Mutex::new(EncState::default()), cv: std::sync::Condvar::new() });
        unsafe {
            let fmt = CFNumber::new_i32(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange as i32);
            let (wn, hn) = (CFNumber::new_i32(width as i32), CFNumber::new_i32(height as i32));
            fn k(x: &CFString) -> &CFType {
                x
            }
            let attrs = CFDictionary::<CFType, CFType>::from_slices(&[k(kCVPixelBufferPixelFormatTypeKey), k(kCVPixelBufferWidthKey), k(kCVPixelBufferHeightKey)], &[&*fmt as &CFType, &*wn as &CFType, &*hn as &CFType]);
            let mut sp: *mut VTCompressionSession = std::ptr::null_mut();
            let r = VTCompressionSession::create(None, width as i32, height as i32, objc2_core_media::kCMVideoCodecType_H264, None, Some(attrs.as_opaque()), None, Some(enc_output), &*shared as *const EncShared as *mut std::ffi::c_void, std::ptr::NonNull::new(&mut sp).unwrap());
            if r != 0 || sp.is_null() {
                return Err(format!("VTCompressionSessionCreate: {r}"));
            }
            let session = CFRetained::from_raw(std::ptr::NonNull::new(sp).unwrap());
            let vs: &VTSession = &*(sp as *const VTSession);
            let set = |key: &CFString, v: &CFType| -> R<()> {
                let r = VTSessionSetProperty(vs, key, Some(v));
                if r != 0 { Err(format!("VTSessionSetProperty {key}: {r}")) } else { Ok(()) }
            };
            set(kVTCompressionPropertyKey_AverageBitRate, &CFNumber::new_i32((kbps * 1000) as i32))?;
            set(kVTCompressionPropertyKey_MaxKeyFrameInterval, &CFNumber::new_i32((fps * 2.0).round() as i32))?;
            set(kVTCompressionPropertyKey_ExpectedFrameRate, &CFNumber::new_f64(fps))?;
            set(kVTCompressionPropertyKey_AllowFrameReordering, CFBoolean::new(false))?;
            set(kVTCompressionPropertyKey_RealTime, CFBoolean::new(false))?;
            set(kVTCompressionPropertyKey_ProfileLevel, kVTProfileLevel_H264_High_AutoLevel)?;
            set(kVTCompressionPropertyKey_ColorPrimaries, kCVImageBufferColorPrimaries_ITU_R_709_2)?;
            set(kVTCompressionPropertyKey_TransferFunction, kCVImageBufferTransferFunction_ITU_R_709_2)?;
            set(kVTCompressionPropertyKey_YCbCrMatrix, kCVImageBufferYCbCrMatrix_ITU_R_709_2)?;
            Ok(PacketEncoder { session, shared, times: Vec::new(), next_out: 0, width, height })
        }
    }

    /// One frame (NV12 at the encoder's size); the packets now ready, in order.
    pub fn encode(&mut self, nv12: &[u8], pts: f64, dur: f64) -> R<Vec<Packet>> {
        let (w, h) = (self.width as usize, self.height as usize);
        if nv12.len() < w * h + w * (h / 2) {
            return Err(format!("a frame of {} bytes for a {w}x{h} encoder", nv12.len()));
        }
        let idx = self.times.len();
        self.times.push((pts, dur));
        objc2::rc::autoreleasepool(|_| -> R<()> {
            unsafe {
                let pool = self.session.pixel_buffer_pool().ok_or("no pixel buffer pool")?;
                let mut pb: *mut CVPixelBuffer = std::ptr::null_mut();
                if CVPixelBufferPool::create_pixel_buffer(None, &pool, std::ptr::NonNull::new(&mut pb).unwrap()) != 0 || pb.is_null() {
                    return Err("CVPixelBufferPoolCreatePixelBuffer failed".into());
                }
                let pb = CFRetained::from_raw(std::ptr::NonNull::new(pb).unwrap());
                CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags(0));
                for (plane, rows, off) in [(0usize, h, 0usize), (1, h / 2, w * h)] {
                    let base = CVPixelBufferGetBaseAddressOfPlane(&pb, plane) as *mut u8;
                    let bpr = CVPixelBufferGetBytesPerRowOfPlane(&pb, plane);
                    for r in 0..rows {
                        std::ptr::copy_nonoverlapping(nv12.as_ptr().add(off + r * w), base.add(r * bpr), w);
                    }
                }
                CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags(0));
                self.shared.st.lock().unwrap().inflight += 1;
                let r = self.session.encode_frame(&pb, cmtime(pts), cmtime(dur), None, idx as *mut std::ffi::c_void, std::ptr::null_mut());
                if r != 0 {
                    self.shared.st.lock().unwrap().inflight -= 1;
                    return Err(format!("VTCompressionSessionEncodeFrame: {r}"));
                }
            }
            Ok(())
        })?;
        self.take()
    }

    fn take(&mut self) -> R<Vec<Packet>> {
        let mut st = self.shared.st.lock().unwrap();
        if let Some(e) = st.err.take() {
            return Err(e);
        }
        let mut out = Vec::new();
        while let Some((data, key)) = st.done.remove(&self.next_out) {
            let (pts, dur) = self.times[self.next_out];
            out.push(Packet { data, key, pts, dur });
            self.next_out += 1;
        }
        Ok(out)
    }

    /// Every frame out: the remaining packets.
    pub fn finish(&mut self) -> R<Vec<Packet>> {
        unsafe { self.session.complete_frames(kCMTimeInvalid) };
        {
            let mut st = self.shared.st.lock().unwrap();
            while st.inflight > 0 && st.err.is_none() {
                st = self.shared.cv.wait(st).unwrap();
            }
        }
        self.take()
    }

    /// The avcC decoder configuration (from the first keyframe's SPS and PPS), once a packet is out.
    pub fn description(&self) -> Option<Vec<u8>> {
        let st = self.shared.st.lock().unwrap();
        let (sps, pps) = (st.sps.as_ref()?, st.pps.as_ref()?);
        let mut d = vec![1, sps[1], sps[2], sps[3], 0xFF, 0xE1];
        d.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        d.extend_from_slice(sps);
        d.push(1);
        d.extend_from_slice(&(pps.len() as u16).to_be_bytes());
        d.extend_from_slice(pps);
        Some(d)
    }
}

impl Drop for PacketEncoder {
    fn drop(&mut self) {
        unsafe { self.session.invalidate() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // a round trip: frames encoded to packets, the packets decoded with the encoder's own avcC, the same pictures back
    #[test]
    fn packets_round_trip() {
        let (w, h) = (320u32, 240u32);
        let mut enc = PacketEncoder::new(w, h, 30.0, 4000).unwrap();
        let mut packets = Vec::new();
        let frame = |i: usize| -> Vec<u8> {
            let mut v = vec![0u8; (w * h + w * (h / 2)) as usize];
            for y in 0..h as usize {
                for x in 0..w as usize {
                    v[y * w as usize + x] = (16 + (x + i * 4) % 200) as u8;
                }
            }
            for b in &mut v[(w * h) as usize..] {
                *b = 128;
            }
            v
        };
        for i in 0..20 {
            packets.extend(enc.encode(&frame(i), i as f64 / 30.0, 1.0 / 30.0).unwrap());
        }
        packets.extend(enc.finish().unwrap());
        assert_eq!(packets.len(), 20);
        assert!(packets[0].key);
        let desc = enc.description().expect("an avcC");
        let mut dec = PacketDecoder::new("avc1", &desc).unwrap();
        let mut frames = Vec::new();
        for p in &packets {
            frames.extend(dec.decode(&p.data, p.pts, p.dur).unwrap());
        }
        frames.extend(dec.flush().unwrap());
        assert_eq!(frames.len(), 20);
        assert!(frames.windows(2).all(|f| f[0].pts < f[1].pts));
        // the luma ramp comes back within a few levels
        let want = frame(10);
        let got = &frames[10].nv12;
        let err: f64 = (0..(w * h) as usize).step_by(7).map(|i| (want[i] as f64 - got[i] as f64).abs()).sum::<f64>() / ((w * h) as f64 / 7.0);
        assert!(err < 4.0, "mean luma error {err}");
    }
}

/// The video codecs this machine encodes in hardware (VideoToolbox's encoder list, by four-character code: avc1, hvc1,
/// apcn, ..., and av01 or vp09 only where a chip has them). A coder offers a codec only when it's here.
pub fn hardware_encoders() -> Vec<String> {
    use objc2_core_foundation::CFArray;
    let mut out: Vec<String> = Vec::new();
    unsafe {
        let mut list: *const CFArray = std::ptr::null();
        if VTCopyVideoEncoderList(None, std::ptr::NonNull::new(&mut list as *mut _).unwrap()) != 0 || list.is_null() {
            return out;
        }
        let list = CFRetained::from_raw(std::ptr::NonNull::new(list as *mut CFArray).unwrap());
        for i in 0..list.count() {
            let item = list.value_at_index(i) as *const CFDictionary;
            if item.is_null() {
                continue;
            }
            let d = &*item;
            let get = |k: &CFString| d.value(k as *const CFString as *const std::ffi::c_void);
            let hw = get(kVTVideoEncoderList_IsHardwareAccelerated);
            if hw.is_null() || !(*(hw as *const CFBoolean)).as_bool() {
                continue;
            }
            let ct = get(kVTVideoEncoderList_CodecType);
            if ct.is_null() {
                continue;
            }
            let t = (*(ct as *const CFNumber)).as_i64().unwrap_or(0) as u32;
            let s: String = t.to_be_bytes().iter().map(|b| *b as char).collect();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out.sort();
    out
}
