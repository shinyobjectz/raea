//! The graph's two nodes: the voices (every sounding source and instrument note, placed on the sample its event names)
//! and the master (gain, then the look-ahead true-peak limiter at the ceiling). Both are pure functions of their events
//! and the clock: the same events give the same samples whatever the block sizes, live or offline.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use firewheel::channel_config::{ChannelConfig, ChannelCount};
use firewheel::clock::EventInstant;
use firewheel::event::{NodeEventType, ProcEvents};
use firewheel::node::{AudioNode, AudioNodeInfo, AudioNodeProcessor, ConstructProcessorContext, NodeError, ProcBuffers, ProcExtra, ProcInfo, ProcessStatus};

use crate::score::Ev;

/// What the audio thread saw: events that came after their sample (live only: a start then skips in by that much),
/// samples a voice had no block for (live only: silence), voices playing.
#[derive(Default)]
pub struct Stats {
    pub late: AtomicU64,
    pub late_samples: AtomicU64,
    pub missing: AtomicU64,
    pub voices: AtomicU64,
}

enum Src {
    Pcm { pcm: Arc<crate::pcm::Pcm>, pos: f64, step: f64 },
    Tone(Box<crate::tone::ToneVoice>),
}

struct Voice {
    id: u64,
    src: Src,
    env: Env,
    /// the voices node's output it plays from
    port: usize,
    /// the Tone instrument it belongs to
    node: Option<u32>,
}

struct Env {
    gain: f32,
    from: f32,
    ramp: (u32, u32),
    fade_in: u32,
    age: u64,
    stopping: Option<(u32, u32)>,
    /// back from a missing block: ramps up over 5 ms (live only)
    hold: f32,
    done: bool,
}

impl Env {
    fn level(&mut self) -> f32 {
        let mut g = if self.ramp.0 > 0 {
            let k = self.ramp.0 as f32 / self.ramp.1.max(1) as f32;
            self.ramp.0 -= 1;
            self.gain + (self.from - self.gain) * k
        } else {
            self.gain
        };
        if (self.age as u32) < self.fade_in {
            g *= self.age as f32 / self.fade_in as f32;
        }
        if let Some((left, total)) = self.stopping.as_mut() {
            if *left == 0 {
                self.done = true;
                return 0.0;
            }
            g *= *left as f32 / (*total).max(1) as f32;
            *left -= 1;
        }
        g
    }
}

/// The voices node: no inputs; 32 stereo outputs (port 0 to the master, the others to the Tone graph's instruments and
/// buses).
#[derive(Clone)]
pub struct VoicesNode {
    pub stats: Arc<Stats>,
}

impl AudioNode for VoicesNode {
    type Configuration = ();

    fn info(&self, _: &()) -> Result<AudioNodeInfo, NodeError> {
        Ok(AudioNodeInfo::new().debug_name("raea voices").channel_config(ChannelConfig { num_inputs: ChannelCount::ZERO, num_outputs: ChannelCount::new((crate::tonegraph::PORTS * 2) as u32).unwrap() }))
    }

    fn construct_processor(&self, _: &(), cx: ConstructProcessorContext) -> Result<impl AudioNodeProcessor, NodeError> {
        Ok(VoicesProc { voices: Vec::with_capacity(512), stats: self.stats.clone(), rate: cx.stream_info.sample_rate.get() as f64, vols: Vec::with_capacity(64) })
    }
}

struct VoicesProc {
    voices: Vec<Voice>,
    stats: Arc<Stats>,
    rate: f64,
    /// each instrument's volume (dB as a linear gain), ramping: (node, gain, target, step, left)
    vols: Vec<(u32, f32, f32, f32, u32)>,
}

fn at_samples(t: Option<EventInstant>) -> Option<i64> {
    match t {
        Some(EventInstant::AtClockSamples(s)) => Some(s.0),
        _ => None,
    }
}

fn env0(gain: f32, fade_in: u32, age: u64) -> Env {
    Env { gain, from: gain, ramp: (0, 0), fade_in, age, stopping: None, hold: 1.0, done: false }
}

