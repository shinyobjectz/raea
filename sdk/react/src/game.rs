//! Exported games (sdk/react/docs/game-export.md): a game made into a program that plays on its own, outside void and the
//! dev tree.
//!
//!   raea-motion export-game EDIT.tsx -o OUT [--target mac-app|mac-cli] [--name NAME] [--icon PNG] [--keys SETS]
//!
//! bundles the game for live play's runtime ahead of time (bun runs here, never in the export), gathers every file the
//! bundle names (media, staticFile paths, glTF models and what they name, generated sounds, the brand's CSS and its
//! fonts) into the package with paths relative to it, works out what play would work out at open (the composition's
//! size and rate, its 3D, its sound master) and puts the player beside it: a macOS .app (Contents/MacOS, Resources/game,
//! Info.plist) or a folder (the player and game/), signed ad hoc.
//!
//! The player is `raea-game` (src/bin/raea-game.rs, feature `game`): it opens a window and plays the package's game at
//! its rate with keys and pointer, its sound on the one mixer core and its 3D on rdb's engine. F11, Cmd+F or Ctrl+Cmd+F
//! toggles fullscreen; Cmd+Q or Cmd+W quits.
//!
//!   raea-game [--game DIR]                                     play (the package's own game by default)
//!   raea-game --selftest 5s [--snapshot S OUT.png]... [--input-log F] [--json F] [--keys SETS]
//!
//! --selftest plays offscreen (no window, nothing shown, the sound mixed and kept silent) with scripted keys and pointer,
//! then prints what it measured: frames presented and missed, input changes stepped into the game, sound mixed (peak and
//! RMS of the mixer's own output) and panics. Its last line is PASS or FAIL.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::live::Live;

type R<T> = Result<T, String>;

/// The package's manifest, beside the bundle.
const MANIFEST: &str = "game.json";
/// The page's fonts, packaged at export (Live::packaged_fonts_css): what the player reads in place of fetching any
const FONTS: &str = "fonts.css";

/// The packaged font sheet with each inlined face written once to fonts/ and named there: a page's family stack (Inter,
/// Segoe UI, Roboto, Noto Sans ...) inlines the same face under every name, 16 copies of 8 faces in pong (lightspeed R7).
/// fonts_in puts the same data URLs back, quoted as they were, so the page reads the sheet it was exported with.
fn fonts_out(css: &str, game: &Path) -> R<String> {
    use base64::Engine as _;
    let (mut out, mut rest) = (String::with_capacity(css.len() / 8), css);
    while let Some(i) = rest.find("url(") {
        out.push_str(&rest[..i + 4]);
        let after = &rest[i + 4..];
        let q = if after.starts_with('"') { "\"" } else { "" };
        let body = &after[q.len()..];
        let parsed = body.strip_prefix("data:").and_then(|d| {
            let m = d.find(";base64,")?;
            let end = d[m + 8..].find(|c: char| c == '"' || c == ')')? + m + 8;
            Some((&d[..m], &d[m + 8..end], q.len() + 5 + end))
        });
        let Some((mime, b64, end)) = parsed else {
            rest = after;
            continue;
        };
        let ext = match mime {
            "font/woff2" => "woff2",
            "font/woff" => "woff",
            "font/otf" => "otf",
            _ => "ttf",
        };
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| format!("fonts.css: {e}"))?;
        // FNV-1a of the face: the same face under another family is the same file
        let h = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
        let rel = format!("fonts/{h:016x}.{ext}");
        let to = game.join(&rel);
        if !to.exists() {
            std::fs::create_dir_all(game.join("fonts")).map_err(|e| e.to_string())?;
            std::fs::write(&to, &bytes).map_err(|e| format!("{}: {e}", to.display()))?;
        }
        out.push_str(&format!("{q}{rel}"));
        rest = &after[end..];
    }
    out.push_str(rest);
    Ok(out)
}

