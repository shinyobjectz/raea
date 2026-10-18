//! raea's latent space, as records (the roadmap Track L): every moment and every 2 s window of a clip is one
//! hypervector made of labelled slots, `majority(bind(ROLE, filler), ...)`. Binding is its own inverse, so
//! `bind(record, role("SEEN"))` reads a slot back (noisily), and a query names the slots it asks about. The fillers:
//!
//!   SEEN    SigLIP 2 frame -> bits (Projection 768, seed 7); for a window, the majority of its 16 moments
//!   HEARD   YAMNet's 1024-d embedding of the 0.96 s frame centred nearest the moment -> bits (Projection 1024, seed 11)
//!   CHANGE  level of picture change (1 - cosine to the previous frame, square-root scaled)
//!   ONSET   level of sound onsets (mean positive log-mel flux over the moment's 125 ms)
//!   LOUD    level of loudness (mean log-mel over the moment)
//!   CUT     level of nearness to a cut (1 on it, 0 two seconds or more away; cuts as `raea-meaning cuts` finds them)
//!   WHEN    level of time from the start (0..60 s)
//!   ORDER   (windows) the transitions, permute(previous) XOR next, position-bound and bundled (Track K's L2)
//!   *_SHAPE (windows) CHANGE, ONSET and LOUD per moment, position-bound and bundled (Track K's L2b change profiles)
//!
//! Level ranges are the 2nd and 98th percentiles on the R2 corpus's train creators (2026-10-15); every clip uses the
//! same ones, so levels compare across clips. A slot whose source column is missing is left out of the record.

use crate::k::{bind, majority, permute, role, Col, Hv, Levels, Projection};


pub const MOMENT_SLOTS: [&str; 7] = ["SEEN", "HEARD", "CHANGE", "ONSET", "LOUD", "CUT", "WHEN"];
pub const WINDOW_SLOTS: [&str; 8] = ["SEEN", "HEARD", "ORDER", "CHANGE_SHAPE", "ONSET_SHAPE", "LOUD_SHAPE", "CUT", "WHEN"];

// level ranges (2026-10-15, R2 corpus train creators, 8 fps): picture change on a square-root scale
const CHANGE_SQRT: (f32, f32) = (0.0118, 0.4434); // sqrt of p2 0.00014, p98 0.19659
const ONSET: (f32, f32) = (0.0962, 0.4332);
const LOUD: (f32, f32) = (-4.379, 1.040);
const WHEN_MAX_S: f32 = 60.0;

fn scale(x: f32, (lo, hi): (f32, f32)) -> f32 {
    ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
}

pub struct Sources<'a> {
    pub siglip: &'a Col,
    pub fps: f64,
    /// YAMNet embeddings, 1024 a row, one row per 0.48 s hop
    pub yamnet: Option<&'a [f32]>,
    /// log-mel, 64 bands a row, one row per 10 ms
    pub logmel: Option<&'a [f32]>,
    pub cuts: &'a [usize],
    /// extra per-moment amounts in 0..1, each its own slot (SPEECH, MUSIC, TEXT, FACE, BEAT, FLOW ...): the role from
    /// the name, the level vectors seeded from the name, so a new sense needs no format change
    pub tracks: Vec<(String, Vec<f32>)>,
    /// SEEN bits already computed (the phone's Core ML model ends in this space's projection), one per frame
    pub seen_bits: Option<Vec<Hv>>,
}

pub struct Space {
    pub bits: usize,
    p768: Projection,
    p1024: Projection,
    lv: [Levels; 5], // CHANGE, ONSET, LOUD, CUT, WHEN
    roles: std::collections::BTreeMap<&'static str, Hv>,
}

