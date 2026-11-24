use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use raea_core_brief::{make::Make, read};
use raea_core_facts::record::Entry;
use serde_json::{Value, json};

use crate::asked::Ledger;
use crate::status::{Job, status, stalled};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("raea-make-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put(d: &PathBuf, rel: &str, text: &str) {
    let p = d.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn kit() -> Make {
    read("make", r#"{"kind": "kit", "style": "1970s desk, maker Aurora", "objects": [{"id": "phone", "label": "Phone", "h": 0.12, "prompt": "a rotary phone"}]}"#).unwrap()
}

fn delivered(d: &PathBuf) {
    for f in ["out/kit/phone/phone.glb", "out/kit/phone/phone_lod1.glb", "out/kit/thumbs/phone.png", "out/kit/lineup.png", "out/kit/kit.json", "review/refs.png"] {
        put(d, f, "x");
    }
    put(d, "NOTES.md", "# Notes\n\n## What the skill got wrong\n- nothing yet\n");
}

fn round(pass: bool, mean: f64, defects: Value) -> Value {
    json!({"dir": "review/r1", "kind": "kit", "pass": pass, "mean": mean, "defects": defects})
}

fn run(make: &Make, d: &PathBuf, record: &[Entry], rounds: &[Value], judged_at: Option<SystemTime>) -> Value {
    run_with(make, d, record, rounds, judged_at, &Ledger::default())
}

fn run_with(make: &Make, d: &PathBuf, record: &[Entry], rounds: &[Value], judged_at: Option<SystemTime>, ledger: &Ledger) -> Value {
    status(&Job { make, dir: d, project: None, record, rounds, verdict: None, judged_at, ledger, score_gates: false })
}

fn later() -> Option<SystemTime> {
    Some(SystemTime::now() + Duration::from_secs(5))
}

#[test]
fn nothing_made_is_not_done_and_says_what_is_missing() {
    let d = dir("empty");
    let s = run(&kit(), &d, &[], &[], None);
    assert_eq!(s["done"], false);
    let missing = s["missing"].to_string();
    assert!(missing.contains("out/kit/phone/phone_lod1.glb") && missing.contains("NOTES.md"), "{missing}");
    assert!(s["gates"].as_array().unwrap().iter().any(|g| g["gate"] == "critic" && g["clear"] == false));
}

#[test]
fn delivered_and_judged_after_is_done() {
    let d = dir("done");
    delivered(&d);
    let s = run(&kit(), &d, &[], &[round(true, 7.5, json!([]))], later());
    assert_eq!(s["done"], true, "{s:#}");
    assert_eq!(s["short"], false);
}

#[test]
fn a_round_older_than_the_newest_deliverable_is_stale() {
    let d = dir("stale");
    delivered(&d);
    let s = run(&kit(), &d, &[], &[round(true, 7.5, json!([]))], Some(SystemTime::UNIX_EPOCH));
    assert_eq!(s["done"], false);
    assert!(s["gates"].to_string().contains("came after the last round"), "{s:#}");
}

#[test]
fn a_blocking_defect_must_be_fixed_or_answered_in_the_notes() {
    let d = dir("defect");
    delivered(&d);
    let defects = json!([{"defect": "floating", "present": true, "level": "blocking"}]);
    let s = run(&kit(), &d, &[], &[round(false, 6.0, defects.clone())], later());
    assert_eq!(s["done"], false);
    assert!(s["gates"].to_string().contains("floating"));
    put(&d, "NOTES.md", "## What the skill got wrong\n- floating: the handset rests in its cradle; the judge read its shadow as a gap\n");
    let s = run(&kit(), &d, &[], &[round(false, 6.0, defects.clone())], later());
    assert_eq!(s["done"], true, "{s:#}");
    // once the judge is calibrated for kits, its score gates too: two passes in a row (MK6)
    let (rounds, ledger) = ([round(false, 6.0, defects)], Ledger::default());
    let s = status(&Job { make: &kit(), dir: &d, project: None, record: &[], rounds: &rounds, verdict: None, judged_at: later(), ledger: &ledger, score_gates: true });
    assert_eq!(s["done"], false, "{s:#}");
    assert!(s["gates"].to_string().contains("two needed"), "{s:#}");
}

#[test]
fn at_the_cap_it_is_delivered_short() {
    let d = dir("cap");
    let rounds: Vec<Value> = (0..8).map(|i| round(false, 5.0 + i as f64 * 0.5, json!([]))).collect();
    let s = run(&kit(), &d, &[], &rounds, later());
    assert_eq!((s["done"].clone(), s["short"].clone()), (json!(true), json!(true)), "{s:#}");
    assert!(s["why"].to_string().contains("capped"));
}

#[test]
fn three_rounds_without_beating_the_best_is_a_stall() {
    let r = |m: f64| json!({"mean": m});
    assert!(stalled(&[r(5.0), r(6.5), r(6.0), r(6.4), r(6.5)]));
    assert!(!stalled(&[r(5.0), r(6.0), r(6.2), r(6.4), r(6.6)]));
    assert!(!stalled(&[r(5.0), r(5.0), r(5.0)]), "too few rounds to call it");
}

#[test]
fn a_composition_needs_a_clean_check_newer_than_it() {
    let d = dir("checks");
    delivered(&d);
    put(&d, "scene.tsx", "x");
    let mut make = kit();
    make.compositions = vec!["scene.tsx".into()];
    let rounds = [round(true, 7.5, json!([]))];
    let s = run(&make, &d, &[], &rounds, later());
    assert!(s["gates"].to_string().contains("never rendered"), "{s:#}");
    let entry = |at: u64, level: &str| -> Entry {
        serde_json::from_value(json!({"id": "20261123-x", "at": at, "by": "raea render", "of": "scene.tsx",
            "findings": [{"check": "float", "level": level, "says": "the mug floats 2 cm", "by": "raea scene lint"}]})).unwrap()
    };
    let future = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() + 60;
    let s = run(&make, &d, &[entry(future, "error")], &rounds, later());
    assert!(s["gates"].to_string().contains("the mug floats"), "{s:#}");
    let s = run(&make, &d, &[entry(future, "warn")], &rounds, later());
    assert_eq!(s["done"], true, "a warn doesn't block: {s:#}");
    let s = run(&make, &d, &[entry(1, "warn")], &rounds, later());
    assert!(s["gates"].to_string().contains("changed since"), "{s:#}");
}

#[test]
fn a_scene_wants_three_stills() {
    let d = dir("scene");
    let make: Make = read("make", r#"{"kind": "scene", "objects": [{"id": "stool", "label": "Stool", "h": 0.45, "route": "gen", "prompt": "a red stool"}]}"#).unwrap();
    put(&d, "out/stills/a.png", "x");
    let s = run(&make, &d, &[], &[], None);
    assert!(s["missing"].to_string().contains("holds 1 picture"), "{s:#}");
}

#[test]
fn a_round_judged_on_another_kinds_rubric_does_not_count() {
    // docs/make.feature MK5: a kit is judged on the kit's rubric
    let d = dir("rubric");
    delivered(&d);
    let old = json!({"dir": "review/r1", "pass": true, "mean": 8.0, "defects": []});
    let s = run(&kit(), &d, &[], &[old], later());
    assert_eq!(s["done"], false);
    assert!(s["gates"].to_string().contains("--kind kit"), "{s:#}");
}

#[test]
fn a_calibrated_ask_question_gates_and_an_uncalibrated_one_advises() {
    // docs/make.feature MK4
    let d = dir("ask");
    delivered(&d);
    put(&d, "review/r1/clay.png", "clay sheet bytes");
    put(&d, "review/r1/colour.png", "colour sheet bytes");
    let rounds = [round(true, 7.5, json!([]))];
    let floating = raea_core_ask::library::find("make.clay.floating", None).unwrap();
    let mut ledger = Ledger::default();
    // nothing calibrated: every question is advice, and the job is done
    let s = run_with(&kit(), &d, &[], &rounds, later(), &ledger);
    assert_eq!(s["done"], true, "{s:#}");
    assert!(s["gates"].to_string().contains("advice only"), "{s:#}");
    // floating calibrated on the person's labels: it must be asked of this exact clay sheet
    ledger.calibrations.insert(floating.version(), serde_json::from_value(json!({"auc": 0.9, "n": 40, "positives": 16, "threshold": 0.4, "date": "2026-11-23", "by": "you"})).unwrap());
    let s = run_with(&kit(), &d, &[], &rounds, later(), &ledger);
    assert_eq!(s["done"], false);
    let gates = s["gates"].to_string();
    assert!(gates.contains("not asked of this clay sheet") && gates.contains("--std make.clay.floating"), "{gates}");
    // asked, and it flagged: open until fixed or answered by its id in NOTES.md
    let ident = raea_core_ask::ident::ident(&d.join("review/r1/clay.png")).unwrap();
    ledger.idents.insert("c-1".into(), ident);
    ledger.answers.push(json!({"id": "a-1", "at": "100", "call": "c-1", "question": "make.clay.floating", "flagged": true, "problem": 0.8}));
    let s = run_with(&kit(), &d, &[], &rounds, later(), &ledger);
    assert_eq!(s["done"], false);
    assert!(s["gates"].to_string().contains("make.clay.floating flagged (p 0.80)"), "{s:#}");
    put(&d, "NOTES.md", "## What the skill got wrong\n- make.clay.floating: the handset's shadow, not a gap; checked on the low tile\n");
    let s = run_with(&kit(), &d, &[], &rounds, later(), &ledger);
    assert_eq!(s["done"], true, "{s:#}");
    // the sheet rendered again: the old answer was about other bytes, so it's asked again
    put(&d, "review/r1/clay.png", "a new clay sheet");
    let s = run_with(&kit(), &d, &[], &rounds, later(), &ledger);
    assert!(s["gates"].to_string().contains("not asked of this clay sheet"), "{s:#}");
    // an uncalibrated question that flags is said as advice, holding nothing
    let ident = raea_core_ask::ident::ident(&d.join("review/r1/colour.png")).unwrap();
    ledger.idents.insert("c-2".into(), ident);
    ledger.answers.push(json!({"id": "a-2", "at": "101", "call": "c-2", "question": "make.colour.plastic", "flagged": true, "problem": 0.7}));
    let s = run_with(&kit(), &d, &[], &rounds, later(), &ledger);
    assert!(s["advice"].to_string().contains("make.colour.plastic says yes"), "{s:#}");
}

#[test]
fn the_gates_are_check_lines_under_the_jobs_task_in_plan_org() {
    // docs/make.feature MK3
    let not_done = json!({"done": false, "missing": ["out/kit/lineup.png"], "gates": [{"gate": "critic", "clear": false, "says": "no round yet"}, {"gate": "checks", "of": "kit.tsx", "clear": true, "says": "clean"}]});
    let t = crate::plan::update("", "desk", "brief.ts", "kit", &not_done);
    let id = crate::plan::task_id("brief.ts");
    assert!(id.len() == 10 && id.starts_with("t-") && id[2..].chars().all(|c| c.is_ascii_alphanumeric()), "{id}");
    assert!(t.starts_with("#+TITLE: desk\n#+TODO: TODO MAKING REVIEW | DONE DROPPED\n"), "{t}");
    assert!(t.contains("* MAKING make: kit (brief.ts) [1/3]") && t.contains(&format!(":ID: {id}")), "{t}");
    assert!(t.contains("- [ ] missing: out/kit/lineup.png") && t.contains("- [X] checks kit.tsx: clean"), "{t}");
    // the person's own line under it stays; the checks are brought up to date; done is REVIEW
    let mut mine = t.replace("- [X] checks kit.tsx: clean\n", "- [X] checks kit.tsx: clean\nShane: keep the phone's cord\n");
    mine = format!("{mine}* TODO another task\n");
    let done = json!({"done": true, "missing": [], "gates": [{"gate": "critic", "clear": true, "says": "round 2 judged"}]});
    let t2 = crate::plan::update(&mine, "desk", "brief.ts", "kit", &done);
    assert!(t2.contains("* REVIEW make: kit (brief.ts) [1/1]") && t2.contains("Shane: keep the phone's cord") && t2.contains("* TODO another task"), "{t2}");
    assert!(!t2.contains("missing:"), "{t2}");
    assert_eq!(t2.matches(":ID: ").count(), 1);
    // a task the person closed keeps their mark
    let closed = t2.replace("* REVIEW make", "* DONE make");
    assert!(crate::plan::update(&closed, "desk", "brief.ts", "kit", &not_done).contains("* DONE make: kit (brief.ts) [1/3]"));
    // once written, writing the same again changes nothing
    assert_eq!(crate::plan::update(&t2, "desk", "brief.ts", "kit", &done), t2);
}
