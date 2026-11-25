//! The microphone and the speakers (cpal), or files in their place (`--from`, `--to`: a session with no devices, for
//! checks and for measuring).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// A stream's rate changed as it comes, by linear interpolation: speech in, and Pocket's 24 kHz out.
pub struct Resample {
    /// input samples per output sample
    step: f64,
    /// where the next output sample falls, in input samples from the start of the next piece (-1 is `last`)
    pos: f64,
    last: f32,
}

impl Resample {
    pub fn new(from: u32, to: u32) -> Resample {
        Resample { step: from as f64 / to as f64, pos: 0.0, last: 0.0 }
    }

    pub fn push(&mut self, x: &[f32], out: &mut Vec<f32>) {
        let Some(&end) = x.last() else { return };
        while self.pos + 1.0 < x.len() as f64 {
            let i = self.pos.floor();
            let a = if i < 0.0 { self.last } else { x[i as usize] };
            let b = x[(i + 1.0) as usize];
            out.push(a + (b - a) * (self.pos - i) as f32);
            self.pos += self.step;
        }
        self.pos -= x.len() as f64;
        self.last = end;
    }
}

/// Interleaved channels as one: each frame's mean.
fn mono(x: &[f32], ch: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(x.len() / ch + 1);
    for frame in x.chunks(ch) {
        let mut sum = 0.0;
        for s in frame {
            sum += s;
        }
        out.push(sum / frame.len() as f32);
    }
    out
}

/// A WAV's sound as mono samples and its rate: 16-bit PCM or 32-bit float, any channels.
pub fn read_wav(path: &Path) -> anyhow::Result<(Vec<f32>, u32)> {
    let b = std::fs::read(path).with_context(|| format!("{}", path.display()))?;
    anyhow::ensure!(b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WAVE", "{}: not a WAV file", path.display());
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let (mut at, mut fmt) = (12usize, None);
    while at + 8 <= b.len() {
        let (id, len) = (&b[at..at + 4], u32_at(at + 4) as usize);
        let body = at + 8;
        if id == b"fmt " && body + 16 <= b.len() {
            fmt = Some((u16_at(body), u16_at(body + 2) as usize, u32_at(body + 4), u16_at(body + 14)));
        } else if id == b"data" {
            let (kind, ch, rate, bits) = fmt.ok_or_else(|| anyhow!("{}: its sound comes before its format", path.display()))?;
            let data = &b[body..(body + len).min(b.len())];
            let one: Vec<f32> = match (kind, bits) {
                (1, 16) => data.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0).collect(),
                (3, 32) => data.chunks_exact(4).map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]])).collect(),
                _ => bail!("{}: a {bits}-bit WAV of kind {kind}; 16-bit PCM and 32-bit float are read", path.display()),
            };
            return Ok((mono(&one, ch.max(1)), rate));
        }
        at = body + len + (len & 1);
    }
    bail!("{}: a WAV with no sound in it", path.display())
}

/// Mono samples as a 16-bit WAV.
pub fn write_wav(path: &Path, x: &[f32], rate: u32) -> anyhow::Result<()> {
    let mut b = Vec::with_capacity(44 + x.len() * 2);
    let len = (x.len() * 2) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    for v in [16u32, 1 | (1 << 16), rate, rate * 2, 2 | (16 << 16)] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(b"data");
    b.extend_from_slice(&len.to_le_bytes());
    for s in x {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b).with_context(|| format!("{}", path.display()))
}

