//! Live play: a composition run frame by frame at the display's rate and presented on a window's GPU surface, with the
//! person's keys and pointer as its input (One renderer, the native stage). The same React-on-V8 -> ECS -> bevy_ui page
//! path a render takes (src/page.rs), without settling loops, page files or readback: a frame is the commit carrying
//! its done mark (motion.rs settle), then the page drawn on the shared device and its texture shown (`draw`,
//! `texture`). `raea play` presents it in a window of its own (raea_gpu_device::present); the legacy app's stage hands the
//! texture to its renderer (apps/legacy/shell/src/stage_native.rs).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde_json::{json, Value};

use raea_runtime_react::{ReactOutbox, ReactPlugin};

mod open;
mod sound;
mod pointer;
mod pick3d;
pub use pick3d::want_wire;
pub use sound::warm_audio;
mod draw;
mod react;
pub use react::needs_react;
mod props;

type R<T> = Result<T, String>;

pub struct Live {
    app: App,
    dir: PathBuf,
    /// the page the frames are drawn on (bevy_ui, on the shared device)
    page: raea_runtime_page::BevyPage,
    /// the space's brand kit, the page's first sheet
    brand: Option<String>,
    /// a frame was stepped (or the page made again) since the page was last drawn
    dirty: bool,
    /// the sound, opened at the first frame that has any (None: no device, or nothing heard yet)
    mixer: Option<raea_runtime_sound::Mixer>,
    mixer_failed: bool,
    /// seconds the frame being stepped was late, and when that was measured (the sound is placed by the lag at its sync)
    lag: (f64, std::time::Instant),
    /// the page's size: the stage's for a responsive composition (it lays out from useVideoConfig), else its own
    pub w: u32,
    pub h: u32,
    pub fps: f64,
    /// the composition's length in frames (a timeline loops or stops there; a game runs on past it)
    pub frames: i64,
    pub responsive: bool,
    js: PathBuf,
    /// a timeline's mastered sound (motion::sound_only), being built on a thread, then a WAV the mixer streams in
    /// place of the elements' own sounds: what the render's track is, loudness and ceiling included
    master: Option<std::sync::mpsc::Receiver<R<Option<PathBuf>>>>,
    master_wav: Option<PathBuf>,
    /// a 3D scene's rows (motion::scene_program), being built on a thread, then raea-render's engine drawing them under the
    /// page on the paint device: the same 3D a render draws
    scene_rx: Option<std::sync::mpsc::Receiver<R<Option<raea_core_program::ir::Program>>>>,
    scene: Option<raea_render::engine::Engine>,
    /// a game's 3D, played live: its rows made from each frame's own state (motion::live_scene), drawn by `scene`
    live3d: Option<raea_runtime_react::scene::LiveScene>,
    /// the frame last stepped (what the 3D draws)
    frame: i64,
    /// the gamepads' marked keys the last frame was given beside its input ("Pad0:South"), for its input log
    pads: Vec<String>,
    /// the keys an app screen took from the last frame (the page didn't get them), left out of its input log, which
    /// keeps them under `apps` (`app_input`)
    taken: Vec<String>,
    /// the pointer on the page's elements, as the last frame left it (pointer.rs)
    pointer: pointer::PagePointer,
    /// generated sounds' files, by what they were made from (looked up once)
    made: std::collections::HashMap<String, Option<PathBuf>>,
    /// the frame's sounds, kept between frames (ZeroHeap)
    sound_buf: Vec<raea_runtime_sound::Sound>,
    /// the elements holding the 3D, and when each one's props were last made see-through (their change tick)
    holders: Vec<Entity>,
    /// a baked timeline (bake.rs): its frames replayed into the page, no JS running; and the frame the page shows
    baked: Option<std::sync::Arc<raea_runtime_bake::Baked>>,
    /// a game's master (the gain and ceiling its render masters its sound with), worked out on a thread, then held
    game_master: Option<std::sync::mpsc::Receiver<R<Option<(f64, f64)>>>>,
    game_gain: Option<(f64, f64)>,
    applied: i64,
    /// this session's seed and the time (ms) its frame 0 is at: its Math.random and Date (motion/pure.ts), sent with each
    /// frame and kept in an input log, so a replay draws what was played
    session: (u32, f64),
    /// frames told as `raea render` tells them (open_render): the composition's own seed, clock and length, no input
    render: bool,
    /// the props set over its defaults while it plays, and what they change that the 3D holds (props.rs)
    props: props::Props,
}

