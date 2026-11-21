//! A video's URL to what raea reads of it (docs/videoform.feature VF1, VF1b). A platform page, a page with its own
//! player, a playlist, a channel or a direct link to a file is resolved by yt-dlp (fetch.rs) into each video's info:
//! its title, creator, date and streams. `spool` holds the chosen streams (formats.rs) in a private folder for the
//! readers, which take a file; the caller reads them and drops the folder, so no video is kept.
//!
//! When a site breaks (yt-dlp's extractor fails), the latest yt-dlp is fetched and tried once; if it fails too, raea
//! goes back to the one before and says which site failed. A login-only or private video is refused with the site's
//! reason and nothing is retried.

pub mod fetch;
pub mod formats;

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

pub use formats::Pick;

/// The site a URL is on, for what failed
pub fn site(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split(['/', '?', '#']).next().unwrap_or("").trim_start_matches("www.").to_string()
}

/// Whether a source is a URL rather than a path
pub fn is_url(s: &str) -> bool {
    s.split_once("://").is_some_and(|(scheme, _)| scheme == "https" || scheme == "http")
}

/// A refusal that a newer yt-dlp won't change: the video needs a login, is private or gone, or the page has no video
fn final_refusal(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    ["sign in", "log in", "login", "private", "members-only", "members only", "unavailable", "removed", "not available in your country", "unsupported url", "http error 404", "age-restricted"].iter().any(|k| e.contains(k))
}

/// yt-dlp with what every run needs: no config of the person's, its cache under raea's, and a JavaScript runtime for
/// the sites that make it run one (YouTube), from those on this machine
fn ytdlp(bin: &Path) -> Command {
    let mut c = Command::new(bin);
    c.args(["--ignore-config", "--no-warnings"]);
    if let Some(cache) = raea_io_home::caches() {
        c.arg("--cache-dir").arg(cache.join("yt-dlp"));
    }
    for rt in ["deno", "node", "bun"] {
        if let Some(p) = on_path(rt) {
            c.arg("--js-runtimes").arg(format!("{rt}:{}", p.display()));
        }
    }
    c
}

fn on_path(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(&exe)).find(|p| p.is_file())
}

