//! bevy_seedling on raea's one mixer core (sdk/react/docs/audio-stack.md).
//!
//! Seedling's audio graph (its sample players, pools, buses and effects, with raea's Tone effects and the core's limiter
//! registered as seedling nodes) runs in a Bevy App beside the game, with no device of its own: `CorePlatform` activates
//! seedling's Firewheel context and keeps the processor, and each frame, after seedling has flushed the frame's changes
//! (its `Last` systems), the processor is run for exactly that frame's samples on the timeline (round(f * rate / fps)
//! to round((f + 1) * rate / fps)). The chunk is a voice of the core (ToneCue::pcm), on the frame's sample, through the
//! core's master to the device (live, cpal 0.15) or the bounce (a render). Seedling's events reach its processor at the
//! start of their frame's chunk in both, its clock is the timeline's samples, its Time steps one frame per update and
//! it diffs every update, and samples are decoded before their frame: a render and a play of a game hear the same
//! seedling samples.
//!
//! What a game asks seedling for this frame is a list of `Play`s (keyed: a key not seen before starts its player, one
//! no longer asked for stops it). TSX games ask with `<Seedling>` (@raea/motion/seedling); Rust can drive `Host::app`.

use std::collections::HashMap;
use std::num::{NonZeroU32, NonZero};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use audioadapter_buffers::direct::InterleavedSlice;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_seedling::context::{AudioContext, SampleRate};
use bevy_seedling::node::DiffRate;
use bevy_seedling::prelude::*;
use firewheel::backend::BackendProcessInfo;
use firewheel::channel_config::{ChannelConfig, ChannelCount};
use firewheel::node::{AudioNode, AudioNodeInfo, AudioNodeProcessor, ConstructProcessorContext, NodeError, StreamStatus};
use firewheel::ActivateInfo;
use firewheel::processor::FirewheelProcessor;
use serde_json::Value;

/// The core's rate.
pub const RATE: u32 = 48_000;
const BLOCK: u32 = 1024;

/// One of raea's Tone effects (engine/mixer/src/fx.rs: Filter, Reverb, Freeverb, FeedbackDelay, Compressor, EQ3, Convolver,
/// PitchShift, ...) as a seedling node: `kind` is Tone's class name, `options` its options (Tone's names and defaults).
/// Its LFOs and ramps run on seedling's clock, which here is the timeline's.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ToneFx {
    pub kind: String,
    pub options: Value,
}

impl ToneFx {
    pub fn new(kind: &str, options: Value) -> ToneFx {
        ToneFx { kind: kind.into(), options }
    }
}

/// A node's configuration (seedling makes it a required component; nothing in it).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct NoConfig;

impl AudioNode for ToneFx {
    type Configuration = NoConfig;

    fn info(&self, _: &NoConfig) -> Result<AudioNodeInfo, NodeError> {
        // stereo in and out, and the fx node's sidechain pair (a duck's key), which seedling's chains leave silent
        Ok(AudioNodeInfo::new().debug_name("raea tone fx").channel_config(ChannelConfig { num_inputs: ChannelCount::new(4).unwrap(), num_outputs: ChannelCount::STEREO }))
    }

    fn construct_processor(&self, _: &NoConfig, cx: ConstructProcessorContext) -> Result<impl AudioNodeProcessor, NodeError> {
        let ir = (self.kind == "Convolver").then(|| self.options.get("url").and_then(|u| u.as_str()).and_then(|f| raea_media::decode_audio_window(Path::new(f), 0.0, 30.0).ok()).map(Arc::new)).flatten();
        let node = raea_mixer::fx::FxNode { kind: self.kind.clone(), params: self.options.clone(), origin: 0, muted: false, ir };
        Ok(node.processor(cx.stream_info.sample_rate.get() as f64))
    }
}

/// The core's limiter (engine/mixer/src/nodes.rs MasterNode: a 48-tap true-peak look-ahead limiter at -1 dBFS) as a seedling
/// node, for a bus that must be held under the ceiling before the core's master.
#[derive(Component, Clone, Debug, Default)]
pub struct CoreLimiter;

impl AudioNode for CoreLimiter {
    type Configuration = NoConfig;

    fn info(&self, _: &NoConfig) -> Result<AudioNodeInfo, NodeError> {
        raea_mixer::nodes::MasterNode { tap: None, mute: false }.info(&())
    }

