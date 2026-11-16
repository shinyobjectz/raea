//! Opening a live composition: from its TSX, a bundle or a baked game, and what it reads from its edit's folder.

use super::*;

impl Live {
    /// The composition in `tsx` (an edit.tsx and what it imports), at `stage` pixels when it's responsive.
    pub fn open(tsx: &Path, stage: (u32, u32)) -> R<Live> {
        Self::open_with(tsx, stage, true)
    }

    /// `open`, laid out at the stage's size only when `may_respond` and the composition lays out from useVideoConfig.
    /// The rule (the user, 2026-10-18): only games may be responsive. Video and everything played like it (timelines,
    /// motion, decks, scenes; open_baked too) keep the composition's own layout at its own size, never re-flowed to the
    /// window's box; the legacy app's stage only scales that page uniformly (paint_texture_fit). Don't flip this per kind.
    pub fn open_with(tsx: &Path, stage: (u32, u32), may_respond: bool) -> R<Live> {
        warm_audio();
        // the thread that steps and presents live frames, at the frame threads' QoS (as the JS thread)
        raea_runtime_react::interactive_thread();
        raea_io_trace::event("open", serde_json::json!({"composition": tsx, "stage": [stage.0, stage.1]}));
        let tsx = tsx.canonicalize().map_err(|e| format!("{}: {e}", tsx.display()))?;
        let dir = tsx.parent().unwrap().to_path_buf();
        let js = std::env::temp_dir().join(format!("raea-live-{}-{:x}.js", std::process::id(), rand_id()));
        // the project's words and perception, as a render passes them (captions, regions)
        let mut opts: Vec<String> = Vec::new();
        for (flag, file) in [("--words", "words.json"), ("--perception", "perception.json")] {
            let p = dir.join(file);
            if p.exists() && (file != "words.json" || raea_runtime_react::scene::transcript_file(&p)) {
                opts.push(flag.into());
                opts.push(p.display().to_string());
            }
        }
        // played live (a game: timelines play their bake), on raea's live view runtime instead of React: each frame
        // rendered and only what changed sent (RAEA_LIVE_REACT=1 keeps React, as an edit drawing R3F does)
        if std::env::var("RAEA_LIVE_REACT").as_deref() != Ok("1") && !crate::needs_react(&dir) {
            opts.push("--lite".into());
        }
        raea_runtime_react::scene::bundle_composition(&tsx, &js, &raea_runtime_react::scene::comp_opts(&opts)?, None)?;
        let src = std::fs::read_to_string(&tsx).unwrap_or_default();
        let responsive = may_respond && (src.contains("useVideoConfig(") || components_use_config(&dir));
        Self::from_bundle(js, dir, stage, responsive)
    }

    /// A composition already bundled for live play (an exported game, game.rs): `js` is copied to the temp folder (the
    /// copy is what Live removes when it's dropped), `dir` is where its media resolves. No bun runs.
    pub fn open_bundle(js: &Path, dir: &Path, stage: (u32, u32), responsive: bool) -> R<Live> {
        raea_runtime_react::interactive_thread();
        let tmp = std::env::temp_dir().join(format!("raea-live-{}-{:x}.js", std::process::id(), rand_id()));
        std::fs::copy(js, &tmp).map_err(|e| format!("{}: {e}", js.display()))?;
        Self::from_bundle(tmp, dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?, stage, responsive)
    }

    /// The game's master as worked out ahead of time (an exported game carries it): no second React at run time.
    pub fn set_game_gain(&mut self, g: Option<(f64, f64)>) {
        self.game_gain = g;
    }