/// One yt-dlp run's JSON, or its error line
fn run_json(bin: &Path, args: &[&str]) -> Result<Vec<Value>, String> {
    let out = ytdlp(bin).args(args).output().map_err(|e| format!("resolve: running {}: {e}", bin.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let infos: Vec<Value> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    if out.status.success() && !infos.is_empty() {
        return Ok(infos);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let line = err.lines().rev().find(|l| l.starts_with("ERROR")).or(err.lines().last()).unwrap_or("no video found").trim_start_matches("ERROR: ");
    Err(line.to_string())
}

/// A run, with the latest yt-dlp tried once when the extractor fails, and the one before kept when that fails too
fn with_resolver(url: &str, args: &[&str]) -> Result<Vec<Value>, String> {
    let (bin, _) = fetch::current()?;
    match run_json(&bin, args) {
        Ok(v) => Ok(v),
        Err(e) if final_refusal(&e) => Err(format!("{url}: refused by {}: {e}", site(url))),
        Err(first) => match fetch::forward() {
            Ok(Some((newer, version))) => match run_json(&newer, args) {
                Ok(v) => Ok(v),
                Err(e) => {
                    let _ = fetch::back();
                    fetch::failed(&site(url), &e);
                    Err(format!("{url}: {} can't be read now, by this yt-dlp or the latest ({version}): {e}. Add the file from your drive instead.", site(url)))
                }
            },
            _ => {
                fetch::failed(&site(url), &first);
                Err(format!("{url}: {} can't be read now: {first}. Add the file from your drive instead.", site(url)))
            }
        },
    }
}

/// Each video a URL names, as yt-dlp's info: a page is one, a playlist or a channel its newest `last` (all without)
pub fn resolve(url: &str, last: Option<usize>) -> Result<Vec<Value>, String> {
    let end = last.map(|n| n.to_string());
    let mut args = vec!["-J", "--flat-playlist"];
    if let Some(n) = &end {
        args.extend(["--playlist-end", n.as_str()]);
    }
    args.push(url);
    let top = with_resolver(url, &args)?.remove(0);
    if top["_type"].as_str() != Some("playlist") {
        return Ok(vec![top]);
    }
    let entries = top["entries"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for e in entries.iter().take(last.unwrap_or(usize::MAX)) {
        let Some(page) = e["url"].as_str().or(e["webpage_url"].as_str()) else { continue };
        match with_resolver(page, &["-J", "--no-playlist", page]) {
            Ok(mut v) => out.push(v.remove(0)),
            Err(e) => eprintln!("raea: {e}"),
        }
    }
    Ok(out)
}

/// The fields a reference keeps of its source, from yt-dlp's info
pub fn about(info: &Value) -> Value {
    // a page's own player numbers its videos, "(1)" on the first, even when it's the only one
    let title = info["title"].as_str().or(info["fulltitle"].as_str());
    let title = title.map(|t| if info["id"].as_str().is_some_and(|i| i.ends_with("-1")) { t.strip_suffix(" (1)").unwrap_or(t) } else { t });
    let date = info["upload_date"].as_str().filter(|d| d.len() == 8).map(|d| format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..]));
    serde_json::json!({
        "url": info["webpage_url"].as_str().or(info["original_url"].as_str()).or(info["url"].as_str()),
        "site": info["extractor_key"].as_str().or(info["extractor"].as_str()),
        "id": info["id"],
        "title": title,
        "creator": info["uploader"].as_str().or(info["channel"].as_str()).or(info["creator"].as_str()),
        "date": date,
        "views": info["view_count"],
        "likes": info["like_count"],
    })
}

/// The streams the readers take, held in `dir` (the caller's private folder, dropped once they're read): the picture's
/// file, and the sound's when it's a stream of its own
pub fn spool(info: &Value, pick: &Pick, dir: &Path) -> Result<(PathBuf, Option<PathBuf>), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = dir.join("info.json");
    std::fs::write(&json, info.to_string()).map_err(|e| e.to_string())?;
    let ids = match &pick.sound {
        Some(s) => format!("{},{s}", pick.picture),
        None => pick.picture.clone(),
    };
    let template = dir.join("%(format_id)s.%(ext)s");
    let (bin, _) = fetch::current()?;
    let out = ytdlp(&bin)
        .arg("--load-info-json")
        .arg(&json)
        .args(["-f", &ids, "--no-part", "--no-mtime", "--quiet", "--no-progress", "--concurrent-fragments", "4", "-o"])
        .arg(&template)
        .output()
        .map_err(|e| format!("resolve: running {}: {e}", bin.display()))?;
    let _ = std::fs::remove_file(&json);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().rev().find(|l| l.starts_with("ERROR")).unwrap_or("the stream didn't come").trim_start_matches("ERROR: ").to_string());
    }
    let found = |id: &str| -> Option<PathBuf> {
        let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
        std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_stem().is_some_and(|s| s.to_string_lossy() == id || s.to_string_lossy() == safe))
    };
    let picture = found(&pick.picture).ok_or_else(|| format!("the picture stream {} didn't come", pick.picture))?;
    let sound = pick.sound.as_deref().and_then(found);
    Ok((picture, sound))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_and_refusals() {
        assert_eq!(site("https://www.streettalk.com/video-library/ag1-street-interview/"), "streettalk.com");
        assert!(final_refusal("[youtube] abc: Sign in to confirm your age"));
        assert!(final_refusal("[instagram] x: This content is private"));
        assert!(!final_refusal("[youtube] abc: Unable to extract nsig function"));
        assert!(is_url("https://x.com/a") && !is_url("/Users/me/a.mp4"));
    }

    #[test]
    fn today_is_a_date() {
        let d = fetch::today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
    }
}
