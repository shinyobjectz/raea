//! A composition rendered to frames and files by the engine (the Bevy bridge, first slice; raea/motion's
//! spec, "Rendering"). React runs on V8 inside Bevy (ReactPlugin); the bridge steps its Clock frame by frame and, from
//! each committed tree:
//!
//!   - the 2D page is laid out and drawn by bevy_ui (raea-runtime-js's page, src/page.rs; transparent where nothing is);
//!   - each <Video> is decoded natively (raea-media: VideoToolbox / Symphonia) at its data-source-time and drawn in
//!     its element's place, so it stacks with the page as the browser would stack it, fitted by the same CSS
//!     (object-fit);
//!   - <Audio> and a sounding <Video> become sound: audio rows, one per stretch the source time runs on unbroken;
//!   - the 3D elements (data-3d: PerspectiveCamera, lights, Mesh with its Manifold data-shape and material) become
//!     camera, light and mesh rows: ECS entities in Bevy, the 3D drawn under the page (the page's elements that hold
//!     the scene have their backgrounds cleared to show it; their colour becomes the scene's clear colour).
//!
//! The frames become one raea-render program (the painted pages as image rows, each for the frames it shows; the sound and 3D
//! as rows, keyed by frame), built as raea-render's IR directly and drawn by raea-render.
//!
//!   raea render EDIT.tsx -o OUT.mp4|OUT.png|DIR [--frame N | --at A,B] [--composition ID]
//!                     [--w 1080 --h 1920 --fps 30 --frames N]   (for a default export without a <Composition>)
//!                     [--10bit]   (HEVC Main10: decoded colours match the render exactly; H.264 8-bit by default)
//!                     [--kbps N | --quality draft|high|master|0..1]   (the video's average bitrate, by default from its
//!                     size and frame rate: 1080p30 about 20 Mbit/s, 4K30 about 80; or VideoToolbox's constant quality)

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde_json::{Map, Value, json};

pub(crate) use raea_runtime_react::scene::*;
pub(crate) use raea_runtime_react::{ReactKind, ReactOutbox, ReactPlugin, ReactProps};

mod bevy_frame;
use bevy_frame::*;
mod pages;
use pages::*;
mod program;
use program::*;
mod fed;
pub use fed::*;
mod sound;
use sound::*;
mod render;
pub use render::*;
mod review;
mod run;
pub use run::*;
pub use raea_sys_page::{not_applied, render_said};
mod scene;
pub use scene::{all_items, frame_of, items};

/// One <Audio> (or sounding <Video>) on a frame.
#[derive(Clone, Debug)]
pub(crate) struct Sound {
    src: String,
    at: f64,
    volume: f64,
    rate: f64,
    /// the element (its React node): one stretch per element
    key: String,
    /// seconds of fade at the element's start and end (data-fade-in/out), applied by raea-render at the audio rate
    fade_in: Option<f64>,
    fade_out: Option<f64>,
    /// the bus it plays into (data-bus)
    bus: Option<String>,
}

/// `render --manifest PATH` (the CLI's; render_bundle keeps its signature for the iPhone): where render.json goes.
static MANIFEST: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// The manifest, its folder made first (2D stills write it before their frames): the composition's length in frames
/// (whatever was rendered of it; raea check --quick samples by it, #86), boxes as drawn, findings, sounds, and whether
/// it drew 3D (an export in layers draws the 3D in the layer under the page, docs/layers.feature LA2a).
fn write_manifest(cx: &mut Ctx, (w, h, fps, length): (u32, u32, f64, i64), found: Vec<raea_core_facts::Finding>, heard: impl FnOnce(&Ctx) -> Value) -> R<()> {
    let (Some(path), Some(frames)) = (MANIFEST.lock().unwrap().clone(), cx.manifest.take()) else { return Ok(()) };
    path.parent().map_or(Ok(()), std::fs::create_dir_all).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut v = json!({"width": w, "height": h, "fps": fps, "length": length, "frames": frames, "findings": raea_core_facts::finding::ranked(found), "sounds": heard(cx), "not_applied": not_applied::seen(), "layers": raea_sys_layers::finish(fps), "has3d": cx.scene3d.is_some() || cx.live3d.is_some()});
    // the parts SG9 needs (scene.rs's SEEN: stills' parts on screen, a longer render's remounts), and --props and --scale
    for more in [scene::SEEN.lock().unwrap().take(), GIVEN.lock().unwrap().clone()] {
        v.as_object_mut().into_iter().zip(more.and_then(|m| m.as_object().cloned())).for_each(|(m, more)| m.extend(more));
    }
    std::fs::write(&path, v.to_string()).map_err(|e| format!("{}: {e}", path.display()))
}

