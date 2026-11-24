//! Made assets. A composition asks for one by what makes it, `make(model, prompt, seed, params)`, and gets back a
//! content hash, `asset:<sha256>`. The bytes are stored once under `<edit dir>/assets/<hash>.<ext>`; the edit holds
//! only the hash, so re-renders reuse identical bytes whatever the generator would do next time.
//!
//! `make` is a registry keyed by model name: `stand-in` (a deterministic gradient and shapes drawn from the prompt and
//! seed, what tests use), the two local audio models, `tokensynth` (an instrument's notes, for music) and `sao` (Stable
//! Audio Open on Core ML, foley). Every model runs on this computer. `lyria` (music) and `fish` (narration, through
//! raea's API) are retired: a take made before plays from the index, and a new one is refused. Narration is Pocket TTS
//! (raea/speech), and Fish Audio is a `raea gen` backend like fal or Replicate (`fish:<model>`, the user's own key). The local models install themselves on first use (tools/sao/setup.py,
//! tools/tokensynth/setup.py, run through uv on any OS: setup.rs). Each made asset gets a line in `assets/made.jsonl` under its cache key, a hash of
//! (model, prompt, seed, params); `made` reads only that index, so a render or a check that resolves a make() never
//! generates.

use std::path::{Path, PathBuf};

use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};

// splats trained from footage: Store::splat (assets/splat.rs, tools/splat)
mod splat;
// baked simulations as a mesh a frame (.rseq): tools/goo writes them, raea-render's engine/meshseq.rs plays them
pub mod meshseq;
pub use splat::{ensure_splat, splat_dir, splat_params};

mod local;
use local::*;
mod setup;
pub use setup::{ffmpeg_tool, git, run_setup, setup_how, uv, venv_python};
#[cfg(test)]
mod tests;

/// The models `make` knows, by the name an edit writes: the local ones, then the retired ones whose takes still play. `matchmove` (a camera
/// solve) is made from a clip, by `Store::matchmove`, and indexed the same way.
pub const MODELS: [&str; 6] = ["stand-in", "tokensynth", "sao", "fish", "lyria", "matchmove"];

/// TokenSynth's runtimes (tools/tokensynth/run.py --precision), and the one a note gets when its params don't say.
pub const TOKENSYNTH_PRECISIONS: [&str; 5] = ["fp32", "fp16", "bf16", "int8", "coreml"];
pub const TOKENSYNTH_PRECISION: &str = "fp16";

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub hash: String,
    pub ext: String,
    pub origin: J,
}

