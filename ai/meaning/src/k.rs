//! The K kernels: whole-column operations over embedding columns and hypervectors. Every job in the meaning plane is
//! one or two of these over a whole clip at once (docs/relational-plan.feature, "Embeddings are columns").
//!
//! A column is n rows of d floats, row-major, rows L2-normalised (so a dot product is a cosine). Hypervectors are
//! bit-packed u64 words; bind is XOR, bundle is a per-bit majority, similarity is 1 - 2 * hamming / bits. Everything is
//! exact or order-fixed, so results reproduce bit for bit.

/// n rows of d floats.
#[derive(Clone, Debug, PartialEq)]
pub struct Col {
    pub d: usize,
    pub data: Vec<f32>,
}

impl Col {
    pub fn new(d: usize) -> Col {
        Col { d, data: Vec::new() }
    }
    pub fn n(&self) -> usize {
        if self.d == 0 { 0 } else { self.data.len() / self.d }
    }
    pub fn row(&self, i: usize) -> &[f32] {
        &self.data[i * self.d..(i + 1) * self.d]
    }
    pub fn push(&mut self, v: &[f32]) {
        assert_eq!(v.len(), self.d);
        self.data.extend_from_slice(v);
    }
}

pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

pub fn normalise(v: &[f32]) -> Vec<f32> {
    let n = dot(v, v).sqrt().max(1e-12);
    v.iter().map(|x| x / n).collect()
}

/// The query against every row at once: the timeline lights up with similarity.
pub fn dots(c: &Col, q: &[f32]) -> Vec<f32> {
    (0..c.n()).map(|i| dot(c.row(i), q)).collect()
}

/// Distance between each row and the one before it (1 - cosine); the first is 0. Cuts are spikes in this.
pub fn neighbour_distance(c: &Col) -> Vec<f32> {
    (0..c.n()).map(|i| if i == 0 { 0.0 } else { 1.0 - dot(c.row(i - 1), c.row(i)) }).collect()
}

/// The median of each value's neighbours within `w` either side (itself excluded): the local baseline a spike stands
/// out from.
pub fn local_median(x: &[f32], w: usize) -> Vec<f32> {
    (0..x.len())
        .map(|i| {
            let lo = i.saturating_sub(w);
            let hi = (i + w + 1).min(x.len());
            let mut v: Vec<f32> = (lo..hi).filter(|&k| k != i).map(|k| x[k]).collect();
            if v.is_empty() {
                return 0.0;
            }
            v.sort_by(|a, b| a.total_cmp(b));
            v[v.len() / 2]
        })
        .collect()
}

/// Spikes: indices where x is at least `floor` and at least `ratio` times its local median, keeping the strongest
/// within `min_gap` of each other.
pub fn spikes(x: &[f32], w: usize, ratio: f32, floor: f32, min_gap: usize) -> Vec<usize> {
    let med = local_median(x, w);
    let mut out: Vec<usize> = Vec::new();
    for i in 0..x.len() {
        if x[i] >= floor && x[i] >= ratio * med[i].max(1e-4) {
            match out.last() {
                Some(&j) if i - j < min_gap => {
                    if x[i] > x[j] {
                        *out.last_mut().unwrap() = i;
                    }
                }
                _ => out.push(i),
            }
        }
    }
    out
}

/// Indices of the k largest values, largest first (ties by index, so it's stable).
pub fn top_k(x: &[f32], k: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..x.len()).collect();
    idx.sort_by(|&a, &b| x[b].total_cmp(&x[a]).then(a.cmp(&b)));
    idx.truncate(k);
    idx
}

/// For each row of `a`, the nearest row of `b` and its similarity.
pub fn nearest(a: &Col, b: &Col) -> Vec<(usize, f32)> {
    (0..a.n()).map(|i| {
        let s = dots(b, a.row(i));
        let j = top_k(&s, 1)[0];
        (j, s[j])
    }).collect()
}

// === hypervectors ===

/// A bit-packed hypervector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hv(pub Vec<u64>);

impl Hv {
    pub fn bits(&self) -> usize {
        self.0.len() * 64
    }
}

