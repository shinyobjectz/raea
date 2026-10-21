//! Check packs: previz and render checks as data, run here against an edit and its
//! render. A pack is a list of checks; each names what it looks at (its scope), who answers (a rule or clef-omni; a
//! "vote" check, once clef plus a writer model's votes, is clef alone since the founder's two-model rule), the question,
//! which perception facts ground it, its threshold and calibration, and the fix the agent should make. The packs live
//! in the folder RAEA_CHECK_PACKS names (default ~/.raea/checks/packs), shared with a check service when there is one
//! (RAEA_CHECKS_API), so both answer from the same files.
//!
//! The strategy follows what worked for ad review elsewhere (perception-grounded clef questions, questions as data,
//! scope by part of the video), written from scratch for raea.
//! What's ours: a check warns the agent only once calibration (calibrate()) has shown it separates good renders from
//! bad on judged examples; until then it's advisory and only recorded.

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Pack {
    pub pack: String,
    pub version: u32,
    #[serde(default)]
    pub about: String,
    pub checks: Vec<Check>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Check {
    pub id: String,
    pub scope: String,
    /// rule | clef | vote
    pub eye: String,
    /// for rules: which measurement
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// for clef and vote: yes (a probability) or score (0 poor .. 4 excellent)
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// a score faults below its threshold (a yes faults at or above it)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub below: bool,
    /// a yes that is good news (the product is on screen): faults below its threshold
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
    /// advisory | warn
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<Value>,
    /// words in a judge's defect that mean this check should have fired (calibration's labels)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub label_words: Vec<String>,
    pub fix: String,
    /// the native clef question, sent as written (the playtest DSL, sdk/motion/src/playtest.ts, compiles one into every
    /// model check; sdk/react/docs/playtest.md): {"type": "noul" | "choice" | "score", "instructions", "criteria"}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clef: Option<Value>,
    /// a "choice" check: the options whose probabilities together are its value (it faults at or above its threshold)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fault_options: Vec<String>,
    /// a "grade" (choice over levels, read as the expected level) or "rate" (clef's native score) check: its ordered
    /// levels, lowest first (it faults below its threshold, a level index)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub id: String,
    pub status: String,
    /// the answer: a probability, a score, or a count of faults for a rule
    pub value: f64,
    pub fault: bool,
    pub says: String,
    pub fix: String,
}

pub fn packs_dir() -> PathBuf {
    crate::brain::setting("RAEA_CHECK_PACKS").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".raea/checks/packs"))
}

pub fn load(name: &str) -> anyhow::Result<Pack> {
    let p = packs_dir().join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(&p).with_context(|| format!("no pack {}", p.display()))?).with_context(|| format!("pack {name}"))
}

pub fn save(pack: &Pack) -> anyhow::Result<()> {
    std::fs::write(packs_dir().join(format!("{}.json", pack.pack)), serde_json::to_string_pretty(pack)? + "\n")?;
    Ok(())
}

/// What a rule found, measured elsewhere and handed in (the gate has already rendered and cut-checked).
#[derive(Default)]
pub struct Measured {
    pub cuts: Option<String>,
}

