//! The frames as one raea-render program, and the sound lint over it.

use super::*;

/// The frames as raea-render's IR: pages as image rows, sound as audio rows, 3D as camera, light and mesh rows (docs:
/// TypeScript-only editing, phase 5: raea-render's IR is the contract; rrl the language, which this once went through, is
/// retired). A column that changes frame by frame is keyed (linear) and its row holds the first frame's value.
pub(crate) fn program(frames: &[(i64, Frame)], w: u32, h: u32, fps: f64) -> raea_core_program::ir::Program {
    let has3d = frames.iter().any(|(_, f)| !f.scene.is_empty());
    program_of(frames, w, h, fps, has3d)
}

/// program, its 3D's rows made only when `has3d`: false for a step that reads only its sound (the sound lint at a
/// live render's end). That render drew its 3D as the frames came; building the 3D's rows again from every
/// frame's kept scene asked for 28 GB at the end of a 13,175-frame film and got it killed (stage-guards SG4a).
pub(crate) fn program_of(frames: &[(i64, Frame)], w: u32, h: u32, fps: f64, has3d: bool) -> raea_core_program::ir::Program {
    use raea_core_program::ir::V;
    let mut p = raea_core_program::ir::Program { fps, width: w, height: h, frames: frames.len() as i64, ..Default::default() };
    let row = |id: &str, cols: Vec<(&str, V)>| -> BTreeMap<String, V> {
        let mut r: BTreeMap<String, V> = cols.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        r.insert("id".into(), V::S(id.into()));
        r
    };
    // --alpha: nothing under the pages, so what the composition leaves empty stays transparent
    // with no holder background: black under 2D pages, and for 3D no `bg`, so the camera clears to Bevy's ClearColor
    // resource (srgb 43, 44, 47 unless a <BevyResource type="bevy_camera::clear_color::ClearColor"> sets it), as a Bevy
    // scene does (docs/control conventions C17)
    let clear = if ALPHA.load(std::sync::atomic::Ordering::Relaxed) { Some("#00000000".into()) } else { frames.first().and_then(|(_, f)| f.clear.clone()).and_then(|c| hex_of(&c)).or_else(|| (!has3d).then(|| "#000000".into())) };
    let mut cam = row("cam", clear.map(|c| ("bg", V::S(c))).into_iter().collect());
    // a holder's background that changes over the frames: the clear keyed frame by frame (bg_r, bg_g, bg_b, bg_a, sRGB),
    // as the page over it changes. Before, the first frame's clear held for the whole render.
    if has3d && !ALPHA.load(std::sync::atomic::Ordering::Relaxed) {
        let mut last = None;
        let each: Vec<Option<[f64; 4]>> = frames.iter().map(|(_, f)| f.clear.as_deref().and_then(hex_of).and_then(|c| raea_runtime_scene::rgba_of(&c))).collect();
        if each.iter().flatten().any(|c| Some(c) != each[0].as_ref()) {
            let cols: Vec<[f64; 4]> = each.iter().map(|c| *last.insert((*c).or(last).unwrap_or([0.0, 0.0, 0.0, 1.0]))).collect();
            for (k, name) in ["bg_r", "bg_g", "bg_b", "bg_a"].iter().enumerate() {
                raea_runtime_scene::column(&mut p, &mut cam, "cam", name, &cols.iter().map(|c| c[k]).collect::<Vec<_>>());
            }
        }
    }
    // where it is and looks, frame by frame; its lens, ambient light and post-processing (raea-runtime-scene); a second
    // camera's row after it, drawn over it
    let more = if has3d { raea_runtime_scene::camera(&scene::all_items(frames), &mut p, &mut cam) } else { Vec::new() };
    p.spawns.push(("camera".into(), std::iter::once(cam).chain(more).collect()));
    // the pages: each distinct page shown for the frames in a row it's the same
    let mut runs: Vec<(String, usize, usize)> = Vec::new();
    for (i, (_, f)) in frames.iter().enumerate() {
        let Some(hash) = f.page.clone() else { continue };
        match runs.last_mut() {
            Some((hh, _, end)) if *hh == hash && *end == i => *end = i + 1,
            _ => runs.push((hash, i, i + 1)),
        }
    }
    for (k, (hash, a, b)) in runs.iter().enumerate() {
        p.spawns.push(("image".into(), vec![row(&format!("page{k}"), vec![("src", V::S(format!("asset:{hash}"))), ("x", V::N(0.0)), ("y", V::N(0.0)), ("w", V::N(w as f64)), ("h", V::N(h as f64)), ("from", V::N(*a as f64)), ("to", V::N(*b as f64)), ("z", V::N(10.0))])]));
    }
    // each <Screen>'s pictures on the bevy_ui page: a row per run of frames showing the same one, which raea-render's engine
    // draws into the screen's image (engine/render/src/engine/screen_page.rs)
    let mut shown: BTreeMap<String, Vec<(String, usize, usize)>> = BTreeMap::new();
    for (i, (_, f)) in frames.iter().enumerate() {
        for (name, hash) in &f.screens {
            let runs = shown.entry(name.clone()).or_default();
            match runs.last_mut() {
                Some((hh, _, end)) if hh == hash && *end == i => *end = i + 1,
                _ => runs.push((hash.clone(), i, i + 1)),
            }
        }
    }
    for (name, runs) in &shown {
        for (k, (hash, a, b)) in runs.iter().enumerate() {
            p.spawns.push(("screenpage".into(), vec![row(&format!("screenpage-{name}-{k}"), vec![("name", V::S(name.clone())), ("src", V::S(format!("asset:{hash}"))), ("from", V::N(*a as f64)), ("to", V::N(*b as f64))])]));
        }
    }
    // sound: per element, the stretches its source time runs on frame to frame
    let mut by: BTreeMap<String, Vec<(usize, Sound)>> = BTreeMap::new();
    for (i, (_, f)) in frames.iter().enumerate() {
        for s in &f.sounds {
            by.entry(format!("{}|{}", s.key, s.src)).or_default().push((i, s.clone()));
        }
    }
    let mut n = 0;
    let db = |v: f64| if v > 0.0 { 20.0 * v.log10() } else { -90.0 };
    let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
    let r6 = |x: f64| (x * 1e6).round() / 1e6;
    for (_, list) in by {
        let mut k = 0;
        while k < list.len() {
            let mut j = k;
            while j + 1 < list.len() && list[j + 1].0 == list[j].0 + 1 && (list[j + 1].1.at - list[j].1.at - list[j].1.rate / fps).abs() < 1e-6 {
                j += 1;
            }
            let (f0, s0) = (list[k].0, &list[k].1);
            let id = format!("sound{n}");
            let vols: Vec<f64> = list[k..=j].iter().map(|(_, s)| s.volume).collect();
            let mut cols = vec![("src", V::S(s0.src.clone())), ("from", V::N(f0 as f64)), ("to", V::N((list[j].0 + 1) as f64)), ("src_in", V::N(r6(s0.at)))];
            if vols.iter().all(|v| (*v - vols[0]).abs() < 1e-9) {
                cols.push(("gain", V::N(r3(db(vols[0])))));
            } else {
                cols.push(("gain", V::N(r3(db(vols[0])))));
                p.keys.insert((id.clone(), "gain".into()), list[k..=j].iter().map(|(f, s)| (*f as i64, r3(db(s.volume)), "linear".to_string())).collect());
                // the linear volume each frame, exact: the mixer core plays an SQL edit as its TSX plays (rounded dB isn't)
                p.keys.insert((id.clone(), "snd_vol".into()), list[k..=j].iter().map(|(f, s)| (*f as i64, s.volume, "step".to_string())).collect());
            }
            // what the mixer core needs to play these rows exactly as the frames played: the element's key, its exact
            // first volume, source time, rate and bus (snd_*: raea-render's lanes read src_in and gain; the core reads these)
            cols.push(("snd_key", V::S(s0.key.clone())));
            cols.push(("snd_vol", V::N(s0.volume)));
            cols.push(("snd_at", V::N(s0.at)));
            cols.push(("snd_rate", V::N(s0.rate)));
            if let Some(b) = &s0.bus {
                cols.push(("snd_bus", V::S(b.clone())));
            }
            if let Some(v) = s0.fade_in.filter(|v| *v > 0.0 && k == 0) {
                cols.push(("fade_in", V::N(r6(v))));
            }
            if let Some(v) = list[j].1.fade_out.filter(|v| *v > 0.0 && j + 1 == list.len()) {
                cols.push(("fade_out", V::N(r6(v))));
            }
            p.spawns.push(("audio".into(), vec![row(&id, cols)]));
            n += 1;
            k = j + 1;
        }
    }
    if let Some((lufs, ceiling)) = frames.iter().find_map(|(_, f)| f.mix) {
        p.spawns.push(("master".into(), vec![row("mix", vec![("lufs", V::N(lufs)), ("ceiling", V::N(ceiling))])]));
    }
    if has3d {
        // lights, meshes, models and pictures in space, matched across frames by identity (raea-runtime-scene)
        raea_runtime_scene::rows(&scene::all_items(frames), &mut p);
    }
    p
}

