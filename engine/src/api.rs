//! The library's entry points, behind the C API (ffi.rs): probe, lint, snapshot, and a render
//! session that renders frames on worker threads and hands them over in order.
//!
//! These mirror the CLI (main.rs): the same probe JSON, the same lint checks, the same frame math.
//! (`parse_fps`, `resolve_input`, `frame_count`, `default_fps`, `probe_json` and the lint body are
//! copies of main.rs's; main.rs can switch to these once the C API lands.)
//!
//! Threading: a `Composition` holds a V8 isolate and is `!Send`, so it lives and dies on one
//! thread. Every function here that loads one does so on an internal thread with a 64 MB stack
//! (V8 and Stylo recurse deeply; iOS gives the main thread 1 MB and other threads 512 KB), and
//! returns plain data. So all of them may be called from any thread, including the main thread
//! (they block). A `RenderSession` is `Send`: it owns its worker threads and channels, and can be
//! driven from any one thread at a time.

use crate::media::{self, AudioClip, FrameProvider, VideoClip, VideoFrames};
use crate::net::SyncNet;
use crate::runtime;
use crate::scene::{Backend, Composition, Options};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Stack for every thread that runs a composition (same as the CLI's render workers).
pub const STACK_SIZE: usize = 64 << 20;

/// Byte order of delivered frames. Both are 8 bits per channel, premultiplied alpha, sRGB (what
/// vello_cpu and Skia rasterize to); frames are opaque unless `transparent` is set.
/// `Bgra8` is CoreVideo's kCVPixelFormatType_32BGRA (AVAssetWriter's native input).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PixelFormat {
    #[default]
    Rgba8,
    Bgra8,
}

/// RGBA <-> BGRA in place (the swap is its own inverse).
pub fn swap_red_blue(px: &mut [u8]) {
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
}

/// Options shared by probe / lint / snapshot / render; the C API's `opts_json`.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub fps: Option<(u32, u32)>,
    pub scale: f32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub variables: Option<Value>,
    pub backend: Backend,
    pub transparent: bool,
    /// The HTML composition format conventions (default) or a plain web page (scene::Profile)
    pub profile: crate::scene::Profile,
    pub workers: Option<usize>,
    pub network: bool,
    pub video_frames: Option<PathBuf>,
    pub pixel_format: PixelFormat,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            fps: None,
            scale: 1.0,
            width: None,
            height: None,
            start: None,
            end: None,
            variables: None,
            backend: Backend::default(),
            transparent: false,
            profile: crate::scene::Profile::Html,
            workers: None,
            network: true,
            video_frames: None,
            pixel_format: PixelFormat::Rgba8,
        }
    }
}

const OPTION_KEYS: &str = "fps, scale, width, height, start, end, variables, backend, transparent, workers, network, videoFrames, pixelFormat, profile";

impl RenderOptions {
    /// `opts_json`: an object with any of `fps` (number or "N/D"), `scale`, `width`, `height`,
    /// `start`, `end` (seconds), `variables` (object, or JSON text, or "@path"), `backend`
    /// ("vello" | "skia"), `transparent`, `workers`, `network`, `videoFrames` (directory),
    /// `pixelFormat` ("rgba" | "bgra"), `profile` ("html" | "web").
    /// Empty / null means all defaults.
    pub fn from_json(text: Option<&str>) -> Result<RenderOptions, String> {
        let mut o = RenderOptions::default();
        let text = text.map(str::trim).unwrap_or("");
        if text.is_empty() || text == "null" {
            return Ok(o);
        }
        let v: Value = serde_json::from_str(text).map_err(|e| format!("opts_json: {e}"))?;
        let Some(obj) = v.as_object() else { return Err("opts_json must be a JSON object".into()) };
        let num = |k: &str, v: &Value| v.as_f64().filter(|x| x.is_finite()).ok_or(format!("opts_json.{k} must be a number"));
        let uint = |k: &str, v: &Value| v.as_u64().filter(|x| *x <= u32::MAX as u64).ok_or(format!("opts_json.{k} must be a non-negative integer"));
        for (k, v) in obj {
            if v.is_null() {
                continue;
            }
            match k.as_str() {
                "fps" => {
                    o.fps = Some(match v {
                        Value::String(s) => parse_fps(s)?,
                        _ => parse_fps(&num(k, v)?.to_string())?,
                    })
                }
                "scale" => o.scale = num(k, v)? as f32,
                "width" => o.width = Some(uint(k, v)? as u32),
                "height" => o.height = Some(uint(k, v)? as u32),
                "start" => o.start = Some(num(k, v)?),
                "end" => o.end = Some(num(k, v)?),
                "variables" => {
                    o.variables = Some(match v {
                        Value::String(s) => {
                            let text = match s.strip_prefix('@') {
                                Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?,
                                None => s.clone(),
                            };
                            serde_json::from_str(&text).map_err(|e| format!("opts_json.variables: {e}"))?
                        }
                        other => other.clone(),
                    })
                }
                "backend" => {
                    o.backend = match v.as_str() {
                        Some("auto") => Backend::Auto,
                        Some("vello") => Backend::Vello,
                        Some("skia") => Backend::Skia,
                        Some("gpu") => Backend::Gpu,
                        _ => return Err(format!("unknown backend {v} (vello | skia | gpu)")),
                    }
                }
                "transparent" => o.transparent = v.as_bool().ok_or("opts_json.transparent must be a boolean")?,
                "profile" => o.profile = crate::scene::Profile::parse(v.as_str().ok_or("opts_json.profile must be a string")?)?,
                "network" => o.network = v.as_bool().ok_or("opts_json.network must be a boolean")?,
                "workers" => o.workers = Some((uint(k, v)? as usize).max(1)),
                "videoFrames" => o.video_frames = Some(PathBuf::from(v.as_str().ok_or("opts_json.videoFrames must be a path")?)),
                "pixelFormat" => {
                    o.pixel_format = match v.as_str().map(|s| s.to_ascii_lowercase()).as_deref() {
                        Some("rgba" | "rgba8") => PixelFormat::Rgba8,
                        Some("bgra" | "bgra8") => PixelFormat::Bgra8,
                        _ => return Err(format!("unknown pixelFormat {v} (rgba | bgra)")),
                    }
                }
                other => return Err(format!("unknown option {other:?} in opts_json (known: {OPTION_KEYS})")),
            }
        }
        if !(o.scale > 0.0 && o.scale.is_finite()) {
            return Err("opts_json.scale must be > 0".into());
        }
        Ok(o)
    }
}

