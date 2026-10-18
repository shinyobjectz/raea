//! raea-brief: the .brief format (format.md) read, written back, converted from a score script, and compiled.
//!
//!   raea-brief parse FILE [--space SPACE]       the intent graph as JSON; refusals on stderr as FILE:line:col, exit 2
//!                                               (with --space, its parts checked against the space's components' props)
//!   raea-brief fmt FILE                         FILE in one layout
//!   raea-brief convert SCRIPT.fountain          a score script as a .brief
//!   raea-brief ink FILE.brief                   a .brief as an ink story in the raea profile
//!   raea-brief lower STORY.ink [--path 0,1]     an ink story as the .brief its path plays (parse and compile take .ink too)
//!   raea-brief compile FILE SPACE [--o OUT] [--cut RATIO] [--variant NAME] [--no-sound]
//!                                               the piece compiled to an @raea/motion composition (OUT, default
//!                                               SPACE/NAME.tsx) by score's SDK backend; the COMPILED line and the sheet
//!   raea-brief check NAME SPACE [--video PATH]  NAME's `!` rules on its render (@raea/data/check.ts): a PASS/FAIL line a
//!                                               rule, NAME.check.json; exit 0 all pass, 1 a fail, 2 can't check
//!   raea-brief compile --all SPACE              every piece in SPACE (each *.brief with a Format:), then each shared
//!                                               brief's beats against the footage the pieces cut (score/cover.py)

use raea_brief::{convert, fmt, graph, load, load_text, parse, Loaded, Refusal};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn refuse(file: &Path, bad: &[Refusal]) -> ExitCode {
    for r in bad {
        if r.line == 0 || r.msg.contains(".brief:") || r.msg.starts_with('/') {
            eprintln!("{}", r.msg);
        } else {
            eprintln!("{}:{}:{}: {}", file.display(), r.line, r.col, r.msg);
        }
    }
    ExitCode::from(2)
}

fn path_flag() -> Vec<usize> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == "--path").and_then(|i| a.get(i + 1)).map(|p| p.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default()
}

fn graph_of(file: &Path) -> Result<serde_json::Value, ExitCode> {
    let mut docs: Vec<Loaded> = Vec::new();
    if file.extension().is_some_and(|x| x == "ink") {
        // an ink story: run along --path and lowered to .brief (raea_brief::ink), then read as a .brief is
        let src = std::fs::read_to_string(file).map_err(|e| { eprintln!("{}: {e}", file.display()); ExitCode::from(2) })?;
        let (text, _) = raea_brief::ink::lower(&src, &path_flag()).map_err(|b| refuse(file, &b))?;
        return load_text(file, Some(&text), &mut Vec::new(), &mut docs).and_then(|_| graph(&docs)).map_err(|b| {
            let lines: Vec<&str> = text.lines().collect();
            for r in &b {
                let what = lines.get(r.line.wrapping_sub(1)).map(|l| format!(" (the story plays `{}`)", l.trim())).unwrap_or_default();
                eprintln!("{}: {}{what}", file.display(), r.msg.rsplit_once(".ink:").map(|x| x.1).unwrap_or(&r.msg));
            }
            ExitCode::from(2)
        });
    }
    load(file, &mut Vec::new(), &mut docs).map_err(|b| refuse(file, &b))?;
    graph(&docs).map_err(|b| refuse(file, &b))
}

