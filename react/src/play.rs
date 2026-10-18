//! `raea-motion play EDIT.tsx`: the live stage in a window of its own, and the playtest harness. The composition runs
//! at its rate on the live engine (live.rs), keys and pointer as its input, presented every display refresh on the GPU.
//! When it ends (the window closes, or --seconds pass) it reports what it measured: frames stepped and presented, frame
//! times (p50, p99, worst), frames missed against the display, and resident memory over the run.
//!
//!   raea-motion play EDIT.tsx [--size WxH] [--seconds S|end] [--autoplay] [--no-vsync] [--json OUT]
//!
//! --seconds end plays to the composition's last frame (its frames and fps as it opened, however its length is written).
//!
//! --autoplay holds ArrowLeft and ArrowRight in turn (a run nobody plays); --no-vsync presents as fast as frames come,
//! to measure what a frame costs past the display's rate; --offscreen runs with no window (nothing shown, the same
//! work), for runs that shouldn't take anyone's screen.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::live::Live;

/// Live play on Bevy's runner (RAEA_BEVY_RUNNER=1 or --bevy-runner): the offscreen loop's frames as systems.
#[path = "runner.rs"]
mod runner;

type R<T> = Result<T, String>;

struct Run {
    tsx: PathBuf,
    size: (u32, u32),
    seconds: Option<f64>,
    autoplay: bool,
    vsync: bool,
    window: Option<Arc<Window>>,
    stage: Option<raea::quads::Stage>,
    live: Option<Live>,
    keys: Vec<String>,
    pointer: Option<(f64, f64, bool)>,
    start: Option<Instant>,
    frame: i64,
    last_present: Option<Instant>,
    /// per presented frame: the work (step + present) and the gap since the last present, in ms
    work: Vec<f64>,
    gaps: Vec<f64>,
    /// per frame: React's step (event to commit) and the present, in ms
    steps: Vec<f64>,
    presents: Vec<f64>,
    /// per frame: the JS thread's time, Bevy's apply, and the commit JSON, from raea-react's counters
    js: Vec<f64>,
    apply: Vec<f64>,
    bytes: Vec<f64>,
    stepped: u64,
    rss: Vec<u64>,
    /// physical footprint each second (MB): what the process really holds (resident size counts reusable clean pages)
    footprint: Vec<f64>,
    /// of the resident size each second, the reusable pages (KB): freed, there until the kernel takes them
    reusable: Vec<u64>,
    last_rss: Option<Instant>,
    error: Option<String>,
}

fn rss_kb() -> u64 {
    vm_kb().0
}

