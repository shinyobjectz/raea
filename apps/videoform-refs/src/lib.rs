//! A videoform's references over time (docs/videoform.feature VF8b, VF8c): `raea videoform remove NAME REF...` drops
//! references (their rows are no longer read, so the videoform is made again from the rest in seconds, the person's
//! corrections kept); `refresh NAME [REF...]` reads references again with this raea's encoders, the one time a
//! reference is needed after it's read (with no REF, each one read with other encoders than this raea's); `split NAME`
//! makes a videoform of each of the two groups the report found its references fall in (VF8b), copying their rows,
//! the vocabulary and the briefs' rules, never moving them. After an upgrade the videoform keeps working on its rows as
//! they were read; the report says which were read with other encoders, for `refresh`.

use raea_sys_fingerprint::folder::Folder;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Evaluates a .ts file's default export to its JSON (the CLI's spec host)
pub type Eval = fn(&Path) -> Result<String, String>;

const LIST: &str = "videoform/references.jsonl";

fn lines(dir: &Path) -> Vec<Value> {
    std::fs::read_to_string(dir.join(LIST)).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

fn write(dir: &Path, refs: &[Value]) -> Result<(), String> {
    let text: String = refs.iter().map(|r| format!("{r}\n")).collect();
    match std::fs::write(dir.join(LIST), text) {
        Ok(()) => Ok(()),
        Err(e) => Err(format!("{}: {e}", dir.join(LIST).display())),
    }
}

/// Where a reference came from: its link or its path
fn target(r: &Value) -> Option<&str> {
    r["source"]["url"].as_str().or(r["source"]["path"].as_str())
}

/// Whether REF names this reference: its title, its file's name, its link or path, or its hash's start
fn names(r: &Value, want: &str) -> bool {
    let w = want.to_lowercase();
    let file = target(r).and_then(|t| Path::new(t).file_stem()).map(|s| s.to_string_lossy().to_lowercase());
    r["source"]["title"].as_str().is_some_and(|t| t.to_lowercase() == w) || file.as_deref() == Some(w.as_str()) || target(r) == Some(want) || (want.len() >= 8 && r["hash"].as_str().is_some_and(|h| h.starts_with(want)))
}

/// A reference's rows' encoder pins, as it was read
fn pins(dir: &Path, r: &Value) -> Option<BTreeMap<String, String>> {
    let f = dir.join("videoform/rows").join(format!("{}.json", r["hash"].as_str()?));
    let v: Value = serde_json::from_slice(&std::fs::read(f).ok()?).ok()?;
    serde_json::from_value(v["pins"].clone()).ok()
}

/// This raea's encoders' pins, the picture and sound ones rows are read with
fn now() -> Option<BTreeMap<String, String>> {
    raea_sys_columns::models::locate().ok().map(|f| f.pins().into_iter().collect())
}

/// The references read with other encoders than this raea's: each, and which encoders
fn behind_of(dir: &Path) -> Vec<(Value, Vec<String>)> {
    let Some(now) = now() else { return Vec::new() };
    lines(dir).into_iter().filter_map(|r| {
        let was = pins(dir, &r)?;
        let off: Vec<String> = now.iter().filter(|(k, v)| was.get(*k).is_some_and(|w| w != *v)).map(|(k, _)| k.split('/').next().unwrap_or(k).to_string()).collect();
        (!off.is_empty()).then_some((r, off))
    }).collect()
}

/// The report's line on encoders (VF8c), when references were read with others than this raea's
pub fn behind(dir: &Path, name: &str) -> Option<String> {
    let b = behind_of(dir);
    let (_, first) = b.first()?;
    Some(format!("{} of the references were read with other encoders than this raea's ({}): the videoform works on their rows as they were read; raea videoform refresh {name} reads them again with these, the one time the references are needed", b.len(), first.join(", ")))
}

/// `raea videoform remove|refresh|split ...`: what it says, and its exit code
pub fn verb(cmd: &str, words: &[String], eval: Eval) -> (String, i32) {
    let Some(name) = words.first() else { return (format!("usage: raea videoform {cmd} NAME{}", if cmd == "split" { "" } else { " [REF...]" }), 2) };
    let dir = Folder::named(name);
    let fp = match dir.read(Some(eval)) {
        Ok(f) if f.videoform.is_some() => f,
        Ok(_) => return (format!("{} is a fingerprint, not a videoform", dir.dir.display()), 1),
        Err(e) => return (format!("{e:#}"), 1),
    };
    let r = match cmd {
        "remove" => remove(&dir, fp, &words[1..]),
        "refresh" => refresh(&dir.dir, name, &words[1..]),
        _ => split(&dir.dir, name, &fp),
    };
    match r {
        Ok(t) => (t, 0),
        Err(e) => (e, 1),
    }
}

/// The references REF names, refused when one names none
fn picked(refs: &[Value], want: &[String]) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for w in want {
        let hit: Vec<usize> = (0..refs.len()).filter(|&i| names(&refs[i], w)).collect();
        if hit.is_empty() {
            return Err(format!("raea videoform: no reference {w:?} (raea videoform ls NAME lists them)"));
        }
        out.extend(hit);
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

fn remove(dir: &Folder, mut fp: raea_sys_fingerprint::taxonomy::Fingerprint, want: &[String]) -> Result<String, String> {
    if want.is_empty() {
        return Err("usage: raea videoform remove NAME REF... (a title, a file's name, a link or path, or a hash's start)".into());
    }
    let refs = lines(&dir.dir);
    let gone = picked(&refs, want)?;
    let targets: Vec<&str> = gone.iter().filter_map(|&i| target(&refs[i])).collect();
    let section = fp.videoform.as_mut().unwrap();
    for t in &targets {
        section.references.retain(|r| r.target != *t);
    }
    dir.rewrite(&fp).map_err(|e| format!("{e:#}"))?;
    let kept: Vec<Value> = refs.iter().enumerate().filter(|(i, _)| !gone.contains(i)).map(|x| x.1.clone()).collect();
    write(&dir.dir, &kept)?;
    let said: Vec<&str> = gone.iter().map(|&i| refs[i]["source"]["title"].as_str().or(target(&refs[i])).unwrap_or("?")).collect();
    Ok(format!("raea videoform: {} no longer read ({} left); raea videoform report {} makes it again from them, your corrections kept", said.join(", "), kept.len(), fp.name))
}

fn refresh(dir: &Path, name: &str, want: &[String]) -> Result<String, String> {
    let refs = lines(dir);
    let again: Vec<usize> = if want.is_empty() {
        let behind: Vec<String> = behind_of(dir).into_iter().filter_map(|(r, _)| r["hash"].as_str().map(str::to_string)).collect();
        (0..refs.len()).filter(|&i| refs[i]["hash"].as_str().is_some_and(|h| behind.iter().any(|b| b == h))).collect()
    } else {
        picked(&refs, want)?
    };
    if again.is_empty() {
        return Ok(format!("raea videoform: every reference of {name} was read with this raea's encoders; nothing to read again"));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for &i in &again {
        let r = &refs[i];
        let Some(t) = target(r) else { continue };
        // its line taken out, so add reads it as new: its columns too, not the kept ones
        let rest: Vec<Value> = lines(dir).into_iter().filter(|x| x["hash"] != r["hash"]).collect();
        write(dir, &rest)?;
        let mut args = vec!["videoform".to_string(), "add".into(), name.into(), t.into()];
        for (flag, key) in [("--brand", "brand"), ("--group", "group")] {
            if let Some(v) = r["source"][key].as_str() {
                args.extend([flag.to_string(), v.to_string()]);
            }
        }
        let run = std::process::Command::new(&exe).args(&args).env("RAEA_COLUMNS_FRESH", "1").output().map_err(|e| e.to_string())?;
        let said = String::from_utf8_lossy(&run.stdout);
        let why = String::from_utf8_lossy(&run.stderr).lines().last().unwrap_or("not read").to_string();
        out.push(said.lines().find(|l| l.contains("read in")).map_or(format!("  {t}: {why}"), str::to_string));
        if !run.status.success() {
            // the line back, so a reference that can't be reached now isn't lost
            let mut back = lines(dir);
            back.push(r.clone());
            write(dir, &back)?;
        }
    }
    Ok(format!("raea videoform: {} read again with this raea's encoders\n{}", again.len(), out.join("\n")))
}

fn split(dir: &Path, name: &str, fp: &raea_sys_fingerprint::taxonomy::Fingerprint) -> Result<String, String> {
    let found: Value = serde_json::from_slice(&std::fs::read(dir.join("videoform/found.json")).map_err(|_| format!("raea videoform: {name} has no report yet (raea videoform report {name})"))?).map_err(|e| e.to_string())?;
    let Some(groups) = found["sets"]["groups"].as_array() else { return Err(format!("raea videoform: {name}'s references hold together: the report finds no two groups to split it into")) };
    // the references as the report numbers them: each read with its rows kept, in order
    let refs: Vec<Value> = lines(dir).into_iter().filter(|r| r["hash"].as_str().is_some_and(|h| dir.join("videoform/rows").join(format!("{h}.json")).is_file())).collect();
    let section = fp.videoform.as_ref().unwrap();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (g, members) in groups.iter().enumerate() {
        let new = format!("{name}-{}", ["a", "b"][g]);
        let to = Folder::named(&new);
        if to.exists() {
            return Err(format!("raea videoform: {new} is there already: split makes {name}-a and {name}-b afresh"));
        }
        let context = section.ranked().iter().find(|c| c.by == "person").map(|c| c.text.clone()).unwrap_or_default();
        let mut args = vec!["videoform".to_string(), "new".into(), new.clone(), "--context".into(), context];
        if let Some(e) = &fp.extends {
            args.extend(["--extends".into(), e.clone()]);
        }
        let made = std::process::Command::new(&exe).args(&args).output().map_err(|e| e.to_string())?;
        if !made.status.success() {
            return Err(format!("raea videoform: making {new}: {}", String::from_utf8_lossy(&made.stderr).trim()));
        }
        let mine: Vec<&Value> = members.as_array().into_iter().flatten().filter_map(|i| refs.get(i.as_u64()? as usize)).collect();
        copy(dir, &to.dir, &mine)?;
        let mut f = to.read(None).map_err(|e| format!("{e:#}"))?;
        let s = f.videoform.as_mut().unwrap();
        s.contexts = section.contexts.clone();
        s.corrections = section.corrections.clone();
        for r in &mine {
            let t = target(r).unwrap_or_default();
            s.add(t, section.references.iter().find(|x| x.target == t).and_then(|x| x.note.clone()));
        }
        to.rewrite(&f).map_err(|e| format!("{e:#}"))?;
        out.push(format!("  {new}: {} references ({})", mine.len(), mine.iter().filter_map(|r| r["source"]["title"].as_str()).collect::<Vec<_>>().join(", ")));
    }
    Ok(format!("raea videoform: {name} split in two, {name} kept as it is:\n{}\nraea videoform report on each makes it from its own references", out.join("\n")))
}

/// A group's references copied into another videoform's folder: their lines, rows and stills, and the files the
/// person's AI wrote (vocabulary.ts, briefs.ts)
fn copy(from: &Path, to: &Path, refs: &[&Value]) -> Result<(), String> {
    let err = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    std::fs::create_dir_all(to.join("videoform/rows")).map_err(|e| err(to, e))?;
    write(to, &refs.iter().map(|r| (*r).clone()).collect::<Vec<_>>())?;
    for r in refs {
        let Some(h) = r["hash"].as_str() else { continue };
        for ext in ["json", "f16"] {
            let p = from.join("videoform/rows").join(format!("{h}.{ext}"));
            std::fs::copy(&p, to.join("videoform/rows").join(format!("{h}.{ext}"))).map_err(|e| err(&p, e))?;
        }
        let stills = from.join("videoform/stills").join(h);
        if stills.is_dir() {
            let dest: PathBuf = to.join("videoform/stills").join(h);
            std::fs::create_dir_all(&dest).map_err(|e| err(&dest, e))?;
            for f in std::fs::read_dir(&stills).map_err(|e| err(&stills, e))?.flatten() {
                std::fs::copy(f.path(), dest.join(f.file_name())).map_err(|e| err(&f.path(), e))?;
            }
        }
    }
    for f in ["vocabulary.ts", "briefs.ts"] {
        if from.join(f).is_file() {
            std::fs::copy(from.join(f), to.join(f)).map_err(|e| err(&from.join(f), e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_is_named_by_its_title_file_link_or_hash() {
        let r: Value = serde_json::json!({"hash": "a5a5d6679c5f46fa", "source": {"path": "/x/DutchPet_Video1.mp4", "title": "Dutch Pet 1"}});
        assert!(names(&r, "dutch pet 1"));
        assert!(names(&r, "DutchPet_Video1"));
        assert!(names(&r, "/x/DutchPet_Video1.mp4"));
        assert!(names(&r, "a5a5d667"));
        assert!(!names(&r, "a5a5"), "a hash's start is eight characters or more");
    }
}
