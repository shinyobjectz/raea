//! `void eval`: the agent graded on scenarios, headless (apps/void/evals). The same brain, spaces and tools (the shell with
//! `raea` in it, and look) the window uses, so a result is about the agent people use. The agent works by the TypeScript
//! procedure (prompts/sops/ts.md): each scenario's space gets `raea init`'s project.
//!
//!   void eval [SCENARIOS…|DIR] [--jobs N] [--only medium] [--run NAME]
//!
//! A scenario (apps/void/evals/scenarios/<medium>/<file>.feature, Gherkin; tags @medium:ugc @level:easy):
//!   Given the space "ugc"                     a fixture (apps/void/evals/fixtures/<name>/: media/…; an old edit.rrl stub's
//!                                              `edit {width, height, fps}` still gives the project its size), or
//!   Given an empty space
//!   When the person says "…"                  one turn; more lines make a conversation
//!   Then expect edit.width = 1080 and …       a hidden check on the render's facts (render.json: edit.width, height,
//!                                              fps, frames); checks on rows of the retired rrl are skipped
//!   And the judge sees "<rubric>" at frames 0, 45, 90   a vision model (OpenRouter) on rendered frames
//!   And the reply says "<words>"              the last reply contains them (any case)
//!   And it costs under $0.05                  the turns' model cost
//!   And it exports 6 pages                    the space's pages/ holds stills of that many frames (export_pages)
//!   And it's compared with "<reference.mp4>"  a pair judge watches ours and the reference (a creator's real edit of
//!                                              the same raws) in both orders; passes when ours is as good or better
//! Each scenario runs in its own home (~/.raea-void/evals/runs/<run>/<scenario>), traced into Phoenix's "void-evals"
//! project; its result (results/<run>.jsonl) carries the trace id. The run's summary compares with the last run.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::brain::{Brain, BrainEvent, Provider};


#[derive(Debug, Clone, Default)]
pub struct Scenario {
    pub file: PathBuf,
    pub name: String,
    pub medium: String,
    pub level: String,
    pub fixture: Option<String>,
    pub says: Vec<String>,
    pub expects: Vec<String>,
    pub judge: Option<(String, Vec<i64>)>,
    pub reply_says: Vec<String>,
    pub max_cost: Option<f64>,
    pub pages: Option<usize>,
    /// A real edit of the same footage to compare against, in pairs (the reliable form of judging).
    pub reference: Option<String>,
    /// `@design`: judged on craft even without a "the judge sees" line (every real-concept scenario)
    pub design: bool,
    /// Capabilities it needs (@needs:play): skipped until evals/capabilities.txt lists them.
    pub needs: Vec<String>,
    /// Checks on the project's record after the turns (@raea/data/project.ts): what it shows, a question asked, an
    /// object's versions, an object back where a turn left it, an object changed in a turn.
    pub record: Vec<String>,
}

/// A turn that is the person's answer to the agent's question, as `says` holds it: ANSWER marks it, then the answer
/// (a label, "#N" for the Nth option, or "other: words").
const ANSWER: char = '\u{1}';

fn quoted(s: &str) -> Option<String> {
    let a = s.find('"')?;
    let b = s.rfind('"')?;
    (b > a).then(|| s[a + 1..b].replace("\\\"", "\""))
}

/// The scenarios in one .feature file. An unknown step is an error: a scenario that can't be read isn't run.
pub fn parse(file: &Path) -> anyhow::Result<Vec<Scenario>> {
    let text = std::fs::read_to_string(file)?;
    let mut out = Vec::new();
    let mut feature_tags: Vec<String> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    let mut background: Option<String> = None;
    let mut cur: Option<Scenario> = None;
    let mut in_background = false;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('@') {
            let t: Vec<String> = line.split_whitespace().map(String::from).collect();
            if cur.is_none() && out.is_empty() && !line.contains("Scenario") && feature_tags.is_empty() && background.is_none() && !in_background {
                // tags before Feature: apply to every scenario
                tags.extend(t);
            } else {
                tags.extend(t);
            }
            continue;
        }
        if line.starts_with("Feature:") {
            feature_tags = std::mem::take(&mut tags);
            continue;
        }
        if line.starts_with("Background:") {
            in_background = true;
            continue;
        }
        if let Some(name) = line.strip_prefix("Scenario:") {
            in_background = false;
            if let Some(s) = cur.take() {
                out.push(s);
            }
            let all: Vec<String> = feature_tags.iter().chain(tags.iter()).cloned().collect();
            let tag = |k: &str| all.iter().rev().find_map(|t| t.strip_prefix(&format!("@{k}:")).map(String::from)).unwrap_or_default();
            let needs = all.iter().filter_map(|t| t.strip_prefix("@needs:").map(String::from)).collect();
            let design = all.iter().any(|t| *t == "@design");
            cur = Some(Scenario { file: file.to_path_buf(), name: name.trim().into(), medium: tag("medium"), level: tag("level"), fixture: background.clone(), needs, design, ..Default::default() });
            tags.clear();
            continue;
        }
        let keyword = ["Given ", "When ", "Then ", "And ", "But "].iter().find_map(|k| line.strip_prefix(k));
        // prose under a Feature or a Scenario (Gherkin's description) isn't a step
        let Some(step) = keyword else { continue };
        if in_background {
            if step.starts_with("the space") {
                background = quoted(step);
            } else if step.starts_with("an empty space") {
                background = None;
            } else {
                anyhow::bail!("{}:{}: a Background holds only the space: {line}", file.display(), n + 1);
            }
            continue;
        }
        let Some(s) = cur.as_mut() else { anyhow::bail!("{}:{}: a step outside a Scenario", file.display(), n + 1) };
        if step.starts_with("the space") {
            s.fixture = quoted(step);
        } else if step.starts_with("an empty space") {
            s.fixture = None;
        } else if step.starts_with("the person answers") {
            s.says.push(format!("{ANSWER}{}", quoted(step).ok_or_else(|| anyhow::anyhow!("{}:{}: what does the person answer?", file.display(), n + 1))?));
        } else if step.starts_with("the record shows") {
            s.record.push(format!("shows\t{}", quoted(step).unwrap_or_default()));
        } else if step.starts_with("a question was asked") {
            s.record.push("asked".into());
        } else if let Some(rest) = step.strip_prefix("the object ") {
            // the object "pong" has 2 versions | is back where turn 1 left it | changed in turn 3
            let name = quoted(rest).unwrap_or_default();
            let tail = rest.rsplit('"').next().unwrap_or("").trim();
            let num = |t: &str| t.split_whitespace().find_map(|w| w.parse::<u64>().ok());
            let check = if tail.starts_with("has") { num(tail).map(|k| format!("versions\t{name}\t{k}")) }
                else if tail.starts_with("is back where turn") { num(tail).map(|k| format!("back\t{name}\t{k}")) }
                else if tail.starts_with("changed in turn") { num(tail).map(|k| format!("changed\t{name}\t{k}")) }
                else { None };
            s.record.push(check.ok_or_else(|| anyhow::anyhow!("{}:{}: unknown object check: {line}", file.display(), n + 1))?);
        } else if step.starts_with("the person says") {
            s.says.push(quoted(step).ok_or_else(|| anyhow::anyhow!("{}:{}: what does the person say?", file.display(), n + 1))?);
        } else if let Some(e) = step.strip_prefix("expect ") {
            s.expects.push(format!("expect {}", e.trim()));
        } else if step.starts_with("the judge sees") {
            let rubric = quoted(step).ok_or_else(|| anyhow::anyhow!("{}:{}: the judge needs a rubric in quotes", file.display(), n + 1))?;
            let frames: Vec<i64> = step.rsplit_once("at frames").map(|(_, f)| f.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
            s.judge = Some((rubric, if frames.is_empty() { vec![0, 45, 90] } else { frames }));
        } else if step.starts_with("the reply says") {
            s.reply_says.push(quoted(step).unwrap_or_default());
        } else if let Some(c) = step.strip_prefix("it costs under $") {
            s.max_cost = c.trim().parse().ok();
        } else if step.starts_with("it's compared with") {
            s.reference = quoted(step);
        } else if let Some(n) = step.strip_prefix("it exports ").and_then(|r| r.strip_suffix(" pages")) {
            s.pages = n.trim().parse().ok();
        } else {
            anyhow::bail!("{}:{}: unknown step: {line}", file.display(), n + 1);
        }
    }
    if let Some(s) = cur.take() {
        out.push(s);
    }
    Ok(out)
}

/// Every scenario under the paths given (files or folders).
pub fn collect(paths: &[PathBuf]) -> anyhow::Result<Vec<Scenario>> {
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut stack = vec![p.clone()];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d)?.flatten() {
                    let q = e.path();
                    if q.is_dir() {
                        stack.push(q);
                    } else if q.extension().is_some_and(|x| x == "feature") {
                        files.push(q);
                    }
                }
            }
        } else {
            files.push(p.clone());
        }
    }
    files.sort();
    let mut all = Vec::new();
    for f in files {
        all.extend(parse(&f)?);
    }
    Ok(all)
}

fn evals_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("evals")
}

fn slug(s: &str) -> String {
    crate::spaces::slug(s)
}

/// Copy a fixture folder into the scenario's space. Folders are made fresh and their files linked one by one (media is
/// large and read-only), so what a run writes (a brand's logos, keyed footage, transcripts) stays in that run: linking
/// a whole media folder handed one run's output to every later one.
fn place(fixture: &Path, space: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(space)?;
    for e in std::fs::read_dir(fixture)?.flatten() {
        let from = e.path();
        let to = space.join(e.file_name());
        // a broken link or a device (a run once left a `null` link to /dev/null in the shared media) is skipped
        let Ok(real) = std::fs::canonicalize(&from) else { continue };
        if !real.is_dir() && !real.is_file() {
            continue;
        }
        // transcripts and notes are copied: the agent's shell keeps to the space and doesn't follow links out of it
        let text = real.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x, "json" | "txt" | "md" | "srt" | "vtt" | "fountain" | "csv"));
        if real.is_dir() {
            place(&real, &to)?;
        } else if real.metadata()?.len() > 4 << 20 || (!text && (fixture.file_name().is_some_and(|n| n == "media") || from.parent() != Some(fixture))) {
            std::os::unix::fs::symlink(&real, &to)?;
        } else {
            std::fs::copy(&real, &to)?;
        }
    }
    Ok(())
}

