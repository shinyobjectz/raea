//! raea-page-host: a bundle on a Blitz page in a headless Bevy app, for the benches.
//!
//!   raea-page-host BUNDLE [--css FILE[:FILE...]] [--root DIR[:DIR...]] [--size WxH] [--window [--title T] [--chrome native|inset]]
//!
//! BUNDLE is built JS (an app's Blitz build). --css: the page's own CSS, laid out as Chrome lays out a page; without
//! it, bevy_ui's kinds. --root: where the page's own-origin fetches are read from (RAEA_PAGE_ROOT). --size: the
//! window, default 1440x900. RAEA_PAGE_BRP_PORT (default: a free one), RAEA_PAGE_MCP_PORT (default 7477; 0: none),
//! RAEA_PAGE_PICK=0: the page's pointers and keys dispatched by the host (page::input) instead of through raea-page's
//! picking route, the default. RAEA_FINE_RESTYLE=0: an attribute change restyles its element's parent's whole subtree
//! (Blitz's own way; the host's default is the fine restyle, docs/timeline.feature TL13a). The page's
//! `<img data-raea-stage>` plays a composition live (raea-page-stage).
//! --chrome inset: the page under a transparent title bar, its title area moving the window (raea-page-window).
//! --window (built with the `window` feature): on screen, a window of its own that closes the app (raea's desktop app;
//! RAEA_PAGE_HOST sends the page's /host/* to the feed's host: raea-page-net).

use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::window::ExitCondition;
use raea_page_host::chrome::{self, Chrome};
use raea_page_host::{HostPlugin, brp, camera, mcp, page, stage, tree};

struct Args {
    bundle: String,
    css: Option<Vec<String>>,
    size: UVec2,
    window: bool,
    title: String,
    chrome: Chrome,
}

fn args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let (mut bundle, mut css, mut size, mut window, mut title) = (None, None, UVec2::new(1440, 900), false, "raea".to_string());
    let mut chrome = Chrome::default();
    while let Some(a) = it.next() {
        let mut val = || it.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            // a list as PATH is (`;` on Windows, where `:` is in every C:\ path)
            "--css" => css = Some(std::env::split_paths(&val()?).map(|p| p.to_string_lossy().into_owned()).collect()),
            // read by raea-page's net.rs (page_roots), a list as PATH is
            "--root" => unsafe { std::env::set_var("RAEA_PAGE_ROOT", val()?) },
            "--size" => {
                let v = val()?;
                let (w, h) = v.split_once('x').ok_or(format!("--size {v}: WxH"))?;
                size = UVec2::new(w.trim().parse().map_err(|_| format!("--size {v}"))?, h.trim().parse().map_err(|_| format!("--size {v}"))?);
            }
            "--window" if cfg!(feature = "window") => window = true,
            "--window" => return Err("--window: this build has no window (build with --features window)".into()),
            "--title" => title = val()?,
            "--chrome" => chrome = Chrome::parse(&val()?)?,
            s if s.starts_with("--") => return Err(format!("no option {s}")),
            s => bundle = Some(s.to_string()),
        }
    }
    let bundle = bundle.ok_or("usage: raea-page-host BUNDLE [--css FILE[:FILE]] [--root DIR[:DIR]] (lists as PATH is: ; on Windows) [--size WxH]")?;
    Ok(Args { bundle, css, size, window, title, chrome })
}

