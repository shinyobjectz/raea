//! raea: render raea compositions (HTML + CSS + GSAP) to video
//! without a browser. Blitz lays out and paints the HTML/CSS; GSAP timelines are evaluated
//! natively with animato. Logs go to stderr, machine-readable output to stdout.

use raea::{measure, media, runtime, scene};

use scene::{Composition, Options};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

const USAGE: &str = "raea — render raea compositions (HTML/CSS/GSAP) without a browser

USAGE
  raea probe    <project-dir|file.html> [--fps N] [--variables JSON|@file]
  raea render   <project-dir|file.html> -o <out> [options]
  raea snapshot <project-dir|file.html> --at <t>[,<t>...] -o <file.png|dir> [--boxes [--safe-zone T,R,B,L]]
  raea lint     <project-dir|file.html> [--json]
  raea row      rows|print|render FILE.row [-o DIR] [--at F,F] [--every N] [--scale S] [--data FILE.json]   (docs/row.feature)
  raea measure  <video|project-dir|file.html> [--json] [--video RENDER] [--vision BIN] [--keyframes DIR] [--norms DIR]
  raea measure  --cards ROWS.jsonl [--norms DIR]
  raea previz   score <layout.json|-> [--json]   (one layout, or an array: ranked, passing first)
  raea previz   html  <layout.json|-> [-o out.html]   (labelled boxes to snapshot; docs/previz.feature)

MEASURE (docs/ad-card.feature)
  The structure part of an ad card: cuts, pacing, on-screen text and where it sits, faces, loudness, silence, as a
  summary and a 43-number vector, plus advisory notes against the norms measured over thousands of UGC ads and the
  nearest editing style. A video is decoded (ffmpeg) and read by a Vision helper for text and faces (--vision or
  $RAEA_VISION; without one, text and faces are unknown). An edit (.html) is read from its timeline: exact
  cuts and text; --video adds faces and loudness from its render. --keyframes DIR keeps six frames for the image
  embedding. --norms DIR holds editing-profile.json and ugc-edit-styles.json (default $RAEA_NORMS, else a
  knowledge/ folder above the current one). --cards scores corpus rows (one card per line) without decoding.