/// One clip encoded: per moment and per window, the record and (optionally kept) each slot's filler.
pub struct Encoded {
    pub moments: Vec<Hv>,
    pub windows: Vec<Hv>,
    pub window_starts: Vec<usize>,
    pub moment_fillers: Vec<(&'static str, Vec<Hv>)>,
    pub window_fillers: Vec<(&'static str, Vec<Hv>)>,
    /// per moment: change, onset, loud, cut nearness, when (0..1), for checks
    pub levels: Vec<[f32; 5]>,
}

impl Space {
    pub fn new(bits: usize) -> Space {
        let roles = MOMENT_SLOTS.iter().chain(WINDOW_SLOTS.iter()).map(|&r| (r, role(r, bits))).collect();
        Space {
            bits,
            p768: Projection::new(768, bits, 7),
            p1024: Projection::new(1024, bits, 11),
            lv: [21, 22, 23, 24, 25].map(|s| Levels::new(bits, s, 32)),
            roles,
        }
    }

    pub fn role(&self, name: &str) -> &Hv {
        &self.roles[name]
    }

    pub fn level(&self, slot: &str, x: f32) -> &Hv {
        let i = ["CHANGE", "ONSET", "LOUD", "CUT", "WHEN"].iter().position(|s| *s == slot).expect("a level slot");
        self.lv[i].at(x)
    }

    /// SigLIP-space vector (an image or a text embedding, 768) to SEEN bits; YAMNet-space (1024) to HEARD bits.
    pub fn seen(&self, v: &[f32]) -> Hv {
        self.p768.hv8(v)
    }
    pub fn heard(&self, v: &[f32]) -> Hv {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
        self.p1024.hv8(&v.iter().map(|x| x / n).collect::<Vec<_>>())
    }

    pub fn encode(&self, s: &Sources, keep_fillers: bool) -> Encoded {
        let n = s.siglip.n();
        let fps = s.fps as f32;
        let seen: Vec<Hv> = match &s.seen_bits {
            Some(b) if b.len() == n => b.clone(),
            _ => (0..n).map(|t| self.seen(s.siglip.row(t))).collect(),
        };
        let heard: Option<Vec<Hv>> = s.yamnet.map(|y| {
            let rows: Vec<Hv> = y.chunks_exact(1024).map(|r| self.heard(r)).collect(); // each YAMNet row once
            (0..n).map(|t| rows[(((t as f32 / fps - 0.48) / 0.48).round().max(0.0) as usize).min(rows.len() - 1)].clone()).collect()
        });
        let mel = |t: usize| -> Option<(f32, f32)> {
            let l = s.logmel?;
            let rows = l.len() / 64;
            let (a, b) = ((t as f32 * 100.0 / fps) as usize, (((t + 1) as f32 * 100.0 / fps) as usize).max((t as f32 * 100.0 / fps) as usize + 1));
            if a >= rows {
                return Some((0.0, -10.0));
            }
            let b = b.min(rows);
            let (mut flux, mut loud) = (0f32, 0f32);
            for r in a..b {
                for k in 0..64 {
                    let v = l[r * 64 + k];
                    loud += v;
                    if r > 0 {
                        flux += (v - l[(r - 1) * 64 + k]).max(0.0);
                    }
                }
            }
            let m = ((b - a) * 64) as f32;
            Some((flux / m, loud / m))
        };
        let nearest_cut = |t: usize| s.cuts.iter().map(|&c| (c as f32 - t as f32).abs()).fold(f32::MAX, f32::min);
        let levels: Vec<[f32; 5]> = (0..n)
            .map(|t| {
                let change = if t == 0 { 0.0 } else { (1.0 - crate::k::dot(s.siglip.row(t - 1), s.siglip.row(t))).max(0.0).sqrt() };
                let (on, loud) = mel(t).unwrap_or((f32::NAN, f32::NAN));
                [
                    scale(change, CHANGE_SQRT),
                    if on.is_nan() { f32::NAN } else { scale(on, ONSET) },
                    if loud.is_nan() { f32::NAN } else { scale(loud, LOUD) },
                    1.0 - (nearest_cut(t) / fps).min(2.0) / 2.0,
                    (t as f32 / fps / WHEN_MAX_S).min(1.0),
                ]
            })
            .collect();
        let has_mel = s.logmel.is_some();
        // per-moment fillers
        let mut mf: Vec<(&'static str, Vec<Hv>)> = vec![("SEEN", seen.clone())];
        if let Some(h) = &heard {
            mf.push(("HEARD", h.clone()));
        }
        mf.push(("CHANGE", levels.iter().map(|l| self.lv[0].at(l[0]).clone()).collect()));
        if has_mel {
            mf.push(("ONSET", levels.iter().map(|l| self.lv[1].at(l[1]).clone()).collect()));
            mf.push(("LOUD", levels.iter().map(|l| self.lv[2].at(l[2]).clone()).collect()));
        }
        mf.push(("CUT", levels.iter().map(|l| self.lv[3].at(l[3]).clone()).collect()));
        mf.push(("WHEN", levels.iter().map(|l| self.lv[4].at(l[4]).clone()).collect()));
        // extra tracks: names are leaked once per encode (a handful of short strings) to share the slot tables' type
        let track_levels: Vec<(&'static str, Levels, Hv, Vec<f32>)> = s
            .tracks
            .iter()
            .map(|(name, v)| {
                let name: &'static str = Box::leak(name.to_uppercase().into_boxed_str());
                let seed = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
                let per: Vec<f32> = (0..n).map(|t| v.get(t).copied().unwrap_or(0.0)).collect();
                (name, Levels::new(self.bits, seed, 32), role(name, self.bits), per)
            })
            .collect();
        for (name, lv, _, per) in &track_levels {
            mf.push((name, per.iter().map(|x| lv.at(*x).clone()).collect()));
        }
        let role_of = |r: &str| -> Hv { self.roles.get(r).cloned().unwrap_or_else(|| role(r, self.bits)) };
        let moments: Vec<Hv> = (0..n).map(|t| majority(&mf.iter().map(|(r, f)| bind(&role_of(r), &f[t])).collect::<Vec<_>>())).collect();
        // windows: 2 s, 1 s stride
        let (wl, st) = ((2.0 * s.fps).round() as usize, s.fps.round() as usize);
        let starts: Vec<usize> = if n >= wl { (0..=n - wl).step_by(st.max(1)).collect() } else { vec![] };
        let shape = |li: usize, a: usize| majority(&(0..wl).map(|i| permute(self.lv[li].at(levels[a + i][li]), 7 * (i + 1))).collect::<Vec<_>>());
        let mut wf: Vec<(&'static str, Vec<Hv>)> = vec![("SEEN", starts.iter().map(|&a| majority(&seen[a..a + wl])).collect())];
        if let Some(h) = &heard {
            wf.push(("HEARD", starts.iter().map(|&a| majority(&h[a..a + wl])).collect()));
        }
        wf.push((
            "ORDER",
            starts.iter().map(|&a| majority(&(1..wl).map(|i| permute(&bind(&permute(&seen[a + i - 1], 1), &seen[a + i]), 7 * i)).collect::<Vec<_>>())).collect(),
        ));
        wf.push(("CHANGE_SHAPE", starts.iter().map(|&a| shape(0, a)).collect()));
        if has_mel {
            wf.push(("ONSET_SHAPE", starts.iter().map(|&a| shape(1, a)).collect()));
            wf.push(("LOUD_SHAPE", starts.iter().map(|&a| shape(2, a)).collect()));
        }
        wf.push(("CUT", starts.iter().map(|&a| self.lv[3].at((a..a + wl).map(|t| levels[t][3]).fold(0.0, f32::max)).clone()).collect()));
        wf.push(("WHEN", starts.iter().map(|&a| self.lv[4].at(levels[a][4]).clone()).collect()));
        for (name, lv, _, per) in &track_levels {
            wf.push((name, starts.iter().map(|&a| lv.at(per[a..a + wl].iter().sum::<f32>() / wl as f32).clone()).collect()));
        }
        let windows: Vec<Hv> = (0..starts.len()).map(|w| majority(&wf.iter().map(|(r, f)| bind(&role_of(r), &f[w])).collect::<Vec<_>>())).collect();
        Encoded {
            moments,
            windows,
            window_starts: starts,
            moment_fillers: if keep_fillers { mf } else { vec![] },
            window_fillers: if keep_fillers { wf } else { vec![] },
            levels,
        }
    }
}

/// Rows of hypervectors as little-endian u64 words (bits / 64 a row).
pub fn write_hv(path: &str, rows: &[Hv]) -> Result<(), String> {
    let bytes: Vec<u8> = rows.iter().flat_map(|h| h.0.iter().flat_map(|w| w.to_le_bytes())).collect();
    std::fs::write(path, bytes).map_err(|e| format!("{path}: {e}"))
}
