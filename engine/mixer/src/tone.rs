//! Tone.js's instruments on the core: each instrument node of an edit's Tone graph (@raea/motion/tone) parsed from its
//! settings, and the voice that plays one note of it. Synth (oscillator types with fat/am/fm prefixes), MonoSynth
//! (filter and filter envelope), FMSynth, AMSynth, MembraneSynth, NoiseSynth, MetalSynth, PluckSynth (Karplus-Strong),
//! Sampler (the nearest sample, pitched by rate) and Player. Deterministic: a note is a function of its settings, its
//! frequency, its velocity and its seed.

use std::sync::Arc;

use serde_json::Value;

use crate::dsp::*;
use crate::pcm::Pcm;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Synth,
    Mono,
    Fm,
    Am,
    Membrane,
    Noise,
    Metal,
    Pluck,
    Sampler,
    Player,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "Synth" => Kind::Synth,
            "MonoSynth" => Kind::Mono,
            "FMSynth" | "DuoSynth" => Kind::Fm,
            "AMSynth" => Kind::Am,
            "MembraneSynth" => Kind::Membrane,
            "NoiseSynth" => Kind::Noise,
            "MetalSynth" => Kind::Metal,
            "PluckSynth" => Kind::Pluck,
            "Sampler" => Kind::Sampler,
            "Player" | "Players" | "GrainPlayer" => Kind::Player,
            _ => return None,
        })
    }
}

/// An instrument's settings, as its Tone constructor and set() gave them.
pub struct Inst {
    pub kind: Kind,
    pub wave: Wave,
    /// fat oscillators: count and spread (cents)
    pub fat: (usize, f64),
    pub env: Adsr,
    pub volume_db: f64,
    pub detune: f64,
    pub portamento: f64,
    pub filter: Option<(FilterType, f64, f64)>,
    /// filter envelope: its ADSR, base frequency, octaves, exponent
    pub fenv: Option<(Adsr, f64, f64, f64)>,
    pub harmonicity: f64,
    pub mod_index: f64,
    pub mod_wave: Wave,
    pub mod_env: Adsr,
    pub pitch_decay: f64,
    pub octaves: f64,
    pub noise: String,
    pub metal_freq: f64,
    pub resonance: f64,
    pub attack_noise: f64,
    pub dampening: f64,
    pub release: f64,
    /// Sampler: (midi, source); Player: one source (midi unused)
    pub samples: Vec<(f64, Arc<Pcm>)>,
    pub rate: f64,
    pub looped: bool,
    pub loop_start: f64,
    pub loop_end: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub mono: bool,
    pub max_poly: usize,
    pub seed: u64,
    /// raea's seeded variation for one-shots (semitones and dB of jitter)
    pub jitter_pitch: f64,
    pub jitter_db: f64,
}

fn num(v: &Value, path: &[&str], d: f64) -> f64 {
    let mut x = v;
    for k in path {
        match x.get(*k) {
            Some(y) => x = y,
            None => return d,
        }
    }
    x.as_f64().unwrap_or(d)
}

fn text<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut x = v;
    for k in path {
        x = x.get(*k)?;
    }
    x.as_str()
}

fn adsr(v: &Value, key: &str, d: Adsr) -> Adsr {
    Adsr { a: num(v, &[key, "attack"], d.a), d: num(v, &[key, "decay"], d.d), s: num(v, &[key, "sustain"], d.s), r: num(v, &[key, "release"], d.r) }
}

