//! The render's lane: the same graph as live play, its processor pulled by hand into a buffer, frame by frame, as fast
//! as it computes (no device, no wall clock: the clock is the samples processed). Missing blocks are decoded where
//! they're wanted, so the bounce is the same every time.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use audioadapter_buffers::direct::InterleavedSlice;
use firewheel::backend::BackendProcessInfo;
use firewheel::clock::{EventInstant, InstantSamples};
use firewheel::node::StreamStatus;
use firewheel::{ActivateInfo, FirewheelConfig, FirewheelContext};

use crate::nodes::{Stats, event};
use crate::pcm::Pcm;
use crate::score::Score;
use crate::score::ToneOut;
use crate::{RATE, Sound, ToneCue};

/// One frame's sounds and notes.
#[derive(Clone, Debug, Default)]
pub struct Cue {
    pub sounds: Vec<Sound>,
    pub tone: ToneCue,
}

const BLOCK: usize = 4096;

/// The graph with its processor in hand: frames fed in (their events on `origin + t`), samples pulled out in blocks of
/// any size. `bounce` pulls each frame's samples right after feeding it; the tests pull it as a device would.
pub struct Engine {
    // the processor drops before the context: the other way, the context's drop waited 3 s (its timeout) for a processor
    // nobody was running to let go (every bounce cost 3 s more)
    proc: firewheel::processor::FirewheelProcessor,
    cx: FirewheelContext,
    voices: firewheel::node::NodeID,
    master: firewheel::node::NodeID,
    built: crate::tonegraph::Built,
    pub score: Score,
    pcms: HashMap<PathBuf, Option<Arc<Pcm>>>,
    pub clock: usize,
}

impl Engine {
    pub fn new(fps: f64, master: Option<(f64, f64)>) -> Result<Engine, String> {
        let stats = Arc::new(Stats::default());
        let mut cx = FirewheelContext::new(FirewheelConfig::default());
        let (voices, master_id) = crate::graph(&mut cx, stats, None, false)?;
        let proc = cx
            .activate(ActivateInfo { sample_rate: NonZeroU32::new(RATE).unwrap(), max_block_frames: NonZeroU32::new(BLOCK as u32).unwrap(), num_stream_in_channels: 0, num_stream_out_channels: 2, input_to_output_latency_seconds: 0.0 })
            .map_err(|e| format!("the bounce's graph: {e:?}"))?;
        if let Some((g, c)) = master {
            cx.queue_event_for(master_id, crate::master_event(g, c));
        }
        cx.update().map_err(|e| format!("{e:?}"))?;
        Ok(Engine { cx, proc, voices, master: master_id, built: Default::default(), score: Score::new(fps, RATE), pcms: HashMap::new(), clock: 0 })
    }

