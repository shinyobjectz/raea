//! `raea make`: a scene, kit or shot made to its frozen plan (`make` on brief.ts, core/brief make.rs) and held to the
//! review loop (docs/make.feature). `status` decides, in code, what's missing and whether the agent may stop; the Stop
//! hook (apps/check hook.rs) asks it at the end of every turn.
//!
//!   raea make status [DIR] [--json] [--plan]     (--plan: the gates as check lines in plan.org, MK3)
//!   raea make plan|refs|sheet refs|gen|pick-sheet|pick|prep|kit|framing|lenschart ...   (the steps, USAGE)

pub mod asked;
pub mod floor;
pub mod kit;
pub mod kitout;
pub mod plan;
pub mod sheet;
pub mod shots;
pub mod status;

use std::path::{Path, PathBuf};

use raea_core_brief::make::Make;
use serde_json::Value;

pub const USAGE: &str = "raea make status [DIR] [--json] [--plan]   (a scene, kit or shot held to its plan, `make` on brief.ts: what's missing, each gate (renders and checks, raea ask's calibrated questions, the critic on the kind's rubric), the judge's rounds, and whether it's done, decided in code; --plan writes the gates as check lines under the job's task in plan.org; the agent's Stop hook holds it there)
       raea make plan [DIR] [--no-review]   (the plan stop: review/plan/floor-plan.png, a grey-box blockout from the entrance and its review sheets, comparison.png beside reference 0)
       raea make refs [DIR] [--only ID,...] [--model M] [--dry]   (a kit: every object's reference from the style images, in the plan's style, then review/refs.png; --dry quotes it)
       raea make sheet refs [DIR]   (review/refs.png again: the style images, then every reference)
       raea make gen [DIR] [--only ID,...] [--tripo M] [--meshy M] [--dry]   (each reference to both generators, Tripo H3.1 and Meshy on fal: out/kit/gen/ID/)
       raea make pick-sheet [DIR] [--only ID,...]   (review/picks.png: reference | Tripo | Tripo clay | Meshy | Meshy clay, fitted to size)
       raea make pick ID tripo|meshy \"why\" [DIR]   (the pick and its reason, out/kit/picks.json)
       raea make prep [DIR] [--only ID,...]   (each pick fitted to its real size, a quarter-triangle _lod1 beside it, .glb.json sidecars)
       raea make kit [DIR] [--name KIT] [--to FOLDER]   (the kit as a component set: shared/components/KIT, lineup.tsx, cover.png, thumbs, kit.json)
       raea make framing [DIR] [--only NAME,...] [--out DIR]   (a shot list: each shot's first, middle and last frame on framing.png, stamped)
       raea make lenschart SHOT [DIR] [--out DIR]   (a shot's first frame at 18, 24, 35, 50, 85 and 135 mm on lens-SHOT.png)";

/// The brief.ts the job is read from: DIR's own, else the nearest above it inside its raea project.
pub fn brief_path(dir: &Path) -> Option<PathBuf> {
    let dir = dunce::canonicalize(dir).ok()?;
    let project = raea_core_facts::record::up(&dir, ".raea/project");
    for d in dir.ancestors() {
        let p = d.join("brief.ts");
        if p.is_file() {
            return Some(p);
        }
        if project.as_deref() == Some(d) {
            break;
        }
    }
    None
}

/// Whether a brief.ts has a job at all, read from its words: cheap enough for a hook at every turn's end, so the
/// module is only evaluated when it can matter.
pub fn has_make(brief: &Path) -> bool {
    std::fs::read_to_string(brief).is_ok_and(|t| t.contains("make:") || t.contains("make :"))
}

/// The plan from brief.ts, evaluated by `raea spec FILE --as brief` (the module runs; Rust checks it against core/brief).
pub fn read_make(brief: &Path) -> Result<Option<Make>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = std::process::Command::new(exe).arg("spec").arg(brief).args(["--as", "brief"]).env("RAEA_RECORD", "off").output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let start = text.find('{').ok_or_else(|| format!("{} didn't read as a brief: {}", brief.display(), String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("")))?;
    let v: Value = serde_json::from_str(&text[start..]).map_err(|e| format!("{}: {e}", brief.display()))?;
    let b = v.get("brief").cloned().unwrap_or(v);
    match b.get("make") {
        None | Some(Value::Null) => Ok(None),
        Some(m) => serde_json::from_value(m.clone()).map(Some).map_err(|e| format!("{}: make: {e}", brief.display())),
    }
}

/// The judge's rounds beside the brief (review/judge.log.jsonl, as `raea judge` keeps them), the newest round's
/// verdict (judge.json, or judge.BY.json for an agent critic), and when the log was last written.
fn judged(dir: &Path) -> (Vec<Value>, Option<Value>, Option<std::time::SystemTime>) {
    let log = dir.join("review").join("judge.log.jsonl");
    let rounds: Vec<Value> = std::fs::read_to_string(&log).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let verdict = rounds.last().and_then(|r| {
        let d = status::round_dir(dir, r)?;
        let file = r["by"].as_str().filter(|b| *b != "clef").map_or_else(|| "judge.json".to_string(), |b| format!("judge.{b}.json"));
        serde_json::from_str(&std::fs::read_to_string(d.join(file)).ok()?).ok()
    });
    let at = std::fs::metadata(&log).and_then(|m| m.modified()).ok();
    (rounds, verdict, at)
}

/// The status of the job whose brief is at `brief`, with its plan already read.
pub fn status_of(brief: &Path, make: &Make) -> Value {
    let dir = brief.parent().unwrap_or(Path::new("."));
    let project = raea_core_facts::record::up(dir, ".raea/project");
    let record = project.as_deref().map(raea_core_facts::record::read).unwrap_or_default();
    let (rounds, verdict, judged_at) = judged(dir);
    let ledger = asked::Ledger::read();
    let job = status::Job { make, dir, project: project.as_deref(), record: &record, rounds: &rounds, verdict: verdict.as_ref(), judged_at, ledger: &ledger, score_gates: raea_app_judge::kind_gates(status::kind_name(make.kind)) };
    let mut s = status::status(&job);
    s["brief"] = Value::String(brief.display().to_string());
    s
}

/// For the hook: the job's status from a folder, None when there's no job there (no brief.ts, or no `make` on it).
pub fn status_in(dir: &Path) -> Result<Option<Value>, String> {
    let Some(brief) = brief_path(dir).filter(|b| has_make(b)) else { return Ok(None) };
    let Some(make) = read_make(&brief)? else { return Ok(None) };
    Ok(Some(status_of(&brief, &make)))
}

/// The flags that take a value: what follows them isn't a positional argument.
const VALUED: [&str; 7] = ["--only", "--model", "--tripo", "--meshy", "--name", "--to", "--out"];

fn flag<'a>(args: &'a [String], k: &str) -> Option<&'a str> {
    args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// The positional arguments after the verb (words), flags and their values left out.
