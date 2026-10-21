//! raea-motion: an @raea/motion composition rendered by the engine (the Bevy bridge, first slice; raea/motion's
//! spec, "Rendering"). React runs on V8 inside Bevy (ReactPlugin); the bridge steps its Clock frame by frame and, from
//! each committed tree:
//!
//!   - the 2D page is built into a Blitz DOM and painted (transparent where nothing is), as raea-react always has;
//!   - each <Video> is decoded natively (raea-media: VideoToolbox / Symphonia) at its data-source-time and goes into
//!     the page as an <img> in the video's place, so it stacks with the page as the browser would stack it, fitted by
//!     the same CSS (object-fit);
//!   - <Audio> and a sounding <Video> become sound: audio rows, one per stretch the source time runs on unbroken;
//!   - the 3D elements (data-3d: PerspectiveCamera, lights, Mesh with its Manifold data-shape and material) become
//!     camera, light and mesh rows: ECS entities in Bevy, the 3D drawn under the page (the page's elements that hold
//!     the scene have their backgrounds cleared to show it; their colour becomes the scene's clear colour).
//!
//! The frames become one rdb program (the painted pages as image rows, each for the frames it shows; the sound and 3D
//! as rows, keyed by frame), built as rdb's IR directly and drawn by rdb; `-o OUT.sql` writes it as rdb SQL instead.
//!
//!   raea-motion render EDIT.tsx -o OUT.mp4|OUT.png|DIR|OUT.sql [--frame N | --at A,B] [--composition ID]
//!                     [--w 1080 --h 1920 --fps 30 --frames N]   (for a default export without a <Composition>)
//!                     [--10bit]   (HEVC Main10: decoded colours match the render exactly; H.264 8-bit by default)

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use bevy::prelude::*;
use serde_json::{Map, Value, json};

use crate::{ReactEntities, ReactKind, ReactOutbox, ReactPlugin, ReactProps, ReactText};

type R<T> = Result<T, String>;

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn num(p: &Map<String, Value>, k: &str) -> Option<f64> {
    p.get(k).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
}

fn copy_dir(from: &Path, to: &Path, depth: usize) -> R<()> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for e in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            if depth < 6 {
                copy_dir(&p, &to.join(&name), depth + 1)?;
            }
        } else if matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx" | "js" | "jsx" | "json" | "css")) {
            std::fs::copy(&p, to.join(&name)).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    Ok(())
}

const MEDIA_EXTS: [&str; 10] = ["mp4", "mov", "m4v", "mp3", "wav", "m4a", "png", "jpg", "webp", "gif"];

/// The edit's source with its imports pointed at the bundle's copy of @raea/motion (`@raea/motion`, `@raea/motion/3d`,
/// or a relative path into raea/motion/src), and media imported as modules turned into their paths (spec: "Media
/// imported as modules"): `import take from "./media/take01.mp4"` is `const take = "/abs/media/take01.mp4"`. Whole
/// statements, so an import over several lines is rewritten too. With `portable` a folder, imported media is copied
/// into its media/ and named `media/<file>`, so the bundle and that folder travel together (to the iPhone).
fn rewrite_into(src: &str, from_dir: &Path, motion_src: &Path, to_motion: &str, portable: Option<&Path>) -> R<String> {
    let re = regex::Regex::new(r#"(?s)\bimport\s+([^;'"]*?)\s+from\s+(["'])([^"']+)["']"#).unwrap();
    let mut out = String::new();
    let mut last = 0;
    for c in re.captures_iter(src) {
        let whole = c.get(0).unwrap();
        let what = c.get(1).unwrap().as_str().trim();
        let spec = c.get(3).unwrap().as_str();
        out.push_str(&src[last..whole.start()]);
        last = whole.end();
        let line = src[..whole.start()].matches('\n').count() + 1;
        let ext = Path::new(spec).extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
        let local = spec.starts_with('.') || spec.starts_with('/');
        if MEDIA_EXTS.contains(&ext.as_str()) && local {
            let path = from_dir.join(spec);
            if !path.exists() {
                return Err(format!("line {line}: {spec} doesn't exist (imported media resolves beside the edit)"));
            }
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            match portable {
                Some(dir) => {
                    let name = path.file_name().unwrap().to_string_lossy().to_string();
                    std::fs::create_dir_all(dir.join("media")).map_err(|e| e.to_string())?;
                    std::fs::copy(&path, dir.join("media").join(&name)).map_err(|e| format!("{}: {e}", path.display()))?;
                    out.push_str(&format!("const {what} = {:?}", format!("media/{name}")));
                }
                None => out.push_str(&format!("const {what} = {:?}", path.display().to_string())),
            }
            continue;
        }
        // @raea/space and @raea/terminal: their copies beside the SDK's (bundle_fresh: SDKS), on its one copy
        let sdk = SDKS.iter().find(|(name, _)| spec == *name || spec.strip_prefix(*name).is_some_and(|r| r.starts_with('/')));
        let target = if let Some((name, dir)) = sdk {
            let base = format!("{}{}", to_motion.strip_suffix("motion").unwrap_or(to_motion), dir.rsplit('/').nth(1).unwrap_or(dir));
            Some(format!("{base}{}", &spec[name.len()..]))
        } else if spec == "@raea/motion" {
            Some(to_motion.to_string())
        } else if let Some(rest) = spec.strip_prefix("@raea/motion/") {
            Some(format!("{to_motion}/{rest}"))
        } else if local {
            let abs = normalize(&from_dir.join(spec));
            abs.strip_prefix(motion_src).ok().map(|rest| if rest.as_os_str().is_empty() { to_motion.to_string() } else { format!("{to_motion}/{}", rest.display()) })
        } else {
            None
        };
        match target {
            Some(t) => out.push_str(&format!("import {what} from \"{t}\"")),
            None => out.push_str(whole.as_str()),
        }
    }
    out.push_str(&src[last..]);
    Ok(out)
}

/// The SDKs built on @raea/motion's panel runtime that a bundle may import (apps/void/shell.feature: Two SDKs): each copied
/// into the work folder beside motion/ as its package's folder name (space/, terminal/), its own imports of
/// @raea/motion pointed at that one copy, so a controller's hooks read the Clock the engine drives.
const SDKS: [(&str, &str); 2] = [("@raea/space", "sdk/space/src"), ("@raea/terminal", "sdk/terminal/src")];

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            o => out.push(o),
        }
    }
    out
}

/// One <Audio> (or sounding <Video>) on a frame.
#[derive(Clone, Debug)]
struct Sound {
    src: String,
    at: f64,
    volume: f64,
    rate: f64,
    /// the element (its React node): one stretch per element
    key: String,
    /// seconds of fade at the element's start and end (data-fade-in/out), applied by rdb at the audio rate
    fade_in: Option<f64>,
    fade_out: Option<f64>,
    /// the bus it plays into (data-bus)
    bus: Option<String>,
}

/// A 3D element and its children.
#[derive(Clone, Debug)]
struct Node3 {
    /// kind and shape shared too, kept per part with its props (ZeroHeap: each was copied into a String a frame)
    kind: std::sync::Arc<str>,
    /// shared: a part's props are parsed once per change of its data-props and handed to each frame's scene, walk and
    /// rows by reference (ZeroHeap: a live 3D frame parsed and copied every part's props)
    props: std::sync::Arc<Map<String, Value>>,
    shape: Option<std::sync::Arc<str>>,
    kids: Vec<Node3>,
}

/// `render --manifest PATH` (the CLI's; render_bundle keeps its signature for the iPhone): where render.json goes.
static MANIFEST: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// `render --stage WxH`: a responsive composition (one that lays out from useVideoConfig) laid out at a stage's size, as
/// void's stage plays it (live.rs open), so a game's render can be checked against void's frames at its own size.
static STAGE: std::sync::Mutex<Option<(u32, u32)>> = std::sync::Mutex::new(None);

/// `render --10bit` (or RAEA_EXPORT_10BIT=1): the video as HEVC Main10 from P010, whose decoded colours match the
/// render exactly (8-bit limited-range H.264 errs up to 2/255 by its own round trip; lightspeed Q3.colour).
static TEN_BIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn render_opts() -> rdb::RenderOpts {
    rdb::RenderOpts { ten_bit: TEN_BIT.load(std::sync::atomic::Ordering::Relaxed), ..Default::default() }
}

/// What one frame of the composition is.
struct Frame {
    /// a <Mix>: the master's loudness target and true-peak ceiling (the master row)
    mix: Option<(f64, f64)>,
    /// the page's picture as an asset (its hash; the PNG written already), None where nothing is painted: frames keep
    /// only this, so a long composition doesn't hold every frame's pixels (450 frames at 1080x1920 is 3.7 GB, past
    /// what an iPhone lets an app keep)
    page: Option<String>,
    sounds: Vec<Sound>,
    scene: Vec<Node3>,
    clear: Option<String>,
    /// the page's paint, still on the GPU: collected into `page` after the next frame is submitted (`settle_page`)
    pending: Option<raea::row_render::Painting>,
    /// the page is the frame before's (nothing changed, nothing painted)
    same: bool,
}

struct Ctx<'a> {
    dir: &'a Path,
    work: &'a Path,
    decoders: HashMap<PathBuf, Box<dyn raea_media::VideoDecoder>>,
    stills: HashMap<(PathBuf, i64), PathBuf>,
    sounds: Vec<Sound>,
    scene: Vec<Node3>,
    clear: Option<String>,
    marker: Option<i64>,
    mix: Option<(f64, f64)>,
    holders: Vec<(raea::dom::NodeId, Map<String, Value>)>,
    /// whether a file has a sound track (a silent take's <Video> makes no sound row)
    audible: HashMap<PathBuf, bool>,
    /// the page every frame is laid out on, and this frame's root in it
    dom: Option<raea::dom::Dom>,
    root: Option<raea::dom::NodeId>,
    /// generated sounds by model|prompt|seconds: the file made, or None when it couldn't be (said once)
    made: HashMap<String, Option<PathBuf>>,
    /// `--manifest`: what each frame shows, as laid out (each video's source, time and box; each text's words and box)
    manifest: Option<Vec<Value>>,
    /// the frame being snapshotted, for the manifest
    now: i64,
    /// the still each video node shows on the retained page, so an unchanged one isn't reloaded and a frame with no
    /// change at all isn't painted again (it reuses the frame before's page)
    shown: HashMap<u64, PathBuf>,
    painted: bool,
    /// a fed render's page (render_bundle): each frame is painted into it on the GPU, for rdb to composite as it is
    fed: Option<raea::quads::PageBuffer>,
    /// a fed render's 3D (render_fed): the scene's rows opened as a live engine, each frame drawn into its target and
    /// the page painted over it, as play draws it (Live::prepare)
    scene3d: Option<rdb::engine::Engine>,
    /// a game's fed render: its 3D made from each frame's own scene as the frame is stepped (live_scene), as play
    /// draws it, rather than from rows made beforehand for the whole composition; `scene3d` opens at the first frame
    /// with any 3D
    live3d: Option<LiveScene>,
    /// videos as GPU textures (VideoFrames::on) and their decoders
    textures: bool,
    vframes: VideoFrames,
    /// the space's brand kit as CSS for every frame (brand_css)
    brand: Option<String>,
}

/// The space's brand kit for a composition `w` px wide: the nearest brand/brand.css at or above the edit's folder (up
/// to four levels), its @import lines dropped (fonts load by name), then `:root { --brand-scale: 2.5 * w / 1080 }`.
pub(crate) fn brand_css(dir: &Path, w: u32) -> Option<String> {
    let mut d = Some(dir);
    for _ in 0..5 {
        let here = d?;
        let f = here.join("brand").join("brand.css");
        if let Ok(css) = std::fs::read_to_string(&f) {
            let kept: Vec<&str> = css.lines().filter(|l| !l.trim_start().starts_with("@import")).collect();
            return Some(format!("{}\n:root {{ --brand-scale: {}; }}\n", kept.join("\n"), 2.5 * w as f64 / 1080.0));
        }
        d = here.parent();
    }
    None
}

/// The CSS a bundle's modules imported (`import "./x.css"`, CSS modules: raea-bundle, A2), carried in the bundle's
/// last `//# raea-css=` line as a JSON string
pub(crate) fn bundle_css(js: &str) -> Option<String> {
    let line = js.lines().rev().take(4).find_map(|l| l.strip_prefix("//# raea-css="))?;
    serde_json::from_str::<String>(line).ok().filter(|c| !c.is_empty())
}

/// A page's own CSS: the space's brand kit (brand_css), then the CSS the bundle `js` imported (bundle_css)
pub(crate) fn page_css(dir: &Path, w: u32, js: &Path) -> Option<String> {
    let bundled = std::fs::read_to_string(js).ok().and_then(|t| bundle_css(&t));
    match (brand_css(dir, w), bundled) {
        (Some(b), Some(c)) => Some(format!("{b}\n{c}")),
        (b, c) => b.or(c),
    }
}

/// A media src as a file: a path from staticFile (/media/x under the edit's folder), absolute, or relative to it.
pub(crate) fn media_path(dir: &Path, src: &str) -> PathBuf {
    if let Some(rest) = src.strip_prefix("/media/") {
        let p = dir.join("media").join(rest);
        if p.exists() {
            return p;
        }
    }
    let p = PathBuf::from(src);
    if p.is_absolute() && p.exists() {
        return p;
    }
    dir.join(src.trim_start_matches('/'))
}

mod scene3d;
// scene.bounds, the native call place() stands models on surfaces with (motion/scene_lint.rs)
pub(crate) use scene3d::clip3d::lint::register as register_scene;
// splats from footage: `splat train VIDEO|DIR -o OUT.ply`, `splat setup` (motion/splat_train.rs)
mod splat_train;
// physics for compositions: rigid bodies (Rapier), parts cut from a glTF, goo (tools/goo): motion/physics.rs
pub(crate) mod physics;
// the sound tools: sao, tokensynth, setup-audio, sounds (motion/sound.rs)
mod sound;
// a song's energy map: `song FILE [-o OUT.song.json] [--plot OUT.png]` (motion/song.rs)
mod song;
// matchmove: `track` and the matchmove.solve native call (motion/track.rs)
pub(crate) mod track;

thread_local! {
    /// each part's data-props as last parsed: its text and the props, and the scene it was last read in
    static PROPS3D: std::cell::RefCell<HashMap<Entity, (String, std::sync::Arc<Map<String, Value>>, u64)>> = std::cell::RefCell::new(HashMap::new());
    /// each part's data-3d and data-shape as last read, shared
    static NAMES3D: std::cell::RefCell<HashMap<Entity, (std::sync::Arc<str>, Option<std::sync::Arc<str>>)>> = std::cell::RefCell::new(HashMap::new());
    /// scenes read (scene_of): what's in PROPS3D but not read in the last two is let go
    static SCENE3D: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A part's props from its data-props text: parsed again only when the text changed since the last scene
fn props3d(e: Entity, text: Option<&str>) -> std::sync::Arc<Map<String, Value>> {
    let Some(text) = text else { return std::sync::Arc::default() };
    let now = SCENE3D.with(|g| g.get());
    PROPS3D.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((t, p, seen)) = c.get_mut(&e) {
            *seen = now;
            if t.as_str() == text {
                return p.clone();
            }
            // in place where no one else holds them (the last scene let go) and the keys are the same: each value
            // updated where it is (ZeroHeap: a moving part's props were parsed into a new map each frame)
            if let Some(m) = std::sync::Arc::get_mut(p)
                && props_in_place(m, text)
            {
                t.clear();
                t.push_str(text);
                return p.clone();
            }
            let parsed: Map<String, Value> = serde_json::from_str(text).unwrap_or_default();
            t.clear();
            t.push_str(text);
            *p = std::sync::Arc::new(parsed);
            return p.clone();
        }
        let p = std::sync::Arc::new(serde_json::from_str::<Map<String, Value>>(text).unwrap_or_default());
        c.insert(e, (text.to_string(), p.clone(), now));
        p
    })
}

/// A part's kind and shape, the shared ones from its last read when they're the same
fn names3d(e: Entity, kind: &str, shape: Option<&str>) -> (std::sync::Arc<str>, Option<std::sync::Arc<str>>) {
    NAMES3D.with_borrow_mut(|c| {
        if let Some((k, s)) = c.get(&e)
            && &**k == kind
            && s.as_deref() == shape
        {
            return (k.clone(), s.clone());
        }
        let v: (std::sync::Arc<str>, Option<std::sync::Arc<str>>) = (kind.into(), shape.map(Into::into));
        if c.len() > 8192 {
            c.clear();
        }
        c.insert(e, v.clone());
        v
    })
}

/// `m` set to the object `text` holds, value by value in place: false (`m` then partly updated) when it isn't an
/// object with the same keys
fn props_in_place(m: &mut Map<String, Value>, text: &str) -> bool {
    let Ok(raw) = serde_json::from_str::<&serde_json::value::RawValue>(text) else { return false };
    if !crate::rawops::is_object(raw) {
        return false;
    }
    let (mut n, mut ok) = (0usize, true);
    crate::rawops::each_entry(raw, |k, v| {
        n += 1;
        if m.contains_key(k) {
            crate::rawops::put_raw(m, k, v);
        } else {
            ok = false;
        }
    });
    ok && n == m.len()
}

fn read3d(world: &World, e: Entity) -> Option<Node3> {
    let p = world.get::<ReactProps>(e)?;
    let (kind, shape) = names3d(e, p.0.get("data-3d")?.as_str()?, p.0.get("data-shape").and_then(|v| v.as_str()));
    let props = props3d(e, p.0.get("data-props").and_then(|v| v.as_str()));
    // a world's entities (<Entities>): its scene, already in world space, as meshes with their materials
    if &*kind == "Entities" {
        let kids = props.get("scene").and_then(Value::as_array).into_iter().flatten().filter(|n| n["kind"] == "Mesh").map(|n| {
            let mesh = n["mesh"].as_object().cloned().unwrap_or_default();
            let mut p = Map::new();
            p.insert("position".into(), n["position"].clone());
            p.insert("rotation".into(), n["rotation"].clone());
            p.insert("scale".into(), n["scale"].clone());
            let mut mat = Map::new();
            for k in ["color", "roughness", "metalness"] {
                if let Some(x) = mesh.get(k) {
                    mat.insert(k.into(), x.clone());
                }
            }
            Node3 { kind: "Mesh".into(), props: std::sync::Arc::new(p), shape: mesh.get("shape").and_then(Value::as_str).map(Into::into), kids: vec![Node3 { kind: "MeshStandardMaterial".into(), props: std::sync::Arc::new(mat), shape: None, kids: Vec::new() }] }
        }).collect();
        return Some(Node3 { kind, props: std::sync::Arc::default(), shape: None, kids });
    }
    let mut kids = Vec::new();
    kids3d(world, e, &mut kids);
    Some(Node3 { kind, props, shape, kids })
}

/// The 3D children of `e`, looking through plain elements between them (a Sequence's or a component's wrapper div
/// inside a Canvas3D), so 3D timed by a Sequence is still the scene's.
fn kids3d(world: &World, e: Entity, out: &mut Vec<Node3>) {
    let Some(children) = world.get::<Children>(e) else { return };
    for k in children.iter() {
        match read3d(world, k) {
            Some(n) => out.push(n),
            None => kids3d(world, k, out),
        }
    }
}

