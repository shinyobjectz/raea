//! (c) Platform hardware on Apple: AVFoundation, with VideoToolbox underneath, decodes HEVC and H.264 straight from
//! phone files (rotation applied by the asset's video composition) and encodes H.264/HEVC + AAC. Android (MediaCodec)
//! and Linux GPU servers (VA-API, NVDEC) implement the same trait in their own modules.
#![cfg(target_vendor = "apple")]

mod audio;
mod convert;
mod decode;
mod encode;
mod vt;
/// seeking on VideoToolbox driven directly (the frame at a time, decoding only what it depends on)
pub mod vtseek;

pub use audio::{aac_round_trip, playable};
pub use raea_io_codec::SurfaceFrame;
pub use raea_io_apple_coding::{aenc, mezz, packets};

use audio::*;
use decode::*;
use encode::*;
use vt::*;

use std::path::Path;

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::*;
use objc2_core_foundation::CFString;
use objc2_core_media::{CMTime, CMTimeRange, kCMTimePositiveInfinity};
use objc2_core_video::*;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use raea_io_codec::{Audio, Backend, EncodeOpts, EncodeStats, Frame, Info, R, VideoDecoder, VideoEncoder};

pub struct Apple;

/// A CoreFoundation string constant as the NSString key AVFoundation takes (toll-free bridged).
pub fn cf_key(k: &CFString) -> &NSString {
    // CFString and NSString are toll-free bridged
    unsafe { &*(k as *const CFString as *const NSString) }
}

fn asset(path: &Path) -> Retained<AVURLAsset> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.display().to_string()));
    unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) }
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

fn open_video_pooled(path: &Path) -> R<Box<dyn VideoDecoder>> {
    open_any(path).map(|d| d as Box<dyn VideoDecoder>)
}

/// The decoder for `path` through its optimized copies (raea_io_codec::mezz::open, lightspeed F4), each decoder opened
/// in its own pool.
fn open_any(path: &Path) -> R<Box<dyn VideoDecoder + Send>> {
    let open: fn(&Path) -> R<Decoder> = |p| objc2::rc::autoreleasepool(|_| open_decoder(p));
    let d = raea_io_codec::mezz::open(path, open, || raea_io_apple_coding::mezz::probe(path).map(|p| p.frame_secs()))?;
    Ok(Box::new(d))
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
    let alpha;
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
        // whether its frames carry alpha (ProRes 4444 at depth 32, HEVC with alpha): the decoder keeps it (#113)
        alpha = track.formatDescriptions().firstObject().is_some_and(|d| {
            use objc2_core_foundation::{CFBoolean, CFNumber};
            let fd = &*(Retained::as_ptr(&d) as *const objc2_core_media::CMFormatDescription);
            let has = fd.extension(objc2_core_media::kCMFormatDescriptionExtension_ContainsAlphaChannel).is_some_and(|v| v.downcast_ref::<CFBoolean>().is_some_and(|b| b.as_bool()));
            has || fd.extension(objc2_core_media::kCMFormatDescriptionExtension_Depth).and_then(|v| v.downcast_ref::<CFNumber>().and_then(|n| n.as_i32())) == Some(32)
        });
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
    Ok(Decoder {
        asset: a,
        info,
        reader: None,
        cur: None,
        next: None,
        since_restart: 0,
        nv12: false,
        alpha,
        surface: false,
        sample_pts: None,
        shift: None,
        watch: Watch::new(),
        path: path.to_path_buf(),
        seeker: None,
        last_t: None,
    })
}
