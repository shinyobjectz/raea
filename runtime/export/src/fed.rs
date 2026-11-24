//! The fed render: each frame stepped in the App and handed to the engine as it comes.

use super::*;

/// What `scene_program_checked` finds (on its own thread), for the fed render that draws that scene to write in its
/// manifest.
pub(crate) static FED_CLIPPING: std::sync::Mutex<Option<std::thread::JoinHandle<Vec<raea_core_facts::Finding>>>> = std::sync::Mutex::new(None);

/// RAEA_FRAME_LOG=FILE: a fed render (an .mp4 or .mov of a piece with 3D; not --frame or --at, which write none) writes
/// one line a frame (ms, tab-separated): frame, react (the frame's commit
/// settled), sync (the frame's page and 3D made: the page synced and drawn by bevy_ui, the 3D drawn under it), the
/// draw3d, gpu3d, page and gpupage columns (0 since the bevy_ui page: `sync` holds them), gpu (the wait for the frame's
/// GPU work, in raea-render).
pub(crate) static FRAME_LOG: std::sync::Mutex<Option<FrameLog>> = std::sync::Mutex::new(None);

#[derive(Default)]
pub(crate) struct FrameLog {
    /// this frame's: sync, draw3d, gpu3d, page, gpupage
    pub(crate) cur: [f64; 5],
    pub(crate) rows: Vec<(i64, f64, [f64; 5])>,
}

/// Whether any of `frames` shows 3D: each rendered on its own in one runtime (frames are pure, so a frame stepped
/// alone is the frame a whole pass would have stepped), stopping at the first that does.
pub(crate) fn frames_hold_3d(js: &Path, dir: &Path, frames: &[i64]) -> R<bool> {
    Ok(looked(js, dir, frames, true)?.0)
}

/// Whether stills at `frames` can make their 3D live, at each still (SG4a): one of them holds 3D, and none holds a
/// compute that carries GPU state from frame to frame (its rows step every frame before the still instead).
pub(crate) fn stills_live(js: &Path, dir: &Path, frames: &[i64]) -> R<bool> {
    let (held, carries) = looked(js, dir, frames, false)?;
    Ok(held && !carries)
}

/// Each of `frames` stepped alone: whether one holds 3D and whether one holds a compute that carries state (`first`:
/// done at the first that holds 3D).
fn looked(js: &Path, dir: &Path, frames: &[i64], first: bool) -> R<(bool, bool)> {
    edit_dir(dir);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
    app.insert_resource(raea_runtime_react::OneBatch::default());
    settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    let (mut held, mut carries) = (false, false);
    for &f in frames {
        raea_runtime_bake::step_frames(&mut app, f, f + 1, |app, _| {
            let scene = scene_of(app.world_mut());
            held |= !scene.is_empty();
            carries |= raea_runtime_scene::carries_state(&scene);
            Ok(())
        })?;
        if held && first {
            break;
        }
    }
    Ok((held, carries))
}

/// The frames `scene_program_to` reads its rows from, with the composition's size and rate. Stepped as a bake steps
/// (bake.rs, lightspeed T3): React renders a range of frames back to back (raea-frames) and each frame's commit is
/// applied on its own (OneBatch, PreUpdate only), its scene read after it; the frames split across runtimes on threads,
/// since frames are pure (each runtime's first frame is its whole tree). One event a frame, each waited on the JS
/// thread and back, took the Rome film's 8,064 frames 6 s on a quiet machine and 38 s at load 50, before its engine
/// could open. RAEA_BAKE_WORKERS sets how many runtimes (4; one for short timelines).
pub fn scene_frames_to(js: &Path, dir: &Path, upto: i64) -> R<Option<(Vec<(i64, Frame)>, u32, u32, f64)>> {
    scene_frames(js, dir, upto, true, &[])
}

