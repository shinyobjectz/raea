//! Live sound: the composition's <Audio> and <Video> sound played by a native mixer as the frames reach it (One
//! renderer, the native stage; no HTML audio). Each sounding element is a voice: its source decoded once to PCM,
//! started at its source time, resampled to the device's rate as it plays, with the element's gain and fades. A voice
//! that drifts from the frame clock by more than 15 ms is put back in place; one that ends or leaves fades out over 5 ms
//! so nothing clicks. Generated sounds (lyria, sao) play from what was already made; live play never generates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::{Map, Value};

type R<T> = Result<T, String>;

/// One element's sound this frame.
#[derive(Clone, Debug)]
pub struct Sound {
    pub key: String,
    pub file: PathBuf,
    /// seconds into the source this frame shows
    pub at: f64,
    pub volume: f32,
    pub rate: f64,
    pub fade_in: f64,
    pub fade_out: f64,
}

/// A source's sound, streamed: 4 s blocks decoded on a loader thread just ahead of the voices that play it and let go
/// when no voice is near (a ten-minute take decoded whole stalled a frame 1.9 s and held 1.2 GB).
struct Pcm {
    file: PathBuf,
    rate: f64,
    /// seconds of sound in the source (decode_audio's time)
    len_s: f64,
    blocks: Mutex<HashMap<u64, Arc<(Vec<f32>, Vec<f32>)>>>,
    asked: Mutex<std::collections::HashSet<u64>>,
}

const BLOCK: f64 = 4.0;

impl Pcm {
    fn block_len(&self) -> usize {
        (BLOCK * self.rate) as usize
    }

    /// The sample at `pos` (source samples) if its block is loaded; the callback never waits for one.
    fn sample(&self, blocks: &HashMap<u64, Arc<(Vec<f32>, Vec<f32>)>>, pos: f64) -> Option<(f32, f32)> {
        let bl = self.block_len();
        let i = pos as usize;
        let b = blocks.get(&((i / bl) as u64))?;
        let j = i % bl;
        let (l, r) = (&b.0, &b.1);
        if j + 1 >= l.len() {
            return l.get(j).map(|x| (*x, r.get(j).copied().unwrap_or(*x)));
        }
        let t = (pos - i as f64) as f32;
        Some((l[j] * (1.0 - t) + l[j + 1] * t, r[j] * (1.0 - t) + r[j + 1] * t))
    }

    /// The blocks a voice at `pos` needs (this one and the next), asked of the loader; the rest of the blocks let go
    /// unless `keep` holds them for another voice.
    fn want(self: &Arc<Self>, pos: f64, loader: &Sender<(Arc<Pcm>, u64)>) -> Vec<u64> {
        let b = (pos as usize / self.block_len().max(1)) as u64;
        let need = vec![b, b + 1];
        let have: Vec<u64> = self.blocks.lock().unwrap().keys().copied().collect();
        let mut asked = self.asked.lock().unwrap();
        for k in &need {
            if !have.contains(k) && (*k as f64) * BLOCK < self.len_s && asked.insert(*k) {
                let _ = loader.send((self.clone(), *k));
            }
        }
        need
    }
}

/// The loader: blocks decoded off the frame and audio threads.
fn loader() -> Sender<(Arc<Pcm>, u64)> {
    let (tx, rx) = std::sync::mpsc::channel::<(Arc<Pcm>, u64)>();
    std::thread::Builder::new()
        .name("raea audio loader".into())
        .spawn(move || {
            for (pcm, k) in rx {
                let got = raea_media::decode_audio_window(&pcm.file, k as f64 * BLOCK, BLOCK);
                if let Ok(a) = got {
                    let l = a.channels.first().cloned().unwrap_or_default();
                    let r = a.channels.get(1).cloned().unwrap_or_else(|| l.clone());
                    pcm.blocks.lock().unwrap().insert(k, Arc::new((l, r)));
                }
                pcm.asked.lock().unwrap().remove(&k);
            }
        })
        .expect("audio loader thread");
    tx
}

