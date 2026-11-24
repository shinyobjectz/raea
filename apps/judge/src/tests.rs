use super::*;
use serde_json::json;

fn scores(v: f64) -> serde_json::Map<String, Value> {
    rubric::SCORES.iter().map(|s| (s.0.to_string(), json!(v))).collect()
}

#[test]
fn the_pass_is_code_and_a_blocking_defect_holds_it() {
    assert!(rubric::pass(&scores(7.5), &[]).0);
    assert!(!rubric::pass(&scores(6.5), &[]).0, "mean under 7");
    let mut s = scores(8.0);
    s.insert("form".into(), json!(5.0));
    let (ok, why) = rubric::pass(&s, &[]);
    assert!(!ok && why.iter().any(|w| w.contains("form")));
    let d = vec![json!({"defect": "floating", "present": true, "level": "blocking"})];
    assert!(!rubric::pass(&scores(9.0), &d).0);
    // the questions hold no pass line
    let q = format!("{}{}", rubric::questions("colour"), rubric::questions("clay"));
    assert!(!q.contains("mean") && !q.contains(">= 6"));
}

#[test]
fn answers_are_read_and_rounds_say_done_and_stall() {
    let a = json!({"score_form": {"score": 3.5}, "defect_floating": {"noul": 0.8}});
    let (s, d) = rubric::read(&a);
    assert_eq!(s["form"], 7.0);
    assert_eq!(d[0]["present"], true);
    let r = |p: bool, m: f64| json!({"pass": p, "mean": m, "defects": []});
    assert_eq!(progress(&[r(false, 5.0), r(true, 7.2), r(true, 7.4)])["done"], true);
    assert_eq!(progress(&[r(false, 5.0), r(false, 5.3), r(false, 5.6)])["stall"], true);
    assert_eq!(progress(&[r(false, 4.0), r(false, 5.3), r(false, 6.1)])["stall"], false);
}

#[test]
fn calibration_maps_a_lenient_judge_onto_the_eye() {
    // the judge says 8 to everything the eye gives 5, and 9 to what the eye gives 8
    let row = |j: f64, e: f64, p: bool| json!({"judge": scores(j), "eye": scores(e), "eye_pass": p, "defects": []});
    let rows: Vec<Value> = (0..12).map(|i| if i % 2 == 0 { row(8.0, 5.0, false) } else { row(9.0, 8.0, true) }).collect();
    let cal = eye::fit_rows_for(None, &rows);
    assert_eq!(cal["agreement"], 1.0);
    assert_eq!(cal["gate"], true);
    assert!((cal["maps"]["form"]["a"].as_f64().unwrap() - 3.0).abs() < 1e-6);
    let few = eye::fit_rows_for(None, &rows[..4]);
    assert_eq!(few["gate"], false, "too few rounds to gate");
    // an eye that failed everything agrees with a judge that fails everything, which proves nothing
    let fails: Vec<Value> = (0..12).map(|_| row(3.0, 2.0, false)).collect();
    let all_fail = eye::fit_rows_for(None, &fails);
    assert_eq!(all_fail["agreement"], 1.0);
    assert_eq!(all_fail["gate"], false, "no pass among the eye's rounds");
}

#[test]
fn an_agent_verdict_is_read_from_the_end_of_its_thinking() {
    // an agent thinks aloud, may draft the JSON twice, and ends with the one it means
    let text = r#"• Looking at the sheets... draft: {"scores": {"form": 1}} then the answer:
{"blind": {"place": "office", "objects": [], "photo": "no", "tells": []},
 "scores": {"composition": 6, "form": 5.5, "assembly": 7, "materials": 4, "light": 6, "detail": 5, "world": 6, "vibes": 9},
 "defects": [{"tile": "iso", "where": "left", "wrong": "the desks are plain boxes", "real": "a desk has legs", "fix": "model legs", "level": "blocking"},
             {"tile": "low", "wrong": "", "level": "detail"}],
 "show_as_is": "no"}"#;
    let v = crate::agent::last_verdict(text).unwrap();
    let (scores, defects, photo) = crate::agent::read(None, &v);
    assert_eq!(scores.len(), 7, "only the seven categories, each 0-10");
    assert_eq!(scores["form"], serde_json::json!(5.5));
    assert_eq!(defects.len(), 1, "a defect with nothing wrong named is dropped");
    assert_eq!(defects[0]["level"], "blocking");
    assert_eq!(photo, serde_json::json!(0.0));
    let (pass, why) = crate::rubric::pass(&scores, &defects);
    assert!(!pass && why[0].starts_with("blocking"));
    assert!(crate::agent::last_verdict("no json here").is_none());
    // what it saw, as Findings (DR3): a gating judge's blocking defect is an error, an advising one's a warn
    let f = &crate::findings(&defects, "kimi", false)[0];
    assert_eq!((f.check.as_str(), f.level, f.fix.as_deref(), f.by.as_str()), ("judge.iso", raea_core_facts::Level::Error, Some("model legs"), "raea judge (kimi)"));
    assert!(f.says.starts_with("iso sheet: "), "{}", f.says);
    assert_eq!(crate::findings(&defects, "kimi", true)[0].level, raea_core_facts::Level::Warn);
}