/// `whole`: the whole composition at least (scene_frames_to); else only frames 0..upto, as a still at frame f needs
/// (a frame-0 still of 10 000 cubes over 121 frames stepped all 121, 69 s for 2 frames' 12 s; docs/control.feature
/// FC1a). `wanted`, the frames a still or an `--at` list draws: when they and their neighbours (what reads a frame
/// either side, as a shutter does) are under half of the frames, only those are stepped (frames are pure) and each
/// frame between holds the stepped one before it, never drawn (#26: four `--at` frames of 360 stepped all 360, 64 s)
pub fn scene_frames(js: &Path, dir: &Path, upto: i64, whole: bool, wanted: &[i64]) -> R<Option<(Vec<(i64, Frame)>, u32, u32, f64)>> {
    edit_dir(dir);
    let runtime = |js: &Path| {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
        app.insert_resource(raea_runtime_react::OneBatch::default());
        app
    };
    let mut app = runtime(js);
    let conf = settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    let (w, h) = (num(&conf, "width").unwrap_or(1080.0) as u32, num(&conf, "height").unwrap_or(1920.0) as u32);
    let fps = num(&conf, "fps").unwrap_or(30.0);
    let length = num(&conf, "frames").unwrap_or(60.0) as i64;
    let total = if whole || upto <= 0 { length.max(upto) } else { upto };
    let around: std::collections::BTreeSet<i64> = wanted.iter().filter(|f| **f >= 0).flat_map(|&f| f.saturating_sub(1).max(0)..=f + 1).collect();
    type Scenes = Vec<(i64, Vec<Node3>, Option<String>)>;
    let step = |app: &mut App, a: i64, b: i64| -> R<Scenes> {
        let mut out: Scenes = Vec::with_capacity((b - a).max(0) as usize);
        raea_runtime_bake::step_frames(app, a, b, |app, f| {
            let scene = scene_of(app.world_mut());
            let clear = scene_clear(app.world_mut());
            out.push((f, scene, clear));
            // the mount is the first frame's: tools/bevy-parity charges the frames after it, as Bevy's setup isn't in its frame time
            if f == 0 {
                raea_runtime_react::startup_mark("the 3D's rows: frame 0 stepped");
            }
            Ok(())
        })?;
        Ok(out)
    };
    // the wanted frames alone, unless a compute carries GPU state from frame to frame: then every frame up to them
    let scenes = 'scenes: {
        if !around.is_empty() && around.len() * 2 < total as usize {
            // each run of neighbouring frames stepped as one range, the runs one after another in the one runtime
            let runs = raea_runtime_bake::runs(&around);
            let stepped: Scenes = runs.into_iter().map(|(a, b)| step(&mut app, a, b)).collect::<R<Vec<_>>>()?.into_iter().flatten().collect();
            raea_runtime_react::startup_mark(&format!("the 3D's rows: {} of {total} frames stepped (the wanted ones and their neighbours)", stepped.len()));
            if !stepped.iter().any(|s| raea_runtime_scene::carries_state(&s.1)) {
                break 'scenes (0..total.max(stepped.last().map_or(0, |s| s.0 + 1))).map(|f| (f, stepped[stepped.partition_point(|s| s.0 <= f).saturating_sub(1)].clone())).map(|(f, (_, scene, clear))| (f, scene, clear)).collect();
            }
            app = runtime(js);
            settle(&mut app, Some(-1), 20.0)?;
        }
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
                        edit_dir(&dir);
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
        scenes
    };
    let frames: Vec<(i64, Frame)> = scenes.into_iter().map(|(f, scene, clear)| (f, Frame { screens: Vec::new(), mix: None, page: None, sounds: Vec::new(), scene, clear, same: false })).collect();
    if frames.iter().all(|(_, f)| f.scene.is_empty()) {
        return Ok(None);
    }
    Ok(Some((frames, w, h, fps)))
}

