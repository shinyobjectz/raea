//! A project's compositions and components, registered by reading its files (docs/projects.feature PJ8),
//! so the raea app, `raea project scan`, `raea doctor` and `raea next` agree on what each file is:
//! - a composition is each <Composition> a registerRoot registers (its props read from the source, not rendered), or a
//!   .tsx's default export with none that no other file imports, registered for it under its file's name (versions
//!   NAME-vN.tsx are one composition), sized from its newest render's manifest when it has one;
//! - a component is a .tsx a composition imports, directly or through another.
//! What isn't connected is linted (lint.rs), each with its file and line and the fix. raea-app-workspace runs `scan`
//! for `raea project scan`, `raea doctor`, `raea next` and `raea context`.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

mod lint;
mod read;

pub use lint::{Fix, Level, Lint, apply};
pub use read::{Facts, Prop};

/// The nearest folder at or above a path that holds a marker (.raea/workspace, .raea/project), as the record finds it.
pub use raea_core_facts::record::up;

/// The workspace's shared folder of one kind (docs/layout.feature: shared/compositions, components, assets, artifacts),
/// which every project can call on as @shared/<kind>/. A workspace from before shared/ keeps that kind at its root
/// (artifacts/) until `raea workspace tidy` moves it, and it's read there meanwhile.
pub fn shared_dir(ws: &Path, kind: &str) -> PathBuf {
    let old = ws.join(kind);
    if !ws.join("shared").is_dir() && old.is_dir() {
        return old;
    }
    ws.join("shared").join(kind)
}

/// A name as a folder: lowercase, dashes.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests;

/// Folders that hold no compositions: media, renders, packages, raea's own (host/compositions.ts skips the same).
const SKIP: &[&str] = &["node_modules", ".git", ".raea", "media", "out", "renders", "raw-audio", "stills", "dist", "build"];
const MAX_FILES: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum How {
    /// a <Composition> in a registerRoot
    Registered,
    /// a default export with no <Composition>, registered under its file's name
    Default,
}

#[derive(Debug, Clone)]
pub struct Composition {
    pub id: String,
    /// relative to the scanned folder, with `/`
    pub file: String,
    pub line: usize,
    pub how: How,
    pub component: Option<String>,
    pub width: Option<Prop>,
    pub height: Option<Prop>,
    pub fps: Option<Prop>,
    pub frames: Option<Prop>,
}

#[derive(Debug, Clone)]
pub struct Component {
    pub file: String,
    /// the composition files that import it, directly or through another file
    pub used_by: Vec<String>,
}

#[derive(Debug)]
pub struct Registry {
    pub dir: PathBuf,
    /// the folder is in a raea project (.raea/project at or above it)
    pub project: bool,
    pub compositions: Vec<Composition>,
    pub components: Vec<Component>,
    pub lint: Vec<Lint>,
}

/// One file read: its path and what it declares.
pub(crate) struct File {
    pub path: PathBuf,
    pub rel: String,
    pub facts: Facts,
}

/// Every .tsx and .ts under the folder, a few levels down, skipping media, packages, raea's own folders and generated
/// files (NAME.gen.tsx: written by a tool, not a person).
fn sources(dir: &Path, rel: &str, depth: usize, out: &mut Vec<(PathBuf, String)>) {
    if depth > 4 || out.len() >= MAX_FILES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let p = e.path();
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if p.is_dir() {
            if !name.starts_with('.') && !SKIP.contains(&name.as_str()) {
                sources(&p, &r, depth + 1, out);
            }
        } else if (name.ends_with(".tsx") || name.ends_with(".ts")) && !name.ends_with(".d.ts") && !name.ends_with(".gen.tsx") && !name.ends_with(".gen.ts") {
            out.push((p, r));
        }
    }
}

/// NAME for NAME.tsx and NAME-vN.tsx: one composition's versions share it.
pub fn stem(file: &str) -> String {
    let base = file.rsplit('/').next().unwrap_or(file).trim_end_matches(".tsx").trim_end_matches(".ts");
    match base.rsplit_once("-v") {
        Some((name, n)) if !name.is_empty() && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => name.to_string(),
        _ => base.to_string(),
    }
}

