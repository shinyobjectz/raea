//! Optimized media (lightspeed F4): a long-GOP source over 1080p gets an intra-only ProRes 422 copy in
//! ~/Library/Caches/raea/mezz, made in the background, and every decoder opened on the source (play, scrub, render)
//! reads the copy once it is complete. A seek in it decodes one frame where the source's 1-2 s GOPs decoded ~25
//! (k4: p99 53 ms, the media engine's ~590 fps at 4K). ProRes 422 scored VMAF 99.98 against k4's H.264 (422 HQ the
//! same, at 1.5x the bytes). Until the copy is complete everything reads the source, so a render and a play at the same
//! moment read the same pixels.
//!
//! The copy is named by the source's fingerprint (its length and first and last MiB), written to a `.partial` file and
//! renamed when whole; the folder is kept under a cap by last use: RAEA_MEZZ_CAP_GB (default 20) or the disk's free space
//! less 20 GB, whichever is smaller, and a copy that wouldn't fit isn't made. RAEA_MEZZ=0 reads sources only.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use objc2_av_foundation::*;
use objc2_foundation::{NSString, NSURL};

use crate::R;

/// The cache folder (RAEA_MEZZ_DIR, else ~/Library/Caches/raea/mezz).
pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os("RAEA_MEZZ_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Library/Caches/raea/mezz")
}

fn enabled() -> bool {
    std::env::var("RAEA_MEZZ").as_deref() != Ok("0")
}

/// The source's fingerprint: FNV-1a over its length and its first and last MiB, remembered per (path, length,
/// modified) so an open reads the file's ends once a process.
pub fn fingerprint(src: &Path) -> Option<String> {
    static SEEN: Mutex<Option<HashMap<(PathBuf, u64, SystemTime), String>>> = Mutex::new(None);
    let meta = std::fs::metadata(src).ok()?;
    let key = (src.to_path_buf(), meta.len(), meta.modified().ok()?);
    if let Some(f) = SEEN.lock().ok()?.as_ref().and_then(|m| m.get(&key)) {
        return Some(f.clone());
    }
    const MIB: u64 = 1 << 20;
    let mut f = File::open(src).ok()?;
    let mut h: u64 = 0xcbf29ce484222325;
    let mut eat = |b: &[u8]| {
        for x in b {
            h ^= *x as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    eat(&meta.len().to_le_bytes());
    let mut buf = vec![0u8; MIB.min(meta.len()) as usize];
    f.read_exact(&mut buf).ok()?;
    eat(&buf);
    if meta.len() > MIB {
        f.seek(SeekFrom::Start(meta.len().saturating_sub(MIB))).ok()?;
        buf.clear();
        f.read_to_end(&mut buf).ok()?;
        eat(&buf);
    }
    let fp = format!("{h:016x}");
    SEEN.lock().ok()?.get_or_insert_with(HashMap::new).insert(key, fp.clone());
    Some(fp)
}

fn copy_of(fp: &str) -> PathBuf {
    dir().join(format!("{fp}.mov"))
}

/// The path a decoder should open for `src`: its complete copy when there is one, else `src` itself.
pub fn resolve(src: &Path) -> PathBuf {
    if !enabled() {
        return src.to_path_buf();
    }
    match fingerprint(src).map(|fp| copy_of(&fp)) {
        Some(c) if c.is_file() => {
            // last use, for the cap's order
            let _ = File::options().append(true).open(&c).and_then(|f| f.set_modified(SystemTime::now()));
            c
        }
        _ => src.to_path_buf(),
    }
}

/// Whether `src` is worth a copy: H.264 or HEVC over 1920x1080 whose keyframes lie more than 8 frames apart.
pub fn wants(src: &Path) -> bool {
    over_1080p(src) && long_gop(src)
}

fn over_1080p(src: &Path) -> bool {
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&src.display().to_string()));
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let Some(media) = AVMediaTypeVideo else { return false };
        #[allow(deprecated)]
        let tracks = asset.tracksWithMediaType(media);
        let Some(track) = tracks.firstObject() else { return false };
        let size = track.naturalSize();
        size.width * size.height > 1920.0 * 1080.0
    }
}