#[derive(Debug, Clone)]
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    /// The store next to an edit: `<dir>/assets`.
    pub fn beside(edit: &Path) -> Store {
        Store { dir: edit.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")).join("assets") }
    }

    pub fn path(&self, a: &Asset) -> PathBuf {
        self.dir.join(format!("{}.{}", a.hash, a.ext))
    }

    /// Keep bytes under their hash (once); the asset row.
    fn put(&self, bytes: &[u8], ext: &str, origin: J) -> Result<Asset, String> {
        let hash = format!("{:x}", Sha256::digest(bytes));
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let a = Asset { hash, ext: ext.into(), origin };
        let p = self.path(&a);
        if !p.exists() {
            std::fs::write(&p, bytes).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        Ok(a)
    }

    /// The asset an earlier `make` kept for these arguments, from the index alone (None: not made yet).
    pub fn made(&self, model: &str, prompt: &str, seed: u64, params: &J) -> Result<Option<Asset>, String> {
        if !MODELS.contains(&model) {
            return Err(format!("unknown model `{model}`: make() knows {}", MODELS.join(", ")));
        }
        // an asset indexed before keys wrote whole numbers one way is found under the key it was made with
        let (key, legacy) = (make_key(model, prompt, seed, params), legacy_key(model, prompt, seed, params));
        let index = std::fs::read_to_string(self.dir.join("made.jsonl")).unwrap_or_default();
        Ok(index
            .lines()
            .filter_map(|l| serde_json::from_str::<J>(l).ok())
            .filter(|l| l["key"] == key.as_str() || l["key"] == legacy.as_str())
            .last()
            .map(|l| Asset { hash: l["hash"].as_str().unwrap_or_default().into(), ext: l["ext"].as_str().unwrap_or_default().into(), origin: l["origin"].clone() }))
    }

    /// Generation, once per (model, prompt, seed, params): the indexed asset while its bytes are here, else the model's
    /// bytes, kept and indexed.
    pub fn make(&self, model: &str, prompt: &str, seed: u64, params: &J) -> Result<Asset, String> {
        if let Some(a) = self.made(model, prompt, seed, params)?.filter(|a| self.path(a).exists()) {
            return Ok(a);
        }
        let key = make_key(model, prompt, seed, params);
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        // a provider writes its response here; the bytes then go under their hash
        let part = self.dir.join(format!(".{key}.part"));
        let made = match model {
            "stand-in" => stand_in(prompt, seed).map(|png| (png, "png".to_string())),
            "sao" => sao(prompt, seed, params, &part).map(|wav| (wav, "wav".to_string())),
            "tokensynth" => tokensynth(prompt, seed, params, &part).map(|wav| (wav, "wav".to_string())),
            "matchmove" => Err("matchmove is made from a clip: Store::matchmove(clip, params)".to_string()),
            "lyria" => Err("lyria is retired (raea's generation policy, 2026-10-20): make music with tokensynth; a lyria take made before still plays from assets/made.jsonl".to_string()),
            "fish" => Err("fish is no longer part of raea (2026-11-05): narrate with <Speech> from raea/speech (Pocket TTS, on this computer), or make a line with a `raea gen` step on fish:<model> and your FISH_API_KEY; a fish take made before still plays from assets/made.jsonl".to_string()),
            _ => Err(format!("unknown model `{model}`")),
        };
        let _ = std::fs::remove_file(&part);
        let (bytes, ext) = made?;
        let a = self.put(&bytes, &ext, json!({"how": "made", "model": model, "prompt": prompt, "seed": seed, "params": params}))?;
        let index = self.dir.join("made.jsonl");
        let line = json!({"key": key, "hash": a.hash, "ext": a.ext, "origin": a.origin});
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&index).map_err(|e| format!("{}: {e}", index.display()))?;
        std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()).map_err(|e| format!("{}: {e}", index.display()))?;
        Ok(a)
    }
}

/// A 768x768 image from (prompt, seed): two colours and soft discs, all from a hash of the two. Deterministic.
fn stand_in(prompt: &str, seed: u64) -> Result<Vec<u8>, String> {
    let h = Sha256::digest(format!("{prompt}\u{0}{seed}").as_bytes());
    let c = |i: usize| [h[i] as f32, h[i + 1] as f32, h[i + 2] as f32];
    let (a, b) = (c(0), c(3));
    let discs: Vec<(f32, f32, f32, [f32; 3])> = (0..4).map(|k| (h[6 + k * 5] as f32 / 255.0 * 768.0, h[7 + k * 5] as f32 / 255.0 * 768.0, 60.0 + h[8 + k * 5] as f32, c(9 + k * 5))).collect();
    let mut img = image::RgbImage::new(768, 768);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let t = (x as f32 + y as f32) / 1536.0;
        let mut px = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        for (cx, cy, r, col) in &discs {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let w = (1.0 - d / r).clamp(0.0, 1.0) * 0.6;
            for i in 0..3 {
                px[i] = px[i] * (1.0 - w) + col[i] * w;
            }
        }
        *p = image::Rgb(px.map(|v| v.round().clamp(0.0, 255.0) as u8));
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// make()'s cache key: a hash of everything that decides the bytes. Params are hashed with their keys sorted, so the
/// order they're written in doesn't make another asset, and with whole numbers written as integers, so `seconds: 1`
/// from a Tone graph (JSON from JavaScript) and `1.0` from Rust (an <Audio>'s f64, the CLI's) are the same asset.
pub fn make_key(model: &str, prompt: &str, seed: u64, params: &J) -> String {
    format!("{:x}", Sha256::digest(json!([model, prompt, seed, canonical(params, true)]).to_string().as_bytes()))
}

/// The key as it was before whole numbers were made integers (2026-10-19): what older made.jsonl lines are under.
fn legacy_key(model: &str, prompt: &str, seed: u64, params: &J) -> String {
    format!("{:x}", Sha256::digest(json!([model, prompt, seed, canonical(params, false)]).to_string().as_bytes()))
}

/// Params with their keys sorted, and (`whole`) floats with no fractional part as integers.
fn canonical(j: &J, whole: bool) -> J {
    match j {
        J::Object(o) => {
            let mut kv: Vec<_> = o.iter().collect();
            kv.sort_by(|a, b| a.0.cmp(b.0));
            J::Object(kv.into_iter().map(|(k, v)| (k.clone(), canonical(v, whole))).collect())
        }
        J::Array(a) => J::Array(a.iter().map(|v| canonical(v, whole)).collect()),
        J::Number(n) if whole && n.is_f64() => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => json!(f as i64),
            _ => j.clone(),
        },
        other => other.clone(),
    }
}