    /// Frame `f`'s sounds and notes, their events on `origin + t`.
    pub fn feed(&mut self, f: i64, cue: &Cue, origin: i64) -> Result<(), String> {
        let pcms = &mut self.pcms;
        let items: Vec<(Sound, Arc<Pcm>)> = cue.sounds.iter().filter_map(|s| pcms.entry(s.file.to_path_buf()).or_insert_with(|| Pcm::open(&s.file, true)).clone().map(|p| (s.clone(), p))).collect();
        // the edit's Tone graph, built the first frame it shows (and again if it changes)
        if let Some(g) = &cue.tone.graph
            && crate::tonegraph::hash(g) != self.built.hash
        {
            self.built.tear_down(&mut self.cx, self.voices);
            self.built = crate::tonegraph::build(&mut self.cx, self.voices, self.master, g, origin, &mut |f| crate::open_whole(f))?;
        }
        let mut evs = self.score.frame(f, &items, &self.built.bus_port);
        if let Some(a) = &cue.tone.pcm {
            evs.extend(self.score.chunk(f, a));
        }
        if let Some(a) = &cue.tone.speech {
            evs.extend(self.score.chunk_on(f, a, self.built.bus_port.get("voice").copied().unwrap_or(0)));
        }
        for (t, ev) in evs {
            self.cx.schedule_event_for(self.voices, event(ev), Some(EventInstant::AtClockSamples(InstantSamples(origin + t))));
        }
        for (t, out) in self.score.tone(f, &self.built, &cue.tone.evs) {
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
        // the blocks the voices read next, and only those
        let mut keep: HashMap<*const Pcm, Vec<u64>> = HashMap::new();
        for (pcm, at) in self.score.positions() {
            keep.entry(Arc::as_ptr(&pcm)).or_default().extend(pcm.want(at * pcm.rate, None));
        }
        for p in self.pcms.values().flatten() {
            p.retain(keep.get(&Arc::as_ptr(p)).map(|v| v.as_slice()).unwrap_or(&[]));
        }
        self.cx.update().map_err(|e| format!("{e:?}"))
    }

    /// The next `out.len() / 2` stereo frames.
    pub fn pull(&mut self, out: &mut [f32]) -> Result<(), String> {
        let empty: [f32; 0] = [];
        let mut done = 0;
        let n = out.len() / 2;
        while done < n {
            let k = (n - done).min(BLOCK);
            let info = BackendProcessInfo {
                frames: k,
                process_timestamp: None,
                duration_since_stream_start: Duration::from_secs_f64(self.clock as f64 / RATE as f64),
                input_stream_status: StreamStatus::empty(),
                output_stream_status: StreamStatus::empty(),
                dropped_frames: 0,
                process_to_playback_delay: None,
            };
            let input = InterleavedSlice::new(&empty[..], 0, 0).map_err(|e| format!("{e:?}"))?;
            let mut output = InterleavedSlice::new_mut(&mut out[done * 2..(done + k) * 2], 2, k).map_err(|e| format!("{e:?}"))?;
            self.proc.process(&input, &mut output, info);
            done += k;
            self.clock += k;
        }
        Ok(())
    }
}

/// The frames' sound, bounced: each frame's events on its sample, the graph processed to the next frame's sample, to
/// the end of the last frame. `master`: the gain (dB) on the sum and the limiter's ceiling (dBTP); without one, the
/// limiter at -1 dBFS (as live play's default).
pub fn bounce(frames: &[Cue], fps: f64, master: Option<(f64, f64)>) -> Result<raea_media::Audio, String> {
    let mut e = Engine::new(fps, master)?;
    let n = e.score.t(frames.len() as i64).max(0) as usize;
    let mut out = vec![0f32; n * 2];
    let mut done = 0usize;
    let (mut feed_s, mut pull_s) = (0.0, 0.0);
    for (f, cue) in frames.iter().enumerate() {
        let t = std::time::Instant::now();
        e.feed(f as i64, cue, 0)?;
        feed_s += t.elapsed().as_secs_f64();
        let t = std::time::Instant::now();
        let end = (e.score.t(f as i64 + 1).max(0) as usize).min(n);
        e.pull(&mut out[done * 2..end * 2])?;
        pull_s += t.elapsed().as_secs_f64();
        done = end;
    }
    if std::env::var_os("RAEA_MASTER_TRACE").is_some() {
        eprintln!("bounce: {} frames, feed {:.0} ms, process {:.0} ms", frames.len(), feed_s * 1e3, pull_s * 1e3);
    }
    let l: Vec<f32> = out.iter().step_by(2).copied().collect();
    let r: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();
    Ok(raea_media::Audio { rate: RATE, channels: vec![l, r] })
}

/// The frames' sound before the master: the master node passes it as it is (no gain, no limiter, no delay). The
/// master is then `master_raw` over it, the same limiter the live master node runs, sample for sample: one bounce, and
/// mastering (or mastering again for AAC) is a pass over a buffer.
pub fn bounce_raw(frames: &[Cue], fps: f64) -> Result<raea_media::Audio, String> {
    let mut s = Streamer::start(fps);
    for c in frames {
        s.push(c.clone());
    }
    s.finish()
}

/// `raw` through the master: `master` (gain dB, ceiling dBTP), else the live default (unity, -1 dBFS).
pub fn master_raw(raw: &raea_media::Audio, master: Option<(f64, f64)>) -> raea_media::Audio {
    let mut lim = crate::nodes::Limiter::new();
    if let Some((g, c)) = master {
        lim.pre = 10f64.powf(g / 20.0) as f32;
        lim.ceiling = 10f64.powf(c / 20.0) as f32;
    }
    let mut l = raw.channels.first().cloned().unwrap_or_default();
    let mut r = raw.channels.get(1).cloned().unwrap_or_else(|| l.clone());
    for (a, b) in l.chunks_mut(4096).zip(r.chunks_mut(4096)) {
        lim.run(a, b, raw.rate as f64);
    }
    raea_media::Audio { rate: raw.rate, channels: vec![l, r] }
}

/// The master over a raw bounce, the one function every render uses (react's fed and page renders, rdb's SQL renders, a
/// play's master-gain): with a <Mix> (`mix`: target LUFS, ceiling dBTP), the gain from the raw sound to the target, then
/// the limiter at the ceiling, the ceiling lowered by any overshoot `encode` (an AAC round trip) puts back (twice at most);
/// without one, the live default. Returns the master it used and the mastered sound.
pub fn master_mix(raw: &raea_media::Audio, mix: Option<(f64, f64)>, encode: Option<&dyn Fn(&raea_media::Audio) -> Result<raea_media::Audio, String>>) -> (Option<(f64, f64)>, raea_media::Audio) {
    let Some((lufs, ceiling)) = mix.filter(|_| raea_media::audio::lufs(raw).is_finite()) else { return (None, master_raw(raw, None)) };
    let gain = raea_media::audio::master_gained(raw, lufs, ceiling).1;
    let mut master = Some((gain, ceiling));
    let mut a = master_raw(raw, master);
    if let Some(enc) = encode {
        let mut c = ceiling;
        for _ in 0..2 {
            let Ok(rt) = enc(&a) else { break };
            let over = raea_media::audio::true_peak_db(&rt) - ceiling;
            if over <= 0.0 {
                break;
            }
            c -= over + 0.05;
            master = Some((gain, c));
            a = master_raw(raw, master);
        }
    }
    (master, a)
}

/// A raw bounce made as the frames come (a render streams each frame's cue while it draws the frames), on its own
/// thread: the sound is ready when the last frame is.
pub struct Streamer {
    tx: std::sync::mpsc::Sender<Cue>,
    done: std::thread::JoinHandle<Result<raea_media::Audio, String>>,
}

impl Streamer {
    pub fn start(fps: f64) -> Streamer {
        let (tx, rx) = std::sync::mpsc::channel::<Cue>();
        let done = std::thread::Builder::new()
            .name("raea mixer bounce".into())
            .spawn(move || -> Result<raea_media::Audio, String> {
                let mut e = Engine::new(fps, None)?;
                e.cx.queue_event_for(e.master, event(crate::nodes::Master { pre: 1.0, ceiling: 1.0, bypass: true }));
                e.cx.update().map_err(|x| format!("{x:?}"))?;
                let mut out: Vec<f32> = Vec::new();
                let mut f = 0i64;
                for cue in rx {
                    e.feed(f, &cue, 0)?;
                    let end = e.score.t(f + 1).max(0) as usize;
                    let start = out.len() / 2;
                    out.resize(end * 2, 0.0);
                    e.pull(&mut out[start * 2..end * 2])?;
                    f += 1;
                }
                let l: Vec<f32> = out.iter().step_by(2).copied().collect();
                let r: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();
                Ok(raea_media::Audio { rate: RATE, channels: vec![l, r] })
            })
            .expect("bounce thread");
        Streamer { tx, done }
    }

