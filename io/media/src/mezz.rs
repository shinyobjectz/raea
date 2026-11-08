//! Optimized copies (lightspeed F4; the index is raea_io_codec::mezz), made by the platform: ProRes 422 through
//! VideoToolbox on Apple, all-intra H.264 through the soft backend elsewhere, decoding the source with the first
//! backend that reads it (the soft one reads H.264 without B-frames, the ffmpeg CLI the rest). A build that can't make
//! one (off Apple without the soft backend's H.264) wants none.

use std::path::{Path, PathBuf};

use raea_io_codec::R;
use raea_io_codec::mezz::{self as index, Probe};

pub use raea_io_codec::mezz::{RangeCopy, dir, fingerprint, has, resolve};

/// The source's video track from the platform's reader (AVFoundation on Apple, the container's tables elsewhere), which
/// says whether it's worth a copy (Probe::wants, Probe::long_gop); None in a build that makes no copies.
pub fn probe(src: &Path) -> Option<Probe> {
    #[cfg(all(feature = "apple", target_vendor = "apple"))]
    return crate::apple::mezz::probe(src);
    #[cfg(all(feature = "h264", feature = "soft-audio", not(target_vendor = "apple")))]
    return crate::soft::probe(src).ok();
    #[allow(unreachable_code)]
    {
        let _ = src;
        None
    }
}

/// The source's complete range copies, in order.
pub fn ranges(src: &Path) -> Vec<RangeCopy> {
    index::ranges(src, || probe(src).map(|p| p.frame_secs()))
}

/// Makes `src`'s whole copy (Apple's carries its audio as LPCM), into a `.partial` file renamed when whole. Blocks until
/// done. Returns the copy's path.
pub fn make(src: &Path) -> R<PathBuf> {
    let fp = fingerprint(src).ok_or_else(|| format!("{}: can't read it", src.display()))?;
    let out = index::copy_of(&fp);
    if out.is_file() {
        return Ok(out);
    }
    let p = probe(src).ok_or_else(|| format!("{}: no video track to copy", src.display()))?;
    let job = index::begin(src, &fp, p.likely_bytes())?;
    encode(src, &job.part, None, &p)?;
    job.finish()
}

/// Makes a copy (video only) of the source's frames covering `start..end` seconds, unless copies already cover them;
/// under the same disk rule as `make`. Returns the copy's path (an existing one's when covered).
pub fn make_range(src: &Path, start: f64, end: f64) -> R<PathBuf> {
    let fp = fingerprint(src).ok_or_else(|| format!("{}: can't read it", src.display()))?;
    let p = probe(src).ok_or_else(|| format!("{}: no video track to copy", src.display()))?;
    let (first, last) = p.frames(start, end);
    if let Some(r) = ranges(src).into_iter().find(|r| r.first <= first && r.last >= last) {
        return Ok(r.path);
    }
    let secs = (last + 1 - first) as f64 * p.frame_secs();
    let need = (p.likely_bytes() as f64 * secs / p.secs.max(secs)) as u64;
    let job = index::begin(src, &format!("{fp}@{first}-{last}"), need)?;
    encode(src, &job.part, Some((first, last)), &p)?;
    job.finish()
}

#[cfg(all(feature = "apple", target_vendor = "apple"))]
fn encode(src: &Path, part: &Path, range: Option<(usize, usize)>, p: &Probe) -> R<()> {
    crate::apple::mezz::export(src, part, range, p)
}

/// The quantiser of an all-intra copy: near-transparent (the copy stands in for the source's pixels)
#[cfg(all(feature = "h264", not(target_vendor = "apple")))]
const INTRA_QP: u8 = 16;

/// The source's frames (all of them, or `first..=last` counting the probe's minimum frame duration) at its rate into
/// all-intra H.264 at `part`: the copy's frame k is the source's frame at the range's start + k / fps.
#[cfg(all(feature = "h264", not(target_vendor = "apple")))]
fn encode(src: &Path, part: &Path, range: Option<(usize, usize)>, p: &Probe) -> R<()> {
    // a frame too detailed for openh264's buffer at INTRA_QP (Native:3: noise, some 1.6 MB a 1080p frame) is made
    // again at QP 26, which every copy had until openh264 took the quantiser asked for (#28)
    encode_at(src, part, range, p, INTRA_QP).or_else(|e| if e.contains("Native:3") { encode_at(src, part, range, p, 26) } else { Err(e) })
}

#[cfg(all(feature = "h264", not(target_vendor = "apple")))]
fn encode_at(src: &Path, part: &Path, range: Option<(usize, usize)>, p: &Probe, qp: u8) -> R<()> {
    let (t0, t1) = match range {
        Some((a, b)) => (a as f64 * p.frame_secs(), (b + 1) as f64 * p.frame_secs()),
        None => (0.0, p.secs),
    };
    let mut why = Vec::new();
    let mut dec = None;
    for b in crate::preferred() {
        match b.open_video(src) {
            Ok(d) => {
                dec = Some(d);
                break;
            }
            Err(e) => why.push(format!("{}: {e}", b.name())),
        }
    }
    let mut dec = dec.ok_or_else(|| format!("{}: no backend reads it ({})", src.display(), why.join("; ")))?;
    let info = dec.info().clone();
    if info.width % 2 == 1 || info.height % 2 == 1 {
        return Err(format!("{}: {}x{} has an odd side (H.264's 4:2:0 wants even ones)", src.display(), info.width, info.height));
    }
    let fps = if info.fps > 0.0 { info.fps } else { p.fps };
    let opts = crate::EncodeOpts { width: info.width, height: info.height, fps, kbps: 0, quality: None, nv12: false, realtime: false };
    let mut enc = crate::soft::h264_encoder(part, &opts, Some(qp))?;
    let n = ((t1 - t0) * fps).round().max(1.0) as usize;
    for k in 0..n {
        enc.push(&dec.frame_at(t0 + k as f64 / fps)?.rgba)?;
    }
    enc.finish(None).map(|_| ()).inspect_err(|_| {
        let _ = std::fs::remove_file(part);
    })
}

#[cfg(not(any(all(feature = "apple", target_vendor = "apple"), all(feature = "h264", not(target_vendor = "apple")))))]
fn encode(src: &Path, _part: &Path, _range: Option<(usize, usize)>, _p: &Probe) -> R<()> {
    Err(format!("{}: this build makes no optimized copies (raea-io-media without apple or h264)", src.display()))
}
