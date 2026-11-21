//! `raea videoform add`: sources to references. A path is a file or a folder of them, read where it is; a link is
//! resolved (raea-io-resolve) into its videos, whose streams are held in a private folder while they're read and then
//! dropped. Each reference is one line of references.jsonl: where it came from, what it is, and its readings.

use crate::{Eval, flag, name_ok, pass, plain, readers, references, rights};
use raea_core_brief::Videoform;
use raea_sys_columns::Encoders;
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

const MEDIA: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi", "mxf", "mts"];

enum Source {
    File(PathBuf),
    Link(String),
}

fn holding() -> PathBuf {
    raea_io_home::caches().unwrap_or_else(std::env::temp_dir).join("videoform-reading")
}

/// Folders a run that was stopped left behind (a run's own folder is named by its process and goes when it ends)
fn sweep() {
    let Ok(d) = std::fs::read_dir(holding()) else { return };
    let hours = |e: &std::fs::DirEntry| e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map_or(0, |a| a.as_secs() / 3600);
    for e in d.flatten().filter(|e| hours(e) >= 1) {
        let _ = std::fs::remove_dir_all(e.path());
    }
}

fn sources(args: &[String], list: Option<&str>) -> Result<Vec<Source>, String> {
    let mut words: Vec<String> = args.to_vec();
    if let Some(l) = list {
        let text = std::fs::read_to_string(l).map_err(|e| format!("{l}: {e}"))?;
        // one source a line; blank lines and # comments skipped
        for l in text.lines().map(str::trim) {
            if !l.is_empty() && !l.starts_with('#') {
                words.push(l.to_string());
            }
        }
    }
    let mut out = Vec::new();
    for w in words {
        if raea_io_resolve::is_url(&w) {
            out.push(Source::Link(w));
            continue;
        }
        let p = PathBuf::from(&w);
        if p.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&p).map_err(|e| format!("{w}: {e}"))?.flatten().map(|e| e.path()).filter(|f| f.extension().is_some_and(|x| MEDIA.contains(&x.to_string_lossy().to_ascii_lowercase().as_str()))).collect();
            files.sort();
            out.extend(files.into_iter().map(Source::File));
        } else if p.is_file() {
            out.push(Source::File(p));
        } else {
            return Err(format!("raea videoform: {w} is neither a file, a folder nor a link"));
        }
    }
    Ok(out)
}

/// A reference in one line, for `ls` and `add`
pub(crate) fn line(r: &Value) -> String {
    let s = &r["source"];
    let what = s["title"].as_str().or(s["path"].as_str()).unwrap_or("?");
    let what = s["id"].as_str().map_or(what.to_string(), |id| format!("{what} [{id}]"));
    let by = s["creator"].as_str().map(|c| format!(", {c}")).unwrap_or_default();
    let rd = &r["reading"];
    let p = &rd["pace"];
    let loud = rd["loudness"]["lufs"].as_f64().map(|l| format!(", {l} LUFS")).unwrap_or_default();
    let said = rd["words"]["per_minute_spoken"].as_f64().filter(|w| *w > 0.0).map(|w| format!(", {w:.0} words a minute")).unwrap_or_default();
    format!(
        "  {what}{by}: {} s, {}x{}, {} shots ({} cuts a minute, median shot {} s){loud}{said}",
        rd["seconds"], rd["picture"]["width"], rd["picture"]["height"], p["shots"], p["cuts_per_minute"], p["shot_seconds"]["median"]
    )
}

struct Run<'a> {
    name: &'a str,
    /// the videoform's folder, and its section, which each reference read is listed in
    dir: PathBuf,
    section: Videoform,
    known: Vec<Value>,
    enc: Option<Encoders>,
    sheet: Option<PathBuf>,
}

