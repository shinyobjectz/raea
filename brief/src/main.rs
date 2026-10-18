//! Track F1 (the roadmap): a brief split into beats, each a phrase, the kind of shot, what Perception should find
//! in the footage for it, and the words of the brief it comes from, written as rows (beats.json) that F2 looks up in
//! the source table. Splitting free text is the agent's job, so this asks the agent's own model (MiniMax-M3 through
//! raea's API, as the project Agent does), then checks what came back against the brief itself:
//!   grounded: every beat quotes the brief (its `from` found in it, word for word up to case and spacing)
//!   accounted: every unit of the brief (a line, a bullet, or a clause of free text) is used by a beat or set aside
//!     with a reason (a date, a price, a rate: logistics no shot can show)
//!   vocabulary: kinds and finds from the fixed lists F2 searches by
//!
//!   raea-beats split BRIEF.md [-o BEATS.json]      one brief
//!   raea-beats grade DIR                            every *.md in DIR split and checked; a JSON summary
//!   raea-beats brief BEATS.json [-o OUT.brief]      the beats as a shared .brief (format.md): each beat an open beat,
//!                                                   its words and what it came from, and every unit set aside, as comments

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const MODEL: &str = "MiniMax-M3";
const KINDS: [&str; 5] = ["to-camera", "b-roll", "product", "screen", "end-card"];
const FINDS: [&str; 9] = ["face", "speech", "product", "hands", "screen", "text", "place", "action", "logo"];

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

fn token() -> Result<String, String> {
    if let Some(t) = setting("RAEA_API_TOKEN") {
        return Ok(t);
    }
    let api = setting("RAEA_API").ok_or("RAEA_API is not set (raea's API, in the environment or ~/.raea/env)")?;
    let key = setting("RAEA_EVAL_KEY").ok_or("no RAEA_API_TOKEN and no RAEA_EVAL_KEY for raea's API")?;
    let res = reqwest::blocking::Client::new().post(format!("{}/v1/dev/eval-session", api.trim_end_matches('/'))).header("x-eval-key", key).send().map_err(|e| e.to_string())?;
    let body: Value = res.json().map_err(|e| e.to_string())?;
    body["token"].as_str().map(String::from).ok_or_else(|| format!("eval-session answered {body}"))
}

