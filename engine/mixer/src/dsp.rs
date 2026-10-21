//! Deterministic DSP building blocks for Tone's instruments and effects: oscillators (band-limited by polyBLEP),
//! envelopes, biquads, delay lines, LFOs and seeded noise. All f32/f64 arithmetic in a fixed order, so the same input
//! gives the same samples live and offline.

use std::f64::consts::{PI, TAU};

/// Seeded white noise (xorshift64*), -1..1.
#[derive(Clone)]
pub struct Noise {
    s: u64,
    pink: [f32; 7],
    brown: f32,
}

impl Noise {
    pub fn new(seed: u64) -> Noise {
        Noise { s: seed.wrapping_mul(0x9e3779b97f4a7c15) | 1, pink: [0.0; 7], brown: 0.0 }
    }
    pub fn white(&mut self) -> f32 {
        self.s ^= self.s >> 12;
        self.s ^= self.s << 25;
        self.s ^= self.s >> 27;
        let x = self.s.wrapping_mul(0x2545F4914F6CDD1D);
        ((x >> 40) as f32 / (1u64 << 23) as f32) - 1.0
    }
    /// Paul Kellet's pink filter
    pub fn pink(&mut self) -> f32 {
        let w = self.white();
        let b = &mut self.pink;
        b[0] = 0.99886 * b[0] + w * 0.0555179;
        b[1] = 0.99332 * b[1] + w * 0.0750759;
        b[2] = 0.96900 * b[2] + w * 0.1538520;
        b[3] = 0.86650 * b[3] + w * 0.3104856;
        b[4] = 0.55000 * b[4] + w * 0.5329522;
        b[5] = -0.7616 * b[5] - w * 0.0168980;
        let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362;
        b[6] = w * 0.115926;
        out * 0.11
    }
    pub fn brown(&mut self) -> f32 {
        let w = self.white();
        self.brown = (self.brown + 0.02 * w) / 1.02;
        self.brown * 3.5
    }
    pub fn of(&mut self, kind: &str) -> f32 {
        match kind {
            "pink" => self.pink(),
            "brown" => self.brown(),
            _ => self.white(),
        }
    }
}