RENDER OPTIONS
  -o, --output PATH        output file (or directory for png-sequence; '-' = stdout for rgba)
  --format FMT             mp4 (default) | mov | webm | png-sequence | rgba
  --fps N|N/D              frame rate (default: root data-fps, else 30)
  --scale S                device pixel ratio (default 1)
  --width W --height H     override the composition size (CSS px)
  --start S --end S        render only part of the timeline (seconds)
  --workers N              parallel render workers (default: half the cores, max 8)
  --quality Q              draft | standard (default) | high
  --crf N                  override the encoder quality
  --variables JSON|@file   values for getVariables() (same as the HTML composition format --variables)
  --video-frames DIR       pre-extracted <video> frames: DIR/<videoId>/frame_%06d.(jpg|png),
                           one per output frame from the clip's start (no ffmpeg needed)
  --no-network             never fetch http(s) assets; use the on-disk cache only
  --transparent            keep the background transparent (mov/webm/png/rgba)
  --profile P              html (default: the composition format's deterministic fonts) | web (a plain page, fonts as written)
  --clock C                settled (default: the page's timers run while it loads, then its clocks hold still) | frames
                           (Date, performance, timers and requestAnimationFrame follow the frames' times)
  --backend B              raster backend: auto (default: skia if the composition uses backdrop-filter or SVG filters,
                           else gpu) | vello (vello_cpu, pure Rust) | skia (Chrome's rasterizer)
                           | gpu (instanced quads on the GPU, vello_cpu for what it can't paint)
  --quiet                  no progress on stderr

LINT OPTIONS
  --layout                 also render frames (each --at, else every 0.5 s, at most 40) and check the text against its
                           components: text spilling past a card, pill or button is an error, a row of cards most of
                           which wrap their text is a warning; each finding's region is cropped to -o DIR (default
                           <composition dir>/lint-crops) and listed under crops

SNAPSHOT OPTIONS
  --boxes                  also write <frame>.boxes.json: every visible text element's box in composition px
                           (transforms included), canvas edges it crosses, overflow clipping, opacity,
                           safe-zone bands it reaches and the text boxes it overlaps
  --safe-zone T,R,B,L      safe-zone bands in composition px (default 220,140,420,0 at 1080x1920, else none)

Video encoding (mp4/mov/webm) and the audio mix use ffmpeg from PATH (or $RAEA_FFMPEG).
'rgba' streams raw RGBA8 frames (width*height*4 bytes each, sizes from `probe`) for callers that
encode themselves; `probe` prints the audio plan to mix alongside.
";

struct Args {
    command: String,
    input: Option<PathBuf>,
    output: Option<String>,
    format: Option<String>,
    fps: Option<(u32, u32)>,
    scale: f32,
    width: Option<u32>,
    height: Option<u32>,
    start: Option<f64>,
    end: Option<f64>,
    workers: Option<usize>,
    quality: String,
    crf: Option<u32>,
    variables: Option<Value>,
    video_frames: Option<PathBuf>,
    network: bool,
    transparent: bool,
    profile: scene::Profile,
    clock: scene::Clock,
    quiet: bool,
    json: bool,
    at: Vec<f64>,
    backend: scene::Backend,
    boxes: bool,
    /// lint renders frames and checks text against its components (src/textfit.rs)
    layout: bool,
    safe_zone: Option<[f64; 4]>,
    video: Option<PathBuf>,
    vision: Option<PathBuf>,
    perceive: Option<PathBuf>,
    keyframes: Option<PathBuf>,
    norms: Option<PathBuf>,
    cards: Option<PathBuf>,
    breakdown: Option<PathBuf>,
    words: Option<PathBuf>,
    tol: Option<f64>,
    card: Option<PathBuf>,
}

fn parse_fps(s: &str) -> Result<(u32, u32), String> {
    if let Some((n, d)) = s.split_once('/') {
        let n: u32 = n.trim().parse().map_err(|_| format!("bad fps {s}"))?;
        let d: u32 = d.trim().parse().map_err(|_| format!("bad fps {s}"))?;
        if n == 0 || d == 0 {
            return Err(format!("bad fps {s}"));
        }
        return Ok((n, d));
    }
    let f: f64 = s.trim().parse().map_err(|_| format!("bad fps {s}"))?;
    if f <= 0.0 || !f.is_finite() {
        return Err(format!("bad fps {s}"));
    }
    if (f - f.round()).abs() < 1e-9 {
        Ok((f.round() as u32, 1))
    } else {
        Ok(((f * 1000.0).round() as u32, 1000))
    }
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let command = it.next().ok_or_else(|| USAGE.to_string())?;
    let mut a = Args {
        command,
        input: None,
        output: None,
        format: None,
        fps: None,
        scale: 1.0,
        width: None,
        height: None,
        start: None,
        end: None,
        workers: None,
        quality: "standard".into(),
        crf: None,
        variables: None,
        video_frames: None,
        network: true,
        transparent: false,
        quiet: false,
        json: false,
        at: vec![],
        backend: scene::Backend::default(),
        profile: scene::Profile::default(),
        clock: scene::Clock::default(),
        boxes: false,
        layout: false,
        safe_zone: None,
        video: None,
        vision: None,
        perceive: None,
        keyframes: None,
        norms: None,
        cards: None,
        breakdown: None,
        words: None,
        tol: None,
        card: None,
    };
    let mut rest: Vec<String> = it.collect();
    rest.reverse();
    let next = |name: &str, rest: &mut Vec<String>| rest.pop().ok_or(format!("{name} needs a value"));
    while let Some(arg) = rest.pop() {
        match arg.as_str() {
            "-o" | "--output" => a.output = Some(next(&arg, &mut rest)?),
            "--format" | "-f" => a.format = Some(next(&arg, &mut rest)?),
            "--boxes" => a.boxes = true,
            "--layout" => a.layout = true,
            "--safe-zone" => {
                let v: Vec<f64> = next(&arg, &mut rest)?.split(',').map(|x| x.trim().parse::<f64>()).collect::<Result<_, _>>().map_err(|_| "bad --safe-zone (top,right,bottom,left)")?;
                let [top, right, bottom, left] = v[..] else { return Err("--safe-zone needs top,right,bottom,left".into()) };
                a.safe_zone = Some([top, right, bottom, left]);
            }
            "--fps" => a.fps = Some(parse_fps(&next(&arg, &mut rest)?)?),
            "--scale" | "--resolution" => a.scale = next(&arg, &mut rest)?.parse().map_err(|_| "bad --scale")?,
            "--width" => a.width = Some(next(&arg, &mut rest)?.parse().map_err(|_| "bad --width")?),
            "--height" => a.height = Some(next(&arg, &mut rest)?.parse().map_err(|_| "bad --height")?),
            "--start" => a.start = Some(next(&arg, &mut rest)?.parse().map_err(|_| "bad --start")?),
            "--end" => a.end = Some(next(&arg, &mut rest)?.parse().map_err(|_| "bad --end")?),
            "--workers" | "-w" => a.workers = Some(next(&arg, &mut rest)?.parse().map_err(|_| "bad --workers")?),
            "--quality" | "-q" => a.quality = next(&arg, &mut rest)?,
            "--crf" => a.crf = Some(next(&arg, &mut rest)?.parse().map_err(|_| "bad --crf")?),
            "--variables" => {
                let v = next(&arg, &mut rest)?;
                let text = match v.strip_prefix('@') {
                    Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?,
                    None => v,
                };
                a.variables = Some(serde_json::from_str(&text).map_err(|e| format!("--variables: {e}"))?);
            }
            "--variables-file" => {
                let p = next(&arg, &mut rest)?;
                let text = std::fs::read_to_string(&p).map_err(|e| format!("{p}: {e}"))?;
                a.variables = Some(serde_json::from_str(&text).map_err(|e| format!("--variables-file: {e}"))?);
            }
            "--video-frames" => a.video_frames = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--video" => a.video = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--vision" => a.vision = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--perceive" => a.perceive = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--keyframes" => a.keyframes = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--norms" => a.norms = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--cards" => a.cards = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--breakdown" => a.breakdown = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--words" => a.words = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--tol" => a.tol = Some(next(&arg, &mut rest)?.parse().map_err(|_| "bad --tol")?),
            "--card" => a.card = Some(PathBuf::from(next(&arg, &mut rest)?)),
            "--no-network" => a.network = false,
            "--backend" => {
                a.backend = match next(&arg, &mut rest)?.as_str() {
                    "auto" => scene::Backend::Auto,
                    "skia" => scene::Backend::Skia,
                    "vello" => scene::Backend::Vello,
                    "gpu" => scene::Backend::Gpu,
                    other => return Err(format!("unknown --backend {other} (auto | vello | skia | gpu)")),
                }
            }
            "--transparent" => a.transparent = true,
            "--profile" => a.profile = scene::Profile::parse(&next(&arg, &mut rest)?)?,
            "--clock" => a.clock = scene::Clock::parse(&next(&arg, &mut rest)?)?,
            "--quiet" => a.quiet = true,
            "--json" => a.json = true,
            "--at" => {
                for t in next(&arg, &mut rest)?.split(',') {
                    a.at.push(t.trim().parse().map_err(|_| format!("bad --at {t}"))?);
                }
            }
            "-h" | "--help" => return Err(USAGE.to_string()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}\n\n{USAGE}")),
            _ => {
                if a.command == "breakdown" && a.at.is_empty() && a.format.is_none() && ["parse", "from", "check", "merge"].contains(&arg.as_str()) {
                    a.format = Some(arg);   // the breakdown sub-command
                    continue;
                }
                if a.command == "previz" && a.format.is_none() && ["score", "html"].contains(&arg.as_str()) {
                    a.format = Some(arg);   // the previz sub-command
                    continue;
                }
                if a.input.is_some() {
                    return Err(format!("unexpected argument {arg}"));
                }
                a.input = Some(PathBuf::from(arg));
            }
        }
    }
    Ok(a)
}

fn resolve_input(input: &Option<PathBuf>) -> Result<(PathBuf, PathBuf), String> {
    let p = input.clone().unwrap_or_else(|| PathBuf::from("."));
    let p = p.canonicalize().map_err(|e| format!("{}: {e}", p.display()))?;
    if p.is_dir() {
        let html = p.join("index.html");
        if !html.exists() {
            return Err(format!("{} has no index.html", p.display()));
        }
        Ok((html, p))
    } else {
        let dir = p.parent().unwrap_or(Path::new("/")).to_path_buf();
        Ok((p, dir))
    }
}

fn frame_count(duration: f64, fps: f64) -> u64 {
    let raw = duration * fps;
    let nearest = raw.round();
    if (raw - nearest).abs() <= 1e-3 { nearest as u64 } else { raw.ceil() as u64 }
}

fn default_fps(html: &Path) -> (u32, u32) {
    // root data-fps, read without a full parse
    let text = std::fs::read_to_string(html).unwrap_or_default();
    if let Some(i) = text.find("data-fps=") {
        let rest = &text[i + 9..];
        let q = rest.chars().next().unwrap_or('"');
        let v: String = rest[1..].chars().take_while(|c| *c != q).collect();
        if let Ok(f) = parse_fps(&v) {
            return f;
        }
    }
    (30, 1)
}

fn log(quiet: bool, msg: &str) {
    if !quiet {
        eprintln!("{msg}");
    }
}

struct Probe {
    comp: Composition,
    videos: Vec<media::VideoClip>,
    audio: Vec<media::AudioClip>,
}

fn load(a: &Args, html: &Path, dir: &Path, fps: (u32, u32), videos: Arc<media::VideoFrames>) -> Result<Composition, String> {
    let size = match (a.width, a.height) {
        (Some(w), Some(h)) => Some((w, h)),
        _ => None,
    };
    Composition::load(Options {
        html_path: html.to_path_buf(),
        project_dir: dir.to_path_buf(),
        fps,
        scale: a.scale,
        size,
        variables: a.variables.clone(),
        allow_network: a.network,
        transparent: a.transparent,
        videos,
        backend: a.backend,
        profile: a.profile,
        html_source: None,
        clock: a.clock,
    })
}

fn probe(a: &Args, html: &Path, dir: &Path, fps: (u32, u32)) -> Result<Probe, String> {
    let comp = load(a, html, dir, fps, Arc::new(media::VideoFrames::default()))?;
    let (videos, audio) = {
        let inner = blitz_dom::Document::inner(&comp.doc);
        let durations = std::collections::HashMap::new();
        let windows = std::collections::HashMap::new();
        let mut resolver = runtime::Resolver::new(&inner, &durations, &windows);
        media::discover(&inner, &mut resolver, dir, comp.duration)
    };
    Ok(Probe { comp, videos, audio })
}

fn probe_json(p: &Probe, fps: (u32, u32)) -> Value {
    let fpsf = fps.0 as f64 / fps.1 as f64;
    let (w, h) = p.comp.output_size();
    json!({
        "width": w,
        "height": h,
        "cssWidth": p.comp.width,
        "cssHeight": p.comp.height,
        "fps": if fps.1 == 1 { json!(fps.0) } else { json!(format!("{}/{}", fps.0, fps.1)) },
        "duration": p.comp.duration,
        "frames": frame_count(p.comp.duration, fpsf),
        "rootComposition": p.comp.root_comp_id,
        "backend": p.comp.backend_name(),
        "timelineDuration": p.comp.timeline_duration,
        "videos": p.videos,
        "audio": p.audio,
        "warnings": p.comp.warnings,
        "unsupported": p.comp.unsupported,
        "jsErrors": p.comp.js_errors,
    })
}

fn main() {
    // `raea row ...`: .row edits (src/row.rs, src/row_render.rs), with their own flags
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("row") {
        let code = raea::row_render::cli(&argv[1..], &|p: &Path, w, h, rgba: &[u8]| write_png(p, w, h, rgba)).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            1
        });
        std::process::exit(code);
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let code = match run(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn run(a: Args) -> Result<i32, String> {
    match a.command.as_str() {
        "--version" | "version" | "-V" => {
            // which JavaScript engine this build runs, and how (engine/html/docs/js.feature), so nobody has to guess
            let js = if raea::script::jit_on() { "v8 (JIT on: RAEA_JIT=1, local benches only)" } else { "v8 (jitless)" };
            println!("raea {}\njs: {js}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(0)
        }
        "probe" => {
            let (html, dir) = resolve_input(&a.input)?;
            let fps = a.fps.unwrap_or_else(|| default_fps(&html));
            let p = probe(&a, &html, &dir, fps)?;
            println!("{}", serde_json::to_string_pretty(&probe_json(&p, fps)).unwrap());
            Ok(0)
        }
        "lint" => lint(&a),
        "snapshot" => snapshot(&a),
        "render" => render(&a),
        "measure" => measure(&a),
        "breakdown" => breakdown(&a),
        "previz" => previz(&a),
        other => Err(format!("unknown command {other}\n\n{USAGE}")),
    }
}

fn previz(a: &Args) -> Result<i32, String> {
    use raea::previz as pv;
    let sub = a.format.clone().ok_or("previz score <layout.json|-> | previz html <layout.json|-> [-o out.html]")?;
    let p = a.input.clone().ok_or("previz needs a layout file (or -)")?;
    let text = if p.as_os_str() == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).map_err(|e| e.to_string())?;
        s
    } else {
        std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?
    };
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
    let layouts: Vec<pv::Layout> = match v {
        Value::Array(items) => items.into_iter().map(serde_json::from_value).collect::<Result<_, _>>(),
        one => serde_json::from_value(one).map(|l| vec![l]),
    }
    .map_err(|e| format!("{}: {e}", p.display()))?;
    if layouts.is_empty() {
        return Err(format!("{}: no layouts", p.display()));
    }
    match sub.as_str() {
        "score" => {
            let ranked = pv::rank(&layouts);
            if a.json {
                let rows: Vec<Value> = ranked.iter().map(|(i, r)| { let mut j = r.to_json(); j["index"] = json!(i); j }).collect();
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                for (i, r) in &ranked {
                    let soft: Vec<String> = r.soft.iter().map(|(k, v)| format!("{k} {v:.2}")).collect();
                    println!("#{i} {} total {:.2} ({})", if r.pass { "pass" } else { "FAIL" }, r.total, soft.join(", "));
                    for f in &r.fails {
                        println!("    {f}");
                    }
                }
            }
            Ok(if ranked.first().is_some_and(|r| r.1.pass) { 0 } else { 1 })
        }
        "html" => {
            let best = &layouts[pv::rank(&layouts)[0].0];
            let html = pv::to_html(best);
            match &a.output {
                Some(o) => std::fs::write(o, html).map_err(|e| format!("{o}: {e}"))?,
                None => print!("{html}"),
            }
            Ok(0)
        }
        other => Err(format!("unknown previz command {other}")),
    }
}

fn lint(a: &Args) -> Result<i32, String> {
    let (html, dir) = resolve_input(&a.input)?;
    let fps = a.fps.unwrap_or_else(|| default_fps(&html));
    let p = probe(a, &html, &dir, fps)?;
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = p.comp.warnings.clone();
    if p.comp.root_comp_id.is_none() {
        errors.push("no element with data-composition-id: the root composition is missing".into());
    }
    if p.comp.duration <= 0.0 {
        errors.push("composition duration is unknown: set data-duration on the root or register a timeline on window.__timelines".into());
    }
    if p.comp.authored_duration.is_none() && p.comp.timeline_duration > 0.0 {
        warnings.push(format!("root has no data-duration; using the root timeline's {:.3}s", p.comp.timeline_duration));
    }
    for e in &p.comp.js_errors {
        errors.push(format!("script error: {e}"));
    }
    errors.extend(p.comp.structure_errors());
    for v in &p.videos {
        if !v.src.contains("://") && !Path::new(&v.src).exists() {
            errors.push(format!("video {} source not found: {}", v.id, v.src));
        }
    }
    for au in &p.audio {
        if !au.src.contains("://") && !Path::new(&au.src).exists() {
            errors.push(format!("audio {} source not found: {}", au.id, au.src));
        }
    }
    let mut unsupported = p.comp.unsupported.clone();
    // static scan for features the renderer can't paint
    let mut sources = vec![html.clone()];
    if let Ok(rd) = std::fs::read_dir(dir.join("compositions")) {
        sources.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "html")));
    }
    // WebGL renders through ANGLE when this install has it (src/webgl.rs)
    let no_gl: &[(&str, &str)] = &[
        ("webgl", "WebGL canvases are not rendered: ANGLE was not found (RAEA_ANGLE_DIR)"),
        ("THREE.", "three.js / WebGL scenes are not rendered: ANGLE was not found (RAEA_ANGLE_DIR)"),
    ];
    let checks: &[(&str, &str)] = if raea::webgl_available() { &[] } else { no_gl };
    for f in &sources {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        // data-fx effects: unknown names and bad arguments, with how to fix them (effects/catalog.json)
        for e in raea::effects::lint_sources([text.as_str()]) {
            if !errors.contains(&e) {
                errors.push(e);
            }
        }
        for w in raea::effects::lint_warnings([text.as_str()]) {
            if !warnings.contains(&w) {
                warnings.push(w);
            }
        }
        for (needle, msg) in checks {
            if text.contains(needle) {
                let m = msg.to_string();
                if !unsupported.contains(&m) {
                    unsupported.push(m);
                }
            }
        }
    }
    // --layout: render frames (each --at, else every half second, at most 40) and check the text against its
    // components; each finding's region is cropped (24 px around it) beside the output so it can be looked at
    let mut crops: Vec<String> = Vec::new();
    let (pw, ph, pd) = (p.comp.width, p.comp.height, p.comp.duration);
    if a.layout && errors.is_empty() {
        let fpsf = fps.0 as f64 / fps.1 as f64;
        let frames = video_frames_for(a, &p, fpsf, true)?;
        let mut comp = if p.videos.is_empty() { p.comp } else { load(a, &html, &dir, fps, frames)? };
        let times = if a.at.is_empty() { raea::textfit::default_times(comp.duration) } else { a.at.clone() };
        let crop_dir = a.output.clone().map(PathBuf::from).unwrap_or_else(|| dir.join("lint-crops"));
        let tf = raea::textfit::check(&mut raea::stage::Stage::new(comp), &times, Some(&crop_dir))?;
        let strs = |k: &str| tf[k].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>();
        errors.extend(strs("errors"));
        warnings.extend(strs("warnings"));
        crops = strs("crops");
    }
    if a.json {
        println!("{}", serde_json::to_string_pretty(&json!({"ok": errors.is_empty(), "errors": errors, "warnings": warnings, "unsupported": unsupported, "crops": crops})).unwrap());
    } else {
        for e in &errors {
            eprintln!("error: {e}");
        }
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        for u in &unsupported {
            eprintln!("unsupported: {u}");
        }
        for c in &crops {
            eprintln!("crop: {c}");
        }
        if errors.is_empty() {
            eprintln!("ok: {} ({}x{}, {:.2}s)", html.display(), pw, ph, pd);
        }
    }
    Ok(if errors.is_empty() { 0 } else { 1 })
}

fn load_norms(a: &Args) -> Option<measure::Norms> {
    let mut dirs: Vec<PathBuf> = a.norms.iter().cloned().chain(raea::env_os("NORMS").map(PathBuf::from)).collect();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.extend(cwd.ancestors().map(|d| d.join("knowledge")));
    }
    dirs.into_iter().find_map(|d| {
        let profile = std::fs::read_to_string(d.join("editing-profile.json")).ok()?;
        let styles = std::fs::read_to_string(d.join("ugc-edit-styles.json")).unwrap_or_else(|_| "{}".into());
        measure::Norms::from_json(&profile, &styles).ok()
    })
}


