//! clef-omni's limits, and raea-api's where it's stricter (POST /v1/clef/ask), so a call that passes here passes on
//! every route (docs/ask.feature AK1). Verified against clef's schema and probe calls 2026-10-18 (sdk/app playtest.ts).

use crate::question::{Kind, Question};

/// questions in one call (clef refuses 65)
pub const QUESTIONS: usize = 64;
/// a question id's length (raea-api's; clef takes 100)
pub const ID_LEN: usize = 64;
/// a choice's options (raea-api's; clef takes 255)
pub const OPTIONS: (usize, usize) = (2, 16);
/// a rate's levels
pub const LEVELS: (usize, usize) = (2, 10);
/// stills a call: 4 MiB each, 8 MiB in all
pub const STILLS: usize = 4;
pub const STILL_BYTES: usize = 4 << 20;
pub const STILLS_BYTES: usize = 8 << 20;
/// videos a call: 60 s and 16 MiB each, sampled at 2 fps
pub const VIDEOS: usize = 2;
pub const VIDEO_SECONDS: f64 = 60.0;
pub const VIDEO_BYTES: usize = 16 << 20;
/// sounds a call: 300 s and 8 MiB each
pub const SOUNDS: usize = 4;
pub const SOUND_SECONDS: f64 = 300.0;
pub const SOUND_BYTES: usize = 8 << 20;
/// sound and video together, decoded
pub const AV_BYTES: usize = 16 << 20;
/// the state paragraph (raea-api cuts it at 8,000 characters)
pub const STATE_CHARS: usize = 8000;
/// dollars a million input tokens
pub const USD_PER_M: f64 = 0.15;
/// input tokens, for the estimate `--dry` prints: a second of 480 px video (about 1,400 a two-second clip, 11-18), a
/// minute of sound, a still
pub const TOKENS_VIDEO_S: f64 = 700.0;
pub const TOKENS_SOUND_MIN: f64 = 780.0;
pub const TOKENS_STILL: f64 = 1024.0;

/// Why a question can't be sent, or None.
pub fn check(q: &Question) -> Option<String> {
    let id_ok = !q.id.is_empty() && q.id.len() <= ID_LEN && q.id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b));
    if !id_ok {
        return Some(format!("{:?}: an id is 1 to {ID_LEN} letters, digits, _ . or -", q.id));
    }
    if q.ask.trim().is_empty() {
        return Some(format!("{}: it asks nothing", q.id));
    }
    let n = q.levels().len();
    let (lo, hi) = match q.kind {
        Kind::Yes => return None,
        Kind::Choose | Kind::Grade => OPTIONS,
        Kind::Rate => LEVELS,
    };
    if q.picks.is_none() && !(lo..=hi).contains(&n) {
        return Some(format!("{}: {lo} to {hi} {}, not {n}", q.id, if q.kind == Kind::Choose { "options" } else { "levels" }));
    }
    let mut seen = std::collections::BTreeSet::new();
    if let Some(o) = q.levels().into_iter().find(|o| !seen.insert(o.clone())) {
        return Some(format!("{}: {o:?} is there twice", q.id));
    }
    None
}

/// The estimated cost of one call, in dollars.
pub fn estimate(video_s: f64, sound_s: f64, stills: usize, state_chars: usize, questions: usize) -> f64 {
    let text = (state_chars as f64 + questions as f64 * 120.0) / 4.0;
    let tokens = video_s * TOKENS_VIDEO_S + sound_s / 60.0 * TOKENS_SOUND_MIN + stills as f64 * TOKENS_STILL + text;
    tokens * USD_PER_M / 1e6
}