impl AudioNodeProcessor for VoicesProc {
    fn events(&mut self, info: &ProcInfo, events: &mut ProcEvents, _: &mut ProcExtra) {
        let clock = info.clock_samples.0;
        let edge = (0.005 * self.rate) as u32;
        for (mut ev, t) in events.drain_with_timestamps() {
            let Some(e) = ev.downcast_mut::<Option<Ev>>().and_then(Option::take) else { continue };
            // late (live: the frame reached the mixer after its sample was played): a start skips in by the lateness,
            // so the voice is where the timeline is
            let late = at_samples(t).map(|s| (clock - s).max(0) as u64).unwrap_or(0);
            if late > 0 {
                self.stats.late.fetch_add(1, Ordering::Relaxed);
                self.stats.late_samples.fetch_add(late, Ordering::Relaxed);
            }
            match e {
                Ev::Start { id, pcm, at, rate, gain, fade_in, port } => {
                    let step = pcm.rate / self.rate * rate;
                    let pos = at * pcm.rate + late as f64 * step;
                    // a late start skips in, and still fades in from there (it started mid-wave: a click)
                    let fade_in = if late > 0 { edge } else { fade_in };
                    self.voices.push(Voice { id, src: Src::Pcm { pcm, pos, step }, env: env0(gain, fade_in, 0), port, node: None });
                }
                Ev::ToneOn { id, node, port, inst, freq, vel, dur, seed, offset, play_dur } => {
                    // a monophonic instrument (Tone's Synth, MonoSynth, ...) plays its next note on the same voice
                    if inst.mono
                        && let Some(v) = self.voices.iter_mut().rev().find(|v| v.node == Some(node) && v.env.stopping.is_none() && !v.env.done)
                        && let Src::Tone(tv) = &mut v.src
                    {
                        tv.retrigger(freq, vel, dur.map(|d| d as u64));
                        continue;
                    }
                    // polyphony: past the limit the oldest note of the instrument is stolen (a 5 ms fade, no click)
                    let playing = self.voices.iter().filter(|v| v.node == Some(node) && v.env.stopping.is_none()).count();
                    if playing >= inst.max_poly
                        && let Some(old) = self.voices.iter_mut().find(|v| v.node == Some(node) && v.env.stopping.is_none())
                    {
                        old.env.stopping = Some((edge.max(1), edge.max(1)));
                    }
                    let mut tv = crate::tone::ToneVoice::new(inst, freq, vel, dur.map(|d| d as u64), self.rate, seed, offset, play_dur);
                    tv.skip(late);
                    self.voices.push(Voice { id, src: Src::Tone(Box::new(tv)), env: env0(1.0, if late > 0 { edge } else { 0 }, 0), port, node: Some(node) });
                }
                Ev::ToneOff { node, freq } => {
                    for v in self.voices.iter_mut().filter(|v| v.node.is_some() && (node == u32::MAX || v.node == Some(node))) {
                        if node == u32::MAX {
                            // a seek or a pause: stopped, not released
                            if v.env.stopping.is_none() {
                                v.env.stopping = Some((edge.max(1), edge.max(1)));
                            }
                            continue;
                        }
                        if let Src::Tone(tv) = &mut v.src {
                            let player = matches!(tv.inst.kind, crate::tone::Kind::Player);
                            let matches = freq.is_none_or(|f| (tv.freq - f).abs() < 0.01 * f.max(1.0) || player);
                            if !matches || tv.releasing() {
                                continue;
                            }
                            if player {
                                let fade = ((tv.inst.fade_out * self.rate) as u32).max(edge);
                                if v.env.stopping.is_none() {
                                    v.env.stopping = Some((fade.max(1), fade.max(1)));
                                }
                            } else {
                                tv.release();
                            }
                        }
                    }
                }
                Ev::ToneVol { node, db, ramp } => {
                    let g = 10f32.powf(db as f32 / 20.0);
                    match self.vols.iter_mut().find(|x| x.0 == node) {
                        Some(x) => {
                            if ramp == 0 {
                                (x.1, x.2, x.4) = (g, g, 0);
                            } else {
                                (x.2, x.3, x.4) = (g, (g - x.1) / ramp as f32, ramp);
                            }
                        }
                        None => self.vols.push((node, if ramp == 0 { g } else { 1.0 }, g, if ramp == 0 { 0.0 } else { (g - 1.0) / ramp as f32 }, ramp)),
                    }
                }
                Ev::Gain { id, gain, ramp } => {
                    if let Some(v) = self.voices.iter_mut().find(|v| v.id == id && v.env.stopping.is_none()).map(|v| &mut v.env) {
                        // from where the gain is now
                        let now = if v.ramp.0 > 0 { v.gain + (v.from - v.gain) * (v.ramp.0 as f32 / v.ramp.1.max(1) as f32) } else { v.gain };
                        v.from = now;
                        v.gain = gain;
                        v.ramp = (ramp, ramp);
                    }
                }
                Ev::Stop { id, fade } => {
                    if let Some(v) = self.voices.iter_mut().find(|v| v.id == id && v.env.stopping.is_none()).map(|v| &mut v.env) {
                        v.stopping = Some((fade.max(1), fade.max(1)));
                    }
                }
            }
        }
        self.stats.voices.store(self.voices.len() as u64, Ordering::Relaxed);
    }