// --- copies of the CLI's helpers (main.rs) -------------------------------------------------

pub fn parse_fps(s: &str) -> Result<(u32, u32), String> {
    if let Some((n, d)) = s.split_once('/') {
        let n: u32 = n.trim().parse().map_err(|_| format!("bad fps {s}"))?;
        let d: u32 = d.trim().parse().map_err(|_| format!("bad fps {s}"))?;
        if n == 0 || d == 0 {
            return Err(format!("bad fps {s}"));
        }
        return Ok((n, d));
    }
    let f: f64 = s.trim().parse().map_err(|_| format!("bad fps {s}"))?;
    if f <= 0.0 || !f.is_finite() {
        return Err(format!("bad fps {s}"));
    }
    if (f - f.round()).abs() < 1e-9 {
        Ok((f.round() as u32, 1))
    } else {
        Ok(((f * 1000.0).round() as u32, 1000))
    }
}

/// A project directory (its index.html) or an .html file -> (html, project dir).
pub fn resolve_input(p: &Path) -> Result<(PathBuf, PathBuf), String> {
    let p = p.canonicalize().map_err(|e| format!("{}: {e}", p.display()))?;
    if p.is_dir() {
        let html = p.join("index.html");
        if !html.exists() {
            return Err(format!("{} has no index.html", p.display()));
        }
        Ok((html, p))
    } else {
        let dir = p.parent().unwrap_or(Path::new("/")).to_path_buf();
        Ok((p, dir))
    }
}

pub fn frame_count(duration: f64, fps: f64) -> u64 {
    let raw = duration * fps;
    let nearest = raw.round();
    if (raw - nearest).abs() <= 1e-3 { nearest as u64 } else { raw.ceil() as u64 }
}

pub fn default_fps(html: &Path) -> (u32, u32) {
    // root data-fps, read without a full parse
    let text = std::fs::read_to_string(html).unwrap_or_default();
    if let Some(i) = text.find("data-fps=") {
        let rest = &text[i + 9..];
        let q = rest.chars().next().unwrap_or('"');
        let v: String = rest[1..].chars().take_while(|c| *c != q).collect();
        if let Ok(f) = parse_fps(&v) {
            return f;
        }
    }
    (30, 1)
}

/// Default worker count: half the cores (max 8) like the CLI. On iOS at most 2: iPhones have two
/// performance cores, frames are dealt to workers round-robin (so the slowest worker sets the
/// pace), and every worker holds a full document and frame buffers.
pub fn default_workers() -> usize {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let max = if cfg!(target_os = "ios") { 2 } else { 8 };
    (cores / 2).clamp(1, max)
}

pub struct Probe {
    pub comp: Composition,
    pub videos: Vec<VideoClip>,
    pub audio: Vec<AudioClip>,
}