/// Where a local model is installed: $RAEA_SAO / $RAEA_TOKENSYNTH / $RAEA_MATCHMOVE, else ~/.cache/raea-sao,
/// ~/.cache/raea-tokensynth or ~/.cache/raea-matchmove.
pub fn install_dir(model: &str) -> PathBuf {
    let (var, name) = install_var(model);
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| raea_io_home::user_home().unwrap_or_default().join(".cache").join(name))
}

/// A local model's install variable and default folder name.
fn install_var(model: &str) -> (&'static str, &'static str) {
    match model {
        "sao" => ("RAEA_SAO", "raea-sao"),
        "matchmove" => ("RAEA_MATCHMOVE", "raea-matchmove"),
        _ => ("RAEA_TOKENSYNTH", "raea-tokensynth"),
    }
}

/// The script that installs a local model (in raea's own files, raea_io_home::home(); $RAEA_TOOLS names another tools/ folder).
pub fn setup_script(model: &str) -> PathBuf {
    let tools = std::env::var_os("RAEA_TOOLS").map(PathBuf::from).unwrap_or_else(|| raea_io_home::home().join("tools"));
    let p = tools.join(model).join("setup.sh");
    dunce::canonicalize(&p).unwrap_or(p)
}

/// Whether a local model can run from `dir`. SAO: the venv and either run.py with its setup's `.ready`, or the
/// hand-built spike's coreml_run.py (~/.cache/raea-sao before tools/sao existed, still served as it is). TokenSynth:
/// the venv, run.py, and its setup's `.ready` or the fp16 runtime that every finished install has.
pub fn installed(model: &str, dir: &Path) -> bool {
    let py = venv_python(dir).exists();
    match model {
        "sao" => py && ((dir.join("run.py").exists() && dir.join(".ready").exists()) || dir.join("coreml_run.py").exists()),
        "tokensynth" => py && dir.join("run.py").exists() && (dir.join(".ready").exists() || dir.join("weights-fp16/token_synth_aug.pt").exists()),
        "matchmove" => py && dir.join("run.py").exists() && dir.join(".ready").exists(),
        _ => true,
    }
}

