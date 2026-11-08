//! What openh264 is asked for reaches what it writes (#28; Windows' and Linux's encoder):
//! - (3) `--quality`: it ignored EncodeOpts.quality, so draft and master gave byte-identical files; now quality scales
//!   its bitrate (quality_kbps).
//! - (2) a fixed quantiser: the mezz copy's all-intra QP (h264_encoder's `intra_qp`) and a GOP's (GopEncoder). With
//!   rate control off, as both were, openh264 wrote QP 26 whatever was asked: QP 12 and QP 40 gave the same bytes.
#![cfg(feature = "h264")]

use raea_io_codec::EncodeOpts;
use raea_io_soft::{GopEncoder, h264_encoder};

/// soft rings moving across, `w`x`h`, frame `k`: detailed enough that the bitrate or the quantiser decides the size,
/// not noise that every rate leaves at openh264's coarsest quantiser
fn rings(w: u32, h: u32, ring: i64, k: u32) -> Vec<u8> {
    (0..w * h).flat_map(|i| {
        let (x, y) = (i % w, i / w);
        let (dx, dy) = (x as i64 - w as i64 / 2 + k as i64 * 4, y as i64 - h as i64 / 2);
        let v = ((dx * dx + dy * dy) / ring % 256) as u8;
        [v, v.wrapping_mul(3), 255 - v, 255]
    }).collect()
}

/// The size of `n` frames of rings encoded to an MP4 by h264_encoder
fn mp4(o: &EncodeOpts, intra_qp: Option<u8>, ring: i64, n: u32) -> u64 {
    let out = std::env::temp_dir().join(format!("raea-soft-quality-{}-{:?}-{intra_qp:?}.mp4", std::process::id(), o.quality));
    let mut enc = h264_encoder(&out, o, intra_qp).unwrap();
    for k in 0..n {
        enc.push(&rings(o.width, o.height, ring, k)).unwrap();
    }
    enc.finish(None).unwrap();
    let size = std::fs::metadata(&out).unwrap().len();
    let _ = std::fs::remove_file(&out);
    size
}

/// 90 frames of 640x360: three seconds, so the rate outweighs the first frame
fn encode(quality: Option<f32>) -> u64 {
    mp4(&EncodeOpts { width: 640, height: 360, fps: 30.0, kbps: 2000, quality, nv12: false, realtime: false }, None, 23, 90)
}

#[test]
fn quality_changes_what_openh264_writes() {
    let (draft, master, rate) = (encode(Some(0.6)), encode(Some(0.95)), encode(Some(0.85)));
    // before, all three were the same file
    assert!(draft * 5 / 4 < rate && rate * 5 / 4 < master, "draft {draft}, high {rate} and master {master} bytes: a higher quality spends more");
}

const W: u32 = 320;
const H: u32 = 192;

fn intra(qp: u8) -> u64 {
    mp4(&EncodeOpts { width: W, height: H, fps: 30.0, kbps: 0, quality: None, nv12: false, realtime: false }, Some(qp), 13, 6)
}

fn gop(qp: u8) -> Vec<u8> {
    let mut enc = GopEncoder::new(W, H, 30.0, qp).unwrap();
    (0..6).flat_map(|k| enc.push(&rings(W, H, 13, k)).unwrap().0).collect()
}

#[test]
fn the_mezz_copys_quantiser_changes_what_openh264_writes() {
    let (fine, coarse) = (intra(16), intra(40));
    assert!(coarse * 2 < fine, "QP 16 {fine} bytes, QP 40 {coarse}: a finer quantiser spends more");
}

#[test]
fn a_gops_quantiser_changes_what_openh264_writes() {
    let (fine, coarse) = (gop(16).len(), gop(40).len());
    assert!(coarse * 2 < fine, "QP 16 {fine} bytes, QP 40 {coarse}: a finer quantiser spends more");
    // and a GOP is still the same bytes each time it's made (an export stitched from GOPs made at different times)
    assert!(gop(16) == gop(16));
}

#[test]
fn a_quantiser_below_openh264s_floor_is_its_floor() {
    // openh264 refuses a pinned QP under 14 with rate control on (Native:3); asked for less, it gets 14
    assert!(gop(4) == gop(14));
}