fn load(o: &RenderOptions, html: &Path, dir: &Path, fps: (u32, u32), videos: Arc<VideoFrames>) -> Result<Composition, String> {
    let size = match (o.width, o.height) {
        (Some(w), Some(h)) => Some((w, h)),
        _ => None,
    };
    Composition::load(Options {
        html_path: html.to_path_buf(),
        project_dir: dir.to_path_buf(),
        fps,
        scale: o.scale,
        size,
        variables: o.variables.clone(),
        allow_network: o.network,
        transparent: o.transparent,
        videos,
        backend: o.backend,
        profile: o.profile,
        html_source: None,
    })
}

pub fn probe(o: &RenderOptions, html: &Path, dir: &Path, fps: (u32, u32)) -> Result<Probe, String> {
    let comp = load(o, html, dir, fps, Arc::new(VideoFrames::default()))?;
    let (videos, audio) = {
        let inner = blitz_dom::Document::inner(&comp.doc);
        let durations = std::collections::HashMap::new();
        let windows = std::collections::HashMap::new();
        let mut resolver = runtime::Resolver::new(&inner, &durations, &windows);
        media::discover(&inner, &mut resolver, dir, comp.duration)
    };
    Ok(Probe { comp, videos, audio })
}

pub fn probe_json(p: &Probe, fps: (u32, u32)) -> Value {
    let fpsf = fps.0 as f64 / fps.1 as f64;
    let (w, h) = p.comp.output_size();
    json!({
        "width": w,
        "height": h,
        "cssWidth": p.comp.width,
        "cssHeight": p.comp.height,
        "fps": if fps.1 == 1 { json!(fps.0) } else { json!(format!("{}/{}", fps.0, fps.1)) },
        "duration": p.comp.duration,
        "frames": frame_count(p.comp.duration, fpsf),
        "rootComposition": p.comp.root_comp_id,
        "backend": p.comp.backend_name(),
        "timelineDuration": p.comp.timeline_duration,
        "videos": p.videos,
        "audio": p.audio,
        "warnings": p.comp.warnings,
        "unsupported": p.comp.unsupported,
        "jsErrors": p.comp.js_errors,
    })
}

/// `lint --json`: `{ok, errors, warnings, unsupported}`.
pub fn lint_json(o: &RenderOptions, html: &Path, dir: &Path, fps: (u32, u32)) -> Result<Value, String> {
    let p = probe(o, html, dir, fps)?;
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = p.comp.warnings.clone();
    if p.comp.root_comp_id.is_none() {
        errors.push("no element with data-composition-id: the root composition is missing".into());
    }
    if p.comp.duration <= 0.0 {
        errors.push("composition duration is unknown: set data-duration on the root or register a timeline on window.__timelines".into());
    }
    if p.comp.authored_duration.is_none() && p.comp.timeline_duration > 0.0 {
        warnings.push(format!("root has no data-duration; using the root timeline's {:.3}s", p.comp.timeline_duration));
    }
    for e in &p.comp.js_errors {
        errors.push(format!("script error: {e}"));
    }
    errors.extend(p.comp.structure_errors());
    for v in &p.videos {
        if !v.src.contains("://") && !Path::new(&v.src).exists() {
            errors.push(format!("video {} source not found: {}", v.id, v.src));
        }
    }
    for au in &p.audio {
        if !au.src.contains("://") && !Path::new(&au.src).exists() {
            errors.push(format!("audio {} source not found: {}", au.id, au.src));
        }
    }
    let mut unsupported = p.comp.unsupported.clone();
    // static scan for features the renderer can't paint
    let mut sources = vec![html.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(dir.join("compositions")) {
        sources.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "html")));
    }
    // WebGL renders through ANGLE when this install has it (src/webgl.rs)
    let no_gl: &[(&str, &str)] = &[
        ("webgl", "WebGL canvases are not rendered: ANGLE was not found (RAEA_ANGLE_DIR)"),
        ("THREE.", "three.js / WebGL scenes are not rendered: ANGLE was not found (RAEA_ANGLE_DIR)"),
    ];
    let checks: &[(&str, &str)] = if crate::webgl_available() { &[] } else { no_gl };
    for f in &sources {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        // data-fx effects: unknown names and bad arguments, with how to fix them (effects/catalog.json)
        for e in crate::effects::lint_sources([text.as_str()]) {
            if !errors.contains(&e) {
                errors.push(e);
            }
        }
        for w in crate::effects::lint_warnings([text.as_str()]) {
            if !warnings.contains(&w) {
                warnings.push(w);
            }
        }
        for (needle, msg) in checks {
            if text.contains(needle) {
                let m = msg.to_string();
                if !unsupported.contains(&m) {
                    unsupported.push(m);
                }
            }
        }
    }
    Ok(json!({"ok": errors.is_empty(), "errors": errors, "warnings": warnings, "unsupported": unsupported}))
}

// --- threads ----------------------------------------------------------------------------------