/// A sound to generate (<Audio src={lyria("…", {seconds})} /> or sao): made once through rdb's store (cached by its own
/// index, counted in $RAEA_GEN_LEDGER against the run's cap), then played as the file it made. False when it couldn't
/// be made (said once): the element is left out. The spec always said the renderer generates these; until 2026-10-17
/// nothing did, and every generated sound was silent.
fn generated(props: &mut Map<String, Value>, cx: &mut Ctx) -> bool {
    let (Some(model), Some(prompt)) = (props.get("data-generate").and_then(|v| v.as_str()).map(str::to_string), props.get("data-prompt").and_then(|v| v.as_str()).map(str::to_string)) else { return true };
    let seconds = num(props, "data-seconds").unwrap_or(4.0);
    let key = format!("{model}|{prompt}|{seconds}");
    if !cx.made.contains_key(&key) {
        let store = rdb::assets::Store::beside(&cx.dir.join("edit.tsx"));
        let got = store.make(&model, &prompt, 1, &json!({"seconds": seconds})).map(|a| store.path(&a));
        if let Err(e) = &got {
            eprintln!("raea-motion: {model} \"{prompt}\" wasn't generated, so it's silent: {e}");
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
        let file = media_path(cx.dir, src);
        // a sound track with samples: a short window says so (decoding the whole take for it held a long take whole)
        let audible = *cx.audible.entry(file.clone()).or_insert_with(|| raea_media::decode_audio_window(&file, 0.0, 0.05).is_ok_and(|a| a.len() > 0));
        if !muted && volume > 0.0 && audible {
            cx.sounds.push(Sound { src: file.display().to_string(), at, volume, rate: num(props, "data-playback-rate").unwrap_or(1.0), key: skey.to_string(), fade_in: num(props, "data-fade-in"), fade_out: num(props, "data-fade-out"), bus: props.get("data-bus").and_then(|v| v.as_str()).map(str::to_string) });
        }
    }
}

/// One frame from the retained page (phase 1 of One renderer): React's commits already patched the mirror
/// (DomMirror, page mode), so this only collects the frame's sound, mix and marker, points each video's image at this
/// frame's still, and paints. Blitz restyles and lays out only what the commits changed. None when the frame holds a
/// 3D scene, which the rebuilding `snapshot` still draws.
fn snapshot_retained(world: &mut World, w: u32, h: u32, cx: &mut Ctx) -> R<Option<Frame>> {
    // a game's fed render: this frame's 3D, just stepped, set on its engine (opened at the first frame with any)
    if let Some(ls) = cx.live3d.as_mut() {
        let fps = world.get_non_send_resource::<crate::DomMirror>().map_or(30.0, |m| m.fps);
        live_scene(world, ls, &mut cx.scene3d, (w, h), fps, cx.dir)?;
    }
    // a layered Canvas3D's slot: the 3D drawn there in the page's paint order, the holders left as they are
    let slot = if cx.scene3d.is_some() { scene_slot(world) } else { None };
    // the elements holding a fed render's 3D: shown through, their backgrounds gone (as Live::prepare)
    let holders: Vec<(Option<u64>, Map<String, Value>)> = if cx.scene3d.is_some() && slot.is_none() {
        holders_of(world).into_iter().map(|(e, p)| (world.get::<crate::ReactNode>(e).map(|n| n.0), p)).collect()
    } else {
        Vec::new()
    };
    cx.sounds.clear();
    cx.scene.clear();
    cx.holders.clear();
    cx.marker = None;
    cx.mix = None;
    let mut q = world.query::<(&ReactKind, Option<&ReactProps>, Option<&crate::ReactNode>)>();
    let items: Vec<(String, Map<String, Value>, Option<u64>)> = q.iter(world).map(|(k, p, n)| (k.0.clone(), p.map(|p| p.0.clone()).unwrap_or_default(), n.map(|n| n.0))).collect();
    let mut videos: Vec<(u64, String, f64, PathBuf)> = Vec::new();
    for (kind, mut props, node) in items {
        if props.contains_key("data-3d") {
            if cx.scene3d.is_none() && cx.live3d.is_none() {
                return Ok(None);
            }
            continue;
        }
        if let Some(f) = props.get("data-raea-frame").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))) {
            cx.marker = Some(f as i64);
            continue;
        }
        if props.get("data-mix").and_then(|v| v.as_str()) == Some("true") {
            cx.mix = Some((num(&props, "data-lufs").unwrap_or(-14.0), num(&props, "data-ceiling").unwrap_or(-1.5)));
            continue;
        }
        if kind != "audio" && kind != "video" {
            continue;
        }
        if !generated(&mut props, cx) {
            continue;
        }
        let skey = node.map(|n| format!("n{n}")).unwrap_or_default();
        push_sound(cx, &props, &skey);
        if kind == "video" {
            let src = props.get("src").and_then(|v| v.as_str()).ok_or("a <Video> without src")?.to_string();
            let at = num(&props, "data-source-time").unwrap_or(0.0);
            let file = media_path(cx.dir, &src);
            // on the GPU path the frame is a texture (placed below), not a still
            let png = if cx.textures { file.clone() } else { still(cx, &file, at)? };
            if let Some(n) = node {
                videos.push((n, src, at, png));
            }
        }
    }
    let manifest = cx.manifest.is_some();
    let now = cx.now;
    let mut m = world.non_send_resource_mut::<crate::DomMirror>();
    m.sync_clips();
    // only what changed: a video whose still is the one it shows already (a held or frozen frame) is left alone; a
    // page with a CSS animation or transition running changes on its own
    let mut changed = m.dirty || !cx.painted || cx.scene3d.is_some() || m.dom.is_animating();
    for (n, src, at, png) in &videos {
        if cx.textures {
            if cx.vframes.place(&mut m, *n, png, *at)? {
                changed = true;
            }
            if manifest {
                if let Some(node) = m.node(*n) {
                    m.dom.set_attribute(node, "data-raea-src", src);
                    m.dom.set_attribute(node, "data-raea-at", &format!("{at:.3}"));
                }
            }
            continue;
        }
        if cx.shown.get(n) == Some(png) {
            continue;
        }
        changed = true;
        cx.shown.insert(*n, png.clone());
        if let Some(node) = m.node(*n) {
            // clear first: the vendored Blitz unloads an image's picture when its src is cleared, so last frame's
            // still goes (setting the new src alone kept every frame's decoded still in the page: 70 MB a frame on a
            // 16:9 take, killed at 24 GB)
            m.dom.clear_attribute(node, "src");
            m.dom.set_attribute(node, "src", &format!("file://{}", png.display()));
            if manifest {
                m.dom.set_attribute(node, "data-raea-src", src);
                m.dom.set_attribute(node, "data-raea-at", &format!("{at:.3}"));
            }
        }
    }
    // fonts only when a commit could have brought a new one (a stylesheet, an inline font), not on every change
    if m.fonts_dirty {
        let _ = m.dom.load_fonts();
        m.fonts_dirty = false;
    }
    let entry = manifest.then(|| manifest_frame(&mut m.dom, now));
    // nothing changed since the frame before: its page again, unpainted (settle_page copies it)
    if !changed {
        drop(m);
        if let Some(e) = entry {
            cx.manifest.as_mut().unwrap().push(e);
        }
        return Ok(Some(Frame { mix: cx.mix, page: None, sounds: cx.sounds.clone(), scene: Vec::new(), clear: None, pending: None, same: true }));
    }
    // a fed render: painted into the page buffer, nothing read back (rdb composites it on the same device)
    if let Some(buf) = cx.fed.as_ref() {
        match cx.scene3d.as_mut() {
            Some(e) => {
                for (id, mut p) in holders {
                    let Some(node) = id.and_then(|i| m.node(i)) else { continue };
                    if see_through(&mut p).0 {
                        crate::apply_props(&mut m.dom, node, &p);
                    }
                }
                let logging = frame_log_on();
                let t = std::time::Instant::now();
                e.draw(now);
                if let Some(err) = e.take_error() {
                    eprintln!("raea-motion: frame {now}: the 3D: {err}");
                }
                if logging {
                    frame_log_add(1, t);
                    frame_log_gpu_wait(2);
                }
                let t = std::time::Instant::now();
                let view = e.texture().ok_or("the 3D drew no target")?;
                raea::quads::set_texture(FED_SCENE_TEXTURE, view, w, h)?;
                match slot.and_then(|s| m.node(s)) {
                    // layered: the 3D (transparent where it has nothing) is the slot's picture, in the page's paint
                    // order; one paint, so a frame costs what it did
                    Some(node) => {
                        set_scene_slot(&mut m.dom, node, FED_SCENE_TEXTURE, w, h);
                        m.dom.paint_buffer(true, None, buf)?;
                    }
                    None => m.dom.paint_buffer(true, Some(FED_SCENE_TEXTURE), buf)?,
                }
                if logging {
                    frame_log_add(3, t);
                    frame_log_gpu_wait(4);
                }
            }
            None => m.dom.paint_transparent_buffer(buf)?,
        }
        m.dirty = false;
        cx.painted = true;
        drop(m);
        if let Some(e) = entry {
            cx.manifest.as_mut().unwrap().push(e);
        }
        return Ok(Some(Frame { mix: cx.mix, page: None, sounds: cx.sounds.clone(), scene: Vec::new(), clear: None, pending: None, same: false }));
    }
    // painted on the GPU while the next frame's React and layout run; the render loop collects it (settle_page)
    let painting = m.dom.paint_transparent_submit();
    m.dirty = false;
    cx.painted = true;
    drop(m);
    if let Some(e) = entry {
        cx.manifest.as_mut().unwrap().push(e);
    }
    let _ = (w, h);
    Ok(Some(Frame { mix: cx.mix, page: None, sounds: cx.sounds.clone(), scene: Vec::new(), clear: None, pending: Some(painting), same: false }))
}

fn build(world: &World, e: Entity, dom: &mut raea::dom::Dom, parent: raea::dom::NodeId, path: &str, holders: &mut Vec<(raea::dom::NodeId, Map<String, Value>)>, cx: &mut Ctx) -> R<()> {
    let kind = world.get::<ReactKind>(e).map(|k| k.0.as_str()).unwrap_or("");
    let mut props = world.get::<ReactProps>(e).map(|p| p.0.clone()).unwrap_or_default();
    if !generated(&mut props, cx) {
        return Ok(());
    }
    if kind == "#text" || kind == "#span" {
        let t = world.get::<ReactText>(e).map(|t| t.0.as_str()).unwrap_or("");
        let n = dom.create_text(t);
        dom.append(parent, n);
        return Ok(());
    }
    if let Some(f) = props.get("data-raea-frame").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))) {
        cx.marker = Some(f as i64);
        return Ok(());
    }
    // <Mix lufs ceiling>: the master, not a picture
    if props.get("data-mix").and_then(|v| v.as_str()) == Some("true") {
        cx.mix = Some((num(&props, "data-lufs").unwrap_or(-14.0), num(&props, "data-ceiling").unwrap_or(-1.5)));
        return Ok(());
    }
    // 3D: rows for Bevy, not DOM; the page's elements holding the scene show it through
    if props.contains_key("data-3d") {
        if let Some(n) = read3d(world, e) {
            cx.scene.push(n);
        }
        if cx.holders.is_empty() {
            cx.holders = holders.clone();
        }
        return Ok(());
    }
    // a sound's identity is its React element (the fiber's node), not its place in the tree: a sound put in earlier
    // shifts the paths of the ones after it, which split a later sound into extra rows
    let skey = world.get::<crate::ReactNode>(e).map(|n| format!("n{}", n.0)).unwrap_or_else(|| path.to_string());
    let sound = |cx: &mut Ctx, props: &Map<String, Value>| push_sound(cx, props, &skey);
    if kind == "audio" {
        sound(cx, &props);
        return Ok(());
    }
    let node = if kind == "video" {
        sound(cx, &props);
        let src = props.get("src").and_then(|v| v.as_str()).ok_or("a <Video> without src")?;
        let at = num(&props, "data-source-time").unwrap_or(0.0);
        let file = media_path(cx.dir, src);
        let png = still(cx, &file, at)?;
        let n = dom.create_element("img");
        let mut p = props.clone();
        p.retain(|k, _| !k.starts_with("data-") && k != "src");
        crate::apply_props(dom, n, &p);
        dom.set_attribute(n, "src", &format!("file://{}", png.display()));
        if cx.manifest.is_some() {
            dom.set_attribute(n, "data-raea-src", src);
            dom.set_attribute(n, "data-raea-at", &format!("{at:.3}"));
        }
        n
    } else {
        let tag = if kind.is_empty() { "div" } else { kind };
        let n = dom.create_element(tag);
        let mut p = props.clone();
        // an <img> from the edit's folder or staticFile
        if tag == "img" {
            if let Some(src) = p.get("src").and_then(|v| v.as_str()).map(str::to_string) {
                if !src.contains("://") {
                    p.insert("src".into(), Value::String(format!("file://{}", media_path(cx.dir, &src).display())));
                }
            }
        }
        crate::apply_props(dom, n, &p);
        if let Some(t) = world.get::<ReactText>(e) {
            let tn = dom.create_text(&t.0);
            dom.append(n, tn);
        }
        n
    };
    dom.append(parent, node);
    holders.push((node, props));
    if let Some(ch) = world.get::<Children>(e) {
        for (i, c) in ch.iter().enumerate() {
            build(world, c, dom, node, &format!("{path}.{i}"), holders, cx)?;
        }
    }
    holders.pop();
    Ok(())
}

/// The video's frame at `at` seconds, decoded natively, as a PNG (one per source frame).
/// The page's videos as frames on the GPU (the GPU paint path, One renderer phase 3): each <Video>'s source decoded at
/// its time into an NV12 pixel buffer, wrapped as textures on the paint device and drawn by its <img
/// data-raea-texture>, so no frame becomes a PNG or touches the CPU. RAEA_VIDEO_TEXTURES=0, or the CPU paint, keeps
/// the stills.
pub(crate) struct VideoFrames {
    decoders: HashMap<PathBuf, Box<dyn raea_media::VideoDecoder>>,
    shown: HashMap<u64, (PathBuf, i64)>,
    /// decoders being opened and seeked on workers (prefetch), taken by the frame that first shows their file
    #[allow(clippy::type_complexity)]
    pending: HashMap<PathBuf, std::sync::mpsc::Receiver<R<Box<dyn raea_media::VideoDecoder + Send>>>>,
}

impl VideoFrames {
    pub(crate) fn new() -> VideoFrames {
        VideoFrames { decoders: HashMap::new(), shown: HashMap::new(), pending: HashMap::new() }
    }

    /// `file` about to show at `at` seconds (a baked timeline sees its cuts coming): its decoder opened and seeked there
    /// on a worker now, so the frame that cuts to it doesn't wait for the open and the seek (a 4K cut took 115-120 ms,
    /// three frames at 25 fps). Nothing when it's open or on its way already.
    #[cfg(target_vendor = "apple")]
    pub(crate) fn prefetch(&mut self, file: &Path, at: f64) {
        if self.decoders.contains_key(file) || self.pending.contains_key(file) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let path = file.to_path_buf();
        let nv12 = std::env::var("RAEA_VIDEO_NV12").as_deref() != Ok("0");
        let spawned = std::thread::Builder::new().name("raea video prefetch".into()).spawn(move || {
            // asked twice, as the frame that shows it will: the first is a seek (VideoToolbox directly), the second starts
            // the reader there for the play that follows; with only the seek done, that frame started the reader itself
            // (130 ms on the main thread at a 4K cut)
            let r = raea_media::apple::open_video_send(&path).map(|mut d| {
                for _ in 0..2 {
                    let _ = if nv12 { d.frame_surface_at(at).map(|_| ()) } else { d.frame_surface_rgb_at(at).map(|_| ()) };
                }
                d
            });
            let _ = tx.send(r);
        });
        if spawned.is_ok() {
            if std::env::var_os("RAEA_PREFETCH_TRACE").is_some() {
                eprintln!("raea video: prefetching {} at {at:.2} s", file.display());
            }
            self.pending.insert(file.to_path_buf(), rx);
        }
    }

    #[cfg(not(target_vendor = "apple"))]
    pub(crate) fn prefetch(&mut self, _file: &Path, _at: f64) {}

    /// The page was rebuilt (a baked seek's keyframe): every video element is new, so none shows its texture yet. The
    /// cache of what each node shows went stale, and a node shown before the seek came back with no picture.
    pub(crate) fn forget_shown(&mut self) {
        self.shown.clear();
    }

    /// Whether this process paints videos as textures.
    pub(crate) fn on() -> bool {
        cfg!(target_vendor = "apple") && raea::quads::enabled() && std::env::var("RAEA_VIDEO_TEXTURES").as_deref() != Ok("0")
    }

    /// Video `node` showing `file` at `at` seconds, on its element; true when its frame changed.
    #[cfg(target_vendor = "apple")]
    pub(crate) fn place(&mut self, m: &mut crate::DomMirror, node: u64, file: &Path, at: f64) -> R<bool> {
        if !self.decoders.contains_key(file) {
            // prefetched: the worker's decoder (waited for if it isn't done: the open is under way already)
            let t = std::time::Instant::now();
            let ready = self.pending.remove(file).and_then(|rx| rx.recv().ok()).and_then(|r| r.ok());
            if std::env::var_os("RAEA_PREFETCH_TRACE").is_some() {
                eprintln!("raea video: {} {} ({:.1} ms waited)", file.display(), if ready.is_some() { "taken prefetched" } else { "opened here" }, t.elapsed().as_secs_f64() * 1e3);
            }
            let d = match ready {
                Some(d) => d as Box<dyn raea_media::VideoDecoder>,
                None => raea_media::backend("apple")?.open_video(file).map_err(|e| format!("{}: {e}", file.display()))?,
            };
            self.decoders.insert(file.to_path_buf(), d);
        }
        let d = self.decoders.get_mut(file).unwrap();
        // the decoder's NV12 planes, sampled and converted by the compositor (BT.709), for renders and play alike: no
        // colour conversion in VideoToolbox, so a 4K seek is ~5 ms quicker (scrub p99 55 -> 49.6 ms) and a frame is one
        // texture pair from decoder to page. RAEA_VIDEO_NV12=0 takes VideoToolbox's BGRA instead (the pictures stills had;
        // SSIM 0.998 from these). The decoder keeps its current frame, so asking again within a frame costs nothing; the
        // frame's own time says whether it changed
        let sf = if std::env::var("RAEA_VIDEO_NV12").as_deref() != Ok("0") { d.frame_surface_at(at)? } else { d.frame_surface_rgb_at(at)? };
        let sf = sf.ok_or_else(|| format!("{}: the decoder gave no surface", file.display()))?;
        let key = (sf.pts * 1e6).round() as i64;
        if self.shown.get(&node).is_some_and(|(f, k)| f == file && *k == key) {
            return Ok(false);
        }
        let (w, h) = (sf.width, sf.height);
        raea::quads::set_video(node, sf.buffer, w, h)?;
        if let Some(el) = m.node(node) {
            if !self.shown.contains_key(&node) {
                m.dom.clear_attribute(el, "src");
                m.dom.set_attribute(el, "data-raea-texture", &node.to_string());
            }
            m.dom.set_attribute(el, "data-raea-w", &w.to_string());
            m.dom.set_attribute(el, "data-raea-h", &h.to_string());
        }
        self.shown.insert(node, (file.to_path_buf(), key));
        Ok(true)
    }

    #[cfg(not(target_vendor = "apple"))]
    pub(crate) fn place(&mut self, _m: &mut crate::DomMirror, _node: u64, _file: &Path, _at: f64) -> R<bool> {
        Err("video textures need the Apple decoders".into())
    }
}

/// Every schedule of `app` on the single-threaded executor: its systems hold the page (DomMirror, not Send) and run on
/// this thread anyway, and the multi-threaded one spawned a task per system a frame (ZeroHeap; runner.rs does the same)
pub(crate) fn one_thread(app: &mut App) {
    let mut schedules = app.world_mut().resource_mut::<Schedules>();
    for (_, s) in schedules.iter_mut() {
        s.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    }
}