impl Inst {
    /// From a graph node's kind and options (motion/src/tone.ts writes them with Tone's names and defaults filled in).
    pub fn parse(kind: Kind, o: &Value, seed: u64, open: &mut dyn FnMut(&str) -> Option<Arc<Pcm>>) -> Inst {
        let osc_type = text(o, &["oscillator", "type"]).unwrap_or(match kind {
            Kind::Membrane | Kind::Am | Kind::Fm => "sine",
            Kind::Mono => "square",
            _ => "triangle",
        });
        let fat = if osc_type.starts_with("fat") { (num(o, &["oscillator", "count"], 3.0).max(1.0) as usize, num(o, &["oscillator", "spread"], 20.0)) } else { (1, 0.0) };
        let wave = Wave::parse(osc_type);
        let wave = if let Wave::Pulse(_) = wave { Wave::Pulse(num(o, &["oscillator", "width"], 0.2).clamp(0.01, 0.99) * 0.5 + 0.5) } else { wave };
        let env_default = match kind {
            Kind::Membrane => Adsr { a: 0.001, d: 0.5, s: 0.01, r: 1.4 },
            Kind::Noise => Adsr { a: 0.005, d: 0.1, s: 0.0, r: 1.0 },
            Kind::Metal => Adsr { a: 0.001, d: 1.4, s: 0.0, r: 0.2 },
            Kind::Mono => Adsr { a: 0.005, d: 0.1, s: 0.9, r: 1.0 },
            Kind::Fm | Kind::Am => Adsr { a: 0.01, d: 0.01, s: 1.0, r: 0.5 },
            Kind::Sampler | Kind::Player => Adsr { a: 0.0, d: 0.0, s: 1.0, r: num(o, &["release"], 0.1) },
            Kind::Pluck => Adsr { a: 0.0, d: 0.0, s: 1.0, r: num(o, &["release"], 1.0) },
            _ => Adsr { a: 0.005, d: 0.1, s: 0.3, r: 1.0 },
        };
        let mut env = adsr(o, "envelope", env_default);
        if matches!(kind, Kind::Sampler) {
            env.a = num(o, &["attack"], 0.0);
            env.r = num(o, &["release"], 0.1);
        }
        let filter = matches!(kind, Kind::Mono).then(|| (FilterType::parse(text(o, &["filter", "type"]).unwrap_or("lowpass")), num(o, &["filter", "Q"], 1.0), num(o, &["filter", "rolloff"], -12.0)));
        let fenv = matches!(kind, Kind::Mono).then(|| {
            (adsr(o, "filterEnvelope", Adsr { a: 0.6, d: 0.2, s: 0.5, r: 2.0 }), num(o, &["filterEnvelope", "baseFrequency"], 200.0), num(o, &["filterEnvelope", "octaves"], 3.0), num(o, &["filterEnvelope", "exponent"], 2.0))
        });
        let mut samples = Vec::new();
        if let Some(urls) = o.get("urls").and_then(|u| u.as_object()) {
            for (midi, file) in urls {
                if let (Ok(m), Some(f)) = (midi.parse::<f64>(), file.as_str())
                    && let Some(p) = open(f)
                {
                    samples.push((m, p));
                }
            }
            samples.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        if let Some(url) = o.get("url").and_then(|u| u.as_str())
            && let Some(p) = open(url)
        {
            samples.push((60.0, p));
        }
        let mono_default = !matches!(kind, Kind::Sampler | Kind::Player);
        Inst {
            kind,
            wave,
            fat,
            env,
            volume_db: num(o, &["volume"], 0.0),
            detune: num(o, &["detune"], 0.0),
            portamento: num(o, &["portamento"], 0.0),
            filter,
            fenv,
            harmonicity: num(o, &["harmonicity"], match kind {
                Kind::Metal => 5.1,
                Kind::Am => 3.0,
                _ => 3.0,
            }),
            mod_index: num(o, &["modulationIndex"], match kind {
                Kind::Metal => 32.0,
                _ => 10.0,
            }),
            mod_wave: Wave::parse(text(o, &["modulation", "type"]).unwrap_or(if matches!(kind, Kind::Am) { "square" } else { "square" })),
            mod_env: adsr(o, "modulationEnvelope", Adsr { a: 0.5, d: 0.0, s: 1.0, r: 0.5 }),
            pitch_decay: num(o, &["pitchDecay"], 0.05),
            octaves: num(o, &["octaves"], match kind {
                Kind::Membrane => 10.0,
                _ => 1.5,
            }),
            noise: text(o, &["noise", "type"]).unwrap_or("white").to_string(),
            metal_freq: num(o, &["frequency"], 200.0),
            resonance: num(o, &["resonance"], match kind {
                Kind::Metal => 4000.0,
                _ => 0.7,
            }),
            attack_noise: num(o, &["attackNoise"], 1.0),
            dampening: num(o, &["dampening"], 4000.0),
            release: num(o, &["release"], 1.0),
            samples,
            rate: num(o, &["playbackRate"], 1.0),
            looped: o.get("loop").and_then(|v| v.as_bool()).unwrap_or(false),
            loop_start: num(o, &["loopStart"], 0.0),
            loop_end: num(o, &["loopEnd"], 0.0),
            fade_in: num(o, &["fadeIn"], 0.0),
            fade_out: num(o, &["fadeOut"], 0.0),
            mono: o.get("poly").and_then(|v| v.as_bool()).map(|p| !p).unwrap_or(mono_default),
            max_poly: num(o, &["maxPolyphony"], 32.0).max(1.0) as usize,
            seed,
            jitter_pitch: num(o, &["variation", "pitch"], 0.0),
            jitter_db: num(o, &["variation", "volume"], 0.0),
        }
    }
}

/// One note of an instrument.
pub struct ToneVoice {
    pub inst: Arc<Inst>,
    pub freq: f64,
    target_freq: f64,
    vel: f32,
    rate: f64,
    pub age: u64,
    /// samples until the release (attack-release); None held until a release event
    pub release_at: Option<u64>,
    env: Env,
    menv: Env,
    fenv: Env,
    osc: [Osc; 7],
    filt: Cascade,
    hp: Biquad,
    noise: Noise,
    pluck: Option<Delay>,
    pluck_lp: OnePole,
    pluck_len: f64,
    /// sample position (source samples) and step (Sampler, Player)
    pos: f64,
    step: f64,
    src: Option<Arc<Pcm>>,
    end_at: Option<f64>,
    pub done: bool,
    gain: f32,
}

impl ToneVoice {
    #[allow(clippy::too_many_arguments)]
    pub fn new(inst: Arc<Inst>, freq: f64, vel: f32, dur: Option<u64>, rate: f64, seed: u64, offset: f64, play_dur: Option<f64>) -> ToneVoice {
        let mut noise = Noise::new(inst.seed ^ seed.wrapping_mul(0x100000001b3));
        // seeded variation: the same event key, the same jitter (replays identical)
        let jp = if inst.jitter_pitch > 0.0 { noise.white() as f64 * inst.jitter_pitch } else { 0.0 };
        let jd = if inst.jitter_db > 0.0 { noise.white() as f64 * inst.jitter_db } else { 0.0 };
        let freq = freq * 2f64.powf((inst.detune / 100.0 + jp) / 12.0);
        let gain = (db(inst.volume_db + jd)) as f32;
        let mut filt = Cascade::new(inst.filter.map(|f| f.2).unwrap_or(-12.0));
        if let Some((t, q, _)) = inst.filter {
            filt.set(t, 20000.0, q, 0.0, rate);
        }
        let mut hp = Biquad::default();
        hp.set(FilterType::Highpass, inst.resonance.max(20.0), 1.0, 0.0, rate);
        let (mut pos, mut step, mut src, mut end_at) = (0.0, 0.0, None, None);
        match inst.kind {
            Kind::Sampler => {
                // the nearest sample by pitch, played at the rate that makes the note
                let m = hz_midi(freq);
                if let Some((root, p)) = inst.samples.iter().min_by(|a, b| (a.0 - m).abs().total_cmp(&(b.0 - m).abs())) {
                    step = p.rate / rate * 2f64.powf((m - root) / 12.0);
                    src = Some(p.clone());
                }
            }
            Kind::Player => {
                if let Some((_, p)) = inst.samples.first() {
                    step = p.rate / rate * inst.rate;
                    pos = offset * p.rate;
                    end_at = play_dur.map(|d| pos + d * p.rate * inst.rate);
                    src = Some(p.clone());
                }
            }
            _ => {}
        }
        let pluck_len = rate / freq.max(20.0);
        let mut v = ToneVoice {
            inst: inst.clone(),
            freq,
            target_freq: freq,
            vel,
            rate,
            age: 0,
            release_at: dur,
            env: Env::default(),
            menv: Env::default(),
            fenv: Env::default(),
            osc: [Osc::new(0.0), Osc::new(0.25), Osc::new(0.5), Osc::new(0.75), Osc::new(0.1), Osc::new(0.6), Osc::new(0.35)],
            filt,
            hp,
            noise,
            pluck: None,
            pluck_lp: OnePole::default(),
            pluck_len,
            pos,
            step,
            src,
            end_at,
            done: false,
            gain,
        };
        if inst.kind == Kind::Pluck {
            // Karplus-Strong: the string's delay line filled with a burst of noise
            let n = pluck_len.ceil() as usize + 2;
            let mut d = Delay::new(n + 2);
            for _ in 0..n {
                let x = v.noise.white() * inst.attack_noise.min(4.0) as f32 * 0.5;
                d.push(x);
            }
            v.pluck = Some(d);
        }
        v.env.attack();
        v.menv.attack();
        v.fenv.attack();
        v
    }

