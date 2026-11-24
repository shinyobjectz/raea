use super::*;

/// lufs as it was: the whole sound weighted (a biquad over each whole channel, then the next), then the blocks
fn lufs_whole(a: &Audio) -> f64 {
    let a = crate::soft_resample(a, RATE);
    let (shelf, hp) = k_filters();
    let biquad = |x: &[f64], c: [f64; 5]| -> Vec<f64> {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        x.iter()
            .map(|&v| {
                let y = c[0] * v + c[1] * x1 + c[2] * x2 - c[3] * y1 - c[4] * y2;
                (x2, x1, y2, y1) = (x1, v, y1, y);
                y
            })
            .collect()
    };
    let weighted: Vec<Vec<f64>> = a.channels.iter().map(|c| biquad(&biquad(&c.iter().map(|x| *x as f64).collect::<Vec<_>>(), shelf), hp)).collect();
    assert!(k_weighted(&a).iter().zip(&weighted).all(|(g, w)| g.iter().zip(w).all(|(x, y)| x.to_bits() == y.to_bits())));
    let len = weighted.first().map(Vec::len).unwrap_or(0);
    let (block, step) = ((0.4 * RATE as f64) as usize, (0.1 * RATE as f64) as usize);
    let (mut z, mut s) = (Vec::new(), 0);
    while s + block <= len {
        z.push(weighted.iter().map(|c| c[s..s + block].iter().map(|v| v * v).sum::<f64>() / block as f64).fold(0.0, |e, x| e + x));
        s += step;
    }
    let l = |e: f64| -0.691 + 10.0 * e.max(1e-15).log10();
    let abs: Vec<f64> = z.iter().copied().filter(|&e| l(e) > -70.0).collect();
    if abs.is_empty() {
        return -70.0;
    }
    let rel = l(abs.iter().sum::<f64>() / abs.len() as f64) - 10.0;
    let gated: Vec<f64> = abs.into_iter().filter(|&e| l(e) > rel).collect();
    l(gated.iter().sum::<f64>() / gated.len().max(1) as f64)
}

#[test]
fn lufs_a_block_at_a_time_is_the_whole_ones_bits() {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut noise = |n: usize, amp: f32| -> Vec<f32> {
        (0..n)
            .map(|i| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                amp * ((seed >> 40) as f32 / (1u64 << 23) as f32 - 1.0) * if (i / 30_000) % 3 == 0 { 0.05 } else { 1.0 }
            })
            .collect()
    };
    for (rate, chans, n) in [(48000, 2, 123_457), (44100, 2, 99_991), (48000, 1, 40_000), (48000, 2, 10_000)] {
        let a = Audio { rate, channels: (0..chans).map(|_| noise(n, 0.5)).collect() };
        assert_eq!(lufs(&a).to_bits(), lufs_whole(&a).to_bits(), "{rate} Hz, {chans} channels, {n} samples");
    }
}

#[test]
fn the_true_peak_and_envelope_a_block_at_a_time_are_the_whole_ones() {
    // a tone and its clicks, past one block of the 4x sound (2^20 samples)
    let ch = |f: f32| (0..400_000).map(|i| (i as f32 * f).sin() * if i % 9973 == 0 { 1.4 } else { 0.6 }).collect::<Vec<f32>>();
    let a = Audio { rate: 48000, channels: vec![ch(0.031), ch(0.047)] };
    let up = crate::soft_resample(&a, a.rate * 4);
    assert_eq!(true_peak_db(&a).to_bits(), peak_db(&up).to_bits());
    let mut want = vec![0f32; a.len()];
    for c in &up.channels {
        for (j, y) in c.iter().enumerate() {
            for i in [(j + 2) / 4, (j + 1) / 4] {
                if i < want.len() && y.abs() > want[i] {
                    want[i] = y.abs();
                }
            }
        }
    }
    for c in &a.channels {
        for (i, x) in c.iter().enumerate() {
            want[i] = want[i].max(x.abs());
        }
    }
    assert!(true_envelope(&a).iter().zip(&want).all(|(g, w)| g.to_bits() == w.to_bits()));
}

/// A tone at a quarter of the rate, 45 degrees off the samples: every sample sits at 0.707 of the wave's peak,
/// so a sample-peak limiter lets the true peak through 3 dB over its ceiling. The target is louder than the
/// ceiling allows, so the limiter has to work.
#[test]
fn the_master_ceiling_holds_between_samples() {
    let n = 3 * RATE as usize;
    let ch: Vec<f32> = (0..n).map(|i| 0.5 * (std::f32::consts::FRAC_PI_2 * i as f32 + std::f32::consts::FRAC_PI_4).sin()).collect();
    let a = Audio { rate: RATE, channels: vec![ch.clone(), ch] };
    let out = master(&a, 6.0, -1.0);
    let (tp, sp) = (true_peak_db(&out), peak_db(&out));
    assert!(tp <= -1.0, "true peak {tp:.2} dBTP over the -1 ceiling (sample peak {sp:.2})");
    assert!(tp > -2.0, "limited far below the ceiling: {tp:.2} dBTP");
}

/// A range's sound: a rifle's crack every 1.5 s with a short decaying tail over a quiet bed of wind, its peaks far
/// over its loudness, so reaching -14 LUFS under a -1.5 dBTP ceiling takes limiting that eats most of the gain (a
/// gambit range demo, made up a dB for a dB in three passes, came out at -15.8). Mastered, it reaches the target
/// (made up the old way it stops 0.25 short). Its true peak isn't held here: limited 10 dB, noise like this comes
/// out up to a dB over the ceiling between samples, which `master_for` takes off after the encoder.
#[test]
fn a_percussive_master_reaches_its_loudness() {
    let n = 9 * RATE as usize;
    // (noise through a one-pole low-pass near 4 kHz, as a crack heard at the firing point is darker than white noise)
    let (mut seed, mut lp) = (12345u32, 0f32);
    let ch: Vec<f32> = (0..n)
        .map(|i| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            lp += 0.4 * (((seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) - lp);
            let since = (i % (RATE as usize * 3 / 2)) as f32 / RATE as f32;
            1.6 * lp * (-since / 0.06).exp() + 0.1 * lp
        })
        .collect();
    let a = Audio { rate: RATE, channels: vec![ch.clone(), ch] };
    let (out, _) = master_gained(&a, -14.0, -1.5);
    let (l, tp) = (lufs(&out), true_peak_db(&out));
    assert!((l + 14.0).abs() < 0.15, "mastered to {l:.2} LUFS, not -14 (true peak {tp:.2})");
    assert!(peak_db(&out) <= -1.5 + 0.05, "sample peak {:.2} dB over the -1.5 ceiling", peak_db(&out));
}