/// Whether `src` is H.264 or HEVC whose keyframes lie more than 8 frames apart, at any size: a seek in it decodes a
/// chain of frames. Such a source under 1080p gets copies of the ranges an edit uses (`make_range`), not the whole file.
pub fn long_gop(src: &Path) -> bool {
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&src.display().to_string()));
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let Some(media) = AVMediaTypeVideo else { return false };
        #[allow(deprecated)]
        let tracks = asset.tracksWithMediaType(media);
        let Some(track) = tracks.firstObject() else { return false };
        let descs = track.formatDescriptions();
        let Some(d) = descs.firstObject() else { return false };
        let fd = &*(objc2::rc::Retained::as_ptr(&d) as *const objc2_core_media::CMFormatDescription);
        let codec = fd.media_sub_type().to_be_bytes();
        if !matches!(&codec, b"avc1" | b"avc3" | b"hvc1" | b"hev1") {
            return false;
        }
        let Some(cur) = track.makeSampleCursorAtFirstSampleInDecodeOrder() else { return false };
        let (mut since, mut longest) = (0usize, 0usize);
        loop {
            if cur.currentSampleSyncInfo().sampleIsFullSync.as_bool() {
                since = 0;
            } else {
                since += 1;
                longest = longest.max(since);
            }
            if longest > 8 || cur.stepInDecodeOrderByCount(1) != 1 {
                break;
            }
        }
        longest > 8
    }
}

/// Whether `src` already has its complete copy.
pub fn has(src: &Path) -> bool {
    fingerprint(src).is_some_and(|fp| copy_of(&fp).is_file())
}

/// A complete copy of `start..end` seconds of a source, where `first..=last` count the track's minimum frame duration
/// (a variable-rate phone clip's runs finer than its frames: 1/92 s at 30 fps): the copy's time t is the source's
/// start + t.
#[derive(Clone, Debug)]
pub struct RangeCopy {
    pub first: usize,
    pub last: usize,
    pub start: f64,
    pub end: f64,
    pub path: PathBuf,
}

/// The source's frame duration (its track's minFrameDuration) as (value, timescale), and in seconds.
fn frame_duration(src: &Path) -> Option<(i64, i32, f64)> {
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&src.display().to_string()));
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let media = AVMediaTypeVideo?;
        #[allow(deprecated)]
        let tracks = asset.tracksWithMediaType(media);
        let track = tracks.firstObject()?;
        let d = track.minFrameDuration();
        (d.value > 0 && d.timescale > 0).then(|| (d.value, d.timescale, d.value as f64 / d.timescale as f64))
    }
}

/// The source's complete range copies, in order. Named `FP@FIRST-LAST.mov` beside the whole-file copies.
pub fn ranges(src: &Path) -> Vec<RangeCopy> {
    if !enabled() {
        return Vec::new();
    }
    let Some(fp) = fingerprint(src) else { return Vec::new() };
    let prefix = format!("{fp}@");
    let Ok(rd) = std::fs::read_dir(dir()) else { return Vec::new() };
    let names: Vec<(PathBuf, usize, usize)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let rest = name.strip_prefix(&prefix)?.strip_suffix(".mov")?;
            if rest.contains("partial") {
                return None;
            }
            let (a, b) = rest.split_once('-')?;
            Some((e.path(), a.parse().ok()?, b.parse().ok()?))
        })
        .collect();
    if names.is_empty() {
        return Vec::new();
    }
    let Some((_, _, fd)) = frame_duration(src) else { return Vec::new() };
    let mut out: Vec<RangeCopy> = names.into_iter().map(|(path, first, last)| RangeCopy { first, last, start: first as f64 * fd, end: (last + 1) as f64 * fd, path }).collect();
    out.sort_by_key(|r| r.first);
    out
}

