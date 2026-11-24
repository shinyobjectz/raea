//! A render's sound: the bounce of its cues and tones, mastered.

use super::*;

/// The frames' sound bounced through the one mixer core (raea-mixer), mastered as live play masters it: the gain raea-render
/// works out for the <Mix> (what a game's live voices get, motion::master_gain) and the limiter at its ceiling; without
/// a <Mix>, the limiter at -1 dBFS. None when nothing sounds.
pub(crate) fn core_bounce(frames: &[(i64, Frame)], notes: &[raea_sys_mix::ToneCue], fps: f64, p: &raea_core_program::ir::Program, rows: &[raea_core_program::eval::Tup], dir: &Path) -> R<Option<raea_io_media::Audio>> {
    if frames.iter().all(|(_, f)| f.sounds.is_empty()) && notes.iter().all(|n| n.is_empty()) {
        return Ok(None);
    }
    let cues: Vec<raea_sys_mix::offline::Cue> = frames
        .iter()
        .enumerate()
        .map(|(i, (_, f))| raea_sys_mix::offline::Cue {
            sounds: f.sounds.iter().map(|s| raea_sys_mix::Sound { key: s.key.as_str().into(), file: Path::new(&s.src).into(), at: s.at, volume: s.volume as f32, rate: s.rate, fade_in: s.fade_in.unwrap_or(0.0), fade_out: s.fade_out.unwrap_or(0.0), bus: s.bus.as_deref().map(Into::into) }).collect(),
            tone: notes.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    sound_lint(p, fps);
    let t = std::time::Instant::now();
    let (master, a) = core_master(&cues, fps, frames.iter().find_map(|(_, f)| f.mix), p, rows, dir)?;
    // RAEA_AUDIO_BOUNCE=OUT.wav: the bounce as it left the core (before the encoder), to hold against live play's tap
    if let Some(p) = std::env::var_os("RAEA_AUDIO_BOUNCE") {
        raea_sys_mix::write_wav(&a, Path::new(&p))?;
    }
    if raea_io_trace::stat("RAEA_MASTER_TRACE") {
        eprintln!("core bounce: {:.2} s of sound in {:.0} ms, master {master:?}", a.duration(), t.elapsed().as_secs_f64() * 1e3);
    }
    Ok(Some(a))
}

/// A render that would have no sound although the edit writes an <Audio> that always plays (raea_runtime_sound::
/// always_plays_audio, read from its bundle): an error, not a silent file. A wrapper that drops its children (a <Mix>
/// did, 2026-10-18: a 20 s ad rendered with no audio track), a sound that's never on screen, or a generated sound that
/// was never made all end here. The folder's .tsx files were read before, so a sibling's <Audio> refused an edit with
/// none (10-24); and an <Audio> behind a condition refused the starter edit before it had cuts.
pub(crate) fn no_sound_but_declared(js: &Path) -> R<()> {
    if raea_runtime_sound::always_plays_audio(&std::fs::read_to_string(js).unwrap_or_default()) {
        return Err("the edit has <Audio> but no sound reached the render, so the file would be silent: is an <Audio> inside a component that doesn't render its children, outside every frame's Sequence, or a generated sound that wasn't made? (an <Audio> inside <Mix>...</Mix> was dropped before 2026-10-18)".into());
    }
    Ok(())
}

pub(crate) fn core_sound(s: &Sound) -> raea_sys_mix::Sound {
    raea_sys_mix::Sound { key: s.key.as_str().into(), file: Path::new(&s.src).into(), at: s.at, volume: s.volume as f32, rate: s.rate, fade_in: s.fade_in.unwrap_or(0.0), fade_out: s.fade_out.unwrap_or(0.0), bus: s.bus.as_deref().map(Into::into) }
}

/// The one mixer core's master for these frames, and their bounce through it: with a <Mix>, the gain raea-render works out
/// for it and its ceiling, held after AAC on Apple as the render's master is (master_for): where the encoder lifts the
/// true peak over the ceiling, the limiter's ceiling goes down by the overshoot and the frames are bounced again (twice
/// at most). Live play is given the same (master-gain), so its tap is still the bounce.
pub(crate) fn core_master(cues: &[raea_sys_mix::offline::Cue], fps: f64, mix: Option<(f64, f64)>, _p: &raea_core_program::ir::Program, _rows: &[raea_core_program::eval::Tup], _dir: &Path) -> R<(Option<(f64, f64)>, raea_io_media::Audio)> {
    core_master_raw(&raea_sys_mix::offline::bounce_raw(cues, fps)?, mix)
}

/// The master over a raw bounce (raea_sys_mix::offline::master_raw: the limiter live play's master node runs): the gain
/// to the <Mix>'s loudness from the raw sound, and on Apple the ceiling lowered by any AAC overshoot. Each try is a
/// pass of the limiter over the buffer, not another bounce.
pub(crate) fn core_master_raw(raw: &raea_io_media::Audio, mix: Option<(f64, f64)>) -> R<(Option<(f64, f64)>, raea_io_media::Audio)> {
    #[cfg(target_vendor = "apple")]
    let enc: Option<&dyn Fn(&raea_io_media::Audio) -> R<raea_io_media::Audio>> = Some(&raea_io_media::apple::aac_round_trip);
    #[cfg(not(target_vendor = "apple"))]
    let enc: Option<&dyn Fn(&raea_io_media::Audio) -> R<raea_io_media::Audio>> = None;
    Ok(raea_sys_mix::offline::master_mix(raw, mix, enc))
}

/// master_gain with the one mixer core: core_master's gain and ceiling for the frames, the ones the render's bounce uses.
pub(crate) fn core_master_gain(js: &Path, dir: &Path) -> R<Option<(f64, f64)>> {
    let Some((cues, mix, p, rows, fps)) = core_cues(js, dir, true)? else { return Ok(None) };
    Ok(core_master(&cues, fps, mix, &p, &rows, dir)?.0)
}

/// React stepped through every frame for its sounds and notes, as the render steps it: the frames' cues, whether there's
/// a <Mix>, their raea-render program and rows, and the fps. With `need_mix`, None at once when the first frame has no <Mix>.
#[allow(clippy::type_complexity)]
pub(crate) fn core_cues(js: &Path, dir: &Path, need_mix: bool) -> R<Option<(Vec<raea_sys_mix::offline::Cue>, Option<(f64, f64)>, raea_core_program::ir::Program, Vec<raea_core_program::eval::Tup>, f64)>> {
    raea_runtime_react::native::speech_in_background();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
    let conf = settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    let (w, h) = (num(&conf, "width").unwrap_or(1080.0) as u32, num(&conf, "height").unwrap_or(1920.0) as u32);
    let fps = num(&conf, "fps").unwrap_or(30.0);
    let total = num(&conf, "frames").unwrap_or(60.0) as i64;
    let work = std::env::temp_dir();
    let mut cx = Ctx { dir, work: &work, sounds: Vec::new(), scene: Vec::new(), clear: None, marker: None, mix: None, audible: HashMap::new(), brand: None, manifest: None, now: 0, said: Default::default(), made: HashMap::new(), scene3d: None, live3d: None };
    let (mut frames, mut notes) = (Vec::new(), Vec::new());
    for f in 0..total {
        app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": f}));
        // (a layout read here gets no boxes: the sound pass draws no page, docs/layout-reads.feature LR2)
        settle_with(&mut app, Some(f), 20.0, &mut raea_runtime_react::layout::nothing_drawn())?;
        cx.now = f;
        cx.sounds.clear();
        cx.mix = None;
        let world = app.world_mut();
        let mut q = world.query::<(&ReactKind, Option<&ReactProps>, Option<&raea_runtime_react::ReactNode>)>();
        let items: Vec<(String, Map<String, Value>, Option<u64>)> = q.iter(world).map(|(k, p, n)| (k.0.clone(), p.map(|p| p.0.clone()).unwrap_or_default(), n.map(|n| n.0))).collect();
        for (kind, mut props, node) in items {
            if props.get("data-mix").and_then(|v| v.as_str()) == Some("true") {
                cx.mix = Some((num(&props, "data-lufs").unwrap_or(-14.0), num(&props, "data-ceiling").unwrap_or(-1.5)));
                continue;
            }
            if (kind == "audio" || kind == "video") && generated(&mut props, &mut cx) {
                push_sound(&mut cx, &props, &node.map(|n| format!("n{n}")).unwrap_or_default());
            }
        }
        if need_mix && f == 0 && cx.mix.is_none() {
            return Ok(None);
        }
        notes.push(raea_runtime_sound::tone_at(app.world_mut(), f, fps, dir, media_path, true));
        frames.push((f, Frame { screens: Vec::new(), mix: cx.mix, page: None, sounds: cx.sounds.clone(), scene: Vec::new(), clear: None, same: false }));
    }
    drop(app);
    let p = program(&frames, w, h, fps);
    let rows = raea_render::inserted_of(&p);
    let cues: Vec<raea_sys_mix::offline::Cue> = frames
        .iter()
        .zip(&notes)
        .map(|((_, f), n)| raea_sys_mix::offline::Cue {
            sounds: f.sounds.iter().map(|s| raea_sys_mix::Sound { key: s.key.as_str().into(), file: Path::new(&s.src).into(), at: s.at, volume: s.volume as f32, rate: s.rate, fade_in: s.fade_in.unwrap_or(0.0), fade_out: s.fade_out.unwrap_or(0.0), bus: s.bus.as_deref().map(Into::into) }).collect(),
            tone: n.clone(),
        })
        .collect();
    let mix = frames.iter().find_map(|(_, f)| f.mix);
    Ok(Some((cues, mix, p, rows, fps)))
}
