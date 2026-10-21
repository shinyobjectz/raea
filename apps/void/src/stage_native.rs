//! The native stage: void's stage drawn by the RAEA engine itself, on the GPU, inside void's window (One renderer; the
//! user, 2026-10-17: "one RAEA engine that plays, playtests and renders games and video at the display's rate"). A
//! layer-hosting view (CAMetalLayer) sits under the webview at the stage's box, and the webview draws no background, so
//! the stage shows where the page is transparent and the app's overlays draw over it; every mouse and key event goes to
//! the webview, which hands the stage keys and pointer through `key` and `pointer`. (A sublayer over the webview stayed
//! black: WebKit's layers covered it.) A thread of its own
//! runs the live engine (raea-react's live.rs: React on V8 -> ECS page -> Blitz -> quads) and presents every display
//! refresh, paced by the layer's vsync. No pixels come back to the CPU and nothing is written to disk.
//!
//! The app (app.rs, ui.rs) calls, all from the main thread:
//!   open(ns_window)            once, with the window
//!   rect(x, y, w, h, scale)    whenever the stage's box moves or resizes (points, from the content's top-left)
//!   show(folder, kind)         when the object on screen changes (None: nothing)
//!   visible(on)                the layer hidden or shown, nothing else (an overlay open over the box)
//!   control(Control)           play, pause, seek, fullscreen
//!   key(name, down), pointer(x, y, down)   input while the stage has focus (DOM key names; points in the box)
//!   status()                   frame, fps, frames missed, the last error

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use objc2::runtime::AnyObject;
use objc2::{Encode, Encoding, class, msg_send};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Point {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Size {
    w: f64,
    h: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Rect {
    origin: Point,
    size: Size,
}
unsafe impl Encode for Point {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for Size {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for Rect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[Point::ENCODING, Size::ENCODING]);
}

#[derive(Clone, Copy, Debug)]
pub enum Control {
    Play,
    Pause,
    Seek(i64),
    Fullscreen(bool),
}

/// The object's clips (a baked timeline's Sequences: Live::clips), to the panels as void:timeline when it opens or is
/// baked again: the dock's timeline control lays them out.
fn timeline(l: &raea_react::live::Live) {
    crate::space_ui::context(vec![format!("void:timeline {}", serde_json::json!({"clips": l.clips(), "frames": l.frames, "fps": l.fps}))]);
}

/// What the stage is doing, for the app's controls.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub showing: Option<PathBuf>,
    pub frame: i64,
    pub playing: bool,
    /// frames presented per second over the last second, and frames that took longer than 1.5 refreshes
    pub fps: f64,
    pub missed: u64,
    /// a timeline's length in frames (0 for a game, which runs on) and its rate: the app's player bar
    pub frames: i64,
    pub rate: f64,
    pub error: Option<String>,
    /// the page's size in pixels: what the chrome's <img data-raea-w data-raea-h> says in composited mode
    pub page: (u32, u32),
}

enum Cmd {
    Show(Option<PathBuf>, Option<String>),
    Resize(u32, u32),
    Control(Control),
    Key(String, bool),
    /// points in the box, whether a button is down, the box in points and the screen's scale
    Pointer(f64, f64, bool, (f64, f64), f64),
    Composited(bool),
    /// the page read back to a PNG (void's playtest probe, apps/void/bench/probe.rs): the frame, the page's size
    Snapshot(PathBuf, Sender<Result<(i64, u32, u32), String>>),
    /// the same, `Snap::Stage`: the texture the stage paints, after its next paint (what the window draws, at the
    /// stage's size); `Snap::Page`: the page at its own size, composited on the GPU as the stage composites it
    SnapshotAs(PathBuf, Snap, Sender<Result<(i64, u32, u32), String>>),
    /// the open timeline's files changed (`watch`: the object's generation, whether files other than code changed, when
    /// the change was seen)
    Changed(u64, bool, Instant),
    /// its new bake, made on a worker (`Live::rebake`), for the object of that generation
    Rebaked(u64, Box<Result<raea_react::live::Rebaked, String>>, bool, Instant),
    /// an optimized copy of the open object's footage was completed (host::optimize_media): the stage's decoders opened
    /// before it read the source, so they're dropped and the next frame opens the copy, as a render now does
    MediaOptimized,
}

/// The open object's footage got an optimized copy: the stage reads it from its next frame (Cmd::MediaOptimized).
pub fn media_optimized() {
    if let Some(s) = STAGE.lock().unwrap().as_ref() {
        let _ = s.tx.send(Cmd::MediaOptimized);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Snap {
    Stage,
    Page,
}

/// Whether the stage bakes an object of this kind again itself when its files change (a timeline, played from its
/// bake: `watch` and `Live::rebake`, the playhead kept), rather than the app opening it again (a game, live).
pub fn rebakes(kind: Option<&str>) -> bool {
    matches!(kind, None | Some("video" | "motion" | "music" | "scene3d"))
}

/// The texture id the stage's page is painted under in composited mode: a native chrome draws the stage with
/// `<img data-raea-texture={TEXTURE} data-raea-w data-raea-h>` (the page's size, status().page) in its own frame.
pub const TEXTURE: u64 = u64::MAX - 11;

/// Called after each frame the stage paints in composited mode (from the stage's thread): the chrome asks for a redraw.
static ON_FRAME: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);

/// Composited mode (a native chrome): the stage paints its page into TEXTURE instead of presenting to its own layer
/// (hidden then), paced at the composition's rate, and calls `on_frame` after each; the chrome draws it in its own frame,
/// so its glass blurs the stage and one present shows both. Off: the stage presents to its layer as before.
pub fn composited(on: bool) {
    let g = STAGE.lock().unwrap();
    let Some(s) = g.as_ref() else { return };
    unsafe {
        let view = &*(s.layer as *const AnyObject);
        let _: () = msg_send![class!(CATransaction), begin];
        let _: () = msg_send![class!(CATransaction), setDisableActions: true];
        if on {
            let _: () = msg_send![view, setHidden: true];
        }
        let _: () = msg_send![class!(CATransaction), commit];
    }
    let _ = s.tx.send(Cmd::Composited(on));
}

/// What to call after each composited frame (the chrome's redraw request). It runs on the stage's thread with nothing
/// held, and must not block: hand off to the UI's own thread (a one-slot channel, an event-loop proxy), never wait on it.
pub fn on_frame(f: Box<dyn Fn() + Send + Sync>) {
    *ON_FRAME.lock().unwrap() = Some(Arc::from(f));
}

struct Handle {
    /// the stage's view (layer-hosting: its layer is the CAMetalLayer the engine presents to), or with `sublayer` the
    /// CAMetalLayer itself, inside the content view's layer under the chrome's (the native chrome)
    layer: usize,
    sublayer: bool,
    content: usize,
    tx: Sender<Cmd>,
    status: Arc<Mutex<Status>>,
    scale: f64,
    /// the stage's box in points (rect)
    size: (f64, f64),
}

static STAGE: Mutex<Option<Handle>> = Mutex::new(None);

/// Put the stage's layer in the window (hidden until a rect and an object arrive) and start its thread.
pub fn open(ns_window: *mut c_void) -> Result<(), String> {
    if ns_window.is_null() {
        return Err("no window".into());
    }
    let mut g = STAGE.lock().unwrap();
    if g.is_some() {
        return Ok(());
    }
    let (layer, content, sublayer) = unsafe {
        let window = &*(ns_window as *const AnyObject);
        let mut content: *mut AnyObject = msg_send![window, contentView];
        // the webview: the content view itself (wry), or one of its subviews
        let wk = class!(WKWebView);
        let is_wk = |v: *mut AnyObject| -> bool { !v.is_null() && msg_send![&*v, isKindOfClass: wk] };
        let mut webview: *mut AnyObject = std::ptr::null_mut();
        if is_wk(content) {
            // a container in its place, holding the stage's view under the webview
            let frame: Rect = msg_send![&*content, frame];
            let container: *mut AnyObject = msg_send![class!(NSView), alloc];
            let container: *mut AnyObject = msg_send![container, initWithFrame: frame];
            let _: () = msg_send![container, setAutoresizesSubviews: true];
            webview = content;
            let _: *mut AnyObject = msg_send![webview, retain];
            let _: () = msg_send![window, setContentView: container];
            let _: () = msg_send![webview, setFrame: Rect { origin: Point::default(), size: frame.size }];
            let _: () = msg_send![webview, setAutoresizingMask: 18usize];
            let _: () = msg_send![container, addSubview: webview];
            content = container;
        } else {
            let subs: *mut AnyObject = msg_send![&*content, subviews];
            let n: usize = msg_send![subs, count];
            for i in 0..n {
                let v: *mut AnyObject = msg_send![subs, objectAtIndex: i];
                if is_wk(v) {
                    webview = v;
                }
            }
        }
        // the webview draws nothing where its page is transparent (the stage's box), so the stage shows through
        if !webview.is_null() {
            let no: *mut AnyObject = msg_send![class!(NSNumber), numberWithBool: false];
            let key: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: c"drawsBackground".as_ptr()];
            let _: () = msg_send![webview, setValue: no, forKey: key];
        }
        // the stage: a layer-hosting view (CAMetalLayer) under the webview, so overlays draw over it and every event
        // goes to the webview
        let view: *mut AnyObject = msg_send![class!(NSView), alloc];
        let view: *mut AnyObject = msg_send![view, initWithFrame: Rect::default()];
        let layer: *mut AnyObject = msg_send![class!(CAMetalLayer), layer];
        let _: *mut AnyObject = msg_send![layer, retain];
        let _: () = msg_send![layer, setOpaque: true];
        let _: () = msg_send![view, setLayer: layer];
        let _: () = msg_send![view, setWantsLayer: true];
        let _: () = msg_send![view, setHidden: true];
        // no webview (the native chrome: winit's view, whose own layer holds the chrome's CAMetalLayer as a sublayer):
        // the stage's layer goes under the chrome's inside that layer, as its first sublayer; a view added above it
        // drew over everything the chrome painted in the stage's box
        let root: *mut AnyObject = if webview.is_null() {
            let _: () = msg_send![content, setWantsLayer: true];
            msg_send![content, layer]
        } else {
            std::ptr::null_mut()
        };
        let root_is_metal = !root.is_null() && { let b: bool = msg_send![&*root, isKindOfClass: class!(CAMetalLayer)]; b };
        let sublayer = !root.is_null() && !root_is_metal;
        if sublayer {
            let _: () = msg_send![layer, setHidden: true];
            let _: () = msg_send![root, insertSublayer: layer, atIndex: 0u32];
        } else if webview.is_null() {
            let _: () = msg_send![content, addSubview: view];
        } else {
            // NSWindowBelow = -1
            let _: () = msg_send![content, addSubview: view, positioned: -1isize, relativeTo: webview];
        }
        crate::mcp::log(format!(
            "stage: {}",
            if sublayer { "layer under the chrome's, inside the content view's layer".to_string() } else { format!("view under {} (webview {}){}", if webview.is_null() { "the content view" } else { "the webview" }, if is_wk(content) { "is the content" } else { "found" }, if root_is_metal { "; the content view's own layer is a CAMetalLayer, so the stage sits above it" } else { "" }) }
        ));
        (if sublayer { layer as usize } else { view as usize }, content as usize, sublayer)
    };
    let (tx, rx) = channel();
    let status = Arc::new(Mutex::new(Status::default()));
    let st = status.clone();
    let metal = if sublayer {
        layer
    } else {
        unsafe {
            let view = &*(layer as *const AnyObject);
            let m: *mut AnyObject = msg_send![view, layer];
            m as usize
        }
    };
    let window = ns_window as usize;
    WINDOW.store(window, std::sync::atomic::Ordering::Relaxed);
    let me = tx.clone();
    std::thread::Builder::new().name("void stage".into()).spawn(move || run(metal, window, rx, st, me)).map_err(|e| e.to_string())?;
    // the launch's object opened now, as the stage thread starts, instead of when the window's app has shown it
    // (app.rs launch_target; lightspeed F5). The app's own show of the same object then finds it open; a different one
    // replaces it as before
    if let Some((folder, kind)) = crate::app::launch_target() {
        let _ = tx.send(Cmd::Show(Some(folder), kind));
    }
    *g = Some(Handle { layer, content, tx, status, scale: 2.0, sublayer, size: (0.0, 0.0) });
    Ok(())
}

/// The stage's box in the window, in points from the content's top-left, and the screen's scale.
pub fn rect(x: f64, y: f64, w: f64, h: f64, scale: f64) {
    let mut g = STAGE.lock().unwrap();
    let Some(s) = g.as_mut() else { return };
    s.scale = scale;
    s.size = (w, h);
    unsafe {
        let content = &*(s.content as *const AnyObject);
        let layer = &*(s.layer as *const AnyObject);
        let flipped: bool = msg_send![content, isFlipped];
        let bounds: Rect = msg_send![content, bounds];
        let oy = if flipped { y } else { bounds.size.h - y - h };
        let frame = Rect { origin: Point { x, y: oy }, size: Size { w, h } };
        let _: () = msg_send![class!(CATransaction), begin];
        let _: () = msg_send![class!(CATransaction), setDisableActions: true];
        if s.sublayer {
            // the content view's layer can be replaced after open() (AppKit re-making a layer-backed view's layer, the
            // chrome's surface set up after the stage): put the stage back under whatever holds the chrome's now
            let root: *mut AnyObject = msg_send![content, layer];
            let sup: *mut AnyObject = msg_send![layer, superlayer];
            if !root.is_null() && sup != root {
                let _: () = msg_send![layer, removeFromSuperlayer];
                let _: () = msg_send![root, insertSublayer: layer, atIndex: 0u32];
                crate::mcp::log("stage: its layer was out of the content view's layer; put back under the chrome's");
            }
        }
        let _: () = msg_send![layer, setFrame: frame];
        let metal: *mut AnyObject = if s.sublayer { s.layer as *mut AnyObject } else { msg_send![layer, layer] };
        let _: () = msg_send![metal, setContentsScale: scale];
        // RAEA_STAGE_TINT=1: the stage's layer itself magenta behind its frames (a hole that shows magenta: the layer is
        // there and visible but its frames don't show; white or anything else: it's hidden, covered or misplaced)
        if std::env::var("RAEA_STAGE_TINT").as_deref() == Ok("1") {
            let c: *mut AnyObject = msg_send![class!(NSColor), magentaColor];
            let cg: *mut AnyObject = msg_send![c, CGColor];
            let _: () = msg_send![metal, setBackgroundColor: cg];
        }
        let _: () = msg_send![class!(CATransaction), commit];
        if s.sublayer {
            // the layer tree the stage sits in, bottom first: where it is, what's over it
            let root: *mut AnyObject = msg_send![content, layer];
            let rb: Rect = msg_send![root, bounds];
            let gf: bool = msg_send![root, isGeometryFlipped];
            let subs: *mut AnyObject = msg_send![root, sublayers];
            let n: usize = if subs.is_null() { 0 } else { msg_send![subs, count] };
            let mut tree = format!("stage: root layer {:.0}x{:.0} flipped {gf} (view flipped {flipped}), {n} sublayers:", rb.size.w, rb.size.h);
            for i in 0..n {
                let l: *mut AnyObject = msg_send![subs, objectAtIndex: i];
                let f: Rect = msg_send![l, frame];
                let hidden: bool = msg_send![l, isHidden];
                let opaque: bool = msg_send![l, isOpaque];
                let is_stage = l == metal;
                tree.push_str(&format!(" [{i}{} {:.0},{:.0} {:.0}x{:.0} hidden {hidden} opaque {opaque}]", if is_stage { " stage" } else { "" }, f.origin.x, f.origin.y, f.size.w, f.size.h));
            }
            crate::mcp::log(tree);
        }
    }
    let _ = s.tx.send(Cmd::Resize((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32));
}

/// The object on screen (its folder; its kind from space.json), or nothing.
pub fn show(folder: Option<&Path>, kind: Option<&str>) {
    let g = STAGE.lock().unwrap();
    let Some(s) = g.as_ref() else { return };
    unsafe {
        let view = &*(s.layer as *const AnyObject);
        // no fade (a layer's hidden is animated unless the transaction says not)
        let _: () = msg_send![class!(CATransaction), begin];
        let _: () = msg_send![class!(CATransaction), setDisableActions: true];
        let _: () = msg_send![view, setHidden: folder.is_none()];
        let _: () = msg_send![class!(CATransaction), commit];
    }
    let _ = s.tx.send(Cmd::Show(folder.map(Path::to_path_buf), kind.map(str::to_string)));
}

/// Hide or show the layer and nothing else (no Show, no Resize: the object keeps its state), for an overlay that
/// opens over the stage's box and would sit under the layer.
pub fn visible(on: bool) {
    let g = STAGE.lock().unwrap();
    let Some(s) = g.as_ref() else { return };
    unsafe {
        let view = &*(s.layer as *const AnyObject);
        // no fade (a layer's hidden is animated unless the transaction says not)
        let _: () = msg_send![class!(CATransaction), begin];
        let _: () = msg_send![class!(CATransaction), setDisableActions: true];
        let _: () = msg_send![view, setHidden: !on];
        let _: () = msg_send![class!(CATransaction), commit];
    }
}

/// Counts the person's actions on the stage (a key, the pointer, play, pause, a seek): work done ahead of them
/// (host::speculate) stops when it moves (lightspeed T1)
static ACTIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many actions the stage has had (ACTIONS).
pub fn actions() -> u64 {
    ACTIONS.load(std::sync::atomic::Ordering::Relaxed)
}

fn acted() {
    ACTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub fn control(c: Control) {
    acted();
    if let Some(s) = STAGE.lock().unwrap().as_ref() {
        let _ = s.tx.send(Cmd::Control(c));
    }
}

/// A key the person pressed or let go while the stage has focus (a DOM key name: "ArrowLeft", " ", "a").
pub fn key(name: &str, down: bool) {
    acted();
    if let Some(s) = STAGE.lock().unwrap().as_ref() {
        let _ = s.tx.send(Cmd::Key(name.to_string(), down));
    }
}

/// The pointer in the stage's box (points from its top-left) and whether a button is down.
pub fn pointer(x: f64, y: f64, down: bool) {
    acted();
    if let Some(s) = STAGE.lock().unwrap().as_ref() {
        let _ = s.tx.send(Cmd::Pointer(x, y, down, s.size, s.scale));
    }
}

/// The stage's box in points and the screen's scale, as the chrome last set them (rect).
pub fn box_size() -> Option<(f64, f64, f64)> {
    STAGE.lock().unwrap().as_ref().map(|s| (s.size.0, s.size.1, s.scale))
}

/// The stage's page as it is now, read back to a PNG at `path` (void's playtest probe): the frame and the page's size.
pub fn snapshot(path: &Path) -> Result<(i64, u32, u32), String> {
    let (tx, rx) = channel();
    STAGE.lock().unwrap().as_ref().ok_or("no stage")?.tx.send(Cmd::Snapshot(path.to_path_buf(), tx)).map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(20)).map_err(|e| e.to_string())?
}

/// `snapshot`, of the stage's texture or of its page at full size (`Snap`).
pub fn snapshot_as(path: &Path, snap: Snap) -> Result<(i64, u32, u32), String> {
    let (tx, rx) = channel();
    STAGE.lock().unwrap().as_ref().ok_or("no stage")?.tx.send(Cmd::SnapshotAs(path.to_path_buf(), snap, tx)).map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(20)).map_err(|e| e.to_string())?
}

/// The texture id the probe's full-size page is composited under (Snap::Page).
const PAGE_CHECK: u64 = u64::MAX - 19;

pub(crate) fn save_rgba(path: &Path, w: u32, h: u32, px: &[u8]) -> Result<(), String> {
    raea_react::live::save_png(path, w, h, px)
}

/// The open timeline's folder watched (every 50 ms, by stage_ts::sources: code and data by content, media by name, size
/// and time) while it's the object of generation `mine`: a change, once the files hold still for a poll, is sent to the
/// stage as Cmd::Changed, which bakes it again on a worker.
fn watch(folder: PathBuf, mine: u64, current: Arc<std::sync::atomic::AtomicU64>, tx: Sender<Cmd>) {
    let _ = std::thread::Builder::new().name("void stage watch".into()).spawn(move || {
        let media = |f: &Path| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            let mut walk: Vec<PathBuf> = ["media", "assets"].iter().map(|d| f.join(d)).collect();
            while let Some(d) = walk.pop() {
                let mut es: Vec<_> = std::fs::read_dir(&d).into_iter().flatten().flatten().collect();
                es.sort_by_key(|e| e.file_name());
                for e in es {
                    let Ok(m) = std::fs::metadata(e.path()) else { continue };
                    if m.is_dir() {
                        walk.push(e.path());
                    } else if m.is_file() {
                        (e.path(), m.len(), m.modified().ok()).hash(&mut h);
                    }
                }
            }
            h.finish()
        };
        let mut v = crate::stage_ts::sources(&folder).version;
        let mut m = media(&folder);
        // the folder's file events (FSEvents), so a change is seen as it lands: a 50 ms poll, and a second poll to see the
        // file hold still, put 50-100 ms between a save and its re-bake starting (lightspeed F5). Without a watcher, the
        // poll as before
        let (etx, erx) = std::sync::mpsc::channel::<()>();
        let watcher = notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
            if r.is_ok() {
                let _ = etx.send(());
            }
        })
        .and_then(|mut w| {
            use notify::Watcher;
            w.watch(&folder, notify::RecursiveMode::Recursive).map(|_| w)
        });
        let watching = watcher.is_ok();
        let mut seen: Option<Instant> = None;
        loop {
            // an event, or a second without one (a change the events missed); 50 ms with no watcher
            let woke = erx.recv_timeout(Duration::from_millis(if watching { 1000 } else { 50 })).is_ok();
            if current.load(std::sync::atomic::Ordering::Relaxed) != mine {
                return;
            }
            if woke {
                // the file still being written while events keep coming: read once they've stopped for 10 ms
                seen.get_or_insert_with(Instant::now);
                while erx.recv_timeout(Duration::from_millis(10)).is_ok() {}
            }
            let now = crate::stage_ts::sources(&folder).version;
            if now != v {
                v = now;
                let t = seen.take().unwrap_or_else(Instant::now);
                if !watching {
                    // polled: once more to see it hold still (a file half written)
                    seen = Some(t);
                    continue;
                }
                let nm = media(&folder);
                let changed_media = nm != m;
                m = nm;
                if tx.send(Cmd::Changed(mine, changed_media, t)).is_err() {
                    return;
                }
            } else if let Some(t) = seen.take().filter(|_| !watching) {
                let nm = media(&folder);
                let changed_media = nm != m;
                m = nm;
                if tx.send(Cmd::Changed(mine, changed_media, t)).is_err() {
                    return;
                }
            }
        }
    });
}

