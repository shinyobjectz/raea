//! Splats made from footage: a video (or a folder of pictures) of a real place or thing trained into a 3D Gaussian
//! splat (tools/splat/run.py: frames, COLMAP through pycolmap, Brush on Metal), made once per (the input's content,
//! params) and kept in the edit's asset store as `<hash>.ply` with its sidecar `<hash>.ply.json` (where the splat
//! stands in raea's world, the cameras that shot it; run.py says what it holds). The training installs itself on first
//! use (tools/splat/setup.sh into $RAEA_SPLAT, else ~/.cache/raea-splat).
//!
//! Cached as a `splat` line in `assets/made.jsonl` under make_key("splat", "sha256:<input>", 1, params), as matchmove
//! solves are; `made()` doesn't serve it (it's made from footage, never from a prompt).

use super::*;

/// Where the splat trainer is installed: $RAEA_SPLAT, else ~/.cache/raea-splat.
pub fn splat_dir() -> PathBuf {
    std::env::var_os("RAEA_SPLAT").filter(|v| !v.is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/raea-splat"))
}

fn splat_installed(dir: &Path) -> bool {
    dir.join(".venv/bin/python").exists() && dir.join("run.py").exists() && dir.join(".ready").exists() && dir.join("brush/brush_app").exists()
}

/// The trainer's install, set up first if it isn't there (tools/splat/setup.sh, about 360 MB, once), unless
/// $RAEA_AUDIO_AUTOSETUP=0 (raea's one switch for local tools installing themselves).
pub fn ensure_splat() -> Result<PathBuf, String> {
    let dir = splat_dir();
    if splat_installed(&dir) {
        return Ok(dir);
    }
    let script = setup_script("splat");
    let how = format!("{} (installs it into {}, about 360 MB, once)", script.display(), dir.display());
    if std::env::var("RAEA_AUDIO_AUTOSETUP").as_deref() == Ok("0") {
        return Err(format!("the splat trainer isn't installed: run {how}"));
    }
    if !script.exists() {
        return Err(format!("the splat trainer isn't installed in {}, and its setup script isn't at {}", dir.display(), script.display()));
    }
    eprintln!("raea: the splat trainer isn't installed yet; setting it up now with {how}");
    let status = std::process::Command::new("sh")
        .arg(&script)
        .env("RAEA_SPLAT", &dir)
        .stdout(std::io::stderr())
        .status()
        .map_err(|e| format!("splat setup: {e}"))?;
    if !status.success() || !splat_installed(&dir) {
        return Err(format!("splat setup didn't finish ({status}): run {how}"));
    }
    Ok(dir)
}

/// The params a splat is cached under, filled with their defaults so a splat asked for with and without a default is
/// the same asset: {frames (pictures trained on), steps (Brush's), max_res (their long side, px), from, seconds (of a
/// video; 0: to its end), max_splats, matte ("none", or "black": an object filmed on black, trained alone), match
/// ("": sequential for a video of more than 60 frames, else exhaustive)}.
pub fn splat_params(p: &J) -> Result<J, String> {
    let num = |k: &str, d: f64| -> Result<f64, String> {
        match p.get(k) {
            None | Some(J::Null) => Ok(d),
            Some(v) => v.as_f64().filter(|n| n.is_finite()).ok_or(format!("splat: {k} must be a number, got {v}")),
        }
    };
    let s = |k: &str, d: &str, ok: &[&str]| -> Result<String, String> {
        let v = p.get(k).and_then(J::as_str).unwrap_or(d).to_string();
        if ok.contains(&v.as_str()) { Ok(v) } else { Err(format!("splat: {k} is one of {}, got `{v}`", ok.join(", "))) }
    };
    let (frames, steps, max_res, from, seconds, max_splats) =
        (num("frames", 150.0)?, num("steps", 20000.0)?, num("max_res", 1600.0)?, num("from", 0.0)?, num("seconds", 0.0)?, num("max_splats", 2_000_000.0)?);
    if !(8.0..=2000.0).contains(&frames) || !(100.0..=200_000.0).contains(&steps) || !(256.0..=4096.0).contains(&max_res) || from < 0.0 || seconds < 0.0 || max_splats < 1000.0 {
        return Err(format!("splat: frames 8..2000, steps 100..200000, max_res 256..4096, from and seconds 0 or more, max_splats 1000 or more; got {p}"));
    }
    let r = |x: f64| x.round();
    Ok(json!({"frames": r(frames), "steps": r(steps), "max_res": r(max_res), "from": from, "seconds": seconds, "max_splats": r(max_splats),
              "matte": s("matte", "none", &["none", "black"])?, "match": s("match", "", &["", "sequential", "exhaustive"])?}))
}

/// A file's SHA-256, streamed (its own copy: this module stands apart from the matchmove code beside it).
fn sha_file(p: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("{}: {e}", p.display()))?;
        if n == 0 {
            return Ok(format!("{:x}", h.finalize()));
        }
        h.update(&buf[..n]);
    }
}

