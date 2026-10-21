//! A word clock for speech that's still being made: which word a voice is on, worked out from the audio as it arrives,
//! so captions can follow the real voice while the rest of the line is still being generated.
//!
//! Pocket TTS says a line sentence by sentence and gives no alignment, so the clock listens. The audio's loudness every
//! 10 ms (RMS over 20 ms, in dB, averaged over the last 50 ms) is scanned for syllable nuclei: a peak that is the
//! loudest from 90 ms before it to 30 ms after, within 24 dB of the loudest heard so far, and 1.5 dB over the quietest
//! moment since the last nucleus. Each sentence's words are counted out in syllables (an English estimate from vowel
//! groups) and each nucleus spends one; a word starts 120 ms before its first syllable's nucleus. A pause (150 ms
//! quieter than 25 dB under the loudest) within two syllables of a clause's or sentence's end puts the count there, so
//! a miscount doesn't run on past it, and the end of one of Pocket's sentences finishes its words.
//!
//! Tuned on six narrator lines against Parakeet's word times: a median error of 0.10 s, 63% of words within 150 ms and
//! 80% within 300 ms (sdk/motion/docs/speech.md). Good for captions that follow the voice; not a forced aligner.
//!
//! Every decision about a moment uses only the audio up to 180 ms after it (LOOKAHEAD_S), and the frames are taken in
//! order, so the clock's answer for a time is the same whether the line was finished long ago (a render) or is still
//! being made (live play): it's a function of the audio and the text alone.

/// How far past a moment the clock must have heard before it can say which word is being spoken then.
pub const LOOKAHEAD_S: f64 = 0.18;

const HOP_S: f64 = 0.010;
const WIN_S: f64 = 0.020;
/// loudness is averaged over this many hops (causally)
const SMOOTH: usize = 5;
/// a nucleus is the loudest from BEFORE hops before to AFTER after
const BEFORE: usize = 9;
const AFTER: usize = 3;
/// within this many dB of the loudest so far, and this many over the quietest since the last nucleus
const PEAK_DB: f32 = 24.0;
const DIP_DB: f32 = 1.5;
/// a word starts this long before its first nucleus
const LEAD_S: f64 = 0.120;
/// a pause: this many hops quieter than SILENCE_DB under the loudest
const PAUSE_HOPS: usize = 15;
const SILENCE_DB: f32 = 25.0;
/// a pause takes the count to a clause's end this many syllables away at most
const SNAP: usize = 2;

/// Syllables in an English word, estimated: vowel groups (y counts as a vowel after the first letter), a silent final
/// "e" dropped, at least one. Digits count one each.
pub fn syllables(word: &str) -> usize {
    let w: Vec<char> = word.chars().filter(|c| c.is_alphanumeric() || *c == '\'').flat_map(|c| c.to_lowercase()).collect();
    let digits = w.iter().filter(|c| c.is_ascii_digit()).count();
    let letters: Vec<char> = w.into_iter().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return digits.max(1);
    }
    let vowel = |i: usize| matches!(letters[i], 'a' | 'e' | 'i' | 'o' | 'u') || (letters[i] == 'y' && i > 0);
    let mut n = 0;
    let mut prev = false;
    for i in 0..letters.len() {
        let v = vowel(i);
        if v && !prev {
            n += 1;
        }
        prev = v;
    }
    let l = letters.len();
    // a silent final e ("time", "stone"), but not "-le" after a consonant ("table") or a lone vowel group ("the")
    if l > 2 && letters[l - 1] == 'e' && !vowel(l - 2) && !(letters[l - 2] == 'l' && l > 3 && !vowel(l - 3)) && n > 1 {
        n -= 1;
    }
    n.max(1) + digits
}

/// A word ends a clause when it ends with , ; : . ! ? or a dash.
fn ends_clause(word: &str) -> bool {
    word.trim_end_matches(['"', '\'', ')', '\u{201d}', '\u{2019}']).ends_with([',', ';', ':', '.', '!', '?', '-', '\u{2014}', '\u{2013}'])
}

struct Sent {
    start: usize,
    end: Option<usize>,
    /// (syllables before the word, the word's syllables)
    syll: Vec<(usize, usize)>,
    total: usize,
    /// syllable counts at each clause's end
    clauses: Vec<usize>,
    /// the line's index of its first word
    first: usize,
}

