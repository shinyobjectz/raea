//! Motion read back into keys (the roadmap J3): a tracked element's position over frames, split into moves, each fitted
//! with rdb's own easing functions (rdb/src/eval.rs ease_fn: linear, in, out, inout, all cubic): for every start and
//! end frame near the move and every ease, the start value and the distance that fit best (least squares; the frames
//! read are the only data, so a move can start before its element is first seen, as a fade-in does); of the fits
//! about as good as the best, the shortest move is kept. The keys come out as rdb writes them: (frame, value) at each end and the ease between.

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Key {
    pub from: usize,
    pub to: usize,
    pub start: f64,
    pub end: f64,
    pub ease: &'static str,
    /// the fit's root-mean-square error over the frames used (same units as the values)
    pub rms: f64,
}

pub fn ease(e: &str, p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    match e {
        "out" => 1.0 - (1.0 - p).powi(3),
        "in" => p * p * p,
        "linear" => p,
        _ => {
            if p < 0.5 {
                4.0 * p * p * p
            } else {
                1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
            }
        }
    }
}

const EASES: [&str; 4] = ["linear", "in", "out", "inout"];

/// The moves in a series of (frame, value): stretches where the value travels `min_travel` or more, each fitted.
pub fn keys(series: &[(usize, f64)], min_travel: f64) -> Vec<Key> {
    if series.len() < 3 {
        return Vec::new();
    }
    // moving frames: a step of a quarter of min_travel / 10 or more from the previous sample
    let step = min_travel / 40.0;
    let moving: Vec<bool> = (0..series.len()).map(|i| i > 0 && (series[i].1 - series[i - 1].1).abs() >= step.max(0.5)).collect();
    // spans of moving samples, gaps of up to 3 still samples bridged
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for (i, m) in moving.iter().enumerate() {
        if !m {
            continue;
        }
        match spans.last_mut() {
            Some(s) if i <= s.1 + 4 => s.1 = i,
            _ => spans.push((i.saturating_sub(1), i)),
        }
    }
    let mut out = Vec::new();
    for (a, b) in spans {
        let travel = (series[b].1 - series[a].1).abs();
        if travel < min_travel {
            continue;
        }
        // the data: the span and up to 12 samples either side (the settle and the still before)
        let (lo, hi) = (a.saturating_sub(12), (b + 12).min(series.len() - 1));
        let data = &series[lo..=hi];
        let (fa, fb) = (series[a].0 as i64, series[b].0 as i64);
        let mut fits: Vec<Key> = Vec::new();
        // a move under way from the element's first sample began one frame before it: the frame it was there unseen (a
        // fade from opacity 0, or an entrance); the tail of an eased move is itself nearly the same ease, so the data
        // alone can't place an unseen start
        let starts: Vec<i64> = if a == 0 { vec![(series[0].0 as i64 - 1).max(0)] } else { ((fa - 8).max(0)..=fa).collect() };
        for f0 in starts {
            for f1 in fb..=fb + 12 {
                if f1 <= f0 {
                    continue;
                }
                for e in EASES {
                    // y = s + d * ease(p): least squares for s and d
                    let pts: Vec<(f64, f64)> = data.iter().map(|(f, y)| (ease(e, (*f as f64 - f0 as f64) / (f1 - f0) as f64), *y)).collect();
                    let n = pts.len() as f64;
                    let (sx, sy) = (pts.iter().map(|p| p.0).sum::<f64>(), pts.iter().map(|p| p.1).sum::<f64>());
                    let (sxx, sxy) = (pts.iter().map(|p| p.0 * p.0).sum::<f64>(), pts.iter().map(|p| p.0 * p.1).sum::<f64>());
                    let den = n * sxx - sx * sx;
                    if den.abs() < 1e-12 {
                        continue;
                    }
                    let d = (n * sxy - sx * sy) / den;
                    let s = (sy - d * sx) / n;
                    let rms = (pts.iter().map(|(x, y)| (s + d * x - y).powi(2)).sum::<f64>() / n).sqrt();
                    fits.push(Key { from: f0 as usize, to: f1 as usize, start: s, end: s + d, ease: e, rms });
                }
            }
        }
        // of the fits about as good as the best (within 20% and a third of a unit), the shortest move: frames nobody saw
        // (an element fading in mid-move) don't get a longer, made-up move through them
        let best_rms = fits.iter().map(|k| k.rms).fold(f64::MAX, f64::min);
        let best = fits.into_iter().filter(|k| k.rms <= best_rms * 1.2 + 0.33)
            .min_by(|a, b| (a.to - a.from).cmp(&(b.to - b.from)).then(a.rms.total_cmp(&b.rms)));
        out.extend(best);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_eased_move_is_read_back() {
        // y from 340 to 260 over frames 0..18, ease out; seen only from frame 1 (a fade-in), held after
        let series: Vec<(usize, f64)> = (1..60).map(|f| (f, 340.0 - 80.0 * ease("out", f as f64 / 18.0))).collect();
        let k = keys(&series, 20.0);
        assert_eq!(k.len(), 1, "{k:?}");
        assert_eq!((k[0].from, k[0].to, k[0].ease), (0, 18, "out"), "{k:?}");
        assert!((k[0].start - 340.0).abs() < 0.5 && (k[0].end - 260.0).abs() < 0.5, "{k:?}");
    }

    #[test]
    fn a_linear_move_is_linear() {
        let series: Vec<(usize, f64)> = (0..90).map(|f| (f, if f < 30 { 0.0 } else if f > 60 { 300.0 } else { 10.0 * (f - 30) as f64 })).collect();
        let k = keys(&series, 20.0);
        assert_eq!((k[0].from, k[0].to, k[0].ease), (30, 60, "linear"), "{k:?}");
    }

    #[test]
    fn a_move_seen_only_from_its_middle_isnt_stretched() {
        // 470 to 410 over frames 14..32, ease out, first seen at frame 15 (a fade-in)
        let series: Vec<(usize, f64)> = (15..90).map(|f| (f, 470.0 - 60.0 * ease("out", (f as f64 - 14.0) / 18.0))).collect();
        let k = keys(&series, 20.0);
        assert_eq!((k[0].from, k[0].to, k[0].ease), (14, 32, "out"), "{k:?}");
        assert!((k[0].end - 410.0).abs() < 1.0, "{k:?}");
    }

    #[test]
    fn stillness_has_no_keys() {
        let series: Vec<(usize, f64)> = (0..50).map(|f| (f, 100.0 + if f % 2 == 0 { 0.4 } else { 0.0 })).collect();
        assert!(keys(&series, 20.0).is_empty());
    }
}

/// J5: an element's motion as one hypervector (4096 bits), so motions can be matched across edits ("the same pop-in as
/// the hook's title"): each move's features (its axis and way, its ease, its length and its distance on log scales) are
/// role vectors bound (XOR) to value vectors, the features of a move bundled (majority), and the moves bundled in order
/// (each shifted by its place); an element that holds still is the "still" vector. Similarity is the share of bits that
/// agree (0.5 for unrelated vectors).
pub const BITS: usize = 4096;
pub type Hv = [u64; BITS / 64];

fn seeded(name: &str) -> Hv {
    let mut s: u64 = 0xcbf29ce484222325;
    for b in name.bytes() {
        s = (s ^ b as u64).wrapping_mul(0x100000001b3);
    }
    let mut v = [0u64; BITS / 64];
    for w in v.iter_mut() {
        // splitmix64
        s = s.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        *w = z ^ (z >> 31);
    }
    v
}

fn xor(a: &Hv, b: &Hv) -> Hv {
    let mut o = [0u64; BITS / 64];
    for i in 0..o.len() {
        o[i] = a[i] ^ b[i];
    }
    o
}

fn bundle(vs: &[Hv]) -> Hv {
    let mut o = [0u64; BITS / 64];
    if vs.is_empty() {
        return o;
    }
    for bit in 0..BITS {
        let (w, m) = (bit / 64, 1u64 << (bit % 64));
        let ones = vs.iter().filter(|v| v[w] & m != 0).count();
        // ties go to the first vector, so an even count still decides
        if ones * 2 > vs.len() || (ones * 2 == vs.len() && vs[0][w] & m != 0) {
            o[w] |= m;
        }
    }
    o
}

fn shift(v: &Hv, by: usize) -> Hv {
    let mut o = [0u64; BITS / 64];
    for bit in 0..BITS {
        if v[bit / 64] & (1 << (bit % 64)) != 0 {
            let t = (bit + by) % BITS;
            o[t / 64] |= 1 << (t % 64);
        }
    }
    o
}

/// A move's features: (axis, way), ease, frames (log2), distance in hundredths of the frame (log2); `size` is the frame
/// width for x moves, its height for y.
pub fn move_hv(axis: &str, k: &Key, size: f64) -> Hv {
    let way = if k.end < k.start { "back" } else { "forward" };
    let frames = (((k.to - k.from).max(1)) as f64).log2().round() as i64;
    let dist = (((k.end - k.start).abs() / size * 100.0).max(1.0)).log2().round() as i64;
    let f = |role: &str, value: String| xor(&seeded(&format!("role:{role}")), &seeded(&format!("{role}={value}")));
    bundle(&[f("axis", format!("{axis}-{way}")), f("ease", k.ease.into()), f("frames", frames.to_string()), f("distance", dist.to_string()),
             f("axis", format!("{axis}-{way}"))])
}

/// An element's motion: its moves (x and y, in order of their start) bundled, each shifted by its place.
pub fn motion_hv(keys_x: &[Key], keys_y: &[Key], w: f64, h: f64) -> Hv {
    let mut moves: Vec<(usize, Hv)> = keys_x.iter().map(|k| (k.from, move_hv("x", k, w))).chain(keys_y.iter().map(|k| (k.from, move_hv("y", k, h)))).collect();
    if moves.is_empty() {
        return seeded("still");
    }
    moves.sort_by_key(|m| m.0);
    let placed: Vec<Hv> = moves.iter().enumerate().map(|(i, (_, v))| shift(v, i * 7)).collect();
    if placed.len() == 1 { placed[0] } else { bundle(&placed) }
}

pub fn similarity(a: &Hv, b: &Hv) -> f64 {
    1.0 - a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum::<u32>() as f64 / BITS as f64
}

#[cfg(test)]
mod hv_tests {
    use super::*;

    fn key(from: usize, to: usize, start: f64, end: f64, ease: &'static str) -> Key {
        Key { from, to, start, end, ease, rms: 0.0 }
    }

    #[test]
    fn like_moves_match_and_unlike_ones_dont() {
        let rise_out = motion_hv(&[], &[key(0, 18, 340.0, 260.0, "out")], 1080.0, 1920.0);
        let rise_out_2 = motion_hv(&[], &[key(14, 32, 470.0, 410.0, "out")], 1080.0, 1920.0);
        let slide_in = motion_hv(&[key(0, 20, -300.0, 0.0, "inout")], &[], 1080.0, 1920.0);
        let still = motion_hv(&[], &[], 1080.0, 1920.0);
        assert!(similarity(&rise_out, &rise_out_2) > 0.7, "{}", similarity(&rise_out, &rise_out_2));
        assert!(similarity(&rise_out, &slide_in) < 0.6, "{}", similarity(&rise_out, &slide_in));
        assert!((similarity(&rise_out, &still) - 0.5).abs() < 0.05);
    }
}
