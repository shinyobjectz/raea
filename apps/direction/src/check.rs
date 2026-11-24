//! `raea check PROJECT|EDIT.tsx` (docs/direction.feature DR4): every check that applies, run over each
//! composition's worlds. The rules come from raea's floor and the taste files: the person's, the workspace's and the
//! project's; each is one of raea's named checks (systems.rs) or a project's TS check (ts.rs), and where it was
//! written changes nothing about how it runs. A check that needs a render runs on the
//! composition's newest render; with none, it waits, and the command says what for. `raea check VIDEO.mp4` is the
//! floor alone (apps/check).

use std::path::{Path, PathBuf};

use raea_core_brief::taste;
use raea_core_facts::{finding, record, Facts, Finding, Level, Rule, Shot};
use raea_runtime_bundle::flag;
use raea_runtime_world::{Ask, Render};
use serde_json::{json, Value};

use crate::systems::{self, Frames};
use crate::ts;

type R<T> = Result<T, String>;

pub const USAGE: &str = "raea check [PROJECT|EDIT.tsx] [--composition ID] [--only ID] [--taste T] [--render OUT | --no-render] [--json]   (every check that applies to each composition, over its frames: raea's floor on its newest render, the person's, the workspace's and the project's taste.ts (each line's check: one of raea's forms or checks by name, or a project's .ts check); what needs a render and has none waits, and says so)";

/// The flags raea check takes with a value (its own, and the floor's on a video).
const TAKES: [&str; 10] = ["--composition", "--only", "--taste", "--render", "--manifest", "--words", "--perception", "--shape", "--platform", "-o"];

/// What the CLI lends `raea check`: its TS evaluator and headless run, and the taste kinds a mode needs (as `raea
/// context` reads the person's taste).
pub struct Host {
    pub ts: ts::Host,
    pub kinds: fn(&str) -> &'static [&'static str],
}

/// The person's taste folder: RAEA_TASTE, else ~/.raea/taste.
pub fn taste_dir() -> Option<PathBuf> {
    std::env::var_os("RAEA_TASTE").map(PathBuf::from).or_else(|| raea_io_home::user_home().map(|h| h.join(".raea/taste")))
}

/// The .ts files `--taste` names: the file, or every .ts in the folder. A file named under the person's taste folder
/// that isn't there yet is a note, not a refusal: the skill prescribes --taste on that folder's own taste file, and a
/// machine with none yet should still check (#89); any other missing path is left to the reader to refuse.
pub fn taste_files(args: &[String]) -> Vec<PathBuf> {
    let Some(t) = flag(args, "--taste").map(PathBuf::from) else { return vec![] };
    if t.extension().is_some_and(|x| x == "ts") {
        let home = taste_dir().map(std::path::absolute).and_then(Result::ok);
        if !t.is_file() && t.parent().map(std::path::absolute).and_then(Result::ok) == home {
            eprintln!("note: no taste yet ({} isn't there); checking without it", t.display());
            return vec![];
        }
        return vec![t];
    }
    let mut v: Vec<PathBuf> = std::fs::read_dir(&t).into_iter().flatten().filter_map(|e| e.ok().map(|e| e.path())).filter(|f| f.extension().is_some_and(|x| x == "ts")).collect();
    v.sort();
    v
}

