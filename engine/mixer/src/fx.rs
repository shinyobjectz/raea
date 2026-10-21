//! Tone.js's effects and mixing nodes on the core, as one Firewheel node type (`ToneFx`): stereo in, a stereo sidechain
//! in (a ducked channel's key), stereo out. Each kind is a deterministic DSP: Gain, Volume, Panner, Channel/PanVol,
//! Destination, Filter, AutoFilter, FeedbackDelay, PingPongDelay, Reverb, Freeverb, JCReverb, Compressor, Limiter, EQ3,
//! Distortion, Chorus, BitCrusher, Tremolo, Vibrato, Phaser, AutoPanner, Convolver, PitchShift. Parameters ramp on the
//! sample their event names (Tone's rampTo, linearRampTo, exponentialRampTo, setValueAtTime); LFOs (an effect's own,
//! and a Tone.LFO connected to a parameter) run on the timeline's time (from the origin the node is given), so a play
//! and a render of the same events are the same samples.

use firewheel::channel_config::{ChannelConfig, ChannelCount};
use firewheel::event::ProcEvents;
use firewheel::node::{AudioNode, AudioNodeInfo, AudioNodeProcessor, ConstructProcessorContext, NodeError, ProcBuffers, ProcExtra, ProcInfo, ProcessStatus};
use serde_json::Value;

use crate::dsp::*;

/// An event for a ToneFx node.
pub enum FxEv {
    /// a parameter to `value` over `ramp` samples (0: at once), exponentially when `exp` (Tone's exponentialRampTo)
    Param { name: String, value: f64, ramp: u32, exp: bool },
    /// the clock sample of the timeline's 0 (live: where the timeline sits on the device's clock)
    Origin(i64),
}

#[derive(Clone)]
pub struct FxNode {
    pub kind: String,
    pub params: Value,
    pub origin: i64,
    pub muted: bool,
    /// a Convolver's impulse response
    pub ir: Option<std::sync::Arc<raea_media::Audio>>,
}

impl AudioNode for FxNode {
    type Configuration = ();

    fn info(&self, _: &()) -> Result<AudioNodeInfo, NodeError> {
        Ok(AudioNodeInfo::new().debug_name("tone fx").channel_config(ChannelConfig { num_inputs: ChannelCount::new(4).unwrap(), num_outputs: ChannelCount::STEREO }))
    }

    fn construct_processor(&self, _: &(), cx: ConstructProcessorContext) -> Result<impl AudioNodeProcessor, NodeError> {
        Ok(self.processor(cx.stream_info.sample_rate.get() as f64))
    }
}

impl FxNode {
    /// The node's processor at `rate` (for a node type that wraps this one: bevy_seedling's, raea-seedling).
    pub fn processor(&self, rate: f64) -> impl AudioNodeProcessor + use<> {
        let mut fx = Fx::new(&self.kind, &self.params, rate, self.origin, self.muted);
        if let Some(ir) = &self.ir {
            fx.conv = Some(Box::new(crate::conv::pair(ir, rate, self.params.get("normalize").and_then(|v| v.as_bool()).unwrap_or(true))));
        }
        fx
    }
}

#[derive(Clone, Copy)]
struct Ramp {
    v: f64,
    to: f64,
    step: f64,
    left: u32,
    /// the step is a ratio (an exponential ramp)
    mul: bool,
}

impl Ramp {
    fn new(v: f64) -> Ramp {
        Ramp { v, to: v, step: 0.0, left: 0, mul: false }
    }
    fn set(&mut self, to: f64, n: u32, exp: bool) {
        if n == 0 {
            self.v = to;
            self.to = to;
            self.left = 0;
        } else {
            self.to = to;
            self.left = n;
            // exponential: the same ratio each sample (Tone's exponentialRampTo; as Tone does, from or to 0 or across
            // a sign change it can't be, and it's linear)
            self.mul = exp && self.v * to > 0.0;
            self.step = if self.mul { (to / self.v).powf(1.0 / n as f64) } else { (to - self.v) / n as f64 };
        }
    }
    fn tick(&mut self) -> f64 {
        if self.left > 0 {
            if self.mul {
                self.v *= self.step;
            } else {
                self.v += self.step;
            }
            self.left -= 1;
            if self.left == 0 {
                self.v = self.to;
            }
        }
        self.v
    }
}