/// One scenario, run and graded. Its home is its own, so scenarios run side by side.
async fn run_one(sc: Scenario, run_dir: PathBuf, provider: Provider, rep: usize, sop: Option<String>) -> Value {
    let started = std::time::Instant::now();
    let id = format!("{}--{}{}", slug(&sc.medium), slug(&sc.name), if rep > 0 { format!("--r{rep}") } else { String::new() });
    let home = run_dir.join(&id);
    let _ = std::fs::remove_dir_all(&home);
    // the scenario's space: a fixture or an empty edit
    let space_dir = home.join("spaces").join("eval");
    let placed = match &sc.fixture {
        Some(f) => place(&evals_dir().join("fixtures").join(f), &space_dir),
        // nothing decided for the agent: `raea init` below lays out the project
        None => std::fs::create_dir_all(&space_dir).map_err(Into::into),
    };
    if let Err(e) = placed {
        return json!({"scenario": sc.name, "medium": sc.medium, "level": sc.level, "pass": false, "error": format!("fixture: {e}")});
    }
    // a neutral name: the scenario's title (\"Automate vs simulate, as a designed moment\") would tell the agent the answer
    let space = crate::spaces::Space::at(&space_dir, "eval");
    // listed in its own spaces folder, the only one this brain sees
    let _ = space.save();
    let open = std::sync::Arc::new(std::sync::Mutex::new(Some(space)));
    let mut brain = Brain::new(provider, open);
    brain.spaces = home.join("spaces");
    brain.temperature = Some(EVAL_TEMPERATURE);
    brain.rounds = std::env::var("RAEA_VOID_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(90);
    brain.self_reviews = 2;
    brain.cut_gates = 2;
    brain.stall_nudges = true;
    brain.sop = sop.clone();
    if let Some(s) = &sop {
        brain.tags.insert("raea.eval.sop".into(), json!(s));
    }
    brain.tags.insert("raea.eval.temperature".into(), json!(EVAL_TEMPERATURE));
    brain.tags.insert("raea.eval.scenario".into(), json!(sc.name));
    brain.tags.insert("raea.eval.medium".into(), json!(sc.medium));
    brain.tags.insert("raea.eval.level".into(), json!(sc.level));
    brain.tags.insert("raea.eval.run".into(), json!(run_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    // the TypeScript procedure starts from a project (edit.tsx, cuts.ts, AGENTS.md), as `raea init` makes it
    if let Ok(mut t) = crate::shell::Terminal::open(&space_dir).await {
        let _ = t.exec("raea init", 120_000).await;
    }
    let (mut cost, mut traces, mut replies, mut tools, mut refused, mut errors) = (0.0, Vec::new(), Vec::new(), 0u32, 0u32, Vec::new());
    let mut log: Vec<Value> = Vec::new();
    // the questions the agent asked, and each object's last version after each turn (for the record's checks)
    let mut asked: Vec<Value> = Vec::new();
    let mut after_turn: Vec<std::collections::HashMap<String, u64>> = Vec::new();
    let mut record_notes: Vec<String> = Vec::new();
    for said in &sc.says {
        // the person's answer to the question waiting: written to the record, and said as the app says it
        let said = match said.strip_prefix(ANSWER) {
            Some(a) => match asked.last() {
                Some(q) => {
                    let label = match a.strip_prefix('#').and_then(|k| k.parse::<usize>().ok()) {
                        Some(k) => q["options"][k.saturating_sub(1)]["label"].as_str().unwrap_or(a).to_string(),
                        None => a.to_string(),
                    };
                    let folder = brain.open.lock().unwrap().as_ref().map(|s| s.folder()).unwrap_or(space_dir.clone());
                    let id = q["id"].as_str().unwrap_or("").to_string();
                    let (out, code) = crate::host::project(&folder, &["answer".into(), id.clone(), label.clone()], true).await;
                    if code != 0 {
                        record_notes.push(format!("the scripted answer \"{label}\" was refused: {out}"));
                    }
                    let meant = q["options"].as_array().into_iter().flatten().find(|o| o["label"].as_str() == Some(label.as_str()))
                        .map(|o| format!("{}: {}", label, o["meaning"].as_str().unwrap_or(""))).unwrap_or_else(|| label.trim_start_matches("other:").trim().to_string());
                    format!("(answering {id}, \"{}\") {meant}", q["text"].as_str().unwrap_or(""))
                }
                None => {
                    record_notes.push(format!("the person answered \"{a}\" but no question was asked"));
                    a.trim_start_matches("other:").trim().to_string()
                }
            },
            None => said.clone(),
        };
        let said = &said;
        let mut reply = String::new();
        let mut ev = |e: BrainEvent| match e {
            BrainEvent::Text(t) => reply.push_str(&t),
            BrainEvent::Doing(name, what) => log.push(json!({"tool": name, "what": what})),
            BrainEvent::Done(name, ok, out) => {
                tools += 1;
                if !ok || out.starts_with("REFUSED") {
                    refused += 1;
                }
                log.push(json!({"tool": name, "ok": ok, "out": out.chars().take(1200).collect::<String>()}));
            }
            BrainEvent::Error(e) => errors.push(e),
            BrainEvent::Ask(q) => asked.push(q),
            _ => {}
        };
        brain.turn(said, &mut ev).await;
        cost += brain.last_cost;
        traces.push(brain.last_trace.clone());
        replies.push(reply);
        after_turn.push(last_versions(&space_dir).await);
    }
    // grading 0: the project's record (tasks, questions, objects and their versions), as its checks ask
    let mut record: Vec<Value> = record_notes.iter().map(|n| json!({"check": "scripted answer", "pass": false, "out": n})).collect();
    for c in &sc.record {
        let (pass, out) = record_check(&space_dir, c, &asked, &after_turn).await;
        record.push(json!({"check": c.replace('\t', " "), "pass": pass, "out": out}));
    }
    let record_ok = record.iter().all(|r| r["pass"] == true);
    let usage = brain.usage.clone();
    // the piece is the object on screen when the turns end: an agent that made a new object (new_object, as the
    // procedure says for anything new) built it there, and the scenario's first folder is still raea init's empty
    // project (2026-10-18: a 3D game built in spaces/gem-run was graded on the empty spaces/eval, 1080x1920)
    let space_dir = brain.open.lock().unwrap().as_ref().map(|s| s.folder()).filter(|f| f.join("edit.tsx").exists()).unwrap_or(space_dir);
    // the piece is the project's own render (raea render: the project through raea-motion), said the way a render
    // went ("APPLIED") so the judge reads it the same
    let video = home.join("final.mp4");
    let rendered = match crate::host::handle(&space_dir, "render", &json!({})).await {
        Ok(v) if v.as_str().is_some_and(|t| t.starts_with("final.mp4")) => std::fs::copy(space_dir.join("final.mp4"), &video).map(|_| "APPLIED (raea render)".to_string()).map_err(Into::into),
        Ok(v) => Ok(v.as_str().unwrap_or("").to_string()),
        Err(e) => Err(e),
    };
    // grading 1: the hidden expects, on the render's facts (render.json, which the render above wrote)
    let facts = render_facts(&space_dir);
    let mut checks: Vec<Value> = Vec::new();
    let mut checks_ok = true;
    for e in &sc.expects {
        match grade_expect(e, facts.as_ref()) {
            Some((pass, why)) => {
                checks_ok &= pass;
                checks.push(json!({"check": e, "pass": pass, "out": why}));
            }
            // a check on rows (captions, titles, cubes) read the retired rrl edit; a project has no rows to ask
            None => checks.push(json!({"check": e, "pass": Value::Null, "skipped": true, "out": "not checkable on a TypeScript project (it asks about rrl rows); the judge covers it"})),
        }
    }
    // grading 2: a strict design judge watches the whole video (evals/design/rubric.md): it passes only on "ship"
    let mut judge = Value::Null;
    let mut judge_ok = true;
    let mut judge_cost = 0.0;
    if sc.judge.is_some() || sc.design {
        match rendered {
            Ok(o) if o.lines().any(|l| l.starts_with("APPLIED")) => match design_judge(&video, &sc.says, sc.judge.as_ref().map(|j| j.0.as_str())).await {
                Ok((v, c)) => {
                    judge_ok = v["verdict"] == "ship" && v["overall"].as_f64().unwrap_or(0.0) >= SHIP;
                    judge_cost = c;
                    judge = v;
                }
                Err(e) => {
                    judge_ok = false;
                    judge = json!({"error": e.to_string()});
                }
            },
            other => {
                judge_ok = false;
                judge = json!({"error": format!("render: {}", other.map(|o| o.chars().take(200).collect::<String>()).unwrap_or_else(|e| e.to_string()))});
            }
        }
    }
    // grading 2c: against a creator's real edit of the same raws, in pairs, both orders
    let mut pair = Value::Null;
    let mut pair_ok = true;
    if let Some(reference) = &sc.reference {
        match (rendered_ok(&video), std::path::Path::new(reference).exists()) {
            (true, true) => match pair_judge(&video, Path::new(reference), &sc.says).await {
                Ok(v) => {
                    // and never a silent piece against a reference with sound, measured (clef barely hears it: a silent
                    // copy against its voiced original drew -0.42 to -0.73, either side of the mark)
                    pair_ok = v["score"].as_f64().unwrap_or(-9.0) >= PAIR_PASS && !(v["silent"]["ours"] == true && v["silent"]["reference"] == false);
                    pair = v;
                }
                Err(e) => { pair_ok = false; pair = json!({"error": e.to_string()}); }
            },
            (_, false) => { pair_ok = false; pair = json!({"error": format!("the reference isn't there (drive unplugged?): {reference}")}); }
            _ => { pair_ok = false; pair = json!({"error": "no render to compare"}); }
        }
    }
    let judge_cost = judge_cost + pair["cost"].as_f64().unwrap_or(0.0);
    // with a reference, the pair decides, but only between watchable pieces: a mismatched reference (an animated cut, a
    // stock-footage version) loses to anything that follows the brief, so a broken piece (the judge's 4 or less) fails
    let judge_ok = if sc.reference.is_some() { pair_ok && judge["overall"].as_f64().unwrap_or(0.0) >= 5.0 } else { judge_ok };
    let parts = Value::Null;
    let cuts = if sc.design && video.exists() { cutcheck(&space_dir, &video).await.unwrap_or_else(|e| json!({"error": e.to_string()})) } else { Value::Null };
    // grading 3 and 4: what the reply says, what it cost
    let last = replies.last().cloned().unwrap_or_default().to_lowercase();
    let says_ok = sc.reply_says.iter().all(|w| last.contains(&w.to_lowercase()));
    let cost_ok = sc.max_cost.is_none_or(|m| cost <= m);
    let pages = exported_pages(&space_dir.join("pages"));
    let pages_ok = sc.pages.is_none_or(|n| pages == n);
    // a piece that was never built is its own failure, named as such: a render of one frame or less, or no render at
    // all, means edit.tsx is still raea init's template or doesn't draw what was made (2026-10-18: a 3D game's files
    // sat unwired in game/ and the render was one empty frame, which read as a broken engine)
    let built = match facts.as_ref().map(|f| f["frames"]) {
        Some(n) if n > 1.0 => None,
        Some(n) => Some(format!("not built: the render has {n} frame(s); edit.tsx is raea init's template or doesn't draw the piece")),
        None => Some("not built: the project never rendered (no render.json)".to_string()),
    };
    let pass = errors.is_empty() && checks_ok && judge_ok && says_ok && cost_ok && pages_ok && record_ok && built.is_none();
    // a provider we pay ran out of credit (raea API's 424 provider_paused): infrastructure, not the agent
    // the model's host refusing (OpenRouter's 404/429/5xx "Provider returned error") is the provider's failure, not the
    // agent's: a whole run on 2026-10-16 failed in seconds while CoreWeave and DeepInfra were down
    let infra = errors.iter().any(|e: &String| e.contains("provider_paused") || e.contains("Buddy is paused")
        || (e.starts_with("model:") && (e.contains("Provider returned error") || e.contains("rate-limited") || e.contains("503") || e.contains("502")
            // OpenRouter's host dropping a stream ("provider_unavailable": 2026-10-18, Together's engine shut down mid-answer
            // and an ad the judge passed at 6.8 went down as a FAIL)
            || e.contains("provider_unavailable") || e.contains("Upstream error"))));
    let result = json!({
        "scenario": sc.name, "file": sc.file.strip_prefix(evals_dir()).unwrap_or(&sc.file), "medium": sc.medium, "level": sc.level,
        "pass": pass, "infra": infra, "built": built.is_none(), "not_built": built, "checks": checks, "checks_ok": checks_ok, "record": record, "record_ok": record_ok, "asked": asked, "judge": judge, "judge_ok": judge_ok, "says_ok": says_ok, "cost_ok": cost_ok, "repeat": rep, "sop": sop, "version": version(&run_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), false), "pages": pages, "pages_ok": pages_ok, "parts": parts, "parts_mean": parts["mean"], "cuts": cuts, "pair": pair,
        "errors": errors, "turns": sc.says.len(), "tools": tools, "refused": refused,
        "cost_usd": cost + judge_cost, "agent_cost_usd": cost, "judge_cost_usd": judge_cost,
        "tokens": usage, "seconds": started.elapsed().as_secs_f64(), "traces": traces, "reply": replies.last(), "log": log,
    });
    let _ = std::fs::write(home.join("result.json"), serde_json::to_string_pretty(&result).unwrap_or_default());
    if let Some(run) = run_dir.file_name() {
        to_gallery(&gallery_dir(run), &result, &space_dir.join("pages"), &sc.says, Some(&video)).await;
    }
    result
}

/// Each object's last version number now (the project's objects, through @raea/data/project.ts).
async fn last_versions(space_dir: &Path) -> std::collections::HashMap<String, u64> {
    let (j, _) = crate::host::project(space_dir, &["objects".into()], true).await;
    serde_json::from_str::<Vec<Value>>(&j).unwrap_or_default().iter()
        .filter_map(|o| Some((o["name"].as_str()?.to_string(), o["last"].as_u64().unwrap_or(0)))).collect()
}

/// One check on the project's record: whether it holds, and what was found.
async fn record_check(space_dir: &Path, check: &str, asked: &[Value], after: &[std::collections::HashMap<String, u64>]) -> (bool, String) {
    let parts: Vec<&str> = check.split('\t').collect();
    let project = |argv: Vec<String>| async move { crate::host::project(space_dir, &argv, false).await.0 };
    // an object by its slug or title, as the record names it
    let slug_of = |name: String| async move {
        let (j, _) = crate::host::project(space_dir, &["objects".into()], true).await;
        let rows = serde_json::from_str::<Vec<Value>>(&j).unwrap_or_default();
        let n = name.to_lowercase();
        rows.iter().find(|o| o["name"].as_str().is_some_and(|x| x.to_lowercase() == n) || o["title"].as_str().is_some_and(|x| x.to_lowercase().contains(&n)))
            .and_then(|o| o["name"].as_str().map(String::from))
    };
    match parts.as_slice() {
        ["asked"] => (!asked.is_empty(), asked.iter().map(|q| format!("{}: {}", q["id"].as_str().unwrap_or(""), q["text"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("; ")),
        ["shows", want] => {
            let text = format!("{}\n{}\n{}", project(vec!["status".into()]).await, project(vec!["objects".into()]).await, project(vec!["tasks".into()]).await);
            (text.to_lowercase().contains(&want.to_lowercase()), text.chars().take(600).collect())
        }
        ["versions", name, k] => match slug_of(name.to_string()).await {
            Some(o) => {
                let (j, _) = crate::host::project(space_dir, &["versions".into(), "--object".into(), o.clone()], true).await;
                let n = serde_json::from_str::<Vec<Value>>(&j).map(|v| v.len()).unwrap_or(0);
                (n as u64 >= k.parse::<u64>().unwrap_or(1), format!("{o} has {n} versions"))
            }
            None => (false, format!("no object {name}")),
        },
        // back where a turn left it: the object restored (`raea restore N`, done) to a version that turn had made. Not its
        // files at the end: the harness's own review and gates go on to change it after the restore (2026-10-17: the
        // agent restored v26, then fixed the reviewer's notes)
        ["back", name, t] => match (slug_of(name.to_string()).await, t.parse::<usize>().ok().and_then(|t| after.get(t.wrapping_sub(1)))) {
            (Some(o), Some(m)) => match m.get(&o).copied().filter(|n| *n > 0) {
                Some(n) => {
                    let log = std::fs::read_to_string(space_dir.parent().unwrap_or(space_dir).join(&o).join(".raea/actions.jsonl")).unwrap_or_default();
                    let restored: Vec<u64> = log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())
                        .filter(|r| r["exit"] == 0)
                        .filter_map(|r| r["cmd"].as_str().and_then(|c| c.split("raea restore ").nth(1)).and_then(|a| a.split_whitespace().next()).and_then(|k| k.trim_start_matches('v').parse().ok()))
                        .collect();
                    let ok = restored.iter().any(|k| *k <= n);
                    (ok, format!("{o}: turn {t} left it at v{n}; restores done: {:?}", restored))
                }
                None => (false, format!("{o} had no version after turn {t}")),
            },
            (o, _) => (false, format!("no object {name} ({o:?}) or no turn {t}")),
        },
        ["changed", name, t] => {
            let t: usize = t.parse().unwrap_or(0);
            match (slug_of(name.to_string()).await, after.get(t.wrapping_sub(1)), after.get(t.wrapping_sub(2))) {
                (Some(o), Some(now), before) => {
                    let (a, b) = (before.and_then(|m| m.get(&o)).copied().unwrap_or(0), now.get(&o).copied().unwrap_or(0));
                    (b > a, format!("{o}: v{a} before turn {t}, v{b} after"))
                }
                _ => (false, format!("no object {name} or no turn {t}")),
            }
        }
        _ => (false, format!("unknown record check {check}")),
    }
}

async fn backfill(run: &str) -> i32 {
    let run_dir = crate::spaces::home().join("evals").join("runs").join(run);
    let says: std::collections::HashMap<String, Vec<String>> = collect(&[evals_dir().join("scenarios")]).unwrap_or_default().into_iter().map(|s| (s.name, s.says)).collect();
    let mut n = 0;
    for d in std::fs::read_dir(&run_dir).into_iter().flatten().flatten() {
        let Ok(r) = std::fs::read_to_string(d.path().join("result.json")).map_err(|_| ()).and_then(|t| serde_json::from_str::<Value>(&t).map_err(|_| ())) else { continue };
        let space = d.path().join("spaces").join("eval");
        let asked = says.get(r["scenario"].as_str().unwrap_or("")).cloned().unwrap_or_default();
        to_gallery(&gallery_dir(std::ffi::OsStr::new(run)), &r, &space.join("pages"), &asked, Some(&d.path().join("final.mp4"))).await;
        n += 1;
        println!("{n}  {}", r["scenario"].as_str().unwrap_or(""));
    }
    println!("{n} videos in {}", gallery_dir(std::ffi::OsStr::new(run)).display());
    i32::from(n == 0)
}

/// Where each scenario's video lands for a person to watch: RAEA_VOID_EVAL_GALLERY, else ~/Desktop/Void Evals.
/// The trace ids Phoenix holds for the evals project since `since` (ISO time), paged.
async fn phoenix_traces(since: &str) -> anyhow::Result<std::collections::HashSet<String>> {
    let base = crate::telemetry::endpoint();
    let project = std::env::var("RAEA_VOID_TRACE_PROJECT").unwrap_or_else(|_| "void-evals".into());
    let client = reqwest::Client::new();
    let (mut seen, mut cursor) = (std::collections::HashSet::new(), None::<String>);
    loop {
        let mut q = vec![("limit", "1000".to_string()), ("start_time", since.to_string())];
        if let Some(c) = &cursor { q.push(("cursor", c.clone())); }
        let d: Value = client.get(format!("{}/v1/projects/{project}/spans", base.trim_end_matches('/'))).query(&q).send().await?.json().await?;
        for s in d["data"].as_array().into_iter().flatten() {
            if let Some(t) = s["context"]["trace_id"].as_str() { seen.insert(t.to_string()); }
        }
        cursor = d["next_cursor"].as_str().map(String::from);
        if cursor.is_none() { break }
    }
    Ok(seen)
}

/// A run's version, "M.m" in run order (~/.raea-void/evals/versions.json): every run takes the next minor, and
/// `--major` starts the next major (a new suite, procedure or agent era). The gallery folder leads with it.
fn version(run: &str, major: bool) -> String {
    let file = crate::spaces::home().join("evals").join("versions.json");
    let mut all: serde_json::Map<String, Value> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if let Some(v) = all.get(run).and_then(|v| v.as_str()) {
        return v.to_string();
    }
    let parse = |v: &str| v.split_once('.').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)));
    let (m, n) = all.values().filter_map(|v| v.as_str().and_then(parse)).max().unwrap_or((0, 0));
    let v = if major { format!("{}.0", m + 1) } else { format!("{m}.{}", n + 1) };
    all.insert(run.to_string(), json!(v));
    let _ = std::fs::write(&file, serde_json::to_string_pretty(&all).unwrap_or_default());
    v
}

/// A run's gallery folder: "v1.3 · <run>", so the Desktop sorts in run order.
fn gallery_dir(run: &std::ffi::OsStr) -> PathBuf {
    let run = run.to_string_lossy();
    gallery().join(format!("v{} · {run}", version(&run, false)))
}

fn gallery() -> PathBuf {
    std::env::var_os("RAEA_VOID_EVAL_GALLERY").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Desktop").join("Void Evals"))
}

/// The scenario's finished edit as an MP4 in the run's gallery folder, named "PASS · ugc · Yellow captions", with a
/// note beside it (what was asked, what failed), and a deck's exported pages.
async fn to_gallery(dir: &Path, r: &Value, pages: &Path, says: &[String], made: Option<&Path>) {
    let rep = r["repeat"].as_u64().unwrap_or(0);
    let name = format!(
        "{} · {} · {}{}",
        if r["pass"].as_bool().unwrap_or(false) { "PASS" } else { "FAIL" },
        r["medium"].as_str().unwrap_or("?"),
        r["scenario"].as_str().unwrap_or("?").replace('/', "-"),
        if rep > 0 { format!(" ({})", rep + 1) } else { String::new() }
    );
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let mp4 = dir.join(format!("{name}.mp4"));
    // the video the judge watched (the project's render)
    let said = match made.filter(|m| m.exists()) {
        Some(m) => std::fs::copy(m, &mp4).map(|_| "APPLIED".to_string()).unwrap_or_else(|e| e.to_string()),
        None => "no render".to_string(),
    };
    let mut note = format!("{name}\n\nasked:\n{}\n\n", says.iter().map(|s| format!("  \"{s}\"")).collect::<Vec<_>>().join("\n"));
    for c in r["checks"].as_array().into_iter().flatten().filter(|c| c["pass"] == false) {
        note += &format!("check failed: {}\n", c["check"].as_str().unwrap_or(""));
    }
    if let Some(why) = r["not_built"].as_str() {
        note += &format!("{why}\n");
    }
    let j = &r["judge"];
    if j["overall"].is_number() {
        let crit: Vec<String> = ["hook", "message", "pacing", "cuts", "captions", "visuals", "structure", "cta", "authenticity"]
            .iter().filter(|k| j[**k].is_number()).map(|k| format!("{k} {}/5", j[*k])).collect();
        note += &format!("\ndesign judge: {}/10 overall, {}\n  {}\n", j["overall"], j["verdict"].as_str().unwrap_or("?"), crit.join(" · "));
        for d in j["defects"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
            note += &format!("  - {d}\n");
        }
        if let Some(b) = j["best"].as_str() {
            note += &format!("  best: {b}\n");
        }
    }
    if let Some(cs) = r["cuts"]["issues"].as_array() {
        note += &format!("\ncuts, measured ({} joins): {}\n", r["cuts"]["cuts"], if cs.is_empty() { "clean".to_string() } else { format!("{} problems", cs.len()) });
        for c in cs.iter().filter_map(|c| c["says"].as_str()) {
            note += &format!("  - {c}\n");
        }
    }
    if let Some(ps) = r["parts"]["parts"].as_array().filter(|p| !p.is_empty()) {
        // each part's crop beside the video, named with its score
        let to = dir.join(format!("{name} parts"));
        let _ = std::fs::create_dir_all(&to);
        note += &format!("\nparts up close (mean {:.1}):\n", r["parts"]["mean"].as_f64().unwrap_or(0.0));
        for p in ps {
            let pname = p["name"].as_str().unwrap_or("?");
            if let Some(c) = p["crop"].as_str() {
                let _ = std::fs::copy(c, to.join(format!("{} {pname}.png", p["score"])));
            }
            note += &format!("  {pname} {}/10: {}\n", p["score"], p["issues"].as_array().map(|i| i.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("; ")).unwrap_or_default());
        }
    }
    if !said.lines().any(|l| l.starts_with("APPLIED")) {
        note += &format!("render: {}\n", said.lines().next().unwrap_or(""));
    }
    note += &format!("\nreply:\n{}\n\ncost ${:.4} · {:.0} s · trace {}\n", r["reply"].as_str().unwrap_or(""), r["cost_usd"].as_f64().unwrap_or(0.0), r["seconds"].as_f64().unwrap_or(0.0), r["traces"][0].as_str().unwrap_or(""));
    let _ = std::fs::write(dir.join(format!("{name}.txt")), note);
    if let Ok(files) = std::fs::read_dir(pages) {
        let to = dir.join(format!("{name} pages"));
        let _ = std::fs::create_dir_all(&to);
        for f in files.flatten() {
            let _ = std::fs::copy(f.path(), to.join(f.file_name()));
        }
    }
}

fn rendered_ok(video: &Path) -> bool {
    video.metadata().map(|m| m.len() > 10_000).unwrap_or(false)
}

// The judges below all ask clef-omni, the decision model (the founder, 2026-10-18: "we want only two models ever used
// right now. The primary model and the decision model. which is glm and clef omni"). clef answers structured
// questions only (a choice over named options, with probabilities, or a yes as a probability); it writes no text. So
// each rubric heuristic is a choice over its levels, a score is the probability-weighted level, and the defects and
// issues are written here from the heuristics that score low: their names, the rubric's own words, and the moment
// clef picks as the worst for each.

/// How long a piece the judges send: 90 s, as the Gemini judges watched (clef's checks see 59).
const JUDGE_SECS: u32 = 90;

/// The judges' media: at most 90 s of a render, shrunk as clef's checks are.
async fn judge_media(video: &Path) -> anyhow::Result<Vec<u8>> {
    crate::checks::shrink(video, JUDGE_SECS).await
}

/// A video's length in seconds (ffprobe), 0 when it can't be read.
async fn seconds(video: &Path) -> f64 {
    let out = tokio::process::Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"]).arg(video)
        .output().await.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    out.parse().unwrap_or(0.0)
}

/// The stretches of a piece clef can point at for "where": about 3 s each, ten at most, as "m:ss-m:ss".
fn stretches(secs: f64) -> Vec<String> {
    let secs = secs.min(JUDGE_SECS as f64);
    // clef refuses a choice of fewer than two options, so even a piece of a second or less (or one ffprobe can't read)
    // is split in two
    if secs <= 0.0 {
        return vec!["0:00-0:01".into(), "0:01-0:03".into()];
    }
    let n = ((secs / 3.0).ceil() as usize).clamp(2, 10);
    let at = |t: f64| format!("{}:{:02}", (t as u64) / 60, (t as u64) % 60);
    (0..n).map(|i| format!("{}-{}", at(secs * i as f64 / n as f64), at(secs * (i + 1) as f64 / n as f64))).collect()
}

/// A "where" question: the stretch of the piece where it does worst on `what`.
fn where_question(what: &str, stretches: &[String]) -> Value {
    json!({"type": "choice", "instructions": format!("Where in the video does it do worst on this: {what}"),
        "criteria": stretches.iter().map(|s| (s.clone(), Value::Null)).collect::<serde_json::Map<_, _>>()})
}

/// clef's top stretch for a "where" answer.
fn worst_at(ans: &Value) -> String {
    ans["choice"].as_str().unwrap_or("").to_string()
}

/// A choice question over named options with their descriptions.
fn choice(instructions: String, options: &[(&str, &str)]) -> Value {
    json!({"type": "choice", "instructions": instructions,
        "criteria": options.iter().map(|(k, d)| (k.to_string(), json!(d))).collect::<serde_json::Map<_, _>>()})
}

/// A rubric's "- head: text" bullets after the line holding `after` (continuation lines indented), up to the first
/// line that is neither.
fn bullets(md: &str, after: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let Some((_, rest)) = md.split_once(after) else { return out };
    for line in rest.lines().skip(1) {
        if let Some(item) = line.strip_prefix("- ") {
            let (head, text) = item.split_once(':').unwrap_or((item, ""));
            out.push((head.trim().to_string(), text.trim().to_string()));
        } else if line.starts_with(' ') && !line.trim().is_empty() && !out.is_empty() {
            let last = out.last_mut().unwrap();
            last.1 = format!("{} {}", last.1, line.trim());
        } else if !out.is_empty() {
            break;
        }
    }
    out
}

/// The first clause of a rubric line, for a defect (at most 140 characters).
fn gist(text: &str) -> String {
    let end = text.find(['.', ';']).unwrap_or(text.len());
    text[..end].chars().take(140).collect()
}

/// One clef call, traced as an evaluator span.
async fn clef_span(span_name: &str, media: &crate::checks::Media<'_>, state: &str, questions: &serde_json::Map<String, Value>) -> anyhow::Result<(Value, f64)> {
    let mut span = crate::telemetry::tracer().root(span_name);
    span.kind("EVALUATOR").set("llm.model_name", "clef-omni".to_string()).set("input.value", state.to_string());
    let r = crate::checks::ask(media, state, questions, span_name).await;
    if let Ok((a, cost)) = &r {
        span.set("output.value", a.to_string()).set("raea.cost_usd", *cost);
    }
    span.end();
    r
}

/// Two edits of the same raw footage, judged side by side as UGC ads (the host app's pair judge, docs/judge.feature): both
/// videos go to clef in one call (it takes several and tells them apart by order: a fractal clip and a test pattern
/// were picked out right in both orders, 2026-10-18), asked which is better and by how much, then again with the
/// order swapped. Score: the margin (1 slightly, 2 clearly, 3 far better), + when ours wins and - when the reference
/// does, probability-weighted and averaged over both orders, so 0 is "as good". The score blends the whole-piece answer
/// (PAIR_WHOLE) with a sound answer, the soundtracks sent as audio inputs; a side that is silent, measured, loses the
/// sound clearly (-2). The pass is PAIR_PASS, re-checked on clef's answers 2026-10-18.
async fn pair_judge(ours: &Path, reference: &Path, says: &[String]) -> anyhow::Result<Value> {
    let (a, b) = (judge_media(ours).await?, judge_media(reference).await?);
    // each soundtrack also as an audio input, in the videos' order (silence for a render with none): with only the
    // videos, clef judged a piece and its muted copy identical (2026-10-18)
    let (sa, sb) = (crate::checks::sound(ours, JUDGE_SECS).await?, crate::checks::sound(reference, JUDGE_SECS).await?);
    let margins: [(&str, &str, f64); 6] = [("first_far", "video 1 is far better", 3.0), ("first_clearly", "video 1 is clearly better", 2.0),
        ("first_slightly", "video 1 is slightly better", 1.0), ("second_slightly", "video 2 is slightly better", -1.0),
        ("second_clearly", "video 2 is clearly better", -2.0), ("second_far", "video 2 is far better", -3.0)];
    let options: Vec<(&str, &str)> = margins.iter().map(|m| (m.0, m.1)).collect();
    let mut questions = serde_json::Map::new();
    questions.insert("better".into(), choice("Which is the better piece for what the brief asks, judged as something that would run on its platform, watched with its sound: does it stop the scroll, land its message, hold attention, cut cleanly, look designed for this brand, sound right, and get the viewer to act, with nothing broken? Captions and a call to action count only where the piece calls for them; there is no one right structure. And by how much?".into(), &options));
    questions.insert("sound".into(), choice("Listen to the two soundtracks (audio 1 is video 1's, audio 2 is video 2's). Which piece sounds better for what the brief asks: the voice clear over any music, music that fits, effects on their moments, no clipping, and no silence where sound is expected? And by how much?".into(), &options));
    let state = format!("You are a senior UGC editor. Two edits of the SAME raw footage follow: video 1, then video 2, and their soundtracks as audio 1 and audio 2.\nThe brief was: {}", says.join(" / "));
    let mut verdicts = Vec::new();
    let (mut picture, mut heard, mut cost) = (0.0, 0.0, 0.0);
    let value = |ans: &Value| crate::checks::weighted(ans, &margins.iter().map(|m| (m.0, m.2)).collect::<Vec<_>>());
    for ours_first in [true, false] {
        let media = if ours_first { crate::checks::Media::Clips(vec![&a[..], &b[..]], vec![&sa[..], &sb[..]]) } else { crate::checks::Media::Clips(vec![&b[..], &a[..]], vec![&sb[..], &sa[..]]) };
        let (ans, c) = clef_span("eval.pair_judge", &media, &state, &questions).await?;
        cost += c;
        let first = value(&ans["better"]).ok_or_else(|| anyhow::anyhow!("clef gave no answer to the pair: {ans}"))?;
        let first_sound = value(&ans["sound"]).unwrap_or(0.0);
        let (ours, ours_sound) = if ours_first { (first, first_sound) } else { (-first, -first_sound) };
        picture += ours;
        heard += ours_sound;
        verdicts.push(json!({"ours_first": ours_first, "ours_won": ours > 0.0, "margin": (ours.abs() * 100.0).round() / 100.0, "choice": ans["better"]["choice"], "probabilities": ans["better"]["probabilities"], "sound": (ours_sound * 100.0).round() / 100.0, "sound_choice": ans["sound"]["choice"]}));
    }
    let (picture, mut heard) = (picture / 2.0, heard / 2.0);
    // one side silent and the other not: the silent one is clearly worse on sound, measured (clef doesn't hear it)
    let (quiet_ours, quiet_ref) = (crate::checks::silent(ours, JUDGE_SECS).await, crate::checks::silent(reference, JUDGE_SECS).await);
    if quiet_ours != quiet_ref {
        heard = if quiet_ours { -2.0 } else { 2.0 };
    }
    // the piece as a whole, with its sound given a quarter of the say
    let score = PAIR_WHOLE * picture + (1.0 - PAIR_WHOLE) * heard;
    Ok(json!({"score": score, "whole": picture, "sound": heard, "silent": {"ours": quiet_ours, "reference": quiet_ref}, "verdicts": verdicts, "reference": reference, "cost": cost, "model": "clef-omni", "provisional": true}))
}

/// The pair judge's pass: ours at least this far from the reference's side. Re-checked on clef 2026-10-18 with 18 pairs
/// of known order (apps/void/evals/results/2026-10-18-new-stack.md): a piece against itself scores 0 (clef's first-position
/// lean, 0.01 to 0.37, cancels over the two orders), against a copy blurred with 2 s of black and no sound +1.02 to
/// +1.57 (and the copy -1.02 to -1.57), so -0.5 sits between "as good" and "clearly worse". Kept.
const PAIR_PASS: f64 = -0.5;

/// The pair judge's score: this share from the whole-piece question, the rest from the sound question.
const PAIR_WHOLE: f64 = 0.75;

/// The overall score a design judge must give for a pass (its verdict is "ship" from here up). PROVISIONAL: 5.6 on
/// clef's overall (the probability-weighted band of evals/design/rubric.md's anchored scale), re-calibrated 2026-10-18
/// on the only judged renders that hold no creator's footage: 10 decks Gemini had scored. clef ranks them as Gemini did
/// (Spearman 0.82) on a compressed scale: Gemini's three ships (7, 8, 8) got 5.7, 5.8 and 5.6, the rest 5.9 (a Gemini
/// 5), 5.5 (a 6), 5.3, 4.9, 4.4 and 2.3 twice. 5.6 passes all three ships and one of the seven others. Gemini's 7.0
/// (2026-10-16, 20 of Kelly's published ads, a hand-built Reel, the agent's v1.3) would pass nothing on clef's scale.
/// Ten decks are a thin base and no UGC piece is among them: re-calibrate on our own or synthetic UGC pieces.
const SHIP: f64 = 5.6;

/// The design judge's sound questions, each over the rubric's five levels or n/a, asked of the soundtrack sent as an
/// audio input. Until 2026-10-18 the judges sent only the video, and a render and its muted copy got the same answers.
const SOUND_QUESTIONS: [(&str, &str); 4] = [
    ("voice", "Is the voice (a speaker or a voiceover) clear and easy to follow over any music or effects?"),
    ("music", "Does the music or bed fit the piece and sit at the right level?"),
    ("effects", "Do the sound effects land on their moments on screen (a hit, a jump, a pickup, a transition)?"),
    ("clean", "Is the sound clean: no clipping or distortion, no dropouts, and no silence where sound is expected?"),
];

/// Whether a piece is meant to have sound, from what was asked: an ad, a game, a video from footage or takes, or any
/// mention of a voice, music or sound. A deck, a document or a still piece isn't, and is judged from its video alone.
fn expects_sound(says: &[String], focus: Option<&str>) -> bool {
    let text = format!("{} {}", says.join(" "), focus.unwrap_or("")).to_lowercase();
    let quiet = ["slide", "deck", "document", "one-pager", "poster", "still"].iter().any(|w| text.contains(w));
    let loud = ["voice", "music", "sound", "audio", "song", "sfx", "foley", " ad ", " ad.", " ad,", "advert", "game", "footage", "take", "talking", "jingle", "beat"].iter().any(|w| text.contains(w));
    loud || !quiet
}

/// How far the sound moves the design judge's overall: points per level away from typical (3 of 5), at most one down
/// and half up.
const SOUND_WEIGHT: f64 = 0.5;

/// The level names of evals/design/rubric.md's criteria ("anchored 1-5"), with its words for each.
const DESIGN_LEVELS: [(&str, &str); 5] = [("broken", "1: broken"), ("weak", "2: weak"), ("typical", "3: typical of a working UGC ad"), ("strong", "4: strong"), ("excellent", "5: excellent")];

/// The strict design judge: the whole video (90 s at most, with its sound) to clef-omni with evals/design/rubric.md,
/// what the person asked, and what this scenario must show. Returns its scores, verdict and defects, and the call's
/// cost.
async fn design_judge(video: &Path, says: &[String], focus: Option<&str>) -> anyhow::Result<(Value, f64)> {
    watch(video, says, focus).await
}

/// The grader on clef-omni: each criterion of evals/design/rubric.md is a choice over its five levels (scored 1-5 as
/// the probability-weighted level), with a "where" question for the stretch it does worst; the overall is a choice over
/// the rubric's anchored bands (scored at the weighted band middle, 1.5 for 1-2 up to 9.5 for 9-10); what the piece
/// must show is a yes. Defects are the criteria under 2.5 (nearer weak than typical), worst first, at most six, each with
/// its stretch; a piece that doesn't show what it must is a defect and can't ship. The answer keeps the Gemini judge's
/// shape ({hook, ..., overall, verdict, defects, best}). The agent's own review doesn't use this rubric
/// (prompts/craft/rubric.md instead), so it can't tune itself to the grader.
pub async fn watch(video: &Path, says: &[String], focus: Option<&str>) -> anyhow::Result<(Value, f64)> {
    let media = judge_media(video).await?;
    let rubric = std::fs::read_to_string(evals_dir().join("design").join("rubric.md"))?;
    let criteria = bullets(&rubric, "Each criterion is anchored");
    let bands: Vec<(String, String, f64)> = bullets(&rubric, "The overall scale is anchored").into_iter().filter_map(|(head, text)| {
        let (lo, hi) = head.split_once('-')?;
        Some((head.clone(), text, (lo.trim().parse::<f64>().ok()? + hi.trim().parse::<f64>().ok()?) / 2.0))
    }).collect();
    anyhow::ensure!(!criteria.is_empty() && !bands.is_empty(), "evals/design/rubric.md: no criteria or overall bands to ask");
    let stretches = stretches(seconds(video).await);
    let intro = rubric.split("\n\n").next().unwrap_or("").replace('\n', " ");
    let mut state = format!("{intro}\n\nWhat the person asked the editor for:\n{}", says.iter().map(|s| format!("- \"{s}\"")).collect::<Vec<_>>().join("\n"));
    if let Some(f) = focus {
        state += &format!("\n\nThis piece must also show: {f}");
    }
    let mut questions = serde_json::Map::new();
    for (name, text) in &criteria {
        questions.insert(format!("c.{name}"), choice(format!("Score the video on {name}: {text}"), &DESIGN_LEVELS));
        questions.insert(format!("w.{name}"), where_question(&format!("{name}: {}", gist(text)), &stretches));
    }
    let band_options: Vec<(&str, &str)> = bands.iter().map(|b| (b.0.as_str(), b.1.as_str())).collect();
    questions.insert("overall".into(), choice("Your score for the piece as a whole on the anchored scale: your gut for the piece, not an average. A real creator's ordinary published ad is a 7. Don't mark a piece down for being plain or native if it's clean and clear; mark it down for what's broken, confusing, slow, or amateur.".into(), &band_options));
    if let Some(f) = focus {
        questions.insert("focus".into(), json!({"type": "noul", "instructions": format!("Does the piece show this: {f}?")}));
    }
    // the sound, heard, for a piece that's meant to have it: the soundtrack goes as an audio input beside the video
    // (silence when the render has none), with the sound questions. A deck or document is judged as before, from the
    // video alone: sending a silent track and sound questions moved the 10 calibration decks' overalls by up to a point
    // and broke their ranking (2026-10-18)
    let audible = expects_sound(says, focus);
    let mut sound_levels: Vec<(&str, &str)> = DESIGN_LEVELS.to_vec();
    sound_levels.push(("not_applicable", "n/a: this piece has nothing of this kind and needs none"));
    if audible {
        for (id, ask) in SOUND_QUESTIONS {
            questions.insert(format!("s.{id}"), choice(format!("Listen to the soundtrack (the audio input). {ask}"), &sound_levels));
        }
    }
    let heard = if audible { crate::checks::sound(video, JUDGE_SECS).await? } else { Vec::new() };
    let clip = if audible { crate::checks::Media::Clips(vec![&media[..]], vec![&heard[..]]) } else { crate::checks::Media::Videos(vec![&media[..]]) };
    let (a, cost) = clef_span("eval.design_judge", &clip, &state, &questions).await?;
    let levels: Vec<&str> = DESIGN_LEVELS.iter().map(|l| l.0).collect();
    let mut v = json!({});
    let mut scored: Vec<(String, f64, String, String)> = Vec::new();
    for (name, text) in &criteria {
        let Some(x) = crate::checks::expected(&a[format!("c.{name}")], &levels).map(|i| i + 1.0) else { continue };
        v[name] = json!((x * 10.0).round() / 10.0);
        scored.push((name.clone(), x, gist(text), worst_at(&a[format!("w.{name}")])));
    }
    let gut = crate::checks::weighted(&a["overall"], &bands.iter().map(|b| (b.0.as_str(), b.2)).collect::<Vec<_>>())
        .ok_or_else(|| anyhow::anyhow!("clef gave no overall: {a}"))?;
    // the sound's questions, each 1-5 or n/a (half the probability on n/a); their mean moves the overall by up to a
    // point down (broken sound) or half a point up (excellent), around 3 (typical) which leaves it as it is
    let mut sounds: Vec<(&str, f64)> = Vec::new();
    for (id, ask) in SOUND_QUESTIONS.iter().filter(|_| audible) {
        let ans = &a[format!("s.{id}")];
        if ans["probabilities"]["not_applicable"].as_f64().unwrap_or(0.0) >= 0.5 {
            continue;
        }
        if let Some(x) = crate::checks::expected(ans, &levels) {
            v[format!("sound_{id}")] = json!(((x + 1.0) * 10.0).round() / 10.0);
            sounds.push((ask, x + 1.0));
        }
    }
    // a silent render of a piece meant to have sound, measured, is broken sound (1 of 5) whatever clef answered
    let quiet = audible && crate::checks::silent(video, JUDGE_SECS).await;
    if quiet {
        sounds = vec![("is there any sound at all? the render is silent", 1.0)];
    }
    let sound = (!sounds.is_empty()).then(|| sounds.iter().map(|s| s.1).sum::<f64>() / sounds.len() as f64);
    let overall = sound.map_or(gut, |s| (gut + ((s - 3.0) * SOUND_WEIGHT).clamp(-1.0, 0.5)).clamp(1.0, 10.0));
    scored.sort_by(|x, y| x.1.total_cmp(&y.1));
    let mut defects: Vec<String> = Vec::new();
    let shows = focus.map(|_| a["focus"]["noul"].as_f64().unwrap_or(0.0));
    if let (Some(f), Some(p)) = (focus, shows.filter(|p| *p < 0.5)) {
        defects.push(format!("doesn't show what it must: {f} (p {p:.2})"));
    }
    for (ask, x) in sounds.iter().filter(|s| s.1 < 2.5) {
        defects.push(format!("sound {x:.1} of 5: {}", ask.trim_end_matches('?')));
    }
    defects.extend(scored.iter().filter(|s| s.1 < 2.5).map(|(name, x, text, at)| format!("{at} {name} {x:.1} of 5: short of \"{text}\"")).take(6usize.saturating_sub(defects.len())));
    let verdict = if overall >= SHIP && shows.is_none_or(|p| p >= 0.5) { "ship" } else if overall >= 5.0 { "fix" } else { "reject" };
    v["overall"] = json!((overall * 10.0).round() / 10.0);
    v["gut"] = json!((gut * 10.0).round() / 10.0);
    v["sound"] = json!(sound.map(|s| (s * 10.0).round() / 10.0));
    v["silent"] = json!(quiet);
    v["verdict"] = json!(verdict);
    v["defects"] = json!(defects);
    v["best"] = json!(scored.last().filter(|s| s.1 >= 3.5).map(|(name, x, _, _)| format!("{name} ({x:.1} of 5)")).unwrap_or_default());
    v["model"] = json!("clef-omni");
    Ok((v, cost))
}

/// The playbook each heuristic of prompts/craft/rubric.md points to when it scores low (the rubric names playbooks for
/// issues: `refine/pace`, `refine/typeset`, a return to `stages/direct`), matched on the heuristic's name.
const REVIEW_FIXES: [(&str, &str); 10] = [("first second", "refine/punch"), ("one thing", "refine/distill"), ("proof", "stages/strategy"),
    ("close", "stages/strategy"), ("rhythm", "refine/pace"), ("read under", "refine/layout"), ("words on screen", "refine/typeset"),
    ("sound", "refine/sound"), ("one world", "stages/direct"), ("specificity", "stages/direct")];

/// prompts/craft/rubric.md's levels (0-4), and n/a for a heuristic the mode makes meaningless.
const REVIEW_LEVELS: [(&str, &str); 5] = [("absent_or_broken", "0: absent or broken"), ("weak", "1: weak"), ("works_but_generic", "2: works but generic"),
    ("strong_and_specific", "3: strong and specific"), ("nothing_to_add", "4: nothing to add")];

/// The agent's own review (the `review` tool) on clef-omni: each of the rubric's ten heuristics (prompts/craft/
/// rubric.md, `rubric`) is a choice over its levels 0-4 or n/a, with the brand, the piece's direction and the ask as
/// clef's state, and a "where" question for its worst stretch. A heuristic is n/a when clef puts half its probability
/// there; otherwise it scores the probability-weighted level. Issues are the heuristics under 2.5, worst first, with the
/// rubric's severity read off the score (under 0.5 broken: P0; under 1.5 weak, a viewer leaves: P1; under 2.5 generic,
/// costs some attention: P2) and the playbook that fixes it. Answers {scores, total, issues, works} as the Gemini
/// reviewer did.
pub async fn review(video: &Path, rubric: &str, brand: &str, direction: &str, asked: &str) -> anyhow::Result<(Value, f64)> {
    let media = judge_media(video).await?;
    let section = rubric.split_once("## The ten heuristics").map(|x| x.1).unwrap_or("");
    let section = section.split("\n## ").next().unwrap_or("");
    let mut heuristics: Vec<(String, String)> = Vec::new();
    for line in section.lines() {
        let numbered = line.split_once(". **").filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if let Some((_, rest)) = numbered {
            let (name, text) = rest.split_once("**").unwrap_or((rest, ""));
            heuristics.push((name.trim().trim_end_matches('.').to_string(), text.trim().to_string()));
        } else if line.starts_with(' ') && !line.trim().is_empty() {
            if let Some(last) = heuristics.last_mut() { last.1 = format!("{} {}", last.1, line.trim()); }
        }
    }
    anyhow::ensure!(!heuristics.is_empty(), "prompts/craft/rubric.md: no heuristics to ask");
    let stretches = stretches(seconds(video).await);
    let state = format!("A short video an editor made, to be watched with sound and scored from what is on screen and in the ear.\n\nThe brand (BRAND.md):\n{}\n\nThe piece's own direction (DIRECTION.md):\n{}\n\nWhat the person asked for:\n- \"{asked}\"",
        brand.chars().take(3000).collect::<String>(), direction.chars().take(2000).collect::<String>());
    let mut options: Vec<(&str, &str)> = REVIEW_LEVELS.to_vec();
    options.push(("not_applicable", "n/a: this kind of piece makes the heuristic meaningless (a game has no ask; a deck has no hook frame)"));
    let mut questions = serde_json::Map::new();
    for (i, (name, text)) in heuristics.iter().enumerate() {
        questions.insert(format!("h{i}"), choice(format!("{name}. {text}"), &options));
        questions.insert(format!("w{i}"), where_question(&format!("{name}. {}", gist(text)), &stretches));
    }
    // the soundtrack heard too, as an audio input (the rubric's sound heuristic was judged from the video alone)
    let heard = crate::checks::sound(video, JUDGE_SECS).await?;
    let (a, cost) = clef_span("agent.review", &crate::checks::Media::Clips(vec![&media[..]], vec![&heard[..]]), &state, &questions).await?;
    let levels: Vec<&str> = REVIEW_LEVELS.iter().map(|l| l.0).collect();
    let mut scores = serde_json::Map::new();
    let mut scored: Vec<(usize, f64)> = Vec::new();
    for (i, (name, _)) in heuristics.iter().enumerate() {
        let ans = &a[format!("h{i}")];
        if ans["probabilities"]["not_applicable"].as_f64().unwrap_or(0.0) >= 0.5 || (ans["probabilities"].is_null() && ans["choice"] == "not_applicable") {
            scores.insert(name.clone(), json!("n/a"));
            continue;
        }
        let Some(x) = crate::checks::expected(ans, &levels) else { continue };
        scores.insert(name.clone(), json!((x * 10.0).round() / 10.0));
        scored.push((i, x));
    }
    anyhow::ensure!(!scored.is_empty(), "clef scored none of the rubric's heuristics: {a}");
    let total = scored.iter().map(|s| s.1).sum::<f64>() / (4.0 * scored.len() as f64) * 100.0;
    scored.sort_by(|x, y| x.1.total_cmp(&y.1));
    let issues: Vec<Value> = scored.iter().filter(|s| s.1 < 2.5).take(8).map(|&(i, x)| {
        let (name, text) = &heuristics[i];
        let p = if x < 0.5 { "P0" } else if x < 1.5 { "P1" } else { "P2" };
        let lname = name.to_lowercase();
        let fix = REVIEW_FIXES.iter().find(|f| lname.contains(f.0)).map(|f| f.1).unwrap_or("stages/critique");
        json!({"p": p, "at": worst_at(&a[format!("w{i}")]), "issue": format!("{name} scores {x:.1} of 4: short of \"{}\"", gist(text)), "fix": fix})
    }).collect();
    let works = scored.last().filter(|s| s.1 >= 3.0).map(|&(i, x)| format!("{} ({x:.1} of 4)", heuristics[i].0)).unwrap_or_default();
    Ok((json!({"scores": scores, "total": total, "issues": issues, "works": works, "model": "clef-omni"}), cost))
}

/// The cuts of a rendered edit, measured rather than judged (a video model samples about a frame a second and can't
/// see one stray frame): shot changes in the render that sit beside one of the edit's clip boundaries but not on it
/// (a jump cut left in the source flashing a frame or two), frozen frames (a hung frame at a join), sound at a cut
/// (both sides of the join loud: a word clipped), and dead air inside the talk. Returns {issues: [{frame, kind, says}]}.
/// Words of a take that a clip's in or out point falls inside (60 ms into the word or more: transcript timings are
/// good to a few tens of ms) while the take is sounding there (its own audio, not the mix), as (word, edit frame, "in"
/// or "out"). Takes come from `use "path" as name` lines, transcripts from path.words.json or words.json beside it.
/// A word of a take's transcript: its text, start and end in seconds.
pub(crate) type Word = (String, f64, f64);

/// A clip that plays a take with a transcript: its words, its file, src_in and src_out in seconds, from and to in frames.
pub(crate) type Spoken = (std::sync::Arc<Vec<Word>>, std::path::PathBuf, f64, f64, i64, i64);

fn clipped_words(spoken: &[Spoken]) -> Vec<(String, i64, &'static str)> {
    let mut out = Vec::new();
    for (words, media, src_in, src_out, from, to) in spoken.iter().cloned() {
        for (edge, at, frame) in [("in", src_in, from), ("out", src_out, to)] {
            let Some(w) = words.iter().find(|w| at > w.1 + 0.06 && at < w.2 - 0.06) else { continue };
            // and the take is really sounding there: transcript spans can swallow the pause after a word, so a join in
            // silence would read as inside one. Loud is 30% of the word's own level, 40 ms either side of the cut
            let level = |a: f64, b: f64| -> f64 {
                let out = std::process::Command::new("ffmpeg").args(["-v", "error", "-ss", &format!("{:.3}", a.max(0.0)), "-t", &format!("{:.3}", (b - a).max(0.02)), "-i"]).arg(&media)
                    .args(["-f", "f32le", "-ac", "1", "-ar", "16000", "-"]).output();
                let pcm: Vec<f32> = out.map(|o| o.stdout.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()).unwrap_or_default();
                if pcm.is_empty() { return 0.0 }
                (pcm.iter().map(|x| (x * x) as f64).sum::<f64>() / pcm.len() as f64).sqrt()
            };
            let word = level(w.1, w.2);
            if word > 0.0 && level(at - 0.04, at) > 0.3 * word && level(at, at + 0.04) > 0.3 * word {
                out.push((w.0.clone(), frame, edge));
            }
        }
    }
    out
}

/// The piece's last spoken line stopping mid-sentence: the last word the last spoken clip keeps doesn't end a sentence
/// and the speaker goes straight on (the next word within 0.8 s). Judges call it "cuts off mid-sentence" (3 of 28 renders
/// on 2026-10-16). Answers the word, the frame, and where the sentence does end (the word and seconds more of the take).
fn unfinished_end(spoken: &[Spoken]) -> Option<(String, i64, String, f64)> {
    let (words, _, src_in, src_out, _, to) = spoken.iter().cloned().max_by_key(|c| c.5)?;
    // the last word the clip starts to play: kept whole, or cut partway (a word cut in half ends nothing)
    let last = words.iter().rposition(|w| w.1 < src_out - 0.06 && w.1 >= src_in - 0.06)?;
    let ends = |w: &Word| w.0.ends_with(['.', '!', '?', '…']);
    let cut = words[last].2 > src_out + 0.06;
    if !cut && (ends(&words[last]) || words.get(last + 1).is_none_or(|n| n.1 - words[last].2 > 0.8)) {
        return None;
    }
    let last = if cut && ends(&words[last]) { return Some((words[last].0.clone(), to, words[last].0.clone(), words[last].2 - src_out)) } else { last };
    let end = words[last + 1..].iter().position(ends).map(|i| &words[last + 1 + i])?;
    Some((words[last].0.clone(), to, end.0.clone(), end.2 - src_out))
}

/// The render's facts the hidden expects ask about: render.json's width, height and fps, and its frame count. None
/// when the project never rendered.
fn render_facts(project: &Path) -> Option<std::collections::BTreeMap<&'static str, f64>> {
    let m: Value = serde_json::from_str(&std::fs::read_to_string(project.join("render.json")).ok()?).ok()?;
    let mut f = std::collections::BTreeMap::new();
    f.insert("width", m["width"].as_f64()?);
    f.insert("height", m["height"].as_f64()?);
    f.insert("fps", m["fps"].as_f64().unwrap_or(30.0));
    f.insert("frames", m["frames"].as_array().map(|a| a.len() as f64)?);
    Some(f)
}

