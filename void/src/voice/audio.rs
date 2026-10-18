//! Mic and speaker (cpal). The mic comes out as 16 kHz mono chunks; the speaker is a queue that takes any rate, can be
//! cleared at once (barge-in), and reports the level of what it played (the echo guard's reference).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// A streaming linear resampler, mono. Good enough for speech into 16 kHz models and 24 kHz speech out.
pub struct Resampler {
    step: f64,
    pos: f64,
    last: f32,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self { step: from as f64 / to as f64, pos: 0.0, last: 0.0 }
    }

    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if (self.step - 1.0).abs() < 1e-9 {
            out.extend_from_slice(input);
            return;
        }
        // positions count from the sample before input[0] (self.last) at 0; input[k] sits at k + 1
        let at = |k: usize| if k == 0 { self.last } else { input[k - 1] };
        while self.pos < input.len() as f64 {
            let i = self.pos.floor() as usize;
            let t = (self.pos - i as f64) as f32;
            out.push(at(i) + (at(i + 1) - at(i)) * t);
            self.pos += self.step;
        }
        self.pos -= input.len() as f64;
        if let Some(&l) = input.last() {
            self.last = l;
        }
    }
}

/// The default input device, as 16 kHz mono chunks sent to `tx`. The stream lives as long as the returned handle.
pub struct Mic {
    _stream: cpal::Stream,
    pub device: String,
}

impl Mic {
    pub fn start(tx: std::sync::mpsc::Sender<Vec<f32>>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow!("no microphone"))?;
        let name = device.name().unwrap_or_else(|_| "microphone".into());
        let config = device.default_input_config().context("mic config")?;
        let channels = config.channels() as usize;
        let rate = config.sample_rate().0;
        let mut rs = Resampler::new(rate, super::LISTEN_RATE);
        let mut mono = Vec::new();
        let mut out = Vec::new();
        let err = |e| eprintln!("void: mic: {e}");
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    mono.clear();
                    mono.extend(data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
                    out.clear();
                    rs.push(&mono, &mut out);
                    let _ = tx.send(out.clone());
                },
                err,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    mono.clear();
                    mono.extend(data.chunks(channels).map(|f| f.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / channels as f32));
                    out.clear();
                    rs.push(&mono, &mut out);
                    let _ = tx.send(out.clone());
                },
                err,
                None,
            )?,
            f => return Err(anyhow!("mic sample format {f:?}")),
        };
        stream.play()?;
        Ok(Self { _stream: stream, device: name })
    }
}

/// What the speaker shares with its callback.
#[derive(Default)]
struct Queue {
    samples: VecDeque<f32>,
}

/// The default output device, fed from a queue at the device's rate.
pub struct Speaker {
    _stream: cpal::Stream,
    queue: Arc<Mutex<Queue>>,
    /// RMS of the last callback's samples, as f32 bits.
    level: Arc<AtomicU32>,
    pub rate: u32,
}

impl Speaker {
    pub fn start() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("no speaker"))?;
        let config = device.default_output_config().context("speaker config")?;
        let channels = config.channels() as usize;
        let rate = config.sample_rate().0;
        let queue = Arc::new(Mutex::new(Queue::default()));
        let level = Arc::new(AtomicU32::new(0));
        let (q, lv) = (queue.clone(), level.clone());
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    let mut q = q.lock().unwrap();
                    let mut sum = 0.0f32;
                    for frame in data.chunks_mut(channels) {
                        let s = q.samples.pop_front().unwrap_or(0.0);
                        sum += s * s;
                        frame.fill(s);
                    }
                    lv.store((sum / (data.len() / channels).max(1) as f32).sqrt().to_bits(), Ordering::Relaxed);
                },
                |e| eprintln!("void: speaker: {e}"),
                None,
            )?,
            f => return Err(anyhow!("speaker sample format {f:?}")),
        };
        stream.play()?;
        Ok(Self { _stream: stream, queue, level, rate })
    }

    /// Queue mono samples already at `self.rate`.
    pub fn push(&self, samples: &[f32]) {
        self.queue.lock().unwrap().samples.extend(samples.iter().copied());
    }

    /// Drop everything not yet played (barge-in, or a new line replacing the old).
    pub fn clear(&self) {
        self.queue.lock().unwrap().samples.clear();
    }

    /// Seconds of audio still queued.
    pub fn queued(&self) -> f32 {
        self.queue.lock().unwrap().samples.len() as f32 / self.rate as f32
    }

    /// RMS of what's playing now.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

/// RMS of a chunk.
pub fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}
