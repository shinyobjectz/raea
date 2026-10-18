//! Find-or-make assets (G4). A snippet asks for footage by meaning, `find('hands holding the serum')`, and gets back a
//! content hash, `asset:<sha256>`: retrieval first (raea-meaning searches the footage library, the best frame above
//! a threshold is taken), generation as the fallback (`make(model, prompt, seed, params)`). The bytes are stored once
//! under `<edit dir>/assets/<hash>.<ext>`; the edit's rows hold only the hash, so re-renders reuse identical bytes
//! whatever the generator would do next time. How each asset came to be (found: source, time, score, rank; made:
//! model, prompt, seed, params) is its provenance row, used only by an explicit `remake`, which is a recorded move.
//!
//! `make` is a registry keyed by model name: `stand-in` (a deterministic gradient and shapes drawn from the prompt and
//! seed, what tests use), `sao` (Stable Audio Open on Core ML, foley), and the the host app API's `fish` (speech) and
//! `lyria` (music). Each made asset gets a line in `assets/made.jsonl` under its cache key, a hash of (model, prompt,
//! seed, params); `made` reads only that index, so a render or a check that resolves a make() never generates.

use std::path::{Path, PathBuf};

use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};

/// Below this SigLIP 2 text-image score the library has nothing like the phrase (measured: present concepts score
/// 0.12-0.15, absent ones 0.03-0.06 on the eval footage).
pub const FIND_THRESHOLD: f64 = 0.10;

/// The models `make` knows, by the name an edit writes.
pub const MODELS: [&str; 4] = ["stand-in", "sao", "fish", "lyria"];

/// Paid requests a run may send when the host doesn't say ($RAEA_GEN_BUDGET): the founder's choice, on and capped small.
pub const PAID_PER_RUN: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub hash: String,
    pub ext: String,
    pub origin: J,
}

#[derive(Debug, Clone)]
pub struct Store {
    pub dir: PathBuf,
    pub library: PathBuf,
}

impl Store {
    /// The store next to an edit: `<dir>/assets`, searching $RAEA_LIBRARY (default the raea-meaning column cache).
    pub fn beside(edit: &Path) -> Store {
        let dir = edit.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")).join("assets");
        let library = std::env::var_os("RAEA_LIBRARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/raea-meaning/cols"));
        Store { dir, library }
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

    /// The library's best matches for a phrase, best first: (score, source video, time).
    fn search(&self, query: &str, k: usize) -> Result<Vec<(f64, String, f64)>, String> {
        // the sibling crate's release build (meaning/), unless $RAEA_MEANING names another
        let bin = std::env::var("RAEA_MEANING").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../meaning/target/release/raea-meaning").into());
        let out = std::process::Command::new(&bin)
            .arg("find")
            .arg(&self.library)
            .arg(query)
            .args(["-k", &k.to_string()])
            .output()
            .map_err(|e| format!("{bin}: {e}"))?;
        if !out.status.success() {
            return Err(format!("find: {}", String::from_utf8_lossy(&out.stderr)));
        }
        let hits: Vec<J> = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
        Ok(hits.iter().map(|h| (h["score"].as_f64().unwrap_or(0.0), h["source"].as_str().unwrap_or_default().to_string(), h["t"].as_f64().unwrap_or(0.0))).collect())
    }

    /// Retrieval first; generation when nothing in the library is close enough. `rank` 0 is the best match.
    pub fn find(&self, query: &str, rank: usize) -> Result<Asset, String> {
        let hits: Vec<_> = self.search(query, rank + 1)?.into_iter().filter(|h| h.0 >= FIND_THRESHOLD).collect();
        match hits.get(rank) {
            Some((score, source, t)) => {
                let out = std::process::Command::new("ffmpeg")
                    .args(["-v", "error", "-ss", &format!("{t:.3}"), "-i", source, "-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"])
                    .output()
                    .map_err(|e| format!("ffmpeg: {e}"))?;
                if !out.status.success() || out.stdout.is_empty() {
                    return Err(format!("ffmpeg: couldn't take the frame at {t:.2}s of {source}"));
                }
                self.put(&out.stdout, "png", json!({"how": "found", "query": query, "source": source, "t": t, "score": score, "rank": rank}))
            }
            None => self.make("stand-in", query, 1, &json!({})),
        }
    }