/// One hidden expect graded on the render's facts: `expect edit.width = 1080 and edit.frames >= 562` (clauses joined
/// by "and", each `edit.COLUMN OP NUMBER`). (pass, what was measured), or None when it asks about anything else (rows
/// of the retired rrl edit: captions, titles, `if caption C`).
fn grade_expect(e: &str, facts: Option<&std::collections::BTreeMap<&'static str, f64>>) -> Option<(bool, String)> {
    let body = e.trim().strip_prefix("expect ")?.trim();
    let mut clauses = Vec::new();
    for c in body.split(" and ") {
        let t: Vec<&str> = c.split_whitespace().collect();
        let [lhs, op, rhs] = t.as_slice() else { return None };
        let col = lhs.strip_prefix("edit.")?;
        if !matches!(col, "width" | "height" | "fps" | "frames") || !matches!(*op, "=" | "!=" | ">=" | "<=" | ">" | "<") {
            return None;
        }
        clauses.push((col, *op, rhs.parse::<f64>().ok()?));
    }
    let Some(f) = facts else { return Some((false, "no render.json: the project never rendered".into())) };
    let mut said = Vec::new();
    let mut pass = true;
    for (col, op, want) in clauses {
        let got = f[col];
        let ok = match op { "=" => (got - want).abs() < 1e-6, "!=" => (got - want).abs() >= 1e-6, ">=" => got >= want, "<=" => got <= want, ">" => got > want, _ => got < want };
        pass &= ok;
        said.push(format!("edit.{col} is {got}{}", if ok { "" } else { " (FAIL)" }));
    }
    Some((pass, format!("{}  {}", if pass { "PASS" } else { "FAIL" }, said.join(", "))))
}

