//! Room tone at the joins: a gap the mix drops into digital silence and comes straight back from (two lines of a
//! take joined with a gap between them) is filled with the room's own tone, the quietest stretch of sound before it.
//! A dead-silent gap reads as a dropout (11-14: the walkaround's v1; raea check called it one, and the agent rebuilt
//! the voice outside raea to fill its gaps). The tone is the quietest stretch of sound within LOOK of the gap, before
//! it or, where the line before runs on without a pause, after it. The gap is the check's dropout exactly (raea check's defects.rs): from
//! HAD dBFS or more to under SILENT in one 10 ms step, back within two to within 10 dB, 50 ms to 3 s later. A pause that
//! fades out or in is the edit's own and is left as it is. The render's master fills them (it has the whole bounce);
//! live play hears the gap.
use raea_io_media::Audio;

/// The check's dropout: from this level (dBFS, a 10 ms step's RMS) ...
pub const HAD: f64 = -45.0;
/// ... to under this
pub const SILENT: f64 = -80.0;
/// How far back the room tone is looked for (s), and how long a stretch of it is taken (s)
const LOOK: f64 = 3.0;
const TONE: f64 = 0.3;
/// How far under HAD a gap's edge may be and still be a hard join (dB)
const EDGE: f64 = 5.0;
/// A stretch quieter than this is silence, not tone (dBFS)
const QUIETEST: f64 = -75.0;

fn db(sq: f64) -> f64 {
    10.0 * sq.max(1e-20).log10()
}

/// Each 10 ms step's level (dBFS RMS over the channels)
fn steps(a: &Audio, hop: usize) -> Vec<f64> {
    let n = a.len() / hop;
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let mut s = 0f64;
        for c in &a.channels {
            for v in &c[k * hop..(k + 1) * hop] {
                s += (*v as f64) * (*v as f64);
            }
        }
        out.push(db(s / (hop * a.channels.len().max(1)) as f64));
    }
    out
}

