//! Frames to voice events: the one rule both lanes use. A frame's sounds (each element's source at its source time,
//! gain and fades) are compared with the frame before: an element whose source time moved on by its rate is the same
//! voice, a new one (or one that jumped: a cut, a seek, a loop) starts a voice, one that's gone stops its voice, a gain
//! that changed ramps to the new gain over a frame. Each event lands on its frame's sample of the timeline
//! (round(f * rate / fps)), so live play (a frame at a time, as it's stepped) and a render (every frame, in order) put
//! the same events on the same samples. This is the render program's rule (motion.rs `program`: runs of a key and a
//! source whose time moves on by rate/fps) made into events instead of rows.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::Sound;
use crate::pcm::Pcm;

/// The default fade at a voice's ends (5 ms: no click), as rdb's lanes fade.
pub const EDGE: f64 = 0.005;

pub enum Ev {
    /// a voice starts reading `pcm` at `at` source seconds, `rate` times as fast, at `gain`, fading in over `fade_in`
    /// samples, out of the voices node's `port` (0: the master; a bus's port: that bus)
    Start { id: u64, pcm: Arc<Pcm>, at: f64, rate: f64, gain: f32, fade_in: u32, port: usize },
    /// its gain ramps linearly to `gain` over `ramp` samples
    Gain { id: u64, gain: f32, ramp: u32 },
    /// it fades out over `fade` samples and goes
    Stop { id: u64, fade: u32 },
    /// a Tone instrument's note on (attack, or attack-release over `dur` samples), out of its port
    ToneOn { id: u64, node: u32, port: usize, inst: Arc<crate::tone::Inst>, freq: f64, vel: f32, dur: Option<u32>, seed: u64, offset: f64, play_dur: Option<f64> },
    /// its notes released (one frequency, or all of them: releaseAll, a Player's stop)
    ToneOff { node: u32, freq: Option<f64> },
    /// an instrument's volume (dB) over `ramp` samples
    ToneVol { node: u32, db: f64, ramp: u32 },
}

/// A Tone event's destination: the voices node, or one of the graph's ToneFx nodes (by its Tone id).
pub enum ToneOut {
    Voice(Ev),
    Fx(u32, crate::fx::FxEv),
}

struct Run {
    id: u64,
    pcm: Arc<Pcm>,
    f: i64,
    at: f64,
    rate: f64,
    volume: f32,
    fade_out: f64,
}

pub struct Score {
    pub fps: f64,
    pub rate: u32,
    next: u64,
    runs: BTreeMap<String, Run>,
    /// elements heard before (a fade-in is the element's first run's only)
    heard: std::collections::HashSet<String>,
    heard_notes: std::collections::HashSet<String>,
    /// a frame's sounds' keys ("key|file"), written into one string and found by span, sorted: kept between frames so a
    /// frame where nothing starts or stops allocates nothing (ZeroHeap; a key was format!-ed per sound per frame)
    keys: String,
    spans: Vec<(usize, usize, usize)>,
}

impl Score {
    pub fn new(fps: f64, rate: u32) -> Score {
        Score { fps, rate, next: 1, runs: BTreeMap::new(), heard: Default::default(), heard_notes: Default::default(), keys: String::new(), spans: Vec::new() }
    }

    /// Frame `f`'s sample on the timeline.
    pub fn t(&self, f: i64) -> i64 {
        (f as f64 * self.rate as f64 / self.fps).round() as i64
    }

    fn samples(&self, s: f64) -> u32 {
        (s * self.rate as f64).round().max(0.0) as u32
    }

    /// A fresh id for a voice made outside the frames (an instrument's note).
    pub fn id(&mut self) -> u64 {
        self.next += 1;
        self.next - 1
    }

    /// Frame `f`'s events, each with its timeline sample. `sounds` pairs each sound with its opened source (sounds
    /// whose source isn't open yet are left out by the caller; they start at the frame their source opens).
    pub fn frame<S: std::borrow::Borrow<Sound>>(&mut self, f: i64, sounds: &[(S, Arc<Pcm>)], buses: &std::collections::HashMap<String, usize>) -> Vec<(i64, Ev)> {
        self.frame_with(f, sounds.len(), |i| (sounds[i].0.borrow(), &sounds[i].1), buses)
    }

