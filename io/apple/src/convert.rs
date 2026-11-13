//! VideoToolbox's BGRA as the RGBA a Frame holds.

/// A row of BGRA as RGBA, its alpha kept or made 255. Four bytes at a time as one word, which the compiler turns into
/// vector code: a 4K frame took 5.5 ms a byte at a time, most of what playing one cost (RAEA_MEDIA_STATS, 11-13).
pub(crate) fn bgra_to_rgba(src: &[u8], dst: &mut [u8], alpha: bool) {
    let opaque = if alpha { 0 } else { 0xff00_0000 };
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let v = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        let out = (v & 0xff00_ff00) | ((v & 0xff) << 16) | ((v >> 16) & 0xff) | opaque;
        d.copy_from_slice(&out.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::bgra_to_rgba;

    #[test]
    fn bgra_becomes_rgba_with_its_alpha_kept_or_made_opaque() {
        let src = [10, 20, 30, 40, 1, 2, 3, 0];
        let mut out = [0u8; 8];
        bgra_to_rgba(&src, &mut out, true);
        assert_eq!(out, [30, 20, 10, 40, 3, 2, 1, 0]);
        bgra_to_rgba(&src, &mut out, false);
        assert_eq!(out, [30, 20, 10, 255, 3, 2, 1, 255]);
    }

    /// A 4K frame's worth, timed (cargo test -p raea-io-apple --release convert -- --nocapture).
    #[test]
    fn a_4k_frame_is_converted_in_a_frame_s_small_part() {
        let src: Vec<u8> = (0..3840 * 2160 * 4).map(|i| (i % 251) as u8).collect();
        let mut out = vec![0u8; src.len()];
        let t = std::time::Instant::now();
        for _ in 0..10 {
            bgra_to_rgba(&src, &mut out, false);
        }
        eprintln!("4K BGRA to RGBA: {:.2} ms a frame", t.elapsed().as_secs_f64() * 100.0);
        assert_eq!(&out[..4], &[src[2], src[1], src[0], 255]);
    }
}