/// The program's sounds that run out of source before the stretch playing them ends: the sound stops dead there and its
/// fade-out (timed to the stretch's end) never plays. A render doesn't loop a source (`loop` is live play's only), so a
/// bed shorter than its Sequence is the usual one: the noir doc's 10 s projector under a 12.3 s cue cut off mid-line.
/// A take that's quiet by its last 0.1 s and asked for no fade-out (a voice line ending before its cue does) isn't one.
/// Said as WARN lines, one per stretch; the render goes on.
pub(crate) fn sound_lint(p: &raea_core_program::ir::Program, fps: f64) {
    use raea_core_program::ir::V;
    let mut len: HashMap<String, Option<f64>> = HashMap::new();
    for (kind, rows) in &p.spawns {
        if kind != "audio" {
            continue;
        }
        for r in rows {
            let n = |k: &str| match r.get(k) {
                Some(V::N(x)) => Some(*x),
                _ => None,
            };
            let (Some(V::S(src)), Some(from), Some(to)) = (r.get("src"), n("from"), n("to")) else { continue };
            let (rate, src_in) = (n("snd_rate").unwrap_or(1.0), n("src_in").unwrap_or(0.0));
            let Some(have) = *len.entry(src.clone()).or_insert_with(|| raea_io_media::probe_seconds(Path::new(src))) else { continue };
            // seconds of the stretch left when the source ends
            let short = (src_in + (to - from) / fps * rate - have) / rate.max(1e-6);
            // a take that has gone quiet by its end (a line said, its room after) ends as it would have; one still
            // sounding, or whose fade-out was asked for, is cut
            let fades = n("fade_out").is_some_and(|v| v > 0.0);
            let sounding = || {
                raea_io_media::decode_audio_window(Path::new(src), (have - 0.1).max(0.0), 0.1).is_ok_and(|a| {
                    let (sum, k) = a.channels.iter().flatten().fold((0.0f64, 0usize), |(s, k), x| (s + (*x as f64).powi(2), k + 1));
                    k > 0 && 10.0 * (sum / k as f64).log10() > -50.0
                })
            };
            if short > 1.5 / fps && (fades || sounding()) {
                let name = Path::new(src).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| src.clone());
                eprintln!("WARN  sound {name} runs out {short:.2} s before its stretch ends (at {:.2} s of {:.2}-{:.2} s): it stops dead there and its fade-out never plays. Make the source as long as the Sequence, or loop it: loop with endAt repeats startFrom to endAt (without endAt nothing loops)", to / fps - short, from / fps, to / fps);
            }
        }
    }
}

