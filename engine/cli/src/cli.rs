use super::*;

/// The sandbox's WASI tools (RAEA_SANDBOX_TOOLS, else raea/sandbox/modules) as wsx.* calls, in the current folder.
pub fn register_sandbox() {
    let dir = std::env::var_os("RAEA_SANDBOX_TOOLS").map(PathBuf::from).unwrap_or_else(|| raea_io_home::home().join("engine/sandbox/modules"));
    if !dir.is_dir() {
        return;
    }
    match wsx::Sandbox::new(&dir) {
        Ok(sb) => wsx::register(sb, std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        Err(e) => eprintln!("raea: the sandbox's tools didn't load ({}): {e}", dir.display()),
    }
}

pub fn cli(args: &[String]) -> i32 {
    // the media verbs (`media probe|still|sheet|...`, `ffmpeg`, `ffprobe`: raea-media, no ffmpeg binary) and an HTML
    // composition's render are raea-app-media's (apps/media/src/command.rs)
    let html = args.first().map(String::as_str) == Some("render") && args.get(1).is_some_and(|e| e.ends_with(".html") || Path::new(e).join("index.html").is_file());
    if html || matches!(args.first().map(String::as_str), Some("media" | "ffmpeg" | "ffprobe" | "shots")) {
        return raea_app_media::command(args);
    }
    // `app run|check|bundle APP.tsx` and `rune ...`: an app on the web or native target (apps/targets, docs/targets.feature)
    if matches!(args.first().map(String::as_str), Some("app" | "rune")) {
        return match raea_app_targets::command(args) {
            Ok(m) => {
                if !m.is_empty() {
                    println!("{m}");
                }
                0
            }
            Err(e) => {
                eprintln!("raea: {e}");
                1
            }
        };
    }
    raea_runtime_react::native::register();
    raea_runtime_react::startup_mark("native calls registered");
    if matches!(args.first().map(String::as_str), Some("run" | "test" | "types")) {
        register_sandbox();
    }
    // a play or a render leaves its session to look back at (raea_io_trace: ~/Library/Logs/raea/raea/)
    let traced = matches!(args.first().map(String::as_str), Some("play" | "render"));
    if traced {
        raea_io_trace::start("raea");
        raea_runtime_react::startup_mark("trace started");
    }
    let code = match run(args) {
        Ok(msg) => {
            crate::refused::clear(args);
            // a still is an export like a video (whose encoder signs and marks it): when this program signs, Content
            // Credentials saying raea drew it, and a PNG's pixels carry its export id (raea_io_media::sign)
            if raea_io_media::signs() && args.first().map(String::as_str) == Some("render") {
                let still = flag(args, "-o").map(PathBuf::from).filter(|o| o.is_file() && matches!(o.extension().and_then(|e| e.to_str()), Some("png" | "jpg" | "jpeg" | "webp")));
                if let Some(o) = still { raea_io_media::sign::sign_file(&o, raea_io_media::sign::Made::Created); }
            }
            // and once, what the page asked for and didn't get (#34: dropped CSS, loads that failed)
            let said = raea_runtime_export::not_applied::said().filter(|_| args.first().is_some_and(|a| a == "render"));
            for line in [msg].into_iter().chain(said).filter(|m| !m.is_empty()) {
                println!("{line}");
            }
            raea_app_direction::rendered(args) // a render's findings counted last; 1 on an error not let through (DR3)
        }
        Err(e) => {
            raea_io_trace::event("refused", json!({"error": e}));
            // and what changed since this file was last refused, so a fix that strays is seen (refused.rs)
            println!("REFUSED  {e}{}", crate::refused::note(args, &e));
            1
        }
    };
    if traced {
        raea_io_trace::end();
    }
    code
}

/// `raea traces [show [N|last|DIR]]`: the sessions every raea app left (newest first: how each ended, its
/// glitches), or one told in full (why it crashed, its worst frames, its errors, its memory).
pub fn traces(args: &[String]) -> R<String> {
    let all = raea_io_trace::sessions();
    if args.get(1).map(String::as_str) == Some("show") {
        let which = args.get(2).map(String::as_str).unwrap_or("last");
        let dir = match which {
            "last" => all.first().map(|s| s.1.clone()),
            n if n.parse::<usize>().is_ok() => all.get(n.parse::<usize>().unwrap()).map(|s| s.1.clone()),
            p => Some(PathBuf::from(p)),
        }
        .ok_or("no such session (raea traces lists them)")?;
        return Ok(raea_io_trace::summarize(&dir));
    }
    let mut out = format!("{:<4} {:<12} {:<28} {:<18} {:>9} {:>8}  {}\n", "#", "app", "session", "ended", "glitches", "errors", "folder");
    for (i, (app, d)) in all.iter().enumerate().take(40) {
        let events = std::fs::read_to_string(d.join("events.jsonl")).unwrap_or_default();
        let glitches = events.lines().filter(|l| l.contains("\"kind\":\"glitch\"")).count();
        let errors = events.lines().filter(|l| l.contains("\"level\":\"ERROR\"") || l.contains("\"kind\":\"error\"") || l.contains("\"kind\":\"gpu_error\"")).count();
        out += &format!("{:<4} {:<12} {:<28} {:<18} {:>9} {:>8}  {}\n", i, app, d.file_name().unwrap_or_default().to_string_lossy(), raea_io_trace::status(d), glitches, errors, d.display());
    }
    if all.is_empty() {
        out += "(no sessions yet)\n";
    }
    Ok(out)
}

/// `raea check`'s host (the TS evaluator, the bundle runner, taste's kinds), which runs the notes' asks too (DR6).
const CHECK: raea_app_direction::check::Host = raea_app_direction::check::Host { ts: raea_app_direction::ts::Host { eval: spec::spec_json, run: run_bundle }, kinds: taste_kinds };
fn notes_run(project: &Path, rules: &[raea_app_notes::check::Rule], args: &[String], whole: bool) -> R<Vec<raea_app_notes::check::Ran>> {
    raea_app_direction::check::run(project, rules, args, whole, &CHECK)
}

/// `raea run SCRIPT.ts`: a TypeScript program against the engine, headless (the native calls: solids, ECS
/// worlds, print). The script's default export, if a function, is called; what it prints goes to stdout; a throw
/// ends the run with exit 1.
pub fn run_script(args: &[String]) -> R<i32> {
    let ts = dunce::canonicalize(&PathBuf::from(&args[1])).map_err(|e| format!("{}: {e}", args[1]))?;
    let out = std::env::temp_dir().join(format!("raea-cli-{}.js", std::process::id()));
    bundle_script(&ts, &out)?;
    let secs: f64 = flag(args, "--timeout").and_then(|t| t.parse().ok()).unwrap_or(120.0);
    let (code, _) = run_bundle(&out, secs)?;
    let _ = std::fs::remove_file(&out);
    Ok(code)
}

pub fn run(args: &[String]) -> R<String> {
    // no arguments, or asking for help: the usage, plainly (the render line's refusal is for a render missing its file)
    // `help --json`: the same help, each verb's usage, flags and access tag (tools/skills.sh gen reads it)
    if args.len() == 2 && args[0] == "help" && args[1] == "--json" {
        return Ok(usage::help_json(&run(&["help".to_string()])?).to_string());
    }
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        let neural_usage = neural::USAGE;
        let light_usage = light::USAGE;
        let was = IN_HELP.replace(true);
        let usage = run(&["render".to_string()]).err().unwrap_or_default();
        IN_HELP.set(was);
        // `raea library` (engine/cli/src/library.ts) and `raea show` (show.ts: in the raea app, not Preview, Shane 11-09), under render's usage
        let usage = format!("{usage}\n       raea library [new NAME [--cloud] | add LIBRARY PATH | files LIBRARY | tag SHA TAGS | status | scan ...] | raea library search \"WORDS\" [--library LIBRARY] [--kind KIND] [--limit N] [--sound]   (the person's libraries in the raea app: folders and drives indexed on this machine, searched by meaning; a folder an agent adds waits for the person's yes)");
        let usage = format!("{usage}\n       raea show FILE... [--title T] [--ask LINE] [--project P] [--replaces ITEM] [--as NAME] [--role ROLE] [--inbox NAME] [--open] [--label LABEL] [--detail PARAGRAPH] [--labels FILE.json] | raea show --question Q --choice A --choice B [STILL...] | raea show [--title T] --question Q [--preview FILE] --choice A [--preview FILE] --choice B ... [--question Q2] ... | raea show --on ITEM [--working ID] [--ask TEXT] [--status S] [--states S1,S2] [--sign N --clip FILE --render HASH] [--done ID] [--note LINE] [--sub NAME]   (shown in the raea app as an item the person can use and answer, posted quietly with a notice, the window left where they are unless --open: stills (several, a storyboard), a video, a sound, a NAME.script.ts or .md, a .tsx (an interactive artifact; a project's composition opens in its editor), or questions, all of them one item; --label \"PLACE TAG\" a label beside the picture, never on it, that points at PLACE (x0,y0,x1,y1 or x,y in its 0-1, then #N the Nth file, @T or @T-TO a film's seconds, or part=KEY a scene's part; the tag the rest), --detail the paragraph of the --label before it, --labels a JSON list of them as feed_post takes them, and a sheet raea media sheet made comes labelled; each question and choice with a --preview to play or read while answering (a still, a video, a sound, a script or .md; after a --choice that choice's, else the question's); what they send comes to ~/.raea/inbox/INBOX.jsonl; their edit comments are tasks: --working ID as you start one (each --ask with it one ask you read it as, for the person to correct; --status and --states where the note and each ask stand: to restate, to check, failing, to sign, passing, signed; --sign N --clip FILE --render HASH a clip under ask N for them to sign; a note closes when they retire it, never with --done), --done ID when it's settled, or with the version that settles it, FILE --replaces ITEM --done ID; never open media in Preview while the app is installed)");
        // clothes for SOMA's body, sewn or laid on the skin (docs/character.feature CH45c): engine/cli/src/clothes.ts, wardrobe.ts and sewn.ts
        let usage = format!("{usage}\n       raea clothes [make NAME [\"WHAT IT IS\"] [--plain] [--set PATH=VALUE,...] [--colour #RRGGBB] | settings | cover NAME --parts PART[:SHARE],... --colour #RRGGBB [--stand MM] [--over UNDER] [--hard [--sole MM] [--sole-colour #RRGGBB] [--panels #RRGGBB,...]] | wear OUT_DIR GARMENT... [--body DIR] [--head HEAD_DIR]]   (clothes for SOMA's body from words: make sews a garment: a sewing pattern (GarmentCode, MIT) from the words, or the plain base with --plain, its settings changed by --set PATH=VALUE (settings lists them and what each may be), cut to SOMA's measurements, draped by raea's own cloth and baked, as assets/clothes/NAME.json and its .glb, in --colour or the palette's next; the first make sets up GarmentCode and its Python in raea's models folder; cover makes one by procedure in the same file, no picture: the skin of the parts named (hips, waist, torso, chest, neck, head, upperArm, foreArm, hand, thigh, shin, foot, toes; a limb's SHARE is how far along it from its top), in one colour, MM off the skin (6 unless said), --over a garment already made as the one under it; wear puts garments on a body, innermost first (each GARMENT a name or its .json): OUT_DIR/NAME.glb each, with OUT_DIR/body.glb the body without the skin wholly under cloth; --body DIR another SOMA body folder (a headless one for a head part); --head HEAD_DIR the head that joins it (head.json), whose skin the body's is graded to at the collar; alone, the project's garments. The first use fetches SOMA's body (NVlabs/SOMA-X, Apache-2.0) into raea's models folder and needs Blender: raea setup blender)");
        // body motion made by Kimodo, on MLX on a Mac and on PyTorch elsewhere (docs/character.feature CH15c):
        // apps/tools/src/motion.rs; `raea setup motion` is listed with the other setups, after blender's
        let usage = format!("{usage}\n       {}   ({})", tools::motion::USAGE, tools::motion::ABOUT);
        // an agent's report of a bug in raea itself, to raea's server (engine/cli/src/bug.ts; Shane 11-16)
        let usage = format!("{usage}\n       raea bug TITLE... --text TEXT [--command CMD] [--error TEXT] [--agent NAME] [--dry-run]   (a bug in raea itself, not in your own composition, sent to raea's server with raea's version and the OS, no account needed: words only, paths, emails and keys taken out before it goes; five a day from a machine, the same bug again counted on the first report; --dry-run prints what would go and sends nothing)");
        // a workspace's files and specs (docs/projects.feature), listed under render's usage like library
        let usage = format!("{usage}\n       raea spec FILE.ts [--as brief|script|taste|plan|controller]   (a spec's default export as JSON, checked against its schema: brief.ts, script.ts, taste.ts, plan.ts, controllers/ID.ts)\n       raea workspace init [NAME] [--layout LAYOUT] | raea workspace adopt DIR [--name NAME] | raea workspace [--json]   (a workspace: a git repo of projects and controllers under raea.root, ~/Documents/raea unless git config says)\n       raea project new NAME [--storage inline|submodule] | raea project split NAME [--remote URL] | raea project join NAME | raea project [--json]   (a project in the workspace: its folder, brief.ts, media/, out/; split and join keep its history)\n       raea controller ls [--json] | raea controller get PATH [--json] | raea controller set PATH VALUE [--project] [--by NAME] | raea controller write FILE.ts [--by NAME] | raea controller rm ID [--by NAME]   (the workspace's controllers, controllers/ID.ts; PATH is controller.group.field; every change a commit)\n       raea check EDIT.tsx --quick [--shape S] [--platform P] [--taste T] [-o check.json]   (three stills of a composition checked for text: clipped, cramped, safe areas, sizes; about a second)\n       raea hook [claude|codex|gemini|kimi]   (every agent's tool hooks, which raea skills install adds: before a shell command, media opened in the Mac's viewer while the raea app is installed is stopped once for raea show (the same command again goes through); after a save, the quick check on a composition)\n       raea next [DIR]   (the two or three useful next steps for the project, from its brief, compositions, renders and checks)\n       raea doctor [--fix] [DIR]   (what doesn't match this raea: old brief files, an older workspace format, an agent's skill from another version)\n       raea app run|check|bundle APP.tsx [--target web|native] [--size WxH] [--script steps.json] [--png DIR] [-o App.app]   (a TSX app on @raea/ui: its window, the headless check, a Mac app; web is Blitz on Bevy, native is Rune; the app's `export const app` sets the target, web when it doesn't; raea rune ... is --target native)\n       {}\n       {}\n       {}   ({})\n       {}\n       {}\n       {}", skill::USAGE, tools::blender::USAGE, tools::motion::SETUP_USAGE, tools::motion::SETUP_ABOUT, raea_app_engines::godot::USAGE, raea_app_engines::shot::USAGE, raea_app_engines::round::USAGE);
        let (export_game, capcut, layers, xml, scene, physics, splat, wear) = (raea_app_game::USAGE, raea_app_capcut::USAGE, raea_app_layers::USAGE, raea_app_layers::USAGE_XML, export::SCENE_USAGE, motion::physics::USAGE, tools::splat::USAGE, raea_runtime_wear::USAGE);
        let apps = format!("       {}\n       {}\n       {}\n       {}\n       {}\n       {}\n       {}\n       {}\n       {}\n       {}\n", raea_app_direction::check::USAGE, raea_app_check::USAGE, raea_app_solid::USAGE, raea_app_judge::USAGE, raea_app_direction::USAGE, raea_app_direction::findings::USAGE, raea_app_notes::USAGE, raea_app_fingerprint::USAGE, raea_sys_index::cli::USAGE, raea_app_videoform::USAGE);
        return Ok(format!("raea: renders, plays and checks raea compositions (React and TypeScript) on the GPU.\n\nusage: {usage}\n       raea init   (a project's files where they're missing: edit.tsx, cuts.ts, components/, AGENTS.md)\n       raea cut TAKE \"line\" ... | --at A-B ... [--append]   (cuts.ts and words.json: a take under media/ cut in the pauses around each line it says, transcribed first if it has no TAKE.words.json; or at those seconds)\n       raea frame TAKE [--at S]   (how a take fits the edit: a talking take for <FaceFrame>, a screen for <ScreenFit>, from where its face is)\n       raea component list | raea component add NAME ... [--shared] [--force] | raea part list | raea part add NAME ... [--shared] [--force]   (a ready component or set copied into components/, or the workspace's shared/components/ with --shared, yours to change; raea part is its old name)\n       raea brand DOMAIN|URL   (the brand kit read from its website on this machine: brand/brand.css, brand/brand.json, BRAND.md, logos in media/brand; says when a plain fetch saw too little)\n       raea guide [NAME]   (craft, the floor read before any edit; the modes: pitch, explain, story, play, tool; motion, music, layout and the rest; bevy and bevy/NAME for 3D)\n       raea context PROJECT   (what to read before working on a project: taste, brief, look, the floor and its modes' guides)\n{apps}       raea play EDIT.tsx [--input | --input-script KEYS.json] [--size WxH] [--seconds N|end]   (a game, live; also: --input-log FILE --offscreen --snapshot T FILE --json FILE --seek F FILE --pick X,Y --view YAW,PITCH,DISTANCE --scrub N --baked --autoplay --no-vsync --master --3d)\n       {export_game}   (a game as a program of its own: a Mac app or a command)\n       {capcut}\n       {layers}\n       {xml}\n       {scene}   (what each 3D part rests on: floating, sunk and clipping parts; light: the scene's lighting)\n       {physics}   (the bakes <Physics> and <Goo> make)\n       {wear}   (a .glb's weathering causes for raea/wear: the model with a second UV set and its cause maps, made once in the asset store)\n       {splat}   (footage trained into a Gaussian splat for <Splat>; setup installs its tools)\n       raea run SCRIPT.ts   (a TypeScript program on the engine, headless: its default export is called, what it prints is the answer; a check like a room's checkLayout; also: --timeout S)\n       raea gen image|video|music|sfx|speech|mesh|text \"PROMPT\" | gen plan|run FLOW.ts|FLOW.json | gen comfy import|export|run|schema FILE | gen models | gen balance   (generated media through made.app, fal, Replicate, OpenRouter, Modal or ComfyUI; ComfyUI workflows both ways: raea gen alone lists its flags)\n       {neural_usage}   (DLSS 5 neural rendering on a `render --aov` folder, with its motion and depth: your own MLX-DLSS build and weights, RAEA_MLXDLSS and RAEA_DLSS_MODEL)\n       {light_usage}   (each scene on each light path against its path-traced reference: distance in stops, the worst tile, a moving shot's flicker, a frame's cost; DIR/report.json)\n       raea mezz [--check] PATH...   (an intra-only ProRes copy of each long-GOP source over 1080p, for smooth scrubbing; --check says which would get one)\n       raea traces [show [N|last|DIR]]   (the sessions raea left, newest first: how each ended, its glitches; show tells one in full)"));
    }
    // `--version`: the release's version, which a skill's front matter names (docs/skills.feature SK6)
    if args.len() == 1 && matches!(args[0].as_str(), "--version" | "-V") {
        return Ok(skill::version());
    }
    // an argument the verb doesn't take is refused with its usage, never dropped (zsh's "--frame 20", one word, rendered
    // every frame): the verb's flags and how many arguments it takes are read from its lines in the help, the one list
    if !IN_HELP.get() {
        if let Some(why) = stray(&run(&["help".to_string()])?, args) {
            return Err(why);
        }
    }
    // `guide [NAME]` (or `craft`, its old name): the craft guides the skill points to, from the checkout or the install (share/raea)
    if matches!(args[0].as_str(), "guide" | "craft") {
        return guide(args.get(1).map(String::as_str));
    }
    // `context PROJECT`: what an agent reads before any work on a project (docs/craft-system.md, A)
    if args[0] == "context" {
        return context(args.get(1).map(String::as_str));
    }
    // `workspace …` and `project …`: the folder format, configured by committed git-config (docs/projects.feature PJ1)
    if args[0] == "workspace" {
        return raea_app_workspace::workspace(&args[1..]);
    }
    if args[0] == "project" {
        return raea_app_workspace::project(&args[1..]);
    }
    // `controller …`: the workspace's controllers/<id>.ts and a project's overrides (PJ2)
    if args[0] == "controller" {
        return raea_app_workspace::controller(&args[1..], spec::spec_json);
    }
    // `skill [NAME]` and `skills install --agent A`: the skills this raea ships, for its version (SK6)
    if args[0] == "skill" {
        return skill::skill(&args[1..]);
    }
    if args[0] == "skills" {
        return skill::install(&args[1..]);
    }
    // `next [DIR]` and `doctor [--fix] [DIR]`: the project's next steps, and its drift from this raea (SK10)
    if args[0] == "next" {
        return next(&args[1..]);
    }
    if args[0] == "doctor" {
        return skill::doctor(&args[1..]);
    }
    // `setup motion [--check]`: what `raea motion` runs on here, made or checked: MLX's models on a Mac, the PyTorch
    // environment and its weights elsewhere (apps/tools motion.rs)
    if args[0] == "setup" && args.get(1).map(String::as_str) == Some("motion") {
        return tools::motion::setup(&args[2..]);
    }
    // `setup blender|godot`, `blender ...` and `godot ...`: Blender and Godot scripted (SK11, apps/engines)
    if let Some(done) = raea_app_engines::cli(args) {
        return done;
    }
    // `spec FILE.ts`: a spec's data as JSON, for a program that reads specs without evaluating TypeScript (the feed's host)
    if args[0] == "spec" { return spec::spec_cli(&args[1..]); }
    // `check VIDEO --taste taste.ts|DIR`: the taste's .ts files evaluated here, then the check (main.rs takes the rest)
    if args[0] == "check" && args.iter().any(|a| a == "--quick") {
        let (report, bad) = raea_app_check::quick::quick(&args[1..], spec::tastes_ts(args)?)?;
        println!("{report}");
        std::process::exit(i32::from(bad));
    }
    // `hook [AGENT]`: every agent harness's tool hooks (A5d): raea show over Preview, the quick check after a save (SK10)
    if args[0] == "hook" { return raea_app_check::hook::hook(&args[1..]); }
    // `check PROJECT|EDIT.tsx`: every rule's check over the compositions' worlds (DR4); a video is the floor alone
    if args[0] == "check" && !raea_app_direction::check::of_video(&args[1..]) { std::process::exit(raea_app_direction::check::cli(&args[1..], &CHECK)); }
    if args[0] == "check" { std::process::exit(raea_app_check::cli_with(&args[1..], spec::tastes_ts(args)?)); }
    if args[0] == "world" { std::process::exit(raea_app_direction::cli(&args[1..])); } // a frame as one world of facts (docs/direction.feature DR1)
    if args[0] == "findings" { return raea_app_direction::findings::cli(args); } // the project's record read back (DR3)
    if args[0] == "notes" { std::process::exit(raea_app_notes::cli(&args[1..], spec::spec_json, notes_run)); } // a person's notes as files, restated as asks, each with a check (DR5, DR6)
    if args[0] == "fingerprint" { std::process::exit(raea_app_fingerprint::cli(&args[1..], spec::spec_json)); } // a person's sense of good work as axes over on-device encoders (docs/fingerprint.feature FP3a, FP14a)
    if args[0] == "videoform" { std::process::exit(raea_app_videoform::cli(&args[1..], spec::spec_json)); } // a kind of video read once from its references (docs/videoform.feature VF1)
    if args[0] == "index" { std::process::exit(raea_sys_index::cli::cli(&args[1..])); } // every picture on a folder or a drive searchable by words, embedded on the Neural Engine (docs/index.feature IX13)
    if args[0] == "traces" { return traces(args); }
    if args.first().map(String::as_str) == Some("play") {
        return raea_app_play::play(args);
    }
    // `export layers EDIT.tsx` (the composition as its layers and layers.json, every editor's export's) and `export xml EDIT.tsx` (an FCP7 XML project for Premiere Pro and DaVinci Resolve): apps/layers
    if let Some(done) = raea_app_layers::cli(args) { return done; }
    // `export capcut EDIT.tsx` (a CapCut draft) and `render ... -o OUT.mov --alpha` (a transparent movie): apps/capcut
    if let Some(done) = raea_app_capcut::cli(args, |a| raea_app_sound::render(a).unwrap_or_else(|| raea_runtime_export::render(a))) { return done; }
    // `export-game EDIT.tsx -o OUT`: the game as a program of its own (game.rs, sdk/react/docs/game-export.md)
    if args.first().map(String::as_str) == Some("export-game") {
        return raea_app_game::export(args);
    }
    // `add PKG... [--dev] | remove NAME... | install [--offline]  [--in DIR]`: npm packages for the project (its
    // package.json and raea.lock, a pnpm-style node_modules from one verified store; install scripts never run), which
    // a composition, an app screen or a legacy-app panel imports (io/npm)
    if matches!(args.first().map(String::as_str), Some("add" | "remove" | "install")) {
        return raea_io_npm::cli(args);
    }
    // `mezz [--check] PATH...`: optimized media (raea_io_media::mezz, lightspeed F4): each long-GOP source over 1080p under
    // PATH (a file, or a folder's videos) gets its intra-only ProRes copy, one at a time; --check only says which would
    // `neural mlxdlss DIR -o OUT`: DLSS 5 on a `render --aov` folder, through the user's own MLX-DLSS (RT2)
    if args.first().map(String::as_str) == Some("neural") {
        return neural::neural(&args[1..]);
    }
    // `light check SCENES.json -o DIR`: raea Light's bar (docs/realism.feature LT1)
    if args.first().map(String::as_str) == Some("light") {
        return light::light(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("mezz") {
        return mezz(&args[1..]);
    }
    // the sound tools (apps/tools sound.rs): `sao "PROMPT" -o OUT.wav` and `tokensynth "TIMBRE" --notes C3,C4 -o DIR` make
    // through the edit's asset store as its render would; `setup-audio` installs or checks both local models;
    // `sounds [WORDS]` and `sounds get PACK/INSTRUMENT` search and fetch the open sample libraries
    match args.first().map(String::as_str) {
        Some("sao") => return tools::sound::sao(args),
        Some("tokensynth") => return tools::sound::tokensynth(args),
        Some("setup-audio") => return tools::sound::setup_audio(args),
        Some("sounds") => return tools::sound::sounds(args),
        // `splat train VIDEO|DIR -o OUT.ply`: footage trained into a Gaussian splat for <Splat> (apps/tools splat.rs)
        Some("splat") => return tools::splat::splat(args),
        // `physics bake|goo|part ...`: the bakes <Physics> and <Goo> make (systems/bake, registered in motion/physics.rs)
        Some("physics") => return motion::physics::cli(args),
        // `wear bake REQUEST.json [-o OUT.json]`: raea/wear's cause maps of a .glb (raea-runtime-wear, registered in thread.rs)
        Some("wear") => return raea_runtime_wear::cli(args, motion::media_path),
        // `scene lint|explain EDIT.tsx`: what each 3D part rests on; floating, sunk and clipping parts (raea-sys-contact, through motion/contact.rs)
        Some("scene") => return export::scene_cli(args),
        // `song FILE [-o OUT.song.json] [--plot OUT.png]`: the song's energy map for pacing (apps/tools/src/song.rs)
        Some("song") => return tools::song::song(args),
        // `track CLIP.mp4 [--subject X,Y,W,H] [--from S]`: the clip's camera solved for <Matchmove> (motion/track.rs)
        Some("track") => return motion::track::track(args),
        // `motion "LINE" [--keys KEYS.json]`: body motion made by Kimodo, on MLX on a Mac, on PyTorch elsewhere (apps/tools motion.rs, raea-sys-kimodo)
        Some("motion") => return tools::motion::motion(args),
        // `gen …`: generation flows on made.app, fal, Replicate, OpenRouter, Modal and ComfyUI, and ComfyUI workflows read
        // and written (raea-io-gen, through apps/tools generate.rs for the local models)
        Some("gen") => return raea_app_tools::generate::generate(args),
        // the agent shell's own verbs ported here (`init`, `cut`, `frame`): engine/cli's handler, run by its native.ts (shell())
        Some(v) if SHELL.contains(&v) => return shell(args),
        _ => if let Some(r) = raea_app_solid::verb(args) { return r }, // `solid info|view FILE.ts`, `judge DIR` (apps/solid)
    }
    // `master-gain BUNDLE.js DIR`: the gain and ceiling a composition's render masters its sound with, as JSON (live
    // play asks this of a child process, so its second React and the whole mix never sit in the game's memory)
    if args.first().map(String::as_str) == Some("master-gain") && args.len() >= 3 {
        let g = master_gain(Path::new(&args[1]), Path::new(&args[2]))?;
        return Ok(match g {
            Some((gain, ceiling)) => json!({"gain_db": gain, "ceiling_dbtp": ceiling}),
            None => json!({"none": true}),
        }
        .to_string());
    }
    if args.first().map(String::as_str) == Some("bundle") && args.len() >= 2 {
        let ts = dunce::canonicalize(&PathBuf::from(&args[1])).map_err(|e| format!("{}: {e}", args[1]))?;
        let out = PathBuf::from(flag(args, "-o").ok_or("raea bundle SCRIPT.ts -o OUT.js")?);
        if args.iter().any(|a| a == "--composition-bundle") {
            let portable = flag(args, "--portable").map(PathBuf::from);
            bundle_composition(&ts, &out, &comp_opts(args)?, portable.as_deref())?;
            return Ok(format!("APPLIED  {} bundled as a composition to {}", ts.display(), out.display()));
        }
        bundle_script(&ts, &out)?;
        return Ok(format!("APPLIED  {} bundled to {}", ts.display(), out.display()));
    }
    if args.first().map(String::as_str) == Some("types") {
        // the native calls' TypeScript, from the registry (sdk/raea/src/native-calls.ts): every call a composition's
        // thread answers
        raea_runtime_react::register_natives();
        let ts = raea_native::typescript();
        match flag(args, "-o") {
            Some(o) => {
                std::fs::write(&o, &ts).map_err(|e| format!("{o}: {e}"))?;
                return Ok(format!("APPLIED  {} native calls' types written to {o}", raea_native::calls().len()));
            }
            None => return Ok(ts),
        }
    }
    if args.first().map(String::as_str) == Some("test") && args.len() >= 2 {
        // conformance tests on the engine's V8: raea test FILE... [--filter TEXT] [--timeout S] [--bundle-only OUT.js]
        // --define K=V, --alias SPEC=PATH and --node-stub a,b go to the bundler (each may repeat)
        let mut files = Vec::new();
        let mut owned: Vec<String> = Vec::new();
        let mut i = 1;
        while i < args.len() {
            if args[i].starts_with("--") {
                if matches!(args[i].as_str(), "--define" | "--alias" | "--node-stub") && i + 1 < args.len() {
                    owned.push(args[i].clone());
                    // an alias's path is the caller's, so it's made absolute here (the bundler runs elsewhere)
                    owned.push(match (args[i].as_str(), args[i + 1].split_once('=')) {
                        ("--alias", Some((k, v))) => format!("{k}={}", std::path::absolute(v).map(|p| p.display().to_string()).unwrap_or_else(|_| v.to_string())),
                        _ => args[i + 1].clone(),
                    });
                }
                i += 2;
                continue;
            }
            files.push(dunce::canonicalize(&PathBuf::from(&args[i])).map_err(|e| format!("{}: {e}", args[i]))?);
            i += 1;
        }
        let extra: Vec<&str> = owned.iter().map(String::as_str).collect();
        if let Some(o) = flag(args, "--bundle-only") {
            // the bundler runs in its own folder, so the output path is made absolute here
            let o = std::path::absolute(&o).map_err(|e| format!("{o}: {e}"))?.display().to_string();
            bundle_tests(&files, Path::new(&o), flag(args, "--filter").as_deref(), &extra)?;
            return Ok(format!("APPLIED  {} test files bundled to {o}", files.len()));
        }
        // the repo root for fixtures (tests read RAEA_ROOT through host.env)
        if std::env::var_os("RAEA_ROOT").is_none() {
            unsafe { std::env::set_var("RAEA_ROOT", dunce::canonicalize(&raea_io_home::home()).unwrap_or_default()) };
        }
        let out = std::env::temp_dir().join(format!("raea-cli-t{}.js", std::process::id()));
        bundle_tests(&files, &out, flag(args, "--filter").as_deref(), &extra)?;
        let secs: f64 = flag(args, "--timeout").and_then(|t| t.parse().ok()).unwrap_or(300.0);
        let (code, _) = run_bundle(&out, secs)?;
        let _ = std::fs::remove_file(&out);
        if code != 0 {
            return Err(format!("{} test file(s): some failed", files.len()));
        }
        return Ok(String::new());
    }
    if args.first().map(String::as_str) == Some("run") && args.len() >= 2 {
        let code = run_script(args)?;
        if code != 0 {
            return Err(format!("{} ended with exit {code}", args[1]));
        }
        return Ok(String::new());
    }
    // a composition already bundled (NAME.comp.js, as the iPhone's probe renders it), its media under DIR; --frame N
    // draws the one frame the probe's raea_motion_frame draws, for comparing the phone's pixels with the Mac's
    if args.first().map(String::as_str) == Some("render") && args.get(1).map(String::as_str) == Some("--bundle") && args.len() >= 3 {
        let js = PathBuf::from(&args[2]);
        let dir = PathBuf::from(flag(args, "--dir").unwrap_or_else(|| js.parent().unwrap().to_string_lossy().into_owned()));
        let out = PathBuf::from(flag(args, "-o").ok_or("-o OUT.mp4|OUT.png|DIR")?);
        let wanted = flag(args, "--frame").map(|f| f.parse::<i64>().map(|f| vec![f]).map_err(|_| "--frame takes a frame number")).transpose()?;
        return render_bundle(&js, &dir, &out, wanted);
    }
    if args.first().map(String::as_str) != Some("render") || args.len() < 2 {
        return Err(format!(
            "raea render EDIT.tsx -o OUT.mp4|OUT.png|OUT.wav|DIR [--audio OUT.wav] [--frame N | --at A,B] [--composition ID] [--w W] [--h H] [--fps FPS] [--frames N] [--words W.json] [--perception P.json] [--input I.json|inputs.rrl] [--manifest render.json] [--layers] [--stage WxH] [--10bit] [--aov] [--alpha [--alpha-codec prores|hevc|rle|png]] [--kbps N | --quality draft|high|master|0..1] [--clay] [--despite CHECK WHY]   (also: --lite --comps)\n       raea render EDIT.tsx --review DIR [--frame N]   (fixed cameras hero, front, right, top, iso, low in colour and clay: DIR/colour.png, clay.png, review.json)\n       raea render PAGE.html -o OUT.mp4|OUT.png|DIR [--at A,B] [--alpha]   (an HTML composition)\n       raea render --bundle NAME.comp.js -o OUT.png|DIR [--dir DIR] [--frame N]   (a bundle from raea bundle, as render_bundle draws it)
       raea media probe|still|sheet|audio|loudness|motion|cuts|freezes|proxy|lod|trim|concat FILE ... | ffmpeg ARGS | ffprobe ARGS   (media work, native: media help)
       raea bundle EDIT.tsx --composition-bundle -o OUT.js [--portable DIR]   (a composition for render_bundle, on the Mac or the iPhone)\n       raea add PKG[@RANGE]... [--dev] | remove NAME... | install [--offline]  [--in DIR]   (npm packages for the project: raea.lock, node_modules)\n       {}   (foley, local)\n       {}   (an instrument's notes, local)\n       raea setup-audio [--check] [sao|tokensynth]   (install or check the local audio models)\n       {}   (the song's energy map: beats, sections, builds, drops, stops, hits, cuts per bar; also: --backend NAME)\n       raea sounds [WORDS] | sounds get PACK/INSTRUMENT [--notes ..|--all] [-o DIR]   (open sample libraries for a Sampler)\n       {}   (the clip's camera solved, for <Matchmove>)",
            tools::sound::SAO_USAGE, tools::sound::TOKENSYNTH_USAGE, tools::song::USAGE, motion::track::USAGE
        ));
    }
    // the sound alone (-o OUT.wav), or a video's sound beside it (--audio OUT.wav): apps/sound
    raea_app_sound::render(args).unwrap_or_else(|| raea_runtime_export::render(args))
}

