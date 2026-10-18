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
    Pointer(f64, f64, bool),
    Composited(bool),
    /// the page read back to a PNG (void's playtest probe, void/bench/probe.rs): the frame, the page's size
    Snapshot(PathBuf, Sender<Result<(i64, u32, u32), String>>),
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
    std::thread::Builder::new().name("void stage".into()).spawn(move || run(metal, rx, st)).map_err(|e| e.to_string())?;
    *g = Some(Handle { layer, content, tx, status, scale: 2.0, sublayer });
    Ok(())
}

/// The stage's box in the window, in points from the content's top-left, and the screen's scale.
pub fn rect(x: f64, y: f64, w: f64, h: f64, scale: f64) {
    let mut g = STAGE.lock().unwrap();
    let Some(s) = g.as_mut() else { return };
    s.scale = scale;
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

pub fn control(c: Control) {
    if let Some(s) = STAGE.lock().unwrap().as_ref() {
        let _ = s.tx.send(Cmd::Control(c));
    }
}

/// A key the person pressed or let go while the stage has focus (a DOM key name: "ArrowLeft", " ", "a").
pub fn key(name: &str, down: bool) {
    if let Some(s) = STAGE.lock().unwrap().as_ref() {
        let _ = s.tx.send(Cmd::Key(name.to_string(), down));
    }
}

/// The pointer in the stage's box (points from its top-left) and whether a button is down.
pub fn pointer(x: f64, y: f64, down: bool) {
    if let Some(s) = STAGE.lock().unwrap().as_ref() {
        let k = s.scale;
        let _ = s.tx.send(Cmd::Pointer(x * k, y * k, down));
    }
}

/// The stage's page as it is now, read back to a PNG at `path` (void's playtest probe): the frame and the page's size.
pub fn snapshot(path: &Path) -> Result<(i64, u32, u32), String> {
    let (tx, rx) = channel();
    STAGE.lock().unwrap().as_ref().ok_or("no stage")?.tx.send(Cmd::Snapshot(path.to_path_buf(), tx)).map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(20)).map_err(|e| e.to_string())?
}

pub fn status() -> Status {
    STAGE.lock().unwrap().as_ref().map(|s| s.status.lock().unwrap().clone()).unwrap_or_default()
}

/// The stage thread: the object's live engine stepped at its rate and presented every refresh.
fn run(layer: usize, rx: Receiver<Cmd>, status: Arc<Mutex<Status>>) {
    let mut size = (1u32, 1u32);
    let mut stage: Option<raea::quads::Stage> = None;
    let mut live: Option<raea_react::live::Live> = None;
    let mut keys: Vec<String> = Vec::new();
    let mut pointer: Option<(f64, f64, bool)> = None;
    let mut playing = true;
    let mut looping = true;
    let mut composite = false;
    // the composition's clock: frame `base` at `since`
    let (mut base, mut since, mut frame) = (0i64, Instant::now(), -1i64);
    let (mut shown, mut window_start, mut last_present, mut missed, mut hidden) = (0u32, Instant::now(), None::<Instant>, 0u64, 0u32);
    let set_error = |e: String| {
        crate::mcp::log(format!("stage: {e}"));
        status.lock().unwrap().error = Some(e);
    };
    loop {
        // commands: wait for one when there's nothing to play, else take what's there
        let mut cmds: Vec<Cmd> = Vec::new();
        if live.is_none() || !playing {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(c) => cmds.push(c),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return,
            }
        }
        cmds.extend(rx.try_iter());
        crate::probe::drained();
        for c in cmds {
            match c {
                Cmd::Resize(w, h) => {
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
                        let opened = if looping { raea_react::live::Live::open_baked(&f.join("edit.tsx")) } else { raea_react::live::Live::open(&f.join("edit.tsx"), size) };
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
                                live = Some(l);
                                (base, since, frame) = (0, Instant::now(), -1);
                                playing = true;
                            }
                            Err(e) => set_error(format!("{}: {e}", f.display())),
                        }
                    }
                }
                Cmd::Control(Control::Play) => {
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
                }
                Cmd::Control(Control::Seek(f)) => {
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
                    }
                }
                Cmd::Control(Control::Fullscreen(_)) => {}
                Cmd::Key(k, down) => {
                    if down && !keys.contains(&k) {
                        keys.push(k);
                    } else if !down {
                        keys.retain(|x| *x != k);
                    }
                }
                Cmd::Pointer(x, y, down) => pointer = Some((x, y, down)),
                Cmd::Composited(on) => composite = on,
                Cmd::Snapshot(path, reply) => {
                    let r = match live.as_mut() {
                        Some(l) => l.save_snapshot(&path).map(|()| (frame, l.w, l.h)),
                        None => Err("nothing on the stage".into()),
                    };
                    let _ = reply.send(r);
                }
            }
        }
        let (Some(l), Some(s)) = (live.as_mut(), stage.as_ref()) else { continue };
        let t_work = Instant::now();
        if playing {
            let due = base + (since.elapsed().as_secs_f64() * l.fps).floor() as i64;
            let mut n = 0;
            while frame < due && n < 4 {
                frame += 1;
                let wrap = looping && frame >= l.frames;
                if wrap {
                    // past the end a timeline shows nothing of its own (the last frame's captions over the background)
                    (base, since, frame) = (0, Instant::now(), 0);
                }
                let lag = since.elapsed().as_secs_f64() - (frame - base) as f64 / l.fps;
                if let Err(e) = l.step_at(frame, &keys, pointer, lag) {
                    set_error(e);
                    playing = false;
                    break;
                }
                crate::probe::stepped(frame, &keys);
                n += 1;
                if wrap {
                    break;
                }
            }
            // far behind: the clock waits for the frames rather than skipping them (a skipped frame loses its input)
            if frame < due {
                (base, since) = (frame, Instant::now());
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
        // the loop waits for the composition's next frame itself
        if composite {
            match l.paint_texture(TEXTURE) {
                Ok(()) => {
                    crate::probe::painted(t_work, l.fps);
                    shown += 1;
                    // the callback taken out first: nothing of the stage's is held while it runs
                    let f = ON_FRAME.lock().unwrap().clone();
                    if let Some(f) = f {
                        f();
                    }
                }
                Err(e) => set_error(e),
            }
            status.lock().unwrap().page = (l.w, l.h);
            let next = since + Duration::from_secs_f64(((frame - base + 1) as f64 / l.fps).max(0.0));
            if let Some(d) = next.checked_duration_since(Instant::now()) {
                std::thread::sleep(d.min(Duration::from_millis(50)));
            }
            if window_start.elapsed() >= Duration::from_secs(1) {
                let mut st = status.lock().unwrap();
                st.fps = shown as f64 / window_start.elapsed().as_secs_f64();
                st.frame = frame;
                st.playing = playing;
                crate::mcp::log(format!("stage: composited frame {frame}, {:.0} fps, page {}x{}, error {:?}", st.fps, l.w, l.h, st.error));
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