/// The clock for one line.
pub struct Clock {
    rate: f64,
    hop: usize,
    win: usize,
    /// each hop's loudness (dB), raw and smoothed
    raw: Vec<f32>,
    env: Vec<f32>,
    /// the quietest since the last nucleus
    dip: f32,
    /// the next hop to decide (hops before it are decided)
    k: usize,
    run_max: f32,
    silence: usize,
    snapped: bool,
    sents: Vec<Sent>,
    cur: usize,
    count: usize,
    finished: bool,
    /// every word of the line, as Pocket says it
    pub words: Vec<String>,
    /// each word's start (seconds into the line), for the words started so far
    pub starts: Vec<f64>,
}

impl Clock {
    pub fn new(rate: u32) -> Clock {
        let r = rate as f64;
        Clock { rate: r, hop: (HOP_S * r).round() as usize, win: (WIN_S * r).round() as usize, raw: Vec::new(), env: Vec::new(), dip: f32::INFINITY, k: 0, run_max: f32::NEG_INFINITY, silence: 0, snapped: false, sents: Vec::new(), cur: 0, count: 0, finished: false, words: Vec::new(), starts: Vec::new() }
    }

    /// A sentence begins at sample `at` with these words.
    pub fn begin_sentence(&mut self, text: &str, at: usize) {
        let mut syll = Vec::new();
        let mut clauses = Vec::new();
        let mut before = 0;
        let first = self.words.len();
        for w in text.split_whitespace() {
            let s = syllables(w);
            syll.push((before, s));
            before += s;
            if ends_clause(w) {
                clauses.push(before);
            }
            self.words.push(w.to_string());
        }
        self.sents.push(Sent { start: at, end: None, syll, total: before, clauses, first });
    }

    /// The sentence being said ended at sample `at`.
    pub fn end_sentence(&mut self, at: usize) {
        if let Some(s) = self.sents.last_mut()
            && s.end.is_none()
        {
            s.end = Some(at);
        }
    }

    /// Decides what `audio` (the whole line so far) lets it; `done` when the line has all been made.
    pub fn advance(&mut self, audio: &[f32], done: bool) {
        // the loudness of every hop whose window is complete (with the line done, the last windows padded with silence)
        loop {
            let j = self.env.len();
            let a = j * self.hop;
            if a + self.win > audio.len() && !(done && a < audio.len() + self.win * 2 + self.hop * (AFTER + 2)) {
                break;
            }
            let end = (a + self.win).min(audio.len());
            let sum: f32 = if a < end { audio[a..end].iter().map(|x| x * x).sum() } else { 0.0 };
            let db = 20.0 * ((sum / self.win as f32 + 1e-12).sqrt() + 1e-9).log10();
            self.raw.push(db);
            let lo = self.raw.len().saturating_sub(SMOOTH);
            let n = self.raw.len() - lo;
            self.env.push(self.raw[lo..].iter().sum::<f32>() / n as f32);
        }
        // hop k is decided once hop k + AFTER's loudness is known
        while self.k + AFTER < self.env.len() {
            self.decide(self.k);
            self.k += 1;
        }
        if done && self.k + AFTER >= self.env.len() && !self.finished {
            // the line's end: every word left starts at its sentence's end (or here)
            let t = (self.k * self.hop) as f64 / self.rate;
            while self.starts.len() < self.words.len() {
                let at = self.starts.last().map_or(t, |p| p.max(t));
                self.starts.push(at);
            }
            self.finished = true;
        }
    }

    fn emit_through(&mut self, word: usize, at: f64) {
        while self.starts.len() <= word && self.starts.len() < self.words.len() {
            let floor = self.starts.last().map_or(0.0, |p| p + 0.03);
            self.starts.push(at.max(floor));
        }
    }

