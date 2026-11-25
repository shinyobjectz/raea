//! `raea next` (docs/skills.feature SK10): the two or three useful next steps for a project, read from its files: the
//! brief (what it still needs, from the raea CLI's brief doctor), the storyboard its modes want, its compositions, and
//! from the project's record (docs/direction.feature DR3) its renders and their checks and what each found. A menu for
//! the agent or the person to pick from; nothing runs.
use super::*;
use raea_core_facts::Level;
use raea_core_facts::record::{self, Entry};
use std::time::{SystemTime, UNIX_EPOCH};

/// What the raea CLI read from the project's brief.
pub struct Brief {
    /// what the brief still needs (empty when it's shaped)
    pub needs: Vec<String>,
    pub modes: Vec<String>,
    /// what the person approves before the work goes on (the brief's approve)
    pub approve: Vec<String>,
}

fn modified(p: &Path) -> SystemTime {
    std::fs::metadata(p).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH)
}

fn files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file() && p.to_string_lossy().ends_with(ext)).collect();
    v.sort_by_key(|p| std::cmp::Reverse(modified(p)));
    v
}

fn name(p: &Path) -> String {
    let full = p.to_string_lossy();
    full.rsplit(['/', '\\']).next().unwrap_or_default().to_string()
}

/// An entry's first error, as `check: says`.
fn first_error(e: &Entry) -> Option<String> {
    e.findings.iter().find(|f| f.level == Level::Error).map(|f| [f.check.as_str(), f.says.as_str()].join(": "))
}

/// The step after a composition's latest render in the project's record (docs/direction.feature DR3): its errors,
/// then its video (the newest render of it that wrote one: stills or a review since don't replace it, and one made
/// before the composition's last change, at `changed` seconds, is stale), then the video's check (a `raea check` of
/// it, kept after it) and what that found.
fn after_render(c: &Path, record: &[Entry], i: usize, changed: u64) -> String {
    let e = &record[i];
    if let Some(err) = first_error(e) {
        return format!("fix {}: {err} (its render {} found it; `raea findings {}` lists them all)", name(c), e.id, name(c));
    }
    let video = record[..=i].iter().rposition(|k| k.by == "raea render" && k.of == e.of && k.out.as_deref().is_some_and(|o| o.ends_with(".mp4")));
    let Some((v, out)) = video.and_then(|v| Some((v, record[v].out.as_deref()?))) else {
        return format!("render the video: `raea render {} -o out/{}`", name(c), name(&c.with_extension("mp4")));
    };
    if record[v].at < changed {
        return format!("render the video again: {} changed after {} made {out}", name(c), record[v].id);
    }
    let check = out.replace(".mp4", ".check.json");
    match record[v + 1..].iter().rev().find(|k| (k.by.as_str(), k.of.as_str()) == ("raea check", out)) {
        None => format!("check {out}: `raea check {out} -o {check}` (add --manifest when the render wrote one)"),
        Some(k) => match first_error(k) {
            Some(err) => format!("polish {out}: {err} (`raea guide polish`)"),
            None => format!("show {out}: post it to the project's feed (feed_post), or give the person its path, and read their notes"),
        },
    }
}

/// The steps, most useful first, at most three. `stale` is what `raea doctor` found here.
pub fn steps(dir: &Path, brief: &Brief, stale: usize) -> Vec<String> {
    let mut out = Vec::new();
    if stale > 0 {
        out.push(format!("raea doctor: {stale} thing(s) in this project's files or your raea skill don't match this raea (`raea doctor --fix`)"));
    }
    if let Some(first) = brief.needs.first() {
        out.push(format!("shape the brief: {first} (ask the person, or write your best guess and add it to `assumed`)"));
    }
    // the project's choice (the brief's approve: ["script"]) or its modes': a script approved before anything is edited
    let approves = brief.approve.iter().any(|a| a == "script");
    let boarded = approves || brief.modes.iter().any(|m| ["pitch", "explain", "story"].contains(&m.as_str()));
    let scripts = files(dir, ".script.ts");
    if boarded && scripts.is_empty() {
        out.push("plan it: write the storyboard as NAME.script.ts beside the brief (`raea spec NAME.script.ts` checks it) and wait for the person's approval".into());
    } else if approves && let Some(s) = scripts.first() && crate::registry::scan(dir).composition_files().is_empty() {
        out.push(format!("this project approves its script before anything is edited: show {} (`raea show {}`) and edit nothing until the person approves it", name(s), name(s)));
    }
    // the compositions as raea's project system reads them (PJ8), newest first; what isn't connected is a step before
    let reg = crate::registry::scan(dir);
    let loose: Vec<&crate::registry::Lint> = reg.warnings().collect();
    if let Some(first) = loose.first() {
        out.push(format!("connect the project's files: {} thing(s) aren't connected, first {} (`raea project scan` lists them, `raea doctor --fix` fixes what it can)", loose.len(), first.line()));
    }
    // the newest composition's renders and checks, from the project's record (DR3)
    let comps = reg.composition_files();
    let project = record::up(dir, ".raea/project").unwrap_or_else(|| dir.to_path_buf());
    let kept = record::read(&project);
    let secs = |p: &Path| modified(p).duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    match comps.first() {
        None => out.push("make the first item: one .tsx whose root registers a Composition (`raea guide craft` first)".into()),
        Some(c) => match kept.iter().rposition(|e| e.by == "raea render" && e.of == record::rel(&project, c)) {
            None => out.push(format!("render {}: stills to look at first (`raea render {} --at 0,45,89 -o frames/`), then the video into out/", name(c), name(c))),
            Some(i) if secs(c) > kept[i].at => out.push(format!("render again: {} changed after its render {}", name(c), kept[i].id)),
            Some(i) => out.push(after_render(c, &kept, i, secs(c))),
        },
    }
    out.truncate(3);
    out
}

/// `raea next`'s text.
pub fn text(dir: &Path, brief: &Brief, stale: usize) -> String {
    let s = steps(dir, brief, stale);
    let mut t = format!("raea next: {}\n", dir.display());
    for (i, line) in s.iter().enumerate() {
        t.push_str(&format!("{}. {line}\n", i + 1));
    }
    t.trim_end().to_string()
}
