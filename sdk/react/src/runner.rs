//! Live play on Bevy's runner (no-V8 goal, gate 7: two runners, one world). `raea-motion play --offscreen` with
//! RAEA_BEVY_RUNNER=1 (or --bevy-runner) runs its frames as systems of a Bevy App driven by Bevy's ScheduleRunnerPlugin,
//! instead of play.rs's own loop: the same frames, the same input, the same present, the same report.
//!
//!   PreUpdate   input    the frame's index (one more than the last) and this moment's keys and pointer
//!   Update      step     the composition stepped to that frame with that input (Live::step_world: React or the lite
//!                        runtime, its commit applied to the page, the frame's videos placed)
//!               sound    the frame's sounds synced to the mixer, placed by how late the frame is (Live::sync_sound)
//!   PostUpdate  present  the page composited on the GPU and presented (or waited for offscreen: two frames in flight,
//!                        so the GPU works on frame N while the next update builds N+1)
//!   Last        measure  work, gaps, input to present, memory, snapshots
//!               pace     sleep to the next frame's due time (a window's vsync stands here: its present blocks on the
//!                        drawable instead); none with --no-vsync
//!               finish   at the end (or an error), the state handed back and AppExit written
//!
//! A render steps the same world by frame index (motion.rs render_bundle / render_fed): the input it reads is the one
//! `--input-log` wrote here, so pixels depend on the frame and its input alone (bench/games/replay.sh checks it).
//!
//! RAEA_BEVY_RUNNER=wait paces with Bevy's own ScheduleRunnerPlugin::run_loop(1/fps) in place of the pace system: that
//! waits out each update's remainder (no deadline), so its frames drift late; kept to measure it.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::app::{AppExit, ScheduleRunnerPlugin};
use bevy::ecs::schedule::{ScheduleLabel, SingleThreadedExecutor};
use bevy::prelude::*;

use super::{rss_kb, Run, STAGE_TEXTURE};
use crate::live::Live;

type R<T> = Result<T, String>;

/// Live play runs on Bevy's runner: RAEA_BEVY_RUNNER set (not "0"), or --bevy-runner.
pub(super) fn enabled() -> bool {
    std::env::var("RAEA_BEVY_RUNNER").is_ok_and(|v| v != "0") || std::env::args().any(|a| a == "--bevy-runner")
}

/// What the offscreen harness holds while it plays: the run's measurements and the loop's own state.
pub(super) struct Play {
    pub run: Run,
    start: Instant,
    seconds: f64,
    fps: f64,
    input: Option<Arc<Mutex<(Vec<String>, Instant, u64)>>>,
    chrome: Option<(raea::dom::Dom, bool, raea::dom::NodeId)>,
    snaps: Vec<(f64, String)>,
    flat_out: bool,
    log_input: bool,
    pub latency: Vec<f64>,
    pub logged: serde_json::Map<String, serde_json::Value>,
    f: i64,
    seen: u64,
    waiting: Option<Instant>,
    /// this frame: when it started, when its step ended, and raea-react's counters at its start
    t0: Instant,
    t1: Instant,
    counters: (u64, u64, u64),
    error: Option<String>,
}

impl Play {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(run: Run, start: Instant, seconds: f64, fps: f64, input: Option<Arc<Mutex<(Vec<String>, Instant, u64)>>>, chrome: Option<(raea::dom::Dom, bool, raea::dom::NodeId)>, snaps: Vec<(f64, String)>, flat_out: bool, log_input: bool) -> Play {
        let now = Instant::now();
        Play { run, start, seconds, fps, input, chrome, snaps, flat_out, log_input, latency: Vec::new(), logged: Default::default(), f: -1, seen: 0, waiting: None, t0: now, t1: now, counters: (0, 0, 0), error: None }
    }
}

thread_local! {
    /// The state handed back when the app exits (Bevy's runner owns the App and drops it when it returns).
    static DONE: RefCell<Option<(Play, Live, raea::quads::Stage)>> = const { RefCell::new(None) };
}

/// The frames played on Bevy's runner until `seconds` pass: the harness's state, the composition and the stage back.
pub(super) fn run(play: Play, live: Live, stage: raea::quads::Stage) -> R<(Play, Live, raea::quads::Stage)> {
    let wait = std::env::var("RAEA_BEVY_RUNNER").as_deref() == Ok("wait");
    let fps = play.fps.max(1.0);
    let mut app = App::new();
    let runner = if wait && !play.flat_out { ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / fps)) } else { ScheduleRunnerPlugin::default() };
    app.add_plugins(MinimalPlugins.set(runner));
    // every system holds the composition (not Send) and runs on this thread, the one that opened it at the frame
    // threads' QoS: the single-threaded executor, without the task pool's hand-offs
    for label in [First.intern(), PreUpdate.intern(), Update.intern(), PostUpdate.intern(), Last.intern()] {
        app.edit_schedule(label, |s| {
            s.set_executor(SingleThreadedExecutor::new());
        });
    }
    app.insert_non_send(play).insert_non_send(live).insert_non_send(stage);
    app.add_systems(PreUpdate, input)
        .add_systems(Update, (step, sound).chain())
        .add_systems(PostUpdate, present)
        .add_systems(Last, (measure, pace.run_if(move || !wait), finish).chain());
    let exit = app.run();
    let (play, live, stage) = DONE.with(|d| d.borrow_mut().take()).ok_or_else(|| format!("Bevy's runner ended without handing the play back ({exit:?})"))?;
    if let Some(e) = &play.error {
        return Err(e.clone());
    }
    Ok((play, live, stage))
}

