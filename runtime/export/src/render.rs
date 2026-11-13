//! `raea render`: a bundle rendered to video or frames, fed or through the program.

use super::*;

/// A fed render met a 3D frame it has no scene for: it starts again with the scene's rows (or with page files).
pub(crate) const FED_3D: &str = "a 3D frame: the render starts again with its 3D";

/// What `-o` names: an output type's extension, lowercase, or "" for a folder. A path ending in '/', an existing
/// directory, or a name whose extension is no output type (`frames.d`) is a folder: `-o opening7.tsx@opening-phone/` was
/// taken for a "tsx@opening-phone" file, so the --at stills went through page files and every <Screen> came out black
/// (10-23).
pub(crate) fn out_ext(out: &Path) -> String {
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let known = matches!(ext.as_str(), "mp4" | "mov" | "m4v" | "webm" | "gif" | "png" | "jpg" | "jpeg" | "webp" | "sql" | "txt" | "raw");
    if !known || out.as_os_str().to_string_lossy().ends_with('/') || out.is_dir() {
        return String::new();
    }
    ext
}

/// `raea render EDIT.tsx -o OUT ...`: the edit bundled with the words and perception beside it (a flag wins), its
/// export options set, then rendered, or cloned from the export cache when a whole video was made from the same inputs
pub fn render(args: &[String]) -> R<String> {
    // --review DIR: the fixed cameras in colour and clay, stitched (review.rs); --clay: every standard surface grey
    if let Some(d) = flag(args, "--review") {
        return crate::review::review(args, Path::new(&d));
    }
    if args.iter().any(|a| a == "--clay") {
        raea_runtime_scene3d::clay::CLAY.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let tsx = PathBuf::from(&args[1]).canonicalize().map_err(|e| format!("{}: {e}", args[1]))?;
    let out = PathBuf::from(flag(args, "-o").ok_or("-o OUT.mp4|OUT.png|DIR")?);
    let dir = tsx.parent().unwrap().to_path_buf();
    let js = std::env::temp_dir().join(format!("raea-cli-{}.js", std::process::id()));
    // the edit's words.json and perception.json beside it, as play loads them (a render without the flags lost the
    // captions its words drive; the corpus found 10 edits so); a flag still wins
    let mut o = comp_opts(args)?;
    for (slot, file) in [(&mut o.words, "words.json"), (&mut o.perception, "perception.json")] {
        if slot.is_none() && dir.join(file).exists() {
            let v = json_file(&dir.join(file).display().to_string())?;
            // a words.json that isn't a cut's transcript (another tool's file of the same name) is left out, said once
            if file == "words.json" {
                if let Err(e) = words_shape(&v) {
                    eprintln!("raea: {file} beside the edit isn't left in: {e} (pass --words FILE for a transcript elsewhere)");
                    continue;
                }
            }
            *slot = Some(v);
        }
    }
    // a layer is a stamped thing (raea_sys_layers), so --layers stamps the source; RAEA_SOURCE_STAMPS=0 keeps them off
    let layers_on = args.iter().any(|a| a == "--layers") || std::env::var_os("RAEA_LAYERS").is_some_and(|v| v == "1");
    if layers_on && std::env::var_os("RAEA_SOURCE_STAMPS").is_none() {
        // SAFETY: before any thread of the render starts (the bundler and React's runtime start below)
        unsafe { std::env::set_var("RAEA_SOURCE_STAMPS", "1") };
    }
    bundle_composition(&tsx, &js, &o, None)?;
    raea_runtime_react::startup_mark("the composition bundled");
    let wanted = match (flag(args, "--frame"), flag(args, "--at")) {
        (Some(f), _) => Some(vec![f.parse().map_err(|_| "--frame takes a frame number")?]),
        (_, Some(at)) => Some(at.split(',').map(|f| f.trim().parse::<i64>().map_err(|_| "--at takes frame numbers".to_string())).collect::<R<_>>()?),
        _ => None,
    };
    raea_sys_layers::start(layers_on, &out, &tsx);
    *MANIFEST.lock().unwrap() = Some(flag(args, "--manifest").map_or_else(|| raea_sys_layers::manifest_for(&out, &tsx), PathBuf::from));
    TEN_BIT.store(args.iter().any(|a| a == "--10bit"), std::sync::atomic::Ordering::Relaxed);
    AOV.store(args.iter().any(|a| a == "--aov"), std::sync::atomic::Ordering::Relaxed);
    if AOV.load(std::sync::atomic::Ordering::Relaxed) && !out_ext(&out).is_empty() {
        return Err("--aov writes frames: -o names a folder (DIR/frame-NNNNN.png, DIR/motion, DIR/depth)".into());
    }
    let alpha = args.iter().any(|a| a == "--alpha");
    if alpha && !matches!(out_ext(&out).as_str(), "png" | "") {
        return Err(format!("--alpha writes PNGs (OUT.png or a folder) or a transparent movie (OUT.mov, --alpha-codec {}): an .{} holds no alpha", raea_io_alpha::NAMES, out_ext(&out)));
    }
    ALPHA.store(alpha, std::sync::atomic::Ordering::Relaxed);
    let kbps = flag(args, "--kbps").map(|k| k.parse::<u32>().ok().filter(|k| *k > 0).ok_or("--kbps takes a bitrate in kbit/s")).transpose()?;
    KBPS.store(kbps.unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
    let quality = flag(args, "--quality").map(|q| raea_io_media::parse_quality(&q)).transpose()?;
    QUALITY.store(quality.map(f32::to_bits).unwrap_or(u32::MAX), std::sync::atomic::Ordering::Relaxed);
    // --stage WxH: only a responsive composition takes it (as play: a fixed one keeps its own size, scaled to fit)
    *STAGE.lock().unwrap() = match flag(args, "--stage") {
        Some(v) => {
            let size = v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?.max(1), b.parse::<u32>().ok()?.max(1)))).ok_or("--stage takes WxH")?;
            let uses = |p: &Path| std::fs::read_to_string(p).is_ok_and(|t| t.contains("useVideoConfig("));
            let responsive = uses(&tsx) || std::fs::read_dir(dir.join("components")).into_iter().flatten().flatten().any(|e| uses(&e.path()));
            if !responsive {
                eprintln!("raea: --stage {v}: the composition doesn't lay out from useVideoConfig, so it renders at its own size (as the feed's stage scales it)");
            }
            responsive.then_some(size)
        }
        None => None,
    };
    // a whole video made from these same inputs before is the export: cloned from the cache, not rendered again
    let cache = if wanted.is_none() && !raea_sys_layers::on() { export_cache(&tsx, &js, &out, args) } else { None };
    raea_runtime_react::startup_mark("the export cache looked up");
    let manifest = MANIFEST.lock().unwrap().clone();
    if let Some(c) = &cache {
        // with --manifest, the manifest kept beside the export comes back too
        let kept_manifest = manifest.as_ref().map(|m| (m, c.with_extension("manifest")));
        // and what it found when it was drawn (one kept before findings were is drawn again, so its checks run)
        if let (Ok(said), true, true, true) = (std::fs::read_to_string(c.with_extension("said")), c.exists(), kept_manifest.as_ref().is_none_or(|(_, k)| k.exists()), c.with_extension("findings").exists()) {
            if std::fs::copy(c, &out).is_ok() && kept_manifest.as_ref().is_none_or(|(m, k)| std::fs::copy(k, m).is_ok()) {
                raea_core_facts::finding::replay(&c.with_extension("findings"));
                let _ = std::fs::File::options().append(true).open(c).and_then(|f| f.set_modified(std::time::SystemTime::now()));
                let _ = std::fs::remove_file(&js);
                return Ok(with_exposure(format!("{} (the same export as before: from the export cache)", said.replace("{OUT}", &out.display().to_string())), &out, manifest.as_deref()));
            }
        }
    }
    let said = render_bundle(&js, &dir, &out, wanted);
    let _ = std::fs::remove_file(&js);
    if let (Some(c), Ok(s)) = (&cache, &said) {
        if c.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) {
            let tmp = c.with_extension(format!("{}.tmp", std::process::id()));
            let manifest_kept = manifest.as_ref().is_none_or(|m| std::fs::copy(m, c.with_extension("manifest")).is_ok());
            if manifest_kept && std::fs::copy(&out, &tmp).is_ok() && std::fs::rename(&tmp, c).is_ok() && std::fs::write(c.with_extension("said"), s.replace(&out.display().to_string(), "{OUT}")).is_ok() && raea_core_facts::finding::keep_to(&c.with_extension("findings")) {
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
    said.map(|s| with_exposure(s, &out, manifest.as_deref()))
}

/// The render's message with its pictures' exposure after it (a still, or a folder's frames; a video's frames are
/// measured by rendering them to a folder with --at), and the stats in the manifest with --manifest.
fn with_exposure(said: String, out: &Path, manifest: Option<&Path>) -> String {
    let pngs: Vec<PathBuf> = if out.is_dir() {
        raea_io_alpha::frames_in(out)
    } else if out.extension().is_some_and(|x| x == "png") {
        vec![out.to_path_buf()]
    } else {
        return said;
    };
    if pngs.is_empty() {
        return said;
    }
    match raea_sys_light_meter::exposure::of_pngs(&pngs) {
        Ok((e, v)) => {
            let fs = raea_sys_light_meter::exposure::said(&v); // counted with the render's other findings (DR3)
            if let Some(m) = manifest {
                if let Ok(mut doc) = std::fs::read_to_string(m).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())) {
                    if let Some(o) = doc.as_object_mut() {
                        o.insert("exposure".into(), v.clone());
                        let _ = std::fs::write(m, doc.to_string());
                    }
                }
            }
            let mut msg = format!("{said}\nEXPOSURE  {}", e.line());
            for f in &fs {
                msg.push_str(&format!("\n  {}", f.line()));
            }
            msg
        }
        Err(e) => format!("{said}\nEXPOSURE  not measured: {e}"),
    }
}

