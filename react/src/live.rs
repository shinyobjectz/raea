//! Live play: a composition run frame by frame at the display's rate and presented on a window's GPU surface, with the
//! person's keys and pointer as its input (One renderer, the native stage). The same React-on-V8 -> ECS page -> Blitz
//! -> quads path a render takes, without settling loops, page files or readback: a frame is the commit carrying its
//! done mark (motion.rs settle), then the page presented. `raea-motion play` drives it in a window of its own; void's
//! stage drives it in its window (void/src/stage_native.rs).

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
    /// the frame last stepped (what the 3D draws)
    frame: i64,
    /// generated sounds' files, by what they were made from (looked up once)
    made: std::collections::HashMap<String, Option<PathBuf>>,
    /// a baked timeline (bake.rs): its frames replayed into the page, no JS running; and the frame the page shows
    baked: Option<crate::bake::Baked>,
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
        let responsive = src.contains("useVideoConfig(") || components_use_config(&dir);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.clone()));
        app.insert_non_send(DomMirror::new_page(stage.0.max(1), stage.1.max(1), &dir));
        let conf = crate::motion::settle_frame(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
        let n = |k: &str, d: f64| conf.get(k).and_then(Value::as_f64).unwrap_or(d);
        let (cw, ch, fps, frames) = (n("width", 1080.0) as u32, n("height", 1920.0) as u32, n("fps", 30.0), n("frames", 1.0).max(1.0) as i64);
        let (w, h) = if responsive { (stage.0.max(1), stage.1.max(1)) } else { (cw, ch) };
        let mut live = Live { app, dir: dir.clone(), videos: crate::motion::VideoFrames::new(), mixer: None, mixer_failed: false, lag: (0.0, std::time::Instant::now()), w, h, fps, frames, responsive, js, master: None, master_wav: None, game_master: None, game_gain: None, scene_rx: None, scene: None, frame: 0, made: Default::default(), baked: None, applied: -1, scanned: -1 };
        {
            let brand = crate::motion::brand_css(&dir, w);
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
        Ok(live)
    }

    /// A timeline baked (bake.rs) and played from its bake: React steps through every frame once, here and now, and
    /// from then on each frame is its recorded commits applied to the page, no JS running (a seek: the keyframe before
    /// it, then the frames after). The page is the composition's own size.
    pub fn open_baked(tsx: &Path) -> R<Live> {
        let tsx = tsx.canonicalize().map_err(|e| format!("{}: {e}", tsx.display()))?;
        let dir = tsx.parent().unwrap().to_path_buf();
        let js = std::env::temp_dir().join(format!("raea-live-{}-{:x}.js", std::process::id(), rand_id()));
        let mut opts: Vec<String> = Vec::new();
        for (flag, file) in [("--words", "words.json"), ("--perception", "perception.json")] {
            let p = dir.join(file);
            if p.exists() {
                opts.push(flag.into());
                opts.push(p.display().to_string());
            }
        }
        // on live play's runtime, each frame's components recorded (an edit re-bakes only where they render)
        opts.push("--lite".into());
        opts.push("--comps".into());
        let o = crate::motion::comp_opts(&opts)?;
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
                crate::bake::bake(&js)?
            }
        };
        let (w, h, fps, frames) = (baked.w, baked.h, baked.fps, baked.frames);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(crate::ReplayPlugin);
        app.insert_non_send(DomMirror::new_page(w, h, &dir));
        let mut live = Live { app, dir: dir.clone(), videos: crate::motion::VideoFrames::new(), mixer: None, mixer_failed: false, lag: (0.0, std::time::Instant::now()), w, h, fps, frames, responsive: false, js, master: None, master_wav: None, game_master: None, game_gain: None, scene_rx: None, scene: None, frame: 0, made: Default::default(), baked: None, applied: -1, scanned: -1 };
        {
            let brand = crate::motion::brand_css(&dir, w);
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
        live.baked = Some(baked);
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
        let mut input = json!({"keys": keys});
        if let Some((x, y, down)) = pointer {
            input["pointer"] = json!({"x": x, "y": y, "down": down});
        }
        let t0 = std::time::Instant::now();
        if self.baked.is_some() {
            // a baked timeline: its recorded commits, no JS; the clips it cuts to soon opened ahead
            self.replay(f);
            self.prefetch_ahead(f);
        } else {
            self.send("raea-frame", json!({"frame": f, "input": input}));
            crate::motion::settle_frame(&mut self.app, Some(f), 5.0)?;
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
        if self.scene_rx.is_some() || self.scene.is_some() {
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
        let wav = std::env::temp_dir().join(format!("raea-master-{:016x}.wav", sound_key(&self.js, &self.dir)));
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
                Some(a) => crate::audio::write_wav(&a, &part).and_then(|_| std::fs::rename(&part, &wav).map_err(|e| e.to_string())).map(|_| Some(wav)),
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
        let cache = std::env::temp_dir().join(format!("raea-master-gain-v2-{key:016x}.json"));
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

    /// The game's master, once worked out: (gain dB, ceiling dBTP).
    pub fn game_gain(&self) -> Option<(f64, f64)> {
        self.game_gain
    }

    /// The frame's sounds to the mixer (opened the first time there are any).
    /// The mixer opened and the current frame's sound sources loading, nothing started (open_baked's frame 0).
    fn warm_sound(&mut self) {
        let world = self.app.world_mut();
        let mut q = world.query::<(&crate::ReactKind, Option<&crate::ReactProps>, Option<&crate::ReactNode>)>();
        let items: Vec<(String, serde_json::Map<String, Value>, Option<u64>)> = q.iter(world).filter(|(k, _, _)| k.0 == "audio" || k.0 == "video").map(|(k, p, n)| (k.0.clone(), p.map(|p| p.0.clone()).unwrap_or_default(), n.map(|n| n.0))).collect();
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
        let sounds = match &self.master_wav {
            // the render's own track, placed at the frame's time
            Some(wav) => vec![crate::audio::Sound { key: "master".into(), file: wav.clone(), at: f as f64 / self.fps, volume: 1.0, rate: 1.0, fade_in: 0.0, fade_out: 0.0 }],
            None => {
                let world = self.app.world_mut();
                let mut q = world.query::<(&crate::ReactKind, Option<&crate::ReactProps>, Option<&crate::ReactNode>)>();
                let items: Vec<(String, serde_json::Map<String, Value>, Option<u64>)> = q.iter(world).filter(|(k, _, _)| k.0 == "audio" || k.0 == "video").map(|(k, p, n)| (k.0.clone(), p.map(|p| p.0.clone()).unwrap_or_default(), n.map(|n| n.0))).collect();
                crate::audio::sounds_of(&items, &self.dir, crate::motion::media_path, &mut self.made)
            }
        };
        // a baked timeline past its last frame holds that frame's picture, not its sound: the held <Video> said the same
        // source time every frame and the mixer put its voice back each one (the 80 ms "drift" past a 4K edit's end).
        // Played unbaked, the composition past its end shows no <Video> either, so both fall silent there.
        let sounds = if self.baked.as_ref().is_some_and(|b| f >= b.frames) { Vec::new() } else { sounds };
        if std::env::var_os("RAEA_SOUND_TRACE").is_some() && f % 30 == 0 {
            eprintln!("raea live: frame {f} sounds {:?}", sounds.iter().map(|s| (s.key.clone(), s.file.file_name().map(|n| n.to_string_lossy().into_owned()), (s.at * 100.0).round() / 100.0, s.volume)).collect::<Vec<_>>());
        }
        if sounds.is_empty() && self.mixer.is_none() {
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
            // the lag as of now: settling and placing the videos took time since it was measured
            m.sync(&sounds, self.lag.0 + self.lag.1.elapsed().as_secs_f64());
        }
    }

    /// The page as it stands, read back as premultiplied RGBA (the playtest's --snapshot): what the stage shows.
    pub fn snapshot(&mut self) -> R<(u32, u32, Vec<u8>)> {
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

    /// The page as it stands, presented on `stage` (only when something changed since the last present, or `force`);
    /// true when a window's layer took the frame.
    pub fn present(&mut self, stage: &raea::quads::Stage, force: bool) -> R<bool> {
        {
            let m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
            if !m.dirty && !force {
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

    /// What every paint of the page starts with: fonts, the 3D under it (its engine opened when its rows arrive, this
    /// frame drawn), and the elements holding the 3D see-through. The texture the page goes over, if any.
    fn prepare(&mut self) -> R<Option<u64>> {
        let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
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
                e.texture().map(|v| raea::quads::set_texture(SCENE_TEXTURE, v, self.w, self.h)).transpose()?.map(|_| SCENE_TEXTURE)
            }
            None => None,
        };
        // the elements holding the 3D show it through, as in a render: their backgrounds go (again after a commit,
        // which can set their style back)
        if under.is_some() {
            let holders = crate::motion::holders_of(self.app.world_mut());
            let ids: Vec<(Option<u64>, serde_json::Map<String, Value>)> = holders.iter().map(|(e, p)| (self.app.world().get::<crate::ReactNode>(*e).map(|n| n.0), p.clone())).collect();
            let mut m = self.app.world_mut().non_send_resource_mut::<DomMirror>();
            for (id, mut p) in ids {
                let Some(node) = id.and_then(|i| m.node(i)) else { continue };
                if let Some(Value::Object(st)) = p.get_mut("style") {
                    if st.remove("background").is_some() | st.remove("backgroundColor").is_some() {
                        crate::apply_props(&mut m.dom, node, &p);
                    }
                }
            }
        }
        Ok(under)
    }
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
    pub(crate) fn step_world(&mut self, f: i64, keys: &[String], pointer: Option<(f64, f64, bool)>, lag: f64) -> R<()> {
        self.step_inner(f, keys, pointer, lag, false)
    }

    /// The last frame stepped's sounds to the mixer, placed by its lag as of now.
    pub(crate) fn sync_sound(&mut self) {
        let f = self.frame;
        self.sound(f);
    }
}
