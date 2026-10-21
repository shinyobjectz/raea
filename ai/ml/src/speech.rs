//! Speech for games and renders: lines said by Pocket TTS on one worker thread, streamed into the frames that play
//! them, with a word clock (words.rs) a game can read back.
//!
//! A line is named by what it says: its text, voice, seed and temperature. Pocket with a fixed seed says the same text
//! the same way every time, so the line's samples, the moment each of its words starts and where a cut lands are all
//! functions of that name, not of when the audio happened to be made. Live play and a render differ only in waiting:
//! a render waits for every sample it needs; live play asks for a line early (the game's `start` a moment after its
//! request) and only waits if the worker hasn't got that far by then.
//!
//! The worker says one piece (160 ms of audio, or a sentence's start) at a time, always of the line with the least
//! audio made beyond where it's needed, so a line that interrupts gets ahead of a long one already well ahead.
//!
//! Finished lines are kept on disk (~/.cache/raea-speech, RAEA_SPEECH_CACHE): a render of a play reads back the very
//! samples the play made, and a line said once is ready at once the next time.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::pocket::{Out, Pocket};
use crate::words::{Clock, LOOKAHEAD_S};

/// Pocket's output rate.
pub const RATE: u32 = 24_000;
/// What a line's name covers beyond its words: the model bundle and the way it's said (bump when either changes).
const VERSION: &str = "pocket-english_2026-04/1";
/// A cut's fade (seconds), ending at the cut.
const CUT_FADE_S: f64 = 0.025;
/// A cut waits at most this long for the next word.
const CUT_MAX_S: f64 = 0.8;

thread_local! {
    static BACKGROUND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Marks this thread's waits as background work: the worker serves them only when nothing live is waiting (a game's
/// master, worked out by a second React on a thread while the game plays).
pub fn set_background(on: bool) {
    BACKGROUND.with(|b| b.set(on));
}

/// What to say.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub text: String,
    pub voice: String,
    pub seed: u64,
    pub temperature: f32,
}

impl Spec {
    /// The line's name: FNV-1a over everything that changes its samples.
    pub fn key(&self) -> String {
        let mut h: u64 = 0xcbf29ce484222325;
        for part in [VERSION, &self.text, &self.voice, &self.seed.to_string(), &format!("{:.4}", self.temperature)] {
            for b in part.bytes().chain([0u8]) {
                h = (h ^ b as u64).wrapping_mul(0x100000001b3);
            }
        }
        format!("{h:016x}")
    }
}

/// A line as far as it's been made.
pub struct Line {
    pub spec: Spec,
    pub audio: Vec<f32>,
    pub clock: Clock,
    pub done: bool,
    pub error: Option<String>,
    /// how far into the line (samples) something has asked for: the worker's priority
    pub need: usize,
    /// the same for background work (a game's master worked out on a thread): served only when nothing's needed now
    pub bg_need: usize,
    /// for the report: when it was asked for, when its first audio came, the time spent making it
    pub asked: Instant,
    pub first_audio: Option<Duration>,
    pub made_in: Duration,
    pub from_cache: bool,
    /// sentences as said: (text, start sample, end sample)
    sentences: Vec<(String, usize, usize)>,
}

pub struct Slot {
    pub key: String,
    line: Mutex<Line>,
    grew: Condvar,
}

impl Slot {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Line> {
        self.line.lock().unwrap()
    }
}

#[derive(Default)]
struct Shared {
    lines: HashMap<String, Arc<Slot>>,
    /// lines the worker still has to finish, in the order asked
    todo: Vec<Arc<Slot>>,
    status: Option<String>,
    /// Pocket asked to load ahead of the first line
    warm: bool,
}

/// The waits the frames made on the worker, for the report: (count, total, longest).
#[derive(Default, Clone, Copy, Debug)]
pub struct Waits {
    pub count: u64,
    pub total: Duration,
    pub longest: Duration,
}

pub struct Service {
    shared: Mutex<Shared>,
    work: Condvar,
    waits: Mutex<Waits>,
}

/// The process's speech service (its worker starts, and Pocket loads, on first use).
pub fn service() -> &'static Arc<Service> {
    static S: OnceLock<Arc<Service>> = OnceLock::new();
    S.get_or_init(|| {
        let s = Arc::new(Service { shared: Mutex::new(Shared::default()), work: Condvar::new(), waits: Mutex::new(Waits::default()) });
        let w = s.clone();
        std::thread::Builder::new().name("raea-speech".into()).spawn(move || w.worker()).expect("speech worker");
        s
    })
}

