//! What a videoform's references hold, said to the person after every `add`, and their corrections (docs/videoform.feature
//! VF4, VF4b): `raea videoform report NAME` (each measure's range, how strongly it holds, the groups and the references
//! far from the rest), `set NAME MEASURE ignore|free|keep|target [LO HI] --why "…"` and `unset NAME MEASURE` (the
//! person's word over the references', kept in the section with why), and `score NAME FILE` (a cut read as a reference
//! is, against what's checked). What's found is made from the kept rows alone and written to videoform/found.json; the
//! vocabulary's part (VF5) is vocab.rs's, said after it.

use crate::{Eval, flag, plain, references};
use raea_core_brief::videoform::{Feel, SAYS};
use raea_core_brief::{Correction, Videoform};
use raea_sys_videoform::ranges::{FEW, Found, Measures, NAMES, critique, found, measures};
use raea_sys_videoform::rows::Rows;
use serde_json::Value;
use std::path::Path;

/// A reference's measures: its rows', and its loudness from its reading
pub(crate) fn measured(dir: &Path, r: &Value) -> Option<Measures> {
    Some(of(&rows(dir, r)?, r))
}

fn rows(dir: &Path, r: &Value) -> Option<Rows> {
    Rows::load(dir, r["hash"].as_str()?, raea_sys_columns::SIG, raea_sys_columns::CLAP)
}

fn of(rows: &Rows, r: &Value) -> Measures {
    let mut m = measures(rows);
    let loud = &r["reading"]["loudness"];
    for (k, at) in [("loudness", "lufs"), ("true_peak", "true_peak_db")] {
        if let Some(v) = loud[at].as_f64() {
            m.insert(k.into(), v);
        }
    }
    m
}

/// A reference's short name: its title before the site's, or its file's name
fn short(r: &Value) -> String {
    let s = &r["source"];
    let t = s["title"].as_str().and_then(|t| t.split(" | ").next()).map(str::to_string);
    t.or_else(|| s["path"].as_str().and_then(|p| Path::new(p).file_stem()).map(|f| f.to_string_lossy().into_owned())).unwrap_or_else(|| "?".into())
}

/// Each reference read at this rows version, by its short name: its rows
pub(crate) fn kept(dir: &Path) -> Vec<(String, Rows, Value)> {
    let mut refs: Vec<(String, Rows, Value)> = references(dir).into_iter().filter_map(|r| Some((short(&r), rows(dir, &r)?, r))).collect();
    // a title more than one has ("Video by brannonharbur", as Instagram calls every reel) gets its post's id
    let names: Vec<String> = refs.iter().map(|r| r.0.clone()).collect();
    for r in refs.iter_mut().filter(|r| names.iter().filter(|n| **n == r.0).count() > 1) {
        if let Some(id) = r.2["source"]["id"].as_str() {
            r.0 = format!("{} {id}", r.0);
        }
    }
    // the words every name shares ("Street Interview") say nothing about which it is
    let all: Vec<String> = refs.first().map(|r| r.0.split_whitespace().map(str::to_string).collect()).unwrap_or_default();
    let common: Vec<&String> = all.iter().filter(|w| refs.len() > 1 && refs.iter().all(|r| r.0.split_whitespace().any(|x| x == w.as_str()))).collect();
    for r in &mut refs {
        let kept: Vec<&str> = r.0.split_whitespace().filter(|w| !common.iter().any(|c| c == w)).collect();
        if !kept.is_empty() {
            r.0 = kept.join(" ");
        }
    }
    refs
}