fn main() {
    let a = args().unwrap_or_else(|e| {
        eprintln!("raea-page-host: {e}");
        std::process::exit(2);
    });
    // a page's attribute set or cleared restyles what Stylo's invalidation finds, and each ancestor itself, not the
    // element's parent's whole subtree (vendor/blitz-dom mutator.rs): ui-parity's 271 cases come out the same case by
    // case, and a 400-clip timeline drag in raea feed's window went from 38-41 to 48-50 frames a second
    // (docs/timeline.feature TL13a). RAEA_FINE_RESTYLE=0 restyles the parent's whole subtree, as before. Set here,
    // before any thread starts, as --root's RAEA_PAGE_ROOT is.
    if std::env::var_os("RAEA_FINE_RESTYLE").is_none() {
        unsafe { std::env::set_var("RAEA_FINE_RESTYLE", "1") }
    }
    // headless (the benches, raea app check): the system's clipboard, files, dialogs and browser stay untouched (T2d)
    if !a.window {
        raea_io_os::set_headless();
    }
    let code = std::fs::read_to_string(&a.bundle).unwrap_or_else(|e| {
        eprintln!("raea-page-host: {}: {e}", a.bundle);
        std::process::exit(1);
    });
    // with the window feature, bevy_winit is in every Bevy app this process makes: the stage's live engine (raea-render's
    // Engine, on its own thread) would make a second event loop and panic, so it leaves WinitPlugin out
    #[cfg(feature = "window")]
    raea_runtime_plugins::register(raea_runtime_plugins::Registered { name: "raea-page-host window", install: |_| {}, group: Some(|b| b.disable::<bevy::winit::WinitPlugin>()), on_demand: false });
    let picking = !std::env::var("RAEA_PAGE_PICK").is_ok_and(|v| v == "0");
    mcp::serve(mcp::port(), mcp::Server { name: "raea-page-host".into(), instructions: "A raea page. Drive it with input, read it with ui; brp reaches the whole ECS (Bevy Remote Protocol).".into(), prefix: "page".into() });

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(a.chrome.window(&a.title, a.size)),
                // on screen, closing the window ends the app; headless, nothing closes it but the process
                exit_condition: if a.window { ExitCondition::OnPrimaryClosed } else { ExitCondition::DontExit },
                close_when_requested: a.window,
                ..default()
            })
            .set(LogPlugin { level: Level::WARN, ..default() })
            // the shared device (raea_gpu_device), so a game on the stage (raea-page-stage), drawn there by the live engine, is a
            // texture this renderer copies with no trip across devices (RAEA_SHARED_DEVICE=0: Bevy's own, no stage)
            .set(bevy::render::RenderPlugin { render_creation: raea_runtime_view::render_creation(), ..default() })
            // each update draws its own frame: the frame `ui` reports is the one rendered
            .disable::<PipelinedRenderingPlugin>(),
    );
    // every schedule on one thread: the window's ~290 systems a frame are each a few µs, and the multi-threaded executor
    // spent more waking its workers for them than they took (35% of the window's busy time at rest). On one thread, the
    // app's window used 34% less CPU at rest, 8% less through a drag and 4% less playing a film, its frames as long
    // (11-16, three rounds each). Systems' own par_iter still use the task pools. On by default since 0.1.6 (the
    // developer, 11-16); RAEA_ST=0 for Bevy's multi-threaded one, in the environment or raea's settings file
    // (~/.raea/env: the app's window has no shell to set it in). Known: playback's "a seek while it plays" shows one more
    // frame from where it was after the seek (3 runs of 3; the multi-threaded run shows two, as the spec allows)
    if raea_io_home::setting("RAEA_ST").as_deref() != Some("0") {
        // a schedule that's running is out of Schedules: First sets the rest, Last sets First (Main is one system)
        app.add_systems(First, single_threaded).add_systems(Last, single_threaded);
        app.sub_app_mut(bevy::render::RenderApp).add_systems(bevy::render::ExtractSchedule, single_threaded).add_systems(bevy::render::Render, single_threaded);
    }
    // on screen, a window with nothing changing waits for input (raea_page_host::idle): on by default since 0.1.6 (the
    // developer, 11-16), RAEA_IDLE_GATE=0 (the environment or ~/.raea/env, as RAEA_ST) for every frame drawn
    #[cfg(feature = "window")]
    if a.window && raea_io_home::setting("RAEA_IDLE_GATE").as_deref() != Some("0") {
        app.add_plugins(raea_page_host::idle::IdlePlugin);
    }
    // on screen, the page's accessibility tree is the OS's (VoiceOver, macOS accessibility clients: raea-page-ax)
    #[cfg(feature = "window")]
    if a.window {
        app.add_plugins(raea_page_ax::AxPlugin(raea_page_ax::Host { page: page::shown, frame: raea_page_host::inject::frame, queue: raea_page_host::inject::queue }));
    }
    // on screen, bevy_winit's event loop is the runner (a ScheduleRunner here replaced it: no window, 11-04)
    if !a.window && !app.is_plugin_added::<ScheduleRunnerPlugin>() {
        // 60 frames a second, as most screens; RAEA_PAGE_FPS stands in for another screen's rate (a 120 Hz one, to
        // measure how a 60 fps video plays where the window draws twice a frame)
        let fps = std::env::var("RAEA_PAGE_FPS").ok().and_then(|v| v.parse::<f64>().ok()).filter(|f| (1.0..=480.0).contains(f)).unwrap_or(60.0);
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / fps)));
    }
    app.add_plugins(HostPlugin { size: a.size, double_click: Duration::from_millis(500), picking })
        .add_plugins(brp::plugins(brp::port(), |r| r))
        .add_plugins(stage::StagePlugin)
        // a page's interaction sounds, fired on a mixer of its own (docs/hardware.feature HW0)
        .add_plugins(raea_page_sound::PageSoundPlugin)
        // the clipboard, links, notifications, the dock badge, raea:// links and the chrome (raea-page-window)
        .add_plugins(raea_page_window::DesktopPlugin { chrome: a.chrome, live: a.window });
    raea_page_host::page_plugins(&mut app, picking);
    // Rune layers (docs/targets.feature T6c): what's under a data-raea-renderer="rune" element is drawn by Rune on this
    // device and shown in it; a page without one isn't touched. RAEA_RUNE_LAYERS=0: Blitz draws it all
    if std::env::var("RAEA_RUNE_LAYERS").as_deref() != Ok("0") {
        app.add_plugins(raea_rune_slots::RuneSlotsPlugin);
    }
    let (css, size) = (a.css, a.size);
    let preamble = vec![a.chrome.preamble()];
    // the page's code handed over once, not kept (the window's memory, 11-18: the feed's bundle is 21 to 26 MB, held
    // all the app's life beside the JS engine's own copy)
    let mut code = Some(code);
    let mount = move |world: &mut World| {
        chrome::install(world);
        raea_page_host::scheme::install(world);
        let m = page::Mount { code: code.take().unwrap_or_default(), preamble: preamble.clone(), css: css.clone(), styles: None, picking, size };
        if let Err(e) = page::mount(world, m) {
            eprintln!("raea-page-host: {e}");
            std::process::exit(1);
        }
    };
    if a.window {
        app.add_systems(Startup, (raea_page_host::camera_window, mount).chain());
    } else {
        app.add_systems(Startup, (camera, mount).chain());
    }
    if picking {
        app.add_systems(PreUpdate, (page::read_input, page::frame_input, page::size).chain().before(bevy::picking::PickingSystems::ProcessInput))
            .add_systems(Update, (page::step_dropping_asks, page::show).chain());
    } else {
        app.add_systems(Update, (page::read_input, page::input, page::size, page::step_dropping_asks, page::show).chain());
    }
    app.add_systems(PreUpdate, raea_page_host::scheme::follow.run_if(resource_exists::<raea_page_host::scheme::Scheme>));
    app.add_systems(Last, page::tree.before(tree::answer));
    app.run();
}

/// Every schedule of this world that isn't running now, on the single-threaded executor (once, in each schedule it's in).
fn single_threaded(world: &mut World, mut done: Local<bool>) {
    if std::mem::replace(&mut *done, true) {
        return;
    }
    if let Some(mut schedules) = world.get_resource_mut::<bevy::ecs::schedule::Schedules>() {
        for (_, s) in schedules.iter_mut() {
            s.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
        }
    }
}