    fn construct_processor(&self, _: &NoConfig, cx: ConstructProcessorContext) -> Result<impl AudioNodeProcessor, NodeError> {
        Ok(raea_mixer::nodes::MasterNode { tap: None, mute: false }.processor(cx.stream_info.sample_rate.get() as f64))
    }
}

/// Seedling's processor, run by `chunk`, and the samples it has made.
#[derive(Resource)]
pub struct CoreStream {
    proc: Mutex<Option<FirewheelProcessor>>,
    pub clock: u64,
}

/// Seedling on the core: its core plugin, the platform (no device; `chunk` runs the processor) and raea's nodes.
pub struct SeedlingOnCore {
    pub fps: f64,
}

impl Plugin for SeedlingOnCore {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy_seedling::SeedlingCorePlugin)
            // every change diffed on the update that made it (seedling's default waits 10 ms of audio time)
            .insert_resource(DiffRate(Duration::ZERO))
            // pools made big at the start: a pool growing mid-game adds samplers and recompiles Firewheel's graph, a
            // 40-100 ms frame (seedling's default starts at 4)
            .insert_resource(DefaultPoolSize(32..=64))
            // one frame per update: seedling's queue lifetimes count Bevy time
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / self.fps.max(1.0))))
            .add_systems(PostStartup, start_stream.in_set(SeedlingStartupSystems::StreamInitialization))
            .register_simple_node::<ToneFx>()
            .register_simple_node::<CoreLimiter>();
    }
}

fn start_stream(mut context: ResMut<AudioContext>, commands: Commands) {
    let proc = context.with(|cx| {
        cx.activate(ActivateInfo { sample_rate: NonZeroU32::new(RATE).unwrap(), max_block_frames: NonZero::new(BLOCK).unwrap(), num_stream_in_channels: 0, num_stream_out_channels: 2, input_to_output_latency_seconds: 0.0 }).map_err(|e| format!("{e:?}"))
    });
    let proc = match proc {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("raea seedling: the graph didn't start: {e}");
            None
        }
    };
    let mut commands = commands;
    commands.insert_resource(CoreStream { proc: Mutex::new(proc), clock: 0 });
    bevy_seedling::platform::initialize_stream(SampleRate::new(NonZeroU32::new(RATE).unwrap()), commands);
}

/// The next `frames` samples of seedling's graph, interleaved stereo.
pub fn chunk(world: &mut World, frames: usize) -> Vec<f32> {
    let mut out = vec![0f32; frames * 2];
    let Some(mut s) = world.get_resource_mut::<CoreStream>() else { return out };
    let clock = s.clock;
    let mut guard = s.proc.lock().unwrap();
    let Some(proc) = guard.as_mut() else { return out };
    let empty: [f32; 0] = [];
    let mut done = 0;
    while done < frames {
        let k = (frames - done).min(BLOCK as usize);
        let info = BackendProcessInfo {
            frames: k,
            process_timestamp: None,
            duration_since_stream_start: Duration::from_secs_f64((clock + done as u64) as f64 / RATE as f64),
            input_stream_status: StreamStatus::empty(),
            output_stream_status: StreamStatus::empty(),
            dropped_frames: 0,
            process_to_playback_delay: None,
        };
        let input = InterleavedSlice::new(&empty[..], 0, 0).unwrap();
        let mut output = InterleavedSlice::new_mut(&mut out[done * 2..(done + k) * 2], 2, k).unwrap();
        proc.process(&input, &mut output, info);
        done += k;
    }
    drop(guard);
    s.clock += frames as u64;
    out
}

/// Sources decoded whole at the core's rate, shared by every host (`prefetch` fills it ahead on a thread; a source not in
/// yet is decoded where it's wanted: the same samples either way).
static DECODED: std::sync::LazyLock<Mutex<HashMap<PathBuf, Option<Arc<raea_media::Audio>>>>> = std::sync::LazyLock::new(Default::default);

fn decode(file: &Path) -> Option<Arc<raea_media::Audio>> {
    let secs = raea_media::probe_seconds(file).unwrap_or(600.0).min(600.0);
    raea_media::decode_audio_window(file, 0.0, secs + 0.1).ok().filter(|a| a.len() > 0).map(|a| Arc::new(if a.rate != RATE { raea_media::soft_resample(&a, RATE) } else { a }))
}