    pub(crate) fn from_bundle(js: PathBuf, dir: PathBuf, stage: (u32, u32), responsive: bool) -> R<Live> {
        raea_runtime_react::native::open_in(&dir, &js);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.clone()));
        raea_runtime_react::scene::one_thread(&mut app);
        let conf = raea_runtime_react::scene::settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
        let n = |k: &str, d: f64| conf.get(k).and_then(Value::as_f64).unwrap_or(d);
        let (cw, ch, fps, frames) = (n("width", 1080.0) as u32, n("height", 1920.0) as u32, n("fps", 30.0), n("frames", 1.0).max(1.0) as i64);
        let (w, h) = if responsive { (stage.0.max(1), stage.1.max(1)) } else { (cw, ch) };
        let page = raea_runtime_page::BevyPage::new(w, h, &dir)?;
        let brand = raea_runtime_react::scene::page_css(&dir, w, &js);
        let mut live = Live { app, dir: dir.clone(), page, brand, dirty: true, mixer: None, mixer_failed: false, lag: (0.0, std::time::Instant::now()), w, h, fps, frames, responsive, js, master: None, master_wav: None, game_master: None, game_gain: None, scene_rx: None, scene: None, live3d: None, frame: 0, pads: Vec::new(), taken: Vec::new(), pointer: Default::default(), made: Default::default(), sound_buf: Vec::new(), holders: Vec::new(), baked: None, applied: -1, session: new_session() };
        if responsive {
            live.send("raea-size", json!({"w": w, "h": h}));
        }
        // 3D (raea/3d in the bundle): drawn from each frame's own state, as it's played. Frame 0 (the game's
        // initial state, whatever the input) stepped now, so its scene's engine opens before the clock starts.
        // RAEA_LIVE_3D=0: no 3D played live (as before 2026-10-18)
        if std::env::var("RAEA_LIVE_3D").as_deref() != Ok("0") && std::fs::read_to_string(&live.js).is_ok_and(|t| t.contains("data-3d")) {
            live.live3d = Some(Default::default());
            live.step_world(0, &[], None, 0.0)?;
            live.live3d.as_mut().unwrap().warm = true;
        }
        // <Seedling> (raea/seedling): seedling's graph made now, before the clock starts (made on frame 0 it
        // took 50 ms, and the frames after came too late for their samples)
        raea_runtime_sound::warm_seedling(live.app.world_mut(), live.fps, &live.js, &live.dir);
        Ok(live)
    }

    /// A timeline baked (bake.rs) and played from its bake: React steps through every frame once, here and now, and
    /// from then on each frame is its recorded commits applied to the page, no JS running (a seek: the keyframe before
    /// it, then the frames after). The page is the composition's own size.
    pub fn open_baked(tsx: &Path) -> R<Live> {
        warm_audio();
        let tsx = tsx.canonicalize().map_err(|e| format!("{}: {e}", tsx.display()))?;
        let dir = tsx.parent().unwrap().to_path_buf();
        *raea_runtime_react::native::ASSET_DIR.lock().unwrap() = Some(dir.clone());
        let js = std::env::temp_dir().join(format!("raea-live-{}-{:x}.js", std::process::id(), rand_id()));
        let o = baked_opts(&dir)?;
        // RAEA_BAKE_FROM=OLD.tsx: OLD baked first and this edit baked from it (bake_incremental), as an edit re-bakes
        let baked = match std::env::var("RAEA_BAKE_FROM").ok() {
            Some(from) => {
                let old_js = std::env::temp_dir().join(format!("raea-live-{}-{:x}-from.js", std::process::id(), rand_id()));
                raea_runtime_react::scene::bundle_composition(Path::new(&from), &old_js, &o, None)?;
                let old = raea_runtime_bake::bake(&old_js)?;
                let _ = std::fs::remove_file(&old_js);
                raea_runtime_react::scene::bundle_composition(&tsx, &js, &o, None)?;
                let t = std::time::Instant::now();
                let (b, stepped) = raea_runtime_bake::bake_incremental(&old, &js)?;
                eprintln!("raea bake: incremental, {stepped} of {} frames stepped, {:.0} ms", b.frames, t.elapsed().as_secs_f64() * 1e3);
                b
            }
            None => {
                raea_runtime_react::scene::bundle_composition(&tsx, &js, &o, None)?;
                raea_runtime_bake::bake_cached(&js, build_id())?
            }
        };
        let (w, h, fps, frames) = (baked.w, baked.h, baked.fps, baked.frames);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(raea_runtime_react::ReplayPlugin);
        raea_runtime_react::scene::one_thread(&mut app);
        let page = raea_runtime_page::BevyPage::new(w, h, &dir)?;
        let brand = raea_runtime_react::scene::page_css(&dir, w, &js);
        let mut live = Live { app, dir: dir.clone(), page, brand, dirty: true, mixer: None, mixer_failed: false, lag: (0.0, std::time::Instant::now()), w, h, fps, frames, responsive: false, js, master: None, master_wav: None, game_master: None, game_gain: None, scene_rx: None, scene: None, live3d: None, frame: 0, pads: Vec::new(), taken: Vec::new(), pointer: Default::default(), made: Default::default(), sound_buf: Vec::new(), holders: Vec::new(), baked: None, applied: -1, session: new_session() };
        for b in &baked.init {
            live.app.world().resource::<raea_runtime_react::ReplayFeed>().feed(b.clone());
        }
        live.app.update();
        // 3D: its rows (the scene_program a 3D render uses) and raea-render's live engine opened now, so the first frame draws
        // its scene under the page as the render does (it came from a thread a few frames late, and only with --3d)
        if baked.has_3d {
            if let Some(prog) = raea_runtime_export::scene_program(&live.js, &dir)? {
                live.scene = Some(raea_render::engine::Engine::new_live(&prog, &raea_render::inserted_of(&prog), &dir).map_err(|e| format!("the 3D didn't open: {e}"))?);
            }
        }
        live.baked = Some(std::sync::Arc::new(baked));
        // frame 0 made ready before the clock starts, as a player loads before it plays: its commits applied, the page
        // drawn (its videos decoded, its fonts and pictures loaded), the mixer opened and its sounds loading, none
        // started
        live.replay(0);
        live.draw()?;
        live.warm_sound();
        Ok(live)
    }
}