struct Timeline {
    card: measure::Card,
    /// the main picture's stretches: (start, end, a take's video?)
    top: Vec<(f64, f64, bool)>,
    /// the spoken words at their edit times (from each take's words.json)
    words: Vec<raea::breakdown::Word>,
    music_bed: bool,
}

/// An edit's timeline as a card: cuts where the top video clip changes, text as the renderer lays it out each second,
/// a face wherever one of the creator's takes is on top; plus the stretches, the words and the music.
fn timeline(a: &Args, norms: Option<&measure::Norms>) -> Result<Timeline, String> {
    let (html, dir) = resolve_input(&a.input)?;
    let fps = a.fps.unwrap_or_else(|| default_fps(&html));
    let mut p = probe(a, &html, &dir, fps)?;
    let d = p.comp.duration;
    let clips: Vec<(f64, f64)> = p.videos.iter().map(|v| (v.start, if v.end.is_finite() { v.end } else { d })).collect();
    let top_at = |t: f64| p.videos.iter().enumerate().filter(|(_, v)| v.start <= t && t < if v.end.is_finite() { v.end } else { d }).max_by_key(|(i, _)| *i).map(|(i, v)| (i, v.src.contains("/takes/")));
    let mut buf = Vec::new();
    let face_guess = norms.and_then(|n| n.profile["groups"]["all"]["face_size"]["p50"].as_f64()).unwrap_or(0.05);
    // each second's frame is a run of Bevy's schedule (stage.rs)
    let (comp_w, comp_h) = (p.comp.width, p.comp.height);
    let mut stage = raea::stage::Stage::new(p.comp);
    let seconds: Vec<measure::Second> = (0..(d.ceil() as u32).min(180))
        .map(|t| {
            let at = (t as f64 + 0.5).min(d - 0.01).max(0.0);
            let mut s = measure::second_from_boxes(t, &stage.text_boxes(at, &mut buf, None));
            s.face = if top_at(at).map(|x| x.1).unwrap_or(false) { face_guess } else { 0.0 };
            s
        })
        .collect();
    let cuts = measure::timeline_cuts(&clips);
    let mut edges: Vec<f64> = std::iter::once(0.0).chain(cuts.iter().copied()).chain(std::iter::once(d)).collect();
    edges.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
    let top: Vec<(f64, f64, bool)> = edges.windows(2).map(|w| (w[0], w[1], top_at((w[0] + w[1]) / 2.0).map(|x| x.1).unwrap_or(false))).collect();
    // words: each take sound clip's words, moved to edit time
    let mut words: Vec<raea::breakdown::Word> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let sources = p.audio.iter().map(|x| (x.src.clone(), x.start, x.end, x.media_start))
        .chain(p.videos.iter().filter(|v| v.has_audio).map(|v| (v.src.clone(), v.start, v.end, v.media_start)));
    for (src, start, end, ms) in sources {
        if !src.contains("/takes/") || !seen.insert(((start * 1000.0) as i64, (ms * 1000.0) as i64)) {
            continue;
        }
        let Some(file) = Path::new(&src).parent().map(|d| d.join("words.json")) else { continue };
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let Ok(list) = serde_json::from_str::<Vec<Value>>(&text) else { continue };
        let end = if end.is_finite() { end } else { d };
        for w in list {
            let (Some(s0), Some(e0)) = (w["start_ms"].as_f64(), w["end_ms"].as_f64()) else { continue };
            let (s0, e0) = (s0 / 1000.0, e0 / 1000.0);
            if s0 >= ms && s0 < ms + (end - start) {
                words.push(raea::breakdown::Word { w: w["text"].as_str().unwrap_or("").to_string(), s: start + s0 - ms, e: start + e0 - ms });
            }
        }
    }
    words.sort_by(|x, y| x.s.partial_cmp(&y.s).unwrap());
    let music_bed = p.audio.iter().any(|x| !x.src.contains("/takes/"));
    let card = measure::Card {
        source: "timeline".into(),
        duration: d,
        w: Some(comp_w as f64),
        h: Some(comp_h as f64),
        cuts,
        has_audio: !p.audio.is_empty() || p.videos.iter().any(|v| v.has_audio),
        seconds,
        faces_known: true,
        ..Default::default()
    };
    Ok(Timeline { card, top, words, music_bed })
}

