use super::*;

/// The key sets the self-test cycles through when the export names none: arrows, space, a run and jump, and nothing.
pub(crate) const DEFAULT_KEYS: &str = "ArrowLeft,ArrowRight,ArrowUp,ArrowDown, ,ArrowRight+ArrowUp,";

/// The player's dSYM (profile.game's packed split debuginfo: the shipped player has no symbols) kept by its UUID under
/// ~/Library/Caches/raea/dsym, so a player's panic.txt or crash.txt (which carry the UUID and load address) can be
/// symbolicated later: `atos -o ~/Library/Caches/raea/dsym/UUID/raea-game.dSYM -l LOAD ADDR...`. Never shipped.
pub(crate) fn keep_dsym(player: &Path) {
    let dsym = player.with_extension("dSYM");
    if !dsym.is_dir() {
        return;
    }
    let Ok(o) = std::process::Command::new("dwarfdump").arg("--uuid").arg(&dsym).output() else { return };
    let text = String::from_utf8_lossy(&o.stdout);
    let Some(uuid) = text.split_whitespace().nth(1).filter(|u| u.len() == 36) else { return };
    let Some(home) = raea_io_home::user_home() else { return };
    let to = Path::new(&home).join("Library/Caches/raea/dsym").join(uuid).join("raea-game.dSYM");
    if to.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(to.parent().unwrap());
    if std::process::Command::new("ditto").arg(&dsym).arg(&to).status().map(|s| s.success()).unwrap_or(false) {
        eprintln!("raea export-game: the player's symbols kept at {}", to.display());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Export

pub const USAGE: &str = "raea export-game EDIT.tsx -o OUT [--target mac-app|mac-cli] [--name NAME] [--icon PNG] [--keys SETS] [--player BIN] [--no-strip]";

/// `raea export-game EDIT.tsx -o OUT ...`
pub fn export(args: &[String]) -> R<String> {
    // its targets are a Mac's, signed with codesign, and raea's Windows and Linux releases carry no game player: there
    // it said "no player ... (build it: cargo build ...)", which a person can't act on
    if !cfg!(target_os = "macos") {
        return Err("raea export-game makes a Mac game (an .app or a command) and runs on a Mac; Windows and Linux games aren't there yet (sdk/react/docs/game-export.md)".into());
    }
    let usage = USAGE;
    let tsx = PathBuf::from(args.get(1).filter(|a| !a.starts_with("--")).ok_or(usage)?);
    let tsx = dunce::canonicalize(&tsx).map_err(|e| format!("{}: {e}", tsx.display()))?;
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
        return Err(format!("no game player at {}: this raea ships without raea-game, which a game's export is made from; `raea bug \"export-game: no player\"` asks for it (docs/nobuild.feature NB3a)", player.display()));
    }
    // the release build's player is ~128 MB stripped, the game profile's ~74 MB (fat LTO, one codegen unit; lightspeed
    // R7): say so when the bigger one ships because it's the newer, to a checkout's build (an installed raea has only its own)
    if !player.to_string_lossy().contains("/game/") && player.to_string_lossy().contains("/target/") && flag(args, "--player").is_none() {
        let mb = std::fs::metadata(&player).map(|m| m.len() as f64 / 1048576.0).unwrap_or(0.0);
        eprintln!("raea export-game: shipping the release build's player ({mb:.0} MB before stripping): `cargo build --profile game -p raea-app-game --bin raea-game --features player` makes the ~74 MB one, which ships when it's the newer");
    }
    keep_dsym(&player);

    // 1. the bundle, for live play's runtime, with imported media copied in and named relatively
    let mut opts: Vec<String> = Vec::new();
    for (f, file) in [("--words", "words.json"), ("--perception", "perception.json")] {
        let p = dir.join(file);
        if p.exists() && (file != "words.json" || raea_runtime_react::scene::transcript_file(&p)) {
            opts.push(f.into());
            opts.push(p.display().to_string());
        }
    }
    // on live play's runtime, or React for an edit drawing R3F (raea_runtime_live::needs_react)
    if !raea_runtime_live::needs_react(&dir) {
        opts.push("--lite".into());
    }
    let js = game.join("bundle.js");
    raea_runtime_react::scene::bundle_composition(&tsx, &js, &raea_runtime_react::scene::comp_opts(&opts)?, Some(&game))?;

    // 2. every file it names, gathered (paths outside the game's folder rewritten into the package)
    let mut bundle = std::fs::read_to_string(&js).map_err(|e| e.to_string())?;
    let mut got = Gathered::default();
    gather(&mut bundle, &dir, &game, &mut got)?;
    std::fs::write(&js, &bundle).map_err(|e| e.to_string())?;

    // an app screen (<Screen app>) draws on the player's own Blitz page (raea-runtime-appscreen): every player has one

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
    // 3D (raea/3d): live play draws it from each frame's own state (Live, scene3d::LiveScene)
    let has_3d = bundle.contains("data-3d");
    // sound: files that sound, or notes and instruments because the game declares raea's audio plugin (package.json
    // raea.plugins "audio", docs/sdk.feature S6b; the bundle carries the SDK's sound code whether the game sounds or
    // not). A game that declares no plugins is read as before, by its sources' imports, until M5's codemod declares them
    let plays = sources_use(&dir, &["raea/tone", "raea/music", "raea/seedling", "raea/tone", "raea/music", "raea/seedling", "raea/audio", "@raea/audio", "motion/src/tone", "motion/src/music", "motion/src/seedling"]);
    let sound = got.sounds > 0
        || match declared_plugins(&dir) {
            Some((_, names)) if names.iter().any(|n| n == "audio") => true,
            Some((pkg, _)) if plays => {
                return Err(format!("this game plays notes or instruments (@raea/audio), and {} declares plugins but not audio, so it would export silent: add \"audio\": {{ \"from\": \"@raea/audio\" }} to its raea.plugins", pkg.display()));
            }
            Some(_) => false,
            None => plays,
        };
    // the game's master (its render's gain and ceiling): stepping its scripted run through every frame, here and once
    let master = if sound { raea_runtime_export::master_gain(&js, &game)? } else { None };
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

/// The plugins a game's project declares (docs/sdk.feature S6b): the nearest package.json at or above its folder that
/// declares any, under raea.plugins or its first name raea.bevy.plugins, with their names. None when none does.
pub(crate) fn declared_plugins(dir: &Path) -> Option<(PathBuf, Vec<String>)> {
    for d in dir.ancestors() {
        let pkg = d.join("package.json");
        let Some(v) = std::fs::read_to_string(&pkg).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { continue };
        let mut names: Vec<String> = [&v["raea"]["plugins"], &v["raea"]["bevy"]["plugins"]].into_iter().filter_map(|p| p.as_object()).flat_map(|o| o.keys().cloned()).collect();
        if !names.is_empty() {
            names.sort();
            names.dedup();
            return Some((pkg, names));
        }
    }
    None
}

/// Whether any of the game's own sources (its folder, two levels down) says one of `needles`.
pub(crate) fn sources_use(dir: &Path, needles: &[&str]) -> bool {
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

/// Whether one of the game's own sources imports `file` (`from "./components/card"`): then it's code the bundle holds,
/// not an app screen's module, which is only named (`<Screen app="chat.tsx">`) and never imported.
pub(crate) fn imported(dir: &Path, file: &Path) -> bool {
    let Some(stem) = file.file_stem().and_then(|s| s.to_str()) else { return false };
    let re = regex::Regex::new(&format!(r#"(?:from|import)\s*\(?\s*["'][^"']*\b{}(?:\.[jt]sx?)?["']"#, regex::escape(stem))).unwrap();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            let name = e.file_name();
            if name.to_string_lossy().starts_with('.') || name == "node_modules" {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p != file && matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx" | "js" | "jsx")) && std::fs::read_to_string(&p).is_ok_and(|t| re.is_match(&t)) {
                return true;
            }
        }
    }
    false
}

pub(crate) fn default_name(tsx: &Path) -> String {
    let stem = tsx.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let base = if stem == "edit" { tsx.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or(stem) } else { stem };
    base.split(['-', '_', ' ']).filter(|w| !w.is_empty()).map(|w| {
        let mut c = w.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }).collect::<Vec<_>>().join(" ")
}

pub(crate) fn slug(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "game".into() } else { s }
}

pub(crate) fn dir_size(d: &Path) -> u64 {
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