/// A deterministic generator (splitmix64), so projections and random role vectors are the same everywhere.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// A standard normal (Box-Muller), for the projection's hyperplanes.
    pub fn normal(&mut self) -> f32 {
        let u = ((self.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64;
        let v = ((self.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64;
        ((-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()) as f32
    }
}

/// A random hypervector (a role or a symbol), from a seed.
pub fn random_hv(bits: usize, seed: u64) -> Hv {
    let mut r = Rng::new(seed);
    Hv((0..bits / 64).map(|_| r.next()).collect())
}

/// The bridge from learned to symbolic: `bits` random hyperplanes (seeded), one bit per side. Hamming distance between
/// two projections tracks the angle between the vectors (P[bit differs] = angle / pi).
pub struct Projection {
    planes: Vec<f32>,
    d: usize,
    bits: usize,
}

impl Projection {
    pub fn new(d: usize, bits: usize, seed: u64) -> Projection {
        assert!(bits % 64 == 0);
        let mut r = Rng::new(seed);
        Projection { planes: (0..d * bits).map(|_| r.normal()).collect(), d, bits }
    }
    pub fn hv(&self, v: &[f32]) -> Hv {
        assert_eq!(v.len(), self.d);
        let mut words = vec![0u64; self.bits / 64];
        for b in 0..self.bits {
            if dot(&self.planes[b * self.d..(b + 1) * self.d], v) > 0.0 {
                words[b / 64] |= 1 << (b % 64);
            }
        }
        Hv(words)
    }
}

impl Projection {
    /// The planes, row-major [bits][d] (for building the same projection elsewhere, e.g. inside a Core ML model).
    pub fn planes(&self) -> &[f32] {
        &self.planes
    }

    /// The same sign projection with 8 accumulators per dot (SIMD-friendly). Summation order differs from `hv`, so a
    /// bit can differ only when a dot is within rounding of 0; measured 0 of 1.79 M bits on a corpus clip (Track K).
    pub fn hv8(&self, v: &[f32]) -> Hv {
        assert_eq!(v.len(), self.d);
        let mut words = vec![0u64; self.bits / 64];
        for b in 0..self.bits {
            let plane = &self.planes[b * self.d..(b + 1) * self.d];
            let mut acc = [0f32; 8];
            for (x, y) in plane.chunks_exact(8).zip(v.chunks_exact(8)) {
                for j in 0..8 {
                    acc[j] += x[j] * y[j];
                }
            }
            let tail: f32 = plane.chunks_exact(8).remainder().iter().zip(v.chunks_exact(8).remainder()).map(|(x, y)| x * y).sum();
            if acc.iter().sum::<f32>() + tail > 0.0 {
                words[b / 64] |= 1 << (b % 64);
            }
        }
        Hv(words)
    }
}

/// A role's vector: random, from the role's name (FNV-1a), so "SEEN" is the same vector everywhere.
pub fn role(name: &str, bits: usize) -> Hv {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    random_hv(bits, h)
}

/// Level hypervectors, HDC's code for amounts: a base vector, and amount x in 0..1 flips the first x * bits/2 bits in
/// a fixed random order, so near amounts get near vectors (similarity falls linearly) and 0 against 1 is orthogonal.
pub struct Levels {
    table: Vec<Hv>,
}

impl Levels {
    pub fn new(bits: usize, seed: u64, steps: usize) -> Levels {
        let mut r = Rng::new(seed);
        let base = random_hv(bits, seed ^ 0x5eed);
        let mut order: Vec<usize> = (0..bits).collect();
        for i in (1..bits).rev() {
            let j = (r.next() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        let table = (0..=steps)
            .map(|s| {
                let mut v = base.clone();
                for &b in &order[..s * bits / 2 / steps] {
                    v.0[b / 64] ^= 1 << (b % 64);
                }
                v
            })
            .collect();
        Levels { table }
    }
    pub fn at(&self, x: f32) -> &Hv {
        let steps = self.table.len() - 1;
        &self.table[((x.clamp(0.0, 1.0) * steps as f32).round() as usize).min(steps)]
    }
    /// The amount a vector is closest to (decoding a level after unbinding its role).
    pub fn read(&self, v: &Hv) -> f32 {
        let steps = self.table.len() - 1;
        let best = (0..=steps).min_by_key(|&s| hamming(&self.table[s], v)).unwrap();
        best as f32 / steps as f32
    }
}

/// Bind (XOR): the result is unlike both, and binding again with either recovers the other.
pub fn bind(a: &Hv, b: &Hv) -> Hv {
    Hv(a.0.iter().zip(&b.0).map(|(x, y)| x ^ y).collect())
}

/// Permute (rotate by k bits): order in a sequence, so (a, b) and (b, a) bundle differently. Bit i moves to
/// (i + k) mod bits; whole words move, then a carry shift (the bit-by-bit loop it replaces is in the tests).
pub fn permute(a: &Hv, k: usize) -> Hv {
    let n = a.0.len();
    let k = k % a.bits();
    let (ws, bs) = (k / 64, k % 64);
    let mut out = vec![0u64; n];
    for o in 0..n {
        let hi = a.0[(o + n - ws) % n];
        out[o] = if bs == 0 { hi } else { (hi << bs) | (a.0[(o + 2 * n - ws - 1) % n] >> (64 - bs)) };
    }
    Hv(out)
}

/// Bundle (per-bit majority): like each input. Ties break toward a fixed seeded vector, so the result is exact.
/// Counts are bit-sliced (a ripple-carry adder over whole words), so a bundle of n vectors costs about n * words * 6
/// word operations instead of a loop per bit (the per-bit loop it replaces is in the tests).
pub fn majority(hvs: &[Hv]) -> Hv {
    let n = hvs.len();
    let words = hvs[0].0.len();
    let tie = random_hv(words * 64, 0x7175_6e64_6c65);
    let planes_n = (usize::BITS - n.leading_zeros()) as usize; // enough bit planes to count to n
    let above = n / 2 + 1; // count >= above <=> 2 * count > n
    let mut out = vec![0u64; words];
    let mut planes = vec![0u64; planes_n];
    for w in 0..words {
        planes.iter_mut().for_each(|p| *p = 0);
        for h in hvs {
            let mut carry = h.0[w];
            for p in planes.iter_mut() {
                let sum = *p ^ carry;
                carry &= *p;
                *p = sum;
                if carry == 0 {
                    break;
                }
            }
        }
        // compare the per-bit counts with constants, most significant plane first
        let cmp = |k: usize| -> (u64, u64) {
            let (mut gt, mut eq) = (0u64, !0u64);
            for (i, p) in planes.iter().enumerate().rev() {
                if (k >> i) & 1 == 1 {
                    eq &= *p;
                } else {
                    gt |= eq & *p;
                    eq &= !*p;
                }
            }
            (gt, eq)
        };
        let (gt, eq) = cmp(above);
        let mut word = gt | eq;
        if n % 2 == 0 {
            let (_, tied) = cmp(n / 2);
            word |= tied & tie.0[w];
        }
        out[w] = word;
    }
    Hv(out)
}

pub fn hamming(a: &Hv, b: &Hv) -> u32 {
    a.0.iter().zip(&b.0).map(|(x, y)| (x ^ y).count_ones()).sum()
}

/// 1 for the same vector, about 0 for unrelated ones, -1 for opposites.
pub fn hv_sim(a: &Hv, b: &Hv) -> f32 {
    1.0 - 2.0 * hamming(a, b) as f32 / a.bits() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(rows: &[&[f32]]) -> Col {
        let mut c = Col::new(rows[0].len());
        for r in rows {
            c.push(&normalise(r));
        }
        c
    }

    #[test]
    fn a_cut_is_a_spike_in_neighbour_distance() {
        // ten frames of one shot, then ten of another
        let a = [1.0, 0.1, 0.0];
        let b = [0.0, 0.2, 1.0];
        let rows: Vec<&[f32]> = (0..20).map(|i| if i < 10 { &a[..] } else { &b[..] }).collect();
        let d = neighbour_distance(&col(&rows));
        assert_eq!(spikes(&d, 5, 4.0, 0.05, 5), vec![10]);
    }

    #[test]
    fn search_ranks_the_matching_frames() {
        let c = col(&[&[1.0, 0.0], &[0.0, 1.0], &[0.9, 0.1], &[0.1, 0.9]]);
        assert_eq!(top_k(&dots(&c, &normalise(&[1.0, 0.0])), 2), vec![0, 2]);
    }

    #[test]
    fn nearest_finds_where_b_arrives() {
        let a = col(&[&[1.0, 0.0, 0.0]]);
        let b = col(&[&[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0], &[0.95, 0.05, 0.0]]);
        assert_eq!(nearest(&a, &b)[0].0, 2);
    }

    #[test]
    fn projection_keeps_similarity() {
        let p = Projection::new(3, 4096, 7);
        let (x, y, z) = (normalise(&[1.0, 0.2, 0.0]), normalise(&[0.9, 0.3, 0.1]), normalise(&[-0.2, 0.1, 1.0]));
        let (hx, hy, hz) = (p.hv(&x), p.hv(&y), p.hv(&z));
        assert!(hv_sim(&hx, &hy) > hv_sim(&hx, &hz) + 0.3, "{} vs {}", hv_sim(&hx, &hy), hv_sim(&hx, &hz));
        assert_eq!(p.hv(&x), Projection::new(3, 4096, 7).hv(&x), "seeded, so the same everywhere");
    }

    #[test]
    fn bind_unbinds_and_bundles_stay_similar() {
        let (role, filler, other) = (random_hv(8192, 1), random_hv(8192, 2), random_hv(8192, 3));
        let bound = bind(&role, &filler);
        assert!(hv_sim(&bound, &filler).abs() < 0.05);
        assert_eq!(bind(&bound, &role), filler);
        let m = majority(&[role.clone(), filler.clone(), other.clone()]);
        assert!(hv_sim(&m, &role) > 0.4 && hv_sim(&m, &filler) > 0.4);
        assert!(hv_sim(&permute(&role, 1), &role).abs() < 0.05);
        assert_eq!(permute(&permute(&role, 3), 8192 - 3), role);
    }

    #[test]
    fn levels_are_graded_and_read_back() {
        let l = Levels::new(8192, 3, 32);
        let (a, b, c) = (l.at(0.2), l.at(0.25), l.at(0.9));
        assert!(hv_sim(a, b) > hv_sim(a, c) + 0.3, "near amounts are near");
        assert!(hv_sim(l.at(0.0), l.at(1.0)).abs() < 0.05, "0 and 1 are orthogonal");
        assert!((l.read(l.at(0.4)) - 0.4).abs() < 0.04);
        let r = role("SEEN", 8192);
        assert_eq!(r, role("SEEN", 8192));
        assert!(hv_sim(&r, &role("HEARD", 8192)).abs() < 0.05);
    }

    /// The word-level permute and the bit-sliced majority give exactly what the per-bit loops they replaced gave.
    #[test]
    fn fast_kernels_match_the_per_bit_ones() {
        let slow_permute = |a: &Hv, k: usize| {
            let bits = a.bits();
            let mut out = vec![0u64; a.0.len()];
            for i in 0..bits {
                let j = (i + k) % bits;
                out[j / 64] |= ((a.0[i / 64] >> (i % 64)) & 1) << (j % 64);
            }
            Hv(out)
        };
        let slow_majority = |hvs: &[Hv]| {
            let words = hvs[0].0.len();
            let tie = random_hv(words * 64, 0x7175_6e64_6c65);
            let mut out = vec![0u64; words];
            for w in 0..words {
                for b in 0..64 {
                    let ones = hvs.iter().filter(|h| (h.0[w] >> b) & 1 == 1).count() * 2;
                    let bit = if ones > hvs.len() { 1 } else if ones < hvs.len() { 0 } else { (tie.0[w] >> b) & 1 };
                    out[w] |= bit << b;
                }
            }
            Hv(out)
        };
        let a = random_hv(8192, 9);
        for k in [0, 1, 7, 63, 64, 65, 131, 4097, 8191, 8192 + 5] {
            assert_eq!(permute(&a, k), slow_permute(&a, k % 8192), "permute by {k}");
        }
        for n in 1..=17 {
            let hs: Vec<Hv> = (0..n).map(|i| random_hv(8192, 100 + i as u64)).collect();
            assert_eq!(majority(&hs), slow_majority(&hs), "majority of {n}");
        }
    }
}