/// What an input is, by content: a file's SHA-256, or a folder's pictures' (their names and hashes, in name order).
fn input_hash(p: &Path) -> Result<String, String> {
    if !p.is_dir() {
        return sha_file(p);
    }
    let mut names: Vec<PathBuf> = std::fs::read_dir(p).map_err(|e| format!("{}: {e}", p.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|f| f.extension().and_then(|x| x.to_str()).is_some_and(|x| ["jpg", "jpeg", "png", "tif", "tiff", "webp"].contains(&x.to_ascii_lowercase().as_str())))
        .collect();
    names.sort();
    let mut h = Sha256::new();
    for f in &names {
        h.update(f.file_name().unwrap_or_default().as_encoded_bytes());
        h.update(sha_file(f)?.as_bytes());
    }
    Ok(format!("{:x}", h.finalize()))
}

impl Store {
    /// A splat's sidecar beside it in the store (`<hash>.ply.json`).
    pub fn sidecar(&self, a: &Asset) -> PathBuf {
        self.dir.join(format!("{}.{}.json", a.hash, a.ext))
    }

    /// A splat trained from footage (a video, or a folder of pictures), made once per (the input's content, params)
    /// and kept as `<hash>.ply` + `<hash>.ply.json`. `fresh`: whether it was trained now (false: from the index).
    pub fn splat(&self, input: &Path, params: &J) -> Result<(Asset, bool), String> {
        let params = splat_params(params)?;
        let prompt = format!("sha256:{}", input_hash(input)?);
        let key = make_key("splat", &prompt, 1, &params);
        let index = std::fs::read_to_string(self.dir.join("made.jsonl")).unwrap_or_default();
        if let Some(a) = index.lines().filter_map(|l| serde_json::from_str::<J>(l).ok()).filter(|l| l["key"] == key.as_str()).last()
            .map(|l| Asset { hash: l["hash"].as_str().unwrap_or_default().into(), ext: l["ext"].as_str().unwrap_or_default().into(), origin: l["origin"].clone() })
            .filter(|a| self.path(a).exists() && self.sidecar(a).exists())
        {
            return Ok((a, false));
        }
        let dir = ensure_splat()?;
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let out = self.dir.join(format!(".splat-{key}.part"));
        let _ = std::fs::remove_dir_all(&out);
        let mut cmd = std::process::Command::new(dir.join(".venv/bin/python"));
        cmd.arg("-I").arg(dir.join("run.py")).arg("--input").arg(input).arg("--out").arg(&out).arg("--brush").arg(dir.join("brush/brush_app"));
        for (k, flag) in [("frames", "--frames"), ("steps", "--steps"), ("max_res", "--max-res"), ("max_splats", "--max-splats")] {
            cmd.args([flag.to_string(), (params[k].as_f64().unwrap_or(0.0) as i64).to_string()]);
        }
        for k in ["from", "seconds"] {
            cmd.args([format!("--{k}"), params[k].as_f64().unwrap_or(0.0).to_string()]);
        }
        cmd.args(["--matte", params["matte"].as_str().unwrap_or("none")]);
        if let Some(m) = params["match"].as_str().filter(|m| !m.is_empty()) {
            cmd.args(["--match", m]);
        }
        // its progress on stderr: stdout is the caller's answer
        let status = cmd.stdout(std::io::stderr()).status().map_err(|e| format!("splat: {}: {e}", dir.display()))?;
        let (ply, side) = (std::fs::read(out.join("splat.ply")), std::fs::read(out.join("splat.ply.json")));
        let (Ok(ply), Ok(side)) = (ply, side) else {
            let _ = std::fs::remove_dir_all(&out);
            return Err(format!("splat: training on {} failed ({status}); its log is above", input.display()));
        };
        if !status.success() {
            let _ = std::fs::remove_dir_all(&out);
            return Err(format!("splat: training on {} failed ({status}); its log is above", input.display()));
        }
        let a = self.put(&ply, "ply", json!({"how": "made", "model": "splat", "prompt": prompt, "seed": 1, "params": params, "input": input.display().to_string()}))?;
        let sc = self.sidecar(&a);
        std::fs::write(&sc, &side).map_err(|e| format!("{}: {e}", sc.display()))?;
        // Brush's eval renders (the held-out views it rendered at the end), kept beside it for a look
        if out.join("eval").is_dir() {
            let keep = self.dir.join(format!("splat-{}-eval", &a.hash[..16]));
            let _ = std::fs::remove_dir_all(&keep);
            let _ = std::fs::rename(out.join("eval"), &keep);
        }
        let _ = std::fs::remove_dir_all(&out);
        let idx = self.dir.join("made.jsonl");
        let line = json!({"key": key, "hash": a.hash, "ext": a.ext, "origin": a.origin});
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&idx).map_err(|e| format!("{}: {e}", idx.display()))?;
        std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()).map_err(|e| format!("{}: {e}", idx.display()))?;
        Ok((a, true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splat_params_fill_defaults_and_refuse_nonsense() {
        let a = splat_params(&json!({})).unwrap();
        let b = splat_params(&json!({"frames": 150, "steps": 20000.0, "matte": "none"})).unwrap();
        assert_eq!(make_key("splat", "x", 1, &a), make_key("splat", "x", 1, &b));
        assert!(splat_params(&json!({"matte": "green"})).is_err());
        assert!(splat_params(&json!({"steps": 5})).is_err());
        assert!(splat_params(&json!({"frames": "many"})).is_err());
    }
}
