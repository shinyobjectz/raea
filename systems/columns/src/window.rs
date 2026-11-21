//! A span's features from a file's columns, as the heads read them. Each axis reads its own modality only (FP0b: a
//! head given two modalities learns worse from a few labels):
//!
//! - picture: the frames' mean SigLIP vector, then the change (below): 771 values
//! - change: the frame-to-frame SigLIP distance's mean and largest step, and the vectors' spread: 3
//! - look: the frames' mean look and its largest frame-to-frame step per value: 32
//! - sound: the mean of the CLAP windows that start in the span (the nearest one when none does): 512
//! - events: YAMNet's mean over the span: 1024
//! - text: EmbeddingGemma 2's mean over the passages in the span, a passage a second (FP1b): 768

use crate::look::LOOK;
use crate::{CLAP, CLAP_HOP, Columns, FPS, SIG, TEXT, YAM, YAM_HOP};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Picture,
    Change,
    Look,
    Sound,
    Events,
    Text,
}

impl Modality {
    /// Text is last, so a row written before it is the same row with the text left empty (evidence.rs)
    pub const ALL: [Modality; 6] = [Modality::Picture, Modality::Change, Modality::Look, Modality::Sound, Modality::Events, Modality::Text];
    pub fn dim(self) -> usize {
        match self {
            Modality::Picture => SIG + 3,
            Modality::Change => 3,
            Modality::Look => 2 * LOOK,
            Modality::Sound => CLAP,
            Modality::Events => YAM,
            Modality::Text => TEXT,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Modality::Picture => "picture",
            Modality::Change => "change",
            Modality::Look => "look",
            Modality::Sound => "sound",
            Modality::Events => "events",
            Modality::Text => "text",
        }
    }
    /// Whether a file's columns have rows this modality reads: a script has no picture, a video no text
    pub fn in_file(self, c: &Columns) -> bool {
        // a file with no sound track still gets one empty clip's CLAP vector (lib.rs sound()), so sound is there only
        // when there are levels to read: a silent render's sound axes judged nothing and flagged it as a dropout
        if matches!(self, Modality::Sound | Modality::Events) && c.level.is_empty() {
            return false;
        }
        !match self {
            Modality::Picture | Modality::Change => &c.sig,
            Modality::Look => &c.look,
            Modality::Sound => &c.clap,
            Modality::Events => &c.yam,
            Modality::Text => &c.text,
        }
        .is_empty()
    }
}

fn rows(v: &[f32], dim: usize, a: usize, b: usize) -> &[f32] {
    let n = v.len() / dim;
    if n == 0 {
        return &[];
    }
    let a = a.min(n - 1);
    let b = b.clamp(a + 1, n);
    &v[a * dim..b * dim]
}

/// The mean row of rows `dim` wide
pub fn mean(r: &[f32], dim: usize) -> Vec<f32> {
    let n = (r.len() / dim).max(1) as f32;
    let mut m = vec![0f32; dim];
    for row in r.chunks(dim) {
        m.iter_mut().zip(row).for_each(|(a, b)| *a += b / n);
    }
    m
}

fn change(s: &[f32]) -> [f32; 3] {
    let n = s.len() / SIG;
    if n < 2 {
        return [0.0; 3];
    }
    let d: Vec<f32> = (1..n).map(|i| s[i * SIG..(i + 1) * SIG].iter().zip(&s[(i - 1) * SIG..i * SIG]).map(|(a, b)| (a - b).powi(2)).sum::<f32>().sqrt()).collect();
    let m = mean(s, SIG);
    let spread = (0..SIG).map(|k| ((0..n).map(|i| (s[i * SIG + k] - m[k]).powi(2)).sum::<f32>() / n as f32).sqrt()).sum::<f32>() / SIG as f32;
    [d.iter().sum::<f32>() / d.len() as f32, d.iter().cloned().fold(0.0, f32::max), spread]
}

/// The span [a, b) seconds of `c` in modality `m`: m.dim() values (zeros where the column is missing)
pub fn features(c: &Columns, a: f64, b: f64, m: Modality) -> Vec<f32> {
    let (fa, fb) = ((a * FPS).floor() as usize, (b * FPS).ceil() as usize);
    let v = match m {
        Modality::Picture => {
            let s = rows(&c.sig, SIG, fa, fb);
            let mut v = mean(s, SIG);
            v.extend(change(s));
            v
        }
        Modality::Change => change(rows(&c.sig, SIG, fa, fb)).to_vec(),
        Modality::Look => {
            let l = rows(&c.look, LOOK, fa, fb);
            let mut v = mean(l, LOOK);
            let n = l.len() / LOOK;
            v.extend((0..LOOK).map(|k| (1..n).map(|i| (l[i * LOOK + k] - l[(i - 1) * LOOK + k]).abs()).fold(0.0, f32::max)));
            v
        }
        Modality::Sound => {
            let n = c.clap_windows();
            let first = ((a / CLAP_HOP).ceil() as usize).min(n.saturating_sub(1));
            let last = (((b - crate::CLAP_WIN) / CLAP_HOP).floor().max(first as f64) as usize + 1).min(n);
            mean(rows(&c.clap, CLAP, first, last), CLAP)
        }
        Modality::Events => mean(rows(&c.yam, YAM, (a / YAM_HOP) as usize, (b / YAM_HOP).ceil() as usize), YAM),
        Modality::Text => mean(rows(&c.text, TEXT, a.floor() as usize, b.ceil() as usize), TEXT),
    };
    if v.len() == m.dim() { v } else { vec![0.0; m.dim()] }
}