pub fn positional(args: &[String], skip: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = skip;
    while i < args.len() {
        if VALUED.contains(&args[i].as_str()) {
            i += 2;
            continue;
        }
        if !args[i].starts_with("--") {
            out.push(args[i].clone());
        }
        i += 1;
    }
    out
}

/// A flag's comma list (`--only a,b`), empty when it isn't given.
fn list(args: &[String], k: &str) -> Vec<String> {
    let Some(v) = flag(args, k) else { return vec![] };
    v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

/// What `--only` names of a list (all of it when it names nothing), each item known by `name`.
pub(crate) fn only<'a, T>(all: &'a [T], only: &[String], name: impl Fn(&T) -> &str) -> Vec<&'a T> {
    all.iter().filter(|t| only.is_empty() || only.iter().any(|n| n.as_str() == name(t))).collect()
}

/// The job's brief and plan from a folder.
fn job(dir: &Path) -> Result<(PathBuf, Make), String> {
    let brief = brief_path(dir).ok_or_else(|| format!("raea make: no brief.ts at or above {} in its project", dir.display()))?;
    let make = read_make(&brief)?.ok_or_else(|| format!("raea make: {} has no `make` (scene(), kit() or shot() from raea/brief)", brief.display()))?;
    Ok((brief, make))
}