/// Run a pack on a TypeScript project (`edit`: its folder or a file in it, edit.tsx) and its render; the findings and
/// what the model calls cost. `only` keeps those check ids.
pub async fn run(pack: &Pack, edit: &Path, video: &Path, measured: &Measured, only: Option<&[String]>) -> (Vec<Finding>, f64) {
    // the check service (RAEA_CHECKS_API) when one is set: perception's facts and the media go to it, the answers come back
    if let Ok(api) = std::env::var("RAEA_CHECKS_API") {
        match remote(&api, pack, edit, video, measured, only).await {
            Ok(r) => return r,
            Err(e) => eprintln!("checks: the service failed ({e}); answering here"),
        }
    }
    let mut out = Vec::new();
    let mut cost = 0.0;
    let checks: Vec<&Check> = pack.checks.iter().filter(|c| only.is_none_or(|o| o.contains(&c.id))).collect();
    // rules
    for c in checks.iter().filter(|c| c.eye == "rule") {
        let lines = rule(c.rule.as_deref().unwrap_or(""), edit, video, measured).await;
        out.push(Finding { id: c.id.clone(), status: c.status.clone(), value: lines.len() as f64, fault: !lines.is_empty(), says: lines.join("\n"), fix: c.fix.clone() });
    }
    // model checks, one clef call per scope and context: a check's answer depends on the paragraph it reads, so a
    // calibrated check keeps exactly the context it was calibrated with, whatever else the pack asks
    let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
    for c in checks.iter().filter(|c| c.eye != "rule") {
        let mut k = c.context.clone();
        k.sort();
        if !groups.iter().any(|g| g.0 == c.scope && g.1 == k) { groups.push((c.scope.as_str(), k)) }
    }
    // each scope shrunk once, all at once; then every group's clef call at once (one after another, a full check
    // spent ~17 s on the network)
    let scopes: std::collections::BTreeSet<&str> = groups.iter().map(|g| g.0).collect();
    let shrunk: std::collections::HashMap<&str, Vec<u8>> = futures_util::future::join_all(scopes.into_iter().map(|scope| async move { (scope, media_for(scope, edit, video).await.ok()) }))
        .await.into_iter().filter_map(|(s, m)| Some((s, m?))).collect();
    let jobs = groups.iter().filter_map(|(scope, ctx)| {
        let media = shrunk.get(scope)?.clone();
        let these: Vec<&Check> = checks.iter().filter(|c| c.eye != "rule" && c.scope == *scope && { let mut k = c.context.clone(); k.sort(); k == *ctx }).copied().collect();
        Some(async move {
            let keys: std::collections::BTreeSet<&str> = ctx.iter().map(String::as_str).collect();
            let state = context(&keys, scope, edit, video).await;
            let mut found = Vec::new();
            let mut cost = 0.0;
            match clef(&media, *scope == "sheet", &state, &these, pack.version).await {
                Ok((answers, c)) => {
                    cost += c;
                    for ch in &these {
                        // a vote is clef's answer alone: the writer's votes (a second model) went with the founder's
                        // two-model rule (2026-10-18), and calibration had only ever measured clef's half
                        let Some(v) = answers.get(&ch.id).copied() else { continue };
                        found.push(finding(ch, v));
                    }
                }
                Err(e) => found.push(Finding { id: format!("{scope}.unavailable"), status: "advisory".into(), value: 0.0, fault: false, says: format!("clef couldn't answer: {e}"), fix: String::new() }),
            }
            (found, cost)
        })
    });
    for (found, c) in futures_util::future::join_all(jobs).await {
        out.extend(found);
        cost += c;
    }
    (out, cost)
}

fn finding(c: &Check, v: f64) -> Finding {
    let score = matches!(c.kind.as_deref(), Some("score" | "grade" | "rate"));
    let top = if c.levels.is_empty() { 4 } else { c.levels.len() - 1 };
    let t = c.threshold.unwrap_or(if score { 2.0 } else { 0.5 });
    let fault = if score || c.below || c.invert { v < t } else { v >= t };
    let shown = if score { format!("{v:.1} of {top}") } else { format!("p {v:.2}") };
    Finding { id: c.id.clone(), status: c.status.clone(), value: v, fault,
        says: format!("{} ({shown})", c.ask.clone().unwrap_or_default()), fix: c.fix.clone() }
}

async fn rule(name: &str, edit: &Path, video: &Path, m: &Measured) -> Vec<String> {
    let root = crate::host::root_of(edit);
    match name {
        "cuts" => match &m.cuts {
            Some(t) => if t.starts_with("PASS") { vec![] } else { t.lines().skip(1).map(|l| l.trim_start_matches("- ").to_string()).collect() },
            None => match crate::host::measure_cuts(&root, video, true).await {
                Ok(v) => v["issues"].as_array().into_iter().flatten().filter_map(|i| i["says"].as_str().map(String::from)).collect(),
                Err(e) => vec![format!("couldn't measure the cuts: {e}")],
            },
        },
        // text over the face, off the frame, the speaker out of the picture: read from the render's manifest
        "face_layout" => crate::host::face_faults(&root, video).await,
        "legible" => {
            let fps = crate::host::shape(&root).fps;
            let frames = render_frames(video, fps).await;
            crate::legible::video_faults(video, frames, fps).await
        }
        "speech_starts" => {
            let fps = crate::host::shape(&root).fps;
            match crate::host::spoken_words(&root).first() {
                Some((_, f)) if *f as f64 / fps > 0.6 => vec![format!("the first word comes {:.1} s in", *f as f64 / fps)],
                _ => vec![],
            }
        }
        _ => vec![format!("unknown rule {name}")],
    }
}

