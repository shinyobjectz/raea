//! The voice running: a listening thread (mic → Silero → Smart Turn → Parakeet → turns.rs) and a speaking thread
//! (lines → Pocket → speaker), sharing the speaker's level and a cancel flag so talking over the voice stops it at once.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;

use super::audio::{Mic, Resampler, Speaker};
use super::turns::{Step, Turns};
use super::{models, Parakeet, Pocket, SmartTurn, Vad};

#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// Loading, downloading, or what's wrong, in a few words.
    Status(String),
    /// Listening models loaded and the mic open.
    Listening(bool),
    /// Pocket loaded.
    VoiceReady,
    /// The mic's level (0-1), a few times a second.
    Level(f32),
    /// The words of the turn so far (at each pause).
    Hearing(String),
    /// A finished turn.
    Heard(String),
    /// Work done and how long it took, for the trace: ("voice.hear", Parakeet and Smart Turn at the last pause, the
    /// words) or ("voice.say", Pocket for one clause, its text).
    Measured(&'static str, std::time::Duration, String),
    /// The voice started or stopped talking.
    Speaking(bool),
    /// The user talked over the voice; it stopped.
    BargeIn,
}

/// What both threads share.
#[derive(Default)]
struct Shared {
    listening: AtomicBool,
    /// The voice has lines to say or audio still playing.
    voice_on: AtomicBool,
    cancel: AtomicBool,
    /// The speaker's output level, as f32 bits.
    out_level: AtomicU32,
    voice: Mutex<String>,
    /// The listening thread's input, for audio heard from a file (Voice::hear) beside or instead of the mic.
    inject: Mutex<Option<mpsc::Sender<Vec<f32>>>>,
}

enum Say {
    Line(String),
    Stop,
    /// The voice switched on (true) or off: a short chime.
    Chime(bool),
}

/// The app's handle on the voice.
#[derive(Clone)]
pub struct Voice {
    shared: Arc<Shared>,
    say: mpsc::Sender<Say>,
}

impl Voice {
    /// Starts both threads; models load (and download, the first time) in the background, reported as events.
    pub fn spawn(events: UnboundedSender<VoiceEvent>, voice: &str) -> Self {
        let shared = Arc::new(Shared::default());
        *shared.voice.lock().unwrap() = voice.to_string();
        let (say, lines) = mpsc::channel();
        {
            let (shared, events) = (shared.clone(), events.clone());
            std::thread::Builder::new().name("void-speak".into()).spawn(move || speak(shared, lines, events)).unwrap();
        }
        Self { shared, say }
    }

    /// Start listening (loads the listening models the first time).
    pub fn listen(&self, events: UnboundedSender<VoiceEvent>) {
        if self.shared.listening.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.shared.clone();
        std::thread::Builder::new().name("void-listen".into()).spawn(move || {
            if let Err(e) = listen(&shared, &events) {
                let _ = events.send(VoiceEvent::Status(format!("listening stopped: {e}")));
            }
            shared.listening.store(false, Ordering::SeqCst);
            let _ = events.send(VoiceEvent::Listening(false));
        }).unwrap();
    }

    pub fn mute(&self) {
        self.shared.listening.store(false, Ordering::SeqCst);
    }

    pub fn listening(&self) -> bool {
        self.shared.listening.load(Ordering::SeqCst)
    }

    /// One clause or sentence to say, after whatever is already queued.
    pub fn say(&self, text: &str) {
        let text = text.trim();
        if !text.is_empty() {
            self.shared.cancel.store(false, Ordering::SeqCst);
            let _ = self.say.send(Say::Line(text.to_string()));
        }
    }

    /// Stop talking now and drop what's queued.
    pub fn stop(&self) {
        self.shared.cancel.store(true, Ordering::SeqCst);
        let _ = self.say.send(Say::Stop);
    }

    /// Feed 16 kHz mono samples into listening at real time, as if the mic heard them (tests, the MCP `hear` tool).
    /// Starts listening if needed and returns once it has all been fed.
    pub fn hear(&self, samples: Vec<f32>, events: UnboundedSender<VoiceEvent>) -> anyhow::Result<()> {
        self.listen(events);
        // the listening models may still be loading (or downloading)
        let started = std::time::Instant::now();
        let tx = loop {
            if let Some(tx) = self.shared.inject.lock().unwrap().clone() {
                break tx;
            }
            anyhow::ensure!(self.listening(), "listening stopped before it was ready");
            anyhow::ensure!(started.elapsed() < Duration::from_secs(600), "listening never became ready");
            std::thread::sleep(Duration::from_millis(100));
        };
        let t0 = std::time::Instant::now();
        for (i, chunk) in samples.chunks(super::vad::CHUNK).enumerate() {
            tx.send(chunk.to_vec())?;
            let due = Duration::from_secs_f64((i + 1) as f64 * super::vad::CHUNK as f64 / super::LISTEN_RATE as f64);
            if let Some(wait) = due.checked_sub(t0.elapsed()) {
                std::thread::sleep(wait);
            }
        }
        Ok(())
    }