/// The steps: each reads the plan from DIR's brief.ts and works beside it.
fn step(verb: &str, args: &[String]) -> Result<String, String> {
    let skip = if verb == "sheet" { 3 } else { 2 };
    let pos = positional(args, skip);
    let (words, dir_at) = match verb {
        "pick" => (3, 3),
        "lenschart" => (1, 1),
        _ => (0, 0),
    };
    if pos.len() < words {
        return Err(USAGE.into());
    }
    let dir = pos.get(dir_at).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let (brief, m) = job(&dir)?;
    let here = brief.parent().unwrap_or(Path::new(".")).to_path_buf();
    let only = list(args, "--only");
    let dry = args.iter().any(|a| a == "--dry");
    let out_or = |d: PathBuf| flag(args, "--out").map(PathBuf::from).unwrap_or(d);
    match verb {
        "plan" => floor::draw_plan(&here, &m, !args.iter().any(|a| a == "--no-review")),
        "refs" => kit::refs(&here, &m, &only, flag(args, "--model"), dry),
        "sheet" => match args.get(2).map(String::as_str) {
            Some("refs") => kit::sheet_refs(&here, &m),
            Some("picks") => kitout::pick_sheet(&here, &m, &only),
            Some("framing") => shots::framing(&here, &m, &only, &out_or(here.join("review"))),
            _ => Err("raea make sheet refs|picks|framing [DIR]".into()),
        },
        "gen" => kit::generate(&here, &m, &only, flag(args, "--tripo").unwrap_or(kit::TRIPO), flag(args, "--meshy").unwrap_or(kit::MESHY), dry),
        "pick-sheet" => kitout::pick_sheet(&here, &m, &only),
        "pick" => kit::choose(&here, &m, &pos[0], &pos[1], &pos[2]),
        "prep" => kit::prep(&here, &m, &only),
        "kit" => kitout::land(&here, &m, flag(args, "--name"), flag(args, "--to")),
        "framing" => shots::framing(&here, &m, &only, &out_or(here.join("review"))),
        "lenschart" => shots::lenschart(&here, &m, &pos[0], &out_or(here.join("review"))),
        _ => Err(USAGE.into()),
    }
}

pub fn cli(args: &[String]) -> Result<String, String> {
    match args.get(1).map(String::as_str) {
        Some("status") => {}
        Some(v @ ("plan" | "refs" | "sheet" | "gen" | "pick-sheet" | "pick" | "prep" | "kit" | "framing" | "lenschart")) => return step(v, args),
        _ => return Err(USAGE.into()),
    }
    let dir = positional(args, 2).first().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let (brief, make) = job(&dir)?;
    let mut s = status_of(&brief, &make);
    if args.iter().any(|a| a == "--plan") {
        let dir = brief.parent().unwrap_or(Path::new("."));
        let root = raea_core_facts::record::up(dir, ".raea/project").unwrap_or_else(|| dir.to_path_buf());
        let kind = s["kind"].as_str().unwrap_or("make").to_string();
        let (file, task) = plan::write(&root, &brief, &kind, &s)?;
        s["planOrg"] = Value::String(file.display().to_string());
        s["planTask"] = Value::String(task);
    }
    if args.iter().any(|a| a == "--json") {
        return Ok(serde_json::to_string_pretty(&s).unwrap_or_default());
    }
    let mut t = status::text(&s);
    if let (Some(p), Some(task)) = (s["planOrg"].as_str(), s["planTask"].as_str()) {
        t.push_str(&format!("\nplan.org: the gates are check lines under task {task} in {p}"));
    }
    Ok(t)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_steps;