/// A bundled composition rendered by the engine, in this process (the Mac or the iPhone): React on JavaScriptCore inside Bevy steps
/// its Clock; each frame's page goes through Blitz, its <Video> decoded natively, its 3D as rows; the frames become one
/// raea-render program drawn by raea-render into `out` (.mp4, .png for one frame, a folder of PNGs). `dir` is where relative media resolves. `wanted`: the frames (all when None).
pub fn render_bundle(js: &Path, dir: &Path, out: &Path, wanted: Option<Vec<i64>>) -> R<String> {
    // stills of a 3D scene: its engine opened live beside the page (as the fed path opens it), the page laid over each
    // frame's 3D and each <Screen> painted from the page's own picture of it
    let ext = out_ext(out);
    let stills = (ext == "png" && wanted.as_ref().is_some_and(|w| w.len() == 1)) || ext.is_empty();
    let how = std::env::var_os("RAEA_3D").and_then(|v| v.into_string().ok()).unwrap_or_default();
    // a game: its 3D made frame by frame as it's stepped (rows made beforehand hold every frame's scene: a 10-minute
    // game took 24 GB before its first frame); RAEA_3D=live or =program chooses
    let game = how == "live" || (how != "program" && how != "files" && is_game(js));
    if game && raea_runtime_view::gpuview::paint_device().is_some() {
        let video = wanted.is_none() && matches!(ext.as_str(), "mp4" | "mov" | "txt");
        return render_bundle_with(js, dir, out, wanted, video, None, true);
    }
    if stills && how != "files" {
        let upto = wanted.as_ref().and_then(|w| w.iter().max()).map_or(0, |f| f + 1);
        if let Some(scene) = scene_program_checked(js, dir, upto, wanted.as_deref())? {
            return render_bundle_with(js, dir, out, wanted, false, Some(scene), false);
        }
    }
    // a video: fed, its pages handed to the encoder on the GPU (B1d), until a frame holds 3D, which starts it again
    // with its 3D drawn live under each page
    let video = wanted.is_none() && matches!(ext.as_str(), "mp4" | "mov" | "txt") && how != "files" && raea_runtime_view::gpuview::paint_device().is_some();
    BEVY_3D_PROBE.store(video, std::sync::atomic::Ordering::Relaxed);
    let r = render_bundle_with(js, dir, out, wanted, video, None, false);
    BEVY_3D_PROBE.store(false, std::sync::atomic::Ordering::Relaxed);
    if video && r.as_ref().err().is_some_and(|e| e.contains(FED_3D)) {
        raea_runtime_react::startup_mark("a 3D frame met: the render starts again with its 3D");
        // its 3D made live as each frame steps, React once a frame (#26, #38); RAEA_3D=program makes its rows first
        if how != "program" {
            LIVE_CLIPS.store(true, std::sync::atomic::Ordering::Relaxed);
            let r = render_bundle_with(js, dir, out, None, true, None, true);
            LIVE_CLIPS.store(false, std::sync::atomic::Ordering::Relaxed);
            return r;
        }
        let scene = scene_program_checked(js, dir, 0, None)?;
        return render_bundle_with(js, dir, out, None, scene.is_some(), scene, false);
    }
    r
}

