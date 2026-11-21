//! `raea judge`: a review's sheets (`raea render --review DIR`) judged by clef-omni on the frozen rubric (rubric.rs),
//! fresh every round and blind to the code, with the pass decided here in code. Until it is calibrated against Shane's
//! eye (`raea judge eye`, `raea judge calibrate`, eye.rs) its verdict is advice: `gating` says which.
//!
//!   raea judge DIR [--by clef|kimi|claude] [--brief brief.ts] [--target T.png] [--json] [--dry]
//!   raea judge eye DIR composition=6 form=5 ... [--pass yes|no]     (or --by kimi: an agent's reading as the eye)
//!   raea judge calibrate [--json]
//!
//! DIR/judge.json is the verdict; DIR's parent's judge.log.jsonl holds every round's, so "done" (two passes in a row,
//! each a fresh call) and the stall (the mean up by less than a point over two rounds) are read from it.

mod agent;
mod eye;
pub mod rubric;

pub use agent::BY;

use std::path::{Path, PathBuf};

use raea_core_facts::{Finding, Level, finding};
use raea_runtime_bundle::flag;
use serde_json::{Value, json};

pub const USAGE: &str = "raea judge DIR [--by clef|kimi|claude] [--brief brief.ts] [--target T.png] [--json] [--dry] | judge eye DIR (CATEGORY=SCORE ... [--pass yes|no] | --by kimi|claude) | judge calibrate [--json]   (a --review folder's sheets judged blind: clef-omni on the frozen rubric, or a fresh kimi or claude session on the judge prompt; the pass decided in code; clef advises until calibrated against your eye or an agent's)";

/// One call through raea's one clef client (raea-io-clef: the person's plan first, then their own Cloudflare keys),
/// with the rubric's questions as written and each sheet as a JPEG still.
fn ask(state: &str, questions: &Value, images: &[PathBuf]) -> Result<(Value, f64), String> {
    let qs = questions.as_object().ok_or("judge: the questions aren't a map")?;
    let questions = qs.iter().map(|(id, w)| raea_core_ask::Question::from_wire(id, w).ok_or(format!("judge: {id} isn't one of clef's question types"))).collect::<Result<_, _>>()?;
    let stills = images.iter().map(|p| raea_io_clef::media::still(p, None)).collect::<Result<_, _>>()?;
    let call = raea_io_clef::Call { state: state.into(), questions, media: raea_io_clef::Media { stills, ..Default::default() } };
    let a = raea_io_clef::ask(&call).map_err(|f| f.to_string())?;
    Ok((Value::Object(a.answers), a.cost))
}