fn blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// One oscillator: Tone's basic types (sine, square, triangle, sawtooth, pulse with width), phase 0..1.
#[derive(Clone, Copy)]
pub struct Osc {
    pub phase: f64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Wave {
    Sine,
    Square,
    Triangle,
    Saw,
    Pulse(f64),
}

impl Wave {
    pub fn parse(s: &str) -> Wave {
        let base = s.trim_start_matches("fat").trim_start_matches("fm").trim_start_matches("am");
        let base = base.trim_end_matches(|c: char| c.is_ascii_digit());
        match base {
            "square" => Wave::Square,
            "triangle" => Wave::Triangle,
            "sawtooth" | "saw" => Wave::Saw,
            "pulse" => Wave::Pulse(0.25),
            _ => Wave::Sine,
        }
    }
}

impl Osc {
    pub fn new(phase: f64) -> Osc {
        Osc { phase }
    }
    pub fn next(&mut self, wave: Wave, freq: f64, rate: f64) -> f32 {
        let dt = (freq / rate).clamp(0.0, 0.5);
        let p = self.phase;
        let v = match wave {
            Wave::Sine => (p * TAU).sin(),
            Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
            Wave::Saw => 2.0 * p - 1.0 - blep(p, dt),
            Wave::Square => {
                let mut v = if p < 0.5 { 1.0 } else { -1.0 };
                v += blep(p, dt);
                v -= blep((p + 0.5) % 1.0, dt);
                v
            }
            Wave::Pulse(w) => {
                let mut v = if p < w { 1.0 } else { -1.0 };
                v += blep(p, dt);
                v -= blep((p + 1.0 - w) % 1.0, dt);
                v
            }
        };
        self.phase = (p + freq / rate).rem_euclid(1.0);
        v as f32
    }
}

/// Tone's Envelope (attack, decay, sustain, release; linear attack, exponential decay and release as Tone's defaults
/// approximate them). `gate` on at attack, off at release; the level carries over a retrigger.
#[derive(Clone, Copy, Debug)]
pub struct Adsr {
    pub a: f64,
    pub d: f64,
    pub s: f64,
    pub r: f64,
}

impl Default for Adsr {
    fn default() -> Self {
        Adsr { a: 0.005, d: 0.1, s: 0.3, r: 1.0 }
    }
}

#[derive(Clone, Copy)]
pub struct Env {
    pub level: f64,
    stage: u8, // 0 idle, 1 attack, 2 decay/sustain, 3 release
    from: f64,
    t: f64,
}

impl Default for Env {
    fn default() -> Self {
        Env { level: 0.0, stage: 0, from: 0.0, t: 0.0 }
    }
}

impl Env {
    pub fn attack(&mut self) {
        self.stage = 1;
        self.from = self.level;
        self.t = 0.0;
    }
    pub fn release(&mut self) {
        if self.stage != 0 {
            self.stage = 3;
            self.from = self.level;
            self.t = 0.0;
        }
    }
    pub fn idle(&self) -> bool {
        self.stage == 0
    }
    pub fn releasing(&self) -> bool {
        self.stage == 3
    }
    pub fn next(&mut self, e: &Adsr, rate: f64) -> f64 {
        let dt = 1.0 / rate;
        match self.stage {
            1 => {
                let a = e.a.max(1e-4);
                self.level = self.from + (1.0 - self.from) * (self.t / a).min(1.0);
                self.t += dt;
                if self.t >= a {
                    self.stage = 2;
                    self.t = 0.0;
                    self.from = 1.0;
                }
            }
            2 => {
                // exponential approach to sustain over the decay time (Tone's exponential decay)
                let d = e.d.max(1e-4);
                let k = (-self.t / (d / 4.0)).exp();
                self.level = e.s + (1.0 - e.s) * k;
                self.t += dt;
            }
            3 => {
                let r = e.r.max(1e-4);
                let k = (-self.t / (r / 5.0)).exp();
                self.level = self.from * k;
                self.t += dt;
                if self.t >= r {
                    self.level = 0.0;
                    self.stage = 0;
                }
            }
            _ => self.level = 0.0,
        }
        self.level
    }
}

/// A biquad (RBJ cookbook), direct form I, coefficients set per block or per sample.
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FilterType {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
    Allpass,
    Peaking,
    Lowshelf,
    Highshelf,
}

impl FilterType {
    pub fn parse(s: &str) -> FilterType {
        match s {
            "highpass" => FilterType::Highpass,
            "bandpass" => FilterType::Bandpass,
            "notch" => FilterType::Notch,
            "allpass" => FilterType::Allpass,
            "peaking" => FilterType::Peaking,
            "lowshelf" => FilterType::Lowshelf,
            "highshelf" => FilterType::Highshelf,
            _ => FilterType::Lowpass,
        }
    }
}

impl Biquad {
    pub fn set(&mut self, t: FilterType, freq: f64, q: f64, gain_db: f64, rate: f64) {
        let f = freq.clamp(10.0, rate * 0.49);
        let w = TAU * f / rate;
        let (sw, cw) = (w.sin(), w.cos());
        let q = q.max(1e-4);
        let alpha = sw / (2.0 * q);
        let a_ = 10f64.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match t {
            FilterType::Lowpass => ((1.0 - cw) / 2.0, 1.0 - cw, (1.0 - cw) / 2.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::Highpass => ((1.0 + cw) / 2.0, -(1.0 + cw), (1.0 + cw) / 2.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::Bandpass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::Notch => (1.0, -2.0 * cw, 1.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::Allpass => (1.0 - alpha, -2.0 * cw, 1.0 + alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::Peaking => (1.0 + alpha * a_, -2.0 * cw, 1.0 - alpha * a_, 1.0 + alpha / a_, -2.0 * cw, 1.0 - alpha / a_),
            FilterType::Lowshelf => {
                let s = 2.0 * a_.sqrt() * alpha;
                (a_ * ((a_ + 1.0) - (a_ - 1.0) * cw + s), 2.0 * a_ * ((a_ - 1.0) - (a_ + 1.0) * cw), a_ * ((a_ + 1.0) - (a_ - 1.0) * cw - s), (a_ + 1.0) + (a_ - 1.0) * cw + s, -2.0 * ((a_ - 1.0) + (a_ + 1.0) * cw), (a_ + 1.0) + (a_ - 1.0) * cw - s)
            }
            FilterType::Highshelf => {
                let s = 2.0 * a_.sqrt() * alpha;
                (a_ * ((a_ + 1.0) + (a_ - 1.0) * cw + s), -2.0 * a_ * ((a_ - 1.0) + (a_ + 1.0) * cw), a_ * ((a_ + 1.0) + (a_ - 1.0) * cw - s), (a_ + 1.0) - (a_ - 1.0) * cw + s, 2.0 * ((a_ - 1.0) - (a_ + 1.0) * cw), (a_ + 1.0) - (a_ - 1.0) * cw - s)
            }
        };
        self.b = [b0 / a0, b1 / a0, b2 / a0];
        self.a = [a1 / a0, a2 / a0];
    }
    pub fn run(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        // denormals and blow-ups held off
        let y = if y.abs() < 1e-20 { 0.0 } else { y.clamp(-1e6, 1e6) };
        self.y = [y, self.y[0]];
        y as f32
    }
}

/// Tone's rolloff (-12, -24, -48 dB/octave) as a cascade of biquads.
#[derive(Clone)]
pub struct Cascade {
    pub stages: Vec<Biquad>,
}

impl Cascade {
    pub fn new(rolloff: f64) -> Cascade {
        let n = match rolloff as i64 {
            -24 => 2,
            -48 => 4,
            -96 => 8,
            _ => 1,
        };
        Cascade { stages: vec![Biquad::default(); n] }
    }
    pub fn set(&mut self, t: FilterType, freq: f64, q: f64, gain_db: f64, rate: f64) {
        for s in self.stages.iter_mut() {
            s.set(t, freq, q, gain_db, rate);
        }
    }
    pub fn run(&mut self, mut x: f32) -> f32 {
        for s in self.stages.iter_mut() {
            x = s.run(x);
        }
        x
    }
}

/// A delay line with fractional (linear) reads.
#[derive(Clone)]
pub struct Delay {
    buf: Vec<f32>,
    at: usize,
}

impl Delay {
    pub fn new(max_samples: usize) -> Delay {
        Delay { buf: vec![0.0; max_samples.max(2) + 2], at: 0 }
    }
    pub fn push(&mut self, x: f32) {
        self.buf[self.at] = x;
        self.at = (self.at + 1) % self.buf.len();
    }
    /// `d` samples back from the last pushed (d >= 1)
    pub fn read(&self, d: f64) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(1.0, (n - 2) as f64);
        let i = d.floor() as usize;
        let t = (d - i as f64) as f32;
        let a = self.buf[(self.at + n - i) % n];
        let b = self.buf[(self.at + n - i - 1) % n];
        a * (1.0 - t) + b * t
    }
}

/// A one-pole lowpass (Freeverb's damping, smoothing).
#[derive(Clone, Copy, Default)]
pub struct OnePole {
    pub y: f32,
}

impl OnePole {
    pub fn run(&mut self, x: f32, a: f32) -> f32 {
        self.y += (x - self.y) * a;
        self.y
    }
}

/// An LFO (sine), Tone's frequency in Hz, phase in turns.
#[derive(Clone, Copy, Default)]
pub struct Lfo {
    pub phase: f64,
}

impl Lfo {
    pub fn next(&mut self, freq: f64, rate: f64) -> f64 {
        let v = (self.phase * TAU).sin();
        self.phase = (self.phase + freq / rate).rem_euclid(1.0);
        v
    }
}

pub fn db(x: f64) -> f64 {
    10f64.powf(x / 20.0)
}

pub fn midi_hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

pub fn hz_midi(f: f64) -> f64 {
    69.0 + 12.0 * (f.max(1e-6) / 440.0).log2()
}

/// Equal-power pan: (left, right) gains for -1..1.
pub fn pan(p: f64) -> (f32, f32) {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    (a.cos() as f32, a.sin() as f32)
}