pub fn status() -> Status {
    STAGE.lock().unwrap().as_ref().map(|s| s.status.lock().unwrap().clone()).unwrap_or_default()
}

/// The window the stage is in (its NSWindow), for `window_state`.
static WINDOW: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A game steps its next frame the moment a key changes instead of at the next tick of its clock (up to a frame
/// sooner), the clock going on from there. Off unless RAEA_STAGE_KEY_STEP=1 (the probe can flip it, /keystep): only
/// stepping on the clock's tick is measured in void's window so far (2026-10-18: pong input to present p99 28.0 ms, 1.68
/// refreshes at 60 Hz); the key-step run never got its window.
pub static KEY_STEP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The window's state as AppKit has it (void's probe: why a test window stopped being drawn): visible, miniaturized,
/// on the active Space, its occlusion state, level, frame, and whether the app is hidden or active.
pub fn window_state() -> serde_json::Value {
    let w = WINDOW.load(std::sync::atomic::Ordering::Relaxed);
    if w == 0 {
        return serde_json::Value::Null;
    }
    unsafe {
        let win = &*(w as *const AnyObject);
        let visible: bool = msg_send![win, isVisible];
        let mini: bool = msg_send![win, isMiniaturized];
        let space: bool = msg_send![win, isOnActiveSpace];
        let occ: usize = msg_send![win, occlusionState];
        let level: isize = msg_send![win, level];
        let frame: Rect = msg_send![win, frame];
        let screen: *mut AnyObject = msg_send![win, screen];
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let hidden: bool = msg_send![&*app, isHidden];
        let active: bool = msg_send![&*app, isActive];
        // the Metal layers in the content view's layer: the chrome's and the stage's own (drawables, vsync)
        let mut layers = Vec::new();
        let content: *mut AnyObject = msg_send![win, contentView];
        if !content.is_null() {
            let root: *mut AnyObject = msg_send![&*content, layer];
            let mut stack: Vec<*mut AnyObject> = if root.is_null() { vec![] } else { vec![root] };
            while let Some(l) = stack.pop() {
                let metal: bool = msg_send![&*l, isKindOfClass: class!(CAMetalLayer)];
                if metal {
                    let n: usize = msg_send![&*l, maximumDrawableCount];
                    let sync: bool = msg_send![&*l, displaySyncEnabled];
                    let txn: bool = msg_send![&*l, presentsWithTransaction];
                    let f: Rect = msg_send![&*l, frame];
                    layers.push(serde_json::json!({"drawables": n, "display_sync": sync, "with_transaction": txn, "size": [f.size.w, f.size.h]}));
                }
                let subs: *mut AnyObject = msg_send![&*l, sublayers];
                let k: usize = if subs.is_null() { 0 } else { msg_send![&*subs, count] };
                for i in 0..k {
                    stack.push(msg_send![&*subs, objectAtIndex: i]);
                }
            }
        }
        serde_json::json!({"metal_layers": layers, "visible": visible, "miniaturized": mini, "on_active_space": space, "occlusion_visible": occ & 2 != 0,
            "level": level, "frame": [frame.origin.x, frame.origin.y, frame.size.w, frame.size.h], "has_screen": !screen.is_null(),
            "app_hidden": hidden, "app_active": active})
    }
}

