//! Live play: a composition run frame by frame at the display's rate and presented on a window's GPU surface, with the
//! person's keys and pointer as its input (One renderer, the native stage). The same React-on-V8 -> ECS page -> Blitz
//! -> quads path a render takes, without settling loops, page files or readback: a frame is the commit carrying its
//! done mark (motion.rs settle), then the page presented. `raea-motion play` drives it in a window of its own; void's
//! stage drives it in its window (apps/void/src/stage_native.rs).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde_json::{json, Value};

use crate::{DomMirror, ReactOutbox, ReactPlugin};

type R<T> = Result<T, String>;

pub struct Live {
    app: App,
    dir: PathBuf,
    videos: crate::motion::VideoFrames,
    /// the sound, opened at the first frame that has any (None: no device, or nothing heard yet)
    mixer: Option<crate::audio::Mixer>,
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
    /// a 3D scene's rows (motion::scene_program), being built on a thread, then rdb's engine drawing them under the
    /// page on the paint device: the same 3D a render draws
    scene_rx: Option<std::sync::mpsc::Receiver<R<Option<rdb::ir::Program>>>>,
    scene: Option<rdb::engine::Engine>,
    /// a game's 3D, played live: its rows made from each frame's own state (motion::live_scene), drawn by `scene`
    live3d: Option<crate::motion::LiveScene>,
    /// the frame last stepped (what the 3D draws)
    frame: i64,
    /// generated sounds' files, by what they were made from (looked up once)
    made: std::collections::HashMap<String, Option<PathBuf>>,
    /// the frame's sounds, kept between frames (ZeroHeap)
    sound_buf: Vec<crate::audio::Sound>,
    /// the elements holding the 3D, and when each one's props were last made see-through (their change tick)
    holders: Vec<Entity>,
    see_through: std::collections::HashMap<Entity, bevy::ecs::change_detection::Tick>,
    /// a baked timeline (bake.rs): its frames replayed into the page, no JS running; and the frame the page shows
    baked: Option<std::sync::Arc<crate::bake::Baked>>,
    /// the last frame of a baked timeline looked through for videos about to show (prefetch_ahead)
    scanned: i64,
    /// a game's master (the gain and ceiling its render masters its sound with), worked out on a thread, then held
    game_master: Option<std::sync::mpsc::Receiver<R<Option<(f64, f64)>>>>,
    game_gain: Option<(f64, f64)>,
    applied: i64,
}

/// The texture id a live 3D scene's frame is drawn under the page by (no React node has it).
const SCENE_TEXTURE: u64 = u64::MAX - 3;

/// What one frame cost, in milliseconds.
#[derive(Debug, Default, Clone, Copy)]
pub struct Times {
    pub react_ms: f64,
    pub present_ms: f64,
}

impl Live {
    /// The composition in `tsx` (an edit.tsx and what it imports), at `stage` pixels when it's responsive.
    pub fn open(tsx: &Path, stage: (u32, u32)) -> R<Live> {
        Self::open_with(tsx, stage, true)
    }

    /// `open`, laid out at the stage's size only when `may_respond` and the composition lays out from useVideoConfig.
    /// The rule (the founder, 2026-10-18): only games may be responsive. Video and everything played like it (timelines,
    /// motion, decks, scenes; open_baked too) keep the composition's own layout at its own size, never re-flowed to the
    /// window's box; void's stage only scales that page uniformly (paint_texture_fit). Don't flip this per kind.
    pub fn open_with(tsx: &Path, stage: (u32, u32), may_respond: bool) -> R<Live> {
        warm_audio();
        // the thread that steps and presents live frames, at the frame threads' QoS (as the JS thread)
        crate::interactive_thread();
        raea_native::trace::event("open", serde_json::json!({"composition": tsx, "stage": [stage.0, stage.1]}));
        let tsx = tsx.canonicalize().map_err(|e| format!("{}: {e}", tsx.display()))?;
        let dir = tsx.parent().unwrap().to_path_buf();
        let js = std::env::temp_dir().join(format!("raea-live-{}-{:x}.js", std::process::id(), rand_id()));
        // the project's words and perception, as a render passes them (captions, regions)
        let mut opts: Vec<String> = Vec::new();
        for (flag, file) in [("--words", "words.json"), ("--perception", "perception.json")] {
            let p = dir.join(file);
            if p.exists() {
                opts.push(flag.into());
                opts.push(p.display().to_string());
            }
        }
        // played live (a game: timelines play their bake), on raea's live view runtime instead of React: each frame
        // rendered and only what changed sent (RAEA_LIVE_REACT=1 keeps React)
        if std::env::var("RAEA_LIVE_REACT").as_deref() != Ok("1") {
            opts.push("--lite".into());
        }
        crate::motion::bundle_composition(&tsx, &js, &crate::motion::comp_opts(&opts)?, None)?;
        let src = std::fs::read_to_string(&tsx).unwrap_or_default();
        let responsive = may_respond && (src.contains("useVideoConfig(") || components_use_config(&dir));
        Self::from_bundle(js, dir, stage, responsive)
    }

    /// A composition already bundled for live play (an exported game, game.rs): `js` is copied to the temp folder (the
    /// copy is what Live removes when it's dropped), `dir` is where its media resolves. No bun runs.
    pub fn open_bundle(js: &Path, dir: &Path, stage: (u32, u32), responsive: bool) -> R<Live> {
        crate::interactive_thread();
        let tmp = std::env::temp_dir().join(format!("raea-live-{}-{:x}.js", std::process::id(), rand_id()));
        std::fs::copy(js, &tmp).map_err(|e| format!("{}: {e}", js.display()))?;
        Self::from_bundle(tmp, dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?, stage, responsive)
    }

    /// The game's master as worked out ahead of time (an exported game carries it): no second React at run time.
    pub fn set_game_gain(&mut self, g: Option<(f64, f64)>) {
        self.game_gain = g;
    }

