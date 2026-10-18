//! Skills and commands: what's on the computer, for the agent and for the person's slash commands (the user,
//! 2026-10-17: "slash commands that sync to skills ... pulling skills from whatever's on the computer ... our own
//! commands for the terminal").
//!
//! A skill is a folder with a SKILL.md (front matter `name` and `description`, then the instructions), the format
//! Claude Code uses, found in, first match by name winning:
//!   <project>/.void/skills/, ~/.raea-void/skills/, <project>/.claude/skills/, ~/.claude/skills/ (and one level of
//!   subfolders under each, as Claude Code's synced skills are kept)
//! A command is a markdown file whose body is a prompt, `$ARGUMENTS` standing for what follows it (`/review the hook`),
//! found in <project>/.void/commands/, ~/.raea-void/commands/, ~/.claude/commands/.
//! The agent sees each skill's name and description at the start of a turn and reads one whole with `raea skill NAME`;
//! the person runs `/NAME args` for a skill or a command, and `/skills` lists them.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Its folder (SKILL.md is in it, with whatever files the skill brings).
    pub dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub body: String,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// Front matter's `key: value` (quoted or not) and the body after it.
fn front(text: &str) -> (Vec<(String, String)>, &str) {
    let Some(rest) = text.strip_prefix("---") else { return (Vec::new(), text) };
    let Some(end) = rest.find("\n---") else { return (Vec::new(), text) };
    let head = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['-', '\n', '\r']);
    let pairs = head.lines().filter_map(|l| {
        let (k, v) = l.split_once(':')?;
        Some((k.trim().to_string(), v.trim().trim_matches('"').trim_matches('\'').to_string()))
    }).collect();
    (pairs, body)
}

fn skill_dirs(project: Option<&Path>) -> Vec<PathBuf> {
    let mut d = Vec::new();
    if let Some(p) = project {
        d.push(p.join(".void/skills"));
    }
    d.push(crate::spaces::home().join("skills"));
    if let Some(p) = project {
        d.push(p.join(".claude/skills"));
    }
    d.push(home().join(".claude/skills"));
    d
}

fn read_skill(dir: &Path) -> Option<Skill> {
    let text = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    let (pairs, body) = front(&text);
    let get = |k: &str| pairs.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
    let name = get("name").unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let description = get("description").unwrap_or_else(|| body.lines().find(|l| !l.trim().is_empty() && !l.starts_with('#')).unwrap_or("").trim().to_string());
    (!name.is_empty()).then(|| Skill { name, description, dir: dir.to_path_buf() })
}