/// A live session's seed and start: RAEA_SEED when it's set (a test), else from the clock and the process
fn new_session() -> (u32, f64) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let seed = std::env::var("RAEA_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(|| (now.as_nanos() as u64 ^ ((std::process::id() as u64) << 32)).wrapping_mul(0x9e3779b97f4a7c15) >> 32) as u32;
    (seed, (now.as_millis() as f64).floor())
}

impl Live {
    /// A baked timeline's frame `f` put on the page: the next frame's commits, or (a jump) the keyframe before it with
    /// what's there removed first, then the commits after it.
    fn replay(&mut self, f: i64) {
        let Some(b) = self.baked.as_ref() else { return };
        let f = f.clamp(0, b.frames - 1);
        if f == self.applied {
            return;
        }
        let mut batches: Vec<String> = Vec::new();
        if f == self.applied + 1 {
            batches.extend(b.frame_ops[f as usize].iter().cloned());
        } else {
            let (k, key) = b.keys.iter().rev().find(|(k, _)| *k < f).map(|(k, ops)| (*k, ops)).unwrap_or((b.keys[0].0, &b.keys[0].1));
            // what's under the root now goes (the mirror and the ECS free a removed subtree), then the keyframe's tree
            batches.push(raea_runtime_bake::RESET.to_string());
            batches.push(Value::Array(key.clone()).to_string());
            for j in (k + 1)..=f {
                batches.extend(b.frame_ops[j as usize].iter().cloned());
            }
        }
        for j in batches {
            if j == raea_runtime_bake::RESET {
                // what's fed so far applied, then what's under the root removed (frames re-stepped after an edit come
                // with a tree of their own: bake_incremental)
                self.app.update();
                let world = self.app.world();
                let root = world.resource::<raea_runtime_react::ReactEntities>().by_id.get(&raea_runtime_react::ROOT_ID).copied();
                let ops: Vec<Value> = root.and_then(|r| world.get::<Children>(r)).map(|c| c.iter().filter_map(|e| world.get::<raea_runtime_react::ReactNode>(e)).map(|n| json!({"op": "remove", "parent": raea_runtime_react::ROOT_ID, "child": n.0})).collect()).unwrap_or_default();
                world.resource::<raea_runtime_react::ReplayFeed>().feed(Value::Array(ops).to_string());
            } else {
                self.app.world().resource::<raea_runtime_react::ReplayFeed>().feed(j);
            }
        }
        self.app.update();
        self.applied = f;
    }

    fn send(&self, name: &str, value: Value) {
        self.app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": name, "value": value}));
    }

    /// The stage's new size: a responsive composition lays out again at it; a fixed one is scaled to fit when presented.
    pub fn resize(&mut self, w: u32, h: u32) {
        if !self.responsive || (w, h) == (self.w, self.h) || w == 0 || h == 0 {
            return;
        }
        match raea_runtime_page::BevyPage::new(w, h, &self.dir) {
            Ok(p) => {
                (self.w, self.h) = (w, h);
                self.page = p;
                self.brand = raea_runtime_react::scene::page_css(&self.dir, w, &self.js);
                self.dirty = true;
                self.send("raea-size", json!({"w": w, "h": h}));
            }
            Err(e) => eprintln!("raea live: the page at {w}x{h} didn't open: {e}"),
        }
    }

    /// Frame `f` with this moment's input: React renders it and its commit lands in the page.
    /// Where the point (`x`, `y`, the stage's pixels) lands on a screen in the 3D, as the pointer reaches an app screen
    /// (runtime/page/src/screen.rs route): the screen's name and the point on its picture, 0..1 across and down.
    pub fn screen_at(&mut self, x: f64, y: f64) -> Option<(String, f64, f64)> {
        let (w, h) = (self.w.max(1) as f64, self.h.max(1) as f64);
        self.scene.as_mut()?.screen_at(x / w, y / h)
    }