/// The gaps (first and last step, end exclusive) the check would call dropouts, on steps measured as the master will
/// play them: a run of dead silence, 50 ms to 3 s, with sound (within EDGE dB of HAD) inside the three steps either
/// side of it. The check's own grid of steps can split a 12 ms fade where this one doesn't, so the edge is looked for
/// over 30 ms; a fade that takes longer than that is a pause, and stays.
pub fn gaps(rms: &[f64]) -> Vec<(usize, usize)> {
    let loud = |s: &[f64]| s.iter().any(|l| *l >= HAD - EDGE);
    let mut out = Vec::new();
    let mut i = 1;
    while i < rms.len() {
        if rms[i] >= SILENT || rms[i - 1] < SILENT {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < rms.len() && rms[j] < SILENT {
            j += 1;
        }
        if (5..=300).contains(&(j - i)) && loud(&rms[i.saturating_sub(3)..i]) && loud(&rms[j..(j + 3).min(rms.len())]) {
            out.push((i, j));
        }
        i = j.max(i + 1);
    }
    out
}

/// `a` with each dropout filled by the room's tone: the quietest TONE of sound (not silence) in the LOOK before it,
/// looped with 10 ms crossfades and faded in and out at the gap's edges. Its gaps are found as the master will play
/// it, `gain` dB up, where the check hears them. `a` itself where there's none, not a copy of it (a 7-minute film's
/// sound is 169 MB, SG4a).
pub fn filled(a: &Audio, gain: f64) -> std::borrow::Cow<'_, Audio> {
    let hop = (a.rate as usize / 100).max(1);
    let rms: Vec<f64> = steps(a, hop).into_iter().map(|l| l + gain).collect();
    let found = gaps(&rms);
    if found.is_empty() {
        return std::borrow::Cow::Borrowed(a);
    }
    let mut out = a.clone();
    let longest = (TONE * 100.0) as usize;
    let xf = hop;
    for (i, j) in found {
        // the quietest stretch of sound near the gap, before it or after it: the room between words, the longest of
        // them up to TONE (a line that runs on leaves only short pauses: 120 ms loops well enough under a short gap)
        let quietest = |tone: usize| {
            let near = (i.saturating_sub((LOOK * 100.0) as usize)..i.saturating_sub(tone)).chain(j..(j + (LOOK * 100.0) as usize).min(rms.len().saturating_sub(tone)));
            near.filter(|&s| rms[s..s + tone].iter().all(|l| *l >= QUIETEST && *l < HAD + 15.0)).min_by(|&x, &y| {
                let m = |s: usize| rms[s..s + tone].iter().sum::<f64>();
                m(x).total_cmp(&m(y))
            }).map(|t| (t, tone))
        };
        let Some((t, tone)) = [longest, longest * 2 / 3, longest * 2 / 5].into_iter().find_map(quietest) else {
            continue;
        };
        let (src, len) = (t * hop, tone * hop);
        let (start, end) = (i * hop, (j * hop).min(a.len()));
        for c in 0..out.channels.len() {
            for (k, s) in (start..end).enumerate() {
                // looped, each pass crossfaded into the next; in and out over 10 ms at the gap's edges
                let (pass, at) = (k / (len - xf), k % (len - xf));
                let mut v = a.channels[c][src + at];
                if pass > 0 && at < xf {
                    let w = at as f32 / xf as f32;
                    v = v * w + a.channels[c][src + len - xf + at] * (1.0 - w);
                }
                let edge = (k.min(end - s) as f32 / xf as f32).min(1.0);
                out.channels[c][s] += v * edge;
            }
        }
    }
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, level: f32, seed: &mut u32) -> Vec<f32> {
        (0..n).map(|_| {
            *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((*seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * level
        }).collect()
    }

    /// Two lines with room tone under them and a dead gap between: the gap is filled, and the check's dropout is gone;
    /// a pause that fades is left alone
    #[test]
    fn a_dead_gap_gets_the_room_s_tone() {
        let rate = 48_000u32;
        let mut seed = 7;
        // 1 s of a line (-20 dBFS) with a pause of room tone in it (-65), 0.2 s dead, 1 s more
        let mut x = noise(rate as usize, 0.1, &mut seed);
        for v in x[rate as usize / 4..].iter_mut().take(rate as usize / 2) {
            *v *= 0.01;
        }
        x.extend(vec![0.0; rate as usize / 5]);
        x.extend(noise(rate as usize, 0.1, &mut seed));
        let a = Audio { rate, channels: vec![x.clone(), x] };
        let hop = rate as usize / 100;
        assert_eq!(gaps(&steps(&a, hop)).len(), 1);
        let f = filled(&a, 0.0);
        // a gap under the line before the master's gain is still one once it's played: found at the level heard
        let quiet = Audio { rate, channels: a.channels.iter().map(|c| c.iter().map(|v| v * 0.03).collect()).collect() };
        assert!(gaps(&steps(&quiet, hop)).is_empty());
        assert!(!gaps(&steps(&quiet, hop).into_iter().map(|l| l + 30.0).collect::<Vec<_>>()).is_empty());
        assert!(gaps(&steps(&filled(&quiet, 30.0), hop).into_iter().map(|l| l + 30.0).collect::<Vec<_>>()).is_empty());
        assert!(gaps(&steps(&f, hop)).is_empty());
        let gap = &f.channels[0][rate as usize + rate as usize / 20..rate as usize + rate as usize / 10];
        assert!(gap.iter().any(|v| v.abs() > 1e-4), "the gap holds tone");
        // a fade to silence and back over 0.3 s is a pause, not a dropout
        let fade: Vec<f32> = (0..rate as usize * 2).map(|i| {
            let t = i as f32 / rate as f32;
            let g = ((t - 1.0).abs() / 0.15).min(1.0);
            ((i % 97) as f32 / 97.0 - 0.5) * 0.2 * g
        }).collect();
        assert!(gaps(&steps(&Audio { rate, channels: vec![fade] }, hop)).is_empty());
    }
}
