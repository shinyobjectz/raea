use super::*;
use std::collections::BTreeSet;

/// A source the edit reaches outside its own folder by a relative import (content/anything/paint/shots.tsx's
/// ../../../examples/objects/wake): `file` where it is, `copy` where bundle_fresh puts it, relative to the work folder
/// (ext/raea/<its path from raea's home>, or ext/project/<its path from the project's git root>).
#[derive(Debug, Clone, PartialEq)]
pub struct Outside {
    pub file: PathBuf,
    pub copy: PathBuf,
}

fn specs() -> regex::Regex {
    regex::Regex::new(r#"(\bfrom\s+)(["'])(\.[^"']*)(["'])"#).unwrap()
}

fn absolute(p: &Path) -> PathBuf {
    normalize(&std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()))
}

/// The file a relative `spec` names from `base`, as the bundler would find it: itself, with a script or JSON extension,
/// or a folder's index
fn resolve(base: &Path, spec: &str) -> Option<PathBuf> {
    let p = normalize(&base.join(spec));
    let with = |ext: &str| PathBuf::from(format!("{}{ext}", p.display()));
    let tries = [p.clone(), with(".tsx"), with(".ts"), with(".jsx"), with(".js"), with(".json"), p.join("index.tsx"), p.join("index.ts"), p.join("index.js")];
    tries.into_iter().find(|t| t.is_file())
}

/// Where an edit may import from outside its folder, each with its name under ext/: raea's home, and the project's
/// root (the nearest folder above the edit holding .git) when that is somewhere else
fn roots(dir: &Path) -> Vec<(&'static str, PathBuf)> {
    let home = absolute(&raea_io_home::home());
    let mut out = vec![("raea", home.clone())];
    if let Some(git) = dir.ancestors().find(|d| d.join(".git").exists()) {
        if git != home {
            out.push(("project", git.to_path_buf()));
        }
    }
    out
}

/// Every source outside the edit's folder `dir` that its sources import by relative path, and what those import in
/// turn (each file once, so a cycle ends). The SDK packages' own files are left out: they come by name
/// (rewrite_into). A file outside both raea's home and the project's root is refused, naming its importer.
pub fn outside(dir: &Path) -> R<Vec<Outside>> {
    outside_in(&absolute(dir), &roots(&absolute(dir)))
}

pub fn outside_in(dir: &Path, roots: &[(&'static str, PathBuf)]) -> R<Vec<Outside>> {
    // the SDK's package sources, which come by name; anything else under sdk/ (a bench's shared helper) is copied
    let home = raea_io_home::home();
    let sdk: Vec<PathBuf> = std::iter::once("sdk/raea/src".to_string()).chain(crate::rewrite::sdks().into_iter().map(|(_, d)| d)).map(|d| absolute(&home.join(d))).collect();
    let re = specs();
    let mut seen = BTreeSet::new();
    let mut out = vec![];
    let mut queue: Vec<PathBuf> = sources(dir)?.into_iter().map(|r| dir.join(r)).collect();
    while let Some(f) = queue.pop() {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let base = f.parent().unwrap_or(dir).to_path_buf();
        for c in re.captures_iter(&text) {
            let spec = &c[3];
            let ext = Path::new(spec).extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
            if MEDIA_EXTS.contains(&ext.as_str()) {
                continue;
            }
            // one the bundler can't find either: left for it to name
            let Some(t) = resolve(&base, spec) else { continue };
            if t.starts_with(dir) || sdk.iter().any(|s| t.starts_with(s)) || t.components().any(|c| c.as_os_str() == "node_modules") || !seen.insert(t.clone()) {
                continue;
            }
            let Some((name, root)) = roots.iter().find(|(_, r)| t.starts_with(r)) else {
                return Err(format!("{}: {spec} is {}, outside raea's home and the project's folder", f.display(), t.display()));
            };
            out.push(Outside { file: t.clone(), copy: Path::new("ext").join(name).join(t.strip_prefix(root).unwrap()) });
            if matches!(t.extension().and_then(|x| x.to_str()), Some("ts" | "tsx" | "js" | "jsx" | "mjs")) {
                queue.push(t);
            }
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(out)
}

/// `src` (a file that sits in `from_dir`, copied to `at` under the work folder) with each relative import of an
/// outside source pointed at its copy, and, from an outside source, each import back into the edit's folder `dir` at
/// that file's copy under src/
pub fn point_outside(src: &str, from_dir: &Path, at: &Path, dir: &Path, outs: &[Outside]) -> String {
    let (from_dir, dir) = (absolute(from_dir), absolute(dir));
    specs()
        .replace_all(src, |c: &regex::Captures| {
            let target = resolve(&from_dir, &c[3]).and_then(|t| match outs.iter().find(|o| o.file == t) {
                Some(o) => Some(o.copy.clone()),
                None if !from_dir.starts_with(&dir) => t.strip_prefix(&dir).ok().map(|r| Path::new("src").join(r)),
                None => None,
            });
            match target {
                Some(t) => format!("{}{}{}{}", &c[1], &c[2], relative(at, &t), &c[4]),
                None => c[0].to_string(),
            }
        })
        .into_owned()
}

/// The import path from a file in `from` to `to`, both under the work folder
fn relative(from: &Path, to: &Path) -> String {
    let (a, b): (Vec<_>, Vec<_>) = (from.components().collect(), to.components().collect());
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut parts = vec![".."; a.len() - common].into_iter().map(String::from).collect::<Vec<_>>();
    parts.extend(b[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    let p = parts.join("/");
    if p.starts_with("..") { p } else { format!("./{p}") }
}