/// Text of a panic payload.
pub fn panic_message(p: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        format!("panic: {s}")
    } else if let Some(s) = p.downcast_ref::<String>() {
        format!("panic: {s}")
    } else {
        "panic".into()
    }
}

/// Run `f` on a fresh thread with a composition-sized stack and wait for it.
pub fn on_render_thread<T: Send>(f: impl FnOnce() -> Result<T, String> + Send) -> Result<T, String> {
    std::thread::scope(|s| {
        let h = std::thread::Builder::new()
            .name("raea".into())
            .stack_size(STACK_SIZE)
            .spawn_scoped(s, f)
            .map_err(|e| format!("spawn: {e}"))?;
        h.join().map_err(panic_message)?
    })
}

/// `<video>` frame source, in order of preference: the host's provider, a frames directory,
/// ffmpeg (when built with it). Without any, videos render empty (with a warning).
fn video_frames(
    o: &RenderOptions,
    dir: &Path,
    videos: &[VideoClip],
    fps: f64,
    provider: Option<&FrameProvider>,
    warnings: &mut Vec<String>,
) -> Result<Arc<VideoFrames>, String> {
    if videos.is_empty() {
        return Ok(Arc::new(VideoFrames { fps, ..Default::default() }));
    }
    if let Some(p) = provider {
        return Ok(Arc::new(VideoFrames::from_provider(fps, videos, p.clone())));
    }
    if let Some(d) = &o.video_frames {
        return Ok(Arc::new(VideoFrames::from_dir(d, fps, videos)));
    }
    if cfg!(feature = "ffmpeg") {
        let net = SyncNet::new(dir, o.network);
        let cache = net.cache_dir.clone().unwrap_or_else(std::env::temp_dir).join("video");
        return Ok(Arc::new(VideoFrames::extract(&cache, fps, videos, &net, &|_| {})?));
    }
    for v in videos {
        warnings.push(format!("no frames for video {} (set a frame provider or videoFrames): it renders empty", v.id));
    }
    Ok(Arc::new(VideoFrames { fps, ..Default::default() }))
}

fn input(path: &Path, o: &RenderOptions) -> Result<(PathBuf, PathBuf, (u32, u32)), String> {
    let (html, dir) = resolve_input(path)?;
    let fps = o.fps.unwrap_or_else(|| default_fps(&html));
    Ok((html, dir, fps))
}

/// `probe` JSON for a project dir or .html file.
pub fn probe_path(path: &Path, o: &RenderOptions) -> Result<Value, String> {
    on_render_thread(|| {
        let (html, dir, fps) = input(path, o)?;
        let p = probe(o, &html, &dir, fps)?;
        Ok(probe_json(&p, fps))
    })
}

/// `lint --json` for a project dir or .html file.
pub fn lint_path(path: &Path, o: &RenderOptions) -> Result<Value, String> {
    on_render_thread(|| {
        let (html, dir, fps) = input(path, o)?;
        lint_json(o, &html, &dir, fps)
    })
}

/// Text that doesn't fit its component (`lint --layout`; textfit.rs): renders `at` (or every half second, at most 40
/// frames) and returns {"errors", "warnings", "crops"}, each finding's region cropped to `crop_dir` when given. the host app's
/// Agent calls it before a render (the roadmap K2).
pub fn layout_path(path: &Path, at: &[f64], crop_dir: Option<&Path>, o: &RenderOptions) -> Result<Value, String> {
    on_render_thread(|| {
        let (html, dir, fps) = input(path, o)?;
        let fpsf = fps.0 as f64 / fps.1 as f64;
        let p = probe(o, &html, &dir, fps)?;
        let mut warnings = Vec::new();
        let frames = video_frames(o, &dir, &p.videos, fpsf, None, &mut warnings)?;
        let mut comp = if p.videos.is_empty() { p.comp } else { load(o, &html, &dir, fps, frames)? };
        let times = if at.is_empty() { crate::textfit::default_times(comp.duration) } else { at.to_vec() };
        crate::textfit::check(&mut crate::stage::Stage::new(comp), &times, crop_dir)
    })
}

/// One frame at time `t` (seconds): `(width, height, RGBA8)` at device pixels.
pub fn snapshot_path(path: &Path, t: f64, o: &RenderOptions) -> Result<(u32, u32, Vec<u8>), String> {
    on_render_thread(|| {
        let (html, dir, fps) = input(path, o)?;
        let fpsf = fps.0 as f64 / fps.1 as f64;
        let p = probe(o, &html, &dir, fps)?;
        let mut warnings = Vec::new();
        let frames = video_frames(o, &dir, &p.videos, fpsf, None, &mut warnings)?;
        let comp = if p.videos.is_empty() { p.comp } else { load(o, &html, &dir, fps, frames)? };
        let (w, h) = comp.output_size();
        let mut buf = Vec::new();
        // Bevy's schedule makes the frame (stage.rs)
        crate::stage::Stage::new(comp).frame(t, &mut buf);
        if o.pixel_format == PixelFormat::Bgra8 {
            swap_red_blue(&mut buf);
        }
        Ok((w, h, buf))
    })
}

