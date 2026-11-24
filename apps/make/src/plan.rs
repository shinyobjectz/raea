//! The job's gates in the project's plan.org (docs/make.feature MK3; docs/plan.feature PL1): one task, its :ID: from the
//! brief's path, each gate and each missing thing a check line under it with a progress cookie, so the person, the app
//! and raea cloud read the same list the Stop hook holds the agent to. MAKING while it isn't done, REVIEW once it is
//! (the person closes it); a task the person marked DONE or DROPPED keeps their mark, and their own lines under it stay.

use std::path::Path;

use serde_json::Value;

/// The task's id: t- and eight letters and digits (PL1b), the same for the same brief.
pub fn task_id(brief_rel: &str) -> String {
    let h = brief_rel.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    format!("t-mk{:06x}", h & 0xff_ffff)
}

fn is_check(line: &str) -> bool {
    let t = line.trim_start();
    let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("+ "));
    rest.is_some_and(|r| r.starts_with("[ ]") || r.starts_with("[X]") || r.starts_with("[x]") || r.starts_with("[-]"))
}

fn short(s: &str) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() > 140 { format!("{}…", s.chars().take(139).collect::<String>()) } else { s }
}

/// The check lines for a status: what's missing, then each gate.
pub fn checks(status: &Value) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = Vec::new();
    for m in status["missing"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        out.push((false, format!("missing: {}", short(m))));
    }
    for g in status["gates"].as_array().into_iter().flatten() {
        let of = g["of"].as_str().map(|o| format!(" {o}")).unwrap_or_default();
        out.push((g["clear"] == true, format!("{}{of}: {}", g["gate"].as_str().unwrap_or("gate"), short(g["says"].as_str().unwrap_or("")))));
    }
    out
}

/// plan.org's text with the job's task brought up to date (made at the end when it isn't there).
pub fn update(text: &str, title: &str, brief_rel: &str, kind: &str, status: &Value) -> String {
    let id = task_id(brief_rel);
    let lines: Vec<&str> = text.lines().collect();
    let done = status["done"] == true;
    let cs = checks(status);
    let cookie = format!("[{}/{}]", cs.iter().filter(|c| c.0).count(), cs.len());
    let check_lines: Vec<String> = cs.iter().map(|(d, t)| format!("- [{}] {t}", if *d { "X" } else { " " })).collect();
    let id_line = lines.iter().position(|l| l.trim() == format!(":ID: {id}") || l.trim() == format!(":ID:      {id}"));
    let Some(id_at) = id_line else {
        let mut out = if text.trim().is_empty() { format!("#+TITLE: {title}\n#+TODO: TODO MAKING REVIEW | DONE DROPPED\n") } else { text.trim_end().to_string() + "\n" };
        out.push_str(&format!("\n* {} make: {kind} ({brief_rel}) {cookie}\n:PROPERTIES:\n:ID: {id}\n:KIND: make\n:VERB: {kind}\n:END:\n", if done { "REVIEW" } else { "MAKING" }));
        out.push_str(&(check_lines.join("\n") + "\n"));
        return out;
    };
    let head = (0..id_at).rev().find(|&i| lines[i].starts_with('*')).unwrap_or(0);
    let stars = lines[head].chars().take_while(|c| *c == '*').count();
    let end = (id_at + 1..lines.len()).find(|&i| lines[i].starts_with('*') && lines[i].chars().take_while(|c| *c == '*').count() <= stars).unwrap_or(lines.len());
    let drawer_end = (id_at..end).find(|&i| lines[i].trim() == ":END:").unwrap_or(id_at);
    // the headline: its keyword brought up to date unless the person closed it, its cookie replaced
    let words: Vec<&str> = lines[head][stars..].split_whitespace().collect();
    let kw = words.first().copied().unwrap_or("");
    let closed = kw == "DONE" || kw == "DROPPED";
    let rest: Vec<&str> = words.iter().skip(if ["TODO", "MAKING", "REVIEW", "DONE", "DROPPED"].contains(&kw) { 1 } else { 0 }).copied().filter(|w| !(w.starts_with('[') && w.ends_with(']') && w.contains('/'))).collect();
    let kw = if closed { kw } else if done { "REVIEW" } else { "MAKING" };
    let headline = format!("{} {kw} {} {cookie}", "*".repeat(stars), rest.join(" "));
    let theirs: Vec<&str> = lines[drawer_end + 1..end].iter().copied().filter(|l| !is_check(l)).collect();
    let mut out: Vec<String> = lines[..head].iter().map(|l| l.to_string()).collect();
    out.push(headline);
    out.extend(lines[head + 1..=drawer_end].iter().map(|l| l.to_string()));
    out.extend(check_lines);
    out.extend(theirs.iter().map(|l| l.to_string()));
    out.extend(lines[end..].iter().map(|l| l.to_string()));
    out.join("\n") + "\n"
}

/// Writes the job's task into plan.org at the top of `root` (the project's folder, else the brief's): the file and the
/// task's id.
pub fn write(root: &Path, brief: &Path, kind: &str, status: &Value) -> Result<(std::path::PathBuf, String), String> {
    let file = root.join("plan.org");
    let rel = brief.strip_prefix(root).unwrap_or(brief).to_string_lossy().replace('\\', "/");
    let title = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "plan".into());
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let next = update(&text, &title, &rel, kind, status);
    if next != text {
        replace(&file, &next).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok((file, task_id(&rel)))
}

/// A file's text replaced: written beside it, then moved over it, so a reader never sees half a plan.
fn replace(file: &Path, text: &str) -> std::io::Result<()> {
    let tmp = file.with_extension("org.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, file)
}
