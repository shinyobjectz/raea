//! Whether a job is done (docs/make.feature MK1), from what's on disk and nothing the agent says: the deliverables, the
//! notes, the record of renders and checks, and the judge's rounds. Pure over a `Job` so it is tested on fixtures.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use raea_core_brief::make::{self, Make};
use raea_core_facts::{Level, record::Entry};
use serde_json::{Value, json};

/// The heading the notes end with, so the next job (and the skill) learns from this one.
pub const SKILL_HEADING: &str = "What the skill got wrong";

/// Everything status reads, gathered by `lib.rs` (or a test).
pub struct Job<'a> {
    pub make: &'a Make,
    /// the brief's folder: deliverables, NOTES.md and review/ are relative to it
    pub dir: &'a Path,
    /// the project's folder (record paths are relative to it), when it is a raea project
    pub project: Option<&'a Path>,
    pub record: &'a [Entry],
    /// review/judge.log.jsonl's rounds, oldest first
    pub rounds: &'a [Value],
    /// the latest round's judge.json, when there is one
    pub verdict: Option<&'a Value>,
    /// when the judge last wrote its log
    pub judged_at: Option<SystemTime>,
    /// `raea ask`'s ledger, for the make questions' gate (asked.rs)
    pub ledger: &'a crate::asked::Ledger,
    /// `raea judge calibrate --kind KIND` agreed with the person's eye for this kind: its score gates too (MK6)
    pub score_gates: bool,
}

/// A round's folder: as the judge wrote it when absolute, else the folder of that name beside the log (review/), since
/// `raea judge DIR` keeps its log in DIR's parent and DIR was written relative to wherever it ran.
pub fn round_dir(dir: &Path, round: &Value) -> Option<PathBuf> {
    let d = Path::new(round["dir"].as_str()?);
    if d.is_absolute() {
        return Some(d.to_path_buf());
    }
    Some(dir.join("review").join(d.file_name()?))
}

/// The sheets the make questions are asked of: the newest round's (its framing sheet for a shot list, its colour and
/// clay sheets otherwise) and a kit's reference sheet.
pub fn sheets(job: &Job) -> Vec<(&'static str, PathBuf)> {
    let mut out = Vec::new();
    if let Some(r) = job.rounds.last().and_then(|r| round_dir(job.dir, r)) {
        if job.make.kind == make::Kind::Shot {
            out.push(("framing", r.join("framing.png")));
        } else {
            out.push(("clay", r.join("clay.png")));
            out.push(("colour", r.join("colour.png")));
        }
    }
    if job.make.kind == make::Kind::Kit {
        out.push(("refs", job.dir.join("review/refs.png")));
    }
    out.into_iter().filter(|(_, f)| f.is_file()).collect()
}

pub(crate) fn kind_name(k: make::Kind) -> &'static str {
    match k {
        make::Kind::Scene => "scene",
        make::Kind::Kit => "kit",
        make::Kind::Shot => "shot",
    }
}

/// When a file last changed, and that as seconds since 1970 (a record entry's clock).
fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok()?.modified().ok()
}
fn mtime_secs(p: &Path) -> Option<u64> {
    Some(mtime(p)?.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs())
}

fn is_picture(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("png" | "jpg" | "jpeg" | "exr"))
}

/// One deliverable: a file, `a|b` (either), or `dir/` (three or more pictures in it). Returns the files that answer it.
fn answered(dir: &Path, want: &str) -> Result<Vec<PathBuf>, String> {
    if let Some(folder) = want.strip_suffix('/') {
        let pics: Vec<PathBuf> = std::fs::read_dir(dir.join(folder)).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| is_picture(p)).collect();
        return if pics.len() >= 3 { Ok(pics) } else { Err(format!("{want} holds {} picture{}; three or more", pics.len(), if pics.len() == 1 { "" } else { "s" })) };
    }
    let found: Vec<PathBuf> = want.split('|').map(|w| dir.join(w)).filter(|p| p.is_file()).collect();
    if found.is_empty() { Err(want.replace('|', " or ")) } else { Ok(found) }
}