/// The brief's units: each non-empty line that isn't a heading, bullets on their own, and free text cut at sentence
/// ends, semicolons and commas (a call's notes run one ask into the next).
fn units(brief: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in brief.lines() {
        let l = l.trim().trim_start_matches(['-', '*']).trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        // a full stop or comma ends a clause only before a space ("$3.49", "1,200" stay whole)
        let cut = l.replace(". ", "\n").replace("; ", "\n").replace(", ", "\n");
        for part in cut.split('\n').map(|p| p.trim_end_matches(['.', ';', ','])) {
            let p = part.trim();
            if p.split_whitespace().count() >= 2 {
                out.push(p.to_string());
            }
        }
    }
    out
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '$' { c } else { ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ask(token: &str, brief: &str) -> Result<Value, String> {
    let units = units(brief);
    let numbered: String = units.iter().enumerate().map(|(i, u)| format!("{}. {u}\n", i + 1)).collect();
    let prompt = format!(r#"You plan the shots of a short-form video from a brief. Split the brief into beats, in the order a viewer sees them.

Each beat is one moment of the video:
- "phrase": what happens, in plain words (what is said or shown)
- "kind": one of {kinds}
- "find": what must be visible or audible in the footage for this beat, from {finds} (one or more)
- "said": the words spoken, if the brief gives them (else "")
- "from": the words of the brief this beat comes from, copied exactly (a phrase of the brief, not a paraphrase)
- "units": the numbers of the brief's units below that this beat uses

Then list every unit no beat uses in "set_aside", each with a short reason (a launch date, a price, a deliverable count, an instruction to you).
Every unit is either in some beat's "units" or in "set_aside".

Answer with JSON only: {{"beats": [...], "set_aside": [{{"unit": N, "reason": "..."}}]}}

The brief:
{brief}

Its units:
{numbered}"#, kinds = KINDS.join(", "), finds = FINDS.join(", "));
    let body = json!({"model": MODEL, "max_tokens": 4096, "messages": [{"role": "user", "content": prompt}]});
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(180)).build().map_err(|e| e.to_string())?;
    let api = setting("RAEA_API").ok_or("RAEA_API is not set (raea's API, in the environment or ~/.raea/env)")?;
    let res = client.post(format!("{}/v1/ai/anthropic/v1/messages", api.trim_end_matches('/'))).header("x-api-key", token).header("anthropic-version", "2023-06-01")
        .json(&body).send().map_err(|e| e.to_string())?;
    let status = res.status();
    let v: Value = res.json().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{status}: {v}"));
    }
    let text: String = v["content"].as_array().map(|a| a.iter().filter_map(|c| c["text"].as_str()).collect()).unwrap_or_default();
    let start = text.find('{').ok_or("no JSON in the answer")?;
    let end = text.rfind('}').ok_or("no JSON in the answer")?;
    let mut out: Value = serde_json::from_str(&text[start..=end]).map_err(|e| format!("{e}: {}", &text[start..=end.min(start + 400)]))?;
    out["units"] = json!(units);
    out["model"] = json!(MODEL);
    Ok(out)
}

/// What came back, checked against the brief.
fn check(brief: &str, b: &Value) -> Value {
    let n = norm(brief);
    let units = b["units"].as_array().cloned().unwrap_or_default();
    let beats = b["beats"].as_array().cloned().unwrap_or_default();
    let grounded = beats.iter().filter(|x| x["from"].as_str().is_some_and(|f| !norm(f).is_empty() && n.contains(&norm(f)))).count();
    let vocab = beats.iter().filter(|x| {
        KINDS.contains(&x["kind"].as_str().unwrap_or("")) && x["find"].as_array().is_some_and(|f| !f.is_empty() && f.iter().all(|q| FINDS.contains(&q.as_str().unwrap_or(""))))
    }).count();
    let mut used = vec![false; units.len()];
    let mut aside = vec![false; units.len()];
    for x in &beats {
        for u in x["units"].as_array().into_iter().flatten() {
            if let Some(i) = u.as_u64().and_then(|i| (i as usize).checked_sub(1)).filter(|i| *i < units.len()) {
                used[i] = true;
            }
        }
    }
    for s in b["set_aside"].as_array().into_iter().flatten() {
        if let Some(i) = s["unit"].as_u64().and_then(|i| (i as usize).checked_sub(1)).filter(|i| *i < units.len()) {
            aside[i] = true;
        }
    }
    let accounted = (0..units.len()).filter(|&i| used[i] || aside[i]).count();
    json!({"beats": beats.len(), "grounded": grounded, "vocabulary": vocab, "units": units.len(), "accounted": accounted,
           "used": used.iter().filter(|u| **u).count(), "set_aside": aside.iter().filter(|u| **u).count()})
}

/// a split brief as a shared .brief: the client's ask that pieces `Uses:` and are checked against (format.feature)
fn as_brief(name: &str, b: &Value) -> String {
    let units = b["units"].as_array().cloned().unwrap_or_default();
    let unit = |i: &Value| i.as_u64().and_then(|i| units.get((i as usize).wrapping_sub(1))).and_then(|u| u.as_str()).unwrap_or("").to_string();
    let clean = |s: &str| s.replace(['[', ']'], "").replace('\n', " ").trim().to_string();
    let mut out = vec![format!("Title: {name}"), String::new(),
        format!("// the client's ask, split into beats by raea-beats ({}); a piece made from it says `Uses: {name}.brief`", b["model"].as_str().unwrap_or("?")), String::new()];
    for x in b["beats"].as_array().into_iter().flatten() {
        let find: Vec<&str> = x["find"].as_array().into_iter().flatten().filter_map(|f| f.as_str()).filter(|f| FINDS.contains(f)).collect();
        let kind = x["kind"].as_str().filter(|k| KINDS.contains(k)).unwrap_or("b-roll");
        let phrase = clean(x["phrase"].as_str().unwrap_or(""));
        out.push(if find.is_empty() { format!("~ {kind}: {phrase}") } else { format!("~ {kind}: {phrase} [find: {}]", find.join(" ")) });
        if let Some(s) = x["said"].as_str().filter(|s| !s.trim().is_empty()) { out.push(format!("// says: \"{}\"", clean(s))); }
        if let Some(f) = x["from"].as_str() { out.push(format!("// from the brief: \"{}\"", clean(f))); }
    }
    let aside: Vec<String> = b["set_aside"].as_array().into_iter().flatten()
        .map(|s| format!("// set aside: \"{}\" ({})", clean(&unit(&s["unit"])), clean(s["reason"].as_str().unwrap_or("")))).collect();
    if !aside.is_empty() { out.push(String::new()); out.extend(aside); }
    out.join("\n") + "\n"
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("brief") && args.len() >= 2 {
        let b: Value = match std::fs::read_to_string(&args[1]).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())) {
            Ok(b) => b,
            Err(e) => { eprintln!("raea-beats: {}: {e}", args[1]); std::process::exit(2); }
        };
        let name = Path::new(&args[1]).file_name().unwrap().to_string_lossy().trim_end_matches(".beats.json").to_string();
        let text = as_brief(&name, &b);
        match args.iter().position(|a| a == "-o").and_then(|i| args.get(i + 1)) {
            Some(o) => { let _ = std::fs::write(o, text); }
            None => print!("{text}"),
        }
        return;
    }
    let tok = match token() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("raea-beats: no token: {e}");
            std::process::exit(1);
        }
    };
    match args.first().map(String::as_str) {
        Some("split") if args.len() >= 2 => {
            let brief = std::fs::read_to_string(&args[1]).unwrap_or_default();
            match ask(&tok, &brief) {
                Ok(mut b) => {
                    b["check"] = check(&brief, &b);
                    let out = args.iter().position(|a| a == "-o").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| args[1].replace(".md", ".beats.json"));
                    let _ = std::fs::write(&out, serde_json::to_string_pretty(&b).unwrap());
                    println!("{}", serde_json::to_string_pretty(&b["check"]).unwrap());
                }
                Err(e) => eprintln!("raea-beats: {e}"),
            }
        }
        Some("grade") if args.len() >= 2 => {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&args[1]).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "md")).collect()).unwrap_or_default();
            files.sort();
            let mut rows = Vec::new();
            let (mut b, mut g, mut v, mut u, mut a) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for f in files {
                let brief = std::fs::read_to_string(&f).unwrap_or_default();
                let name = f.file_stem().unwrap().to_string_lossy().to_string();
                match ask(&tok, &brief) {
                    Ok(mut beats) => {
                        let c = check(&brief, &beats);
                        beats["check"] = c.clone();
                        let _ = std::fs::write(Path::new(&args[1]).join(format!("{name}.beats.json")), serde_json::to_string_pretty(&beats).unwrap());
                        b += c["beats"].as_u64().unwrap_or(0);
                        g += c["grounded"].as_u64().unwrap_or(0);
                        v += c["vocabulary"].as_u64().unwrap_or(0);
                        u += c["units"].as_u64().unwrap_or(0);
                        a += c["accounted"].as_u64().unwrap_or(0);
                        rows.push(json!({"brief": name, "check": c}));
                    }
                    Err(e) => rows.push(json!({"brief": name, "error": e})),
                }
            }
            println!("{}", serde_json::to_string_pretty(&json!({"briefs": rows.len(), "beats": b, "grounded": g, "vocabulary": v, "units": u, "accounted": a, "each": rows})).unwrap());
        }
        _ => eprintln!("raea-beats split BRIEF.md [-o BEATS.json] | grade DIR | brief BEATS.json [-o OUT.brief]"),
    }
}