// --- render session ---------------------------------------------------------------------------

pub enum FrameError {
    /// the caller's buffer holds fewer than this many bytes
    BufferTooSmall(usize),
    Failed(String),
}

struct Running {
    receivers: Vec<Receiver<Result<Vec<u8>, String>>>,
    recycle: Vec<SyncSender<Vec<u8>>>,
    handles: Vec<JoinHandle<()>>,
}

/// A render of frames `[first, last)` on `workers` threads, each with its own composition,
/// delivered in order. Workers start on the first `next_frame`, so a frame provider can be
/// set after `begin`.
pub struct RenderSession {
    html: PathBuf,
    dir: PathBuf,
    opts: RenderOptions,
    fps: (u32, u32),
    first: u64,
    last: u64,
    workers: usize,
    width: usize,
    height: usize,
    frame_bytes: usize,
    videos: Vec<VideoClip>,
    info: Value,
    provider: Option<FrameProvider>,
    running: Option<Running>,
    /// random-access worker for `frame_at` (scrubbing), started on first use
    scrub: Option<Scrub>,
    delivered: u64,
    error: Option<String>,
}

/// One composition kept loaded on its own thread, rendering whatever time it is asked for.
/// A request to the scrub worker: a frame at t, or the text-layout report at t (answered as UTF-8 JSON).
enum ScrubRequest {
    Frame(f64),
    Boxes(f64, Option<[f64; 4]>),
}

struct Scrub {
    requests: SyncSender<ScrubRequest>,
    frames: Receiver<Result<Vec<u8>, String>>,
    handle: Option<JoinHandle<()>>,
}

impl RenderSession {
    pub fn begin(path: &Path, o: RenderOptions) -> Result<RenderSession, String> {
        let (html, dir, fps, mut info, videos, frames) = on_render_thread(|| {
            let (html, dir, fps) = input(path, &o)?;
            let p = probe(&o, &html, &dir, fps)?;
            Ok((html, dir, fps, probe_json(&p, fps), p.videos.clone(), frame_count(p.comp.duration, fps.0 as f64 / fps.1 as f64)))
        })?;
        let duration = info["duration"].as_f64().unwrap_or(0.0);
        if duration <= 0.0 {
            return Err("composition duration is unknown (no data-duration on the root and no root timeline)".into());
        }
        let fpsf = fps.0 as f64 / fps.1 as f64;
        let total = frames;
        let first = o.start.map(|s| (s * fpsf).round() as u64).unwrap_or(0).min(total);
        let last = o.end.map(|e| frame_count(e.min(duration), fpsf)).unwrap_or(total).max(first);
        let n = last - first;
        let workers = o.workers.unwrap_or_else(default_workers).clamp(1, 32).min(n.max(1) as usize);
        let (w, h) = (info["width"].as_u64().unwrap_or(0), info["height"].as_u64().unwrap_or(0));
        let frame_bytes = (w * h * 4) as usize;
        info["firstFrame"] = json!(first);
        info["endFrame"] = json!(last);
        info["frameCount"] = json!(n);
        info["workers"] = json!(workers);
        info["bytesPerFrame"] = json!(frame_bytes);
        Ok(RenderSession {
            html,
            dir,
            opts: o,
            fps,
            first,
            last,
            workers,
            width: w as usize,
            height: h as usize,
            frame_bytes,
            videos,
            info,
            provider: None,
            running: None,
            scrub: None,
            delivered: 0,
            error: None,
        })
    }

    /// probe JSON plus `firstFrame`, `endFrame` (exclusive), `frameCount`, `workers`, `bytesPerFrame`.
    pub fn info(&self) -> &Value {
        &self.info
    }

    pub fn frame_bytes(&self) -> usize {
        self.frame_bytes
    }

    /// The composition's `<video>` clips (ids, windows, mediaStart, playbackRate, loop).
    pub fn videos(&self) -> &[VideoClip] {
        &self.videos
    }

    pub fn fps(&self) -> f64 {
        self.fps.0 as f64 / self.fps.1 as f64
    }

    /// Bytes `next_frame` needs for rows `row_bytes` apart (0 = packed, width * 4).
    pub fn required_len(&self, row_bytes: usize) -> usize {
        let row = if row_bytes == 0 { self.width * 4 } else { row_bytes };
        if self.height == 0 { 0 } else { row * (self.height - 1) + self.width * 4 }
    }

