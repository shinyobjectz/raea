//! A space's assets: the files the person brings in (the Files tab, ui.rs FilesPane), listed by kind, imported,
//! revealed and removed. Nothing leaves the machine: footage is linked or copied into the space's folder, never
//! uploaded.
//!
//! Where each kind lands, laid out for the space as a TypeScript project (edit.tsx, cuts.ts, components/,
//! brand/brand.css, worked through the `raea` CLI):
//!
//!   media/    video, audio and images: what an edit plays and shows, by path (`media/cj.mov`). A video's or audio
//!             file's transcript sits beside it as <stem>.words.json (brain.rs `listen`, as `import_media` makes it).
//!   models/   3D: glb, gltf, obj, fbx, usdz, usd, stl, ply
//!   docs/     documents: pdf, md, txt, rtf, doc, docx, pages, key, csv
//!   assets/   anything else (fonts, data, project files)
//!
//! A file under 256 MB is copied, so the project holds its own pictures, models and documents; a larger one is
//! linked, as `import_media` links footage (a source can be gigabytes, often on a read-only drive). Nothing is ever
//! written through a link: a folder on the way that is a link refuses the import, and removing a linked file removes
//! the link, never what it points to.

use std::path::{Path, PathBuf};

use crate::nomic;

/// Above this a file is linked rather than copied.
const COPY_UNDER: u64 = 256 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Video,
    Image,
    Audio,
    Model,
    Document,
    Other,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Video, Kind::Image, Kind::Audio, Kind::Model, Kind::Document, Kind::Other];

    pub fn of(path: &Path) -> Kind {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "mov" | "mp4" | "m4v" | "webm" | "mkv" | "avi" | "mts" | "mxf" => Kind::Video,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "svg" | "tif" | "tiff" | "bmp" | "avif" => Kind::Image,
            "wav" | "mp3" | "m4a" | "aac" | "aif" | "aiff" | "flac" | "ogg" | "caf" => Kind::Audio,
            "glb" | "gltf" | "obj" | "fbx" | "usdz" | "usd" | "usda" | "usdc" | "stl" | "ply" => Kind::Model,
            "pdf" | "md" | "txt" | "rtf" | "doc" | "docx" | "pages" | "key" | "csv" => Kind::Document,
            _ => Kind::Other,
        }
    }

    /// The folder in the space this kind lands in.
    pub fn dir(self) -> &'static str {
        match self {
            Kind::Video | Kind::Image | Kind::Audio => "media",
            Kind::Model => "models",
            Kind::Document => "docs",
            Kind::Other => "assets",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Video => "Video",
            Kind::Image => "Images",
            Kind::Audio => "Audio",
            Kind::Model => "3D",
            Kind::Document => "Documents",
            Kind::Other => "Other",
        }
    }

    /// Heard: imports are transcribed.
    pub fn heard(self) -> bool {
        matches!(self, Kind::Video | Kind::Audio)
    }
}

/// The folders the panel lists and may remove from.
pub const ROOTS: [&str; 4] = ["media", "models", "docs", "assets"];

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    /// Relative to the space's folder (media/cj.mov).
    pub rel: String,
    pub name: String,
    pub kind: Kind,
    pub bytes: u64,
    /// A link (to footage elsewhere).
    pub linked: bool,
    /// A link whose target is gone.
    pub broken: bool,
    /// Its transcript is beside it.
    pub words: bool,
}

/// Files the panel never shows: transcripts, the space's own bookkeeping, hidden files.
fn skipped(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".words.json") || name == "words.json"
}