/// Makes a copy (ProRes 422, video only) of the source's frames covering `start..end` seconds, unless copies already
/// cover them; under the same disk rule as `make`. Returns the copy's path (an existing one's when covered).
pub fn make_range(src: &Path, start: f64, end: f64) -> R<PathBuf> {
    let fp = fingerprint(src).ok_or_else(|| format!("{}: can't read it", src.display()))?;
    let (fv, fts, fd) = frame_duration(src).ok_or_else(|| format!("{}: no frame duration", src.display()))?;
    let first = (start.max(0.0) / fd).floor() as usize;
    let last = ((end / fd).ceil() as usize).max(first + 1) - 1;
    if let Some(r) = ranges(src).into_iter().find(|r| r.first <= first && r.last >= last) {
        return Ok(r.path);
    }
    let out = dir().join(format!("{fp}@{first}-{last}.mov"));
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    sweep();
    let whole = likely_bytes(src);
    let secs = (last + 1 - first) as f64 * fd;
    let total = probe_secs(src).unwrap_or(secs).max(secs);
    let need = (whole as f64 * secs / total) as u64;
    let (cap, free) = (cap_bytes(), free_bytes().unwrap_or(0));
    if free < need + RESERVE || used_bytes() + need > cap {
        return Err(format!("{}: no room for a range copy ({:.2} GB wanted, {:.1} GB free, cap {:.1} GB)", src.display(), need as f64 / 1e9, free as f64 / 1e9, cap as f64 / 1e9));
    }
    let part = dir().join(format!("{fp}@{first}-{last}.{}.partial.mov", std::process::id()));
    let _ = std::fs::remove_file(&part);
    #[cfg(not(target_os = "macos"))]
    return Err("optimized copies are made on macOS only".into());
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2_core_media::{CMTime, CMTimeRange};
        let url = NSURL::fileURLWithPath(&NSString::from_str(&src.display().to_string()));
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let session = AVAssetExportSession::exportSessionWithAsset_presetName(&asset, AVAssetExportPresetAppleProRes422LPCM).ok_or("AVAssetExportSession: ProRes 422 isn't offered for this source")?;
        session.setOutputURL(Some(&NSURL::fileURLWithPath(&NSString::from_str(&part.display().to_string()))));
        session.setOutputFileType(AVFileTypeQuickTimeMovie);
        // whole frames of the source: the copy's frame k is the source's frame first + k
        let range = CMTimeRange { start: CMTime::new(first as i64 * fv, fts), duration: CMTime::new((last + 1 - first) as i64 * fv, fts) };
        session.setTimeRange(range);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let done = block2::RcBlock::new(move || {
            let _ = tx.send(());
        });
        session.exportAsynchronouslyWithCompletionHandler(&done);
        rx.recv().map_err(|_| "the export never finished".to_string())?;
        if session.status() != AVAssetExportSessionStatus::Completed {
            let _ = std::fs::remove_file(&part);
            let why = session.error().map(|e| e.localizedDescription().to_string()).unwrap_or_else(|| format!("status {:?}", session.status()));
            return Err(format!("{}: the ProRes range copy failed: {why}", src.display()));
        }
    }
    std::fs::rename(&part, &out).map_err(|e| e.to_string())?;
    prune();
    Ok(out)
}

/// The source's length in seconds.
fn probe_secs(src: &Path) -> Option<f64> {
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&src.display().to_string()));
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        #[allow(deprecated)]
        let s = asset.duration().seconds();
        (s > 0.0).then_some(s)
    }
}

/// Makes `src`'s copy (ProRes 422 video, its audio as LPCM) through AVAssetExportSession, into a `.partial` file renamed
/// when whole. Blocks until done. Returns the copy's path.
pub fn make(src: &Path) -> R<PathBuf> {
    let fp = fingerprint(src).ok_or_else(|| format!("{}: can't read it", src.display()))?;
    let out = copy_of(&fp);
    if out.is_file() {
        return Ok(out);
    }
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    sweep();
    // room for it: the copy's likely size (ProRes 422 is ~2 bits a pixel; k4's 4K25 ran 410 Mbps), under the cap, and
    // 20 GB left free on the disk after it
    let need = likely_bytes(src);
    let (cap, free) = (cap_bytes(), free_bytes().unwrap_or(0));
    if free < need + RESERVE || used_bytes() + need > cap {
        return Err(format!("{}: no room for its copy ({:.1} GB wanted, {:.1} GB free, cap {:.1} GB)", src.display(), need as f64 / 1e9, free as f64 / 1e9, cap as f64 / 1e9));
    }
    let part = dir().join(format!("{fp}.{}.partial.mov", std::process::id()));
    let _ = std::fs::remove_file(&part);
    // the ProRes 422 export preset is macOS's only (iOS reads copies it's given, never makes one)
    #[cfg(not(target_os = "macos"))]
    return Err("optimized copies are made on macOS only".into());
    #[cfg(target_os = "macos")]
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&src.display().to_string()));
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let session = AVAssetExportSession::exportSessionWithAsset_presetName(&asset, AVAssetExportPresetAppleProRes422LPCM).ok_or("AVAssetExportSession: ProRes 422 isn't offered for this source")?;
        session.setOutputURL(Some(&NSURL::fileURLWithPath(&NSString::from_str(&part.display().to_string()))));
        session.setOutputFileType(AVFileTypeQuickTimeMovie);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let done = block2::RcBlock::new(move || {
            let _ = tx.send(());
        });
        session.exportAsynchronouslyWithCompletionHandler(&done);
        rx.recv().map_err(|_| "the export never finished".to_string())?;
        if session.status() != AVAssetExportSessionStatus::Completed {
            let _ = std::fs::remove_file(&part);
            let why = session.error().map(|e| e.localizedDescription().to_string()).unwrap_or_else(|| format!("status {:?}", session.status()));
            return Err(format!("{}: the ProRes copy failed: {why}", src.display()));
        }
    }
    std::fs::rename(&part, &out).map_err(|e| e.to_string())?;
    prune();
    Ok(out)
}

