//! The stage for a TypeScript space (one with edit.tsx: a React + TypeScript project, rendered by raea-motion). The
//! stage shows the project's render: final.mp4 from the agent's last `raea render` / `raea check` (host.rs) when it is
//! newer than every source, else a preview rendered here with raea-motion once the project settles. Either is kept
//! under previews/<version>.mp4 (stage.rs preview_path), the version being a hash of the project's sources' contents,
//! so a change shows as stale at once and going back to an earlier state finds its render again. Scrubbing a stale
//! project draws stills (`raea-motion render --frame N`), cached in frames.rs by (version, frame).

use std::path::{Path, PathBuf};

use anyhow::anyhow;
use serde_json::Value;

use crate::stage::{preview_path, previews_dir, Shape};

/// raea init's edit.tsx, unchanged: with no cuts, a new space's project has nothing to show yet.
const CLI_EDIT: &str = include_str!("../../cli/template/edit.tsx");

/// The project's state on disk: its sources' version, and when the newest of them changed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sources {
    pub version: u64,
    pub newest: Option<std::time::SystemTime>,
}

/// Root files that are a render's output or void's own, not sources (styles.gen.ts is rebuilt from brand/ and the
/// sources by every render, so it would make each render look like a change).
const NOT_SOURCES: &[&str] = &["styles.gen.ts", "render.json", "perception.json", "space.json", "terminal.json"];

/// Folders read whole: by content (code and styles), or by name, size and time (footage, sound and pictures).
const BY_CONTENT: &[&str] = &["components", "brand"];
const BY_STAMP: &[&str] = &["media", "assets"];

fn top(rel: &Path) -> &str {
    rel.components().next().and_then(|c| c.as_os_str().to_str()).unwrap_or("")
}

fn is_source(rel: &Path) -> bool {
    let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.starts_with('.') || name.ends_with(".part.mp4") {
        return false;
    }
    if rel.components().count() > 1 {
        // in components/, brand/, media/ or assets/: everything
        return true;
    }
    let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext, "tsx" | "ts" | "jsx" | "js" | "json" | "css")
        && !NOT_SOURCES.contains(&name)
        && !name.ends_with(".render.json")
        && !name.ends_with(".perception.json")
}

/// The project's sources hashed: the root's code and data (edit.tsx, cuts.ts, cuts.json, words.json, ...), components/
/// and brand/ by content (a file over 4 MB by its size and time), media/ and assets/ by name, size and time, and the
/// folder itself (two spaces with the same code can cut different takes). Blocking (a few small reads and stats): run
/// it off the UI thread.
pub fn sources(folder: &Path) -> Sources {
    use std::hash::{Hash, Hasher};
    let mut files: Vec<PathBuf> = Vec::new();
    let mut walk = vec![folder.to_path_buf()];
    while let Some(d) = walk.pop() {
        for p in std::fs::read_dir(&d).into_iter().flatten().flatten().map(|e| e.path()) {
            let rel = p.strip_prefix(folder).unwrap_or(&p).to_path_buf();
            if p.is_dir() {
                let hidden = rel.file_name().and_then(|n| n.to_str()).unwrap_or("").starts_with('.');
                let t = top(&rel);
                if (BY_CONTENT.contains(&t) || BY_STAMP.contains(&t)) && !hidden {
                    walk.push(p);
                }
            } else if is_source(&rel) {
                files.push(rel);
            }
        }
    }
    files.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    folder.hash(&mut h);
    let mut newest = None;
    for rel in files {
        let p = folder.join(&rel);
        // regular files only: a link to a device (media/null -> /dev/null) changes its time with every write to it
        let Ok(m) = std::fs::metadata(&p).map_err(|_| ()).and_then(|m| if m.is_file() { Ok(m) } else { Err(()) }) else { continue };
        rel.hash(&mut h);
        if m.len() < 4 << 20 && !BY_STAMP.contains(&top(&rel)) {
            std::fs::read(&p).unwrap_or_default().hash(&mut h);
        } else {
            (m.len(), m.modified().ok()).hash(&mut h);
        }
        newest = newest.max(m.modified().ok());
    }
    // never 0: the stage's "no version yet"
    Sources { version: h.finish() | 1, newest }
}

pub fn final_mp4(folder: &Path) -> PathBuf {
    folder.join("final.mp4")
}