    /// `frame` over `n` sounds read by index (the live mixer's kept list: ZeroHeap)
    pub fn frame_with<'s>(&mut self, f: i64, n: usize, sound: impl Fn(usize) -> (&'s Sound, &'s Arc<Pcm>), buses: &std::collections::HashMap<String, usize>) -> Vec<(i64, Ev)> {
        let t = self.t(f);
        let ramp = self.samples(1.0 / self.fps);
        let edge = self.samples(EDGE);
        let mut out = Vec::new();
        // sorted by key: the voices' order (and so the order the mix sums them in) is the same whatever order the
        // page's elements were read in. The keys are written into one kept string; spans index it (start, end, sound)
        let (mut keys, mut spans) = (std::mem::take(&mut self.keys), std::mem::take(&mut self.spans));
        keys.clear();
        spans.clear();
        for i in 0..n {
            use std::fmt::Write as _;
            let s: &Sound = sound(i).0;
            let a = keys.len();
            let _ = write!(keys, "{}|{}", s.key, s.file.display());
            spans.push((a, keys.len(), i));
        }
        let at = |sp: &(usize, usize, usize)| &keys[sp.0..sp.1];
        spans.sort_by(|a, b| at(a).cmp(at(b)));
        spans.dedup_by(|a, b| at(a) == at(b));
        // gone: the element left, its voice fades out (its own fade-out when it has one)
        let gone: Vec<String> = self.runs.keys().filter(|k| spans.binary_search_by(|sp| at(sp).cmp(k.as_str())).is_err()).cloned().collect();
        for k in gone {
            let r = self.runs.remove(&k).unwrap();
            let fade = self.samples(if r.fade_out > 0.0 { r.fade_out } else { EDGE });
            out.push((t, Ev::Stop { id: r.id, fade }));
        }
        for sp in &spans {
            let k = at(sp);
            let (s, pcm): (&Sound, _) = sound(sp.2);
            if let Some(r) = self.runs.get_mut(k) {
                let moved = r.at + r.rate * (f - r.f) as f64 / self.fps;
                if f > r.f && (s.at - moved).abs() < 1e-6 && (s.rate - r.rate).abs() < 1e-9 {
                    if (s.volume - r.volume).abs() > 1e-9 {
                        out.push((t, Ev::Gain { id: r.id, gain: s.volume, ramp }));
                    }
                    (r.f, r.at, r.volume, r.fade_out) = (f, s.at, s.volume, s.fade_out);
                    continue;
                }
                // a jump: the old voice fades out as the new one fades in
                out.push((t, Ev::Stop { id: r.id, fade: edge }));
                self.runs.remove(k);
            }
            let first = self.heard.insert(k.to_string());
            // a negative fade is none at all (a mastered track: its fades are in it already)
            let fade_in = if s.fade_in < 0.0 { 0 } else if first && s.fade_in > 0.0 { self.samples(s.fade_in) } else { edge };
            let id = self.next;
            self.next += 1;
            let port = s.bus.as_ref().and_then(|b| buses.get(&**b)).copied().unwrap_or(0);
            out.push((t, Ev::Start { id, pcm: pcm.clone(), at: s.at, rate: s.rate, gain: s.volume, fade_in, port }));
            self.runs.insert(k.to_string(), Run { id, pcm: pcm.clone(), f, at: s.at, rate: s.rate, volume: s.volume, fade_out: s.fade_out });
        }
        (self.keys, self.spans) = (keys, spans);
        out
    }

    /// Every voice stops (fading out) at frame `f`: the end of a render, a pause, a seek.
    pub fn stop_all(&mut self, f: i64) -> Vec<(i64, Ev)> {
        let t = self.t(f);
        let edge = self.samples(EDGE);
        self.heard_notes.clear();
        let mut out: Vec<(i64, Ev)> = std::mem::take(&mut self.runs).into_values().map(|r| (t, Ev::Stop { id: r.id, fade: edge })).collect();
        // every instrument's notes too (node u32::MAX: all of them), stopped rather than released
        out.push((t, Ev::ToneOff { node: u32::MAX, freq: None }));
        out
    }

    /// Each playing voice's source and where in it the frame is (seconds): which blocks to have loaded.
    pub fn positions(&self) -> Vec<(Arc<Pcm>, f64)> {
        self.positions_iter().map(|(p, at)| (p.clone(), at)).collect()
    }

    /// `positions`, borrowed
    pub fn positions_iter(&self) -> impl Iterator<Item = (&Arc<Pcm>, f64)> {
        self.runs.values().map(|r| (&r.pcm, r.at))
    }

    /// Frame `f`'s Tone events (@raea/motion/tone: each with its key, node, time on the timeline and what it does). Each
    /// fires once, on its own sample (not the frame's: a note between two frames starts between them). An event is
    /// shown from the frame its time falls in; one shown again on later frames is the same event.
    /// Frame `f`'s own samples (ToneCue::pcm) as a voice from the frame's sample, at unity gain, to the master.
    pub fn chunk(&mut self, f: i64, a: &raea_media::Audio) -> Option<(i64, Ev)> {
        self.chunk_on(f, a, 0)
    }

    /// `chunk` into one of the graph's ports (a bus's), 0 the master.
    pub fn chunk_on(&mut self, f: i64, a: &raea_media::Audio, port: usize) -> Option<(i64, Ev)> {
        if a.len() == 0 {
            return None;
        }
        let id = self.id();
        Some((self.t(f), Ev::Start { id, pcm: Pcm::from_audio(&format!("chunk{f}"), a), at: 0.0, rate: 1.0, gain: 1.0, fade_in: 0, port }))
    }

    pub fn tone(&mut self, f: i64, built: &crate::tonegraph::Built, evs: &[serde_json::Value]) -> Vec<(i64, ToneOut)> {
        let _ = f;
        let mut list: Vec<&serde_json::Value> = evs.iter().collect();
        let key = |e: &serde_json::Value| e.get("k").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let time = |e: &serde_json::Value| e.get("t").and_then(|v| v.as_f64()).unwrap_or(0.0);
        list.sort_by(|a, b| time(a).total_cmp(&time(b)).then_with(|| key(a).cmp(&key(b))));
        let mut out = Vec::new();
        for e in list {
            let k = key(e);
            if !self.heard_notes.insert(k.clone()) {
                continue;
            }
            let node = e.get("n").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let t = (time(e) * self.rate as f64).round() as i64;
            let seed = k.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
            let freqs: Vec<f64> = e.get("f").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
            let vel = e.get("v").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
            let kind = e.get("e").and_then(|v| v.as_str()).unwrap_or("");
            let inst = built.insts.get(&node).cloned();
            match (kind, inst) {
                ("ar" | "a", Some((inst, port))) => {
                    let dur = (kind == "ar").then(|| self.samples(e.get("d").and_then(|v| v.as_f64()).unwrap_or(0.25)));
                    for (j, fr) in freqs.iter().enumerate() {
                        let id = self.next;
                        self.next += 1;
                        out.push((t, ToneOut::Voice(Ev::ToneOn { id, node, port, inst: inst.clone(), freq: *fr, vel, dur, seed: seed.wrapping_add(j as u64), offset: 0.0, play_dur: None })));
                    }
                }
                ("start", Some((inst, port))) => {
                    let id = self.next;
                    self.next += 1;
                    let offset = e.get("o").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let play_dur = e.get("d").and_then(|v| v.as_f64());
                    out.push((t, ToneOut::Voice(Ev::ToneOn { id, node, port, inst, freq: 261.6255653005986, vel, dur: None, seed, offset, play_dur })));
                }
                ("r", Some(_)) => {
                    for fr in &freqs {
                        out.push((t, ToneOut::Voice(Ev::ToneOff { node, freq: Some(*fr) })));
                    }
                }
                ("ra" | "stop", Some(_)) => out.push((t, ToneOut::Voice(Ev::ToneOff { node, freq: None }))),
                ("param", inst) => {
                    let name = e.get("p").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let value = e.get("val").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let ramp = self.samples(e.get("ramp").and_then(|v| v.as_f64()).unwrap_or(0.0));
                    let exp = e.get("curve").and_then(|v| v.as_str()) == Some("exp");
                    if inst.is_some() {
                        if name == "volume" {
                            out.push((t, ToneOut::Voice(Ev::ToneVol { node, db: value, ramp })));
                        }
                    } else if built.fx.contains_key(&node) {
                        out.push((t, ToneOut::Fx(node, crate::fx::FxEv::Param { name, value, ramp, exp })));
                    }
                }
                _ => {}
            }
        }
        out
    }
}