/// The brief's words, from `raea spec brief.ts --as brief` (look, logline, notDoing), or nothing.
fn brief_of(path: Option<&str>) -> Result<Value, String> {
    let Some(p) = path else { return Ok(json!({})) };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = std::process::Command::new(exe).args(["spec", p, "--as", "brief"]).output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let start = text.find('{').ok_or_else(|| format!("judge: {p} didn't read as a brief: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("")))?;
    let v: Value = serde_json::from_str(&text[start..]).map_err(|e| format!("judge: {p}: {e}"))?;
    Ok(v.get("brief").cloned().unwrap_or(v))
}

/// The rounds judged so far beside this one (DIR's parent's judge.log.jsonl).
fn log_of(dir: &Path) -> PathBuf {
    dir.parent().unwrap_or(dir).join("judge.log.jsonl")
}

/// Done and stall, from the log's rounds (oldest first), in code.
pub fn progress(rounds: &[Value]) -> Value {
    let passes = rounds.iter().rev().take_while(|r| r["pass"] == true).count();
    let means: Vec<f64> = rounds.iter().filter_map(|r| r["mean"].as_f64()).collect();
    let stall = means.len() >= 3 && {
        let best_before = means[..means.len() - 2].iter().cloned().fold(f64::MIN, f64::max);
        let best_now = means[means.len() - 2..].iter().cloned().fold(f64::MIN, f64::max);
        best_now - best_before < 1.0
    };
    // the same blocking defect present in the last two rounds
    let named = |r: &Value| -> Vec<String> { rubric::blocking(r["defects"].as_array().map(Vec::as_slice).unwrap_or_default()).into_iter().map(str::to_string).collect() };
    let again: Vec<String> = match rounds {
        [.., a, b] => named(a).into_iter().filter(|d| named(b).contains(d)).collect(),
        _ => Vec::new(),
    };
    json!({"rounds": rounds.len(), "passesInARow": passes, "done": passes >= 2, "stall": stall || !again.is_empty(), "again": again})
}

pub fn cli(args: &[String]) -> Result<String, String> {
    match args.get(1).map(String::as_str) {
        Some("eye") => return eye::record(args),
        Some("calibrate") => return eye::calibrate(args),
        Some(_) => {}
        None => return Err(USAGE.into()),
    }
    let dir = PathBuf::from(&args[1]);
    let (colour, clay, hero) = (dir.join("colour.png"), dir.join("clay.png"), dir.join("hero-colour.png"));
    for f in [&colour, &clay] {
        if !f.is_file() {
            return Err(format!("judge: no {} (make the sheets with raea render EDIT.tsx --review {})", f.display(), dir.display()));
        }
    }
    let brief = brief_of(flag(args, "--brief").as_deref())?;
    let target = flag(args, "--target").map(PathBuf::from);
    let dry = args.iter().any(|a| a == "--dry");
    let by = flag(args, "--by").unwrap_or_else(|| "clef".into());
    if by != "clef" {
        if dry {
            return Ok(agent::prompt(&dir, &brief, target.as_deref()));
        }
        let v = agent::ask(&by, &dir, &brief, target.as_deref())?;
        let (scores, defects, photo) = agent::read(&v);
        return finish(args, &dir, scores.clone(), scores, defects, photo, 0.0, &by, false, json!({"blind": v["blind"], "show_as_is": v["show_as_is"]}));
    }
    let mut calls = Vec::new();
    for (sheet, img) in [("colour", &colour), ("clay", &clay)] {
        let mut images = vec![img.clone()];
        if sheet == "colour" {
            images.extend(target.clone());
        }
        calls.push((sheet, rubric::state(sheet, &brief), rubric::questions(sheet), images));
    }
    if hero.is_file() {
        calls.push(("blind", "One frame. Judge only what is in the pixels.".into(), rubric::blind(), vec![hero.clone()]));
    }
    if dry {
        let shown: Vec<Value> = calls.iter().map(|(s, st, q, im)| json!({"sheet": s, "state": st, "questions": q, "images": im.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()})).collect();
        return Ok(serde_json::to_string_pretty(&json!({"would_ask": shown})).unwrap_or_default());
    }
    let mut scores = serde_json::Map::new();
    let mut defects = Vec::new();
    let mut photo = Value::Null;
    let mut cost = 0.0;
    for (sheet, st, q, images) in &calls {
        let (answers, c) = ask(st, q, images)?;
        cost += c;
        if *sheet == "blind" {
            photo = json!(answers["photo"]["noul"].as_f64().map(|p| (p * 1000.0).round() / 1000.0));
            continue;
        }
        let (s, d) = rubric::read(&answers);
        scores.extend(s);
        defects.extend(d.into_iter().map(|mut x| {
            x["sheet"] = json!(sheet);
            x
        }));
    }
    let raw = scores.clone();
    let cal = eye::calibration();
    if let Some(c) = &cal {
        eye::apply(c, &mut scores);
    }
    let gating = cal.as_ref().is_some_and(|c| c["gate"] == true);
    finish(args, &dir, scores, raw, defects, photo, cost, "clef", !gating, json!({"calibrated": cal.is_some()}))
}

/// The defects a judge saw, as raea's one Finding (docs/direction.feature DR3): a blocking one an error and a detail a
/// warn, each a level lower from a judge that only advises. A judge sees pictures, not parts: a finding names its
/// sheet in its words and is about no entity.
pub fn findings(defects: &[Value], by: &str, advice: bool) -> Vec<Finding> {
    let found = defects.iter().filter(|d| d["present"] == true).map(|d| {
        let blocking = d["level"] == "blocking";
        let level = match (blocking, advice) {
            (true, false) => Level::Error,
            (true, true) | (false, false) => Level::Warn,
            (false, true) => Level::Note,
        };
        let sheet = d["sheet"].as_str().unwrap_or("");
        let check = format!("judge.{}", if by == "clef" { d["defect"].as_str().unwrap_or("defect") } else { sheet });
        let mut f = Finding::new(&check, level, format!("{sheet} sheet: {}", d["asked"].as_str().unwrap_or("")), &format!("raea judge ({by})"));
        if let Some(p) = d["p"].as_f64() {
            f = f.measure("p", p, "", Some(0.5));
        }
        match d["fix"].as_str() {
            Some(x) if !x.is_empty() => f.fix(x),
            _ => f,
        }
    });
    finding::ranked(found.collect())
}

/// The verdict decided in code, written (DIR/judge.json for clef, DIR/judge.BY.json for an agent; every round in the
/// log), and said. `advice` marks a judge not yet trusted to gate (clef before calibration).
#[allow(clippy::too_many_arguments)]
fn finish(args: &[String], dir: &Path, scores: serde_json::Map<String, Value>, raw: serde_json::Map<String, Value>, defects: Vec<Value>, photo: Value, cost: f64, by: &str, advice: bool, extra: Value) -> Result<String, String> {
    let (pass, why) = rubric::pass(&scores, &defects);
    let vals: Vec<f64> = scores.values().filter_map(Value::as_f64).collect();
    let mean = if vals.is_empty() { 0.0 } else { (vals.iter().sum::<f64>() / vals.len() as f64 * 100.0).round() / 100.0 };
    let model = if by == "clef" { "@cf/cloudflare/clef-omni".to_string() } else { format!("{by} cli") };
    let found = findings(&defects, by, advice);
    let mut verdict = json!({"dir": dir.display().to_string(), "by": by, "scores": scores, "raw": raw, "mean": mean, "defects": defects, "photo": photo,
        "pass": pass, "why": why, "gating": !advice, "cost": cost, "model": model, "findings": found, "count": finding::count(&found)});
    if let (Some(v), Some(e)) = (verdict.as_object_mut(), extra.as_object()) {
        v.extend(e.clone());
    }
    let log = log_of(dir);
    let mut rounds: Vec<Value> = std::fs::read_to_string(&log).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).filter(|r: &Value| r["dir"] != verdict["dir"]).collect();
    rounds.push(verdict.clone());
    verdict["progress"] = progress(&rounds);
    let file = if by == "clef" { "judge.json".to_string() } else { format!("judge.{by}.json") };
    std::fs::write(dir.join(file), serde_json::to_string_pretty(&verdict).unwrap_or_default()).map_err(|e| e.to_string())?;
    let lines: Vec<String> = rounds.iter().map(|r| serde_json::to_string(&json!({"dir": r["dir"], "pass": r["pass"], "mean": r["mean"], "defects": r["defects"]})).unwrap_or_default()).collect();
    std::fs::write(&log, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    if cost > 0.0 {
        let _ = std::process::Command::new("antsy").args(["spend", &format!("{cost:.6}"), "raea judge (clef-omni)"]).output();
    }
    if args.iter().any(|a| a == "--json") {
        return Ok(serde_json::to_string_pretty(&verdict).unwrap_or_default());
    }
    let who = if by == "clef" { String::new() } else { format!("({by}) ") };
    let mut t = vec![format!("{}  {who}{}{}", if pass { "PASS" } else { "NOT YET" }, if advice { "(advice: the judge isn't calibrated against your eye yet) " } else { "" }, why.join("; "))];
    t.push(format!("scores: {}", scores.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")));
    for f in &found {
        let p = f.measure.as_ref().map(|m| format!(" (p {})", m.value)).unwrap_or_default();
        t.push(format!("  {}{p}", f.line()));
    }
    let p = &verdict["progress"];
    t.push(format!("round {}: {} passes in a row{}{}; ${cost:.4}", p["rounds"], p["passesInARow"], if p["done"] == true { ", done" } else { "" }, if p["stall"] == true { ", stalled: change the approach for the part, not the dose" } else { "" }));
    t.push(format!("raea judge: {}", finding::count(&found)));
    Ok(t.join("\n"))
}

#[cfg(test)]
mod tests;