/// A live game's 3D (Live): the frame just stepped's scene, as play computed it, made into rows for raea-render's live engine:
/// the engine opened at the first frame with any 3D (from that frame's rows, as a render's program holds them there),
/// then each frame only what changed set on it (LiveScene). A new page size starts the engine again.
pub fn live_scene(world: &mut World, ls: &mut LiveScene, engine: &mut Option<raea_render::engine::Engine>, size: (u32, u32), fps: f64, dir: &Path) -> R<()> {
    if engine.as_ref().is_some_and(|e| e.size() != size) {
        *engine = None;
        ls.reset();
    }
    let scene = scene_of(world);
    if scene.is_empty() && engine.is_none() {
        return Ok(());
    }
    edit_dir(dir);
    match engine {
        None => {
            let clear = scene_clear(world);
            let p = ls.open(Shot { scene: &scene, clear: clear.as_deref() }, size.0, size.1, fps);
            let mut e = aov_on(raea_render::engine::Engine::new_live(&p, &raea_render::inserted_of(&p), dir).map_err(|e| format!("the 3D didn't open: {e}"))?);
            e.set_rows(&[])?;
            *engine = Some(e);
        }
        Some(e) => {
            // the frame's own clear, keyed as a render keys a background that changes (RR1)
            let clear = scene_clear(world);
            let (rows, gone) = ls.frame(Shot { scene: &scene, clear: clear.as_deref() });
            if !gone.is_empty() {
                e.remove_rows(&gone)?;
            }
            if !rows.is_empty() {
                e.set_rows(&rows)?;
            }
        }
    }
    Ok(())
}

/// The Engine's reads (docs/sdk.feature S2d; off with RAEA_ENGINE_READS=0): the Engine stepped through frame f - 1 and its pushes sent before React
/// is asked for frame f, so frame f's reads answer from frame f - 1's World in every render: a jump to f steps 0..f - 1
/// first (it laid out f with no answer), and in order it's the frame already drawn, left as it was.
pub(crate) fn engine_before(app: &App, cx: &mut Ctx, f: i64) {
    if f <= 0 || !raea_runtime_react::shared::engine_reads() {
        return;
    }
    if let Some(e) = cx.scene3d.as_mut() {
        // in order, frame f - 1 is the one drawn last and its World is f - 1's already. Drawn again, it was a whole update
        // more: every frame of a render drew twice, and every temporal pass (Solari's, MetalFX's, the split emission's)
        // took a second frame in which nothing moved, which live play never draws (a moving orb's rim, seen still,
        // stayed as rings at every place it had been)
        if e.drawn() != Some(f - 1) {
            e.draw(f - 1);
        }
    }
    raea_runtime_react::shared::engine_flush(app.world().resource::<ReactOutbox>());
}