    /// A short, quiet chime for the voice switching on (rising) or off (falling).
    pub fn chime(&self, on: bool) {
        let _ = self.say.send(Say::Chime(on));
    }

    pub fn set_voice(&self, voice: &str) {
        *self.shared.voice.lock().unwrap() = voice.to_string();
    }
}

fn status(events: &UnboundedSender<VoiceEvent>, s: impl Into<String>) {
    let _ = events.send(VoiceEvent::Status(s.into()));
}

/// Loads one model's files, reporting downloads.
fn fetch(spec: &models::Spec, what: &str, events: &UnboundedSender<VoiceEvent>) -> anyhow::Result<std::path::PathBuf> {
    let mut last = 0u64;
    models::ensure(spec, &mut |file, n| {
        if n >= last + (8 << 20) {
            last = n;
            status(events, format!("Downloading {what} · {file} · {} MB", n >> 20));
        }
    })
}

fn listen(shared: &Shared, events: &UnboundedSender<VoiceEvent>) -> anyhow::Result<()> {
    status(events, "Waking the ears");
    let mut vad = Vad::load(&fetch(&Vad::SPEC, "Silero VAD", events)?)?;
    let mut turns = Turns::new();
    // the turn model and the recognizer: without Smart Turn the heuristic decides; without Parakeet nothing is heard
    let mut smart = match fetch(&SmartTurn::SPEC, "Smart Turn", events).and_then(|d| SmartTurn::load(&d)) {
        Ok(m) => Some(m),
        Err(e) => {
            status(events, format!("Smart Turn unavailable ({e}); pauses decide"));
            turns.heuristic = true;
            None
        }
    };
    let mut stt = Parakeet::load(&fetch(&Parakeet::SPEC, "Parakeet", events)?)?;
    let (tx, rx) = mpsc::channel();
    *shared.inject.lock().unwrap() = Some(tx.clone());
    // RAEA_VOID_NO_MIC=1: only what's fed through Voice::hear (a test hears exactly its recording)
    let mic = if std::env::var_os("RAEA_VOID_NO_MIC").is_some() { None } else { Some(Mic::start(tx)?) };
    status(events, format!("Listening · {}", mic.as_ref().map(|m| m.device.as_str()).unwrap_or("no mic (RAEA_VOID_NO_MIC)")));
    let _ = events.send(VoiceEvent::Listening(true));
    let mut pending: Vec<f32> = Vec::new();
    let mut level_tick = 0;
    let mut heard_in = std::time::Duration::ZERO;
    let mut voice_was = false;
    while shared.listening.load(Ordering::SeqCst) {
        let Ok(samples) = rx.recv_timeout(Duration::from_millis(200)) else { continue };
        pending.extend_from_slice(&samples);
        while pending.len() >= super::vad::CHUNK {
            let chunk: Vec<f32> = pending.drain(..super::vad::CHUNK).collect();
            let voice_on = shared.voice_on.load(Ordering::SeqCst);
            if voice_on != voice_was {
                turns.voice(voice_on);
                voice_was = voice_on;
            }
            let p = vad.prob(&chunk)?;
            let out_level = f32::from_bits(shared.out_level.load(Ordering::Relaxed));
            level_tick += 1;
            if level_tick % 3 == 0 {
                let _ = events.send(VoiceEvent::Level((super::audio::rms(&chunk) * 6.0).min(1.0)));
            }
            match turns.feed(p, &chunk, out_level) {
                Step::Nothing => {}
                Step::Started => {}
                Step::Paused => {
                    let t = std::time::Instant::now();
                    let words = stt.transcribe(turns.utterance()).unwrap_or_default();
                    let complete = smart.as_mut().and_then(|m| m.complete(turns.utterance()).ok());
                    heard_in = t.elapsed();
                    let _ = events.send(VoiceEvent::Hearing(words.clone()));
                    turns.judged(complete, words);
                }
                Step::Send => {
                    let _ = events.send(VoiceEvent::Measured("voice.hear", heard_in, turns.words().trim().to_string()));
                    let _ = events.send(VoiceEvent::Heard(turns.words().trim().to_string()));
                    turns.reset();
                }
                Step::BargeIn => {
                    shared.cancel.store(true, Ordering::SeqCst);
                    let _ = events.send(VoiceEvent::BargeIn);
                }
            }
        }
    }
    *shared.inject.lock().unwrap() = None;
    drop(mic);
    Ok(())
}