fn breakdown(a: &Args) -> Result<i32, String> {
    use raea::breakdown as bd;
    let sub = a.format.clone().ok_or("breakdown parse <file> | breakdown from <edit.html> | breakdown check <video|edit.html> --breakdown <file> [--words <file>] [--tol S]")?;
    let read = |p: &PathBuf| -> Result<String, String> {
        if p.as_os_str() == "-" { let mut s = String::new(); std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).map_err(|e| e.to_string())?; Ok(s) }
        else { std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display())) }
    };
    match sub.as_str() {
        "parse" => {
            let b = bd::parse(&read(a.input.as_ref().ok_or("parse needs a file (or -)")?)?);
            let (ok, all) = b.parse_rate();
            println!("{}", serde_json::to_string(&json!({"parsed": ok, "total_lines": all, "beats": b.beats.len(), "breakdown": b})).unwrap());
            Ok(0)
        }
        "from" => {
            let input = a.input.clone().ok_or("from needs an edit (.html or a project folder)")?;
            let t = timeline(a, load_norms(a).as_ref())?;
            let title = input.file_stem().map(|s| s.to_string_lossy().trim_end_matches(".w").to_string()).unwrap_or_default();
            print!("{}", bd::from_timeline(&title, &t.card, &t.top, &t.words, t.music_bed));
            Ok(0)
        }
        "check" => {
            let b = bd::parse(&read(a.breakdown.as_ref().ok_or("check needs --breakdown <file>")?)?);
            let input = a.input.clone().unwrap_or_default();
            let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            // --card: a stored measurement (the index's measure JSON) instead of measuring again
            let (card, words) = if let Some(cp) = &a.card {
                (serde_json::from_str::<measure::Card>(&read(cp)?).map_err(|e| format!("--card: {e}"))?, Vec::new())
            } else if a.input.is_none() {
                return Err("check needs a video, an edit, or --card".into());
            } else if matches!(ext.as_str(), "mp4" | "mov" | "m4v" | "webm" | "mkv") {
                let vision = a.vision.clone().or_else(|| raea::env_os("VISION").map(PathBuf::from)).filter(|p| p.exists());
                let ff = media::ffmpeg_path().ok_or("checking a video needs ffmpeg")?;
                (measure::from_video(&input, &ff, vision.as_deref(), None)?, Vec::new())
            } else {
                let t = timeline(a, load_norms(a).as_ref())?;
                (t.card, t.words)
            };
            let words = match &a.words {
                Some(p) => serde_json::from_str::<Vec<bd::Word>>(&read(p)?).map_err(|e| format!("--words: {e}"))?,
                None => words,
            };
            let r = bd::check(&b, &card, &words, a.tol.unwrap_or(0.15));
            if a.json {
                println!("{}", serde_json::to_string_pretty(&r).unwrap());
            } else {
                for l in r["lines"].as_array().unwrap() {
                    let mark = match l["ok"].as_bool() { Some(true) => "pass", Some(false) => "FAIL", None => "  - " };
                    println!("{mark}  {}  ({})", l["line"].as_str().unwrap_or(""), l["why"].as_str().unwrap_or(""));
                }
                println!("{} pass, {} fail, {} unchecked; cuts precision {} recall {}", r["pass"], r["fail"], r["unchecked"], r["cuts"]["precision"], r["cuts"]["recall"]);
            }
            Ok(if r["fail"].as_u64().unwrap_or(0) == 0 { 0 } else { 1 })
        }
        "merge" => {
            // measured shot changes merged into a perceived breakdown: --breakdown, --card (a stored measure), --words
            let b = bd::parse(&read(a.breakdown.as_ref().ok_or("merge needs --breakdown <file>")?)?);
            let card = serde_json::from_str::<measure::Card>(&read(a.card.as_ref().ok_or("merge needs --card <file>")?)?).map_err(|e| format!("--card: {e}"))?;
            let words = match &a.words { Some(p) => serde_json::from_str::<Vec<bd::Word>>(&read(p)?).map_err(|e| format!("--words: {e}"))?, None => Vec::new() };
            print!("{}", bd::merge(&b, &card, &words, a.tol.unwrap_or(0.5)));
            Ok(0)
        }
        other => Err(format!("unknown breakdown command {other}")),
    }
}

