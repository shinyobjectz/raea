//! Headless runs, a composition's sound and mix alone, and its 3D program, each from React stepped through its frames.

use super::*;

/// Run a bundled program (bundle_script's) in the engine's V8 with the native calls, headless: its exit code and what
/// it printed. The same on the Mac and the iPhone (raea/react/ios).
pub fn run_bundle(bundle: &Path, secs: f64) -> R<(i32, String)> {
    raea_runtime_react::native::DONE.store(-1, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut p) = raea_runtime_react::native::PRINTED.lock() {
        p.clear();
    }
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(bundle.to_path_buf()));
    let t0 = std::time::Instant::now();
    loop {
        app.update();
        let d = raea_runtime_react::native::DONE.load(std::sync::atomic::Ordering::SeqCst);
        if d >= 0 {
            let printed = raea_runtime_react::native::PRINTED.lock().map(|p| p.clone()).unwrap_or_default();
            return Ok((d, printed));
        }
        if t0.elapsed().as_secs_f64() > secs {
            return Err(format!("{} didn't finish in {secs} s (it throws at load, or never returns)", bundle.display()));
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// `run_bundle` for a program whose output is the terminal (`raea run`, `raea test`, `raea blender run`): its console
/// goes there too, log and info to stdout, warn and error to stderr (#168). raea check's asks and a spec's read keep
/// theirs out of stdout, which is their answer.
pub fn run_bundle_to_terminal(bundle: &Path, secs: f64) -> R<(i32, String)> {
    let was = raea_runtime_react::native::CONSOLE.swap(true, std::sync::atomic::Ordering::SeqCst);
    let ran = run_bundle(bundle, secs);
    raea_runtime_react::native::CONSOLE.store(was, std::sync::atomic::Ordering::SeqCst);
    ran
}

/// The composition's sound alone, mixed and mastered as a render's is (its <Mix>'s loudness and ceiling through raea-render's
/// master row): React is stepped through every frame for its sounds, with no page painted and no video decoded. Live
/// play hands this to the mixer, so a timeline sounds as its render does.
pub fn sound_only(js: &Path, dir: &Path) -> R<Option<raea_io_media::Audio>> {
    // the one mixer core: its bounce of the frames, mastered as the render's bounce is (core_master); its staticFile
    // assets (a <FramePass> file) found in the edit's folder, as a render finds them, not at the filesystem's /media
    edit_dir(dir);
    let Some((cues, mix, p, rows, fps)) = core_cues(js, dir, false)? else { return Ok(None) };
    if cues.iter().all(|c| c.sounds.is_empty() && c.tone.graph.is_none() && c.tone.evs.is_empty() && c.tone.speech.is_none()) { return Ok(None) }
    Ok(Some(core_master(&cues, fps, mix, &p, &rows, dir)?.1))
}

/// The gain (dB) and ceiling (dBTP) the composition's render masters its sound with: a game's live voices go through the
/// same (its scripted run, played through every frame, as its render plays it). None without sound or a <Mix>.
pub fn master_gain(js: &Path, dir: &Path) -> R<Option<(f64, f64)>> {
    core_master_gain(js, dir)
}

/// A composition's 3D as the rows its render draws (camera, lights, meshes, their motion keyed by frame): React stepped
/// through every frame for its 3D, no page painted. None when it has none. Live play draws a 3D scene from this
/// (raea-render's engine, live), so the scene played is the scene rendered.
pub fn scene_program(js: &Path, dir: &Path) -> R<Option<raea_core_program::ir::Program>> {
    scene_program_to(js, dir, 0)
}

/// `scene_program` stepped through frame `upto - 1` at least: a game's frame past the composition's length (it plays on
/// past it, live) has its 3D, not the base pose a frame past the program's last shows.
pub(crate) fn scene_program_to(js: &Path, dir: &Path, upto: i64) -> R<Option<raea_core_program::ir::Program>> {
    Ok(scene_frames_to(js, dir, upto)?.map(|(frames, w, h, fps)| program(&frames, w, h, fps)))
}

/// A render's 3D rows as `scene_program_to` makes them, and the 3D parts that pass through each other on screen in
/// `wanted` (one frame, a still; else all): said on stderr, and kept for the render's manifest (FED_CLIPPING).
pub(crate) fn scene_program_checked(js: &Path, dir: &Path, upto: i64, wanted: Option<&[i64]>) -> R<Option<raea_core_program::ir::Program>> {
    // stills: the frames asked for looked at first, each on its own (frames are pure). When none of them shows 3D,
    // their render needs no rows, and a 2D piece's still skips stepping its whole timeline (an 80 s film's still took
    // 16 s here, all of it the rows' pass finding no 3D)
    if let Some(w) = wanted.filter(|w| !w.is_empty()) {
        raea_runtime_react::startup_mark("the 3D's rows: the wanted frames looked at");
        if !frames_hold_3d(js, dir, w)? {
            raea_runtime_react::startup_mark("the 3D's rows: none in the wanted frames");
            return Ok(None);
        }
    }
    raea_runtime_react::startup_mark("the 3D's rows: stepping React through the frames");
    // a still or an --at list steps only its frames and their neighbours (scene_frames), through one past the last
    // (what reads ahead, as a shutter might, has it); anything else steps the whole composition
    let got = match wanted {
        Some(w) if !w.is_empty() => scene_frames(js, dir, upto.max(w.iter().max().map_or(0, |f| f + 1)) + 1, false, w)?,
        _ => scene_frames_to(js, dir, upto)?,
    };
    let Some((frames, w, h, fps)) = got else { return Ok(None) };
    // the count too: tools/bevy-parity charges raea the time a frame takes to step (docs/control.feature FC0a)
    raea_runtime_react::startup_mark(&format!("the 3D's rows: {} frames stepped", frames.len()));
    let p = program(&frames, w, h, fps);
    raea_runtime_react::startup_mark("the 3D's rows: program made");
    // a still or an --at list: its own frames (the rest are only stepped, or copies: scene_frames)
    let frames: Vec<(i64, Frame)> = match wanted.map(|w| w.iter().copied().collect::<std::collections::HashSet<i64>>()) {
        Some(w) => frames.into_iter().filter(|(f, _)| w.contains(f)).collect(),
        None => frames,
    };
    // stills: their end checks run once their page is laid out (render_bundle_with keeps each still's scene), so
    // the page's words are known to them (SG11)
    if wanted.is_some_and(|w| !w.is_empty()) {
        return Ok(Some(p));
    }
    // the clipping check on a thread of its own while the engine opens and the frames render (3 s over the Rome
    // film's 8,064 frames, which the render waited for before its engine opened); the render joins it at its end
    struct Prepassed(Vec<(i64, Frame)>);
    // SAFETY: these frames were stepped for their 3D alone: no page was painted, so none holds a Painting (the one
    // field that isn't Send), and the thread is their only owner
    unsafe impl Send for Prepassed {}
    let owned = Prepassed(frames);
    *FED_CLIPPING.lock().unwrap() = Some(std::thread::spawn(move || {
        let mut o = owned;
        scene::told_taking(&mut o.0, None, w, h, fps)
    }));
    Ok(Some(p))
}