/// A TypeScript project's cuts measured on a render (`void cutcheck`, the render pack's cuts rule): `edit` is the
/// project's folder or a file in it.
pub async fn cutcheck(edit: &Path, video: &Path) -> anyhow::Result<Value> {
    crate::host::measure_cuts(&crate::host::root_of(edit), video, true).await
}

/// The joins and the ending of a render, measured, from its clips' spans in frames and the clips that play a take with
/// a transcript: a TypeScript project's cuts.json (`raea check`, host.rs).
pub(crate) async fn measure_cuts(video: &Path, fps: f64, frames: i64, mut spans: Vec<(i64, i64)>, spoken: &[Spoken]) -> anyhow::Result<Value> {
    let mut bounds: Vec<i64> = spans.iter().flat_map(|c| [c.0, c.1]).collect();
    bounds.sort();
    bounds.dedup();
    let mut issues: Vec<Value> = Vec::new();
    let t = |f: i64| format!("{}:{:04.1}", f / (fps as i64 * 60), (f as f64 / fps) % 60.0);
    // 0. frames no clip covers: black (it reads as a freeze in the render, so name it from the edit). Only for
    // footage edits: a deck, motion piece or game has no clips, and every frame of it is drawn
    let footage = !spans.is_empty();
    if spans.is_empty() {
        spans.push((0, frames));
    }
    spans.sort();
    let mut covered = 0i64;
    for (f, to) in spans.iter().cloned().chain([(frames, frames)]) {
        if f > covered + 2 && covered < frames {
            let end = f.min(frames);
            issues.push(json!({"frame": covered, "kind": "black", "says": format!("{} no clip covers frames {covered}-{end} ({:.1} s of black): fill it with a clip, or move the clips after it up and shorten the edit", t(covered), (end - covered) as f64 / fps)}));
        }
        covered = covered.max(to);
    }
    // 1. shot changes in the render beside a join
    let out = tokio::process::Command::new("ffmpeg").args(["-nostats", "-i"]).arg(video)
        .args(["-vf", "select='gt(scene,0.08)',showinfo", "-an", "-f", "null", "-"]).output().await?;
    let shots: Vec<i64> = String::from_utf8_lossy(&out.stderr).split("pts_time:").skip(1)
        .filter_map(|s| s.split_whitespace().next()?.parse::<f64>().ok()).map(|s| (s * fps).round() as i64).collect();
    for &b in &bounds {
        for &s in shots.iter().filter(|&&s| s != b && (s - b).abs() <= 3) {
            let (side, fix) = if s < b { ("before", "end the clip before it a frame or two earlier") } else { ("after", "start the clip after it a frame or two later") };
            issues.push(json!({"frame": s, "kind": "stray frame", "says": format!("{} a shot change {} frame(s) {side} the cut at frame {b}: a jump cut in the source flashes; {fix}", t(s), (s - b).abs())}));
        }
    }
    // 2. frozen frames (a hung frame at a join, or a held picture)
    let out = tokio::process::Command::new("ffmpeg").args(["-nostats", "-i"]).arg(video)
        .args(["-vf", "freezedetect=n=0.002:d=0.12", "-an", "-f", "null", "-"]).output().await?;
    let log = String::from_utf8_lossy(&out.stderr);
    let starts: Vec<f64> = log.split("freeze_start: ").skip(1).filter_map(|s| s.split_whitespace().next()?.parse().ok()).collect();
    let durs: Vec<f64> = log.split("freeze_duration: ").skip(1).filter_map(|s| s.split_whitespace().next()?.parse().ok()).collect();
    for (s, d) in starts.iter().zip(durs.iter()) {
        let f = (s * fps).round() as i64;
        let near = bounds.iter().find(|&&b| (b - f).abs() <= 3);
        // a slide or a held title is still on purpose; a freeze is a fault only in footage
        if footage && (near.is_some() || *d >= 0.6) {
            issues.push(json!({"frame": f, "kind": "frozen", "says": format!("{} the picture freezes for {:.2} s{}", t(f), d, near.map(|b| format!(" at the cut at frame {b} (a hung frame: the clip's source runs out or repeats)")).unwrap_or_default())}));
        }
    }
    // 3 and 4. the sound: loud on both sides of a join (a clipped word), and dead air inside the talk
    let out = tokio::process::Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(video).args(["-f", "f32le", "-ac", "1", "-ar", "16000", "-"]).output().await?;
    let pcm: Vec<f32> = out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    if !pcm.is_empty() {
        let rms = |a: usize, b: usize| -> f32 {
            let (a, b) = (a.min(pcm.len()), b.min(pcm.len()));
            if b <= a { return 0.0 }
            (pcm[a..b].iter().map(|x| x * x).sum::<f32>() / (b - a) as f32).sqrt()
        };
        let win = 320; // 20 ms at 16 kHz
        let levels: Vec<f32> = (0..pcm.len() / win).map(|i| rms(i * win, (i + 1) * win)).collect();
        let mut sorted = levels.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let speech = sorted[(sorted.len() * 3 / 4).min(sorted.len().saturating_sub(1))].max(1e-4);
        // a word clipped is a word of the take running across the clip's in or out point, read from the take's
        // transcript: the mix's loudness can't tell a voice from a bed running through a beat cut (a music-only promo
        // had every join flagged). A take with no transcript, or no words, has nothing to clip
        for (word, at, edge) in clipped_words(spoken) {
            issues.push(json!({"frame": at, "kind": "clipped", "says": format!("{} the clip's {edge} cuts \"{word}\" mid-word: move the join into the pause before or after it", t(at))}));
        }
        // the ending: still loud in the last 0.1 s is a word cut off by the edit's end
        let n = levels.len();
        if n > 10 && levels[n - 5..].iter().all(|&l| l > speech * 0.5) {
            issues.push(json!({"frame": frames, "kind": "cut off", "says": format!("{} the piece ends while the speaker is still talking: a word is cut off; end on a finished sentence (or let the last clip run to the pause)", t(frames))}));
        }
        if let Some((word, at, end, more)) = unfinished_end(spoken) {
            issues.push(json!({"frame": at, "kind": "unfinished", "says": format!("{} the last line stops mid-sentence after \"{word}\" while the speaker goes on: run the clip {more:.1} s longer to \"{end}\", or end on an earlier finished sentence", t(at))}));
        }
        // dead air: 0.6 s or more far below speech, after the first word and before the last
        let quiet = speech * 0.08;
        let first = levels.iter().position(|&l| l > speech * 0.5).unwrap_or(0);
        let last = levels.iter().rposition(|&l| l > speech * 0.5).unwrap_or(0);
        let mut run = 0usize;
        for (i, &l) in levels.iter().enumerate().take(last).skip(first) {
            if l < quiet { run += 1 } else {
                if run * 20 >= 600 {
                    let f = (((i - run) * win) as f64 / 16000.0 * fps) as i64;
                    issues.push(json!({"frame": f, "kind": "dead air", "says": format!("{} {:.1} s of near-silence inside the talk: cut the pause", t(f), run as f64 * 0.02)}));
                }
                run = 0;
            }
        }
    }
    issues.sort_by_key(|i| i["frame"].as_i64().unwrap_or(0));
    Ok(json!({"issues": issues, "cuts": bounds.len()}))
}