/// A render's length in frames at `fps` (rdb media probe).
async fn render_frames(video: &Path, fps: f64) -> i64 {
    let out = crate::stage::rdb(vec!["media".into(), "probe".into(), video.to_string_lossy().into()]).await.unwrap_or_default();
    let secs = serde_json::from_str::<Value>(&out).ok().and_then(|v| v["duration"].as_f64()).unwrap_or(0.0);
    (secs * fps).round() as i64
}

/// The media a scope looks at, shrunk the way clef was calibrated (x264 at 480 wide, crf 30): the whole render up to
/// 59 s, or its first 3 s for the hook.
async fn media_for(scope: &str, edit: &Path, video: &Path) -> anyhow::Result<Vec<u8>> {
    // the storyboard (a previz sheet of sketch frames, before anything renders) came from the retired rrl's previz; a
    // TypeScript project has none, so the sheet scope's checks don't run
    anyhow::ensure!(scope != "sheet", "no storyboard sheet for a TypeScript project ({})", edit.display());
    shrink(video, if scope == "hook" { 3 } else { 59 }).await
}

/// A pack through the check service: rules measured here (they need the edit and perception), the media each scope looks
/// at, and the facts its context reads; the service batches, caches, votes and meters.
async fn remote(api: &str, pack: &Pack, edit: &Path, video: &Path, measured: &Measured, only: Option<&[String]>) -> anyhow::Result<(Vec<Finding>, f64)> {
    use base64::Engine as _;
    let token = std::env::var("RAEA_CHECKS_TOKEN").or_else(|_| std::env::var("RAEA_GEN_TOKEN")).map_err(|_| anyhow!("no token (RAEA_CHECKS_TOKEN)"))?;
    let checks: Vec<&Check> = pack.checks.iter().filter(|c| only.is_none_or(|o| o.contains(&c.id))).collect();
    let mut rules = serde_json::Map::new();
    for c in checks.iter().filter(|c| c.eye == "rule") {
        rules.insert(c.id.clone(), json!(rule(c.rule.as_deref().unwrap_or(""), edit, video, measured).await));
    }
    let mut videos = serde_json::Map::new();
    let mut images = serde_json::Map::new();
    let scopes: std::collections::BTreeSet<&str> = checks.iter().filter(|c| c.eye != "rule").map(|c| c.scope.as_str()).collect();
    for scope in &scopes {
        let b = base64::engine::general_purpose::STANDARD.encode(media_for(scope, edit, video).await?);
        if *scope == "sheet" { images.insert(scope.to_string(), json!([b])); } else { videos.insert(scope.to_string(), json!(b)); }
    }
    let keys: std::collections::BTreeSet<&str> = checks.iter().flat_map(|c| c.context.iter().map(String::as_str)).collect();
    let facts = facts(&keys, scopes.iter().next().copied().unwrap_or("render"), edit, video).await;
    let body = json!({"pack": pack.pack, "only": checks.iter().map(|c| c.id.clone()).collect::<Vec<_>>(), "media": {"video": videos, "images": images},
        "facts": {"transcript": facts.transcript, "ocr": facts.ocr, "brand": facts.brand, "platform": facts.platform, "rules": rules}});
    let res = reqwest::Client::new().post(format!("{}/v1/checks/run", api.trim_end_matches('/'))).bearer_auth(token).json(&body)
        .timeout(std::time::Duration::from_secs(300)).send().await?;
    let status = res.status();
    let v: Value = res.json().await?;
    anyhow::ensure!(status.is_success(), "{status}: {}", v["error"]);
    let found = v["findings"].as_array().into_iter().flatten().map(|f| Finding {
        id: f["id"].as_str().unwrap_or("").into(), status: f["status"].as_str().unwrap_or("advisory").into(), value: f["value"].as_f64().unwrap_or(0.0),
        fault: f["fault"].as_bool().unwrap_or(false), says: f["says"].as_str().unwrap_or("").into(), fix: f["fix"].as_str().unwrap_or("").into(),
    }).collect();
    Ok((found, v["cost_usd"].as_f64().unwrap_or(0.0)))
}

/// What perception knows about an edit and its render, as facts (the service builds the paragraph from them; here
/// context() does, with the same sentences).
#[derive(Default)]
struct Facts {
    transcript: Option<String>,
    ocr: Option<Vec<String>>,
    brand: Option<String>,
    platform: Option<String>,
}

