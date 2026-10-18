//! Turn-taking: when to send what was heard, and when talking over the voice stops it. The rules and numbers are UGC
//! Pro's (~/host/docs/voice-mode.feature, VoiceTurns.swift), measured there; time here is the mic's own clock
//! (samples heard / 16 kHz), so the rules run the same live and on a recording.

use super::vad::CHUNK;

const RATE: f32 = super::LISTEN_RATE as f32;
/// Silero's defaults: speech from 0.5, silence below 0.35.
const START_AT: f32 = 0.5;
const END_BELOW: f32 = 0.35;
/// ~100 ms of speech starts a turn; a ~200 ms pause is a soft end, judged by Smart Turn.
const START_CHUNKS: u32 = 3;
const PAUSE_CHUNKS: u32 = 6;
/// Smart Turn reads the last 8 s.
const WINDOW: usize = 8 * 16_000;
/// Words a thought doesn't end on: a pause after one waits longest.
const HANGING: &[&str] = &[
    "about", "and", "the", "to", "a", "an", "of", "or", "but", "so", "with", "for", "in", "on", "at", "like", "my",
    "your", "that", "um", "uh", "because", "if", "then", "into", "is", "are", "was",
];

/// What the caller should do after a chunk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    Nothing,
    /// Someone started (or resumed) talking.
    Started,
    /// A pause began: transcribe the utterance and ask Smart Turn, then call `judged`.
    Paused,
    /// The pause outlasted its wait: send `words()` and call `reset`.
    Send,
    /// Talking over the voice: stop it now.
    BargeIn,
}

#[derive(Default)]
pub struct Turns {
    /// Seconds heard so far (the clock).
    now: f32,
    speaking: bool,
    pause_since: Option<f32>,
    /// When the current pause is sent, once judged.
    deadline: Option<f32>,
    speech_run: u32,
    silence_run: u32,
    utterance: Vec<f32>,
    preroll: Vec<f32>,
    words: String,
    /// The voice is talking: speech counts only as barge-in.
    voice_on: bool,
    /// How loud the voice comes back into the mic, per unit of its output level (learned while it talks).
    echo_gain: f32,
    barge_run: u32,
    /// The turn detector's models aren't loaded: the level heuristic decides instead.
    pub heuristic: bool,
}

impl Turns {
    pub fn new() -> Self {
        Self::default()
    }

    /// The utterance so far (16 kHz), for Parakeet and Smart Turn.
    pub fn utterance(&self) -> &[f32] {
        &self.utterance
    }

    pub fn words(&self) -> &str {
        &self.words
    }

    pub fn speaking(&self) -> bool {
        self.speaking
    }

    /// A new turn: forget the last one (it was sent).
    pub fn reset(&mut self) {
        self.utterance.clear();
        self.words.clear();
        self.speaking = false;
        self.pause_since = None;
        self.deadline = None;
        self.speech_run = 0;
        self.silence_run = 0;
    }

    /// The voice started or stopped talking.
    pub fn voice(&mut self, on: bool) {
        self.voice_on = on;
        self.barge_run = 0;
        if on {
            self.reset();
        }
    }

    /// One 32 ms chunk: its speech probability, its samples, and the speaker's output level while it played.
    pub fn feed(&mut self, p: f32, chunk: &[f32], out_level: f32) -> Step {
        debug_assert_eq!(chunk.len(), CHUNK);
        self.now += CHUNK as f32 / RATE;
        if self.voice_on {
            return self.while_voice(p, chunk, out_level);
        }
        if self.speaking || self.pause_since.is_some() {
            self.utterance.extend_from_slice(chunk);
            if self.utterance.len() > WINDOW {
                self.utterance.drain(..self.utterance.len() - WINDOW);
            }
        } else {
            self.preroll.extend_from_slice(chunk);
            if self.preroll.len() > 10 * CHUNK {
                self.preroll.drain(..self.preroll.len() - 10 * CHUNK);
            }
        }
        if p >= START_AT {
            self.speech_run += 1;
            self.silence_run = 0;
            if self.speech_run >= START_CHUNKS && !self.speaking {
                // talking (again): a pause being judged is reopened
                self.speaking = true;
                self.pause_since = None;
                self.deadline = None;
                if self.utterance.is_empty() {
                    self.utterance = std::mem::take(&mut self.preroll);
                }
                return Step::Started;
            }
        } else if p < END_BELOW {
            self.speech_run = 0;
            self.silence_run += 1;
            if self.speaking && self.silence_run >= PAUSE_CHUNKS {
                self.speaking = false;
                self.pause_since = Some(self.now - (self.silence_run as usize * CHUNK) as f32 / RATE);
                return Step::Paused;
            }
        }
        if let Some(since) = self.pause_since {
            // nothing new for 2 s: send whatever was judged
            let due = self.deadline.is_some_and(|d| self.now >= d) || self.now - since >= 2.0;
            if due && !self.words.trim().is_empty() {
                self.pause_since = None;
                return Step::Send;
            }
        }
        Step::Nothing
    }