/// The page's elements with their kind, props and node, as a frame reads them: one query kept in the world (ZeroHeap:
/// `world.query` made a QueryState each call, twice a live frame)
#[derive(Resource)]
pub(crate) struct KindQuery(pub QueryState<(&'static ReactKind, Option<&'static ReactProps>, Option<&'static crate::ReactNode>)>);

/// `f` with the world and its kept KindQuery
pub(crate) fn with_kinds<T>(world: &mut World, f: impl FnOnce(&mut World, &mut KindQuery) -> T) -> T {
    if !world.contains_resource::<KindQuery>() {
        let q = world.query();
        world.insert_resource(KindQuery(q));
    }
    world.resource_scope(|world, mut q: Mut<KindQuery>| f(world, &mut q))
}

/// The page's videos placed for this frame (live play): each <Video>'s frame at its source time. True when any changed.
pub(crate) fn place_videos(world: &mut World, dir: &Path, vf: &mut VideoFrames) -> R<bool> {
    let vids: Vec<(u64, PathBuf, f64)> = with_kinds(world, |world, q| q.0
        .iter(world)
        .filter(|(k, _, _)| k.0 == "video")
        .filter_map(|(_, p, n)| {
            let p = p?;
            let src = p.0.get("src")?.as_str()?;
            Some((n?.0, media_path(dir, src), num(&p.0, "data-source-time").unwrap_or(0.0)))
        })
        .collect());
    let mut m = world.non_send_resource_mut::<crate::DomMirror>();
    let mut changed = false;
    for (n, file, at) in vids {
        changed |= vf.place(&mut m, n, &file, at)?;
    }
    if changed {
        m.dirty = true;
    }
    Ok(changed)
}

fn still(cx: &mut Ctx, file: &Path, at: f64) -> R<PathBuf> {
    if !cx.decoders.contains_key(file) {
        let be = raea_media::backend(&std::env::var("RAEA_MEDIA").unwrap_or_else(|_| if cfg!(target_vendor = "apple") { "apple".into() } else { "soft".into() }))?;
        let d = be.open_video(file).map_err(|e| format!("{}: {e}", file.display()))?;
        cx.decoders.insert(file.to_path_buf(), d);
    }
    let d = cx.decoders.get_mut(file).unwrap();
    let key = d.index_at(at).map(|i| i as i64).unwrap_or((at * 1e6) as i64 + (1 << 40));
    if let Some(p) = cx.stills.get(&(file.to_path_buf(), key)) {
        return Ok(p.clone());
    }
    let f = d.frame_at(at)?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (file, key).hash(&mut h);
    let p = cx.work.join("frames").join(format!("{:016x}.png", h.finish()));
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    fast_png(&p, &f.rgba, f.width, f.height)?;
    cx.stills.insert((file.to_path_buf(), key), p.clone());
    Ok(p)
}

/// The surface's tree as a frame.
/// One frame of render.json: the videos (source, seconds into it, box) and the text (its words, box, and its colour,
/// font size in px, weight and alignment, as computed) as Blitz laid them out, each with its effective opacity; hidden ones
/// (display none, visibility hidden, opacity 0) left out. Boxes are [x, y, w, h] in composition px. Checks read this
/// instead of an edit's rows (docs: TypeScript-only editing, phase 4); the legibility lint reads the colour and size.
fn manifest_frame(dom: &mut raea::dom::Dom, f: i64) -> Value {
    let laid = dom.laid_out(&["opacity", "visibility", "display", "color", "font-size", "font-weight", "text-align"]);
    let alpha = |i: usize| -> f64 {
        let (mut a, mut at) = (1.0, Some(i));
        while let Some(k) = at {
            let l = &laid[k];
            if l.css.get("display").is_some_and(|d| d == "none") || l.css.get("visibility").is_some_and(|v| v == "hidden") {
                return 0.0;
            }
            a *= l.css.get("opacity").and_then(|o| o.parse::<f64>().ok()).unwrap_or(1.0);
            at = l.parent;
        }
        a
    };
    let r = |v: f32| (v as f64 * 10.0).round() / 10.0;
    let (mut videos, mut text) = (Vec::new(), Vec::new());
    // an inline element (a <span> around a caption word) has no box of its own: its words belong to the nearest
    // ancestor that has one, in document order (laid_out lists parents first). Skipping them hid every caption and
    // lockup word from the checks
    let mut owned: Vec<(usize, Vec<String>)> = Vec::new();
    for (i, l) in laid.iter().enumerate() {
        if l.tag == "style" || l.tag == "script" {
            continue;
        }
        let a = alpha(i);
        if a <= 0.01 {
            continue;
        }
        let attr = |k: &str| l.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        if let Some(src) = attr("data-raea-src") {
            if l.w > 0.0 && l.h > 0.0 {
                videos.push(json!({"src": src, "at": attr("data-raea-at").and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0), "box": [r(l.x), r(l.y), r(l.w), r(l.h)], "opacity": (a * 100.0).round() / 100.0}));
            }
            continue;
        }
        let words: Vec<String> = l.kids.iter().filter_map(|k| k.as_ref().err()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        if words.is_empty() {
            continue;
        }
        let mut at = Some(i);
        while let Some(k) = at {
            if laid[k].w > 0.0 && laid[k].h > 0.0 {
                break;
            }
            at = laid[k].parent;
        }
        let Some(k) = at else { continue };
        match owned.iter_mut().find(|(o, _)| *o == k) {
            Some((_, ws)) => ws.extend(words),
            None => owned.push((k, words)),
        }
    }
    for (k, words) in owned {
        let l = &laid[k];
        let css = |k: &str| l.css.get(k).cloned().unwrap_or_default();
        let size = css("font-size").trim_end_matches("px").parse::<f64>().unwrap_or(0.0);
        text.push(json!({"text": words.join(" "), "box": [r(l.x), r(l.y), r(l.w), r(l.h)], "opacity": (alpha(k) * 100.0).round() / 100.0,
            "color": css("color"), "size": size, "weight": css("font-weight").parse::<f64>().unwrap_or(400.0), "align": css("text-align")}));
    }
    json!({"frame": f, "videos": videos, "text": text})
}

fn snapshot(world: &mut World, w: u32, h: u32, cx: &mut Ctx) -> R<Frame> {
    let mut q = world.query::<(Entity, &ReactKind, Option<&ReactProps>)>();
    let surf = q.iter(world).find(|(_, k, p)| k.0 == "surface" && p.and_then(|p| p.0.get("id")).and_then(|v| v.as_str()) == Some("motion")).map(|(e, _, _)| e).ok_or("the composition's surface isn't there")?;
    let kids: Vec<Entity> = world.get::<Children>(surf).map(|c| c.iter().collect()).unwrap_or_default();
    // one page for the whole render: fonts and the brand kit load once (a page per frame re-parsed every font file,
    // 13 ms a frame), and each frame's tree goes in a fresh root the size of the page, the last frame's removed
    let mut dom = match cx.dom.take() {
        Some(d) => d,
        None => {
            let mut d = raea::dom::Dom::new(w, h, cx.dir);
            let body = d.body();
            // the page's own defaults: no margin, and Inter (bundled) where a part names no face, not the UA's serif
            d.set_attribute(body, "style", "margin: 0; font-family: Inter, sans-serif");
            // RAEA_MOTION_TEXT=hidden: every glyph drawn transparent (shadows, plates and pictures stay), the frame as it
            // would be without its words, for the legibility lint (sdk/motion/bench/legible) to measure what's behind them
            if std::env::var("RAEA_MOTION_TEXT").as_deref() == Ok("hidden") {
                let st = d.create_element("style");
                let t = d.create_text("* { color: transparent !important; -webkit-text-fill-color: transparent !important; caret-color: transparent !important }");
                d.append(st, t);
                d.append(body, st);
            }
            // the space's brand kit (brand/brand.css), as the component tool and the SDK backend give it:
            // var(--brand-*) and --brand-scale work in any part
            if let Some(css) = &cx.brand {
                let st = d.create_element("style");
                let t = d.create_text(css);
                d.append(st, t);
                d.append(body, st);
            }
            d
        }
    };
    if let Some(prev) = cx.root.take() {
        dom.remove(prev);
    }
    let page_body = dom.body();
    let body = dom.create_element("div");
    dom.set_attribute(body, "style", &format!("position: absolute; left: 0; top: 0; width: {w}px; height: {h}px"));
    dom.append(page_body, body);
    cx.root = Some(body);
    cx.sounds.clear();
    cx.scene.clear();
    cx.holders.clear();
    cx.marker = None;
    cx.mix = None;
    let mut holders = Vec::new();
    for (i, k) in kids.iter().enumerate() {
        build(world, *k, &mut dom, body, &i.to_string(), &mut holders, cx)?;
    }
    // the elements holding a 3D scene: their backgrounds go (the scene is drawn under the page), the nearest one's
    // colour is the scene's clear colour
    cx.clear = None;
    for (node, props) in cx.holders.clone().iter().rev() {
        let mut p = props.clone();
        let (_, clear) = see_through(&mut p);
        if cx.clear.is_none() {
            cx.clear = clear;
        }
        crate::apply_props(&mut dom, *node, &p);
    }
    let _ = dom.load_fonts();
    if cx.manifest.is_some() {
        let entry = manifest_frame(&mut dom, cx.now);
        cx.manifest.as_mut().unwrap().push(entry);
    }
    let mut pixels = Vec::new();
    dom.paint_transparent(&mut pixels);
    let page = store_page(&pixels, w, h, cx.work)?;
    cx.dom = Some(dom);
    Ok(Frame { mix: cx.mix, page, sounds: cx.sounds.clone(), scene: cx.scene.clone(), clear: cx.clear.clone(), pending: None, same: false })
}

/// The entry a render's bundle is made from: the composition (a registered <Composition>, or the module's default export with the CLI's
/// settings) inside a Clock whose frame Rust sets through the "raea-frame" event; a hidden marker says which frame
/// the commit is.
fn entry(file: &str, o: &CompOpts) -> String {
    let (comp, w, h, fps, frames) = (o.composition.as_deref(), o.w, o.h, o.fps, o.frames);
    let js = |v: &Option<Value>| v.as_ref().map(Value::to_string).unwrap_or("null".into());
    let (words, perception, input) = (js(&o.words), js(&o.perception), js(&o.input));
    format!(
        r#"import {{ mount, addEventListener }} from "bevy-react";
import {{ Component, useEffect, useState }} from "react";
import * as M from "./src/{file}";
import {{ Clock, getCompositions }} from "./motion";
const want = {comp};
const comps = getCompositions();
const reg = comps.find((c) => want === null || c.id === want) ?? null;
if (want !== null && !reg) throw new Error(`no composition "${{want}}" (registered: ${{comps.map((c) => c.id).join(", ") || "none"}})`);
const C: any = reg ? reg.component : ((M as any).default ?? Object.values(M).find((v) => typeof v === "function"));
if (!C) throw new Error("{file} exports no component and registers no composition");
// an edit with no <Composition> may state its size and length as `export const config = {{width, height, fps,
// durationInFrames}}` (score's SDK backend writes it); its positive numbers win over the CLI's settings
const MC: any = (M as any).config ?? {{}};
const pos = (v: any, d: number) => (typeof v === "number" && v > 0 ? v : d);
const cfg = reg ?? {{ id: "main", width: pos(MC.width, {w}), height: pos(MC.height, {h}), fps: pos(MC.fps, {fps}), durationInFrames: pos(MC.durationInFrames, {frames}), defaultProps: {{}} }};
// the Clock's words (frames {{text, from, to}}, or seconds {{text, start, end}} / {{text, start_ms, end_ms}}), perception
// (one for every frame, or by frame: the latest at or before it) and input (a timeline, or inputs.rrl's step keys)
const RAW_WORDS: any[] | null = {words};
const PERC: any = {perception};
const RAW_INPUT: any = {input};
const sec = (w: any, k: string) => (w[k + "_ms"] != null ? w[k + "_ms"] / 1000 : w[k]);
const WORDS = (RAW_WORDS ?? []).map((w: any) => {{
  if (w.from != null) return {{ text: w.text ?? w.word, from: w.from, to: w.to }};
  const from = Math.round(sec(w, "start") * cfg.fps);
  return {{ text: w.text ?? w.word, from, to: Math.max(from + 1, Math.round(sec(w, "end") * cfg.fps)) }};
}});
const byFrame = PERC && typeof PERC === "object" && Object.keys(PERC).length > 0 && Object.keys(PERC).every((k) => /^\d+$/.test(k));
const percAt = (f: number) => {{
  if (!PERC) return {{}};
  if (!byFrame) return PERC;
  let v = {{}};
  for (const k of Object.keys(PERC).map(Number).sort((a, b) => a - b)) if (k <= f) v = PERC[k];
  return v;
}};
const INPUT: any = (() => {{
  if (!RAW_INPUT) return {{}};
  if (!RAW_INPUT.steps) return RAW_INPUT;
  const keys = Object.entries(RAW_INPUT.steps as Record<string, Record<string, number>>).map(([k, m]) => [k, Object.keys(m).map(Number).sort((a, b) => a - b).map((f) => [f, m[f]])] as const);
  const out: any = {{}};
  for (let f = 0; f < cfg.durationInFrames; f++) {{
    const held = keys.filter(([, st]) => {{ let v = 0; for (const [g, x] of st) if (g <= f) v = x; return v > 0; }}).map(([k]) => k);
    out[f] = {{ keys: held }};
  }}
  return out;
}})();
// a live session's input replayed (render --input of what play or void logged, every entry marked live): rendered as it
// was played, on an endless clock, so frames past the composition's length are the frames the person saw
const REPLAYS_LIVE = Object.values(INPUT).some((e: any) => e && e.live);
// a throw while rendering the composition, caught and put where Rust reads it (settle), with its message (JSC's stack
// leaves it out) and React's component stack, so the render says what went wrong instead of waiting 20 s for a frame that never commits
class Catch extends Component<{{ children?: any }}, {{ err: string | null }}> {{
  state = {{ err: null as string | null }};
  static getDerivedStateFromError(e: any) {{ return {{ err: (e && e.message ? (e.name || "Error") + ": " + e.message + (e.stack ? "\n" + e.stack : "") : String(e)) }}; }}
  componentDidCatch(e: any, info: any) {{ this.setState({{ err: (e && e.message ? (e.name || "Error") + ": " + e.message + (e.stack ? "\n" + e.stack : "") : String(e)) + (info && info.componentStack ? "\ncomponents:" + info.componentStack : "") }}); }}
  render() {{ return this.state.err ? <div data-raea-error={{this.state.err.slice(0, 4000)}} /> : this.props.children; }}
}}
// live play (raea-motion play, void's stage): each frame's event carries the person's input, kept in one timeline that
// grows a frame at a time (the same object each frame, so a game's fold only checks what's new), marked live: true
const LIVE: any = {{}};
let live = false;
function Root() {{
  const [f, setF] = useState(-1);
  // the stage's size, for a composition that lays out from useVideoConfig (raea-size)
  const [size, setSize] = useState<{{ w: number; h: number }} | null>(null);
  useEffect(() => addEventListener("raea-size", (v: any) => setSize(v)), []);
  // the frame is done when its commit and the effects it set off have run: a timer queued after them marks it, and
  // Rust takes the frame the moment that mark lands (settle), with no waiting for quiet
  const [done, setDone] = useState(-1);
  useEffect(() => addEventListener("raea-frame", (v: any) => {{
    if (typeof v === "number") return setF(v);
    live = true;
    // a live game keeps 4096 frames of input (motion games.ts LIVE_KEEP): it only moves forward
    if (v.input) {{ LIVE[v.frame] = {{ ...v.input, live: true }}; delete LIVE[v.frame - 4096]; }}
    setF(v.frame);
  }}), []);
  useEffect(() => {{
    if (f >= 0) setDone(f);
  }}, [f]);
  // a range of frames rendered back to back (raea-frames: the 3D's rows stepped as a bake steps, scene_frames_to):
  // each frame's commit and its done mark as one raea-frame event makes them, the next frame set off by the mark
  const [to, setTo] = useState(-1);
  useEffect(() => addEventListener("raea-frames", (v: any) => {{ setTo(v.to); setF(v.from); }}), []);
  useEffect(() => {{
    if (f >= 0 && done === f && f + 1 < to) setF(f + 1);
  }}, [done]);
  const conf = JSON.stringify({{ width: cfg.width, height: cfg.height, fps: cfg.fps, frames: cfg.durationInFrames }});
  if (f < 0) return <div data-raea-frame={{-1}} data-raea-config={{conf}} />;
  return (
    <>
      <Clock frame={{f}} fps={{cfg.fps}} width={{size ? size.w : cfg.width}} height={{size ? size.h : cfg.height}} durationInFrames={{live || REPLAYS_LIVE ? 1000000000 : cfg.durationInFrames}} id={{cfg.id}} defaultProps={{cfg.defaultProps}} words={{WORDS}} perception={{percAt(f)}} input={{live ? LIVE : INPUT}}>
        <Catch><C {{...cfg.defaultProps}} />{{(() => {{ const T = (globalThis as any).__raeaToneOut; return T ? <T /> : null; }})()}}</Catch>
      </Clock>
      <div data-raea-frame={{f}} data-raea-done={{done}} data-raea-config={{conf}} />
    </>
  );
}}
mount(<surface id="motion" w={{1}} px={{cfg.width}} pxH={{cfg.height}}><Root /></surface>);
"#,
        comp = comp.map(|c| format!("{c:?}")).unwrap_or("null".into())
    )
}

/// The entry of a live game's bundle: the composition rendered by raea's live view runtime (sdk/react/js/lite) instead of
/// React. Each frame's event renders the tree once and sends what changed; the frame is marked done in that same pass
/// (React's entry marked it from an effect, a second commit).
fn entry_lite(file: &str, o: &CompOpts) -> String {
    let full = entry(file, o);
    // the shared preamble (the composition found, the Clock's words, perception and input), up to React's Root
    let start = full.find("import * as M").expect("entry's imports");
    let end = full.find("// a throw while rendering the composition").expect("entry's Root");
    let bridge = rdb::home().join("sdk/react/js/node_modules/bevy-react/src/bridge.ts");
    format!(
        r#"import {{ addEventListener, reset, runEventLoop }} from {bridge:?};
import {{ renderRoot, takeRendered }} from "react";
{preamble}const LIVE: any = {{}};
let live = false;
let f = -1;
let size: {{ w: number; h: number }} | null = null;
const conf = JSON.stringify({{ width: cfg.width, height: cfg.height, fps: cfg.fps, frames: cfg.durationInFrames }});
// the composition's element and the Tone graph's output, made once (each frame made them again: lightspeed Z3)
const COMP = <C {{...cfg.defaultProps}} />;
let toneEl: any = null;
function toneOut() {{
  const T = (globalThis as any).__raeaToneOut;
  if (!T) return null;
  if (!toneEl || toneEl.type !== T) toneEl = <T />;
  return toneEl;
}}
function tree() {{
  if (f < 0) return <surface id="motion" w={{1}} px={{cfg.width}} pxH={{cfg.height}}><div data-raea-frame={{-1}} data-raea-config={{conf}} /></surface>;
  return (
    <surface id="motion" w={{1}} px={{cfg.width}} pxH={{cfg.height}}>
      <Clock frame={{f}} fps={{cfg.fps}} width={{size ? size.w : cfg.width}} height={{size ? size.h : cfg.height}} durationInFrames={{live || REPLAYS_LIVE ? 1000000000 : cfg.durationInFrames}} id={{cfg.id}} defaultProps={{cfg.defaultProps}} words={{WORDS}} perception={{percAt(f)}} input={{live ? LIVE : INPUT}}>
        {{COMP}}{{toneOut()}}
      </Clock>
      {marker}
    </surface>
  );
}}
// --comps (a bake): the marker rendered last, carrying the code of every component this frame rendered
function Marker(p: any) {{
  return <div data-raea-frame={{p.f}} data-raea-done={{p.f}} data-raea-config={{conf}} data-raea-comps={{takeRendered()}} />;
}}
// a throw while rendering is put where Rust reads it (settle), as React's entry's Catch did
function draw() {{
  try {{
    renderRoot(tree());
  }} catch (e: any) {{
    renderRoot(<surface id="motion" w={{1}} px={{cfg.width}} pxH={{cfg.height}}><div data-raea-error={{(e && e.message ? (e.name || "Error") + ": " + e.message + (e.stack ? "\n" + e.stack : "") : String(e)).slice(0, 4000)}} data-raea-frame={{f}} data-raea-done={{f}} data-raea-config={{conf}} /></surface>);
  }}
}}
addEventListener("raea-size", (v: any) => {{ size = v; if (f >= 0) draw(); }});
// a range of frames, rendered back to back, each its own commit (a bake: lightspeed T3); between frames the effects'
// state changes render, as between two raea-frame events
addEventListener("raea-frames", async (v: any) => {{
  for (let g = v.from; g < v.to; g++) {{
    f = g;
    draw();
    await Promise.resolve();
  }}
}});
addEventListener("raea-frame", (v: any) => {{
  if (typeof v === "number") {{ f = v; return draw(); }}
  live = true;
  // a live game keeps 4096 frames of input (motion games.ts LIVE_KEEP): it only moves forward
  if (v.input) {{ LIVE[v.frame] = {{ ...v.input, live: true }}; delete LIVE[v.frame - 4096]; }}
  f = v.frame;
  draw();
}});
// as bevy-react's mount: the bridge reset, the first page, then the events pulled from Bevy for good (each frame's
// render sends its own ops)
reset();
draw();
runEventLoop();
"#,
        preamble = &full[start..end],
        marker = if o.comps { "<Marker f={f} />" } else { "<div data-raea-frame={f} data-raea-done={f} data-raea-config={conf} />" },
    )
}

/// Run React until its commits settle and the marker says `want` (or anything, for None).
fn settle(app: &mut App, want: Option<i64>, secs: f64) -> R<Option<Map<String, Value>>> {
    // no polling for quiet: each pass applies what React flushed, then waits for its next flush. The composition marks
    // a frame done (data-raea-done) once its commit and that commit's effects have run, so the frame is taken the
    // moment the mark lands (it waited for three quiet updates with 1 ms sleeps, ~4 ms a frame)
    let t0 = std::time::Instant::now();
    // time the composition spends waiting on something made on first use (a matchmove solve: minutes) doesn't count
    let making0 = track::making_secs();
    // the count before the first update: a flush is in the channel before it's counted, so one that landed before this
    // read is applied by the update below, and one after it wakes the wait. (A first pass that waited for a flush
    // before updating lost the frame's flush whenever React was quicker than this read: the wait then ran its 0.5 s
    // out, once per launch in void's bake and as 0.5 s hitches in live play; lightspeed F5, F6.)
    let mut seen = crate::flushed();
    // the page styled at the frame's time: its CSS animations and transitions where they are at frame `want`
    if let Some(f) = want {
        if let Some(mut m) = app.world_mut().get_non_send_resource_mut::<crate::DomMirror>() {
            m.at_frame(f);
        }
    }
    // a bake's runtime (OneBatch): only what a commit needs, applied and its animations sampled (PreUpdate). The whole
    // main schedule ran each frame for nothing else, ~50 us a frame, more than React's render (lightspeed T3)
    let pre_only = app.world().get_resource::<crate::OneBatch>().is_some();
    loop {
        if pre_only {
            app.world_mut().run_schedule(PreUpdate);
        } else {
            app.update();
        }
        if let Some(e) = crate::COMPOSITION_ERROR.lock().unwrap().take() {
            return Err(format!("the composition threw at frame {}: {e}", want.map(|w| w.to_string()).unwrap_or("?".into())));
        }
        let world = app.world_mut();
        // the marker's entity, remembered (a scan of every entity's props for it each pass was most of a settle)
        let marker = world.get_resource::<MarkerEntity>().map(|m| m.0).filter(|e| world.get::<ReactProps>(*e).is_some_and(|p| p.0.contains_key("data-raea-frame")));
        let marker = match marker {
            Some(e) => Some(e),
            None => {
                let mut q = world.query::<(Entity, &ReactProps)>();
                if let Some(e) = q.iter(world).find_map(|(_, p)| p.0.get("data-raea-error").and_then(|v| v.as_str()).map(str::to_string)) {
                    return Err(format!("the composition threw at frame {}: {e}", want.map(|w| w.to_string()).unwrap_or("?".into())));
                }
                let found = q.iter(world).find(|(_, p)| p.0.contains_key("data-raea-frame")).map(|(e, _)| e);
                if let Some(e) = found {
                    world.insert_resource(MarkerEntity(e));
                }
                found
            }
        };
        if let Some(m) = marker.and_then(|e| world.get::<ReactProps>(e)) {
            if let Some(e) = m.0.get("data-raea-error").and_then(|v| v.as_str()) {
                return Err(format!("the composition threw at frame {}: {e}", want.map(|w| w.to_string()).unwrap_or("?".into())));
            }
            let at = num(&m.0, "data-raea-frame").unwrap_or(-2.0) as i64;
            let done = num(&m.0, "data-raea-done").unwrap_or(-2.0) as i64;
            let ok = match want {
                // the settings: the first marker, before any frame
                None => true,
                Some(w) if w < 0 => at == w,
                Some(w) => at == w && done == w,
            };
            if ok {
                // the settings, read for the settings' settle only (a frame's settle returns None: every caller of one
                // ignores it, and parsing them each frame was most of a settle's allocations; ZeroHeap)
                if want.is_some_and(|w| w >= 0) {
                    return Ok(None);
                }
                let conf = m.0.get("data-raea-config").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str(s).ok());
                return Ok(conf);
            }
        }
        let left = secs + (track::making_secs() - making0) - t0.elapsed().as_secs_f64();
        if left <= 0.0 {
            break;
        }
        // commits held to be applied one an update (OneBatch): the next is applied without waiting
        if app.world().get_resource::<crate::OneBatch>().is_some_and(|q| !q.0.is_empty()) {
            continue;
        }
        seen = crate::wait_flush(seen, std::time::Duration::from_secs_f64(left.min(0.5)));
    }
    Err(format!("the composition didn't commit frame {want:?} in {secs} s (does it throw? see the [js] log)"))
}

/// The entity carrying the frame marker (data-raea-frame), once found.
#[derive(Resource)]
struct MarkerEntity(Entity);

fn hex_of(c: &str) -> Option<String> {
    hex_cow(c).map(std::borrow::Cow::into_owned)
}

/// `hex_of`, borrowing where the colour is already hex or named (ZeroHeap: a live 3D frame read each mesh's colour)
fn hex_cow(c: &str) -> Option<std::borrow::Cow<'_, str>> {
    use std::borrow::Cow;
    let c = c.trim();
    // #rrggbbaa too: a layered Canvas3D's transparent clear (scene_clear)
    if c.starts_with('#') && (c.len() == 7 || c.len() == 4 || c.len() == 9) {
        return Some(Cow::Borrowed(c));
    }
    let named = [("black", "#000000"), ("white", "#ffffff"), ("red", "#ff0000"), ("transparent", "#000000")];
    named.iter().find(|(n, _)| *n == c).map(|(_, h)| Cow::Borrowed(*h)).or_else(|| {
        let inner = c.strip_prefix("rgb(").or_else(|| c.strip_prefix("rgba("))?.trim_end_matches(')');
        let v: Vec<u8> = inner.split(',').take(3).filter_map(|x| x.trim().parse::<f64>().ok()).map(|x| x.round().clamp(0.0, 255.0) as u8).collect();
        (v.len() == 3).then(|| Cow::Owned(format!("#{:02x}{:02x}{:02x}", v[0], v[1], v[2])))
    })
}

fn vec3(p: &Map<String, Value>, k: &str, d: [f64; 3]) -> [f64; 3] {
    p.get(k).and_then(|v| v.as_array()).filter(|a| a.len() == 3).map(|a| [a[0].as_f64().unwrap_or(d[0]), a[1].as_f64().unwrap_or(d[1]), a[2].as_f64().unwrap_or(d[2])]).unwrap_or(d)
}

/// A frame's page written once as an asset (assets/HASH.png, straight alpha), its hash; None when nothing is painted.
/// Pages being written on workers, and the paths already started (a repeated page is written once).
static WRITERS: std::sync::Mutex<Vec<std::thread::JoinHandle<R<()>>>> = std::sync::Mutex::new(Vec::new());
static PENDING: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> = std::sync::LazyLock::new(Default::default);

/// Every page written: before rdb (or the -o OUT.sql export) reads them.
fn pages_written() -> R<()> {
    let jobs: Vec<_> = std::mem::take(&mut *WRITERS.lock().unwrap());
    for j in jobs {
        j.join().map_err(|_| "a page writer panicked".to_string())??;
    }
    Ok(())
}

/// A PNG written for the engine to read straight back (video stills, pages): no filtering, the fastest deflate. The
/// default level spent 12 ms a page and as long again on every video still, for files that live a few seconds.
fn fast_png(path: &Path, rgba: &[u8], w: u32, h: u32) -> R<()> {
    use image::ImageEncoder;
    let f = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?);
    image::codecs::png::PngEncoder::new_with_quality(f, image::codecs::png::CompressionType::Fast, image::codecs::png::FilterType::NoFilter)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())
}

