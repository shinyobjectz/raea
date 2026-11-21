//! Which fingerprint judges a piece (FP14c), found the way .editorconfig is: PROJECT/.raea/fingerprints, then
//! WORKSPACE/.raea/fingerprints, then the person's own (folder::root). At each level the one whose `applies` matches
//! the piece's kind and tags most closely wins, else that level's `default` file, else the next level up; the
//! account's default always matches. A project or workspace pins one by writing its level's `default`.
//!
//! A piece's kind is its project's `kind` in .raea/project (film, ad, game, playable), else what the file is (a still
//! or a sound); its tags are the project's `tags`.

use raea_sys_fingerprint::folder::{Folder, root};
use raea_sys_fingerprint::taxonomy::{Fingerprint, words_of};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub struct Choice {
    pub name: String,
    pub dir: PathBuf,
    pub about: Option<String>,
    /// project, workspace or account
    pub level: String,
    /// the rule that chose it, in words
    pub why: String,
}

/// The piece's kind and tags
#[derive(Clone, Debug, Default, Serialize)]
pub struct Piece {
    pub kind: Option<String>,
    pub tags: Vec<String>,
    pub project: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
}

/// A key's value in a git-config style file (.raea/project, .raea/workspace)
fn key(text: &str, k: &str) -> Option<String> {
    text.lines().map(str::trim).find_map(|l| l.strip_prefix(k).map(str::trim_start).and_then(|r| r.strip_prefix('=')).map(|v| v.trim().to_string())).filter(|v| !v.is_empty())
}

fn up(from: &Path, marker: &str) -> Option<PathBuf> {
    from.ancestors().find(|d| d.join(".raea").join(marker).is_file()).map(Path::to_path_buf)
}

/// A folder and the fingerprint read from it, when it reads
fn opened(f: Folder) -> Option<(Folder, Fingerprint)> {
    let fp = f.read(None).ok()?;
    Some((f, fp))
}

pub fn piece(path: &Path) -> Piece {
    let start = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
    let project = up(start, "project");
    let workspace = up(start, "workspace");
    let text = project.as_ref().map(|p| p.join(".raea/project")).and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let by_file = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" => Some("still"),
        "wav" | "mp3" | "flac" | "m4a" | "ogg" => Some("sound"),
        _ => None,
    };
    let kind = key(&text, "kind").or_else(|| by_file.map(str::to_string));
    let tags = key(&text, "tags").map(|t| words_of(&t)).unwrap_or_default();
    Piece { kind, tags, project, workspace }
}

/// The fingerprints kept at a level: each folder under it with a fingerprint.ts
pub fn at_level(dir: &Path) -> Vec<(Folder, Fingerprint)> {
    let mut out: Vec<(Folder, Fingerprint)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| Folder { dir: e.path() })
        .filter(Folder::exists)
        .filter_map(opened)
        .collect();
    out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    out
}

/// How closely a fingerprint's `applies` matches the piece: 2 for its kind, 1 a tag; 0 is no match
fn closeness(fp: &Fingerprint, p: &Piece) -> usize {
    let Some(a) = &fp.applies else { return 0 };
    let kind = p.kind.as_ref().is_some_and(|k| a.kinds.contains(k));
    if !a.kinds.is_empty() && !kind {
        return 0;
    }
    2 * usize::from(kind) + a.tags.iter().filter(|t| p.tags.contains(t)).count()
}

/// The levels to look in, nearest first
pub fn levels(p: &Piece) -> Vec<(&'static str, PathBuf)> {
    let mut v = Vec::new();
    if let Some(d) = &p.project {
        v.push(("project", d.join(".raea/fingerprints")));
    }
    if let Some(d) = &p.workspace {
        v.push(("workspace", d.join(".raea/fingerprints")));
    }
    v.push(("account", root()));
    v
}

/// A level's `default`, found at that level or above
fn named(name: &str, levels: &[(&'static str, PathBuf)], from: usize) -> Option<(Folder, Fingerprint)> {
    levels[from..].iter().map(|(_, d)| Folder { dir: d.join(name) }).find(Folder::exists).and_then(opened)
}

fn choice((f, fp): (Folder, Fingerprint), level: &str, why: String) -> Choice {
    Choice { name: fp.name.clone(), dir: f.dir, about: fp.about, level: level.into(), why }
}

/// The fingerprint that judges the piece at `path`, and why; None only when the person has none at all
pub fn which(path: &Path) -> Option<Choice> {
    let p = piece(path);
    let levels = levels(&p);
    for (i, (level, dir)) in levels.iter().enumerate() {
        let here = at_level(dir);
        let best = here.iter().map(|x| (closeness(&x.1, &p), x)).filter(|(c, _)| *c > 0).max_by_key(|(c, _)| *c);
        if let Some((_, (f, fp))) = best {
            let what = p.kind.as_deref().map_or("the project's tags".to_string(), |k| format!("{k} pieces"));
            return Some(choice((Folder { dir: f.dir.clone() }, fp.clone()), level, format!("the {level}'s, and it applies to {what}")));
        }
        let pinned = std::fs::read_to_string(dir.join("default")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        if let Some(found) = pinned.as_deref().and_then(|n| named(n, &levels, i)) {
            return Some(choice(found, level, format!("the {level}'s default")));
        }
    }
    // the account always has one: its default, else "me", else the first it has that isn't a videoform (one judges
    // only the pieces it applies to, or where it's pinned)
    let here: Vec<_> = at_level(&root()).into_iter().filter(|x| x.1.videoform.is_none()).collect();
    let me = here.iter().position(|x| x.1.name == "me").unwrap_or(0);
    here.into_iter().nth(me).map(|x| choice(x, "account", "the account's only fingerprint, with no default set".into()))
}

/// The `default` file `use` writes for a level: PROJECT/.raea/fingerprints, WORKSPACE/.raea/fingerprints or the
/// account's
pub fn default_file(path: &Path, level: &str) -> Option<PathBuf> {
    let p = piece(path);
    levels(&p).into_iter().find(|(l, _)| *l == level).map(|(_, d)| d.join("default"))
}