fn summarize(results: &[Value], last: Option<&Vec<Value>>) -> String {
    use std::collections::BTreeMap;
    let mut by: BTreeMap<String, (u32, u32, f64, f64, u64, u64)> = BTreeMap::new();
    let infra = results.iter().filter(|r| r["infra"] == true).count();
    for r in results.iter().filter(|r| r["infra"] != true) {
        let e = by.entry(r["medium"].as_str().unwrap_or("?").to_string()).or_default();
        e.0 += 1;
        e.1 += u32::from(r["pass"].as_bool().unwrap_or(false));
        e.2 += r["cost_usd"].as_f64().unwrap_or(0.0);
        e.3 += r["seconds"].as_f64().unwrap_or(0.0);
        e.4 += r["tokens"]["cache_read"].as_u64().unwrap_or(0);
        e.5 += r["tokens"]["input"].as_u64().unwrap_or(0) + r["tokens"]["cache_read"].as_u64().unwrap_or(0);
    }
    let mut out = String::from("medium        pass        cost      time    cache\n");
    let (mut n, mut p, mut c) = (0, 0, 0.0);
    for (m, (total, passed, cost, secs, cr, inp)) in &by {
        out += &format!("{m:<13} {passed:>3}/{total:<3} {:>4.0}%  ${cost:>7.3}  {:>6.0}s  {:>5.1}%\n",
            100.0 * *passed as f64 / (*total).max(1) as f64, secs, 100.0 * *cr as f64 / (*inp).max(1) as f64);
        n += total;
        p += passed;
        c += cost;
    }
    out += &format!("all           {p:>3}/{n:<3} {:>4.0}%  ${c:>7.3}\n", 100.0 * p as f64 / n.max(1) as f64);
    if infra > 0 {
        out += &format!("INFRA         {infra} scenario(s) not counted: a paid provider was out of credit (provider_paused)\n");
    }
    if let Some(last) = last {
        // per scenario, the share of its runs that passed (repeats make it a rate); a move of half or more is reported
        let rate = |rs: &[Value]| {
            let mut m: BTreeMap<String, (u32, u32)> = BTreeMap::new();
            for r in rs {
                let e = m.entry(r["scenario"].as_str().unwrap_or("").to_string()).or_default();
                e.0 += u32::from(r["pass"].as_bool().unwrap_or(false));
                e.1 += 1;
            }
            m
        };
        let (was, now) = (rate(last), rate(results));
        let mut changes = Vec::new();
        for (name, (p, n)) in &now {
            let Some((wp, wn)) = was.get(name) else { continue };
            let (a, b) = (*wp as f64 / *wn as f64, *p as f64 / *n as f64);
            let counts = if *n > 1 || *wn > 1 { format!("  ({wp}/{wn} -> {p}/{n})") } else { String::new() };
            if b <= a - 0.5 {
                changes.push(format!("REGRESSED  {name}{counts}"));
            } else if b >= a + 0.5 {
                changes.push(format!("FIXED      {name}{counts}"));
            }
        }
        if !changes.is_empty() {
            out += &format!("\nsince the last run:\n  {}\n", changes.join("\n  "));
        }
    }
    out
}