impl Run<'_> {
    /// Whether a source is read already, by its link or its content hash, at this rows version and with its stills
    /// (one read before the rows changed, or before stills were kept, is read again)
    fn seen(&self, key: &str, v: &str) -> bool {
        let stills = |r: &Value| r["hash"].as_str().is_some_and(|h| raea_sys_videoform::rows::stills(&self.dir, h).is_dir());
        let rows = |r: &Value| r["reading"]["rows"]["version"].as_u64() == Some(u64::from(raea_sys_videoform::rows::VERSION)) && stills(r);
        self.known.iter().any(|r| (r["source"][key].as_str() == Some(v) || (key == "hash" && r["hash"].as_str() == Some(v))) && rows(r))
    }

    fn encoders(&mut self) -> Result<&mut Encoders, String> {
        if self.enc.is_none() {
            eprintln!("raea videoform: loading the encoders (SigLIP 2, CLAP, YAMNet)");
            self.enc = Some(Encoders::load().map_err(|e| format!("raea videoform: loading the encoders: {e:#}"))?);
        }
        Ok(self.enc.as_mut().unwrap())
    }

    /// Read one reference's streams and keep its line
    fn keep(&mut self, picture: &Path, sound: Option<&Path>, source: Value) -> Result<String, String> {
        let hash = readers::hash(picture, sound)?;
        if self.seen("hash", &hash) {
            return Ok(format!("  (already read: {})", source["title"].as_str().or(source["path"].as_str()).unwrap_or(&hash)));
        }
        let t = std::time::Instant::now();
        let work = holding().join(format!("{}-work-{}", std::process::id(), self.known.len()));
        let dir = self.dir.clone();
        let read = pass::read(picture, sound, &hash, self.encoders()?, &work, &dir);
        let _ = std::fs::remove_dir_all(&work);
        let reading = read?;
        if let Some(dir) = &self.sheet {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let out = dir.join(format!("{}-{}.jpg", self.name, &hash[..12]));
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let _ = std::process::Command::new(exe).args(["media", "sheet"]).arg(picture).args(["--n", "12", "-o"]).arg(&out).output();
        }
        let r = json!({"hash": hash, "source": source, "read": crate::today(), "read_seconds": (t.elapsed().as_secs_f64() * 10.0).round() / 10.0, "reading": reading});
        // its line, in place of one read before the rows changed
        let key = source["url"].as_str().or(source["path"].as_str()).map(str::to_string);
        self.known.retain(|k| k["hash"].as_str() != Some(hash.as_str()) && (key.is_none() || k["source"]["url"].as_str().or(k["source"]["path"].as_str()) != key.as_deref()));
        let path = self.dir.join("videoform/references.jsonl");
        std::fs::create_dir_all(self.dir.join("videoform")).map_err(|e| e.to_string())?;
        let mut f = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for k in self.known.iter().chain(std::iter::once(&r)) {
            writeln!(f, "{k}").map_err(|e| e.to_string())?;
        }
        let l = line(&r);
        let target = source["url"].as_str().or(source["path"].as_str()).unwrap_or(&hash);
        let note = [source["title"].as_str(), source["creator"].as_str()].into_iter().flatten().collect::<Vec<_>>().join(", ");
        self.section.add(target, Some(note).filter(|n| !n.is_empty()));
        self.known.push(r);
        Ok(format!("{l}  (read in {:.1} s)", t.elapsed().as_secs_f64()))
    }

    fn file(&mut self, p: &Path) -> Result<String, String> {
        let path = dunce::canonicalize(p).unwrap_or(p.to_path_buf());
        let source = json!({"path": path, "title": path.file_stem().map(|s| s.to_string_lossy().into_owned())});
        self.keep(&path, None, source)
    }

    fn link(&mut self, url: &str, last: Option<usize>, out: &mut Vec<String>, failed: &mut usize) {
        if self.seen("url", url) {
            out.push(format!("  (already read: {url})"));
            return;
        }
        let videos = match raea_io_resolve::resolve(url, last) {
            Ok(v) => v,
            Err(e) => {
                out.push(format!("  {e}"));
                *failed += 1;
                return;
            }
        };
        for info in videos {
            let source = raea_io_resolve::about(&info);
            let page = source["url"].as_str().unwrap_or(url).to_string();
            if self.seen("url", &page) {
                out.push(format!("  (already read: {page})"));
                continue;
            }
            let Some(pick) = raea_io_resolve::formats::pick(&info) else {
                out.push(format!("  {page}: no picture raea can read at 1080p or under"));
                *failed += 1;
                continue;
            };
            // the private folder its streams are held in while they're read, dropped after on success or failure
            let held = holding().join(format!("{}-{}", std::process::id(), out.len()));
            let stream = if pick.height > 0 { format!(" ({}p {})", pick.height, pick.vcodec) } else { String::new() };
            eprintln!("raea videoform: reading {}{stream}", source["title"].as_str().unwrap_or(&page));
            let done = raea_io_resolve::spool(&info, &pick, &held).and_then(|(pic, snd)| self.keep(&pic, snd.as_deref(), source.clone()));
            let _ = std::fs::remove_dir_all(&held);
            match done {
                Ok(l) => out.push(l),
                Err(e) => {
                    out.push(format!("  {page}: {e}"));
                    *failed += 1;
                }
            }
        }
    }
}