/// The taste files a project is held to: `--taste`'s when given, else the person's taste.ts (RAEA_TASTE, or
/// ~/.raea/taste) and the kinds its brief's modes need, then the workspace's house rules.
fn tastes(project: &Path, given: Vec<PathBuf>, host: &Host) -> Vec<PathBuf> {
    if !given.is_empty() {
        return given;
    }
    let mut out = Vec::new();
    if let Some(dir) = taste_dir().filter(|d| d.join("taste.ts").is_file()) {
        out.push(dir.join("taste.ts"));
        let brief: Value = (host.ts.eval)(&project.join("brief.ts")).ok().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        for m in brief["mode"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            for f in (host.kinds)(&m.trim().to_ascii_lowercase()).iter().map(|k| dir.join(format!("{k}.ts"))) {
                if f.is_file() && !out.contains(&f) {
                    out.push(f);
                }
            }
        }
    }
    // the workspace's, then the project's own (the same file when the project is the workspace's folder)
    for f in record::up(project, ".raea/workspace").map(|ws| ws.join("taste.ts")).into_iter().chain([project.join("taste.ts")]) {
        if f.is_file() && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// One rule as it runs: a named check, or a project's check file's declaration.
enum How {
    Named(systems::Named),
    File(ts::Meta),
}

/// Every rule `project` is held to (`given`: `--taste`'s files) with its notes' asks (DR6), the notes, and a finding
/// for each file of rules or note raea couldn't read.
fn rules(project: &Path, given: Vec<PathBuf>, host: &Host) -> (Vec<Rule>, Vec<Finding>, Vec<raea_app_notes::Kept>) {
    let (mut out, mut bad) = held(project, given, host);
    let notes = raea_app_notes::read(project, host.ts.eval);
    let (asks, unread) = raea_app_notes::check::rules(project, &notes);
    out.extend(asks);
    bad.extend(unread);
    // a check's path made plain (no `..`), as its import will name it
    for r in out.iter_mut() {
        if let Some(p) = r.file() {
            r.check = raea_runtime_bundle::normalize(p).display().to_string();
        }
    }
    (out, bad, notes)
}

/// raea's floor and every taste file's rules (the person's, the workspace's, the project's), and a finding for each
/// file of them raea couldn't read.
fn held(project: &Path, given: Vec<PathBuf>, host: &Host) -> (Vec<Rule>, Vec<Finding>) {
    // raea's own: the floor, and the reading check (DR28: its findings keep their own levels, so no rule level)
    let read = Rule { level: None, ..Rule::named("text-read", "text can be read for as long as it needs, and OCR reads it", Default::default(), Level::Warn, "raea") };
    let mut out = vec![Rule::named("floor", "raea's floor", Default::default(), Level::Error, "raea"), read];
    let mut bad = Vec::new();
    let name = |f: &Path| f.strip_prefix(project).unwrap_or(f).display().to_string();
    for f in tastes(project, given, host) {
        let (dir, by) = (f.parent().unwrap_or(Path::new(".")).to_path_buf(), format!("taste {}", name(&f)));
        match (host.ts.eval)(&f).and_then(|j| raea_core_brief::read::<taste::Taste>(&name(&f), &j)) {
            Ok(t) => out.extend(t.sections.iter().flat_map(|s| &s.rules).filter_map(|r| Rule::of_taste(r, &dir, &by))),
            Err(e) => bad.push(Finding::new("taste", Level::Error, format!("raea can't read it: {e}"), "raea check").about(name(&f), None)),
        }
    }
    (out, bad)
}

/// The compositions `target` names: each (its file, its id when the file registers several).
fn compositions(target: &Path, args: &[String]) -> R<Vec<(PathBuf, Option<String>)>> {
    let one = flag(args, "--composition");
    let (dir, file) = if target.is_dir() { (target.to_path_buf(), None) } else { (target.parent().unwrap_or(Path::new(".")).to_path_buf(), Some(dunce::canonicalize(&target).map_err(|e| format!("{}: {e}", target.display()))?)) };
    let reg = raea_app_registry::scan(&dir);
    let mut out: Vec<(PathBuf, Option<String>)> = Vec::new();
    for c in &reg.compositions {
        let path = reg.dir.join(&c.file);
        let path = dunce::canonicalize(&path).unwrap_or(path);
        let id = (c.how == raea_app_registry::How::Registered).then(|| c.id.clone());
        if file.as_ref().is_none_or(|f| *f == path) && one.as_ref().is_none_or(|o| Some(o) == id.as_ref()) {
            out.push((path, id));
        }
    }
    if out.is_empty() {
        match (file, one) {
            // a file the scan didn't register (outside a project): itself
            (Some(f), one) => out.push((f, one)),
            (None, one) => return Err(format!("no composition in {}{}", dir.display(), one.map(|o| format!(" named {o}")).unwrap_or_default())),
        }
    }
    Ok(out)
}

/// What one composition's checks found, each check that waits with what for, and its newest render, its named shots
/// (each from its first frame to its end) and its rate, for a note's clip (DR6).
struct Checked {
    file: PathBuf,
    id: Option<String>,
    found: Vec<Finding>,
    waits: Vec<(String, String)>,
    render: Option<raea_app_notes::check::Rendered>,
    shots: Vec<(String, i64, Option<i64>)>,
    fps: f64,
}

/// One composition checked against `rules` (`--only`'s already picked), every frame read when `whole` or a check
/// ranges over them.
fn check_one(tsx: &Path, id: Option<String>, rules: &[Rule], args: &[String], whole: bool, host: &Host) -> R<Checked> {
    let named = systems::named();
    let mut found = Vec::new();
    let mut hows = Vec::new();
    for r in rules {
        match r.file() {
            Some(f) => match ts::meta(f, &host.ts) {
                Ok(m) => hows.push((r, How::File(m))),
                Err(e) => found.push(Finding::new(&r.id, Level::Error, e, &r.by)),
            },
            None => match named.iter().find(|n| n.name == r.check) {
                Some(n) => hows.push((r, How::Named(n.clone()))),
                None => found.push(Finding::new(&r.id, Level::Error, format!("raea has no check named {} (its checks: {}, or a .ts check's path)", r.check, named.iter().map(|n| n.name).collect::<Vec<_>>().join(", ")), &r.by)),
            },
        }
    }
    let frames: Vec<&Frames> = hows.iter().map(|(_, h)| match h { How::Named(n) => &n.frames, How::File(m) => &m.frames }).collect();
    // every frame when a check ranges over them (cuts and lines are found in them), else the frames named and the first
    let whole = whole || frames.iter().any(|f| f.whole());
    let mut asked: Vec<i64> = if whole { Vec::new() } else { frames.iter().flat_map(|f| if let Frames::At(v) = f { v.clone() } else { vec![] }).chain([0]).collect() };
    asked.sort();
    asked.dedup();
    // the render joins the first frame (what it is, or that there is none) and where a TS check needs it
    let join_all = hows.iter().any(|(_, h)| match h {
        How::File(m) => m.render && m.frames.whole(),
        How::Named(n) => n.render && n.frames.whole(),
    });
    let join_on = (!join_all).then(|| hows.iter().flat_map(|(_, h)| match h { How::File(m) if m.render => if let Frames::At(v) = &m.frames { v.clone() } else { vec![] }, _ => vec![] }).chain([0]).collect());
    let render_how = match (flag(args, "--render"), args.iter().any(|a| a == "--no-render")) {
        (Some(o), _) => Render::From(o.into()),
        (None, true) => Render::Off,
        (None, false) => Render::Newest,
    };
    let mut opts_args = vec![tsx.display().to_string()];
    if let Some(i) = &id {
        opts_args.extend(["--composition".to_string(), i.clone()]);
    }
    let opts = raea_runtime_bundle::comp_opts(&opts_args)?;
    let worlds = raea_runtime_world::read(tsx, &opts, &Ask { frames: asked, layout: true, render: render_how, join_on })?;
    let rendered = worlds.first().and_then(|w| w.resources.get("Rendered")).cloned();
    let mut waits = Vec::new();
    let needs = |w: &mut Vec<(String, String)>, r: &Rule| w.push((r.id.clone(), "a render (raea render EDIT.tsx)".into()));
    if let Some(r) = rendered.as_ref().filter(|r| r["stale"] == true) {
        found.push(Finding::new("render", Level::Note, "the newest render is older than the composition's last change: what it measured may have moved", "raea check").about(r["out"].as_str().unwrap_or("").to_string(), None));
    }
    // raea's named checks: the text forms over every frame; the floor (and the loudness and stillness it measures) on the render
    // the forms the floor measures on the video (each its form, value and line), and where each was written
    let (mut held, mut bys) = (Vec::new(), Vec::new());
    let mut floor = false;
    for (r, h) in &hows {
        let How::Named(n) = h else { continue };
        match n.name {
            "floor" => floor = true,
            "loudness" | "still-max" => {
                held.push((n.name, r.number("value").unwrap_or(if n.name == "loudness" { -14.0 } else { 3.0 }), r.text.clone()));
                bys.push(r.by.clone());
            }
            "text-read" if rendered.is_none() => needs(&mut waits, r),
            "text-read" => found.extend(crate::read::read(r, &worlds)),
            _ => found.extend(systems::text(r, &worlds)),
        }
    }
    if floor || !held.is_empty() {
        match &rendered {
            Some(r) if is_video(r["out"].as_str().unwrap_or("")) => {
                let a = [r["out"].as_str().unwrap_or("").to_string(), "--manifest".into(), r["manifest"].as_str().unwrap_or("").to_string()];
                // where text-read runs, the floor's own reading rule (held against when a text is on) is its
                let without: &[&str] = if hows.iter().any(|(_, h)| matches!(h, How::Named(n) if n.name == "text-read")) { &["reading"] } else { &[] };
                for mut f in raea_app_check::floor(&a, &held, without)? {
                    match held.iter().position(|h| h.0 == f.check) {
                        Some(k) => f.by = bys[k].clone(),
                        None if !floor => continue,
                        None => f.by = "raea".into(),
                    }
                    found.push(f);
                }
            }
            Some(_) => waits.extend(hows.iter().filter(|(_, h)| matches!(h, How::Named(n) if n.render && n.name != "text-read")).map(|(r, _)| (r.id.clone(), "a video render (its render is stills)".to_string()))),
            None => hows.iter().filter(|(_, h)| matches!(h, How::Named(n) if n.render && n.name != "text-read")).for_each(|(r, _)| needs(&mut waits, r)),
        }
    }
    // the project's own checks, in one run
    let mut files: Vec<PathBuf> = Vec::new();
    let mut plan = Vec::new();
    for (r, h) in &hows {
        let How::File(m) = h else { continue };
        if m.render && rendered.is_none() {
            needs(&mut waits, r);
            continue;
        }
        let k = file_index(&mut files, &m.file);
        let mut on = m.frames.pick(&worlds);
        if let (true, Some(j)) = (m.render, &join_on_of(&worlds)) {
            on.retain(|f| j.contains(f));
        }
        plan.push(ts::Planned { check: k, frames: on, values: r.values.clone(), level: r.level, by: r.by.clone() });
    }
    found.extend(ts::run(&files, &plan, &worlds, &host.ts)?);
    let render = rendered.map(|r| {
        // how many frames it drew, as its manifest lists them
        let m: Value = std::fs::read(r["manifest"].as_str().unwrap_or("")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        raea_app_notes::check::Rendered { out: PathBuf::from(r["out"].as_str().unwrap_or("")), stale: r["stale"] == true, frames: m["frames"].as_array().map_or(0, |f| f.len() as i64) }
    });
    let mut shots: Vec<(String, i64, Option<i64>)> = worlds.iter().flat_map(|w| w.find(&["Shot"]).into_iter().filter_map(|e| w.get::<Shot>(e.id)).filter_map(|s| Some((s.name?, s.from, s.to)))).collect();
    shots.sort();
    shots.dedup();
    let fps = worlds.first().map_or(30.0, |w| w.fps);
    Ok(Checked { file: tsx.to_path_buf(), id, found: finding::ranked(found), waits, render, shots, fps })
}

/// Where `f` is in `files`, added at the end when it isn't yet: the one run's checks, each file once.
fn file_index(files: &mut Vec<PathBuf>, f: &Path) -> usize {
    if let Some(at) = files.iter().position(|x| x == f) {
        return at;
    }
    files.push(f.to_path_buf());
    files.len() - 1
}

/// Whether `r` runs on the composition `tsx` (`id` where its file registers several) of `project`: a rule a note's ask
/// scopes to one (`of`) runs there alone.
fn applies(r: &Rule, project: &Path, tsx: &Path, id: Option<&str>) -> bool {
    r.of.as_ref().is_none_or(|of| raea_app_notes::check::is_of(of, &named_as(project, tsx, id)))
}

/// A composition as a note's ask names it: its file from the project, `#ID` after it where the file registers several.
fn named_as(project: &Path, tsx: &Path, id: Option<&str>) -> String {
    format!("{}{}", record::rel(project, tsx), id.map(|i| format!("#{i}")).unwrap_or_default())
}

/// `rules` run on `project`'s compositions (`--composition` picks one), each where it applies, as `raea check` runs
/// its own: what `raea notes check` runs a project's asks through (DR6).
pub fn run(project: &Path, rules: &[Rule], args: &[String], whole: bool, host: &Host) -> R<Vec<raea_app_notes::check::Ran>> {
    let mut out = Vec::new();
    for (tsx, id) in compositions(project, args)? {
        let mine: Vec<Rule> = rules.iter().filter(|r| applies(r, project, &tsx, id.as_deref())).cloned().collect();
        let c = check_one(&tsx, id, &mine, args, whole, host)?;
        out.push(raea_app_notes::check::Ran { of: named_as(project, &c.file, c.id.as_deref()), found: c.found, waits: c.waits, render: c.render, shots: c.shots, fps: c.fps });
    }
    Ok(out)
}

/// The frames the render joined (each with what it rendered, or says it waits), or None when it joined none.
fn join_on_of(worlds: &[Facts]) -> Option<Vec<i64>> {
    let on: Vec<i64> = worlds.iter().filter(|w| w.resources.contains_key("Rendered")).map(|w| w.frame).collect();
    (!on.is_empty()).then_some(on)
}

/// Whether `raea check`'s arguments name a file that isn't a composition (a video, a still: the floor alone, apps/check)
/// rather than a project, a folder or an EDIT.tsx.
pub fn of_video(args: &[String]) -> bool {
    raea_app_check::words(args, &TAKES).first().is_some_and(|a| !a.ends_with(".tsx") && !Path::new(a).is_dir())
}

fn is_video(out: &str) -> bool {
    Path::new(out).extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "mp4" | "mov"))
}

/// `raea check PROJECT|EDIT.tsx ...`: the report, and whether an error is among what it found.
pub fn check(args: &[String], host: &Host) -> R<(String, bool)> {
    let target = raea_app_check::words(args, &TAKES).first().map_or_else(|| PathBuf::from("."), PathBuf::from);
    // the folder it is in, made whole (`lyric.tsx`'s is the empty path): a rule's check is a path from it
    let here = if target.is_dir() { target.clone() } else { target.parent().filter(|p| !p.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), Path::to_path_buf) };
    let here = dunce::canonicalize(&here).map_err(|e| format!("{}: {e}", here.display()))?;
    let project = record::up(&here, ".raea/project").unwrap_or(here);
    let (mut rules, unread, notes) = rules(&project, taste_files(args), host);
    if let Some(only) = flag(args, "--only") {
        rules.retain(|r| r.id == only);
        if rules.is_empty() {
            return Err(format!("--only {only}: no rule here has that id"));
        }
    }
    if target.is_file() && target.to_string_lossy().ends_with(".story.ts") {
        return check_story(&target, &rules, unread, args, host);
    }
    let mut all = Vec::new();
    for (tsx, id) in compositions(&target, args)? {
        let mine: Vec<Rule> = rules.iter().filter(|r| applies(r, &project, &tsx, id.as_deref())).cloned().collect();
        all.push(check_one(&tsx, id, &mine, args, false, host)?);
    }
    let stamp = raea_io_trace::utc(std::time::SystemTime::now()).1;
    // what the notes' asks found, kept with what each has been through (DR6): an ask's first run that passes is refused
    let ran: Vec<raea_app_notes::check::Ran> = all.iter().map(|c| raea_app_notes::check::Ran { of: named_as(&project, &c.file, c.id.as_deref()), found: c.found.clone(), waits: c.waits.clone(), render: None, shots: Vec::new(), fps: c.fps }).collect();
    let mut kept = raea_app_notes::state::read(&project);
    let (asks, _) = raea_app_notes::check::settle(&project, &notes, &rules, &ran, &stamp, &mut kept);
    if !asks.is_empty() {
        raea_app_notes::state::write(&project, &kept)?;
    }
    let mut bad = unread.iter().any(|f| f.level == Level::Error);
    if args.iter().any(|a| a == "--json") {
        let v: Vec<Value> = all.iter().map(|c| json!({"file": c.file, "composition": c.id, "findings": c.found, "waits": c.waits.iter().map(|(r, w)| json!({"rule": r, "for": w})).collect::<Vec<_>>()})).collect();
        bad |= all.iter().any(|c| c.found.iter().any(|f| f.level == Level::Error));
        return Ok((serde_json::to_string_pretty(&json!({"unread": unread, "compositions": v, "asks": asks})).unwrap_or_default(), bad));
    }
    let mut out: Vec<String> = unread.iter().map(Finding::line).collect();
    let mut every = unread.clone();
    for c in &all {
        out.push(format!("raea check {}{}", c.file.display(), c.id.as_ref().map(|i| format!(" ({i})")).unwrap_or_default()));
        // a note's ask's finding says which ask, as two asks can run one check
        out.extend(c.found.iter().map(|f| if f.by.starts_with("note ") { format!("{}  [{}]", f.line(), f.by) } else { f.line() }));
        out.extend(c.waits.iter().map(|(r, w)| format!("waits {r}: needs {w}")));
        // kept in its project's record (DR3), what raea findings and raea next read
        if let Some(e) = record::keep("raea check", &c.file, None, &c.found, vec![], &stamp) {
            out.push(format!("raea check: kept in the project's record as {} (raea findings reads it)", e.id));
        }
        bad |= c.found.iter().any(|f| f.level == Level::Error);
        every.extend(c.found.iter().cloned());
    }
    out.extend(asks.iter().map(|a| format!("raea check: note {a}")));
    out.push(format!("raea check: {}", finding::count(&every)));
    Ok((out.join("\n"), bad))
}

