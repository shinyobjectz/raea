//! The brain: one conversation with a model that works on raea through a few tools: the shell over the open space's
//! project (with the `raea` CLI in it, host.rs doing the machine's side) and eyes (look). Anthropic Messages, streamed,
//! so the voice starts on the first clause.
//!
//! The model: by default GLM 5.3 Flash on OpenRouter. The founder, 2026-10-18: "we want only two models ever used right
//! now. The primary model and the decision model. which is glm and clef omni": GLM is the agent and every writer,
//! clef-omni (checks.rs) every judge, reviewer, check and scorer. Anyone else picks their own provider with
//! RAEA_LLM_PROVIDER (see `Provider::resolve`); nothing falls back silently: a missing key fails the turn and says
//! which setting to add. Settings come from the environment or ~/.raea/env (`setting`).

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context};
use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::spaces::{self, Space};

#[derive(Debug, Clone)]
pub struct Provider {
    pub base: String,
    pub key: String,
    pub model: String,
    pub label: String,
    /// Send the key as `Authorization: Bearer` (the Anthropic-compatible endpoints of Z.ai, DeepSeek and Moonshot)
    /// rather than `x-api-key` (Anthropic and OpenRouter).
    pub bearer: bool,
}

/// The primary model: the agent and every writer (the founder, 2026-10-17: "lets pivot to using glm 5.3 flash for now, not
/// mimo"; fast, cheap, and it sees images, as `look` needs).
pub const PRIMARY: &str = "z-ai/glm-5.3-flash";

/// A setting: the process environment first, then raea's settings file ($RAEA_ENV_FILE, else ~/.raea/env: KEY=value
/// lines, # comments, optional quotes). Empty values count as unset.
pub(crate) fn setting(name: &str) -> Option<String> {
    if let Some(v) = std::env::var(name).ok().filter(|v| !v.trim().is_empty()) {
        return Some(v.trim().to_string());
    }
    let file = std::env::var_os("RAEA_ENV_FILE").map(std::path::PathBuf::from).unwrap_or_else(|| {
        std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".raea/env")
    });
    std::fs::read_to_string(file).ok()?.lines().find_map(|l| {
        let (k, v) = l.trim().split_once('=')?;
        (k.trim() == name && !l.trim_start().starts_with('#')).then(|| v.trim().trim_matches('"').to_string())
    }).filter(|v| !v.is_empty())
}

/// The OpenRouter key (OPENROUTER_API_KEY).
pub(crate) fn openrouter_key() -> Option<String> {
    setting("OPENROUTER_API_KEY")
}

/// raea's API (RAEA_API): brand kits, sound generation, the eval session and remote runners. No default: a host points
/// it at its own deployment (README: "Services").
pub(crate) fn raea_api() -> anyhow::Result<String> {
    setting("RAEA_API").map(|a| a.trim_end_matches('/').to_string()).ok_or_else(|| anyhow!("RAEA_API is not set: point it at your raea API (see the README's Services)"))
}

impl Provider {
    /// The agent's model, chosen by RAEA_LLM_PROVIDER. Every provider here serves the Anthropic Messages API, which is
    /// what the brain streams:
    ///   openrouter (default)  https://openrouter.ai/api       OPENROUTER_API_KEY   model default z-ai/glm-5.3-flash
    ///   anthropic             https://api.anthropic.com       ANTHROPIC_API_KEY
    ///   zai                   https://api.z.ai/api/anthropic  ZAI_API_KEY
    ///   deepseek              https://api.deepseek.com/anthropic  DEEPSEEK_API_KEY
    ///   moonshot              https://api.moonshot.ai/anthropic   MOONSHOT_API_KEY
    ///   custom                RAEA_LLM_BASE_URL (required)
    /// RAEA_LLM_MODEL names the model (required except on openrouter; RAEA_VOID_MODEL is the older name), and
    /// RAEA_LLM_API_KEY overrides the provider's own key variable. A missing key is an error, never a fallback.
    pub async fn resolve() -> anyhow::Result<Self> {
        let name = setting("RAEA_LLM_PROVIDER").unwrap_or_else(|| "openrouter".into()).to_lowercase();
        let (base, key_var, bearer, label) = match name.as_str() {
            "openrouter" => ("https://openrouter.ai/api".to_string(), "OPENROUTER_API_KEY", false, "OpenRouter"),
            "anthropic" => ("https://api.anthropic.com".to_string(), "ANTHROPIC_API_KEY", false, "Anthropic"),
            "zai" => ("https://api.z.ai/api/anthropic".to_string(), "ZAI_API_KEY", true, "Z.ai"),
            "deepseek" => ("https://api.deepseek.com/anthropic".to_string(), "DEEPSEEK_API_KEY", true, "DeepSeek"),
            "moonshot" => ("https://api.moonshot.ai/anthropic".to_string(), "MOONSHOT_API_KEY", true, "Moonshot"),
            "custom" => (
                setting("RAEA_LLM_BASE_URL").ok_or_else(|| anyhow!("RAEA_LLM_PROVIDER=custom needs RAEA_LLM_BASE_URL (an Anthropic Messages endpoint)"))?,
                "RAEA_LLM_API_KEY",
                setting("RAEA_LLM_AUTH").as_deref() == Some("bearer"),
                "custom",
            ),
            other => anyhow::bail!("RAEA_LLM_PROVIDER={other}: use openrouter, anthropic, zai, deepseek, moonshot or custom"),
        };
        let model = setting("RAEA_LLM_MODEL").or_else(|| setting("RAEA_VOID_MODEL"));
        let model = match (model, name.as_str()) {
            (Some(m), _) => m,
            (None, "openrouter") => PRIMARY.to_string(),
            (None, _) => anyhow::bail!("RAEA_LLM_PROVIDER={name} needs RAEA_LLM_MODEL (the model's name at {label})"),
        };
        let key = setting("RAEA_LLM_API_KEY")
            .or_else(|| setting(key_var))
            .ok_or_else(|| anyhow!("no key for {model} at {label}: set {key_var} (or RAEA_LLM_API_KEY) in the environment or ~/.raea/env"))?;
        Ok(Self { base, key, label: format!("{model} · {label}"), model, bearer })
    }
}

/// A session token for raea's API: RAEA_API_TOKEN when the host gives one, else one minted for the eval user from
/// RAEA_EVAL_KEY at {RAEA_API}/v1/dev/eval-session. Used by the brands tool, remote runners and the eval.
pub(crate) async fn api_session() -> anyhow::Result<String> {
    if let Some(t) = setting("RAEA_API_TOKEN") {
        return Ok(t);
    }
    let eval_key = setting("RAEA_EVAL_KEY").ok_or_else(|| anyhow!("no RAEA_API_TOKEN and no RAEA_EVAL_KEY for raea's API"))?;
    let res: Value = reqwest::Client::new()
        .post(format!("{}/v1/dev/eval-session", raea_api()?))
        .header("x-eval-key", eval_key)
        .send()
        .await?
        .json()
        .await?;
    Ok(res["token"].as_str().ok_or_else(|| anyhow!("eval-session: {res}"))?.to_string())
}

/// What the brain tells the shell as it works.
#[derive(Debug, Clone)]
pub enum BrainEvent {
    /// Reply text as it streams (spoken a clause at a time).
    Text(String),
    /// A tool started: its name and a few words on what it's doing.
    Doing(String, String),
    /// A tool finished; ok and its result in brief.
    Done(String, bool, String),
    /// Swap this space onto the stage.
    Open(Space),
    /// The open space's program changed: redraw.
    Changed,
    /// The agent asked the person a question to answer by choosing (`raea ask`): {id, task, text, options:
    /// [{label, meaning}], multi}. The turn ends after it; the answer comes as the person's next message.
    Ask(Value),
    /// A model round ended; true when it called tools (its text was a murmur, not the reply).
    RoundEnd(bool),
    /// The turn is over.
    End,
    Error(String),
}

/// Who it is: each piece is a React + TypeScript project worked through `raea` (the founder, 2026-10-17: fully on
/// TypeScript; the record language rrl is retired).
const ROLE: &str = "You are raea void. You make whatever the person asks for that renders: an edit of their footage, an ad, an explainer, a motion piece, a deck, a game. You design it and you build it, the way a programmer builds software. Each piece is a small React + TypeScript project (edit.tsx, cuts.ts, components/, brand/brand.css) that raea renders. You work in its folder with the shell: read files, write TSX, and run `raea` (raea help lists its commands) to cut takes, add components, render, check and pull stills. Your first steps follow from the request, not from what the object on screen happens to hold. Keep working, one step after another, until the piece is done: it is rendered, `raea check --full` has run on the last render, and you have looked at its stills and fixed what they showed. Then answer once: what changed, what you verified and what still fails. End a turn earlier only to ask the person something with `raea ask`.";

/// The procedure the agent works by unless another is asked for (prompts/sops/<name>.md).
const PROCEDURE: &str = "ts";

/// The most a round may write. GLM thinks before it acts (its thinking can't be turned off), as long as it needs:
/// its thinking is never capped (the founder, 2026-10-17: "stop capping budgets, it's not helpful"), so the round gets
/// room for the thinking and the work after it. At 4096, then 16384, rounds of a long build thought to the limit and
/// said nothing (MiMo five times running, GLM 5.3 Flash twice); 65536 by default there (both allow far more;
/// RAEA_VOID_MAX_TOKENS overrides).
fn max_tokens() -> u64 {
    std::env::var("RAEA_VOID_MAX_TOKENS").ok().and_then(|v| v.parse().ok()).unwrap_or(65536)
}

/// The system prompt: who it is, the craft's router and floor, and the procedure.
fn system(sop: Option<&str>) -> String {
    // a procedure (prompts/sops/<name>.md, read at run time so a new one needs no build) goes last
    let overlay = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/sops").join(format!("{}.md", sop.unwrap_or(PROCEDURE))))
        .unwrap_or_default();
    // the craft (prompts/craft, read at run time like the procedures): the router and the floor ride on every turn
    let craft = ["ROUTER", "floor"].iter().filter_map(|n| craft_file(n)).collect::<Vec<_>>().join("\n\n");
    // the open space (space_ui.rs): its agent files, skills and panels' actions, after the procedure
    match crate::space_ui::agent_text() {
        Some(space) => format!("{ROLE}\n\n{craft}\n\n{overlay}\n\n{space}"),
        None => format!("{ROLE}\n\n{craft}\n\n{overlay}"),
    }
}

/// The craft references the `guide` tool reads: files in the folder RAEA_GUIDES names (environment or ~/.raea/env),
/// one per topic. A host brings its own house knowledge; without the folder, `guide` says what's missing.
const GUIDES: &[(&str, &str)] = &[
    ("captions", "captions.md"),
    ("motion", "motion-rules.md"),
    ("shots", "motion-shots.feature"),
    ("editing", "editing.md"),
    ("composition", "composition.md"),
    ("transitions", "transitions.md"),
    ("easing", "easing-and-stagger.md"),
    ("hooks", "hooks.md"),
    ("brand", "brand-kit.md"),
];

/// The most model rounds one turn takes: making a game or a motion piece from nothing runs past 20.
const ROUNDS: usize = 40;

/// A turn's model usage, summed over its rounds.
#[derive(Default)]
struct Totals {
    rounds: u64,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    cost: f64,
}