fn measure(a: &Args) -> Result<i32, String> {
    let norms = load_norms(a);
    if let Some(rows) = &a.cards {
        let text = std::fs::read_to_string(rows).map_err(|e| format!("{}: {e}", rows.display()))?;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let row: Value = serde_json::from_str(line).map_err(|e| format!("{}: {e}", rows.display()))?;
            let Ok(card) = serde_json::from_value::<measure::Card>(row.clone()) else { continue };
            println!("{}", json!({"id": row["id"], "summary": card.summary(), "vector": card.vector(),
                                   "style": norms.as_ref().and_then(|n| n.style(&card))}));
        }
        return Ok(0);
    }
    let input = a.input.clone().ok_or("measure needs a video, a project folder or an edit's .html")?;
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let vision = a.vision.clone().or_else(|| raea::env_os("VISION").map(PathBuf::from)).filter(|p| p.exists());
    // The creator's performance (T-332): the app's Perception.swift as a Mac command, when given.
    let perceive = a.perceive.clone().or_else(|| raea::env_os("PERCEIVE").map(PathBuf::from)).filter(|p| p.exists());
    let ffmpeg = || media::ffmpeg_path().ok_or_else(|| "measuring a video needs ffmpeg (PATH or $RAEA_FFMPEG)".to_string());
    let mut card = if matches!(ext.as_str(), "mp4" | "mov" | "m4v" | "webm" | "mkv") {
        measure::from_video(&input, &ffmpeg()?, vision.as_deref(), a.keyframes.as_deref())?
    } else {
        let t = timeline(a, norms.as_ref())?;
        let mut card = t.card;
        if let Some(render) = &a.video {
            // faces and loudness come from the render; cuts and text stay the timeline's (exact)
            let r = measure::from_video(render, &ffmpeg()?, vision.as_deref(), a.keyframes.as_deref())?;
            (card.lufs, card.lra, card.silence_s, card.has_audio) = (r.lufs, r.lra, r.silence_s, r.has_audio);
            if r.faces_known {
                for s in &mut card.seconds {
                    s.face = r.seconds.iter().find(|x| x.t == s.t).map(|x| x.face).unwrap_or(0.0);
                }
                card.faces_known = true;
            }
        }
        card
    };
    if let Some(p) = &perceive {
        let video = if matches!(ext.as_str(), "mp4" | "mov" | "m4v" | "webm" | "mkv") { Some(&input) } else { a.video.as_ref() };
        if let Some(v) = video {
            card.performance = measure::performance(v, p).map_err(|e| eprintln!("performance: {e}")).ok();
        }
    }
    let notes = norms.as_ref().map(|n| n.notes(&card)).unwrap_or_default();
    let style = norms.as_ref().and_then(|n| n.style(&card));
    let summary = card.summary();
    if a.json {
        println!("{}", serde_json::to_string_pretty(&json!({"card": card, "summary": summary, "vector": card.vector(), "notes": notes, "style": style,
                                                              "norms": norms.is_some()})).unwrap());
    } else {
        let f = |k: &str| summary.get(k).copied().flatten().map(|v| format!("{v:.2}")).unwrap_or_else(|| "-".into());
        println!("{} ({}, {:.1}s, {})", input.display(), card.source, card.duration, card.shape());
        println!("  cuts/10s {}  first cut {}s  median shot {}s  text {}  text y {}  face {}  LUFS {}",
                 f("cuts_per_10s"), f("first_cut_s"), f("median_shot_s"), f("text_share"), f("text_center_y"), f("face_share"), f("lufs"));
        if let Some(n) = style.as_ref().and_then(|s| s["nearest"].as_object().cloned()) {
            println!("  nearest style: {} (distance {})", n["describe"].as_str().unwrap_or("?"), n["distance"]);
        }
        for n in &notes {
            println!("  note: {}", n["note"].as_str().unwrap_or(""));
        }
        if norms.is_none() {
            println!("  (no norms found: pass --norms DIR)");
        }
    }
    Ok(0)
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    image::save_buffer(path, &straight_alpha(rgba), w, h, image::ColorType::Rgba8).map_err(|e| format!("{}: {e}", path.display()))
}