/// The microphone: 16 kHz mono in pieces down `tx` for as long as the stream it returns is kept.
pub fn microphone(tx: Sender<Vec<f32>>) -> anyhow::Result<cpal::Stream> {
    let dev = cpal::default_host().default_input_device().ok_or_else(|| anyhow!("this computer has no microphone to listen on"))?;
    let cfg = dev.default_input_config().map_err(|e| anyhow!("the microphone: {e}"))?;
    let ch = cfg.channels().max(1) as usize;
    let mut rs = Resample::new(cfg.sample_rate().0, crate::turns::RATE as u32);
    let mut take = move |mono: Vec<f32>| {
        let mut out = Vec::with_capacity(mono.len());
        rs.push(&mono, &mut out);
        let _ = tx.send(out);
    };
    let bad = |e| eprintln!("raea voice: the microphone: {e}");
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => {
            dev.build_input_stream(&cfg.config(), move |d: &[f32], _: &cpal::InputCallbackInfo| take(mono(d, ch)), bad, None)
        }
        cpal::SampleFormat::I16 => dev.build_input_stream(
            &cfg.config(),
            move |d: &[i16], _: &cpal::InputCallbackInfo| {
                let x: Vec<f32> = d.iter().map(|s| *s as f32 / 32768.0).collect();
                take(mono(&x, ch))
            },
            bad,
            None,
        ),
        f => bail!("the microphone gives {f:?} samples, which raea voice doesn't read"),
    }
    .map_err(|e| anyhow!("the microphone didn't open (is raea allowed to use it? System Settings > Privacy & Security > Microphone): {e}"))?;
    stream.play().map_err(|e| anyhow!("the microphone didn't start: {e}"))?;
    Ok(stream)
}

/// A WAV in the microphone's place: its sound at the pace it was recorded (at once with `fast`), a second and a half
/// of quiet after it so its last turn ends, and then the channel closes.
pub fn microphone_file(path: &Path, fast: bool, tx: Sender<Vec<f32>>) -> anyhow::Result<()> {
    let (x, rate) = read_wav(path)?;
    let mut heard = Vec::with_capacity(x.len());
    Resample::new(rate, crate::turns::RATE as u32).push(&x, &mut heard);
    heard.extend(std::iter::repeat_n(0.0, crate::turns::RATE * 3 / 2));
    std::thread::spawn(move || {
        // by the clock from its start, so a late wake-up is made up and it keeps a microphone's pace
        let (began, mut sent) = (Instant::now(), 0u64);
        for piece in heard.chunks(crate::turns::WIN) {
            if tx.send(piece.to_vec()).is_err() {
                return;
            }
            sent += piece.len() as u64;
            if !fast {
                std::thread::sleep((began + Duration::from_micros(1_000_000 * sent / crate::turns::RATE as u64)).saturating_duration_since(Instant::now()));
            }
        }
    });
    Ok(())
}

/// What is waiting to be played, shared with whoever makes the sound.
pub struct Line {
    pub queue: Mutex<VecDeque<f32>>,
    /// samples played so far, at the voice's rate
    pub played: AtomicU64,
    /// everything played, when it goes to a file
    kept: Mutex<Vec<f32>>,
    keep: AtomicBool,
    /// what went to the speakers, quiet included, for the echo to be taken out of the microphone (echo.rs): kept
    /// while `tap` is on, and taken by the session
    pub far: Mutex<Vec<f32>>,
    pub tap: AtomicBool,
}

impl Line {
    /// Drops what hasn't been played yet (the person spoke over it, or it was stopped).
    pub fn clear(&self) {
        self.queue.lock().unwrap().clear();
    }

    /// (samples played so far, samples still waiting to be)
    pub fn at(&self) -> (u64, usize) {
        (self.played.load(Ordering::Relaxed), self.queue.lock().unwrap().len())
    }

    /// The next `n` samples (fewer when the queue runs out), counted as played.
    fn take(&self, want: usize) -> Vec<f32> {
        let mut q = self.queue.lock().unwrap();
        let n = want.min(q.len());
        let got: Vec<f32> = q.drain(..n).collect();
        self.played.fetch_add(n as u64, Ordering::Relaxed);
        if self.keep.load(Ordering::Relaxed) {
            self.kept.lock().unwrap().extend(&got);
        }
        if self.tap.load(Ordering::Relaxed) && want < 1 << 20 {
            let mut far = self.far.lock().unwrap();
            far.extend(&got);
            far.extend(std::iter::repeat_n(0.0, want - n));
        }
        got
    }
}

