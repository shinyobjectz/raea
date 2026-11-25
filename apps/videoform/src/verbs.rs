//! `raea videoform new` and `ls`, and what `raea context` prints of a project's videoform (docs/videoform.feature
//! VF2). A videoform is a fingerprint with a `videoform` section: `new` makes the fingerprint as `raea fingerprint new`
//! does (its axes, words and heads), extending the person's own by default, then writes the section into its
//! fingerprint.ts. It judges the pieces its `applies` names (the projects tagged with its name, unless --kinds or
//! --tags say otherwise) or where it's pinned, never everything by default.

use crate::{Eval, flag, plain, references};
use raea_core_brief::Videoform;
use raea_sys_fingerprint::folder::{Folder, root};
use raea_sys_fingerprint::taxonomy::Fingerprint;
use std::path::Path;

/// A videoform's folder and its fingerprint, read through fingerprint.ts when `eval` is given
pub(crate) fn open(name: &str, eval: Option<Eval>) -> Result<(Folder, Fingerprint), String> {
    let dir = Folder::named(name);
    let fp = dir.read(eval).map_err(|e| format!("{e:#}"))?;
    if fp.videoform.is_none() {
        return Err(format!("{} is a fingerprint, not a videoform (raea videoform new NAME --context \"…\" makes one)", dir.dir.display()));
    }
    Ok((dir, fp))
}

/// The person's own fingerprint, which a new videoform extends unless --extends names another
fn own() -> Option<String> {
    let at = |n: &str| Folder::named(n).read(None).ok().filter(|f| f.videoform.is_none()).map(|f| f.name);
    let pinned = std::fs::read_to_string(root().join("default")).ok().map(|s| s.trim().to_string());
    pinned.as_deref().and_then(at).or_else(|| at("me"))
}

/// `raea videoform new NAME --context "…" [--extends FP] [--kinds K] [--tags T]`: the fingerprint made, with its
/// section. The words a fingerprint new is given pass through; its `about` is the context.
pub(crate) fn new(name: &str, args: &[String]) -> Result<String, String> {
    let dir = Folder::named(name);
    if dir.exists() {
        return Err(match open(name, None) {
            Ok(_) => format!("raea videoform: {name} is made already (raea videoform add {name} SOURCE... reads more)"),
            Err(e) => e,
        });
    }
    let context = flag(args, "--context").map(str::trim).unwrap_or("").to_string();
    let mut make = vec!["fingerprint".to_string(), "new".into(), name.into()];
    if !context.is_empty() {
        make.extend(["--about".into(), context.clone()]);
    }
    if let Some(p) = flag(args, "--extends").map(str::to_string).or_else(own) {
        make.extend(["--extends".into(), p]);
    }
    let (kinds, tags) = (flag(args, "--kinds"), flag(args, "--tags"));
    if let Some(k) = kinds {
        make.extend(["--kinds".into(), k.into()]);
    }
    // a videoform judges the projects tagged with its name, unless it's told which
    let short = dir.dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| name.into());
    make.extend(["--tags".into(), tags.map(str::to_string).unwrap_or_else(|| if kinds.is_none() { short.clone() } else { String::new() })]);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = std::process::Command::new(exe).args(&make).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("raea videoform: making the fingerprint: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    // fingerprint new makes the account's first fingerprint its default; a videoform judges only what it applies to
    let default = root().join("default");
    if std::fs::read_to_string(&default).is_ok_and(|d| d.trim() == short) {
        let _ = std::fs::remove_file(&default);
    }
    let mut fp = dir.read(None).map_err(|e| format!("{e:#}"))?;
    let extends = fp.extends.clone();
    fp.videoform = Some(Videoform { context, ..Default::default() });
    dir.rewrite(&fp).map_err(|e| format!("{e:#}"))?;
    let applies = fp.applies.as_ref().map(|a| [a.kinds.join(", "), a.tags.join(", ")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("; ")).unwrap_or_default();
    Ok(format!(
        "raea videoform: made {name} in {}{}; it judges pieces that are {applies} (a project's tags in .raea/project, or `raea fingerprint use {name}` there)",
        dir.dir.display(),
        extends.map(|e| format!(", extending {e}")).unwrap_or_default()
    ))
}

/// The videoforms a level holds, by name
fn all() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root()).into_iter().flatten().flatten().filter_map(|e| Folder { dir: e.path() }.read(None).ok()).filter(|f| f.videoform.is_some()).map(|f| f.name).collect();
    names.sort();
    names
}

/// `raea videoform ls [NAME] [--json]`
pub(crate) fn ls(args: &[String]) -> (String, i32) {
    let Some(name) = plain(args).into_iter().next() else {
        let names = all();
        if names.is_empty() {
            return ("raea videoform: none yet (raea videoform new NAME --context \"…\" SOURCE...)".into(), 0);
        }
        return (names.iter().map(|n| format!("{n}: {} references", references(&Folder::named(n).dir).len())).collect::<Vec<_>>().join("\n"), 0);
    };
    let (dir, fp) = match open(&name, None) {
        Ok(x) => x,
        Err(e) => return (e, 1),
    };
    let refs = references(&dir.dir);
    if args.iter().any(|a| a == "--json") {
        return (serde_json::to_string_pretty(&serde_json::json!({"videoform": fp.videoform, "read": refs})).unwrap(), 0);
    }
    let mut out = vec![format!("{name}: {} references ({})", refs.len(), dir.dir.display())];
    if let Some(v) = fp.videoform.as_ref().filter(|v| !v.ranked().is_empty()) {
        out.push(format!("  context: {}", v.ranked().iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join("; ")));
    }
    out.extend(refs.iter().map(crate::ingest::line));
    (out.join("\n"), 0)
}

/// The videoform that judges a project, for `raea context`: its name and folder, and what it holds so far
pub fn context(project: &Path) -> Option<(String, String)> {
    let chosen = raea_sys_fingerprint_use::which::which(project)?;
    let fp = Folder { dir: chosen.dir.clone() }.read(None).ok()?;
    let v = fp.videoform?;
    let mut body = vec![format!("{} judges this project ({}).", fp.name, chosen.why), format!("Context: {}", if v.ranked().is_empty() { "(none yet)".to_string() } else { v.ranked().iter().map(|c| format!("{} ({})", c.text, c.group.as_deref().unwrap_or(&c.by))).collect::<Vec<_>>().join("; ") })];
    let refs = references(&chosen.dir);
    body.push(format!("References read: {}", refs.len()));
    body.extend(refs.iter().map(crate::ingest::line));
    // the section names them once `vocab` starts the vocabulary; ones written by hand are there by their usual names
    let usual = [crate::vocab::FILE.to_string(), crate::vocab::template_file(&fp.name), crate::vocab::rules_file(&fp.name)];
    for ((what, file), usual) in [("Vocabulary", &v.vocabulary), ("Template", &v.template), ("Rules", &v.rules)].into_iter().zip(usual) {
        let f = file.clone().unwrap_or(usual);
        if chosen.dir.join(&f).is_file() {
            body.push(format!("{what}: {f}"));
        }
    }
    Some((format!("VIDEOFORM {}", chosen.dir.display()), body.join("\n")))
}
