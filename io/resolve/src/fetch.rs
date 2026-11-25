//! The yt-dlp raea runs: the official standalone binary for this platform, fetched on first use into ~/.raea/bin and
//! checked against its release's SHA-256 sums before it ever runs. Each raea release pins the version it was tested
//! with (PIN, its sums written here); when a site breaks, the latest release is fetched (its sums read from the
//! release) and the one before is kept, to go back to when the new one does worse. `RAEA_YTDLP` names another binary.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;

/// The yt-dlp this raea was tested with
pub const PIN: &str = "2026.10.26";
/// PIN's sums, from its release's SHA2-256SUMS
const SUMS: &[(&str, &str)] = &[
    ("yt-dlp_macos", "0f922c15ccc0b83eefa1c8bc9104e434318c4bf16e408e1ee93b7745a6efe0aa"),
    ("yt-dlp_linux", "767ba9dce7052aa196f8b12652f433d76de128ba168966fa84b6394d5e6c6795"),
    ("yt-dlp_linux_aarch64", "e87e9533e89fe5889abaa21f5114dfe7756d6aa6219b83551add04d48b18e483"),
    ("yt-dlp.exe", "f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d"),
];
const RELEASES: &str = "https://github.com/yt-dlp/yt-dlp/releases";

/// This platform's standalone binary, by its release asset's name
pub fn asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", _) => Some("yt-dlp_macos"),
        ("linux", "x86_64") => Some("yt-dlp_linux"),
        ("linux", "aarch64") => Some("yt-dlp_linux_aarch64"),
        ("windows", _) => Some("yt-dlp.exe"),
        _ => None,
    }
}

pub fn dir() -> PathBuf {
    raea_io_home::env_path("RAEA_BIN", ".raea/bin")
}

fn state_path() -> PathBuf {
    dir().join("yt-dlp.json")
}

/// A JSON file's object, or an empty one when it's missing or unreadable
pub fn json_file(path: &std::path::Path) -> Value {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| json!({}))
}

/// What's in use: {version, fetched, previous, failed: {site, at, why}}
pub fn state() -> Value {
    json_file(&state_path())
}

fn save(s: &Value) -> Result<(), String> {
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    std::fs::write(state_path(), serde_json::to_string_pretty(s).unwrap()).map_err(|e| format!("{}: {e}", state_path().display()))
}

fn path_of(version: &str) -> PathBuf {
    dir().join(format!("yt-dlp-{version}{}", if cfg!(windows) { ".exe" } else { "" }))
}

pub fn today() -> String {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0) as i64;
    // days since 1970-01-01 to a civil date (Howard Hinnant's days_from_civil, inverted)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{m:02}-{d:02}", yoe + era * 400 + i64::from(m <= 2))
}

/// The binary to run and its version, fetched first when it isn't here
pub fn current() -> Result<(PathBuf, String), String> {
    if let Some(p) = std::env::var_os("RAEA_YTDLP").filter(|v| !v.is_empty()) {
        return Ok((PathBuf::from(p), "RAEA_YTDLP".into()));
    }
    let version = state()["version"].as_str().unwrap_or(PIN).to_string();
    let path = path_of(&version);
    if !path.is_file() {
        fetch(&version)?;
        let mut s = state();
        s["version"] = json!(version);
        s["fetched"] = json!(today());
        save(&s)?;
    }
    Ok((path, version))
}

fn get(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url).set("User-Agent", "raea").timeout(std::time::Duration::from_secs(300)).call().map_err(|e| format!("{url}: {e}"))?;
    let mut body = Vec::new();
    resp.into_reader().take(200 << 20).read_to_end(&mut body).map_err(|e| format!("{url}: {e}"))?;
    Ok(body)
}