/// `render --stage WxH`: a responsive composition (one that lays out from useVideoConfig) laid out at a stage's size, as
/// the legacy app's stage plays it (live.rs open), so a game's render can be checked against its frames at its own size.
static STAGE: std::sync::Mutex<Option<(u32, u32)>> = std::sync::Mutex::new(None);

/// `render --10bit` (or RAEA_EXPORT_10BIT=1): the video as HEVC Main10 from P010, whose decoded colours match the
/// render exactly (8-bit limited-range H.264 errs up to 2/255 by its own round trip; lightspeed Q3.colour).
static TEN_BIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `render --aov` (docs/raytracing.feature RT1): each frame's motion vectors and depth beside its picture, in DIR/motion
/// and DIR/depth, for a neural or denoising pass after the render (`raea neural`).
static AOV: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `render --alpha`: PNGs (a frame or a folder) kept transparent where the composition draws nothing, the pages
/// composited over a clear background instead of black: overlays for another editor's timeline.
static ALPHA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `render --kbps N`: the video's average bitrate (raea_io_media::default_kbps when 0, from the size and frame rate).
static KBPS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// `render --quality Q` (0..1, or draft 0.6, high 0.85, master 0.95), as f32 bits: constant quality in place of a
/// bitrate (u32::MAX: none).
static QUALITY: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);

fn render_opts() -> raea_runtime_encode::RenderOpts {
    use std::sync::atomic::Ordering::Relaxed;
    let kbps = Some(KBPS.load(Relaxed)).filter(|k| *k > 0);
    let quality = Some(QUALITY.load(Relaxed)).filter(|q| *q != u32::MAX).map(f32::from_bits);
    raea_runtime_encode::RenderOpts { ten_bit: TEN_BIT.load(Relaxed), kbps, quality, ..Default::default() }
}

/// What one frame of the composition is.
pub struct Frame {
    /// a <Mix>: the master's loudness target and true-peak ceiling (the master row)
    mix: Option<(f64, f64)>,
    /// the page's picture as an asset (its hash; the PNG written already), None where nothing is painted: frames keep
    /// only this, so a long composition doesn't hold every frame's pixels (450 frames at 1080x1920 is 3.7 GB, past
    /// what an iPhone lets an app keep)
    page: Option<String>,
    sounds: Vec<Sound>,
    pub scene: Vec<Node3>,
    clear: Option<String>,
    /// the page is the frame before's (nothing changed, nothing painted)
    same: bool,
    /// each <Screen>'s picture this frame on the bevy_ui page: its name and the asset (as `page`), drawn into its image
    /// by raea-render's engine (the `screenpage` rows)
    screens: Vec<(String, String)>,
}

