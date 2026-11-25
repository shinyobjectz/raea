//! Live sound: the composition's <Audio> and <Video> sound, and its Tone, played by the one mixer core (raea-mixer: a
//! Firewheel graph that also renders the bounce) as the frames reach it (One renderer, the native stage; no HTML
//! audio). This file gathers each frame's sounds and Tone for it. Generated sounds (lyria, sao) play from what was
//! already made; live play never generates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

type R<T> = Result<T, String>;

/// One element's sound this frame: the mixer's own (its key, file and bus shared, so a frame's sounds hand over without
/// copies; ZeroHeap)
pub use raea_sys_mix::Sound;

mod names;
mod declared;
pub use declared::{always_plays_audio, there};
pub use names::*;

/// The live mixer: the one mixer core's device stream (raea_sys_mix::live::Live).
pub struct Mixer {
    live: raea_sys_mix::live::Live,
    /// how far each frame was from where the clock had it, in ms
    pub drift_ms: Vec<f64>,
}

impl Mixer {
    pub fn open() -> R<Mixer> {
        Ok(Mixer { live: raea_sys_mix::live::Live::open()?, drift_ms: Vec::new() })
    }

    /// The master a game's voices go through, as its render masters its sound: `gain_db` on their sum, then the
    /// look-ahead limiter at `ceiling_db`.
    pub fn master(&mut self, gain_db: f64, ceiling_db: f64) {
        self.live.master(gain_db, ceiling_db);
    }

    /// A mastered track is what plays: the core lets it through as it is.
    pub fn premastered(&mut self, on: bool) {
        self.live.premastered(on);
    }

    /// `file` loading on a worker now, nothing started: a timeline's first sounds ready before its clock starts.
    pub fn preload(&mut self, file: &Path) {
        self.live.preload(file);
    }

    /// Frame `f`'s sounds and Tone, stepped `lag` seconds after its own time.
    pub fn sync_frame(&mut self, f: i64, fps: f64, sounds: &[Sound], tone: &raea_sys_mix::ToneCue, lag: f64) {
        self.live.sync(f, fps, sounds, tone, lag);
        self.drift_ms.append(&mut self.live.drift_ms);
    }

    /// Everything stops (fading out).
    pub fn stop_all(&mut self) {
        self.live.stop_all();
    }

    /// The device stream stopped while nothing plays (raea_sys_mix::live::Live::rest).
    pub fn rest(&mut self) {
        self.live.rest();
    }

    /// The core's clock past the device's startup (raea_sys_mix::live::Live::settle_clock): nothing to wait for when the
    /// device opened early enough; else the first frame's placing waits for it.
    pub fn settle(&mut self) {
        self.live.settle_clock();
    }

    /// Seconds of sound the device has played.
    pub fn played(&self) -> f64 {
        self.live.played()
    }
}

/// The frame's Tone (raea/tone): the edit's graph (`<audio data-tone-graph>`, the same every frame) with its
/// sources resolved (a Sampler's or Player's files through the media folder, generated ones through the asset store),
/// and the events the frame shows (`<audio data-tone-evs>`).
pub fn tone_of(world: &mut bevy::prelude::World, dir: &Path, media_path: impl Fn(&Path, &str) -> PathBuf, make: bool) -> raea_sys_mix::ToneCue {
    let mut cue = raea_sys_mix::ToneCue::default();
    raea_runtime_react::scene::with_kinds(world, |world, q| for (k, p, _) in q.0.iter(world) {
        if k.0 != "audio" {
            continue;
        }
        let Some(p) = p else { continue };
        if let Some(g) = p.0.get("data-tone-graph").and_then(|v| v.as_str()) {
            cue.graph = Some(resolve_graph(g, dir, &media_path, make));
        }
        if let Some(evs) = p.0.get("data-tone-evs").and_then(|v| v.as_str())
            && let Ok(Value::Array(a)) = serde_json::from_str::<Value>(evs)
        {
            cue.evs.extend(a);
        }
    });
    cue
}