    pub fn set_provider(&mut self, p: Option<FrameProvider>) -> Result<(), String> {
        if self.running.is_some() || self.scrub.is_some() {
            return Err("the frame provider must be set before the first frame is rendered".into());
        }
        self.provider = p;
        Ok(())
    }

    fn start(&mut self) -> Result<(), String> {
        let fpsf = self.fps.0 as f64 / self.fps.1 as f64;
        let mut warnings = Vec::new();
        let frames = video_frames(&self.opts, &self.dir, &self.videos, fpsf, self.provider.as_ref(), &mut warnings)?;
        if let Some(w) = self.info["warnings"].as_array_mut() {
            w.extend(warnings.into_iter().map(Value::String));
        }
        let mut running = Running { receivers: Vec::new(), recycle: Vec::new(), handles: Vec::new() };
        let (first, last, workers, fps) = (self.first, self.last, self.workers, self.fps);
        for k in 0..workers {
            // one finished frame queued per worker (frames are width*height*4 bytes each)
            let (tx, rx) = sync_channel::<Result<Vec<u8>, String>>(1);
            let (rtx, rrx) = sync_channel::<Vec<u8>>(2);
            let (html, dir, opts, frames) = (self.html.clone(), self.dir.clone(), self.opts.clone(), frames.clone());
            let handle = std::thread::Builder::new()
                .name(format!("raea-render-{k}"))
                .stack_size(STACK_SIZE)
                .spawn(move || {
                    let run = std::panic::AssertUnwindSafe(|| {
                        let mut comp = match load(&opts, &html, &dir, fps, frames) {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Err(e));
                                return;
                            }
                        };
                        // a video render: moving text from outlines (anyrender::moving_text)
                        anyrender::moving_text::enable(true);
                        // Bevy's schedule is the frame loop (stage.rs, docs/bevy.feature B2)
                        let mut stage = crate::stage::Stage::new(comp);
                        let mut i = first + k as u64;
                        while i < last {
                            let t = i as f64 * fps.1 as f64 / fps.0 as f64;
                            let mut buf = rrx.try_recv().unwrap_or_default();
                            stage.frame(t, &mut buf);
                            if opts.pixel_format == PixelFormat::Bgra8 {
                                swap_red_blue(&mut buf);
                            }
                            if tx.send(Ok(buf)).is_err() {
                                return;
                            }
                            i += workers as u64;
                        }
                    });
                    if let Err(p) = std::panic::catch_unwind(run) {
                        let _ = tx.send(Err(panic_message(p)));
                    }
                })
                .map_err(|e| format!("spawn: {e}"))?;
            running.receivers.push(rx);
            running.recycle.push(rtx);
            running.handles.push(handle);
        }
        self.running = Some(running);
        Ok(())
    }

    /// Copy the next frame into `buf`, rows `row_bytes` apart (0 = packed: width * 4; a
    /// CVPixelBuffer's bytesPerRow otherwise); `Ok(Some(frame index))`, `Ok(None)` when done.
    pub fn next_frame(&mut self, buf: &mut [u8], row_bytes: usize) -> Result<Option<u64>, FrameError> {
        if let Some(e) = &self.error {
            return Err(FrameError::Failed(e.clone()));
        }
        let n = self.last - self.first;
        if self.delivered >= n {
            return Ok(None);
        }
        let row = if row_bytes == 0 { self.width * 4 } else { row_bytes };
        if row < self.width * 4 {
            return Err(FrameError::Failed(format!("row_bytes {row_bytes} is less than width * 4 = {}", self.width * 4)));
        }
        let need = self.required_len(row_bytes);
        if buf.len() < need {
            return Err(FrameError::BufferTooSmall(need));
        }
        if self.running.is_none() {
            if let Err(e) = self.start() {
                self.error = Some(e.clone());
                return Err(FrameError::Failed(e));
            }
        }
        let running = self.running.as_ref().unwrap();
        let k = (self.delivered % self.workers as u64) as usize;
        let index = self.first + self.delivered;
        let result = match running.receivers[k].recv() {
            Ok(Ok(frame)) if frame.len() == self.frame_bytes => {
                let packed = self.width * 4;
                if row == packed {
                    buf[..self.frame_bytes].copy_from_slice(&frame);
                } else {
                    for (y, src) in frame.chunks_exact(packed).enumerate() {
                        buf[y * row..y * row + packed].copy_from_slice(src);
                    }
                }
                let _ = running.recycle[k].try_send(frame);
                Ok(index)
            }
            Ok(Ok(frame)) => Err(format!("frame {index} has {} bytes, expected {}", frame.len(), self.frame_bytes)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(format!("render worker stopped at frame {index}")),
        };
        match result {
            Ok(i) => {
                self.delivered += 1;
                Ok(Some(i))
            }
            Err(e) => {
                self.error = Some(e.clone());
                Err(FrameError::Failed(e))
            }
        }
    }
}

