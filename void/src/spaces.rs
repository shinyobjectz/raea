//! Spaces: each a folder under the open workspace's spaces/ (workspace.rs has the layout) holding one piece: a React +
//! TypeScript project as `raea init` lays it out (edit.tsx, cuts.ts, components/, media/), and a space.json (an older
//! space holding only an rrl program, edit.rrl, shows an empty stage until its first turn runs `raea init`). Opening a space swaps it onto the stage; making something new is a new
//! space or a change to this one. With no workspace open (evals, `void tool`), spaces are the legacy global folder
//! ~/.raea-void/spaces; RAEA_VOID_HOME moves ~/.raea-void.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Space {
    pub name: String,
    /// Folder name under spaces/.
    pub slug: String,
    /// Unix seconds.
    pub created: u64,
    #[serde(default)]
    pub opened: u64,
    /// One of the hues the box menu tints a space with (0-359).
    #[serde(default)]
    pub hue: u32,
    /// What it is: video, game, deck, document, music, motion, scene3d or still (`raea object kind`, @raea/data/project.ts); None until
    /// it's said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Its folder, when known (listed or made in a folder, or an eval scenario's own); else dir()/slug.
    #[serde(skip)]
    pub root: Option<PathBuf>,
    /// The workspace's root itself (no longer listed: the project's objects are its folders).
    #[serde(skip)]
    pub all: bool,
}

pub fn home() -> PathBuf {
    if let Some(p) = std::env::var_os("RAEA_VOID_HOME") {
        return PathBuf::from(p);
    }
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".raea-void")
}

/// The folder spaces are listed from and made in: the open workspace's, else the legacy global folder.
pub fn dir() -> PathBuf {
    crate::workspace::current().map(|w| w.spaces_dir()).unwrap_or_else(|| home().join("spaces"))
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "space".into() } else { s }
}

impl Space {
    pub fn folder(&self) -> PathBuf {
        self.root.clone().unwrap_or_else(|| dir().join(&self.slug))
    }

    /// A space in a folder of its own (an eval scenario), not listed with the person's.
    pub fn at(folder: &Path, name: &str) -> Space {
        Space { name: name.into(), slug: "eval".into(), created: 0, opened: 0, hue: 0, kind: None, root: Some(folder.to_path_buf()), all: false }
    }

    pub fn is_all(&self) -> bool {
        self.all
    }

    /// The TypeScript project's edit (edit.tsx), what a space holds now.
    pub fn tsx(&self) -> PathBuf {
        self.folder().join("edit.tsx")
    }

    /// A React + TypeScript project: the stage shows its render (stage_ts.rs).
    pub fn is_ts(&self) -> bool {
        !self.all && self.tsx().exists()
    }

    /// Something to show: a TypeScript project.
    pub fn has_program(&self) -> bool {
        self.tsx().exists()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        // the root has no space.json: it would list itself as a space of its parent
        if self.all {
            return Ok(());
        }
        std::fs::create_dir_all(self.folder())?;
        // a kind said on disk (`raea object kind`, from the agent's shell) is kept when this copy hasn't heard of it
        let mut me = self.clone();
        if me.kind.is_none() {
            me.kind = std::fs::read_to_string(self.folder().join("space.json")).ok().and_then(|t| serde_json::from_str::<Space>(&t).ok()).and_then(|s| s.kind);
        }
        std::fs::write(self.folder().join("space.json"), serde_json::to_string_pretty(&me)?)?;
        Ok(())
    }

    pub fn touch(&mut self) {
        self.opened = now();
        let _ = self.save();
    }
}

/// Every space, most recently opened first.
pub fn list() -> Vec<Space> {
    list_in(&dir())
}

/// Every space in `folder`, each knowing its folder.
pub fn list_in(folder: &Path) -> Vec<Space> {
    let mut out: Vec<Space> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let mut s: Space = serde_json::from_str(&std::fs::read_to_string(e.path().join("space.json")).ok()?).ok()?;
            s.root = Some(e.path());
            s.all = false;
            Some(s)
        })
        .collect();
    out.sort_by(|a, b| b.opened.max(b.created).cmp(&a.opened.max(a.created)));
    out
}

/// A new, empty space (a numbered slug if the name is taken).
pub fn create(name: &str) -> anyhow::Result<Space> {
    create_in(&dir(), name)
}