/// A local model's install, set up first if it isn't there: its setup runs once, its progress on stderr (a render's
/// log, the CLI's terminal), unless $RAEA_AUDIO_AUTOSETUP=0, when the error names the one command that installs it.
pub fn ensure_installed(model: &str) -> Result<PathBuf, String> {
    let dir = install_dir(model);
    if installed(model, &dir) {
        return Ok(dir);
    }
    let script = setup_script(model);
    let size = match model {
        "sao" => "about 2 GB",
        "matchmove" => "about 250 MB",
        _ => "about 6 GB",
    };
    let how = format!("{} (installs it into {}, {size}, once)", setup_how(&script), dir.display());
    if std::env::var("RAEA_AUDIO_AUTOSETUP").as_deref() == Ok("0") {
        return Err(format!("{model} isn't installed: run {how}"));
    }
    raea_io_home::offline(&format!("installing {model} ({size} downloaded)"))?;
    if !script.exists() {
        return Err(format!("{model} isn't installed in {}, and its setup script isn't at {}", dir.display(), script.display()));
    }
    eprintln!("raea: {model} isn't installed yet; setting it up now with {how}");
    let status = run_setup(&script, install_var(model).0, &dir).map_err(|e| format!("{model} setup: {e}"))?;
    if !status.success() || !installed(model, &dir) {
        return Err(format!("{model} setup didn't finish ({status}): run {how}"));
    }
    eprintln!("raea: {model} is set up in {}", dir.display());
    Ok(dir)
}

/// The params a matchmove solve is cached under, filled with their defaults so a solve asked for with and without a
/// default is the same asset: {from, seconds (0: to the clip's end), subject ([x, y, w, h] in 0..1 of the first frame,
/// or null), height (the camera's height over the ground, metres), fov (vertical degrees for a solve without parallax;
/// 0: COLMAP's or 40), smooth (frames of Gaussian smoothing on the path)}.
pub fn matchmove_params(p: &J) -> Result<J, String> {
    let num = |k: &str, d: f64| -> Result<f64, String> {
        match p.get(k) {
            None | Some(J::Null) => Ok(d),
            Some(v) => v.as_f64().filter(|n| n.is_finite()).ok_or(format!("matchmove: {k} must be a number, got {v}")),
        }
    };
    let subject = match p.get("subject") {
        None | Some(J::Null) => J::Null,
        Some(J::Array(a)) if a.len() == 4 && a.iter().all(|v| v.as_f64().is_some_and(|n| (0.0..=1.0).contains(&n))) => J::Array(a.clone()),
        Some(v) => return Err(format!("matchmove: subject is [x, y, w, h] in 0..1 of the frame, got {v}")),
    };
    let (from, seconds, height, fov, smooth) = (num("from", 0.0)?, num("seconds", 0.0)?, num("height", 1.6)?, num("fov", 0.0)?, num("smooth", 1.0)?);
    if from < 0.0 || seconds < 0.0 || height <= 0.0 || !(0.0..180.0).contains(&fov) || smooth < 0.0 {
        return Err(format!("matchmove: from and seconds 0 or more, height above 0, fov 0..180, smooth 0 or more; got {p}"));
    }
    Ok(json!({"from": from, "seconds": seconds, "subject": subject, "height": height, "fov": fov, "smooth": smooth}))
}

