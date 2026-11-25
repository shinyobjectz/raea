//! The voice: what the agent says, said by Pocket TTS as its words arrive. Text comes in pieces (a model's stream);
//! each whole sentence is said as soon as it's there, so the first sound doesn't wait for the last word. The word
//! clock says when each word starts, for a caption that follows the voice.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use raea_sys_speech::pocket::{Out, Pocket};
use raea_sys_speech::words::Clock;
use serde_json::json;

use crate::audio::Line;

/// Pocket's rate.
pub const RATE: u32 = 24_000;
const TEMPERATURE: f32 = 0.7;

pub enum Say {
    /// more of what `id` says
    Text { id: String, text: String },
    /// `id` has all its words
    End(String),
    /// stop talking now
    Stop,
    /// say what's left, then finish
    Close,
}

/// The words being said and when each starts, for whoever follows the playback.
#[derive(Default)]
pub struct Words {
    pub id: String,
    pub words: Vec<String>,
    /// seconds into the utterance, for the words whose start is decided
    pub starts: Vec<f64>,
    /// how many have been told
    pub told: usize,
    /// the playback position (samples played) its first sample has
    pub start_at: u64,
}

/// The sentences `buf` holds whole, taken out of it: up to its last full stop, question or exclamation mark that a
/// space or a line end follows. With `all`, everything.
pub fn whole(buf: &mut String, all: bool) -> Option<String> {
    let cut = if all {
        buf.len()
    } else {
        let b = buf.as_bytes();
        let ends = |i: usize| matches!(b[i], b'.' | b'!' | b'?') && b.get(i + 1).is_some_and(|c| c.is_ascii_whitespace());
        (0..b.len()).rev().find(|i| ends(*i) || b[*i] == b'\n').map(|i| i + 1)?
    };
    let said: String = buf.drain(..cut).collect();
    (!said.trim().is_empty()).then(|| said.trim().to_string())
}

struct Utter {
    id: String,
    buf: String,
    ended: bool,
    line: Vec<f32>,
    clock: Clock,
    asked: Instant,
    sounded: bool,
}

struct Voice {
    pocket: Pocket,
    name: String,
    out: Arc<Line>,
    words: Arc<Mutex<Words>>,
    emit: fn(serde_json::Value),
    now: Option<Utter>,
    closing: bool,
    seed: u64,
}

impl Voice {
    fn take(&mut self, m: Say) {
        match m {
            Say::Text { id, text } => {
                if self.now.as_ref().is_none_or(|u| u.id != id) {
                    *self.words.lock().unwrap() = Words { id: id.clone(), ..Words::default() };
                    self.now = Some(Utter { id, buf: String::new(), ended: false, line: Vec::new(), clock: Clock::new(RATE), asked: Instant::now(), sounded: false });
                }
                self.now.as_mut().unwrap().buf.push_str(&text);
            }
            Say::End(id) => {
                if let Some(u) = self.now.as_mut().filter(|u| u.id == id) {
                    u.ended = true;
                }
            }
            Say::Stop => {
                self.out.clear();
                if let Some(u) = self.now.take() {
                    self.words.lock().unwrap().id.clear();
                    (self.emit)(json!({ "stopped": { "id": u.id } }));
                }
            }
            Say::Close => {
                self.closing = true;
                if let Some(u) = self.now.as_mut() {
                    u.ended = true;
                }
            }
        }
    }

    /// Everything waiting on the channel; false once the other end is gone.
    fn drain(&mut self, rx: &Receiver<Say>) -> bool {
        loop {
            match rx.try_recv() {
                Ok(m) => self.take(m),
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => {
                    self.take(Say::Close);
                    return false;
                }
            }
        }
    }

    /// Says `text` for the utterance `id`; stops early when that utterance is stopped or replaced.
    fn say(&mut self, id: &str, text: &str, rx: &Receiver<Say>) -> anyhow::Result<()> {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut g = self.pocket.begin(text, &self.name, self.seed, TEMPERATURE)?;
        while let Some(out) = self.pocket.step(&mut g)? {
            let Some(u) = self.now.as_mut().filter(|u| u.id == id) else { return Ok(()) };
            match out {
                Out::Begin(sentence) => u.clock.begin_sentence(&sentence, u.line.len()),
                Out::End => u.clock.end_sentence(u.line.len()),
                Out::Audio(a) => {
                    if !u.sounded {
                        u.sounded = true;
                        let (played, waiting) = self.out.at();
                        self.words.lock().unwrap().start_at = played + waiting as u64;
                        (self.emit)(json!({ "saying": { "id": id, "first_ms": u.asked.elapsed().as_millis() as u64 } }));
                    }
                    self.out.queue.lock().unwrap().extend(&a);
                    u.line.extend_from_slice(&a);
                    u.clock.advance(&u.line, false);
                    self.publish();
                }
            }
            self.drain(rx);
        }
        Ok(())
    }