/// fonts_out undone: each url("fonts/NAME") or url(fonts/NAME) in the sheet as the data URL it was exported from.
fn fonts_in(css: &str, dir: &Path) -> String {
    use base64::Engine as _;
    let (mut out, mut rest) = (String::with_capacity(css.len() * 4), css);
    while let Some(i) = rest.find("url(") {
        out.push_str(&rest[..i + 4]);
        let after = &rest[i + 4..];
        let q = if after.starts_with('"') { 1 } else { 0 };
        let body = &after[q..];
        let named = body.starts_with("fonts/").then(|| body.find(|c: char| c == '"' || c == ')')).flatten();
        let Some(close) = named else {
            rest = after;
            continue;
        };
        let rel = &body[..close];
        let mime = match rel.rsplit('.').next() {
            Some("woff2") => "font/woff2",
            Some("woff") => "font/woff",
            Some("otf") => "font/otf",
            _ => "font/ttf",
        };
        out.push_str(&after[..q]);
        match std::fs::read(dir.join(rel)) {
            Ok(b) => out.push_str(&format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&b))),
            Err(_) => out.push_str(rel),
        }
        rest = &body[close..];
    }
    out.push_str(rest);
    out
}
/// The key sets the self-test cycles through when the export names none: arrows, space, a run and jump, and nothing.
const DEFAULT_KEYS: &str = "ArrowLeft,ArrowRight,ArrowUp,ArrowDown, ,ArrowRight+ArrowUp,";

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// The player's dSYM (profile.game's packed split debuginfo: the shipped player has no symbols) kept by its UUID under
/// ~/Library/Caches/raea/dsym, so a player's panic.txt or crash.txt (which carry the UUID and load address) can be
/// symbolicated later: `atos -o ~/Library/Caches/raea/dsym/UUID/raea-game.dSYM -l LOAD ADDR...`. Never shipped.
fn keep_dsym(player: &Path) {
    let dsym = player.with_extension("dSYM");
    if !dsym.is_dir() {
        return;
    }
    let Ok(o) = std::process::Command::new("dwarfdump").arg("--uuid").arg(&dsym).output() else { return };
    let text = String::from_utf8_lossy(&o.stdout);
    let Some(uuid) = text.split_whitespace().nth(1).filter(|u| u.len() == 36) else { return };
    let Some(home) = std::env::var_os("HOME") else { return };
    let to = Path::new(&home).join("Library/Caches/raea/dsym").join(uuid).join("raea-game.dSYM");
    if to.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(to.parent().unwrap());
    if std::process::Command::new("ditto").arg(&dsym).arg(&to).status().map(|s| s.success()).unwrap_or(false) {
        eprintln!("raea-motion export-game: the player's symbols kept at {}", to.display());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Export

/// `raea-motion export-game EDIT.tsx -o OUT ...`
pub fn export(args: &[String]) -> R<String> {
    let usage = "raea-motion export-game EDIT.tsx -o OUT [--target mac-app|mac-cli] [--name NAME] [--icon PNG] [--keys SETS] [--player BIN]";
    let tsx = PathBuf::from(args.get(1).filter(|a| !a.starts_with("--")).ok_or(usage)?);
    let tsx = tsx.canonicalize().map_err(|e| format!("{}: {e}", tsx.display()))?;
    let dir = tsx.parent().unwrap().to_path_buf();
    let out = PathBuf::from(flag(args, "-o").ok_or(usage)?);
    let target = flag(args, "--target").unwrap_or_else(|| if out.extension().is_some_and(|x| x == "app") { "mac-app".into() } else { "mac-cli".into() });
    if target != "mac-app" && target != "mac-cli" {
        return Err(format!("--target {target}: mac-app or mac-cli (Windows and Linux aren't there yet: sdk/react/docs/game-export.md)"));
    }
    let name = flag(args, "--name").unwrap_or_else(|| default_name(&tsx));
    let exec = slug(&name);
    let t0 = Instant::now();

    // OUT is made afresh; an earlier export there is replaced, anything else is left alone
    if out.exists() {
        let ours = out.join("Contents/Resources/game").join(MANIFEST).exists() || out.join("game").join(MANIFEST).exists();
        if !ours {
            return Err(format!("{} exists and isn't an exported game; choose another -o", out.display()));
        }
        std::fs::remove_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    let (exe_dir, game) = if target == "mac-app" { (out.join("Contents/MacOS"), out.join("Contents/Resources/game")) } else { (out.clone(), out.join("game")) };
    std::fs::create_dir_all(&exe_dir).map_err(|e| format!("{}: {e}", exe_dir.display()))?;
    std::fs::create_dir_all(&game).map_err(|e| format!("{}: {e}", game.display()))?;

    // the player first: an export that can't have one stops before the bundling
    // the newest of the release build's raea-game and the game profile's (target/game: fat LTO, no symbols)
    let player = match flag(args, "--player") {
        Some(p) => PathBuf::from(p),
        None => {
            let me = std::env::current_exe().map_err(|e| e.to_string())?;
            let cands = [me.with_file_name("raea-game"), me.parent().and_then(|d| d.parent()).map(|t| t.join("game/raea-game")).unwrap_or_default()];
            let newest = cands.iter().filter(|p| p.is_file()).max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok()).cloned();
            newest.unwrap_or_else(|| cands[0].clone())
        }
    };
    if !player.exists() {
        return Err(format!("no player at {} (build it: cd react && cargo build --profile game --bin raea-game --no-default-features --features game)", player.display()));
    }
    // the release build's player is ~128 MB stripped, the game profile's ~74 MB (fat LTO, one codegen unit; lightspeed
    // R7): say so when the bigger one ships because it's the newer
    if !player.to_string_lossy().contains("/game/") && flag(args, "--player").is_none() {
        let mb = std::fs::metadata(&player).map(|m| m.len() as f64 / 1048576.0).unwrap_or(0.0);
        eprintln!("raea-motion export-game: shipping the release build's player ({mb:.0} MB before stripping): `cd react && cargo build --profile game --bin raea-game --no-default-features --features game` makes the ~74 MB one, which ships when it's the newer");
    }
    keep_dsym(&player);


    // 1. the bundle, for live play's runtime, with imported media copied in and named relatively
    let mut opts: Vec<String> = Vec::new();
    for (f, file) in [("--words", "words.json"), ("--perception", "perception.json")] {
        let p = dir.join(file);
        if p.exists() {
            opts.push(f.into());
            opts.push(p.display().to_string());
        }
    }
    opts.push("--lite".into());
    let js = game.join("bundle.js");
    crate::motion::bundle_composition(&tsx, &js, &crate::motion::comp_opts(&opts)?, Some(&game))?;

    // 2. every file it names, gathered (paths outside the game's folder rewritten into the package)
    let mut bundle = std::fs::read_to_string(&js).map_err(|e| e.to_string())?;
    let mut got = Gathered::default();
    gather(&mut bundle, &dir, &game, &mut got)?;
    std::fs::write(&js, &bundle).map_err(|e| e.to_string())?;

    // 3. what play works out at open, worked out now from the package itself (so the package is known to load)
    let src = std::fs::read_to_string(&tsx).unwrap_or_default();
    let responsive = src.contains("useVideoConfig(") || std::fs::read_dir(dir.join("components")).into_iter().flatten().flatten().any(|e| std::fs::read_to_string(e.path()).is_ok_and(|t| t.contains("useVideoConfig(")));
    let (w, h, fps, frames, fonts) = {
        let mut live = Live::open_bundle(&js, &game, (1, 1), false)?;
        // the fonts its page injects, each remote face fetched now and inlined: the player has no network and fetches
        // nothing (lightspeed R7)
        let fonts = live.packaged_fonts_css()?;
        (live.w, live.h, live.fps, live.frames, fonts)
    };
    if let Some(css) = &fonts {
        let css = fonts_out(css, &game)?;
        std::fs::write(game.join(FONTS), css).map_err(|e| format!("{}: {e}", game.join(FONTS).display()))?;
    }
    // 3D (@raea/motion/3d): live play draws it from each frame's own state (Live, scene3d::LiveScene)
    let has_3d = bundle.contains("data-3d");
    // sound: files that sound, or notes and instruments (@raea/motion/tone, /music) in the game's own sources (the
    // bundle carries the SDK's sound code whether the game sounds or not)
    let sound = got.sounds > 0 || sources_use(&dir, &["@raea/motion/tone", "@raea/motion/music", "@raea/motion/seedling", "motion/src/tone", "motion/src/music", "motion/src/seedling"]);
    // the game's master (its render's gain and ceiling): stepping its scripted run through every frame, here and once
    let master = if sound { crate::motion::master_gain(&js, &game)? } else { None };
    let keys = flag(args, "--keys").unwrap_or_else(|| DEFAULT_KEYS.into());
    let manifest = json!({
        "name": name,
        "width": w, "height": h, "fps": fps, "frames": frames,
        "responsive": responsive,
        "has_3d": has_3d,
        "sound": sound,
        "master": master.map(|(g, c)| json!({"gain_db": g, "ceiling_dbtp": c})),
        "selftest_keys": keys,
        "exported_from": tsx.display().to_string(),
        "exported_at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "assets": got.files.iter().map(|(rel, _)| rel.clone()).collect::<Vec<_>>(),
        "fonts_css": fonts.as_ref().map(|_| FONTS),
    });
    std::fs::write(game.join(MANIFEST), serde_json::to_string_pretty(&manifest).unwrap()).map_err(|e| e.to_string())?;

    // 4. the player, stripped of its symbols, and (an app) its Info.plist and icon
    let exe = exe_dir.join(&exec);
    std::fs::copy(&player, &exe).map_err(|e| format!("{}: {e}", exe.display()))?;
    if !args.iter().any(|a| a == "--no-strip") {
        // `strip` from the command line tools; a player already stripped (the game profile) is left as it is
        let _ = std::process::Command::new("strip").arg("-x").arg(&exe).output();
    }
    if target == "mac-app" {
        let icon = match flag(args, "--icon") {
            Some(png) => Some(make_icns(Path::new(&png), &out.join("Contents/Resources/AppIcon.icns"))?),
            None => None,
        };
        std::fs::write(out.join("Contents/Info.plist"), info_plist(&name, &exec, icon.is_some())).map_err(|e| e.to_string())?;
        std::fs::write(out.join("Contents/PkgInfo"), "APPL????").map_err(|e| e.to_string())?;
    }

    // 5. signed ad hoc, so it runs here (distribution needs a Developer ID and notarization: game-export.md)
    let sign = std::process::Command::new("codesign").args(["--force", "--sign", "-", "--timestamp=none"]).arg(if target == "mac-app" { &out } else { &exe }).output().map_err(|e| format!("codesign: {e}"))?;
    if !sign.status.success() {
        return Err(format!("codesign: {}", String::from_utf8_lossy(&sign.stderr).trim()));
    }

    let exe_mb = std::fs::metadata(&exe).map(|m| m.len()).unwrap_or(0) as f64 / 1e6;
    let game_mb = dir_size(&game) as f64 / 1e6;
    let total_mb = dir_size(&out) as f64 / 1e6;
    let report = json!({
        "out": out.display().to_string(),
        "target": target,
        "name": name,
        "executable": exe.display().to_string(),
        "executable_mb": (exe_mb * 10.0).round() / 10.0,
        "player": player.display().to_string(),
        "game_mb": (game_mb * 100.0).round() / 100.0,
        "total_mb": (total_mb * 10.0).round() / 10.0,
        "bundle_kb": std::fs::metadata(&js).map(|m| m.len() / 1024).unwrap_or(0),
        "page": [w, h], "fps": fps, "responsive": responsive, "has_3d": has_3d, "sound": sound,
        "master": manifest["master"],
        "assets": got.files.len(),
        "rewritten": got.rewritten,
        "seconds": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    });
    Ok(format!("EXPORTED  {}", serde_json::to_string_pretty(&report).unwrap()))
}

/// Whether any of the game's own sources (its folder, two levels down) says one of `needles`.
fn sources_use(dir: &Path, needles: &[&str]) -> bool {
    fn walk(d: &Path, needles: &[&str], depth: usize) -> bool {
        let Ok(rd) = std::fs::read_dir(d) else { return false };
        rd.flatten().any(|e| {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" {
                return false;
            }
            if p.is_dir() {
                return depth < 2 && walk(&p, needles, depth + 1);
            }
            matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx" | "js" | "jsx")) && std::fs::read_to_string(&p).is_ok_and(|t| needles.iter().any(|n| t.contains(n)))
        })
    }
    walk(dir, needles, 0)
}

fn default_name(tsx: &Path) -> String {
    let stem = tsx.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let base = if stem == "edit" { tsx.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or(stem) } else { stem };
    base.split(['-', '_', ' ']).filter(|w| !w.is_empty()).map(|w| {
        let mut c = w.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }).collect::<Vec<_>>().join(" ")
}

fn slug(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "game".into() } else { s }
}