fn decoded(file: &Path) -> Option<Arc<raea_media::Audio>> {
    if let Some(a) = DECODED.lock().unwrap().get(file) {
        return a.clone();
    }
    let a = decode(file);
    DECODED.lock().unwrap().insert(file.to_path_buf(), a.clone());
    a
}

/// `files` decoded on a thread now, so a frame that first plays one doesn't decode it (a 40 s bed took 44 ms).
pub fn prefetch(files: Vec<PathBuf>) {
    let _ = std::thread::Builder::new().name("raea seedling prefetch".into()).spawn(move || {
        for f in files {
            if DECODED.lock().unwrap().contains_key(&f) {
                continue;
            }
            let a = decode(&f);
            DECODED.lock().unwrap().entry(f).or_insert(a);
        }
    });
}

/// A sample a game asks seedling to play this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Play {
    /// the same key frame after frame is the same player
    pub key: String,
    pub file: PathBuf,
    /// linear gain, from the player's first frame
    pub volume: f32,
    pub speed: f64,
    pub looping: bool,
    /// a serial chain of raea's Tone effects on this player (seedling's SampleEffects)
    pub fx: Vec<ToneFx>,
    /// where it sounds from, in the scene (metres): heard from the listener (`Host::listen`), panned, fading and
    /// dulling with distance (seedling's SpatialBasicNode) and pitched by its speed toward or away (doppler); None is a
    /// sound with no place (music, a voice-over)
    pub at: Option<[f32; 3]>,
    /// a lowpass on it (Hz) for what stands between: a wall, a window, water (none by default)
    pub muffle: Option<f32>,
}

/// Sound's speed in air (m/s), for doppler.
const SOUND: f64 = 343.0;

/// A seedling App beside a game, a frame at a time.
pub struct Host {
    pub app: App,
    pub fps: f64,
    next: i64,
    players: HashMap<String, Entity>,
    samples: HashMap<PathBuf, Option<Handle<AudioSample>>>,
    /// the ears placed sounds are heard from (the camera)
    listener: Entity,
    /// each placed player's distance from the listener last frame, for doppler
    last: HashMap<String, f32>,
}