fn num(v: &Value, path: &[&str], d: f64) -> f64 {
    let mut x = v;
    for k in path {
        match x.get(*k) {
            Some(y) => x = y,
            None => return d,
        }
    }
    x.as_f64().or_else(|| x.as_bool().map(|b| if b { 1.0 } else { 0.0 })).unwrap_or(d)
}

/// Freeverb's tunings at 44.1 kHz, scaled to the rate.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

struct Verb {
    combs: Vec<(Delay, usize, OnePole)>,
    alls: Vec<(Delay, usize)>,
}

impl Verb {
    fn new(rate: f64, spread: usize) -> Verb {
        let k = rate / 44100.0;
        Verb {
            combs: COMBS.iter().map(|n| {
                let l = ((n + spread) as f64 * k) as usize;
                (Delay::new(l + 2), l, OnePole::default())
            }).collect(),
            alls: ALLPASSES.iter().map(|n| {
                let l = ((n + spread) as f64 * k) as usize;
                (Delay::new(l + 2), l)
            }).collect(),
        }
    }
    /// `fb(len)`: each comb's feedback by its length; `damp` the damping lowpass coefficient
    fn run(&mut self, x: f32, fb: &dyn Fn(usize) -> f32, damp: f32) -> f32 {
        let mut out = 0f32;
        for (d, l, lp) in self.combs.iter_mut() {
            let y = d.read(*l as f64);
            let f = lp.run(y, 1.0 - damp);
            d.push(x + f * fb(*l));
            out += y;
        }
        for (d, l) in self.alls.iter_mut() {
            let b = d.read(*l as f64);
            let y = -out + b;
            d.push(out + b * 0.5);
            out = y;
        }
        out
    }
}

/// The node's kind, picked once (a string matched per sample cost the reverb song a fifth of a core).
#[derive(Clone, Copy, PartialEq, Debug)]
enum K {
    Gain,
    Volume,
    Panner,
    Channel,
    Filter,
    AutoFilter,
    Delay,
    PingPong,
    Reverb,
    Freeverb,
    JcReverb,
    Comp,
    Limiter,
    Eq3,
    Dist,
    Chorus,
    Vibrato,
    Crusher,
    Tremolo,
    AutoPanner,
    Phaser,
    Convolver,
    PitchShift,
    Pass,
}

/// A Tone.LFO connected to a parameter: its value is the parameter's from `started` on (before, Tone's stopped value:
/// where the wave starts).
struct Mod {
    p: usize,
    freq: f64,
    min: f64,
    max: f64,
    amp: f64,
    wave: u8,
    phase: f64,
    started: Option<f64>,
}

impl Mod {
    fn value(&self, t: f64) -> f64 {
        let lt = self.started.map(|s| (t - s).max(0.0)).unwrap_or(0.0);
        let ph = (lt * self.freq + self.phase / 360.0).rem_euclid(1.0);
        let w = match self.wave {
            1 => if ph < 0.5 { 1.0 } else { -1.0 },
            2 => if ph < 0.25 { 4.0 * ph } else if ph < 0.75 { 2.0 - 4.0 * ph } else { 4.0 * ph - 4.0 },
            3 => if ph < 0.5 { 2.0 * ph } else { 2.0 * ph - 2.0 },
            _ => (ph * std::f64::consts::TAU).sin(),
        } * self.amp;
        self.min + (self.max - self.min) * (0.5 + 0.5 * w)
    }
}

fn kind_of(s: &str) -> K {
    match s {
        "Gain" => K::Gain,
        "Volume" => K::Volume,
        "Destination" => K::Volume,
        "Panner" => K::Panner,
        "Channel" => K::Channel,
        "PanVol" => K::Channel,
        "Filter" => K::Filter,
        "AutoFilter" => K::AutoFilter,
        "FeedbackDelay" => K::Delay,
        "PingPongDelay" => K::PingPong,
        "Reverb" => K::Reverb,
        "Freeverb" => K::Freeverb,
        "JCReverb" => K::JcReverb,
        "Compressor" => K::Comp,
        "Limiter" => K::Limiter,
        "EQ3" => K::Eq3,
        "Distortion" => K::Dist,
        "Chorus" => K::Chorus,
        "Vibrato" => K::Vibrato,
        "BitCrusher" => K::Crusher,
        "Tremolo" => K::Tremolo,
        "AutoPanner" => K::AutoPanner,
        "Phaser" => K::Phaser,
        "Convolver" => K::Convolver,
        "PitchShift" => K::PitchShift,
        _ => K::Pass,
    }
}