/// final.mp4 is from these sources: written after the newest of them, and finished (untouched for a second; probe()
/// then has to read it whole, which an unfinished mp4 fails).
pub fn final_newer(folder: &Path, src: &Sources) -> bool {
    let Some(t) = std::fs::metadata(final_mp4(folder)).ok().filter(|m| m.len() > 0).and_then(|m| m.modified().ok()) else { return false };
    src.newest.is_none_or(|n| t >= n) && t.elapsed().is_ok_and(|e| e.as_secs_f64() > 1.0)
}

/// final.mp4 there at all and untouched for a second (an older render, shown while a new one is made).
pub fn final_settled(folder: &Path) -> bool {
    std::fs::metadata(final_mp4(folder)).ok().filter(|m| m.len() > 0).and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).is_some_and(|e| e.as_secs_f64() > 1.0)
}

/// A new space's project as raea init left it: the template's edit.tsx, no cuts and no render. Nothing to show.
pub fn blank(folder: &Path) -> bool {
    let edit = std::fs::read_to_string(folder.join("edit.tsx")).unwrap_or_default();
    let cuts = std::fs::read_to_string(folder.join("cuts.ts")).unwrap_or_default();
    let list = cuts.split_once("export const cuts").map(|(_, r)| r.split("];").next().unwrap_or(r)).unwrap_or("");
    edit.trim() == CLI_EDIT.trim() && !list.contains("from") && !final_mp4(folder).exists()
}

/// A video's size, rate and length (ffprobe), or None while it's unfinished or unreadable.
pub async fn probe(video: &Path) -> Option<Shape> {
    let out = tokio::process::Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate,nb_frames:format=duration", "-of", "json"])
        .arg(video)
        .kill_on_drop(true)
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())?;
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    let st = &v["streams"][0];
    let (w, h) = (st["width"].as_u64()?, st["height"].as_u64()?);
    let fps = st["r_frame_rate"]
        .as_str()
        .and_then(|r| r.split_once('/'))
        .and_then(|(a, b)| Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok()?.max(1.0)))
        .filter(|f| *f > 0.0)
        .unwrap_or(30.0);
    let frames = st["nb_frames"]
        .as_str()
        .and_then(|n| n.parse::<i64>().ok())
        .filter(|n| *n > 0)
        .or_else(|| v["format"]["duration"].as_str().and_then(|d| d.parse::<f64>().ok()).map(|d| (d * fps).round() as i64))?;
    Some(Shape { fps, width: w as u32, height: h as u32, frames: frames.max(1) })
}

/// The newest 8 previews kept.
fn prune() {
    let Ok(rd) = std::fs::read_dir(previews_dir()) else { return };
    let mut all: Vec<(std::time::SystemTime, PathBuf)> = rd.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    all.sort();
    let n = all.len();
    for (_, p) in all.into_iter().take(n.saturating_sub(8)) {
        let _ = std::fs::remove_file(p);
    }
}

/// `video` kept as preview `key`: a copy (on APFS std::fs::copy clones, so it's instant and takes no space until either
/// changes), so a later render rewriting final.mp4 never changes the video playing. Blocking.
pub fn keep(video: &Path, key: u64) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(previews_dir())?;
    let out = preview_path(key);
    if out.exists() {
        return Ok(out);
    }
    let part = previews_dir().join(format!("{key:x}.part.mp4"));
    std::fs::copy(video, &part)?;
    // a clone keeps the source's time: made now, as far as pruning goes
    let _ = std::fs::File::options().write(true).open(&part).and_then(|f| f.set_modified(std::time::SystemTime::now()));
    std::fs::rename(&part, &out)?;
    prune();
    Ok(out)
}

/// Whether the object's code (its root's and components/' .ts and .tsx) imports ./styles.gen.
pub fn imports_styles(folder: &Path) -> bool {
    [folder.to_path_buf(), folder.join("components")].iter().flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten()).any(|e| {
        let p = e.path();
        p.extension().is_some_and(|x| x == "ts" || x == "tsx") && p.file_name().is_some_and(|n| n != "styles.gen.ts") && std::fs::read_to_string(&p).is_ok_and(|t| t.contains("styles.gen"))
    })
}

/// styles.gen.ts built once per version (bun and Tailwind): a version's stills and its preview share it.
async fn styled(folder: &Path, version: u64) {
    static DONE: std::sync::Mutex<Option<(PathBuf, u64)>> = std::sync::Mutex::new(None);
    if DONE.lock().unwrap().as_ref().is_some_and(|(f, v)| f == folder && *v == version) {
        return;
    }
    match crate::host::styles(folder).await {
        Ok(()) => *DONE.lock().unwrap() = Some((folder.to_path_buf(), version)),
        // the render still runs, with the styles.gen.ts already there
        Err(e) => crate::mcp::log(format!("stage: styles: {e}")),
    }
}