/// The render with its pages handed to raea-render on the GPU: raea-render asks for each frame in order, the frame is stepped and its
/// page painted into one page buffer on the paint device, and raea-render composites it as it is and encodes it (no page
/// file, no page read back; gate 5 of no-V8). The sound is the frames' sound rows, mixed once the last frame is made.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_fed(app: &mut App, js: &Path, w: u32, h: u32, fps: f64, total: i64, cx: &mut Ctx, out: &Path, work: &Path, wanted: &[i64], bevy: &mut raea_runtime_page::BevyPage) -> R<String> {
    let page = bevy_fed_buffer(w, h)?;
    // the 3D made from the scene's rows beforehand (not a game's): stepped again once a camera's box in the page is known
    let programmed = cx.scene3d.is_some() && cx.live3d.is_none();
    let mut restepped = false;
    let frames: std::cell::RefCell<Vec<(i64, Frame)>> = std::cell::RefCell::new(Vec::with_capacity(total.max(0) as usize));
    // the sound is the one mixer core's bounce of the frames' sounds and notes, as live play mixes them (raea-mixer)
    let notes: std::cell::RefCell<Vec<raea_sys_mix::ToneCue>> = std::cell::RefCell::new(Vec::new());
    let dir = cx.dir.to_path_buf();
    // the sound bounced as the frames come, on its own thread (it was mixed after the last frame: 3 s on the export)
    let streamer = std::cell::RefCell::new(Some(raea_sys_mix::offline::Streamer::start(fps)));
    // a video's 3D made live is checked as its frames come, each frame's parts let go once read (SG4a)
    let ends = std::cell::RefCell::new(LIVE_CLIPS.load(std::sync::atomic::Ordering::Relaxed).then(|| raea_runtime_endchecks::EndChecks::new(w, h, fps)));
    let t0 = std::time::Instant::now();
    let pic = raea_core_program::ir::Program { fps, width: w, height: h, frames: total, ..Default::default() };
    let frame_log = std::env::var_os("RAEA_FRAME_LOG").map(PathBuf::from);
    if frame_log.is_some() {
        *FRAME_LOG.lock().unwrap() = Some(FrameLog::default());
        *raea_runtime_view::gpuview::FRAME_WAITS.lock().unwrap() = Some(Vec::new());
    }
    // the frame React was last asked for ahead of its turn (the render loop below)
    let mut asked: Option<i64> = None;
    // RAEA_REACT_AHEAD=0: each frame asked for in its turn, as before; and with the Engine's reads, whose frame f - 1
    // is stepped before f is asked for (engine_before)
    let ahead = std::env::var("RAEA_REACT_AHEAD").as_deref() != Ok("0") && !raea_runtime_react::shared::engine_reads();
    let cuts = raea_runtime_encode::Cuts::default();
    let r = raea_runtime_encode::render_fed(
        &pic,
        work,
        out,
        &raea_runtime_encode::RenderOpts { cuts: cuts.clone(), ..render_opts() },
        |f| {
            if f == 0 {
                raea_runtime_react::startup_mark("the fed render's first frame");
            }
            let t_react = std::time::Instant::now();
            engine_before(app, cx, f);
            if asked != Some(f) {
                app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": f}));
            }
            settle_with(app, Some(f), 20.0, &mut |w, q| layout_answer(w, bevy, cx, f, fps, q))?;
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
            bevy.at_frame(f, fps);
            let mut fr = bevy_fed(app.world_mut(), bevy, cx, &page, fps)?;
            // a camera in a page element (S3a): the 3D's rows were stepped before the page was laid out, so its parts'
            // on* were picked in the whole frame; with the box known (page.viewBox), they're stepped again in it and
            // this frame drawn again, as a still's render does
            if programmed && !restepped && raea_runtime_page::VIEW_BOX.lock().unwrap().is_some() {
                restepped = true;
                if let Some(p) = scene_program_checked(js, &dir, total, Some(wanted))? {
                    cx.scene3d = Some(raea_render::engine::Engine::new_live(&p, &raea_render::inserted_of(&p), &dir)?);
                    if let Some(m) = cx.manifest.as_mut() {
                        m.pop();
                    }
                    fr = bevy_fed(app.world_mut(), bevy, cx, &page, fps)?;
                }
            }
            // a page draw past Vello's bump buffers, painted again at Vello's own sizes before the frame is encoded (#23)
            fr = raea_gpu_device::check::fed_checked(f, fr, raea_sys_page::wait_drawn, raea_sys_page::redraw_full, || { cx.manifest.as_mut().map(|m| m.pop()); bevy_fed(app.world_mut(), bevy, cx, &page, fps) })?;
            // a shot starts here (an outermost Sequence, a camera cut): the video's next segment does too (RR3)
            cuts.find(f, raea_runtime_page::shot_starts(app.world_mut()), matches!(&cx.scene3d, Some(e) if raea_sys_frame::shots::Cut::at(e.world(), f)));
            // a video's 3D made live: this frame's parts to the end checks (`ends`), with its page (the manifest's newest)
            if let Some(e) = ends.borrow_mut().as_mut() {
                let page = cx.manifest.as_deref().map(|m| raea_sys_stage::pages(&m[m.len().saturating_sub(4)..], &[f], w, h).remove(0));
                e.push(f, &raea_runtime_scene::items_of(&scene_of(app.world_mut())), page.as_ref());
            }
            if cx.scene3d.is_none() && cx.live3d.is_none() && !cx.scene.is_empty() && BEVY_3D_PROBE.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(FED_3D.into());
            }
            if let Some(l) = FRAME_LOG.lock().unwrap().as_mut() {
                let mut cur = std::mem::take(&mut l.cur);
                // sync: the snapshot's time less the 3D's and the page's (and the serial waits)
                cur[0] = t_snap.elapsed().as_secs_f64() * 1e3 - cur[1] - cur[2] - cur[3] - cur[4];
                l.rows.push((f, react_ms, cur));
            }
            let tone = raea_runtime_sound::tone_at(app.world_mut(), f, fps, &dir, media_path, true);
            if let Some(st) = streamer.borrow().as_ref() {
                st.push(raea_sys_mix::offline::Cue { sounds: fr.sounds.iter().map(core_sound).collect(), tone: tone.clone() });
            }
            notes.borrow_mut().push(tone);
            frames.borrow_mut().push((f, fr));
            if f as usize % raea_render::engine::mark_every() == 0 {
                raea_render::engine::mem_mark(&format!("frame {f}"));
            }
            Ok(page.clone())
        },
        || {
            // its sound's rows alone: the 3D was drawn as the frames came (SG4a)
            let p = program_of(&frames.borrow(), w, h, fps, false);
            sound_lint(&p, fps);
            let st = streamer.borrow_mut().take().ok_or("the sound was asked for twice")?;
            if frames.borrow().iter().all(|(_, f)| f.sounds.is_empty()) && notes.borrow().iter().all(|n| n.is_empty()) {
                st.discard()?;
                no_sound_but_declared(js)?;
                return Ok(None);
            }
            let t_bounce = std::time::Instant::now();
            let raw = st.finish()?;
            let bounce_ms = t_bounce.elapsed().as_secs_f64() * 1e3;
            let t_master = std::time::Instant::now();
            let (master, a) = core_master_raw(&raw, frames.borrow().iter().find_map(|(_, f)| f.mix))?;
            if let Some(p) = std::env::var_os("RAEA_AUDIO_BOUNCE") {
                raea_sys_mix::write_wav(&a, Path::new(&p))?;
            }
            if raea_io_trace::stat("RAEA_MASTER_TRACE") {
                eprintln!("core bounce (streamed): {:.2} s of sound, master {master:?}; the bounce finished {bounce_ms:.0} ms after the last frame, the master took {:.0} ms", a.duration(), t_master.elapsed().as_secs_f64() * 1e3);
            }
            Ok(Some(a))
        },
    );
    if let Some(path) = frame_log {
        let log = FRAME_LOG.lock().unwrap().take().unwrap_or_default();
        let waits: HashMap<i64, f64> = raea_runtime_view::gpuview::FRAME_WAITS.lock().unwrap().take().unwrap_or_default().into_iter().collect();
        let mut out = String::from("frame\treact\tsync\tdraw3d\tgpu3d\tpage\tgpupage\tgpu\n");
        for (f, react, c) in &log.rows {
            out += &format!("{f}\t{react:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\n", c[0], c[1], c[2], c[3], c[4], waits.get(f).copied().unwrap_or(0.0));
        }
        std::fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    r?;
    raea_render::engine::mem_mark("end: the frames, sound and video done");
    let n = frames.borrow().len();
    // 3D parts passing through each other on screen: on stderr, and in the manifest for the legacy app's checks
    let found = FED_CLIPPING.lock().unwrap().take().and_then(|t| t.join().ok());
    let clipping = match (found, ends.into_inner()) {
        (Some(found), _) => found,
        (None, Some(e)) => scene::ended(e),
        (None, None) => scene::told_taking(&mut frames.borrow_mut(), cx.manifest.as_deref(), w, h, fps),
    };
    write_manifest(cx, (w, h, fps, total), clipping, |cx| heard(&frames.borrow(), &notes.borrow(), cx, fps))?;
    let ms = t0.elapsed().as_secs_f64() * 1e3 / n.max(1) as f64;
    Ok(format!("APPLIED  {n} frames rendered to {} ({ms:.1} ms a frame, pages handed to raea-render on the GPU)", out.display()))
}