impl RenderSession {
    /// Render the frame at time `t` seconds (any order: scrubbing, previews) into `buf`, rows
    /// `row_bytes` apart (0 = packed). One worker keeps the composition loaded, so after the first
    /// call each frame costs one seek + paint. Independent of `next_frame`.
    pub fn frame_at(&mut self, t: f64, buf: &mut [u8], row_bytes: usize) -> Result<(), FrameError> {
        let row = if row_bytes == 0 { self.width * 4 } else { row_bytes };
        if row < self.width * 4 {
            return Err(FrameError::Failed(format!("row_bytes {row_bytes} is less than width * 4 = {}", self.width * 4)));
        }
        let need = self.required_len(row_bytes);
        if buf.len() < need {
            return Err(FrameError::BufferTooSmall(need));
        }
        self.start_scrub()?;
        let scrub = self.scrub.as_ref().unwrap();
        if scrub.requests.send(ScrubRequest::Frame(t)).is_err() {
            return Err(FrameError::Failed("scrub worker stopped".into()));
        }
        match scrub.frames.recv() {
            Ok(Ok(frame)) if frame.len() == self.frame_bytes => {
                let packed = self.width * 4;
                if row == packed {
                    buf[..self.frame_bytes].copy_from_slice(&frame);
                } else {
                    for (y, src) in frame.chunks_exact(packed).enumerate() {
                        buf[y * row..y * row + packed].copy_from_slice(src);
                    }
                }
                Ok(())
            }
            Ok(Ok(frame)) => Err(FrameError::Failed(format!("frame has {} bytes, expected {}", frame.len(), self.frame_bytes))),
            Ok(Err(e)) => Err(FrameError::Failed(e)),
            Err(_) => Err(FrameError::Failed("scrub worker stopped".into())),
        }
    }

    /// The text-layout report for time `t` as JSON (the CLI's `snapshot --boxes` sidecar): every visible
    /// text element's box in composition px with transforms, canvas edges crossed, overflow clipping,
    /// opacity, safe-zone bands reached and overlaps. `safe_zone` is top, right, bottom, left in
    /// composition px; None uses 220,140,420,0 at 1080x1920 and no zone otherwise. Renders `t` on the
    /// scrub worker (the same composition `frame_at` uses).
    pub fn boxes_at(&mut self, t: f64, safe_zone: Option<[f64; 4]>) -> Result<String, String> {
        self.start_scrub().map_err(|e| match e {
            FrameError::Failed(e) => e,
            FrameError::BufferTooSmall(_) => "scrub worker failed to start".into(),
        })?;
        let scrub = self.scrub.as_ref().unwrap();
        if scrub.requests.send(ScrubRequest::Boxes(t, safe_zone)).is_err() {
            return Err("scrub worker stopped".into());
        }
        match scrub.frames.recv() {
            Ok(Ok(json)) => String::from_utf8(json).map_err(|e| e.to_string()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("scrub worker stopped".into()),
        }
    }

    /// Load the composition on the scrub worker (once).
    fn start_scrub(&mut self) -> Result<(), FrameError> {
        if self.scrub.is_none() {
            let fpsf = self.fps.0 as f64 / self.fps.1 as f64;
            let mut warnings = Vec::new();
            let frames = video_frames(&self.opts, &self.dir, &self.videos, fpsf, self.provider.as_ref(), &mut warnings).map_err(FrameError::Failed)?;
            let (req_tx, req_rx) = sync_channel::<ScrubRequest>(1);
            let (tx, rx) = sync_channel::<Result<Vec<u8>, String>>(1);
            let (html, dir, opts, fps) = (self.html.clone(), self.dir.clone(), self.opts.clone(), self.fps);
            let handle = std::thread::Builder::new()
                .name("raea-scrub".into())
                .stack_size(STACK_SIZE)
                .spawn(move || {
                    let run = std::panic::AssertUnwindSafe(|| {
                        let comp = match load(&opts, &html, &dir, fps, frames) {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Err(e));
                                return;
                            }
                        };
                        let (cw, ch) = (comp.width, comp.height);
                        // the scrubber's frames are runs of Bevy's schedule too (stage.rs)
                        let mut stage = crate::stage::Stage::new(comp);
                        let mut buf = Vec::new();
                        while let Ok(req) = req_rx.recv() {
                            let reply = match req {
                                ScrubRequest::Frame(t) => {
                                    let mut frame = std::mem::take(&mut buf);
                                    stage.frame(t, &mut frame);
                                    if opts.pixel_format == PixelFormat::Bgra8 {
                                        swap_red_blue(&mut frame);
                                    }
                                    frame
                                }
                                ScrubRequest::Boxes(t, safe) => {
                                    let safe = safe.or(((cw, ch) == (1080, 1920)).then_some([220.0, 140.0, 420.0, 0.0]));
                                    stage.text_boxes(t, &mut buf, safe).to_string().into_bytes()
                                }
                            };
                            if tx.send(Ok(reply)).is_err() {
                                return;
                            }
                        }
                    });
                    if let Err(p) = std::panic::catch_unwind(run) {
                        let _ = tx.send(Err(panic_message(p)));
                    }
                })
                .map_err(|e| FrameError::Failed(format!("spawn: {e}")))?;
            self.scrub = Some(Scrub { requests: req_tx, frames: rx, handle: Some(handle) });
        }
        Ok(())
    }
}