/// The space's assets: every file under its asset folders (two levels deep in each, e.g. media/takes/<take>/video.mov),
/// by kind then name.
pub fn list(folder: &Path) -> Vec<Asset> {
    let mut out = Vec::new();
    for root in ROOTS {
        let mut walk = vec![(folder.join(root), 0usize)];
        while let Some((dir, depth)) = walk.pop() {
            for p in std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()) {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if skipped(&name) {
                    continue;
                }
                let Ok(lmeta) = std::fs::symlink_metadata(&p) else { continue };
                let linked = lmeta.file_type().is_symlink();
                let meta = std::fs::metadata(&p).ok();
                if meta.as_ref().is_some_and(|m| m.is_dir()) {
                    if depth < 2 {
                        walk.push((p, depth + 1));
                    }
                    continue;
                }
                let rel = p.strip_prefix(folder).unwrap_or(&p).to_string_lossy().into_owned();
                let stem = name.rsplit_once('.').map(|s| s.0).unwrap_or(&name);
                let words = p.with_file_name(format!("{stem}.words.json")).exists() || p.with_file_name("words.json").exists();
                out.push(Asset { kind: Kind::of(&p), bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0), broken: meta.is_none(), linked, words, name, rel });
            }
        }
    }
    out.sort_by(|a, b| (a.kind, a.rel.to_lowercase()).cmp(&(b.kind, b.rel.to_lowercase())));
    out
}

/// An error when any folder from the space's (exclusive) down to `dir` (inclusive) is a link: writing or removing
/// there would reach through it, onto a drive the space only points at.
fn no_links(folder: &Path, dir: &Path) -> anyhow::Result<()> {
    let rel = dir.strip_prefix(folder).map_err(|_| anyhow::anyhow!("{} is outside the space", dir.display()))?;
    let mut at = folder.to_path_buf();
    for c in rel.components() {
        anyhow::ensure!(matches!(c, std::path::Component::Normal(_)), "{} isn't a plain path in the space", dir.display());
        at.push(c);
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            anyhow::bail!("{} is a link; void doesn't write through links", at.strip_prefix(folder).unwrap_or(&at).display());
        }
    }
    Ok(())
}

/// What became of one file brought in.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub dest: PathBuf,
    pub rel: String,
    pub kind: Kind,
    pub linked: bool,
    /// It was already in the space (the same file): nothing was written.
    pub already: bool,
}

/// Bring `src` into the space at `folder`: into its kind's folder, named as `import_media` names it, copied when small
/// and linked when large; a different file of the same name gets a numbered name. Blocking (a copy can take a while).
pub fn place(folder: &Path, src: &Path) -> anyhow::Result<Placed> {
    let meta = std::fs::metadata(src).map_err(|e| anyhow::anyhow!("{}: {e}", src.display()))?;
    anyhow::ensure!(meta.is_file(), "{} isn't a file", src.display());
    let kind = Kind::of(src);
    let dir = folder.join(kind.dir());
    no_links(folder, &dir)?;
    std::fs::create_dir_all(&dir)?;
    let name = crate::brain::media_name(src);
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.clone(), String::new()),
    };
    let real_src = std::fs::canonicalize(src)?;
    let mut n = 1;
    let dest = loop {
        let candidate = if n == 1 { dir.join(&name) } else { dir.join(format!("{stem}-{n}{ext}")) };
        if std::fs::symlink_metadata(&candidate).is_err() {
            break candidate;
        }
        // the same file already here (linked or copied before): use it
        let same = std::fs::canonicalize(&candidate).is_ok_and(|c| c == real_src)
            || std::fs::metadata(&candidate).is_ok_and(|m| {
                m.len() == meta.len()
                    && (m.modified().ok() == meta.modified().ok()
                        || (m.len() < 64 << 20 && std::fs::read(&candidate).ok() == std::fs::read(src).ok()))
            });
        if same {
            let rel = candidate.strip_prefix(folder).unwrap_or(&candidate).to_string_lossy().into_owned();
            let linked = std::fs::symlink_metadata(&candidate).is_ok_and(|m| m.file_type().is_symlink());
            return Ok(Placed { dest: candidate, rel, kind, linked, already: true });
        }
        n += 1;
    };
    let linked = if meta.len() >= COPY_UNDER {
        match std::os::unix::fs::symlink(&real_src, &dest) {
            Ok(()) => true,
            Err(_) => {
                std::fs::copy(src, &dest)?;
                false
            }
        }
    } else {
        std::fs::copy(src, &dest)?;
        false
    };
    let rel = dest.strip_prefix(folder).unwrap_or(&dest).to_string_lossy().into_owned();
    Ok(Placed { dest, rel, kind, linked, already: false })
}