/// Every skill on the computer, a name once (the first place it's found wins), sorted by name.
pub fn skills(project: Option<&Path>) -> Vec<Skill> {
    let mut out: Vec<Skill> = Vec::new();
    for root in skill_dirs(project) {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for d in dirs {
            // a skill's own folder, or a folder of skills (Claude Code keeps synced ones a level down)
            let found: Vec<Skill> = match read_skill(&d) {
                Some(s) => vec![s],
                None => std::fs::read_dir(&d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).filter_map(|p| read_skill(&p)).collect(),
            };
            for s in found {
                if !out.iter().any(|o| o.name == s.name) {
                    out.push(s);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A skill by name (case-insensitive).
pub fn skill(project: Option<&Path>, name: &str) -> Option<Skill> {
    let n = name.trim().trim_start_matches('/').to_lowercase();
    skills(project).into_iter().find(|s| s.name.to_lowercase() == n)
}

/// A skill's SKILL.md whole, and the other files it brings (for `raea skill NAME FILE`).
pub fn skill_text(s: &Skill) -> (String, Vec<String>) {
    let text = std::fs::read_to_string(s.dir.join("SKILL.md")).unwrap_or_default();
    let mut files = Vec::new();
    let mut stack = vec![s.dir.clone()];
    while let Some(d) = stack.pop() {
        for p in std::fs::read_dir(&d).into_iter().flatten().flatten().map(|e| e.path()) {
            if p.is_dir() {
                if files.len() < 200 { stack.push(p); }
            } else if p.file_name().is_some_and(|n| n != "SKILL.md") {
                files.push(p.strip_prefix(&s.dir).unwrap_or(&p).to_string_lossy().into_owned());
            }
        }
    }
    files.sort();
    (text, files)
}

/// A file a skill brings, by its path inside the skill's folder (never outside it).
pub fn skill_file(s: &Skill, rel: &str) -> Option<String> {
    let p = s.dir.join(rel.trim_start_matches('/'));
    let real = std::fs::canonicalize(&p).ok()?;
    let base = std::fs::canonicalize(&s.dir).ok()?;
    real.starts_with(&base).then(|| std::fs::read_to_string(real).ok()).flatten()
}

fn command_dirs(project: Option<&Path>) -> Vec<PathBuf> {
    let mut d = Vec::new();
    if let Some(p) = project {
        d.push(p.join(".void/commands"));
    }
    d.push(crate::spaces::home().join("commands"));
    d.push(home().join(".claude/commands"));
    d
}

/// Every command, a name once, sorted.
pub fn commands(project: Option<&Path>) -> Vec<Command> {
    let mut out: Vec<Command> = Vec::new();
    for root in command_dirs(project) {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for p in rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "md")) {
            let Some(name) = p.file_stem().map(|n| n.to_string_lossy().into_owned()) else { continue };
            if out.iter().any(|o| o.name == name) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let (pairs, body) = front(&text);
            let description = pairs.iter().find(|(k, _)| k == "description").map(|(_, v)| v.clone())
                .unwrap_or_else(|| body.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim_start_matches('#').trim().to_string());
            out.push(Command { name, description, body: body.to_string() });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// What a typed `/name args` becomes for the agent: a command's prompt with its arguments put in, or a skill's
/// instructions with what was asked. None when the name is neither.
pub fn expand(project: Option<&Path>, line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix('/')?;
    let (name, args) = rest.split_once(char::is_whitespace).map(|(n, a)| (n, a.trim())).unwrap_or((rest, ""));
    let n = name.to_lowercase();
    if let Some(c) = commands(project).into_iter().find(|c| c.name.to_lowercase() == n) {
        let body = if c.body.contains("$ARGUMENTS") { c.body.replace("$ARGUMENTS", args) } else if args.is_empty() { c.body.clone() } else { format!("{}\n\n{args}", c.body.trim_end()) };
        return Some(body);
    }
    let s = skill(project, &n)?;
    let (text, files) = skill_text(&s);
    let (_, body) = front(&text);
    let more = if files.is_empty() { String::new() } else { format!("\n(Files it brings, read with `raea skill {} FILE`: {})", s.name, files.iter().take(30).cloned().collect::<Vec<_>>().join(", ")) };
    Some(format!("(the person ran the skill {}; its instructions follow, then what they asked)\n\n{}{more}\n\n{}", s.name, body.trim(), if args.is_empty() { "Go ahead with it." } else { args }))
}

/// The skills, for the agent's first message: a line each, the description cut short.
pub fn context(project: Option<&Path>) -> String {
    let all = skills(project);
    if all.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = all.iter().map(|s| {
        let d: String = s.description.chars().take(140).collect();
        format!("- {}: {}{}", s.name, d, if s.description.chars().count() > 140 { "…" } else { "" })
    }).collect();
    format!("# Skills on this computer (`raea skill NAME` reads one whole; use one when the request fits it)\n{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_skills_and_commands_and_expands_them() {
        let d = std::env::temp_dir().join(format!("void-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(".void/skills/tidy")).unwrap();
        std::fs::write(d.join(".void/skills/tidy/SKILL.md"), "---\nname: tidy\ndescription: \"Make a piece calmer\"\n---\n# Tidy\nRemove one thing.\n").unwrap();
        std::fs::write(d.join(".void/skills/tidy/notes.md"), "more").unwrap();
        std::fs::create_dir_all(d.join(".void/skills/group/inner")).unwrap();
        std::fs::write(d.join(".void/skills/group/inner/SKILL.md"), "---\nname: inner\ndescription: a nested one\n---\nbody").unwrap();
        std::fs::create_dir_all(d.join(".void/commands")).unwrap();
        std::fs::write(d.join(".void/commands/shorter.md"), "---\ndescription: Cut it shorter\n---\nCut the piece to $ARGUMENTS seconds, keeping the hook.").unwrap();
        let p = Some(d.as_path());
        let names: Vec<String> = super::skills(p).into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"tidy".to_string()) && names.contains(&"inner".to_string()), "{names:?}");
        assert_eq!(super::expand(p, "/shorter 15").unwrap(), "Cut the piece to 15 seconds, keeping the hook.");
        let s = super::expand(p, "/tidy the hook").unwrap();
        assert!(s.contains("Remove one thing.") && s.contains("the hook") && s.contains("notes.md"), "{s}");
        assert!(super::expand(p, "/nothing").is_none());
        assert!(super::skill_file(&super::skill(p, "tidy").unwrap(), "../../../etc/passwd").is_none());
        assert!(super::context(p).contains("- tidy: Make a piece calmer"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