struct Fx {
    k: K,
    rate: f64,
    origin: i64,
    muted: bool,
    names: Vec<String>,
    p: Vec<Ramp>,
    filt: [Cascade; 2],
    bq: [[Biquad; 4]; 2],
    conv: Option<Box<[crate::conv::Conv; 2]>>,
    mods: Vec<Mod>,
    fb: [f32; 2],
    phaser: [Vec<Biquad>; 2],
    delay: [Delay; 2],
    verb: Option<[Verb; 2]>,
    env: f64,
    duck_env: f64,
    duck_gain: f64,
    started: Option<f64>,
    filter_type: FilterType,
    rolloff: f64,
    duck: Option<(f64, f64, f64, f64)>,
    primed: bool,
}

impl Fx {
    fn new(kind: &str, o: &Value, rate: f64, origin: i64, muted: bool) -> Fx {
        // each kind's parameters, by Tone's names, with Tone's defaults
        let spec: &[(&str, f64)] = match kind {
            "Gain" => &[("gain", 1.0)],
            "Volume" | "Destination" => &[("volume", 0.0)],
            "Panner" => &[("pan", 0.0)],
            "Channel" | "PanVol" => &[("volume", 0.0), ("pan", 0.0)],
            "Filter" => &[("frequency", 350.0), ("Q", 1.0), ("gain", 0.0)],
            "AutoFilter" => &[("frequency", 1.0), ("depth", 1.0), ("baseFrequency", 200.0), ("octaves", 2.6), ("wet", 1.0)],
            "FeedbackDelay" | "PingPongDelay" => &[("delayTime", 0.25), ("feedback", 0.125), ("wet", 1.0)],
            "Reverb" => &[("decay", 1.5), ("preDelay", 0.01), ("wet", 1.0)],
            "Freeverb" => &[("roomSize", 0.7), ("dampening", 3000.0), ("wet", 1.0)],
            "JCReverb" => &[("roomSize", 0.5), ("wet", 1.0)],
            "Compressor" => &[("threshold", -24.0), ("ratio", 12.0), ("attack", 0.003), ("release", 0.25), ("knee", 30.0)],
            "Limiter" => &[("threshold", -12.0)],
            "EQ3" => &[("low", 0.0), ("mid", 0.0), ("high", 0.0), ("lowFrequency", 400.0), ("highFrequency", 2500.0)],
            "Distortion" => &[("distortion", 0.4), ("wet", 1.0)],
            "Chorus" => &[("frequency", 1.5), ("delayTime", 3.5), ("depth", 0.7), ("spread", 180.0), ("wet", 0.5)],
            "BitCrusher" => &[("bits", 4.0), ("wet", 1.0)],
            "Tremolo" => &[("frequency", 10.0), ("depth", 0.5), ("spread", 180.0), ("wet", 1.0)],
            "Vibrato" => &[("frequency", 5.0), ("depth", 0.1), ("wet", 1.0)],
            "Phaser" => &[("frequency", 0.5), ("octaves", 3.0), ("baseFrequency", 350.0), ("Q", 10.0), ("wet", 1.0)],
            "AutoPanner" => &[("frequency", 1.0), ("depth", 1.0), ("wet", 1.0)],
            "Convolver" => &[("wet", 1.0)],
            "PitchShift" => &[("pitch", 0.0), ("windowSize", 0.1), ("delayTime", 0.0), ("feedback", 0.0), ("wet", 1.0)],
            _ => &[],
        };
        let names: Vec<String> = spec.iter().map(|(n, _)| n.to_string()).collect();
        let p: Vec<Ramp> = spec.iter().map(|(n, d)| Ramp::new(num(o, &[n], *d))).collect();
        let rolloff = num(o, &["rolloff"], num(o, &["filter", "rolloff"], -12.0));
        let filter_type = FilterType::parse(o.get("type").or_else(|| o.get("filter").and_then(|f| f.get("type"))).and_then(|v| v.as_str()).unwrap_or(if kind == "AutoFilter" { "lowpass" } else { "lowpass" }));
        let max_delay = (num(o, &["maxDelay"], 2.0).max(0.05) * rate) as usize + 4;
        let verb = matches!(kind, "Reverb" | "Freeverb" | "JCReverb").then(|| [Verb::new(rate, 0), Verb::new(rate, STEREO_SPREAD)]);
        let started = o.get("started").and_then(|v| v.as_f64());
        let mods = o.get("mods").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|m| {
            let p = names.iter().position(|n| Some(n.as_str()) == m.get("p").and_then(|v| v.as_str()))?;
            let wave = match m.get("type").and_then(|v| v.as_str()).unwrap_or("sine") {
                "square" => 1,
                "triangle" => 2,
                "sawtooth" => 3,
                _ => 0,
            };
            Some(Mod { p, freq: num(m, &["frequency"], 1.0), min: num(m, &["min"], 0.0), max: num(m, &["max"], 1.0), amp: num(m, &["amplitude"], 1.0), wave, phase: num(m, &["phase"], 0.0), started: m.get("started").and_then(|v| v.as_f64()) })
        }).collect()).unwrap_or_default();
        let duck = o.get("duck").filter(|d| d.is_object()).map(|d| (num(d, &["amount"], -12.0), num(d, &["threshold"], -40.0), num(d, &["attack"], 0.05), num(d, &["release"], 0.3)));
        Fx {
            k: kind_of(kind),
            rate,
            origin,
            muted: muted || o.get("mute").and_then(|v| v.as_bool()).unwrap_or(false),
            names,
            p,
            filt: [Cascade::new(rolloff), Cascade::new(rolloff)],
            bq: [[Biquad::default(); 4]; 2],
            conv: None,
            mods,
            fb: [0.0; 2],
            phaser: [vec![Biquad::default(); 10], vec![Biquad::default(); 10]],
            delay: [Delay::new(max_delay), Delay::new(max_delay)],
            verb,
            env: -120.0,
            duck_env: -120.0,
            duck_gain: 1.0,
            started,
            filter_type,
            rolloff,
            duck,
            primed: false,
        }
    }

    fn i(&self, name: &str) -> usize {
        self.names.iter().position(|n| n == name).unwrap_or(usize::MAX)
    }
}

