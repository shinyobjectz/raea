//! raea-app-check-make: the Stop hook's half of `raea make` (docs/make.feature MK2): an agent working in a project whose brief.ts has a
//! `make` job can't end its turn until `raea make status` says the job is done. Cheap when there's no job: the brief's
//! words are read, and only a brief with `make:` is evaluated (by `raea make status`, a child process, so this crate
//! doesn't carry the judge). A session blocked three times with the same open items goes through: it's going in
//! circles, and the person is better told than the agent looped (PL7's keeper stops the same way).

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// How many times the same open items may block one session before the stop goes through.
pub const SAME_BLOCKS: u32 = 3;

/// The brief.ts with a job, at or above `cwd` inside its raea project.
pub fn brief_with_job(cwd: &Path) -> Option<PathBuf> {
    let project = raea_core_facts::record::up(cwd, ".raea/project");
    for d in dunce::canonicalize(cwd).ok()?.ancestors() {
        let p = d.join("brief.ts");
        if p.is_file() {
            return std::fs::read_to_string(&p).is_ok_and(|t| t.contains("make:") || t.contains("make :")).then_some(p);
        }
        if project.as_deref() == Some(d) {
            return None;
        }
    }
    None
}

fn hash(parts: &[&str]) -> u64 {
    let mut h = DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

/// Counts this session's blocks on the same open items in `guard` ("<items hash> <count>"); true while it may block.
pub fn may_block(guard: &Path, items: &str) -> bool {
    let key = hash(&[items]).to_string();
    let count = std::fs::read_to_string(guard).ok().and_then(|t| {
        let (k, n) = t.trim().split_once(' ')?;
        (k == key).then(|| n.parse::<u32>().ok()).flatten()
    });
    let next = count.unwrap_or(0) + 1;
    let _ = std::fs::create_dir_all(guard.parent().unwrap_or(Path::new(".")));
    let _ = std::fs::write(guard, format!("{key} {next}"));
    next <= SAME_BLOCKS
}

/// What the agent is told when its turn may not end, or None to let it end: no job here, the job is done, the job
/// didn't load (never block on raea's own failure), or the same items have blocked it three times already.
/// Each turn's status also brings the job's task in plan.org up to date (`--plan`, docs/make.feature MK3).
pub fn stop(v: &Value) -> Option<String> {
    let cwd = PathBuf::from(v["cwd"].as_str()?);
    let brief = brief_with_job(&cwd)?;
    let exe = std::env::current_exe().ok()?;
    let out = std::process::Command::new(exe).args(["make", "status"]).arg(brief.parent()?).args(["--json", "--plan"]).env("RAEA_RECORD", "off").output().ok()?;
    let s: Value = serde_json::from_slice(&out.stdout).ok()?;
    let home = raea_io_home::user_home()?;
    let session = v["session_id"].as_str().unwrap_or("");
    let guard = home.join(".raea/hooks/make").join(format!("{:x}", hash(&[session, &brief.display().to_string()])));
    if s["done"] == true {
        let _ = std::fs::remove_file(&guard);
        return None;
    }
    let items = open_items(&s);
    if !may_block(&guard, &items) {
        return None;
    }
    Some(format!(
        "Not done ({} job, {}): {items}. Keep going: `raea make status` lists what's open; a defect you judge wrong is answered in NOTES.md by its name.",
        s["kind"].as_str().unwrap_or("make"),
        brief.display()
    ))
}

/// The open items, one line each: what's missing, then each gate that isn't clear.
pub fn open_items(s: &Value) -> String {
    let mut items: Vec<String> = s["missing"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(String::from)).collect();
    items.extend(s["gates"].as_array().into_iter().flatten().filter(|g| g["clear"] != true).map(|g| format!("{}{}: {}", g["gate"].as_str().unwrap_or(""), g["of"].as_str().map(|o| format!(" {o}")).unwrap_or_default(), g["says"].as_str().unwrap_or(""))));
    items.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("raea-make-gate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("compositions")).unwrap();
        d
    }

    #[test]
    fn only_a_brief_with_a_job_is_held() {
        let d = tmp("job");
        std::fs::write(d.join("brief.ts"), "export default brief({ title: \"x\" })").unwrap();
        assert_eq!(brief_with_job(&d.join("compositions")), None, "a brief without make is never held");
        std::fs::write(d.join("brief.ts"), "export default brief({ title: \"x\", make: kit({ objects: [] }) })").unwrap();
        assert_eq!(brief_with_job(&d.join("compositions")).map(|p| p.file_name().unwrap().to_owned()), Some("brief.ts".into()));
    }

    #[test]
    fn the_same_open_items_block_three_times_then_go_through() {
        let g = tmp("guard").join("g");
        assert!(may_block(&g, "a") && may_block(&g, "a") && may_block(&g, "a"));
        assert!(!may_block(&g, "a"), "a fourth stop on the same items goes through");
        assert!(may_block(&g, "a; b"), "new items start the count again");
    }

    #[test]
    fn open_items_list_what_is_missing_then_the_gates_that_are_not_clear() {
        let s = json!({"missing": ["out/kit/lineup.png"], "gates": [
            {"gate": "checks", "of": "scene.tsx", "clear": false, "says": "never rendered"},
            {"gate": "critic", "clear": true, "says": "ok"}]});
        assert_eq!(open_items(&s), "out/kit/lineup.png; checks scene.tsx: never rendered");
    }
}