/// Whether any of the window shows (NSWindow's occlusion state: visible unless it's minimized, on another Space,
/// covered whole or the screen is off). wgpu reads it from the presenting thread before each drawable too.
fn window_visible(window: usize) -> bool {
    if window == 0 {
        return true;
    }
    const VISIBLE: usize = 1 << 1;
    let state: usize = unsafe { msg_send![&*(window as *const AnyObject), occlusionState] };
    state & VISIBLE != 0
}

/// The stage thread: the object's live engine stepped at its rate and presented every refresh.
fn run(layer: usize, window: usize, rx: Receiver<Cmd>, status: Arc<Mutex<Status>>, me: Sender<Cmd>) {
    let mut size = (1u32, 1u32);
    let mut stage: Option<raea::quads::Stage> = None;
    let mut live: Option<raea_react::live::Live> = None;
    // what the open `live` is: its folder and kind, so a show of the same object (the app's, after the stage opened the
    // launch's object itself) is kept instead of opened again
    let mut shown_obj: Option<(PathBuf, Option<String>)> = None;
    // the first frame's sound, held back until that frame is painted (see the step below)
    let mut first_sound_due = false;
    let mut keys: Vec<String> = Vec::new();
    let mut pointer: Option<(f64, f64, bool)> = None;
    let mut last_publish = Instant::now();
    let mut playing = true;
    // when a pause rests the mixer's stream: a second in, its fade-outs done (lightspeed R6)
    let mut rest_at: Option<Instant> = None;
    let mut looping = true;
    let mut composite = false;
    // the composition's clock: frame `base` at `since`
    let (mut base, mut since, mut frame) = (0i64, Instant::now(), -1i64);
    let (mut shown, mut window_start, mut last_present, mut missed, mut hidden) = (0u32, Instant::now(), None::<Instant>, 0u64, 0u32);
    let set_error = |e: String| {
        crate::mcp::log(format!("stage: {e}"));
        status.lock().unwrap().error = Some(e);
    };
    // a command that woke the loop's wait for the next frame (a key: taken at once, not a frame later)
    let mut woke: Option<Cmd> = None;
    // the window hidden or covered whole: nothing is stepped or painted (nobody sees it; a game holds where it is, a
    // timeline's clock and sound wait), checked every 100 ms. RAEA_STAGE_HIDDEN_PLAYS=1 plays on (a probe's stage-only
    // run)
    // (a headless test instance, RAEA_VOID_HEADLESS=1, has no window on screen: it plays)
    let hidden_plays = std::env::var("RAEA_STAGE_HIDDEN_PLAYS").as_deref() == Ok("1") || std::env::var("RAEA_VOID_HEADLESS").as_deref() == Ok("1");
    // the object on the stage by generation (its folder's watcher stops when it changes), a re-bake on a worker, and a
    // change that came in while it ran (whether media changed): baked again when it's done
    let shown_gen = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let (mut rebaking, mut again) = (false, None::<(bool, Instant)>);
    // the mixer's drift samples already handed to the probe
    let mut drift_seen = 0usize;
    // a mixer opened ahead of the next object (lightspeed F5): the one mixer core waits ~0.1 s for its device's clock
    // before a first frame, and one opened while nothing's waiting has run that by the time an object opens. One now,
    // and another after each object's first frame (each object has its own: nothing of the last one's carries over)
    let mut spare = raea_react::audio::Mixer::open().ok();
    let mut refill = false;
    // a spare nobody took for two minutes is let go, so an idle void holds no audio device (R6); the next object then
    // opens its own
    let mut spare_since = Instant::now();
    // a probe's read of the stage's texture, answered after the next paint
    let mut snap_stage: Option<(PathBuf, Sender<Result<(i64, u32, u32), String>>)> = None;
    let (mut visible, mut visible_at) = (true, Instant::now() - Duration::from_secs(1));
    let mut hidden_secs = 0.0f64;
    // something to paint though no frame was stepped: a new object, size, seek, or mode
    let mut repaint = true;
    // the texture's size: the page's, or smaller when it's painted at the size the chrome shows it
    let mut page = (0u32, 0u32);
    if std::env::var("RAEA_STAGE_KEY_STEP").as_deref() == Ok("1") {
        KEY_STEP.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    loop {
        if spare.is_some() && spare_since.elapsed() > Duration::from_secs(120) {
            spare = None;
        }
        // the spare's stream rests once its clock has settled (its first 100 ms of sound), so the device callback isn't
        // rendering silence the whole time it waits; the open that takes it starts it again at its first frame
        if spare_since.elapsed() > Duration::from_secs(2) {
            if let Some(m) = spare.as_mut() {
                m.settle();
                m.rest();
            }
        }
        let mut key_changed = false;
        // commands: wait for one when there's nothing to play, else take what's there
        let mut cmds: Vec<Cmd> = woke.take().into_iter().collect();
        if visible_at.elapsed() >= Duration::from_millis(100) {
            let now = window_visible(window) || hidden_plays;
            if now != visible {
                crate::mcp::log(format!("stage: the window {} ({})", if now { "shows again: playing on" } else { "is hidden or covered: nothing stepped or painted" }, window_state()));
                if now {
                    // the clock from here, as after a pause
                    (base, since) = (frame.max(0), Instant::now());
                } else if let Some(l) = live.as_mut() {
                    l.silence();
                }
                visible = now;
            }
            visible_at = Instant::now();
            crate::probe::visible(visible);
        }
        // paused a second: the mixer's stream stopped, so the device callback doesn't render silence while it waits; the
        // next frame's sound starts it again
        if rest_at.is_some_and(|t| Instant::now() >= t) {
            rest_at = None;
            if !playing {
                if let Some(l) = live.as_mut() {
                    l.rest();
                }
            }
        }
        if cmds.is_empty() && (live.is_none() || !playing || !visible) {
            match rx.recv_timeout(Duration::from_millis(if visible { 250 } else { 100 })) {
                Ok(c) => cmds.push(c),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return,
            }
            if !visible {
                hidden_secs += 0.1;
            }
        }
        cmds.extend(rx.try_iter());
        crate::probe::drained(cmds.iter().any(|c| matches!(c, Cmd::Key(..))));
        for c in cmds {
            match c {
                Cmd::Resize(w, h) => {
                    repaint = true;
                    size = (w, h);
                    match stage.as_mut() {
                        Some(s) => {
                            if let Err(e) = s.resize(w, h) {
                                set_error(e);
                            }
                        }
                        None => match unsafe { raea::quads::stage_raw(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer as *mut c_void), w, h, true) } {
                            Ok(s) => stage = Some(s),
                            Err(e) => set_error(e),
                        },
                    }
                    if let Some(l) = live.as_mut() {
                        l.resize(w, h);
                    }
                }
                Cmd::Show(folder, kind) => {
                    if live.is_some() && folder.is_some() && shown_obj.as_ref().map(|o| (Some(&o.0), &o.1)) == Some((folder.as_ref(), &kind)) {
                        repaint = true;
                        continue;
                    }
                    shown_obj = None;
                    raea_react::startup_mark("void stage show");
                    repaint = true;
                    let g = shown_gen.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    (again, drift_seen) = (None, 0);
                    // a timeline (an edit, motion, music) plays to its last frame and starts again; a game runs on
                    looping = matches!(kind.as_deref(), None | Some("video" | "motion" | "music" | "scene3d"));
                    let three_d = matches!(kind.as_deref(), Some("scene3d" | "motion"));
                    live = None;
                    keys.clear();
                    let mut st = status.lock().unwrap();
                    st.showing = folder.clone();
                    st.error = None;
                    drop(st);
                    if let Some(f) = folder {
                        // a timeline plays from its bake (no JS while it plays; the same frames a render makes); a game
                        // runs live
                        // only a game may lay out at the stage's box (the founder, 2026-10-18: video playback is always
                        // fixed scale, never responsive); everything else keeps its own size and is scaled to fit
                        let game = matches!(kind.as_deref(), Some("game"));
                        let t_open = Instant::now();
                        let opened = if looping { raea_react::live::Live::open_baked(&f.join("edit.tsx")) } else { raea_react::live::Live::open_with(&f.join("edit.tsx"), size, game) };
                        let open_ms = t_open.elapsed().as_secs_f64() * 1e3;
                        match opened {
                            Ok(mut l) => {
                                // a timeline sounds as its render does: its mastered track, built on a thread
                                if looping {
                                    l.master_sound();
                                }
                                // a 3D scene under the page, drawn as its render draws it
                                if three_d {
                                    l.scene_3d();
                                }
                                // the mixer's device open before the stage's clock starts, so the first frame doesn't
                                // wait on it (its sounds skipped in by the open's time)
                                match spare.take() {
                                    Some(m) => l.give_mixer(m),
                                    None => l.open_mixer(),
                                }
                                refill = true;
                                crate::mcp::log(format!("stage: opened in {open_ms:.0} ms{}, ready in {:.0} ms", if looping { " (baked)" } else { "" }, t_open.elapsed().as_secs_f64() * 1e3));
                                raea_react::startup_mark("void stage ready");
                                // a timeline's files watched: an edit is baked again and swapped in where it plays
                                if looping && rebakes(kind.as_deref()) {
                                    watch(f.clone(), g, shown_gen.clone(), me.clone());
                                }
                                timeline(&l);
                                live = Some(l);
                                shown_obj = Some((f.clone(), kind.clone()));
                                (base, since, frame) = (0, Instant::now(), -1);
                                playing = true;
                            }
                            Err(e) => set_error(format!("{}: {e}", f.display())),
                        }
                    }
                }
                Cmd::Control(Control::Play) => {
                    rest_at = None;
                    if !playing {
                        (base, since) = (frame.max(0), Instant::now());
                        playing = true;
                    }
                }
                Cmd::Control(Control::Pause) => {
                    playing = false;
                    if let Some(l) = live.as_mut() {
                        l.silence();
                    }
                    rest_at = Some(Instant::now() + Duration::from_secs(1));
                }
                Cmd::Control(Control::Seek(f)) => {
                    repaint = true;
                    let f = match live.as_ref() {
                        Some(l) if looping => f.clamp(0, l.frames - 1),
                        _ => f.max(0),
                    };
                    (base, since) = (f, Instant::now());
                    if let Some(l) = live.as_mut() {
                        frame = f;
                        if let Err(e) = l.step(frame, &keys, pointer) {
                            set_error(e);
                        }
                        // paused: the frame's sound placed by the step stays silent (it started and played on under a
                        // still picture)
                        if !playing {
                            l.silence();
                        }
                    }
                }
                Cmd::Changed(g, media, seen) => {
                    if g != shown_gen.load(std::sync::atomic::Ordering::Relaxed) {
                        continue;
                    }
                    if rebaking {
                        let m = again.map(|a| a.0).unwrap_or(false) || media;
                        again = Some((m, again.map(|a| a.1).unwrap_or(seen)));
                        continue;
                    }
                    if let Some(job) = live.as_ref().and_then(|l| l.rebake()) {
                        rebaking = true;
                        let tx = me.clone();
                        let _ = std::thread::Builder::new().name("void stage rebake".into()).spawn(move || {
                            let r = job.run();
                            let _ = tx.send(Cmd::Rebaked(g, Box::new(r), media, seen));
                        });
                    }
                }
                Cmd::Rebaked(g, r, media, seen) => {
                    rebaking = false;
                    let current = g == shown_gen.load(std::sync::atomic::Ordering::Relaxed);
                    match (*r, live.as_mut()) {
                        (Ok(r), Some(l)) if current => {
                            let (stepped, frames, bundle_ms, bake_ms) = (r.stepped, r.frames, r.bundle_ms, r.bake_ms);
                            let t = Instant::now();
                            let before = frame;
                            l.swap_bake(r, media);
                            // the same frame from the new bake, the clock going on (a shorter timeline from its start)
                            if frame >= l.frames {
                                (base, since, frame) = (0, Instant::now(), 0);
                            }
                            if frame >= 0 {
                                if let Err(e) = l.step(frame, &keys, pointer) {
                                    set_error(e);
                                }
                                if !playing {
                                    l.silence();
                                }
                            }
                            repaint = true;
                            timeline(l);
                            status.lock().unwrap().error = None;
                            let swap_ms = t.elapsed().as_secs_f64() * 1e3;
                            let total_ms = seen.elapsed().as_secs_f64() * 1e3;
                            crate::mcp::log(format!("stage: baked again, {stepped} of {frames} frames stepped (bundle {bundle_ms:.0} ms, bake {bake_ms:.0} ms, swap {swap_ms:.1} ms; {total_ms:.0} ms from the change seen), at frame {frame}"));
                            crate::probe::rebaked(serde_json::json!({"stepped": stepped, "frames": frames, "bundle_ms": bundle_ms, "bake_ms": bake_ms, "swap_ms": swap_ms,
                                "seen_to_swap_ms": total_ms, "frame_before": before, "frame_after": frame, "media": media, "playing": playing,
                                "at_unix_ms": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)}));
                        }
                        (Err(e), _) if current => {
                            set_error(format!("baking the edit again: {e}"));
                            crate::probe::rebaked(serde_json::json!({"error": e}));
                        }
                        _ => {}
                    }
                    if let Some((m, t)) = again.take() {
                        let _ = me.send(Cmd::Changed(shown_gen.load(std::sync::atomic::Ordering::Relaxed), m, t));
                    }
                }
                Cmd::Control(Control::Fullscreen(_)) => {}
                Cmd::Key(k, down) => {
                    if down && !keys.contains(&k) {
                        keys.push(k);
                        key_changed = true;
                    } else if !down && keys.contains(&k) {
                        keys.retain(|x| *x != k);
                        key_changed = true;
                    }
                }
                // into the page's pixels: the page is fitted whole in the box and centred (a responsive page fills it, so
                // that's the box's pixels)
                Cmd::Pointer(x, y, down, (bw, bh), k) => {
                    let (cw, ch) = live.as_ref().map(|l| (l.w as f64, l.h as f64)).unwrap_or((bw * k, bh * k));
                    let f = if bw > 0.0 && bh > 0.0 && cw > 0.0 && ch > 0.0 { (bw / cw).min(bh / ch) } else { 1.0 / k };
                    pointer = Some(((x - (bw - cw * f) / 2.0) / f, (y - (bh - ch * f) / 2.0) / f, down));
                }
                Cmd::Composited(on) => {
                    composite = on;
                    repaint = true;
                }
                Cmd::MediaOptimized => {
                    if let Some(l) = live.as_mut() {
                        l.reopen_videos();
                        repaint = true;
                    }
                }
                Cmd::Snapshot(path, reply) => {
                    let r = match live.as_mut() {
                        Some(l) => l.save_snapshot(&path).map(|()| (frame, l.w, l.h)),
                        None => Err("nothing on the stage".into()),
                    };
                    let _ = reply.send(r);
                }
                Cmd::SnapshotAs(path, Snap::Stage, reply) => {
                    repaint = true;
                    snap_stage = Some((path, reply));
                }
                Cmd::SnapshotAs(path, Snap::Page, reply) => {
                    let r = match live.as_mut() {
                        Some(l) => l.paint_texture_fit(PAGE_CHECK, (0, 0)).and_then(|_| raea::quads::read_texture(PAGE_CHECK)).and_then(|(w, h, px)| save_rgba(&path, w, h, &px).map(|()| (frame, w, h))),
                        None => Err("nothing on the stage".into()),
                    };
                    let _ = reply.send(r);
                }
            }
        }
        let (Some(l), Some(s)) = (live.as_mut(), stage.as_ref()) else { continue };
        if !visible {
            // a seek while hidden still shows when the window comes back (the page holds it); nothing painted now
            continue;
        }
        // the held-back first sound, if a frame was stepped without it and nothing has synced it yet (the window's own
        // present path)
        if std::mem::take(&mut first_sound_due) {
            l.sync_sound();
        }
        let t_work = Instant::now();
        let mut stepped = 0;
        if playing {
            // a game's key change: its next frame now, the clock from here (a timeline keeps its clock)
            if key_changed && !looping && KEY_STEP.load(std::sync::atomic::Ordering::Relaxed) {
                (base, since) = (frame + 1, Instant::now());
            }
            let due = base + (since.elapsed().as_secs_f64() * l.fps).floor() as i64;
            let mut n = 0;
            let mut wrapped = false;
            while frame < due && n < 4 {
                frame += 1;
                let wrap = looping && frame >= l.frames;
                if wrap {
                    // past the end a timeline shows nothing of its own (the last frame's captions over the background)
                    // the clock goes on: frame 0 is due when the frame past the end was, not from now (which lost the
                    // frame's lateness, so the loop's sound was placed off by it)
                    since += Duration::from_secs_f64((frame - base) as f64 / l.fps);
                    (base, frame) = (0, 0);
                }
                let lag = since.elapsed().as_secs_f64() - (frame - base) as f64 / l.fps;
                // the stage's first frame painted before its sound is placed, then the sound synced by its lag as of then:
                // the mixer's clock is still starting at launch, and placing frame 0's sound waited ~62 ms for it before
                // the first paint (lightspeed F5; play.rs presents its frame 0 first the same way)
                let quiet_first = page.0 == 0 && frame == 0 && !first_sound_due;
                let r = if quiet_first {
                    first_sound_due = true;
                    l.step_world(frame, &keys, pointer, lag)
                } else {
                    l.step_at(frame, &keys, pointer, lag)
                };
                if let Err(e) = r {
                    set_error(e);
                    playing = false;
                    break;
                }
                crate::probe::stepped(frame, &keys);
                n += 1;
                stepped += 1;
                if wrap {
                    wrapped = true;
                    break;
                }
            }
            // far behind: the clock waits for the frames rather than skipping them (a skipped frame loses its input). Not
            // after a loop back to frame 0: `due` counted the pass before, and restarting the clock here dropped frame
            // 0's lateness, so every frame after the loop was placed that much late (3.8 ms, slewed off over 20 frames)
            if frame < due && !wrapped {
                (base, since) = (frame, Instant::now());
            }
        }
        // the mixer's drift since the last frame, to the probe
        let drift = l.drift_ms();
        if drift.len() > drift_seen {
            crate::probe::drift(&drift[drift_seen.min(drift.len())..]);
        }
        drift_seen = drift.len();
        // what the object publishes (data-void-publish), to the open space's panels as void:stage, a few times a second
        if last_publish.elapsed() > Duration::from_millis(100) && crate::space_ui::running() {
            last_publish = Instant::now();
            crate::space_ui::stage(l.published());
            // what sounds now, each source (the dock's audio control, while one is open)
            if crate::shell_ui::AUDIO_OPEN.load(std::sync::atomic::Ordering::Relaxed) {
                crate::space_ui::context(vec![format!("void:sounds {}", serde_json::Value::from(l.sounds()))]);
            }
        }
        // the playhead every frame, for the app's player bar (the rest of the status once a second, below)
        {
            let mut st = status.lock().unwrap();
            st.frame = frame;
            st.playing = playing;
            st.frames = if looping { l.frames as i64 } else { 0 };
            st.rate = l.fps;
        }
        // composited: the page into the chrome's texture, then the chrome told; nothing waits on a drawable here, so
        // the loop waits for the composition's next frame itself. A page only when it changed (a wait woken by a
        // command steps nothing new)
        if composite {
            if stepped > 0 || repaint {
                repaint = false;
                let t_paint = Instant::now();
                // at the size the chrome shows it (the stage's box, object-fit contain): RAEA_STAGE_FULL_PAGE=1 paints
                // the whole page and lets the chrome shrink it
                let fit = if std::env::var_os("RAEA_STAGE_FULL_PAGE").is_some() { (0, 0) } else { size };
                match l.paint_texture_fit(TEXTURE, fit) {
                    Ok(painted) => {
                        if shown == 0 && page.0 == 0 {
                            raea_react::startup_mark("void stage first paint");
                        }
                        if std::mem::take(&mut refill) && spare.is_none() {
                            spare = raea_react::audio::Mixer::open().ok();
                            spare_since = Instant::now();
                        }
                        page = painted;
                        crate::probe::painted(t_work, t_paint, l.fps, frame);
                        if let Some((path, reply)) = snap_stage.take() {
                            let _ = reply.send(raea::quads::read_texture(TEXTURE).and_then(|(w, h, px)| save_rgba(&path, w, h, &px).map(|()| (frame, w, h))));
                        }
                        shown += 1;
                        // the callback taken out first: nothing of the stage's is held while it runs
                        let f = ON_FRAME.lock().unwrap().clone();
                        if let Some(f) = f {
                            f();
                        }
                    }
                    Err(e) => set_error(e),
                }
                status.lock().unwrap().page = page;
                if std::mem::take(&mut first_sound_due) {
                    l.sync_sound();
                }
            }
            // until the next frame is due, or a command comes (a key is in the frame after it, not a frame later)
            let next = since + Duration::from_secs_f64(((frame - base + 1) as f64 / l.fps).max(0.0));
            if let Some(d) = next.checked_duration_since(Instant::now()) {
                match rx.recv_timeout(d.min(Duration::from_millis(50))) {
                    Ok(c) => woke = Some(c),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => return,
                }
            }
            if window_start.elapsed() >= Duration::from_secs(1) {
                let mut st = status.lock().unwrap();
                st.fps = shown as f64 / window_start.elapsed().as_secs_f64();
                st.frame = frame;
                st.playing = playing;
                crate::mcp::log(format!("stage: composited frame {frame}, {:.0} fps, page {}x{}, hidden {hidden_secs:.1} s, error {:?}", st.fps, l.w, l.h, st.error));
                drop(st);
                (shown, hidden, window_start) = (0, 0, Instant::now());
            }
            continue;
        }
        // presenting waits for the layer's next drawable: the display's refresh paces the loop
        match l.present(s, true) {
            // the layer gave no drawable (hidden or covered): nothing paces the loop, so it waits a refresh itself
            Ok(false) => {
                hidden += 1;
                std::thread::sleep(Duration::from_millis(16));
            }
            Ok(true) => {
                let now = Instant::now();
                if let Some(p) = last_present {
                    if (now - p).as_secs_f64() > 1.5 / 60.0 {
                        missed += 1;
                    }
                }
                last_present = Some(now);
                shown += 1;
            }
            Err(e) => set_error(e),
        }
        if window_start.elapsed() >= Duration::from_secs(1) {
            let mut st = status.lock().unwrap();
            st.fps = shown as f64 / window_start.elapsed().as_secs_f64();
            st.frame = frame;
            st.playing = playing;
            st.missed = missed;
            // the stage's trace, a line a second (mcp log), and with RAEA_STAGE_DUMP=DIR what it painted every 2 s
            crate::mcp::log(format!("stage: frame {frame}, {:.0} fps, missed {missed}, hidden {hidden}, page {}x{}, stage {}x{}, error {:?}", st.fps, l.w, l.h, s.w, s.h, st.error));
            drop(st);
            if let Some(dir) = std::env::var_os("RAEA_STAGE_DUMP") {
                if frame % (2.0 * l.fps).max(1.0) as i64 <= l.fps as i64 / 2 {
                    let _ = l.save_snapshot(&std::path::Path::new(&dir).join(format!("stage-{frame:06}.png")));
                }
            }
            (shown, hidden, window_start) = (0, 0, Instant::now());
        }
    }
}