pub fn create_in(folder: &Path, name: &str) -> anyhow::Result<Space> {
    let base = slug(name);
    let mut s = base.clone();
    let mut n = 2;
    while folder.join(&s).exists() {
        s = format!("{base}-{n}");
        n += 1;
    }
    let hue = (base.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))) % 360;
    let root = Some(folder.join(&s));
    let space = Space { name: name.trim().to_string(), slug: s, created: now(), opened: now(), hue, kind: None, root, all: false };
    space.save()?;
    Ok(space)
}

/// The space whose name or slug best matches `query` (exact, then prefix, then contains).
pub fn find(query: &str) -> Option<Space> {
    find_in(&dir(), query)
}

pub fn find_in(folder: &Path, query: &str) -> Option<Space> {
    let q = query.trim().to_lowercase();
    let qs = slug(&q);
    let all = list_in(folder);
    all.iter()
        .find(|s| s.name.to_lowercase() == q || s.slug == qs)
        .or_else(|| all.iter().find(|s| s.name.to_lowercase().starts_with(&q) || s.slug.starts_with(&qs)))
        .or_else(|| all.iter().find(|s| s.name.to_lowercase().contains(&q)))
        .cloned()
}

/// A short name from a first request: its first few meaningful words, capitalised ("make a fun bouncing ball
/// animation" -> "Fun bouncing ball").
pub fn name_from(request: &str) -> String {
    const SKIP: &[&str] = &["make", "me", "a", "an", "the", "create", "build", "please", "can", "you", "i", "want", "to", "do", "some", "of", "with", "for", "and", "new", "let's", "lets", "let", "us"];
    let words: Vec<String> = request
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .skip_while(|w| SKIP.contains(&w.as_str()))
        .filter(|w| !matches!(w.as_str(), "animation" | "video" | "please"))
        .take(3)
        .collect();
    let name = words.join(" ");
    let mut c = name.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "Untitled".into(),
    }
}

pub fn delete(space: &Space) -> anyhow::Result<()> {
    anyhow::ensure!(!space.all && space.folder().starts_with(dir()) && space.folder() != dir() && !space.slug.is_empty(), "not a space folder");
    std::fs::remove_dir_all(space.folder())?;
    Ok(())
}

/// A new space's folder, empty: its project comes from `raea init` (brain.rs new_space, a turn's first look), or
/// from scaffold in a workspace.
pub fn blank(space: &Space) -> anyhow::Result<()> {
    std::fs::create_dir_all(space.folder())?;
    Ok(())
}

/// A new object's frame by what it is: a game, a deck, a motion piece or a 3D scene is landscape 1920x1080 (a game at
/// 60 fps), a video or a still the vertical 1080x1920 at 30 (the founder, 2026-10-17: "not everything is a video and in a
/// 9x16"). Written as cuts.json's `edit` before `raea init`, which sizes the project from it.
pub fn shape_for(kind: Option<&str>) -> (u32, u32, u32) {
    match kind {
        Some("game") => (1920, 1080, 60),
        Some("deck" | "motion" | "scene3d" | "music") => (1920, 1080, 30),
        // a page: A4 at 150 dpi
        Some("document") => (1240, 1754, 30),
        _ => (1080, 1920, 30),
    }
}

pub fn set_shape(space: &Space) -> anyhow::Result<()> {
    let (w, h, fps) = shape_for(space.kind.as_deref());
    let f = space.folder().join("cuts.json");
    if !f.exists() {
        std::fs::create_dir_all(space.folder())?;
        std::fs::write(f, serde_json::json!({"edit": {"width": w, "height": h, "fps": fps}, "cuts": []}).to_string())?;
    }
    Ok(())
}

/// A new space's project: raea init's files (workspace.rs scaffold_space); the stage shows its render (stage_ts.rs).
pub fn scaffold(space: &Space) -> anyhow::Result<()> {
    crate::workspace::scaffold_space(&space.folder())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn slugs() {
        assert_eq!(super::slug("My  Coffee Ad!"), "my-coffee-ad");
        assert_eq!(super::slug("  "), "space");
        assert_eq!(super::name_from("make me a fun bouncing ball animation"), "Fun bouncing ball");
        assert_eq!(super::name_from("turn this clip into an explainer"), "Turn this clip");
    }

    #[test]
    fn spaces_elsewhere_stay_there() {
        let d = std::env::temp_dir().join(format!("void-spaces-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let sp = super::create_in(&d, "Pong that plays").unwrap();
        assert_eq!(sp.folder(), d.join("pong-that-plays"));
        let all = super::list_in(&d);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].folder(), d.join("pong-that-plays"));
        assert!(super::find_in(&d, "pong").is_some() && super::find_in(&d, "hook").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