/// What's found in a videoform's kept rows with its section's corrections and its context's feel words, written
/// beside them
pub(crate) fn make(dir: &Path, section: &Videoform, feel: &[Feel]) -> Result<Found, String> {
    let refs: Vec<(String, Measures)> = kept(dir).into_iter().map(|(n, rows, r)| (n, of(&rows, &r))).collect();
    let f = found(&refs, &section.corrections, feel);
    let path = dir.join("videoform/found.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&f).unwrap()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(f)
}

fn words(k: &str) -> &str {
    NAMES.iter().find(|n| n.0 == k).map_or("true peak (dBTP)", |n| n.1)
}

fn num(k: &str, v: f64) -> String {
    match k {
        "close_up" | "medium" | "wide" | "no_people" | "moving" | "text" | "captions" | "speech" | "music" | "on_beat" | "caption_size" | "sentence_cuts" | "music_under" | "talking" => format!("{:.0}%", v * 100.0),
        _ if v.abs() < 10.0 => format!("{v:.2}"),
        _ => format!("{v:.1}"),
    }
}

/// References by their names, as a list
fn named(f: &Found, ix: &[usize]) -> String {
    ix.iter().map(|&j| f.references[j].as_str()).collect::<Vec<_>>().join(", ")
}

/// What's found, for the person to read and correct
pub(crate) fn report(name: &str, f: &Found) -> String {
    let n = f.references.len();
    let mut out = vec![format!("What {n} references of {name} hold. Correct any of it: raea videoform set {name} MEASURE ignore|free|keep|target [LO HI] --why \"…\"")];
    if f.weak {
        out.push(format!("  (under {FEW} references: nothing here is strong enough to gate on yet)"));
    }
    let mut ms: Vec<(&String, &raea_sys_videoform::ranges::Range)> = f.measures.iter().collect();
    let rank = |s: &str| ["holds", "loose", "keep", "target", "free", "ignore", "floor"].iter().position(|x| *x == s).unwrap_or(9);
    ms.sort_by_key(|(_, r)| (r.by != "you", rank(&r.state)));
    for (k, r) in ms {
        let span = if r.lo == r.hi { num(k, r.lo) } else { format!("{} to {} (median {})", num(k, r.lo), num(k, r.hi), num(k, r.median)) };
        let state = if r.by == "you" { format!("you: {}", r.state) } else { r.state.clone() };
        let mut line = format!("  {state:<12} {k:<14} {}: {span}", words(k));
        if r.state == "target" {
            line += &r.checked.map(|c| format!(", checked {} to {}", num(k, c[0]), num(k, c[1]))).unwrap_or_default();
        }
        if r.groups.len() == 2 {
            line += &format!("; two groups: {} | {}", named(f, &r.groups[0]), named(f, &r.groups[1]));
        }
        if let Some(fl) = raea_sys_videoform::ranges::FLOOR.iter().find(|fl| fl.0 == k.as_str() && r.by != "you") {
            let off: Vec<&str> = r.values.iter().enumerate().filter(|(_, v)| v.is_some_and(|v| (v - fl.1).abs() > 5.0)).map(|(i, _)| f.references[i].as_str()).collect();
            if !off.is_empty() && k == "loudness" {
                line += &format!("; {} far from it, likely not mastered", off.join(", "));
            }
        }
        if let Some(w) = &r.why {
            line += &format!(" ({w})");
        }
        out.push(line);
    }
    for (i, ks) in &f.outliers {
        out.push(format!("  {} is far from the others on {}: keep it, or take it out of the videoform", f.references[*i], ks.join(", ")));
    }
    // two kinds of video read as one (VF8b): a videoform offered for each, never averaged
    if let Some(s) = &f.sets {
        let g = |i: usize| named(f, &s.groups[i]);
        let on: Vec<String> = s.on.iter().take(4).map(|(k, a, b)| format!("{} {} against {}", words(k), num(k, *a), num(k, *b))).collect();
        out.push(format!("  The references fall in two groups across the measures ({:.0}% apart): {} | {}; they differ on {}. raea videoform split {name} makes a videoform of each, or keep them as one", s.clear * 100.0, g(0), g(1), if on.is_empty() { "many measures a little".into() } else { on.join("; ") }));
    }
    out.join("\n")
}

/// What the references hold and what the vocabulary did on them, as the person reads it after `add` and on
/// `report`; JSON with --json
pub(crate) fn whole(name: &str, dir: &Path, section: &Videoform, eval: Eval, json: bool) -> Result<String, String> {
    let vocab = crate::vocab::read(dir, eval)?;
    let f = make(dir, section, vocab.as_ref().map(|v| v.feel.as_slice()).unwrap_or_default())?;
    let words = match &vocab {
        Some(v) => Some(crate::vocab::found(dir, v, &section.corrections)?),
        None => None,
    };
    if json {
        return Ok(serde_json::to_string_pretty(&serde_json::json!({"measures": f, "vocabulary": words.as_ref().map(|w| &w.1), "rules": words.as_ref().map(|w| &w.2)})).unwrap());
    }
    let tail = match words {
        Some((text, ..)) => text,
        None => format!("No vocabulary yet: your AI writes {} from the context, the things, sections and feel it names (raea videoform vocab {name} starts one)", dir.join(crate::vocab::FILE).display()),
    };
    // what was asked, and how it was made (VF8f)
    let asked = raea_app_videoform_docs::learn(dir, name, &section.ranked(), &kept(dir), eval)?;
    // references read with other encoders than this raea's (VF8c)
    let pins = raea_app_videoform_refs::behind(dir, name).map(|l| format!("\n  {l}")).unwrap_or_default();
    Ok(format!("{}{pins}\n\n{tail}{}", report(name, &f), if asked.is_empty() { String::new() } else { format!("\n\n{asked}") }))
}

/// `raea videoform report NAME [--json]`
pub(crate) fn verb_report(args: &[String], eval: Eval) -> (String, i32) {
    let Some(name) = plain(args).into_iter().next() else { return (format!("usage: {}", crate::USAGE), 2) };
    let (dir, fp) = match crate::verbs::open(&name, None) {
        Ok(x) => x,
        Err(e) => return (e, 1),
    };
    match whole(&name, &dir.dir, fp.videoform.as_ref().unwrap(), eval, args.iter().any(|a| a == "--json")) {
        Ok(t) => (t, 0),
        Err(e) => (e, 1),
    }
}

/// `raea videoform set NAME MEASURE SAY [LO HI] [--why "…"]` and `unset NAME MEASURE`: the correction kept in the
/// section, and what's found made again
pub(crate) fn verb_set(args: &[String], eval: Eval, unset: bool) -> (String, i32) {
    let w = plain(args);
    let (Some(name), Some(measure)) = (w.first(), w.get(1)) else { return (format!("usage: {}", crate::USAGE), 2) };
    let (dir, mut fp) = match crate::verbs::open(name, Some(eval)) {
        Ok(x) => x,
        Err(e) => return (e, 1),
    };
    // a rule's id, as the last report found it (VF8), or a measure; a rule is kept, free or ignored
    let rules: Vec<serde_json::Value> = std::fs::read(dir.dir.join("videoform/rules.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let rule = rules.iter().filter_map(|r| r["id"].as_str()).any(|id| id == measure) || fp.videoform.as_ref().is_some_and(|v| v.corrections.iter().any(|c| &c.measure == measure && measure.contains(':')));
    if !rule && !NAMES.iter().any(|n| n.0 == measure) && measure != "true_peak" {
        return (format!("raea videoform: no measure or rule {measure}; the measures are {}, and the rules' ids are in raea videoform report {name}", NAMES.iter().map(|n| n.0).collect::<Vec<_>>().join(", ")), 2);
    }
    let c = if unset {
        None
    } else {
        let say = w.get(2).map(String::as_str).unwrap_or("");
        let says: &[&str] = if rule { &["keep", "free", "ignore"] } else { &SAYS };
        if !says.contains(&say) {
            return (format!("raea videoform: say {} for {measure}", says.join(", ")), 2);
        }
        let num = |i: usize| w.get(i).and_then(|x| x.parse::<f64>().ok());
        let range = num(3).zip(num(4)).map(|(a, b)| [a.min(b), a.max(b)]);
        if say == "target" && range.is_none() {
            return (format!("raea videoform: a target is a range, LO HI (set {name} {measure} target 30 50)"), 2);
        }
        Some(Correction { measure: measure.clone(), say: say.into(), range, why: flag(args, "--why").map(str::to_string) })
    };
    fp.videoform.as_mut().unwrap().correct(measure, c);
    if let Err(e) = dir.rewrite(&fp) {
        return (format!("{e:#}"), 1);
    }
    match whole(name, &dir.dir, fp.videoform.as_ref().unwrap(), eval, false) {
        Ok(t) => (t, 0),
        Err(e) => (e, 1),
    }
}

/// `raea videoform score NAME FILE [--json]`: a cut read as a reference is, its measures against what's checked
pub(crate) fn verb_score(args: &[String], eval: Eval) -> (String, i32) {
    let w = plain(args);
    let (Some(name), Some(file)) = (w.first(), w.get(1)) else { return (format!("usage: {}", crate::USAGE), 2) };
    let (dir, fp) = match crate::verbs::open(name, None) {
        Ok(x) => x,
        Err(e) => return (e, 1),
    };
    let feel = match crate::vocab::read(&dir.dir, eval) {
        Ok(v) => v.map(|v| v.feel).unwrap_or_default(),
        Err(e) => return (e, 1),
    };
    let f = match make(&dir.dir, fp.videoform.as_ref().unwrap(), &feel) {
        Ok(f) => f,
        Err(e) => return (e, 1),
    };
    let (m, read) = match crate::ingest::measure(Path::new(file)) {
        Ok(x) => x,
        Err(e) => return (format!("raea videoform: {file}: {e}"), 1),
    };
    let out = critique(&f, &m);
    if args.iter().any(|a| a == "--json") {
        return (serde_json::to_string_pretty(&serde_json::json!({"measures": m, "out": out, "read": read})).unwrap(), 0);
    }
    let checked = f.measures.values().filter(|r| r.checked.is_some()).count();
    let mut lines = vec![format!("{file} against {name}: {} of {checked} checked measures out{}", out.len(), if f.weak { " (few references: nothing gates)" } else { "" })];
    lines.extend(out.iter().map(|o| format!("  {}: {} is {}, {} it into {} to {}", o.measure, words(&o.measure), num(&o.measure, o.value), o.move_, num(&o.measure, o.range[0]), num(&o.measure, o.range[1]))));
    (lines.join("\n"), if out.is_empty() { 0 } else { 1 })
}