fn notes_text(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join("NOTES.md")).ok()
}

/// AnyKit's stall: three rounds in a row without beating the best mean before them. Not the judge's own `stall`
/// (the same defect twice: change the approach), which is advice to the agent, not a reason to let it stop.
pub fn stalled(rounds: &[Value]) -> bool {
    let means: Vec<f64> = rounds.iter().filter_map(|r| r["mean"].as_f64()).collect();
    means.len() >= 4 && {
        let (before, last) = means.split_at(means.len() - 3);
        last.iter().cloned().fold(f64::MIN, f64::max) <= before.iter().cloned().fold(f64::MIN, f64::max)
    }
}

fn gate(name: &str, of: Option<&str>, clear: bool, says: String) -> Value {
    let mut g = json!({"gate": name, "clear": clear, "says": says});
    if let Some(of) = of {
        g["of"] = json!(of);
    }
    g
}

/// Tier 1: each composition the plan names has a render or check newer than it that found no error.
fn checks(job: &Job) -> Vec<Value> {
    job.make
        .compositions
        .iter()
        .map(|c| {
            let file = job.dir.join(c);
            let rel = job.project.map(|p| raea_core_facts::record::rel(p, &file)).unwrap_or_else(|| c.clone());
            let Some(last) = job.record.iter().rev().find(|e| e.of == rel) else {
                return gate("checks", Some(c), false, format!("never rendered or checked: raea render {c} (or raea scene lint {c})"));
            };
            let changed = mtime_secs(&file);
            if changed.is_some_and(|t| t > last.at) {
                return gate("checks", Some(c), false, format!("changed since its last {} ({}): run it again", last.by, last.id));
            }
            let errors: Vec<&str> = last.findings.iter().filter(|f| f.level == Level::Error).map(|f| f.says.as_str()).collect();
            match errors.as_slice() {
                [] => gate("checks", Some(c), true, format!("clean at {} ({})", last.id, last.by)),
                [one] => gate("checks", Some(c), false, format!("{} found: {one}", last.id)),
                many => gate("checks", Some(c), false, format!("{} found {} errors, first: {}", last.id, many.len(), many[0])),
            }
        })
        .collect()
}

/// Tier 3: a fresh critic has judged a round newer than the newest deliverable, and each blocking defect it named is
/// fixed (the round passed) or answered in NOTES.md by its name. Its score gates once the judge is calibrated.
fn critic(job: &Job, newest: Option<SystemTime>, notes: &str) -> Vec<Value> {
    let Some(last) = job.rounds.last() else {
        let kind = kind_name(job.make.kind);
        let what = match job.make.kind {
            make::Kind::Kit => "raea render shared/components/KIT/lineup.tsx --review review/r1",
            make::Kind::Shot => "raea make framing --out review/r1",
            make::Kind::Scene => "raea render EDIT.tsx --review review/r1",
        };
        return vec![gate("critic", None, false, format!("no round yet: {what}, then raea judge review/r1 --kind {kind} --by claude --brief brief.ts"))];
    };
    let mut out = Vec::new();
    let fresh = match (job.judged_at, newest) {
        (Some(j), Some(n)) => j >= n,
        (Some(_), None) => true,
        _ => false,
    };
    out.push(gate("critic", None, fresh, if fresh { format!("round {} judged after the newest deliverable", job.rounds.len()) } else { "the newest deliverable came after the last round: review and judge it again".into() }));
    // the kind's own rubric (MK5): a round judged on another's doesn't count
    let kind = kind_name(job.make.kind);
    if last["kind"].as_str() != Some(kind) {
        let dir = last["dir"].as_str().unwrap_or("review/rN");
        out.push(gate("rubric", None, false, format!("the last round wasn't judged on the {kind} rubric: raea judge {dir} --kind {kind} --by claude")));
    }
    let blocking: Vec<&str> = raea_app_judge::rubric::blocking(last["defects"].as_array().map(Vec::as_slice).unwrap_or_default());
    let open: Vec<&str> = if last["pass"] == true { Vec::new() } else { blocking.into_iter().filter(|d| !notes.contains(d)).collect() };
    out.push(gate("defects", None, open.is_empty(), if open.is_empty() { "no blocking defect left unanswered".into() } else { format!("blocking, neither fixed nor answered in NOTES.md: {}", open.join(", ")) }));
    // the score gates once the judge is calibrated for this kind; before that, fixed-or-answered is enough (MK6)
    if job.score_gates {
        let p = raea_app_judge::progress(job.rounds);
        let done = p["done"] == true;
        out.push(gate("score", None, done, format!("the calibrated judge: {} passes in a row, two needed", p["passesInARow"])));
    }
    out
}