async fn facts(keys: &std::collections::BTreeSet<&str>, scope: &str, edit: &Path, video: &Path) -> Facts {
    let mut f = Facts::default();
    let root = crate::host::root_of(edit);
    let sh = crate::host::shape(&root);
    if keys.contains("platform") {
        f.platform = Some(if sh.width < sh.height { format!("It is vertical ({}x{}), for TikTok, Reels or Shorts, where the platform's buttons cover the right edge and the bottom fifth.", sh.width, sh.height) } else { format!("It is {}x{}, for a feed.", sh.width, sh.height) });
    }
    if keys.contains("transcript") {
        let fps = sh.fps;
        let words: Vec<String> = crate::host::spoken_words(&root).into_iter()
            .filter(|w| scope != "hook" || (w.1 as f64) < 3.0 * fps).map(|w| w.0).collect();
        if !words.is_empty() { f.transcript = Some(words.join(" ").chars().take(3000).collect()) }
    }
    if keys.contains("ocr") {
        let read = crate::legible::read_text(video, &[0.5, 1.5, 2.5].iter().map(|t| if scope == "hook" { *t } else { t * 8.0 }).collect::<Vec<_>>()).await;
        if !read.is_empty() { f.ocr = Some(read) }
    }
    if keys.contains("brand") {
        if let Ok(b) = std::fs::read_to_string(root.join("BRAND.md")) {
            f.brand = Some(b.lines().filter(|l| !l.trim().is_empty()).take(6).collect::<Vec<_>>().join(" ").chars().take(500).collect());
        }
    }
    f
}

/// The paragraph clef reads before the video: what perception knows that the pixels alone don't say.
async fn context(keys: &std::collections::BTreeSet<&str>, scope: &str, edit: &Path, video: &Path) -> String {
    let f = facts(keys, scope, edit, video).await;
    let mut s = vec![if scope == "hook" { "The first three seconds of a short social video edited from real footage.".to_string() } else { "A short social video edited from a creator's real footage.".to_string() }];
    if let Some(p) = f.platform { s.push(p) }
    if let Some(t) = f.transcript { s.push(format!("What is said: \"{t}\"")) }
    if let Some(o) = f.ocr { s.push(format!("On-screen text read from sample frames: {}", o.join(" | "))) }
    if let Some(b) = f.brand { s.push(format!("The brand: {b}")) }
    s.join("\n")
}