/// The bevy_ui page's video render stops at the first frame with 3D (Err(FED_3D)), to start again fed with its 3D live.
pub(crate) static BEVY_3D_PROBE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// A video's 3D made live (not a game's): each frame's scene kept in its Frame for the clipping check at the end.
pub(crate) static LIVE_CLIPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A game's bundle: it steps state with useGameState (bundles aren't minified, and what isn't used is left out).
pub(crate) fn is_game(js: &Path) -> bool {
    std::fs::read_to_string(js).is_ok_and(|t| t.contains("useGameState("))
}

pub(crate) fn render_bundle_with(js: &Path, dir: &Path, out: &Path, wanted: Option<Vec<i64>>, fed: bool, scene: Option<raea_core_program::ir::Program>, live3d: bool) -> R<String> {
    edit_dir(dir);
    raea_gpu_device::check::scene_black(Some(false));
    raea_runtime_react::native::rendering_in(dir);
    // RAEA_KEEP=DIR: the work folder is DIR, and stays (a folder of its own: renders run side by side, and a
    // newest-folder guess picked up another session's render)
    let keep_dir = std::env::var_os("RAEA_KEEP").map(PathBuf::from).filter(|v| v.starts_with("/"));
    let work = keep_dir.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("raea-cli-r{}-{:?}", std::process::id(), std::thread::current().id())));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    // the work folder goes however the render ends; one a killed render left (memguard's SIGKILL runs no cleanup) goes
    // when it's two hours stale. On 2026-10-17, 169 left behind (several GB each with fast pages) filled the disk
    let _gone = Gone(work.clone());
    sweep_stale_work();
    // React on V8 inside Bevy: the first commit says the composition's size and length
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
    let conf = settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    let (mut w, mut h) = (num(&conf, "width").unwrap_or(1080.0) as u32, num(&conf, "height").unwrap_or(1920.0) as u32);
    // --stage WxH: the page at the stage's size, the composition told it (raea-size), as live.rs opens it for the legacy app
    let stage = *STAGE.lock().unwrap();
    if let Some((sw, sh)) = stage {
        (w, h) = (sw, sh);
        app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-size", "value": {"w": w, "h": h}}));
    }
    let fps = num(&conf, "fps").unwrap_or(30.0);
    let total = num(&conf, "frames").unwrap_or(60.0) as i64;
    let wanted = wanted.unwrap_or_else(|| (0..total).collect());
    raea_gpu_device::check::mem_fits(w, h, None)?;
    let mut page = raea_runtime_page::BevyPage::new(w, h, dir)?;
    let mut cx = Ctx { dir, work: &work, sounds: Vec::new(), scene: Vec::new(), clear: None, marker: None, mix: None, audible: HashMap::new(), brand: page_css(dir, w, js), manifest: MANIFEST.lock().unwrap().as_ref().map(|_| Vec::new()), now: 0, said: Default::default(), made: HashMap::new(), scene3d: None, live3d: None };
    if fed {
        if let Some(p) = scene {
            cx.scene3d = Some(aov_on(raea_render::engine::Engine::new_live(&p, &raea_render::inserted_of(&p), dir)?));
            raea_runtime_react::startup_mark("the 3D's engine open");
        }
        if live3d {
            cx.live3d = Some(LiveScene::default());
        }
        return render_fed(&mut app, js, w, h, fps, total, &mut cx, out, &work, &wanted, &mut page);
    }
    if scene.is_some() || live3d {
        let pg = &mut page;
        let programmed = scene.is_some();
        match scene {
            Some(p) => {
                cx.scene3d = Some(aov_on(raea_render::engine::Engine::new_live(&p, &raea_render::inserted_of(&p), dir)?));
                raea_runtime_react::startup_mark("the 3D's engine open");
            }
            None => cx.live3d = Some(LiveScene::default()),
        }
        let folder = out_ext(out).is_empty();
        if folder {
            std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
        }
        for (i, &f) in wanted.iter().enumerate() {
            let px = raea_gpu_device::check::frame_caught(f, || { // a panic drawing it an error naming it (#33)
            engine_before(&app, &mut cx, f);
            app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": f}));
            settle_with(&mut app, Some(f), 20.0, &mut |w, q| layout_answer(w, pg, &mut cx, f, fps, q))?;
            cx.now = f;
            pg.at_frame(f, fps);
            let mut px = bevy_over_3d(app.world_mut(), pg, &mut cx, fps)?;
            // a camera in a page element (S3a): the 3D's rows were stepped before the page was laid out, so its parts'
            // on* were picked in the whole frame; with the box known (page.viewBox), they're stepped again in it
            if i == 0 && programmed && raea_runtime_page::VIEW_BOX.lock().unwrap().is_some() {
                let upto = wanted.iter().max().map_or(0, |m| m + 1);
                if let Some(p) = scene_program_checked(js, dir, upto, Some(&wanted))? {
                    cx.scene3d = Some(aov_on(raea_render::engine::Engine::new_live(&p, &raea_render::inserted_of(&p), dir)?));
                    px = bevy_over_3d(app.world_mut(), pg, &mut cx, fps)?;
                }
            }
            let (px, twice) = raea_gpu_device::check::black_checked(f, px, raea_sys_page::redraw_full, || bevy_over_3d(app.world_mut(), pg, &mut cx, fps))?;
            if twice { raea_gpu_device::check::black_twice(f, pg.canary().and_then(|()| cx.scene3d.as_ref().map_or(Ok(()), |e| e.canary())))?; lost(f, pg.visible_paint()); }
            Ok(px)
            })
            .map_err(|e| if i == 0 { format!("{e}; no frame was written") } else { format!("{e}; the {i} frames before it are in {}", out.display()) })?;
            let dest = if folder { out.join(format!("frame-{i:05}.png")) } else { out.to_path_buf() };
            image::save_buffer(&dest, &px, w, h, image::ColorType::Rgba8).map_err(|e| format!("{}: {e}", dest.display()))?;
            if folder { write_aov(out, i, cx.scene3d.as_mut())?; }
        }
        // with --manifest, its file as a 2D render writes it (it wrote none, and an older one stayed looking current, #28)
        let clipping = FED_CLIPPING.lock().unwrap().take().and_then(|t| t.join().ok()).unwrap_or_default();
        write_manifest(&mut cx, (w, h, fps, total), clipping, |_| json!([]))?;
        return Ok(if folder {
            format!("APPLIED  {} frames rendered to {} (bevy_ui, the page over its 3D)", wanted.len(), out.display())
        } else {
            format!("APPLIED  frame {} rendered to {} (bevy_ui, the page over its 3D)", wanted.last().copied().unwrap_or(0), out.display())
        });
    }
    let mut frames = Vec::new();
    let mut tones: Vec<raea_sys_mix::ToneCue> = Vec::new();
    let t0 = std::time::Instant::now();
    // RAEA_TIMES: where a render's frames go (React's commit, the page recorded and submitted, the page before
    // collected and stored), totals in ms
    let times = std::env::var_os("RAEA_TIMES").is_some();
    let (mut t_react, mut t_snap, mut t_page) = (0f64, 0f64, 0f64);
    for &f in &wanted {
        let t0 = std::time::Instant::now();
        app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": f}));
        settle_with(&mut app, Some(f), 20.0, &mut |w, q| layout_answer(w, &mut page, &mut cx, f, fps, q))?;
        cx.now = f;
        let t1 = std::time::Instant::now();
        page.at_frame(f, fps);
        let fr = snapshot_bevy(app.world_mut(), &mut page, &mut cx)?;
        if !fr.scene.is_empty() && BEVY_3D_PROBE.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(FED_3D.into());
        }
        tones.push(raea_runtime_sound::tone_at(app.world_mut(), f, fps, dir, media_path, true));
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
        eprintln!("raea times: react {t_react:.0} ms, page recorded and submitted {t_snap:.0} ms, page collected and stored {t_page:.0} ms over {} frames", wanted.len());
    }
    if !frames.is_empty() {
        let k = frames.len() - 1;
        settle_page(&mut frames, k, w, h, &work)?;
    }
    let tw = std::time::Instant::now();
    pages_written()?;
    if times {
        eprintln!("raea times: waiting for the page writers {:.0} ms", tw.elapsed().as_secs_f64() * 1e3);
    }
    let react_ms = t0.elapsed().as_secs_f64() * 1e3 / wanted.len().max(1) as f64;
    drop(app);
    // 3D parts passing through each other on screen: on stderr, and in the manifest for the legacy app's checks
    let clipping = scene::told(&frames, w, h, fps);
    write_manifest(&mut cx, (w, h, fps, total), clipping, |cx| heard(&frames, &tones, cx, fps))?;

    let ext = out_ext(out);
    // raea-render's IR built directly from the frames and drawn by raea-render (checked equal to the retired rrl's compile of the same
    // frames on the goldens, a video edit and a gain ramp before the switch)
    let p = program(&frames, w, h, fps);
    let rows = raea_render::inserted_of(&p);
    let png = ext == "png";
    let target = if png { work.join("png") } else { out.to_path_buf() };
    // the sound: the one mixer core's bounce of the frames, as the fed render and play make it
    // a folder of PNGs (no extension) carries no sound either, so a silent one isn't refused
    let pictures = png || ext.is_empty();
    let sound = if !pictures { core_bounce(&frames, &tones, fps, &p, &rows, dir)? } else { None };
    if !pictures && sound.is_none() && !rows.iter().any(|t| t.kind == "audio") {
        no_sound_but_declared(js)?;
    }
    let opts = raea_runtime_encode::RenderOpts { frames: if png { Some(vec![0]) } else { None }, sound, ..render_opts() };
    let tr = std::time::Instant::now();
    raea_runtime_encode::render(&p, &rows, &work, &target, &opts)?;
    if times {
        eprintln!("raea times: raea-render's render (pages composited over the clips, encoded) {:.0} ms", tr.elapsed().as_secs_f64() * 1e3);
    }
    if png {
        let f = std::fs::read_dir(&target).map_err(|e| e.to_string())?.filter_map(|e| e.ok().map(|e| e.path())).find(|p| p.extension().is_some_and(|x| x == "png")).ok_or("no frame drawn")?;
        std::fs::copy(&f, out).map_err(|e| e.to_string())?;
    }
    if std::env::var_os("RAEA_KEEP").is_none() {
        let _ = std::fs::remove_dir_all(&work);
    }
    drop(page);
    Ok(format!("APPLIED  {} frames rendered to {} ({react_ms:.0} ms a frame in React and bevy_ui)", frames.len(), out.display()))
}

/// An engine for the scene path, drawing the guide data when `--aov` asked for it.
pub(crate) fn aov_on(mut e: raea_render::engine::Engine) -> raea_render::engine::Engine {
    if AOV.load(std::sync::atomic::Ordering::Relaxed) {
        e.want_aov();
    }
    e
}

/// Frame i's guide data (RT1) as mlxdlss run-sequence reads it: DIR/motion/frame-NNNNN.f32 (H x W x 2 float32) and
/// DIR/depth/frame-NNNNN.f32 (H x W float32), little-endian, rows top to bottom.
fn write_aov(out: &Path, i: usize, e: Option<&mut raea_render::engine::Engine>) -> R<()> {
    if !AOV.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }
    let a = e.and_then(|e| e.take_aov()).ok_or_else(|| format!("frame {i}: --aov, but the frame drew no motion vectors or depth (a 3D camera draws them)"))?;
    for (sub, v) in [("motion", &a.motion), ("depth", &a.depth)] {
        let d = out.join(sub);
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        let f = d.join(format!("frame-{i:05}.f32"));
        std::fs::write(&f, v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).map_err(|e| format!("{}: {e}", f.display()))?;
    }
    Ok(())
}
