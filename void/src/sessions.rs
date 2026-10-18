//! Sessions: the chats in a project (the founder, 2026-10-17: "the project is the folder you chose to work in, then
//! clicking on the title of the chat would be changing sessions"). A session is one conversation with the agent: its
//! transcript (the terminal's lines) and the model's history, kept in <workspace>/.void/sessions/<id>.json. Objects are
//! the project's, not a session's: any session can open, change and render any of them (the cube on the stage).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    /// Unix seconds.
    pub created: u64,
    #[serde(default)]
    pub updated: u64,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Saved {
    #[serde(flatten)]
    pub meta: Option<Session>,
    #[serde(default)]
    pub lines: Vec<crate::app::Line>,
    #[serde(default)]
    pub history: Vec<serde_json::Value>,
}

/// Where a project's sessions are: its .void/sessions/.
pub fn dir(root: &Path) -> PathBuf {
    root.join(".void").join("sessions")
}

fn file(root: &Path, id: &str) -> PathBuf {
    dir(root).join(format!("{id}.json"))
}

/// Every session, most recently used first.
pub fn list(root: &Path) -> Vec<Session> {
    let mut out: Vec<Session> = std::fs::read_dir(dir(root)).into_iter().flatten().flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            Some(Session { id: v["id"].as_str()?.into(), name: v["name"].as_str()?.into(), created: v["created"].as_u64().unwrap_or(0), updated: v["updated"].as_u64().unwrap_or(0) })
        })
        .collect();
    out.sort_by(|a, b| b.updated.max(b.created).cmp(&a.updated.max(a.created)));
    out
}

/// A new session, saved empty.
pub fn create(root: &Path, name: &str) -> anyhow::Result<Session> {
    let now = crate::workspace::now();
    let id = format!("{now:x}{:03x}", std::process::id() as u64 % 4096 ^ (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_micros() as u64).unwrap_or(0) % 4096));
    let s = Session { id, name: name.into(), created: now, updated: now };
    save(root, &s, &[], &[])?;
    Ok(s)
}

pub fn load(root: &Path, id: &str) -> Option<(Session, Vec<crate::app::Line>, Vec<serde_json::Value>)> {
    let saved: Saved = serde_json::from_str(&std::fs::read_to_string(file(root, id)).ok()?).ok()?;
    Some((saved.meta?, saved.lines, saved.history))
}

pub fn save(root: &Path, s: &Session, lines: &[crate::app::Line], history: &[serde_json::Value]) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir(root))?;
    let mut meta = s.clone();
    meta.updated = crate::workspace::now();
    let body = serde_json::to_string(&serde_json::json!({"id": meta.id, "name": meta.name, "created": meta.created, "updated": meta.updated, "lines": lines, "history": history}))?;
    // written whole, then moved over the old one, so a crash mid-write leaves the last good copy
    let tmp = file(root, &format!("{}.part", s.id));
    std::fs::write(&tmp, body)?;
    std::fs::rename(tmp, file(root, &s.id))?;
    Ok(())
}

/// Only the transcript changed (a turn settling): the history kept as it was saved.
pub fn save_lines(root: &Path, s: &Session, lines: &[crate::app::Line]) -> anyhow::Result<()> {
    let history = load(root, &s.id).map(|(_, _, h)| h).unwrap_or_default();
    save(root, s, lines, &history)
}

pub fn rename(root: &Path, s: &mut Session, name: &str) -> anyhow::Result<()> {
    let (_, lines, history) = load(root, &s.id).unwrap_or((s.clone(), Vec::new(), Vec::new()));
    s.name = name.into();
    save(root, s, &lines, &history)
}

#[cfg(test)]
mod tests {
    #[test]
    fn kept_and_listed_newest_first() {
        let d = std::env::temp_dir().join(format!("void-sessions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let a = super::create(&d, "Pong").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let mut b = super::create(&d, "Untitled").unwrap();
        super::save(&d, &b, &[], &[serde_json::json!({"role": "user", "content": "hi"})]).unwrap();
        super::rename(&d, &mut b, "Ferro ad").unwrap();
        let l = super::list(&d);
        assert_eq!(l.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["Ferro ad", "Pong"]);
        assert_eq!(super::load(&d, &b.id).unwrap().2.len(), 1, "the history survives a rename");
        assert!(super::load(&d, &a.id).is_some());
        let _ = std::fs::remove_dir_all(&d);
    }
}