/// This process's resident size and, of it, the reusable pages, in KB (task_info's TASK_VM_INFO, what `ps` reads; no
/// process spawned each second). Reusable pages are ones an allocator freed with MADV_FREE_REUSABLE, as JavaScriptCore's
/// libpas does when it returns its free pages: they stay resident, so counted in RSS, until the kernel wants them, and
/// are in no footprint. A live game's RSS crept 2-4 MB in 4 minutes with all of it there (vmmap: "WebKit Malloc").
fn vm_kb() -> (u64, u64) {
    #[cfg(target_os = "macos")]
    unsafe {
        unsafe extern "C" {
            static mach_task_self_: u32;
            fn task_info(task: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
        }
        // task_vm_info through phys_footprint: resident_size is [2], reusable [10]
        let (mut info, mut count) = ([0u64; 19], 38u32);
        if task_info(mach_task_self_, 22, info.as_mut_ptr() as *mut i32, &mut count) == 0 {
            return (info[2] / 1024, info[10] / 1024);
        }
    }
    (0, 0)
}

fn key_name(k: &Key) -> Option<String> {
    Some(match k {
        Key::Named(NamedKey::ArrowLeft) => "ArrowLeft".into(),
        Key::Named(NamedKey::ArrowRight) => "ArrowRight".into(),
        Key::Named(NamedKey::ArrowUp) => "ArrowUp".into(),
        Key::Named(NamedKey::ArrowDown) => "ArrowDown".into(),
        Key::Named(NamedKey::Space) => " ".into(),
        Key::Named(NamedKey::Enter) => "Enter".into(),
        Key::Named(NamedKey::Escape) => "Escape".into(),
        Key::Named(NamedKey::Shift) => "Shift".into(),
        Key::Character(c) => c.to_string(),
        _ => return None,
    })
}

impl Run {
    fn frame(&mut self, el: &ActiveEventLoop) -> R<()> {
        let (Some(live), Some(stage)) = (self.live.as_mut(), self.stage.as_ref()) else { return Ok(()) };
        let start = *self.start.get_or_insert_with(Instant::now);
        let t0 = Instant::now();
        let elapsed = start.elapsed().as_secs_f64();
        if self.seconds.is_some_and(|s| elapsed >= s) {
            el.exit();
            return Ok(());
        }
        if self.autoplay {
            self.keys = vec![if (elapsed * 1.5) as i64 % 2 == 0 { "ArrowLeft".into() } else { "ArrowRight".into() }];
        }
        // the composition's clock: every frame due by now, each with this moment's input (a few at most when behind)
        let due = (elapsed * live.fps).floor() as i64;
        let mut n = 0;
        while self.frame < due && n < 4 {
            self.frame += 1;
            live.step(self.frame, &self.keys, self.pointer)?;
            self.stepped += 1;
            n += 1;
        }
        if self.frame < due {
            self.frame = due;
        }
        live.present(stage, true)?;
        let now = Instant::now();
        let work = (now - t0).as_secs_f64() * 1e3;
        self.work.push(work);
        let gap = self.last_present.map(|l| (now - l).as_secs_f64() * 1e3);
        if let Some(g) = gap {
            self.gaps.push(g);
        }
        let budget = 1000.0 / live.fps.max(1.0);
        let frame = self.frame;
        raea_native::trace::frame(work, gap.is_some_and(|g| g > budget * 1.5), budget, || serde_json::json!({"frame": frame, "gap_ms": gap, "stepped": n}));
        self.last_present = Some(now);
        if self.last_rss.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            let (rss, reusable) = vm_kb();
            self.rss.push(rss);
            self.reusable.push(reusable);
            self.footprint.push(raea_native::trace::footprint_mb());
            self.last_rss = Some(Instant::now());
        }
        Ok(())
    }

    fn report(&self) -> serde_json::Value {
        let pct = |v: &[f64], p: f64| {
            if v.is_empty() {
                return 0.0;
            }
            let mut s = v.to_vec();
            s.sort_by(|a, b| a.total_cmp(b));
            s[((s.len() - 1) as f64 * p).round() as usize]
        };
        let fps = self.live.as_ref().map(|l| l.fps).unwrap_or(0.0);
        let refresh = self.window.as_ref().and_then(|w| w.current_monitor()).and_then(|m| m.refresh_rate_millihertz()).map(|m| m as f64 / 1000.0).unwrap_or(60.0);
        // a frame is due every display refresh, or every composition frame when the composition is slower (a 30 fps edit
        // on a 60 Hz display presents every other refresh by design)
        let budget = 1000.0 / if fps > 0.0 { refresh.min(fps) } else { refresh };
        let missed = self.gaps.iter().filter(|g| **g > budget * 1.5).count();
        let secs = self.start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
        let mut order: Vec<usize> = (0..self.work.len()).collect();
        order.sort_by(|a, b| self.work[*b].total_cmp(&self.work[*a]));
        let slowest: Vec<serde_json::Value> = order.iter().take(8).map(|&i| serde_json::json!({"frame": i, "at_s": (i as f64 / fps.max(1.0) * 100.0).round() / 100.0, "work": self.work[i], "step": self.steps.get(i), "present": self.presents.get(i)})).collect();
        serde_json::json!({
            "seconds": (secs * 10.0).round() / 10.0,
            "composition_fps": fps,
            "display_hz": refresh,
            "page": self.live.as_ref().map(|l| [l.w, l.h]),
            "stepped": self.stepped,
            "presented": self.work.len(),
            "presented_fps": if secs > 0.0 { (self.work.len() as f64 / secs * 10.0).round() / 10.0 } else { 0.0 },
            "work_ms": {"p50": pct(&self.work, 0.5), "p99": pct(&self.work, 0.99), "max": pct(&self.work, 1.0)},
            "step_ms": {"p50": pct(&self.steps, 0.5), "p99": pct(&self.steps, 0.99)},
            "present_ms": {"p50": pct(&self.presents, 0.5), "p99": pct(&self.presents, 0.99)},
            "js_ms": {"p50": pct(&self.js, 0.5), "p99": pct(&self.js, 0.99)},
            "apply_ms": {"p50": pct(&self.apply, 0.5), "p99": pct(&self.apply, 0.99)},
            "commit_kb": {"p50": pct(&self.bytes, 0.5) / 1024.0, "p99": pct(&self.bytes, 0.99) / 1024.0},
            "gap_ms": {"p50": pct(&self.gaps, 0.5), "p99": pct(&self.gaps, 0.99), "max": pct(&self.gaps, 1.0)},
            "missed_vs_display": missed,
            "slowest": slowest,
            "rss_mb": {"first": self.rss.first().map(|k| k / 1024), "last": self.rss.last().map(|k| k / 1024), "max": self.rss.iter().max().map(|k| k / 1024), "each_second": self.rss.iter().map(|k| k / 1024).collect::<Vec<_>>()},
            "reusable_mb": {"each_second": self.reusable.iter().map(|k| (*k as f64 / 102.4).round() / 10.0).collect::<Vec<_>>()},
            "footprint_mb": {"each_second": self.footprint.iter().map(|m| (m * 10.0).round() / 10.0).collect::<Vec<_>>()},
            "error": self.error,
        })
    }
}