/// A file in the speakers' place: what is waiting is taken at the pace it would play (all of it at once with `fast`).
fn drain(line: Arc<Line>, each: usize) {
    std::thread::spawn(move || {
        let (began, mut ticks) = (Instant::now(), 0u32);
        while Arc::strong_count(&line) > 1 {
            ticks += 1;
            std::thread::sleep((began + Duration::from_millis(20) * ticks).saturating_duration_since(Instant::now()));
            line.take(each);
        }
    });
}

/// The speakers, or a file in their place.
pub struct Speaker {
    pub line: Arc<Line>,
    rate: u32,
    to: Option<PathBuf>,
    _stream: Option<cpal::Stream>,
}

impl Speaker {
    /// `rate`: the voice's. With `to`, nothing sounds: what would have played is written there when the session ends.
    pub fn open(rate: u32, to: Option<PathBuf>, fast: bool) -> anyhow::Result<Speaker> {
        let line = Arc::new(Line { queue: Mutex::new(VecDeque::new()), played: AtomicU64::new(0), kept: Mutex::new(Vec::new()), keep: AtomicBool::new(to.is_some()), far: Mutex::new(Vec::new()), tap: AtomicBool::new(false) });
        if to.is_some() {
            drain(line.clone(), if fast { usize::MAX } else { rate as usize / 50 });
            return Ok(Speaker { line, rate, to, _stream: None });
        }
        let dev = cpal::default_host().default_output_device().ok_or_else(|| anyhow!("this computer has nothing to play sound on"))?;
        let cfg = dev.default_output_config().map_err(|e| anyhow!("the speakers: {e}"))?;
        anyhow::ensure!(cfg.sample_format() == cpal::SampleFormat::F32, "the speakers take {:?} samples, which raea voice doesn't write", cfg.sample_format());
        let ch = cfg.channels().max(1) as usize;
        let mut rs = Resample::new(rate, cfg.sample_rate().0);
        let need = rate as f64 / cfg.sample_rate().0 as f64;
        let l = line.clone();
        let mut ready: VecDeque<f32> = VecDeque::new();
        let play = move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let frames = out.len() / ch;
            if ready.len() < frames {
                let mut made = Vec::new();
                rs.push(&l.take(((frames - ready.len()) as f64 * need).ceil() as usize + 2), &mut made);
                ready.extend(made);
            }
            for f in out.chunks_mut(ch) {
                f.fill(ready.pop_front().unwrap_or(0.0));
            }
        };
        let stream = dev.build_output_stream(&cfg.config(), play, |e| eprintln!("raea voice: the speakers: {e}"), None).map_err(|e| anyhow!("the speakers didn't open: {e}"))?;
        stream.play().map_err(|e| anyhow!("the speakers didn't start: {e}"))?;
        Ok(Speaker { line, rate, to, _stream: Some(stream) })
    }

    /// Writes the file, when the session's sound went to one.
    pub fn close(&self) -> anyhow::Result<()> {
        match &self.to {
            Some(p) => write_wav(p, &self.line.kept.lock().unwrap(), self.rate),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_changes_piece_by_piece_as_it_would_whole() {
        let x: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut whole = Vec::new();
        Resample::new(48_000, 16_000).push(&x, &mut whole);
        let mut pieces = Vec::new();
        let mut rs = Resample::new(48_000, 16_000);
        for p in x.chunks(477) {
            rs.push(p, &mut pieces);
        }
        assert!((whole.len() as i64 - 1600).abs() <= 1, "a third as many samples: {}", whole.len());
        assert_eq!(whole.len(), pieces.len());
        assert!(whole.iter().zip(&pieces).all(|(a, b)| (a - b).abs() < 1e-5));
    }

    #[test]
    fn a_wav_written_reads_back() {
        let f = std::env::temp_dir().join(format!("raea-voice-{}.wav", std::process::id()));
        let x: Vec<f32> = (0..2400).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        write_wav(&f, &x, 24_000).unwrap();
        let (y, rate) = read_wav(&f).unwrap();
        std::fs::remove_file(&f).unwrap();
        assert_eq!((rate, y.len()), (24_000, x.len()));
        assert!(x.iter().zip(&y).all(|(a, b)| (a - b).abs() < 1e-3));
    }
}