    fn decide(&mut self, k: usize) {
        let t_k = (k * self.hop) as f64 / self.rate;
        let at_sample = k * self.hop;
        // the sentence this hop is in: past a sentence's end, its words are all started and the next begins
        while let Some(s) = self.sents.get(self.cur) {
            match s.end {
                Some(e) if at_sample >= e => {
                    if s.syll.len() > 0 {
                        let last = s.first + s.syll.len() - 1;
                        let at = (e as f64 / self.rate - 0.05).max(s.start as f64 / self.rate);
                        self.emit_through(last, at);
                    }
                    self.cur += 1;
                    self.count = 0;
                    self.snapped = false;
                }
                _ => break,
            }
        }
        let Some(s) = self.sents.get(self.cur) else { return };
        if at_sample < s.start {
            return;
        }
        let (s_start, s_total, s_first) = (s.start, s.total, s.first);
        let e = self.env[k];
        self.run_max = self.run_max.max(self.env[k + AFTER]).max(e);
        self.dip = self.dip.min(e);
        // a pause: a clause's end within SNAP syllables takes the count there
        if e < self.run_max - SILENCE_DB {
            self.silence += 1;
            if self.silence == PAUSE_HOPS && !self.snapped && self.count > 0 {
                let c = self.count;
                let s = &self.sents[self.cur];
                let snap = s.clauses.iter().copied().filter(|&b| b != c && b.abs_diff(c) <= SNAP).min_by_key(|&b| b.abs_diff(c));
                if let Some(b) = snap {
                    // words missed before the pause: started now
                    let missed = (b > c).then(|| s.first + s.syll.iter().rposition(|&(before, _)| before < b).unwrap_or(0));
                    if let Some(gw) = missed {
                        self.emit_through(gw, t_k - LEAD_S);
                    }
                    self.count = b;
                    self.snapped = true;
                }
            }
        } else {
            if self.silence >= PAUSE_HOPS {
                self.snapped = false;
            }
            self.silence = 0;
        }
        // a nucleus at k
        let lo = k.saturating_sub(BEFORE);
        let peak = (lo..k).all(|j| self.env[j] <= e) && (k + 1..=k + AFTER).all(|j| self.env[j] < e);
        if peak && e > self.run_max - PEAK_DB && e - self.dip >= DIP_DB {
            if self.count < s_total {
                let c = self.count;
                let starts_word = self.sents[self.cur].syll.iter().position(|&(before, _)| before == c);
                if let Some(w) = starts_word {
                    self.emit_through(s_first + w, (t_k - LEAD_S).max(s_start as f64 / self.rate));
                }
                self.count += 1;
            }
            self.dip = e;
        }
    }

    /// Seconds into the line up to which `word_at` is decided (infinite once the line is done and heard through).
    pub fn decided_until(&self) -> f64 {
        if self.finished {
            return f64::INFINITY;
        }
        (self.k.saturating_sub(1) * self.hop) as f64 / self.rate - LEAD_S
    }

    /// The word being spoken `t` seconds into the line (the last one started by then), None before the first. Only
    /// meaningful for t up to decided_until().
    pub fn word_at(&self, t: f64) -> Option<usize> {
        let n = self.starts.partition_point(|&s| s <= t);
        n.checked_sub(1)
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    /// The loudness every 10 ms (for a cut placed in the quietest moment near a word's start).
    pub fn envelope(&self) -> (&[f32], f64) {
        (&self.env, HOP_S)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_syllables() {
        for (w, n) in [("the", 1), ("journey", 2), ("strange", 1), ("business", 3), ("standing", 2), ("table", 2), ("simply", 2), ("you", 1), ("anywhere", 3), ("time,", 1)] {
            assert_eq!(syllables(w), n, "{w}");
        }
    }

    /// three tone bursts a word, the clock finds each word's start near its burst whether fed at once or in pieces
    #[test]
    fn follows_bursts_and_is_prefix_stable() {
        let rate = 24_000u32;
        let mut audio = vec![0f32; rate as usize / 4];
        let mut truth = Vec::new();
        for _ in 0..4 {
            truth.push(audio.len() as f64 / rate as f64);
            for i in 0..(rate as usize / 6) {
                let t = i as f32 / rate as f32;
                let env = (std::f32::consts::PI * t * 6.0).sin().max(0.0);
                audio.push(0.3 * env * (2.0 * std::f32::consts::PI * 220.0 * t).sin());
            }
            audio.extend(std::iter::repeat_n(0.0, rate as usize / 8));
        }
        let text = "one two three four";
        let mut whole = Clock::new(rate);
        whole.begin_sentence(text, 0);
        whole.end_sentence(audio.len());
        whole.advance(&audio, true);
        assert_eq!(whole.starts.len(), 4);
        for (s, t) in whole.starts.iter().zip(&truth) {
            assert!((s - t).abs() < 0.12, "start {s} vs burst {t}");
        }
        let mut parts = Clock::new(rate);
        parts.begin_sentence(text, 0);
        let mut fed = 0;
        while fed < audio.len() {
            fed = (fed + 1234).min(audio.len());
            if fed == audio.len() {
                parts.end_sentence(audio.len());
            }
            parts.advance(&audio[..fed], fed == audio.len());
            // what's decided never changes later
            let until = parts.decided_until().min(audio.len() as f64 / rate as f64);
            for (i, s) in parts.starts.iter().enumerate() {
                if *s <= until {
                    assert_eq!(*s, whole.starts[i]);
                }
            }
        }
        assert_eq!(parts.starts, whole.starts);
    }
}