#[test]
fn each_make_kind_has_its_own_rubric_and_the_default_is_unchanged() {
    // docs/make.feature MK5: a kit's categories, a shot list's on the framing sheet alone, the bar said to each
    let kit = rubric::questions_for(Some("kit"), "clay");
    for k in ["score_mesh", "score_assembly", "score_scale"] {
        assert!(kit.get(k).is_some(), "{k} in {kit}");
    }
    assert!(kit.get("score_cohesion").is_none(), "cohesion is judged in colour");
    assert_eq!(rubric::sheets(Some("shot")), &[("colour", "framing.png")]);
    assert_eq!(rubric::sheets(None).len(), 2);
    let shot = rubric::questions_for(Some("shot"), "colour");
    assert!(shot.get("score_clearance").is_some() && shot.get("score_composition").is_none());
    assert!(rubric::state_for(Some("kit"), "colour", &json!({})).contains(rubric::BAR));
    assert!(rubric::state_for(Some("shot"), "colour", &json!({})).contains("framing sheet"));
    // the default's words are what it was calibrated on
    let d = rubric::state("clay", &json!({"notDoing": ["cats"]}));
    assert!(d.contains("as the person who will use the scene would") && d.contains("The scene must not contain: cats.") && !d.contains(rubric::BAR), "{d}");
    // every kind scored in full to pass
    let all = |k: Option<&str>, v: f64| -> serde_json::Map<String, Value> { rubric::scores(k).iter().map(|s| (s.0.to_string(), json!(v))).collect() };
    assert!(rubric::pass_for(Some("kit"), &all(Some("kit"), 8.0), &[]).0);
    let (ok, why) = rubric::pass_for(Some("kit"), &all(None, 8.0), &[]);
    assert!(!ok && why.iter().any(|w| w.contains("of 8 categories")), "{why:?}");
    assert!(kind_of(&["judge".into(), "d".into(), "--kind".into(), "film".into()]).is_err());
    assert_eq!(kind_of(&["judge".into(), "d".into(), "--kind".into(), "shot".into()]).unwrap().as_deref(), Some("shot"));
}

#[test]
fn an_agent_critic_is_asked_its_kinds_categories() {
    let p = crate::agent::prompt(Some("shot"), std::path::Path::new("/r/r1"), &json!({"look": "dusk"}), None);
    assert!(p.contains("/r/r1/framing.png") && p.contains("\"clearance\": 0") && p.contains(rubric::BAR) && !p.contains("PART 1, BLIND"), "{p}");
    let k = crate::agent::prompt(Some("kit"), std::path::Path::new("/r/r1"), &json!({}), None);
    assert!(k.contains("\"cohesion\": 0") && k.contains("scale bar") && k.contains("(judged on CLAY)"));
    let d = crate::agent::prompt(None, std::path::Path::new("/r/r1"), &json!({}), None);
    assert!(d.contains("\"composition\": 0") && !d.contains(rubric::BAR));
    let v = json!({"scores": {"cohesion": 7, "fidelity": 8, "composition": 9}});
    let (s, _, _) = crate::agent::read(Some("kit"), &v);
    assert_eq!(s.len(), 2, "only the kit's categories");
}

#[test]
fn a_kind_is_calibrated_on_its_own_categories() {
    // docs/make.feature MK6
    let s = |j: f64| -> serde_json::Map<String, Value> { rubric::KIT.iter().map(|c| (c.0.to_string(), json!(j))).collect() };
    let rows: Vec<Value> = (0..12).map(|i| if i % 2 == 0 { json!({"kind": "kit", "judge": s(8.0), "eye": s(5.0), "eye_pass": false, "defects": []}) } else { json!({"kind": "kit", "judge": s(9.0), "eye": s(8.0), "eye_pass": true, "defects": []}) }).collect();
    let cal = eye::fit_rows_for(Some("kit"), &rows);
    assert_eq!(cal["gate"], true);
    assert!(cal["maps"]["cohesion"].is_object() && cal["maps"]["form"].is_null());
}