struct Voice {
    pcm: Arc<Pcm>,
    /// position in the source, in source samples
    pos: f64,
    rate: f64,
    /// the small rate correction that pulls a voice back into step without a jump
    nudge: f64,
    gain: f32,
    target: f32,
    fade_in: f64,
    fade_out: f64,
    /// samples of the 5 ms stop fade left (None: playing)
    stopping: Option<u32>,
    /// where the last callback started this voice, and its source samples per device sample then
    pos_cb: f64,
    step_cb: f64,
    /// a voice (re)placed: when, the source sample the timeline wanted then, and source samples per second; its next
    /// callback starts it where the timeline is when that callback's first sample is heard
    anchor: Option<(std::time::Instant, f64, f64)>,
}

struct State {
    voices: HashMap<String, Voice>,
    /// RAEA_AUDIO_MUTE=1: everything plays and keeps time, silently (tests on someone's machine)
    mute: bool,
    rate: f64,
    /// device samples played, for a clock
    played: u64,
    /// when the last callback started, and how long its first sample takes to be heard (the device's output latency)
    cb_at: Option<std::time::Instant>,
    latency: f64,
    /// RAEA_AUDIO_TAP=OUT.wav: what went to the device (before the mute), interleaved, written when the mixer goes
    tap: Option<(PathBuf, Vec<f32>, usize)>,
    limiter: Limiter,
}

/// How far the limiter looks ahead (it delays the sound this much; the clock counts it as latency) and its ceiling,
/// -1 dBFS as renders master to.
const LOOKAHEAD: f64 = 0.0015;
const CEILING: f32 = 0.891;

/// A peak limiter: each frame's gain is the least any frame in the look-ahead window needs to stay under the ceiling,
/// reached over that window (no step, no click) and let go over 80 ms; a last clamp catches what rounding leaves.
struct Limiter {
    delay: Vec<f32>,
    need: Vec<f32>,
    at: usize,
    gain: f32,
    /// the master's gain on what comes in (a game's voices, as its render masters them) and the ceiling (linear)
    pre: f32,
    ceiling: f32,
    /// each channel's last three samples in, for the peak between samples
    hist: Vec<[f32; 3]>,
}

impl Limiter {
    fn new() -> Limiter {
        Limiter { delay: Vec::new(), need: Vec::new(), at: 0, gain: 1.0, pre: 1.0, ceiling: CEILING, hist: Vec::new() }
    }

    fn run(&mut self, out: &mut [f32], ch: usize, rate: f64) {
        let n = ((LOOKAHEAD * rate).round() as usize).max(1);
        if self.need.len() != n || self.delay.len() != n * ch {
            self.need = vec![1.0; n];
            self.delay = vec![0.0; n * ch];
            self.at = 0;
        }
        let attack = 1.0 - (-3.0 / n as f32).exp();
        let release = 1.0 - (-1.0 / (0.080 * rate as f32)).exp();
        // aimed 0.3 dB under the ceiling: the gain's ramps and the last clamp reshape the waveform a little after
        // detection (0.15 dB left pong 0.1 dB over its -1.5 dBTP ceiling once it played at its render's full gain; the
        // render's master aims lower the same way when its true peak gets over)
        let (pre, ceiling) = (self.pre, self.ceiling * 0.966);
        if self.hist.len() != ch {
            self.hist = vec![[0.0; 3]; ch];
        }
        for frame in out.chunks_exact_mut(ch) {
            // each sample, and the true peak half a sample before it (a cubic through the four around it): the ceiling
            // is a true-peak one, as the render's master holds it (sample peaks alone let -1.2 dBTP past a -1.5 ceiling)
            let mut peak = 0f32;
            for (c, x) in frame.iter_mut().enumerate() {
                *x *= pre;
                let h = &mut self.hist[c];
                // between h[1] and h[2] at a quarter, half and three quarters (Catmull-Rom through h[0], h[1], h[2], x):
                // the 4x oversampled peak true-peak meters read
                let (p0, p1, p2, p3) = (h[0], h[1], h[2], *x);
                for t in [0.25f32, 0.5, 0.75] {
                    let v = 0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t);
                    peak = peak.max(v.abs());
                }
                peak = peak.max(x.abs());
                *h = [h[1], h[2], *x];
            }
            self.need[self.at] = if peak > ceiling { ceiling / peak } else { 1.0 };
            let want = self.need.iter().cloned().fold(1.0f32, f32::min);
            self.gain += (want - self.gain) * if want < self.gain { attack } else { release };
            for (c, x) in frame.iter_mut().enumerate() {
                let late = std::mem::replace(&mut self.delay[self.at * ch + c], *x);
                *x = (late * self.gain).clamp(-ceiling, ceiling);
            }
            self.at = (self.at + 1) % n;
        }
    }
}