pub(crate) struct Ctx<'a> {
    dir: &'a Path,
    work: &'a Path,
    sounds: Vec<Sound>,
    scene: Vec<Node3>,
    clear: Option<String>,
    marker: Option<i64>,
    mix: Option<(f64, f64)>,
    /// whether a file has a sound track (a silent take's <Video> makes no sound row)
    audible: HashMap<PathBuf, bool>,
    /// generated sounds by model|prompt|seconds: the file made, or None when it couldn't be (said once)
    made: HashMap<String, Option<PathBuf>>,
    /// `--manifest`: what each frame shows, as laid out (each video's source, time and box; each text's words and box)
    manifest: Option<Vec<Value>>,
    /// the frame being snapshotted, for the manifest
    now: i64,
    /// `--manifest`: each line said (raea/speech: its data-speech) and the first and last frames it was on
    said: std::collections::BTreeMap<String, (i64, i64)>,
    /// a fed render's 3D (render_fed): the scene's rows opened as a live engine, each frame drawn into its target and
    /// the page painted over it, as play draws it (Live::prepare)
    scene3d: Option<raea_render::engine::Engine>,
    /// a game's fed render: its 3D made from each frame's own scene as the frame is stepped (live_scene), as play
    /// draws it, rather than from rows made beforehand for the whole composition; `scene3d` opens at the first frame
    /// with any 3D
    live3d: Option<LiveScene>,
    /// the space's brand kit as CSS for every frame (brand_css)
    brand: Option<String>,
}

/// A sound to generate (<Audio src={lyria("…", {seconds})} /> or sao): made once through raea-render's store (cached by its own
/// index, counted in $RAEA_GEN_LEDGER against the run's cap), then played as the file it made. False when it couldn't
/// be made (said once): the element is left out. The spec always said the renderer generates these; until 2026-10-17
/// nothing did, and every generated sound was silent.
fn generated(props: &mut Map<String, Value>, cx: &mut Ctx) -> bool {
    let (Some(model), Some(prompt)) = (props.get("data-generate").and_then(|v| v.as_str()).map(str::to_string), props.get("data-prompt").and_then(|v| v.as_str()).map(str::to_string)) else { return true };
    let seconds = num(props, "data-seconds").unwrap_or(4.0);
    // the generator's own settings (a TokenSynth note's pitch and seed) make another sound
    let options = props.get("data-options").and_then(|v| v.as_str()).map(str::to_string);
    let key = format!("{model}|{prompt}|{seconds}|{}", options.as_deref().unwrap_or(""));
    if !cx.made.contains_key(&key) {
        let store = raea_io_assets::Store::beside(&cx.dir.join("edit.tsx"));
        let (params, seed) = raea_runtime_sound::generated_params(seconds, options.as_deref());
        let got = store.make(&model, &prompt, seed, &params).map(|a| store.path(&a));
        if let Err(e) = &got {
            eprintln!("raea: {model} \"{prompt}\" wasn't generated, so it's silent: {e}");
        }
        cx.made.insert(key.clone(), got.ok());
    }
    match cx.made.get(&key).cloned().flatten() {
        Some(file) => { props.insert("src".into(), Value::String(file.display().to_string())); true }
        None => false,
    }
}

/// A sound the frame plays: the element's source at its source time, gain and fades, keyed by its React node.
fn push_sound(cx: &mut Ctx, props: &Map<String, Value>, skey: &str) {
    let muted = props.get("data-muted").and_then(|v| v.as_str()) == Some("true");
    // linear gain, any non-negative (above 1 boosts); data-gain-db, when the element carries one, wins
    let volume = num(props, "data-gain-db").map(|db| 10f64.powf(db / 20.0)).or_else(|| num(props, "data-volume")).unwrap_or(1.0);
    if let (Some(src), Some(at)) = (props.get("src").and_then(|v| v.as_str()), num(props, "data-source-time")) {
        let Some(file) = raea_io_net::cache::remote(src).map_or_else(|| Some(media_path(cx.dir, src)), Result::ok) else { return }; // http(s): raea's URL cache (#57)
        // a sound track with samples: a short window says so (decoding the whole take for it held a long take whole)
        let audible = *cx.audible.entry(file.clone()).or_insert_with(|| raea_runtime_sound::there(&file, src) && raea_io_media::decode_audio_window(&file, 0.0, 0.05).is_ok_and(|a| a.len() > 0)); // a file that isn't there is named, once (sound's there)
        if !muted && volume > 0.0 && audible {
            // raea's mix: dialogue at one level, gainDb an offset on it (raea-sys-mix level.rs), as live play has it
            let volume = volume * raea_sys_mix::level::gain(&file, num(props, "data-clip-from").unwrap_or(0.0), num(props, "data-clip-to"), props.get("data-bus").and_then(|v| v.as_str())) as f64;
            cx.sounds.push(Sound { src: file.display().to_string(), at, volume, rate: num(props, "data-playback-rate").unwrap_or(1.0), key: skey.to_string(), fade_in: num(props, "data-fade-in"), fade_out: num(props, "data-fade-out"), bus: props.get("data-bus").and_then(|v| v.as_str()).map(str::to_string) });
        }
    }
}