/// How a timeline is bundled for its bake: its words and perception, on live play's runtime, each frame's components
/// recorded (an edit re-bakes only where they render).
pub(crate) fn baked_opts(dir: &Path) -> R<raea_runtime_react::scene::CompOpts> {
    let mut opts: Vec<String> = Vec::new();
    for (flag, file) in [("--words", "words.json"), ("--perception", "perception.json")] {
        let p = dir.join(file);
        if p.exists() && (file != "words.json" || raea_runtime_react::scene::transcript_file(&p)) {
            opts.push(flag.into());
            opts.push(p.display().to_string());
        }
    }
    opts.push("--lite".into());
    opts.push("--comps".into());
    raea_runtime_react::scene::comp_opts(&opts)
}

impl Live {
    /// This session's seed and start, as an input log keeps them ({"seed", "start"}: a replay's Math.random and Date)
    pub fn session(&self) -> Value {
        serde_json::json!({"seed": self.session.0, "start": self.session.1})
    }

    /// The page's fonts as an exported game carries them (Dom::packaged_fonts_css: the injected faces, every remote one
    /// inlined), frame 0's stylesheets in.
    pub fn packaged_fonts_css(&mut self) -> R<Option<String>> {
        self.step(0, &[], None)?;
        self.draw()?;
        Ok(self.page.packaged_fonts_css())
    }

    /// An exported game's packaged fonts put in the page before its fonts load, so nothing is fetched (its player has no
    /// network).
    pub fn add_packaged_fonts_css(&mut self, css: &str) {
        self.brand = Some(match self.brand.take() {
            Some(b) => format!("{css}\n{b}"),
            None => css.to_string(),
        });
        self.dirty = true;
    }

    /// Every video frame a baked timeline shows: each frame replayed (no painting, nothing decoded) and each <Video>'s
    /// file and source time read from its props (`data-source-time`, what a render places), for the optimized range
    /// copies (raea mezz). Hidden videos are counted too.
    pub fn video_times(&mut self) -> Vec<(PathBuf, f64)> {
        let mut out = Vec::new();
        for f in 0..self.frames {
            self.replay(f);
            let world = self.app.world_mut();
            let mut q = world.query::<(&raea_runtime_react::ReactKind, &raea_runtime_react::ReactProps)>();
            for (k, p) in q.iter(world) {
                if k.0 != "video" {
                    continue;
                }
                let Some(src) = p.0.get("src").and_then(Value::as_str) else { continue };
                let at = p.0.get("data-source-time").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                // an http(s) src from raea's URL cache (#57)
                let file = match raea_io_net::cache::remote(src) {
                    None => raea_runtime_react::scene::media_path(&self.dir, src),
                    Some(Ok(f)) => f,
                    Some(Err(_)) => continue,
                };
                out.push((file, at));
            }
        }
        out
    }

}