impl Drop for RenderSession {
    fn drop(&mut self) {
        if let Some(mut s) = self.scrub.take() {
            let handle = s.handle.take();
            drop(s);
            if let Some(h) = handle {
                let _ = h.join();
            }
        }
        if let Some(r) = self.running.take() {
            // dropping the receivers makes every worker's next send fail, so they exit after
            // the frame they are on
            drop(r.receivers);
            drop(r.recycle);
            for h in r.handles {
                let _ = h.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The composition format's media rules in the audio plan: data-volume and clamped fades on a data-has-audio
    /// video, and data-hf-media-start-basis="global" keeping a nested element's literal start.
    #[test]
    fn media_plan_volume_fades_and_start_basis() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/media-plan");
        let o = RenderOptions::from_json(Some(r#"{"network": false}"#)).unwrap();
        let v = probe_path(&dir, &o).unwrap();
        let audio = v["audio"].as_array().unwrap();
        let get = |id: &str| audio.iter().find(|a| a["id"] == id).unwrap_or_else(|| panic!("no audio {id}: {v}"));
        let video = get("v-audio");
        assert_eq!(video["volume"], 0.5);
        // fades 3 + 3 on a 4 s clip shrink proportionally (clampFadesToDuration): 2 + 2
        assert!((video["fadeIn"].as_f64().unwrap() - 2.0).abs() < 1e-9, "{video}");
        assert!((video["fadeOut"].as_f64().unwrap() - 2.0).abs() < 1e-9, "{video}");
        assert_eq!(video["start"], 1.0);
        // nested in a composition starting at 2: local basis adds it, global basis keeps the literal 1
        assert!((get("local")["start"].as_f64().unwrap() - 3.0).abs() < 1e-9, "{v}");
        assert!((get("global")["start"].as_f64().unwrap() - 1.0).abs() < 1e-9, "{v}");
    }

    /// The text-layout report: boxes with transforms, nesting not counted as overlap, clipping flags.
    #[test]
    fn boxes_at_reports_text_layout() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/boxes");
        let o = RenderOptions::from_json(Some(r#"{"network": false}"#)).unwrap();
        let mut session = RenderSession::begin(&dir, o).unwrap();
        let v: Value = serde_json::from_str(&session.boxes_at(1.0, None).unwrap()).unwrap();
        assert_eq!(v["safeZone"]["bottom"], 420.0);
        let boxes = v["boxes"].as_array().unwrap();
        let find = |id: &str| boxes.iter().position(|b| b["id"] == id).unwrap_or_else(|| panic!("no box for #{id}: {v}"));
        let title = &boxes[find("title")];
        assert!(title["centerOffset"]["x"].as_f64().unwrap().abs() < 1.0, "title centred: {title}");
        assert_eq!(title["overlaps"], json!([]), "nested span is not an overlap");
        assert_eq!(boxes[find("a")]["overlaps"], json!([find("b")]));
        let scaled = &boxes[find("tilt")];
        assert!(scaled["h"].as_f64().unwrap() > 120.0, "scale(2) doubles the box: {scaled}");
        assert_eq!(boxes[find("off")]["clipped"], json!(["right"]));
        assert!(boxes[find("hidden")]["visibleFraction"].as_f64().unwrap() < 0.1);
    }

    /// Scrubbing in any order gives the same pixels as independent snapshots.
    #[test]
    fn frame_at_matches_snapshots() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/css-anim/keyframes");
        let o = RenderOptions::from_json(Some(r#"{"network": false}"#)).unwrap();
        let mut session = RenderSession::begin(&dir, o.clone()).unwrap();
        let mut buf = vec![0u8; session.frame_bytes()];
        for t in [1.7, 0.4, 2.6, 1.0] {
            session.frame_at(t, &mut buf, 0).ok().unwrap();
            let (_, _, snap) = snapshot_path(&dir, t, &o).unwrap();
            assert!(buf == snap, "frame_at({t}) differs from snapshot");
        }
    }
}