/// The person's spaces (~/.raea-void/spaces) and when each changed: the same before and after a run, or an eval leaked.
fn persons_spaces() -> std::collections::BTreeMap<String, Option<std::time::SystemTime>> {
    std::fs::read_dir(crate::spaces::dir()).into_iter().flatten().flatten()
        .map(|e| {
            let changed = ["edit.tsx", "cuts.json", "edit.rrl"].iter().filter_map(|f| e.path().join(f).metadata().ok()?.modified().ok()).max();
            (e.file_name().to_string_lossy().into_owned(), changed)
        })
        .collect()
}

/// How many pages a deck exported: stills are named <edit>-f<frame>-<format>.png, so distinct frames.
fn exported_pages(dir: &Path) -> usize {
    let frames: std::collections::BTreeSet<String> = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .filter_map(|e| e.file_name().to_str()?.split('-').find(|p| p.starts_with('f') && p[1..].chars().all(|c| c.is_ascii_digit()) && p.len() > 1).map(String::from))
        .collect();
    frames.len()
}

/// The agent's temperature in evals, pinned and recorded: 1.0, MiniMax's recommended setting and GLM 5.3 Flash's (Z.ai). At 0.2 it looped on refusals and
/// wrote tool results without calling tools (full51-c: motion 0/5, games 0/4), so variance is met with repeats instead.
const EVAL_TEMPERATURE: f64 = 1.0;