/// A frame's page collected from its paint and written as an asset (once per distinct page).
fn settle_page(frames: &mut [(i64, Frame)], k: usize, w: u32, h: u32, work: &Path) -> R<()> {
    if frames[k].1.same {
        frames[k].1.page = k.checked_sub(1).and_then(|j| frames[j].1.page.clone());
        if std::env::var_os("RAEA_MOTION_KEEP").is_some() {
            if let Some(hash) = &frames[k].1.page {
                use std::io::Write as _;
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(work.join("pages.txt")) {
                    let _ = writeln!(f, "{hash}");
                }
            }
        }
        return Ok(());
    }
    if let Some(p) = frames[k].1.pending.take() {
        let px = p.finish()?;
        frames[k].1.page = store_page(&px, w, h, work)?;
    }
    Ok(())
}

fn store_page(page: &[u8], w: u32, h: u32, work: &Path) -> R<Option<String>> {
    if page.iter().skip(3).step_by(4).all(|a| *a == 0) {
        return Ok(None);
    }
    // xxHash3 (a page is 8 MB: SipHash spent milliseconds on each one)
    let hash = format!("motion{:016x}", twox_hash::XxHash3_64::oneshot(page));
    // RAEA_MOTION_KEEP: each frame's page in order, in pages.txt, to compare renders page by page
    if std::env::var_os("RAEA_MOTION_KEEP").is_some() {
        use std::io::Write as _;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(work.join("pages.txt")) {
            let _ = writeln!(f, "{hash}");
        }
    }
    std::fs::create_dir_all(work.join("assets")).map_err(|e| e.to_string())?;
    let p = work.join("assets").join(format!("{hash}.png"));
    if !p.exists() && PENDING.lock().unwrap().insert(p.clone()) {
        // premultiplied to straight, as a PNG holds it; written on a worker so the next frame can start (WRITERS,
        // joined before rdb reads the pages)
        let mut straight = page.to_vec();
        let path = p.clone();
        let job = std::thread::spawn(move || {
            for px in straight.chunks_exact_mut(4) {
                let a = px[3] as u32;
                if a > 0 && a < 255 {
                    for c in &mut px[..3] {
                        *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                    }
                }
            }
            fast_png(&path, &straight, w, h)
        });
        let mut ws = WRITERS.lock().unwrap();
        ws.push(job);
        if ws.len() > 8 {
            let first = ws.remove(0);
            drop(ws);
            first.join().map_err(|_| "a page writer panicked".to_string())??;
        }
    }
    Ok(Some(hash))
}

/// The frames as rdb's IR: pages as image rows, sound as audio rows, 3D as camera, light and mesh rows (docs:
/// TypeScript-only editing, phase 5: rdb's IR is the contract; rrl the language, which this once went through, is
/// retired). A column that changes frame by frame is keyed (linear) and its row holds the first frame's value.
fn program(frames: &[(i64, Frame)], w: u32, h: u32, fps: f64) -> rdb::ir::Program {
    use rdb::ir::V;
    let mut p = rdb::ir::Program { fps, width: w, height: h, frames: frames.len() as i64, ..Default::default() };
    let row = |id: &str, cols: Vec<(&str, V)>| -> BTreeMap<String, V> {
        let mut r: BTreeMap<String, V> = cols.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        r.insert("id".into(), V::S(id.into()));
        r
    };
    let clear = frames.first().and_then(|(_, f)| f.clear.clone()).and_then(|c| hex_of(&c)).unwrap_or("#000000".into());
    let has3d = frames.iter().any(|(_, f)| !f.scene.is_empty());
    let mut cam = row("cam", vec![("bg", V::S(clear))]);
    if has3d {
        // where it is and looks, frame by frame; its lens, ambient light and post-processing (scene3d)
        scene3d::camera(frames, &mut p, &mut cam);
    }
    p.spawns.push(("camera".into(), vec![cam]));
    // the pages: each distinct page shown for the frames in a row it's the same
    let mut runs: Vec<(String, usize, usize)> = Vec::new();
    for (i, (_, f)) in frames.iter().enumerate() {
        let Some(hash) = f.page.clone() else { continue };
        match runs.last_mut() {
            Some((hh, _, end)) if *hh == hash && *end == i => *end = i + 1,
            _ => runs.push((hash, i, i + 1)),
        }
    }
    for (k, (hash, a, b)) in runs.iter().enumerate() {
        p.spawns.push(("image".into(), vec![row(&format!("page{k}"), vec![("src", V::S(format!("asset:{hash}"))), ("x", V::N(0.0)), ("y", V::N(0.0)), ("w", V::N(w as f64)), ("h", V::N(h as f64)), ("from", V::N(*a as f64)), ("to", V::N(*b as f64)), ("z", V::N(10.0))])]));
    }
    // sound: per element, the stretches its source time runs on frame to frame
    let mut by: BTreeMap<String, Vec<(usize, Sound)>> = BTreeMap::new();
    for (i, (_, f)) in frames.iter().enumerate() {
        for s in &f.sounds {
            by.entry(format!("{}|{}", s.key, s.src)).or_default().push((i, s.clone()));
        }
    }
    let mut n = 0;
    let db = |v: f64| if v > 0.0 { 20.0 * v.log10() } else { -90.0 };
    let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
    let r6 = |x: f64| (x * 1e6).round() / 1e6;
    for (_, list) in by {
        let mut k = 0;
        while k < list.len() {
            let mut j = k;
            while j + 1 < list.len() && list[j + 1].0 == list[j].0 + 1 && (list[j + 1].1.at - list[j].1.at - list[j].1.rate / fps).abs() < 1e-6 {
                j += 1;
            }
            let (f0, s0) = (list[k].0, &list[k].1);
            let id = format!("sound{n}");
            let vols: Vec<f64> = list[k..=j].iter().map(|(_, s)| s.volume).collect();
            let mut cols = vec![("src", V::S(s0.src.clone())), ("from", V::N(f0 as f64)), ("to", V::N((list[j].0 + 1) as f64)), ("src_in", V::N(r6(s0.at)))];
            if vols.iter().all(|v| (*v - vols[0]).abs() < 1e-9) {
                cols.push(("gain", V::N(r3(db(vols[0])))));
            } else {
                cols.push(("gain", V::N(r3(db(vols[0])))));
                p.keys.insert((id.clone(), "gain".into()), list[k..=j].iter().map(|(f, s)| (*f as i64, r3(db(s.volume)), "linear".to_string())).collect());
                // the linear volume each frame, exact: the mixer core plays an SQL edit as its TSX plays (rounded dB isn't)
                p.keys.insert((id.clone(), "snd_vol".into()), list[k..=j].iter().map(|(f, s)| (*f as i64, s.volume, "step".to_string())).collect());
            }
            // what the mixer core needs to play these rows exactly as the frames played: the element's key, its exact
            // first volume, source time, rate and bus (snd_*: rdb's lanes read src_in and gain; the core reads these)
            cols.push(("snd_key", V::S(s0.key.clone())));
            cols.push(("snd_vol", V::N(s0.volume)));
            cols.push(("snd_at", V::N(s0.at)));
            cols.push(("snd_rate", V::N(s0.rate)));
            if let Some(b) = &s0.bus {
                cols.push(("snd_bus", V::S(b.clone())));
            }
            if let Some(v) = s0.fade_in.filter(|v| *v > 0.0 && k == 0) {
                cols.push(("fade_in", V::N(r6(v))));
            }
            if let Some(v) = list[j].1.fade_out.filter(|v| *v > 0.0 && j + 1 == list.len()) {
                cols.push(("fade_out", V::N(r6(v))));
            }
            p.spawns.push(("audio".into(), vec![row(&id, cols)]));
            n += 1;
            k = j + 1;
        }
    }
    if let Some((lufs, ceiling)) = frames.iter().find_map(|(_, f)| f.mix) {
        p.spawns.push(("master".into(), vec![row("mix", vec![("lufs", V::N(lufs)), ("ceiling", V::N(ceiling))])]));
    }
    if has3d {
        // lights, meshes, models and pictures in space, matched across frames by identity (scene3d)
        scene3d::rows(frames, &mut p);
    }
    p
}

/// The program's sounds that run out of source before the stretch playing them ends: the sound stops dead there and its
/// fade-out (timed to the stretch's end) never plays. A render doesn't loop a source (`loop` is live play's only), so a
/// bed shorter than its Sequence is the usual one: the noir doc's 10 s projector under a 12.3 s cue cut off mid-line.
/// A take that's quiet by its last 0.1 s and asked for no fade-out (a voice line ending before its cue does) isn't one.
/// Said as WARN lines, one per stretch; the render goes on.
fn sound_lint(p: &rdb::ir::Program, fps: f64) {
    use rdb::ir::V;
    let mut len: HashMap<String, Option<f64>> = HashMap::new();
    for (kind, rows) in &p.spawns {
        if kind != "audio" {
            continue;
        }
        for r in rows {
            let n = |k: &str| match r.get(k) {
                Some(V::N(x)) => Some(*x),
                _ => None,
            };
            let (Some(V::S(src)), Some(from), Some(to)) = (r.get("src"), n("from"), n("to")) else { continue };
            let (rate, src_in) = (n("snd_rate").unwrap_or(1.0), n("src_in").unwrap_or(0.0));
            let Some(have) = *len.entry(src.clone()).or_insert_with(|| raea_media::probe_seconds(Path::new(src))) else { continue };
            // seconds of the stretch left when the source ends
            let short = (src_in + (to - from) / fps * rate - have) / rate.max(1e-6);
            // a take that has gone quiet by its end (a line said, its room after) ends as it would have; one still
            // sounding, or whose fade-out was asked for, is cut
            let fades = n("fade_out").is_some_and(|v| v > 0.0);
            let sounding = || {
                raea_media::decode_audio_window(Path::new(src), (have - 0.1).max(0.0), 0.1).is_ok_and(|a| {
                    let (sum, k) = a.channels.iter().flatten().fold((0.0f64, 0usize), |(s, k), x| (s + (*x as f64).powi(2), k + 1));
                    k > 0 && 10.0 * (sum / k as f64).log10() > -50.0
                })
            };
            if short > 1.5 / fps && (fades || sounding()) {
                let name = Path::new(src).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| src.clone());
                eprintln!("WARN  sound {name} runs out {short:.2} s before its stretch ends (at {:.2} s of {:.2}-{:.2} s): it stops dead there and its fade-out never plays. Make the source as long as the Sequence (a render doesn't loop a sound)", to / fps - short, from / fps, to / fps);
            }
        }
    }
}

/// An edit's Tone as rows (@raea/motion/tone): the graph once (`tone` "graph") and each event on the frames it's shown
/// (`tone_ev`, from..to), so an SQL edit plays its Tone through the mixer core as the TSX did.
fn tone_rows(p: &mut rdb::ir::Program, tones: &[raea_mixer::ToneCue]) {
    use rdb::ir::V;
    let row = |id: &str, cols: Vec<(&str, V)>| -> BTreeMap<String, V> {
        let mut r: BTreeMap<String, V> = cols.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        r.insert("id".into(), V::S(id.into()));
        r
    };
    if let Some(g) = tones.iter().find_map(|t| t.graph.clone()) {
        p.spawns.push(("tone".into(), vec![row("graph", vec![("tone_json", V::S(g))])]));
    }
    // each event from the first frame it's shown to the last
    let mut seen: BTreeMap<String, (usize, usize, String)> = BTreeMap::new();
    for (f, t) in tones.iter().enumerate() {
        for e in &t.evs {
            let k = e.get("k").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let entry = seen.entry(k).or_insert((f, f, e.to_string()));
            entry.1 = f;
        }
    }
    for (n, (_, (a, b, ev))) in seen.into_iter().enumerate() {
        p.spawns.push(("tone_ev".into(), vec![row(&format!("tev{n}"), vec![("tone_json", V::S(ev)), ("from", V::N(a as f64)), ("to", V::N((b + 1) as f64))])]));
    }
}