    fn from_bundle(js: PathBuf, dir: PathBuf, stage: (u32, u32), responsive: bool) -> R<Live> {
        // a game that says lines (@raea/motion/speech): Pocket TTS loaded on its worker now, before the clock starts
        #[cfg(feature = "ml")]
        {
            crate::native::ml::set_dir(&dir);
            if crate::native::ml::bundle_speaks(&js) {
                crate::native::ml::warm();
            }
        }
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.clone()));
        crate::motion::one_thread(&mut app);
        app.insert_non_send(DomMirror::new_page(stage.0.max(1), stage.1.max(1), &dir));
        let conf = crate::motion::settle_frame(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
        let n = |k: &str, d: f64| conf.get(k).and_then(Value::as_f64).unwrap_or(d);
        let (cw, ch, fps, frames) = (n("width", 1080.0) as u32, n("height", 1920.0) as u32, n("fps", 30.0), n("frames", 1.0).max(1.0) as i64);
        app.world_mut().non_send_resource_mut::<DomMirror>().fps = fps;
        let (w, h) = if responsive { (stage.0.max(1), stage.1.max(1)) } else { (cw, ch) };
        let mut live = Live { app, dir: dir.clone(), videos: crate::motion::VideoFrames::new(), mixer: None, mixer_failed: false, lag: (0.0, std::time::Instant::now()), w, h, fps, frames, responsive, js, master: None, master_wav: None, game_master: None, game_gain: None, scene_rx: None, scene: None, live3d: None, frame: 0, made: Default::default(), sound_buf: Vec::new(), holders: Vec::new(), see_through: Default::default(), baked: None, applied: -1, scanned: -1 };
        {
            let brand = crate::motion::page_css(&dir, w, &live.js);
            let mut m = live.app.world_mut().non_send_resource_mut::<DomMirror>();
            m.dom.resize(w, h);
            let body = m.dom.body();
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
        if responsive {
            live.send("raea-size", json!({"w": w, "h": h}));
        }
        // 3D (@raea/motion/3d in the bundle): drawn from each frame's own state, as it's played. Frame 0 (the game's
        // initial state, whatever the input) stepped now, so its scene's engine opens before the clock starts.
        // RAEA_LIVE_3D=0: no 3D played live (as before 2026-10-18)
        if std::env::var("RAEA_LIVE_3D").as_deref() != Ok("0") && std::fs::read_to_string(&live.js).is_ok_and(|t| t.contains("data-3d")) {
            live.live3d = Some(Default::default());
            live.step_world(0, &[], None, 0.0)?;
            live.live3d.as_mut().unwrap().warm = true;
        }
        // <Seedling> (@raea/motion/seedling): seedling's graph made now, before the clock starts (made on frame 0 it
        // took 50 ms, and the frames after came too late for their samples)
        crate::audio::warm_seedling(live.app.world_mut(), live.fps, &live.js, &live.dir);
        Ok(live)
    }

    /// A timeline baked (bake.rs) and played from its bake: React steps through every frame once, here and now, and
    /// from then on each frame is its recorded commits applied to the page, no JS running (a seek: the keyframe before
    /// it, then the frames after). The page is the composition's own size.
    pub fn open_baked(tsx: &Path) -> R<Live> {
        warm_audio();
        let tsx = tsx.canonicalize().map_err(|e| format!("{}: {e}", tsx.display()))?;
        let dir = tsx.parent().unwrap().to_path_buf();
        let js = std::env::temp_dir().join(format!("raea-live-{}-{:x}.js", std::process::id(), rand_id()));
        let o = baked_opts(&dir)?;
        // RAEA_BAKE_FROM=OLD.tsx: OLD baked first and this edit baked from it (bake_incremental), as an edit re-bakes
        let baked = match std::env::var("RAEA_BAKE_FROM").ok() {
            Some(from) => {
                let old_js = std::env::temp_dir().join(format!("raea-live-{}-{:x}-from.js", std::process::id(), rand_id()));
                crate::motion::bundle_composition(Path::new(&from), &old_js, &o, None)?;
                let old = crate::bake::bake(&old_js)?;
                let _ = std::fs::remove_file(&old_js);
                crate::motion::bundle_composition(&tsx, &js, &o, None)?;
                let t = std::time::Instant::now();
                let (b, stepped) = crate::bake::bake_incremental(&old, &js)?;
                eprintln!("raea bake: incremental, {stepped} of {} frames stepped, {:.0} ms", b.frames, t.elapsed().as_secs_f64() * 1e3);
                b
            }
            None => {
                crate::motion::bundle_composition(&tsx, &js, &o, None)?;
                crate::bake::bake_cached(&js, build_id())?
            }
        };
        let (w, h, fps, frames) = (baked.w, baked.h, baked.fps, baked.frames);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(crate::ReplayPlugin);
        crate::motion::one_thread(&mut app);
        let mut page = DomMirror::new_page(w, h, &dir);
        page.fps = fps;
        app.insert_non_send(page);
        let mut live = Live { app, dir: dir.clone(), videos: crate::motion::VideoFrames::new(), mixer: None, mixer_failed: false, lag: (0.0, std::time::Instant::now()), w, h, fps, frames, responsive: false, js, master: None, master_wav: None, game_master: None, game_gain: None, scene_rx: None, scene: None, live3d: None, frame: 0, made: Default::default(), sound_buf: Vec::new(), holders: Vec::new(), see_through: Default::default(), baked: None, applied: -1, scanned: -1 };
        {
            let brand = crate::motion::page_css(&dir, w, &live.js);
            let mut m = live.app.world_mut().non_send_resource_mut::<DomMirror>();
            m.dom.resize(w, h);
            let body = m.dom.body();
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
        for b in &baked.init {
            live.app.world().resource::<crate::ReplayFeed>().feed(b.clone());
        }
        live.app.update();
        // 3D: its rows (the scene_program a 3D render uses) and rdb's live engine opened now, so the first frame draws
        // its scene under the page as the render does (it came from a thread a few frames late, and only with --3d)
        if baked.has_3d {
            if let Some(prog) = crate::motion::scene_program(&live.js, &dir)? {
                live.scene = Some(rdb::engine::Engine::new_live(&prog, &rdb::inserted_of(&prog), &dir).map_err(|e| format!("the 3D didn't open: {e}"))?);
            }
        }
        live.baked = Some(std::sync::Arc::new(baked));
        // the first frames' clips opened while the page gets ready
        live.prefetch_ahead(-1);
        // and frame 0 made ready before the clock starts, as a player loads before it plays: its commits applied, its
        // videos in place (the prefetched decoders taken), the mixer opened and its sounds loading, none started. The
        // first frame waited ~195 ms for these (a 4K clip's open, the device and the first sound's load)
        live.replay(0);
        crate::motion::place_videos(live.app.world_mut(), &live.dir, &mut live.videos)?;
        live.warm_sound();
        Ok(live)
    }

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
            batches.push(crate::bake::RESET.to_string());
            batches.push(Value::Array(key.clone()).to_string());
            for j in (k + 1)..=f {
                batches.extend(b.frame_ops[j as usize].iter().cloned());
            }
        }
        for j in batches {
            if j == crate::bake::RESET {
                // what's fed so far applied, then what's under the root removed (frames re-stepped after an edit come
                // with a tree of their own: bake_incremental)
                self.app.update();
                self.videos.forget_shown();
                let world = self.app.world();
                let root = world.resource::<crate::ReactEntities>().by_id.get(&crate::ROOT_ID).copied();
                let ops: Vec<Value> = root.and_then(|r| world.get::<Children>(r)).map(|c| c.iter().filter_map(|e| world.get::<crate::ReactNode>(e)).map(|n| json!({"op": "remove", "parent": crate::ROOT_ID, "child": n.0})).collect()).unwrap_or_default();
                world.resource::<crate::ReplayFeed>().feed(Value::Array(ops).to_string());
            } else {
                self.app.world().resource::<crate::ReplayFeed>().feed(j);
            }
        }
        self.app.update();
        self.applied = f;
    }

    /// A baked timeline's next frames looked through for the videos they show (each one's source and time, from its
    /// recorded commits): a file not open yet is opened and seeked on a worker now (VideoFrames::prefetch), so the frame
    /// that cuts to it doesn't wait (a cut to a 4K clip cost 120 ms, three frames missed at 25 fps).
    fn prefetch_ahead(&mut self, f: i64) {
        const LOOK: i64 = 15;
        // RAEA_VIDEO_PREFETCH=0: clips opened when they show, as live play does
        if std::env::var("RAEA_VIDEO_PREFETCH").as_deref() == Ok("0") {
            return;
        }
        let Some(b) = self.baked.as_ref() else { return };
        if f > self.scanned || f + LOOK < self.scanned {
            // a seek: from here
            self.scanned = f;
        }
        let end = (f + LOOK).min(b.frames - 1);
        let mut found: Vec<(String, f64)> = Vec::new();
        for g in (self.scanned + 1).max(0)..=end {
            for j in &b.frame_ops[g as usize] {
                if !j.contains("data-source-time") {
                    continue;
                }
                let Ok(Value::Array(ops)) = serde_json::from_str::<Value>(j) else { continue };
                for op in &ops {
                    let Some(p) = op.get("props") else { continue };
                    if let (Some(src), Some(at)) = (p.get("src").and_then(Value::as_str), p.get("data-source-time").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))) {
                        found.push((src.to_string(), at));
                    }
                }
            }
        }
        self.scanned = self.scanned.max(end);
        for (src, at) in found {
            let file = crate::motion::media_path(&self.dir, &src);
            self.videos.prefetch(&file, at);
        }
    }

    fn send(&self, name: &str, value: Value) {
        self.app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": name, "value": value}));
    }

    /// The stage's new size: a responsive composition lays out again at it; a fixed one is scaled to fit when presented.
    pub fn resize(&mut self, w: u32, h: u32) {
        if !self.responsive || (w, h) == (self.w, self.h) || w == 0 || h == 0 {
            return;
        }
        (self.w, self.h) = (w, h);
        self.app.world_mut().non_send_resource_mut::<DomMirror>().dom.resize(w, h);
        self.app.world_mut().non_send_resource_mut::<DomMirror>().dirty = true;
        self.send("raea-size", json!({"w": w, "h": h}));
    }

    /// Frame `f` with this moment's input: React renders it and its commit lands in the page.
    pub fn step(&mut self, f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>) -> R<()> {
        self.step_at(f, keys, pointer, 0.0)
    }

    /// `step_at`, its sound synced too when `sound` (runner.rs syncs it in a system of its own: step_world, sync_sound).
    fn step_inner(&mut self, f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>, lag: f64, sound: bool) -> R<()> {
        self.lag = (lag, std::time::Instant::now());
        self.frame = f;
        let t0 = std::time::Instant::now();
        if self.baked.is_some() {
            // a baked timeline: its recorded commits, no JS; the clips it cuts to soon opened ahead; the page styled at
            // the frame's time (its CSS animations), as the render styled it
            self.app.world_mut().non_send_resource_mut::<DomMirror>().at_frame(f);
            self.replay(f);
            self.prefetch_ahead(f);
        } else if self.live3d.as_mut().is_some_and(|l| std::mem::take(&mut l.warm)) && f == 0 && keys.is_empty() && pointer.is_none() {
            // frame 0 with no input is on the page already (stepped when it opened): stepped again it commits nothing,
            // and the settle waited out its 0.5 s for a commit
        } else {
            self.app.world().resource::<ReactOutbox>().send_raw(frame_event(f, keys, pointer));
            crate::motion::settle_frame(&mut self.app, Some(f), 5.0)?;
            // a game's 3D: this frame's scene, as it was just computed, set on the live engine
            if let Some(ls) = self.live3d.as_mut() {
                let t = std::time::Instant::now();
                crate::motion::live_scene(self.app.world_mut(), ls, &mut self.scene, (self.w, self.h), self.fps, &self.dir)?;
                if std::env::var_os("RAEA_LIVE_TRACE").is_some() && t.elapsed().as_secs_f64() > 0.002 {
                    eprintln!("raea live: frame {f}: its 3D rows {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
                }
            }
        }
        let t1 = std::time::Instant::now();
        // the frame's videos as textures on the GPU
        crate::motion::place_videos(self.app.world_mut(), &self.dir, &mut self.videos)?;
        let t2 = std::time::Instant::now();
        if sound {
            self.sound(f);
        }
        if std::env::var_os("RAEA_LIVE_TRACE").is_some() && t0.elapsed().as_secs_f64() > 0.02 {
            eprintln!("raea live: frame {f}: settle {:.1} ms, videos {:.1} ms, sound {:.1} ms", (t1 - t0).as_secs_f64() * 1e3, (t2 - t1).as_secs_f64() * 1e3, t2.elapsed().as_secs_f64() * 1e3);
        }
        Ok(())
    }

    /// A composition's 3D drawn under the page as its render draws it: its rows built on a thread from now (React
    /// stepped through every frame), then rdb's engine opened on this thread and drawing each frame presented.
    pub fn scene_3d(&mut self) {
        // played live, the 3D follows each frame's state already (live3d)
        if self.scene_rx.is_some() || self.scene.is_some() || self.live3d.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (js, dir) = (self.js.clone(), self.dir.clone());
        if std::thread::Builder::new().name("raea live 3d".into()).spawn(move || {
            let _ = tx.send(crate::motion::scene_program(&js, &dir));
        }).is_ok() {
            self.scene_rx = Some(rx);
        }
    }

    /// The 3D's rows waited for (at most `secs`) and its engine opened: a check's snapshots all have their 3D.
    pub fn wait_3d(&mut self, secs: f64) -> R<()> {
        if let Some(rx) = self.scene_rx.take() {
            match rx.recv_timeout(std::time::Duration::from_secs_f64(secs)) {
                Ok(Ok(Some(prog))) => self.scene = Some(rdb::engine::Engine::new_live(&prog, &rdb::inserted_of(&prog), &self.dir).map_err(|e| format!("the 3D didn't open: {e}"))?),
                Ok(Ok(None)) => {}
                Ok(Err(e)) => return Err(format!("the 3D's rows failed: {e}")),
                Err(e) => return Err(format!("the 3D's rows didn't arrive: {e}")),
            }
        }
        Ok(())
    }

    /// A timeline's sound mastered as its render's is, built on a thread from now (a few seconds: React stepped
    /// through every frame, no pages painted); until it's ready the elements play their own sounds, unmastered.
    pub fn master_sound(&mut self) {
        if self.master.is_some() || self.master_wav.is_some() {
            return;
        }
        // kept in the temp folder by what makes it (the bundle, and every file in the project: its path, size and
        // time), so opening it again plays it at once (each open made a new 3.8 MB file, kept forever, ~10 s after the
        // open for an edit on an external drive)
        // keyed by the core's mastering version and the build: a track an earlier core mastered isn't this one's
        let engine = format!("fw{}-{:x}-", raea_mixer::MASTER_VERSION, build_id());
        let wav = std::env::temp_dir().join(format!("raea-master-{engine}{:016x}.wav", sound_key(&self.js, &self.dir)));
        if wav.exists() {
            self.master_wav = Some(wav);
            return;
        }
        // the 40 newest tracks kept
        if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
            let mut old: Vec<(std::time::SystemTime, PathBuf)> = rd.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("raea-master-") && e.file_name().to_string_lossy().ends_with(".wav")).filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
            old.sort();
            while old.len() > 40 {
                let _ = std::fs::remove_file(old.remove(0).1);
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (js, dir) = (self.js.clone(), self.dir.clone());
        let spawned = std::thread::Builder::new().name("raea live master".into()).spawn(move || {
            let part = wav.with_extension("part.wav");
            let r = crate::motion::sound_only(&js, &dir).and_then(|a| match a {
                Some(a) => raea_mixer::write_wav(&a, &part).and_then(|_| std::fs::rename(&part, &wav).map_err(|e| e.to_string())).map(|_| Some(wav)),
                None => Ok(None),
            });
            let _ = tx.send(r);
        });
        if spawned.is_ok() {
            self.master = Some(rx);
        }
    }

    /// A game's live voices mastered as its render masters them: the gain that takes its sound (its scripted run, every
    /// frame) to the <Mix>'s loudness, and the look-ahead limiter at its ceiling, worked out on a thread from now. Until
    /// it's in, and without a <Mix>, the voices play at their own gain under the -1 dBFS ceiling.
    pub fn game_master(&mut self) {
        if self.game_master.is_some() || self.game_gain.is_some() {
            return;
        }
        // known already: kept in the temp folder by the bundle's content (the same game, the same master)
        let key = std::fs::read(&self.js).map(|b| twox_hash::XxHash3_64::oneshot(&b)).unwrap_or(0);
        // v2: the gain before the master's limiter (v1 held the loudness change after it, counting the limiter twice)
        // with the core's mastering version: a master an earlier core worked out isn't this one's
        let engine = format!("fw{}-{:x}-", raea_mixer::MASTER_VERSION, build_id());
        let cache = std::env::temp_dir().join(format!("raea-master-gain-v2-{engine}{key:016x}.json"));
        let parse = |v: &Value| v.get("gain_db").and_then(Value::as_f64).zip(v.get("ceiling_dbtp").and_then(Value::as_f64));
        if let Some(v) = std::fs::read_to_string(&cache).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
            self.game_gain = parse(&v);
            if v.get("none").is_none() || self.game_gain.is_some() {
                return;
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (js, dir) = (self.js.clone(), self.dir.clone());
        if std::thread::Builder::new().name("raea live game master".into()).spawn(move || {
            // in a child process (raea-motion master-gain): stepping the game through every frame in a second React,
            // and mixing its whole sound, took ~260 MB in the game's own process. In process only without the binary.
            let bin = std::env::var_os("RAEA_MOTION_BIN").map(PathBuf::from).or_else(|| std::env::current_exe().ok().map(|e| e.with_file_name("raea-motion"))).filter(|b| b.exists());
            let got: R<Option<(f64, f64)>> = match bin {
                Some(bin) => std::process::Command::new(bin).arg("master-gain").arg(&js).arg(&dir).output().map_err(|e| e.to_string()).and_then(|o| {
                    let text = String::from_utf8_lossy(&o.stdout);
                    let v: Value = text.lines().rev().find_map(|l| serde_json::from_str(l).ok()).ok_or_else(|| format!("master-gain said: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("")))?;
                    let _ = std::fs::write(&cache, v.to_string());
                    Ok(parse(&v))
                }),
                None => crate::motion::master_gain(&js, &dir),
            };
            let _ = tx.send(got);
        }).is_ok() {
            self.game_master = Some(rx);
        }
    }

    /// The mixer opened now, before the play's clock starts. Opened at the first sound instead, that frame was stepped
    /// late by the open (the one mixer core waits ~0.1 s for its device's clock to run), and its sounds skipped in by
    /// that much, so the play wasn't the bounce.
    pub fn open_mixer(&mut self) {
        if self.mixer.is_some() || self.mixer_failed {
            return;
        }
        match crate::audio::Mixer::open() {
            Ok(mut m) => {
                if let Some((gain, ceiling)) = self.game_gain {
                    m.master(gain, ceiling);
                }
                self.mixer = Some(m);
            }
            Err(e) => {
                eprintln!("raea live: no sound: {e}");
                self.mixer_failed = true;
            }
        }
    }

    /// A mixer opened earlier, by whoever knew a play was coming (void's stage keeps one spare), taken as this play's
    /// own, in place of open_mixer: its device's clock has run while the object opened, so the first frame doesn't
    /// wait the ~0.1 s the one mixer core waits for it (lightspeed F5). Nothing of another play's is in it.
    pub fn give_mixer(&mut self, mut m: crate::audio::Mixer) {
        if self.mixer.is_some() {
            return;
        }
        if let Some((gain, ceiling)) = self.game_gain {
            m.master(gain, ceiling);
        }
        self.mixer = Some(m);
    }

    /// The master waited for (at most `secs`) before the play's clock starts, so its first frames are mastered as the
    /// render's are (worked out on a thread, it came in seconds into the play: those seconds weren't the bounce).
    pub fn wait_master(&mut self, secs: f64) {
        if let Some(rx) = self.game_master.take() {
            match rx.recv_timeout(std::time::Duration::from_secs_f64(secs)) {
                Ok(Ok(g)) => {
                    self.game_gain = g;
                    if let (Some(m), Some((gain, ceiling))) = (self.mixer.as_mut(), g) {
                        m.master(gain, ceiling);
                    }
                }
                Ok(Err(e)) => eprintln!("raea live: the master failed, the voices play at their own gain: {e}"),
                // not in yet: play starts without it and the frame it lands swaps it in (step's try_recv)
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => self.game_master = Some(rx),
                Err(_) => {}
            }
        }
    }

    /// The mixer's clock past the device's startup, before the play's clock starts (raea_mixer's settle_clock: free
    /// when the device opened beside the edit's open).
    pub fn settle_mixer(&mut self) {
        if let Some(m) = self.mixer.as_mut() {
            m.settle();
        }
    }

    /// The game's master, once worked out: (gain dB, ceiling dBTP).
    pub fn game_gain(&self) -> Option<(f64, f64)> {
        self.game_gain
    }

    /// The frame's sounds to the mixer (opened the first time there are any).
    /// The mixer opened and the current frame's sound sources loading, nothing started (open_baked's frame 0).
    fn warm_sound(&mut self) {
        let world = self.app.world_mut();
        let mut q = world.query::<(&crate::ReactKind, Option<&crate::ReactProps>, Option<&crate::ReactNode>)>();
        let items: Vec<(&str, Option<&serde_json::Map<String, Value>>, Option<u64>)> = q.iter(world).filter(|(k, _, _)| k.0 == "audio" || k.0 == "video").map(|(k, p, n)| (k.0.as_str(), p.map(|p| &p.0), n.map(|n| n.0))).collect();
        let sounds = crate::audio::sounds_of(&items, &self.dir, crate::motion::media_path, &mut self.made);
        if sounds.is_empty() {
            return;
        }
        if self.mixer.is_none() && !self.mixer_failed {
            match crate::audio::Mixer::open() {
                Ok(mut m) => {
                    if let Some((gain, ceiling)) = self.game_gain {
                        m.master(gain, ceiling);
                    }
                    self.mixer = Some(m)
                }
                Err(e) => {
                    eprintln!("raea live: no sound: {e}");
                    self.mixer_failed = true;
                }
            }
        }
        if let Some(m) = self.mixer.as_mut() {
            for s in &sounds {
                m.preload(&s.file);
            }
        }
    }

    fn sound(&mut self, f: i64) {
        if let Some(rx) = &self.master {
            match rx.try_recv() {
                Ok(Ok(wav)) => {
                    if std::env::var_os("RAEA_LIVE_TRACE").is_some() {
                        eprintln!("raea live: the mastered sound is in at frame {f}");
                    }
                    self.master_wav = wav;
                    self.master = None;
                }
                Ok(Err(e)) => {
                    eprintln!("raea live: the mastered sound failed, the elements play their own: {e}");
                    self.master = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.master = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        // the frame's sounds, in the list kept between frames (ZeroHeap)
        let mut sounds = std::mem::take(&mut self.sound_buf);
        sounds.clear();
        match &self.master_wav {
            // the render's own track, placed at the frame's time
            Some(wav) => sounds.push(crate::audio::Sound { key: "master".into(), file: wav.as_path().into(), at: f as f64 / self.fps, volume: 1.0, rate: 1.0, fade_in: -1.0, fade_out: 0.0, bus: None }),
            None => {
                let (dir, made) = (&self.dir, &mut self.made);
                crate::motion::with_kinds(self.app.world_mut(), |world, q| {
                    let items = q.0.iter(world).filter(|(k, _, _)| k.0 == "audio" || k.0 == "video").map(|(k, p, n)| (k.0.as_str(), p.map(|p| &p.0), n.map(|n| n.0)));
                    crate::audio::sounds_into(items, dir, crate::motion::media_path, made, &mut sounds)
                })
            }
        }
        // a baked timeline past its last frame holds that frame's picture, not its sound: the held <Video> said the same
        // source time every frame and the mixer put its voice back each one (the 80 ms "drift" past a 4K edit's end).
        // Played unbaked, the composition past its end shows no <Video> either, so both fall silent there.
        if self.baked.as_ref().is_some_and(|b| f >= b.frames) {
            sounds.clear();
        }
        // the edit's Tone (@raea/motion/tone)
        let tone = if self.baked.as_ref().is_some_and(|b| f >= b.frames) { raea_mixer::ToneCue::default() } else { crate::audio::tone_at(self.app.world_mut(), f, self.fps, &self.dir, crate::motion::media_path, false) };
        if std::env::var_os("RAEA_SOUND_TRACE").is_some() && f % 30 == 0 {
            eprintln!("raea live: frame {f} sounds {:?}", sounds.iter().map(|s| (s.key.clone(), s.file.file_name().map(|n| n.to_string_lossy().into_owned()), (s.at * 100.0).round() / 100.0, s.volume)).collect::<Vec<_>>());
        }
        if sounds.is_empty() && tone.is_empty() && self.mixer.is_none() {
            self.sound_buf = sounds;
            return;
        }
        if self.mixer.is_none() && !self.mixer_failed {
            match crate::audio::Mixer::open() {
                Ok(mut m) => {
                    if let Some((gain, ceiling)) = self.game_gain {
                        m.master(gain, ceiling);
                    }
                    self.mixer = Some(m)
                }
                Err(e) => {
                    eprintln!("raea live: no sound: {e}");
                    self.mixer_failed = true;
                }
            }
        }
        if let Some(rx) = &self.game_master {
            match rx.try_recv() {
                Ok(Ok(g)) => {
                    if std::env::var_os("RAEA_LIVE_TRACE").is_some() {
                        eprintln!("raea live: the game's master is in at frame {f}: {g:?}");
                    }
                    self.game_gain = g;
                    self.game_master = None;
                    if let (Some(m), Some((gain, ceiling))) = (self.mixer.as_mut(), g) {
                        m.master(gain, ceiling);
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("raea live: the game's master failed, its voices play at their own gain: {e}");
                    self.game_master = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.game_master = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(m) = self.mixer.as_mut() {
            // the mastered track passes the core's master as it is (it was mastered by the same core)
            m.premastered(self.master_wav.is_some());
            // the lag as of now: settling and placing the videos took time since it was measured
            m.sync_frame(f, self.fps, &sounds, &tone, self.lag.0 + self.lag.1.elapsed().as_secs_f64());
        }
        self.sound_buf = sounds;
    }

    /// The page as it stands, read back as premultiplied RGBA (the playtest's --snapshot): what the stage shows.
    ///
    /// RAEA_SNAPSHOT_FIT=WxH: the page as void's stage paints it for a box of that size and its window draws it
    /// (paint_texture_fit), read back from the texture: the check that a fixed-size page is only scaled, never re-flowed.
    pub fn snapshot(&mut self) -> R<(u32, u32, Vec<u8>)> {
        if let Some(fit) = std::env::var("RAEA_SNAPSHOT_FIT").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))) {
            const CHECK: u64 = u64::MAX - 17;
            self.paint_texture_fit(CHECK, fit)?;
            return raea::quads::read_texture(CHECK);
        }
        // with a 3D scene: the page painted over this frame's scene on the GPU and read back, as presented and rendered
        if self.scene.is_some() {
            let under = self.prepare()?;
            let buf = raea::quads::page_buffer(self.w, self.h)?;
            let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
            m.dom.paint_buffer(true, under, &buf)?;
            drop(m);
            return Ok((self.w, self.h, raea::quads::read_buffer(&buf)?));
        }
        let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
        if m.fonts_dirty {
            let _ = m.dom.load_fonts();
            m.fonts_dirty = false;
        }
        let mut px = Vec::new();
        m.dom.paint_transparent(&mut px);
        Ok((self.w, self.h, px))
    }

    /// Whether the page changed since it was last painted (a commit that changed it, a resize, fonts, or a CSS animation
    /// running): a host painting it only when it changes asks this after a step (void's shell, apps/void/shell.feature).
    pub fn changed(&self) -> bool {
        let m = self.app.world().non_send_resource::<DomMirror>();
        m.dirty || m.fonts_dirty || m.dom.is_animating()
    }

    /// A baked timeline's clips (its Sequences' boxes, data-sequence), for a timeline control (void's dock): each one's
    /// name, its first frame (data-raea-start) and the frame after its last (the commit that took its box away, after the
    /// last keyframe it's in; else the next keyframe, or the end), how deeply it's nested in others, and what media it
    /// holds (video, audio, img). Empty played live (a game).
    pub fn clips(&self) -> Vec<Value> {
        let Some(b) = self.baked.as_ref() else { return Vec::new() };
        struct Clip {
            name: String,
            start: i64,
            last_key: i64,
            /// its node at the last keyframe it's in (a keyframe's nodes are its own: ids differ between them, and a
            /// removed node's id is given to a later one)
            id: u64,
            depth: usize,
            holds: std::collections::BTreeSet<String>,
        }
        // by name and start, not node: a Sequence's node differs from keyframe to keyframe, and a removed one's id is
        // given to a later one (six Sequences one after another read as one clip, then as some twice)
        let mut clips: std::collections::BTreeMap<(i64, String), Clip> = Default::default();
        for (f, ops) in &b.keys {
            let (mut parent, mut kids, mut kind): (std::collections::HashMap<u64, u64>, std::collections::HashMap<u64, Vec<u64>>, std::collections::HashMap<u64, &str>) = Default::default();
            let mut boxes: Vec<(u64, String, i64)> = Vec::new();
            for op in ops {
                let id = |k: &str| op.get(k).and_then(|v| v.as_u64());
                match op.get("op").and_then(|o| o.as_str()) {
                    Some("create") => {
                        let Some(i) = id("id") else { continue };
                        kind.insert(i, op.get("kind").and_then(|k| k.as_str()).unwrap_or(""));
                        let props = op.get("props");
                        let start = props.and_then(|p| p.get("data-raea-start")).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
                        if let Some(start) = start {
                            let name = props.and_then(|p| p.get("data-sequence")).and_then(|v| v.as_str()).unwrap_or("Sequence").to_string();
                            boxes.push((i, name, start.round() as i64));
                        }
                    }
                    Some("append") => {
                        if let (Some(p), Some(c)) = (id("parent"), id("child")) {
                            parent.insert(c, p);
                            kids.entry(p).or_default().push(c);
                        }
                    }
                    _ => {}
                }
            }
            let is_box: std::collections::HashSet<u64> = boxes.iter().map(|x| x.0).collect();
            for (i, name, start) in boxes {
                let mut depth = 0;
                let mut at = i;
                while let Some(&p) = parent.get(&at) {
                    depth += is_box.contains(&p) as usize;
                    at = p;
                }
                let mut holds = std::collections::BTreeSet::new();
                let mut stack = vec![i];
                while let Some(n) = stack.pop() {
                    if let Some(k @ ("video" | "audio" | "img")) = kind.get(&n).copied() {
                        holds.insert(k.to_string());
                    }
                    stack.extend(kids.get(&n).into_iter().flatten().copied());
                }
                let c = clips.entry((start, name.clone())).or_insert(Clip { name, start, last_key: *f, id: i, depth, holds: Default::default() });
                (c.last_key, c.id) = (*f, i);
                c.holds.extend(holds);
            }
        }
        let gone = |id: u64, from: i64| -> i64 {
            let (child, until) = (format!("\"child\":{id}"), (from + crate::bake::KEY).min(b.frames));
            (from.max(0)..until).find(|&f| b.frame_ops.get(f as usize).is_some_and(|ops| ops.iter().any(|o| o.contains("\"remove") && o.contains(&child)))).unwrap_or(until)
        };
        clips.into_values().map(|c| json!({"name": c.name, "start": c.start, "end": gone(c.id, c.last_key + 1), "depth": c.depth, "holds": c.holds})).collect()
    }

    /// What sounds on this frame, each source as the page has it (not the mastered track a timeline plays): its key, its
    /// file, its gain and the bus it goes to; for an audio control (void's dock).
    pub fn sounds(&mut self) -> Vec<Value> {
        let mut sounds = Vec::new();
        let (dir, made) = (&self.dir, &mut self.made);
        crate::motion::with_kinds(self.app.world_mut(), |world, q| {
            let items = q.0.iter(world).filter(|(k, _, _)| k.0 == "audio" || k.0 == "video").map(|(k, p, n)| (k.0.as_str(), p.map(|p| &p.0), n.map(|n| n.0)));
            crate::audio::sounds_into(items, dir, crate::motion::media_path, made, &mut sounds)
        });
        sounds.iter().map(|s| json!({"key": &*s.key, "file": s.file.file_name().map(|n| n.to_string_lossy().into_owned()), "volume": s.volume, "bus": s.bus.as_deref()})).collect()
    }

    /// What the page publishes for the app around it: the `data-void-publish` prop of the first element carrying one
    /// (a void space's panel tells the object on the stage what it picked; apps/void/src/space_ui.rs). None if none does.
    pub fn published(&mut self) -> Option<String> {
        let world = self.app.world_mut();
        let mut q = world.query::<&crate::ReactProps>();
        q.iter(world).find_map(|p| p.0.get("data-void-publish").map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
    }

    /// The page as it stands, written as a PNG (void's RAEA_STAGE_DUMP).
    pub fn save_snapshot(&mut self, path: &Path) -> R<()> {
        let (w, h, px) = self.snapshot()?;
        image::save_buffer(path, &px, w, h, image::ColorType::Rgba8).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The mixer's drift samples (ms), for the playtest report.
    pub fn drift_ms(&self) -> &[f64] {
        self.mixer.as_ref().map(|m| m.drift_ms.as_slice()).unwrap_or(&[])
    }

    /// Paused or stopped: the sound fades out (it comes back with the next frame stepped).
    pub fn silence(&mut self) {
        if let Some(m) = self.mixer.as_mut() {
            m.stop_all();
        }
    }

    /// The sound device's stream stopped while paused (after silence and its fade-outs): no callback renders silence
    /// through the graph (lightspeed R6). The next frame's sound starts it again.
    pub fn rest(&mut self) {
        if let Some(m) = self.mixer.as_mut() {
            m.rest();
        }
    }

    /// The page as it stands, presented on `stage` (only when something changed since the last present, or `force`);
    /// true when a window's layer took the frame.
    pub fn present(&mut self, stage: &raea::quads::Stage, force: bool) -> R<bool> {
        {
            let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
            m.sync_clips();
            // a page with a CSS animation or transition running changes on its own
            if !m.dirty && !force && !m.dom.is_animating() {
                return Ok(false);
            }
        }
        let under = self.prepare()?;
        let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
        let st = match under {
            Some(id) => m.dom.present_under(stage, true, id)?,
            None => m.dom.present(stage, true)?,
        };
        m.dirty = false;
        // shown: a window's layer took the frame (false offscreen, or when the layer gave no drawable)
        Ok(st.shown)
    }

    /// The page painted into texture `id` instead of a window: for a window's chrome to draw the stage in its own frame
    /// (`<img data-raea-texture=id data-raea-w data-raea-h>`, the page's size), so the chrome's glass blurs the stage.
    pub fn paint_texture(&mut self, id: u64) -> R<()> {
        let under = self.prepare()?;
        let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
        match m.dom.paint_texture(true, id, under) {
            Ok(_) => {}
            // a frame the compositor can't do: painted on the CPU and handed over as the same texture
            Err(e) if e == raea::quads::NEEDS_CPU => return Err("the stage's page needs the CPU, which a texture for the chrome can't take yet".into()),
            Err(e) => return Err(e),
        }
        m.dirty = false;
        Ok(())
    }

    /// `paint_texture` for a box of `fit` pixels (void's stage, object-fit contain): a page larger than the box is
    /// composited whole at its own size and layout, then shrunk by area to the size it's shown at, so the window draws it
    /// one to one (one bilinear tap of the whole page aliased text and edges). Only a uniform scale of the fixed layout:
    /// never laid out again at the box (the founder, 2026-10-18: video is fixed scale). The texture's size.
    pub fn paint_texture_fit(&mut self, id: u64, fit: (u32, u32)) -> R<(u32, u32)> {
        let k = (fit.0 as f64 / self.w as f64).min(fit.1 as f64 / self.h as f64);
        let size = if k < 1.0 && fit.0 > 0 && fit.1 > 0 { (((self.w as f64 * k).round() as u32).max(1), ((self.h as f64 * k).round() as u32).max(1)) } else { (self.w, self.h) };
        let under = self.prepare()?;
        let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
        match m.dom.paint_texture_fit(true, id, under, (size != (self.w, self.h)).then_some(size)) {
            Ok(_) => {}
            Err(e) if e == raea::quads::NEEDS_CPU => return Err("the stage's page needs the CPU, which a texture for the chrome can't take yet".into()),
            Err(e) => return Err(e),
        }
        m.dirty = false;
        Ok(size)
    }

    /// What every paint of the page starts with: fonts, the 3D under it (its engine opened when its rows arrive, this
    /// frame drawn), and the elements holding the 3D see-through. The texture the page goes over, if any.
    fn prepare(&mut self) -> R<Option<u64>> {
        let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
        m.sync_clips();
        if m.fonts_dirty {
            let _ = m.dom.load_fonts();
            m.fonts_dirty = false;
        }
        drop(m);
        // the 3D: its rows in from the thread (the engine opens here, once), then this frame drawn under the page
        if let Some(rx) = &self.scene_rx {
            match rx.try_recv() {
                Ok(Ok(Some(prog))) => {
                    match rdb::engine::Engine::new_live(&prog, &rdb::inserted_of(&prog), &self.dir) {
                        Ok(e) => self.scene = Some(e),
                        Err(e) => eprintln!("raea live: the 3D didn't open: {e}"),
                    }
                    self.scene_rx = None;
                }
                Ok(Ok(None)) | Err(std::sync::mpsc::TryRecvError::Disconnected) => self.scene_rx = None,
                Ok(Err(e)) => {
                    eprintln!("raea live: the 3D's rows failed: {e}");
                    self.scene_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let under = match self.scene.as_mut() {
            Some(e) => {
                e.draw(self.frame);
                if let Some(err) = e.take_error() {
                    eprintln!("raea live: frame {}: the 3D: {err}", self.frame);
                }
                e.texture().map(|v| raea::quads::set_texture(SCENE_TEXTURE, v, self.w, self.h)).transpose()?.map(|_| SCENE_TEXTURE)
            }
            None => None,
        };
        // a layered Canvas3D: the 3D is its slot's picture, in the page's paint order (as a render draws it), not a
        // texture under the whole page; the holders keep their backgrounds (the page under the 3D)
        if let Some(id) = under {
            if let Some(slot) = crate::motion::scene_slot(self.app.world_mut()) {
                let (w, h) = (self.w, self.h);
                let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
                if let Some(node) = m.node(slot) {
                    crate::motion::set_scene_slot(&mut m.dom, node, id, w, h);
                    return Ok(None);
                }
            }
        }
        // the elements holding the 3D show it through, as in a render: their backgrounds go (again after a commit,
        // which can set their style back)
        if under.is_some() {
            // only a holder whose props a commit changed since it was made see-through (each was made so again every
            // frame: its props copied and its style set, restyling it; ZeroHeap)
            let mut holders = std::mem::take(&mut self.holders);
            crate::motion::holder_entities(self.app.world_mut(), &mut holders);
            for &e in &holders {
                let world = self.app.world();
                let Some(props) = world.entity(e).get_ref::<crate::ReactProps>() else { continue };
                let tick = props.last_changed();
                if self.see_through.get(&e) == Some(&tick) {
                    continue;
                }
                let mut p = props.0.clone();
                let id = world.get::<crate::ReactNode>(e).map(|n| n.0);
                self.see_through.insert(e, tick);
                let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
                let Some(node) = id.and_then(|i| m.node(i)) else { continue };
                if crate::motion::see_through(&mut p).0 {
                    crate::apply_props(&mut m.dom, node, &p);
                }
            }
            if self.see_through.len() > holders.len() + 64 {
                self.see_through.retain(|e, _| holders.contains(e));
            }
            self.holders = holders;
        }
        Ok(under)
    }
}

/// CoreAudio's output device found and its format read on a thread, once a process, so the HAL's start-up (~95 ms) is
/// done by the time the mixer opens, after the object's open and before the clock (lightspeed F5). Called as an open
/// starts, and earlier by whoever knows sound will come (raea-motion play, void's launch).
pub fn warm_audio() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = std::thread::Builder::new().name("raea audio warm".into()).spawn(|| {
            use cpal::traits::{DeviceTrait, HostTrait};
            let t = std::time::Instant::now();
            let ok = cpal::default_host().default_output_device().map(|d| d.default_output_config().is_ok()).unwrap_or(false);
            crate::startup_mark(&format!("audio device warm ({ok}, {:.0} ms)", t.elapsed().as_secs_f64() * 1e3));
        });
    });
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

/// RGBA pixels written as a PNG (void's probe reads its stage's texture back with quads::read_texture).
pub fn save_png(path: &Path, w: u32, h: u32, px: &[u8]) -> R<()> {
    image::save_buffer(path, px, w, h, image::ColorType::Rgba8).map_err(|e| format!("{}: {e}", path.display()))
}

/// How a timeline is bundled for its bake: its words and perception, on live play's runtime, each frame's components
/// recorded (an edit re-bakes only where they render).
fn baked_opts(dir: &Path) -> R<crate::motion::CompOpts> {
    let mut opts: Vec<String> = Vec::new();
    for (flag, file) in [("--words", "words.json"), ("--perception", "perception.json")] {
        let p = dir.join(file);
        if p.exists() {
            opts.push(flag.into());
            opts.push(p.display().to_string());
        }
    }
    opts.push("--lite".into());
    opts.push("--comps".into());
    crate::motion::comp_opts(&opts)
}

/// A baked timeline's bake made again after its files changed (void's stage, when an agent edits them): `run` it on a
/// worker, then hand what it made to `Live::swap_bake` on the thread that plays.
pub struct Rebake {
    old: std::sync::Arc<crate::bake::Baked>,
    tsx: PathBuf,
}

/// What a `Rebake` made: the new bake, its bundle, and how many of its frames React stepped (bake_incremental's count;
/// every frame when it baked them all again), with the bundle's and the bake's times.
pub struct Rebaked {
    baked: crate::bake::Baked,
    js: PathBuf,
    pub stepped: i64,
    pub frames: i64,
    pub bundle_ms: f64,
    pub bake_ms: f64,
}

impl Rebake {
    /// The edit bundled again and baked from the bake before it (only the frames where a changed component renders are
    /// stepped; anything else bakes every frame again). Blocking: seconds at worst; run it off the playing thread.
    pub fn run(self) -> R<Rebaked> {
        let dir = self.tsx.parent().unwrap().to_path_buf();
        let js = std::env::temp_dir().join(format!("raea-live-{}-{:x}.js", std::process::id(), rand_id()));
        let t = std::time::Instant::now();
        crate::motion::bundle_composition(&self.tsx, &js, &baked_opts(&dir)?, None)?;
        let bundle_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = std::time::Instant::now();
        let (baked, stepped) = match crate::bake::bake_incremental(&self.old, &js) {
            Ok(b) => b,
            Err(e) => {
                let _ = std::fs::remove_file(&js);
                return Err(e);
            }
        };
        let frames = baked.frames;
        Ok(Rebaked { baked, js, stepped, frames, bundle_ms, bake_ms: t.elapsed().as_secs_f64() * 1e3 })
    }
}

impl Live {
    /// The job that bakes this timeline again from its current bake (None: it isn't baked).
    pub fn rebake(&self) -> Option<Rebake> {
        Some(Rebake { old: self.baked.clone()?, tsx: self.dir.join("edit.tsx") })
    }

    /// A new bake (`Rebake::run`) put in place of the one playing: the next frame stepped jumps (the keyframe before
    /// it, then the frames after) into the new bake, so the playhead and the clock go on where they are. `media`: files
    /// other than code changed, so the clips are opened again. The mastered sound is made again on a thread; the old
    /// track plays until it's in.
    /// The videos' decoders dropped, so the next step opens them again: what each opens is decided at its open
    /// (raea_media::mezz::resolve), and an optimized copy completed since then is read from here on, as a render
    /// opened now reads it (void's stage, Cmd::MediaOptimized).
    pub fn reopen_videos(&mut self) {
        self.videos = crate::motion::VideoFrames::new();
        self.applied = -1;
        self.scanned = -1;
    }

    /// The page's fonts as an exported game carries them (Dom::packaged_fonts_css: the injected faces, every remote one
    /// inlined), frame 0's stylesheets in.
    pub fn packaged_fonts_css(&mut self) -> R<Option<String>> {
        self.step(0, &[], None)?;
        let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
        Ok(m.dom.packaged_fonts_css())
    }

    /// An exported game's packaged fonts put in the page before its fonts load, so nothing is fetched (its player has no
    /// network).
    pub fn add_packaged_fonts_css(&mut self, css: &str) {
        let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
        m.dom.add_packaged_fonts_css(css);
        m.fonts_dirty = true;
    }

    /// Every video frame a baked timeline shows: each frame replayed (no painting, nothing decoded) and each <Video>'s
    /// file and source time read from its props (`data-source-time`, what a render places), for the optimized range
    /// copies (raea-motion mezz). Hidden videos are counted too.
    pub fn video_times(&mut self) -> Vec<(PathBuf, f64)> {
        let mut out = Vec::new();
        for f in 0..self.frames {
            self.replay(f);
            let world = self.app.world_mut();
            let mut q = world.query::<(&crate::ReactKind, &crate::ReactProps)>();
            for (k, p) in q.iter(world) {
                if k.0 != "video" {
                    continue;
                }
                let Some(src) = p.0.get("src").and_then(Value::as_str) else { continue };
                let at = p.0.get("data-source-time").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                out.push((crate::motion::media_path(&self.dir, src), at));
            }
        }
        out
    }

    pub fn swap_bake(&mut self, r: Rebaked, media: bool) {
        let b = r.baked;
        if (b.w, b.h) != (self.w, self.h) {
            (self.w, self.h) = (b.w, b.h);
            let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
            m.dom.resize(b.w, b.h);
            m.dirty = true;
        }
        (self.fps, self.frames) = (b.fps, b.frames);
        let has_3d = b.has_3d;
        self.baked = Some(std::sync::Arc::new(b));
        let _ = std::fs::remove_file(&self.js);
        self.js = r.js;
        // a jump on the next step; the clips ahead looked through again
        self.applied = -1;
        self.scanned = -1;
        if media {
            self.videos = crate::motion::VideoFrames::new();
        }
        // the 3D's rows from the new bundle
        if has_3d || self.scene.is_some() {
            self.scene = None;
            self.scene_rx = None;
            self.scene_3d();
        }
        // the sound made again from the new bundle and files (kept by what makes it: unchanged sound is found at once)
        if self.master.is_some() || self.master_wav.is_some() {
            self.master = None;
            let keep = self.master_wav.take();
            self.master_sound();
            if self.master_wav.is_none() {
                self.master_wav = keep;
            }
        }
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
    if std::env::var_os("RAEA_SOUND_TRACE").is_some() {
        eprintln!("raea live: sound key: bundle {bundle:016x} ({} bytes), with the project {:016x}", std::fs::metadata(js).map(|m| m.len()).unwrap_or(0), twox_hash::XxHash3_64::oneshot(&h));
    }
    twox_hash::XxHash3_64::oneshot(&h)
}

/// A step's parts, for live play on Bevy's runner (runner.rs): the frame stepped in one system, its sound synced in
/// the next. `step_at` is both.
impl Live {
    /// `step`, `lag` seconds after the frame's own time (the sound is placed where it should be now).
    pub fn step_at(&mut self, f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>, lag: f64) -> R<()> {
        self.step_inner(f, keys, pointer, lag, true)
    }

    /// `step_at` without the sound: the frame's commit in the page and its videos placed.
    pub fn step_world(&mut self, f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>, lag: f64) -> R<()> {
        self.step_inner(f, keys, pointer, lag, false)
    }

    /// The last frame stepped's sounds to the mixer, placed by its lag as of now.
    pub fn sync_sound(&mut self) {
        let f = self.frame;
        self.sound(f);
    }
}

/// The frame's "raea-frame" event, written straight into its message (ZeroHeap: built as a json! tree and serialized, a
/// map, key strings and the keys array a frame): {"t":"event","name":"raea-frame","value":{"frame":f,"input":{"keys":[..],
/// "pointer":{"x","y","down"}}}}, which the page reads the same way.
fn frame_event(f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>) -> String {
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
    m.push_str("}}}");
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
            let got: Value = serde_json::from_str(&frame_event(7, &keys, pointer)).unwrap();
            assert_eq!(got, want);
        }
    }
}