/// A cut read as a reference is, in a private folder dropped after: its measures, and its reading
pub(crate) fn measure(p: &Path) -> Result<(raea_sys_videoform::ranges::Measures, Value), String> {
    let hash = readers::hash(p, None)?;
    let work = holding().join(format!("{}-score", std::process::id()));
    let mut enc = Encoders::load().map_err(|e| format!("loading the encoders: {e:#}"))?;
    let read = pass::read(p, None, &hash, &mut enc, &work.join("work"), &work).and_then(|reading| {
        let m = crate::found::measured(&work, &json!({"hash": hash, "reading": reading})).ok_or("its rows weren't kept")?;
        Ok((m, reading))
    });
    let _ = std::fs::remove_dir_all(&work);
    read
}

/// `raea videoform add NAME SOURCE... [--from LIST] [--last N] [--context "…"] [--sheet DIR]`, the videoform made
/// first when it's new (with new's flags)
pub fn add(args: &[String], eval: Eval) -> (String, i32) {
    let words = plain(args);
    let Some(name) = words.first() else { return (format!("usage: {}", crate::USAGE), 2) };
    if !name_ok(name) {
        return (format!("raea videoform: {name:?} isn't a name (letters, digits, - and _)"), 2);
    }
    let last = match flag(args, "--last").map(str::parse::<usize>) {
        Some(Ok(n)) => Some(n),
        Some(Err(_)) => return ("raea videoform: --last takes a number".into(), 2),
        None => None,
    };
    let srcs = match sources(&words[1..], flag(args, "--from")) {
        Ok(s) => s,
        Err(e) => return (e, 2),
    };
    let mut said = Vec::new();
    if crate::verbs::open(name, None).is_err() {
        match crate::verbs::new(name, args) {
            Ok(m) => said.push(m),
            Err(e) => return (e, 1),
        }
    }
    let (folder, mut fp) = match crate::verbs::open(name, Some(eval)) {
        Ok(x) => x,
        Err(e) => return (e, 1),
    };
    let mut section = fp.videoform.take().unwrap_or_default();
    if let Some(c) = flag(args, "--context").map(str::trim).filter(|c| !c.is_empty()) {
        section.context = c.to_string();
    }
    if srcs.is_empty() {
        fp.videoform = Some(section);
        if let Err(e) = folder.rewrite(&fp) {
            return (format!("{e:#}"), 1);
        }
        said.push(format!("raea videoform: {name} has {} references; add files, folders or links to read more", references(&folder.dir).len()));
        return (said.join("\n"), 0);
    }
    if srcs.iter().any(|s| matches!(s, Source::Link(_))) {
        if let Err(e) = rights::ask() {
            return (e, 1);
        }
        sweep();
    }
    let mut run = Run { name, dir: folder.dir.clone(), section, known: references(&folder.dir), enc: None, sheet: flag(args, "--sheet").map(PathBuf::from) };
    let (mut out, mut failed) = (Vec::new(), 0usize);
    for s in &srcs {
        match s {
            Source::File(p) => match run.file(p) {
                Ok(l) => out.push(l),
                Err(e) => {
                    out.push(format!("  {}: {e}", p.display()));
                    failed += 1;
                }
            },
            Source::Link(u) => run.link(u, last, &mut out, &mut failed),
        }
    }
    fp.videoform = Some(run.section);
    if let Err(e) = folder.rewrite(&fp) {
        out.push(format!("  {}: {e:#}", folder.ts().display()));
        failed += 1;
    }
    let added = out.iter().filter(|l| l.contains("(read in")).count();
    let total = references(&folder.dir).len();
    let failures = if failed > 0 { format!(", {failed} couldn't be read") } else { String::new() };
    out.insert(0, format!("raea videoform: {name}, {total} references ({added} read now{failures})"));
    out.splice(0..0, said);
    // what they hold now, for the person to correct
    match crate::found::whole(name, &folder.dir, fp.videoform.as_ref().unwrap(), eval, false) {
        Ok(t) => out.push(format!("\n{t}")),
        Err(e) => out.push(format!("  what they hold: {e}")),
    }
    (out.join("\n"), if failed > 0 && added == 0 { 1 } else { 0 })
}