/// raea-perceive (EdgeTAM tracking, face tracks): $RAEA_PERCEIVE, else the first found beside this raea (an install's
/// ~/.raea/bin, which the release fills), in the checkout's release build or on PATH.
pub fn perceive_bin() -> PathBuf {
    // raea-perceive.exe on Windows (#43)
    const NAME: &str = if cfg!(windows) { "raea-perceive.exe" } else { "raea-perceive" };
    let beside = std::env::current_exe().ok().and_then(|e| dunce::canonicalize(&e).ok()).and_then(|e| Some(e.parent()?.join(NAME)));
    let found = beside.into_iter().chain([PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/release")).join(NAME)]).chain(std::env::var_os("PATH").iter().flat_map(std::env::split_paths).map(|d| d.join(NAME))).find(|p| p.is_file());
    std::env::var_os("RAEA_PERCEIVE").filter(|v| !v.is_empty()).map(PathBuf::from).or(found).unwrap_or_else(|| NAME.into())
}

/// A file's SHA-256, streamed.
fn file_hash(p: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("{}: {e}", p.display()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

impl Store {
    /// A clip's camera solve (tools/matchmove/run.py: COLMAP, with a rotation-only fallback), made once per (the clip's
    /// content, params) and kept as JSON (`matchmove_params` says what the params are; run.py what the JSON holds). A
    /// subject's cut-outs go beside it in `matchmove-<key>/cut/`, and the JSON's `cut` names them relative to the
    /// store. `fresh`: whether it was solved now (false: from the index).
    pub fn matchmove(&self, clip: &Path, params: &J) -> Result<(Asset, bool), String> {
        let params = matchmove_params(params)?;
        let prompt = format!("sha256:{}", file_hash(clip)?);
        if let Some(a) = self.made("matchmove", &prompt, 1, &params)?.filter(|a| self.path(a).exists()) {
            return Ok((a, false));
        }
        let dir = ensure_installed("matchmove")?;
        let key = make_key("matchmove", &prompt, 1, &params);
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let out = self.dir.join(format!(".matchmove-{key}.part"));
        let _ = std::fs::remove_dir_all(&out);
        let mut cmd = std::process::Command::new(venv_python(&dir));
        // the repo's run.py when raea's files are here (so an edit to it is used without reinstalling), else the installed copy
        let repo = setup_script("matchmove").with_file_name("run.py");
        cmd.arg("-I").arg(if repo.exists() { repo } else { dir.join("run.py") }).arg("--video").arg(clip).arg("--out").arg(&out);
        for k in ["from", "seconds", "height", "fov", "smooth"] {
            cmd.args([format!("--{k}"), params[k].as_f64().unwrap_or(0.0).to_string()]);
        }
        if let J::Array(b) = &params["subject"] {
            let bin = Some(perceive_bin()).filter(|b| b.exists()).ok_or("matchmove: a subject is tracked by raea-perceive (EdgeTAM), not found beside raea: update raea (`curl -fsSL https://raea.io/install | sh`: releases ship it in ~/.raea/bin), or set RAEA_PERCEIVE")?;
            cmd.args(["--subject", &b.iter().map(|v| v.as_f64().unwrap_or(0.0).to_string()).collect::<Vec<_>>().join(",")]).arg("--perceive").arg(bin);
        }
        // its progress on stderr: stdout is the caller's answer
        let status = cmd.stdout(std::io::stderr()).status().map_err(|e| format!("matchmove: {}: {e}", dir.display()))?;
        let solved = std::fs::read(out.join("solve.json"));
        if !status.success() || solved.is_err() {
            let _ = std::fs::remove_dir_all(&out);
            return Err(format!("matchmove: the solve of {} failed ({status}); its log is above", clip.display()));
        }
        let mut solve: J = serde_json::from_slice(&solved.unwrap()).map_err(|e| format!("matchmove: solve.json: {e}"))?;
        if solve["cut"].is_string() {
            let keep = self.dir.join(format!("matchmove-{}", &key[..16]));
            let _ = std::fs::remove_dir_all(&keep);
            std::fs::create_dir_all(&keep).map_err(|e| format!("{}: {e}", keep.display()))?;
            std::fs::rename(out.join("cut"), keep.join("cut")).map_err(|e| format!("{}: {e}", keep.display()))?;
            solve["cut"] = json!(format!("matchmove-{}/cut/%04d.png", &key[..16]));
        }
        let _ = std::fs::remove_dir_all(&out);
        let a = self.put(solve.to_string().as_bytes(), "json", json!({"how": "made", "model": "matchmove", "prompt": prompt, "seed": 1, "params": params, "clip": clip.display().to_string()}))?;
        let index = self.dir.join("made.jsonl");
        let line = json!({"key": key, "hash": a.hash, "ext": a.ext, "origin": a.origin});
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&index).map_err(|e| format!("{}: {e}", index.display()))?;
        std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()).map_err(|e| format!("{}: {e}", index.display()))?;
        Ok((a, true))
    }
}