fn video_frames_for(a: &Args, p: &Probe, fps: f64, quiet: bool) -> Result<Arc<media::VideoFrames>, String> {
    if p.videos.is_empty() {
        return Ok(Arc::new(media::VideoFrames { fps, ..Default::default() }));
    }
    if let Some(dir) = &a.video_frames {
        return Ok(Arc::new(media::VideoFrames::from_dir(dir, fps, &p.videos)));
    }
    let cache = p.comp.net.cache_dir.clone().unwrap_or_else(std::env::temp_dir).join("video");
    let frames = media::VideoFrames::extract(&cache, fps, &p.videos, &p.comp.net, &|m| log(quiet, m))?;
    Ok(Arc::new(frames))
}

fn snapshot(a: &Args) -> Result<i32, String> {
    let (html, dir) = resolve_input(&a.input)?;
    let fps = a.fps.unwrap_or_else(|| default_fps(&html));
    let fpsf = fps.0 as f64 / fps.1 as f64;
    let p = probe(a, &html, &dir, fps)?;
    let frames = video_frames_for(a, &p, fpsf, a.quiet)?;
    let comp = if p.videos.is_empty() { p.comp } else { load(a, &html, &dir, fps, frames)? };
    // Bevy's schedule is the frame loop (stage.rs, docs/bevy.feature B2); what isn't a frame goes through stage.comp()
    let mut stage = raea::stage::Stage::new(comp);
    let times = if a.at.is_empty() { vec![0.0] } else { a.at.clone() };
    let out = a.output.clone().unwrap_or_else(|| "snapshot.png".into());
    let (w, h) = stage.comp().output_size();
    let mut buf = Vec::new();
    for (i, t) in times.iter().enumerate() {
        if let Ok(code) = raea::env_var("DEBUG_PRE_EVAL") {
            stage.comp().seek(*t);
            stage.comp().doc.eval(&code);
        }
        let boxes = if a.boxes {
            // The HTML composition format/UGC 9:16 safe zone unless given
            let safe = a.safe_zone.or(((stage.comp().width, stage.comp().height) == (1080, 1920)).then_some([220.0, 140.0, 420.0, 0.0]));
            Some(stage.text_boxes(*t, &mut buf, safe))
        } else {
            stage.frame(*t, &mut buf);
            None
        };
        if let Ok(code) = raea::env_var("DEBUG_EVAL") {
            stage.comp().doc.eval(&code);
            for m in stage.comp().doc.take_messages() {
                eprintln!("[eval] {m}");
            }
            for e in stage.comp().doc.take_js_errors() {
                eprintln!("[eval error] {e}");
            }
        }
        let path = if times.len() == 1 && out.ends_with(".png") {
            PathBuf::from(&out)
        } else {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            Path::new(&out).join(format!("frame-{i:03}-{t:.3}s.png"))
        };
        write_png(&path, w, h, &buf)?;
        if let Some(b) = boxes {
            let jp = path.with_extension("boxes.json");
            std::fs::write(&jp, serde_json::to_string_pretty(&b).unwrap()).map_err(|e| format!("{}: {e}", jp.display()))?;
        }
        println!("{}", path.display());
    }
    for w in stage.comp().drain_warnings() {
        log(a.quiet, &format!("warning: {w}"));
    }
    for e in &stage.comp().js_errors {
        log(a.quiet, &format!("script error: {e}"));
    }
    Ok(0)
}

