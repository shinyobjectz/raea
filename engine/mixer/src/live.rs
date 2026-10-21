//! The live lane: the same graph in the device callback (Firewheel's CPAL stream). A frame's events land on the clock
//! sample where its timeline sample is due: `origin + t`, where `origin` is set when play starts so that the frame
//! being stepped now is heard `HEAD` from now (time for the events to reach the callback before their sample). Voices
//! then run on the device's clock, as the bounce runs on its own; the frames are only checked against it. A frame that
//! drifts more than 40 ms from where the clock has it (a pause, a seek, a stall) puts the timeline back in place
//! (its voices start again where it is); a smaller drift moves the timeline's start by at most 0.25 ms a frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use audioadapter_buffers::direct::InterleavedSlice;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use firewheel::backend::BackendProcessInfo;
use firewheel::clock::{EventInstant, InstantSamples};
use firewheel::node::{NodeID, StreamStatus};
use firewheel::{ActivateInfo, FirewheelConfig, FirewheelContext};

use crate::nodes::{Stats, event};
use crate::pcm::Pcm;
use crate::score::Score;
use crate::score::ToneOut;
use crate::{RATE, Sound, ToneCue};

/// past this a frame puts the timeline back in place; over DEADBAND its start moves by at most SLEW a frame
const DRIFT: f64 = 0.040;
const DEADBAND: f64 = 0.002;
const SLEW: f64 = 0.00025;

pub struct Live {
    cx: FirewheelContext,
    _stream: cpal::Stream,
    voices: NodeID,
    master: NodeID,
    score: Option<Score>,
    pcm: HashMap<PathBuf, Option<Arc<Pcm>>>,
    opening: HashMap<PathBuf, Receiver<Option<Arc<Pcm>>>>,
    loader: Sender<(Arc<Pcm>, u64)>,
    pub rate: u32,
    /// samples between now and where the frame being stepped is heard, and its part that doesn't depend on the fps
    head: i64,
    head_base: i64,
    origin: Option<i64>,
    /// how far each frame was from where the clock had it (ms), at each sync
    pub drift_ms: Vec<f64>,
    /// (frame, origin) each time the timeline was placed: the tap's alignment
    anchors: Vec<(i64, i64)>,
    slews: u64,
    /// ms spent opening sources and decoding blocks on the frame's thread
    load_ms: Vec<f64>,
    /// how long a frame waits for a sound's block (RAEA_AUDIO_LOAD_WAIT_MS, 80)
    load_wait: std::time::Duration,
    /// each source's last use, and each of its blocks' last use
    used: HashMap<*const Pcm, (std::time::Instant, Vec<(u64, std::time::Instant)>)>,
    /// sync's lists, kept between frames (ZeroHeap): the frame's sounds' indices and sources, the blocks each source's
    /// voices read next, one source's blocks to keep
    items: Vec<(usize, Arc<Pcm>)>,
    keep: HashMap<*const Pcm, Vec<u64>>,
    blocks: Vec<u64>,
    /// the master's gain and ceiling (dB, dBTP), and whether a mastered track passes it by
    master_set: (f64, f64),
    bypass: bool,
    /// the device stream stopped while nothing plays (rest): its callback rendered silence through the graph for as long
    /// as a paused stage stayed open (lightspeed R6); the next sync starts it again
    resting: bool,
    built: crate::tonegraph::Built,
    pub stats: Arc<Stats>,
    tap: Option<(PathBuf, Arc<Mutex<Vec<f32>>>)>,
    /// when the stream was opened, and whether its clock has been waited for (settle_clock)
    opened: std::time::Instant,
    settled: bool,
}