/// Prompt caching: the system prompt (role, card, skill: the same every turn) is the first cache point...
fn cached_system(sop: Option<&str>) -> Value {
    json!([{"type": "text", "text": system(sop), "cache_control": {"type": "ephemeral"}}])
}

/// The extra tools a procedure asks for: its `Tools: a, b` line (prompts/sops/<name>.md). None by default.
fn sop_tools(sop: Option<&str>) -> Vec<String> {
    sop.and_then(|s| std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/sops").join(format!("{s}.md"))).ok())
        .and_then(|t| t.lines().find_map(|l| l.trim().strip_prefix("Tools:").map(|v| v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())))
        .unwrap_or_default()
}

/// ...the tools the second...
fn cached_tools(sop: Option<&str>) -> Value {
    let mut t = tools();
    // look always; the shell and the browser where the procedure names them (`Tools: shell` in sops/ts.md)
    let named = sop_tools(Some(sop.unwrap_or(PROCEDURE)));
    if let Some(a) = t.as_array_mut() {
        a.retain(|x| x["name"] == "look" || x["name"].as_str().is_some_and(|n| named.iter().any(|t| t == n)));
        // the open space's panels' actions, one tool for them all (what a click does there, the agent can do)
        let acts = crate::space_ui::agent_actions();
        if !acts.is_empty() {
            let list = acts.iter().map(|(p, x)| format!("{p}.{} ({}): {}", x.name, x.args, x.about)).collect::<Vec<_>>().join("; ");
            a.push(json!({"name": "panel_action", "description": format!("Do what a click in one of the open space's panels does: run its action, handed to the panel live as input. Actions: {list}"),
                "input_schema": {"type": "object", "properties": {"panel": {"type": "string"}, "action": {"type": "string"}, "args": {"type": "object"}}, "required": ["action"]}}));
        }
    }
    if let Some(last) = t.as_array_mut().and_then(|a| a.last_mut()) {
        last["cache_control"] = json!({"type": "ephemeral"});
    }
    t
}

/// ...and the conversation so far the third, marked on the newest message, so each round reads everything before it
/// from the cache and pays only for what's new.

/// What a provider refused, kept in ~/.raea-void/refused (not the eval's home) to see why.
fn keep_refused(body: &Value, why: &str) {
    let dir = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".raea-void/refused");
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(dir.join(format!("{}-{why}.json", now_ms())), body.to_string());
    }
}

/// Past 16 stills in the history, all but the newest 8 become a line of text. CoreWeave's M3 fails a request holding
/// about 24 images (a 500, or a 422 "Input should be a valid string" mid-stream), and old frames of an edit since
/// changed only cost tokens. In batches, so the cached prefix changes once every 8 stills, not on every look.
fn prune_stills(history: &mut [Value]) {
    // pictures sit beside tool results now (and inside them in older turns): both count, both are dropped oldest first
    let count = |h: &[Value]| h.iter().flat_map(|m| m["content"].as_array().into_iter().flatten())
        .map(|b| (b["type"] == "image") as usize + b["content"].as_array().into_iter().flatten().filter(|c| c["type"] == "image").count()).sum::<usize>();
    let total = count(history);
    // past 10, back to the newest 6: in batches, since each rewrite of an old message re-reads the history uncached
    if total <= 10 {
        return;
    }
    let mut drop = total - 6;
    for m in history.iter_mut() {
        for b in m["content"].as_array_mut().into_iter().flatten() {
            if drop > 0 && b["type"] == "image" {
                *b = json!({"type": "text", "text": "(an earlier still, dropped to save room)"});
                drop -= 1;
                continue;
            }
            for c in b["content"].as_array_mut().into_iter().flatten() {
                if drop > 0 && c["type"] == "image" {
                    *c = json!({"type": "text", "text": "(an earlier still, dropped to save room)"});
                    drop -= 1;
                }
            }
        }
    }
}

/// ffmpeg and ffprobe as native media verbs (`rdb ffmpeg …`, `rdb ffprobe …`: raea-media, the same on the iPhone; no
/// ffmpeg binary ships, docs/media.feature). Same arguments and output, so the parsers here don't change.
fn media_verb(prog: &str) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(crate::stage::rdb_bin());
    c.arg(prog);
    c
}

fn media_blocking(prog: &str) -> std::process::Command {
    let mut c = std::process::Command::new(crate::stage::rdb_bin());
    c.arg(prog);
    c
}

/// The craft files the `craft` tool serves (prompts/craft/<name>.md, adapted from Impeccable, Apache 2.0: see its
/// NOTICE); ROUTER and floor are always in the system prompt.
pub(crate) const CRAFT: &[&str] = &[
    "rubric",
    "modes/direct-response", "modes/organic", "modes/explainer", "modes/deck", "modes/game", "modes/app-demo",
    "stages/brand", "stages/strategy", "stages/direct", "stages/build", "stages/critique", "stages/polish",
    "refine/pace", "refine/sound", "refine/punch", "refine/quieter", "refine/distill", "refine/typeset",
    "refine/colorize", "refine/animate", "refine/layout",
    "motion", "motion/compositions", "motion/timing", "motion/animating", "motion/footage", "motion/sound", "motion/words",
    "motion/placement", "motion/layout", "motion/brand", "motion/assets", "motion/3d", "motion/games", "motion/rendering",
    "bevy", "bevy/scene", "bevy/lights", "bevy/materials", "bevy/meshes", "bevy/animation", "bevy/games", "bevy/assets",
    "bevy/post", "bevy/limits", "bevy/library",
];

pub(crate) fn craft_file(name: &str) -> Option<String> {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/craft").join(format!("{name}.md"))).ok()
}

fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn cached_messages(history: &[Value]) -> Value {
    let mut h: Vec<Value> = history.to_vec();
    if let Some(last) = h.last_mut() {
        if let Some(text) = last["content"].as_str().map(str::to_string) {
            last["content"] = json!([{"type": "text", "text": text}]);
        }
        if let Some(block) = last["content"].as_array_mut().and_then(|c| c.last_mut()) {
            block["cache_control"] = json!({"type": "ephemeral"});
        }
    }
    json!(h)
}

