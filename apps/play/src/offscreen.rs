use super::*;

/// No window: the same frames, composited on the GPU and waited for, paced at the composition's rate (or as fast as they
/// come with --no-vsync), and the same report. What a playtest measures when nobody's screen should be taken.
pub(crate) fn offscreen(mut run: Run, json: Option<String>, flat_out: bool, snaps: Vec<(f64, String)>) -> R<String> {
    let (w, h) = run.size;
    // RAEA_PLAY_LAYER=WxH: present to a CAMetalLayer of that size in no window, as the legacy app's stage does
    let layer = std::env::var("RAEA_PLAY_LAYER").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))));
    // a snapshot on a layer is what the layer was given (the page over the 3D, composited as play composites it):
    // surfaces made from now can be read back
    if layer.is_some() && !snaps.is_empty() {
        raea_gpu_device::present::read_next_present();
    }
    raea_runtime_react::startup_mark("offscreen play");
    #[cfg(target_vendor = "apple")]
    let mut stage = match layer {
        Some((lw, lh)) => raea_gpu_device::present::layer(lw, lh)?,
        None => raea_gpu_device::present::offscreen(w, h)?,
    };
    #[cfg(not(target_vendor = "apple"))]
    let mut stage = { let _ = layer; raea_gpu_device::present::offscreen(w, h)? };
    // --baked: a timeline played from its bake (React stepped through every frame once, then no JS while playing)
    raea_runtime_react::startup_mark("stage made");
    let t_open = Instant::now();
    let mut live = if std::env::args().any(|a| a == "--baked") { Live::open_baked(&run.tsx)? } else { Live::open(&run.tsx, (w, h))? };
    let open_ms = t_open.elapsed().as_secs_f64() * 1e3;
    raea_runtime_react::startup_mark("opened");
    // --master: a timeline's sound mastered as its render's (what the legacy app's stage does for an edit)
    if std::env::args().any(|a| a == "--master") {
        live.master_sound();
    } else if std::env::var("RAEA_GAME_MASTER").as_deref() != Ok("0") {
        // its voices mastered as its render masters its sound: a game live (void's stage does the same), and a baked
        // timeline too, so its play is its render's bounce
        live.game_master();
    }
    // --3d: a 3D scene drawn under the page as its render draws it (what the legacy app's stage does for scene3d and motion)
    if std::env::args().any(|a| a == "--3d") {
        live.scene_3d();
        // snapshots are checks: the 3D's rows are waited for before the clock starts, so every snapshot has its 3D
        if !snaps.is_empty() {
            live.wait_3d(120.0)?;
        }
    }
    // --scrub N: N seeks to frames spread over the timeline in a shuffled order, each stepped and presented: what a
    // scrub costs (the decoder's restart included); the report is that alone
    // --seek F OUT.png (repeatable): straight to frame F (a baked timeline's jump: its keyframe, then the frames after),
    // the page snapshotted, then the next; the report is that alone. render equals play holds for a seek too
    // (bench/timelines/check.sh compares these with renders)
    let seeks: Vec<(i64, String)> = std::env::args().collect::<Vec<_>>().windows(3).filter(|w| w[0] == "--seek").filter_map(|w| Some((w[1].parse().ok()?, w[2].clone()))).collect();
    if !seeks.is_empty() {
        // --view YAW,PITCH,DISTANCE[,X,Y,Z]: the 3D drawn from that orbit (radians, metres, about X,Y,Z) instead of
        // the composition's camera; --pick X,Y (repeatable): what's under that point of each seeked frame (page px),
        // the page and its 3D as one picture, a line of JSON each (docs/preview.feature PV1)
        let args: Vec<String> = std::env::args().collect();
        let nums = |flag: &str| -> Vec<Vec<f64>> { args.windows(2).filter(|w| w[0] == flag).map(|w| w[1].split(',').filter_map(|v| v.trim().parse().ok()).collect()).collect() };
        let view = nums("--view").into_iter().find(|v| v.len() == 3 || v.len() == 6);
        let picks: Vec<(f64, f64)> = nums("--pick").into_iter().filter(|p| p.len() == 2).map(|p| (p[0], p[1])).collect();
        let mut said = Vec::new();
        for (f, out) in &seeks {
            live.step(*f, &[], None)?;
            if let Some(v) = &view {
                // the scene's engine opens at the first draw
                live.snapshot()?;
                let t = if v.len() == 6 { [v[3] as f32, v[4] as f32, v[5] as f32] } else { [0.0; 3] };
                if !live.set_view(Some((v[0] as f32, v[1] as f32, v[2] as f32, t))) {
                    return Err("--view: this composition has no 3D to look around".into());
                }
            }
            live.save_snapshot(std::path::Path::new(out))?;
            eprintln!("seek to {f} -> {out}");
            for (x, y) in &picks {
                said.push(serde_json::json!({"frame": f, "x": x, "y": y, "pick": live.pick(*x, *y)}).to_string());
            }
        }
        if !said.is_empty() {
            return Ok(said.join("\n"));
        }
        return Ok(format!("SEEKED  {} frames", seeks.len()));
    }
    if let Some(n) = std::env::args().skip_while(|a| a != "--scrub").nth(1).and_then(|v| v.parse::<usize>().ok()) {
        let total = live.frames.max(1);
        let mut order: Vec<i64> = (0..n as i64).map(|k| k * total / n.max(1) as i64).collect();
        // a fixed shuffle (no rand crate): every seek jumps somewhere far from the last
        let mut x: u64 = 0x9e3779b97f4a7c15;
        for i in (1..order.len()).rev() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            order.swap(i, (x % (i as u64 + 1)) as usize);
        }
        // the stage shows its first frame when a piece opens (as the legacy app's does): that one, which opens the decoders and
        // the hardware behind them, is reported apart from the seeks
        let t0 = Instant::now();
        live.step(0, &run.keys, run.pointer)?;
        live.present(&mut stage, true)?;
        let first_ms = t0.elapsed().as_secs_f64() * 1e3;
        let mut ms = Vec::new();
        // each seek's two parts: the step (the bake's jump, the videos decoded to the frame) and the present (painted,
        // composited and waited for on the GPU)
        let (mut steps, mut presents) = (Vec::new(), Vec::new());
        for f in order {
            let t0 = Instant::now();
            live.step(f, &run.keys, run.pointer)?;
            let t1 = Instant::now();
            live.present(&mut stage, true)?;
            steps.push((t1 - t0).as_secs_f64() * 1e3);
            presents.push(t1.elapsed().as_secs_f64() * 1e3);
            ms.push((f, t0.elapsed().as_secs_f64() * 1e3));
        }
        let part = |mut v: Vec<f64>| {
            v.sort_by(f64::total_cmp);
            let at = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
            serde_json::json!({"p50": at(0.5), "p99": at(0.99)})
        };
        let (step_ms, present_ms) = (part(steps), part(presents));
        let mut v: Vec<f64> = ms.iter().map(|m| m.1).collect();
        v.sort_by(f64::total_cmp);
        let pct = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
        let worst: Vec<_> = { let mut w = ms.clone(); w.sort_by(|a, b| b.1.total_cmp(&a.1)); w.into_iter().take(5).map(|(f, t)| serde_json::json!({"frame": f, "ms": t})).collect() };
        return Ok(format!("SCRUBBED  {}", serde_json::to_string_pretty(&serde_json::json!({"seeks": v.len(), "first_frame_ms": first_ms, "scrub_ms": {"p50": pct(0.5), "p99": pct(0.99), "max": pct(1.0)}, "step_ms": step_ms, "present_ms": present_ms, "slowest": worst, "rss_mb": rss_kb() / 1024})).unwrap()));
    }
    let fps = live.fps;
    // --seconds end: to the last frame, as the composition says its length (a gate's guess from the source missed a
    // computed durationInFrames, and played a baked edit 1.3 s past its end)
    let to_end = std::env::args().skip_while(|a| a != "--seconds").nth(1).as_deref() == Some("end");
    let seconds = if to_end { live.frames as f64 / fps.max(1.0) } else { run.seconds.unwrap_or(10.0) };
    // the per-frame lists sized for the whole run and their pages touched now: grown as it went, they were the
    // harness's own memory growing in the RSS it judges (36,000 frames x 7 lists x 8 bytes in a 5-minute run at 120 fps)
    let cap = (seconds * fps.max(1.0) * 1.05) as usize + 64;
    for v in [&mut run.work, &mut run.gaps, &mut run.steps, &mut run.presents, &mut run.js, &mut run.apply, &mut run.bytes] {
        v.resize(cap, 0.0);
        v.clear();
    }
    // the one mixer core: the device open before the clock starts (live.rs open_mixer), and the master if it's in within
    // 80 ms (a cached one is); a cold one never holds the first frame, it's swapped in on the frame it lands.
    // RAEA_PLAY_WAIT_MASTER=SECS waits up to SECS for it: the checks that hold a play's tap against its render's bounce
    // from frame 0 (sdk/react/bench/games/audio.sh, sdk/react/bench/tone/check.sh)
    // frame 0 shown now, while the sound is readied below (its picture doesn't wait on the mixer: the master, the
    // device and the core's clock took ~190 ms before it, lightspeed F5); the clock still starts once the sound is in.
    // RAEA_PLAY_EARLY_FRAME=0 presents it with the clock, as before
    if std::env::var("RAEA_PLAY_EARLY_FRAME").as_deref() != Ok("0") {
        live.present(&mut stage, true)?;
        first_frame();
    }
    live.wait_master(std::env::var("RAEA_PLAY_WAIT_MASTER").ok().and_then(|v| v.parse().ok()).unwrap_or(0.08));
    raea_runtime_react::startup_mark("master in");
    live.open_mixer();
    raea_runtime_react::startup_mark("mixer open");
    live.settle_mixer();
    let start = Instant::now();
    run.start = Some(start);
    let mut f: i64 = -1;
    // --input: a thread presses and releases the arrow keys at random moments (50-250 ms apart), as a player's hand
    // would, stamping each change; the time from a change to the end of the present that first carries it is the
    // input-to-photon latency short of the display's own scan-out
    let input: Option<std::sync::Arc<std::sync::Mutex<(Vec<String>, Instant, u64)>>> = std::env::args().any(|a| a == "--input").then(|| {
        let sh = std::sync::Arc::new(std::sync::Mutex::new((Vec::new(), Instant::now(), 0u64)));
        let s2 = sh.clone();
        let until = start + Duration::from_secs_f64(seconds);
        // the key sets it cycles through (RAEA_INPUT_KEYS: sets split by commas, keys in a set by "+", an empty set
        // for none; a platformer runs and jumps: "ArrowRight,ArrowRight+ArrowUp,"); pong's up, down and none otherwise
        let sets: Vec<Vec<String>> = std::env::var("RAEA_INPUT_KEYS")
            .map(|v| v.split(',').map(|set| set.split('+').filter(|k| !k.is_empty()).map(str::to_string).collect()).collect())
            .unwrap_or_else(|_| vec![vec![], vec!["ArrowUp".to_string()], vec!["ArrowDown".to_string()]]);
        std::thread::spawn(move || {
            let mut x: u64 = 0x2545f4914f6cdd1d;
            let mut k = 0u64;
            while Instant::now() < until {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                std::thread::sleep(Duration::from_millis(50 + x % 200));
                k += 1;
                let keys = sets[(k % sets.len() as u64) as usize].clone();
                *s2.lock().unwrap() = (keys, Instant::now(), k);
            }
        });
        sh
    });
    let (mut seen, mut waiting, mut latency) = (0u64, None::<Instant>, Vec::<f64>::new());
    // --input-log FILE: each frame's input as played, written as the input timeline `render --input` takes, so a render
    // of the same frames can be checked against the play (pixels from frame and input alone)
    let input_log = std::env::args().skip_while(|a| a != "--input-log").nth(1);
    let mut logged = serde_json::Map::new();
    // --input-script FILE: {"FRAME": [keys], ...}, each set held from its frame until the next; marked keys
    // ("Mouse:Right", "Wheel:Y 100", "Pad0:South", "Touch:0:16:16") are the live wire a host's other input takes
    // (games.ts wireKeys), so a play probes it as a window's events would deliver it (sdk/raea/bench/bevy-ts);
    // "Pointer:X:Y" (":down" pressed) is the pointer
    let script: Option<std::collections::BTreeMap<i64, Vec<String>>> = match std::env::args().skip_while(|a| a != "--input-script").nth(1) {
        Some(p) => Some(serde_json::from_str(&std::fs::read_to_string(&p).map_err(|e| format!("{p}: {e}"))?).map_err(|e| format!("{p}: {{\"FRAME\": [keys]}}: {e}"))?),
        None => None,
    };
    // a snapshot's frame is played even when the seconds pass first (under load fewer frames fit in them: a 0.6 s
    // snapshot in a 0.9 s play was never taken at load 76, 10-27); a frame that doesn't come is the step's to refuse
    let snapping = |f: i64| snaps.iter().any(|(at, _)| (*at * fps).round() as i64 > f);
    while start.elapsed().as_secs_f64() < seconds || snapping(f) {
        let t0 = Instant::now();
        let elapsed = start.elapsed().as_secs_f64();
        if run.autoplay {
            run.keys = vec![if (elapsed * 1.5) as i64 % 2 == 0 { "ArrowLeft".into() } else { "ArrowRight".into() }];
        }
        if let Some(k) = script.as_ref().and_then(|s| s.range(..=f + 1).next_back()) {
            // "Pointer:X:Y" (":down" while pressed) puts the pointer at X, Y in frame px; it stays there until another
            run.keys = Vec::new();
            for key in k.1 {
                match key.strip_prefix("Pointer:").map(|p| p.split(':').collect::<Vec<_>>()) {
                    Some(p) if p.len() >= 2 => {
                        let n = |s: &str| s.parse::<f64>().map_err(|e| format!("--input-script: {key}: {e}"));
                        run.pointer = Some((n(p[0])?, n(p[1])?, p.get(2) == Some(&"down")));
                    }
                    _ => run.keys.push(key.clone()),
                }
            }
        }
        // --input: the keys as the input thread last set them; a change not yet shown is timed to its present
        if let Some(sh) = &input {
            let (keys, at, seq) = sh.lock().unwrap().clone();
            run.keys = keys;
            if seq != seen {
                seen = seq;
                waiting = Some(at);
            }
        }
        f += 1;
        if f == fps.max(1.0) as i64 {
            raea_core_alloc::restart_sites();
        }
        let ld = |a: &std::sync::atomic::AtomicU64| a.load(std::sync::atomic::Ordering::Relaxed);
        let (j0, a0, b0) = (ld(&raea_runtime_react::JS_NS), ld(&raea_runtime_react::APPLY_NS), ld(&raea_runtime_react::FLUSH_BYTES));
        let (n0, nb0, fl0) = (ld(&raea_runtime_react::APPLY_ALLOCS), ld(&raea_runtime_react::APPLY_ALLOC_BYTES), ld(&raea_runtime_react::FLUSH_ALLOCS));
        let all0 = raea_core_alloc::all();
        let lag = start.elapsed().as_secs_f64() - f as f64 / fps;
        live.step_at(f, &run.keys, run.pointer, lag)?;
        if input_log.is_some() {
            // marked live as the game saw it (isLive), so a render of the log plays the same match, not its script
            let mut e = serde_json::json!({"keys": live.played_keys(&run.keys), "live": true});
            if let Some((x, y, down)) = run.pointer {
                e["pointer"] = serde_json::json!({"x": x, "y": y, "down": down});
            }
            // what reached its app screens (the keys they took aren't the page's above), given them again in a render
            if let Some(apps) = live.app_input() {
                e["apps"] = apps;
            }
            // the session's seed and start with its first frame: a render of the log draws the same Math.random and Date
            if logged.is_empty() {
                e["session"] = live.session();
            }
            logged.insert(f.to_string(), e);
        }
        run.js.push((ld(&raea_runtime_react::JS_NS) - j0) as f64 / 1e6);
        run.apply.push((ld(&raea_runtime_react::APPLY_NS) - a0) as f64 / 1e6);
        run.bytes.push((ld(&raea_runtime_react::FLUSH_BYTES) - b0) as f64);
        run.allocs.push(((ld(&raea_runtime_react::APPLY_ALLOCS) - n0) as f64, (ld(&raea_runtime_react::APPLY_ALLOC_BYTES) - nb0) as f64, (ld(&raea_runtime_react::FLUSH_ALLOCS) - fl0) as f64));
        run.stepped += 1;
        let t1 = Instant::now();
        if layer.is_some() && snaps.iter().any(|(at, _)| (*at * fps).round() as i64 == f) {
            raea_gpu_device::present::read_next_present();
        }
        live.present(&mut stage, true)?;
        let presented = if layer.is_some() { raea_gpu_device::present::take_present() } else { None };
        for (at, out) in snaps.iter().filter(|(at, _)| (*at * fps).round() as i64 == f) {
            let (w, h, px) = match presented.clone() {
                Some(p) => p,
                None => live.snapshot()?,
            };
            image::save_buffer(out, &px, w, h, image::ColorType::Rgba8).map_err(|e| format!("{out}: {e}"))?;
            eprintln!("snapshot at {at} s -> {out}");
        }
        let (step_ms, present_ms) = ((t1 - t0).as_secs_f64() * 1e3, t1.elapsed().as_secs_f64() * 1e3);
        run.steps.push(step_ms);
        run.presents.push(present_ms);
        let now = Instant::now();
        let work = (now - t0).as_secs_f64() * 1e3;
        run.work.push(work);
        let all1 = raea_core_alloc::all();
        run.allocs_all.push(((all1.0 - all0.0) as f64, (all1.1 - all0.1) as f64));
        first_frame();
        let gap = run.last_present.map(|l| (now - l).as_secs_f64() * 1e3);
        if let Some(g) = gap {
            run.gaps.push(g);
        }
        let budget = 1000.0 / fps.max(1.0);
        raea_io_trace::frame(work, gap.is_some_and(|g| g > budget * 1.5), budget, || serde_json::json!({"frame": f, "step_ms": step_ms, "present_ms": present_ms, "gap_ms": gap}));
        run.last_present = Some(now);
        if let Some(at) = waiting.take() {
            latency.push((now - at).as_secs_f64() * 1e3);
        }
        if run.last_rss.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            let (rss, reusable) = vm_kb();
            run.rss.push(rss);
            run.reusable.push(reusable);
            run.footprint.push(raea_io_trace::footprint_mb());
            run.last_rss = Some(Instant::now());
        }
        if !flat_out {
            let next = start + Duration::from_secs_f64((f + 1) as f64 / fps);
            if let Some(d) = next.checked_duration_since(Instant::now()) {
                std::thread::sleep(d);
            }
        }
    }
    if let Some(path) = &input_log {
        std::fs::write(path, serde_json::Value::Object(logged).to_string()).map_err(|e| format!("{path}: {e}"))?;
    }
    let (pw, ph) = (live.w, live.h);
    let drift: Vec<f64> = live.drift_ms().to_vec();
    run.live = Some(live);
    let mut report = run.report();
    if !drift.is_empty() {
        let mut a: Vec<f64> = drift.iter().map(|d| d.abs()).collect();
        a.sort_by(|x, y| x.total_cmp(y));
        report["audio_drift_ms"] = serde_json::json!({"p50": a[a.len() / 2], "p99": a[(a.len() - 1) * 99 / 100], "max": a[a.len() - 1], "samples": a.len()});
    }
    report["offscreen"] = serde_json::json!(true);
    report["open_ms"] = serde_json::json!(open_ms);
    report["release_ms_total"] = serde_json::json!(raea_runtime_react::RELEASE_NS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6);
    report["eden_ms_total"] = serde_json::json!(raea_runtime_react::EDEN_NS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6);
    if let Some((gain, ceiling)) = run.live.as_ref().and_then(|l| l.game_gain()) {
        report["game_master"] = serde_json::json!({"gain_db": gain, "ceiling_dbtp": ceiling});
    }
    if !latency.is_empty() {
        let mut v = latency.clone();
        v.sort_by(f64::total_cmp);
        let pct = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
        let frame = 1000.0 / fps;
        report["input_to_present_ms"] = serde_json::json!({"changes": v.len(), "p50": pct(0.5), "p99": pct(0.99), "max": pct(1.0), "p99_frames": pct(0.99) / frame});
    }
    report["page"] = serde_json::json!([pw, ph]);
    // a game that said lines (raea/speech): each line's first audio and real-time factor, and how long frames
    // waited on the speech worker
    #[cfg(feature = "ml")]
    if !raea_sys_speech::speech::service().lines().is_empty() {
        report["speech"] = raea_runtime_react::native::ml::stats();
    }
    if let Some(out) = json {
        std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).map_err(|e| format!("{out}: {e}"))?;
    }
    Ok(format!("PLAYED  {}", serde_json::to_string_pretty(&report).unwrap()))
}