/// `raea check NAME.story.ts` (docs/story.feature ST5a): a story checked before anything is drawn. Every rule whose .ts
/// check reads stories (`over: "story"` or `"both"`) runs once with no frames, so only its `after` speaks; the check
/// imports the story it holds (raea/story's storyCheck), and the file named says which project and that no composition
/// is wanted.
fn check_story(file: &Path, rules: &[Rule], unread: Vec<Finding>, args: &[String], host: &Host) -> R<(String, bool)> {
    let (mut found, mut files, mut plan) = (Vec::new(), Vec::<PathBuf>::new(), Vec::new());
    for r in rules {
        let Some(f) = r.file() else { continue };
        match ts::meta(f, &host.ts) {
            Ok(m) if m.story => {
                let k = file_index(&mut files, &m.file);
                plan.push(ts::Planned { check: k, frames: Vec::new(), values: r.values.clone(), level: r.level, by: r.by.clone() });
            }
            Ok(_) => {}
            Err(e) => found.push(Finding::new(&r.id, Level::Error, e, &r.by)),
        }
    }
    if plan.is_empty() {
        found.push(Finding::new("story", Level::Note, "no check here reads a story: a .ts check over \"story\" or \"both\" (raea/story's storyCheck is one) named in taste.ts", "raea check"));
    }
    found.extend(ts::run(&files, &plan, &[], &host.ts)?);
    let found = finding::ranked(found);
    let bad = unread.iter().chain(&found).any(|f| f.level == Level::Error);
    if args.iter().any(|a| a == "--json") {
        return Ok((serde_json::to_string_pretty(&json!({"unread": unread, "story": file, "findings": found})).unwrap_or_default(), bad));
    }
    let mut out: Vec<String> = unread.iter().map(Finding::line).collect();
    out.push(format!("raea check {} (a story: no frames)", file.display()));
    out.extend(found.iter().map(Finding::line));
    let every: Vec<Finding> = unread.into_iter().chain(found).collect();
    out.push(format!("raea check: {}", finding::count(&every)));
    Ok((out.join("\n"), bad))
}

/// `raea check ...`: its exit code (1 on an error found, 2 when it couldn't check).
pub fn cli(args: &[String], host: &Host) -> i32 {
    raea_app_check::said(check(args, host), "raea check: ")
}