    fn process(&mut self, info: &ProcInfo, buffers: ProcBuffers, _: &mut ProcExtra) -> ProcessStatus {
        let n = info.frames;
        for ch in buffers.outputs.iter_mut() {
            ch[..n].fill(0.0);
        }
        if self.voices.is_empty() {
            return ProcessStatus::OutputsModified;
        }
        let back = (1.0 / (0.005 * self.rate)) as f32;
        // instruments' volumes, one ramp per block sample
        let mut vol_at: Vec<(u32, Vec<f32>)> = Vec::new();
        if !self.vols.is_empty() {
            for x in self.vols.iter_mut() {
                let mut g = Vec::with_capacity(n);
                for _ in 0..n {
                    if x.4 > 0 {
                        x.1 += x.3;
                        x.4 -= 1;
                        if x.4 == 0 {
                            x.1 = x.2;
                        }
                    }
                    g.push(x.1);
                }
                vol_at.push((x.0, g));
            }
        }
        for v in self.voices.iter_mut() {
            let e = &mut v.env;
            let port = v.port.min(crate::tonegraph::PORTS - 1);
            let (l_out, r_out) = {
                let (a, b) = buffers.outputs.split_at_mut(port * 2 + 1);
                (&mut a[port * 2][..n], &mut b[0][..n])
            };
            match &mut v.src {
                Src::Pcm { pcm, pos, step } => {
                    let pcm = pcm.clone();
                    let len = pcm.len_s * pcm.rate;
                    // the lock is only ever held for a map's insert or remove (blocks are decoded and freed outside it)
                    let guard = pcm.blocks.lock().ok();
                    let Some(mut blocks) = guard else {
                        *pos += *step * n as f64;
                        e.age += n as u64;
                        e.hold = 0.0;
                        self.stats.missing.fetch_add(n as u64, Ordering::Relaxed);
                        continue;
                    };
                    for i in 0..n {
                        if *pos >= len {
                            e.done = true;
                            break;
                        }
                        let g = e.level();
                        if e.done {
                            break;
                        }
                        match pcm.sample(&mut blocks, *pos) {
                            Some((l, r)) => {
                                let g = g * e.hold;
                                e.hold = (e.hold + back).min(1.0);
                                l_out[i] += l * g;
                                r_out[i] += r * g;
                            }
                            None => {
                                e.hold = 0.0;
                                self.stats.missing.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        *pos += *step;
                        e.age += 1;
                    }
                }
                Src::Tone(tv) => {
                    let vol = v.node.and_then(|nd| vol_at.iter().find(|x| x.0 == nd)).map(|x| &x.1);
                    for i in 0..n {
                        let g = e.level();
                        if e.done {
                            break;
                        }
                        let (l, r) = tv.next();
                        if tv.done {
                            e.done = true;
                            break;
                        }
                        let g = g * vol.map(|x| x[i]).unwrap_or(1.0);
                        l_out[i] += l * g;
                        r_out[i] += r * g;
                        e.age += 1;
                    }
                }
            }
        }
        // in order (the sum's order is the voices' order): retain, not swap_remove
        self.voices.retain(|v| !v.env.done);
        ProcessStatus::OutputsModified
    }
}

/// How far the limiter looks ahead (it delays the sound this much) and its default ceiling, -1 dBFS.
pub const LOOKAHEAD: f64 = 0.0015;
pub const CEILING: f32 = 0.891;

/// The master's settings: gain on what comes in, the ceiling (linear), and whether the device hears it (the tap
/// always does).
pub struct Master {
    pub pre: f32,
    pub ceiling: f32,
    /// a track mastered already (a timeline's mastered bounce): through as it is, no gain, no limiter, no delay
    pub bypass: bool,
}

/// The master node: stereo in, stereo out: the look-ahead true-peak limiter (Limiter, below), as a node.
#[derive(Clone)]
pub struct MasterNode {
    /// RAEA_AUDIO_TAP: what the limiter gave, before the mute and the device (interleaved stereo)
    pub tap: Option<Arc<Mutex<Vec<f32>>>>,
    pub mute: bool,
}

impl AudioNode for MasterNode {
    type Configuration = ();

    fn info(&self, _: &()) -> Result<AudioNodeInfo, NodeError> {
        Ok(AudioNodeInfo::new().debug_name("raea master").channel_config(ChannelConfig { num_inputs: ChannelCount::STEREO, num_outputs: ChannelCount::STEREO }))
    }

    fn construct_processor(&self, _: &(), cx: ConstructProcessorContext) -> Result<impl AudioNodeProcessor, NodeError> {
        Ok(self.processor(cx.stream_info.sample_rate.get() as f64))
    }
}

impl MasterNode {
    /// The master's processor at `rate` (for a node type that wraps this one: raea-seedling's CoreLimiter).
    pub fn processor(&self, rate: f64) -> impl AudioNodeProcessor + use<> {
        MasterProc { lim: Limiter::new(), rate, tap: self.tap.clone(), mute: self.mute, bypass: false }
    }
}

struct MasterProc {
    lim: Limiter,
    rate: f64,
    tap: Option<Arc<Mutex<Vec<f32>>>>,
    mute: bool,
    bypass: bool,
}

impl AudioNodeProcessor for MasterProc {
    fn events(&mut self, _: &ProcInfo, events: &mut ProcEvents, _: &mut ProcExtra) {
        for mut ev in events.drain() {
            if let Some(m) = ev.downcast_mut::<Option<Master>>().and_then(Option::take) {
                self.lim.pre = m.pre;
                self.lim.ceiling = m.ceiling;
                self.bypass = m.bypass;
            }
        }
    }

    fn process(&mut self, info: &ProcInfo, buffers: ProcBuffers, _: &mut ProcExtra) -> ProcessStatus {
        let n = info.frames;
        let (l_out, rest) = buffers.outputs.split_first_mut().unwrap();
        let (l_out, r_out) = (&mut l_out[..n], &mut rest[0][..n]);
        l_out.copy_from_slice(&buffers.inputs[0][..n]);
        r_out.copy_from_slice(&buffers.inputs[1][..n]);
        if !self.bypass {
            self.lim.run(l_out, r_out, self.rate);
        }
        if let Some(tap) = &self.tap
            && let Ok(mut t) = tap.lock()
        {
            for i in 0..n {
                t.push(l_out[i]);
                t.push(r_out[i]);
            }
        }
        if self.mute {
            l_out.fill(0.0);
            r_out.fill(0.0);
        }
        ProcessStatus::OutputsModified
    }
}

/// The look-ahead true-peak limiter, streaming the render master's rule (engine/media/src/audio.rs `limit`): each sample's
/// need is the gain that keeps its true peak under the ceiling; the gain falls linearly, by at most 1/lookahead a sample,
/// to the least need in the look-ahead window (so it is there before the peak leaves the window) and is let go over 80
/// ms; each sample also takes no more than its own need. It never overshoots, so nothing is clamped (a clamp made a
/// corner at every peak of a limited kick: a click each cycle). The true peak is read 4x oversampled through a 48-tap
/// windowed sinc, as BS.1770 meters read it.
pub struct Limiter {
    delay: Vec<[f32; 2]>,
    need: Vec<f32>,
    at: usize,
    gain: f32,
    pub pre: f32,
    pub ceiling: f32,
    /// each channel's last TAPS samples in
    hist: [[f32; TAPS]; 2],
    taps: [[f32; TAPS]; 3],
}

const TAPS: usize = 12;

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Limiter {
    pub fn new() -> Limiter {
        // the samples between hist[5] and hist[6] at a quarter, half and three quarters
        let mut taps = [[0f32; TAPS]; 3];
        for (k, p) in [0.25f64, 0.5, 0.75].iter().enumerate() {
            for (j, t) in taps[k].iter_mut().enumerate() {
                let x = 5.0 + p - j as f64;
                let sinc = if x.abs() < 1e-12 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
                // a Hann window over +-6 samples
                let w = 0.5 * (1.0 + (std::f64::consts::PI * x / 6.0).cos());
                *t = (sinc * w) as f32;
            }
        }
        Limiter { delay: Vec::new(), need: Vec::new(), at: 0, gain: 1.0, pre: 1.0, ceiling: CEILING, hist: [[0.0; TAPS]; 2], taps }
    }

    pub fn run(&mut self, l: &mut [f32], r: &mut [f32], rate: f64) {
        let n = ((LOOKAHEAD * rate).round() as usize).max(8);
        if self.need.len() != n {
            self.need = vec![1.0; n];
            self.delay = vec![[0.0; 2]; n];
            self.at = 0;
        }
        let release = (-1.0 / (0.080 * rate)).exp() as f32;
        let attack = 1.0 / n as f32;
        // aimed 0.26 dB under: the 12-tap meter reads a peak between samples up to ~1% low
        let (pre, ceiling) = (self.pre, self.ceiling * 0.97);
        let need = |v: f32| if v > ceiling { ceiling / v } else { 1.0 };
        for i in 0..l.len() {
            let frame = [l[i] * pre, r[i] * pre];
            // the sample leaving the window: its own need, settled
            let own = self.need[self.at];
            let late = std::mem::replace(&mut self.delay[self.at], frame);
            self.need[self.at] = need(frame[0].abs().max(frame[1].abs()));
            // the true peak between the samples 6 and 5 back (the oversampler's delay), on both their slots
            let mut peak = 0f32;
            for c in 0..2 {
                let h = &mut self.hist[c];
                h.copy_within(1.., 0);
                h[TAPS - 1] = frame[c];
                for tp in &self.taps {
                    let v: f32 = tp.iter().zip(h.iter()).map(|(a, b)| a * b).sum();
                    peak = peak.max(v.abs());
                }
            }
            let g = need(peak);
            for back in [6usize, 5] {
                let k = (self.at + n - back) % n;
                self.need[k] = self.need[k].min(g);
            }
            let want = self.need.iter().cloned().fold(1.0f32, f32::min).min(own);
            self.gain = if want < self.gain { (self.gain - attack).max(want) } else { want + (self.gain - want) * release };
            let k = self.gain.min(own);
            l[i] = late[0] * k;
            r[i] = late[1] * k;
            self.at = (self.at + 1) % n;
        }
    }
}

#[cfg(test)]
mod limiter_tests {
    use super::*;

    #[test]
    fn a_loud_wave_stays_under_the_ceiling_with_no_clamp_and_quiet_sound_passes() {
        let rate = 48_000.0;
        // a 2.0 sine at a quarter of the rate, 45 degrees off the samples: the true peak sits 3 dB over the samples
        let n = 48_000;
        let wave = |amp: f32| -> Vec<f32> { (0..n).map(|i| amp * (std::f32::consts::FRAC_PI_2 * i as f32 + std::f32::consts::FRAC_PI_4).sin()).collect() };
        let mut lim = Limiter::new();
        let (mut l, mut r) = (wave(2.0), wave(2.0));
        for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            lim.run(a, b, rate);
        }
        // the sample peak under the ceiling less the 3 dB the true peak adds
        let sp = l.iter().skip(4800).fold(0f32, |m, v| m.max(v.abs()));
        assert!(sp <= CEILING * 0.7072 + 1e-3, "sample peak {sp} lets the true peak over {CEILING}");
        // quiet: through unchanged, only later by the look-ahead
        let mut q = Limiter::new();
        let (mut a, mut b) = (wave(0.3), wave(0.3));
        let input = a.clone();
        q.run(&mut a, &mut b, rate);
        let d = ((LOOKAHEAD * rate).round() as usize).max(8);
        assert!((d..n).all(|i| (a[i] - input[i - d]).abs() < 1e-6), "quiet sound changed");
    }
}

/// An event for a node: the payload goes in an Option so the audio thread can take it and leave the empty box to be
/// dropped off the audio thread.
pub fn event<T: Send + 'static>(v: T) -> NodeEventType {
    NodeEventType::custom(Some(v))
}