fn counters() -> (u64, u64, u64) {
    let ld = |a: &std::sync::atomic::AtomicU64| a.load(std::sync::atomic::Ordering::Relaxed);
    (ld(&crate::JS_NS), ld(&crate::APPLY_NS), ld(&crate::FLUSH_BYTES))
}

/// The frame's index and this moment's input (the input thread's keys, or autoplay's).
fn input(mut p: NonSendMut<Play>) {
    let p = &mut *p;
    if p.error.is_some() {
        return;
    }
    p.t0 = Instant::now();
    let elapsed = p.start.elapsed().as_secs_f64();
    if p.run.autoplay {
        p.run.keys = vec![if (elapsed * 1.5) as i64 % 2 == 0 { "ArrowLeft".into() } else { "ArrowRight".into() }];
    }
    if let Some(sh) = &p.input {
        let (keys, at, seq) = sh.lock().unwrap().clone();
        p.run.keys = keys;
        if seq != p.seen {
            p.seen = seq;
            p.waiting = Some(at);
        }
    }
    p.f += 1;
    p.counters = counters();
}

/// The composition stepped to the frame with its input; the input logged as played.
fn step(mut p: NonSendMut<Play>, mut live: NonSendMut<Live>) {
    let p = &mut *p;
    if p.error.is_some() {
        return;
    }
    let lag = p.start.elapsed().as_secs_f64() - p.f as f64 / p.fps;
    if let Err(e) = live.step_world(p.f, &p.run.keys, p.run.pointer, lag) {
        p.error = Some(e);
        return;
    }
    if p.log_input {
        let mut e = serde_json::json!({"keys": p.run.keys});
        if let Some((x, y, down)) = p.run.pointer {
            e["pointer"] = serde_json::json!({"x": x, "y": y, "down": down});
        }
        p.logged.insert(p.f.to_string(), e);
    }
    let (j0, a0, b0) = p.counters;
    let (j1, a1, b1) = counters();
    p.run.js.push((j1 - j0) as f64 / 1e6);
    p.run.apply.push((a1 - a0) as f64 / 1e6);
    p.run.bytes.push((b1 - b0) as f64);
    p.run.stepped += 1;
}

/// The frame's sounds to the mixer.
fn sound(p: NonSend<Play>, mut live: NonSendMut<Live>) {
    if p.error.is_none() {
        live.sync_sound();
    }
}

/// The page presented (or, RAEA_PLAY_CHROME=1, painted into a texture a chrome page draws under its glass).
fn present(mut p: NonSendMut<Play>, mut live: NonSendMut<Live>, stage: NonSend<raea::quads::Stage>) {
    let p = &mut *p;
    if p.error.is_some() {
        return;
    }
    p.t1 = Instant::now();
    let r = (|| -> R<()> {
        if let Some(c) = p.chrome.as_mut() {
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
            live.present(&stage, true)?;
        }
        let fps = p.fps;
        for (at, out) in p.snaps.iter().filter(|(at, _)| (*at * fps).round() as i64 == p.f) {
            let (w, h, px) = live.snapshot()?;
            image::save_buffer(out, &px, w, h, image::ColorType::Rgba8).map_err(|e| format!("{out}: {e}"))?;
            eprintln!("snapshot at {at} s -> {out}");
        }
        Ok(())
    })();
    if let Err(e) = r {
        p.error = Some(e);
    }
}

/// The frame's times, as play.rs's loop takes them: step, present, work, the gap since the last present, input to
/// present; memory each second.
fn measure(mut p: NonSendMut<Play>) {
    let p = &mut *p;
    if p.error.is_some() {
        return;
    }
    let (step_ms, present_ms) = ((p.t1 - p.t0).as_secs_f64() * 1e3, p.t1.elapsed().as_secs_f64() * 1e3);
    p.run.steps.push(step_ms);
    p.run.presents.push(present_ms);
    let now = Instant::now();
    let work = (now - p.t0).as_secs_f64() * 1e3;
    p.run.work.push(work);
    let gap = p.run.last_present.map(|l| (now - l).as_secs_f64() * 1e3);
    if let Some(g) = gap {
        p.run.gaps.push(g);
    }
    let budget = 1000.0 / p.fps.max(1.0);
    let f = p.f;
    raea_native::trace::frame(work, gap.is_some_and(|g| g > budget * 1.5), budget, || serde_json::json!({"frame": f, "step_ms": step_ms, "present_ms": present_ms, "gap_ms": gap, "runner": "bevy"}));
    p.run.last_present = Some(now);
    if let Some(at) = p.waiting.take() {
        p.latency.push((now - at).as_secs_f64() * 1e3);
    }
    if p.run.last_rss.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
        p.run.rss.push(rss_kb());
        p.run.footprint.push(raea_native::trace::footprint_mb());
        p.run.last_rss = Some(Instant::now());
    }
}

/// To the next frame's due time (the composition's clock), unless frames go as fast as they come.
fn pace(p: NonSend<Play>) {
    if p.flat_out || p.error.is_some() {
        return;
    }
    let next = p.start + Duration::from_secs_f64((p.f + 1) as f64 / p.fps);
    if let Some(d) = next.checked_duration_since(Instant::now()) {
        std::thread::sleep(d);
    }
}

/// The run's end (its seconds passed, or an error): the state handed back and the app told to exit.
fn finish(world: &mut World) {
    let done = {
        let p = world.non_send_resource::<Play>();
        p.error.is_some() || p.start.elapsed().as_secs_f64() >= p.seconds
    };
    if !done {
        return;
    }
    let (Some(play), Some(live), Some(stage)) = (world.remove_non_send::<Play>(), world.remove_non_send::<Live>(), world.remove_non_send::<raea::quads::Stage>()) else { return };
    DONE.with(|d| *d.borrow_mut() = Some((play, live, stage)));
    world.write_message(AppExit::Success);
}