/// The sandbox's WASI tools (RAEA_SANDBOX_TOOLS, else raea/sandbox/modules) as wsx.* calls, in the current folder.
#[cfg(feature = "sandbox")]
fn register_sandbox() {
    let dir = std::env::var_os("RAEA_SANDBOX_TOOLS").map(PathBuf::from).unwrap_or_else(|| rdb::home().join("engine/sandbox/modules"));
    if !dir.is_dir() {
        return;
    }
    match wsx::Sandbox::new(&dir) {
        Ok(sb) => wsx::register(sb, std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        Err(e) => eprintln!("raea-motion: the sandbox's tools didn't load ({}): {e}", dir.display()),
    }
}

pub fn cli(args: &[String]) -> i32 {
    crate::native::register();
    crate::startup_mark("native calls registered");
    #[cfg(feature = "sandbox")]
    if matches!(args.first().map(String::as_str), Some("run" | "test" | "types" | "shell" | "shell-serve")) {
        register_sandbox();
    }
    #[cfg(feature = "browser")]
    raea_browser::register();
    // a play or a render leaves its session to look back at (raea_native::trace: ~/Library/Logs/raea/raea-motion/)
    let traced = matches!(args.first().map(String::as_str), Some("play" | "render"));
    if traced {
        raea_native::trace::start("raea-motion");
        crate::startup_mark("trace started");
    }
    let code = match run(args) {
        Ok(msg) => {
            if !msg.is_empty() {
                println!("{msg}");
            }
            0
        }
        Err(e) => {
            raea_native::trace::event("refused", json!({"error": e}));
            println!("REFUSED  {e}");
            1
        }
    };
    if traced {
        raea_native::trace::end();
    }
    code
}

/// `raea-motion traces [show [N|last|DIR]]`: the sessions every raea app left (newest first: how each ended, its
/// glitches), or one told in full (why it crashed, its worst frames, its errors, its memory).
fn traces(args: &[String]) -> R<String> {
    let all = raea_native::trace::sessions();
    if args.get(1).map(String::as_str) == Some("show") {
        let which = args.get(2).map(String::as_str).unwrap_or("last");
        let dir = match which {
            "last" => all.first().map(|s| s.1.clone()),
            n if n.parse::<usize>().is_ok() => all.get(n.parse::<usize>().unwrap()).map(|s| s.1.clone()),
            p => Some(PathBuf::from(p)),
        }
        .ok_or("no such session (raea-motion traces lists them)")?;
        return Ok(raea_native::trace::summarize(&dir));
    }
    let mut out = format!("{:<4} {:<12} {:<28} {:<18} {:>9} {:>8}  {}\n", "#", "app", "session", "ended", "glitches", "errors", "folder");
    for (i, (app, d)) in all.iter().enumerate().take(40) {
        let events = std::fs::read_to_string(d.join("events.jsonl")).unwrap_or_default();
        let glitches = events.lines().filter(|l| l.contains("\"kind\":\"glitch\"")).count();
        let errors = events.lines().filter(|l| l.contains("\"level\":\"ERROR\"") || l.contains("\"kind\":\"error\"") || l.contains("\"kind\":\"gpu_error\"")).count();
        out += &format!("{:<4} {:<12} {:<28} {:<18} {:>9} {:>8}  {}\n", i, app, d.file_name().unwrap_or_default().to_string_lossy(), raea_native::trace::status(d), glitches, errors, d.display());
    }
    if all.is_empty() {
        out += "(no sessions yet)\n";
    }
    Ok(out)
}

/// A TypeScript program bundled for the engine (`raea-motion bundle`): the script and @raea/motion as one IIFE whose
/// entry calls the script's default export and reports the end through `done` (a throw prints and ends with 1).
/// A headless script bundled for the engine: imported where it lives (its own node_modules resolve), with the engine's
/// resolution on top (raea-bundle's script.rs: @raea/motion from sdk/motion/src, the test API, raea-react's node_modules as the
/// fallback); its default export, if a function, is called and awaited, then the run ends.
pub fn bundle_script(ts: &Path, out: &Path) -> R<()> {
    let entry = format!(
        "import * as M from {:?};\nconst call = (globalThis as any).__raea_call;\nPromise.resolve().then(() => (typeof (M as any).default === \"function\" ? (M as any).default() : undefined)).then(() => call(\"done\", JSON.stringify({{ code: 0 }})), (e: any) => {{ call(\"print\", JSON.stringify({{ text: \"error: \" + (e && e.stack ? e.stack : String(e)) }})); call(\"done\", JSON.stringify({{ code: 1 }})); }});\n",
        ts.display().to_string()
    );
    bundle_entry(&entry, out, &[])
}

/// `entry` (TypeScript source) bundled into `out` by raea-bundle's library (engine/bundle/src/script.rs's resolution); `extra`
/// are bundle.ts's flags (--define K=V, --alias SPEC=PATH, --node-stub a,b).
pub fn bundle_entry(entry: &str, out: &Path, extra: &[&str]) -> R<()> {
    let out = &std::path::absolute(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let work = std::env::temp_dir().join(format!("raea-motion-e{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    std::fs::write(work.join("entry.ts"), entry).map_err(|e| e.to_string())?;
    let js = rdb::home().join("sdk/react/js");
    std::fs::copy(js.join("tsconfig.json"), work.join("tsconfig.json")).map_err(|e| e.to_string())?;
    // in process by raea-bundle's library: the engine's resolution (engine/bundle/src/script.rs) and the flags
    {
        let (mut defines, mut aliases, mut stubs) = (serde_json::Map::new(), serde_json::Map::new(), Vec::new());
        let mut it = extra.iter();
        while let Some(flag) = it.next() {
            let Some(v) = it.next() else { break };
            match *flag {
                "--define" => {
                    if let Some((k, x)) = v.split_once('=') {
                        defines.insert(k.into(), x.into());
                    }
                }
                "--alias" => {
                    if let Some((k, p)) = v.split_once('=') {
                        aliases.insert(k.into(), std::path::absolute(p).unwrap_or_else(|_| PathBuf::from(p)).display().to_string().into());
                    }
                }
                "--node-stub" => stubs.extend(v.split(',').filter(|s| !s.is_empty()).map(|s| Value::from(s))),
                _ => {}
            }
        }
        let root = rdb::home();
        let opts = json!({
            "script": {"js": js, "motion": root.join("sdk/motion/src"), "browser": root.join("engine/browser/ts/index.ts"), "shell": root.join("sdk/shell/src/index.ts"),
                "aliases": aliases, "stubs": stubs},
            "defines": defines,
        });
        let r = inproc::bundle(&work.join("entry.ts"), opts);
        let _ = std::fs::remove_dir_all(&work);
        let (code, _css) = r.map_err(|e| format!("couldn't bundle it: {}", e.lines().filter(|l| !l.trim().is_empty()).take(8).collect::<Vec<_>>().join(" | ")))?;
        std::fs::write(out, code).map_err(|e| format!("{}: {e}", out.display()))
    }
}

/// Test files (bun:test style) bundled with the engine's test API and run on its V8: each case a PASS or FAIL line,
/// a summary, and the exit code 1 when any failed. `filter` keeps cases whose full name contains it.
pub fn bundle_tests(files: &[PathBuf], out: &Path, filter: Option<&str>, extra: &[&str]) -> R<()> {
    let mut entry = String::new();
    for f in files {
        entry += &format!("import {:?};\n", f.display().to_string());
    }
    entry += &format!(
        "import {{ run }} from \"bun:test\";\nconst call = (globalThis as any).__raea_call;\nconst print = (text: string) => call(\"print\", JSON.stringify({{ text }}));\nrun(print, {}).then((r) => call(\"done\", JSON.stringify({{ code: r.fail ? 1 : 0 }})), (e: any) => {{ print(\"error: \" + (e && e.stack ? e.stack : String(e))); call(\"done\", JSON.stringify({{ code: 1 }})); }});\n",
        filter.map(|f| format!("{f:?}")).unwrap_or("undefined".into())
    );
    bundle_entry(&entry, out, extra)
}

/// Run a bundled program (bundle_script's) in the engine's V8 with the native calls, headless: its exit code and what
/// it printed. The same on the Mac and the iPhone (raea/react/ios).
pub fn run_bundle(bundle: &Path, secs: f64) -> R<(i32, String)> {
    crate::native::DONE.store(-1, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut p) = crate::native::PRINTED.lock() {
        p.clear();
    }
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(bundle.to_path_buf()));
    let t0 = std::time::Instant::now();
    loop {
        app.update();
        let d = crate::native::DONE.load(std::sync::atomic::Ordering::SeqCst);
        if d >= 0 {
            let printed = crate::native::PRINTED.lock().map(|p| p.clone()).unwrap_or_default();
            return Ok((d, printed));
        }
        if t0.elapsed().as_secs_f64() > secs {
            return Err(format!("{} didn't finish in {secs} s (it throws at load, or never returns)", bundle.display()));
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// `raea-motion run SCRIPT.ts`: a TypeScript program against the engine, headless (the native calls: solids, ECS
/// worlds, print). The script's default export, if a function, is called; what it prints goes to stdout; a throw
/// ends the run with exit 1.
fn run_script(args: &[String]) -> R<i32> {
    let ts = PathBuf::from(&args[1]).canonicalize().map_err(|e| format!("{}: {e}", args[1]))?;
    let out = std::env::temp_dir().join(format!("raea-motion-{}.js", std::process::id()));
    bundle_script(&ts, &out)?;
    let secs: f64 = flag(args, "--timeout").and_then(|t| t.parse().ok()).unwrap_or(120.0);
    let (code, _) = run_bundle(&out, secs)?;
    let _ = std::fs::remove_file(&out);
    Ok(code)
}

/// `raea-motion mezz [--check] PATH...`: the sources under each PATH (followed through links, folders to four levels)
/// that want an optimized copy, made one at a time (or only listed, with --check). JSON: each source, what it is.
fn mezz(args: &[String]) -> R<String> {
    let check = args.iter().any(|a| a == "--check");
    let mut files = Vec::new();
    fn walk(p: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if p.is_dir() {
            if depth > 4 {
                return;
            }
            let Ok(rd) = std::fs::read_dir(p) else { return };
            let mut es: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))).collect();
            es.sort();
            for e in es {
                walk(&e, depth + 1, out);
            }
        } else if matches!(p.extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("mp4" | "mov" | "m4v")) {
            out.push(p.to_path_buf());
        }
    }
    for a in args.iter().filter(|a| !a.starts_with("--")) {
        walk(Path::new(a), 0, &mut files);
    }
    let mut report = Vec::new();
    for f in files {
        let state = if raea_media::mezz::has(&f) {
            "ready".to_string()
        } else if !raea_media::mezz::wants(&f) {
            "not needed".to_string()
        } else if check {
            "wanted".to_string()
        } else {
            let t = std::time::Instant::now();
            match raea_media::mezz::make(&f) {
                Ok(_) => format!("made in {:.1} s", t.elapsed().as_secs_f64()),
                Err(e) => format!("failed: {e}"),
            }
        };
        report.push(json!({"source": f, "copy": state}));
    }
    // range copies: an edit's frames (a folder's edit.tsx, or a .tsx named) show parts of long-GOP sources that have no
    // whole copy (1080p and under, or too long to copy whole): those parts, merged across gaps of up to 2 s and
    // padded by 1 s, get copies of their own (mezz::make_range)
    for a in args.iter().filter(|a| !a.starts_with("--")) {
        let p = Path::new(a);
        let tsx = if p.is_dir() { p.join("edit.tsx") } else { p.to_path_buf() };
        if tsx.extension().and_then(|x| x.to_str()) != Some("tsx") || !tsx.is_file() {
            continue;
        }
        let times = match crate::live::Live::open_baked(&tsx) {
            Ok(mut l) => l.video_times(),
            Err(e) => {
                report.push(json!({"edit": tsx, "ranges": format!("failed: {e}")}));
                continue;
            }
        };
        let mut by: std::collections::BTreeMap<PathBuf, Vec<f64>> = Default::default();
        for (file, t) in times {
            by.entry(file).or_default().push(t);
        }
        for (file, mut ts) in by {
            if raea_media::mezz::has(&file) || !raea_media::mezz::long_gop(&file) {
                continue;
            }
            ts.sort_by(f64::total_cmp);
            let mut spans: Vec<(f64, f64)> = Vec::new();
            for t in ts {
                match spans.last_mut() {
                    Some(s) if t - s.1 <= 2.0 => s.1 = t,
                    _ => spans.push((t, t)),
                }
            }
            for (a, b) in spans {
                let (a, b) = ((a - 1.0).max(0.0), b + 1.0);
                let covered = raea_media::mezz::ranges(&file).iter().any(|r| r.start <= a + 1e-6 && r.end >= b - 1e-6);
                let state = if covered {
                    "ready".to_string()
                } else if check {
                    "wanted".to_string()
                } else {
                    let t = std::time::Instant::now();
                    match raea_media::mezz::make_range(&file, a, b) {
                        Ok(_) => format!("made in {:.1} s", t.elapsed().as_secs_f64()),
                        Err(e) => format!("failed: {e}"),
                    }
                };
                report.push(json!({"source": file, "range": [(a * 1000.0).round() / 1000.0, (b * 1000.0).round() / 1000.0], "copy": state}));
            }
        }
    }
    Ok(serde_json::to_string_pretty(&report).unwrap_or_default())
}

fn run(args: &[String]) -> R<String> {
    if args.first().map(String::as_str) == Some("traces") {
        return traces(args);
    }
    if args.first().map(String::as_str) == Some("play") {
        return crate::play::play(args);
    }
    // `export-game EDIT.tsx -o OUT`: the game as a program of its own (game.rs, sdk/react/docs/game-export.md)
    if args.first().map(String::as_str) == Some("export-game") {
        return crate::game::export(args);
    }
    // `mezz [--check] PATH...`: optimized media (raea_media::mezz, lightspeed F4): each long-GOP source over 1080p under
    // PATH (a file, or a folder's videos) gets its intra-only ProRes copy, one at a time; --check only says which would
    if args.first().map(String::as_str) == Some("mezz") {
        return mezz(&args[1..]);
    }
    // the sound tools (motion/sound.rs): `sao "PROMPT" -o OUT.wav` and `tokensynth "TIMBRE" --notes C3,C4 -o DIR` make
    // through the edit's asset store as its render would; `setup-audio` installs or checks both local models;
    // `sounds [WORDS]` and `sounds get PACK/INSTRUMENT` search and fetch the open sample libraries
    match args.first().map(String::as_str) {
        Some("sao") => return sound::sao(args),
        Some("tokensynth") => return sound::tokensynth(args),
        Some("setup-audio") => return sound::setup_audio(args),
        Some("sounds") => return sound::sounds(args),
        // `splat train VIDEO|DIR -o OUT.ply`: footage trained into a Gaussian splat for <Splat> (motion/splat_train.rs)
        Some("splat") => return splat_train::splat(args),
        // `physics bake|goo|part ...`: the bakes <Physics> and <Goo> make (motion/physics.rs)
        Some("physics") => return physics::cli(args),
        // `scene lint|explain EDIT.tsx`: what each 3D part rests on; floating, sunk and clipping parts (motion/scene_lint.rs)
        Some("scene") => return scene3d::clip3d::lint::cli(args),
        // `song FILE [-o OUT.song.json] [--plot OUT.png]`: the song's energy map for pacing (motion/song.rs)
        Some("song") => return song::song(args),
        // `track CLIP.mp4 [--subject X,Y,W,H] [--from S]`: the clip's camera solved for <Matchmove> (motion/track.rs)
        Some("track") => return track::track(args),
        _ => {}
    }
    // `master-gain BUNDLE.js DIR`: the gain and ceiling a composition's render masters its sound with, as JSON (live
    // play asks this of a child process, so its second React and the whole mix never sit in the game's memory)
    if args.first().map(String::as_str) == Some("master-gain") && args.len() >= 3 {
        let g = master_gain(Path::new(&args[1]), Path::new(&args[2]))?;
        return Ok(match g {
            Some((gain, ceiling)) => json!({"gain_db": gain, "ceiling_dbtp": ceiling}),
            None => json!({"none": true}),
        }
        .to_string());
    }
    if args.first().map(String::as_str) == Some("bundle") && args.len() >= 2 {
        let ts = PathBuf::from(&args[1]).canonicalize().map_err(|e| format!("{}: {e}", args[1]))?;
        let out = PathBuf::from(flag(args, "-o").ok_or("raea-motion bundle SCRIPT.ts -o OUT.js")?);
        if args.iter().any(|a| a == "--composition-bundle") {
            let portable = flag(args, "--portable").map(PathBuf::from);
            bundle_composition(&ts, &out, &comp_opts(args)?, portable.as_deref())?;
            return Ok(format!("APPLIED  {} bundled as a composition to {}", ts.display(), out.display()));
        }
        bundle_script(&ts, &out)?;
        return Ok(format!("APPLIED  {} bundled to {}", ts.display(), out.display()));
    }
    if args.first().map(String::as_str) == Some("types") {
        // the native calls' TypeScript, from the registry (sdk/motion/src/native-calls.ts)
        crate::native::register();
        let ts = raea_native::typescript();
        match flag(args, "-o") {
            Some(o) => {
                std::fs::write(&o, &ts).map_err(|e| format!("{o}: {e}"))?;
                return Ok(format!("APPLIED  {} native calls' types written to {o}", raea_native::calls().len()));
            }
            None => return Ok(ts),
        }
    }
    if args.first().map(String::as_str) == Some("test") && args.len() >= 2 {
        // conformance tests on the engine's V8: raea-motion test FILE... [--filter TEXT] [--timeout S] [--bundle-only OUT.js]
        // --define K=V, --alias SPEC=PATH and --node-stub a,b go to the bundler (each may repeat)
        let mut files = Vec::new();
        let mut owned: Vec<String> = Vec::new();
        let mut i = 1;
        while i < args.len() {
            if args[i].starts_with("--") {
                if matches!(args[i].as_str(), "--define" | "--alias" | "--node-stub") && i + 1 < args.len() {
                    owned.push(args[i].clone());
                    // an alias's path is the caller's, so it's made absolute here (the bundler runs elsewhere)
                    owned.push(match (args[i].as_str(), args[i + 1].split_once('=')) {
                        ("--alias", Some((k, v))) => format!("{k}={}", std::path::absolute(v).map(|p| p.display().to_string()).unwrap_or_else(|_| v.to_string())),
                        _ => args[i + 1].clone(),
                    });
                }
                i += 2;
                continue;
            }
            files.push(PathBuf::from(&args[i]).canonicalize().map_err(|e| format!("{}: {e}", args[i]))?);
            i += 1;
        }
        let extra: Vec<&str> = owned.iter().map(String::as_str).collect();
        if let Some(o) = flag(args, "--bundle-only") {
            // the bundler runs in its own folder, so the output path is made absolute here
            let o = std::path::absolute(&o).map_err(|e| format!("{o}: {e}"))?.display().to_string();
            bundle_tests(&files, Path::new(&o), flag(args, "--filter").as_deref(), &extra)?;
            return Ok(format!("APPLIED  {} test files bundled to {o}", files.len()));
        }
        // the repo root for fixtures (tests read RAEA_ROOT through host.env)
        if std::env::var_os("RAEA_ROOT").is_none() {
            unsafe { std::env::set_var("RAEA_ROOT", rdb::home().canonicalize().unwrap_or_default()) };
        }
        let out = std::env::temp_dir().join(format!("raea-motion-t{}.js", std::process::id()));
        bundle_tests(&files, &out, flag(args, "--filter").as_deref(), &extra)?;
        let secs: f64 = flag(args, "--timeout").and_then(|t| t.parse().ok()).unwrap_or(300.0);
        let (code, _) = run_bundle(&out, secs)?;
        let _ = std::fs::remove_file(&out);
        if code != 0 {
            return Err(format!("{} test file(s): some failed", files.len()));
        }
        return Ok(String::new());
    }
    if matches!(args.first().map(String::as_str), Some("shell" | "shell-serve")) && args.len() >= 2 {
        // the agent's shell for a host outside V8 (void): `shell ROOT -c CMD [--timeout MS]` answers one JSON line;
        // `shell-serve ROOT` keeps a Shell and answers JSON lines on stdin until it closes; --network POLICY.json
        // gives curl its allow-list (and the env names of its secrets)
        let root = std::path::absolute(&args[1]).map_err(|e| format!("{}: {e}", args[1]))?.display().to_string();
        let network = match flag(args, "--network") {
            Some(p) => Some(json_file(&p)?),
            None => None,
        };
        let opts = json!({"root": root, "network": network});
        let call = if args[0] == "shell" {
            let cmd = flag(args, "-c").ok_or("raea-motion shell ROOT -c CMD")?;
            let t: Option<u64> = flag(args, "--timeout").and_then(|t| t.parse().ok());
            format!("once(opts, {}, {})", serde_json::to_string(&cmd).unwrap(), t.map(|t| t.to_string()).unwrap_or("undefined".into()))
        } else {
            "serve(opts)".into()
        };
        let entry = format!(
            "import {{ once, serve }} from {:?};\nconst opts: any = {};\nif (!opts.network) delete opts.network;\nconst call = (globalThis as any).__raea_call;\n{call}.then(() => call(\"done\", JSON.stringify({{ code: 0 }})), (e: any) => {{ call(\"host.write\", JSON.stringify({{ text: JSON.stringify({{ error: String(e && e.stack || e) }}) + \"\\n\" }})); call(\"done\", JSON.stringify({{ code: 1 }})); }});\n",
            rdb::home().join("sdk/shell/src/serve.ts").canonicalize().map_err(|e| e.to_string())?.display().to_string(),
            opts
        );
        let out = std::env::temp_dir().join(format!("raea-shell-{}.js", std::process::id()));
        bundle_entry(&entry, &out, &[])?;
        crate::native::ECHO.store(false, std::sync::atomic::Ordering::Relaxed);
        let (code, _) = run_bundle(&out, f64::INFINITY)?;
        let _ = std::fs::remove_file(&out);
        if code != 0 {
            return Err("the shell ended with an error".into());
        }
        return Ok(String::new());
    }
    if args.first().map(String::as_str) == Some("run") && args.len() >= 2 {
        let code = run_script(args)?;
        if code != 0 {
            return Err(format!("{} ended with exit {code}", args[1]));
        }
        return Ok(String::new());
    }
    // a composition already bundled (NAME.comp.js, as the iPhone's probe renders it), its media under DIR; --frame N
    // draws the one frame the probe's raea_motion_frame draws, for comparing the phone's pixels with the Mac's
    if args.first().map(String::as_str) == Some("render") && args.get(1).map(String::as_str) == Some("--bundle") && args.len() >= 3 {
        let js = PathBuf::from(&args[2]);
        let dir = PathBuf::from(flag(args, "--dir").unwrap_or_else(|| js.parent().unwrap().to_string_lossy().into_owned()));
        let out = PathBuf::from(flag(args, "-o").ok_or("-o OUT.mp4|OUT.png|DIR|OUT.sql")?);
        let wanted = flag(args, "--frame").map(|f| f.parse::<i64>().map(|f| vec![f]).map_err(|_| "--frame takes a frame number")).transpose()?;
        return render_bundle(&js, &dir, &out, wanted);
    }
    if args.first().map(String::as_str) != Some("render") || args.len() < 2 {
        return Err("raea-motion render EDIT.tsx -o OUT.mp4|OUT.png|DIR|OUT.sql [--frame N | --at A,B] [--composition ID] [--w --h --fps --frames] [--words W.json] [--perception P.json] [--input I.json|inputs.rrl] [--manifest render.json] [--stage WxH] [--10bit]\n       raea-motion bundle EDIT.tsx --composition-bundle -o OUT.js [--portable DIR]   (a composition for render_bundle, on the Mac or the iPhone)\n       raea-motion sao \"PROMPT\" [--seconds 2] [--seed 1] [--dir EDIT_DIR] [-o OUT.wav]   (foley, local)\n       raea-motion tokensynth \"TIMBRE\" [--notes C2,C3,C4] [--seconds 2] [--velocity 100] [--seed 1234] [--precision fp16] [--dir EDIT_DIR] [-o OUT_DIR]   (an instrument's notes, local)\n       raea-motion setup-audio [--check] [sao|tokensynth]   (install or check the local audio models)\n       raea-motion song FILE [-o OUT.song.json] [--srt S.srt|none] [--plot OUT.png]   (the song's energy map: beats, sections, builds, drops, stops, hits, cuts per bar)\n       raea-motion sounds [WORDS] | sounds get PACK/INSTRUMENT [--notes ..|--all] [-o DIR]   (open sample libraries for a Sampler)\n       raea-motion track CLIP.mp4 [--subject X,Y,W,H] [--from S --seconds N] [--height M] [--fov DEG] [--dir EDIT_DIR] [-o OUT.json]   (the clip's camera solved, for <Matchmove>)".into());
    }
    let tsx = PathBuf::from(&args[1]).canonicalize().map_err(|e| format!("{}: {e}", args[1]))?;
    let out = PathBuf::from(flag(args, "-o").ok_or("-o OUT.mp4|OUT.png|DIR|OUT.sql")?);
    let dir = tsx.parent().unwrap().to_path_buf();
    let js = std::env::temp_dir().join(format!("raea-motion-{}.js", std::process::id()));
    // the edit's words.json and perception.json beside it, as play loads them (a render without the flags lost the
    // captions its words drive; the corpus found 10 edits so); a flag still wins
    let mut o = comp_opts(args)?;
    for (slot, file) in [(&mut o.words, "words.json"), (&mut o.perception, "perception.json")] {
        if slot.is_none() && dir.join(file).exists() {
            *slot = Some(json_file(&dir.join(file).display().to_string())?);
        }
    }
    bundle_composition(&tsx, &js, &o, None)?;
    let wanted = match (flag(args, "--frame"), flag(args, "--at")) {
        (Some(f), _) => Some(vec![f.parse().map_err(|_| "--frame takes a frame number")?]),
        (_, Some(at)) => Some(at.split(',').map(|f| f.trim().parse::<i64>().map_err(|_| "--at takes frame numbers".to_string())).collect::<R<_>>()?),
        _ => None,
    };
    *MANIFEST.lock().unwrap() = flag(args, "--manifest").map(PathBuf::from);
    TEN_BIT.store(args.iter().any(|a| a == "--10bit"), std::sync::atomic::Ordering::Relaxed);
    // --stage WxH: only a responsive composition takes it (as play: a fixed one keeps its own size, scaled to fit)
    *STAGE.lock().unwrap() = match flag(args, "--stage") {
        Some(v) => {
            let size = v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?.max(1), b.parse::<u32>().ok()?.max(1)))).ok_or("--stage takes WxH")?;
            let uses = |p: &Path| std::fs::read_to_string(p).is_ok_and(|t| t.contains("useVideoConfig("));
            let responsive = uses(&tsx) || std::fs::read_dir(dir.join("components")).into_iter().flatten().flatten().any(|e| uses(&e.path()));
            if !responsive {
                eprintln!("raea-motion: --stage {v}: the composition doesn't lay out from useVideoConfig, so it renders at its own size (as void's stage scales it)");
            }
            responsive.then_some(size)
        }
        None => None,
    };
    // a whole video made from these same inputs before is the export: cloned from the cache, not rendered again
    let cache = if wanted.is_none() { export_cache(&tsx, &js, &out, args) } else { None };
    let manifest = MANIFEST.lock().unwrap().clone();
    if let Some(c) = &cache {
        // with --manifest, the manifest kept beside the export comes back too
        let kept_manifest = manifest.as_ref().map(|m| (m, c.with_extension("manifest")));
        if let (Ok(said), true, true) = (std::fs::read_to_string(c.with_extension("said")), c.exists(), kept_manifest.as_ref().is_none_or(|(_, k)| k.exists())) {
            if std::fs::copy(c, &out).is_ok() && kept_manifest.as_ref().is_none_or(|(m, k)| std::fs::copy(k, m).is_ok()) {
                let _ = std::fs::File::options().append(true).open(c).and_then(|f| f.set_modified(std::time::SystemTime::now()));
                let _ = std::fs::remove_file(&js);
                return Ok(format!("{} (the same export as before: from the export cache)", said.replace("{OUT}", &out.display().to_string())));
            }
        }
    }
    let said = render_bundle(&js, &dir, &out, wanted);
    let _ = std::fs::remove_file(&js);
    if let (Some(c), Ok(s)) = (&cache, &said) {
        if c.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) {
            let tmp = c.with_extension(format!("{}.tmp", std::process::id()));
            let manifest_kept = manifest.as_ref().is_none_or(|m| std::fs::copy(m, c.with_extension("manifest")).is_ok());
            if manifest_kept && std::fs::copy(&out, &tmp).is_ok() && std::fs::rename(&tmp, c).is_ok() && std::fs::write(c.with_extension("said"), s.replace(&out.display().to_string(), "{OUT}")).is_ok() {
                if let Some(d) = c.parent() {
                    prune_cache(d, "mp4", 16);
                    // this export is an output: later keys leave it out of the edit's folder
                    if let Ok(abs) = std::fs::canonicalize(&out) {
                        use std::io::Write as _;
                        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.join("outputs.list")) {
                            let _ = writeln!(f, "{}", abs.display());
                        }
                    }
                }
            } else {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
    said
}

/// Where a whole-video export of `tsx` is kept (lightspeed T1): named by everything it's made from. That's the bundle's
/// bytes; this binary; the arguments but -o; every RAEA_ setting that isn't a timing or a trace; and each media file
/// the edit can read, by path, size and modified time (those under its folder, links followed, and every absolute media
/// path the bundle names). The output's own extension goes in too, and paths as they really are; where it and a
/// --manifest go, a generation session's token and ledger, playback's mute, and void's own RAEA_VOID_ settings don't. With --manifest the manifest is kept beside it and comes back with it. None for anything but
/// an .mp4 or .mov, when
/// RAEA_AUDIO_BOUNCE asks for a side file, with RAEA_FRAME_LOG, or with RAEA_EXPORT_CACHE=0; RAEA_EXPORT_CACHE=DIR moves it from
/// ~/Library/Caches/raea/exports. A hit is the earlier export's file, so it's the same bytes.
fn export_cache(tsx: &Path, js: &Path, out: &Path, args: &[String]) -> Option<PathBuf> {
    let ext = out.extension()?.to_str()?.to_ascii_lowercase();
    // RAEA_FRAME_LOG measures a render, so it renders (a hit would hand back the earlier file and write no log)
    if !matches!(ext.as_str(), "mp4" | "mov") || std::env::var_os("RAEA_AUDIO_BOUNCE").is_some() || std::env::var_os("RAEA_FRAME_LOG").is_some() {
        return None;
    }
    let root = match std::env::var("RAEA_EXPORT_CACHE") {
        Ok(v) if v == "0" => return None,
        Ok(v) => PathBuf::from(v),
        Err(_) => PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches/raea/exports"),
    };
    let bundle = std::fs::read(js).ok()?;
    let mut key: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: &[u8]| {
        for x in b {
            key = (key ^ *x as u64).wrapping_mul(0x100_0000_01b3);
        }
        key = (key ^ 0xff).wrapping_mul(0x100_0000_01b3);
    };
    eat(&bundle);
    eat(ext.as_bytes());
    // this build (its binary's modification time and size): another engine paints other bytes
    if let Some(m) = std::env::current_exe().ok().and_then(|e| std::fs::canonicalize(e).ok()).and_then(|e| e.metadata().ok()) {
        let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
        eat(&t.to_le_bytes());
        eat(&m.len().to_le_bytes());
    }
    let mut skip = false;
    for a in args.iter().skip(1) {
        if std::mem::take(&mut skip) {
            continue;
        }
        // where the video and its manifest go isn't what's in them
        if a == "-o" || a == "--manifest" {
            skip = true;
            continue;
        }
        // a path by where it really is (/var and /private/var are one folder)
        match std::fs::canonicalize(a) {
            Ok(p) => eat(p.as_os_str().as_encoded_bytes()),
            Err(_) => eat(a.as_bytes()),
        }
    }
    let mut env: Vec<(String, String)> = std::env::vars().filter(|(k, _)| k.starts_with("RAEA_") && !k.starts_with("RAEA_VOID_") && !matches!(k.as_str(), "RAEA_MOTION_TIMES" | "RAEA_STARTUP_TRACE" | "RAEA_BAKE_TRACE" | "RAEA_LIVE_TRACE" | "RAEA_GPU_TIMES" | "RAEA_MASTER_TRACE" | "RAEA_SOUND_TRACE" | "RAEA_GEN_TOKEN" | "RAEA_GEN_LEDGER" | "RAEA_AUDIO_MUTE")).collect();
    env.sort();
    for (k, v) in &env {
        eat(k.as_bytes());
        eat(v.as_bytes());
    }
    let mut files: Vec<PathBuf> = Vec::new();
    fn walk(d: &Path, depth: usize, out: &Path, files: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') || p == out {
                continue;
            }
            if p.is_dir() {
                if depth < 6 {
                    walk(&p, depth + 1, out, files);
                }
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| MEDIA_EXTS.contains(&x.to_ascii_lowercase().as_str()) || matches!(x, "ttf" | "otf" | "woff" | "woff2" | "glb" | "gltf" | "bin" | "json" | "css")) {
                files.push(p);
            }
        }
    }
    let out_abs = std::fs::canonicalize(out).unwrap_or_else(|_| out.to_path_buf());
    walk(tsx.parent()?, 0, &out_abs, &mut files);
    // exports this cache wrote (an earlier render's OUT.mp4 beside the edit) are outputs, not inputs, unless the
    // composition plays them: a cut laid from its shots' renders (anything/cut/intro.tsx over intro/renders/*.mp4)
    // must miss when a shot is rendered again, and kept them out, it came back from the cache unchanged (10-21). A
    // render counts as played when the bundle names it (its name without the extension in the bundle's text, as a path
    // built from a template holds only that: `renders/${name}.mp4`; a name that only looks played costs a render,
    // never a stale export)
    let text = String::from_utf8_lossy(&bundle);
    let made: std::collections::HashSet<PathBuf> = std::fs::read_to_string(root.join("outputs.list")).unwrap_or_default().lines().map(PathBuf::from).collect();
    files.retain(|p| !made.contains(&std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())) || p.file_stem().is_some_and(|n| text.contains(&*n.to_string_lossy())));
    // absolute media paths the bundle names (an import from outside the edit's folder becomes its path)
    for q in text.split('"').skip(1).step_by(2) {
        if q.starts_with('/') && q.len() < 4096 && Path::new(q).extension().and_then(|x| x.to_str()).is_some_and(|x| MEDIA_EXTS.contains(&x.to_ascii_lowercase().as_str())) {
            files.push(PathBuf::from(q));
        }
    }
    files.push(std::env::current_exe().ok().and_then(|e| std::fs::canonicalize(e).ok())?);
    files.sort();
    files.dedup();
    for p in &files {
        eat(p.as_os_str().as_encoded_bytes());
        match std::fs::metadata(p) {
            Ok(m) => {
                eat(&m.len().to_le_bytes());
                eat(&m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0).to_le_bytes());
            }
            Err(_) => eat(b"missing"),
        }
        // which file its decoders read: a video's optimized copy, once complete, changes the pixels without touching
        // the source (raea_media::mezz)
        if MEDIA_EXTS.contains(&p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase().as_str()) {
            if raea_media::mezz::has(p) {
                eat(b"mezz");
            }
            // its range copies too: the frames inside them come from the copies
            for r in raea_media::mezz::ranges(p) {
                eat(format!("mezz@{}-{}", r.first, r.last).as_bytes());
            }
        }
    }
    Some(root.join(format!("{key:016x}.{ext}")))
}