impl State {
    /// The source sample of `v` heard at `now`: its position at the last callback, moved on by the time since then
    /// less the latency (between callbacks a voice's position stands still, a buffer at a time).
    fn heard(&self, v: &Voice, now: std::time::Instant) -> f64 {
        match self.cb_at {
            Some(at) => v.pos_cb + ((now - at).as_secs_f64() - self.latency) * self.rate * v.step_cb,
            None => v.pos,
        }
    }
}

pub struct Mixer {
    _stream: cpal::Stream,
    state: Arc<Mutex<State>>,
    pcm: HashMap<PathBuf, Option<Arc<Pcm>>>,
    /// sources being opened on a thread
    opening: HashMap<PathBuf, Receiver<Option<Arc<Pcm>>>>,
    loader: Sender<(Arc<Pcm>, u64)>,
    /// how far each playing voice was from where the frame wanted it, in ms, at each sync (the sync gate)
    pub drift_ms: Vec<f64>,
}

const STOP: f64 = 0.005;
/// past this a voice jumps back in place; under it, its rate is nudged by at most NUDGE until it's in place
const DRIFT: f64 = 0.040;
const NUDGE: f64 = 0.01;

impl Mixer {
    /// The default output device, stereo at its own rate.
    pub fn open() -> R<Mixer> {
        let host = cpal::default_host();
        let dev = host.default_output_device().ok_or("no audio output device")?;
        let cfg = dev.default_output_config().map_err(|e| format!("audio output: {e}"))?;
        let rate = cfg.sample_rate().0 as f64;
        let channels = cfg.channels() as usize;
        let mute = std::env::var("RAEA_AUDIO_MUTE").as_deref() == Ok("1");
        let state = Arc::new(Mutex::new(State { voices: HashMap::new(), mute, rate, played: 0, cb_at: None, latency: LOOKAHEAD, limiter: Limiter::new(), tap: std::env::var_os("RAEA_AUDIO_TAP").map(|p| (PathBuf::from(p), Vec::new(), channels)) }));
        let st = state.clone();
        let config: cpal::StreamConfig = cfg.clone().into();
        let stream = dev
            .build_output_stream(
                &config,
                move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                    let ts = info.timestamp();
                    render(&st, out, channels, ts.playback.duration_since(&ts.callback).map(|d| d.as_secs_f64()));
                },
                |e| eprintln!("raea audio: {e}"),
                None,
            )
            .map_err(|e| format!("audio stream: {e}"))?;
        stream.play().map_err(|e| format!("audio stream: {e}"))?;
        Ok(Mixer { _stream: stream, state, pcm: HashMap::new(), opening: HashMap::new(), loader: loader(), drift_ms: Vec::new() })
    }

    /// A source's sound, once it's open: opening (its rate from a first short window, its length from the container)
    /// reads the file, ~80 ms cold, so it happens on a thread and the voice starts at the sync after (placed where it
    /// should be by then), not holding the frame.
    fn pcm(&mut self, file: &Path) -> Option<Arc<Pcm>> {
        if let Some(p) = self.pcm.get(file) {
            return p.clone();
        }
        let rx = self.opening.entry(file.to_path_buf()).or_insert_with(|| {
            let (tx, rx) = channel();
            let f = file.to_path_buf();
            let _ = std::thread::Builder::new().name("raea audio open".into()).spawn(move || {
                let p = raea_media::decode_audio_window(&f, 0.0, 0.05).ok().map(|a| {
                    let len_s = raea_media::probe_seconds(&f).unwrap_or(f64::MAX);
                    Arc::new(Pcm { file: f.clone(), rate: a.rate as f64, len_s, blocks: Mutex::new(HashMap::new()), asked: Mutex::new(Default::default()) })
                });
                let _ = tx.send(p);
            });
            rx
        });
        match rx.try_recv() {
            Ok(p) => {
                self.opening.remove(file);
                self.pcm.insert(file.to_path_buf(), p.clone());
                p
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(_) => {
                self.opening.remove(file);
                self.pcm.insert(file.to_path_buf(), None);
                None
            }
        }
    }

    /// The master a game's voices go through, as its render masters its sound: `gain_db` on their sum, then the
    /// look-ahead limiter at `ceiling_db` (a timeline's mastered track needs neither: it comes mastered).
    pub fn master(&mut self, gain_db: f64, ceiling_db: f64) {
        let mut st = self.state.lock().unwrap();
        st.limiter.pre = 10f64.powf(gain_db / 20.0) as f32;
        st.limiter.ceiling = 10f64.powf(ceiling_db / 20.0) as f32;
    }

    /// The frame's sounds: new ones start, playing ones keep going (pulled back if they drifted), gone ones fade out.
    /// `lag`: seconds since the frame's own time (it was stepped late by that much), so a voice is judged against
    /// where its source should be now.
    pub fn sync(&mut self, sounds: &[Sound], lag: f64) {
        let pcms: Vec<Option<Arc<Pcm>>> = sounds.iter().map(|s| self.pcm(&s.file)).collect();
        let drift = &mut self.drift_ms;
        let mut st = self.state.lock().unwrap();
        let now = std::time::Instant::now();
        let rate = st.rate;
        let keys: std::collections::HashSet<&str> = sounds.iter().map(|s| s.key.as_str()).collect();
        // an element that left fades out (its own fade-out when it has one: live play learns the end as it comes)
        for (k, v) in st.voices.iter_mut() {
            if !keys.contains(k.as_str()) && v.stopping.is_none() {
                v.stopping = Some((v.fade_out.max(STOP) * rate) as u32);
            }
        }
        for (s, pcm) in sounds.iter().zip(pcms) {
            let Some(pcm) = pcm else { continue };
            let want = (s.at + lag.max(0.0) * s.rate) * pcm.rate;
            let heard = st.voices.get(&s.key).filter(|v| v.stopping.is_none() && v.anchor.is_none()).map(|v| st.heard(v, now));
            let anchor = Some((now, want, s.rate * pcm.rate));
            match (st.voices.get_mut(&s.key), heard) {
                (Some(v), Some(heard)) => {
                    let off = (heard - want) / pcm.rate;
                    drift.push(off * 1e3);
                    if off.abs() > DRIFT {
                        v.anchor = anchor;
                        v.nudge = 0.0;
                    } else {
                        // ahead: a little slower; behind: a little faster, closing the gap over about a quarter second
                        v.nudge = (-off * 4.0).clamp(-NUDGE, NUDGE);
                    }
                    v.target = s.volume;
                    v.rate = s.rate;
                }
                (Some(v), None) if v.stopping.is_none() => {
                    // not started yet: its anchor moves to this sync
                    v.anchor = anchor;
                    v.target = s.volume;
                    v.rate = s.rate;
                }
                _ => {
                    st.voices.insert(
                        s.key.clone(),
                        Voice { pcm: pcm.clone(), pos: want, rate: s.rate, nudge: 0.0, gain: 0.0, target: s.volume, fade_in: s.fade_in, fade_out: s.fade_out, stopping: None, pos_cb: want, step_cb: 0.0, anchor },
                    );
                }
            }
        }
        st.voices.retain(|_, v| v.stopping != Some(0) && v.pos / v.pcm.rate < v.pcm.len_s);
        // each source's blocks: the ones its voices need are asked for, the others let go
        let mut need: HashMap<*const Pcm, Vec<u64>> = HashMap::new();
        for v in st.voices.values() {
            need.entry(Arc::as_ptr(&v.pcm)).or_default().extend(v.pcm.want(v.pos, &self.loader));
        }
        drop(st);
        for p in self.pcm.values().flatten() {
            let keep = need.get(&Arc::as_ptr(p)).cloned().unwrap_or_default();
            p.blocks.lock().unwrap().retain(|k, _| keep.contains(k) || keep.iter().any(|n| *k + 1 == *n));
        }
    }

    /// Everything stops (fading out).
    pub fn stop_all(&mut self) {
        let mut st = self.state.lock().unwrap();
        let n = (STOP * st.rate) as u32;
        for v in st.voices.values_mut() {
            v.stopping.get_or_insert(n);
        }
    }

    /// Seconds of sound the device has played.
    pub fn played(&self) -> f64 {
        let st = self.state.lock().unwrap();
        st.played as f64 / st.rate
    }
}