    /// The element under the point (`x`, `y` in page px) as the frame was last drawn, by the page's hit test, as
    /// `element` describes it. What the feed's play stage picks (engine/page/host/src/stage.rs).
    pub fn element_at(&mut self, x: f64, y: f64) -> Option<Value> {
        let id = self.page.hit(x as f32, y as f32)?;
        self.element(id)
    }

    /// An element of the page as it is now: its id, kind, the components it was rendered in (`owner`, nearest first,
    /// from live play's runtime), its box in page px as drawn, its props (handlers and children left out) and its text.
    pub fn element(&mut self, id: u64) -> Option<Value> {
        let rect = self.page.client_box(id);
        let w = self.app.world_mut();
        let mut q = w.query::<(&raea_runtime_react::ReactNode, &raea_runtime_react::ReactKind, Option<&raea_runtime_react::ReactProps>, Option<&Children>)>();
        let (_, kind, props, kids) = q.iter(w).find(|(n, ..)| n.0 == id)?;
        let mut props = props.map(|p| p.0.clone()).unwrap_or_default();
        let owner = props.remove("data-raea-owner").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        // a 3D part's element holds its own props as the SDK's JSON (data-props) and its kind as data-3d: answered as
        // those, so an inspector that asks again by id shows the part's props, not the holder's
        let mut kind = kind.0.clone();
        let mut part = None;
        if let (Some(k), Some(own)) = (props.get("data-3d").and_then(Value::as_str).map(str::to_string), props.get("data-props").and_then(Value::as_str).and_then(|t| serde_json::from_str::<serde_json::Map<String, Value>>(t).ok())) {
            kind = k;
            part = raea_sys_frame::layer::key_from_props(&own);
            props = own;
            props.retain(|k, _| !k.starts_with("data-raea-"));
        }
        props.retain(|k, v| k != "children" && !(k.starts_with("on") && k[2..].starts_with(|c: char| c.is_ascii_uppercase())) && !v.is_null());
        let text: String = kids.map(|ks| ks.iter().filter_map(|e| w.get::<raea_runtime_react::ReactText>(e).map(|t| t.0.clone())).collect()).unwrap_or_default();
        let mut e = json!({"id": id, "kind": kind, "owner": owner, "rect": rect, "props": props, "text": text});
        // a 3D part's box is where it is drawn now, not its holder's (which has none): an inspector asking again by id
        // as the scene moves lost the outline it had from the click
        if let Some(key) = part {
            let at = self.locate(std::slice::from_ref(&key), &[])["parts"][0].take();
            if at.is_array() {
                e["rect"] = at;
            }
            e["part"] = json!(true);
            e["key"] = json!(key);
        }
        Some(e)
    }

    pub fn step(&mut self, f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>) -> R<()> {
        self.step_at(f, keys, pointer, 0.0)
    }

