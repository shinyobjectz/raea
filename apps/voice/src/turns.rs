//! Whose turn it is: Silero VAD v5 (MIT) window by window as the microphone comes in, and from it when the person
//! starts speaking and when they've stopped (a pause long enough to be the end of what they were saying).

use std::path::PathBuf;

use anyhow::anyhow;
use ort::session::Session;
use ort::value::Tensor;

pub const RATE: usize = 16_000;
/// Silero's window: 32 ms.
pub const WIN: usize = 512;
const CTX: usize = 64;
/// The silero-vad 5.1.2 package's model.
const URL: &str = "https://github.com/snakers4/silero-vad/raw/v5.1.2/src/silero_vad/data/silero_vad.onnx";

pub fn model(progress: &mut dyn FnMut(&str, u64)) -> anyhow::Result<PathBuf> {
    raea_sys_ml::models::ensure_url("silero-vad", "silero_vad.onnx", URL, progress)
}

pub struct Vad {
    session: Session,
    state: Vec<f32>,
    ctx: Vec<f32>,
}

impl Vad {
    pub fn load(file: &std::path::Path) -> anyhow::Result<Vad> {
        Ok(Vad { session: raea_sys_ml::session(file)?, state: vec![0.0; 2 * 128], ctx: vec![0.0; CTX] })
    }

    /// How likely this window (WIN samples at 16 kHz) is speech; the state carries on from the last one.
    pub fn speech(&mut self, win: &[f32]) -> anyhow::Result<f32> {
        let e = |e: ort::Error| anyhow!("silero vad: {e}");
        let mut input = self.ctx.clone();
        input.extend_from_slice(win);
        self.ctx.copy_from_slice(&input[input.len() - CTX..]);
        let o = self
            .session
            .run(ort::inputs![
                "input" => Tensor::from_array(([1usize, CTX + WIN], input)).map_err(e)?,
                "state" => Tensor::from_array(([2usize, 1, 128], self.state.clone())).map_err(e)?,
                "sr" => Tensor::from_array(((), vec![RATE as i64])).map_err(e)?,
            ])
            .map_err(e)?;
        let p = o["output"].try_extract_tensor::<f32>().map_err(e)?.1[0];
        self.state.copy_from_slice(&o["stateN"].try_extract_tensor::<f32>().map_err(e)?.1[..256]);
        Ok(p)
    }

    pub fn reset(&mut self) {
        self.state.fill(0.0);
        self.ctx.fill(0.0);
    }
}

/// What a window of sound changed.
#[derive(Debug, PartialEq)]
pub enum Turn {
    /// the person started speaking
    Began,
    /// they stopped: everything they said, with a little quiet on either side
    Ended(Vec<f32>),
}

/// The turn-taking over window probabilities: speech begins after `ON` windows in a row at 0.5 or more, and ends
/// after `quiet` windows in a row under 0.35 (Silero's own two thresholds).
pub struct Turns {
    /// windows of quiet that end a turn
    pub quiet: usize,
    speaking: bool,
    run: usize,
    silent: usize,
    /// the sound kept: the last PRE windows while nobody speaks, the whole turn while someone does
    kept: Vec<f32>,
}

/// Windows of speech in a row that begin a turn (96 ms: a click or a cough is shorter).
const ON: usize = 3;
/// Windows kept from before a turn began (its first sound comes before Silero is sure).
const PRE: usize = 10;
/// A turn is cut off here (a minute), so one never grows without end under steady noise.
const LONGEST: usize = RATE * 60;

impl Turns {
    /// `pause_ms`: how long a pause ends the turn.
    pub fn new(pause_ms: usize) -> Turns {
        Turns { quiet: (pause_ms * RATE / 1000 / WIN).max(1), speaking: false, run: 0, silent: 0, kept: Vec::new() }
    }

    pub fn speaking(&self) -> bool {
        self.speaking
    }

    /// Forgets a turn under way (the microphone was muted, or raea itself was talking).
    pub fn clear(&mut self) {
        (self.speaking, self.run, self.silent) = (false, 0, 0);
        self.kept.clear();
    }

    /// The turn under way, as far as it got, when raea itself starts talking over it: half a second or more of it is
    /// still the person's words (they began before the agent's greeting reached them), anything shorter is forgotten.
    pub fn cut(&mut self) -> Option<Vec<f32>> {
        let said = (self.speaking && self.kept.len() >= RATE / 2).then(|| std::mem::take(&mut self.kept));
        self.clear();
        said
    }

    /// One more window and how likely it is speech.
    pub fn push(&mut self, win: &[f32], p: f32) -> Option<Turn> {
        self.kept.extend_from_slice(win);
        if !self.speaking {
            self.run = if p >= 0.5 { self.run + 1 } else { 0 };
            if self.run >= ON {
                (self.speaking, self.silent) = (true, 0);
                return Some(Turn::Began);
            }
            let keep = (PRE + self.run) * WIN;
            if self.kept.len() > keep {
                self.kept.drain(..self.kept.len() - keep);
            }
            return None;
        }
        self.silent = if p < 0.35 { self.silent + 1 } else { 0 };
        if self.silent < self.quiet && self.kept.len() < LONGEST {
            return None;
        }
        // the closing quiet but for a few windows goes: it's the pause, not the turn
        let cut = self.silent.saturating_sub(4) * WIN;
        let mut said = std::mem::take(&mut self.kept);
        said.truncate(said.len() - cut.min(said.len()));
        self.clear();
        Some(Turn::Ended(said))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_cut_short_keeps_what_was_said() {
        let mut t = Turns::new(700);
        assert!(t.cut().is_none());
        for _ in 0..40 {
            t.push(&[0.1; WIN], 0.9);
        }
        assert!(t.speaking());
        assert!(t.cut().is_some_and(|said| said.len() >= RATE / 2));
        assert!(!t.speaking() && t.cut().is_none());
        // a moment of it isn't a turn
        for _ in 0..ON + 1 {
            t.push(&[0.1; WIN], 0.9);
        }
        assert!(t.speaking() && t.cut().is_none());
    }

    fn feed(t: &mut Turns, probs: &[(f32, usize)]) -> Vec<Turn> {
        let mut out = Vec::new();
        for (p, n) in probs {
            for _ in 0..*n {
                out.extend(t.push(&[0.1; WIN], *p));
            }
        }
        out
    }

    #[test]
    fn a_turn_begins_on_steady_speech_and_ends_on_a_long_pause() {
        let mut t = Turns::new(640);
        // a click (one window), then half a second of speech, a short pause mid-sentence, more speech, then quiet
        let got = feed(&mut t, &[(0.1, 20), (0.9, 1), (0.1, 5), (0.9, 16), (0.1, 8), (0.9, 16), (0.1, 30)]);
        assert_eq!(got.len(), 2, "one turn: the click begins nothing, the short pause ends nothing: {got:?}");
        assert_eq!(got[0], Turn::Began);
        let Turn::Ended(said) = &got[1] else { panic!("{got:?}") };
        // what was said: the quiet kept from before, both runs of speech and the pause between, a little closing quiet
        let windows = said.len() / WIN;
        assert!((16 + 8 + 16 + 4..=16 + 8 + 16 + 4 + PRE + ON).contains(&windows), "{windows} windows");
        assert!(!t.speaking());
    }

    #[test]
    fn clearing_forgets_a_turn_under_way() {
        let mut t = Turns::new(640);
        assert_eq!(feed(&mut t, &[(0.9, 10)]), vec![Turn::Began]);
        t.clear();
        assert!(feed(&mut t, &[(0.1, 40)]).is_empty(), "nothing ends: nothing was under way");
    }
}