/// The file a relative or absolute import names: as written, or with .tsx, .ts, .jsx or .js (in place of an extension
/// or after the name), or its folder's index.
pub(crate) fn resolve(from: &Path, spec: &str) -> Option<PathBuf> {
    // @shared/<kind>/<name>: the workspace's shared/ (docs/layout.feature LY2), as the engine's loader resolves it
    let base = if let Some(rest) = spec.strip_prefix(SHARED) {
        up(from.parent()?, ".raea/workspace")?.join("shared").join(rest)
    } else if spec.starts_with('/') {
        PathBuf::from(spec)
    } else {
        from.parent()?.join(spec)
    };
    let mut tries = vec![base.clone()];
    for ext in ["tsx", "ts", "jsx", "js"] {
        tries.push(base.with_extension(ext));
        tries.push(PathBuf::from(format!("{}.{ext}", base.display())));
    }
    tries.extend([base.join("index.tsx"), base.join("index.ts")]);
    let found = tries.into_iter().find(|p| p.is_file())?;
    Some(dunce::canonicalize(&found).unwrap_or(found))
}

/// An import of a file (not a package): relative, absolute, or the workspace's shared/ (@shared/...).
pub(crate) fn is_path(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../") || spec.starts_with('/') || spec.starts_with(SHARED)
}

/// The prefix a project imports its workspace's shared files by: @shared/components/desk (docs/layout.feature LY2).
pub const SHARED: &str = "@shared/";

/// A version's newest render manifest: NAME[-vN][.play].render.json beside it, or the folder's render.json for edit.tsx.
fn manifest(path: &Path) -> Option<Value> {
    let dir = path.parent()?;
    let base = path.file_name()?.to_string_lossy().trim_end_matches(".tsx").to_string();
    let mut tries = vec![dir.join(format!("{base}.render.json")), dir.join(format!("{base}.play.render.json"))];
    if base == "edit" {
        tries.push(dir.join("render.json"));
    }
    let f = tries.into_iter().find(|p| p.is_file())?;
    serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()
}

/// The folder's registry.
pub fn scan(dir: &Path) -> Registry {
    let dir = dunce::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let project = up(&dir, ".raea/project").is_some();
    let mut found = Vec::new();
    sources(&dir, "", 0, &mut found);
    let files: Vec<File> = found
        .into_iter()
        .map(|(path, rel)| {
            let src = std::fs::read_to_string(&path).unwrap_or_default();
            let facts = read::read(&src, rel.ends_with(".tsx"));
            File { path: dunce::canonicalize(&path).unwrap_or(path), rel, facts }
        })
        .collect();

    // a default-exported part another file imports (kit/statcard.tsx) is a component, not a piece of its own
    let imported: BTreeSet<PathBuf> = files.iter().flat_map(|f| f.facts.imports.iter().filter(|(s, _)| is_path(s)).filter_map(|(s, _)| resolve(&f.path, s))).collect();
    let mut compositions = Vec::new();
    for f in &files {
        if !f.facts.registered.is_empty() {
            for r in &f.facts.registered {
                let id = match &r.id {
                    Some(Prop::Text(t)) => t.clone(),
                    Some(Prop::Num(n)) => n.to_string(),
                    _ => stem(&f.rel),
                };
                compositions.push(Composition { id, file: f.rel.clone(), line: r.line, how: How::Registered, component: r.component.clone(), width: r.width.clone(), height: r.height.clone(), fps: r.fps.clone(), frames: r.frames.clone() });
            }
        } else if f.rel.ends_with(".tsx") && f.facts.default.is_some() && !imported.contains(&f.path) {
            let m = manifest(&f.path);
            let num = |k: &str| m.as_ref().and_then(|m| m[k].as_f64()).map(Prop::Num);
            let frames = m.as_ref().and_then(|m| m["frames"].as_array()).map(|a| Prop::Num(a.len() as f64));
            compositions.push(Composition { id: stem(&f.rel), file: f.rel.clone(), line: 1, how: How::Default, component: f.facts.default.clone().flatten(), width: num("width"), height: num("height"), fps: num("fps"), frames });
        }
    }

    // the files each composition reaches through its imports, inside the scanned folder
    let by_path: BTreeMap<&Path, &File> = files.iter().map(|f| (f.path.as_path(), f)).collect();
    let comp_files: BTreeSet<&str> = compositions.iter().map(|c| c.file.as_str()).collect();
    let mut used_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for f in files.iter().filter(|f| comp_files.contains(f.rel.as_str())) {
        let mut seen = BTreeSet::new();
        let mut stack = vec![f];
        while let Some(at) = stack.pop() {
            for (spec, _) in at.facts.imports.iter().filter(|(s, _)| is_path(s)) {
                let Some(to) = resolve(&at.path, spec) else { continue };
                if let Some(g) = by_path.get(to.as_path())
                    && seen.insert(g.rel.clone())
                {
                    stack.push(g);
                }
            }
        }
        for r in seen {
            used_by.entry(r).or_default().insert(f.rel.clone());
        }
    }
    let components = files
        .iter()
        .filter(|f| f.rel.ends_with(".tsx") && !comp_files.contains(f.rel.as_str()) && used_by.contains_key(&f.rel))
        .map(|f| Component { file: f.rel.clone(), used_by: used_by[&f.rel].iter().cloned().collect() })
        .collect();

    let mut reg = Registry { dir, project, compositions, components, lint: Vec::new() };
    reg.lint = lint::lint(&reg, &files, &used_by);
    reg
}