fn cache_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("RAEA_SPEECH_CACHE") {
        return PathBuf::from(p);
    }
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join(".cache/raea-speech")
}

impl Service {
    /// The line for `spec`: the one already asked for, one read back from the cache, or a new one for the worker.
    pub fn line(&self, spec: &Spec) -> Arc<Slot> {
        let key = spec.key();
        let mut sh = self.shared.lock().unwrap();
        if let Some(s) = sh.lines.get(&key) {
            return s.clone();
        }
        let mut line = Line { spec: spec.clone(), audio: Vec::new(), clock: Clock::new(RATE), done: false, error: None, need: 0, bg_need: 0, asked: Instant::now(), first_audio: None, made_in: Duration::ZERO, from_cache: false, sentences: Vec::new() };
        if let Some((audio, sentences)) = read_cache(&key) {
            for (text, a, b) in &sentences {
                line.clock.begin_sentence(text, *a);
                line.clock.end_sentence(*b);
            }
            line.clock.advance(&audio, true);
            line.audio = audio;
            line.sentences = sentences;
            line.done = true;
            line.from_cache = true;
            line.first_audio = Some(Duration::ZERO);
        }
        let done = line.done;
        let slot = Arc::new(Slot { key: key.clone(), line: Mutex::new(line), grew: Condvar::new() });
        sh.lines.insert(key, slot.clone());
        if !done {
            sh.todo.push(slot.clone());
            self.work.notify_one();
        }
        slot
    }

    /// What the worker is doing, if anything's wrong or loading.
    pub fn status(&self) -> Option<String> {
        self.shared.lock().unwrap().status.clone()
    }

    /// Every line asked for, for the report.
    pub fn lines(&self) -> Vec<Arc<Slot>> {
        self.shared.lock().unwrap().lines.values().cloned().collect()
    }

    pub fn waits(&self) -> Waits {
        *self.waits.lock().unwrap()
    }