/// Rendered frames are premultiplied (api.rs); PNG and ffmpeg's `rgba` input hold straight alpha, so
/// translucent pixels (shadows, glows, soft edges) are divided back out before they're written.
fn straight_alpha(rgba: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if rgba.chunks_exact(4).all(|p| p[3] == 255 || p[3] == 0) {
        return std::borrow::Cow::Borrowed(rgba);
    }
    let mut out = rgba.to_vec();
    for p in out.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    std::borrow::Cow::Owned(out)
}

fn encoder_args(format: &str, quality: &str, crf: Option<u32>, transparent: bool) -> Vec<String> {
    let (preset, default_crf) = match quality {
        "draft" => ("ultrafast", 28),
        "high" => ("slow", 15),
        _ => ("medium", 18),
    };
    let crf = crf.unwrap_or(default_crf).to_string();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match format {
        "mov" => {
            let mut v = s(&["-c:v", "prores_ks", "-profile:v", "4444", "-vendor", "apl0"]);
            v.extend(s(&["-pix_fmt", if transparent { "yuva444p10le" } else { "yuv444p10le" }]));
            v
        }
        "webm" => {
            let mut v = s(&["-c:v", "libvpx-vp9", "-b:v", "0", "-crf"]);
            v.push(if crf == "18" { "30".into() } else { crf });
            v.extend(s(&["-row-mt", "1", "-pix_fmt", if transparent { "yuva420p" } else { "yuv420p" }]));
            v
        }
        _ => {
            let mut v = s(&["-c:v", "libx264", "-preset", preset, "-crf"]);
            v.push(crf);
            v.extend(s(&[
                "-vf", "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p",
                "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709",
                "-movflags", "+faststart",
            ]));
            v
        }
    }
}