/// Frame `f`'s Tone (`tone_of`) and, when the page has `<Seedling>`s (raea/seedling), the frame's samples from
/// bevy_seedling's graph stepped beside the game (raea-seedling: the same samples in a render and a play), as the cue's
/// `pcm`: a voice of the core from the frame's sample.
pub fn tone_at(world: &mut bevy::prelude::World, f: i64, fps: f64, dir: &Path, media_path: impl Fn(&Path, &str) -> PathBuf, make: bool) -> raea_sys_mix::ToneCue {
    let t0 = std::time::Instant::now();
    #[cfg(feature = "seedling")]
    let plays = seedling_plays(world, dir, &media_path, make);
    #[cfg(feature = "seedling")]
    let ears = if plays.iter().any(|p| p.at.is_some()) { scene_ears(world) } else { None };
    let t1 = std::time::Instant::now();
    #[cfg_attr(not(any(feature = "ml", feature = "seedling")), allow(unused_mut))] let mut cue = tone_of(world, dir, media_path, make);
    // the lines said on this frame (raea/speech): their samples, waited for when the worker is behind
    #[cfg(feature = "ml")]
    {
        let mut speech = None;
        raea_runtime_react::scene::with_kinds(world, |world, q| {
            let items: Vec<&Map<String, Value>> = q.0.iter(world).filter(|(k, p, _)| k.0 == "audio" && p.is_some_and(|p| p.0.contains_key("data-speech"))).filter_map(|(_, p, _)| p.map(|p| &p.0)).collect();
            speech = raea_runtime_calls::ml::frame_speech(&items, f, fps);
        });
        cue.speech = speech;
    }
    let t2 = std::time::Instant::now();
    #[cfg(feature = "seedling")]
    {
        if !plays.is_empty() || world.get_non_send::<SeedlingHost>().is_some() {
            if world.get_non_send::<SeedlingHost>().is_none() {
                world.insert_non_send(SeedlingHost(raea_seedling::Host::new(fps)));
            }
            let mut h = world.non_send_mut::<SeedlingHost>();
            if (h.0.fps - fps).abs() > 1e-9 {
                h.0 = raea_seedling::Host::new(fps);
            }
            if let Some((eye, look, up)) = ears {
                h.0.listen(eye, look, up);
            }
            cue.pcm = Some(std::sync::Arc::new(h.0.frame(f, &plays)));
        }
    }
    #[cfg(not(feature = "seedling"))]
    let _ = (f, fps);
    if raea_io_trace::stat("RAEA_SEEDLING_TRACE") && t0.elapsed().as_secs_f64() > 0.008 {
        eprintln!("raea seedling: tone_at {f}: plays {:.1} ms, tone {:.1} ms, seedling {:.1} ms", (t1 - t0).as_secs_f64() * 1e3, (t2 - t1).as_secs_f64() * 1e3, t2.elapsed().as_secs_f64() * 1e3);
    }
    cue
}

/// Seedling's graph made ahead of the first frame, when the bundle has <Seedling>s, and the sources the page shows now
/// decoded.
pub fn warm_seedling(world: &mut bevy::prelude::World, fps: f64, js: &Path, dir: &Path) {
    #[cfg(feature = "seedling")]
    {
        if world.get_non_send::<SeedlingHost>().is_some() || !std::fs::read_to_string(js).is_ok_and(|t| t.contains("data-seedling")) {
            return;
        }
        // the edit's sounds (its asset store and media folder) decoded on a thread from now
        let mut files: Vec<PathBuf> = Vec::new();
        for sub in ["assets", "media", "public"] {
            if let Ok(rd) = std::fs::read_dir(dir.join(sub)) {
                for e in rd.flatten() {
                    let p = e.path();
                    let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                    if matches!(ext.as_str(), "wav" | "mp3" | "ogg" | "flac" | "m4a" | "aac") && e.metadata().is_ok_and(|m| m.len() < 64 << 20) {
                        files.push(p);
                    }
                }
            }
        }
        raea_seedling::prefetch(files);
        let plays = seedling_plays(world, dir, &raea_runtime_react::scene::media_path, false);
        let mut h = raea_seedling::Host::new(fps);
        h.preload(&plays);
        world.insert_non_send(SeedlingHost(h));
    }
    #[cfg(not(feature = "seedling"))]
    let _ = (world, fps, js, dir);
}

/// The game's seedling graph, beside its world.
#[cfg(feature = "seedling")]
struct SeedlingHost(raea_seedling::Host);