    /// Waits until `ok(line)` holds or the line is done (or failed). Counted as a wait when it had to wait at all.
    fn wait_until<'a>(&self, slot: &'a Slot, need: usize, ok: impl Fn(&Line) -> bool) -> std::sync::MutexGuard<'a, Line> {
        let mut l = slot.line.lock().unwrap();
        if BACKGROUND.with(|b| b.get()) {
            l.bg_need = l.bg_need.max(need);
        } else if l.need < need {
            l.need = need;
        }
        if ok(&l) || l.done {
            return l;
        }
        self.work.notify_one();
        let t = Instant::now();
        while !ok(&l) && !l.done {
            l = slot.grew.wait(l).unwrap();
        }
        let d = t.elapsed();
        let mut w = self.waits.lock().unwrap();
        w.count += 1;
        w.total += d;
        w.longest = w.longest.max(d);
        l
    }

    /// Which word is being spoken `t` seconds into the line (None before the first), whether the line has ended by
    /// then, and its words. Waits until that's decided: the same answer in a render and in live play.
    pub fn word_at(&self, slot: &Slot, t: f64) -> (Option<usize>, bool, Vec<String>) {
        let need = ((t + LOOKAHEAD_S).max(0.0) * RATE as f64) as usize;
        let l = self.wait_until(slot, need, |l| l.clock.decided_until() > t);
        let ended = l.done && t >= l.audio.len() as f64 / RATE as f64;
        (l.clock.word_at(t), ended, l.clock.words.clone())
    }

    /// Where a line cut at `t` seconds stops (seconds into it): the quietest moment near the start of the next word
    /// after t (at most CUT_MAX_S on), or the line's end. Waits until that's decided.
    pub fn cut_at(&self, slot: &Slot, t: f64) -> f64 {
        let t = t.max(0.0);
        let need = ((t + CUT_MAX_S + LOOKAHEAD_S) * RATE as f64) as usize;
        let l = self.wait_until(slot, need, |l| l.clock.starts.iter().any(|&s| s > t) || l.clock.decided_until() > t + CUT_MAX_S);
        let len = l.audio.len() as f64 / RATE as f64;
        if t >= len && l.done {
            return len;
        }
        let next = l.clock.starts.iter().copied().find(|&s| s > t);
        let target = match next {
            Some(s) if s <= t + CUT_MAX_S => s,
            _ => (t + CUT_MAX_S).min(if l.done { len } else { f64::INFINITY }),
        };
        // the quietest 10 ms from 120 ms before the word's start to 40 ms after (not before the cut was asked), once
        // the loudness reaches 40 ms after it: searched short in live play, the cut would land elsewhere than in a
        // render (sdk/react/bench/speech/check.sh, a play cut by a random hand)
        let want = ((target + 0.04) / l.clock.envelope().1).ceil() as usize;
        let l = if l.clock.envelope().0.len() > want || l.done {
            l
        } else {
            drop(l);
            let need = (((target + 0.04) * RATE as f64) as usize).saturating_add((0.05 * RATE as f64) as usize);
            self.wait_until(slot, need, |l| l.clock.envelope().0.len() > want)
        };
        let len = l.audio.len() as f64 / RATE as f64;
        let (env, hop) = l.clock.envelope();
        let lo = ((target - 0.12).max(t) / hop).floor() as usize;
        let hi = (((target + 0.04) / hop).ceil() as usize).min(env.len().saturating_sub(1));
        let mut best = (target, f32::INFINITY);
        for j in lo..=hi.max(lo) {
            if let Some(&e) = env.get(j) {
                // ties go to the earlier moment
                if e < best.1 {
                    best = (j as f64 * hop + 0.01, e);
                }
            }
        }
        best.0.max(t).min(if l.done { len } else { f64::INFINITY })
    }

    /// The line's sound for output samples [from, from + n) at `out_rate` (its own samples resampled), each × gain,
    /// faded out to nothing at `cut` seconds when there's a cut. Waits for the samples it needs. Zeros past the end.
    pub fn render(&self, slot: &Slot, out_rate: u32, from: usize, n: usize, cut: Option<f64>, gain: f32, out: &mut [f32]) {
        let r = RATE as f64 / out_rate as f64;
        let last = ((from + n) as f64 * r).ceil() as usize + 2;
        let l = self.wait_until(slot, last, |l| l.audio.len() >= last);
        let a = &l.audio;
        let at = |i: isize| -> f32 { if i < 0 || i as usize >= a.len() { 0.0 } else { a[i as usize] } };
        for (k, o) in out.iter_mut().enumerate().take(n) {
            let m = from + k;
            let x = m as f64 * r;
            let t = m as f64 / out_rate as f64;
            let g = match cut {
                Some(c) if t >= c => 0.0,
                Some(c) if t > c - CUT_FADE_S => ((c - t) / CUT_FADE_S) as f32,
                _ => 1.0,
            };
            if g == 0.0 {
                continue;
            }
            // Catmull-Rom between the two samples around x
            let i = x.floor() as isize;
            let f = (x - i as f64) as f32;
            let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            let v = p1 + 0.5 * f * (p2 - p0 + f * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + f * (3.0 * (p1 - p2) + p3 - p0)));
            *o += v * g * gain;
        }
    }

    /// The line's length in seconds once it's all made (waits for it).
    pub fn duration(&self, slot: &Slot) -> f64 {
        let l = self.wait_until(slot, usize::MAX, |l| l.done);
        l.audio.len() as f64 / RATE as f64
    }

    /// Loads Pocket now (and the default voice), so the first line doesn't wait for it.
    pub fn warm(&self) {
        self.shared.lock().unwrap().warm = true;
        self.work.notify_one();
    }

    fn worker(&self) {
        let mut pocket: Option<Pocket> = None;
        let mut gens: HashMap<String, crate::pocket::Gen> = HashMap::new();
        loop {
            // the line with the least made beyond where it's needed
            let slot = {
                let mut sh = self.shared.lock().unwrap();
                loop {
                    sh.todo.retain(|s| !s.line.lock().unwrap().done);
                    // what play needs now first (the least made beyond its need), then background work, then the rest
                    let pick = sh.todo.iter().min_by_key(|s| {
                        let l = s.line.lock().unwrap();
                        let made = l.audio.len() as i64;
                        let ahead = made - l.need as i64;
                        if ahead < 0 { (0, ahead) } else if made < l.bg_need as i64 { (1, made - l.bg_need as i64) } else { (2, ahead) }
                    });
                    if let Some(s) = pick {
                        break s.clone();
                    }
                    if std::mem::take(&mut sh.warm) {
                        drop(sh);
                        if pocket.is_none() {
                            pocket = load_pocket(self);
                        }
                        if let Some(p) = pocket.as_mut() {
                            let voice = Pocket::default_voice();
                            let _ = p.speak("Okay.", &voice, &std::sync::atomic::AtomicBool::new(false), &mut |_| {});
                        }
                        sh = self.shared.lock().unwrap();
                        continue;
                    }
                    sh = self.work.wait(sh).unwrap();
                }
            };
            if pocket.is_none() {
                pocket = load_pocket(self);
            }
            let fail = |e: String| {
                let mut l = slot.line.lock().unwrap();
                l.error = Some(e.clone());
                l.done = true;
                let audio = std::mem::take(&mut l.audio);
                l.clock.advance(&audio, true);
                l.audio = audio;
                drop(l);
                slot.grew.notify_all();
                self.shared.lock().unwrap().status = Some(e);
            };
            let Some(p) = pocket.as_mut() else {
                fail("Pocket TTS couldn't load".into());
                continue;
            };
            let t = Instant::now();
            if !gens.contains_key(&slot.key) {
                let spec = slot.line.lock().unwrap().spec.clone();
                match p.begin(&spec.text, &spec.voice, spec.seed, spec.temperature) {
                    Ok(g) => {
                        gens.insert(slot.key.clone(), g);
                    }
                    Err(e) => {
                        fail(format!("speech: {e:#}"));
                        continue;
                    }
                }
            }
            let g = gens.get_mut(&slot.key).unwrap();
            let step = p.step(g);
            let mut l = slot.line.lock().unwrap();
            l.made_in += t.elapsed();
            match step {
                Ok(Some(Out::Begin(text))) => {
                    let at = l.audio.len();
                    l.clock.begin_sentence(&text, at);
                    l.sentences.push((text, at, at));
                }
                Ok(Some(Out::Audio(a))) => {
                    if l.first_audio.is_none() {
                        l.first_audio = Some(l.asked.elapsed());
                    }
                    l.audio.extend_from_slice(&a);
                    let Line { audio, clock, .. } = &mut *l;
                    clock.advance(audio, false);
                }
                Ok(Some(Out::End)) => {
                    let at = l.audio.len();
                    l.clock.end_sentence(at);
                    if let Some(s) = l.sentences.last_mut() {
                        s.2 = at;
                    }
                }
                Ok(None) => {
                    l.done = true;
                    let Line { audio, clock, .. } = &mut *l;
                    clock.advance(audio, true);
                    gens.remove(&slot.key);
                    write_cache(&slot.key, &l.audio, &l.sentences);
                }
                Err(e) => {
                    drop(l);
                    gens.remove(&slot.key);
                    fail(format!("speech: {e:#}"));
                    continue;
                }
            }
            drop(l);
            slot.grew.notify_all();
        }
    }
}

