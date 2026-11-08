//! #28 (2): a fixed quantiser reaches openh264: the mezz copy's all-intra QP (h264_encoder's `intra_qp`) and a GOP's
//! (GopEncoder) each change what it writes. With rate control off, as both were, openh264 wrote QP 26 whatever was
//! asked: QP 12 and QP 40 gave the same bytes.
#![cfg(feature = "h264")]

use raea_io_codec::EncodeOpts;
use raea_io_soft::{GopEncoder, h264_encoder};

const W: u32 = 320;
const H: u32 = 192;

/// soft rings moving across: detail every quantiser keeps a different amount of
fn frame(k: u32) -> Vec<u8> {
    (0..W * H).flat_map(|i| {
        let (x, y) = (i % W, i / W);
        let (dx, dy) = (x as i64 - 160 + k as i64 * 4, y as i64 - 96);
        let v = ((dx * dx + dy * dy) / 13 % 256) as u8;
        [v, v.wrapping_mul(3), 255 - v, 255]
    }).collect()
}

fn intra(qp: u8) -> u64 {
    let out = std::env::temp_dir().join(format!("raea-soft-qp-{}-{qp}.mp4", std::process::id()));
    let opts = EncodeOpts { width: W, height: H, fps: 30.0, kbps: 0, quality: None, nv12: false, realtime: false };
    let mut enc = h264_encoder(&out, &opts, Some(qp)).unwrap();
    for k in 0..6 {
        enc.push(&frame(k)).unwrap();
    }
    enc.finish(None).unwrap();
    let n = std::fs::metadata(&out).unwrap().len();
    let _ = std::fs::remove_file(&out);
    n
}

fn gop(qp: u8) -> Vec<u8> {
    let mut enc = GopEncoder::new(W, H, 30.0, qp).unwrap();
    (0..6).flat_map(|k| enc.push(&frame(k)).unwrap().0).collect()
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