/// Where to cut a take for each line: the lines found word for word in its transcript, in order (a line that follows
/// the last one in the source joins its clip), each cut in the pauses around it and off the source's own jump cuts, on
/// the source's frame grid. (in, out) in the take's seconds and the words kept, one per clip. `raea cut` (host.rs)
/// writes them as cuts.ts.
pub(crate) async fn cut_plan(media: &std::path::Path, words_path: &std::path::Path, lines: &[String]) -> anyhow::Result<(Vec<(f64, f64, String)>, Vec<String>)> {
    let words: Vec<(String, f64, f64)> = serde_json::from_str::<Vec<Value>>(&std::fs::read_to_string(&words_path)?)?
        .iter().filter_map(|w| Some((w["text"].as_str()?.to_string(), w["start_ms"].as_f64()? / 1000.0, w["end_ms"].as_f64()? / 1000.0)))
        .filter(|w| !w.0.trim().is_empty()).collect();
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == '\'' || c.is_whitespace()).collect::<String>();
    let toks: Vec<String> = words.iter().map(|w| norm(&w.0).trim().to_string()).collect();
    // the source's own cuts (talking-head jump cuts score low, hence the low threshold), read once per take: a
    // 10-minute take took over 5 minutes a cut, every cut (kept in the temp folder by the file's path, size and time)
    let cuts: Vec<f64> = {
        let meta = std::fs::metadata(media).ok();
        let key = format!("{}|{}|{:?}", media.display(), meta.as_ref().map(|m| m.len()).unwrap_or(0), meta.and_then(|m| m.modified().ok()));
        let cache = std::env::temp_dir().join("raea-scene-cuts").join(format!("{:016x}.json", { use std::hash::{Hash, Hasher}; let mut h = std::collections::hash_map::DefaultHasher::new(); key.hash(&mut h); h.finish() }));
        match std::fs::read_to_string(&cache).ok().and_then(|t| serde_json::from_str::<Vec<f64>>(&t).ok()) {
            Some(c) => c,
            None => {
                let out = media_verb("ffmpeg").args(["-nostats", "-i"]).arg(&media)
                    .args(["-vf", "select='gt(scene,0.08)',showinfo", "-an", "-f", "null", "-"]).output().await?;
                let c: Vec<f64> = String::from_utf8_lossy(&out.stderr).split("pts_time:").skip(1).filter_map(|s| s.split_whitespace().next()?.parse().ok()).collect();
                let _ = std::fs::create_dir_all(cache.parent().unwrap());
                let _ = std::fs::write(&cache, serde_json::to_string(&c).unwrap_or_default());
                c
            }
        }
    };
    let src_fps: f64 = {
        let out = media_verb("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=r_frame_rate", "-of", "csv=p=0"]).arg(&media).output().await?;
        let r = String::from_utf8_lossy(&out.stdout);
        let (n, d) = r.trim().split_once('/').unwrap_or((r.trim(), "1"));
        n.parse::<f64>().unwrap_or(30.0) / d.parse::<f64>().unwrap_or(1.0).max(1.0)
    };
    let mut search_from = 0usize;
    // each line's word span; a line that follows the last one in the source joins its clip (no cut, no
    // overlap: two clips over the same words is the "they—they" stutter)
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for (k, line) in lines.iter().enumerate() {
        let want: Vec<String> = norm(line).split_whitespace().map(String::from).collect();
        anyhow::ensure!(!want.is_empty(), "line {} is empty", k + 1);
        let fits = |i: usize| want.iter().enumerate().all(|(o, t)| toks.get(i + o).is_some_and(|w| w == t));
        // word for word first; else the closest wording in the take when 70% or more of the line's words are there in a
        // window its length (a line remembered a little off is cut where it's really said, and said so)
        let close = || -> Option<usize> {
            let n = want.len();
            if n < 3 || toks.len() < n {
                return None;
            }
            let score = |i: usize| -> usize {
                let win = &toks[i..(i + n).min(toks.len())];
                want.iter().filter(|t| win.contains(t)).count()
            };
            let best = (0..=toks.len() - n).max_by_key(|&i| (score(i), usize::MAX - i.abs_diff(search_from)))?;
            (score(best) * 10 >= n * 7).then_some(best)
        };
        let exact = (search_from..toks.len()).find(|&i| fits(i)).or_else(|| (0..toks.len()).find(|&i| fits(i)));
        let i = exact.or_else(close)
            .ok_or_else(|| {
                let first = &want[0];
                let near: Vec<String> = toks.iter().enumerate().filter(|(_, t)| *t == first).take(3)
                    .map(|(i, _)| words[i..(i + 10).min(words.len())].iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ")).collect();
                anyhow!("line {} isn't in the transcript word for word: \"{line}\"; it says near there: {}", k + 1, if near.is_empty() { "(no match for its first word)".into() } else { near.join(" | ") })
            })?;
        let j = (i + want.len() - 1).min(toks.len() - 1);
        if exact.is_none() {
            notes.push(format!("line {} isn't said word for word; cut where the take says \"{}\"", k + 1, words[i..=j].iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ")));
        }
        search_from = j + 1;
        match spans.last_mut() {
            Some(last) if last.1 + 1 == i => last.1 = j,
            _ => spans.push((i, j)),
        }
    }
    let count = spans.len();
    let mut out = Vec::new();
    for (k, (i, j)) in spans.into_iter().enumerate() {
        let gap_before = if i > 0 { words[i].1 - words[i - 1].2 } else { 0.4 };
        let gap_after = if j + 1 < words.len() { words[j + 1].1 - words[j].2 } else { 0.6 };
        // transcript word ends run early: a generous tail (more on the last line), a short lead-in, both
        // inside the pause so the next or last word never bleeds in
        let tail = if k + 1 == count { 0.35 } else { 0.22 };
        let mut a = words[i].1 - (gap_before * 0.6).min(0.12);
        let mut b = words[j].2 + (gap_after * 0.7).min(tail);
        // a source jump cut near a join: start one source frame after it (or end one before), never on it,
        // since a join placed exactly on the cut can show the last frame of the other shot (a stray frame)
        let sf = 1.0 / src_fps;
        if let Some(c) = cuts.iter().find(|&&c| (c - a).abs() <= 0.25 && c <= words[i].1 + 0.05) { a = a.max(c + sf); }
        if let Some(c) = cuts.iter().find(|&&c| (c - b).abs() <= 0.25 && c >= words[j].2 - 0.05) { b = b.min(c - sf); }
        // on the source's frame grid: the first whole frame at or after a, the last before b
        a = (a.max(0.0) * src_fps).ceil() / src_fps;
        b = (b * src_fps).floor() / src_fps;
        out.push((a, b, words[i..=j].iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ")));
    }
    Ok((out, notes))
}

/// A tool result made short for the history (`Brain::compact`): the primary model (GLM on OpenRouter when there's an
/// OpenRouter key; else the head and tail; ScaleDown's
/// summarize before the founder's two-model rule), told what to keep, else the head and tail. GLM thinks first and its
/// thinking isn't capped, so the call gets 65536 tokens and two minutes. The summary and what it cost.
async fn summarize(client: &reqwest::Client, text: &str) -> (String, f64) {
    let note = |body: &str| format!("[compacted: {} characters, summarized; run the command again for all of it]\n{body}", text.len());
    let cut = || {
        let chars: Vec<char> = text.chars().collect();
        let (head, tail): (String, String) = (chars.iter().take(500).collect(), chars.iter().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect());
        format!("[compacted: {} characters, cut; run the command again for all of it]\n{head}\n…\n{tail}", text.len())
    };
    let Some(key) = openrouter_key() else { return (cut(), 0.0) };
    let ask = "This is a tool's output an editing agent already read. Rewrite it as briefly as it can be while keeping every command, file name, path, number, time, error, warning and fault exactly; drop explanation and repetition.";
    let body = json!({"model": PRIMARY, "max_tokens": 65536, "temperature": 1.0, "usage": {"include": true},
        "messages": [{"role": "user", "content": format!("{ask}\n\n{}", text.chars().take(60_000).collect::<String>())}]});
    let res = client.post("https://openrouter.ai/api/v1/chat/completions").bearer_auth(key).json(&body)
        .timeout(std::time::Duration::from_secs(120)).send().await;
    let Ok(v) = (match res { Ok(r) if r.status().is_success() => r.json::<Value>().await.map_err(|_| ()), _ => Err(()) }) else { return (cut(), 0.0) };
    let short = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
    let cost = v["usage"]["cost"].as_f64().unwrap_or(0.0);
    if short.is_empty() || short.len() >= text.len() { (cut(), cost) } else { (note(&short), cost) }
}

/// A TypeScript project as the turn sees it: its clips (cuts.json), its components, the other files at its top, and
/// AGENTS.md (the project's own notes, @raea/motion's API among them).
fn ts_context(dir: &std::path::Path) -> String {
    let mut out = String::from("# The project (React + TypeScript; work in it with the shell and `raea`)\n");
    let cuts = std::fs::read_to_string(dir.join("cuts.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    match cuts.as_ref().and_then(|v| v["cuts"].as_array().cloned()).filter(|a| !a.is_empty()) {
        Some(a) => {
            let total: f64 = a.iter().map(|c| c["to"].as_f64().unwrap_or(0.0) - c["from"].as_f64().unwrap_or(0.0)).sum();
            out += &format!("cuts.ts: {} clips, {total:.1} s\n", a.len());
            for c in a.iter().take(20) {
                out += &format!("  {}  {:.2}-{:.2}  \"{}\"\n", c["take"].as_str().unwrap_or(""), c["from"].as_f64().unwrap_or(0.0), c["to"].as_f64().unwrap_or(0.0), c["line"].as_str().unwrap_or(""));
            }
        }
        None => out += "cuts.ts: no clips yet (raea cut)\n",
    }
    let comps: Vec<String> = std::fs::read_dir(dir.join("components")).into_iter().flatten().flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from)).filter(|n| n.ends_with(".tsx")).collect();
    out += &format!("components/: {}\n", if comps.is_empty() { "none yet (raea add)".into() } else { comps.join(", ") });
    let top: Vec<String> = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from)).filter(|n| !n.starts_with('.')).collect();
    out += &format!("files: {}\n", top.join(", "));
    if dir.join("brand/brand.css").exists() { out += "brand/brand.css: the brand's kit is here\n" } else { out += "brand/brand.css: none yet (raea brand DOMAIN)\n" }
    if let Ok(notes) = std::fs::read_to_string(dir.join("AGENTS.md")) {
        out += &format!("\n## AGENTS.md\n{}\n", notes.chars().take(6000).collect::<String>());
    }
    out
}

/// Whether a TypeScript project has a piece in it yet: clips in cuts.json, or an edit.tsx changed from raea init's.
fn ts_built(dir: &std::path::Path) -> bool {
    let clips = std::fs::read_to_string(dir.join("cuts.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["cuts"].as_array().map(|a| !a.is_empty())).unwrap_or(false);
    let edited = std::fs::read_to_string(dir.join("edit.tsx")).is_ok_and(|t| !t.contains("a talking take: <FaceFrame>; a screen recording: <ScreenFit>"));
    clips || edited
}

fn tools() -> Value {
    let o = |props: Value, req: &[&str]| json!({"type": "object", "properties": props, "required": req});
    json!([
        {"name": "list_objects", "description": "The project's objects (each anything that renders: an ad, a game, a deck), most recent first, and which is on screen.", "input_schema": o(json!({}), &[])},
        {"name": "open_object", "description": "Put one of the project's objects on screen and work in its folder: by name.", "input_schema": o(json!({"name": {"type": "string"}}), &["name"])},
        {"name": "new_object", "description": "Make a new object in the project and put it on screen: a folder of its own with an empty React + TypeScript project (raea init) to build it in. Say what it is.",
         "input_schema": o(json!({"name": {"type": "string"}, "kind": {"type": "string", "enum": ["video", "game", "deck", "document", "music", "motion", "scene3d", "still"]}}), &["name", "kind"])},
        {"name": "craft", "description": "Load a craft file by name: a mode (modes/organic ...), a stage (stages/strategy, stages/direct ...), a refine playbook (refine/pace ...) or the rubric. The router in your instructions says which and when; load one at a time.",
         "input_schema": o(json!({"name": {"type": "string", "enum": CRAFT}}), &["name"])},
        {"name": "guide", "description": "the host app's craft guides, optional references (one studio's taste, not a standard to match). Read one when it helps: captions (styles, karaoke, pills vs outlines, grouping), motion (recipes: marker highlight, lower third, counters, easing and stagger), shots (directions for hook titles, stats, this-not-that, callouts, end cards), editing (house style: safe zones, hook in 3 s, tight cuts), composition, transitions, hooks (hook families, ad structure, concept types), brand. They're written for the host app's HTML compositions: take the craft (sizes, timings, styles, structure) and write it in the project's TSX; never paste HTML.",
         "input_schema": o(json!({"topic": {"type": "string", "enum": GUIDES.iter().map(|g| g.0).collect::<Vec<_>>()}}), &["topic"])},
        {"name": "search_assets", "description": "Search the space's files by meaning: what's in a picture or a video's frames, what's said in footage (with the moment), what a document says, and names. E.g. \"woman holding a coffee cup\", \"talks about the price\", \"logo on white\". Answers with each file, a 0-1 match and why, best first. Files live in media/ (video, audio, images), models/ (3D), docs/ (documents), assets/ (other).",
         "input_schema": o(json!({"query": {"type": "string"}}), &["query"])},
        {"name": "game_library", "description": "raea's game library (sdk/motion/library/games): Bevy examples with their TSX equivalents, open-source Bevy games, game-design patterns (jump feel, screen shake, level design, genre templates) and free 3D models, rigs and animations, each with its licence. Search it by meaning with `query` (e.g. \"low-poly knight with sword animations\", \"coyote time\"), optionally `kind` (asset, animation, rig, example, game, pattern, plugin; comma-separated). Then `fetch` a record's id (one whose ship is download) to copy its model into this object's media/models/ID/ and get the staticFile path for <Model>; for a pack, give `file` (one of the models fetch lists). Craft bevy/library says how to use it.",
         "input_schema": o(json!({"query": {"type": "string"}, "kind": {"type": "string"}, "fetch": {"type": "string"}, "file": {"type": "string"}}), &[])},
        {"name": "media", "description": "The footage and sound this space has (its media/ folder, and take folders inside it): each file with its duration and size, and whether its words are already transcribed. Look here before asking the person for files.",
         "input_schema": o(json!({}), &[])},
        {"name": "review", "description": "Render the project (raea render) and have a strict creative director watch it (a video model, ~30-60 s): scores against the rubric, a verdict and the defects to fix with timestamps.",
         "input_schema": o(json!({"asked": {"type": "string", "description": "what the person asked for, in their words"}}), &["asked"])},
        {"name": "brand", "description": "A brand's kit from its website (the host app's brands API, context.dev): name, slogan, colours, and its logos downloaded into the space (media/brand/<domain>-*.svg|png, light and dark versions when there are), with brand/brand.css written for the project. Give a domain or a name to search.",
         "input_schema": o(json!({"brand": {"type": "string", "description": "a domain or a brand name"}}), &["brand"])},
        {"name": "shell", "description": "A bash terminal over the space folder (and nothing outside it), kept between calls: cd and exports carry over. Text tools (ls, cat, grep, sed, awk, jq, find, sort ...), raea (the project's CLI: raea help), rdb, raea-motion, sqlite3, js-exec (TS/JS in its own isolate), tar, html-to-markdown, and `browser`. No python, no network. Every command is logged and can be taken back with undo: true.",
         "input_schema": o(json!({"command": {"type": "string", "description": "the bash to run (required unless undo is true), e.g. raea check --full, or cat > edit.tsx <<'EOF' ... EOF"}, "undo": {"type": "boolean", "description": "take back the last command's file changes instead of running one"}, "timeout_s": {"type": "integer", "description": "default 60"}}), &[])},
        {"name": "browser", "description": "A browser kept between calls: open URL, snapshot -i (the page's interactive parts as refs e1, e2 ...), click @e3, fill @e3 TEXT, get text|title|value, eval JS, wait, screenshot PATH (in the space; the picture comes back to you). For pages in the space (file paths) or the kit; offline.",
         "input_schema": o(json!({"args": {"type": "string", "description": "e.g. open parts/preview.html, or snapshot -i"}}), &["args"])},
        {"name": "look", "description": "See pictures, sent back as images: image files in the project (paths, e.g. .stills/3.50.png from `raea still`, .sheets/take.png from `raea media`).",
         "input_schema": o(json!({"paths": {"type": "array", "items": {"type": "string"}, "maxItems": 4}}), &["paths"])}
    ])
}

pub struct Brain {
    provider: Provider,
    history: Vec<Value>,
    client: reqwest::Client,
    /// The space on the stage, shared with the shell.
    pub open: Arc<Mutex<Option<Space>>>,
    /// Frames `look` rendered this call, as base64 JPEG.
    images: Vec<String>,
    /// the open space's terminal (shell and browser), started on first use
    shell: Option<crate::shell::Terminal>,
    /// the history's length when it was last compacted (old tool results summarized; see `compact`)
    compacted_at: usize,
    /// The last turn's trace id (evals keep it with their result) and what it cost.
    pub last_trace: String,
    pub last_cost: f64,
    /// What tools spent this turn (the review's video model), added to the turn's cost at its end.
    tool_cost: f64,
    /// Attributes put on every turn's span (an eval's scenario, medium, level and run).
    pub tags: serde_json::Map<String, Value>,
    /// Tokens used over all turns: input, output, cache_read, cache_write, rounds.
    pub usage: Value,
    /// Where its spaces live: the person's (~/.raea-void/spaces), or an eval scenario's own folder, so an eval can
    /// never list, open or change the person's spaces.
    pub spaces: std::path::PathBuf,
    /// Sampling temperature: None is the model's default (the app); evals pin it low so a pass or fail is the agent's,
    /// not the dice's.
    pub temperature: Option<f64>,
    /// The most model rounds a turn takes (ROUNDS in the app; evals give real builds more).
    pub rounds: usize,
    /// Review rounds void runs itself when a turn that changed the edit ends (0 in the app; evals turn it on): the
    /// model rarely asks for a critique, so the harness gets one and hands the notes back.
    pub self_reviews: usize,
    /// How many times a turn's last edits are cutchecked before it may end (evals: 2; the app: 1).
    pub cut_gates: usize,
    /// A turn that ends before anything is built is sent back to build (evals; the app's person can say "go on").
    pub stall_nudges: bool,
    /// The procedure the agent works by (prompts/sops/<name>.md); None is the default, ts.
    pub sop: Option<String>,
    /// Why the last model round stopped (end_turn, tool_use, max_tokens).
    last_stop: String,
}

impl Brain {
    pub fn new(provider: Provider, open: Arc<Mutex<Option<Space>>>) -> Self {
        Self { provider, history: Vec::new(), client: reqwest::Client::new(), open, images: Vec::new(), last_trace: String::new(), last_cost: 0.0, tool_cost: 0.0, tags: serde_json::Map::new(), usage: json!({"input": 0, "output": 0, "cache_read": 0, "cache_write": 0, "rounds": 0}), spaces: spaces::dir(), temperature: None, rounds: ROUNDS, self_reviews: 0, cut_gates: 1, stall_nudges: false, sop: Some("ts".into()), shell: None, compacted_at: 0, last_stop: String::new() }
    }

    pub fn forget(&mut self) {
        self.history.clear();
    }

    /// A command run in the agent's own shell, in `folder` (the window's Restore: one of its commands, so undo takes it
    /// back).
    pub async fn exec_in(&mut self, folder: &std::path::Path, cmd: &str) -> anyhow::Result<String> {
        if !self.shell.as_mut().is_some_and(|t| t.alive() && t.root() == folder) {
            self.shell = Some(crate::shell::Terminal::open(folder).await?);
        }
        self.shell.as_mut().unwrap().exec(cmd, 300_000).await
    }

    /// The conversation so far (a session keeps it: app.rs sessions), and one put back.
    pub fn history(&self) -> &[Value] {
        &self.history
    }
    pub fn set_history(&mut self, h: Vec<Value>) {
        self.history = h;
        self.compacted_at = 0;
        self.repair();
    }

    /// After a turn was cut off (barge-in, a new message): drop an assistant tool call left without its result, so the
    /// history stays valid.
    pub fn repair(&mut self) {
        while self.history.last().is_some_and(|m| {
            m["role"] == "assistant" && m["content"].as_array().is_some_and(|c| c.iter().any(|b| b["type"] == "tool_use"))
        }) {
            self.history.pop();
        }
    }

    /// What the open space holds, for the first message: `media`'s list (each file, its length and shape), and how to
    /// read it in full. Empty when the space has no media. Not the transcripts or contact sheets: those went with every
    /// first message, and a request with nothing to do with the footage (a game, a deck) began by studying a UGC take
    /// (raea-01, 2026-10-17); `raea media` gives them in one call when the piece uses them.
    async fn footage_list(&mut self, ev: &mut dyn FnMut(BrainEvent)) -> String {
        match self.hands("media", &json!({}), ev).await {
            Ok(l) if !l.starts_with("no media") => format!("# What's in this space's media/ (read it only if the request uses it)
{l}
`raea media` gives every take's transcript as timed sentences and a contact sheet to look at.
"),
            _ => String::new(),
        }
    }

    /// One user turn, to the end: model rounds and tool calls until it answers. Events go to `ev`.
    pub async fn turn(&mut self, text: &str, ev: &mut dyn FnMut(BrainEvent)) {
        // the turn's context: the project (made where it's missing: `raea init`) and what's in it
        let space = self.open.lock().unwrap().clone();
        if let Some(s) = space.as_ref() {
            // on a session's first turn too: raea init brings an older generated AGENTS.md up to date (the notes the
            // agent reads), and makes nothing that's already there
            if !s.folder().join("edit.tsx").exists() || self.history.is_empty() {
                if !self.shell.as_mut().is_some_and(|t| t.alive() && t.root() == s.folder()) {
                    self.shell = crate::shell::Terminal::open(&s.folder()).await.ok();
                }
                if let Some(t) = self.shell.as_mut() {
                    let _ = t.exec("raea init", 120_000).await;
                }
            }
        }
        let context = match space {
            Some(s) => format!("[open space: {}]\n{}\n# The person\n", s.name, ts_context(&s.folder())),
            None => "[no object on screen: new_object makes one]\n".into(),
        };
        // what the space's media/ holds, listed on a session's first turn; the request decides whether it's read
        // (`raea media`). On 2026-10-16 smoke turns spent 7-69 of their 90 rounds digging the takes out with
        // ls/cat/jq; the list names the one command that reads them
        let sheet = if self.history.is_empty() { self.footage_list(ev).await } else { String::new() };
        // the skills on the computer, a line each, on a session's first turn (skills.rs; `raea skill NAME` reads one)
        let sheet = if self.history.is_empty() { format!("{sheet}{}", crate::skills::context(crate::workspace::current().map(|w| w.root).as_deref())) } else { sheet };
        let context = match context.strip_suffix("# The person\n") {
            Some(head) if !sheet.is_empty() => format!("{head}{sheet}"),
            _ => format!("{context}{sheet}"),
        };
        // what the person did to the space's files since (the Files tab), so the agent knows they're there
        let told = self.open.lock().unwrap().as_ref().map(|s| take_notices(&s.folder())).unwrap_or_default();
        let context = if told.is_empty() { context } else {
            let block = format!("# Since your last turn the person changed this space's files (the Files tab)\n{}\n\n", told.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n"));
            match context.strip_suffix("# The person\n") {
                Some(head) => format!("{head}{block}# The person\n"),
                None => format!("{context}{block}"),
            }
        };
        // the project's record (@raea/data/project.ts): its objects, open tasks and their next steps, questions, notes
        // with nothing on screen, the project's own folder (the workspace's root)
        let record = self.open.lock().unwrap().clone().map(|s| s.folder()).or_else(|| crate::workspace::current().map(|w| w.root));
        let context = match record {
            Some(folder) => match crate::host::project(&folder, &["status".into()], false).await {
                (st, 0) => {
                    let block = format!("# The project's record (raea status)\n{st}\n\n");
                    match context.strip_suffix("# The person\n") {
                        Some(head) => format!("{head}{block}# The person\n"),
                        None => format!("{context}{block}"),
                    }
                }
                _ => context,
            },
            None => context,
        };
        let turn_start = self.history.len();
        let first = format!("{context}{text}");
        self.history.push(json!({"role": "user", "content": first}));
        // the turn's trace: one agent span, a model span per round, a tool span per call
        let mut turn = crate::telemetry::tracer().root("agent.turn");
        for (k, v) in &self.tags {
            turn.set(k, v.clone());
        }
        turn.kind("AGENT").set("input.value", text).set("raea.space", self.open.lock().unwrap().as_ref().map(|s| s.name.clone()).unwrap_or_default());
        self.last_trace = turn.trace_id();
        let mut totals = Totals::default();
        let mut reply = String::new();
        let mut finished = false;
        let (mut reviewed, mut changed, mut gated, mut stalls) = (0usize, false, 0usize, 0usize);
        let mut ready_gated = false;
        let mut empties = 0usize;
        for round in 0..self.rounds {
            let content = match self.round(ev, &turn, &mut totals).await {
                Ok(c) => c,
                Err(e) => {
                    turn.error(&e.to_string());
                    // its rounds were still paid for
                    turn.set("raea.cost_usd", totals.cost + self.tool_cost).set("raea.rounds", totals.rounds);
                    self.last_cost = totals.cost + std::mem::take(&mut self.tool_cost);
                    // a failed turn leaves nothing behind: a tool call without its result would break every later call
                    self.history.truncate(turn_start);
                    ev(BrainEvent::Error(e.to_string()));
                    ev(BrainEvent::End);
                    return;
                }
            };
            let calls: Vec<Value> = content.iter().filter(|b| b["type"] == "tool_use").cloned().collect();
            if calls.is_empty() {
                reply = content.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("");
            }
            ev(BrainEvent::RoundEnd(!calls.is_empty()));
            // an empty answer (no words, no tool: MiMo mid-build on 2026-10-17, after reading the craft) ended the turn
            // with nothing said or built; it goes on instead, twice at most
            if calls.is_empty() && reply.trim().is_empty() && empties < 2 {
                empties += 1;
                turn.set("raea.empty_answers", empties as u64);
                // the round's thinking stays when it ended on its own (Z.ai: reasoning goes back whole and in order, and
                // dropped reasoning makes GLM loop); one cut off at max_tokens is left out
                let mut kept: Vec<Value> = if self.last_stop == "max_tokens" { Vec::new() } else { content.iter().filter(|b| b["type"] == "thinking").cloned().collect() };
                kept.push(json!({"type": "text", "text": "(no answer)"}));
                self.history.push(json!({"role": "assistant", "content": kept}));
                self.history.push(json!({"role": "user", "content": if self.last_stop == "max_tokens" {
                    "(void) Your last answer ran out of room before it said or did anything (thinking, or one file too big for one call, is cut off). Act now, in smaller steps: write a big file in parts (one heredoc per component, then edit.tsx), each its own shell call."
                } else {
                    "(void) Your last answer was empty. Go on with the next step of the plan."
                }}));
                continue;
            }
            self.history.push(json!({"role": "assistant", "content": content}));
            // a turn that stops before it built anything, its last words a plan ("First, the closing CTA words."): on
            // 2026-10-16 MiMo v2.6 Pro ended 6 of 7 eval turns this way after 5-12 tool calls, each a 3 s black render.
            // Evals only (the app's person can say "go on"), and at most 4 times
            // the shell both reads and builds, so "built" is the project itself: clips in cuts.json, or an edit.tsx that
            // isn't the template any more (Glimpse ended all 7 turns on 2026-10-17 after reading files only, and a
            // shell call had counted as building)
            let built_now = self.open.lock().unwrap().as_ref().map(|s| ts_built(&s.folder())).unwrap_or(false);
            if calls.is_empty() && self.stall_nudges && !built_now && stalls < 4 {
                stalls += 1;
                turn.set("raea.stalled", stalls as u64);
                self.history.push(json!({"role": "user", "content": "(void) Nothing is built yet: cuts.ts has no clips and edit.tsx is the template. Keep going: `raea cut` the lines, build the piece in edit.tsx, `raea check` it, look at stills, then answer."}));
                continue;
            }
            if calls.is_empty() && changed && reviewed < self.self_reviews {
                reviewed += 1;
                ev(BrainEvent::Doing("review".into(), String::new()));
                // the cuts, the face and the frame's edges are measured by the gate below (`raea check`)
                let notes = match self.hands("review", &json!({"asked": text}), ev).await {
                    Ok(o) => o,
                    Err(e) => format!("(the review couldn't run: {e})"),
                };
                ev(BrainEvent::Done("review".into(), true, notes.clone()));
                turn.set(&format!("raea.self_review.{reviewed}"), notes.lines().next().unwrap_or(""));
                // its first line is "N overall, verdict  (scores)": anything short of ship goes back to the model
                if !notes.lines().next().unwrap_or("").contains(", ship") {
                    changed = false;
                    self.history.push(json!({"role": "user", "content": format!("(void) A strict creative director watched the whole piece:\n{notes}\nFix these, worst first, then look at the frames they name. Keep what works.")}));
                    continue;
                }
            }
            // the last word on the cuts: edits made after the reviews ran (or with none left) go out unmeasured, and
            // black gaps and words cut off at the end were the judge's commonest defects (2026-10-16 drive runs)
            if calls.is_empty() && changed && gated < self.cut_gates {
                gated += 1;
                let notes = self.measured(&mut turn, &gated.to_string(), ev).await;
                if !notes.is_empty() {
                    changed = false;
                    self.history.push(json!({"role": "user", "content": format!("(void) Before this goes out, it was measured:\n{}\nFix these, then answer.", notes.join("\n"))}));
                    continue;
                }
            }
            // ready is the checks' word, not the agent's (the founder, 2026-10-17: "ready only with a written waiver"): a reply
            // that calls the object ready or done while its record says draft goes back once
            if calls.is_empty() && !ready_gated {
                if let Some(n) = self.ready_gate(&reply).await {
                    ready_gated = true;
                    turn.set("raea.ready_gate", n.as_str());
                    self.history.push(json!({"role": "user", "content": n}));
                    continue;
                }
            }
            if calls.is_empty() {
                finished = true;
                break;
            }
            if calls.iter().any(|c| matches!(c["name"].as_str(), Some("shell" | "new_object" | "brand"))) {
                changed = true;
            }
            let mut results = Vec::new();
            let mut images: Vec<Value> = Vec::new();
            let mut asked: Option<Value> = None;
            for call in calls {
                let name = call["name"].as_str().unwrap_or("").to_string();
                ev(BrainEvent::Doing(name.clone(), brief(&name, &call["input"])));
                self.images.clear();
                let mut tool = turn.child(&format!("execute_tool {name}"));
                tool.tool(&name, &call["input"]);
                let malformed = call["input"].get("__malformed").is_some();
                let (ok, out) = if malformed {
                    (false, "your tool call's arguments arrived malformed (cut off or not valid JSON): send that call again, alone, with its arguments whole (write a long file in its own call, a quoted heredoc)".to_string())
                } else {
                    match self.hands(&name, &call["input"], ev).await {
                        Ok(o) => (true, o),
                        Err(e) => (false, e.to_string()),
                    }
                };
                tool.set("output.value", out.chars().take(32 * 1024).collect::<String>())
                    .set("raea.verdict", out.split_whitespace().next().unwrap_or(""))
                    .set("raea.images", self.images.len() as u64);
                if !ok || out.starts_with("REFUSED") {
                    tool.error(out.lines().next().unwrap_or("failed"));
                }
                tool.end();
                // a question for the person (`raea ask`): the turn ends after this round, and the app shows it to answer
                if ok && name == "shell" && asked.is_none() {
                    if let Some(q) = out.split_whitespace().next().filter(|w| w.starts_with('q') && out.contains(" asked: the person answers")) {
                        if let Some(folder) = self.open.lock().unwrap().as_ref().map(|s| s.folder()) {
                            let (j, code) = crate::host::project(&folder, &["question".into(), q.to_string()], true).await;
                            asked = serde_json::from_str::<Value>(&j).ok().filter(|v| code == 0 && v.is_object());
                        }
                    }
                }
                ev(BrainEvent::Done(name, ok, out.chars().take(4000).collect()));
                // look's frames go back as images right after its result, not inside it: DeepInfra (behind OpenRouter, for
                // MiMo and M3 alike) refuses an image inside a tool_result ("422 Input should be a valid string"), which
                // ended three eval turns on 2026-10-16; as sibling blocks every host takes them
                let shown = self.images.len();
                let text = if shown == 0 { out } else { format!("{out}\n({shown} image{} follow{})", if shown == 1 { "" } else { "s" }, if shown == 1 { "s" } else { "" }) };
                results.push(json!({"type": "tool_result", "tool_use_id": call["id"], "content": text, "is_error": !ok}));
                images.extend(self.images.drain(..).map(|b64| json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": b64}})));
            }
            // the gate's measurements at 60% of the rounds too, the edit as it stands: on 2026-10-16 every smoke turn ran
            // to its last round (90 or 150) without stopping, so the end-of-turn gate never ran and its faults shipped
            if self.cut_gates > 0 && self.rounds > 24 && round + 1 == self.rounds * 3 / 5 && changed {
                let notes = self.measured(&mut turn, "mid", ev).await;
                turn.set("raea.mid_gate", notes.len() as u64);
                // the mid gate's notes alone didn't turn a build around: every smoke turn on 2026-10-16 had them and
                // went on exploring to its last round, so the rest of the turn is named as finishing
                let left = self.rounds - round - 1;
                results.push(json!({"type": "text", "text": if notes.is_empty() {
                    format!("(void) Measured at this point ({left} rounds left): the cuts, the face and what a viewer can read are clean. From here the turn is for finishing: make the piece whole, then answer.")
                } else {
                    format!("(void) Measured on a render of the project as it stands ({left} rounds left):\n{}\nFrom here the turn is for finishing. Fix these, worst first, then answer.", notes.join("\n"))
                }}));
            }
            // a turn that runs out of rounds ends mid-build, unreviewed (2 of 26 drive runs on 2026-10-16): say so
            // while there's still room to finish
            if self.rounds > 24 && round + 1 == self.rounds - 12 {
                turn.set("raea.rounds_warned", true);
                results.push(json!({"type": "text", "text": "(void) 12 rounds left in this turn. Stop exploring: make the piece whole (every frame covered, ending on a finished sentence), then answer."}));
            }
            // the round's pictures after every tool_result (results must come first in the message)
            let at = results.iter().rposition(|b| b["type"] == "tool_result").map(|i| i + 1).unwrap_or(0);
            results.splice(at..at, images);
            self.history.push(json!({"role": "user", "content": results}));
            if let Some(q) = asked {
                // the turn waits on the person: closed as the model's, so their answer is the next message
                reply = format!("{} (waiting for your answer)", q["text"].as_str().unwrap_or("A question for you"));
                self.history.push(json!({"role": "assistant", "content": [{"type": "text", "text": format!("I asked: {reply}")}]}));
                turn.set("raea.asked", q["id"].as_str().unwrap_or(""));
                ev(BrainEvent::Ask(q));
                finished = true;
                break;
            }
        }
        if !finished {
            // out of rounds: close the turn as the model's, so the history alternates and the next turn can go on
            reply = format!("I stopped after {} steps without finishing. Say \"go on\" and I'll pick up where I left off.", self.rounds);
            self.history.push(json!({"role": "assistant", "content": [{"type": "text", "text": reply}]}));
            ev(BrainEvent::Text(reply.clone()));
            turn.set("raea.out_of_rounds", true);
        }
        turn.set("output.value", reply)
            .set("raea.cost_usd", totals.cost + self.tool_cost)
            .set("raea.tool_cost_usd", self.tool_cost)
            .set("raea.rounds", totals.rounds)
            .set("llm.token_count.prompt", totals.input + totals.cache_read + totals.cache_write)
            .set("llm.token_count.completion", totals.output)
            .set("llm.token_count.prompt_details.cache_read", totals.cache_read);
        self.last_cost = totals.cost + std::mem::take(&mut self.tool_cost);
        for (k, v) in [("input", totals.input), ("output", totals.output), ("cache_read", totals.cache_read), ("cache_write", totals.cache_write), ("rounds", totals.rounds)] {
            self.usage[k] = json!(self.usage[k].as_u64().unwrap_or(0) + v);
        }
        turn.end();
        ev(BrainEvent::End);
    }

    /// Old tool results made small: every 16 messages (about 8 rounds), each result older than the last 12 messages and
    /// longer than 1,500 characters becomes a summary (GLM's, `summarize`, told to keep every command, file, number, error
    /// and fault), or its head and tail when that fails. Each round re-sends the
    /// whole history, so a 90-round turn re-read its early `cat`s 80 times: Nemotron's 7 eval turns on 2026-10-17 sent
    /// 22.8M prompt tokens for 412 rounds. Batched so the provider's prompt cache breaks once per batch, not every round.
    /// RAEA_VOID_COMPACT=0 turns it off.
    async fn compact(&mut self, span: &mut raea_otel::Span) {
        if std::env::var("RAEA_VOID_COMPACT").as_deref() == Ok("0") || self.history.len() < self.compacted_at + 16 {
            return;
        }
        let keep = 12;
        let upto = self.history.len().saturating_sub(keep);
        let mut jobs = Vec::new();
        for (m, msg) in self.history.iter().enumerate().take(upto) {
            for (b, block) in msg["content"].as_array().into_iter().flatten().enumerate() {
                let Some(text) = block["content"].as_str() else { continue };
                if block["type"] == "tool_result" && text.len() > 1500 && !text.starts_with("[compacted") {
                    jobs.push((m, b, text.to_string()));
                }
            }
        }
        self.compacted_at = self.history.len();
        if jobs.is_empty() {
            return;
        }
        let client = self.client.clone();
        let done = futures_util::future::join_all(jobs.into_iter().map(|(m, b, text)| {
            let client = client.clone();
            async move { let (short, cost) = summarize(&client, &text).await; (m, b, text.len(), short, cost) }
        })).await;
        let (mut before, mut after) = (0usize, 0usize);
        for (m, b, len, short, cost) in done {
            self.tool_cost += cost;
            before += len;
            after += short.len();
            if let Some(block) = self.history[m]["content"].as_array_mut().and_then(|c| c.get_mut(b)) {
                block["content"] = json!(short);
            }
        }
        span.set("raea.compacted_chars", format!("{before} -> {after}"));
    }

    /// One streamed model call; returns the assistant's content blocks.
    async fn round(&mut self, ev: &mut dyn FnMut(BrainEvent), turn: &raea_otel::Span, totals: &mut Totals) -> anyhow::Result<Vec<Value>> {
        let mut span = turn.child(&format!("chat {}", self.provider.model));
        let started = std::time::Instant::now();
        self.compact(&mut span).await;
        prune_stills(&mut self.history);
        let mut body = json!({
            "model": self.provider.model, "max_tokens": max_tokens(), "stream": true,
            "system": cached_system(self.sop.as_deref()), "tools": cached_tools(self.sop.as_deref()), "messages": cached_messages(&self.history),
        });
        // GLM 5.3 Flash's documented setting is temperature 1.0 (top_p left at its 0.95: Z.ai says to tune one of them),
        // sent so no host picks its own
        body["temperature"] = json!(self.temperature.unwrap_or(1.0));
        // OpenRouter picks GLM's host unless RAEA_VOID_HOSTS pins some (OpenRouter once moved a run between hosts
        // mid-turn, which counted the same history as 466k more tokens)
        let hosts: Vec<String> = std::env::var("RAEA_VOID_HOSTS").ok().map(|h| h.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
        if !hosts.is_empty() {
            body["provider"] = json!({"order": hosts, "allow_fallbacks": false});
        }
        span.set("llm.invocation_parameters", json!({"temperature": self.temperature}).to_string());
        let (mut input, mut output, mut cache_read, mut cache_write) = (0u64, 0u64, 0u64, 0u64);
        let mut first_token: Option<f64> = None;
        // overloaded (529), rate-limited (429) and server errors are retried with backoff: one busy minute on the
        // provider shouldn't end a turn halfway through a build
        let mut wait = std::time::Duration::from_secs(2);
        let res = loop {
            let res = self
                .client
                .post(format!("{}/v1/messages", self.provider.base.trim_end_matches('/')))
                .header(if self.provider.bearer { "authorization" } else { "x-api-key" }, if self.provider.bearer { format!("Bearer {}", self.provider.key) } else { self.provider.key.clone() })
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await;
            let busy = match &res {
                Ok(r) => matches!(r.status().as_u16(), 429 | 500 | 502 | 503 | 504 | 529),
                Err(e) => e.is_timeout() || e.is_connect(),
            };
            if busy && wait <= std::time::Duration::from_secs(32) {
                span.event("retry", serde_json::Map::from_iter([("wait_s".to_string(), json!(wait.as_secs()))]));
                tokio::time::sleep(wait).await;
                wait *= 2;
                continue;
            }
            break res?;
        };
        if !res.status().is_success() {
            let status = res.status();
            // keep what the provider refused, to see why (OpenRouter's CoreWeave once answered 422 "Input should be a
            // valid string" mid-turn and the same request shape passed alone)
            keep_refused(&body, &status.as_u16().to_string());
            let msg = format!("model: {status}: {}", res.text().await.unwrap_or_default().chars().take(300).collect::<String>());
            span.error(&msg);
            return Err(anyhow!(msg));
        }
        let mut blocks: Vec<Value> = Vec::new();
        let mut partial_json: Vec<String> = Vec::new();
        let mut buf = String::new();
        let mut stop = String::new();
        let mut host = String::new();
        let mut billed: Option<f64> = None;
        let dump = std::env::var("RAEA_VOID_DUMP").ok();
        let mut stream = res.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if let Some(d) = &dump {
                use std::io::Write;
                let _ = std::fs::OpenOptions::new().create(true).append(true).open(d).and_then(|mut f| f.write_all(&chunk));
            }
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(i) = buf.find('\n') {
                let line: String = buf.drain(..=i).collect();
                let Some(data) = line.trim().strip_prefix("data:") else { continue };
                let Ok(e) = serde_json::from_str::<Value>(data.trim()) else { continue };
                match e["type"].as_str().unwrap_or("") {
                    // usage: the prompt's tokens (fresh, read from cache, written to cache) arrive first, the output's last
                    "message_start" => {
                        if let Some(p) = e["message"]["provider"].as_str() {
                            host = p.to_string();
                        }
                        let u = &e["message"]["usage"];
                        input = u["input_tokens"].as_u64().unwrap_or(0);
                        cache_read = u["cache_read_input_tokens"].as_u64().unwrap_or(0);
                        cache_write = u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                    }
                    // some hosts send zeros at message_start and the whole usage here, cache reads included; others
                    // only the output. Take whatever this event has.
                    "message_delta" => {
                        // OpenRouter says what the call actually cost; prefer it to the rate table
                        if let Some(c) = e["usage"]["cost"].as_f64() {
                            billed = Some(c);
                        }
                        if let Some(r) = e["delta"]["stop_reason"].as_str() {
                            stop = r.to_string();
                        }
                        let u = &e["usage"];
                        if let Some(o) = u["output_tokens"].as_u64() {
                            output = o;
                        }
                        if let Some(i) = u["input_tokens"].as_u64().filter(|i| *i > 0) {
                            input = i;
                        }
                        if let Some(r) = u["cache_read_input_tokens"].as_u64().filter(|r| *r > 0) {
                            cache_read = r;
                        }
                        if let Some(w) = u["cache_creation_input_tokens"].as_u64().filter(|w| *w > 0) {
                            cache_write = w;
                        }
                    }
                    "content_block_start" => {
                        if first_token.is_none() {
                            first_token = Some(started.elapsed().as_secs_f64());
                        }
                        blocks.push(e["content_block"].clone());
                        partial_json.push(String::new());
                    }
                    "content_block_delta" => {
                        let (Some(b), Some(j)) = (blocks.last_mut(), partial_json.last_mut()) else { continue };
                        let d = &e["delta"];
                        match d["type"].as_str().unwrap_or("") {
                            "text_delta" => {
                                let t = d["text"].as_str().unwrap_or("");
                                let whole = format!("{}{t}", b["text"].as_str().unwrap_or(""));
                                b["text"] = json!(whole);
                                ev(BrainEvent::Text(t.to_string()));
                            }
                            "input_json_delta" => j.push_str(d["partial_json"].as_str().unwrap_or("")),
                            "thinking_delta" | "signature_delta" => {
                                let k = if d["type"] == "thinking_delta" { "thinking" } else { "signature" };
                                let whole = format!("{}{}", b[k].as_str().unwrap_or(""), d[k].as_str().unwrap_or(""));
                                b[k] = json!(whole);
                            }
                            _ => {}
                        }
                    }
                    "content_block_stop" => {
                        if let (Some(b), Some(j)) = (blocks.last_mut(), partial_json.last()) {
                            if b["type"] == "tool_use" {
                                // arguments that don't parse (cut off, or not JSON) are said to be malformed, not turned
                                // into an empty call the tool then refuses as "command is required" (GLM_NOTES.md: the
                                // agent never learned its arguments were lost)
                                b["input"] = if j.trim().is_empty() { json!({}) } else {
                                    serde_json::from_str(j).unwrap_or_else(|_| json!({"__malformed": j.chars().take(160).collect::<String>()}))
                                };
                            }
                        }
                    }
                    // a provider can refuse inside a 200 stream too (CoreWeave's 422 "Input should be a valid string")
                    "error" => {
                        keep_refused(&body, "stream");
                        return Err(anyhow!("model: {}", e["error"]));
                    }
                    _ => {}
                }
            }
        }
        // empty text blocks are refused when sent back
        blocks.retain(|b| b["type"] != "text" || !b["text"].as_str().unwrap_or("").is_empty());
        let cost = billed.or_else(|| crate::telemetry::cost(&self.provider.model, input, output, cache_read, cache_write));
        let said: String = blocks.iter().map(|b| match b["type"].as_str() {
            Some("text") => b["text"].as_str().unwrap_or("").to_string(),
            Some("tool_use") => format!("\n[{} {}]", b["name"].as_str().unwrap_or(""), b["input"]),
            _ => String::new(),
        }).collect();
        let asked = self.history.last().map(|m| m["content"].to_string()).unwrap_or_default();
        span.llm(&self.provider.label, &self.provider.model, input, output, cache_read, cache_write, cost.unwrap_or(0.0))
            .io(&asked, &said)
            .set("raea.priced", cost.is_some())
            .set("llm.stop_reason", stop.as_str())
            .set("llm.host", host.as_str())
            .set("raea.time_to_first_token_s", first_token.unwrap_or(0.0))
            .set("raea.latency_s", started.elapsed().as_secs_f64());
        span.end();
        self.last_stop = stop;
        totals.rounds += 1;
        totals.input += input;
        totals.output += output;
        totals.cache_read += cache_read;
        totals.cache_write += cache_write;
        totals.cost += cost.unwrap_or(0.0);
        Ok(blocks)
    }

    /// A reply that calls the object on screen ready (or done, finished, shipped) while its record says otherwise: the
    /// note that sends it back. None when the reply claims nothing, or the record agrees.
    async fn ready_gate(&mut self, reply: &str) -> Option<String> {
        let r = reply.to_lowercase();
        let claims = (r.contains("ready") && !r.contains("not ready") && !r.contains("draft")) || ["it's done", "it is done", "all done", "finished", "ship it", "ready to ship"].iter().any(|w| r.contains(w));
        if !claims {
            return None;
        }
        let folder = self.open.lock().unwrap().as_ref().map(|s| s.folder())?;
        let (j, code) = crate::host::project(&folder, &["objects".into()], true).await;
        let rows: Vec<Value> = serde_json::from_str(&j).ok().filter(|_| code == 0)?;
        let me = rows.iter().find(|o| o["current"] == true).or(rows.first())?;
        let state = me["state"].as_str().unwrap_or("");
        if matches!(state, "ready" | "delivered") {
            return None;
        }
        Some(format!("(void) You called {} ready, but its record says {state}{}: ready is set by the checks, not said. Either run `raea check --full`, fix what fails (or waive each failure in writing with a reason), and `raea task state ID ready`; or present it as a draft: what changed, the version, the check in a line, what you assumed, and what you need from the person.",
            me["title"].as_str().unwrap_or("it"), me["check"].as_str().map(|c| format!(" ({c})")).unwrap_or_default()))
    }

    /// The gate's measurements on a render of the project as it stands: `raea check` (host.rs renders it and measures
    /// the cuts, the picture, the manifest, and at a turn's end the render pack's model checks). A block of notes for the
    /// model; empty when it all measures clean.
    async fn measured(&mut self, turn: &mut raea_otel::Span, tag: &str, _ev: &mut dyn FnMut(BrainEvent)) -> Vec<String> {
        let Some(space) = self.open.lock().unwrap().clone() else { return vec![] };
        let said = crate::host::handle(&space.folder(), "check", &json!({"full": tag != "mid"})).await.map(|v| v.as_str().unwrap_or("").to_string()).unwrap_or_else(|e| format!("raea check: {e}"));
        turn.set(&format!("raea.cut_gate.{tag}"), said.lines().nth(1).unwrap_or(""));
        if said.contains("PASS  no faults") { vec![] } else { vec![format!("raea check, run for you:\n{said}")] }
    }

    async fn hands(&mut self, name: &str, input: &Value, ev: &mut dyn FnMut(BrainEvent)) -> anyhow::Result<String> {
        let s = |k: &str| input[k].as_str().map(str::to_string).ok_or_else(|| anyhow!("{k} is required"));
        let open = || self.open.lock().unwrap().clone().ok_or_else(|| anyhow!("no object is on screen: open_object or new_object first"));
        match name {
            "panel_action" => crate::space_ui::act(input["panel"].as_str(), &s("action")?, &input["args"]).map_err(|e| anyhow!(e)),
            "list_objects" => {
                let current = self.open.lock().unwrap().as_ref().map(|s| s.slug.clone());
                Ok(spaces::list_in(&self.spaces).iter()
                    .map(|s| format!("{}{}", s.name, if Some(&s.slug) == current.as_ref() { " (open)" } else { "" }))
                    .collect::<Vec<_>>().join("\n"))
            }
            "open_object" => {
                let mut space = spaces::find_in(&self.spaces, &s("name")?).ok_or_else(|| anyhow!("no object called that; list_objects shows them"))?;
                space.touch();
                *self.open.lock().unwrap() = Some(space.clone());
                let _ = crate::host::project(&space.folder(), &["object".into(), "open".into(), space.slug.clone()], false).await;
                ev(BrainEvent::Open(space.clone()));
                Ok(format!("{} is on screen; the shell works in its folder now", space.name))
            }
            "new_object" => {
                let mut space = spaces::create_in(&self.spaces, &s("name")?)?;
                space.kind = input["kind"].as_str().map(str::to_string).or(Some("video".into()));
                spaces::set_shape(&space)?;
                // in the window's workspace, raea init's project comes with the space; elsewhere (evals, `void tool`)
                // raea init lays it out here
                if crate::workspace::current().is_some() { spaces::scaffold(&space)?; } else { spaces::blank(&space)?; }
                if !space.folder().join("edit.tsx").exists() {
                    let mut t = crate::shell::Terminal::open(&space.folder()).await?;
                    t.exec("raea init", 120_000).await?;
                }
                space.touch();
                *self.open.lock().unwrap() = Some(space.clone());
                let _ = crate::host::project(&space.folder(), &["object".into(), "open".into(), space.slug.clone()], false).await;
                ev(BrainEvent::Open(space.clone()));
                Ok(format!("made {} (a {}) and put it on screen: an empty React + TypeScript project (edit.tsx, cuts.ts, components/); the shell works in its folder now. Set its size and rate from the request and the footage", space.name, space.kind.as_deref().unwrap_or("video")))
            }
            "brand" => {
                let space = open()?;
                // brand kits come through raea's API (RAEA_API): the host's token, else an eval session
                let token = api_session().await?;
                let api = format!("{}/v1/brands", raea_api()?);
                let mut q = s("brand")?.trim().to_lowercase();
                if !q.contains('.') {
                    // a name: the best domain match
                    let found: Value = self.client.get(format!("{api}/search")).query(&[("q", q.as_str()), ("mode", "match")])
                        .bearer_auth(&token).header("user-agent", "raea-void/0.1").send().await?.json().await?;
                    q = found["results"][0]["domain"].as_str().ok_or_else(|| anyhow!("no brand called {q}"))?.to_string();
                }
                let kit: Value = self.client.post(format!("{api}/retrieve")).json(&json!({"domain": q}))
                    .bearer_auth(&token).header("user-agent", "raea-void/0.1").send().await?.json().await?;
                let b = &kit["brand"];
                anyhow::ensure!(b.is_object(), "no brand kit for {q}: {}", kit.to_string().chars().take(200).collect::<String>());
                let dir = space.folder().join("media").join("brand");
                std::fs::create_dir_all(&dir)?;
                let (mut files, mut logos) = (Vec::new(), Vec::new());
                for l in b["logos"].as_array().into_iter().flatten().take(6) {
                    let Some(url) = l["url"].as_str() else { continue };
                    let ext = url.rsplit('.').next().filter(|e| e.len() <= 4).unwrap_or("png");
                    let name = format!("{q}-{}-{}.{ext}", l["type"].as_str().unwrap_or("logo"), l["mode"].as_str().unwrap_or("any"));
                    let bytes = self.client.get(url).send().await?.bytes().await?;
                    std::fs::write(dir.join(&name), &bytes)?;
                    files.push(format!("media/brand/{name}  ({}, for {} backgrounds, {}x{})", l["type"].as_str().unwrap_or("logo"),
                        match l["mode"].as_str() { Some("dark") => "light", Some("light") => "dark", _ => "any" }, l["width"], l["height"]));
                    logos.push(json!({"file": format!("media/brand/{name}"), "type": l["type"], "mode": l["mode"], "width": l["width"], "height": l["height"]}));
                }
                // the site's tokens (context.dev, as the host app's kit uses them); parts/brand.ts writes the space's brand kit:
                // brand/styleguide.json, brand/brand.css (the host app's format), brand/brand.json (what parts read), BRAND.md
                let sg: Value = match self.client.get(format!("{api}/styleguide")).query(&[("domain", q.as_str())]).bearer_auth(&token)
                    .header("user-agent", "raea-void/0.1").timeout(std::time::Duration::from_secs(90)).send().await {
                    Ok(r) => r.json::<Value>().await.map(|v| v["styleguide"].clone()).unwrap_or(Value::Null), Err(_) => Value::Null };
                let input = space.folder().join(".brand.in.json");
                std::fs::write(&input, json!({"brand": b, "styleguide": sg, "logos": logos}).to_string())?;
                // parts/brand.ts in process on raea's JavaScriptCore with Node's core (raea-native js::run_node), as bun ran it
                let (script, args) = (std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("parts/brand.ts"), vec![space.folder().display().to_string(), input.display().to_string()]);
                let ran = tokio::task::spawn_blocking(move || raea_native::js::run_node(&script, &args, String::new(), std::time::Duration::from_secs(60))).await.map_err(|e| anyhow!("brand kit: {e}"))?;
                let _ = std::fs::remove_file(&input);
                anyhow::ensure!(ran.exit == 0, "brand kit: {}", ran.stderr.lines().take(6).collect::<Vec<_>>().join(" | "));
                Ok(format!("{} ({q}){}\ncolours: {}\nlogos:\n  {}\n{}",
                    b["name"].as_str().unwrap_or(&q),
                    b["slogan"].as_str().map(|s| format!(": \"{s}\"")).unwrap_or_default(),
                    b["colors"].as_array().map(|c| c.iter().filter_map(|x| x.as_str().or(x["hex"].as_str())).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
                    if files.is_empty() { "none".to_string() } else { files.join("\n  ") },
                    ran.stdout.trim()))
            }
            "shell" | "browser" => {
                let space = open()?;
                if !self.shell.as_mut().is_some_and(|t| t.alive() && t.root() == space.folder()) {
                    self.shell = Some(crate::shell::Terminal::open(&space.folder()).await?);
                }
                let term = self.shell.as_mut().unwrap();
                if name == "shell" && input["undo"].as_bool() == Some(true) {
                    let v = term.ask(json!({"op": "undo"}), 60).await?;
                    ev(BrainEvent::Changed);
                    return Ok(format!("took back: {}", v["row"]["cmd"].as_str().map(|c| c.trim_start_matches("undo ")).unwrap_or("the last command")));
                }
                let cmd = if name == "browser" { format!("browser {}", s("args")?) } else { s("command")? };
                // a minute by default; a `raea` command renders or checks (a 20 s edit renders in about a minute), so 20
                let raea = cmd.split(|c: char| !c.is_alphanumeric() && c != '-').any(|w| w == "raea");
                let limit = if raea { 1200 } else { 300 };
                let out = term.exec(&cmd, input["timeout_s"].as_u64().unwrap_or(if raea { 1200 } else { 60 }).min(limit) * 1000).await?;
                // a screenshot comes back as a picture
                if name == "browser" && cmd.split_whitespace().nth(1) == Some("screenshot") {
                    if let Some(p) = cmd.split_whitespace().nth(2).map(|p| space.folder().join(p)).filter(|p| p.exists()) {
                        self.images.push(crate::stage::jpeg_b64(&p, 720)?);
                    }
                }
                ev(BrainEvent::Changed);
                Ok(out.chars().take(12_000).collect())
            }
            "search_assets" => {
                let folder = open()?.folder();
                let q = s("query")?;
                let hits = tokio::task::spawn_blocking(move || {
                    let _ = crate::assets::reindex(&folder);
                    crate::assets::search(&folder, &q, true)
                }).await?;
                Ok(if hits.is_empty() { "nothing in this space matches".into() } else {
                    hits.iter().take(12).map(|h| format!("{}  {:.2}  {}", h.rel, h.score, h.why)).collect::<Vec<_>>().join("\n")
                })
            }
            "game_library" => {
                // raea-meaning's games-find / games-fetch (the shell has no network and no raea-meaning)
                let bin = std::env::var("RAEA_MEANING").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../meaning/target/release/raea-meaning").into());
                let mut c = tokio::process::Command::new(&bin);
                let fetch = input["fetch"].as_str().map(str::to_string);
                if let Some(id) = &fetch {
                    anyhow::ensure!(id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'), "fetch takes a record id");
                    c.args(["games-fetch", id, "--into"]).arg(open()?.folder().join("media/models").join(id));
                    if let Some(f) = input["file"].as_str() {
                        c.args(["--file", f]);
                    }
                } else {
                    c.args(["games-find", &s("query")?, "-k", "8"]);
                    if let Some(k) = input["kind"].as_str().filter(|k| !k.is_empty()) {
                        c.args(["--kind", k]);
                    }
                }
                let out = c.output().await?;
                anyhow::ensure!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr).trim());
                let mut text = String::from_utf8_lossy(&out.stdout).to_string();
                if let (Some(id), Ok(v)) = (&fetch, serde_json::from_str::<Value>(&text)) {
                    if let Some(f) = v["file"].as_str() {
                        text.push_str(&format!("\nuse it as <Model src={{staticFile(\"models/{id}/{f}\")}} />; keep the attribution with the piece"));
                    }
                }
                ev(BrainEvent::Changed);
                Ok(text.chars().take(12_000).collect())
            }
            "media" => {
                let root = open()?.folder().join("media");
                let mut lines = Vec::new();
                let mut walk = vec![(root.clone(), 0usize)];
                while let Some((dir, depth)) = walk.pop() {
                    let mut entries: Vec<_> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
                    entries.sort();
                    for p in entries {
                        let rel = p.strip_prefix(open()?.folder()).unwrap_or(&p).display().to_string();
                        if p.is_dir() {
                            if depth < 2 { walk.push((p, depth + 1)); }
                            continue;
                        }
                        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                        if !["mov", "mp4", "m4v", "m4a", "wav", "mp3", "png", "jpg"].contains(&ext.as_str()) { continue; }
                        let probe = media_verb("ffprobe")
                            .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height:format=duration", "-of", "default=nw=1"]).arg(&p)
                            .output().await.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
                        let get = |k: &str| probe.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").to_string();
                        let secs: f64 = get("duration").parse().unwrap_or(0.0);
                        let words = p.with_file_name("words.json").exists() || p.with_extension("words.json").exists();
                        let size = if get("width").is_empty() { String::new() } else { format!(", {}x{}", get("width"), get("height")) };
                        lines.push(format!("{rel}  {secs:.1} s{size}{}", if words { ", words ready" } else { "" }));
                    }
                }
                Ok(if lines.is_empty() { "no media in this space".into() } else { lines.join("\n") })
            }
            "craft" => {
                let name = s("name")?;
                anyhow::ensure!(CRAFT.contains(&name.as_str()), "no craft file {name}: one of {}", CRAFT.join(", "));
                craft_file(&name).ok_or_else(|| anyhow!("prompts/craft/{name}.md is missing"))
            }
            "guide" => {
                let topic = s("topic")?;
                let (_, rel) = GUIDES.iter().find(|g| g.0 == topic).ok_or_else(|| anyhow!("no guide {topic}"))?;
                let root = setting("RAEA_GUIDES").map(std::path::PathBuf::from)
                    .ok_or_else(|| anyhow!("no guides: set RAEA_GUIDES to a folder of craft references ({rel} for {topic})"))?;
                std::fs::read_to_string(root.join(rel)).map_err(|e| anyhow!("{}: {e}", root.join(rel).display()))
            }
            "review" => {
                let space = open()?;
                // the project's own render (raea render), then clef-omni scores it
                let said = crate::host::handle(&space.folder(), "render", &json!({})).await?;
                let said = said.as_str().unwrap_or("").to_string();
                anyhow::ensure!(said.starts_with("final.mp4"), "render: {}", said.lines().take(8).collect::<Vec<_>>().join(" | "));
                let video = space.folder().join("final.mp4");
                // the agent's own rubric (prompts/craft/rubric.md) against this brand and this piece's direction, never
                // the grader's: the review can't tune the agent to the eval. clef-omni scores it (eval::review)
                let read = |f: &str| std::fs::read_to_string(space.folder().join(f)).unwrap_or_default();
                let (v, cost) = crate::eval::review(&video, &craft_file("rubric").unwrap_or_default(), &read("BRAND.md"), &read("DIRECTION.md"), &s("asked")?).await?;
                self.tool_cost += cost;
                anyhow::ensure!(v["total"].is_number(), "the reviewer didn't answer in the rubric's form: {}", v["raw"].as_str().map(str::to_string).unwrap_or_else(|| v.to_string()).chars().take(400).collect::<String>());
                let total = v["total"].as_f64().unwrap_or(0.0);
                // the rubric's bands: 90+ ship, 70-89 fix the P1s and ship, below that a pass or a rethink is owed
                let verdict = if total >= 90.0 { "ship" } else if total >= 70.0 { "fix the P1s" } else if total >= 50.0 { "a real pass is owed" } else { "rethink a stage" };
                let scores: Vec<String> = v["scores"].as_object().into_iter().flatten().map(|(k, x)| format!("{k} {}", x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string()))).collect();
                let mut out = format!("{total:.0}% overall, {verdict}  ({})\n", scores.join(" · "));
                for i in v["issues"].as_array().into_iter().flatten() {
                    out += &format!("- {} {} {} -> {}\n", i["p"].as_str().unwrap_or(""), i["at"].as_str().unwrap_or(""), i["issue"].as_str().unwrap_or(""), i["fix"].as_str().unwrap_or(""));
                }
                if let Some(b) = v["works"].as_str() { out += &format!("works: {b}\n"); }
                Ok(out)
            }
            "look" => {
                let space = open()?;
                let paths: Vec<String> = input["paths"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(String::from)).take(4).collect();
                anyhow::ensure!(!paths.is_empty(), "paths is required: image files in the project (raea still, raea media make them)");
                let mut seen = Vec::new();
                for p in &paths {
                    let f = space.folder().join(p.trim_start_matches('/'));
                    anyhow::ensure!(f.starts_with(space.folder()) && f.exists(), "no image at {p}");
                    self.images.push(crate::stage::jpeg_b64(&f, 720)?);
                    seen.push(p.as_str());
                }
                Ok(format!("showing {}", seen.join(", ")))
            }
            other => Err(anyhow!("no tool {other}")),
        }
    }
}

/// One tool on the space at `dir`, without a model turn (`void tool`): the provider is resolved as for a turn, so
/// tools that call the host app (brand, make()) work.
pub async fn run_tool(dir: &std::path::Path, name: &str, input: &Value) -> anyhow::Result<String> {
    let provider = Provider::resolve().await?;
    let space = Space::at(dir, "tool");
    let mut brain = Brain::new(provider, Arc::new(Mutex::new(Some(space))));
    brain.spaces = dir.parent().map(|p| p.to_path_buf()).unwrap_or_else(spaces::dir);
    brain.hands(name, input, &mut |_| {}).await
}

/// What the person did to a space's files outside a turn (the Files tab's imports and removals), keyed by the
/// space's folder and told to the agent at the start of its next turn there.
static NOTICES: Mutex<Vec<(std::path::PathBuf, String)>> = Mutex::new(Vec::new());

pub(crate) fn notice(folder: &std::path::Path, line: impl Into<String>) {
    if let Ok(mut n) = NOTICES.lock() {
        n.push((folder.to_path_buf(), line.into()));
    }
}

fn take_notices(folder: &std::path::Path) -> Vec<String> {
    let Ok(mut n) = NOTICES.lock() else { return Vec::new() };
    let (mine, rest): (Vec<_>, Vec<_>) = n.drain(..).partition(|(f, _)| f == folder);
    *n = rest;
    mine.into_iter().map(|(_, l)| l).collect()
}

/// The name a file takes in the space: spaces as dashes, no brackets, and a take folder's generic video.mov named for
/// its folder, so two takes don't collide.
pub(crate) fn media_name(src: &std::path::Path) -> String {
    let mut file: String = src.file_name().map(|n| n.to_string_lossy().replace(' ', "-").replace(['(', ')'], "")).unwrap_or("clip".into());
    if file.starts_with("video.") {
        if let Some(dir) = src.parent().and_then(|d| d.file_name()) {
            file = format!("{}-{file}", dir.to_string_lossy());
        }
    }
    file
}

/// Speech in an imported file: Parakeet over the whole of `dest` (or words already made for `src`: a the host app take's
/// words.json, or <stem>.words.json beside it), kept beside `dest` as <stem>.words.json for captions. The words, the
/// length in seconds (the last word's end when the words were already made) and the words file. Shared by
/// `import_media` and the Files tab (assets.rs), so a file brought in either way is heard the same.
pub(crate) async fn listen(src: &std::path::Path, dest: &std::path::Path) -> anyhow::Result<(Vec<raea_void::voice::stt::Word>, f64, std::path::PathBuf)> {
    let file = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = file.rsplit_once('.').map(|p| p.0).unwrap_or(&file).to_string();
    let words_path = dest.with_file_name(format!("{stem}.words.json"));
    let src_stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let known = [src.with_file_name(format!("{src_stem}.words.json")), src.with_file_name("words.json")]
        .into_iter()
        .find_map(|p| serde_json::from_str::<Vec<Value>>(&std::fs::read_to_string(p).ok()?).ok())
        .map(|ws| ws.iter().map(|w| raea_void::voice::stt::Word {
            text: w["text"].as_str().unwrap_or("").to_string(),
            start: w["start_ms"].as_f64().unwrap_or(0.0) / 1000.0,
            end: w["end_ms"].as_f64().unwrap_or(0.0) / 1000.0,
        }).collect::<Vec<_>>());
    let (words, secs) = match known {
        Some(ws) => {
            let secs = ws.last().map(|w| w.end).unwrap_or(0.0);
            (ws, secs)
        }
        None => tokio::task::spawn_blocking({
            let dest = dest.to_path_buf();
            move || -> anyhow::Result<(Vec<raea_void::voice::stt::Word>, f64)> {
                let out = media_blocking("ffmpeg")
                    .args(["-v", "error", "-i"]).arg(&dest).args(["-f", "f32le", "-ac", "1", "-ar", "16000", "-"]).output()?;
                anyhow::ensure!(out.status.success(), "ffmpeg: {}", String::from_utf8_lossy(&out.stderr).trim());
                let audio: Vec<f32> = out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                let dir = raea_void::voice::models::ensure(&raea_void::voice::Parakeet::SPEC, &mut |_, _| {})?;
                let mut stt = raea_void::voice::Parakeet::load(&dir)?;
                Ok((stt.words(&audio)?, audio.len() as f64 / 16_000.0))
            }
        }).await??,
    };
    let json_words: Vec<Value> = words.iter().map(|w| json!({"text": w.text, "start_ms": (w.start * 1000.0).round(), "end_ms": (w.end * 1000.0).round()})).collect();
    std::fs::write(&words_path, serde_json::to_string(&json_words)?)?;
    Ok((words, secs, words_path))
}

/// A few words on what a tool call is doing, for the transcript.
fn brief(name: &str, input: &Value) -> String {
    let first = |k: &str| input[k].as_str().unwrap_or("").lines().next().unwrap_or("").chars().take(80).collect::<String>();
    match name {
        "open_object" | "new_object" => first("name"),
        "search_assets" => input["query"].as_str().unwrap_or("").to_string(),
        "game_library" => input["fetch"].as_str().or(input["query"].as_str()).unwrap_or("").to_string(),
        "look" => input["paths"].as_array().map(|p| p.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        "guide" => first("topic"),
        "craft" => first("name"),
        "brand" => first("brand"),
        "shell" => first("command"),
        "browser" => first("args"),
        "panel_action" => first("action"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    /// compaction: old long tool results become summaries (or head and tail), the last 12 messages stay as they were
    #[tokio::test]
    async fn compacts_old_tool_results() {
        let provider = super::Provider { base: String::new(), key: String::new(), model: "test".into(), label: String::new(), bearer: false };
        let mut b = super::Brain::new(provider, std::sync::Arc::new(std::sync::Mutex::new(None)));
        let long = (0..120).map(|i| format!("media/take{i:02}.mov  12.4 s  1080x1920  raea cut take{i:02} \"line {i}\"\n")).collect::<String>();
        for i in 0..11 {
            b.history.push(serde_json::json!({"role": "assistant", "content": [{"type": "tool_use", "id": format!("t{i}"), "name": "shell", "input": {"command": "raea media"}}]}));
            b.history.push(serde_json::json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": format!("t{i}"), "content": long.clone()}]}));
        }
        let mut span = raea_otel::Tracer::new("test", "test", Default::default()).root("compact");
        b.compact(&mut span).await;
        let text = |m: usize| b.history[m]["content"][0]["content"].as_str().unwrap_or("").to_string();
        assert!(text(1).starts_with("[compacted"), "the oldest result is compacted");
        assert!(text(1).len() < long.len() / 2, "and much shorter: {}", text(1).len());
        assert_eq!(text(21), long, "the newest results stay as they were");
        assert_eq!(b.compacted_at, 22);
    }

    #[test]
    fn stills_pruned() {
        use serde_json::json;
        let look = |n: usize| json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t", "content":
            std::iter::once(json!({"type": "text", "text": "frames"})).chain((0..n).map(|_| json!({"type": "image", "source": {}}))).collect::<Vec<_>>()}]});
        let images = |h: &[serde_json::Value]| h.iter().map(|m| m.to_string().matches("\"image\"").count()).sum::<usize>();
        // up to 10 kept as they are; past 10, back to the newest 6 (in a batch, so the prompt cache breaks rarely)
        let mut h = vec![look(4), look(4)];
        super::prune_stills(&mut h);
        assert_eq!(images(&h), 8);
        h.push(look(4));
        super::prune_stills(&mut h);
        assert_eq!(images(&h), 6);
        assert_eq!(h[2].to_string().matches("\"image\"").count(), 4, "the newest stills are the ones kept");
    }

    #[test]
    fn offers_the_procedures_tools() {
        let t = super::cached_tools(None);
        let names: Vec<&str> = t.as_array().unwrap().iter().filter_map(|x| x["name"].as_str()).collect();
        assert_eq!(names, vec!["open_object", "new_object", "game_library", "shell", "look"]);
        assert!(super::system(None).contains("React + TypeScript project"));
    }
}