/// One version's binary into ~/.raea/bin, refused unless its sha256 is the release's
fn fetch(version: &str) -> Result<PathBuf, String> {
    let asset = asset().ok_or_else(|| format!("resolve: yt-dlp has no standalone binary for {} {}", std::env::consts::OS, std::env::consts::ARCH))?;
    let want = match SUMS.iter().find(|(a, _)| *a == asset).filter(|_| version == PIN) {
        Some((_, s)) => s.to_string(),
        None => {
            let sums = String::from_utf8_lossy(&get(&format!("{RELEASES}/download/{version}/SHA2-256SUMS"))?).into_owned();
            sums.lines().find_map(|l| l.split_once("  ").filter(|(_, name)| name.trim() == asset).map(|(s, _)| s.to_string())).ok_or_else(|| format!("resolve: yt-dlp {version}'s SHA2-256SUMS doesn't list {asset}"))?
        }
    };
    eprintln!("raea: fetching yt-dlp {version} (the page resolver, once) into {}", dir().display());
    let body = get(&format!("{RELEASES}/download/{version}/{asset}"))?;
    let got: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
    if got != want {
        return Err(format!("resolve: yt-dlp {version} ({asset}) has sha256 {got}, not the release's {want}: not run"));
    }
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let path = path_of(version);
    let part = path.with_extension("part");
    std::fs::write(&part, &body).map_err(|e| format!("{}: {e}", part.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&part, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// The latest release's version, from GitHub
fn latest() -> Result<String, String> {
    let body = get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")?;
    let v: Value = serde_json::from_slice(&body).map_err(|e| format!("resolve: the latest yt-dlp: {e}"))?;
    v["tag_name"].as_str().map(str::to_string).ok_or_else(|| "resolve: the latest yt-dlp release names no version".into())
}

/// Move to the latest release, keeping the one in use to go back to; None when this is the latest already
pub(crate) fn forward() -> Result<Option<(PathBuf, String)>, String> {
    if std::env::var_os("RAEA_YTDLP").is_some() {
        return Ok(None);
    }
    let (_, now) = current()?;
    let next = latest()?;
    if next == now {
        return Ok(None);
    }
    let path = fetch(&next)?;
    let mut s = state();
    s["previous"] = json!(now);
    s["version"] = json!(next);
    s["fetched"] = json!(today());
    save(&s)?;
    Ok(Some((path, next)))
}

/// Back to the version before, when the new one failed where it worked
pub(crate) fn back() -> Result<(), String> {
    let mut s = state();
    if let Some(prev) = s["previous"].as_str().map(str::to_string) {
        s["version"] = json!(prev);
        s["previous"] = Value::Null;
        save(&s)?;
    }
    Ok(())
}

/// The last site that failed, kept for `raea doctor`
pub(crate) fn failed(site: &str, why: &str) {
    let mut s = state();
    s["failed"] = json!({"site": site, "at": today(), "why": why});
    let _ = save(&s);
}

/// Between releases, the latest yt-dlp tried at most once a day when a link is read (its sums checked as any fetch's,
/// the one in use kept to go back to); a check that can't reach GitHub leaves the one in use as it is
pub(crate) fn daily() {
    let mut s = state();
    if std::env::var_os("RAEA_YTDLP").is_some() || s["checked"].as_str() == Some(today().as_str()) {
        return;
    }
    s["checked"] = json!(today());
    let _ = save(&s);
    if let Err(e) = forward() {
        eprintln!("raea: yt-dlp kept as it is (the day's check for a newer one: {e})");
    }
}

/// What `raea doctor` says of the resolver: the yt-dlp in use, when it was fetched, and the last site that failed
pub fn doctor() -> String {
    if let Some(p) = std::env::var_os("RAEA_YTDLP") {
        return format!("yt-dlp: {} (RAEA_YTDLP)", std::path::Path::new(&p).display());
    }
    let s = state();
    let version = s["version"].as_str().unwrap_or(PIN);
    let here = if path_of(version).is_file() { s["fetched"].as_str().map(|d| format!(", fetched {d}")).unwrap_or_default() } else { ", fetched on the first link read".into() };
    let pinned = if version == PIN { " (this raea's pin)".to_string() } else { format!(" (this raea pins {PIN})") };
    let failed = match &s["failed"] {
        Value::Null => String::new(),
        f => format!("; the last site that failed: {} on {} ({})", f["site"].as_str().unwrap_or("?"), f["at"].as_str().unwrap_or("?"), f["why"].as_str().unwrap_or("")),
    };
    format!("yt-dlp: {version}{pinned}{here}{failed}")
}

/// The Deno raea fetches for a site that makes yt-dlp run JavaScript (YouTube), when this machine has no runtime: the
/// version pinned here, checked against its release's sha256 sum, kept in ~/.raea/bin
pub const DENO: &str = "v2.5.6";

fn deno_asset() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "deno-aarch64-apple-darwin.zip",
        ("macos", "x86_64") => "deno-x86_64-apple-darwin.zip",
        ("linux", "x86_64") => "deno-x86_64-unknown-linux-gnu.zip",
        ("linux", "aarch64") => "deno-aarch64-unknown-linux-gnu.zip",
        ("windows", "x86_64") => "deno-x86_64-pc-windows-msvc.zip",
        _ => return None,
    })
}

/// Deno for yt-dlp: kept from before, else fetched once and checked
pub fn deno() -> Result<PathBuf, String> {
    let exe = dir().join(format!("deno-{DENO}{}", if cfg!(windows) { ".exe" } else { "" }));
    if exe.is_file() {
        return Ok(exe);
    }
    let Some(asset) = deno_asset() else { return Err(format!("resolve: Deno has no build for {} {}", std::env::consts::OS, std::env::consts::ARCH)) };
    let base = format!("https://github.com/denoland/deno/releases/download/{DENO}");
    let sum = String::from_utf8_lossy(&get(&format!("{base}/{asset}.sha256sum"))?).into_owned();
    let want = sum.split_whitespace().find(|w| w.len() == 64).ok_or_else(|| format!("resolve: Deno {DENO}'s sum for {asset} isn't one"))?.to_ascii_lowercase();
    eprintln!("raea: fetching Deno {DENO} (yt-dlp runs YouTube's JavaScript on it, once) into {}", dir().display());
    let body = get(&format!("{base}/{asset}"))?;
    let got: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
    if got != want {
        return Err(format!("resolve: Deno {DENO} ({asset}) has sha256 {got}, not the release's {want}: not run"));
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(body)).map_err(|e| format!("resolve: Deno's zip: {e}"))?;
    let mut inner = zip.by_index(0).map_err(|e| format!("resolve: Deno's zip: {e}"))?;
    let mut bytes = Vec::new();
    inner.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let part = exe.with_extension("part");
    std::fs::write(&part, bytes).map_err(|e| format!("{}: {e}", part.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&part, &exe).map_err(|e| e.to_string())?;
    Ok(exe)
}
