//! The host's side of `raea` (@raea/cli, ~/raea/apps/cli): the ops a TypeScript project asks of the machine it runs on, over
//! shell-serve's channel (sdk/shell/src/serve.ts: {"host": {op, args}} out, {"host_reply"} or {"host_error"} back). What
//! needs perception, ffmpeg, the renderer or a model lives here; the project's files are written by the CLI. Each op
//! answers text a person would read, or JSON the CLI turns into files.

use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const VIDEO: [&str; 4] = ["mp4", "mov", "m4v", "webm"];

pub async fn handle(root: &Path, op: &str, args: &Value) -> anyhow::Result<Value> {
    match op {
        "shape" => Ok(json!(shape(root))),
        "cut" => cut(root, args).await,
        "frame" => frame(root, args).await.map(Value::String),
        "media" => media(root).await.map(Value::String),
        "render" => render_saved(root, &Piece::of(root, args)).await.map(Value::String),
        "check" => check(root, &Piece::of(root, args), args["full"].as_bool() == Some(true)).await.map(Value::String),
        "still" => still(root, args).await.map(Value::String),
        "sheet" => sheet(root).await.map(Value::String),
        "script" => script(root, args).await.map(Value::String),
        // raea sao, tokensynth and sounds: raea-motion's sound tools, run for this object
        "audio" => audio(root, args).await.map(Value::String),
        // the skills on the computer (skills.rs): the list, one whole, or a file it brings
        "skill" => {
            let project = crate::workspace::current().map(|w| w.root);
            let p = project.as_deref();
            match args["name"].as_str().filter(|n| !n.trim().is_empty()) {
                None => {
                    let all = crate::skills::skills(p);
                    let cmds = crate::skills::commands(p);
                    Ok(Value::String(format!("{}{}",
                        if all.is_empty() { "no skills on this computer\n".to_string() } else { all.iter().map(|s| format!("{}: {}\n", s.name, s.description.chars().take(160).collect::<String>())).collect::<String>() },
                        if cmds.is_empty() { String::new() } else { format!("\nthe person's commands: {}\n", cmds.iter().map(|c| format!("/{}", c.name)).collect::<Vec<_>>().join(", ")) })))
                }
                Some(n) => {
                    let s = crate::skills::skill(p, n).ok_or_else(|| anyhow!("no skill \"{n}\" (raea skills lists them)"))?;
                    match args["file"].as_str().filter(|f| !f.is_empty()) {
                        Some(f) => crate::skills::skill_file(&s, f).map(Value::String).ok_or_else(|| anyhow!("{} has no file {f}", s.name)),
                        None => {
                            let (text, files) = crate::skills::skill_text(&s);
                            Ok(Value::String(format!("{}{}", text.chars().take(60_000).collect::<String>(), if files.is_empty() { String::new() } else { format!("\n\n(files: {})", files.join(", ")) })))
                        }
                    }
                }
            }
        }
        // the project's record (@raea/data/project.ts): tasks, questions, notes, objects and each object's versions
        "project" => {
            let argv: Vec<String> = args["argv"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)).collect();
            let (text, code) = project(root, &argv, args["json"].as_bool() == Some(true)).await;
            Ok(json!({"text": text, "code": code}))
        }
        // the craft library (prompts/craft: stages, modes, playbooks), read on demand: the model keeps two tools
        "craft" => Ok(Value::String(match args["name"].as_str().map(|n| n.trim().trim_end_matches(".md")).filter(|n| !n.is_empty()) {
            Some(n) => crate::brain::craft_file(n).ok_or_else(|| anyhow!("no craft \"{n}\" (raea craft lists them)"))?,
            None => format!("the craft library (raea craft NAME):\n{}", crate::brain::CRAFT.iter().map(|c| format!("  {c}")).collect::<Vec<_>>().join("\n")),
        })),
        // the brand's kit (the host app's brands API: colours, type, logos into media/brand, brand/brand.css), as void's tool
        "brand" => Box::pin(crate::brain::run_tool(root, "brand", &json!({"brand": args["brand"]}))).await.map(Value::String),
        other => Err(anyhow!("no host op \"{other}\"")),
    }
}

/// The project an object's folder belongs to: the open workspace when the folder is in it; else the folder holding its
/// objects/ or spaces/ folder (an eval's home); else the folder itself (`void tool` on a lone folder), its one object.
pub(crate) fn project_root(object: &Path) -> PathBuf {
    match crate::workspace::current() {
        Some(ws) if object.starts_with(&ws.root) && object != ws.root => ws.root,
        _ => match object.parent() {
            Some(p) if p.file_name().is_some_and(|n| n == "objects" || n == "spaces") && object.join("space.json").exists() => p.parent().map(Path::to_path_buf).unwrap_or_else(|| object.to_path_buf()),
            _ => object.to_path_buf(),
        },
    }
}