impl AudioNodeProcessor for Fx {
    fn events(&mut self, _: &ProcInfo, events: &mut ProcEvents, _: &mut ProcExtra) {
        for mut ev in events.drain() {
            let Some(e) = ev.downcast_mut::<Option<FxEv>>().and_then(Option::take) else { continue };
            match e {
                FxEv::Param { name, value, ramp, exp } => {
                    if name == "mute" {
                        self.muted = value != 0.0;
                    } else if let Some(k) = self.names.iter().position(|n| *n == name) {
                        self.p[k].set(value, ramp, exp);
                    }
                }
                FxEv::Origin(o) => self.origin = o,
            }
        }
    }

    fn process(&mut self, info: &ProcInfo, buffers: ProcBuffers, _: &mut ProcExtra) -> ProcessStatus {
        let n = info.frames;
        let rate = self.rate;
        let t0 = (info.clock_samples.0 - self.origin) as f64 / rate;
        // coefficients are worked out on the timeline's grid (every 16th sample from its 0), not the block's: blocks fall in
        // other places live and offline, and the play must still be the bounce
        let tl = (info.clock_samples.0 - self.origin).rem_euclid(1 << 20);
        let (inl, inr) = (&buffers.inputs[0][..n], &buffers.inputs[1][..n]);
        let (kl, kr) = (&buffers.inputs[2][..n], &buffers.inputs[3][..n]);
        let (l_out, rest) = buffers.outputs.split_first_mut().unwrap();
        let (lo, ro) = (&mut l_out[..n], &mut rest[0][..n]);
        let wet_i = self.i("wet");
        let kind = self.k;
        for s in 0..n {
            let t = t0 + s as f64 / rate;
            // before the timeline's 0 nothing has played (live: the head before the first frame): no state moves, so
            // the node is at time 0 as the bounce's is (a compressor's detector decayed there, and the start differed)
            if t < 0.0 {
                lo[s] = 0.0;
                ro[s] = 0.0;
                continue;
            }
            let lfo_t = self.started.map(|st| t - st);
            for r in self.p.iter_mut() {
                r.tick();
            }
            for m in &self.mods {
                self.p[m.p].v = m.value(t);
            }
            let pv = |k: usize, d: f64| -> f64 { self.p.get(k).map(|r| r.v).unwrap_or(d) };
            let (x0, x1) = (inl[s], inr[s]);
            let (mut y0, mut y1) = (x0, x1);
            match kind {
                K::Gain => {
                    let g = pv(0, 1.0) as f32;
                    (y0, y1) = (x0 * g, x1 * g);
                }
                K::Volume => {
                    let g = db(pv(0, 0.0)) as f32;
                    (y0, y1) = (x0 * g, x1 * g);
                }
                K::Panner => {
                    let (a, b) = pan(pv(0, 0.0));
                    (y0, y1) = (x0 * a * std::f32::consts::SQRT_2, x1 * b * std::f32::consts::SQRT_2);
                }
                K::Channel => {
                    let g = db(pv(0, 0.0)) as f32;
                    let (a, b) = pan(pv(1, 0.0));
                    (y0, y1) = (x0 * g * a * std::f32::consts::SQRT_2, x1 * g * b * std::f32::consts::SQRT_2);
                }
                K::Filter => {
                    if (tl + s as i64) % 16 == 0 || !self.primed {
                        let (f, q, g) = (pv(0, 350.0), pv(1, 1.0), pv(2, 0.0));
                        for c in self.filt.iter_mut() {
                            c.set(self.filter_type, f, q, g, rate);
                        }
                    }
                    (y0, y1) = (self.filt[0].run(x0), self.filt[1].run(x1));
                }
                K::AutoFilter => {
                    if (tl + s as i64) % 16 == 0 || !self.primed {
                        let l = lfo_t.map(|lt| (lt * pv(0, 1.0) * std::f64::consts::TAU).sin()).unwrap_or(-1.0);
                        let f = pv(2, 200.0) * 2f64.powf(pv(3, 2.6) * pv(1, 1.0) * (l * 0.5 + 0.5));
                        for c in self.filt.iter_mut() {
                            c.set(self.filter_type, f, 1.0, 0.0, rate);
                        }
                    }
                    (y0, y1) = (self.filt[0].run(x0), self.filt[1].run(x1));
                }
                K::Delay => {
                    let d = (pv(0, 0.25) * rate).max(1.0);
                    let fb = pv(1, 0.125) as f32;
                    let (a, b) = (self.delay[0].read(d), self.delay[1].read(d));
                    self.delay[0].push(x0 + a * fb);
                    self.delay[1].push(x1 + b * fb);
                    (y0, y1) = (a, b);
                }
                K::PingPong => {
                    let d = (pv(0, 0.25) * rate).max(1.0);
                    let fb = pv(1, 0.125) as f32;
                    let (a, b) = (self.delay[0].read(d), self.delay[1].read(d));
                    // the left's echo feeds the right and back: a ball between the speakers
                    self.delay[0].push(0.5 * (x0 + x1) + b * fb);
                    self.delay[1].push(a * fb);
                    (y0, y1) = (a, b);
                }
                K::Reverb | K::Freeverb | K::JcReverb => {
                    let input = 0.015 * (x0 + x1);
                    let (fb, damp): (Box<dyn Fn(usize) -> f32>, f32) = match kind {
                        K::Reverb => {
                            // each comb's feedback for a 60 dB decay over `decay` seconds
                            let decay = pv(0, 1.5).max(0.05);
                            (Box::new(move |l: usize| 10f64.powf(-3.0 * l as f64 / (decay * rate)) as f32), 0.2)
                        }
                        K::Freeverb => {
                            let room = (0.7 + 0.28 * pv(0, 0.7)) as f32;
                            let damp = (1.0 - (pv(1, 3000.0) / 20000.0).clamp(0.0, 1.0)) as f32 * 0.4;
                            (Box::new(move |_| room), damp)
                        }
                        _ => {
                            let room = (0.7 + 0.28 * pv(0, 0.5)) as f32;
                            (Box::new(move |_| room), 0.05)
                        }
                    };
                    let v = self.verb.as_mut().unwrap();
                    (y0, y1) = (v[0].run(input, &*fb, damp), v[1].run(input, &*fb, damp));
                }
                K::Comp | K::Limiter => {
                    let (th, ratio, at, rel, knee) = if kind == K::Limiter { (pv(0, -12.0), 20.0, 0.001, 0.05, 0.0) } else { (pv(0, -24.0), pv(1, 12.0), pv(2, 0.003), pv(3, 0.25), pv(4, 30.0)) };
                    let lvl = 20.0 * (x0.abs().max(x1.abs()) as f64).max(1e-9).log10();
                    let c = if lvl > self.env { 1.0 - (-1.0 / (at.max(1e-4) * rate)).exp() } else { 1.0 - (-1.0 / (rel.max(1e-4) * rate)).exp() };
                    self.env += (lvl - self.env) * c;
                    let over = self.env - th;
                    let gr = if knee > 0.0 && over.abs() < knee / 2.0 { (over + knee / 2.0).powi(2) / (2.0 * knee) * (1.0 - 1.0 / ratio) } else if over > 0.0 { over * (1.0 - 1.0 / ratio) } else { 0.0 };
                    let g = db(-gr) as f32;
                    (y0, y1) = (x0 * g, x1 * g);
                }
                K::Eq3 => {
                    // Tone's EQ3: a three-band split (MultibandSplit: lowpass at lowFrequency; highpass there then
                    // lowpass at highFrequency; highpass at highFrequency, each Q 1, 12 dB/oct), each band's gain, summed
                    if (tl + s as i64) % 32 == 0 || !self.primed {
                        let (lf, hf) = (pv(3, 400.0), pv(4, 2500.0));
                        for b in self.bq.iter_mut() {
                            b[0].set(FilterType::Lowpass, lf, 1.0, 0.0, rate);
                            b[1].set(FilterType::Highpass, lf, 1.0, 0.0, rate);
                            b[2].set(FilterType::Lowpass, hf, 1.0, 0.0, rate);
                            b[3].set(FilterType::Highpass, hf, 1.0, 0.0, rate);
                        }
                    }
                    let (gl, gm, gh) = (db(pv(0, 0.0)) as f32, db(pv(1, 0.0)) as f32, db(pv(2, 0.0)) as f32);
                    let run = |b: &mut [Biquad; 4], x: f32| {
                        let lo = b[0].run(x);
                        let m = b[1].run(x);
                        let mid = b[2].run(m);
                        let hi = b[3].run(x);
                        lo * gl + mid * gm + hi * gh
                    };
                    y0 = run(&mut self.bq[0], x0);
                    y1 = run(&mut self.bq[1], x1);
                }
                K::Convolver => {
                    if let Some(c) = self.conv.as_mut() {
                        let start = (tl + s as i64) % crate::conv::B as i64 == 0;
                        (y0, y1) = (c[0].run(x0, start), c[1].run(x1, start));
                    } else {
                        (y0, y1) = (0.0, 0.0);
                    }
                }
                K::PitchShift => {
                    // Tone's PitchShift: two delay taps sweeping the window by sawtooth, half a cycle apart, cross-faded
                    // by a triangle so each tap is silent where it jumps; feedback from the output to the input
                    let (pitch, win, dt, fbk) = (pv(0, 0.0), pv(1, 0.1).max(0.01), pv(2, 0.0), pv(3, 0.0) as f32);
                    let factor = if pitch < 0.0 { 2f64.powf((pitch - 1.0) / 12.0) + 1.0 } else { 2f64.powf(pitch / 12.0) - 1.0 };
                    let f = factor * 1.2 / win;
                    let ph = (t * f).rem_euclid(1.0);
                    let saw = |p: f64| if pitch < 0.0 { p * win } else { (1.0 - p) * win };
                    let (da, dbb) = ((dt + saw(ph)) * rate, (dt + saw((ph + 0.5).rem_euclid(1.0))) * rate);
                    let fade = 1.0 - (1.0 - 2.0 * ph).abs();
                    let (ga, gb) = ((fade * std::f64::consts::FRAC_PI_2).sin() as f32, (fade * std::f64::consts::FRAC_PI_2).cos() as f32);
                    self.delay[0].push(x0 + self.fb[0] * fbk);
                    self.delay[1].push(x1 + self.fb[1] * fbk);
                    y0 = self.delay[0].read(da.max(1.0)) * ga + self.delay[0].read(dbb.max(1.0)) * gb;
                    y1 = self.delay[1].read(da.max(1.0)) * ga + self.delay[1].read(dbb.max(1.0)) * gb;
                    self.fb = [y0, y1];
                }
                K::Dist => {
                    // Tone's curve: (3 + k) x 20 deg / (pi + k |x|), k = amount * 100
                    let k = pv(0, 0.4) * 100.0;
                    let deg = std::f64::consts::PI / 180.0;
                    let w = |x: f32| -> f32 { let x = x as f64; ((3.0 + k) * x * 20.0 * deg / (std::f64::consts::PI + k * x.abs())) as f32 };
                    (y0, y1) = (w(x0), w(x1));
                }
                K::Chorus | K::Vibrato => {
                    let (f, base, depth, spread) = if kind == K::Chorus { (pv(0, 1.5), pv(1, 3.5) / 1000.0, pv(2, 0.7), pv(3, 180.0)) } else { (pv(0, 5.0), 0.005, pv(1, 0.1), 0.0) };
                    let ph = lfo_t.unwrap_or(0.0) * f * std::f64::consts::TAU;
                    let (m0, m1) = (ph.sin(), (ph + spread.to_radians()).sin());
                    self.delay[0].push(x0);
                    self.delay[1].push(x1);
                    let d0 = (base * (1.0 + depth * m0 * if lfo_t.is_some() { 1.0 } else { 0.0 }) * rate).max(1.0);
                    let d1 = (base * (1.0 + depth * m1 * if lfo_t.is_some() { 1.0 } else { 0.0 }) * rate).max(1.0);
                    (y0, y1) = (self.delay[0].read(d0), self.delay[1].read(d1));
                }
                K::Crusher => {
                    let steps = 2f64.powf(pv(0, 4.0).clamp(1.0, 16.0)) as f32;
                    (y0, y1) = ((x0 * steps).round() / steps, (x1 * steps).round() / steps);
                }
                K::Tremolo | K::AutoPanner => {
                    let f = pv(0, if kind == K::Tremolo { 10.0 } else { 1.0 });
                    let depth = pv(1, 0.5);
                    let ph = lfo_t.map(|lt| lt * f * std::f64::consts::TAU);
                    match ph {
                        None => {}
                        Some(ph) if kind == K::Tremolo => {
                            let spread = pv(2, 180.0).to_radians();
                            let g0 = 1.0 - depth * (0.5 - 0.5 * ph.sin());
                            let g1 = 1.0 - depth * (0.5 - 0.5 * (ph + spread).sin());
                            (y0, y1) = (x0 * g0 as f32, x1 * g1 as f32);
                        }
                        Some(ph) => {
                            let (a, b) = pan(ph.sin() * depth);
                            (y0, y1) = (x0 * a * std::f32::consts::SQRT_2, x1 * b * std::f32::consts::SQRT_2);
                        }
                    }
                }
                K::Phaser => {
                    if (tl + s as i64) % 16 == 0 || !self.primed {
                        let l = lfo_t.map(|lt| (lt * pv(0, 0.5) * std::f64::consts::TAU).sin()).unwrap_or(0.0);
                        let f = pv(2, 350.0) * 2f64.powf(pv(1, 3.0) * (l * 0.5 + 0.5));
                        for ch in self.phaser.iter_mut() {
                            for b in ch.iter_mut() {
                                b.set(FilterType::Allpass, f, pv(3, 10.0) / 10.0, 0.0, rate);
                            }
                        }
                    }
                    let mut a = x0;
                    let mut b = x1;
                    for q in self.phaser[0].iter_mut() {
                        a = q.run(a);
                    }
                    for q in self.phaser[1].iter_mut() {
                        b = q.run(b);
                    }
                    (y0, y1) = (a, b);
                }
                _ => {}
            }
            if wet_i != usize::MAX {
                let w = self.p[wet_i].v as f32;
                y0 = x0 * (1.0 - w) + y0 * w;
                y1 = x1 * (1.0 - w) + y1 * w;
            }
            // ducking: the key's level over the threshold takes the channel down by `amount`
            if let Some((amount, th, at, rel)) = self.duck {
                let k = 20.0 * (kl[s].abs().max(kr[s].abs()) as f64).max(1e-9).log10();
                let c = if k > self.duck_env { 1.0 - (-1.0 / (0.005 * rate)).exp() } else { 1.0 - (-1.0 / (0.1 * rate)).exp() };
                self.duck_env += (k - self.duck_env) * c;
                let target = if self.duck_env > th { db(amount) } else { 1.0 };
                let tc = if target < self.duck_gain { at } else { rel };
                self.duck_gain += (target - self.duck_gain) * (1.0 - (-1.0 / (tc.max(1e-4) * rate)).exp());
                y0 *= self.duck_gain as f32;
                y1 *= self.duck_gain as f32;
            }
            if self.muted {
                (y0, y1) = (0.0, 0.0);
            }
            lo[s] = y0;
            ro[s] = y1;
            self.primed = true;
        }
        let _ = self.rolloff;
        ProcessStatus::OutputsModified
    }
}