impl Live {
    /// The default output device, asked for 48 kHz (the bounce's rate) in 256-frame blocks.
    pub fn open() -> Result<Live, String> {
        let stats = Arc::new(Stats::default());
        let mute = std::env::var("RAEA_AUDIO_MUTE").as_deref() == Ok("1");
        let tap = std::env::var_os("RAEA_AUDIO_TAP").map(|p| (PathBuf::from(p), Arc::new(Mutex::new(Vec::with_capacity(48_000 * 2 * 120)))));
        let mut cx = FirewheelContext::new(FirewheelConfig::default());
        let (voices, master) = crate::graph(&mut cx, stats.clone(), tap.as_ref().map(|t| t.1.clone()), mute)?;
        let block = std::env::var("RAEA_AUDIO_BLOCK").ok().and_then(|v| v.parse().ok()).unwrap_or(256u32);
        let (stream, rate) = start(&mut cx, block)?;
        if rate != RATE {
            eprintln!("raea mixer: the device runs at {rate} Hz, not {RATE}: live play can't be the bounce sample for sample");
        }
        // two blocks and 5 ms: the frame's events reach the callback before their sample is processed
        let head_s = std::env::var("RAEA_AUDIO_HEAD_MS").ok().and_then(|v| v.parse::<f64>().ok()).map(|ms| ms / 1e3).unwrap_or(2.0 * block as f64 / rate as f64 + 0.005);
        let head = (head_s * rate as f64).round() as i64;
        cx.update().map_err(|e| format!("{e:?}"))?;
        // open returns now; the clock is waited for when the first frame is placed (settle_clock), counted from here, so
        // a device opened beside the edit's open waits for nothing by then
        Ok(Live {
            opened: std::time::Instant::now(),
            settled: false, cx, _stream: stream, voices, master, score: None, pcm: HashMap::new(), opening: HashMap::new(), loader: crate::pcm::loader(), rate, head, head_base: head, origin: None, drift_ms: Vec::new(), anchors: Vec::new(), slews: 0, load_ms: Vec::new(), load_wait: std::time::Duration::from_millis(std::env::var("RAEA_AUDIO_LOAD_WAIT_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(80)), used: HashMap::new(), items: Vec::new(), keep: HashMap::new(), blocks: Vec::new(), master_set: (0.0, 20.0 * (crate::nodes::CEILING as f64).log10()), bypass: false, resting: false, built: Default::default(), stats, tap })
    }

    /// The master: `gain_db` on the sum, then the limiter at `ceiling_db` (dBTP).
    /// The clock running before the first frame places the timeline on it: placed before the stream's first callback,
    /// the timeline was placed again a frame or two later, 67 ms away (its voices restarted); placed in CoreAudio's
    /// startup burst (its first callbacks come back to back), the first frame's events were 75 ms late. Waits until
    /// 100 ms of sound has been processed, at most 500 ms after the open: nothing when the device opened early enough.
    pub fn settle_clock(&mut self) {
        if self.settled {
            return;
        }
        self.settled = true;
        let rate = self.rate as i64;
        while self.cx.audio_clock().samples.0 <= rate / 10 && self.opened.elapsed() < std::time::Duration::from_millis(500) {
            std::thread::sleep(std::time::Duration::from_millis(1));
            let _ = self.cx.update();
        }
    }

    pub fn master(&mut self, gain_db: f64, ceiling_db: f64) {
        self.master_set = (gain_db, ceiling_db);
        self.send_master();
    }

    /// A mastered track playing (a timeline's mastered bounce, Live::master_sound): the master lets it through as it is,
    /// so the play is the render's bounce. Off again, the master's gain and limiter come back.
    pub fn premastered(&mut self, on: bool) {
        if self.bypass != on {
            self.bypass = on;
            self.send_master();
        }
    }

    fn send_master(&mut self) {
        let (g, c) = self.master_set;
        self.cx.queue_event_for(self.master, crate::nodes::event(crate::nodes::Master { pre: 10f64.powf(g / 20.0) as f32, ceiling: 10f64.powf(c / 20.0) as f32, bypass: self.bypass }));
        let _ = self.cx.update();
    }

    /// `file` opening on a worker now, nothing started.
    pub fn preload(&mut self, file: &Path) {
        let _ = self.open_async(file);
    }

    /// A source, opened here the first time a frame has it (timed in the report): opened on a thread (a first voice
    /// opened this late starts frames late, and the play isn't the bounce). preload() opens ahead on a thread.
    fn pcm(&mut self, file: &Path) -> Option<Arc<Pcm>> {
        if let Some(p) = self.pcm.get(file) {
            return p.clone();
        }
        // opened (and its first block decoded) on a worker; the frame waits for it at most the load wait (a cold disk
        // takes seconds), and a later frame takes it when it's in
        let fresh = !self.opening.contains_key(file);
        let _ = self.open_async(file);
        if let Some(p) = self.pcm.get(file) {
            return p.clone();
        }
        let t = std::time::Instant::now();
        let rx = self.opening.get(file)?;
        let got = if fresh { rx.recv_timeout(self.load_wait).map_err(|e| matches!(e, std::sync::mpsc::RecvTimeoutError::Disconnected)) } else { rx.try_recv().map_err(|e| matches!(e, std::sync::mpsc::TryRecvError::Disconnected)) };
        match got {
            Ok(p) => {
                self.load_ms.push(t.elapsed().as_secs_f64() * 1e3);
                self.opening.remove(file);
                self.pcm.insert(file.to_path_buf(), p.clone());
                p
            }
            Err(true) => {
                self.opening.remove(file);
                self.pcm.insert(file.to_path_buf(), None);
                None
            }
            Err(false) => None,
        }
    }

    /// `file` opening on a worker, for preload().
    fn open_async(&mut self, file: &Path) -> Option<Arc<Pcm>> {
        if let Some(p) = self.pcm.get(file) {
            return p.clone();
        }
        let rx = self.opening.entry(file.to_path_buf()).or_insert_with(|| {
            let (tx, rx) = channel();
            let f = file.to_path_buf();
            let _ = std::thread::Builder::new().name("raea mixer open".into()).spawn(move || {
                // its first block too, off the frame's thread (a cold disk takes seconds)
                let p = Pcm::open(&f, false);
                if let Some(p) = &p {
                    p.ensure(0.0);
                }
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

    /// Frame `f`'s sounds and notes, the frame stepped `lag` seconds after its own time.
    pub fn sync(&mut self, f: i64, fps: f64, sounds: &[Sound], tone: &ToneCue, lag: f64) {
        self.wake();
        let rate = self.rate;
        let score = self.score.get_or_insert_with(|| Score::new(fps, rate));
        if (score.fps - fps).abs() > 1e-9 {
            *score = Score::new(fps, rate);
        }
        let t = self.score.as_ref().unwrap().t(f);
        // the head: two blocks and 5 ms for the events to reach the callback, and half a frame for a frame stepped late
        // (a cut's frame, opening its clip, was stepped 30 ms late and its events came after their sample)
        if self.origin.is_none() {
            self.settle_clock();
            self.head = self.head_base + (0.5 / fps * rate as f64).round() as i64;
        }
        let clock = self.cx.audio_clock_corrected().samples.0;
        // where frame f is due on the clock: heard HEAD from now, less the time it was stepped late
        let due = clock + self.head - (lag.max(0.0) * rate as f64).round() as i64;
        let mut evs = Vec::new();
        match self.origin {
            None => {
                self.origin = Some(due - t);
                self.anchors.push((f, due - t));
            }
            Some(o) => {
                let d = (o + t - due) as f64 / rate as f64;
                self.drift_ms.push(d * 1e3);
                if d.abs() > DRIFT {
                    // out of place: the voices stop where they are and start again where the timeline is
                    let score = self.score.as_mut().unwrap();
                    for (_, ev) in score.stop_all(f) {
                        self.cx.queue_event_for(self.voices, event(ev));
                    }
                    self.origin = Some(due - t);
                    self.anchors.push((f, due - t));
                    // the Tone graph's LFOs and ramps follow the timeline's new place
                    for id in self.built.fx.values() {
                        self.cx.queue_event_for(*id, event(crate::fx::FxEv::Origin(due - t)));
                    }
                } else if d.abs() > DEADBAND {
                    let step = (d.signum() * d.abs().min(SLEW) * rate as f64).round() as i64;
                    self.origin = Some(o - step);
                    self.slews += 1;
                    self.anchors.push((f, o - step));
                }
            }
        }
        let origin = self.origin.unwrap();
        if std::env::var_os("RAEA_MIXER_TRACE").is_some() {
            eprintln!("raea mixer: frame {f} lag {:.2} ms clock {clock} due {due} origin {origin} drift {:.3} ms", lag * 1e3, self.drift_ms.last().copied().unwrap_or(0.0));
        }
        // the sounds by index into the kept list, not cloned (ZeroHeap)
        let mut items = std::mem::take(&mut self.items);
        items.clear();
        for (i, s) in sounds.iter().enumerate() {
            if let Some(p) = self.pcm(&s.file) {
                items.push((i, p));
            }
        }
        // the block each sound reads now, in before its voice starts (a start or a jump); the next one comes from the loader
        for (i, p) in &items {
            let snd = &sounds[*i];
            let t = std::time::Instant::now();
            if p.ensure_within(snd.at * p.rate, self.load_wait) {
                self.load_ms.push(t.elapsed().as_secs_f64() * 1e3);
            }
        }
        // the edit's Tone graph, built the first frame it shows (and again if it changes)
        if let Some(g) = &tone.graph
            && crate::tonegraph::hash(g) != self.built.hash
        {
            self.built.tear_down(&mut self.cx, self.voices);
            match crate::tonegraph::build(&mut self.cx, self.voices, self.master, g, origin, &mut |f| crate::open_whole(f)) {
                Ok(b) => self.built = b,
                Err(e) => eprintln!("raea mixer: {e}"),
            }
        }
        let score = self.score.as_mut().unwrap();
        evs.extend(score.frame_with(f, items.len(), |k| (&sounds[items[k].0], &items[k].1), &self.built.bus_port));
        self.items = items;
        if let Some(a) = &tone.pcm {
            evs.extend(score.chunk(f, a));
        }
        if let Some(a) = &tone.speech {
            evs.extend(score.chunk_on(f, a, self.built.bus_port.get("voice").copied().unwrap_or(0)));
        }
        let tone_evs = score.tone(f, &self.built, &tone.evs);
        for (t, ev) in evs {
            self.cx.schedule_event_for(self.voices, event(ev), Some(EventInstant::AtClockSamples(InstantSamples(origin + t))));
        }
        for (t, out) in tone_evs {
            let at = Some(EventInstant::AtClockSamples(InstantSamples(origin + t)));
            match out {
                ToneOut::Voice(ev) => self.cx.schedule_event_for(self.voices, event(ev), at),
                ToneOut::Fx(node, fe) => {
                    if let Some(id) = self.built.fx.get(&node) {
                        self.cx.schedule_event_for(*id, event(fe), at);
                    }
                }
            }
        }
        // the blocks the voices read next, asked of the loader; the rest let go, a source's a second after its last use
        // (its voices play on past the frame that dropped them: their last frames and fade read a block let go at once)
        let keep = &mut self.keep;
        for v in keep.values_mut() {
            v.clear();
        }
        for (pcm, at) in score.positions_iter() {
            keep.entry(Arc::as_ptr(pcm)).or_default().extend(pcm.want(at * pcm.rate, Some(&self.loader)));
        }
        let now = std::time::Instant::now();
        for p in self.pcm.values().flatten() {
            let ptr = Arc::as_ptr(p);
            // each block a second after the last frame that needed it (a cut to another part of the same take still
            // has the outgoing voice reading its block until its stop and fade)
            let used = self.used.entry(ptr).or_insert_with(|| (now, Vec::new()));
            if let Some(k) = keep.get(&ptr).filter(|k| !k.is_empty()) {
                used.0 = now;
                for b in k {
                    match used.1.iter_mut().find(|x| x.0 == *b) {
                        Some(x) => x.1 = now,
                        None => used.1.push((*b, now)),
                    }
                }
            }
            used.1.retain(|x| now.duration_since(x.1).as_secs_f64() < 1.0);
            self.blocks.clear();
            self.blocks.extend(used.1.iter().map(|x| x.0));
            p.retain(&self.blocks);
        }
        // sources no voice reads any more: their entries let go
        if self.keep.len() > 64 {
            self.keep.retain(|_, v| !v.is_empty());
        }
        let _ = self.cx.update();
    }

    /// The device stream stopped (after stop_all and its fade-outs: nothing is heard): no callback runs, so neither the
    /// graph nor the device works while a stage sits paused. The next sync starts it again and places the timeline.
    pub fn rest(&mut self) {
        if self.resting || self.origin.is_some() {
            return;
        }
        if self._stream.pause().is_ok() {
            self.resting = true;
        }
    }

    /// A resting stream started again, and its first callback waited for (at most 100 ms): the corrected clock adds the
    /// time since the last callback, which while resting was the whole rest.
    fn wake(&mut self) {
        if !self.resting {
            return;
        }
        self.resting = false;
        let _ = self._stream.play();
        let (s0, t0) = (self.cx.audio_clock().samples.0, std::time::Instant::now());
        while self.cx.audio_clock().samples.0 == s0 && t0.elapsed() < std::time::Duration::from_millis(100) {
            std::thread::sleep(std::time::Duration::from_millis(1));
            let _ = self.cx.update();
        }
    }

    /// Everything stops (fading out); the next frame places the timeline again.
    pub fn stop_all(&mut self) {
        if let Some(score) = self.score.as_mut() {
            for (_, ev) in score.stop_all(0) {
                self.cx.queue_event_for(self.voices, event(ev));
            }
        }
        self.origin = None;
        let _ = self.cx.update();
    }

    /// Seconds of sound the device has played.
    pub fn played(&self) -> f64 {
        self.cx.audio_clock_corrected().samples.0 as f64 / self.rate as f64
    }

    /// (late events, late samples, missing samples, slews, anchors)
    pub fn report(&self) -> String {
        format!(
            "{{\"late_events\": {}, \"late_samples\": {}, \"missing_samples\": {}, \"slews\": {}, \"anchors\": {}, \"head_samples\": {}, \"rate\": {}, \"loads\": {}, \"load_ms_total\": {:.2}, \"load_ms_max\": {:.2}}}",
            self.stats.late.load(Ordering::Relaxed),
            self.stats.late_samples.load(Ordering::Relaxed),
            self.stats.missing.load(Ordering::Relaxed),
            self.slews,
            self.anchors.len(),
            self.head,
            self.rate,
            self.load_ms.len(),
            self.load_ms.iter().sum::<f64>(),
            self.load_ms.iter().cloned().fold(0.0, f64::max)
        )
    }
}

/// The default output device, at 48 kHz when it takes it, `block` frames a callback when it takes that: Firewheel's
/// processor pulled from cpal's callback, as the bounce pulls it into a buffer (stereo, spread over the device's
/// channels).
fn start(cx: &mut FirewheelContext, block: u32) -> Result<(cpal::Stream, u32), String> {
    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or("no audio output device")?;
    let def = dev.default_output_config().map_err(|e| format!("audio output: {e}"))?;
    let channels = def.channels();
    let takes_48k = dev.supported_output_configs().map(|mut c| c.any(|r| r.channels() == channels && r.sample_format() == cpal::SampleFormat::F32 && r.min_sample_rate().0 <= RATE && r.max_sample_rate().0 >= RATE)).unwrap_or(false);
    let rate = if takes_48k { RATE } else { def.sample_rate().0 };
    let max_block = 8192usize;
    let mut proc = cx
        .activate(ActivateInfo { sample_rate: std::num::NonZeroU32::new(rate).unwrap(), max_block_frames: std::num::NonZeroU32::new(max_block as u32).unwrap(), num_stream_in_channels: 0, num_stream_out_channels: 2, input_to_output_latency_seconds: 0.0 })
        .map_err(|e| format!("the mixer's graph: {e:?}"))?;
    let ch = channels as usize;
    let began = std::time::Instant::now();
    let mut stereo = vec![0f32; max_block * 2];
    let empty: [f32; 0] = [];
    let mut callback = move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
        let ts = info.timestamp();
        let delay = ts.playback.duration_since(&ts.callback);
        let frames = out.len() / ch.max(1);
        let mut done = 0;
        while done < frames {
            let k = (frames - done).min(max_block);
            let pi = BackendProcessInfo { frames: k, process_timestamp: Some(std::time::Instant::now()), duration_since_stream_start: began.elapsed(), input_stream_status: StreamStatus::empty(), output_stream_status: StreamStatus::empty(), dropped_frames: 0, process_to_playback_delay: delay };
            if let (Ok(input), Ok(mut output)) = (InterleavedSlice::new(&empty[..], 0, 0), InterleavedSlice::new_mut(&mut stereo[..k * 2], 2, k)) {
                proc.process(&input, &mut output, pi);
            }
            for i in 0..k {
                let (l, r) = (stereo[2 * i], stereo[2 * i + 1]);
                let o = &mut out[(done + i) * ch..(done + i + 1) * ch];
                match ch {
                    1 => o[0] = 0.5 * (l + r),
                    _ => {
                        o[0] = l;
                        o[1] = r;
                        o[2..].fill(0.0);
                    }
                }
            }
            done += k;
        }
    };
    // the block size when the device's range takes it, else its own
    let fits = dev.supported_output_configs().map(|mut c| c.any(|r| r.channels() == channels && matches!(r.buffer_size(), cpal::SupportedBufferSize::Range { min, max } if *min <= block && block <= *max))).unwrap_or(false);
    let config = cpal::StreamConfig { channels, sample_rate: cpal::SampleRate(rate), buffer_size: if fits { cpal::BufferSize::Fixed(block) } else { cpal::BufferSize::Default } };
    let stream = dev.build_output_stream(&config, move |o: &mut [f32], i: &cpal::OutputCallbackInfo| callback(o, i), |e| eprintln!("raea mixer: {e}"), None).map_err(|e| format!("audio stream: {e}"))?;
    stream.play().map_err(|e| format!("audio stream: {e}"))?;
    Ok((stream, rate))
}

impl Drop for Live {
    fn drop(&mut self) {
        let Some((path, buf)) = self.tap.take() else { return };
        let buf = buf.lock().map(|b| b.clone()).unwrap_or_default();
        let a = raea_media::Audio { rate: self.rate, channels: vec![buf.iter().step_by(2).copied().collect(), buf.iter().skip(1).step_by(2).copied().collect()] };
        if let Err(e) = crate::write_wav(&a, &path) {
            eprintln!("raea mixer: tap: {e}");
        }
        // where the timeline sat on the tap: the bounce's sample t is the tap's origin + t (each anchor from its frame on)
        let anchors: Vec<String> = self.anchors.iter().map(|(f, o)| format!("[{f}, {o}]")).collect();
        let side = path.with_extension("json");
        let _ = std::fs::write(&side, format!("{{\"anchors\": [{}], \"report\": {}}}", anchors.join(", "), self.report()));
    }
}