impl Host {
    pub fn new(fps: f64) -> Host {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default(), bevy::transform::TransformPlugin, SeedlingOnCore { fps }));
        app.finish();
        app.cleanup();
        app.update();
        let listener = app.world_mut().spawn((SpatialListener3D, Transform::default())).id();
        Host { app, fps, next: 0, players: HashMap::new(), samples: HashMap::new(), listener, last: HashMap::new() }
    }

    /// The listener this frame: at `eye`, facing `look` (the scene's camera; -z ahead, +x its right ear).
    pub fn listen(&mut self, eye: [f32; 3], look: [f32; 3], up: [f32; 3]) {
        let t = Transform::from_translation(Vec3::from(eye)).looking_at(Vec3::from(look), Vec3::from(up));
        if let Some(mut tr) = self.app.world_mut().get_mut::<Transform>(self.listener) {
            *tr = t;
        }
    }

    /// How far a place is from the listener now.
    fn distance(&self, at: [f32; 3]) -> f32 {
        let l = self.app.world().get::<Transform>(self.listener).map(|t| t.translation).unwrap_or_default();
        l.distance(Vec3::from(at))
    }

    fn t(&self, f: i64) -> i64 {
        (f as f64 * RATE as f64 / self.fps).round() as i64
    }

    /// A sample decoded whole at the core's rate, before its frame (an asset loaded on Bevy's IO pool lands on whatever
    /// frame it lands on, not the same one in a render and a play).
    fn sample(&mut self, file: &Path) -> Option<Handle<AudioSample>> {
        if let Some(h) = self.samples.get(file) {
            return h.clone();
        }
        let t0 = std::time::Instant::now();
        let h = decoded(file).map(|a| {
            let mut ch = a.channels.clone();
            if ch.len() == 1 {
                ch.push(ch[0].clone());
            }
            let s = AudioSample::new(ch, NonZeroU32::new(a.rate.max(1)).unwrap());
            self.app.world_mut().resource_mut::<Assets<AudioSample>>().add(s)
        });
        if h.is_none() {
            eprintln!("raea seedling: {} has no sound", file.display());
        }
        if std::env::var_os("RAEA_SEEDLING_TRACE").is_some() {
            eprintln!("raea seedling: {} decoded in {:.1} ms", file.display(), t0.elapsed().as_secs_f64() * 1e3);
        }
        self.samples.insert(file.to_path_buf(), h.clone());
        h
    }

    /// The plays' samples decoded now (ahead of their frames).
    pub fn preload(&mut self, plays: &[Play]) {
        for p in plays {
            let _ = self.sample(&p.file);
        }
    }

    /// Frame `f`: the players `plays` asks for started, the ones it no longer asks for stopped, volumes and speeds set,
    /// seedling updated, and the frame's samples made. A frame that isn't the next starts seedling again from silence
    /// (a seek); the chunk is the frame's samples on the timeline.
    pub fn frame(&mut self, f: i64, plays: &[Play]) -> raea_media::Audio {
        if std::env::var_os("RAEA_SEEDLING_TRACE").is_some() && f != self.next {
            eprintln!("raea seedling: frame {f}, expected {}", self.next);
        }
        if f != self.next {
            let fps = self.fps;
            let ears = self.app.world().get::<Transform>(self.listener).copied();
            *self = Host::new(fps);
            if let (Some(t), Some(mut tr)) = (ears, self.app.world_mut().get_mut::<Transform>(self.listener)) {
                *tr = t;
            }
            self.next = f;
        }
        let wanted: std::collections::HashSet<&str> = plays.iter().map(|p| p.key.as_str()).collect();
        let gone: Vec<String> = self.players.keys().filter(|k| !wanted.contains(k.as_str())).cloned().collect();
        for k in gone {
            self.last.remove(&k);
            if let Some(e) = self.players.remove(&k)
                && let Ok(em) = self.app.world_mut().get_entity_mut(e)
            {
                em.despawn();
            }
        }
        for p in plays {
            // doppler: the frame's change in distance as a speed toward (faster, higher) or away (slower, lower)
            let speed = match p.at {
                Some(at) => {
                    let d = self.distance(at);
                    let v = self.last.insert(p.key.clone(), d).map(|d0| (d - d0) as f64 * self.fps).unwrap_or(0.0);
                    p.speed * SOUND / (SOUND + v.clamp(-0.5 * SOUND, 0.5 * SOUND))
                }
                None => p.speed,
            };
            match self.players.get(&p.key).copied() {
                Some(e) => {
                    // (a player's volume is its first frame's: SamplePlayer is immutable; a fade belongs on a bus)
                    let world = self.app.world_mut();
                    if let Some(mut ps) = world.get_mut::<PlaybackSettings>(e)
                        && ps.speed != speed
                    {
                        ps.speed = speed;
                    }
                    if let (Some(at), Some(mut tr)) = (p.at, world.get_mut::<Transform>(e)) {
                        tr.translation = Vec3::from(at);
                    }
                }
                None => {
                    let Some(h) = self.sample(&p.file) else { continue };
                    let mut player = SamplePlayer::new(h).with_volume(Volume::Linear(p.volume));
                    if p.looping {
                        player = player.looping();
                    }
                    let world = self.app.world_mut();
                    let e = world.spawn((player, PlaybackSettings::default().with_speed(speed))).id();
                    for fx in &p.fx {
                        world.spawn((fx.clone(), bevy_seedling::pool::sample_effects::EffectOf(e)));
                    }
                    // a placed sound: its place on the player, heard through a spatial node after its effects
                    if let Some(at) = p.at {
                        world.entity_mut(e).insert(Transform::from_translation(Vec3::from(at)));
                        let mut node = SpatialBasicNode::default();
                        if let Some(hz) = p.muffle {
                            node.muffle_cutoff_hz = hz.clamp(20.0, 20_480.0);
                        }
                        world.spawn((node, bevy_seedling::pool::sample_effects::EffectOf(e)));
                    }
                    self.players.insert(p.key.clone(), e);
                }
            }
        }
        let t0 = std::time::Instant::now();
        self.app.update();
        let t1 = std::time::Instant::now();
        let n = (self.t(f + 1) - self.t(f)).max(0) as usize;
        let pcm = chunk(self.app.world_mut(), n);
        if std::env::var_os("RAEA_SEEDLING_TRACE").is_some() && t0.elapsed().as_secs_f64() > 0.002 {
            eprintln!("raea seedling: frame {f}: update {:.1} ms, process {:.1} ms, {} players", (t1 - t0).as_secs_f64() * 1e3, t1.elapsed().as_secs_f64() * 1e3, self.players.len());
        }
        self.next = f + 1;
        let l: Vec<f32> = pcm.iter().step_by(2).copied().collect();
        let r: Vec<f32> = pcm.iter().skip(1).step_by(2).copied().collect();
        raea_media::Audio { rate: RATE, channels: vec![l, r] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_wav(dir: &Path) -> PathBuf {
        let p = dir.join("blip.wav");
        let n = RATE as usize / 4;
        let l: Vec<f32> = (0..n).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.5 * (1.0 - i as f32 / n as f32)).collect();
        raea_mixer::write_wav(&raea_media::Audio { rate: RATE, channels: vec![l.clone(), l] }, &p).unwrap();
        p
    }

    /// two hosts given the same plays on the same frames make the same samples, and the sample sounds on its frame
    #[test]
    fn same_twice() {
        let dir = std::env::temp_dir().join(format!("raea-seedling-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blip = tone_wav(&dir);
        let run = || {
            let mut h = Host::new(30.0);
            let mut out: Vec<f32> = Vec::new();
            for f in 0..45i64 {
                let mut plays = Vec::new();
                if (10..30).contains(&f) {
                    plays.push(Play { key: "a".into(), file: blip.clone(), volume: 0.8, speed: 1.0, looping: false, fx: vec![], at: None, muffle: None });
                }
                if f >= 20 {
                    plays.push(Play { key: "b".into(), file: blip.clone(), volume: 0.5, speed: 1.5, looping: true, fx: vec![ToneFx::new("Freeverb", serde_json::json!({"roomSize": 0.8, "wet": 0.4}))], at: None, muffle: None });
                }
                out.extend(h.frame(f, &plays).channels[0].iter());
            }
            out
        };
        let a = run();
        let b = run();
        assert_eq!(a.len(), 45 * 1600);
        let worst = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0f32, f32::max);
        assert_eq!(worst, 0.0, "two runs differ by {worst}");
        let first = a.iter().position(|x| x.abs() > 1e-4).unwrap();
        // the first player starts on frame 10's sample (seedling may take a block or two to start it)
        assert!(first >= 10 * 1600 && first < 10 * 1600 + 2 * BLOCK as usize, "first sound at {first}");
        assert!(a[20 * 1600..].iter().any(|x| x.abs() > 1e-3));
    }

    /// a placed sound is heard from the listener: louder in the ear it is on, quieter far off, higher coming closer
    #[test]
    fn placed() {
        let dir = std::env::temp_dir().join(format!("raea-seedling-placed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blip = tone_wav(&dir);
        // (left, right) energy over frames 2..8, the listener at the origin facing -z
        let hear = |at: [f32; 3]| {
            let mut h = Host::new(30.0);
            h.listen([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]);
            let (mut l, mut r) = (0f32, 0f32);
            for f in 0..8i64 {
                let a = h.frame(f, &[Play { key: "a".into(), file: blip.clone(), volume: 1.0, speed: 1.0, looping: true, fx: vec![], at: Some(at), muffle: None }]);
                if f >= 2 {
                    l += a.channels[0].iter().map(|x| x * x).sum::<f32>();
                    r += a.channels[1].iter().map(|x| x * x).sum::<f32>();
                }
            }
            (l, r)
        };
        let (l, r) = hear([2.0, 0.0, 0.0]);
        assert!(r > 2.0 * l, "a sound 2 m to the right: left {l}, right {r}");
        let (l2, r2) = hear([-2.0, 0.0, 0.0]);
        assert!(l2 > 2.0 * r2, "a sound 2 m to the left: left {l2}, right {r2}");
        let (nl, nr) = hear([0.0, 0.0, -1.0]);
        let (fl, fr) = hear([0.0, 0.0, -20.0]);
        assert!(nl + nr > 4.0 * (fl + fr), "near {} far {}", nl + nr, fl + fr);
        // doppler: coming closer at 10 m/s plays faster than standing
        let mut h = Host::new(30.0);
        h.listen([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]);
        let mut speed = 1.0;
        for f in 0..3i64 {
            let z = -20.0 + f as f32 * 10.0 / 30.0;
            h.frame(f, &[Play { key: "car".into(), file: blip.clone(), volume: 1.0, speed: 1.0, looping: true, fx: vec![], at: Some([0.0, 0.0, z]), muffle: None }]);
            let e = h.players["car"];
            speed = h.app.world().get::<PlaybackSettings>(e).unwrap().speed;
        }
        assert!((speed - 343.0 / 333.0).abs() < 1e-3, "approaching at 10 m/s: speed {speed}");
    }
}