impl Registry {
    /// What isn't connected: the warnings (the notes say what only the render knows).
    pub fn warnings(&self) -> impl Iterator<Item = &Lint> {
        self.lint.iter().filter(|l| l.level == Level::Warn)
    }

    /// The compositions' files, each once, the newest first.
    pub fn composition_files(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for c in &self.compositions {
            let p = self.dir.join(&c.file);
            if !out.contains(&p) {
                out.push(p);
            }
        }
        out.sort_by_cached_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
        out
    }

    pub fn json(&self) -> Value {
        let p = |v: &Option<Prop>| v.as_ref().map_or(Value::Null, Prop::json);
        json!({
            "dir": self.dir,
            "project": self.project,
            "compositions": self.compositions.iter().map(|c| json!({
                "id": c.id, "file": c.file, "line": c.line,
                "how": if c.how == How::Registered { "registered" } else { "default" },
                "component": c.component, "width": p(&c.width), "height": p(&c.height), "fps": p(&c.fps), "frames": p(&c.frames),
            })).collect::<Vec<_>>(),
            "components": self.components.iter().map(|c| json!({ "file": c.file, "usedBy": c.used_by })).collect::<Vec<_>>(),
            "findings": self.lint.iter().map(Lint::finding).collect::<Vec<_>>(),
        })
    }

    /// For an agent (`raea context`): each composition with its id, file, size, fps and length, the components, and
    /// what isn't connected (its warnings, at most ten; the notes are `raea project scan`'s).
    pub fn summary(&self) -> String {
        let p = |v: &Option<Prop>| match v {
            Some(Prop::Num(n)) => format!("{n}"),
            Some(Prop::Text(t)) => t.clone(),
            Some(Prop::Computed(_)) => "(computed)".into(),
            None => "?".into(),
        };
        let mut out = String::new();
        for c in &self.compositions {
            out.push_str(&format!("composition {}: {}:{}, {}x{}, {} fps, {} frames{}\n", c.id, c.file, c.line, p(&c.width), p(&c.height), p(&c.fps), p(&c.frames), if c.how == How::Default { " (its default export, not registered)" } else { "" }));
        }
        if self.compositions.is_empty() {
            out.push_str("no compositions yet: a .tsx whose registerRoot registers a <Composition id width height fps durationInFrames />\n");
        }
        if !self.components.is_empty() {
            out.push_str(&format!("components: {}\n", self.components.iter().map(|c| c.file.as_str()).collect::<Vec<_>>().join(", ")));
        }
        let warn: Vec<&Lint> = self.warnings().collect();
        if !warn.is_empty() {
            out.push_str(&format!("\nnot connected ({}; raea doctor --fix fixes what it can):\n", warn.len()));
            for l in warn.iter().take(10) {
                out.push_str(&format!("- {}\n", l.line()));
            }
        }
        out.trim_end().to_string()
    }

    /// What it found, for a person: a line for the counts and one each composition, then its findings worst first,
    /// each about its file:line with the fix, and their count last.
    pub fn text(&self) -> String {
        let mut out = format!(
            "{}: {} compositions, {} components",
            self.dir.display(),
            self.compositions.len(),
            self.components.len()
        );
        for c in &self.compositions {
            out.push_str(&format!("\n  composition {} ({}:{}{})", c.id, c.file, c.line, if c.how == How::Default { ", its default export" } else { "" }));
        }
        let found = raea_core_facts::finding::ranked(self.lint.iter().map(Lint::finding).collect());
        for f in &found {
            let at = f.about.first().map(|a| a.key.clone().unwrap_or_else(|| a.name.clone())).unwrap_or_default();
            out.push_str(&format!("\n{at}: {}", f.line()));
        }
        out.push_str(&format!("\nraea project scan: {}", raea_core_facts::finding::count(&found)));
        out
    }
}