    /// The next frame's cue.
    pub fn push(&self, cue: Cue) {
        let _ = self.tx.send(cue);
    }

    /// The raw bounce of every frame pushed.
    pub fn finish(self) -> Result<raea_media::Audio, String> {
        drop(self.tx);
        self.done.join().map_err(|_| "the bounce thread panicked".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(dir: &std::path::Path, name: &str, hz: f32, secs: f32) -> PathBuf {
        let n = (secs * RATE as f32) as usize;
        let ch: Vec<f32> = (0..n).map(|i| 0.5 * (i as f32 * hz * std::f32::consts::TAU / RATE as f32).sin()).collect();
        let p = dir.join(name);
        crate::write_wav(&raea_media::Audio { rate: RATE, channels: vec![ch.clone(), ch] }, &p).unwrap();
        p
    }

    /// The tones fixture's shape (starts, a cut, an overlap, a gain ramp, a stop), and the music fixture's notes.
    fn frames(dir: &std::path::Path) -> Vec<Cue> {
        let (a, b) = (tone(dir, "a.wav", 440.0, 4.0), tone(dir, "b.wav", 660.0, 4.0));
        let fps = 30.0;
        (0..300)
            .map(|f| {
                let mut c = Cue::default();
                let s = |key: &str, file: &PathBuf, from: i64, start: f64, vol: f32| Sound { key: key.into(), file: file.as_path().into(), at: start + (f - from) as f64 / fps, volume: vol, rate: 1.0, fade_in: 0.0, fade_out: 0.0, bus: None };
                if f < 60 { c.sounds.push(s("n1", &a, 0, 0.0, 1.0)); }
                if (60..120).contains(&f) { c.sounds.push(s("n2", &a, 60, 17.0 / fps, 1.0)); }
                if (90..165).contains(&f) { c.sounds.push(s("n3", &b, 90, 0.0, 0.6)); }
                if (170..240).contains(&f) { c.sounds.push(s("n4", &a, 170, 0.0, ((f - 170) as f32 / 35.0).min(1.0))); }
                if (245..285).contains(&f) { c.sounds.push(s("n5", &b, 245, 40.0 / fps, 1.0)); }
                c
            })
            .collect()
    }

    /// A Tone graph (@raea/motion/tone's JSON): a PolySynth through a filter (ramped) and a reverb into a music bus
    /// ducked under a voice bus fed by a tone file, chords and a sweep as events, plus the tones on frames.
    fn tone_frames(dir: &std::path::Path) -> Vec<Cue> {
        let mut fr = frames(dir);
        let graph = serde_json::json!({
            "nodes": [
                {"id": 0, "kind": "Destination", "options": {"volume": -2}},
                {"id": 1, "kind": "Synth", "options": {"oscillator": {"type": "fatsawtooth", "count": 3, "spread": 20}, "envelope": {"attack": 0.05, "decay": 0.2, "sustain": 0.6, "release": 0.5}, "poly": true, "maxPolyphony": 4, "volume": -12}},
                {"id": 2, "kind": "Filter", "options": {"frequency": 2000, "type": "lowpass", "rolloff": -24, "Q": 1}},
                {"id": 3, "kind": "Reverb", "options": {"decay": 2.0, "wet": 0.3}},
                {"id": 4, "kind": "Channel", "name": "music", "options": {"volume": 0, "pan": 0, "duck": {"amount": -12, "threshold": -45, "attack": 0.04, "release": 0.4}}},
                {"id": 5, "kind": "Channel", "name": "voice", "options": {"volume": 0, "pan": 0}},
                {"id": 6, "kind": "MembraneSynth", "options": {"pitchDecay": 0.05, "octaves": 6, "variation": {"pitch": 0.5, "volume": 2}}},
                {"id": 7, "kind": "AutoFilter", "options": {"frequency": 2, "baseFrequency": 300, "octaves": 3, "started": 0.5}}
            ],
            "edges": [[1, 2, 0], [2, 3, 0], [3, 4, 0], [4, 0, 0], [5, 0, 0], [5, 4, 1], [6, 7, 0], [7, 0, 0]]
        }).to_string();
        let chord = |k: &str, t: f64, f: &[f64]| serde_json::json!({"k": k, "n": 1, "t": t, "e": "ar", "f": f, "d": 0.9, "v": 0.9});
        for (i, c) in fr.iter_mut().enumerate() {
            c.tone.graph = Some(graph.clone());
            // the tones play on the voice bus from frame 90 (a key for the duck)
            for s in c.sounds.iter_mut() {
                if i >= 90 {
                    s.bus = Some("voice".into());
                }
            }
            let t = i as f64 / 30.0;
            for (k, at, f) in [("c1", 0.1, vec![220.0, 277.18, 329.63]), ("c2", 1.07, vec![174.61, 220.0, 261.63]), ("c3", 2.51, vec![196.0, 246.94, 293.66]), ("c4", 4.03, vec![220.0, 261.63, 329.63, 392.0, 440.0])] {
                if at >= t - 2.0 / 30.0 && at < t + 1.0 / 30.0 {
                    c.tone.evs.push(chord(k, at, &f));
                }
            }
            if (0.5..0.54).contains(&t) {
                c.tone.evs.push(serde_json::json!({"k": "sweep", "n": 2, "t": 0.5, "e": "param", "p": "frequency", "val": 600.0, "ramp": 2.0}));
            }
            if i % 20 == 0 {
                c.tone.evs.push(serde_json::json!({"k": format!("kick{i}"), "n": 6, "t": t + 0.003, "e": "ar", "f": [55.0], "d": 0.2, "v": 1.0}));
            }
        }
        fr
    }

    #[test]
    fn a_tone_graph_played_like_a_device_is_its_bounce() {
        let dir = std::env::temp_dir().join(format!("raea-mixer-test-tone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fr = tone_frames(&dir);
        let want = bounce(&fr, 30.0, Some((0.0, -1.0))).unwrap();
        assert!(want.channels[0].iter().any(|v| v.abs() > 0.05), "silent");
        assert_eq!(want.channels, bounce(&fr, 30.0, Some((0.0, -1.0))).unwrap().channels, "not the same twice");
        let mut e = Engine::new(30.0, Some((0.0, -1.0))).unwrap();
        let origin: i64 = 7_777;
        let total = origin as usize + want.len();
        let mut out = vec![0f32; total * 2];
        let (mut x, mut next_f, mut done) = (0x51ed270b27f1f3a5u64, 0i64, 0usize);
        while done < total {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let k = (1 + (x % 900) as usize).min(total - done);
            let head = (x >> 20) % 1440;
            while next_f < fr.len() as i64 && origin + e.score.t(next_f) <= (done + k) as i64 + head as i64 {
                e.feed(next_f, &fr[next_f as usize], origin).unwrap();
                next_f += 1;
            }
            e.pull(&mut out[done * 2..(done + k) * 2]).unwrap();
            done += k;
        }
        let o = origin as usize;
        let worst = (0..want.len()).map(|i| (want.channels[0][i] - out[(o + i) * 2]).abs().max((want.channels[1][i] - out[(o + i) * 2 + 1]).abs())).fold(0f32, f32::max);
        assert_eq!(worst, 0.0, "the Tone play differs from its bounce by up to {worst}");
    }

    #[test]
    fn a_raw_bounce_mastered_is_the_mastered_bounce() {
        let dir = std::env::temp_dir().join(format!("raea-mixer-test-raw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fr = tone_frames(&dir);
        for m in [None, Some((6.0, -1.5))] {
            let want = bounce(&fr, 30.0, m).unwrap();
            let got = master_raw(&bounce_raw(&fr, 30.0).unwrap(), m);
            assert_eq!(want.channels, got.channels, "master {m:?}");
        }
    }

    #[test]
    fn the_bounce_is_the_same_every_time() {
        let dir = std::env::temp_dir().join(format!("raea-mixer-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fr = frames(&dir);
        let (x, y) = (bounce(&fr, 30.0, Some((3.0, -1.5))).unwrap(), bounce(&fr, 30.0, Some((3.0, -1.5))).unwrap());
        assert_eq!(x.channels, y.channels);
        assert!(x.channels[0].iter().any(|v| v.abs() > 0.1), "silent");
    }

    /// Live play's shape: the timeline placed at an arbitrary clock sample, each frame's events sent ahead of their sample
    /// by a varying margin, the samples pulled in blocks of varying size, none at a frame's edge. The output from the
    /// origin on is the bounce, sample for sample.
    #[test]
    fn played_like_a_device_it_is_the_bounce() {
        let dir = std::env::temp_dir().join(format!("raea-mixer-test-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fr = frames(&dir);
        let want = bounce(&fr, 30.0, Some((3.0, -1.5))).unwrap();
        let mut e = Engine::new(30.0, Some((3.0, -1.5))).unwrap();
        let origin: i64 = 12_345;
        let total = origin as usize + want.len();
        let mut out = vec![0f32; total * 2];
        let (mut x, mut next_f, mut done) = (0x9e3779b97f4a7c15u64, 0i64, 0usize);
        while done < total {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let k = (1 + (x % 700) as usize).min(total - done);
            // every frame due before this block ends (plus a head of up to 30 ms) is sent first
            let head = (x >> 20) % 1440;
            while next_f < fr.len() as i64 && origin + e.score.t(next_f) <= (done + k) as i64 + head as i64 {
                e.feed(next_f, &fr[next_f as usize], origin).unwrap();
                next_f += 1;
            }
            e.pull(&mut out[done * 2..(done + k) * 2]).unwrap();
            done += k;
        }
        let o = origin as usize;
        let worst = (0..want.len()).map(|i| (want.channels[0][i] - out[(o + i) * 2]).abs().max((want.channels[1][i] - out[(o + i) * 2 + 1]).abs())).fold(0f32, f32::max);
        assert_eq!(worst, 0.0, "the play differs from the bounce by up to {worst}");
    }
}