    fn publish(&mut self) {
        let Some(u) = &self.now else { return };
        let mut w = self.words.lock().unwrap();
        if w.id == u.id && (w.words.len() != u.clock.words.len() || w.starts.len() != u.clock.starts.len()) {
            w.words.clone_from(&u.clock.words);
            w.starts.clone_from(&u.clock.starts);
        }
    }
}

/// The voice's thread: loads Pocket, says `ready` once it can speak, then says what comes down `rx` until it closes.
/// `busy` is up while an utterance is being made or played.
pub fn worker(rx: Receiver<Say>, out: Arc<Line>, words: Arc<Mutex<Words>>, name: Option<String>, busy: Arc<AtomicBool>, ready: std::sync::mpsc::Sender<anyhow::Result<String>>, emit: fn(serde_json::Value)) {
    let loaded = (|| {
        let dir = raea_sys_ml::models::ensure(&Pocket::SPEC, &mut crate::fetching(emit))?;
        let mut pocket = Pocket::load(&dir)?;
        anyhow::ensure!(pocket.sample_rate() == RATE, "Pocket TTS speaks at {} Hz, not {RATE}", pocket.sample_rate());
        // Pocket TTS's own first voice, never a clip of somebody found on this computer (the library's default
        // takes one when it's there): a session speaks in a voice of someone's only when asked to by name
        let name = name.unwrap_or_else(|| Pocket::VOICES[0].to_string());
        // the voice's state loaded now, not under the first words
        pocket.begin("", &name, 1, TEMPERATURE)?;
        Ok((pocket, name))
    })();
    let (pocket, name) = match loaded {
        Ok(p) => p,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(name.clone()));
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1;
    let mut v = Voice { pocket, name, out, words, emit, now: None, closing: false, seed };
    loop {
        busy.store(v.now.is_some() || v.out.at().1 > 0, Ordering::Relaxed);
        let Some(u) = v.now.as_mut() else {
            if v.closing {
                return;
            }
            match rx.recv() {
                Ok(m) => v.take(m),
                Err(_) => return,
            }
            continue;
        };
        let ended = u.ended;
        if let Some(text) = whole(&mut u.buf, ended) {
            let id = u.id.clone();
            if let Err(e) = v.say(&id, &text, &rx) {
                emit(json!({ "error": format!("Pocket TTS: {e:#}") }));
                v.take(Say::Stop);
            }
            continue;
        }
        if ended {
            // every word made: the clock decides the last starts, and the utterance is said once it has all played
            u.clock.advance(&u.line, true);
            let id = u.id.clone();
            v.publish();
            while v.out.at().1 > 0 && v.now.as_ref().is_some_and(|u| u.id == id) {
                std::thread::sleep(Duration::from_millis(15));
                v.drain(&rx);
            }
            if v.now.as_ref().is_some_and(|u| u.id == id) {
                v.now = None;
                emit(json!({ "said": { "id": id } }));
            }
            continue;
        }
        // waiting for the rest of a sentence
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(m) => v.take(m),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => v.take(Say::Close),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_sentences_are_said_as_they_arrive() {
        let mut buf = String::from("Got it. A launch film for the Ridge 2");
        assert_eq!(whole(&mut buf, false).as_deref(), Some("Got it."));
        assert_eq!(whole(&mut buf, false), None, "half a sentence waits for its end");
        buf.push_str(", gritty. Who is it for");
        assert_eq!(whole(&mut buf, false).as_deref(), Some("A launch film for the Ridge 2, gritty."));
        assert_eq!(whole(&mut buf, true).as_deref(), Some("Who is it for"), "with everything in, what's left is said");
        assert_eq!(whole(&mut buf, true), None);
    }

    #[test]
    fn a_number_with_a_point_is_not_a_sentence_end() {
        let mut buf = String::from("It costs 2.5 times more");
        assert_eq!(whole(&mut buf, false), None);
    }
}