fn sha(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn cache_dir() -> PathBuf {
    crate::spaces::home().join("checks-cache")
}

pub const LEVELS: [&str; 5] = ["poor", "weak", "acceptable", "strong", "excellent"];

/// One clef-omni call for the checks of one scope: yes checks as probabilities, score checks as the expected level
/// (0..4) over its five. Cached by the media, the context, the pack version and the questions.
async fn clef(media: &[u8], still: bool, state: &str, checks: &[&Check], version: u32) -> anyhow::Result<(std::collections::HashMap<String, f64>, f64)> {
    let mut questions = serde_json::Map::new();
    for c in checks {
        let id = c.id.replace(|ch: char| !(ch.is_ascii_alphanumeric() || "_.-".contains(ch)), "_");
        let ask = c.ask.clone().unwrap_or_default();
        questions.insert(id, if let Some(q) = &c.clef {
            q.clone()
        } else if c.kind.as_deref() == Some("score") {
            json!({"type": "choice", "instructions": ask, "criteria": LEVELS.iter().map(|l| (l.to_string(), Value::Null)).collect::<serde_json::Map<_, _>>()})
        } else {
            json!({"type": "noul", "instructions": ask})
        });
    }
    let media = if still { Media::Images(vec![media]) } else { Media::Videos(vec![media]) };
    let (a, cost) = ask(&media, state, &questions, &version.to_string()).await?;
    let mut out = std::collections::HashMap::new();
    for c in checks {
        let id = c.id.replace(|ch: char| !(ch.is_ascii_alphanumeric() || "_.-".contains(ch)), "_");
        let ans = &a[&id];
        out.insert(c.id.clone(), value(c, ans));
    }
    Ok((out, cost))
}

/// A check's value from its clef answer, read as the playtest DSL's read() reads it (sdk/motion/src/playtest.ts).
fn value(c: &Check, ans: &Value) -> f64 {
    let levels: Vec<&str> = c.levels.iter().map(String::as_str).collect();
    match c.kind.as_deref() {
        Some("score") => expected(ans, &LEVELS).unwrap_or(2.0),
        // the probability on the options that count as a fault
        Some("choice") => c.fault_options.iter().map(|o| ans["probabilities"][o.as_str()].as_f64().unwrap_or(0.0)).sum(),
        Some("grade") => expected(ans, &levels).unwrap_or(0.0),
        // clef's native score: the probability-weighted level
        Some("rate") => ans["score"].as_f64().unwrap_or(0.0),
        _ => ans["noul"].as_f64().unwrap_or(0.0),
    }
}

/// What a clef-omni call looks at: whole videos (mp4) or stills (jpeg), in the order given.
pub enum Media<'a> {
    Videos(Vec<&'a [u8]>),
    Images(Vec<&'a [u8]>),
    /// videos (at most 2) with their soundtracks also sent as audio inputs (at most 4, MP3), in the same order, so the
    /// sound is heard even when one video has no track (clef hears a video's sound only when every video has one)
    Clips(Vec<&'a [u8]>, Vec<&'a [u8]>),
}

/// One clef-omni call (the decision model: every judge, reviewer, check and scorer; the founder, 2026-10-18): `questions`
/// keyed by id, each {"type": "choice", "instructions", "criteria": {option: description or null}} (answered with
/// "probabilities" over the options and the top "choice"), {"type": "noul", "instructions"} (answered with "noul",
/// the probability of yes) or {"type": "score", "instructions", "criteria": [level, ...]} (2 to 10 levels, lowest first,
/// as an array: a map is refused; answered with "score", the probability-weighted level, and "probabilities" by level
/// index), about `media` after reading `state`. Returns clef's "answers" and the call's cost (input
/// tokens at $0.15 a million; output is free). Cached by the media, the state, `salt` (a pack version, a rubric's
/// hash) and the questions, under ~/.raea-void/checks-cache.
pub async fn ask(media: &Media<'_>, state: &str, questions: &serde_json::Map<String, Value>, salt: &str) -> anyhow::Result<(Value, f64)> {
    use base64::Engine as _;
    let none: Vec<&[u8]> = Vec::new();
    let (kind, items, mime, sounds) = match media {
        Media::Videos(v) => ("videos", v, "video/mp4", &none),
        Media::Images(v) => ("images", v, "image/jpeg", &none),
        Media::Clips(v, a) => ("videos", v, "video/mp4", a),
    };
    let media_key = items.iter().chain(sounds.iter()).map(|b| sha(b)).collect::<Vec<_>>().join(",");
    let key = sha(format!("{media_key}|{state}|{salt}|{}", Value::Object(questions.clone())).as_bytes());
    let cached = cache_dir().join(format!("{key}.json"));
    if let Some(v) = std::fs::read_to_string(&cached).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        return Ok((v, 0.0));
    }
    // clef-omni runs on Cloudflare Workers AI, in the account these two settings name (environment or ~/.raea/env)
    let get = |k: &str| crate::brain::setting(k).ok_or_else(|| anyhow!("{k} is not set: the checks' judge (clef-omni on Workers AI) needs CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN"));
    let (acct, token) = (get("CLOUDFLARE_ACCOUNT_ID")?, get("CLOUDFLARE_API_TOKEN")?);
    let urls: Vec<String> = items.iter().map(|b| format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(b))).collect();
    let mut body = json!({"model": "clef-omni", "state": state, "questions": questions});
    body[kind] = json!(urls);
    if !sounds.is_empty() {
        body["audio"] = json!(sounds.iter().map(|b| format!("data:audio/mpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(b))).collect::<Vec<_>>());
    }
    let res: Value = reqwest::Client::new().post(format!("https://api.cloudflare.com/client/v4/accounts/{acct}/ai/run/@cf/cloudflare/clef-omni"))
        .bearer_auth(token).json(&body).timeout(std::time::Duration::from_secs(300)).send().await?.json().await?;
    anyhow::ensure!(res["success"] == true, "clef: {}", res["errors"]);
    let cost = res["result"]["usage"]["input_tokens"].as_f64().unwrap_or(0.0) * 0.15 / 1e6;
    let a = res["result"]["answers"].clone();
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&cached, a.to_string());
    Ok((a, cost))
}

/// A choice answer's expected position among `options` (0 for the first): the probability-weighted index, or the
/// top choice's index when clef gave no probabilities. None when the answer names none of them.
pub fn expected(ans: &Value, options: &[&str]) -> Option<f64> {
    weighted(ans, &options.iter().enumerate().map(|(i, l)| (*l, i as f64)).collect::<Vec<_>>())
}

/// A choice answer's probability-weighted value over `options` (each option's name and value), renormalized over
/// those options; the top choice's value when clef gave no probabilities. None when the answer names none of them.
pub fn weighted(ans: &Value, options: &[(&str, f64)]) -> Option<f64> {
    let probs = &ans["probabilities"];
    let total: f64 = options.iter().map(|o| probs[o.0].as_f64().unwrap_or(0.0)).sum();
    if total > 0.0 {
        return Some(options.iter().map(|o| o.1 * probs[o.0].as_f64().unwrap_or(0.0)).sum::<f64>() / total);
    }
    options.iter().find(|o| Some(o.0) == ans["choice"].as_str()).map(|o| o.1)
}

/// A render shrunk the way clef was calibrated (x264 at 480 wide, crf 30, AAC at 64k): its first `secs` seconds.
pub async fn shrink(video: &Path, secs: u32) -> anyhow::Result<Vec<u8>> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let small = std::env::temp_dir().join(format!("raea-clef-{}-{}.mp4", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let ok = tokio::process::Command::new("ffmpeg").args(["-nostdin", "-v", "error", "-y", "-t", &secs.to_string(), "-i"]).arg(video)
        .args(["-vf", "scale=480:-2", "-c:v", "libx264", "-crf", "30", "-c:a", "aac", "-b:a", "64k"]).arg(&small).status().await?.success();
    anyhow::ensure!(ok, "ffmpeg couldn't shrink {}", video.display());
    let bytes = std::fs::read(&small)?;
    let _ = std::fs::remove_file(&small);
    Ok(bytes)
}

/// A render's soundtrack as clef hears it: its first `secs` seconds as mono MP3 at 64k. A render with no sound track is
/// sent as that much silence, so "no sound" is heard as silence rather than left out.
pub async fn sound(video: &Path, secs: u32) -> anyhow::Result<Vec<u8>> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let out = std::env::temp_dir().join(format!("raea-clef-{}-{}.mp3", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let probe = tokio::process::Command::new("ffprobe").args(["-v", "error", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0"]).arg(video).output().await?;
    let has_sound = !String::from_utf8_lossy(&probe.stdout).trim().is_empty();
    let mut c = tokio::process::Command::new("ffmpeg");
    c.args(["-nostdin", "-v", "error", "-y"]);
    if has_sound {
        c.args(["-t", &secs.to_string(), "-i"]).arg(video).arg("-vn");
    } else {
        let d = tokio::process::Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]).arg(video).output().await?;
        let d: f64 = String::from_utf8_lossy(&d.stdout).trim().parse().unwrap_or(1.0);
        c.args(["-f", "lavfi", "-t", &format!("{:.2}", d.min(secs as f64).max(0.5)), "-i", "anullsrc=r=16000:cl=mono"]);
    }
    let ok = c.args(["-ac", "1", "-ar", "16000", "-c:a", "libmp3lame", "-b:a", "64k"]).arg(&out).status().await?.success();
    anyhow::ensure!(ok, "ffmpeg couldn't take the sound of {}", video.display());
    let bytes = std::fs::read(&out)?;
    let _ = std::fs::remove_file(&out);
    Ok(bytes)
}

/// Whether a render is silent, measured: no sound track, or one whose loudest moment is under -50 dBFS in its first
/// `secs` seconds. clef doesn't reliably hear the difference (2026-10-18: an ad with its voiceover and the same ad with
/// no track drew -0.10 in a pair, the wrong way, and its sound questions came back n/a), so the judges take it as a fact.
pub async fn silent(video: &Path, secs: u32) -> bool {
    let Ok(o) = tokio::process::Command::new("ffmpeg").args(["-nostdin", "-v", "info", "-t", &secs.to_string(), "-i"]).arg(video)
        .args(["-vn", "-af", "volumedetect", "-f", "null", "-"]).output().await else { return false };
    let log = String::from_utf8_lossy(&o.stderr);
    match log.split("max_volume:").nth(1).and_then(|s| s.split_whitespace().next()).and_then(|v| v.parse::<f64>().ok()) {
        Some(db) => db < -50.0,
        // no audio stream: ffmpeg says the output has no stream, and volumedetect prints nothing
        None => true,
    }
}

/// One judged render: the space's edit, its render, and the judge's defects (lowercased).
struct Judged {
    edit: PathBuf,
    video: PathBuf,
    defects: Vec<String>,
}

fn judged(runs: &Path) -> Vec<Judged> {
    let mut out = Vec::new();
    for run in std::fs::read_dir(runs).into_iter().flatten().flatten() {
        for sc in std::fs::read_dir(run.path()).into_iter().flatten().flatten() {
            let (video, result) = (sc.path().join("final.mp4"), sc.path().join("result.json"));
            let Some(r) = std::fs::read_to_string(&result).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
            // the judge is stored as JSON or as a Python-style dict string
            let (j, raw) = match &r["judge"] {
                Value::String(s) => (serde_json::from_str::<Value>(s).unwrap_or(Value::Null), s.clone()),
                v => (v.clone(), String::new()),
            };
            let mut defects: Vec<String> = j["defects"].as_array().into_iter().flatten().filter_map(|d| d.as_str().map(|s| s.to_lowercase())).collect();
            // a Python-style dict: its defects list as one lowercase blob (labels match by substring)
            if defects.is_empty() {
                if let Some(at) = raw.find("'defects': [").or_else(|| raw.find("\"defects\": [")) {
                    let rest = &raw[at + 12..];
                    defects.push(rest[..rest.find("], '").or_else(|| rest.find("], \"")).unwrap_or(rest.len())].to_lowercase());
                }
            }
            let scored = j["overall"].is_number() || raw.contains("'overall'");
            // the scenario's project (rules read its cuts.json and render.json)
            let edit = sc.path().join("spaces/eval");
            if video.exists() && edit.join("edit.tsx").exists() && (!defects.is_empty() || scored) {
                out.push(Judged { edit, video, defects });
            }
        }
    }
    out.sort_by(|a, b| a.video.cmp(&b.video));
    out
}

/// AUC of `scores` (higher = more faulty) against `labels`, and the cutoff with the most true positives while false
/// alarms stay at or under 10%.
fn auc_and_cut(pairs: &[(f64, bool)]) -> (f64, f64, usize) {
    let pos: Vec<f64> = pairs.iter().filter(|p| p.1).map(|p| p.0).collect();
    let neg: Vec<f64> = pairs.iter().filter(|p| !p.1).map(|p| p.0).collect();
    if pos.is_empty() || neg.is_empty() {
        return (0.5, f64::NAN, pos.len());
    }
    let mut wins = 0.0;
    for p in &pos {
        for n in &neg {
            wins += if p > n { 1.0 } else if p == n { 0.5 } else { 0.0 };
        }
    }
    let auc = wins / (pos.len() * neg.len()) as f64;
    let mut cuts: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    cuts.sort_by(f64::total_cmp);
    let mut best = (f64::NAN, 0usize);
    for c in cuts {
        let fp = neg.iter().filter(|n| **n >= c).count() as f64 / neg.len() as f64;
        let tp = pos.iter().filter(|p| **p >= c).count();
        if fp <= 0.10 && tp > best.1 { best = (c, tp) }
    }
    (auc, best.0, pos.len())
}

/// Calibrate a pack's model checks on the judged renders under `runs`: each check's answers against whether the judge
/// named its defect (its label_words). A check warns from here on only with AUC 0.75 or better, at least 20 renders
/// and 5 positives; the rest are advisory. Writes the numbers into the pack, the per-render answers beside the runs
/// (they quote client footage, so never into a repo). A vote is clef's answer alone, here as in run().
pub async fn calibrate(name: &str, runs: &Path, jobs: usize) -> anyhow::Result<String> {
    let mut pack = load(name)?;
    let items = judged(runs);
    let model: Vec<String> = pack.checks.iter().filter(|c| c.eye != "rule" && !c.label_words.is_empty()).map(|c| c.id.clone()).collect();
    let clef_only: Pack = Pack { checks: pack.checks.iter().map(|c| { let mut c = c.clone(); if c.eye == "vote" { c.eye = "clef".into() } c }).collect(), ..pack.clone() };
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(jobs.max(1)));
    let mut set = tokio::task::JoinSet::new();
    for (i, it) in items.iter().enumerate() {
        let (p, m, edit, video, sem) = (clef_only.clone(), model.clone(), it.edit.clone(), it.video.clone(), sem.clone());
        set.spawn(async move {
            let _g = sem.acquire().await;
            let (f, cost) = run(&p, &edit, &video, &Measured::default(), Some(&m)).await;
            (i, f, cost)
        });
    }
    let mut answers: Vec<Option<Vec<Finding>>> = vec![None; items.len()];
    let mut cost = 0.0;
    while let Some(Ok((i, f, c))) = set.join_next().await {
        cost += c;
        answers[i] = Some(f);
    }
    let log = runs.join(format!("calibration-{name}-v{}.jsonl", pack.version));
    let mut lines = String::new();
    let mut report = vec![format!("{name} v{}: {} judged renders, ${cost:.3}", pack.version, items.len())];
    let today = std::process::Command::new("date").arg("+%F").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    for c in pack.checks.iter_mut().filter(|c| model.contains(&c.id)) {
        let score = c.kind.as_deref() == Some("score");
        let flip = score || c.below || c.invert;
        let mut pairs = Vec::new();
        for (it, a) in items.iter().zip(&answers) {
            let Some(f) = a.as_ref().and_then(|a| a.iter().find(|f| f.id == c.id)) else { continue };
            let label = it.defects.iter().any(|d| c.label_words.iter().any(|w| d.contains(&w.to_lowercase())));
            pairs.push((if flip { -f.value } else { f.value }, label));
            lines += &format!("{}\n", json!({"check": c.id, "video": it.video, "value": f.value, "label": label}));
        }
        let (auc, cut, npos) = auc_and_cut(&pairs);
        let warn = auc >= 0.75 && pairs.len() >= 20 && npos >= 5 && cut.is_finite();
        if warn {
            c.threshold = Some(if flip { -cut } else { cut });
        }
        c.status = if warn { "warn".into() } else { "advisory".into() };
        c.calibration = Some(json!({"auc": (auc * 100.0).round() / 100.0, "n": pairs.len(), "positives": npos, "date": today,
            "threshold_at_fpr10": if cut.is_finite() { json!(if flip { -cut } else { cut }) } else { Value::Null }}));
        report.push(format!("{:24} AUC {:.2}  n {:3}  positives {:3}  -> {}{}", c.id, auc, pairs.len(), npos, c.status,
            if warn { format!(" at {:.2}", c.threshold.unwrap_or(0.0)) } else { String::new() }));
    }
    std::fs::write(&log, lines)?;
    save(&pack)?;
    report.push(format!("per-render answers: {}", log.display()));
    Ok(report.join("\n"))
}

#[cfg(test)]
mod playtest_dsl {
    use super::*;

    /// A pack the playtest DSL compiled (sdk/react/bench/playtest/packs/game.json) parses, carries its native questions,
    /// and choice, grade and rate answers read as the DSL reads them.
    #[test]
    fn a_compiled_pack_parses_and_its_answers_read() {
        let file = concat!(env!("CARGO_MANIFEST_DIR"), "/../react/bench/playtest/packs/game.json");
        let pack: Pack = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(pack.checks.len(), 7);
        let looks = pack.checks.iter().find(|c| c.id == "session.looks").unwrap();
        assert_eq!(looks.clef.as_ref().unwrap()["type"], "choice");
        assert!((value(looks, &json!({"probabilities": {"poor": 0.5, "acceptable": 0.5}})) - 1.0).abs() < 1e-9);
        let mut c = looks.clone();
        c.kind = Some("choice".into());
        c.fault_options = vec!["poor".into(), "weak".into()];
        assert!((value(&c, &json!({"choice": "acceptable", "probabilities": {"poor": 0.2, "weak": 0.3, "acceptable": 0.5}})) - 0.5).abs() < 1e-9);
        c.kind = Some("grade".into());
        c.levels = vec!["low".into(), "high".into()];
        assert!((value(&c, &json!({"probabilities": {"low": 0.25, "high": 0.75}})) - 0.75).abs() < 1e-9);
        c.kind = Some("rate".into());
        assert!((value(&c, &json!({"type": "score", "score": 1.97})) - 1.97).abs() < 1e-9);
        assert!(finding(&Check { threshold: Some(1.0), below: true, ..c.clone() }, 0.5).fault);
    }
}