/// The `<Seedling>`s on the page this frame (`<audio data-seedling>`), their sources resolved.
#[cfg(feature = "seedling")]
fn seedling_plays(world: &mut bevy::prelude::World, dir: &Path, media_path: &impl Fn(&Path, &str) -> PathBuf, make: bool) -> Vec<raea_seedling::Play> {
    let mut out: Vec<raea_seedling::Play> = Vec::new();
    raea_runtime_react::scene::with_kinds(world, |world, q| for (k, p, _) in q.0.iter(world) {
        if k.0 != "audio" {
            continue;
        }
        let Some(spec) = p.and_then(|p| p.0.get("data-seedling")).and_then(|v| v.as_str()).and_then(|t| serde_json::from_str::<Value>(t).ok()) else { continue };
        // a media file, or a generated sound from the asset store (a render makes it; play only finds it)
        let file = match spec.get("src") {
            Some(Value::String(s)) => media_path(dir, s),
            Some(Value::Object(o)) => {
                let (Some(model), Some(prompt)) = (o.get("generate").and_then(|v| v.as_str()), o.get("prompt").and_then(|v| v.as_str())) else { continue };
                let mut opts = o.clone();
                opts.remove("generate");
                opts.remove("prompt");
                let seed = opts.get("seed").and_then(|v| v.as_u64()).unwrap_or(1);
                let store = raea_io_assets::Store::beside(&dir.join("edit.tsx"));
                let got = if make { store.make(model, prompt, seed, &Value::Object(opts)).map(Some) } else { store.made(model, prompt, seed, &Value::Object(opts)) };
                match got {
                    Ok(Some(a)) => store.path(&a),
                    _ => continue,
                }
            }
            _ => continue,
        };
        let src = file.display().to_string();
        let src = src.as_str();
        let num = |k: &str, d: f64| spec.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
        let fx = spec.get("fx").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| Some(raea_seedling::ToneFx::new(x.get("kind")?.as_str()?, x.get("options").cloned().unwrap_or(Value::Null)))).collect()).unwrap_or_default();
        out.push(raea_seedling::Play {
            key: spec.get("key").and_then(|v| v.as_str()).unwrap_or(src).to_string(),
            file: file.clone(),
            volume: num("volume", 1.0) as f32,
            speed: num("speed", 1.0),
            looping: spec.get("loop").and_then(|v| v.as_bool()).unwrap_or(false),
            fx,
            at: spec.get("at").and_then(|v| v.as_array()).filter(|a| a.len() == 3).map(|a| [0, 1, 2].map(|i| a[i].as_f64().unwrap_or(0.0) as f32)),
            muffle: spec.get("muffle").and_then(|v| v.as_f64()).map(|x| x as f32),
        });
    });
    // the page's order isn't the same in a render and a play: players start in key order
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out.dedup_by(|a, b| a.key == b.key);
    out
}

/// The ears placed sounds are heard from this frame: the 3D scene's camera (its <PerspectiveCamera> position, lookAt
/// and up, in the Canvas3D's frame, as <Sound3D at> is).
#[cfg(feature = "seedling")]
fn scene_ears(world: &mut bevy::prelude::World) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
    raea_runtime_react::scene::with_kinds(world, |world, q| {
        for (_, p, _) in q.0.iter(world) {
            let Some(p) = p else { continue };
            if p.0.get("data-3d").and_then(|v| v.as_str()) != Some("PerspectiveCamera") {
                continue;
            }
            let props: Value = p.0.get("data-props").and_then(|v| v.as_str()).and_then(|t| serde_json::from_str(t).ok())?;
            let v3 = |k: &str, d: [f32; 3]| props.get(k).and_then(|v| v.as_array()).filter(|a| a.len() == 3).map(|a| [0, 1, 2].map(|i| a[i].as_f64().unwrap_or(0.0) as f32)).unwrap_or(d);
            return Some((v3("position", [0.0, 0.0, 5.0]), v3("lookAt", [0.0, 0.0, 0.0]), v3("up", [0.0, 1.0, 0.0])));
        }
        None
    })
}