/// How a composition is picked and, for a bare default export, sized (`--composition ID`, `--w --h --fps --frames`).
pub struct CompOpts {
    pub composition: Option<String>,
    pub w: u32,
    pub h: u32,
    pub fps: f64,
    pub frames: i64,
    /// the Clock's words, perception and input (`--words`, `--perception`, `--input`), as JSON for the entry
    pub words: Option<Value>,
    pub perception: Option<Value>,
    pub input: Option<Value>,
    /// live play's runtime instead of React (sdk/react/js/lite: each frame rendered and only what changed sent, no
    /// reconciler): what a game plays on; renders keep React
    pub lite: bool,
    /// with `lite`: each frame's marker says which components rendered (`--comps`; bake.rs)
    pub comps: bool,
}

/// A JSON file's value.
fn json_file(path: &str) -> R<Value> {
    let t = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&t).map_err(|e| format!("{path}: {e}"))
}

/// The DOM key a recorded key's name stands for (inputs.rrl names arrows `left`, `right`, ...).
fn dom_key(name: &str) -> String {
    match name {
        "left" | "right" | "up" | "down" => format!("Arrow{}{}", name[..1].to_uppercase(), &name[1..]),
        "space" => " ".into(),
        "enter" | "return" => "Enter".into(),
        "escape" | "esc" => "Escape".into(),
        _ => name.into(),
    }
}

/// `--input FILE`: a JSON input timeline ({frame: {keys, pointer}}), or rrl's recorded input (inputs.rrl: `key NAME
/// {down: {F: 0|1, ...} step}`) as {"steps": {DOMKEY: {F: 0|1}}}, which the entry expands frame by frame.
fn input_file(path: &str) -> R<Value> {
    if !path.ends_with(".rrl") {
        return json_file(path);
    }
    let t = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut steps = Map::new();
    for line in t.lines().map(str::trim).filter(|l| l.starts_with("key ")) {
        let name = line[4..].split_whitespace().next().unwrap_or("");
        let (Some(a), Some(b)) = (line.find("down:").and_then(|i| line[i..].find('{').map(|j| i + j)), line.rfind('}')) else { continue };
        let body = &line[a + 1..];
        let body = &body[..body.find('}').unwrap_or(b - a - 1)];
        let mut m = Map::new();
        for kv in body.split(',') {
            if let Some((k, v)) = kv.split_once(':') {
                if let (Ok(k), Ok(v)) = (k.trim().parse::<i64>(), v.trim().parse::<f64>()) {
                    m.insert(k.to_string(), json!(v));
                }
            }
        }
        steps.insert(dom_key(name), Value::Object(m));
    }
    if steps.is_empty() {
        return Err(format!("{path}: no `key NAME {{down: {{...}}}}` lines"));
    }
    Ok(json!({ "steps": steps }))
}

pub(crate) fn comp_opts(args: &[String]) -> R<CompOpts> {
    let n = |k: &str| -> R<Option<f64>> { flag(args, k).map(|v| v.parse::<f64>().map_err(|_| format!("{k} takes a number"))).transpose() };
    Ok(CompOpts {
        composition: flag(args, "--composition"),
        w: n("--w")?.unwrap_or(1080.0) as u32,
        h: n("--h")?.unwrap_or(1920.0) as u32,
        fps: n("--fps")?.unwrap_or(30.0),
        frames: n("--frames")?.unwrap_or(60.0) as i64,
        words: flag(args, "--words").map(|p| json_file(&p)).transpose()?,
        perception: flag(args, "--perception").map(|p| json_file(&p)).transpose()?,
        input: flag(args, "--input").map(|p| input_file(&p)).transpose()?,
        lite: args.iter().any(|a| a == "--lite"),
        comps: args.iter().any(|a| a == "--comps"),
    })
}

/// A composition bundled for the engine (raea-bundle's library: Rolldown, on the Mac): the edit's folder and @raea/motion beside raea-react's
/// node_modules (one React), behind the entry that steps its Clock. With `portable` a folder, imported media is copied
/// into its media/ and the bundle names it relatively, so the two travel together (to the iPhone).
pub fn bundle_composition(tsx: &Path, out_js: &Path, o: &CompOpts, portable: Option<&Path>) -> R<()> {
    let dir = tsx.parent().unwrap().to_path_buf();
    #[cfg(feature = "ml")]
    crate::native::ml::set_dir(&dir);
    // a bundle made from these same inputs before is copied instead of bundled again (lightspeed F5): the copying and
    // bundling on every open, render and play. Not a portable one (it copies media beside it)
    let cached = if portable.is_none() { bundle_cache(tsx, o) } else { None };
    if let Some(c) = &cached {
        if std::fs::copy(c, out_js).is_ok() {
            let _ = std::fs::File::options().append(true).open(c).and_then(|f| f.set_modified(std::time::SystemTime::now()));
            return Ok(());
        }
    }
    bundle_fresh(tsx, out_js, o, portable)?;
    if let Some(c) = cached {
        // written whole, then moved in
        let tmp = c.with_extension(format!("{}.tmp", std::process::id()));
        if c.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && std::fs::copy(out_js, &tmp).is_ok() && std::fs::rename(&tmp, &c).is_ok() {
            if let Some(d) = c.parent() {
                prune_cache(d, "js", 64);
            }
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    Ok(())
}

/// Where the bundle of `tsx` with `o` is kept: named by everything the bundle is made from, each file by its path, size
/// and modified time (nanoseconds) as copy_dir would take it (the edit's folder, raea/motion/src, the lite runtime),
/// the JS packages' manifests, bun itself, this binary (the rewriting is its code) and the entry it writes. None when
/// there's nowhere to keep it (RAEA_BUNDLE_CACHE=0 turns it off; RAEA_BUNDLE_CACHE=DIR moves it from
/// ~/Library/Caches/raea/bundles).
fn bundle_cache(tsx: &Path, o: &CompOpts) -> Option<PathBuf> {
    let root = match std::env::var("RAEA_BUNDLE_CACHE") {
        Ok(v) if v == "0" => return None,
        Ok(v) => PathBuf::from(v),
        Err(_) => PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches/raea/bundles"),
    };
    let mut key: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: &[u8]| {
        for x in b {
            key = (key ^ *x as u64).wrapping_mul(0x100_0000_01b3);
        }
    };
    fn stat(p: &Path) -> Option<(u64, u128)> {
        let m = std::fs::metadata(p).ok()?;
        Some((m.len(), m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos()))
    }
    fn walk(from: &Path, depth: usize, out: &mut Vec<(PathBuf, u64, u128)>) {
        let Ok(rd) = std::fs::read_dir(from) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                if depth < 6 {
                    walk(&p, depth + 1, out);
                }
            } else if matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx" | "js" | "jsx" | "json" | "css")) {
                if let Some((n, t)) = stat(&p) {
                    out.push((p, n, t));
                }
            }
        }
    }
    let file = tsx.file_name()?.to_string_lossy().to_string();
    let js = rdb::home().join("sdk/react/js");
    let mut files = Vec::new();
    walk(tsx.parent()?, 0, &mut files);
    walk(&rdb::home().join("sdk/motion/src"), 0, &mut files);
    for (_, d) in SDKS {
        walk(&rdb::home().join(d), 0, &mut files);
    }
    if o.lite {
        walk(&js.join("lite"), 0, &mut files);
        files.extend(stat(&js.join("node_modules/bevy-react/src/bridge.ts")).map(|(n, t)| (js.join("node_modules/bevy-react/src/bridge.ts"), n, t)));
    }
    for p in [js.join("tsconfig.json"), js.join("package.json"), js.join("bun.lock")] {
        files.extend(stat(&p).map(|(n, t)| (p, n, t)));
    }
    files.extend(std::env::current_exe().ok().and_then(|p| stat(&p).map(|(n, t)| (p, n, t))));
    files.sort();
    for (p, n, t) in &files {
        eat(p.as_os_str().as_encoded_bytes());
        eat(&n.to_le_bytes());
        eat(&t.to_le_bytes());
    }
    eat(file.as_bytes());
    eat(if o.lite { entry_lite(&file, o) } else { entry(&file, o) }.as_bytes());
    // the bundler: raea-bundle's library, by its path, size and time
    if let Some((p, n, t)) = inproc::lib().ok().and_then(|l| stat(&l.path).map(|(n, t)| (l.path.clone(), n, t))) {
        eat(p.as_os_str().as_encoded_bytes());
        eat(&n.to_le_bytes());
        eat(&t.to_le_bytes());
    }
    Some(root.join(format!("{key:016x}.js")))
}

/// The newest `keep` files with extension `ext` in `dir` kept, by when they were last written or used.
fn prune_cache(dir: &Path, ext: &str, keep: usize) {
    // a write that never finished (its process killed between the copy and the rename) goes after an hour
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d.as_secs() > 3600);
            if old && e.path().extension().is_some_and(|x| x == "tmp") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut all: Vec<_> = rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == ext)).filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    if all.len() <= keep {
        return;
    }
    all.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, p) in all.into_iter().skip(keep) {
        let _ = std::fs::remove_file(p);
    }
}

/// raea-bundle (bundle/: Rolldown, lightningcss, the Z3 compile) as a library loaded at run time, never linked: its
/// crates' Cargo features would reach raea's own (f60e0b7). Found at RAEA_BUNDLE_LIB, beside this binary, or in
/// engine/bundle/target/release (`cargo build --release` in bundle/).
mod inproc {
    use std::ffi::{c_char, CStr, CString};
    use std::path::{Path, PathBuf};

    pub struct Lib {
        pub path: PathBuf,
        bundle: unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char,
        free: unsafe extern "C" fn(*mut c_char),
        _lib: libloading::Library,
    }

    pub fn lib() -> Result<&'static Lib, String> {
        static LIB: std::sync::OnceLock<Result<Lib, String>> = std::sync::OnceLock::new();
        LIB.get_or_init(|| {
            const NAME: &str = "libraea_bundle.dylib";
            let mut at: Vec<PathBuf> = std::env::var_os("RAEA_BUNDLE_LIB").map(PathBuf::from).into_iter().collect();
            // beside the binary (sdk/react/build.rs puts it there), or a folder up (test binaries run from deps/); the binary
            // itself, not a link to it (an install links ~/.local/bin/raea-motion to ~/.raea/bin)
            if let Some(dir) = std::env::current_exe().ok().map(|e| e.canonicalize().unwrap_or(e)).and_then(|e| e.parent().map(Path::to_path_buf)) {
                at.push(dir.join(NAME));
                at.extend(dir.parent().map(|d| d.join(NAME)));
            }
            // no bundler at all when it isn't there: raea doesn't fall back to bun
            let path = at.into_iter().find(|p| p.is_file()).ok_or(format!("raea can't bundle: no {NAME} beside {} (sdk/react/build.rs builds it with raea-react's modules feature; RAEA_BUNDLE_LIB names another)", std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default()))?;
            // SAFETY: raea-bundle's own library, whose C API (engine/bundle/src/lib.rs ffi) is checked by its version first
            unsafe {
                let lib = libloading::Library::new(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let api: libloading::Symbol<unsafe extern "C" fn() -> u32> = lib.get(b"raea_bundle_api").map_err(|e| format!("{}: {e}", path.display()))?;
                if api() != 1 {
                    return Err(format!("{}: API {} (this raea-motion wants 1): build bundle/ again", path.display(), api()));
                }
                let bundle = *lib.get(b"raea_bundle").map_err(|e| format!("{}: {e}", path.display()))?;
                let free = *lib.get(b"raea_bundle_free").map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(Lib { path, bundle, free, _lib: lib })
            }
        })
        .as_ref()
        .map_err(|e| e.clone())
    }