fn render(a: &Args) -> Result<i32, String> {
    let t0 = Instant::now();
    // video: text a script moves is drawn from outlines, so it moves smoothly (stills keep Chrome's masks)
    anyrender::moving_text::enable(true);
    let (html, dir) = resolve_input(&a.input)?;
    let fps = a.fps.unwrap_or_else(|| default_fps(&html));
    let fpsf = fps.0 as f64 / fps.1 as f64;
    let format = a.format.clone().unwrap_or_else(|| {
        match a.output.as_deref().and_then(|o| Path::new(o).extension()).and_then(|e| e.to_str()) {
            Some("mov") => "mov".into(),
            Some("webm") => "webm".into(),
            Some("rgba") | Some("raw") => "rgba".into(),
            None if a.output.as_deref() == Some("-") => "rgba".into(),
            _ => "mp4".into(),
        }
    });
    let output = a.output.clone().ok_or("render needs -o <output>")?;
    let quiet = a.quiet;

    let p = probe(a, &html, &dir, fps)?;
    for w in &p.comp.warnings {
        log(quiet, &format!("warning: {w}"));
    }
    for e in &p.comp.js_errors {
        log(quiet, &format!("script error: {e}"));
    }
    for u in &p.comp.unsupported {
        log(quiet, &format!("unsupported: {u}"));
    }
    let duration = p.comp.duration;
    if duration <= 0.0 {
        return Err("composition duration is unknown (no data-duration on the root and no root timeline)".into());
    }
    let total = frame_count(duration, fpsf);
    let first = a.start.map(|s| (s * fpsf).round() as u64).unwrap_or(0).min(total);
    let last = a.end.map(|e| frame_count(e.min(duration), fpsf)).unwrap_or(total).max(first);
    let n = last - first;
    let (w, h) = p.comp.output_size();
    let frames_src = video_frames_for(a, &p, fpsf, quiet)?;
    let audio = p.audio.clone();
    drop(p);

    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let workers = a.workers.unwrap_or((cores / 2).clamp(1, 8)).clamp(1, 32).min(n.max(1) as usize);
    log(quiet, &format!("rendering {n} frames ({w}x{h} @ {}/{} fps, {:.2}s) with {workers} worker(s)", fps.0, fps.1, n as f64 / fpsf));

    // Sink
    enum Sink {
        Writer(Box<dyn Write>),
        Ffmpeg(std::process::Child),
        Pngs(PathBuf),
    }
    let mut sink = match format.as_str() {
        "rgba" => {
            if output == "-" {
                Sink::Writer(Box::new(std::io::BufWriter::with_capacity(1 << 20, std::io::stdout().lock())))
            } else {
                Sink::Writer(Box::new(std::io::BufWriter::new(std::fs::File::create(&output).map_err(|e| format!("{output}: {e}"))?)))
            }
        }
        "png-sequence" | "png" => {
            std::fs::create_dir_all(&output).map_err(|e| format!("{output}: {e}"))?;
            Sink::Pngs(PathBuf::from(&output))
        }
        "mp4" | "mov" | "webm" => {
            let ffmpeg = media::ffmpeg_path().ok_or("ffmpeg not found (needed for mp4/mov/webm; use --format rgba or png-sequence to encode yourself)")?;
            let mut cmd = Command::new(ffmpeg);
            cmd.args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
                .arg(format!("{w}x{h}"))
                .arg("-framerate")
                .arg(format!("{}/{}", fps.0, fps.1))
                .args(["-i", "-"]);
            let part_start = first as f64 / fpsf;
            let part_len = n as f64 / fpsf;
            let audio_in_range: Vec<media::AudioClip> = audio
                .iter()
                .filter(|c| c.end > part_start && c.start < part_start + part_len)
                .map(|c| {
                    let mut c = c.clone();
                    // shift into the rendered range
                    let cut = (part_start - c.start).max(0.0);
                    c.media_start += cut * c.playback_rate;
                    c.start = (c.start - part_start).max(0.0);
                    c.end -= part_start;
                    c
                })
                .collect();
            let has_audio = !audio_in_range.is_empty();
            if has_audio {
                let (inputs, graph) = media::audio_filter(&audio_in_range, 1, part_len);
                cmd.args(&inputs).args(["-filter_complex", &graph, "-map", "0:v", "-map", "[aout]"]);
            }
            cmd.args(encoder_args(&format, &a.quality, a.crf, a.transparent));
            if has_audio {
                if format == "webm" {
                    cmd.args(["-c:a", "libopus", "-b:a", "192k"]);
                } else if format == "mov" {
                    cmd.args(["-c:a", "pcm_s16le"]);
                } else {
                    cmd.args(["-c:a", "aac", "-b:a", "192k"]);
                }
                cmd.arg("-shortest");
            }
            cmd.arg(&output);
            cmd.stdin(Stdio::piped());
            Sink::Ffmpeg(cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?)
        }
        other => return Err(format!("unknown format {other}")),
    };

    // Workers render interleaved frames (k, k+N, ...) and hand them over in order.
    let mut receivers = Vec::new();
    let mut handles = Vec::new();
    let png_dir = if let Sink::Pngs(d) = &sink { Some(d.clone()) } else { None };
    for k in 0..workers {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Result<Vec<u8>, String>>(3);
        receivers.push(rx);
        let html = html.clone();
        let dir = dir.clone();
        let frames_src = frames_src.clone();
        let args = Args {
            command: String::new(),
            input: None,
            output: None,
            format: None,
            fps: None,
            scale: a.scale,
            width: a.width,
            height: a.height,
            start: None,
            end: None,
            workers: None,
            quality: String::new(),
            crf: None,
            variables: a.variables.clone(),
            video_frames: None,
            network: a.network,
            transparent: a.transparent,
            quiet: true,
            json: false,
            at: vec![],
            backend: a.backend,
            profile: scene::Profile::default(),
            clock: a.clock,
            boxes: false,
            layout: false,
            safe_zone: None,
            video: None,
            vision: None,
            perceive: None,
            keyframes: None,
            norms: None,
            cards: None,
            breakdown: None,
            words: None,
            tol: None,
            card: None,
        };
        let png_dir = png_dir.clone();
        handles.push(
            std::thread::Builder::new()
                .name(format!("render-{k}"))
                .stack_size(64 << 20)
                .spawn(move || {
                    let mut comp = match load(&args, &html, &dir, fps, frames_src) {
                        Ok(c) => c,
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            return;
                        }
                    };
                    // Bevy's schedule is the frame loop (stage.rs, docs/bevy.feature B2)
                    let mut stage = raea::stage::Stage::new(comp);
                    let mut i = first + k as u64;
                    while i < last {
                        let t = i as f64 * fps.1 as f64 / fps.0 as f64;
                        let mut buf = Vec::new();
                        stage.frame(t, &mut buf);
                        if let Some(d) = &png_dir {
                            let r = write_png(&d.join(format!("frame_{:06}.png", i)), w, h, &buf).map(|_| Vec::new());
                            if tx.send(r).is_err() {
                                return;
                            }
                        } else if tx.send(Ok(buf)).is_err() {
                            return;
                        }
                        i += workers as u64;
                    }
                })
                .map_err(|e| e.to_string())?,
        );
    }

    let mut done = 0u64;
    let mut last_report = Instant::now();
    let mut failure: Option<String> = None;
    for i in 0..n {
        let rx = &receivers[(i % workers as u64) as usize];
        let frame = match rx.recv() {
            Ok(Ok(f)) => f,
            Ok(Err(e)) => {
                failure = Some(e);
                break;
            }
            Err(_) => {
                failure = Some(format!("render worker stopped at frame {}", first + i));
                break;
            }
        };
        let write = match &mut sink {
            Sink::Writer(wr) => wr.write_all(&frame).map_err(|e| e.to_string()),
            Sink::Ffmpeg(child) => {
                let frame = if a.transparent { straight_alpha(&frame) } else { std::borrow::Cow::Borrowed(&frame[..]) };
                child.stdin.as_mut().unwrap().write_all(&frame).map_err(|e| format!("ffmpeg stdin: {e}"))
            }
            Sink::Pngs(_) => Ok(()),
        };
        if let Err(e) = write {
            failure = Some(e);
            break;
        }
        done += 1;
        if !quiet && (last_report.elapsed().as_millis() > 1000 || done == n) {
            last_report = Instant::now();
            let el = t0.elapsed().as_secs_f64();
            eprintln!("  {done}/{n} frames  {:.1} fps", done as f64 / el.max(1e-3));
        }
    }
    drop(receivers);
    for hnd in handles {
        let _ = hnd.join();
    }
    match sink {
        Sink::Writer(mut wr) => wr.flush().map_err(|e| e.to_string())?,
        Sink::Ffmpeg(mut child) => {
            drop(child.stdin.take());
            let status = child.wait().map_err(|e| e.to_string())?;
            if !status.success() && failure.is_none() {
                failure = Some(format!("ffmpeg exited with {status}"));
            }
        }
        Sink::Pngs(_) => {}
    }
    if let Some(f) = failure {
        return Err(f);
    }
    log(quiet, &format!("wrote {output} in {:.1}s", t0.elapsed().as_secs_f64()));
    Ok(0)
}