/// The files to import from what was dropped or picked: files as they are, a folder's own files (not its subfolders),
/// hidden files and transcripts left out.
pub fn expand(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut inner: Vec<PathBuf> = std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path())
                .filter(|f| f.is_file() && !skipped(&f.file_name().unwrap_or_default().to_string_lossy()))
                .collect();
            inner.sort();
            out.extend(inner);
        } else if p.is_file() {
            out.push(p.clone());
        }
    }
    out
}

/// Remove one asset (its path relative to the space). A link is removed, never its target; a transcript made beside
/// it goes with it. Only files under the asset folders, and never through a linked folder.
pub fn remove(folder: &Path, rel: &str) -> anyhow::Result<()> {
    let rel = rel.trim().trim_start_matches("./");
    anyhow::ensure!(!rel.starts_with('/') && !rel.split('/').any(|c| c == ".." || c.is_empty()), "not a path in the space: {rel}");
    anyhow::ensure!(ROOTS.iter().any(|r| rel.starts_with(&format!("{r}/"))), "only files under {} are removed here", ROOTS.join("/, "));
    let path = folder.join(rel);
    let parent = path.parent().ok_or_else(|| anyhow::anyhow!("no folder"))?;
    no_links(folder, parent)?;
    let meta = std::fs::symlink_metadata(&path)?;
    anyhow::ensure!(!meta.is_dir(), "{rel} is a folder");
    // remove_file on a link removes the link itself
    std::fs::remove_file(&path)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = name.rsplit_once('.').map(|s| s.0).unwrap_or(&name);
    let words = path.with_file_name(format!("{stem}.words.json"));
    if std::fs::symlink_metadata(&words).is_ok_and(|m| m.is_file()) {
        let _ = std::fs::remove_file(words);
    }
    Ok(())
}

/// Show the file in Finder (the link itself for a linked file, so it's the space's entry that's selected).
pub fn reveal(path: &Path) {
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
}

/// A video's or audio file's length, from raea-media (`rdb media probe`).
pub fn seconds(path: &Path) -> Option<f64> {
    let out = std::process::Command::new(crate::stage::rdb_bin()).args(["media", "probe"]).arg(path).output().ok()?;
    let j: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    j["duration"].as_f64().or_else(|| j["duration"].as_str()?.parse().ok()).filter(|d| *d > 0.0)
}

pub fn size_label(bytes: u64) -> String {
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.0} KB", bytes as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", bytes as f64 / 1e6),
        _ => format!("{:.2} GB", bytes as f64 / 1e9),
    }
}

pub fn length_label(secs: f64) -> String {
    let s = secs.round() as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// A file's thumbnail: the cached PNG on disk as a file URL, or none
/// yet (`make_thumbs` draws the missing ones in the background).
pub fn thumb_url(path: &Path) -> String {
    cached_thumb(path).map(|p| format!("file://{}", p.display())).unwrap_or_default()
}

/// The cached thumbnail, if Quick Look has drawn it (never draws one).
pub fn cached_thumb(path: &Path) -> Option<PathBuf> {
    use sha2::Digest;
    let meta = std::fs::metadata(path).ok()?;
    let stamp = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let key = format!("{:x}", sha2::Sha256::digest(format!("{}|{}|{stamp}|{}", path.display(), meta.len(), 160)));
    let png = crate::spaces::home().join("thumbs").join(format!("{}.png", &key[..24]));
    png.exists().then_some(png)
}

/// The thumbnails not drawn yet, drawn (blocking: Quick Look, up to 6 s a file); whether any were.
pub fn make_thumbs(paths: &[PathBuf]) -> bool {
    let mut made = false;
    for p in paths {
        if cached_thumb(p).is_none() && quick_look(p, 160).is_some() {
            made = true;
        }
    }
    made
}

/// A small PNG of the file (Quick Look: a picture, a video's poster frame, a PDF's first page, a model where macOS
/// can draw one), cached in ~/.raea-void/thumbs by path, size and time. None when Quick Look draws nothing.
pub fn thumbnail(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(quick_look(path, 160)?).ok()
}

/// Quick Look's picture of a file at `size` px, as a PNG in the cache (keyed by path, size, time and pixels).
fn quick_look(path: &Path, px: u32) -> Option<PathBuf> {
    use sha2::Digest;
    let meta = std::fs::metadata(path).ok()?;
    let stamp = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let key = format!("{:x}", sha2::Sha256::digest(format!("{}|{}|{stamp}|{px}", path.display(), meta.len())));
    let cache = crate::spaces::home().join("thumbs");
    let png = cache.join(format!("{}.png", &key[..24]));
    if png.exists() {
        return Some(png);
    }
    // Quick Look drew nothing for this file before: don't wait on it again
    let none = cache.join(format!("{}.none", &key[..24]));
    if none.exists() {
        return None;
    }
    let tmp = cache.join(format!("tmp-{}", &key[..24]));
    std::fs::create_dir_all(&tmp).ok()?;
    // at most 6 s: with no generator for a file (a .glb macOS can't read), qlmanage can wait forever
    let ok = std::process::Command::new("qlmanage").args(["-t", "-s", &px.to_string(), "-o"]).arg(&tmp).arg(path)
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn()
        .is_ok_and(|mut child| {
            let t0 = std::time::Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(st)) => break st.success(),
                    Ok(None) if t0.elapsed() < std::time::Duration::from_secs(6) => std::thread::sleep(std::time::Duration::from_millis(40)),
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break false;
                    }
                }
            }
        });
    let made = std::fs::read_dir(&tmp).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "png"));
    let out = match (ok, made) {
        (true, Some(m)) => std::fs::rename(&m, &png).ok().map(|_| png),
        _ => None,
    };
    let _ = std::fs::remove_dir_all(&tmp);
    if out.is_none() {
        let _ = std::fs::write(&none, b"");
    }
    out
}