/// The job's status: what's missing, each gate, the judge's rounds, and whether the agent may stop.
pub fn status(job: &Job) -> Value {
    let mut missing = Vec::new();
    let mut newest: Option<SystemTime> = None;
    for want in make::deliverables(job.make) {
        match answered(job.dir, &want) {
            Ok(files) => newest = files.iter().filter_map(|f| mtime(f)).chain(newest).max(),
            Err(why) => missing.push(why),
        }
    }
    let notes = notes_text(job.dir).unwrap_or_default();
    if !notes.to_lowercase().contains(&SKILL_HEADING.to_lowercase()) {
        missing.push(format!("NOTES.md with a \"{SKILL_HEADING}\" section (one line per thing that cost time; what the guide got wrong for this job)"));
    }
    let mut gates = checks(job);
    gates.extend(critic(job, newest, &notes));
    let (asks, said) = crate::asked::gates(&sheets(job), job.ledger, &notes);
    gates.extend(asks);
    let cap = job.make.rounds.unwrap_or(make::ROUNDS) as usize;
    let capped = job.rounds.len() >= cap;
    let stalled = stalled(job.rounds);
    let clear = missing.is_empty() && gates.iter().all(|g| g["clear"] == true);
    let mut why = Vec::new();
    if capped && !clear {
        why.push(format!("capped: {} critic rounds, the most this job gets; delivered with what's still missing", job.rounds.len()));
    }
    if stalled && !clear {
        why.push("stalled: three rounds without beating the best before them; delivered with what's still missing".into());
    }
    let advice = raea_app_judge::progress(job.rounds);
    json!({
        "kind": job.make.kind,
        "done": clear || capped || stalled,
        "short": !clear && (capped || stalled),
        "missing": missing,
        "gates": gates,
        "judge": {"rounds": job.rounds.len(), "cap": cap, "passesInARow": advice["passesInARow"], "changeApproach": advice["stall"], "again": advice["again"]},
        "plan": make::doctor(job.make),
        "advice": said,
        "why": why,
    })
}

/// The status for a person or an agent to read.
pub fn text(s: &Value) -> String {
    let mut t = vec![format!("make: {} {}", s["kind"].as_str().unwrap_or("?"), if s["done"] == true { if s["short"] == true { "delivered short" } else { "done" } } else { "not done" })];
    for w in s["why"].as_array().into_iter().flatten() {
        t.push(format!("  {}", w.as_str().unwrap_or("")));
    }
    for (key, head) in [("plan", "plan:"), ("missing", "missing:"), ("advice", "advice (raea ask, not calibrated yet):")] {
        let items: Vec<&str> = s[key].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if !items.is_empty() {
            t.push(head.into());
            t.extend(items.iter().map(|i| format!("  {i}")));
        }
    }
    t.push("gates:".into());
    for g in s["gates"].as_array().into_iter().flatten() {
        let of = g["of"].as_str().map(|o| format!(" {o}")).unwrap_or_default();
        t.push(format!("  {} {}{of}: {}", if g["clear"] == true { "ok  " } else { "open" }, g["gate"].as_str().unwrap_or(""), g["says"].as_str().unwrap_or("")));
    }
    let j = &s["judge"];
    t.push(format!("judge: {} of {} rounds, {} passes in a row{}", j["rounds"], j["cap"], j["passesInARow"], if j["changeApproach"] == true { "; the same defect keeps coming back: change the approach for that part, not the dose" } else { "" }));
    t.join("\n")
}