/// the space's components' props, as TypeScript types them (@raea/data/props.ts; RAEA_DATA overrides where it is)
fn props_of(space: &Path) -> Option<serde_json::Value> {
    let data = std::env::var("RAEA_DATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("raea/data"));
    if !space.join("components").exists() { return None; }
    let out = Command::new("bun").arg(data.join("src/props.ts")).arg(space).output().ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

/// the graph's parts against the space's components: their refusals, or none
fn typed(file: &Path, g: &serde_json::Value, space: &Path) -> Result<(), ExitCode> {
    match props_of(space) {
        Some(schema) => { let bad = raea_brief::check_props(g, &schema); if bad.is_empty() { Ok(()) } else { Err(refuse(file, &bad)) } }
        None => Ok(()),
    }
}

fn compile(file: &Path, space: &Path, out: Option<PathBuf>, cut: Option<String>) -> ExitCode {
    let g = match graph_of(file) { Ok(g) => g, Err(c) => return c };
    if let Err(c) = typed(file, &g, space) { return c; }
    if g["kind"] != "piece" {
        eprintln!("{}: a shared brief (no Format:): nothing to compile; a piece uses it with `Uses:`", file.display());
        return ExitCode::from(2);
    }
    let work = space.join(".score");
    let _ = std::fs::create_dir_all(&work);
    let stem = file.file_stem().unwrap().to_string_lossy().to_string();
    let gpath = work.join(format!("{stem}.graph.json"));
    std::fs::write(&gpath, serde_json::to_string_pretty(&g).unwrap()).unwrap();
    // every variant (`# hook {A}`) of every deliverable, unless --variant, --cut or --o picks: NAME-A-9x16.tsx and so on,
    // a name part left out where there's only one of it
    let ratios: Vec<String> = g["deliverables"].as_array().map(|d| d.iter().filter_map(|x| x["ratio"].as_str().map(String::from)).collect()).unwrap_or_default();
    let names: Vec<String> = g["variant_names"].as_array().map(|d| d.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let want_variant = std::env::args().collect::<Vec<_>>().windows(2).find(|w| w[0] == "--variant").map(|w| w[1].clone());
    let vs: Vec<Option<String>> = match &want_variant { Some(v) => vec![Some(v.clone())], None if names.is_empty() => vec![None], None => names.iter().cloned().map(Some).collect() };
    let rs: Vec<Option<String>> = match &cut { Some(c) => vec![Some(c.clone())], None if ratios.len() > 1 => ratios.iter().cloned().map(Some).collect(), None => vec![None] };
    if out.is_some() || (vs.len() == 1 && rs.len() == 1) {
        let o = out.unwrap_or_else(|| space.join(format!("{stem}{}{}.tsx", vs[0].as_ref().map(|v| format!("-{v}")).unwrap_or_default(), rs[0].as_ref().filter(|_| ratios.len() > 1 && cut.is_none()).map(|r| format!("-{}", r.replace(':', "x"))).unwrap_or_default())));
        return run(file, space, &gpath, &o, rs[0].clone(), vs[0].clone(), &work);
    }
    let mut worst = ExitCode::SUCCESS;
    for v in &vs {
        for r in &rs {
            let name = format!("{stem}{}{}", v.as_ref().map(|v| format!("-{v}")).unwrap_or_default(), r.as_ref().filter(|_| ratios.len() > 1).map(|r| format!("-{}", r.replace(':', "x"))).unwrap_or_default());
            println!("-- {name}");
            let c = run(file, space, &gpath, &space.join(format!("{name}.tsx")), r.clone(), v.clone(), &work);
            if c != ExitCode::SUCCESS { worst = c; }
        }
    }
    worst
}

fn run(file: &Path, space: &Path, gpath: &Path, out: &Path, cut: Option<String>, variant: Option<String>, work: &Path) -> ExitCode {
    let ostem = out.file_stem().unwrap().to_string_lossy().to_string();
    // each piece compiled has its graph under its own name (raea check finds a brief's piece by .score/NAME.graph.json)
    let own = work.join(format!("{ostem}.graph.json"));
    if own != gpath { let _ = std::fs::copy(gpath, &own); }
    let scorec = std::env::var("RAEA_SCOREC").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("raea/score/scorec.py"));
    let mut cmd = Command::new("uv");
    cmd.args(["run", "-q", "python"]).arg(&scorec).arg(space).arg(file).arg("--graph").arg(gpath).args(["--target", "sdk", "--o"]).arg(out)
        .arg("--json").arg(work.join(format!("{ostem}.ir.json")));
    if let Some(c) = cut { cmd.args(["--cut", &c]); }
    if let Some(v) = variant { cmd.args(["--variant", &v]); }
    if std::env::args().any(|a| a == "--no-sound") { cmd.arg("--no-sound"); }
    match cmd.status() {
        Ok(s) => ExitCode::from(s.code().unwrap_or(1) as u8),
        Err(e) => { eprintln!("scorec: {e}"); ExitCode::from(1) }
    }
}

fn main() -> ExitCode {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let flag = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    match a.first().map(String::as_str) {
        Some("parse") if a.len() > 1 => match graph_of(Path::new(&a[1])) {
            Ok(g) => {
                // --space SPACE: its components' props checked too
                if let Some(sp) = flag("--space") { if let Err(c) = typed(Path::new(&a[1]), &g, Path::new(&sp)) { return c; } }
                println!("{}", serde_json::to_string_pretty(&g).unwrap()); ExitCode::SUCCESS
            }
            Err(c) => c,
        },
        Some("check") if a.len() > 2 => {
            // the piece's `!` rules on its render, through @raea/data (data/src/check.ts): PASS/FAIL a rule, NAME.check.json
            let data = std::env::var("RAEA_DATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("raea/data"));
            let mut cmd = Command::new("bun");
            cmd.arg(data.join("src/check.ts")).arg(&a[1]).arg(&a[2]);
            if let Some(v) = flag("--video") { cmd.args(["--video", &v]); }
            if a.iter().any(|x| x == "--battery") { cmd.arg("--battery"); }
            match cmd.status() { Ok(s) => ExitCode::from(s.code().unwrap_or(2) as u8), Err(e) => { eprintln!("bun: {e}"); ExitCode::from(2) } }
        }
        Some("lower") if a.len() > 1 => match std::fs::read_to_string(&a[1]).map_err(|e| vec![Refusal { line: 0, col: 0, msg: e.to_string() }]).and_then(|t| raea_brief::ink::lower(&t, &path_flag())) {
            Ok((text, _)) => { print!("{text}"); ExitCode::SUCCESS }
            Err(b) => refuse(Path::new(&a[1]), &b),
        },
        Some("ink") if a.len() > 1 => match std::fs::read_to_string(&a[1]).map_err(|e| vec![Refusal { line: 0, col: 0, msg: e.to_string() }]).and_then(|t| parse(&t)) {
            Ok(d) => { print!("{}", raea_brief::ink::from_brief(&d)); ExitCode::SUCCESS }
            Err(b) => refuse(Path::new(&a[1]), &b),
        },
        Some("fmt") if a.len() > 1 => {
            let p = Path::new(&a[1]);
            match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|t| parse(&t).map_err(|b| { refuse(p, &b); String::new() })) {
                Ok(d) => { print!("{}", fmt(&d)); ExitCode::SUCCESS }
                Err(e) => { if !e.is_empty() { eprintln!("{}: {e}", p.display()); } ExitCode::from(2) }
            }
        }
        Some("convert") if a.len() > 1 => match std::fs::read_to_string(&a[1]) {
            Ok(t) => { print!("{}", convert(&t)); ExitCode::SUCCESS }
            Err(e) => { eprintln!("{}: {e}", a[1]); ExitCode::from(2) }
        },
        Some("compile") if a.get(1).map(String::as_str) == Some("--all") && a.len() > 2 => {
            let space = PathBuf::from(&a[2]);
            let mut pieces: Vec<PathBuf> = std::fs::read_dir(&space).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "brief")).collect()).unwrap_or_default();
            pieces.sort();
            pieces.retain(|p| std::fs::read_to_string(p).ok().and_then(|t| parse(&t).ok()).is_none_or(|d| d.title.iter().any(|(k, _, _)| k == "Format")));
            if pieces.is_empty() { eprintln!("{}: no piece (a .brief with a Format:) to compile", space.display()); return ExitCode::from(2); }
            let mut worst = ExitCode::SUCCESS;
            for p in &pieces {
                println!("== {}", p.file_name().unwrap().to_string_lossy());
                let c = compile(p, &space, None, None);
                if c != ExitCode::SUCCESS { worst = c; }
            }
            // the shared asks against what the pieces cut (score/cover.py)
            let cover = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("raea/score/cover.py");
            println!("== coverage");
            let _ = Command::new("uv").args(["run", "-q", "python"]).arg(&cover).arg(&space).status();
            worst
        }
        Some("compile") if a.len() > 2 => compile(Path::new(&a[1]), Path::new(&a[2]), flag("--o").map(PathBuf::from), flag("--cut")),
        _ => {
            eprintln!("raea-brief parse FILE [--space SPACE] | fmt FILE | convert SCRIPT.fountain | ink FILE.brief | lower STORY.ink | compile FILE SPACE [--o OUT] [--cut RATIO] [--variant NAME] | compile --all SPACE | check NAME SPACE [--video PATH]");
            ExitCode::from(2)
        }
    }
}