    /// `step_at`, its sound synced too when `sound`.
    fn step_inner(&mut self, f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>, lag: f64, sound: bool) -> R<()> {
        self.lag = (lag, std::time::Instant::now());
        // the 3D's holds worked out after the last props (props.rs) were this frame's
        self.props.base = None;
        self.frame = f;
        self.pads.clear();
        self.taken.clear();
        // the app screens step to this frame at the page's next sync (screen.rs step_all)
        self.page.set_app_frame(f, self.fps);
        let t0 = std::time::Instant::now();
        if self.baked.is_some() {
            // a baked timeline: its recorded commits, no JS; the clips it cuts to soon opened ahead; the page styled at
            // the frame's time (its CSS animations), as the render styled it
            self.page.at_frame(f, self.fps);
            self.replay(f);
        } else if self.live3d.as_mut().is_some_and(|l| std::mem::take(&mut l.warm)) && f == 0 && keys.is_empty() && pointer.is_none() {
            // frame 0 with no input is on the page already (stepped when it opened): stepped again it commits nothing,
            // and the settle waited out its 0.5 s for a commit
        } else {
            // the gamepads the 3D's Bevy App sees (bevy_gilrs reads them into its Gamepad entities each update), as the
            // live wire's marked keys "Pad0:South", "Pad0:LeftStickX=0.42" (docs/bevy-ts.feature G3b)
            let pads = self.scene.as_mut().map(|e| e.gamepad_keys()).unwrap_or_default();
            self.pads = pads.iter().filter(|p| !keys.contains(p)).cloned().collect();
            let with_pads: Option<Vec<String>> = (!pads.is_empty()).then(|| keys.iter().cloned().chain(pads.into_iter().filter(|p| !keys.contains(p))).collect());
            let keys = with_pads.as_deref().unwrap_or(keys);
            // an app screen's input (runtime/page/src/screen.rs): where the pointer lands on it through the 3D, and the keys while
            // it holds them, which the page then doesn't get
            let routed = match self.scene.as_mut() {
                Some(e) => Some(raea_runtime_page::screen::route(&mut self.page, e, keys, pointer, (self.w, self.h))),
                None => None,
            };
            if let Some(r) = &routed {
                self.taken = keys.iter().filter(|k| !r.contains(k)).cloned().collect();
            }
            let keys = routed.as_deref().unwrap_or(keys);
            // the pointer on the page's elements: their on* handlers, before the frame's step (pointer.rs)
            if let Some(p) = pointer {
                let path = self.page.hit_path(p.0 as f32, p.1 as f32);
                for m in self.pointer.step(p, path) {
                    self.app.world().resource::<ReactOutbox>().send(m);
                }
            }
            self.app.world().resource::<ReactOutbox>().send_raw(if self.render { format!(r#"{{"t":"event","name":"raea-frame","value":{f}}}"#) } else { frame_event(f, keys, pointer, Some(self.session)) });
            // a layout read the composition makes (docs/layout-reads.feature LR2): the live page synced to the World as
            // it is mid-settle, laid out, and its boxes; the frame's draw syncs it again with the 3D's holders. Preview,
            // play, the app's stage and an exported game all step here
            let (page, brand) = (&mut self.page, self.brand.as_deref());
            let mut lay = |w: &mut World, q: &str| {
                page.sync(w, &std::collections::HashMap::new(), brand).map_err(|e| e.to_string())?;
                page.lay_out();
                Ok(page.layout_snapshot(q))
            };
            let seen = raea_runtime_react::flushed();
            raea_runtime_react::scene::settle_with(&mut self.app, Some(f), 5.0, &mut lay)?;
            // a props pass (set_props): the frame is taken once its marker says the props have landed, not at the mark
            // the frame already had, which a paused stage drew again unchanged (docs/realtime.feature RT4e)
            if let Some(n) = self.props.wanted.take() {
                props::landed(&mut self.app, &mut self.props.marker, n, seen, &mut |app| raea_runtime_react::scene::settle_with(app, Some(f), 5.0, &mut lay).map(drop))?;
            }
            // a game's 3D: this frame's scene, as it was just computed, set on the live engine
            if let Some(ls) = self.live3d.as_mut() {
                let t = std::time::Instant::now();
                raea_runtime_export::live_scene(self.app.world_mut(), ls, &mut self.scene, (self.w, self.h), self.fps, &self.dir)?;
                if raea_io_trace::stat("RAEA_LIVE_TRACE") && t.elapsed().as_secs_f64() > 0.002 {
                    eprintln!("raea live: frame {f}: its 3D rows {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
                }
            }
        }
        self.dirty = true;
        let t1 = std::time::Instant::now();
        if sound {
            self.sound(f);
        }
        if raea_io_trace::stat("RAEA_LIVE_TRACE") && t0.elapsed().as_secs_f64() > 0.02 {
            eprintln!("raea live: frame {f}: settle {:.1} ms, sound {:.1} ms", (t1 - t0).as_secs_f64() * 1e3, t1.elapsed().as_secs_f64() * 1e3);
        }
        Ok(())
    }

    /// A composition's 3D drawn under the page as its render draws it: its rows built on a thread from now (React
    /// stepped through every frame), then raea-render's engine opened on this thread and drawing each frame presented.
    pub fn scene_3d(&mut self) {
        // played live, the 3D follows each frame's state already (live3d)
        if self.scene_rx.is_some() || self.scene.is_some() || self.live3d.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (js, dir) = (self.js.clone(), self.dir.clone());
        if std::thread::Builder::new().name("raea live 3d".into()).spawn(move || {
            let _ = tx.send(raea_runtime_export::scene_program(&js, &dir));
        }).is_ok() {
            self.scene_rx = Some(rx);
        }
    }

    /// The 3D's rows waited for (at most `secs`) and its engine opened: a check's snapshots all have their 3D.
    pub fn wait_3d(&mut self, secs: f64) -> R<()> {
        if let Some(rx) = self.scene_rx.take() {
            match rx.recv_timeout(std::time::Duration::from_secs_f64(secs)) {
                Ok(Ok(Some(prog))) => self.scene = Some(raea_render::engine::Engine::new_live(&prog, &raea_render::inserted_of(&prog), &self.dir).map_err(|e| format!("the 3D didn't open: {e}"))?),
                Ok(Ok(None)) => {}
                Ok(Err(e)) => return Err(format!("the 3D's rows failed: {e}")),
                Err(e) => return Err(format!("the 3D's rows didn't arrive: {e}")),
            }
        }
        Ok(())
    }

}

/// This build (its binary's modification time): a master or mastered track cached by another build of the core may be
/// another core's (a fix that moved a sample once left a game a hair louder in its play than in its render).
fn build_id() -> u64 {
    std::env::current_exe().ok().and_then(|e| e.metadata().ok()).and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.js);
        // the mastered track stays: it's the cache the next open plays at once (master_sound prunes old ones)
    }
}

fn rand_id() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0));
    h.finish()
}

