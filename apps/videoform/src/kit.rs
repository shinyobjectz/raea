//! `raea videoform kit BRAND [FILE|DIR...] [--json]` (docs/videoform.feature VF9b): a brand's kit, kept in its
//! fingerprint (brand-SLUG, made when there's none) so a team that shares the fingerprint shares the kit. Each still
//! in a folder given (a video in one is the brand's footage, not its kit), and each file named (a clip named is a
//! graphic clip of the kit), is copied in once by content hash and read for what it is: a logo, a product, a person, a
//! testimonial, a screenshot, a graphic or a clip, with its words and colours. With no files, what the kit holds.

use crate::{plain, readers};
use raea_sys_videoform_kit::{Asset, Kit, KINDS, Seen};
use std::path::PathBuf;
use std::process::Command;

/// The stills a kit takes from a folder
const STILLS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// A brand's fingerprint folder (brand-SLUG), made when there's none
pub(crate) use raea_app_videoform_docs::brand_dir;

/// What raea-perceive kit read of each file, in order
fn read(files: &[PathBuf]) -> Result<Vec<Result<Seen, String>>, String> {
    let out = Command::new(raea_io_assets::perceive_bin()).arg("kit").args(files).output().map_err(|e| format!("raea-perceive kit: {e}"))?;
    if !out.status.success() {
        return Err(format!("raea-perceive kit: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let lines: Vec<serde_json::Value> = String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    Ok(files
        .iter()
        .enumerate()
        .map(|(i, _)| match lines.get(i) {
            Some(v) if v.get("seen").is_some() => serde_json::from_value(v["seen"].clone()).map_err(|e| e.to_string()),
            Some(v) => Err(v["error"].as_str().unwrap_or("not read").to_string()),
            None => Err("not read".into()),
        })
        .collect())
}

fn line(a: &Asset) -> String {
    let words: String = a.words.chars().take(70).collect();
    format!("  {:<11} {}{}", a.kind, a.name, if words.is_empty() { String::new() } else { format!(": \"{words}\"") })
}

/// `raea videoform kit BRAND [FILE|DIR...] [--json]`
pub(crate) fn verb(args: &[String]) -> (String, i32) {
    let words = plain(args);
    let Some(brand) = words.first() else { return ("usage: raea videoform kit BRAND [FILE|DIR...] [--json]".into(), 2) };
    let dir = match brand_dir(brand) {
        Ok(d) => d,
        Err(e) => return (e, 1),
    };
    let mut kit = match Kit::load(&dir) {
        Ok(k) => k,
        Err(e) => return (e, 1),
    };
    // every asset tagged by this raea, from what was read of it
    let changed = kit.retag();
    let mut given = Vec::new();
    for a in &words[1..] {
        let p = PathBuf::from(a);
        if p.is_dir() {
            crate::walk(&p, &mut given, |p| crate::ext_in(p, STILLS));
        } else if p.is_file() && (crate::ext_in(&p, STILLS) || crate::is_media(&p)) {
            given.push(p);
        } else {
            return (format!("raea videoform kit: {a} is neither a still, a clip nor a folder"), 2);
        }
    }
    // each new one copied into the kit by its content hash, then all of them read at once
    let mut fresh: Vec<(PathBuf, PathBuf, String)> = Vec::new();
    if let Err(e) = std::fs::create_dir_all(dir.join("kit")) {
        return (e.to_string(), 1);
    }
    let mut out = Vec::new();
    for p in given {
        let hash = match readers::hash(&p, None) {
            Ok(h) => h,
            Err(e) => {
                out.push(format!("  not added: {}: {e}", p.display()));
                continue;
            }
        };
        if kit.assets.iter().any(|a| a.hash == hash) || fresh.iter().any(|f| f.2 == hash) {
            continue;
        }
        let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let to = dir.join("kit").join(format!("{hash}.{ext}"));
        if let Err(e) = std::fs::copy(&p, &to) {
            out.push(format!("  not added: {}: {e}", p.display()));
            continue;
        }
        fresh.push((p, to, hash));
    }
    let seen = match read(&fresh.iter().map(|f| f.1.clone()).collect::<Vec<_>>()) {
        Ok(s) if !fresh.is_empty() => s,
        Ok(_) => Vec::new(),
        Err(e) => return (e, 1),
    };
    let before = kit.assets.len();
    for ((from, to, hash), s) in fresh.into_iter().zip(seen) {
        let file = to.file_name().unwrap().to_string_lossy().into_owned();
        match s {
            Ok(s) => kit.assets.push(Asset::read(s, hash, file, &from.to_string_lossy(), crate::today())),
            Err(e) => out.push(format!("  not read: {}: {e}", from.display())),
        }
    }
    if (changed || kit.assets.len() > before)
        && let Err(e) = kit.save(&dir)
    {
        return (e, 1);
    }
    if args.iter().any(|a| a == "--json") {
        return (serde_json::to_string_pretty(&kit).unwrap(), 0);
    }
    let mut head = vec![format!("{}'s kit, {}: {}", brand, dir.join("kit").display(), kit.said())];
    if words.len() > 1 {
        head.push(format!("{} added now:", kit.assets.len() - before));
        head.extend(kit.assets[before..].iter().map(line));
    } else {
        for k in KINDS {
            head.extend(kit.assets.iter().filter(|a| a.kind == k).map(line));
        }
    }
    head.extend(out);
    (head.join("\n"), 0)
}
