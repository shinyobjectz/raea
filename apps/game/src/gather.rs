use super::*;

#[derive(Default)]
pub(crate) struct Gathered {
    /// (path in the package, source)
    pub(crate) files: Vec<(String, PathBuf)>,
    /// literals in the bundle pointed into the package
    pub(crate) rewritten: usize,
    pub(crate) sounds: usize,
}

impl Gathered {
    pub(crate) fn has(&self, rel: &str) -> bool {
        self.files.iter().any(|(r, _)| r == rel)
    }
}

pub(crate) const SOURCE_EXTS: [&str; 7] = ["ts", "tsx", "js", "jsx", "mjs", "rrl", "moves"];
pub(crate) const SOUND_EXTS: [&str; 7] = ["wav", "mp3", "m4a", "aac", "flac", "ogg", "aif"];

/// One file into the package at `rel` (once), and what a glTF names beside it.
pub(crate) fn put(src: &Path, rel: &str, game: &Path, got: &mut Gathered) -> R<()> {
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
pub(crate) fn gltf_uris(file: &Path) -> Vec<String> {
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
                out.push(raea_io_net::percent_decode_str(u));
            }
        }
    }
    out
}

/// Every string in the bundle that names a file: under the game's folder it goes into the package at the same place;
/// elsewhere (an absolute path) into media/_ext/ with the string rewritten to it. Then the brand's CSS and the fonts it
/// names, and the generated sounds whose prompts the bundle carries.
pub(crate) fn gather(bundle: &mut String, dir: &Path, game: &Path, got: &mut Gathered) -> R<()> {
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
        let decoded = raea_io_net::percent_decode_str(s);
        let mut cands: Vec<PathBuf> = Vec::new();
        if let Some(rest) = decoded.strip_prefix("/media/") {
            cands.push(raea_runtime_react::scene::media_path(dir, &format!("/media/{rest}")));
        }
        if decoded.starts_with('/') {
            cands.push(PathBuf::from(&decoded));
        }
        cands.push(dir.join(decoded.trim_start_matches("./").trim_start_matches('/')));
        cands.push(dir.join("media").join(decoded.trim_start_matches("./").trim_start_matches('/')));
        let Some(file) = cands.into_iter().find(|p| p.is_file()) else { continue };
        let ext = file.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if SOURCE_EXTS.contains(&ext.as_str()) || file.file_name().is_some_and(|n| n == MANIFEST) {
            // an app screen's module (`<Screen app="chat.tsx">`, a component on react-dom): bundled now with its npm
            // packages beside where it was, as `chat.tsx.surface.js` and its CSS `chat.tsx.surface.css`, which the
            // player opens (it bundles nothing; raea-runtime-appscreen code_of)
            if matches!(ext.as_str(), "tsx" | "jsx") && std::fs::read_to_string(&file).is_ok_and(|t| t.contains("export default") && !t.contains("registerRoot(")) && !imported(dir, &file) {
                if let Some(rel) = inside(&file) {
                    let (js_rel, css_rel) = (format!("{rel}{}", raea_runtime_page::screen::BUNDLED), format!("{rel}{}", raea_runtime_page::screen::BUNDLED_CSS));
                    if !got.has(&js_rel) {
                        let (js, css) = raea_runtime_page::screen::build_app(&file)?;
                        for (r, text) in [(&js_rel, js), (&css_rel, css)] {
                            let out = game.join(r);
                            if let Some(d) = out.parent() {
                                std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
                            }
                            std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
                            got.files.push((r.clone(), file.clone()));
                        }
                    }
                }
            }
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
pub(crate) fn flac(src: &Path, hash: &str, game: &Path, got: &mut Gathered) -> Option<String> {
    let rel = format!("assets/{hash}.flac");
    if got.has(&rel) {
        return Some(rel);
    }
    let to = game.join(&rel);
    std::fs::create_dir_all(to.parent()?).ok()?;
    let made = std::process::Command::new("/usr/bin/afconvert").args(["-d", "flac", "-f", "flac"]).arg(src).arg(&to).output();
    let soft = raea_io_media::backend("soft").ok()?;
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

pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub(crate) fn info_plist(name: &str, exec: &str, icon: bool) -> String {
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
pub(crate) fn make_icns(png: &Path, out: &Path) -> R<PathBuf> {
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