/// Partial files left by a copy that was stopped (its process killed): removed once they're an hour old.
fn sweep() {
    let Ok(rd) = std::fs::read_dir(dir()) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a.as_secs() > 3600);
        if p.to_string_lossy().ends_with(".partial.mov") && old {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// What the disk keeps free whatever the copies want.
const RESERVE: u64 = 20_000_000_000;

/// The folder's cap: RAEA_MEZZ_CAP_GB (default 20 GB), and never more than the disk's free space less 20 GB counting
/// what the copies already hold.
fn cap_bytes() -> u64 {
    let set = (std::env::var("RAEA_MEZZ_CAP_GB").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(20.0) * 1e9) as u64;
    let room = (free_bytes().unwrap_or(0) + used_bytes()).saturating_sub(RESERVE);
    set.min(room)
}

/// The bytes the copies hold now.
fn used_bytes() -> u64 {
    std::fs::read_dir(dir()).map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0)
}

/// The free space on the cache folder's volume (NSFileManager's NSFileSystemFreeSize).
fn free_bytes() -> Option<u64> {
    use objc2_foundation::{NSFileManager, NSFileSystemFreeSize, NSNumber};
    let d = dir();
    let probe = if d.exists() { d } else { d.parent()?.to_path_buf() };
    unsafe {
        let fm = NSFileManager::defaultManager();
        let attrs = fm.attributesOfFileSystemForPath_error(&NSString::from_str(&probe.display().to_string())).ok()?;
        let v = attrs.objectForKey(NSFileSystemFreeSize)?;
        let n = v.downcast::<NSNumber>().ok()?;
        Some(n.unsignedLongLongValue())
    }
}

/// A copy's likely size: ~2 bits a pixel at the source's size, rate and length, plus a tenth.
fn likely_bytes(src: &Path) -> u64 {
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&src.display().to_string()));
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let Some(media) = AVMediaTypeVideo else { return u64::MAX / 4 };
        #[allow(deprecated)]
        let tracks = asset.tracksWithMediaType(media);
        let Some(track) = tracks.firstObject() else { return u64::MAX / 4 };
        let s = track.naturalSize();
        let fps = track.nominalFrameRate().max(1.0) as f64;
        #[allow(deprecated)]
        let secs = asset.duration().seconds().max(0.0);
        (s.width * s.height * fps * secs * 2.0 / 8.0 * 1.1) as u64
    }
}

/// The folder kept under its cap (cap_bytes), the least recently used copies removed first.
fn prune() {
    let cap = cap_bytes() as f64;
    let Ok(rd) = std::fs::read_dir(dir()) else { return };
    let mut copies: Vec<(SystemTime, u64, PathBuf)> = rd
        .flatten()
        .filter(|e| !e.path().to_string_lossy().ends_with(".partial.mov"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    copies.sort();
    let mut total: u64 = copies.iter().map(|c| c.1).sum();
    for (_, len, p) in copies {
        if (total as f64) <= cap {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}