fn components_use_config(dir: &Path) -> bool {
    std::fs::read_dir(dir.join("components")).into_iter().flatten().flatten().any(|e| std::fs::read_to_string(e.path()).is_ok_and(|t| t.contains("useVideoConfig(")))
}

/// What a composition's sound is made of: its bundle and every file in its project (path, size, time), hashed, so a
/// sound worked out from them (the mastered track) is found again until one of them changes.
fn sound_key(js: &Path, dir: &Path) -> u64 {
    let mut h: Vec<u8> = std::fs::read(js).unwrap_or_default();
    fn walk(d: &Path, h: &mut Vec<u8>, depth: usize) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        let mut es: Vec<_> = rd.flatten().collect();
        es.sort_by_key(|e| e.file_name());
        for e in es {
            let name = e.file_name();
            if name.to_string_lossy().starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            // through symlinks too (media is often linked in from a drive)
            let Ok(m) = std::fs::metadata(e.path()) else { continue };
            h.extend_from_slice(e.path().to_string_lossy().as_bytes());
            if m.is_dir() {
                if depth < 4 {
                    walk(&e.path(), h, depth + 1);
                }
            } else {
                h.extend_from_slice(&m.len().to_le_bytes());
                h.extend_from_slice(&m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos() as u64).unwrap_or(0).to_le_bytes());
            }
        }
    }
    let bundle = twox_hash::XxHash3_64::oneshot(&h);
    walk(dir, &mut h, 0);
    if raea_io_trace::stat("RAEA_SOUND_TRACE") {
        eprintln!("raea live: sound key: bundle {bundle:016x} ({} bytes), with the project {:016x}", std::fs::metadata(js).map(|m| m.len()).unwrap_or(0), twox_hash::XxHash3_64::oneshot(&h));
    }
    twox_hash::XxHash3_64::oneshot(&h)
}

/// A step's parts: the frame stepped, and its sound synced. `step_at` is both.
impl Live {
    /// `step`, `lag` seconds after the frame's own time (the sound is placed where it should be now).
    pub fn step_at(&mut self, f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>, lag: f64) -> R<()> {
        self.step_inner(f, keys, pointer, lag, true)
    }

    /// `step_at` without the sound: the frame's commit in the page and its videos placed.
    pub fn step_world(&mut self, f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>, lag: f64) -> R<()> {
        self.step_inner(f, keys, pointer, lag, false)
    }

    /// The keys the last frame stepped was given: `keys` and the gamepads marked beside them, as its input log records
    /// them, so a render of the log gives the game the same pads (bench/render-play's live-gamepad).
    pub fn played_keys(&self, keys: &[String]) -> Vec<String> {
        keys.iter().chain(&self.pads).filter(|k| !self.taken.contains(k)).cloned().collect()
    }