/// The device's callback: every voice mixed into `out` (interleaved, `channels` wide).
fn render(state: &Arc<Mutex<State>>, out: &mut [f32], channels: usize, latency: Option<f64>) {
    out.fill(0.0);
    let at = std::time::Instant::now();
    let Ok(mut st) = state.lock() else { return };
    let rate = st.rate;
    let frames = out.len() / channels.max(1);
    st.played += frames as u64;
    st.cb_at = Some(at);
    if let Some(l) = latency {
        // the device's latency and the limiter's look-ahead: when this callback's first sample is heard
        st.latency = l + LOOKAHEAD;
    }
    let smooth = 1.0 - (-1.0 / (0.004 * rate)).exp() as f32;
    let st_latency = st.latency;
    for v in st.voices.values_mut() {
        let step = v.pcm.rate / rate * v.rate * (1.0 + v.nudge);
        if let Some((t, want, sps)) = v.anchor.take() {
            // this buffer's first sample is heard latency after the callback began: where the timeline is then
            let since = if at >= t { (at - t).as_secs_f64() } else { -(t - at).as_secs_f64() };
            v.pos = want + (since + st_latency) * sps;
        }
        (v.pos_cb, v.step_cb) = (v.pos, step);
        let len_s = v.pcm.len_s;
        // the blocks, held for this callback only if no loader is inserting right now (never wait)
        let Ok(blocks) = v.pcm.blocks.try_lock() else {
            // silence for this buffer, the voice still keeping time
            if v.stopping.is_none() {
                v.pos += step * frames as f64;
            }
            continue;
        };
        for f in 0..frames {
            if v.pos / v.pcm.rate >= len_s {
                break;
            }
            // a block not loaded yet (a voice started mid-file): silence, and its gain held at zero so the sound ramps in
            // when it arrives (the gain had ramped up meanwhile, and the sound came in as a step: a click, tones fixture)
            let Some((l, r)) = v.pcm.sample(&blocks, v.pos) else {
                v.gain = 0.0;
                if let Some(left) = v.stopping.as_mut() {
                    if *left == 0 {
                        break;
                    }
                    *left -= 1;
                }
                v.pos += step;
                continue;
            };
            v.gain += (v.target - v.gain) * smooth;
            let secs = v.pos / v.pcm.rate;
            let mut g = v.gain;
            if v.fade_in > 0.0 && secs < v.fade_in {
                g *= (secs / v.fade_in) as f32;
            }
            let _ = len_s;
            if let Some(left) = v.stopping.as_mut() {
                if *left == 0 {
                    break;
                }
                g *= (*left as f32 / (v.fade_out.max(STOP) * rate) as f32).min(1.0);
                *left -= 1;
            }
            let o = f * channels;
            out[o] += l * g;
            if channels > 1 {
                out[o + 1] += r * g;
            }
            v.pos += step;
        }
    }
    // the ceiling: a look-ahead limiter (a game's voices summed can go over; a timeline's mastered track never does)
    let mute = st.mute;
    st.limiter.run(out, channels.max(1), rate);
    if let Some((_, buf, _)) = st.tap.as_mut() {
        buf.extend_from_slice(out);
    }
    if mute {
        out.fill(0.0);
    }
}