impl ApplicationHandler for Run {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (w, h) = self.size;
        let attrs = Window::default_attributes().with_title("raea play").with_inner_size(winit::dpi::PhysicalSize::new(w, h));
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.error = Some(format!("window: {e}"));
                el.exit();
                return;
            }
        };
        let px = window.inner_size();
        let made = raea::quads::stage(window.clone(), px.width, px.height, self.vsync).and_then(|stage| Live::open(&self.tsx, (px.width, px.height)).map(|live| (stage, live)));
        match made {
            Ok((stage, live)) => {
                self.stage = Some(stage);
                self.live = Some(live);
            }
            Err(e) => {
                self.error = Some(e);
                el.exit();
                return;
            }
        }
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(px) => {
                if let Some(s) = self.stage.as_mut() {
                    let _ = s.resize(px.width, px.height);
                }
                if let Some(l) = self.live.as_mut() {
                    l.resize(px.width, px.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(k) = key_name(&event.logical_key) {
                    match event.state {
                        ElementState::Pressed if !self.keys.contains(&k) => self.keys.push(k),
                        ElementState::Released => self.keys.retain(|x| *x != k),
                        _ => {}
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (Some(w), Some(l)) = (self.window.as_ref(), self.live.as_ref()) {
                    let s = w.inner_size();
                    let k = (s.width as f64 / l.w as f64).min(s.height as f64 / l.h as f64);
                    let (ox, oy) = ((s.width as f64 - l.w as f64 * k) / 2.0, (s.height as f64 - l.h as f64 * k) / 2.0);
                    let down = self.pointer.is_some_and(|p| p.2);
                    self.pointer = Some(((position.x - ox) / k, (position.y - oy) / k, down));
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                if let Some(p) = self.pointer.as_mut() {
                    p.2 = state == ElementState::Pressed;
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.frame(el) {
                    self.error = Some(e);
                    el.exit();
                }
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

pub fn play(args: &[String]) -> R<String> {
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let tsx = PathBuf::from(args.get(1).ok_or("raea-motion play EDIT.tsx [--size WxH] [--seconds S] [--autoplay] [--no-vsync] [--json OUT]")?);
    let size = flag("--size").and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))).unwrap_or((1080, 1920));
    let mut run = Run {
        tsx,
        size,
        seconds: flag("--seconds").and_then(|s| s.parse().ok()),
        autoplay: args.iter().any(|a| a == "--autoplay"),
        vsync: !args.iter().any(|a| a == "--no-vsync"),
        window: None,
        stage: None,
        live: None,
        keys: Vec::new(),
        pointer: None,
        start: None,
        frame: -1,
        last_present: None,
        work: Vec::new(),
        gaps: Vec::new(),
        steps: Vec::new(),
        presents: Vec::new(),
        js: Vec::new(),
        apply: Vec::new(),
        bytes: Vec::new(),
        stepped: 0,
        rss: Vec::new(),
        footprint: Vec::new(),
        reusable: Vec::new(),
        last_rss: None,
        error: None,
    };
    if args.iter().any(|a| a == "--offscreen") {
        // --snapshot S OUT.png (repeatable): the page read back at S seconds
        let snaps: Vec<(f64, String)> = args.windows(3).filter(|w| w[0] == "--snapshot").filter_map(|w| Some((w[1].parse().ok()?, w[2].clone()))).collect();
        return offscreen(run, flag("--json"), args.iter().any(|a| a == "--no-vsync"), snaps);
    }
    let el = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    el.set_control_flow(ControlFlow::Poll);
    el.run_app(&mut run).map_err(|e| format!("event loop: {e}"))?;
    let report = run.report();
    if let Some(out) = flag("--json") {
        std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).map_err(|e| format!("{out}: {e}"))?;
    }
    if let Some(e) = &run.error {
        return Err(format!("{e}\n{}", serde_json::to_string_pretty(&report).unwrap()));
    }
    Ok(format!("PLAYED  {}", serde_json::to_string_pretty(&report).unwrap()))
}

/// The texture id the harness's chrome page draws the stage by (RAEA_PLAY_CHROME).
const STAGE_TEXTURE: u64 = u64::MAX - 7;

/// No window: the same frames, composited on the GPU and waited for, paced at the composition's rate (or as fast as they
/// come with --no-vsync), and the same report. What a playtest measures when nobody's screen should be taken.
fn offscreen(mut run: Run, json: Option<String>, flat_out: bool, snaps: Vec<(f64, String)>) -> R<String> {
    let (w, h) = run.size;
    // RAEA_PLAY_LAYER=WxH: present to a CAMetalLayer of that size in no window, as void's stage does
    let layer = std::env::var("RAEA_PLAY_LAYER").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))));
    // a snapshot on a layer is what the layer was given (the page over the 3D, composited as play composites it):
    // surfaces made from now can be read back
    if layer.is_some() && !snaps.is_empty() {
        raea::quads::read_next_present();
    }
    #[cfg(target_vendor = "apple")]
    let mut stage = match layer {
        Some((lw, lh)) => raea::quads::stage_layer(lw, lh)?,
        None => raea::quads::stage_offscreen(w, h)?,
    };
    #[cfg(not(target_vendor = "apple"))]
    let mut stage = { let _ = layer; raea::quads::stage_offscreen(w, h)? };
    // --baked: a timeline played from its bake (React stepped through every frame once, then no JS while playing)
    let t_open = Instant::now();
    let mut live = if std::env::args().any(|a| a == "--baked") { Live::open_baked(&run.tsx)? } else { Live::open(&run.tsx, (w, h))? };
    let open_ms = t_open.elapsed().as_secs_f64() * 1e3;
    // --master: a timeline's sound mastered as its render's (what void's stage does for an edit)
    if std::env::args().any(|a| a == "--master") {
        live.master_sound();
    } else if !std::env::args().any(|a| a == "--baked") && std::env::var("RAEA_GAME_MASTER").as_deref() != Ok("0") {
        // played live (a game): its voices mastered as its render masters its sound (void's stage does the same)
        live.game_master();
    }
    // --3d: a 3D scene drawn under the page as its render draws it (what void's stage does for scene3d and motion)
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
        for (f, out) in &seeks {
            live.step(*f, &[], None)?;
            live.save_snapshot(std::path::Path::new(out))?;
            eprintln!("seek to {f} -> {out}");
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
        // the stage shows its first frame when a piece opens (as void's does): that one, which opens the decoders and
        // the hardware behind them, is reported apart from the seeks
        let t0 = Instant::now();
        live.step(0, &run.keys, run.pointer)?;
        live.present(&stage, true)?;
        let first_ms = t0.elapsed().as_secs_f64() * 1e3;
        let mut ms = Vec::new();
        for f in order {
            let t0 = Instant::now();
            live.step(f, &run.keys, run.pointer)?;
            live.present(&stage, true)?;
            ms.push((f, t0.elapsed().as_secs_f64() * 1e3));
        }
        let mut v: Vec<f64> = ms.iter().map(|m| m.1).collect();
        v.sort_by(f64::total_cmp);
        let pct = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
        let worst: Vec<_> = { let mut w = ms.clone(); w.sort_by(|a, b| b.1.total_cmp(&a.1)); w.into_iter().take(5).map(|(f, t)| serde_json::json!({"frame": f, "ms": t})).collect() };
        return Ok(format!("SCRUBBED  {}", serde_json::to_string_pretty(&serde_json::json!({"seeks": v.len(), "first_frame_ms": first_ms, "scrub_ms": {"p50": pct(0.5), "p99": pct(0.99), "max": pct(1.0)}, "slowest": worst, "rss_mb": rss_kb() / 1024})).unwrap()));
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
    let mut chrome: Option<(raea::dom::Dom, bool, raea::dom::NodeId)> = (std::env::var("RAEA_PLAY_CHROME").as_deref() == Ok("1")).then(|| {
        let (cw, ch) = layer.unwrap_or((w, h));
        let mut d = raea::dom::Dom::new(cw, ch, run.tsx.parent().unwrap_or(std::path::Path::new(".")));
        let body = d.body();
        d.set_attribute(body, "style", "margin: 0; background: #000");
        let img = d.create_element("img");
        d.set_attribute(img, "style", "position: absolute; left: 0; top: 0; width: 100%; height: 100%; object-fit: contain");
        d.append(body, img);
        let sheet = d.create_element("div");
        d.set_attribute(sheet, "style", "position: absolute; left: 15%; top: 30%; width: 70%; height: 40%; border-radius: 24px; background: rgba(255,255,255,0.12); backdrop-filter: blur(16px)");
        d.append(body, sheet);
        (d, false, img)
    });
    // --input-log FILE: each frame's input as played, written as the input timeline `render --input` takes, so a render
    // of the same frames can be checked against the play (two runners, one world: pixels from frame and input alone)
    let input_log = std::env::args().skip_while(|a| a != "--input-log").nth(1);
    let mut logged = serde_json::Map::new();
    // on Bevy's runner (runner.rs): the same frames stepped, presented and timed by systems in place of the loop below
    let bevy = runner::enabled();
    if bevy {
        let p = runner::Play::new(run, start, seconds, fps, input.clone(), chrome.take(), snaps.clone(), flat_out, input_log.is_some());
        let (p, l, s) = runner::run(p, live, stage)?;
        (run, live, stage) = (p.run, l, s);
        (latency, logged) = (p.latency, p.logged);
    }
    while !bevy && start.elapsed().as_secs_f64() < seconds {
        let t0 = Instant::now();
        let elapsed = start.elapsed().as_secs_f64();
        if run.autoplay {
            run.keys = vec![if (elapsed * 1.5) as i64 % 2 == 0 { "ArrowLeft".into() } else { "ArrowRight".into() }];
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
        let ld = |a: &std::sync::atomic::AtomicU64| a.load(std::sync::atomic::Ordering::Relaxed);
        let (j0, a0, b0) = (ld(&crate::JS_NS), ld(&crate::APPLY_NS), ld(&crate::FLUSH_BYTES));
        let lag = start.elapsed().as_secs_f64() - f as f64 / fps;
        live.step_at(f, &run.keys, run.pointer, lag)?;
        if input_log.is_some() {
            // marked live as the game saw it (isLive), so a render of the log plays the same match, not its script
            let mut e = serde_json::json!({"keys": run.keys, "live": true});
            if let Some((x, y, down)) = run.pointer {
                e["pointer"] = serde_json::json!({"x": x, "y": y, "down": down});
            }
            logged.insert(f.to_string(), e);
        }
        run.js.push((ld(&crate::JS_NS) - j0) as f64 / 1e6);
        run.apply.push((ld(&crate::APPLY_NS) - a0) as f64 / 1e6);
        run.bytes.push((ld(&crate::FLUSH_BYTES) - b0) as f64);
        run.stepped += 1;
        let t1 = Instant::now();
        // RAEA_PLAY_CHROME=1: the stage painted into a texture and drawn by a chrome page in its own frame, under a
        // glass sheet (backdrop blur), as void's native chrome composites it: one present, the glass over the game
        if let Some(c) = chrome.as_mut() {
            live.paint_texture(STAGE_TEXTURE)?;
            if !c.1 {
                let (img, w, h) = (c.2, live.w, live.h);
                c.0.set_attribute(img, "data-raea-texture", &STAGE_TEXTURE.to_string());
                c.0.set_attribute(img, "data-raea-w", &w.to_string());
                c.0.set_attribute(img, "data-raea-h", &h.to_string());
                c.1 = true;
            }
            c.0.present(&stage, true)?;
        } else {
            if layer.is_some() && snaps.iter().any(|(at, _)| (*at * fps).round() as i64 == f) {
                raea::quads::read_next_present();
            }
            live.present(&stage, true)?;
        }
        let presented = if layer.is_some() { raea::quads::take_present() } else { None };
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
        let gap = run.last_present.map(|l| (now - l).as_secs_f64() * 1e3);
        if let Some(g) = gap {
            run.gaps.push(g);
        }
        let budget = 1000.0 / fps.max(1.0);
        raea_native::trace::frame(work, gap.is_some_and(|g| g > budget * 1.5), budget, || serde_json::json!({"frame": f, "step_ms": step_ms, "present_ms": present_ms, "gap_ms": gap}));
        run.last_present = Some(now);
        if let Some(at) = waiting.take() {
            latency.push((now - at).as_secs_f64() * 1e3);
        }
        if run.last_rss.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            let (rss, reusable) = vm_kb();
            run.rss.push(rss);
            run.reusable.push(reusable);
            run.footprint.push(raea_native::trace::footprint_mb());
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
    {
        use std::sync::atomic::Ordering::Relaxed;
        let q = |a: &std::sync::atomic::AtomicU64| a.load(Relaxed);
        // where the CPU still painted (the goal is zeros): whole frames, vello_cpu stretches, rasterized afresh, reused
        let why: serde_json::Map<String, serde_json::Value> = raea::quads::record::WHY.iter().zip(raea::quads::record::WHY_COUNT.iter()).map(|(k, v)| (k.to_string(), serde_json::json!(q(v)))).collect();
        report["cpu_paint"] = serde_json::json!({"frames": q(&raea::quads::CPU_FRAMES), "stretches": q(&raea::quads::CPU_STRETCHES), "rasterized": q(&raea::quads::CPU_RASTERS), "reused": q(&raea::quads::CPU_REUSED), "why": why});
    }
    report["open_ms"] = serde_json::json!(open_ms);
    report["release_ms_total"] = serde_json::json!(crate::RELEASE_NS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6);
    report["eden_ms_total"] = serde_json::json!(crate::EDEN_NS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6);
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
    if let Some(out) = json {
        std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).map_err(|e| format!("{out}: {e}"))?;
    }
    Ok(format!("PLAYED  {}", serde_json::to_string_pretty(&report).unwrap()))
}