    /// The pause's verdict: Smart Turn's probability that the thought is finished (None without the model) and the
    /// words Parakeet heard in the utterance so far. Sets how long the pause waits before it's sent.
    pub fn judged(&mut self, complete: Option<f32>, words: String) {
        let Some(since) = self.pause_since else { return };
        let last = words
            .split_whitespace()
            .last()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .unwrap_or_default();
        let ends_sentence = words.trim_end().ends_with(['.', '?', '!']);
        let wait = match complete {
            _ if HANGING.contains(&last.as_str()) => 1.4,
            Some(c) if c >= 0.5 => 0.0,
            Some(_) => 0.8,
            // no turn model: the heuristic
            None if ends_sentence => 0.5,
            None => 0.85,
        };
        self.words = words;
        self.deadline = Some(since + wait);
    }

    /// While the voice talks: learn its echo, and stop it only for speech clearly louder than that echo.
    fn while_voice(&mut self, p: f32, chunk: &[f32], out_level: f32) -> Step {
        let level = super::audio::rms(chunk);
        if p < END_BELOW && out_level > 0.01 {
            let g = level / out_level;
            self.echo_gain = if self.echo_gain == 0.0 { g } else { self.echo_gain * 0.95 + g * 0.05 };
        }
        let echo = self.echo_gain * out_level;
        // headphones or a quiet room: ~160 ms of speech at 0.7; a speaker that bleeds: ~300 ms clearly over the echo
        let (need, loud) = if self.echo_gain < 0.05 { (5, p >= 0.7) } else { (10, p >= 0.7 && level > echo * 2.0 + 0.01) };
        self.barge_run = if loud { self.barge_run + 1 } else { 0 };
        if self.barge_run >= need {
            self.voice(false);
            self.speaking = true;
            self.utterance = chunk.to_vec();
            return Step::BargeIn;
        }
        Step::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(t: &mut Turns, p: f32, seconds: f32) -> Vec<Step> {
        let n = (seconds * RATE / CHUNK as f32).round() as usize;
        (0..n).map(|_| t.feed(p, &[0.0; CHUNK], 0.0)).filter(|s| *s != Step::Nothing).collect()
    }

    #[test]
    fn a_finished_thought_is_sent_at_the_pause() {
        let mut t = Turns::new();
        assert_eq!(run(&mut t, 0.9, 1.0), vec![Step::Started]);
        assert_eq!(run(&mut t, 0.0, 0.25), vec![Step::Paused]);
        t.judged(Some(0.9), "What's a good hook for a coffee ad?".into());
        assert_eq!(run(&mut t, 0.0, 0.1), vec![Step::Send]);
    }

    #[test]
    fn a_pause_mid_thought_waits_and_reopens() {
        let mut t = Turns::new();
        run(&mut t, 0.9, 1.0);
        run(&mut t, 0.0, 0.25);
        t.judged(Some(0.2), "I want to film a video about".into());
        // "about" hangs: 1.4 s, and talking again within it reopens the turn
        assert!(run(&mut t, 0.0, 0.5).is_empty());
        assert_eq!(run(&mut t, 0.9, 1.0), vec![Step::Started]);
        assert_eq!(run(&mut t, 0.0, 0.25), vec![Step::Paused]);
        t.judged(Some(0.8), "I want to film a video about my morning skincare routine.".into());
        assert_eq!(run(&mut t, 0.0, 0.1), vec![Step::Send]);
    }

    #[test]
    fn an_unsure_pause_is_sent_after_its_wait() {
        let mut t = Turns::new();
        run(&mut t, 0.9, 1.0);
        run(&mut t, 0.0, 0.25);
        t.judged(Some(0.3), "open the hook".into());
        assert!(run(&mut t, 0.0, 0.5).is_empty());
        assert_eq!(run(&mut t, 0.0, 0.2), vec![Step::Send]);
    }

    #[test]
    fn its_own_voice_through_headphones_never_interrupts_but_talking_over_does() {
        let mut t = Turns::new();
        t.voice(true);
        assert!(run(&mut t, 0.1, 2.0).is_empty());
        assert_eq!(run(&mut t, 0.9, 0.2), vec![Step::BargeIn]);
    }
}