fn dir_size(d: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(d) else { return std::fs::metadata(d).map(|m| m.len()).unwrap_or(0) };
    rd.flatten().map(|e| {
        let p = e.path();
        match std::fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() => dir_size(&p),
            Ok(m) => m.len(),
            Err(_) => 0,
        }
    }).sum()
}

#[derive(Default)]
struct Gathered {
    /// (path in the package, source)
    files: Vec<(String, PathBuf)>,
    /// literals in the bundle pointed into the package
    rewritten: usize,
    sounds: usize,
}

impl Gathered {
    fn has(&self, rel: &str) -> bool {
        self.files.iter().any(|(r, _)| r == rel)
    }
}

const SOURCE_EXTS: [&str; 7] = ["ts", "tsx", "js", "jsx", "mjs", "rrl", "moves"];
const SOUND_EXTS: [&str; 7] = ["wav", "mp3", "m4a", "aac", "flac", "ogg", "aif"];

/// One file into the package at `rel` (once), and what a glTF names beside it.
fn put(src: &Path, rel: &str, game: &Path, got: &mut Gathered) -> R<()> {
    if got.has(rel) {
        return Ok(());
    }
    let to = game.join(rel);
    if let Some(p) = to.parent() {
        std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    // through links (media is often linked in from a drive): the package holds the bytes
    std::fs::copy(src, &to).map_err(|e| format!("{}: {e}", src.display()))?;
    got.files.push((rel.to_string(), src.to_path_buf()));
    let ext = src.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
    if SOUND_EXTS.contains(&ext.as_str()) || ext == "mp4" || ext == "mov" {
        got.sounds += 1;
    }
    if ext == "gltf" || ext == "glb" {
        let base = Path::new(rel).parent().unwrap_or(Path::new("")).to_path_buf();
        for uri in gltf_uris(src) {
            let from = src.parent().unwrap_or(Path::new(".")).join(&uri);
            if from.is_file() {
                put(&from, &base.join(&uri).to_string_lossy(), game, got)?;
            }
        }
    }
    Ok(())
}

/// The files a glTF names (buffers and images by relative uri; data: URIs are inside it already).
fn gltf_uris(file: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(file) else { return Vec::new() };
    let json: Option<Value> = if bytes.starts_with(b"glTF") && bytes.len() > 20 {
        let len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        bytes.get(20..20 + len).and_then(|j| serde_json::from_slice(j).ok())
    } else {
        serde_json::from_slice(&bytes).ok()
    };
    let Some(j) = json else { return Vec::new() };
    let mut out = Vec::new();
    for k in ["buffers", "images"] {
        for it in j.get(k).and_then(Value::as_array).into_iter().flatten() {
            if let Some(u) = it.get("uri").and_then(Value::as_str).filter(|u| !u.starts_with("data:") && !u.contains("://")) {
                out.push(percent_decode(u));
            }
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Every string in the bundle that names a file: under the game's folder it goes into the package at the same place;
/// elsewhere (an absolute path) into media/_ext/ with the string rewritten to it. Then the brand's CSS and the fonts it
/// names, and the generated sounds whose prompts the bundle carries.
fn gather(bundle: &mut String, dir: &Path, game: &Path, got: &mut Gathered) -> R<()> {
    let re = regex::Regex::new(r#""((?:[^"\\\n]|\\.){1,600})"|'((?:[^'\\\n]|\\.){1,600})'|`([^`$\\\n]{1,600})`"#).unwrap();
    let mut lits: Vec<String> = re.captures_iter(bundle).filter_map(|c| c.get(1).or(c.get(2)).or(c.get(3)).map(|m| m.as_str().replace("\\/", "/"))).collect();
    lits.sort();
    lits.dedup();
    // already in the package (imported media, copied by the bundling): counted, not copied again
    if let Ok(rd) = std::fs::read_dir(game.join("media")) {
        for e in rd.flatten().filter(|e| e.path().is_file()) {
            let rel = format!("media/{}", e.file_name().to_string_lossy());
            got.files.push((rel, e.path()));
            if SOUND_EXTS.contains(&e.path().extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase().as_str()) {
                got.sounds += 1;
            }
        }
    }
    let inside = |p: &Path| -> Option<String> { p.canonicalize().ok().and_then(|c| c.strip_prefix(dir.canonicalize().ok()?).ok().map(|r| r.to_string_lossy().to_string())) };
    let mut rewrites: Vec<(String, String)> = Vec::new();
    for s in &lits {
        if s.contains("://") || s.starts_with("data:") || s.len() < 2 || s.contains('\u{0}') {
            continue;
        }
        let decoded = percent_decode(s);
        let mut cands: Vec<PathBuf> = Vec::new();
        if let Some(rest) = decoded.strip_prefix("/media/") {
            cands.push(dir.join("media").join(rest));
        }
        if decoded.starts_with('/') {
            cands.push(PathBuf::from(&decoded));
        }
        cands.push(dir.join(decoded.trim_start_matches("./").trim_start_matches('/')));
        cands.push(dir.join("media").join(decoded.trim_start_matches("./").trim_start_matches('/')));
        let Some(file) = cands.into_iter().find(|p| p.is_file()) else { continue };
        let ext = file.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if SOURCE_EXTS.contains(&ext.as_str()) || file.file_name().is_some_and(|n| n == MANIFEST) {
            continue;
        }
        match inside(&file) {
            Some(rel) => {
                put(&file, &rel, game, got)?;
                // an absolute path into the game's folder: relative now
                if decoded.starts_with('/') && !decoded.starts_with("/media/") {
                    rewrites.push((s.clone(), rel));
                }
            }
            None => {
                let canon = file.canonicalize().unwrap_or(file.clone());
                let h = twox_hash::XxHash3_64::oneshot(canon.to_string_lossy().as_bytes());
                let rel = format!("media/_ext/{:08x}-{}", h as u32, file.file_name().unwrap().to_string_lossy());
                put(&file, &rel, game, got)?;
                rewrites.push((s.clone(), rel));
            }
        }
    }
    for (from, to) in rewrites {
        for q in ['"', '\'', '`'] {
            let (a, b) = (format!("{q}{from}{q}"), format!("{q}{to}{q}"));
            if bundle.contains(&a) {
                *bundle = bundle.replace(&a, &b);
                got.rewritten += 1;
            }
        }
    }
    // the brand kit (brand_css: the nearest brand/brand.css at or above the edit's folder) and the files its CSS names
    let mut d = Some(dir);
    for _ in 0..5 {
        let Some(here) = d else { break };
        let css = here.join("brand").join("brand.css");
        if css.is_file() {
            put(&css, "brand/brand.css", game, got)?;
            let text = std::fs::read_to_string(&css).unwrap_or_default();
            let url = regex::Regex::new(r#"url\(\s*["']?([^"')]+)["']?\s*\)"#).unwrap();
            for c in url.captures_iter(&text) {
                let u = &c[1];
                if u.contains("://") || u.starts_with("data:") {
                    continue;
                }
                let from = here.join("brand").join(u);
                if from.is_file() {
                    let rel = Path::new("brand").join(u).to_string_lossy().to_string();
                    put(&from, &rel, game, got)?;
                }
            }
            break;
        }
        d = here.parent();
    }
    // generated sounds (sao(), make()): looked up at play in assets/made.jsonl beside the edit; the rows whose prompt
    // the bundle carries come along with their files
    let index = std::fs::read_to_string(dir.join("assets/made.jsonl")).unwrap_or_default();
    let mut kept = String::new();
    for line in index.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let Some(prompt) = v.pointer("/origin/prompt").and_then(Value::as_str) else { continue };
        let quoted = serde_json::to_string(prompt).unwrap_or_default();
        if !bundle.contains(&quoted) && !bundle.contains(prompt) {
            continue;
        }
        let (Some(hash), Some(ext)) = (v.get("hash").and_then(Value::as_str), v.get("ext").and_then(Value::as_str)) else { continue };
        let f = dir.join("assets").join(format!("{hash}.{ext}"));
        if !f.is_file() {
            continue;
        }
        if ext == "wav" && flac(&f, hash, game, got).is_some() {
            let mut row = v.clone();
            row["ext"] = json!("flac");
            kept.push_str(&row.to_string());
        } else {
            put(&f, &format!("assets/{hash}.{ext}"), game, got)?;
            kept.push_str(line);
        }
        kept.push('\n');
    }
    if !kept.is_empty() {
        std::fs::write(game.join("assets/made.jsonl"), kept).map_err(|e| e.to_string())?;
        got.files.push(("assets/made.jsonl".into(), dir.join("assets/made.jsonl")));
    }
    Ok(())
}

/// A generated WAV packaged as FLAC (`/usr/bin/afconvert`), about half the bytes: kept only when the soft decoder (the
/// one the player plays sounds with) reads back exactly the WAV's samples, so play and the tap sound the same. None, and
/// the WAV is copied as it is, when it isn't (a float WAV, say) or the encoder isn't there.
fn flac(src: &Path, hash: &str, game: &Path, got: &mut Gathered) -> Option<String> {
    let rel = format!("assets/{hash}.flac");
    if got.has(&rel) {
        return Some(rel);
    }
    let to = game.join(&rel);
    std::fs::create_dir_all(to.parent()?).ok()?;
    let made = std::process::Command::new("/usr/bin/afconvert").args(["-d", "flac", "-f", "flac"]).arg(src).arg(&to).output();
    let soft = raea_media::backend("soft").ok()?;
    let same = made.is_ok_and(|o| o.status.success())
        && matches!((soft.decode_audio(src), soft.decode_audio(&to)), (Ok(a), Ok(b)) if a.rate == b.rate && a.channels == b.channels);
    if !same {
        let _ = std::fs::remove_file(&to);
        return None;
    }
    got.files.push((rel.clone(), src.to_path_buf()));
    got.sounds += 1;
    Some(rel)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn info_plist(name: &str, exec: &str, icon: bool) -> String {
    let id = format!("ai.raea.game.{}", exec.to_ascii_lowercase());
    let icon = if icon { "\t<key>CFBundleIconFile</key>\n\t<string>AppIcon</string>\n" } else { "" };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>{name}</string>
	<key>CFBundleDisplayName</key>
	<string>{name}</string>
	<key>CFBundleExecutable</key>
	<string>{exec}</string>
	<key>CFBundleIdentifier</key>
	<string>{id}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>1.0</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
{icon}	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.games</string>
	<key>GCSupportsGameMode</key>
	<true/>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSSupportsAutomaticGraphicsSwitching</key>
	<true/>
	<key>NSPrincipalClass</key>
	<string>NSApplication</string>
</dict>
</plist>
"#,
        name = xml_escape(name),
        exec = xml_escape(exec),
        id = xml_escape(&id),
    )
}

/// A PNG made into an .icns (sips and iconutil, both in macOS).
fn make_icns(png: &Path, out: &Path) -> R<PathBuf> {
    let set = std::env::temp_dir().join(format!("raea-icon-{}.iconset", std::process::id()));
    let _ = std::fs::remove_dir_all(&set);
    std::fs::create_dir_all(&set).map_err(|e| e.to_string())?;
    for s in [16u32, 32, 128, 256, 512] {
        for (k, suffix) in [(1u32, ""), (2, "@2x")] {
            let px = s * k;
            let f = set.join(format!("icon_{s}x{s}{suffix}.png"));
            let o = std::process::Command::new("sips").args(["-z", &px.to_string(), &px.to_string(), "-s", "format", "png"]).arg(png).arg("--out").arg(&f).output().map_err(|e| format!("sips: {e}"))?;
            if !o.status.success() {
                return Err(format!("sips couldn't size {}: {}", png.display(), String::from_utf8_lossy(&o.stderr).trim()));
            }
        }
    }
    let o = std::process::Command::new("iconutil").args(["-c", "icns"]).arg(&set).arg("-o").arg(out).output().map_err(|e| format!("iconutil: {e}"))?;
    let _ = std::fs::remove_dir_all(&set);
    if !o.status.success() {
        return Err(format!("iconutil: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(out.to_path_buf())
}

// ---------------------------------------------------------------------------------------------------------------------
// The player

struct Game {
    dir: PathBuf,
    name: String,
    w: u32,
    h: u32,
    fps: f64,
    responsive: bool,
    has_3d: bool,
    sound: bool,
    master: Option<(f64, f64)>,
    keys: String,
}

impl Game {
    fn load(dir: &Path) -> R<Game> {
        let text = std::fs::read_to_string(dir.join(MANIFEST)).map_err(|e| format!("{}: {e} (not an exported game's folder)", dir.join(MANIFEST).display()))?;
        let m: Value = serde_json::from_str(&text).map_err(|e| format!("{MANIFEST}: {e}"))?;
        let n = |k: &str, d: f64| m.get(k).and_then(Value::as_f64).unwrap_or(d);
        Ok(Game {
            dir: dir.to_path_buf(),
            name: m.get("name").and_then(Value::as_str).unwrap_or("game").to_string(),
            w: n("width", 1080.0) as u32,
            h: n("height", 1920.0) as u32,
            fps: n("fps", 30.0),
            responsive: m.get("responsive").and_then(Value::as_bool).unwrap_or(false),
            has_3d: m.get("has_3d").and_then(Value::as_bool).unwrap_or(false),
            sound: m.get("sound").and_then(Value::as_bool).unwrap_or(false),
            master: m.get("master").and_then(|v| Some((v.get("gain_db")?.as_f64()?, v.get("ceiling_dbtp")?.as_f64()?))),
            keys: m.get("selftest_keys").and_then(Value::as_str).unwrap_or(DEFAULT_KEYS).to_string(),
        })
    }

    fn bundle(&self) -> PathBuf {
        self.dir.join("bundle.js")
    }

    fn open(&self, stage: (u32, u32)) -> R<Live> {
        let mut live = Live::open_bundle(&self.bundle(), &self.dir, stage, self.responsive)?;
        if let Ok(css) = std::fs::read_to_string(self.dir.join(FONTS)) {
            live.add_packaged_fonts_css(&fonts_in(&css, &self.dir));
        }
        live.set_game_gain(self.master);
        Ok(live)
    }
}

/// Where the package's game is: --game DIR, else beside the player (an app's Resources/game, or a folder's game/).
fn find_game(args: &[String]) -> R<PathBuf> {
    if let Some(d) = flag(args, "--game") {
        return Ok(PathBuf::from(d));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let here = exe.parent().unwrap_or(Path::new("."));
    for c in [here.join("../Resources/game"), here.join("game")] {
        if c.join(MANIFEST).exists() {
            return Ok(c.canonicalize().unwrap_or(c));
        }
    }
    Err(format!("no game beside {} (an exported game's player looks in ../Resources/game or game/; --game DIR names one)", exe.display()))
}

static PANICS: AtomicU64 = AtomicU64::new(0);

/// `raea-game ...`: the player's main.
pub fn player_main(args: &[String]) -> i32 {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        PANICS.fetch_add(1, Ordering::Relaxed);
        prev(info);
    }));
    crate::native::register();
    let r = (|| -> R<String> {
        let game = Game::load(&find_game(args)?)?;
        if let Some(secs) = flag(args, "--selftest") {
            let secs: f64 = secs.trim_end_matches('s').parse().map_err(|_| "--selftest takes seconds, e.g. 5s")?;
            return selftest(&game, secs, args);
        }
        if args.iter().any(|a| a == "--info") {
            return Ok(std::fs::read_to_string(game.dir.join(MANIFEST)).unwrap_or_default());
        }
        windowed(game)
    })();
    match r {
        Ok(msg) => {
            if !msg.is_empty() {
                println!("{msg}");
            }
            0
        }
        Err(e) => {
            eprintln!("raea-game: {e}");
            println!("FAIL  {e}");
            1
        }
    }
}

/// The key sets the self-test cycles through: sets split by commas, keys in a set by "+", an empty set for none.
fn key_sets(spec: &str) -> Vec<Vec<String>> {
    spec.split(',').map(|set| set.split('+').filter(|k| !k.is_empty()).map(str::to_string).collect()).collect()
}

/// Peak and RMS (dBFS) of a 32-bit float or 16-bit WAV, and its seconds.
fn wav_levels(path: &Path) -> Option<(f64, f64, f64)> {
    let b = std::fs::read(path).ok()?;
    if b.len() < 44 || &b[0..4] != b"RIFF" {
        return None;
    }
    let (mut fmt, mut ch, mut rate, mut bits) = (0u16, 1u16, 48000u32, 32u16);
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let body = &b[i + 8..(i + 8 + len).min(b.len())];
        if id == b"fmt " && body.len() >= 16 {
            fmt = u16::from_le_bytes([body[0], body[1]]);
            ch = u16::from_le_bytes([body[2], body[3]]).max(1);
            rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            let samples: Vec<f64> = match (fmt, bits) {
                (3, 32) => body.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect(),
                (1, 16) => body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f64 / 32768.0).collect(),
                _ => return None,
            };
            let peak = samples.iter().fold(0.0f64, |m, s| m.max(s.abs()));
            let rms = (samples.iter().map(|s| s * s).sum::<f64>() / samples.len().max(1) as f64).sqrt();
            let db = |x: f64| if x > 0.0 { (20.0 * x.log10() * 10.0).round() / 10.0 } else { -200.0 };
            return Some((db(peak), db(rms), samples.len() as f64 / ch as f64 / rate.max(1) as f64));
        }
        i += 8 + len + (len & 1);
    }
    None
}

/// Offscreen play with scripted input: what the package does, measured, with no window and no sound heard.
fn selftest(game: &Game, secs: f64, args: &[String]) -> R<String> {
    // the mixer's output kept (its tap) and silenced: the sound is mixed as it would be heard, and nobody hears it
    let tap = std::env::temp_dir().join(format!("raea-game-tap-{}.wav", std::process::id()));
    unsafe {
        std::env::set_var("RAEA_AUDIO_MUTE", "1");
        std::env::set_var("RAEA_AUDIO_TAP", &tap);
    }
    let (w, h) = (game.w, game.h);
    let snaps: Vec<(f64, String)> = args.windows(3).filter(|x| x[0] == "--snapshot").filter_map(|x| Some((x[1].parse().ok()?, x[2].clone()))).collect();
    let input_log = flag(args, "--input-log");
    let sets = key_sets(&flag(args, "--keys").unwrap_or_else(|| game.keys.clone()));
    // --no-pointer: keys alone (the pointer moving every frame is what a mouse-driven game needs; a keys-only game too)
    let no_pointer = args.iter().any(|a| a == "--no-pointer");
    let t_open = Instant::now();
    let stage = raea::quads::stage_offscreen(w, h)?;
    let mut live = game.open((w, h))?;
    if game.has_3d {
        live.scene_3d();
        live.wait_3d(120.0)?;
    }
    live.open_mixer();
    let open_ms = t_open.elapsed().as_secs_f64() * 1e3;
    let fps = live.fps;
    let start = Instant::now();
    // a hand on the keys: a change every 50-250 ms, cycling the sets (as play --input), stamped when it's made
    let shared = Arc::new(std::sync::Mutex::new((Vec::<String>::new(), Instant::now(), 0u64)));
    {
        let s2 = shared.clone();
        let until = start + Duration::from_secs_f64(secs);
        std::thread::spawn(move || {
            let mut x: u64 = 0x2545f4914f6cdd1d;
            let mut k = 0u64;
            while Instant::now() < until {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                std::thread::sleep(Duration::from_millis(50 + x % 200));
                k += 1;
                let keys = sets[(k % sets.len().max(1) as u64) as usize].clone();
                *s2.lock().unwrap() = (keys, Instant::now(), k);
            }
        });
    }
    let (mut seen, mut waiting) = (0u64, None::<Instant>);
    let mut latency: Vec<f64> = Vec::new();
    let mut logged = serde_json::Map::new();
    let (mut presented, mut stepped, mut keyed, mut pressed) = (0u64, 0u64, 0u64, 0u64);
    let mut gaps: Vec<f64> = Vec::new();
    let mut work: Vec<f64> = Vec::new();
    let mut last: Option<Instant> = None;
    let mut snapped = 0usize;
    let mut f: i64 = -1;
    let mut keys: Vec<String> = Vec::new();
    while start.elapsed().as_secs_f64() < secs {
        let t0 = Instant::now();
        let (k, at, seq) = shared.lock().unwrap().clone();
        keys = k;
        if seq != seen {
            seen = seq;
            waiting = Some(at);
        }
        f += 1;
        // a pointer too: across the page and back over four seconds, pressed for half of every second
        let ph = (f as f64 / fps / 4.0).fract();
        let px = (if ph < 0.5 { ph * 2.0 } else { 2.0 - ph * 2.0 }) * live.w as f64;
        let py = (0.5 + 0.3 * (f as f64 / fps * std::f64::consts::PI).sin()) * live.h as f64;
        let down = (f as f64 / fps).fract() < 0.5;
        let pointer = (!no_pointer).then_some((px, py, down));
        let lag = start.elapsed().as_secs_f64() - f as f64 / fps;
        live.step_at(f, &keys, pointer, lag)?;
        stepped += 1;
        if !keys.is_empty() {
            keyed += 1;
        }
        if pointer.is_some_and(|p| p.2) {
            pressed += 1;
        }
        if input_log.is_some() {
            let mut e = json!({"keys": keys, "live": true});
            if let Some((x, y, d)) = pointer {
                e["pointer"] = json!({"x": x, "y": y, "down": d});
            }
            logged.insert(f.to_string(), e);
        }
        live.present(&stage, true)?;
        presented += 1;
        for (s, out) in snaps.iter().filter(|(s, _)| (*s * fps).round() as i64 == f) {
            live.save_snapshot(Path::new(out))?;
            eprintln!("snapshot at {s} s (frame {f}) -> {out}");
            snapped += 1;
        }
        let now = Instant::now();
        work.push((now - t0).as_secs_f64() * 1e3);
        if let Some(l) = last {
            gaps.push((now - l).as_secs_f64() * 1e3);
        }
        last = Some(now);
        if let Some(at) = waiting.take() {
            latency.push((now - at).as_secs_f64() * 1e3);
        }
        let next = start + Duration::from_secs_f64((f + 1) as f64 / fps);
        if let Some(d) = next.checked_duration_since(Instant::now()) {
            std::thread::sleep(d);
        }
    }
    let _ = keys;
    if let Some(p) = &input_log {
        std::fs::write(p, Value::Object(logged).to_string()).map_err(|e| format!("{p}: {e}"))?;
    }
    let (pw, ph) = (live.w, live.h);
    let drift: Vec<f64> = live.drift_ms().to_vec();
    // the mixer's tap is written as the mixer goes
    drop(live);
    let levels = wav_levels(&tap);
    let _ = std::fs::remove_file(&tap);
    let budget = 1000.0 / fps.max(1.0);
    let missed = gaps.iter().filter(|g| **g > budget * 1.5).count();
    let pct = |v: &[f64], p: f64| {
        if v.is_empty() {
            return 0.0;
        }
        let mut s = v.to_vec();
        s.sort_by(f64::total_cmp);
        (s[((s.len() - 1) as f64 * p).round() as usize] * 100.0).round() / 100.0
    };
    let expected = (secs * fps).floor() as u64;
    let panics = PANICS.load(Ordering::Relaxed);
    let heard = levels.is_some_and(|(peak, _, _)| peak > -120.0);
    let mut why: Vec<String> = Vec::new();
    if presented < expected * 9 / 10 {
        why.push(format!("presented {presented} of {expected} frames"));
    }
    if missed as u64 > expected / 20 + 1 {
        why.push(format!("{missed} frames missed"));
    }
    if latency.is_empty() {
        why.push("no input change reached a frame".into());
    }
    if game.sound && !heard {
        why.push("the game has sound and none was mixed".into());
    }
    if snapped < snaps.len() {
        why.push(format!("{} of {} snapshots taken", snapped, snaps.len()));
    }
    if panics > 0 {
        why.push(format!("{panics} panics"));
    }
    let report = json!({
        "game": game.name,
        "seconds": secs,
        "fps": fps,
        "page": [pw, ph],
        "open_ms": (open_ms * 10.0).round() / 10.0,
        "frames": {"expected": expected, "stepped": stepped, "presented": presented, "missed": missed},
        "work_ms": {"p50": pct(&work, 0.5), "p99": pct(&work, 0.99), "max": pct(&work, 1.0)},
        "gap_ms": {"p50": pct(&gaps, 0.5), "p99": pct(&gaps, 0.99), "max": pct(&gaps, 1.0)},
        "input": {"changes_reached": latency.len(), "frames_with_keys": keyed, "frames_pointer_down": pressed, "to_present_ms": {"p50": pct(&latency, 0.5), "p99": pct(&latency, 0.99)}},
        "sound": {"in_game": game.sound, "mixed": heard, "peak_dbfs": levels.map(|l| l.0), "rms_dbfs": levels.map(|l| l.1), "seconds": levels.map(|l| (l.2 * 10.0).round() / 10.0), "master": game.master.map(|(g, c)| json!({"gain_db": g, "ceiling_dbtp": c})), "drift_ms_max": drift.iter().fold(0.0f64, |m, d| m.max(d.abs()))},
        "has_3d": game.has_3d,
        "snapshots": snapped,
        "panics": panics,
        "env": {"home": std::env::var("HOME").unwrap_or_default(), "raea_vars_given": std::env::vars().filter(|(k, _)| k.starts_with("RAEA_") && k != "RAEA_AUDIO_MUTE" && k != "RAEA_AUDIO_TAP").count()},
    });
    if let Some(p) = flag(args, "--json") {
        std::fs::write(&p, serde_json::to_string_pretty(&report).unwrap()).map_err(|e| format!("{p}: {e}"))?;
    }
    let verdict = if why.is_empty() { "PASS".to_string() } else { format!("FAIL  {}", why.join("; ")) };
    Ok(format!("{}\n{verdict}", serde_json::to_string_pretty(&report).unwrap()))
}

// ---------------------------------------------------------------------------------------------------------------------
// The window

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

struct Player {
    game: Game,
    window: Option<Arc<Window>>,
    stage: Option<raea::quads::Stage>,
    live: Option<Live>,
    keys: Vec<String>,
    pointer: Option<(f64, f64, bool)>,
    mods: ModifiersState,
    start: Option<Instant>,
    frame: i64,
    error: Option<String>,
    /// when the window was asked for, and when its first paint (the splash) went up (lightspeed F5)
    launched: Instant,
}

fn key_name(k: &Key) -> Option<String> {
    Some(match k {
        Key::Named(NamedKey::ArrowLeft) => "ArrowLeft".into(),
        Key::Named(NamedKey::ArrowRight) => "ArrowRight".into(),
        Key::Named(NamedKey::ArrowUp) => "ArrowUp".into(),
        Key::Named(NamedKey::ArrowDown) => "ArrowDown".into(),
        Key::Named(NamedKey::Space) => " ".into(),
        Key::Named(NamedKey::Enter) => "Enter".into(),
        Key::Named(NamedKey::Escape) => "Escape".into(),
        Key::Named(NamedKey::Shift) => "Shift".into(),
        Key::Named(NamedKey::Tab) => "Tab".into(),
        Key::Named(NamedKey::Backspace) => "Backspace".into(),
        Key::Character(c) => c.to_string(),
        _ => return None,
    })
}

impl Player {
    fn frame(&mut self) -> R<()> {
        // the game opened behind the splash, on the first redraw: a 3D game's first launch compiles its pipelines here
        // (seconds while the system's shader cache is cold). Its clock starts at the next frame, from nothing: keys and
        // pointer from the splash's time are let go, so frame 0 is a render's frame 0
        if self.live.is_none() {
            if self.stage.is_none() {
                let Some(window) = self.window.clone() else { return Ok(()) };
                let px = window.inner_size();
                self.stage = Some(raea::quads::stage(window, px.width, px.height, true)?);
            }
            let Some(stage) = self.stage.as_ref() else { return Ok(()) };
            let (w, h) = (stage.w, stage.h);
            let mut live = self.game.open((w, h))?;
            if self.game.has_3d {
                live.scene_3d();
            }
            live.open_mixer();
            self.live = Some(live);
            (self.start, self.frame) = (None, -1);
            self.keys.clear();
            self.pointer = None;
            if probing() {
                eprintln!("raea game: playable {:.0} ms after launch", self.launched.elapsed().as_secs_f64() * 1e3);
            }
            return Ok(());
        }
        let (Some(live), Some(stage)) = (self.live.as_mut(), self.stage.as_ref()) else { return Ok(()) };
        if self.start.is_none() {
            // input winit queued while the open blocked is delivered before this frame: let go of it too
            self.keys.clear();
            self.pointer = None;
        }
        let start = *self.start.get_or_insert_with(Instant::now);
        let elapsed = start.elapsed().as_secs_f64();
        // every frame due by now, each with this moment's input (a few at most when behind; past that the clock jumps)
        let due = (elapsed * live.fps).floor() as i64;
        let mut n = 0;
        while self.frame < due && n < 4 {
            self.frame += 1;
            let lag = elapsed - self.frame as f64 / live.fps;
            live.step_at(self.frame, &self.keys, self.pointer, lag)?;
            n += 1;
        }
        if self.frame < due {
            self.frame = due;
        }
        // presented every refresh (vsync paces the loop), a new frame or not
        let _ = n;
        live.present(stage, true)?;
        Ok(())
    }

    fn toggle_fullscreen(&self) {
        if let Some(w) = &self.window {
            w.set_fullscreen(if w.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) });
        }
    }
}

impl ApplicationHandler for Player {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        // the composition's shape, as large as fits in most of the screen
        let (cw, ch) = (self.game.w as f64, self.game.h as f64);
        let (sw, sh) = el.primary_monitor().map(|m| { let s = m.size().to_logical::<f64>(m.scale_factor()); (s.width, s.height) }).unwrap_or((1440.0, 900.0));
        let k = (sw * 0.85 / cw).min(sh * 0.85 / ch).min(1.0);
        let attrs = Window::default_attributes().with_title(&self.game.name).with_inner_size(winit::dpi::LogicalSize::new((cw * k).round(), (ch * k).round()));
        // RAEA_PLAYER_PROBE=1: the launch measured with no window on anyone's screen (hidden, never active), its first
        // paint and playable times printed, and the player gone 2 s after it's playable
        let attrs = if probing() { attrs.with_visible(false).with_active(false) } else { attrs };
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.error = Some(format!("window: {e}"));
                el.exit();
                return;
            }
        };
        // the first paint at once, with nothing of ours compiled (lightspeed F5): the window's own background, dark, with
        // the game's name as its title; the stage (whose pipelines compile on a first launch) and the game open on the
        // first redraw, so the window never sits empty however long that takes
        raea::quads::window_background(&*window, [0.05, 0.05, 0.063]);
        if probing() {
            eprintln!("raea game: first paint {:.0} ms after launch", self.launched.elapsed().as_secs_f64() * 1e3);
        }
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(px) => {
                if let Some(s) = self.stage.as_mut() {
                    let _ = s.resize(px.width, px.height);
                }
                if let Some(l) = self.live.as_mut() {
                    l.resize(px.width, px.height);
                }
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::KeyboardInput { event, .. } => {
                let cmd = self.mods.super_key();
                if event.state == ElementState::Pressed && !event.repeat {
                    let ch = match &event.logical_key {
                        Key::Character(c) => c.to_lowercase(),
                        _ => String::new(),
                    };
                    if cmd && (ch == "q" || ch == "w") {
                        el.exit();
                        return;
                    }
                    if matches!(event.logical_key, Key::Named(NamedKey::F11)) || (cmd && ch == "f") {
                        self.toggle_fullscreen();
                        return;
                    }
                }
                // a shortcut held with Cmd isn't the game's
                if cmd {
                    return;
                }
                if let Some(k) = key_name(&event.logical_key) {
                    match event.state {
                        ElementState::Pressed if !self.keys.contains(&k) => self.keys.push(k),
                        ElementState::Released => self.keys.retain(|x| *x != k),
                        _ => {}
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (Some(w), Some(l)) = (self.window.as_ref(), self.live.as_ref()) {
                    // the page is shown whole in the window, centred (object-fit contain)
                    let s = w.inner_size();
                    let k = (s.width as f64 / l.w as f64).min(s.height as f64 / l.h as f64);
                    let (ox, oy) = ((s.width as f64 - l.w as f64 * k) / 2.0, (s.height as f64 - l.h as f64 * k) / 2.0);
                    let down = self.pointer.is_some_and(|p| p.2);
                    self.pointer = Some(((position.x - ox) / k, (position.y - oy) / k, down));
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                if let Some(p) = self.pointer.as_mut() {
                    p.2 = state == ElementState::Pressed;
                }
            }
            WindowEvent::RedrawRequested => {
                if probing() && self.start.is_some_and(|s| s.elapsed().as_secs_f64() > 2.0) {
                    el.exit();
                    return;
                }
                if let Err(e) = self.frame() {
                    self.error = Some(e);
                    el.exit();
                }
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn probing() -> bool {
    std::env::var("RAEA_PLAYER_PROBE").as_deref() == Ok("1")
}

fn windowed(game: Game) -> R<String> {
    let el = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    el.set_control_flow(ControlFlow::Poll);
    let mut p = Player { game, window: None, stage: None, live: None, keys: Vec::new(), pointer: None, mods: ModifiersState::empty(), start: None, frame: -1, error: None, launched: Instant::now() };
    el.run_app(&mut p).map_err(|e| format!("event loop: {e}"))?;
    if let Some(l) = p.live.as_mut() {
        l.silence();
    }
    match p.error {
        Some(e) => Err(e),
        None => Ok(String::new()),
    }
}

#[cfg(test)]
mod tests {
    /// A sheet written out to files and read back is the sheet, byte for byte, and a face under two families is one file.
    #[test]
    fn packaged_fonts_round_trip() {
        use base64::Engine as _;
        let b = |s: &[u8]| base64::engine::general_purpose::STANDARD.encode(s);
        let (a, c) = (b(b"wOF2 face a"), b(b"wOF2 face c"));
        let css = format!(
            "@font-face {{ font-family: \"Inter\"; src: url(data:font/woff2;base64,{a}) format(\"woff2\"); }}\n@font-face {{ font-family: \"Roboto\"; src: url(data:font/woff2;base64,{a}) format(\"woff2\"); }}\n@font-face {{ font-family: \"X\"; src: url(data:font/ttf;base64,{c}); }}\n@font-face {{ font-family: \"Y\"; src: url(\"data:font/woff2;base64,{a}\") format(\"woff2\"); }}\n@font-face {{ src: url(\"https://example.com/x.woff2\"); }}\n"
        );
        let dir = std::env::temp_dir().join(format!("raea-fonts-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = super::fonts_out(&css, &dir).unwrap();
        assert!(!out.contains("base64"), "{out}");
        assert_eq!(std::fs::read_dir(dir.join("fonts")).unwrap().count(), 2);
        assert_eq!(super::fonts_in(&out, &dir), css);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