    /// What reached the app screens in the last frame stepped, as its input log keeps it (`apps`): a render of the
    /// log gives them the same (runtime/page/src/screen.rs).
    pub fn app_input(&self) -> Option<serde_json::Value> {
        raea_runtime_page::screen::logged(&self.page)
    }
}

/// `keys` with the wheel turned since the last frame as `Wheel:Y DY` (CSS px, down positive, as the DOM's deltaY
/// is): what reaches an app screen under the pointer (runtime/page/src/screen.rs route). The keys as they are when it didn't turn.
pub fn keys_with_wheel<'a>(keys: &'a [String], dy: &mut f64) -> std::borrow::Cow<'a, [String]> {
    if *dy == 0.0 {
        return std::borrow::Cow::Borrowed(keys);
    }
    let mut k = keys.to_vec();
    k.push(wheel_key(std::mem::take(dy)));
    std::borrow::Cow::Owned(k)
}

/// The live wire's key for the wheel turned `dy` CSS px this frame (down positive); the one place its name is spelled.
pub fn wheel_key(dy: f64) -> String {
    format!("Wheel:Y {dy}")
}

/// A wheel event's turn in CSS px, down positive (the DOM's deltaY): lines at 40 px each, pixels as they come.
pub fn wheel_px(d: &winit::event::MouseScrollDelta, scale: f64) -> f64 {
    match d {
        winit::event::MouseScrollDelta::LineDelta(_, y) => -*y as f64 * 40.0,
        winit::event::MouseScrollDelta::PixelDelta(p) => -p.y / scale.max(0.1),
    }
}

/// The frame's "raea-frame" event, written straight into its message (ZeroHeap: built as a json! tree and serialized, a
/// map, key strings and the keys array a frame): {"t":"event","name":"raea-frame","value":{"frame":f,"input":{"keys":[..],
/// "pointer":{"x","y","down"}}}}, which the page reads the same way.
fn frame_event(f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>, session: Option<(u32, f64)>) -> String {
    use std::fmt::Write as _;
    let mut m = String::with_capacity(96 + keys.iter().map(|k| k.len() + 3).sum::<usize>());
    let _ = write!(m, r#"{{"t":"event","name":"raea-frame","value":{{"frame":{f},"input":{{"keys":["#);
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            m.push(',');
        }
        if k.bytes().all(|b| b >= 0x20 && b != b'"' && b != b'\\') {
            m.push('"');
            m.push_str(k);
            m.push('"');
        } else {
            m.push_str(&Value::from(k.as_str()).to_string());
        }
    }
    m.push(']');
    if let Some((x, y, down)) = pointer {
        let num = |m: &mut String, v: f64| {
            let _ = match serde_json::Number::from_f64(v) {
                Some(n) => write!(m, "{n}"),
                None => write!(m, "null"),
            };
        };
        m.push_str(r#","pointer":{"x":"#);
        num(&mut m, x);
        m.push_str(r#","y":"#);
        num(&mut m, y);
        let _ = write!(m, r#","down":{down}}}"#);
    }
    m.push_str("}");
    if let Some((seed, start)) = session {
        let _ = write!(m, r#","session":{{"seed":{seed},"start":{start}}}"#);
    }
    m.push_str("}}");
    m
}

#[cfg(test)]
mod frame_event_tests {
    use super::*;

    #[test]
    fn frame_event_matches_json() {
        for (keys, pointer) in [(vec![], None), (vec!["ArrowUp".to_string(), "a\"b\\".into(), "\u{1}".into()], Some((1.5, -2.0, true))), (vec!["x".into()], Some((f64::NAN, 0.0, false)))] {
            let mut input = json!({"keys": keys});
            if let Some((x, y, down)) = pointer {
                input["pointer"] = json!({"x": x, "y": y, "down": down});
            }
            let want = json!({"t": "event", "name": "raea-frame", "value": {"frame": 7, "input": input}});
            let got: Value = serde_json::from_str(&frame_event(7, &keys, pointer, None)).unwrap();
            assert_eq!(got, want);
        }
    }
}
