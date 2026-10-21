//! The shell's tar and html-to-markdown, natively (just-bash's browser build has no tar, and its html-to-markdown needs
//! a DOM). tar works on real files under the project's root only: an archive member with an absolute path or a ".."
//! is refused, and links are extracted only when they stay inside.
//!
//!   tar.run {root, cwd, create?, extract?, list?, gzip?, file, members?, dir?, verbose?} -> {stdout}
//!   html.markdown {html} -> string

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};

use crate::{Call, Handler, R, s};

pub(crate) fn calls() -> Vec<Call> {
    let mk = |name: &str, args: &str, answer: &str, doc: &str, f: Handler| Call { name: name.into(), args: args.into(), answer: answer.into(), doc: doc.into(), f };
    vec![
        mk("html.markdown", "{ html: string }", "string", "HTML as Markdown (htmd: headings, lists, links, emphasis, code; scripts and styles dropped)", Arc::new(|v| {
            let conv = htmd::HtmlToMarkdown::builder().skip_tags(vec!["script", "style", "noscript", "template", "head"]).build();
            conv.convert(s(v, "html")?).map(Value::String).map_err(|e| e.to_string())
        })),
        mk("tar.run", "{ root: string; cwd: string; create?: boolean; extract?: boolean; list?: boolean; gzip?: boolean; file: string; members?: string[]; dir?: string; verbose?: boolean }", "{ stdout: string }", "tar on project files: create, extract (paths kept inside) or list, gzipped or not", Arc::new(run)),
    ]
}

/// A path inside `root` from `base` and `rel`, lexically (no ".." above the root).
fn inside(root: &Path, base: &Path, rel: &Path) -> Option<PathBuf> {
    let mut p = if rel.is_absolute() { root.to_path_buf() } else { base.to_path_buf() };
    for c in rel.components() {
        match c {
            Component::Normal(x) => p.push(x),
            Component::ParentDir => {
                if !p.pop() || !p.starts_with(root) {
                    return None;
                }
            }
            _ => {}
        }
    }
    p.starts_with(root).then_some(p)
}

fn run(v: &Value) -> R<Value> {
    let root = PathBuf::from(s(v, "root")?).canonicalize().map_err(|e| e.to_string())?;
    let cwd = PathBuf::from(s(v, "cwd")?);
    let file = inside(&root, &cwd, Path::new(s(v, "file")?)).ok_or("tar: the archive is outside the project")?;
    let gz = v.get("gzip").and_then(Value::as_bool).unwrap_or(false) || s(v, "file").map(|f| f.ends_with(".tgz") || f.ends_with(".gz")).unwrap_or(false);
    let verbose = v.get("verbose").and_then(Value::as_bool).unwrap_or(false);
    let dir = match v.get("dir").and_then(Value::as_str) {
        Some(d) => inside(&root, &cwd, Path::new(d)).ok_or("tar: -C is outside the project")?,
        None => cwd.clone(),
    };
    let mut out = String::new();
    if v.get("create").and_then(Value::as_bool).unwrap_or(false) {
        let f = std::fs::File::create(&file).map_err(|e| format!("tar: {}: {e}", file.display()))?;
        let w: Box<dyn std::io::Write> = if gz { Box::new(flate2::write::GzEncoder::new(f, flate2::Compression::default())) } else { Box::new(f) };
        let mut b = tar::Builder::new(w);
        b.follow_symlinks(false);
        for m in v.get("members").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            let p = inside(&root, &dir, Path::new(m)).ok_or_else(|| format!("tar: {m}: outside the project"))?;
            if p.starts_with(root.join(".raea")) {
                continue;
            }
            let name = Path::new(m.trim_start_matches('/'));
            if p.is_dir() {
                b.append_dir_all(name, &p).map_err(|e| format!("tar: {m}: {e}"))?;
            } else {
                b.append_path_with_name(&p, name).map_err(|e| format!("tar: {m}: {e}"))?;
            }
            if verbose {
                out += &format!("{m}\n");
            }
        }
        b.into_inner().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())?;
        return Ok(json!({"stdout": out}));
    }
    let f = std::fs::File::open(&file).map_err(|e| format!("tar: {}: {e}", s(v, "file").unwrap_or("")))?;
    let r: Box<dyn std::io::Read> = if gz { Box::new(flate2::read::GzDecoder::new(f)) } else { Box::new(f) };
    let mut a = tar::Archive::new(r);
    let extract = v.get("extract").and_then(Value::as_bool).unwrap_or(false);
    for e in a.entries().map_err(|e| format!("tar: {e}"))? {
        let mut e = e.map_err(|e| format!("tar: {e}"))?;
        let name = e.path().map_err(|e| e.to_string())?.into_owned();
        // a folder listed as GNU tar lists it, with a trailing slash
        let is_dir = e.header().entry_type().is_dir();
        let shown = { let d = name.display().to_string(); if is_dir && !d.ends_with('/') { d + "/" } else { d } };
        if extract {
            if name.is_absolute() || name.components().any(|c| matches!(c, Component::ParentDir)) {
                return Err(format!("tar: {shown}: refusing a member that leaves the folder"));
            }
            let dest = inside(&root, &dir, &name).ok_or_else(|| format!("tar: {shown}: outside the project"))?;
            if let Ok(Some(link)) = e.link_name() {
                let target = dest.parent().and_then(|p| inside(&root, p, &link));
                if target.is_none() {
                    return Err(format!("tar: {shown}: a link that leaves the project"));
                }
            }
            if let Some(p) = dest.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            e.unpack(&dest).map_err(|er| format!("tar: {shown}: {er}"))?;
            if verbose {
                out += &format!("{shown}\n");
            }
        } else {
            out += &if verbose { format!("{} {:>8} {shown}\n", if is_dir { "d" } else { "-" }, e.header().size().unwrap_or(0)) } else { format!("{shown}\n") };
        }
    }
    Ok(json!({"stdout": out}))
}