fn load_pocket(s: &Service) -> Option<Pocket> {
    let r = crate::models::ensure(&Pocket::SPEC, &mut |_, _| {}).and_then(|d| Pocket::load(&d));
    match r {
        Ok(p) => Some(p),
        Err(e) => {
            s.shared.lock().unwrap().status = Some(format!("Pocket TTS: {e:#}"));
            eprintln!("raea speech: Pocket TTS unavailable: {e:#}");
            None
        }
    }
}

/// A finished line kept: its samples (f32 LE) and its sentences (JSON).
fn write_cache(key: &str, audio: &[f32], sentences: &[(String, usize, usize)]) {
    if std::env::var("RAEA_SPEECH_CACHE").as_deref() == Ok("off") {
        return;
    }
    let dir = cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let bytes: Vec<u8> = audio.iter().flat_map(|x| x.to_le_bytes()).collect();
    let meta = serde_json::json!({"version": VERSION, "sentences": sentences.iter().map(|(t, a, b)| serde_json::json!([t, a, b])).collect::<Vec<_>>()});
    let part = dir.join(format!("{key}.{}.part", std::process::id()));
    if std::fs::write(&part, bytes).is_ok() {
        let _ = std::fs::write(dir.join(format!("{key}.json")), meta.to_string());
        let _ = std::fs::rename(&part, dir.join(format!("{key}.f32")));
    }
}

fn read_cache(key: &str) -> Option<(Vec<f32>, Vec<(String, usize, usize)>)> {
    if std::env::var("RAEA_SPEECH_CACHE").as_deref() == Ok("off") {
        return None;
    }
    let dir = cache_dir();
    let bytes = std::fs::read(dir.join(format!("{key}.f32"))).ok()?;
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{key}.json"))).ok()?).ok()?;
    let sentences = meta["sentences"].as_array()?.iter().filter_map(|s| Some((s[0].as_str()?.to_string(), s[1].as_u64()? as usize, s[2].as_u64()? as usize))).collect();
    Some((bytes.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect(), sentences))
}