/// The sounds a page shows this frame, read from its React elements as a render reads them (motion.rs push_sound):
/// each audible <Audio>/<Video> with its source time, gain, rate and fades. Generated ones resolve to what was made.
pub fn sounds_of(items: &[(String, Map<String, Value>, Option<u64>)], dir: &Path, media_path: impl Fn(&Path, &str) -> PathBuf, made: &mut HashMap<String, Option<PathBuf>>) -> Vec<Sound> {
    let num = |p: &Map<String, Value>, k: &str| p.get(k).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
    let mut out = Vec::new();
    for (kind, props, node) in items {
        if kind != "audio" && kind != "video" {
            continue;
        }
        if props.get("data-muted").and_then(|v| v.as_str()) == Some("true") || props.get("data-ended").and_then(|v| v.as_str()) == Some("true") {
            continue;
        }
        let volume = num(props, "data-gain-db").map(|db| 10f64.powf(db / 20.0)).or_else(|| num(props, "data-volume")).unwrap_or(1.0);
        let Some(at) = num(props, "data-source-time") else { continue };
        let file = match (props.get("data-generate").and_then(|v| v.as_str()), props.get("data-prompt").and_then(|v| v.as_str())) {
            (Some(model), Some(prompt)) => {
                let seconds = num(props, "data-seconds").unwrap_or(4.0);
                // looked up once per sound (the store's index was read and parsed every frame: 0.4 ms of a step)
                let key = format!("{model}\u{1}{prompt}\u{1}{seconds}");
                let found = made.entry(key).or_insert_with(|| {
                    let store = rdb::assets::Store::beside(&dir.join("edit.tsx"));
                    match store.made(model, prompt, 1, &serde_json::json!({"seconds": seconds})) {
                        Ok(Some(a)) => Some(store.path(&a)),
                        _ => None,
                    }
                });
                match found {
                    Some(p) => p.clone(),
                    None => continue,
                }
            }
            _ => match props.get("src").and_then(|v| v.as_str()) {
                Some(src) => media_path(dir, src),
                None => continue,
            },
        };
        if volume <= 0.0 {
            continue;
        }
        out.push(Sound {
            key: node.map(|n| format!("n{n}")).unwrap_or_else(|| file.display().to_string()),
            file,
            at,
            volume: volume as f32,
            rate: num(props, "data-playback-rate").unwrap_or(1.0),
            fade_in: num(props, "data-fade-in").unwrap_or(0.0),
            fade_out: num(props, "data-fade-out").unwrap_or(0.0),
        });
    }
    out
}

