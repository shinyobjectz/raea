//! `void`: raea void's window (void.feature). Chromeless: the traffic lights over a black canvas, nothing else until
//! asked for.
// the brain's tool list is one json! literal, past the default 128
#![recursion_limit = "256"]

mod app;
mod assets;
mod brain;
mod checks;
mod eval;
mod frames;
mod host;
mod intent;
mod legible;
mod mcp;
mod mem;
mod narrate;
mod nomic;
mod route;
mod runner;
mod shell;
mod telemetry;
mod term;
mod spaces;
mod skills;
mod win;
mod focus;
mod sessions;
mod workspace;
mod chooser;
mod stage;
mod stage_native;
// void's playtest probe (RAEA_VOID_PROBE=1): the no-V8 goal's gates measured in void's own window
#[path = "../bench/probe.rs"]
mod probe;
mod stage_ts;
mod space_ui;
mod pty;
mod shell_ui;
mod ui;

pub(crate) const CSS: &str = include_str!("../assets/void.css");

fn main() {
    raea_react::startup_mark("void main");
    // a panic anywhere ends void with its report: the UI's thread dying left the window frozen and the process hung
    // (tokio's shutdown waits on workers blocked on the dead main thread)
    std::panic::set_hook(Box::new(|info| {
        eprintln!("void: {info}\n{}", std::backtrace::Backtrace::force_capture());
        std::process::abort();
    }));
    // an agent session's flight recorder (raea_native::trace): a panic's report (message, thread, backtrace, the last events) and
    // a hard crash's land in ~/Library/Logs/raea/void/<session>/; started after the hook above, so its report is
    // written before the abort
    let _trace = raea_native::trace::start("void");
    // `void eval …`: the agent graded on scenarios, headless (eval.rs); traces go to Phoenix's "void-evals" project
    let argv: Vec<String> = std::env::args().collect();
    // `void tool SPACE_DIR NAME 'JSON'`: one of the brain's tools run directly on a space folder, for testing a tool
    // without a model turn
    if argv.get(1).map(String::as_str) == Some("tool") {
        let (Some(dir), Some(name)) = (argv.get(2), argv.get(3)) else {
            eprintln!("void tool SPACE_DIR NAME '{{json input}}'");
            std::process::exit(2);
        };
        let input: serde_json::Value = serde_json::from_str(argv.get(4).map(String::as_str).unwrap_or("{}")).unwrap_or_default();
        let local = tokio::task::LocalSet::new();
        let out = tokio::runtime::Runtime::new().map(|rt| rt.block_on(local.run_until(brain::run_tool(std::path::Path::new(dir), name, &input))));
        match out {
            Ok(Ok(o)) => println!("{o}"),
            Ok(Err(e)) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        std::process::exit(0);
    }
    // `void assets SPACE_DIR [FILE...]`: files brought into a space as the Files tab brings them (placed, video and
    // audio transcribed), then the space's assets listed with their thumbnails' sizes; for testing without the window
    if argv.get(1).map(String::as_str) == Some("assets") {
        let Some(dir) = argv.get(2).map(std::path::PathBuf::from) else {
            eprintln!("void assets SPACE_DIR [FILE...]");
            std::process::exit(2);
        };
        // `void assets SPACE_DIR --search QUERY`: the index brought up to date, then the search as the panel runs it
        if argv.get(3).map(String::as_str) == Some("--search") {
            let q = argv[4..].join(" ");
            let t0 = std::time::Instant::now();
            match assets::reindex(&dir) {
                Ok(n) => println!("indexed {n} in {:.1} s", t0.elapsed().as_secs_f64()),
                Err(e) => println!("index: {e}"),
            }
            let t1 = std::time::Instant::now();
            for h in assets::search(&dir, &q, true) {
                println!("{:.2}  {:<40} {}", h.score, h.rel, h.why);
            }
            println!("searched in {:.1} s", t1.elapsed().as_secs_f64());
            std::process::exit(0);
        }
        let files = assets::expand(&argv[3..].iter().map(std::path::PathBuf::from).collect::<Vec<_>>());
        let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1) });
        let mut failed = false;
        for f in files {
            match assets::place(&dir, &f) {
                Ok(p) => {
                    println!("placed {} -> {}{}{}", f.display(), p.rel, if p.linked { " (linked)" } else { "" }, if p.already { " (already there)" } else { "" });
                    if p.kind.heard() && !p.already {
                        match rt.block_on(brain::listen(&f, &p.dest)) {
                            Ok((w, secs, wp)) => println!("  heard {} words over {secs:.1} s -> {}", w.len(), wp.display()),
                            Err(e) => println!("  not transcribed: {e}"),
                        }
                    }
                }
                Err(e) => { println!("refused {}: {e}", f.display()); failed = true; }
            }
        }
        for a in assets::list(&dir) {
            let thumb = if a.broken { 0 } else { assets::thumbnail(&dir.join(&a.rel)).map(|b| b.len()).unwrap_or(0) };
            println!("{:<9} {:<40} {:>10}{}{}  thumb {} B", a.kind.label(), a.rel, assets::size_label(a.bytes),
                if a.linked { " linked" } else { "" }, if a.words { " transcribed" } else { "" }, thumb);
        }
        std::process::exit(if failed { 1 } else { 0 });
    }
    // `void embed-text doc|query`: the child that embeds text for searching assets (nomic.rs)
    if argv.get(1).map(String::as_str) == Some("embed-text") {
        if let Err(e) = nomic::serve(argv.get(2).map(String::as_str).unwrap_or("doc")) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }
    // `void runner --api URL [--name NAME] [--once]`: this Mac takes jobs from a check service's runner queue
    if argv.get(1).map(String::as_str) == Some("runner") {
        let flag = |k: &str| argv.iter().position(|a| a == k).and_then(|i| argv.get(i + 1)).cloned();
        let api = flag("--api").or_else(|| std::env::var("RAEA_RUNNER_API").ok()).or_else(|| crate::brain::setting("RAEA_API")).unwrap_or_default();
        let host = std::process::Command::new("hostname").arg("-s").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        let name = flag("--name").unwrap_or(host);
        let once = argv.iter().any(|a| a == "--once");
        let out = tokio::runtime::Runtime::new().map(|rt| rt.block_on(runner::run(&api, &name, once)));
        match out {
            Ok(Ok(())) => std::process::exit(0),
            Ok(Err(e)) => { eprintln!("{e}"); std::process::exit(1) }
            Err(e) => { eprintln!("{e}"); std::process::exit(1) }
        }
    }
    // `void checks calibrate PACK [RUNS_DIR] [--jobs N]`: a check pack's model checks against judged eval renders
    if argv.get(1).map(String::as_str) == Some("checks") && argv.get(2).map(String::as_str) == Some("calibrate") {
        let name = argv.get(3).cloned().unwrap_or_else(|| "render".into());
        let runs = argv.get(4).filter(|a| !a.starts_with("--")).map(std::path::PathBuf::from).unwrap_or_else(|| spaces::home().join("evals/runs"));
        let jobs = argv.iter().position(|a| a == "--jobs").and_then(|i| argv.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(4);
        let out = tokio::runtime::Runtime::new().map(|rt| rt.block_on(checks::calibrate(&name, &runs, jobs)));
        match out {
            Ok(Ok(r)) => println!("{r}"),
            Ok(Err(e)) => { eprintln!("{e}"); std::process::exit(1) }
            Err(e) => { eprintln!("{e}"); std::process::exit(1) }
        }
        std::process::exit(0);
    }
    // `void checks run PACK EDIT VIDEO`: one pack on one render (a runner's job; through the check service when
    // RAEA_CHECKS_API is set), the findings and cost as JSON
    if argv.get(1).map(String::as_str) == Some("checks") && argv.get(2).map(String::as_str) == Some("run") {
        let (Some(name), Some(edit), Some(video)) = (argv.get(3), argv.get(4), argv.get(5)) else {
            eprintln!("void checks run PACK EDIT VIDEO [--only ID,ID]");
            std::process::exit(2)
        };
        let pack = checks::load(name).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1) });
        let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1) });
        let only: Option<Vec<String>> = argv.iter().position(|a| a == "--only").and_then(|i| argv.get(i + 1)).map(|v| v.split(',').map(String::from).collect());
        let (found, cost) = rt.block_on(checks::run(&pack, std::path::Path::new(edit), std::path::Path::new(video), &checks::Measured { cuts: None }, only.as_deref()));
        println!("{}", serde_json::json!({"pack": pack.pack, "findings": found, "cost_usd": cost}));
        std::process::exit(0);
    }
    // `void cutcheck EDIT VIDEO`: the joins and the ending of a render, measured (what the gate tells the agent)
    if argv.get(1).map(String::as_str) == Some("cutcheck") {
        let (Some(edit), Some(video)) = (argv.get(2), argv.get(3)) else {
            eprintln!("void cutcheck EDIT VIDEO");
            std::process::exit(2)
        };
        let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1) });
        match rt.block_on(eval::cutcheck(std::path::Path::new(edit), std::path::Path::new(video))) {
            Ok(v) => println!("{v}"),
            Err(e) => { eprintln!("{e}"); std::process::exit(1) }
        }
        std::process::exit(0);
    }
    if argv.get(1).map(String::as_str) == Some("eval") {
        if std::env::var_os("RAEA_VOID_TRACE_PROJECT").is_none() {
            unsafe { std::env::set_var("RAEA_VOID_TRACE_PROJECT", "void-evals") };
        }
        // scenarios run side by side on one thread (the brain's turns aren't Send): mostly they wait on the network
        let local = tokio::task::LocalSet::new();
        let code = tokio::runtime::Runtime::new().map(|rt| rt.block_on(local.run_until(eval::main(&argv[2..])))).unwrap_or(2);
        std::process::exit(code);
    }
    // `void --encode-voice NAME`: the child process that encodes a clip voice (voice/tts.rs ENCODER), then exits
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--encode-voice") {
        let name = args.get(2).cloned().unwrap_or_default();
        let r = raea_void::voice::models::ensure(&raea_void::voice::Pocket::SPEC, &mut |_, _| {})
            .and_then(|d| raea_void::voice::Pocket::load(&d))
            .and_then(|mut p| p.encode_voice(&name));
        if let Err(e) = r {
            eprintln!("void: encoding {name}: {e}");
            std::process::exit(1);
        }
        return;
    }
    // `void PATH` (`void .`): work in that folder, as `claude` or `code .` do (workspace.rs), instead of the last one
    if let Some(dir) = args.get(1).filter(|a| !a.starts_with('-')).map(std::path::Path::new).filter(|p| p.is_dir()) {
        let _ = workspace::LAUNCH.set(std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()));
    }
    // ort logs every session's setup at INFO; void's own log is mcp::log
    let _ = dioxus::logger::init(dioxus::logger::tracing::Level::WARN);
    if let Ok(exe) = std::env::current_exe() {
        let _ = raea_void::voice::tts::ENCODER.set(exe);
    }
    // no App Nap: macOS slows a hidden app's timers and event loop, which stalled every task (an agent at work, the
    // stage, MCP calls) about 20 s after the window went behind others. A user-initiated activity, held for the app's
    // life (the token is deliberately never released).
    unsafe {
        use objc2::runtime::AnyObject;
        let info: *mut AnyObject = objc2::msg_send![objc2::class!(NSProcessInfo), processInfo];
        let reason: *mut AnyObject = objc2::msg_send![objc2::class!(NSString), stringWithUTF8String: c"void keeps working while its window is hidden".as_ptr()];
        // NSActivityUserInitiatedAllowingIdleSystemSleep
        let token: *mut AnyObject = objc2::msg_send![info, beginActivityWithOptions: 0x00EF_FFFFu64, reason: reason];
        let _: *mut AnyObject = objc2::msg_send![token, retain];
    }
    // frames on disk are rdb serve's scratch; the cache is in memory, so start the folder empty
    let _ = std::fs::remove_dir_all(spaces::home().join("frames"));
    probe::start();
    // CoreAudio's start-up on a thread while the window and the first object open (lightspeed F5)
    raea_react::live::warm_audio();
    // the window's first frames come in through the site's pixel diffusion, once a launch (a plain fade with Reduce
    // Motion on), over 280 ms (RAEA_VOID_REVEAL_MS: longer, to look at it); RAEA_VOID_REVEAL=0 shows them at once
    if std::env::var("RAEA_VOID_REVEAL").as_deref() != Ok("0") {
        let ms = std::env::var("RAEA_VOID_REVEAL_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(280.0);
        let _ = dioxus_native::REVEAL.set((reduce_motion(), ms, win::redraw));
    }
    raea_react::startup_mark("void window launching");
    launch();
}

