//! The direct VideoToolbox seek against AVAssetReader's: the same NV12 pixels at shuffled times, and how long each
//! seek takes. `cargo run --release --features apple --example vtseek_check -- VIDEO [SEEKS]`
use objc2_core_video::*;

fn planes(pb: &CVPixelBuffer) -> Vec<u8> {
    unsafe {
        CVPixelBufferLockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
        let (w, h) = (CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb));
        let mut v = Vec::with_capacity(w * h * 3 / 2);
        if CVPixelBufferGetPlaneCount(pb) == 0 {
            let base = CVPixelBufferGetBaseAddress(pb) as *const u8;
            let bpr = CVPixelBufferGetBytesPerRow(pb);
            for r in 0..h {
                v.extend_from_slice(std::slice::from_raw_parts(base.add(r * bpr), w * 4));
            }
            CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
            return v;
        }
        for (plane, rows) in [(0usize, h), (1, h / 2)] {
            let base = CVPixelBufferGetBaseAddressOfPlane(pb, plane) as *const u8;
            let bpr = CVPixelBufferGetBytesPerRowOfPlane(pb, plane);
            for r in 0..rows {
                v.extend_from_slice(std::slice::from_raw_parts(base.add(r * bpr), w));
            }
        }
        CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
        v
    }
}

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("a video"));
    let n: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(40);
    let rgb = std::env::args().nth(3).as_deref() == Some("rgb");
    let be = raea_media::backend("apple").unwrap();
    let mut direct = be.open_video(&path).unwrap();
    let dur = direct.info().duration;
    let mut seed = 7u64;
    let mut times = Vec::new();
    for _ in 0..n {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        times.push((seed >> 11) as f64 / (1u64 << 53) as f64 * (dur - 0.1));
    }
    let (mut td, mut tr, mut same, mut worst) = (Vec::new(), Vec::new(), 0, 0u8);
    for &t in &times {
        let t0 = std::time::Instant::now();
        let a = if rgb { direct.frame_surface_rgb_at(t) } else { direct.frame_surface_at(t) }.unwrap().unwrap();
        td.push(t0.elapsed().as_secs_f64() * 1e3);
        // the reader's frame: a fresh decoder for each, as a seek gets (the direct seeker switched off)
        unsafe { std::env::set_var("RAEA_VT_SEEK", "0") };
        let mut reader = be.open_video(&path).unwrap();
        let t0 = std::time::Instant::now();
        let b = if rgb { reader.frame_surface_rgb_at(t) } else { reader.frame_surface_at(t) }.unwrap().unwrap();
        tr.push(t0.elapsed().as_secs_f64() * 1e3);
        unsafe { std::env::remove_var("RAEA_VT_SEEK") };
        let (pa, pb) = (planes(&a.buffer), planes(&b.buffer));
        let d = pa.iter().zip(&pb).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(255);
        if (a.pts - b.pts).abs() < 1e-3 && pa.len() == pb.len() && d == 0 {
            same += 1;
        } else {
            eprintln!("t {t:.3}: direct pts {:.3} reader pts {:.3}, max diff {d}", a.pts, b.pts);
        }
        worst = worst.max(d);
    }
    let q = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        (v[v.len() / 2], v[(v.len() - 1) * 99 / 100], v[v.len() - 1])
    };
    println!("{} seeks: {} identical (worst diff {worst}); direct p50/p99/max {:.1?} ms, reader {:.1?} ms", n, same, q(&mut td), q(&mut tr));
}