    /// The asset an earlier `make` kept for these arguments, from the index alone (None: not made yet).
    pub fn made(&self, model: &str, prompt: &str, seed: u64, params: &J) -> Result<Option<Asset>, String> {
        if !MODELS.contains(&model) {
            return Err(format!("unknown model `{model}`: make() knows {}", MODELS.join(", ")));
        }
        let key = make_key(model, prompt, seed, params);
        let index = std::fs::read_to_string(self.dir.join("made.jsonl")).unwrap_or_default();
        Ok(index
            .lines()
            .filter_map(|l| serde_json::from_str::<J>(l).ok())
            .filter(|l| l["key"] == key.as_str())
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
            _ => ugc(model, prompt, params, &part),
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

    /// The explicit "do it again": a made asset with the next seed, a found one with the next-ranked match.
    pub fn remake(&self, a: &Asset) -> Result<Asset, String> {
        match a.origin["how"].as_str() {
            Some("made") => {
                // assets made before the registry name their model "stand-in-gradient-v1" and have no params
                let model = a.origin["model"].as_str().filter(|m| MODELS.contains(m)).unwrap_or("stand-in");
                let params = a.origin.get("params").cloned().unwrap_or(json!({}));
                self.make(model, a.origin["prompt"].as_str().unwrap_or_default(), a.origin["seed"].as_u64().unwrap_or(1) + 1, &params)
            }
            Some("found") => self.find(a.origin["query"].as_str().unwrap_or_default(), a.origin["rank"].as_u64().unwrap_or(0) as usize + 1),
            _ => Err(format!("asset {} has no provenance to remake from", a.hash)),
        }
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
/// order they're written in doesn't make another asset.
pub fn make_key(model: &str, prompt: &str, seed: u64, params: &J) -> String {
    fn sorted(j: &J) -> J {
        match j {
            J::Object(o) => {
                let mut kv: Vec<_> = o.iter().collect();
                kv.sort_by(|a, b| a.0.cmp(b.0));
                J::Object(kv.into_iter().map(|(k, v)| (k.clone(), sorted(v))).collect())
            }
            J::Array(a) => J::Array(a.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    format!("{:x}", Sha256::digest(json!([model, prompt, seed, sorted(params)]).to_string().as_bytes()))
}

/// Stable Audio Open Small on Core ML ($RAEA_SAO, else ~/.cache/raea-sao), run as its own process: it writes a wav of
/// `seconds` (default 4; the model's window is 11.9 s) for the prompt and seed.
fn sao(prompt: &str, seed: u64, params: &J, part: &Path) -> Result<Vec<u8>, String> {
    let dir = std::env::var_os("RAEA_SAO").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/raea-sao"));
    let seconds = params["seconds"].as_f64().unwrap_or(4.0);
    // the runner's ffmpeg picks the container from the extension
    let wav = part.with_extension("wav");
    let out = std::process::Command::new(dir.join(".venv/bin/python"))
        .arg(dir.join("coreml_run.py"))
        .args(["--prompt", prompt, "--seed", &seed.to_string(), "--seconds", &seconds.to_string(), "--wav"])
        .arg(&wav)
        .output()
        .map_err(|e| format!("sao: {}: {e}", dir.display()))?;
    let bytes = std::fs::read(&wav);
    let _ = std::fs::remove_file(&wav);
    if !out.status.success() {
        return Err(format!("sao: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("failed")));
    }
    bytes.map_err(|e| format!("sao: {}: {e}", wav.display()))
}

/// What the the host app API is sent for a fish or lyria make(): the path and the JSON body, params passed through.
/// /v1/gen/speech hands its body to Fish's TTS, which reads `text` (and Fish's own fields, such as a voice's
/// reference_id); /v1/gen/music takes {prompt, kind: clip|song, lyrics}. Neither takes a seed: there the seed only
/// names another take in the cache.
pub fn ugc_request(model: &str, prompt: &str, params: &J) -> (&'static str, J) {
    let (path, field) = if model == "fish" { ("/v1/gen/speech", "text") } else { ("/v1/gen/music", "prompt") };
    let mut body = params.as_object().cloned().unwrap_or_default();
    body.insert(field.into(), json!(prompt));
    (path, J::Object(body))
}

/// Where a run's paid requests are counted: the file the host names in $RAEA_GEN_LEDGER (void: one per eval scenario,
/// one per agent turn), whole; else the edit's own assets/paid.jsonl over the last 24 hours. Every render is its
/// own process, so the count has to outlive one.
fn ledger(dir: &Path) -> (PathBuf, Option<u64>) {
    match std::env::var_os("RAEA_GEN_LEDGER").filter(|v| !v.is_empty()) {
        Some(p) => (PathBuf::from(p), None),
        None => (dir.join("paid.jsonl"), Some(24 * 3600)),
    }
}

fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Whether a paid request may go now. On by default and capped (the founder's decision, 2026-10-15): $RAEA_GEN_PAID=0
/// turns paid models off, and $RAEA_GEN_BUDGET=N sets the cap per run (default PAID_PER_RUN). A cached make() sends
/// nothing and counts nothing.
fn paid_allowed(model: &str, dir: &Path) -> Result<(), String> {
    if std::env::var("RAEA_GEN_PAID").as_deref() == Ok("0") {
        return Err(format!("{model} is paid generation, and the host turned it off (RAEA_GEN_PAID=0)"));
    }
    let n = std::env::var("RAEA_GEN_BUDGET").ok().and_then(|n| n.trim().parse::<usize>().ok()).unwrap_or(PAID_PER_RUN);
    let (file, window) = ledger(dir);
    let since = window.map(|w| now_s().saturating_sub(w)).unwrap_or(0);
    let spent = std::fs::read_to_string(&file).unwrap_or_default().lines()
        .filter_map(|l| serde_json::from_str::<J>(l).ok())
        .filter(|l| l["t"].as_u64().unwrap_or(0) >= since)
        .count();
    if spent >= n {
        return Err(format!("{model}: this run has used its {n} paid generations (RAEA_GEN_BUDGET); use sao, a cached take, or no sound"));
    }
    Ok(())
}

/// One paid request, counted when it's sent: a request that fails may still be billed.
fn count_paid(model: &str, dir: &Path) {
    let (file, _) = ledger(dir);
    if let Some(d) = file.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        use std::io::Write;
        let _ = writeln!(f, "{}", json!({"t": now_s(), "model": model}));
    }
}

/// A setting: the process environment first, then raea's settings file ($RAEA_ENV_FILE, else ~/.raea/env).
fn setting(name: &str) -> Option<String> {
    if let Some(v) = std::env::var(name).ok().filter(|v| !v.trim().is_empty()) {
        return Some(v.trim().to_string());
    }
    let file = std::env::var_os("RAEA_ENV_FILE").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".raea/env"));
    std::fs::read_to_string(file).ok()?.lines().find_map(|l| {
        let (k, v) = l.trim().split_once('=')?;
        (k.trim() == name && !l.trim_start().starts_with('#')).then(|| v.trim().trim_matches('"').to_string())
    }).filter(|v| !v.is_empty())
}

/// Speech or music from raea's generation API ($RAEA_GEN_API, else $RAEA_API: POST /v1/gen/music and /v1/gen/speech),
/// billed to whoever's bearer token the host put in $RAEA_GEN_TOKEN (else $RAEA_API_TOKEN): the audio and its extension,
/// from the response's content type.
fn ugc(model: &str, prompt: &str, params: &J, part: &Path) -> Result<(Vec<u8>, String), String> {
    let dir = part.parent().unwrap_or(Path::new("."));
    paid_allowed(model, dir)?;
    let token = setting("RAEA_GEN_TOKEN").or_else(|| setting("RAEA_API_TOKEN")).ok_or(format!("{model} needs a bearer token for raea's API in RAEA_GEN_TOKEN"))?;
    let api = setting("RAEA_GEN_API").or_else(|| setting("RAEA_API")).ok_or(format!("{model} needs raea's API: set RAEA_GEN_API or RAEA_API"))?;
    let (path, body) = ugc_request(model, prompt, params);
    count_paid(model, dir);
    let (code, mime) = post(&format!("{}{path}", api.trim_end_matches('/')), &format!("Authorization: Bearer {}", token.trim()), &body, part)?;
    let bytes = std::fs::read(part).map_err(|e| format!("{model}: {e}"))?;
    if !(200..300).contains(&code) {
        return Err(format!("{model}: {path} answered {code}: {}", String::from_utf8_lossy(&bytes).chars().take(200).collect::<String>()));
    }
    let ext = match mime.split(';').next().unwrap_or("").trim().strip_prefix("audio/") {
        Some("mpeg" | "mp3") => "mp3",
        Some("wav" | "x-wav" | "wave") => "wav",
        Some("mp4" | "x-m4a") => "m4a",
        Some(other) if !other.is_empty() => other,
        _ => return Err(format!("{model}: came back as {mime}, not audio")),
    };
    Ok((bytes, ext.to_string()))
}

/// A JSON POST through curl (rdb has no HTTP client). The URL, headers and body go to curl's stdin as its config, so a
/// key never shows in the process list; the response lands in `out`. (status, content type)
fn post(url: &str, auth: &str, body: &J, out: &Path) -> Result<(u16, String), String> {
    use std::io::Write;
    let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let config = format!(
        "url = {}\nrequest = \"POST\"\nheader = \"User-Agent: raea-rdb\"\nheader = \"Content-Type: application/json\"\nheader = {}\ndata-binary = {}\n",
        q(url),
        q(auth),
        q(&body.to_string())
    );
    let mut child = std::process::Command::new("curl")
        .args(["-sS", "-K", "-", "-o"])
        .arg(out)
        .args(["-w", "%{http_code} %{content_type}"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    child.stdin.take().ok_or("curl: no stdin")?.write_all(config.as_bytes()).map_err(|e| format!("curl: {e}"))?;
    let o = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
    if !o.status.success() {
        return Err(format!("curl: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    let w = String::from_utf8_lossy(&o.stdout).to_string();
    let (code, mime) = w.split_once(' ').unwrap_or((&w, ""));
    Ok((code.parse().unwrap_or(0), mime.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fish_and_lyria_requests_carry_the_prompt_and_params() {
        let (path, body) = ugc_request("fish", "Three drops, every morning.", &json!({"reference_id": "abc"}));
        assert_eq!(path, "/v1/gen/speech");
        assert_eq!(body, json!({"text": "Three drops, every morning.", "reference_id": "abc"}));
        let (path, body) = ugc_request("lyria", "warm lo-fi bed", &json!({"kind": "clip"}));
        assert_eq!(path, "/v1/gen/music");
        assert_eq!(body, json!({"prompt": "warm lo-fi bed", "kind": "clip"}));
    }

    #[test]
    fn paid_models_wait_for_the_host() {
        // the only test that touches these variables; every refusal comes before a request could be sent
        let s = Store { dir: std::env::temp_dir().join(format!("rdb-paid-{}", std::process::id())), library: PathBuf::new() };
        let ledger = s.dir.with_extension("ledger.jsonl");
        unsafe {
            std::env::remove_var("RAEA_GEN_PAID");
            std::env::remove_var("RAEA_GEN_TOKEN");
            std::env::remove_var("RAEA_GEN_BUDGET");
            std::env::set_var("RAEA_GEN_LEDGER", &ledger);
            std::env::set_var("RAEA_GEN_PAID", "0");
        }
        let e = s.make("lyria", "warm lo-fi bed", 1, &json!({})).unwrap_err();
        assert!(e.contains("turned it off (RAEA_GEN_PAID=0)"), "{e}");
        // on by default: without a token it still sends nothing
        unsafe { std::env::remove_var("RAEA_GEN_PAID") };
        let e = s.make("fish", "Three drops.", 1, &json!({})).unwrap_err();
        assert!(e.contains("RAEA_GEN_TOKEN"), "{e}");
        // a run that has spent its default cap is refused before a request could go
        let line = format!("{}\n", json!({"t": super::now_s(), "model": "fish"}));
        std::fs::write(&ledger, line.repeat(super::PAID_PER_RUN)).unwrap();
        let e = s.make("fish", "Three drops.", 1, &json!({})).unwrap_err();
        assert!(e.contains("used its 3 paid generations"), "{e}");
        unsafe { std::env::set_var("RAEA_GEN_BUDGET", "4") };
        let e = s.make("fish", "Three drops.", 1, &json!({})).unwrap_err();
        assert!(e.contains("RAEA_GEN_TOKEN"), "a larger budget lets it through to the token check: {e}");
        unsafe {
            std::env::remove_var("RAEA_GEN_BUDGET");
            std::env::remove_var("RAEA_GEN_LEDGER");
        }
        let _ = std::fs::remove_file(&ledger);
        assert_eq!(std::fs::read_dir(&s.dir).map(|d| d.count()).unwrap_or(0), 0, "nothing was kept");
    }

    #[test]
    fn the_key_ignores_param_order_but_not_seed_or_params() {
        let k = make_key("sao", "rain", 3, &json!({"seconds": 6, "a": 1}));
        assert_eq!(k, make_key("sao", "rain", 3, &json!({"a": 1, "seconds": 6})));
        assert_ne!(k, make_key("sao", "rain", 4, &json!({"a": 1, "seconds": 6})));
        assert_ne!(k, make_key("sao", "rain", 3, &json!({"a": 1, "seconds": 7})));
    }

    #[test]
    fn make_indexes_once_and_refuses_unknown_models() {
        let dir = std::env::temp_dir().join(format!("rdb-make-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = Store { dir: dir.clone(), library: PathBuf::new() };
        assert_eq!(s.made("stand-in", "a cat", 1, &json!({})).unwrap(), None);
        let a = s.make("stand-in", "a cat", 1, &json!({})).unwrap();
        assert_eq!(s.make("stand-in", "a cat", 1, &json!({})).unwrap(), a);
        assert_eq!(s.made("stand-in", "a cat", 1, &json!({})).unwrap(), Some(a));
        assert_eq!(std::fs::read_to_string(dir.join("made.jsonl")).unwrap().lines().count(), 1);
        assert!(s.make("dall-e", "a cat", 1, &json!({})).unwrap_err().contains("stand-in, sao, fish, lyria"));
    }
}