/// @raea/data/src/project.ts CMD ROOT ARGS... for the object at `object` (RAEA_DATA overrides the file), run in process on
/// raea's JavaScriptCore with Node's core (raea-native js::run_node, as bun ran it): its text
/// (JSON with `json`) and exit code (0 done, 1 refused, 2 usage). A command about one object names this one unless it
/// names another.
pub(crate) async fn project(object: &Path, argv: &[String], json: bool) -> (String, i32) {
    let file = std::env::var_os("RAEA_DATA").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/data/src/project.ts"));
    let Some(cmd) = argv.first() else { return ("project: no command".into(), 2) };
    let root = project_root(object);
    let mut rest: Vec<String> = argv[1..].to_vec();
    let scoped = matches!(cmd.as_str(), "save" | "versions" | "diff" | "checkout" | "restore") || (cmd == "task" && rest.first().is_some_and(|s| s == "new"));
    if scoped && root != object && !rest.iter().any(|a| a == "--object") {
        rest.push("--object".into());
        rest.push(object.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    }
    if json {
        rest.push("--json".into());
    }
    let args: Vec<String> = [cmd.clone(), root.display().to_string()].into_iter().chain(rest).collect();
    let o = tokio::task::spawn_blocking(move || raea_native::js::run_node(&file, &args, String::new(), std::time::Duration::from_secs(120))).await;
    match o {
        Ok(o) => (format!("{}{}", o.stdout, o.stderr).trim_end().to_string(), o.exit),
        Err(e) => (format!("project: the run failed: {e}"), 1),
    }
}

/// A render kept as a version of its object (the founder, 2026-10-17: a version at every render), with the check's
/// results on it when it was checked. The line said after the render's.
async fn keep_version(root: &Path, piece: &Piece, check: Option<(&[(String, Vec<String>)], bool)>) -> String {
    let out = piece.out.strip_prefix(root).unwrap_or(&piece.out).to_string_lossy().into_owned();
    let mut argv = vec!["save".to_string(), "--render".into(), out];
    if let Some((families, full)) = check {
        // one row a fault, and a passing row for each family of checks that found none
        let rows: Vec<Value> = families.iter().flat_map(|(family, faults)| {
            if faults.is_empty() { vec![json!({"rule": family, "status": "pass"})] } else { faults.iter().map(|f| json!({"rule": f, "status": "fail"})).collect() }
        }).collect();
        let file = root.join(".score").join("check.json");
        let _ = std::fs::create_dir_all(root.join(".score"));
        if std::fs::write(&file, json!({"full": full, "results": rows}).to_string()).is_ok() {
            argv.extend(["--check".into(), ".score/check.json".into()]);
            if full {
                argv.push("--full".into());
            }
        }
    }
    let (text, code) = project(root, &argv, false).await;
    if code == 0 { text } else { format!("(not saved as a version: {text})") }
}

#[derive(Clone, Copy, serde::Serialize)]
pub struct Shape {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

/// The edit's size and rate: cuts.json's, else an old space's edit.rrl `edit {fps, width, height}` fact (the eval
/// fixtures' stubs still carry one: read as text, the rrl language is gone), else 1080x1920 at 30.
pub(crate) fn shape(root: &Path) -> Shape {
    if let Some(e) = std::fs::read_to_string(root.join("cuts.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).map(|v| v["edit"].clone()) {
        if let (Some(w), Some(h), Some(f)) = (e["width"].as_u64(), e["height"].as_u64(), e["fps"].as_f64()) {
            return Shape { width: w as u32, height: h as u32, fps: f };
        }
    }
    let fact = std::fs::read_to_string(root.join("edit.rrl")).ok()
        .and_then(|t| t.lines().map(str::trim).find(|l| l.starts_with("edit {")).map(String::from)).unwrap_or_default();
    let num = |k: &str| fact.split(&format!("{k}:")).nth(1).and_then(|v| v.split([',', '}']).next()?.trim().parse::<f64>().ok());
    Shape { width: num("width").unwrap_or(1080.0) as u32, height: num("height").unwrap_or(1920.0) as u32, fps: num("fps").filter(|f| *f > 0.0).unwrap_or(30.0) }
}

/// The project a path is in: the folder itself, or the folder a file of it (edit.tsx) sits in.
pub(crate) fn root_of(p: &Path) -> PathBuf {
    if p.is_dir() { p.to_path_buf() } else { p.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")) }
}

/// Every take under media/: its path from the project's root.
fn takes(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut walk = vec![root.join("engine/media")];
    while let Some(d) = walk.pop() {
        for p in std::fs::read_dir(&d).into_iter().flatten().flatten().map(|e| e.path()) {
            if p.is_dir() {
                walk.push(p);
            } else if p.extension().and_then(|e| e.to_str()).is_some_and(|e| VIDEO.contains(&e.to_lowercase().as_str())) {
                out.push(p.strip_prefix(root).unwrap_or(&p).to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

/// A take named by its path, file name, stem or folder.
fn take(root: &Path, name: &str) -> anyhow::Result<String> {
    let t = name.trim().trim_start_matches("./").trim_start_matches('/');
    let all = takes(root);
    // by path with or without its extension (media/take01 or media/take01.mov), then by name, stem or folder
    all.iter().find(|p| p.as_str() == t || Path::new(p.as_str()).with_extension("") == Path::new(t)).or_else(|| all.iter().find(|p| {
        let q = Path::new(p);
        q.file_name().is_some_and(|n| n == t) || q.file_stem().is_some_and(|n| n == t) || p.ends_with(&format!("/{t}")) || p.contains(&format!("{t}/"))
    })).cloned().ok_or_else(|| anyhow!("no take \"{name}\" (raea media lists them: {})", all.join(", ")))
}

fn words_file(media: &Path) -> Option<PathBuf> {
    [media.with_extension("words.json"), media.with_file_name("words.json")].into_iter()
        .chain(std::fs::read_link(media).ok().map(|r| r.with_file_name("words.json")))
        .find(|p| p.exists())
}

fn words(media: &Path) -> Vec<crate::eval::Word> {
    words_file(media).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<Vec<Value>>(&t).ok()).unwrap_or_default()
        .iter().filter_map(|w| Some((w["text"].as_str()?.trim().to_string(), w["start_ms"].as_f64()? / 1000.0, w["end_ms"].as_f64()? / 1000.0)))
        .filter(|w| !w.0.is_empty()).collect()
}

/// A clip's length in the edit's frames: the same rule as cuts.ts's `frames`.
fn frames(from: f64, to: f64, fps: f64) -> i64 {
    ((to - from) * fps).round().max(1.0) as i64
}

/// cuts.json's clips (take, file, from, to, line).
fn cuts_of(root: &Path) -> Vec<Value> {
    std::fs::read_to_string(root.join("cuts.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["cuts"].as_array().cloned()).unwrap_or_default()
}

/// The clips end to end in the edit's frames, and those with a transcript as cutcheck's spoken clips.
fn spans(root: &Path, cuts: &[Value], fps: f64) -> (Vec<(i64, i64)>, Vec<crate::eval::Spoken>) {
    let (mut at, mut spans, mut spoken) = (0i64, Vec::new(), Vec::new());
    for c in cuts {
        let (from, to) = (c["from"].as_f64().unwrap_or(0.0), c["to"].as_f64().unwrap_or(0.0));
        let n = frames(from, to, fps);
        spans.push((at, at + n));
        let media = root.join(c["file"].as_str().unwrap_or(""));
        let w = words(&media);
        if !w.is_empty() {
            spoken.push((std::sync::Arc::new(w), media, from, to, at, at + n));
        }
        at += n;
    }
    (spans, spoken)
}

async fn cut(root: &Path, args: &Value) -> anyhow::Result<Value> {
    let name = args["take"].as_str().ok_or_else(|| anyhow!("cut needs a take"))?;
    let file = take(root, name)?;
    let media = root.join(&file);
    let lines: Vec<String> = args["lines"].as_array().into_iter().flatten().filter_map(|l| l.as_str().map(String::from)).collect();
    // spans in the take's seconds (--at A-B): a take with no words (a screen recording, b-roll) or an exact moment
    let spans: Vec<(f64, f64)> = args["spans"].as_array().into_iter().flatten()
        .filter_map(|s| Some((s.get(0)?.as_f64()?, s.get(1)?.as_f64()?))).filter(|(a, b)| b > a).collect();
    anyhow::ensure!(!lines.is_empty() || !spans.is_empty(), "cut needs lines (\"what's said\") or spans (--at A-B, in the take's seconds)");
    let shape = shape(root);
    let mut cuts: Vec<Value> = args["keep"].as_array().cloned().unwrap_or_default();
    let stem = Path::new(&file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut notes = Vec::new();
    if !lines.is_empty() {
        let wf = words_file(&media).ok_or_else(|| anyhow!("no transcript for {file} (its words.json): cut it by time instead, --at A-B"))?;
        let (plan, n) = crate::brain::cut_plan(&media, &wf, &lines).await?;
        notes = n;
        for (a, b, said) in plan {
            cuts.push(json!({"take": stem, "file": file, "from": (a * 1000.0).round() / 1000.0, "to": (b * 1000.0).round() / 1000.0, "line": said}));
        }
    }
    let (dur, _, _) = probe(&media).await;
    for (a, b) in spans {
        let (a, b) = (a.max(0.0), if dur > 0.0 { b.min(dur) } else { b });
        anyhow::ensure!(b > a, "the span {a:.2}-{b:.2} is outside {file} ({dur:.1} s)");
        let said: Vec<String> = words(&media).into_iter().filter(|w| w.1 >= a - 0.02 && w.2 <= b + 0.08).map(|w| w.0).collect();
        cuts.push(json!({"take": stem, "file": file, "from": (a * 1000.0).round() / 1000.0, "to": (b * 1000.0).round() / 1000.0, "line": said.join(" ")}));
    }
    // the words in the edit's frames, for captions (useWords())
    let (mut at, mut timeline, mut said) = (0i64, Vec::new(), Vec::new());
    for (k, c) in cuts.iter().enumerate() {
        let (from, to) = (c["from"].as_f64().unwrap_or(0.0), c["to"].as_f64().unwrap_or(0.0));
        let n = frames(from, to, shape.fps);
        for w in words(&root.join(c["file"].as_str().unwrap_or(""))) {
            if w.1 >= from - 0.02 && w.2 <= to + 0.08 {
                let f = |s: f64| at + ((s - from) * shape.fps).round() as i64;
                timeline.push(json!({"text": w.0, "from": f(w.1).max(at), "to": f(w.2).min(at + n).max(f(w.1) + 1)}));
            }
        }
        said.push(format!("{:>2}  {:5.2}-{:5.2} s  {}  \"{}\"", k + 1, at as f64 / shape.fps, (at + n) as f64 / shape.fps, c["take"].as_str().unwrap_or(""), c["line"].as_str().unwrap_or("")));
        at += n;
    }
    Ok(json!({"edit": shape, "cuts": cuts, "words": timeline,
        "said": format!("cuts.ts: {} clips, {:.1} s ({} frames at {} fps); words.json for captions\n{}{}", cuts.len(), at as f64 / shape.fps, at, shape.fps, said.join("\n"), notes.iter().map(|n| format!("\nnote: {n}")).collect::<String>())}))
}

fn perceive() -> PathBuf {
    std::env::var_os("RAEA_PERCEIVE").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ai/perception/target/release/raea-perceive"))
}

async fn probe(media: &Path) -> (f64, u32, u32) {
    let out = tokio::process::Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height:format=duration", "-of", "json"])
        .arg(media).output().await.map(|o| o.stdout).unwrap_or_default();
    let v: Value = serde_json::from_slice(&out).unwrap_or_default();
    let s = &v["streams"][0];
    (v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.0), s["width"].as_u64().unwrap_or(0) as u32, s["height"].as_u64().unwrap_or(0) as u32)
}

async fn frame(root: &Path, args: &Value) -> anyhow::Result<String> {
    let file = take(root, args["take"].as_str().unwrap_or(""))?;
    let media = root.join(&file);
    let (dur, sw, sh) = probe(&media).await;
    let at = args["at"].as_f64().unwrap_or(dur / 2.0).clamp(0.0, (dur - 0.1).max(0.0));
    let shape = shape(root);
    let still = std::env::temp_dir().join(format!("raea-frame-{}.jpg", std::process::id()));
    tokio::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-ss", &format!("{at:.2}"), "-i"]).arg(&media).args(["-frames:v", "1"]).arg(&still).output().await?;
    let out = tokio::process::Command::new(perceive()).arg("see-layout").arg(&still).output().await.context("raea-perceive")?;
    let _ = std::fs::remove_file(&still);
    let v: Value = serde_json::from_slice(String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("{}").as_bytes()).unwrap_or_default();
    let face = v["layout"]["zones"].as_array().into_iter().flatten().find_map(|z| z["face"].as_object().cloned());
    let (aspect, edit_aspect) = (sw as f64 / sh.max(1) as f64, shape.width as f64 / shape.height as f64);
    let mut lines = vec![format!("{file} at {at:.1} s: {sw}x{sh} ({:.2}:1); the edit is {}x{}", aspect, shape.width, shape.height)];
    match face {
        Some(f) => {
            let g = |k: &str| f.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            lines.push(format!("a face at x {:.0}% y {:.0}%, {:.0}% of the height: a talking take. <FaceFrame src={{...}} /> (raea add face-frame) keeps it framed for the edit's shape; punchIn={{1.15}} to tighten on a key line",
                (g("x") + g("w") / 2.0) * 100.0, (g("y") + g("h") / 2.0) * 100.0, g("h") * 100.0));
        }
        None if aspect > edit_aspect * 1.3 => lines.push(format!(
            "no face, and wider than the edit: a cover crop keeps only the middle {:.0}% of its width. <ScreenFit src={{...}} aspect={{{:.3}}} /> (raea add screen-fit) shows it whole, with room above and below for a headline and captions",
            edit_aspect / aspect * 100.0, aspect)),
        None => lines.push("no face here: it fills the frame as it is (a <Video> with objectFit \"cover\"); try another --at if a person should be in it".into()),
    }
    Ok(lines.join("\n"))
}

fn sentences(w: &[crate::eval::Word], cap: usize) -> String {
    let (mut out, mut cur, mut start) = (String::new(), Vec::new(), None);
    for (i, x) in w.iter().enumerate() {
        start.get_or_insert(x.1);
        cur.push(x.0.as_str());
        let pause = w.get(i + 1).is_none_or(|n| n.1 - x.2 > 0.6);
        if x.0.ends_with(['.', '?', '!']) || pause {
            let line = format!("  {:6.2}-{:6.2}  {}\n", start.unwrap_or(0.0), x.2, cur.join(" "));
            if out.len() + line.len() > cap {
                out += &format!("  ... (to {:.1} s)\n", w.last().map(|l| l.2).unwrap_or(0.0));
                break;
            }
            out += &line;
            cur.clear();
            start = None;
        }
    }
    out
}

async fn media(root: &Path) -> anyhow::Result<String> {
    let all = takes(root);
    if all.is_empty() {
        return Ok("no takes under media/".into());
    }
    let sheets = root.join(".sheets");
    std::fs::create_dir_all(&sheets)?;
    let mut out = String::new();
    for file in all {
        let media = root.join(&file);
        let (dur, w, h) = probe(&media).await;
        let stem = Path::new(&file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let sheet = sheets.join(format!("{stem}.png"));
        let ok = crate::stage::rdb(vec!["media".into(), "sheet".into(), media.to_string_lossy().into(), "--n".into(), "6".into(), "--width".into(), "200".into(), "-o".into(), sheet.to_string_lossy().into()]).await.is_ok() && sheet.exists();
        let at: Vec<String> = (0..6).map(|i| format!("{:.1}", (i as f64 + 0.5) / 6.0 * dur)).collect();
        out += &format!("{file}  {dur:.1} s  {w}x{h}{}\n", if ok { format!("  sheet .sheets/{stem}.png (frames at {} s)", at.join(", ")) } else { String::new() });
        let ws = words(&media);
        if ws.is_empty() { out += "  (no transcript)\n" } else { out += &sentences(&ws, 5000) }
    }
    Ok(out)
}

/// raea sao / tokensynth / sounds (sdk/react/src/motion/sound.rs): run for this object, through its asset store (--dir), so
/// a sound made here is the one its edit's render finds made, and a library's samples land in its engine/media/. Their answer
/// as raea-motion prints it. The local models may install themselves first (minutes, once), hence the long timeout.
async fn audio(root: &Path, args: &Value) -> anyhow::Result<String> {
    let argv: Vec<String> = args["argv"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)).collect();
    anyhow::ensure!(matches!(argv.first().map(String::as_str), Some("sao" | "tokensynth" | "sounds")), "audio: sao, tokensynth or sounds");
    // what they write stays inside the object
    if let Some(o) = argv.iter().position(|a| a == "-o").and_then(|i| argv.get(i + 1)) {
        anyhow::ensure!(!o.starts_with('/') && !o.split('/').any(|c| c == ".."), "-o {o}: give a path inside this object (media/sfx/hit.wav)");
    }
    let mut cmd = tokio::process::Command::new(motion());
    cmd.args(&argv).arg("--dir").arg(root).current_dir(root).kill_on_drop(true);
    let r = tokio::time::timeout(std::time::Duration::from_secs(1800), cmd.output()).await.map_err(|_| anyhow!("raea {} took longer than 30 minutes and was stopped", argv[0]))??;
    let said = String::from_utf8_lossy(&r.stdout).trim().to_string();
    if r.status.success() {
        return Ok(said);
    }
    // the refusal, and the last of what it printed on the way (a setup's progress)
    let err = String::from_utf8_lossy(&r.stderr);
    let tail: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
    Err(anyhow!("{}{}", said.trim_start_matches("REFUSED  "), if tail.is_empty() { String::new() } else { format!("\n{}", tail[tail.len().saturating_sub(4)..].join("\n")) }))
}

pub fn motion() -> PathBuf {
    std::env::var_os("RAEA_MOTION").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/react/target/release/raea-motion"))
}

/// perception.json for the Clock (Region, --face-*, --safe-*): from each clip's first frame, the face its take shows at
/// the clip's middle, mapped through a centred cover crop into the edit's pixels, and the house safe band (the score
/// compiler's: 220 px under the top, 420 over the bottom at 1920 high). By frame, the latest at or before applies.
async fn perception(root: &Path) -> anyhow::Result<()> {
    let shape = shape(root);
    let (w, h) = (shape.width as f64, shape.height as f64);
    let k = h / 1920.0;
    let safe = json!({"top": (220.0 * k).round(), "bottom": (h - 420.0 * k).round()});
    let mut out = serde_json::Map::new();
    let mut at = 0i64;
    let mut seen: std::collections::HashMap<(String, i64), Option<[f64; 4]>> = Default::default();
    for c in cuts_of(root) {
        let (from, to) = (c["from"].as_f64().unwrap_or(0.0), c["to"].as_f64().unwrap_or(0.0));
        let file = c["file"].as_str().unwrap_or("").to_string();
        let mid = (from + to) / 2.0;
        let key = (file.clone(), (mid * 10.0) as i64);
        if !seen.contains_key(&key) {
            let media = root.join(&file);
            let (_, sw, sh) = probe(&media).await;
            let still = std::env::temp_dir().join(format!("raea-perc-{}-{}.jpg", std::process::id(), seen.len()));
            let _ = tokio::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-ss", &format!("{mid:.2}"), "-i"]).arg(&media).args(["-frames:v", "1"]).arg(&still).output().await;
            let o = tokio::process::Command::new(perceive()).arg("see-layout").arg(&still).output().await.ok();
            let _ = std::fs::remove_file(&still);
            let v: Value = o.and_then(|o| serde_json::from_slice(String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("{}").as_bytes()).ok()).unwrap_or_default();
            let face = v["layout"]["zones"].as_array().into_iter().flatten().find_map(|z| z["face"].as_object().cloned()).map(|f| {
                let g = |k: &str| f.get(k).and_then(Value::as_f64).unwrap_or(0.0);
                // the cover crop: the source scaled to fill the edit, centred
                let (sw, sh) = (sw.max(1) as f64, sh.max(1) as f64);
                let s = (w / sw).max(h / sh);
                let (ox, oy) = ((w - sw * s) / 2.0, (h - sh * s) / 2.0);
                [g("x") * sw * s + ox, g("y") * sh * s + oy, g("w") * sw * s, g("h") * sh * s]
            });
            seen.insert(key.clone(), face);
        }
        let mut p = json!({"safe": safe});
        if let Some([x, y, fw, fh]) = seen[&key] {
            p["face"] = json!({"x": x.round(), "y": y.round(), "w": fw.round(), "h": fh.round()});
        }
        out.insert(at.to_string(), p);
        at += frames(from, to, shape.fps);
    }
    anyhow::ensure!(!out.is_empty(), "no clips");
    std::fs::write(root.join("perception.json"), Value::Object(out).to_string())?;
    Ok(())
}

/// A piece of the project and its files: edit.tsx with words.json, perception.json, render.json and final.mp4, or
/// NAME.tsx (a brief's other pieces, NAME-9x16.tsx per deliverable) with NAME.words.json and the rest beside it, so
/// pieces in one workspace never overwrite each other (raea-brief's sidecar naming).
pub struct Piece {
    pub tsx: PathBuf,
    pub words: PathBuf,
    pub perception: PathBuf,
    pub manifest: PathBuf,
    pub out: PathBuf,
    /// the project's own edit (cuts.json drives it)
    pub main: bool,
}

impl Piece {
    fn of(root: &Path, args: &Value) -> Piece {
        let name = args["piece"].as_str().map(|n| n.trim_end_matches(".tsx").to_string()).filter(|n| !n.is_empty() && n != "edit");
        match name {
            None => Piece { tsx: root.join("edit.tsx"), words: root.join("words.json"), perception: root.join("perception.json"), manifest: root.join("render.json"), out: root.join("final.mp4"), main: true },
            Some(n) => Piece { tsx: root.join(format!("{n}.tsx")), words: root.join(format!("{n}.words.json")), perception: root.join(format!("{n}.perception.json")), manifest: root.join(format!("{n}.render.json")), out: root.join(format!("{n}.mp4")), main: false },
        }
    }
}

/// styles.gen.ts: the brand's variables and the project's Tailwind (raea_void::tailwind: Tailwind's compiler on raea's
/// JavaScriptCore, no bun), written only when it changes (the stage's previews build it too, and an unchanged file keeps
/// its time).
pub async fn styles(root: &Path) -> anyhow::Result<()> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (space, theme) = (root.to_path_buf(), here.join("../cli/template/theme.css"));
    let css = tokio::task::spawn_blocking(move || raea_void::tailwind::css(&space, &theme)).await.context("tailwind")??;
    let body = format!("// written by raea render: the brand's variables and the project's Tailwind\nexport const css = {};\n", serde_json::to_string(&css)?);
    let file = root.join("styles.gen.ts");
    if std::fs::read_to_string(&file).ok().as_deref() != Some(body.as_str()) {
        std::fs::write(file, body)?;
    }
    Ok(())
}

/// styles.gen.ts (the brand's variables and the project's Tailwind), then final.mp4 through raea-motion. The said line,
/// and whether it rendered.
async fn render(root: &Path, piece: &Piece) -> anyhow::Result<(String, bool)> {
    anyhow::ensure!(piece.tsx.exists(), "no {}: raea init makes edit.tsx, raea script a brief's pieces", piece.tsx.strip_prefix(root).unwrap_or(&piece.tsx).display());
    styles(root).await?;
    let out = piece.out.clone();
    let _ = std::fs::remove_file(&out);
    let mut cmd = tokio::process::Command::new(motion());
    // lyria() and sao() in the edit are generated as it renders: rdb needs a the host app token and a ledger that counts the
    // paid requests against the run's cap (the deleted rrl path was the only thing that passed them, so TypeScript
    // renders had no sound from generation). The environment's when it has them (an eval run's ledger), else a session
    // as void's tools get one and a ledger beside the project
    if std::env::var_os("RAEA_GEN_TOKEN").is_none() {
        if let Ok(t) = crate::brain::api_session().await {
            cmd.env("RAEA_GEN_TOKEN", t);
        }
    }
    if std::env::var_os("RAEA_GEN_LEDGER").is_none() {
        cmd.env("RAEA_GEN_LEDGER", root.join(".gen-ledger.jsonl"));
    }
    cmd.args(render_args(root, piece, &out, &piece.manifest).await).current_dir(root).kill_on_drop(true);
    let t0 = std::time::Instant::now();
    let r = tokio::time::timeout(std::time::Duration::from_secs(900), cmd.output()).await.map_err(|_| anyhow!("the render took longer than 15 minutes and was stopped"))??;
    let said = format!("{}{}", String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr));
    if !r.status.success() || !out.exists() {
        // the composition's own errors first ([js] lines, thrown errors: "didn't commit frame 0" alone told the agent
        // nothing), then the last lines
        let lines: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
        let js: Vec<&str> = lines.iter().copied().filter(|l| l.contains("[js]") || l.contains("Error") || l.contains("error:") || l.contains("    at ")).take(14).collect();
        let tail = &lines[lines.len().saturating_sub(6)..];
        return Ok((format!("the render failed:\n{}{}", if js.is_empty() { String::new() } else { format!("{}\n...\n", js.join("\n")) }, tail.join("\n")), false));
    }
    let (dur, w, h) = probe(&out).await;
    Ok((format!("{}: {dur:.1} s, {w}x{h} (rendered in {:.0} s)", out.strip_prefix(root).unwrap_or(&out).display(), t0.elapsed().as_secs_f64()), true))
}

/// raea-motion render's arguments for `piece`, its video to `out` and its manifest to `manifest`: one list, so a render
/// made ahead (speculate) asks for exactly what the agent's will, and the export cache can answer the agent's.
async fn render_args(root: &Path, piece: &Piece, out: &Path, manifest: &Path) -> Vec<std::ffi::OsString> {
    let mut a: Vec<std::ffi::OsString> = vec!["render".into(), piece.tsx.clone().into(), "-o".into(), out.into()];
    if piece.words.exists() {
        a.extend(["--words".into(), piece.words.clone().into()]);
    }
    // the project's edit gets its perception from cuts.json; a compiled piece brings its own (the compiler's, per frame)
    if (piece.main && cuts_of(root).len() > 0 && perception(root).await.is_ok()) || piece.perception.exists() {
        a.extend(["--perception".into(), piece.perception.clone().into()]);
    }
    // what each frame shows, as laid out (the checks read it, not the edit's source)
    a.extend(["--manifest".into(), manifest.into()]);
    // a compiled piece (score's SDK backend) is a default export with `export const config = {width, height, fps,
    // durationInFrames}` and no <Composition>: its size and length go to raea-motion as flags (else 60 frames)
    if let Some(c) = std::fs::read_to_string(&piece.tsx).ok().and_then(|t| {
        let at = t.find("export const config = ")?;
        let rest = &t[at + "export const config = ".len()..];
        serde_json::from_str::<Value>(&rest[..rest.find("};")? + 1]).ok()
    }) {
        for (flag, key) in [("--w", "width"), ("--h", "height"), ("--fps", "fps"), ("--frames", "durationInFrames")] {
            if let Some(v) = c[key].as_f64() {
                a.extend([flag.into(), format!("{v}").into()]);
            }
        }
    }
    a
}

/// The project's edit rendered ahead, while it sits open and unchanged (lightspeed T1): at background priority
/// (taskpolicy -b: the scheduler, the disk and the GPU serve it last), into a scratch file that's thrown away. What it's
/// for is the export cache: the agent's own `raea render` of the same sources is then a copy. Not for an edit that
/// generates (lyria(), fish(): paid calls the agent didn't ask for yet; sao(), tokensynth(): minutes of local compute,
/// or a first-use install). Ok(false) when it didn't run.
pub async fn speculate(root: &Path, cancel: impl std::future::Future<Output = ()>) -> anyhow::Result<bool> {
    let piece = Piece::of(root, &Value::Null);
    if !piece.tsx.exists() || generates(root, 0) || !power_allows().await {
        return Ok(false);
    }
    styles(root).await?;
    let scratch = std::env::temp_dir().join(format!("void-ahead-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let (out, manifest) = (scratch.join("ahead.mp4"), scratch.join("ahead.render.json"));
    let mut cmd = tokio::process::Command::new("taskpolicy");
    cmd.arg("-b").arg(motion()).args(render_args(root, &piece, &out, &manifest).await).current_dir(root).kill_on_drop(true);
    // stopped the moment `cancel` resolves (the person acted, the sources changed): the render is killed with the
    // future, and the export cache only ever moves a finished file in, so nothing half-made is left there
    let r = tokio::select! {
        r = cmd.output() => Some(r),
        _ = cancel => None,
    };
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(match r {
        Some(r) => r?.status.success(),
        None => false,
    })
}

/// The project's long-GOP 4K footage given its optimized copies (raea-motion mezz: intra-only ProRes in
/// ~/Library/Caches/raea/mezz, lightspeed F4), at background priority and under the same power rule as `speculate`.
/// Decoders switch to a copy only once it's complete, so play and render agree throughout. Ok(true) when a copy was
/// made (the export ahead waits for the next look, so it reads the copies); RAEA_VOID_MEZZ=0 turns it off.
pub async fn optimize_media(root: &Path, cancel: impl std::future::Future<Output = ()>) -> anyhow::Result<bool> {
    if std::env::var("RAEA_VOID_MEZZ").as_deref() == Ok("0") || !power_allows().await {
        return Ok(false);
    }
    let check = tokio::process::Command::new(motion()).args(["mezz", "--check"]).arg(root).current_dir(root).output().await?;
    if !String::from_utf8_lossy(&check.stdout).contains("\"wanted\"") {
        return Ok(false);
    }
    let mut cmd = tokio::process::Command::new("taskpolicy");
    cmd.arg("-b").arg(motion()).arg("mezz").arg(root).current_dir(root).kill_on_drop(true);
    // stopped when `cancel` resolves: the copy in progress is a .partial file, never read and swept within the hour
    let r = tokio::select! {
        r = cmd.output() => Some(r),
        _ = cancel => None,
    };
    Ok(match r {
        Some(r) => String::from_utf8_lossy(&r?.stdout).contains("made in"),
        None => false,
    })
}

/// Work ahead runs only on mains power, out of Low Power Mode and with no thermal pressure (R6: a render nobody asked
/// for mustn't cost a battery or a hot machine).
async fn power_allows() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        let info: *mut AnyObject = msg_send![class!(NSProcessInfo), processInfo];
        let low: bool = msg_send![info, isLowPowerModeEnabled];
        // NSProcessInfoThermalState: 0 nominal, 1 fair, 2 serious, 3 critical
        let thermal: isize = msg_send![info, thermalState];
        if low || thermal >= 2 {
            return false;
        }
    }
    match tokio::process::Command::new("pmset").args(["-g", "batt"]).output().await {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains("'AC Power'"),
        Err(_) => false,
    }
}

/// Whether the project's sources call a generator (sao(), tokensynth(), fish(), lyria()): its TypeScript, its parts included.
fn generates(dir: &Path, depth: usize) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    rd.flatten().any(|e| {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" {
            return false;
        }
        if p.is_dir() {
            return depth < 4 && generates(&p, depth + 1);
        }
        matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx")) && std::fs::read_to_string(&p).is_ok_and(|t| ["sao(", "tokensynth(", "fish(", "lyria("].iter().any(|g| t.contains(g)))
    })
}

/// raea render: the render, kept as a version.
async fn render_saved(root: &Path, piece: &Piece) -> anyhow::Result<String> {
    let (said, ok) = render(root, piece).await?;
    Ok(if ok { format!("{said}\n{}", keep_version(root, piece, None).await) } else { said })
}

async fn check(root: &Path, piece: &Piece, full: bool) -> anyhow::Result<String> {
    let (said, ok) = render(root, piece).await?;
    if !ok {
        return Ok(said);
    }
    let video = piece.out.clone();
    let shape = shape(root);
    let (dur, _, _) = probe(&video).await;
    let total = (dur * shape.fps).round() as i64;
    // the four checks at once (each only reads the render): one after another they took the sum, about 47 s on a 20 s
    // edit, where the slowest alone is about 20
    // the cuts and the talk's pauses are measured on footage with speech: a game, a deck, a document or music has none,
    // and "near-silence inside the talk" on a pong rally sent the agent to waive what wasn't wrong (2026-10-17)
    let talks = !matches!(crate::app::kind_of(root).as_deref(), Some("game" | "deck" | "document" | "music" | "scene3d" | "still"));
    let cuts = async { if !talks { return Vec::new(); } measure_cuts(root, &video, piece.main).await.ok().map(|v| v["issues"].as_array().into_iter().flatten().filter_map(|i| i["says"].as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default() };
    let picture = crate::legible::video_faults(&video, total, shape.fps);
    let laid = manifest_faults(root, piece);
    // the render pack's model checks (clef on the picture, the vote's writer calls): only in a full check (`raea check
    // --full`, and the gate at a turn's end), since they're the slow, paid part; its rules are measured here, from the
    // manifest and cuts.json, every time
    let pack = async {
        if !full {
            return Vec::new();
        }
        let Ok(pack) = crate::checks::load("render") else { return Vec::new() };
        // only the checks calibration let warn: an advisory answer is never shown, and the vote (message.clear, three
        // writer calls) alone took ~55 s
        let only: Vec<String> = pack.checks.iter().filter(|c| c.eye != "rule" && c.status == "warn").map(|c| c.id.clone()).collect();
        let (found, _cost) = crate::checks::run(&pack, &piece.tsx, &video, &crate::checks::Measured { cuts: None }, Some(&only)).await;
        found.iter().filter(|f| f.fault && f.status == "warn").map(|f| format!("{}: {} (fix: {})", f.id, f.says, f.fix)).collect::<Vec<_>>()
    };
    let briefed = root.join(".score").join(format!("{}.graph.json", piece.tsx.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())).exists();
    let (mut a, b, c, d, e) = tokio::join!(cuts, picture, laid, pack, brief_check(root, piece));
    // nothing measures clean on nothing: the template's edit, or a render of a frame or two, is a fault of its own
    if (piece.main && crate::stage_ts::blank(root)) || total <= 2 {
        a.push(format!("nothing is built yet: {} (write the piece in edit.tsx, then check again)", if total <= 2 { format!("the render is {total} frame(s)") } else { "edit.tsx is raea init's template".into() }));
    }
    let mut families = vec![("the cuts and the ending".to_string(), a), ("the picture: black frames, legibility".to_string(), b), ("the layout: text on the face, off the frame, the speaker in the picture".to_string(), c)];
    if full {
        families.push(("the model checks: framing, captions, the message".to_string(), d));
    }
    if briefed {
        families.push(("the brief's rules".to_string(), e));
    }
    let faults: Vec<String> = families.iter().flat_map(|(_, f)| f.clone()).collect();
    let kept = keep_version(root, piece, Some((&families, full))).await;
    Ok(if faults.is_empty() { format!("{said}\nPASS  no faults found\n{kept}") } else { format!("{said}\n{} faults:\n{}\n{kept}", faults.len(), faults.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n")) })
}

/// A piece compiled from a brief is checked against the brief's own `!` rules: raea-brief's checker (raea-01's contract,
/// 2026-10-17: `raea-brief check NAME SPACE [--video PATH]`, one `PASS|FAIL|NOT MEASURED  ! rule: why, frames` line a rule,
/// exit 0 all pass, 1 a fail, 2 can't check). Its FAIL lines are faults beside the others; a piece with no brief graph
/// (.score/NAME.graph.json) has nothing to check.
async fn brief_check(root: &Path, piece: &Piece) -> Vec<String> {
    let name = piece.tsx.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "edit".into());
    if !root.join(".score").join(format!("{name}.graph.json")).exists() {
        return Vec::new();
    }
    let bin = std::env::var_os("RAEA_BRIEF").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/brief/target/release/raea-brief"));
    let Ok(o) = tokio::process::Command::new(&bin).arg("check").arg(&name).arg(root).arg("--video").arg(&piece.out).output().await else {
        return vec![format!("the brief's rules weren't checked: {} didn't run", bin.display())];
    };
    let text = String::from_utf8_lossy(&o.stdout);
    match o.status.code() {
        Some(2) => vec![format!("the brief's rules couldn't be checked: {}", String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("no render or manifest"))],
        _ => text.lines().filter(|l| l.starts_with("FAIL")).map(|l| format!("brief {}", l.trim_start_matches("FAIL").trim())).collect(),
    }
}

/// The project's cuts measured on a render of it (eval.rs measure_cuts): its clips end to end from cuts.json (none for a
/// piece that isn't the project's own edit), the takes' transcripts for clipped words and an unfinished ending.
pub(crate) async fn measure_cuts(root: &Path, video: &Path, main: bool) -> anyhow::Result<Value> {
    let shape = shape(root);
    let (dur, _, _) = probe(video).await;
    let total = (dur * shape.fps).round() as i64;
    let cuts = if main { cuts_of(root) } else { Vec::new() };
    let (spans, spoken) = spans(root, &cuts, shape.fps);
    crate::eval::measure_cuts(video, shape.fps, total, spans, &spoken).await
}

/// The project's render.json checks (text over the face, off the frame, the speaker out of the picture) on `video`.
pub(crate) async fn face_faults(root: &Path, video: &Path) -> Vec<String> {
    let mut piece = Piece::of(root, &json!({}));
    piece.out = video.to_path_buf();
    manifest_faults(root, &piece).await
}

/// The words the project's edit plays, in its frames: each clip's transcript between its in and out points.
pub(crate) fn spoken_words(root: &Path) -> Vec<(String, i64)> {
    let fps = shape(root).fps;
    let (_, spoken) = spans(root, &cuts_of(root), fps);
    let mut out = Vec::new();
    for (words, _, from, to, at, _) in spoken {
        for w in words.iter().filter(|w| w.1 >= from - 0.02 && w.1 < to) {
            out.push((w.0.clone(), at + ((w.1 - from) * fps).round().max(0.0) as i64));
        }
    }
    out
}

/// Whether any video take under media/ shows a face: a frame at a quarter, half and three quarters of each, read by
/// raea-perceive's see-layout. Audio-only takes (a voiceover) have none. When perception can't run, it says yes, so the
/// speaker check stays on.
async fn any_take_has_face(root: &Path) -> bool {
    let dir = root.join(".stills/takes");
    let _ = std::fs::create_dir_all(&dir);
    let mut pngs = Vec::new();
    for (i, file) in takes(root).iter().enumerate() {
        let media = root.join(file);
        let (dur, w, _) = probe(&media).await;
        if w == 0 || dur <= 0.0 {
            continue;
        }
        for (k, at) in [0.25, 0.5, 0.75].iter().enumerate() {
            let p = dir.join(format!("{i}-{k}.png"));
            let _ = tokio::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-ss", &format!("{:.2}", dur * at), "-i"]).arg(&media).args(["-frames:v", "1"]).arg(&p).output().await;
            if p.exists() {
                pngs.push(p);
            }
        }
    }
    if pngs.is_empty() {
        return false;
    }
    let Ok(o) = tokio::process::Command::new(perceive()).arg("see-layout").args(&pngs).output().await else { return true };
    if !o.status.success() {
        return true;
    }
    String::from_utf8_lossy(&o.stdout).lines().any(|l| {
        let v: Value = serde_json::from_str(l).unwrap_or_default();
        v["layout"]["zones"].as_array().into_iter().flatten().any(|z| z["face"].is_object())
    })
}

/// The checks that read render.json (each frame's text and videos, as laid out) against the render's own faces, read
/// back at 8 even moments: text over the face (more than 8% of its area, @raea/motion's proofBoxes limit), text off the
/// frame, and the speaker out of the picture while words are said (no face, or the face cut by the edge, at 3 or more
/// moments and 40% of the spoken ones: legible's speaker check).
async fn manifest_faults(root: &Path, piece: &Piece) -> Vec<String> {
    let video = piece.out.as_path();
    let Some(m) = std::fs::read_to_string(&piece.manifest).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { return vec![] };
    let (w, h, fps) = (m["width"].as_f64().unwrap_or(1080.0), m["height"].as_f64().unwrap_or(1920.0), m["fps"].as_f64().unwrap_or(30.0));
    let frames = m["frames"].as_array().cloned().unwrap_or_default();
    if frames.is_empty() {
        return vec![];
    }
    let t = |f: i64| format!("{}:{:04.1}", f / (fps as i64 * 60), (f as f64 / fps) % 60.0);
    let boxes = |fr: &Value| -> Vec<(String, [f64; 4])> {
        fr["text"].as_array().into_iter().flatten().filter_map(|x| {
            let b = x["box"].as_array()?;
            Some((x["text"].as_str()?.to_string(), [b.first()?.as_f64()?, b.get(1)?.as_f64()?, b.get(2)?.as_f64()?, b.get(3)?.as_f64()?]))
        }).collect()
    };
    let mut out = Vec::new();
    // text off the frame: the first frame each line runs past an edge
    let mut off: std::collections::BTreeMap<String, i64> = Default::default();
    for fr in &frames {
        for (txt, b) in boxes(fr) {
            if b[0] < -2.0 || b[1] < -2.0 || b[0] + b[2] > w + 2.0 || b[1] + b[3] > h + 2.0 {
                off.entry(txt).or_insert(fr["frame"].as_i64().unwrap_or(0));
            }
        }
    }
    for (txt, f) in off.iter().take(4) {
        out.push(format!("{} \"{txt}\" runs off the frame: keep every line inside it (a narrower box, smaller type, or inside a <Region>)", t(*f)));
    }
    // 3D parts passing through each other where the camera sees it (raea-motion's clipping check, scene3d/clip3d.rs)
    for c in m["clipping"].as_array().into_iter().flatten().take(4) {
        if let Some(says) = c["says"].as_str() {
            out.push(format!("clipping: {says}"));
        }
    }
    // the render's faces at 8 moments
    let picks: Vec<&Value> = (0..8).map(|i| &frames[((i as f64 + 0.5) / 8.0 * frames.len() as f64) as usize % frames.len()]).collect();
    let dir = root.join(".stills/check");
    let _ = std::fs::create_dir_all(&dir);
    let mut pngs = Vec::new();
    for fr in &picks {
        let f = fr["frame"].as_i64().unwrap_or(0);
        let p = dir.join(format!("{f}.png"));
        let _ = tokio::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-ss", &format!("{:.3}", f as f64 / fps), "-i"]).arg(video).args(["-frames:v", "1"]).arg(&p).output().await;
        pngs.push(p);
    }
    let o = tokio::process::Command::new(perceive()).arg("see-layout").args(&pngs).output().await.ok();
    let faces: Vec<Option<[f64; 4]>> = o.map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| {
        let v: Value = serde_json::from_str(l).unwrap_or_default();
        v["layout"]["zones"].as_array().into_iter().flatten().find_map(|z| z["face"].as_object().cloned()).map(|f| {
            let g = |k: &str| f.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            [g("x") * w, g("y") * h, g("w") * w, g("h") * h]
        })
    }).collect()).unwrap_or_default();
    let words: Vec<(i64, i64)> = std::fs::read_to_string(&piece.words).ok().and_then(|t| serde_json::from_str::<Vec<Value>>(&t).ok()).unwrap_or_default()
        .iter().filter_map(|x| Some((x["from"].as_i64()?, x["to"].as_i64()?))).collect();
    let (mut spoken, mut gone) = (0, 0);
    for (fr, face) in picks.iter().zip(faces.iter().chain(std::iter::repeat(&None))) {
        let f = fr["frame"].as_i64().unwrap_or(0);
        if words.iter().any(|&(a, b)| f >= a - 3 && f < b + 3) {
            spoken += 1;
            if face.is_none_or(|b| b[0] < w * 0.02 || b[0] + b[2] > w * 0.98) {
                gone += 1;
            }
        }
        let Some(fb) = face else { continue };
        let area = (fb[2] * fb[3]).max(1.0);
        let mut covered = 0.0;
        let mut over = Vec::new();
        for (txt, b) in boxes(fr) {
            let ox = (b[0] + b[2]).min(fb[0] + fb[2]) - b[0].max(fb[0]);
            let oy = (b[1] + b[3]).min(fb[1] + fb[3]) - b[1].max(fb[1]);
            if ox > 0.0 && oy > 0.0 {
                covered += ox * oy;
                over.push(txt);
            }
        }
        if covered / area > 0.08 {
            out.push(format!("{} text covers {:.0}% of the face (\"{}\"): move it clear (<Region name=\"auto\"> keeps off the face) or make it smaller", t(f), (covered / area * 100.0).min(100.0), over.join(" ")));
        }
    }
    // only when there's a speaker to show: a take on camera. A voiceover over b-roll, or narration over a screen
    // recording, has no face in any take, and no framing can put one in (2026-10-18: a VO-and-b-roll ad failed this
    // on every check and the agent waived it in writing)
    if spoken >= 4 && gone >= 3 && gone as f64 >= 0.4 * spoken as f64 && any_take_has_face(root).await {
        out.push(format!("the speaker is out of the picture at {gone} of {spoken} moments while they talk (no face, or the face cut by the edge): frame the take on the face (<FaceFrame>), or cut to footage that shows them"));
    }
    out
}

async fn still(root: &Path, args: &Value) -> anyhow::Result<String> {
    let video = root.join("final.mp4");
    anyhow::ensure!(video.exists(), "no final.mp4: raea render first");
    let dir = root.join(".stills");
    std::fs::create_dir_all(&dir)?;
    let mut said = Vec::new();
    for t in args["at"].as_array().into_iter().flatten().filter_map(Value::as_f64) {
        let p = dir.join(format!("{t:.2}.png"));
        tokio::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-ss", &format!("{t:.3}"), "-i"]).arg(&video).args(["-frames:v", "1", "-vf", "scale=540:-2"]).arg(&p).output().await?;
        if p.exists() { said.push(format!(".stills/{t:.2}.png")) }
    }
    Ok(format!("{}\n(look at them to see the frames)", said.join("\n")))
}

async fn sheet(root: &Path) -> anyhow::Result<String> {
    let video = root.join("final.mp4");
    anyhow::ensure!(video.exists(), "no final.mp4: raea render first");
    std::fs::create_dir_all(root.join(".stills"))?;
    let p = root.join(".stills/sheet.png");
    let r = crate::stage::rdb(vec!["media".into(), "sheet".into(), video.to_string_lossy().into(), "--n".into(), "8".into(), "--width".into(), "240".into(), "-o".into(), p.to_string_lossy().into()]).await?;
    anyhow::ensure!(p.exists(), "no sheet: {r}");
    let (dur, _, _) = probe(&video).await;
    let at: Vec<String> = (0..8).map(|i| format!("{:.1}", (i as f64 + 0.5) / 8.0 * dur)).collect();
    Ok(format!(".stills/sheet.png: 8 frames at {} s (look at it)", at.join(", ")))
}

/// Briefs compiled to TSX: raea-brief's contract (raea-01, 2026-10-17), `raea-brief compile FILE SPACE`. With no file,
/// every piece: each NAME.brief with a `Format:` line (the others are shared through `Uses:`).
async fn script(root: &Path, args: &Value) -> anyhow::Result<String> {
    let files: Vec<PathBuf> = match args["file"].as_str() {
        Some(f) => vec![root.join(f)],
        None => std::fs::read_dir(root)?.flatten().map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "brief") && std::fs::read_to_string(p).is_ok_and(|t| t.lines().any(|l| l.trim_start().starts_with("Format:"))))
            .collect(),
    };
    anyhow::ensure!(!files.is_empty(), "no brief with a Format: line here (write NAME.brief first)");
    let bin = std::env::var_os("RAEA_BRIEF").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/brief/target/release/raea-brief"));
    let mut said = String::new();
    for file in files {
        anyhow::ensure!(file.exists(), "no {}", file.strip_prefix(root).unwrap_or(&file).display());
        // the space's one piece (brief.brief, edit.brief) is the edit raea render renders; other pieces keep their names
        let mut cmd = tokio::process::Command::new(&bin);
        cmd.arg("compile").arg(&file).arg(root);
        if matches!(file.file_name().and_then(|n| n.to_str()), Some("brief.brief" | "edit.brief")) {
            cmd.arg("--o").arg(root.join("edit.tsx"));
        }
        let r = cmd.output().await.with_context(|| format!("{}", bin.display()))?;
        said += &format!("{}{}", String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr));
    }
    Ok(said.chars().take(12_000).collect())
}