impl Drop for Mixer {
    fn drop(&mut self) {
        let Ok(mut st) = self.state.lock() else { return };
        let rate = st.rate as u32;
        if let Some((path, buf, ch)) = st.tap.take() {
            let ch = ch.max(1);
            let channels: Vec<Vec<f32>> = (0..ch.min(2)).map(|c| buf.iter().skip(c).step_by(ch).copied().collect()).collect();
            if let Err(e) = write_wav(&raea_media::Audio { rate, channels }, &path) {
                eprintln!("raea audio: tap: {e}");
            }
        }
    }
}

/// `a` as a 32-bit float WAV (stereo when it has two channels): the mastered track live play streams.
pub fn write_wav(a: &raea_media::Audio, out: &Path) -> R<()> {
    let ch = a.channels.len().clamp(1, 2);
    let n = a.len();
    let data = (n * ch * 4) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&(ch as u16).to_le_bytes());
    b.extend_from_slice(&a.rate.to_le_bytes());
    b.extend_from_slice(&(a.rate * ch as u32 * 4).to_le_bytes());
    b.extend_from_slice(&((ch * 4) as u16).to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for i in 0..n {
        for c in 0..ch {
            b.extend_from_slice(&a.channels[c][i].to_le_bytes());
        }
    }
    std::fs::write(out, b).map_err(|e| format!("{}: {e}", out.display()))
}

#[cfg(test)]
mod limiter_tests {
    use super::*;

    #[test]
    fn holds_the_ceiling_smoothly_and_leaves_quiet_sound_alone() {
        let rate = 48_000.0;
        let wave = |amp: f32, n: usize| -> Vec<f32> { (0..n).flat_map(|i| { let v = amp * (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin(); [v, v] }).collect() };
        // loud: a 2.0 sine (6 dB over full scale) never passes the ceiling, and the gain never jumps
        let mut l = Limiter::new();
        let mut loud = wave(2.0, 48_000);
        let input = loud.clone();
        for chunk in loud.chunks_mut(512 * 2) {
            l.run(chunk, 2, rate);
        }
        assert!(loud.iter().all(|x| x.abs() <= CEILING + 1e-6), "over the ceiling");
        let n = (LOOKAHEAD * rate).round() as usize;
        let gains: Vec<f32> = (n + 1..48_000).filter(|i| input[(i - n) * 2].abs() > 0.5).map(|i| loud[i * 2] / input[(i - n) * 2]).collect();
        let worst_step = gains.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0f32, f32::max);
        assert!(worst_step < 0.02, "the gain jumped {worst_step}");
        // quiet: a 0.5 sine comes through as it went in, only later by the look-ahead
        let mut q = Limiter::new();
        let mut quiet = wave(0.5, 4_800);
        let qin = quiet.clone();
        q.run(&mut quiet, 2, rate);
        assert!((n..4_800).all(|i| (quiet[i * 2] - qin[(i - n) * 2]).abs() < 1e-6), "quiet sound changed");
    }
}