fn speak(shared: Arc<Shared>, lines: mpsc::Receiver<Say>, events: UnboundedSender<VoiceEvent>) {
    let speaker = match Speaker::start() {
        Ok(s) => s,
        Err(e) => return status(&events, format!("no speaker: {e}")),
    };
    // the model is fetched (downloaded the first time) now, but loaded only when voice is wanted: switching listening
    // on, or a first line to say. Loaded it holds ~210 MB (onnxruntime's arenas and prepacked weights), which a void
    // that never speaks carried from launch; switching listening off lets it go again.
    let dir = match fetch(&Pocket::SPEC, "Pocket TTS", &events) {
        Ok(d) => {
            let _ = events.send(VoiceEvent::VoiceReady);
            Some(d)
        }
        Err(e) => {
            status(&events, format!("voice unavailable: {e}"));
            None
        }
    };
    let mut pocket: Option<Pocket> = None;
    let load = |pocket: &mut Option<Pocket>| {
        if pocket.is_none()
            && let Some(d) = &dir
        {
            match Pocket::load(d) {
                Ok(p) => *pocket = Some(p),
                Err(e) => status(&events, format!("voice unavailable: {e}")),
            }
        }
    };
    let mut on = false;
    let set_on = |v: bool, on: &mut bool| {
        if *on != v {
            *on = v;
            shared.voice_on.store(v, Ordering::SeqCst);
            let _ = events.send(VoiceEvent::Speaking(v));
        }
    };
    loop {
        shared.out_level.store(speaker.level().to_bits(), Ordering::Relaxed);
        if shared.cancel.load(Ordering::SeqCst) {
            speaker.clear();
        }
        match lines.recv_timeout(Duration::from_millis(20)) {
            Ok(Say::Line(text)) => {
                load(&mut pocket);
                let Some(pocket) = pocket.as_mut() else { continue };
                if shared.cancel.load(Ordering::SeqCst) {
                    continue;
                }
                set_on(true, &mut on);
                let voice = shared.voice.lock().unwrap().clone();
                let mut rs = Resampler::new(pocket.sample_rate(), speaker.rate);
                let mut out = Vec::new();
                let t = std::time::Instant::now();
                let r = pocket.speak(&text, &voice, &shared.cancel, &mut |samples| {
                    out.clear();
                    rs.push(samples, &mut out);
                    speaker.push(&out);
                    shared.out_level.store(speaker.level().to_bits(), Ordering::Relaxed);
                    // Pocket renders at about twice real time: hold it once 2 s are queued, so a long reply doesn't
                    // pile up (and a barge-in has less to throw away)
                    while speaker.queued() > 2.0 && !shared.cancel.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(20));
                        shared.out_level.store(speaker.level().to_bits(), Ordering::Relaxed);
                    }
                });
                let _ = events.send(VoiceEvent::Measured("voice.say", t.elapsed(), text.clone()));
                if let Err(e) = r {
                    status(&events, format!("voice: {e}"));
                }
            }
            Ok(Say::Chime(on)) => {
                speaker.push(&chime(on, speaker.rate));
                if on {
                    load(&mut pocket);
                } else {
                    pocket = None;
                }
            }
            Ok(Say::Stop) => {
                speaker.clear();
                // drop lines queued before the stop
                while let Ok(Say::Line(_)) = lines.try_recv() {}
                set_on(false, &mut on);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if on && speaker.queued() <= 0.0 {
                    set_on(false, &mut on);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The chime: two soft sine notes a fifth apart (rising for on, falling for off), each with a quick attack and an
/// exponential tail, a breath of high shimmer on the first, about 0.2 s and -20 dB.
pub fn chime(on: bool, rate: u32) -> Vec<f32> {
    let r = rate as f32;
    let (a, b) = if on { (880.0f32, 1318.5f32) } else { (1318.5f32, 880.0f32) };
    let note = |f: f32, len: f32, gain: f32, out: &mut Vec<f32>, at: usize| {
        let n = (len * r) as usize;
        if out.len() < at + n {
            out.resize(at + n, 0.0);
        }
        for i in 0..n {
            let t = i as f32 / r;
            let env = (t / 0.004).min(1.0) * (-t / (len * 0.28)).exp();
            let tone = (2.0 * std::f32::consts::PI * f * t).sin() + 0.18 * (2.0 * std::f32::consts::PI * f * 2.0 * t).sin();
            out[at + i] += gain * env * tone;
        }
    };
    let mut out = Vec::new();
    note(a, 0.12, 0.09, &mut out, 0);
    note(b, 0.16, 0.08, &mut out, (0.07 * r) as usize);
    // shimmer: a quiet, fast-decaying high partial on the first note
    note(a * 3.0, 0.05, 0.015, &mut out, 0);
    out
}