// ── search ───────────────────────────────────────────────────────────────────────────────────────────────────────
//
// Assets are found by what they are, not only by name. Each is indexed three ways:
//   pictures  SigLIP 2 (raea/meaning/embed's `raea-embed`, the same vectors the host app's phone makes) over an image
//             itself, three stills of a video, or Quick Look's picture of a PDF or a model; a query is compared as
//             SigLIP text ("woman holding a coffee cup" finds the shot)
//   speech    nomic-embed-text v1.5 (nomic.rs) over a transcript in short windows, each with its time, and over a
//             text document (md, txt, csv) in windows ("talks about the price" finds the take and the moment)
//   words     name, transcript and text contents, for plain matches
// The index is <space>/.assets-index.json, rebuilt per file when the file changes. Without raea-embed or the nomic
// model, that channel is skipped and the others still answer.

/// Transcript windows: this many words each, at most this many per file; a document's windows are longer.
const SAID_WORDS: usize = 20;
const SAID_MAX: usize = 200;
const DOC_WORDS: usize = 60;
/// What the index's vectors were made with: an index made with other models is rebuilt.
const MODELS: &str = "siglip2-base-p16-256+nomic-embed-v1.5";

/// raea-embed: RAEA_EMBED, else raea/meaning's release build.
pub fn embed_bin() -> PathBuf {
    std::env::var_os("RAEA_EMBED")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ai/meaning/target/release/raea-embed"))
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Index {
    model: String,
    items: std::collections::BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Entry {
    /// size and modified time: the entry is rebuilt when either changes
    stamp: String,
    /// name, transcript and text contents, lowercased, for plain matches
    text: String,
    /// picture vectors, with what each is ("still at 3.2 s", "picture")
    #[serde(default)]
    seen: Vec<(String, Vec<f32>)>,
    /// transcript windows (start s, words, vector), or a document's (start -1)
    #[serde(default)]
    said: Vec<(f64, String, Vec<f32>)>,
}

fn index_path(folder: &Path) -> PathBuf {
    folder.join(".assets-index.json")
}

/// A file's size and time, and its transcript's: a transcript written after the file was indexed (an import still
/// listening) remakes the entry.
fn stamp(path: &Path) -> String {
    let one = |p: &Path| {
        let m = std::fs::metadata(p).ok();
        let t = m.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        format!("{}-{t}", m.map(|m| m.len()).unwrap_or(0))
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = name.rsplit_once('.').map(|s| s.0).unwrap_or(&name).to_string();
    format!("{}/{}", one(path), one(&path.with_file_name(format!("{stem}.words.json"))))
}

/// raea-embed over many inputs in one process ("image" files or "text" phrases), in batches; each input's vector, or
/// None when it couldn't be read. Vectors are unit length.
fn embed(what: &str, inputs: &[String]) -> Vec<Option<Vec<f32>>> {
    let mut out: std::collections::HashMap<String, Vec<f32>> = std::collections::HashMap::new();
    let bin = embed_bin();
    if !bin.exists() {
        return vec![None; inputs.len()];
    }
    for batch in inputs.chunks(96) {
        let Ok(o) = std::process::Command::new(&bin).arg(what).args(batch).stderr(std::process::Stdio::null()).output() else { continue };
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            let Ok(j) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let key = j[if what == "image" { "file" } else { "text" }].as_str().unwrap_or("").to_string();
            let v: Vec<f32> = j["v"].as_array().into_iter().flatten().filter_map(|x| x.as_f64()).map(|x| x as f32).collect();
            let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
            out.insert(key, v.iter().map(|x| x / n).collect());
        }
    }
    inputs.iter().map(|i| out.get(i).cloned()).collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// A file's words: its transcript as (start s, word), from <stem>.words.json or a take's words.json beside it.
fn transcript(path: &Path) -> Vec<(f64, String)> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = name.rsplit_once('.').map(|s| s.0).unwrap_or(&name).to_string();
    [path.with_file_name(format!("{stem}.words.json")), path.with_file_name("words.json")]
        .into_iter()
        .find_map(|p| serde_json::from_str::<Vec<serde_json::Value>>(&std::fs::read_to_string(p).ok()?).ok())
        .unwrap_or_default()
        .iter()
        .map(|w| (w["start_ms"].as_f64().unwrap_or(0.0) / 1000.0, w["text"].as_str().unwrap_or("").trim().to_string()))
        .filter(|w| !w.1.is_empty())
        .collect()
}

/// What to look at for a file's pictures: (label, an image file raea-embed reads). Stills go in `scratch`.
fn pictures(path: &Path, kind: Kind, scratch: &Path) -> Vec<(String, PathBuf)> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match kind {
        Kind::Image if matches!(ext.as_str(), "png" | "jpg" | "jpeg") => vec![("picture".into(), path.to_path_buf())],
        Kind::Video => {
            let secs = seconds(path).unwrap_or(0.0);
            let _ = std::fs::create_dir_all(scratch);
            [0.15, 0.5, 0.85].iter().enumerate().filter_map(|(i, f)| {
                let at = secs * f;
                let out = scratch.join(format!("{}-{i}.jpg", std::process::id()));
                let ok = std::process::Command::new(crate::stage::rdb_bin()).args(["media", "still"]).arg(path)
                    .args(["--at", &format!("{at:.2}"), "-o"]).arg(&out).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                    .status().is_ok_and(|s| s.success());
                // each still renamed to its own file, so a batch reads them all
                let kept = scratch.join(format!("{:x}-{i}.jpg", path.to_string_lossy().bytes().fold(0u64, |h, b| h.wrapping_mul(131).wrapping_add(b as u64))));
                (ok && std::fs::rename(&out, &kept).is_ok()).then(|| (format!("still at {at:.1} s"), kept))
            }).collect()
        }
        Kind::Audio => Vec::new(),
        _ => quick_look(path, 256).map(|p| vec![("picture".into(), p)]).unwrap_or_default(),
    }
}

/// The space's index brought up to date: entries made for new or changed assets, dropped for removed ones. How many
/// were made. Blocking: a new video takes a few seconds (three stills and its transcript embedded).
pub fn reindex(folder: &Path) -> anyhow::Result<usize> {
    // one at a time: the panel, an import and the agent's search may all ask at once
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let mut idx: Index = std::fs::read_to_string(index_path(folder)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let model = format!("{}{MODELS}", if embed_bin().exists() { "" } else { "no-siglip:" });
    let model = if nomic::available() { model } else { format!("no-nomic:{model}") };
    let seeing = embed_bin().exists();
    if idx.model != model {
        idx = Index { model: model.into(), items: Default::default() };
    }
    let assets = list(folder);
    let live: std::collections::HashSet<&str> = assets.iter().map(|a| a.rel.as_str()).collect();
    let before = idx.items.len();
    idx.items.retain(|rel, _| live.contains(rel.as_str()));
    let mut changed = idx.items.len() != before;
    let todo: Vec<&Asset> = assets.iter().filter(|a| !a.broken && idx.items.get(&a.rel).is_none_or(|e| e.stamp != stamp(&folder.join(&a.rel)))).collect();
    if todo.is_empty() {
        if changed { std::fs::write(index_path(folder), serde_json::to_string(&idx)?)?; }
        return Ok(0);
    }
    let scratch = crate::spaces::home().join("thumbs").join("stills");
    let mut entries: Vec<(String, Entry, Vec<(String, PathBuf)>, Vec<(f64, String)>)> = Vec::new();
    for a in &todo {
        let path = folder.join(&a.rel);
        let words = transcript(&path);
        let stem = a.name.rsplit_once('.').map(|s| s.0).unwrap_or(&a.name).replace(['-', '_', '.'], " ");
        let mut text = format!("{stem} {}", a.rel);
        if !words.is_empty() {
            text.push(' ');
            text.push_str(&words.iter().map(|w| w.1.as_str()).collect::<Vec<_>>().join(" "));
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut windows: Vec<(f64, String)> = words.chunks(SAID_WORDS).take(SAID_MAX)
            .map(|c| (c[0].0, c.iter().map(|w| w.1.as_str()).collect::<Vec<_>>().join(" ")))
            .collect();
        if matches!(ext.as_str(), "md" | "txt" | "csv") {
            if let Ok(t) = std::fs::read_to_string(&path) {
                let t: String = t.chars().take(40_000).collect();
                text.push(' ');
                text.push_str(&t);
                let ws: Vec<&str> = t.split_whitespace().collect();
                windows.extend(ws.chunks(DOC_WORDS).take(SAID_MAX).map(|c| (-1.0, c.join(" "))));
            }
        }
        let pics = if seeing { pictures(&path, a.kind, &scratch) } else { Vec::new() };
        entries.push((a.rel.clone(), Entry { stamp: stamp(&path), text: text.to_lowercase(), ..Default::default() }, pics, windows));
    }
    {
        let files: Vec<String> = entries.iter().flat_map(|e| e.2.iter().map(|p| p.1.to_string_lossy().into_owned())).collect();
        let phrases: Vec<String> = entries.iter().flat_map(|e| e.3.iter().map(|w| w.1.clone())).collect();
        // pictures and speech side by side: two child processes
        let said = std::thread::spawn(move || nomic::embed_all("doc", &phrases));
        let mut fv = embed("image", &files).into_iter();
        let mut pv = said.join().unwrap_or_default().into_iter();
        for (_, entry, pics, windows) in entries.iter_mut() {
            for (label, _) in pics.iter() {
                if let Some(Some(v)) = fv.next() { entry.seen.push((label.clone(), v)); }
            }
            for (at, words) in windows.iter() {
                if let Some(Some(v)) = pv.next() { entry.said.push((*at, words.clone(), v)); }
            }
        }
        for (_, _, pics, _) in &entries {
            for (_, p) in pics { if p.starts_with(&scratch) { let _ = std::fs::remove_file(p); } }
        }
    }
    let n = entries.len();
    for (rel, entry, _, _) in entries {
        idx.items.insert(rel, entry);
    }
    changed |= n > 0;
    if changed {
        std::fs::write(index_path(folder), serde_json::to_string(&idx)?)?;
    }
    Ok(n)
}

/// One asset a search found: how well it matched (0-1), and why ("seen: still at 3.2 s", "said at 0:12: …").
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub rel: String,
    pub score: f32,
    pub why: String,
}

const STOP: &[&str] = &["a", "an", "the", "of", "in", "on", "with", "and", "or", "to", "for", "is", "at", "by", "it", "that", "this", "my", "me", "about", "where", "who", "some", "from"];

/// The space's assets that match `query`, best first: by meaning (SigLIP 2, pictures and speech) when `semantic` and
/// raea-embed is there, and always by words (name, transcript, text). Reads the index as it is: `reindex` first.
pub fn search(folder: &Path, query: &str, semantic: bool) -> Vec<Hit> {
    let idx: Index = std::fs::read_to_string(index_path(folder)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let terms: Vec<&str> = q.split(|c: char| !c.is_alphanumeric()).filter(|t| t.len() > 1 && !STOP.contains(t)).collect();
    // the query as SigLIP text (against pictures) and as a nomic query (against speech and documents), side by side
    let (qv, tv) = if semantic {
        let q2 = q.clone();
        let t = std::thread::spawn(move || query_vector("nomic", &q2));
        (query_vector("siglip", &q), t.join().ok().flatten())
    } else {
        (None, None)
    };
    let mut hits: Vec<Hit> = idx.items.iter().filter_map(|(rel, e)| {
        let mut best = (0.0f32, String::new());
        // words: every term found scores; in the name counts more (the name is the text's first words)
        if !terms.is_empty() {
            let name = rel.to_lowercase();
            let found = terms.iter().filter(|t| e.text.contains(**t)).count() as f32 / terms.len() as f32;
            let named = terms.iter().filter(|t| name.contains(**t)).count() as f32 / terms.len() as f32;
            let w = (0.55 * found + 0.4 * named).min(0.95);
            if w > best.0 {
                best = (w, if named > 0.0 { "name".into() } else { "words".into() });
            }
        }
        if let Some(qv) = &qv {
            // pictures: SigLIP's image-text cosines sit low (a match is ~0.10-0.18), mapped onto 0-1
            for (label, v) in &e.seen {
                let s = ((cosine(qv, v) - 0.04) / 0.12).clamp(0.0, 1.0);
                if s > best.0 { best = (s, format!("seen: {label}")); }
            }
        }
        if let Some(tv) = &tv {
            // speech and documents: nomic's query-document cosines, mapped onto 0-1 (CALIBRATION below)
            for (at, words, v) in &e.said {
                let s = ((cosine(tv, v) - SAID_FLOOR) / SAID_SPAN).clamp(0.0, 1.0);
                let quote: String = words.chars().take(90).collect();
                let why = if *at < 0.0 { format!("reads: “{quote}”") } else { format!("said at {}: “{quote}”", length_label(*at)) };
                if s > best.0 { best = (s, why); }
            }
        }
        (best.0 >= 0.3).then(|| Hit { rel: rel.clone(), score: (best.0 * 100.0).round() / 100.0, why: best.1 })
    }).collect();
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(30);
    hits
}

/// Where nomic's cosine starts to mean related, and the span to a sure match (CALIBRATION).
const SAID_FLOOR: f32 = 0.5;
const SAID_SPAN: f32 = 0.25;

/// A query's vector ("siglip" or "nomic"), remembered for the app's life (each model takes a second or so to load).
fn query_vector(model: &str, q: &str) -> Option<Vec<f32>> {
    static SEEN: std::sync::Mutex<Vec<(String, Vec<f32>)>> = std::sync::Mutex::new(Vec::new());
    let key = format!("{model}|{q}");
    if let Some(v) = SEEN.lock().ok()?.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone()) {
        return Some(v);
    }
    let v = if model == "nomic" { nomic::embed_all("query", &[q.to_string()]) } else { embed("text", &[q.to_string()]) }.pop().flatten()?;
    let q = key.as_str();
    let mut seen = SEEN.lock().ok()?;
    if seen.len() > 200 { seen.remove(0); }
    seen.push((q.to_string(), v.clone()));
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let base = std::env::var_os("VOID_TEST_TMP").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let d = base.join(format!("void-assets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn kinds_land_in_their_folders() {
        for (f, k, d) in [("a.MOV", Kind::Video, "media"), ("b.png", Kind::Image, "media"), ("c.m4a", Kind::Audio, "media"),
                          ("d.glb", Kind::Model, "models"), ("e.pdf", Kind::Document, "docs"), ("f.ttf", Kind::Other, "assets")] {
            assert_eq!(Kind::of(Path::new(f)), k, "{f}");
            assert_eq!(k.dir(), d, "{f}");
        }
    }

    #[test]
    fn place_list_remove() {
        let root = scratch("place");
        let space = root.join("space");
        let outside = root.join("outside");
        std::fs::create_dir_all(&space).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        for f in ["My Logo.png", "robot.glb", "brief.pdf", "font.ttf"] {
            std::fs::write(outside.join(f), f.as_bytes()).unwrap();
        }
        let p = place(&space, &outside.join("My Logo.png")).unwrap();
        assert_eq!((p.rel.as_str(), p.linked, p.already), ("media/My-Logo.png", false, false));
        // the same file again is the one already there
        assert!(place(&space, &outside.join("My Logo.png")).unwrap().already);
        // a different file of that name is numbered
        let other = root.join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("My Logo.png"), b"a different picture").unwrap();
        assert_eq!(place(&space, &other.join("My Logo.png")).unwrap().rel, "media/My-Logo-2.png");
        assert_eq!(place(&space, &outside.join("robot.glb")).unwrap().rel, "models/robot.glb");
        assert_eq!(place(&space, &outside.join("brief.pdf")).unwrap().rel, "docs/brief.pdf");
        assert_eq!(place(&space, &outside.join("font.ttf")).unwrap().rel, "assets/font.ttf");
        // a transcript beside a clip isn't listed; the clip says it has words
        std::fs::write(space.join("media/take.mov"), b"x").unwrap();
        std::fs::write(space.join("media/take.words.json"), b"[]").unwrap();
        let all = list(&space);
        let rels: Vec<&str> = all.iter().map(|a| a.rel.as_str()).collect();
        assert_eq!(rels, ["media/take.mov", "media/My-Logo-2.png", "media/My-Logo.png", "models/robot.glb", "docs/brief.pdf", "assets/font.ttf"]);
        assert!(all[0].words);
        // removing a clip takes its transcript
        remove(&space, "media/take.mov").unwrap();
        assert!(!space.join("media/take.words.json").exists());
        // outside the asset folders, or climbing out, is refused
        assert!(remove(&space, "space.json").is_err());
        assert!(remove(&space, "media/../../outside/robot.glb").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn links_are_removed_never_their_targets() {
        let root = scratch("links");
        let space = root.join("space");
        let drive = root.join("drive");
        std::fs::create_dir_all(space.join("media")).unwrap();
        std::fs::create_dir_all(&drive).unwrap();
        std::fs::write(drive.join("big.mov"), b"footage").unwrap();
        std::os::unix::fs::symlink(drive.join("big.mov"), space.join("media/big.mov")).unwrap();
        let a = list(&space);
        assert!(a[0].linked && !a[0].broken);
        remove(&space, "media/big.mov").unwrap();
        assert!(drive.join("big.mov").exists(), "the target survives");
        assert!(std::fs::symlink_metadata(space.join("media/big.mov")).is_err(), "the link is gone");
        // a linked media folder is never written or removed through
        std::fs::remove_dir(space.join("media")).unwrap();
        std::os::unix::fs::symlink(&drive, space.join("media")).unwrap();
        std::fs::write(root.join("pic.png"), b"p").unwrap();
        assert!(place(&space, &root.join("pic.png")).is_err());
        assert!(remove(&space, "media/big.mov").is_err());
        assert!(drive.join("big.mov").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn search_finds_words_in_names_and_documents() {
        let space = scratch("search");
        std::fs::create_dir_all(space.join("docs")).unwrap();
        std::fs::create_dir_all(space.join("media")).unwrap();
        std::fs::write(space.join("docs/brief.md"), "The client sells cold brew coffee in cans.").unwrap();
        std::fs::write(space.join("media/logo-dark.svg"), "<svg/>").unwrap();
        assert_eq!(reindex(&space).unwrap(), 2);
        assert_eq!(reindex(&space).unwrap(), 0, "unchanged files aren't indexed again");
        let hits = search(&space, "cold brew", false);
        assert_eq!(hits.first().map(|h| h.rel.as_str()), Some("docs/brief.md"));
        let hits = search(&space, "logo", false);
        assert_eq!((hits[0].rel.as_str(), hits[0].why.as_str()), ("media/logo-dark.svg", "name"));
        assert!(search(&space, "zebra", false).is_empty());
        remove(&space, "docs/brief.md").unwrap();
        reindex(&space).unwrap();
        assert!(search(&space, "cold brew", false).is_empty(), "a removed file leaves the index");
        let _ = std::fs::remove_dir_all(&space);
    }
}