/// `void eval …`
pub async fn main(args: &[String]) -> i32 {
    // `void eval --gallery RUN`: the videos for a run that already finished
    if let [flag, run] = args {
        if flag == "--gallery" {
            return backfill(run).await;
        }
    }
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let jobs: usize = flag("--jobs").and_then(|v| v.parse().ok()).unwrap_or(3).max(1);
    let only = flag("--only");
    let skip: Vec<usize> = ["--jobs", "--only", "--run", "--repeat", "--sop"].iter().filter_map(|n| args.iter().position(|a| a == n)).flat_map(|i| [i, i + 1]).collect();
    // any other flag (--help included) prints usage: a run costs money, so it never starts by accident
    if let Some(bad) = args.iter().enumerate().find(|(i, a)| !skip.contains(i) && a.starts_with('-') && *a != "--major").map(|(_, a)| a) {
        eprintln!("void eval [scenario files or folders] [--jobs N] [--only medium] [--run name] [--repeat N] [--sop ts] [--major]");
        return if bad == "--help" || bad == "-h" { 0 } else { 2 };
    }
    let paths: Vec<PathBuf> = args.iter().enumerate().filter(|(i, a)| !skip.contains(i) && !a.starts_with("--")).map(|(_, a)| PathBuf::from(a)).collect();
    let paths = if paths.is_empty() { vec![evals_dir().join("scenarios")] } else { paths };
    let mut scenarios = match collect(&paths) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("void eval: {e}");
            return 2;
        }
    };
    if let Some(m) = &only {
        scenarios.retain(|s| &s.medium == m);
    }
    // what raea can do so far (evals/capabilities.txt, one per line, the name first): a scenario needing more waits
    let have: Vec<String> = std::fs::read_to_string(evals_dir().join("capabilities.txt")).unwrap_or_default()
        .lines().filter_map(|l| l.split('#').next().unwrap_or("").split_whitespace().next().map(str::to_string)).collect();
    let waiting: Vec<String> = scenarios.iter().filter(|s| s.needs.iter().any(|n| !have.contains(n))).map(|s| format!("{} (needs {})", s.name, s.needs.join(", "))).collect();
    scenarios.retain(|s| s.needs.iter().all(|n| have.contains(n)));
    if !waiting.is_empty() {
        println!("void eval: waiting on capabilities: {}", waiting.join("; "));
    }
    if scenarios.is_empty() {
        println!("void eval: no scenarios to run");
        return 2;
    }
    let stamp = std::process::Command::new("date").arg("+%Y%m%d-%H%M%S").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let run = flag("--run").unwrap_or(stamp);
    let ver = version(&run, args.iter().any(|a| a == "--major"));
    let run_dir = crate::spaces::home().join("evals").join("runs").join(&run);
    let _ = std::fs::create_dir_all(&run_dir);
    // the run's paid generations (lyria, sao) counted in one ledger, so the cap is per run; raea render passes it on
    if std::env::var_os("RAEA_GEN_LEDGER").is_none() {
        unsafe { std::env::set_var("RAEA_GEN_LEDGER", run_dir.join("paid.jsonl")) };
    }
    let provider = match Provider::resolve().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("void eval: no model: {e}");
            return 2;
        }
    };
    println!("void eval: {} scenarios, {jobs} at a time, run v{ver} {run} (traces: Phoenix project void-evals)", scenarios.len());
    let run_started = std::process::Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let before = persons_spaces();
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(jobs));
    let mut tasks = Vec::new();
    // --repeat N: each scenario N times, so a pass rate per scenario is measured, not one sample of it
    // --sop NAME: the procedure the agent works by (prompts/sops/NAME.md), so procedures compare on the same scenarios
    // the TypeScript procedure unless another is asked for (the founder, 2026-10-17: fully on TypeScript)
    let sop = flag("--sop").or_else(|| Some("ts".into()));
    let repeat: usize = flag("--repeat").and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
    for sc in scenarios.iter().flat_map(|sc| (0..repeat).map(move |rep| (sc.clone(), rep))) {
        let (sc, rep) = sc;
        let (sem, run_dir, provider, sop) = (sem.clone(), run_dir.clone(), provider.clone(), sop.clone());
        let meta = (sc.name.clone(), sc.medium.clone(), sc.level.clone());
        tasks.push((meta, tokio::task::spawn_local(async move {
            let _permit = sem.acquire().await;
            let name = sc.name.clone();
            let r = run_one(sc, run_dir, provider, rep, sop).await;
            println!("{} {name}  (${:.4}, {:.0}s){}", if r["infra"] == true { "INFRA" } else if r["pass"].as_bool().unwrap_or(false) { "PASS" } else { "FAIL" }, r["cost_usd"].as_f64().unwrap_or(0.0), r["seconds"].as_f64().unwrap_or(0.0),
                r["not_built"].as_str().map(|w| format!("  {w}")).unwrap_or_default());
            r
        })));
    }
    let mut results = Vec::new();
    for ((name, medium, level), t) in tasks {
        // a runner that panicked is a failure on the ledger, not a scenario that vanished
        results.push(t.await.unwrap_or_else(|e| {
            println!("FAIL {name}  (the runner crashed: {e})");
            json!({"scenario": name, "medium": medium, "level": level, "pass": false, "error": format!("runner crashed: {e}")})
        }));
    }
    let after = persons_spaces();
    if after != before {
        let changed: Vec<&String> = after.keys().chain(before.keys()).filter(|k| after.get(*k) != before.get(*k)).collect();
        eprintln!("void eval: WARNING the person's own spaces changed during this run: {changed:?}. An eval must only touch its run folder.");
    }
    // the ledger: results/<run>.jsonl in the repo, and the summary against the last run
    let ledger = evals_dir().join("results");
    let _ = std::fs::create_dir_all(&ledger);
    let mut previous: Vec<PathBuf> = std::fs::read_dir(&ledger).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect();
    // the newest ledger by time, not by name ("first-14" sorts after every dated run)
    previous.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
    let last: Option<Vec<Value>> = previous.last().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| t.lines().filter_map(|l| serde_json::from_str(l).ok()).collect());
    let mut lines = String::new();
    for r in &results {
        let mut r = r.clone();
        if let Some(o) = r.as_object_mut() {
            o.remove("log");
        }
        lines += &format!("{r}\n");
    }
    let _ = std::fs::write(ledger.join(format!("{run}.jsonl")), lines);
    let mut summary = summarize(&results, last.as_ref());
    // no result without its trace, and every trace visible in Phoenix: checked, not assumed
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let wanted: std::collections::BTreeSet<String> = results.iter().flat_map(|r| r["traces"].as_array().cloned().unwrap_or_default())
        .filter_map(|t| t.as_str().filter(|s| !s.is_empty()).map(String::from)).collect();
    let untraced = results.iter().filter(|r| r["traces"].as_array().is_none_or(|t| t.iter().all(|x| x.as_str().unwrap_or("").is_empty()))).count();
    summary += &match phoenix_traces(&run_started).await {
        Ok(seen) => {
            let missing = wanted.iter().filter(|t| !seen.contains(*t)).count();
            format!("traced in Phoenix: {}/{}{}{}\n", wanted.len() - missing, wanted.len(),
                if missing > 0 { format!(" ({missing} MISSING)") } else { String::new() },
                if untraced > 0 { format!("; {untraced} result(s) without a trace") } else { String::new() })
        }
        Err(e) => format!("traced in Phoenix: couldn't check ({e})\n"),
    };
    let paid = std::fs::read_to_string(run_dir.join("paid.jsonl")).map(|s| s.lines().count()).unwrap_or(0);
    summary += &format!("paid generations (fish, lyria): {paid}\n");
    println!("\n{summary}");
    let _ = std::fs::write(run_dir.join("summary.txt"), &summary);
    // give the trace exporter its last second
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    if results.iter().all(|r| r["pass"].as_bool().unwrap_or(false)) { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_a_scenario() {
        let dir = std::env::temp_dir().join(format!("void-eval-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("t.feature");
        std::fs::write(&f, "@medium:ugc\nFeature: captions\n  Background:\n    Given the space \"ugc\"\n\n  @level:easy\n  Scenario: Three-word captions\n    When the person says \"add captions, three words at a time\"\n    Then expect C.size >= 60 if caption C\n    And the judge sees \"captions readable\" at frames 10, 200\n    And the reply says \"captions\"\n    And it costs under $0.05\n").unwrap();
        let s = super::parse(&f).unwrap();
        assert_eq!(s.len(), 1);
        let s = &s[0];
        assert_eq!((s.medium.as_str(), s.level.as_str(), s.fixture.as_deref()), ("ugc", "easy", Some("ugc")));
        assert_eq!(s.expects, vec!["expect C.size >= 60 if caption C"]);
        assert_eq!(s.judge.as_ref().unwrap().1, vec![10, 200]);
        assert_eq!(s.max_cost, Some(0.05));
        let d = std::env::temp_dir().join(format!("void-pages-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for n in ["edit-f30-native.png", "edit-f30-1x1.png", "edit-f90-native.png"] {
            std::fs::write(d.join(n), b"").unwrap();
        }
        assert_eq!(super::exported_pages(&d), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn reads_the_project_scenarios() {
        let all = super::parse(&super::evals_dir().join("scenarios/project/project.feature")).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].says.len(), 3);
        assert_eq!(all[0].record, vec!["shows\t(game", "versions\tpong\t2", "changed\tpong\t2", "back\tpong\t1"]);
        assert!(all[1].says[1].starts_with(super::ANSWER) && all[1].record.contains(&"asked".to_string()));
    }

    #[test]
    fn grades_expects_on_the_render() {
        let f: std::collections::BTreeMap<&'static str, f64> = [("width", 1080.0), ("height", 1920.0), ("fps", 30.0), ("frames", 900.0)].into_iter().collect();
        assert!(super::grade_expect("expect edit.width = 1080 and edit.height = 1920", Some(&f)).unwrap().0);
        let (pass, why) = super::grade_expect("expect edit.frames >= 562 and edit.frames <= 800", Some(&f)).unwrap();
        assert!(!pass && why.contains("edit.frames is 900 (FAIL)"), "{why}");
        assert!(super::grade_expect("expect C.size >= 60 if caption C", Some(&f)).is_none());
        assert!(!super::grade_expect("expect edit.width = 1080", None).unwrap().0);
    }

    #[test]
    fn rubrics_parse() {
        let design = std::fs::read_to_string(super::evals_dir().join("design/rubric.md")).unwrap();
        let criteria = super::bullets(&design, "Each criterion is anchored");
        assert_eq!(criteria.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["hook", "message", "pacing", "cuts", "captions", "visuals", "structure", "cta", "authenticity"]);
        assert!(criteria[4].1.contains("score 3 and don't count it"), "{}", criteria[4].1);
        let bands = super::bullets(&design, "The overall scale is anchored");
        assert_eq!(bands.iter().map(|b| b.0.as_str()).collect::<Vec<_>>(), ["9-10", "7-8", "5-6", "3-4", "1-2"]);
        assert_eq!(super::stretches(10.0), ["0:00-0:02", "0:02-0:05", "0:05-0:07", "0:07-0:10"]);
        assert_eq!(super::stretches(200.0).len(), 10);
        let v = serde_json::json!({"choice": "b", "probabilities": {"a": 0.25, "b": 0.75}});
        assert_eq!(crate::checks::weighted(&v, &[("a", 1.0), ("b", 3.0)]), Some(2.5));
        assert_eq!(crate::checks::expected(&serde_json::json!({"choice": "weak"}), &crate::checks::LEVELS), Some(1.0));
    }

    /// The clef judges on judged renders that hold no creator's footage, for calibration and to check the port:
    /// RAEA_JUDGE_SET names a JSON list of {video, says, focus?, gemini?, brand?, direction?}. Paid (about a cent a
    /// render); run with --ignored.
    #[tokio::test]
    #[ignore]
    async fn clef_judges_on_a_set() {
        let Ok(set) = std::env::var("RAEA_JUDGE_SET") else { return };
        let items: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(set).unwrap()).unwrap();
        let mut cost = 0.0;
        for it in &items {
            let video = std::path::PathBuf::from(it["video"].as_str().unwrap());
            let says: Vec<String> = it["says"].as_array().unwrap().iter().filter_map(|s| s.as_str().map(String::from)).collect();
            let (v, c) = super::watch(&video, &says, it["focus"].as_str()).await.unwrap();
            cost += c;
            println!("JUDGE {}", serde_json::json!({"video": video, "gemini": it["gemini"], "clef": v}));
            if let Some(brand) = it["brand"].as_str() {
                let rubric = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/craft/rubric.md")).unwrap();
                let (r, c) = super::review(&video, &rubric, brand, it["direction"].as_str().unwrap_or(""), &says.join(" / ")).await.unwrap();
                cost += c;
                println!("REVIEW {}", serde_json::json!({"video": video, "review": r}));
            }
        }
        if let [a, b, ..] = items.as_slice() {
            let says: Vec<String> = a["says"].as_array().unwrap().iter().filter_map(|s| s.as_str().map(String::from)).collect();
            let p = super::pair_judge(std::path::Path::new(a["video"].as_str().unwrap()), std::path::Path::new(b["video"].as_str().unwrap()), &says).await.unwrap();
            cost += p["cost"].as_f64().unwrap_or(0.0);
            println!("PAIR {p}");
        }
        println!("COST {cost:.4}");
    }

    /// The pair judge on pairs whose order is known, to check its pass mark (>= -0.5): RAEA_PAIR_SET names a JSON list
    /// of {ours, reference, says, label}: a piece against itself (the score should sit near 0), against a copy made
    /// worse (ours should win: score > 0), or the worse copy as ours (score < 0). Paid (about a tenth of a cent a pair);
    /// run with --ignored.
    #[tokio::test]
    #[ignore]
    async fn pair_judge_on_a_set() {
        let Ok(set) = std::env::var("RAEA_PAIR_SET") else { return };
        let items: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(set).unwrap()).unwrap();
        let mut cost = 0.0;
        for it in &items {
            let says: Vec<String> = it["says"].as_array().unwrap().iter().filter_map(|s| s.as_str().map(String::from)).collect();
            match super::pair_judge(std::path::Path::new(it["ours"].as_str().unwrap()), std::path::Path::new(it["reference"].as_str().unwrap()), &says).await {
                Ok(p) => {
                    cost += p["cost"].as_f64().unwrap_or(0.0);
                    println!("PAIR {}", serde_json::json!({"label": it["label"], "score": p["score"], "whole": p["whole"], "sound": p["sound"], "silent": p["silent"]}));
                }
                Err(e) => println!("PAIR {}", serde_json::json!({"label": it["label"], "error": e.to_string()})),
            }
        }
        println!("COST {cost:.4}");
    }
}