fn motion(folder: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(crate::host::motion());
    cmd.arg("render").arg(folder.join("edit.tsx")).current_dir(folder).kill_on_drop(true);
    for (flag, file) in [("--words", "words.json"), ("--perception", "perception.json")] {
        if folder.join(file).exists() {
            cmd.arg(flag).arg(folder.join(file));
        }
    }
    cmd
}

/// What went wrong, from raea-motion's output: the composition's own errors, else its last lines.
fn failure(out: &std::process::Output) -> String {
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let lines: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty() && !l.contains("WARN")).collect();
    let js: Vec<&str> = lines.iter().copied().filter(|l| l.contains("[js]") || l.contains("Error") || l.contains("REFUSED") || l.contains("threw")).take(4).collect();
    if js.is_empty() { lines[lines.len().saturating_sub(3)..].join(" / ") } else { js.join(" / ") }
}

/// The project at `version` rendered whole into previews/. raea-motion sizes the frame from the composition (its --w
/// and --h only apply to a module with no registered composition), so the preview is full size: ~45 ms a frame.
/// At most 15 minutes.
pub async fn render(folder: &Path, version: u64) -> anyhow::Result<PathBuf> {
    let out = preview_path(version);
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(previews_dir())?;
    styled(folder, version).await;
    let part = previews_dir().join(format!("{version:x}.part.mp4"));
    let _ = std::fs::remove_file(&part);
    let run = motion(folder).arg("-o").arg(&part).output();
    let r = tokio::time::timeout(std::time::Duration::from_secs(900), run).await.map_err(|_| anyhow!("the preview took over 15 minutes and was stopped"))??;
    if !r.status.success() || !part.exists() {
        let _ = std::fs::remove_file(&part);
        return Err(anyhow!("the render failed: {}", failure(&r)));
    }
    std::fs::rename(&part, &out)?;
    prune();
    Ok(out)
}

/// Frame `f` of the project at `version` as a PNG (~0.4 s: raea-motion starts, bundles and draws one frame).
pub async fn still(folder: &Path, version: u64, f: i64) -> anyhow::Result<Vec<u8>> {
    styled(folder, version).await;
    let dir = crate::spaces::home().join("stills");
    std::fs::create_dir_all(&dir)?;
    let png = dir.join(format!("{version:x}-{f}.png"));
    let run = motion(folder).arg("-o").arg(&png).arg("--frame").arg(f.to_string()).output();
    let r = tokio::time::timeout(std::time::Duration::from_secs(60), run).await.map_err(|_| anyhow!("frame {f} took over a minute"))??;
    let bytes = std::fs::read(&png).ok().filter(|b| !b.is_empty());
    let _ = std::fs::remove_file(&png);
    match bytes {
        Some(b) if r.status.success() => Ok(b),
        _ => Err(anyhow!("frame {f}: {}", failure(&r))),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn version_follows_sources_not_outputs() {
        let d = std::env::temp_dir().join(format!("void-stage-ts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("components")).unwrap();
        std::fs::write(d.join("edit.tsx"), super::CLI_EDIT).unwrap();
        std::fs::write(d.join("cuts.ts"), "export const cuts: Cut[] = [\n\n];\n").unwrap();
        assert!(super::blank(&d));
        let v0 = super::sources(&d).version;
        // outputs and void's own files don't change it
        for f in ["styles.gen.ts", "render.json", "terminal.json", "final.mp4", ".looks"] {
            std::fs::write(d.join(f), "x").unwrap();
        }
        assert_eq!(super::sources(&d).version, v0);
        // a component does, and so does a cut
        std::fs::write(d.join("components/Title.tsx"), "export const T = 1;").unwrap();
        let v1 = super::sources(&d).version;
        assert_ne!(v1, v0);
        std::fs::write(d.join("cuts.ts"), "export const cuts: Cut[] = [\n{\"from\": 1, \"to\": 2}\n];\n").unwrap();
        assert_ne!(super::sources(&d).version, v1);
        let _ = std::fs::remove_file(d.join("final.mp4"));
        assert!(!super::blank(&d));
        assert!(super::is_source(Path::new("words.json")) && !super::is_source(Path::new("edit.render.json")));
        let _ = std::fs::remove_dir_all(&d);
    }
}