/// A Tone graph's sources as paths: "urls" (a Sampler's notes) and "url" (a Player) are a media file, or a generated
/// sound ({"generate": model, "prompt", "seconds", ...}) from the asset store: a render makes it if it isn't made yet
/// (`make`: SAO and TokenSynth run locally), live play only finds it (left out when it was never made).
fn resolve_graph(g: &str, dir: &Path, media_path: &impl Fn(&Path, &str) -> PathBuf, make: bool) -> String {
    let Ok(mut v) = serde_json::from_str::<Value>(g) else { return g.to_string() };
    let store = raea_io_assets::Store::beside(&dir.join("edit.tsx"));
    let resolve = |x: &Value| -> Option<Value> {
        match x {
            Value::String(s) => Some(Value::String(media_path(dir, s).display().to_string())),
            Value::Object(o) => {
                let model = o.get("generate")?.as_str()?;
                let prompt = o.get("prompt")?.as_str()?;
                let mut opts = o.clone();
                opts.remove("generate");
                opts.remove("prompt");
                let seed = opts.get("seed").and_then(|v| v.as_u64()).unwrap_or(1);
                let got = if make { store.make(model, prompt, seed, &Value::Object(opts)).map(Some) } else { store.made(model, prompt, seed, &Value::Object(opts)) };
                match got {
                    Ok(Some(a)) => Some(Value::String(store.path(&a).display().to_string())),
                    Ok(None) => None,
                    Err(e) => {
                        eprintln!("raea: {model} \"{prompt}\" wasn't made, so it's silent: {e}");
                        None
                    }
                }
            }
            _ => None,
        }
    };
    if let Some(nodes) = v.get_mut("nodes").and_then(|n| n.as_array_mut()) {
        for n in nodes {
            let Some(o) = n.get_mut("options").and_then(|o| o.as_object_mut()) else { continue };
            if let Some(Value::Object(urls)) = o.get("urls").cloned() {
                let r: Map<String, Value> = urls.iter().filter_map(|(k, x)| Some((k.clone(), resolve(x)?))).collect();
                o.insert("urls".into(), Value::Object(r));
            }
            if let Some(u) = o.get("url").cloned() {
                match resolve(&u) {
                    Some(x) => o.insert("url".into(), x),
                    None => o.remove("url"),
                };
            }
        }
    }
    v.to_string()
}

/// The sounds a page shows this frame, read from its React elements as a render reads them (motion.rs push_sound):
/// each audible <Audio>/<Video> with its source time, gain, rate and fades. Generated ones resolve to what was made.
/// Generated sounds a live play found not made, being made by a local model on a worker (sao: Stable Audio Open on Core
/// ML, free): by edit folder and sound, the file once it's made (None while it's being made, or when it failed).
static MAKING: std::sync::Mutex<Option<HashMap<String, Option<PathBuf>>>> = std::sync::Mutex::new(None);

/// A generated sound for live play: the file a worker made, once it has; the worker started the first time it's asked
/// for. Only local models: a paid one (lyria, speech) is made by a render, which counts it against the run's cap.
/// A generated <Audio>'s params as the asset store takes them, and its seed (as a Sampler's spec gives them, above):
/// its seconds and the generator's own settings (data-options: a TokenSynth note's midi, velocity, seed and
/// precision), the seed from them (1 without one). Every one of them is in the made sound's key.
pub fn generated_params(seconds: f64, options: Option<&str>) -> (Value, u64) {
    let mut params = serde_json::Map::new();
    if let Some(Value::Object(o)) = options.and_then(|s| serde_json::from_str::<Value>(s).ok()) {
        params.extend(o);
    }
    params.insert("seconds".into(), Value::from(seconds));
    let seed = params.get("seed").and_then(Value::as_u64).unwrap_or(1);
    (Value::Object(params), seed)
}

fn made_live(dir: &Path, model: &str, prompt: &str, seconds: f64, options: Option<&str>) -> Option<PathBuf> {
    if model != "sao" {
        return None;
    }
    let key = format!("{}\u{1}{model}\u{1}{prompt}\u{1}{seconds}\u{1}{}", dir.display(), options.unwrap_or(""));
    let (params, seed) = generated_params(seconds, options);
    let mut g = MAKING.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if let Some(v) = m.get(&key) {
        return v.clone();
    }
    m.insert(key.clone(), None);
    drop(g);
    let (dir, model, prompt) = (dir.to_path_buf(), model.to_string(), prompt.to_string());
    let _ = std::thread::Builder::new().name("raea live sound".into()).spawn(move || {
        // one at a time: each make starts the model in a process of its own (a game's ten sounds at once were ten)
        static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
        let store = raea_io_assets::Store::beside(&dir.join("edit.tsx"));
        match store.make(&model, &prompt, seed, &params) {
            Ok(a) => {
                let p = store.path(&a);
                if let Some(m) = MAKING.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    m.insert(key, Some(p));
                }
            }
            Err(e) => eprintln!("raea live: {model} \"{prompt}\" couldn't be made: {e}"),
        }
    });
    None
}

pub fn sounds_of(items: &[(&str, Option<&Map<String, Value>>, Option<u64>)], dir: &Path, media_path: impl Fn(&Path, &str) -> PathBuf, made: &mut HashMap<String, Option<PathBuf>>) -> Vec<Sound> {
    let mut out = Vec::new();
    sounds_into(items.iter().copied(), dir, media_path, made, &mut out);
    out
}