/// The render manifest's sounds, what was heard (apps/app's listen tracks are drawn from them): each clip, by its
/// element, from the first frame it played to the frame after its last, with its source, source time at its first
/// frame, gain, bus, and its model and prompt when one made it; each note the Tone graph played (a Music part's
/// `music:NAME:...` key, its node, start and length in seconds, pitch and velocity); each line said, from the first
/// frame it was on to the frame after its last.
fn heard(frames: &[(i64, Frame)], tones: &[raea_sys_mix::ToneCue], cx: &Ctx, fps: f64) -> Value {
    let made: HashMap<String, &str> = cx.made.iter().filter_map(|(k, f)| Some((f.as_ref()?.display().to_string(), k.as_str()))).collect();
    let mut clips: Vec<Value> = Vec::new();
    let mut at: HashMap<&str, usize> = HashMap::new();
    for (f, fr) in frames {
        for s in &fr.sounds {
            if let Some(&k) = at.get(s.key.as_str()) {
                clips[k]["to"] = json!(f + 1);
                continue;
            }
            let mut c = json!({"key": s.key, "from": f, "to": f + 1, "src": s.src, "at": (s.at * 1000.0).round() / 1000.0, "gainDb": ((20.0 * s.volume.max(1e-6).log10()) * 10.0).round() / 10.0});
            if let Some(b) = &s.bus {
                c["bus"] = json!(b);
            }
            // its playback rate and fades, when it has them (the CapCut export's audio clips take them)
            let extras = [("rate", Some(s.rate).filter(|r| *r != 1.0)), ("fadeIn", s.fade_in.filter(|v| *v > 0.0)), ("fadeOut", s.fade_out.filter(|v| *v > 0.0))];
            for (k, v) in extras.into_iter().filter_map(|(k, v)| v.map(|v| (k, v))) {
                c[k] = json!(v);
            }
            // a generated sound's key is model|prompt|seconds
            if let Some(k) = made.get(&s.src) {
                let (model, rest) = k.split_once('|').unwrap_or((k, ""));
                c["model"] = json!(model);
                c["prompt"] = json!(rest.rsplit_once('|').map_or(rest, |(p, _)| p));
            }
            at.insert(&s.key, clips.len());
            clips.push(c);
        }
    }
    // a note can be listed on more than one frame (three, in the listen fixture); the mixer plays it once, by its key
    let mut seen = std::collections::HashSet::new();
    let notes: Vec<Value> = tones
        .iter()
        .flat_map(|t| &t.evs)
        .filter(|e| e.get("e").and_then(|v| v.as_str()) == Some("ar") && seen.insert(e.get("k").map(|k| k.to_string())))
        .map(|e| {
            let hz = e.get("f").and_then(|f| f.get(0)).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let midi = if hz > 0.0 { (69.0 + 12.0 * (hz / 440.0).log2()).round() } else { 0.0 };
            json!({"key": e.get("k"), "node": e.get("n"), "time": e.get("t"), "seconds": e.get("d"), "midi": midi, "velocity": e.get("v")})
        })
        .collect();
    let lines: Vec<Value> = cx.said.iter().map(|(line, (f, l))| json!({"from": f, "to": l + 1, "line": serde_json::from_str::<Value>(line).unwrap_or(Value::Null)})).collect();
    json!({"fps": fps, "clips": clips, "notes": notes, "lines": lines})
}