    /// `entry` bundled with `opts` (raea-bundle's: {"z3"}, {"script", "defines"}): the script and the CSS its modules
    /// imported
    pub fn bundle(entry: &Path, opts: serde_json::Value) -> Result<(String, String), String> {
        let lib = lib()?;
        let e = CString::new(entry.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        let o = CString::new(opts.to_string()).map_err(|e| e.to_string())?;
        // SAFETY: NUL-terminated strings in; the reply is the library's, freed by it
        let reply = unsafe {
            let p = (lib.bundle)(e.as_ptr(), o.as_ptr());
            if p.is_null() {
                return Err("raea-bundle gave no reply".into());
            }
            let s = CStr::from_ptr(p).to_string_lossy().into_owned();
            (lib.free)(p);
            s
        };
        let v: serde_json::Value = serde_json::from_str(&reply).map_err(|e| format!("raea-bundle's reply: {e}"))?;
        if let Some(err) = v["error"].as_str() {
            return Err(err.to_string());
        }
        Ok((v["js"].as_str().unwrap_or_default().to_string(), v["css"].as_str().unwrap_or_default().to_string()))
    }
}

fn bundle_fresh(tsx: &Path, out_js: &Path, o: &CompOpts, portable: Option<&Path>) -> R<()> {
    let t0 = std::time::Instant::now();
    let dir = tsx.parent().unwrap().to_path_buf();
    // one at a time: the work folder is the process's (void's stage re-bakes on a worker while another object opens)
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let work = std::env::temp_dir().join(format!("raea-motion-b{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    copy_dir(&dir, &work.join("src"), 0)?;
    let motion_src = rdb::home().join("sdk/motion/src").canonicalize().map_err(|e| format!("raea/motion/src: {e}"))?;
    copy_dir(&motion_src, &work.join("motion"), 0)?;
    for (_, d) in SDKS {
        let from = rdb::home().join(d);
        let to = work.join(d.rsplit('/').nth(1).unwrap_or(d));
        copy_dir(&from, &to, 0)?;
        for e in std::fs::read_dir(&to).map_err(|e| e.to_string())?.flatten() {
            let p = e.path();
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let rewritten = rewrite_into(&text, &from, &motion_src, "../motion", portable)?;
            if rewritten != text {
                std::fs::write(&p, rewritten).map_err(|e| e.to_string())?;
            }
        }
    }
    let file = tsx.file_name().unwrap().to_string_lossy().to_string();
    let src = std::fs::read_to_string(tsx).map_err(|e| e.to_string())?;
    std::fs::write(work.join("src").join(&file), rewrite_into(&src, &dir, &motion_src, "../motion", portable)?).map_err(|e| e.to_string())?;
    // every other source in the edit's folder too (components/captions.tsx importing @raea/motion): one copy of the SDK,
    // so a component's hooks read the Clock the engine drives
    let mut stack = vec![work.join("src")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| e.to_string())?.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if !matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs") || p == work.join("src").join(&file) {
                continue;
            }
            let rel = p.strip_prefix(work.join("src")).unwrap();
            let depth = rel.components().count() - 1;
            let to_motion = format!("{}motion", "../".repeat(depth + 1));
            let from_dir = dir.join(rel.parent().unwrap_or(Path::new("")));
            let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
            let rewritten = rewrite_into(&text, &from_dir, &motion_src, &to_motion, portable).map_err(|e| format!("{}: {e}", rel.display()))?;
            if rewritten != text {
                std::fs::write(&p, rewritten).map_err(|e| e.to_string())?;
            }
        }
    }
    let js = rdb::home().join("sdk/react/js");
    #[cfg(unix)]
    std::os::unix::fs::symlink(js.join("node_modules"), work.join("node_modules")).map_err(|e| format!("node_modules: {e}"))?;
    if o.lite {
        // the game's code and the SDK's resolve "react" to the live view runtime (tsconfig paths cover the files under
        // the work folder; packages in node_modules keep their own React)
        copy_dir(&js.join("lite"), &work.join("lite"), 0)?;
        let bridge = js.join("node_modules/bevy-react/src/bridge.ts");
        let index = std::fs::read_to_string(work.join("lite/index.ts")).map_err(|e| e.to_string())?.replace("\"../node_modules/bevy-react/src/bridge\"", &format!("{:?}", bridge.display().to_string()));
        std::fs::write(work.join("lite/index.ts"), index).map_err(|e| e.to_string())?;
        std::fs::write(work.join("tsconfig.json"), r#"{ "compilerOptions": { "jsx": "react-jsx", "jsxImportSource": "react", "strict": false, "module": "esnext", "target": "es2022", "moduleResolution": "bundler", "baseUrl": ".", "paths": { "react": ["./lite/index.ts"], "react/jsx-runtime": ["./lite/jsx-runtime.ts"], "react/jsx-dev-runtime": ["./lite/jsx-runtime.ts"] } } }"#).map_err(|e| e.to_string())?;
        std::fs::write(work.join("entry.tsx"), entry_lite(&file, o)).map_err(|e| e.to_string())?;
    } else {
        std::fs::copy(js.join("tsconfig.json"), work.join("tsconfig.json")).map_err(|e| e.to_string())?;
        std::fs::write(work.join("entry.tsx"), entry(&file, o)).map_err(|e| e.to_string())?;
    }
    let t_work = t0.elapsed();
    // in process by raea-bundle's library (Rolldown; a live bundle with its Z3 compile in Rust)
    let (js_text, css) = inproc::bundle(&work.join("entry.tsx"), serde_json::json!({"z3": o.lite, "edit": {"dir": dir, "js": js}})).map_err(|e| format!("couldn't bundle {file}: {}", e.lines().filter(|l| !l.trim().is_empty()).take(6).collect::<Vec<_>>().join(" | ")))?;
    // its CSS rides in the bundle's last line (bundle_css), so every copy and cache of it carries it
    let code = if css.is_empty() { js_text } else { format!("{js_text}\n//# raea-css={}\n", serde_json::to_string(&css).unwrap_or_default()) };
    if std::env::var_os("RAEA_BUNDLE_TRACE").is_some() {
        eprintln!("raea bundle: {} in {:.1} ms: the work folder {:.1} ms, Rolldown {:.1} ms", tsx.display(), t0.elapsed().as_secs_f64() * 1e3, t_work.as_secs_f64() * 1e3, (t0.elapsed() - t_work).as_secs_f64() * 1e3);
    }
    std::fs::write(out_js, code).map_err(|e| format!("{}: {e}", out_js.display()))?;
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

/// A bundled composition rendered by the engine, in this process (the Mac or the iPhone): React on V8 inside Bevy steps
/// its Clock; each frame's page goes through Blitz, its <Video> decoded natively, its 3D as rows; the frames become one
/// rdb program drawn by rdb into `out` (.mp4, .png for one frame, a folder of PNGs), or written as rdb SQL (.sql). `dir` is where relative media resolves. `wanted`: the frames (all when None).
/// A render's work folder, removed when the render ends (any way it ends but a kill).
struct Gone(PathBuf);
impl Drop for Gone {
    fn drop(&mut self) {
        // RAEA_MOTION_KEEP keeps it, to look at the pages
        if std::env::var_os("RAEA_MOTION_KEEP").is_none() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// Render work folders nothing has touched for two hours: left by a render that was killed.
fn sweep_stale_work() {
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else { return };
    let now = std::time::SystemTime::now();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("raea-motion-r") || name.starts_with("raea-motion-b")) {
            continue;
        }
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| now.duration_since(t).ok()).is_some_and(|d| d.as_secs() > 2 * 3600);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// The composition's sound alone, mixed and mastered as a render's is (its <Mix>'s loudness and ceiling through rdb's
/// master row): React is stepped through every frame for its sounds, with no page painted and no video decoded. Live
/// play hands this to the mixer, so a timeline sounds as its render does.
pub(crate) fn sound_only(js: &Path, dir: &Path) -> R<Option<raea_media::Audio>> {
    // the one mixer core: its bounce of the frames, mastered as the render's bounce is (core_master)
    let Some((cues, mix, p, rows, fps)) = core_cues(js, dir, false)? else { return Ok(None) };
    if cues.iter().all(|c| c.sounds.is_empty() && c.tone.graph.is_none() && c.tone.evs.is_empty() && c.tone.speech.is_none()) {
        return Ok(None);
    }
    Ok(Some(core_master(&cues, fps, mix, &p, &rows, dir)?.1))
}

/// The gain (dB) and ceiling (dBTP) the composition's render masters its sound with: a game's live voices go through the
/// same (its scripted run, played through every frame, as its render plays it). None without sound or a <Mix>.
pub(crate) fn master_gain(js: &Path, dir: &Path) -> R<Option<(f64, f64)>> {
    core_master_gain(js, dir)
}

/// The frame's 3D (each data-3d root in the page's order, its 3D children under it): what a render turns into rows.
fn scene_of(world: &mut World) -> Vec<Node3> {
    let now = SCENE3D.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    if now % 120 == 0 {
        PROPS3D.with(|c| c.borrow_mut().retain(|_, v| v.2 + 2 >= now));
    }
    let mut q = world.query::<(Entity, &ReactKind, Option<&ReactProps>)>();
    let Some(surf) = q.iter(world).find(|(_, k, p)| k.0 == "surface" && p.and_then(|p| p.0.get("id")).and_then(|v| v.as_str()) == Some("motion")).map(|(e, _, _)| e) else { return Vec::new() };
    fn walk(world: &World, e: Entity, out: &mut Vec<Node3>) {
        if world.get::<ReactProps>(e).is_some_and(|p| p.0.contains_key("data-3d")) {
            if let Some(n) = read3d(world, e) {
                out.push(n);
            }
            return;
        }
        if let Some(c) = world.get::<Children>(e) {
            for k in c.iter() {
                walk(world, k, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(world, surf, &mut out);
    out
}

/// The page elements holding the frame's 3D (each 3D root's ancestors up to the surface, nearest first) with their
/// props: a render clears their backgrounds (the 3D is drawn under the page) and the nearest one's colour is the 3D's.
/// The elements holding the 3D (holders_of's), into `out` (ZeroHeap: live play's present asks each frame)
pub(crate) fn holder_entities(world: &mut World, out: &mut Vec<Entity>) {
    out.clear();
    with_holder_query(world, |world, q| {
        let is3d = |w: &World, e: Entity| w.get::<ReactProps>(e).is_some_and(|p| p.0.contains_key("data-3d"));
        for (r, p, c) in q.0.iter(world) {
            if !p.0.contains_key("data-3d") || c.is_some_and(|c| is3d(world, c.parent())) {
                continue;
            }
            let mut at = world.get::<ChildOf>(r).map(|c| c.parent());
            while let Some(e) = at {
                if world.get::<ReactKind>(e).is_some_and(|k| k.0 == "surface") {
                    break;
                }
                if !out.contains(&e) {
                    out.push(e);
                }
                at = world.get::<ChildOf>(e).map(|c| c.parent());
            }
        }
    })
}

#[derive(Resource)]
struct HolderQuery(QueryState<(Entity, &'static ReactProps, Option<&'static ChildOf>)>);

fn with_holder_query<T>(world: &mut World, f: impl FnOnce(&mut World, &mut HolderQuery) -> T) -> T {
    if !world.contains_resource::<HolderQuery>() {
        let q = world.query();
        world.insert_resource(HolderQuery(q));
    }
    world.resource_scope(|world, mut q: Mut<HolderQuery>| f(world, &mut q))
}

pub(crate) fn holders_of(world: &mut World) -> Vec<(Entity, Map<String, Value>)> {
    let mut q = world.query::<(Entity, &ReactProps, Option<&ChildOf>)>();
    let is3d = |w: &World, e: Entity| w.get::<ReactProps>(e).is_some_and(|p| p.0.contains_key("data-3d"));
    let roots: Vec<Entity> = q.iter(world).filter(|(_, p, c)| p.0.contains_key("data-3d") && !c.is_some_and(|c| is3d(world, c.parent()))).map(|(e, _, _)| e).collect();
    let mut out: Vec<(Entity, Map<String, Value>)> = Vec::new();
    for r in roots {
        let mut at = world.get::<ChildOf>(r).map(|c| c.parent());
        while let Some(e) = at {
            if world.get::<ReactKind>(e).is_some_and(|k| k.0 == "surface") {
                break;
            }
            if !out.iter().any(|(x, _)| *x == e) {
                out.push((e, world.get::<ReactProps>(e).map(|p| p.0.clone()).unwrap_or_default()));
            }
            at = world.get::<ChildOf>(e).map(|c| c.parent());
        }
    }
    out
}

/// A holder of the 3D made see-through, as every path that draws the 3D under the page does: its inline background
/// goes (the nearest one's colour is the 3D's clear colour, returned here), and when it had one or has a class, an
/// inline transparent background and no image override whatever a class gives it. Before, only an inline background
/// went, so `className="bg-brand-bg"` (the brand's own utility) painted an opaque page over the 3D and every frame
/// rendered as the background colour alone (2026-10-18, void's agent's perfume shot, black for 24 renders). Whether
/// the props changed, and the clear colour.
pub(crate) fn see_through(p: &mut Map<String, Value>) -> (bool, Option<String>) {
    let classed = p.get("className").and_then(Value::as_str).is_some_and(|c| !c.trim().is_empty());
    let style = p.entry("style").or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(st) = style else { return (false, None) };
    let mut clear = None;
    let mut had = false;
    for k in ["background", "backgroundColor"] {
        if let Some(v) = st.remove(k) {
            had = true;
            if clear.is_none() {
                clear = v.as_str().map(str::to_string);
            }
        }
    }
    if had || classed {
        st.insert("backgroundColor".into(), Value::String("transparent".into()));
        st.insert("backgroundImage".into(), Value::String("none".into()));
    }
    (had || classed, clear)
}

/// The slot showing texture `id` (the 3D's frame, w x h) where the page paints it (blitz-paint draw_texture).
pub(crate) fn set_scene_slot(dom: &mut raea::dom::Dom, node: raea::dom::NodeId, id: u64, w: u32, h: u32) {
    for (k, v) in [("data-raea-texture", id.to_string()), ("data-raea-w", w.to_string()), ("data-raea-h", h.to_string())] {
        dom.set_attribute(node, k, &v);
    }
}

/// A layered Canvas3D's slot: the `<img data-raea-scene>` it renders, whose place in the page's paint order the 3D's
/// frame takes (as a texture), so page elements before it in the tree are under the 3D and those after it over it.
pub(crate) fn scene_slot(world: &mut World) -> Option<u64> {
    let mut q = world.query::<(&ReactProps, Option<&crate::ReactNode>)>();
    q.iter(world).find(|(p, _)| p.0.contains_key("data-raea-scene")).and_then(|(_, n)| n.map(|n| n.0))
}

/// The 3D's clear colour for a render or play: transparent (#00000000) for a layered Canvas3D, whose background is the
/// page under it; else the nearest holder's background.
pub(crate) fn scene_clear(world: &mut World) -> Option<String> {
    if scene_slot(world).is_some() {
        return Some("#00000000".into());
    }
    clear_of(&holders_of(world))
}

/// The 3D's clear colour: the nearest holder's background, as a render takes it.
pub(crate) fn clear_of(holders: &[(Entity, Map<String, Value>)]) -> Option<String> {
    holders.iter().find_map(|(_, p)| p.get("style").and_then(|s| s.as_object()).and_then(|st| st.get("background").or(st.get("backgroundColor"))).and_then(|v| v.as_str()).map(str::to_string))
}

/// A composition's 3D as the rows its render draws (camera, lights, meshes, their motion keyed by frame): React stepped
/// through every frame for its 3D, no page painted. None when it has none. Live play draws a 3D scene from this
/// (rdb's engine, live), so the scene played is the scene rendered.
pub(crate) fn scene_program(js: &Path, dir: &Path) -> R<Option<rdb::ir::Program>> {
    scene_program_to(js, dir, 0)
}

/// `scene_program` stepped through frame `upto - 1` at least: a game's frame past the composition's length (it plays on
/// past it, live) has its 3D, not the base pose a frame past the program's last shows.
pub(crate) fn scene_program_to(js: &Path, dir: &Path, upto: i64) -> R<Option<rdb::ir::Program>> {
    Ok(scene_frames_to(js, dir, upto)?.map(|(frames, w, h, fps)| program(&frames, w, h, fps)))
}

/// A render's 3D rows as `scene_program_to` makes them, and the 3D parts that pass through each other on screen in
/// `wanted` (one frame, a still; else all): said on stderr, and kept for the render's manifest (FED_CLIPPING).
fn scene_program_checked(js: &Path, dir: &Path, upto: i64, wanted: Option<&[i64]>) -> R<Option<rdb::ir::Program>> {
    crate::startup_mark("the 3D's rows: stepping React through the frames");
    let Some((frames, w, h, fps)) = scene_frames_to(js, dir, upto)? else { return Ok(None) };
    crate::startup_mark("the 3D's rows: frames stepped");
    let p = program(&frames, w, h, fps);
    crate::startup_mark("the 3D's rows: program made");
    // a still: its own frame (the frames are 0..upto in order)
    let seen = match wanted {
        Some(&[f]) if f >= 0 && (f as usize) < frames.len() => f as usize..f as usize + 1,
        _ => 0..frames.len(),
    };
    // the clipping check on a thread of its own while the engine opens and the frames render (3 s over the Rome
    // film's 8,064 frames, which the render waited for before its engine opened); the render joins it at its end
    struct Prepassed(Vec<(i64, Frame)>);
    // SAFETY: these frames were stepped for their 3D alone: no page was painted, so none holds a Painting (the one
    // field that isn't Send), and the thread is their only owner
    unsafe impl Send for Prepassed {}
    let owned = Prepassed(frames);
    *FED_CLIPPING.lock().unwrap() = Some(std::thread::spawn(move || {
        let o = owned;
        scene3d::clip3d::told(&o.0[seen], w, h, fps)
    }));
    Ok(Some(p))
}

/// What `scene_program_checked` finds (on its own thread), for the fed render that draws that scene to write in its
/// manifest.
static FED_CLIPPING: std::sync::Mutex<Option<std::thread::JoinHandle<Vec<Value>>>> = std::sync::Mutex::new(None);

/// RAEA_FRAME_LOG=FILE: a fed render (an .mp4 or .mov of a piece with 3D; not --frame or --at, which write none) writes
/// one line a frame (ms, tab-separated): frame, react (the frame's commit
/// settled), sync (the props read, videos placed, the 3D's holders), draw3d (Bevy's update for the frame: its systems,
/// extract, prepare, queue and the passes recorded and submitted), page (Blitz's style, layout and paint recorded and
/// submitted over the 3D), gpu (the wait for the frame's GPU work, in rdb). RAEA_FRAME_LOG_SERIAL=1 waits for the GPU
/// after the 3D and after the page as well (gpu3d, gpupage), so a frame's GPU time splits by stage; the render then runs
/// one stage at a time, slower, so it's for attribution, not for timing the whole.
static FRAME_LOG: std::sync::Mutex<Option<FrameLog>> = std::sync::Mutex::new(None);

#[derive(Default)]
struct FrameLog {
    serial: bool,
    /// this frame's: sync, draw3d, gpu3d, page, gpupage
    cur: [f64; 5],
    rows: Vec<(i64, f64, [f64; 5])>,
}

fn frame_log_on() -> bool {
    FRAME_LOG.lock().unwrap().is_some()
}

/// RAEA_FRAME_LOG_SERIAL: wait for every GPU submission so far (one device: the page's, Bevy's and rdb's)
fn frame_log_gpu_wait(slot: usize) {
    let serial = FRAME_LOG.lock().unwrap().as_ref().is_some_and(|l| l.serial);
    if !serial {
        return;
    }
    let t = std::time::Instant::now();
    if let Ok((_, _, device, _)) = raea::quads::handles() {
        let _ = device.poll(bevy::render::render_resource::PollType::wait_indefinitely());
    }
    if let Some(l) = FRAME_LOG.lock().unwrap().as_mut() {
        l.cur[slot] += t.elapsed().as_secs_f64() * 1e3;
    }
}

fn frame_log_add(slot: usize, t: std::time::Instant) {
    if let Some(l) = FRAME_LOG.lock().unwrap().as_mut() {
        l.cur[slot] += t.elapsed().as_secs_f64() * 1e3;
    }
}

/// The frames `scene_program_to` reads its rows from, with the composition's size and rate.
///
/// Stepped as a bake steps (bake.rs, lightspeed T3): React renders a range of frames back to back (raea-frames) and
/// each frame's commit is applied on its own (OneBatch, PreUpdate only), its scene read after it; the frames split
/// across runtimes on threads, since frames are pure (each runtime's first frame is its whole tree). One event a frame,
/// each waited on the JS thread and back, took the Rome film's 8,064 frames 6 s on a quiet machine and 38 s at load
/// 50, before its engine could open. RAEA_BAKE_WORKERS sets how many runtimes (4; one for short timelines).
fn scene_frames_to(js: &Path, dir: &Path, upto: i64) -> R<Option<(Vec<(i64, Frame)>, u32, u32, f64)>> {
    scene3d::set_media_dir(dir);
    let runtime = |js: &Path| {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
        app.insert_resource(crate::OneBatch::default());
        app
    };
    let mut app = runtime(js);
    let conf = settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    let (w, h) = (num(&conf, "width").unwrap_or(1080.0) as u32, num(&conf, "height").unwrap_or(1920.0) as u32);
    let fps = num(&conf, "fps").unwrap_or(30.0);
    let total = (num(&conf, "frames").unwrap_or(60.0) as i64).max(upto);
    type Scenes = Vec<(i64, Vec<Node3>, Option<String>)>;
    let step = |app: &mut App, a: i64, b: i64| -> R<Scenes> {
        let mut out: Scenes = Vec::with_capacity((b - a).max(0) as usize);
        crate::bake::step_frames(app, a, b, |app, f| {
            let scene = scene_of(app.world_mut());
            let clear = scene_clear(app.world_mut());
            out.push((f, scene, clear));
            Ok(())
        })?;
        Ok(out)
    };
    let workers = std::env::var("RAEA_BAKE_WORKERS").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(4).clamp(1, 16).min((total / 240).max(1));
    let span = (total + workers - 1) / workers;
    let ranges: Vec<(i64, i64)> = (0..workers).map(|k| (k * span, ((k + 1) * span).min(total))).filter(|(a, b)| a < b).collect();
    let later: Vec<std::thread::JoinHandle<R<Scenes>>> = ranges
        .iter()
        .skip(1)
        .map(|&(a, b)| {
            let js = js.to_path_buf();
            let dir = dir.to_path_buf();
            std::thread::Builder::new()
                .name("raea scene rows".into())
                .spawn(move || {
                    scene3d::set_media_dir(&dir);
                    let mut app = runtime(&js);
                    settle(&mut app, Some(-1), 20.0)?;
                    step(&mut app, a, b)
                })
                .map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    let mut scenes = match ranges.first() {
        Some(&(a, b)) => step(&mut app, a, b)?,
        None => Vec::new(),
    };
    for t in later {
        scenes.extend(t.join().map_err(|_| "a scene-rows runtime panicked".to_string())??);
    }
    let frames: Vec<(i64, Frame)> = scenes.into_iter().map(|(f, scene, clear)| (f, Frame { mix: None, page: None, sounds: Vec::new(), scene, clear, pending: None, same: false })).collect();
    if frames.iter().all(|(_, f)| f.scene.is_empty()) {
        return Ok(None);
    }
    Ok(Some((frames, w, h, fps)))
}

/// A live game's 3D (Live): the frame just stepped's scene, as play computed it, made into rows for rdb's live engine:
/// the engine opened at the first frame with any 3D (from that frame's rows, as a render's program holds them there),
/// then each frame only what changed set on it (scene3d::LiveScene). A new page size starts the engine again.
pub(crate) fn live_scene(world: &mut World, ls: &mut LiveScene, engine: &mut Option<rdb::engine::Engine>, size: (u32, u32), fps: f64, dir: &Path) -> R<()> {
    if engine.as_ref().is_some_and(|e| e.size() != size) {
        *engine = None;
        ls.reset();
    }
    let scene = scene_of(world);
    if scene.is_empty() && engine.is_none() {
        return Ok(());
    }
    scene3d::set_media_dir(dir);
    match engine {
        None => {
            let clear = scene_clear(world);
            let f = (0, Frame { mix: None, page: None, sounds: Vec::new(), scene, clear, pending: None, same: false });
            let p = ls.open(&f, size.0, size.1, fps);
            let mut e = rdb::engine::Engine::new_live(&p, &rdb::inserted_of(&p), dir).map_err(|e| format!("the 3D didn't open: {e}"))?;
            e.set_rows(&[])?;
            *engine = Some(e);
        }
        Some(e) => {
            let f = (0, Frame { mix: None, page: None, sounds: Vec::new(), scene, clear: None, pending: None, same: false });
            let (rows, gone) = ls.frame(&f);
            if !gone.is_empty() {
                e.remove_rows(&gone);
            }
            if !rows.is_empty() {
                e.set_rows(&rows)?;
            }
        }
    }
    Ok(())
}

pub(crate) use scene3d::LiveScene;

pub(crate) fn settle_frame(app: &mut App, want: Option<i64>, secs: f64) -> R<Option<Map<String, Value>>> {
    settle(app, want, secs)
}

/// The render with its pages handed to rdb on the GPU: rdb asks for each frame in order, the frame is stepped and its
/// page painted into one page buffer on the paint device, and rdb composites it as it is and encodes it (no page
/// file, no page read back; gate 5 of no-V8). The sound is the frames' sound rows, mixed once the last frame is made.
#[allow(clippy::too_many_arguments)]
fn render_fed(app: &mut App, w: u32, h: u32, fps: f64, total: i64, retained: bool, cx: &mut Ctx, out: &Path, work: &Path, wanted: &[i64]) -> R<String> {
    if !retained {
        return Err(FED_3D.into());
    }
    let page = raea::quads::page_buffer(w, h)?;
    cx.fed = Some(page.clone());
    // a still of a 3D scene (render --frame N -o F.png): the page painted over the scene's frame on the GPU and read
    // back as RGB, as the video render and play make it (the page-file still blended the page over the 3D in Bevy:
    // SSIM 0.958 against the video's frame, 0.995 for play)
    if out.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")) {
        for &f in wanted {
            app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": f}));
            settle(app, Some(f), 20.0)?;
            cx.now = f;
            snapshot_retained(app.world_mut(), w, h, cx)?.ok_or(FED_3D)?;
        }
        let px = raea::quads::read_buffer(&page)?;
        cx.fed = None;
        image::save_buffer(out, &px, w, h, image::ColorType::Rgba8).map_err(|e| format!("{}: {e}", out.display()))?;
        // the still's own clipping check, said on stderr before the process ends
        if let Some(t) = FED_CLIPPING.lock().unwrap().take() {
            let _ = t.join();
        }
        return Ok(format!("APPLIED  frame {} rendered to {} (the page over its 3D on the GPU)", wanted.last().copied().unwrap_or(0), out.display()));
    }
    let frames: std::cell::RefCell<Vec<(i64, Frame)>> = std::cell::RefCell::new(Vec::with_capacity(total.max(0) as usize));
    // the sound is the one mixer core's bounce of the frames' sounds and notes, as live play mixes the same frames
    // (raea-mixer)
    let notes: std::cell::RefCell<Vec<raea_mixer::ToneCue>> = std::cell::RefCell::new(Vec::new());
    let dir = cx.dir.to_path_buf();
    // the sound bounced as the frames come, on its own thread (it was mixed after the last frame: 3 s on the export)
    let streamer = std::cell::RefCell::new(Some(raea_mixer::offline::Streamer::start(fps)));
    let t0 = std::time::Instant::now();
    let pic = rdb::ir::Program { fps, width: w, height: h, frames: total, ..Default::default() };
    let frame_log = std::env::var_os("RAEA_FRAME_LOG").map(PathBuf::from);
    if frame_log.is_some() {
        *FRAME_LOG.lock().unwrap() = Some(FrameLog { serial: std::env::var("RAEA_FRAME_LOG_SERIAL").as_deref() == Ok("1"), ..Default::default() });
        *rdb::gpuview::FRAME_WAITS.lock().unwrap() = Some(Vec::new());
    }
    // the frame React was last asked for ahead of its turn (the render loop below)
    let mut asked: Option<i64> = None;
    // RAEA_REACT_AHEAD=0: each frame asked for in its turn, as before
    let ahead = std::env::var("RAEA_REACT_AHEAD").as_deref() != Ok("0");
    let r = rdb::render_fed(
        &pic,
        work,
        out,
        &render_opts(),
        |f| {
            if f == 0 {
                crate::startup_mark("the fed render's first frame");
            }
            let t_react = std::time::Instant::now();
            if asked != Some(f) {
                app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": f}));
            }
            settle(app, Some(f), 20.0)?;
            cx.now = f;
            // the next frame asked for now, so React renders it on its own thread while this one's 3D and page are
            // drawn here: its commits wait in the inbox until the next settle applies them, so nothing this frame
            // reads changes (the frames come in order; one asked for out of order is asked for again)
            asked = (f + 1 < total && ahead).then_some(f + 1);
            if let Some(g) = asked {
                app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": g}));
            }
            let react_ms = t_react.elapsed().as_secs_f64() * 1e3;
            let t_snap = std::time::Instant::now();
            let fr = snapshot_retained(app.world_mut(), w, h, cx)?.ok_or(FED_3D)?;
            if let Some(l) = FRAME_LOG.lock().unwrap().as_mut() {
                let mut cur = std::mem::take(&mut l.cur);
                // sync: the snapshot's time less the 3D's and the page's (and the serial waits)
                cur[0] = t_snap.elapsed().as_secs_f64() * 1e3 - cur[1] - cur[2] - cur[3] - cur[4];
                l.rows.push((f, react_ms, cur));
            }
            let tone = crate::audio::tone_at(app.world_mut(), f, fps, &dir, media_path, true);
            if let Some(st) = streamer.borrow().as_ref() {
                st.push(raea_mixer::offline::Cue { sounds: fr.sounds.iter().map(core_sound).collect(), tone: tone.clone() });
            }
            notes.borrow_mut().push(tone);
            frames.borrow_mut().push((f, fr));
            Ok(page.clone())
        },
        || {
            let p = program(&frames.borrow(), w, h, fps);
            sound_lint(&p, fps);
            let st = streamer.borrow_mut().take().ok_or("the sound was asked for twice")?;
            let t_bounce = std::time::Instant::now();
            let raw = st.finish()?;
            let bounce_ms = t_bounce.elapsed().as_secs_f64() * 1e3;
            let t_master = std::time::Instant::now();
            if frames.borrow().iter().all(|(_, f)| f.sounds.is_empty()) && notes.borrow().iter().all(|n| n.is_empty()) {
                no_sound_but_declared(&dir)?;
                return Ok(None);
            }
            let (master, a) = core_master_raw(&raw, frames.borrow().iter().find_map(|(_, f)| f.mix))?;
            if let Some(p) = std::env::var_os("RAEA_AUDIO_BOUNCE") {
                raea_mixer::write_wav(&a, Path::new(&p))?;
            }
            if std::env::var_os("RAEA_MASTER_TRACE").is_some() {
                eprintln!("core bounce (streamed): {:.2} s of sound, master {master:?}; the bounce finished {bounce_ms:.0} ms after the last frame, the master took {:.0} ms", a.duration(), t_master.elapsed().as_secs_f64() * 1e3);
            }
            Ok(Some(a))
        },
    );
    cx.fed = None;
    if let Some(path) = frame_log {
        let log = FRAME_LOG.lock().unwrap().take().unwrap_or_default();
        let waits: HashMap<i64, f64> = rdb::gpuview::FRAME_WAITS.lock().unwrap().take().unwrap_or_default().into_iter().collect();
        let mut out = String::from("frame\treact\tsync\tdraw3d\tgpu3d\tpage\tgpupage\tgpu\n");
        for (f, react, c) in &log.rows {
            out += &format!("{f}\t{react:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\n", c[0], c[1], c[2], c[3], c[4], waits.get(f).copied().unwrap_or(0.0));
        }
        std::fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    r?;
    let n = frames.borrow().len();
    // 3D parts passing through each other on screen: on stderr, and in the manifest for void's checks
    let found = FED_CLIPPING.lock().unwrap().take().and_then(|t| t.join().ok());
    let clipping = found.unwrap_or_else(|| scene3d::clip3d::told(&frames.borrow(), w, h, fps));
    if let (Some(path), Some(frames_seen)) = (MANIFEST.lock().unwrap().clone(), cx.manifest.take()) {
        std::fs::write(&path, json!({"width": w, "height": h, "fps": fps, "frames": frames_seen, "clipping": clipping}).to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let ms = t0.elapsed().as_secs_f64() * 1e3 / n.max(1) as f64;
    Ok(format!("APPLIED  {n} frames rendered to {} ({ms:.1} ms a frame, pages handed to rdb on the GPU)", out.display()))
}

/// The frames' sound bounced through the one mixer core (raea-mixer), mastered as live play masters it: the gain rdb
/// works out for the <Mix> (what a game's live voices get, motion::master_gain) and the limiter at its ceiling; without
/// a <Mix>, the limiter at -1 dBFS. None when nothing sounds.
fn core_bounce(frames: &[(i64, Frame)], notes: &[raea_mixer::ToneCue], fps: f64, p: &rdb::ir::Program, rows: &[rdb::eval::Tup], dir: &Path) -> R<Option<raea_media::Audio>> {
    if frames.iter().all(|(_, f)| f.sounds.is_empty()) && notes.iter().all(|n| n.is_empty()) {
        return Ok(None);
    }
    let cues: Vec<raea_mixer::offline::Cue> = frames
        .iter()
        .enumerate()
        .map(|(i, (_, f))| raea_mixer::offline::Cue {
            sounds: f.sounds.iter().map(|s| raea_mixer::Sound { key: s.key.as_str().into(), file: Path::new(&s.src).into(), at: s.at, volume: s.volume as f32, rate: s.rate, fade_in: s.fade_in.unwrap_or(0.0), fade_out: s.fade_out.unwrap_or(0.0), bus: s.bus.as_deref().map(Into::into) }).collect(),
            tone: notes.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    sound_lint(p, fps);
    let t = std::time::Instant::now();
    let (master, a) = core_master(&cues, fps, frames.iter().find_map(|(_, f)| f.mix), p, rows, dir)?;
    // RAEA_AUDIO_BOUNCE=OUT.wav: the bounce as it left the core (before the encoder), to hold against live play's tap
    if let Some(p) = std::env::var_os("RAEA_AUDIO_BOUNCE") {
        raea_mixer::write_wav(&a, Path::new(&p))?;
    }
    if std::env::var_os("RAEA_MASTER_TRACE").is_some() {
        eprintln!("core bounce: {:.2} s of sound in {:.0} ms, master {master:?}", a.duration(), t.elapsed().as_secs_f64() * 1e3);
    }
    Ok(Some(a))
}

/// A render that would have no sound although the edit writes <Audio>: an error, not a silent file. A wrapper that drops
/// its children (a <Mix> did, 2026-10-18: a 20 s ad rendered with no audio track), a sound that's never on screen, or
/// a generated sound that was never made all end here. The edit's sources are read: edit.tsx, the folder's other .tsx
/// files and components/.
fn no_sound_but_declared(dir: &Path) -> R<()> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    files.extend(std::fs::read_dir(dir.join("components")).into_iter().flatten().flatten().map(|e| e.path()));
    let declares = files.iter().filter(|p| p.extension().is_some_and(|x| x == "tsx")).filter_map(|p| std::fs::read_to_string(p).ok()).any(|t| t.contains("<Audio"));
    if declares {
        return Err("the edit has <Audio> but no sound reached the render, so the file would be silent: is an <Audio> inside a component that doesn't render its children, outside every frame's Sequence, or a generated sound that wasn't made? (an <Audio> inside <Mix>...</Mix> was dropped before 2026-10-18)".into());
    }
    Ok(())
}

fn core_sound(s: &Sound) -> raea_mixer::Sound {
    raea_mixer::Sound { key: s.key.as_str().into(), file: Path::new(&s.src).into(), at: s.at, volume: s.volume as f32, rate: s.rate, fade_in: s.fade_in.unwrap_or(0.0), fade_out: s.fade_out.unwrap_or(0.0), bus: s.bus.as_deref().map(Into::into) }
}

/// The one mixer core's master for these frames, and their bounce through it: with a <Mix>, the gain rdb works out
/// for it and its ceiling, held after AAC on Apple as the render's master is (master_for): where the encoder lifts the
/// true peak over the ceiling, the limiter's ceiling goes down by the overshoot and the frames are bounced again (twice
/// at most). Live play is given the same (master-gain), so its tap is still the bounce.
fn core_master(cues: &[raea_mixer::offline::Cue], fps: f64, mix: Option<(f64, f64)>, _p: &rdb::ir::Program, _rows: &[rdb::eval::Tup], _dir: &Path) -> R<(Option<(f64, f64)>, raea_media::Audio)> {
    core_master_raw(&raea_mixer::offline::bounce_raw(cues, fps)?, mix)
}

/// The master over a raw bounce (raea_mixer::offline::master_raw: the limiter live play's master node runs): the gain
/// to the <Mix>'s loudness from the raw sound, and on Apple the ceiling lowered by any AAC overshoot. Each try is a
/// pass of the limiter over the buffer, not another bounce.
fn core_master_raw(raw: &raea_media::Audio, mix: Option<(f64, f64)>) -> R<(Option<(f64, f64)>, raea_media::Audio)> {
    #[cfg(target_vendor = "apple")]
    let enc: Option<&dyn Fn(&raea_media::Audio) -> R<raea_media::Audio>> = Some(&raea_media::apple::aac_round_trip);
    #[cfg(not(target_vendor = "apple"))]
    let enc: Option<&dyn Fn(&raea_media::Audio) -> R<raea_media::Audio>> = None;
    Ok(raea_mixer::offline::master_mix(raw, mix, enc))
}

/// master_gain with the one mixer core: core_master's gain and ceiling for the frames, the ones the render's bounce uses.
fn core_master_gain(js: &Path, dir: &Path) -> R<Option<(f64, f64)>> {
    let Some((cues, mix, p, rows, fps)) = core_cues(js, dir, true)? else { return Ok(None) };
    Ok(core_master(&cues, fps, mix, &p, &rows, dir)?.0)
}

/// React stepped through every frame for its sounds and notes, as the render steps it: the frames' cues, whether there's
/// a <Mix>, their rdb program and rows, and the fps. With `need_mix`, None at once when the first frame has no <Mix>.
#[allow(clippy::type_complexity)]
fn core_cues(js: &Path, dir: &Path, need_mix: bool) -> R<Option<(Vec<raea_mixer::offline::Cue>, Option<(f64, f64)>, rdb::ir::Program, Vec<rdb::eval::Tup>, f64)>> {
    // its lines (@raea/motion/speech) are made when a live play isn't waiting for its own
    #[cfg(feature = "ml")]
    raea_ml::speech::set_background(true);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
    let conf = settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    let (w, h) = (num(&conf, "width").unwrap_or(1080.0) as u32, num(&conf, "height").unwrap_or(1920.0) as u32);
    let fps = num(&conf, "fps").unwrap_or(30.0);
    let total = num(&conf, "frames").unwrap_or(60.0) as i64;
    let work = std::env::temp_dir();
    let mut cx = Ctx { dir, work: &work, decoders: HashMap::new(), stills: HashMap::new(), sounds: Vec::new(), scene: Vec::new(), clear: None, marker: None, mix: None, holders: Vec::new(), audible: HashMap::new(), brand: None, manifest: None, now: 0, made: HashMap::new(), dom: None, root: None, shown: HashMap::new(), painted: false, fed: None, scene3d: None, live3d: None, textures: false, vframes: VideoFrames::new() };
    let (mut frames, mut notes) = (Vec::new(), Vec::new());
    for f in 0..total {
        app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": f}));
        settle(&mut app, Some(f), 20.0)?;
        cx.now = f;
        cx.sounds.clear();
        cx.mix = None;
        let world = app.world_mut();
        let mut q = world.query::<(&ReactKind, Option<&ReactProps>, Option<&crate::ReactNode>)>();
        let items: Vec<(String, Map<String, Value>, Option<u64>)> = q.iter(world).map(|(k, p, n)| (k.0.clone(), p.map(|p| p.0.clone()).unwrap_or_default(), n.map(|n| n.0))).collect();
        for (kind, mut props, node) in items {
            if props.get("data-mix").and_then(|v| v.as_str()) == Some("true") {
                cx.mix = Some((num(&props, "data-lufs").unwrap_or(-14.0), num(&props, "data-ceiling").unwrap_or(-1.5)));
                continue;
            }
            if (kind == "audio" || kind == "video") && generated(&mut props, &mut cx) {
                push_sound(&mut cx, &props, &node.map(|n| format!("n{n}")).unwrap_or_default());
            }
        }
        if need_mix && f == 0 && cx.mix.is_none() {
            return Ok(None);
        }
        notes.push(crate::audio::tone_at(app.world_mut(), f, fps, dir, media_path, true));
        frames.push((f, Frame { mix: cx.mix, page: None, sounds: cx.sounds.clone(), scene: Vec::new(), clear: None, pending: None, same: false }));
    }
    drop(app);
    let p = program(&frames, w, h, fps);
    let rows = rdb::inserted_of(&p);
    let cues: Vec<raea_mixer::offline::Cue> = frames
        .iter()
        .zip(&notes)
        .map(|((_, f), n)| raea_mixer::offline::Cue {
            sounds: f.sounds.iter().map(|s| raea_mixer::Sound { key: s.key.as_str().into(), file: Path::new(&s.src).into(), at: s.at, volume: s.volume as f32, rate: s.rate, fade_in: s.fade_in.unwrap_or(0.0), fade_out: s.fade_out.unwrap_or(0.0), bus: s.bus.as_deref().map(Into::into) }).collect(),
            tone: n.clone(),
        })
        .collect();
    let mix = frames.iter().find_map(|(_, f)| f.mix);
    Ok(Some((cues, mix, p, rows, fps)))
}

/// A fed render met a 3D frame it has no scene for: it starts again with the scene's rows (or with page files).
const FED_3D: &str = "a 3D frame: the render goes through page files";

/// The texture a fed render's 3D is drawn into for the page to go over (live.rs keeps its own for play).
const FED_SCENE_TEXTURE: u64 = u64::MAX - 5;

pub fn render_bundle(js: &Path, dir: &Path, out: &Path, wanted: Option<Vec<i64>>) -> R<String> {
    // the pages handed to rdb on the GPU (render_fed) for a whole video with no 3D, unless RAEA_MOTION_PAGES=png;
    // a 3D frame found on the way starts the render again with page files
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let fed = wanted.is_none() && matches!(ext.as_str(), "mp4" | "mov" | "txt") && std::env::var("RAEA_MOTION_PAGES").as_deref() != Ok("png")
        && std::env::var("RAEA_MOTION_RETAINED").as_deref() != Ok("0") && rdb::gpuview::paint_device().is_some();
    // a still of a 3D scene: through the fed path too, so it blends as the video and play do (2D stills already match)
    if ext == "png" && wanted.as_ref().is_some_and(|w| w.len() == 1) && std::env::var("RAEA_MOTION_3D").as_deref() != Ok("files") && rdb::gpuview::paint_device().is_some() {
        let upto = wanted.as_ref().and_then(|w| w.iter().max()).map_or(0, |f| f + 1);
        if let Some(scene) = scene_program_checked(js, dir, upto, wanted.as_deref())? {
            return render_bundle_with(js, dir, out, wanted, true, Some(scene), false);
        }
    }
    if fed {
        match render_bundle_with(js, dir, out, None, true, None, false) {
            Err(e) if e == FED_3D => {}
            r => return r,
        }
        crate::startup_mark("a 3D frame met: the render starts again with its 3D");
        // 3D: its rows (the bundle stepped once more for them), drawn live under each fed page as play draws them;
        // RAEA_MOTION_3D=files keeps 3D renders on page files
        let how = std::env::var("RAEA_MOTION_3D").unwrap_or_default();
        // a game: its 3D made frame by frame as it's stepped, as play draws it. Rows made beforehand step React
        // through the whole composition and hold every frame's scene: a 10-minute game took 24 GB before its first
        // frame was drawn, however few frames were asked for (The Hero's Journey, 2026-10-20). RAEA_MOTION_3D=live
        // or =program chooses
        if how == "live" || (how != "program" && how != "files" && is_game(js)) {
            match render_bundle_with(js, dir, out, None, true, None, true) {
                Err(e) if e == FED_3D => {}
                r => return r,
            }
        } else if how != "files" {
            if let Some(scene) = scene_program_checked(js, dir, 0, None)? {
                match render_bundle_with(js, dir, out, None, true, Some(scene), false) {
                    Err(e) if e == FED_3D => {}
                    r => return r,
                }
            }
        }
    }
    render_bundle_with(js, dir, out, wanted, false, None, false)
}

/// A game's bundle: it steps state with useGameState (bundles aren't minified, and what isn't used is left out).
fn is_game(js: &Path) -> bool {
    std::fs::read_to_string(js).is_ok_and(|t| t.contains("useGameState("))
}

fn render_bundle_with(js: &Path, dir: &Path, out: &Path, wanted: Option<Vec<i64>>, fed: bool, scene: Option<rdb::ir::Program>, live3d: bool) -> R<String> {
    scene3d::set_media_dir(dir);
    #[cfg(feature = "ml")]
    crate::native::ml::set_dir(dir);
    // RAEA_MOTION_KEEP=DIR: the work folder is DIR, and stays (a folder of its own: renders run side by side, and a
    // newest-folder guess picked up another session's render)
    let keep_dir = std::env::var("RAEA_MOTION_KEEP").ok().filter(|v| v.starts_with('/')).map(PathBuf::from);
    let work = keep_dir.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("raea-motion-r{}-{:?}", std::process::id(), std::thread::current().id())));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    // the work folder goes however the render ends; one a killed render left (memguard's SIGKILL runs no cleanup) goes
    // when it's two hours stale. On 2026-10-17, 169 left behind (several GB each with fast pages) filled the disk
    let _gone = Gone(work.clone());
    sweep_stale_work();
    // React on V8 inside Bevy: the first commit says the composition's size and length
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
    // the retained page React's commits patch (phase 1 of One renderer); RAEA_MOTION_RETAINED=0 rebuilds each frame
    let retained = std::env::var("RAEA_MOTION_RETAINED").as_deref() != Ok("0");
    if retained {
        app.insert_non_send(crate::DomMirror::new_page(1080, 1920, dir));
    }
    let conf = settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    let (mut w, mut h) = (num(&conf, "width").unwrap_or(1080.0) as u32, num(&conf, "height").unwrap_or(1920.0) as u32);
    // --stage WxH: the page at the stage's size, the composition told it (raea-size), as live.rs opens it for void
    let stage = *STAGE.lock().unwrap();
    if let Some((sw, sh)) = stage {
        (w, h) = (sw, sh);
        app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-size", "value": {"w": w, "h": h}}));
    }
    if retained {
        let brand = page_css(dir, w, js);
        let mut m = app.world_mut().non_send_resource_mut::<crate::DomMirror>();
        m.dom.resize(w, h);
        let body = m.dom.body();
        // the page's own defaults (as `snapshot`): no margin, Inter where a part names no face; the brand kit once
        m.dom.set_attribute(body, "style", "margin: 0; font-family: Inter, sans-serif");
        if let Some(css) = &brand {
            let st = m.dom.create_element("style");
            let t = m.dom.create_text(css);
            m.dom.append(st, t);
            m.dom.append(body, st);
        }
        m.dirty = true;
        m.fonts_dirty = true;
    }
    let fps = num(&conf, "fps").unwrap_or(30.0);
    if let Some(mut m) = app.world_mut().get_non_send_resource_mut::<crate::DomMirror>() {
        m.fps = fps;
    }
    let total = num(&conf, "frames").unwrap_or(60.0) as i64;
    let wanted = wanted.unwrap_or_else(|| (0..total).collect());
    let mut cx = Ctx { dir, work: &work, decoders: HashMap::new(), stills: HashMap::new(), sounds: Vec::new(), scene: Vec::new(), clear: None, marker: None, mix: None, holders: Vec::new(), audible: HashMap::new(), brand: page_css(dir, w, js), manifest: MANIFEST.lock().unwrap().as_ref().map(|_| Vec::new()), now: 0, made: HashMap::new(), dom: None, root: None, shown: HashMap::new(), painted: false, fed: None, scene3d: None, live3d: None, textures: VideoFrames::on(), vframes: VideoFrames::new() };
    if fed {
        if let Some(p) = scene {
            cx.scene3d = Some(rdb::engine::Engine::new_live(&p, &rdb::inserted_of(&p), dir)?);
            crate::startup_mark("the 3D's engine open");
        }
        if live3d {
            cx.live3d = Some(LiveScene::default());
        }
        return render_fed(&mut app, w, h, fps, total, retained, &mut cx, out, &work, &wanted);
    }
    let mut frames = Vec::new();
    let mut tones: Vec<raea_mixer::ToneCue> = Vec::new();
    let t0 = std::time::Instant::now();
    // RAEA_MOTION_TIMES: where a render's frames go (React's commit, the page recorded and submitted, the page before
    // collected and stored), totals in ms
    let times = std::env::var_os("RAEA_MOTION_TIMES").is_some();
    let (mut t_react, mut t_snap, mut t_page) = (0f64, 0f64, 0f64);
    for &f in &wanted {
        let t0 = std::time::Instant::now();
        app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": f}));
        settle(&mut app, Some(f), 20.0)?;
        cx.now = f;
        let t1 = std::time::Instant::now();
        let fr = match retained.then(|| snapshot_retained(app.world_mut(), w, h, &mut cx)).transpose()?.flatten() {
            Some(fr) => fr,
            None => snapshot(app.world_mut(), w, h, &mut cx)?,
        };
        tones.push(crate::audio::tone_at(app.world_mut(), f, fps, dir, media_path, true));
        frames.push((f, fr));
        let t2 = std::time::Instant::now();
        // the frame before's page, now that this one is on the GPU
        if frames.len() >= 2 {
            let k = frames.len() - 2;
            settle_page(&mut frames, k, w, h, &work)?;
        }
        if times {
            t_react += (t1 - t0).as_secs_f64() * 1e3;
            t_snap += (t2 - t1).as_secs_f64() * 1e3;
            t_page += t2.elapsed().as_secs_f64() * 1e3;
        }
    }
    if times {
        eprintln!("raea-motion times: react {t_react:.0} ms, page recorded and submitted {t_snap:.0} ms, page collected and stored {t_page:.0} ms over {} frames", wanted.len());
    }
    if !frames.is_empty() {
        let k = frames.len() - 1;
        settle_page(&mut frames, k, w, h, &work)?;
    }
    let tw = std::time::Instant::now();
    pages_written()?;
    if times {
        eprintln!("raea-motion times: waiting for the page writers {:.0} ms", tw.elapsed().as_secs_f64() * 1e3);
    }
    let react_ms = t0.elapsed().as_secs_f64() * 1e3 / wanted.len().max(1) as f64;
    drop(app);
    // 3D parts passing through each other on screen: on stderr, and in the manifest for void's checks
    let clipping = scene3d::clip3d::told(&frames, w, h, fps);
    if let (Some(path), Some(frames_seen)) = (MANIFEST.lock().unwrap().clone(), cx.manifest.take()) {
        std::fs::write(&path, json!({"width": w, "height": h, "fps": fps, "frames": frames_seen, "clipping": clipping}).to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    }

    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    // rdb's SQL beside its assets: what the iPhone's engine renders (ugc_engine_render), the composition's own IR, read
    // back and checked equal before it's written
    if ext == "sql" {
        let mut p = program(&frames, w, h, fps);
        tone_rows(&mut p, &tones);
        let sql = rdb::ir::to_sql(&p)?;
        let back = rdb::ir::compile(&sql)?;
        if format!("{:?}", (&back.spawns, &back.keys, back.fps, back.width, back.height, back.frames)) != format!("{:?}", (&p.spawns, &p.keys, p.fps, p.width, p.height, p.frames)) {
            return Err("the SQL didn't read back to the same program".into());
        }
        let dest = out.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
        std::fs::create_dir_all(dest.join("assets")).map_err(|e| e.to_string())?;
        for e in std::fs::read_dir(work.join("assets")).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            std::fs::copy(e.path(), dest.join("assets").join(e.file_name())).map_err(|e| e.to_string())?;
        }
        std::fs::write(out, &sql).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&work);
        return Ok(format!("APPLIED  {} frames as rdb SQL in {} (read back equal; assets beside it)", frames.len(), out.display()));
    }
    // rdb's IR built directly from the frames and drawn by rdb (checked equal to the retired rrl's compile of the same
    // frames on the goldens, a video edit and a gain ramp before the switch)
    let p = program(&frames, w, h, fps);
    let rows = rdb::inserted_of(&p);
    let png = ext == "png";
    let target = if png { work.join("png") } else { out.to_path_buf() };
    // the sound: the one mixer core's bounce of the frames, as the fed render and play make it
    let sound = if !png { core_bounce(&frames, &tones, fps, &p, &rows, dir)? } else { None };
    if !png && sound.is_none() && !rows.iter().any(|t| t.kind == "audio") {
        no_sound_but_declared(dir)?;
    }
    let opts = rdb::RenderOpts { frames: if png { Some(vec![0]) } else { None }, sound, ..render_opts() };
    let tr = std::time::Instant::now();
    rdb::render(&p, &rows, &work, &target, &opts)?;
    if times {
        eprintln!("raea-motion times: rdb's render (pages composited over the clips, encoded) {:.0} ms", tr.elapsed().as_secs_f64() * 1e3);
    }
    if png {
        let f = std::fs::read_dir(&target).map_err(|e| e.to_string())?.filter_map(|e| e.ok().map(|e| e.path())).find(|p| p.extension().is_some_and(|x| x == "png")).ok_or("no frame drawn")?;
        std::fs::copy(&f, out).map_err(|e| e.to_string())?;
    }
    if std::env::var_os("RAEA_MOTION_KEEP").is_none() {
        let _ = std::fs::remove_dir_all(&work);
    }
    Ok(format!("APPLIED  {} frames rendered to {} ({react_ms:.0} ms a frame in React and Blitz)", frames.len(), out.display()))
}
