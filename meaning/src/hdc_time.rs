//! Times the HDC sandwich's layers (the roadmap Track K) with k.rs's bit-packed kernels on a real frame column:
//! L0 (a frame's floats to 8192 bits; k.rs's serial dot, and the same with 8 accumulators), L1 transitions, L2 windows
//! (position-bound majority of 15), L2b change profiles (three signals' level vectors per moment, position-bound,
//! majority of 16 each) and one Hamming compare. One thread. Per 2 s window at 8 fps: 16 moments; at 1 s stride,
//! 8 new L0 frames a window. No dependencies beyond k.rs, so the iPhone probe (meaning/ios-hdc) links the same code.

use crate::k::{bind, hamming, majority, permute, random_hv, Hv, Projection, Rng};

fn time(reps: usize, f: &mut dyn FnMut()) -> f64 {
    f();
    let t = std::time::Instant::now();
    for _ in 0..reps {
        f();
    }
    t.elapsed().as_secs_f64() * 1000.0 / reps as f64
}

/// `rows`: n frames of `dim` floats, row-major. Returns one JSON object (no serde, so the probe stays dependency-free).
pub fn run(dim: usize, rows: &[f32], label: &str) -> String {
    let n = (rows.len() / dim).min(400);
    let row = |i: usize| &rows[i * dim..(i + 1) * dim];
    let p = Projection::new(dim, 8192, 7);
    let mut h: Vec<Hv> = Vec::new();
    let l0 = time(1, &mut || h = (0..n).map(|i| p.hv(row(i))).collect()) / n as f64;
    let planes: Vec<f32> = {
        let mut r = Rng::new(7);
        (0..dim * 8192).map(|_| r.normal()).collect()
    };
    let dot8 = |a: &[f32], b: &[f32]| -> f32 {
        let mut acc = [0f32; 8];
        for (x, y) in a.chunks_exact(8).zip(b.chunks_exact(8)) {
            for j in 0..8 {
                acc[j] += x[j] * y[j];
            }
        }
        acc.iter().sum()
    };
    let mut h8: Vec<Hv> = Vec::new();
    let l0_simd = time(1, &mut || {
        h8 = (0..n)
            .map(|i| {
                let mut words = vec![0u64; 128];
                for b in 0..8192 {
                    if dot8(&planes[b * dim..(b + 1) * dim], row(i)) > 0.0 {
                        words[b / 64] |= 1 << (b % 64);
                    }
                }
                Hv(words)
            })
            .collect()
    }) / n as f64;
    let flipped: u32 = h.iter().zip(&h8).map(|(a, b)| hamming(a, b)).sum();
    let win = &h[..16];
    let mut trans: Vec<Hv> = Vec::new();
    let l1 = time(2000, &mut || trans = (1..16).map(|i| bind(&permute(&win[i - 1], 1), &win[i])).collect());
    let mut w2 = Hv(vec![]);
    let l2 = time(2000, &mut || w2 = majority(&trans.iter().enumerate().map(|(i, t)| permute(t, 7 * (i + 1))).collect::<Vec<_>>()));
    let levels: Vec<Vec<Hv>> = (0..3).map(|s| (0..33).map(|j| random_hv(8192, 1000 * s + j)).collect()).collect();
    let picks: Vec<usize> = (0..16).map(|i| (i * 7) % 33).collect();
    let l2b = time(1000, &mut || {
        for lv in &levels {
            let _ = majority(&picks.iter().enumerate().map(|(i, &j)| permute(&lv[j], 7 * (i + 1))).collect::<Vec<_>>());
        }
    });
    let mut d = 0;
    let ham = time(200_000, &mut || d = hamming(&w2, &h[20]));
    format!(
        "{{\"column\": {label:?}, \"bits\": 8192, \"frames_timed\": {n}, \"L0_simd_bits_differing_from_reference\": {flipped}, \
\"L0_simd_bits_total\": {}, \"ms\": {{\"L0_per_frame\": {l0:.4}, \"L0_per_frame_simd\": {l0_simd:.4}, \
\"L1_transitions_per_window\": {l1:.4}, \"L2_window\": {l2:.4}, \"L2b_profiles_3_signals\": {l2b:.4}, \
\"hamming_compare\": {ham:.6}, \"layers_above_L0_per_window\": {:.4}}}, \"check\": {d}}}",
        n * 8192,
        l1 + l2 + l2b
    )
}
