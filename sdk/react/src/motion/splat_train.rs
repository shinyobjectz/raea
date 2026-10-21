//! Splats from footage: a video (or a folder of pictures) of a real place or thing trained into a 3D Gaussian splat for
//! `<Splat src>` (rdb::assets::Store::splat; tools/splat: frames, COLMAP, Brush on Metal).
//!
//!   raea-motion splat train VIDEO|IMAGE_DIR -o OUT.ply [--frames N] [--steps N] [--max-res PX] [--from S]
//!                     [--seconds N] [--max-splats N] [--matte black] [--match sequential|exhaustive]
//!                     [--credit TEXT] [--license TEXT] [--dir EDIT_DIR]
//!   raea-motion splat setup
//!
//! Goes through the asset store beside EDIT_DIR/edit.tsx (default the working folder), so the same footage and params
//! train once; OUT.ply is a copy of the store's `<hash>.ply` and OUT.ply.json of its sidecar (with --credit and
//! --license added: they don't change the splat, so they aren't in its cache key).

use std::path::PathBuf;

use serde_json::{Value, json};

type R<T> = Result<T, String>;

const USAGE: &str = "raea-motion splat train VIDEO|IMAGE_DIR -o OUT.ply [--frames 150] [--steps 20000] [--max-res 1600] [--from S] [--seconds N] [--max-splats 2000000] [--matte none|black] [--match sequential|exhaustive] [--credit TEXT] [--license TEXT] [--dir EDIT_DIR]\n       raea-motion splat setup";
const WITH_VALUE: [&str; 13] = ["--frames", "--steps", "--max-res", "--from", "--seconds", "--max-splats", "--matte", "--match", "--credit", "--license", "--dir", "-o", "--o"];

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// The CLI's input and params (what Store::splat is asked), checked: (input, params JSON).
pub fn parse(args: &[String]) -> R<(PathBuf, Value)> {
    let mut input = None;
    let mut i = 2;
    while i < args.len() {
        let a = args[i].as_str();
        if WITH_VALUE.contains(&a) {
            if i + 1 >= args.len() {
                return Err(format!("{a} needs a value\n{USAGE}"));
            }
            i += 2;
            continue;
        }
        if a.starts_with("--") {
            return Err(format!("unknown flag {a}\n{USAGE}"));
        }
        if input.replace(PathBuf::from(a)).is_some() {
            return Err(format!("one video or folder at a time\n{USAGE}"));
        }
        i += 1;
    }
    let input = input.ok_or(USAGE)?;
    let mut p = serde_json::Map::new();
    for (f, k) in [("--frames", "frames"), ("--steps", "steps"), ("--max-res", "max_res"), ("--from", "from"), ("--seconds", "seconds"), ("--max-splats", "max_splats")] {
        if let Some(v) = flag(args, f) {
            let n: f64 = v.parse().ok().filter(|n: &f64| n.is_finite()).ok_or(format!("{f} takes a number, got `{v}`"))?;
            p.insert(k.into(), json!(n));
        }
    }
    for k in ["matte", "match"] {
        if let Some(v) = flag(args, &format!("--{k}")) {
            p.insert(k.into(), json!(v));
        }
    }
    Ok((input, rdb::assets::splat_params(&Value::Object(p))?))
}

/// `raea-motion splat ...`
pub fn splat(args: &[String]) -> R<String> {
    match args.get(1).map(String::as_str) {
        Some("setup") => {
            let d = rdb::assets::ensure_splat()?;
            Ok(json!({"installed": d.display().to_string()}).to_string())
        }
        Some("train") => train(args),
        _ => Err(USAGE.into()),
    }
}

fn train(args: &[String]) -> R<String> {
    let (input, params) = parse(args)?;
    let input = input.canonicalize().map_err(|e| format!("{}: {e}", input.display()))?;
    let out = PathBuf::from(flag(args, "-o").or_else(|| flag(args, "--o")).ok_or(format!("-o OUT.ply is required\n{USAGE}"))?);
    if out.extension().and_then(|e| e.to_str()) != Some("ply") {
        return Err(format!("-o names a .ply, got {}", out.display()));
    }
    let dir = PathBuf::from(flag(args, "--dir").unwrap_or_else(|| ".".into()));
    let store = rdb::assets::Store::beside(&dir.join("edit.tsx"));
    let t0 = std::time::Instant::now();
    let (a, fresh) = store.splat(&input, &params)?;
    let mut side: Value = serde_json::from_slice(&std::fs::read(store.sidecar(&a)).map_err(|e| format!("{}: {e}", store.sidecar(&a).display()))?).map_err(|e| format!("sidecar: {e}"))?;
    for k in ["credit", "license"] {
        if let Some(v) = flag(args, &format!("--{k}")) {
            side[k] = json!(v);
        }
    }
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::copy(store.path(&a), &out).map_err(|e| format!("{}: {e}", out.display()))?;
    let sc = PathBuf::from(format!("{}.json", out.display()));
    std::fs::write(&sc, serde_json::to_string_pretty(&side).unwrap_or_default()).map_err(|e| format!("{}: {e}", sc.display()))?;
    Ok(json!({
        "ply": out.display().to_string(), "sidecar": sc.display().to_string(), "asset": a.hash, "fresh": fresh,
        "count": side["count"], "psnr": side["psnr"], "registered": side["registered"], "frames": side["frames"],
        "seconds": side["seconds"], "took": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0, "notes": side["notes"],
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_train_flags() {
        let a: Vec<String> = ["splat", "train", "clip.mp4", "-o", "x.ply", "--steps", "3000", "--matte", "black"].iter().map(|s| s.to_string()).collect();
        let (input, p) = parse(&a).unwrap();
        assert_eq!(input, PathBuf::from("clip.mp4"));
        assert_eq!(p["steps"], json!(3000.0));
        assert_eq!(p["matte"], json!("black"));
        let bad: Vec<String> = ["splat", "train", "clip.mp4", "--nope"].iter().map(|s| s.to_string()).collect();
        assert!(parse(&bad).is_err());
    }
}