/// The native chrome: the same App through Dioxus native, drawn by the RAEA engine's quads on the window's surface
/// (vendor/dioxus-native), transparent where the chrome paints nothing so the native stage's view under it shows.
fn launch() {
    use dioxus_native::winit::dpi::LogicalSize;
    let attrs = dioxus_native::WindowAttributes::default()
        .with_title("void")
        .with_surface_size(LogicalSize::new(1180.0, 820.0))
        .with_min_surface_size(LogicalSize::new(520.0, 420.0))
        .with_transparent(true)
        // no title, the titlebar see-through, the content under it (the lights over the void)
        .with_platform_attributes(Box::new(
            dioxus_native::winit::platform::macos::WindowAttributesMacOS::default().with_titlebar_transparent(true).with_title_hidden(true).with_fullsize_content_view(true),
        ));
    // the surface black under the page: the composited stage's texture draws under the page's paint, so the stage's box
    // stays see-through and the black shows round a vertical video. See-through (the window behind) only for the stage's
    // own layer under the chrome (RAEA_VOID_STAGE_LAYER=1)
    let base = if std::env::var("RAEA_VOID_STAGE_LAYER").is_ok() { dioxus_native::Color::TRANSPARENT } else { dioxus_native::Color::BLACK };
    // RAEA_VOID_BACKGROUND=1: the window opens behind, not the key window (a test instance must not take the person's typing)
    let attrs = attrs.with_active(std::env::var("RAEA_VOID_BACKGROUND").is_err());
    // RAEA_VOID_HEADLESS=1: no window on screen at all (a test instance measured through its probe: the stage paints
    // into its texture and plays though nothing shows it)
    let headless = std::env::var("RAEA_VOID_HEADLESS").as_deref() == Ok("1");
    let attrs = if headless { attrs.with_visible(false).with_active(false) } else { attrs };
    let cfg = dioxus_native::Config::new().with_window_attributes(attrs.with_transparent(base.components[3] < 1.0)).with_base_color(base);
    dioxus_native::launch_cfg(app::App, Vec::new(), vec![Box::new(cfg)]);
    raea_native::trace::end();
}

/// macOS's Reduce Motion (System Settings, Accessibility, Display).
fn reduce_motion() -> bool {
    use objc2::runtime::AnyObject;
    unsafe {
        let ws: *mut AnyObject = objc2::msg_send![objc2::class!(NSWorkspace), sharedWorkspace];
        !ws.is_null() && objc2::msg_send![&*ws, accessibilityDisplayShouldReduceMotion]
    }
}