    /// A monophonic instrument's next note on the same voice: Tone retriggers the envelope from where it is, the pitch
    /// gliding over the portamento.
    pub fn retrigger(&mut self, freq: f64, vel: f32, dur: Option<u64>) {
        self.target_freq = freq * 2f64.powf(self.inst.detune / 1200.0);
        if self.inst.portamento <= 0.0 {
            self.freq = self.target_freq;
        }
        self.vel = vel;
        self.release_at = dur.map(|d| self.age + d);
        self.env.attack();
        self.menv.attack();
        self.fenv.attack();
        if self.inst.kind == Kind::Pluck {
            *self = ToneVoice::new(self.inst.clone(), freq, vel, dur, self.rate, self.age, 0.0, None);
        }
    }

    pub fn release(&mut self) {
        self.release_at = Some(self.age);
    }

    pub fn releasing(&self) -> bool {
        self.env.releasing() || self.release_at.is_some_and(|r| self.age >= r)
    }

    /// Skips `n` samples in (an event that reached the mixer late).
    pub fn skip(&mut self, n: u64) {
        for _ in 0..n.min(self.rate as u64 * 4) {
            self.next();
        }
    }

    pub fn next(&mut self) -> (f32, f32) {
        if self.done {
            return (0.0, 0.0);
        }
        if self.release_at == Some(self.age) {
            self.env.release();
            self.menv.release();
            self.fenv.release();
        }
        let rate = self.rate;
        let i = &*self.inst;
        if i.portamento > 0.0 && self.freq != self.target_freq {
            let k = (1.0 / (i.portamento * rate)).min(1.0);
            self.freq += (self.target_freq - self.freq) * k;
        }
        let e = self.env.next(&i.env, rate);
        let f = self.freq;
        let t = self.age as f64 / rate;
        let x: f32 = match i.kind {
            Kind::Synth | Kind::Mono => {
                let mut v = 0f32;
                let n = i.fat.0;
                for k in 0..n.min(7) {
                    let cents = if n > 1 { -i.fat.1 / 2.0 + i.fat.1 * k as f64 / (n - 1) as f64 } else { 0.0 };
                    let wave = i.wave;
                    let ff = f * 2f64.powf(cents / 1200.0);
                    v += self.osc[k].next(wave, ff, rate);
                }
                v /= (n as f32).sqrt().max(1.0);
                if let (Some((ft, q, _)), Some((fe, base, oct, ex))) = (i.filter, i.fenv) {
                    let fl = self.fenv.next(&fe, rate);
                    let cutoff = base * 2f64.powf(oct * fl.powf(1.0 / ex.max(0.1)));
                    if self.age % 16 == 0 {
                        self.filt.set(ft, cutoff, q, 0.0, rate);
                    }
                    v = self.filt.run(v);
                }
                v
            }
            Kind::Fm => {
                let m = self.menv.next(&i.mod_env, rate);
                let modv = self.osc[1].next(i.mod_wave, f * i.harmonicity, rate) as f64;
                let ff = f + modv * f * i.harmonicity * i.mod_index * m;
                self.osc[0].next(i.wave, ff.max(0.0), rate)
            }
            Kind::Am => {
                let m = self.menv.next(&i.mod_env, rate);
                let modv = self.osc[1].next(i.mod_wave, f * i.harmonicity, rate);
                let car = self.osc[0].next(i.wave, f, rate);
                car * (1.0 - 0.5 * m as f32 + 0.5 * m as f32 * modv)
            }
            Kind::Membrane => {
                // Tone: the frequency set to f * octaves at the attack, ramped exponentially to f over the pitch decay
                let k = (t / i.pitch_decay.max(1e-4)).min(1.0);
                let ff = f * i.octaves.max(1.0).powf(1.0 - k);
                self.osc[0].next(i.wave, ff, rate)
            }
            Kind::Noise => self.noise.of(&i.noise),
            Kind::Metal => {
                // six square oscillators at inharmonic ratios, highpassed at the resonance
                let ratios = [1.0, 1.483, 1.932, 2.546, 2.630, 3.897];
                let mut v = 0f32;
                for (k, r) in ratios.iter().enumerate() {
                    v += self.osc[k].next(Wave::Square, i.metal_freq * r * (1.0 + i.harmonicity * 0.001), rate);
                }
                self.hp.run(v / 6.0)
            }
            Kind::Pluck => {
                let d = self.pluck.as_mut().unwrap();
                let y = d.read(self.pluck_len);
                let y2 = d.read(self.pluck_len + 1.0);
                let damp = (1.0 - (-std::f64::consts::TAU * i.dampening / rate).exp()) as f32;
                let avg = 0.5 * (y + y2);
                let fb = self.pluck_lp.run(avg, damp) * (0.9 + 0.099 * i.resonance.clamp(0.0, 1.0) as f32);
                d.push(fb);
                y
            }
            Kind::Sampler | Kind::Player => {
                let Some(p) = self.src.clone() else {
                    self.done = true;
                    return (0.0, 0.0);
                };
                let len = p.len_s * p.rate;
                if i.looped && i.loop_end > i.loop_start && self.pos >= i.loop_end * p.rate {
                    self.pos = i.loop_start * p.rate + (self.pos - i.loop_end * p.rate);
                }
                if self.pos >= len || self.end_at.is_some_and(|e| self.pos >= e) {
                    if i.looped && i.loop_end <= i.loop_start {
                        self.pos = 0.0;
                    } else {
                        self.done = true;
                        return (0.0, 0.0);
                    }
                }
                let mut blocks = p.blocks.lock().unwrap();
                let s = p.sample(&mut blocks, self.pos).unwrap_or((0.0, 0.0));
                drop(blocks);
                self.pos += self.step;
                let mut g = 1.0f32;
                if i.fade_in > 0.0 && t < i.fade_in {
                    g *= (t / i.fade_in) as f32;
                }
                let out = (s.0 * g * e as f32 * self.vel * self.gain, s.1 * g * e as f32 * self.vel * self.gain);
                self.age += 1;
                if self.env.idle() && self.age > 1 {
                    self.done = true;
                }
                return out;
            }
        };
        self.age += 1;
        if self.env.idle() && self.age > 1 {
            self.done = true;
        }
        let v = x * e as f32 * self.vel * self.gain;
        (v, v)
    }
}
