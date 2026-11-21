use super::*;
use serde_json::json;

const RUBRIC: &str = include_str!("../../../sdk/guides/craft/rubric.md");

#[test]
fn declarations_match_the_committed_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../sdk/raea/src/ask.gen.ts");
    let now = declarations();
    if std::env::var_os("RAEA_WRITE").is_some() {
        std::fs::write(path, &now).unwrap();
    }
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert!(committed == now, "sdk/raea/src/ask.gen.ts is out of date: RAEA_WRITE=1 cargo test -p raea-core-ask");
}

#[test]
fn every_standard_question_reads_and_fits_the_limits() {
    let all = library::all(Some(RUBRIC));
    assert!(all.len() >= 45, "{} questions", all.len());
    for q in &all {
        assert_eq!(limits::check(q), None, "{}", q.id);
    }
    let mut ids: Vec<&str> = all.iter().map(|q| q.id.as_str()).collect();
    ids.sort();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len(), "an id is used twice");
    for q in all.iter().filter(|q| q.swap.is_some()) {
        assert!(library::find(q.swap.as_deref().unwrap(), Some(RUBRIC)).is_some(), "{} swaps to a question that isn't there", q.id);
    }
}

#[test]
fn the_rubric_comes_from_its_guide_and_sound_swaps_when_no_one_speaks() {
    let r = library::rubric(RUBRIC);
    let sound = r.iter().find(|q| q.id == "sound").unwrap();
    assert_eq!((sound.kind, sound.when.voice, sound.swap.as_deref()), (Kind::Grade, Some(true), Some("sound-silent")));
    let silent = r.iter().find(|q| q.id == "sound-silent").unwrap();
    assert_eq!(silent.when.voice, Some(false));
    let open = r.iter().find(|q| q.id == "open").unwrap();
    assert_eq!(open.when.modes, ["pitch", "story", "explain"]);
    let facts = Facts { modes: vec!["pitch".into()], voice: Some(false), sound: true, video: true, ..Default::default() };
    assert_eq!(sound.not_for(&facts).as_deref(), Some("no one speaks in it"));
    assert_eq!(silent.not_for(&facts), None);
    assert!(r.iter().find(|q| q.id == "loop").unwrap().not_for(&facts).unwrap().contains("explain"));
}

#[test]
fn a_question_needs_what_the_file_has() {
    let mut q = Question::plain("hum", "Is there a hum under the voice?");
    q.sees = Sees::Nothing;
    let still = Facts { video: false, sound: false, ..Default::default() };
    assert!(q.not_for(&still).unwrap().contains("no sound"));
    let picture = Question::plain("cut", "Does the cut jump?");
    assert!(picture.not_for(&still).unwrap().contains("still"));
    assert_eq!(picture.not_for(&Facts { video: true, ..Default::default() }), None);
}

#[test]
fn each_type_goes_out_as_clefs_own_and_reads_back_as_a_problem_score() {
    let mut q = Question::plain("fits", "Does the sound fit the picture?");
    q.flags = vec!["no".into()];
    assert_eq!(q.wire(), json!({"type": "noul", "instructions": "Does the sound fit the picture?"}));
    let r = q.read(&json!({"noul": 0.2}));
    assert_eq!((r.value, r.problem), (0.2, Some(0.8)));
    assert!(q.flagged(&r), "advice flags past even odds");

    let mut g = Question::plain("pace", "How well is it paced?");
    g.kind = Kind::Grade;
    assert_eq!(g.wire()["criteria"].as_object().unwrap().len(), 5);
    let r = g.read(&json!({"choice": "strong", "probabilities": {"strong": 0.5, "excellent": 0.5}}));
    assert_eq!((r.value, r.problem, r.choice.as_deref()), (3.5, Some(0.125), Some("excellent")));

    let mut rate = Question::plain("look", "How strong is the look?");
    rate.kind = Kind::Rate;
    rate.options = vec!["none".into(), "some".into(), "strong".into()];
    assert_eq!(rate.wire()["criteria"], json!(["none", "some", "strong"]), "a score's levels are a list");
    assert_eq!(rate.read(&json!({"score": 1.0})).problem, Some(0.5));

    let mut c = Question::plain("best", "Which is best?");
    c.kind = Kind::Choose;
    c.options = vec!["first".into(), "second".into()];
    let r = c.read(&json!({"choice": "second", "probabilities": {"first": 0.3, "second": 0.7}}));
    assert_eq!((r.choice.as_deref(), r.problem), (Some("second"), None), "a pick has no problem score");
}

#[test]
fn a_question_in_clefs_own_format_comes_back_as_it_went() {
    for w in [
        json!({"type": "noul", "instructions": "Is it a photograph?"}),
        json!({"type": "noul", "instructions": "Is it a photograph?", "criteria": {"true": "a real place", "false": "a render"}}),
        json!({"type": "score", "instructions": "How is the light?", "criteria": ["none", "flat", "good"]}),
        json!({"type": "choice", "instructions": "Which?", "criteria": {"a": null, "b": "the second"}}),
    ] {
        assert_eq!(Question::from_wire("q", &w).unwrap().wire(), w);
    }
    assert!(Question::from_wire("q", &json!({"type": "essay", "instructions": "Say"})).is_none());
}

#[test]
fn a_calibrated_question_flags_at_its_own_threshold() {
    let q = library::find("face.text_over", None).unwrap();
    assert!(q.calibrated());
    assert!(q.flagged(&q.read(&json!({"noul": 0.36}))), "0.36 is past its 0.3487");
    let weak = library::find("speech.uncaptioned", None).unwrap();
    assert!(!weak.calibrated(), "an AUC of 0.45 is advice");
}

#[test]
fn a_new_wording_is_a_new_version_and_the_id_is_not_part_of_it() {
    let a = Question::plain("a", "Is it loud?");
    let b = Question::plain("b", "Is it loud?");
    let c = Question::plain("a", "Is it too loud?");
    assert_eq!(a.version(), b.version());
    assert_ne!(a.version(), c.version());
    assert_eq!(a.version(), "3ab49a6dcc96d4ad", "the same on every build (FNV-1a)");
}

#[test]
fn limits_refuse_what_raea_api_would() {
    let mut q = Question::plain("a b", "x");
    assert!(limits::check(&q).unwrap().contains("id"));
    q.id = "ok".into();
    q.kind = Kind::Choose;
    q.options = vec!["one".into()];
    assert!(limits::check(&q).unwrap().contains("2 to 16"));
    q.options = vec!["one".into(), "one".into()];
    assert!(limits::check(&q).unwrap().contains("twice"));
    let e = limits::estimate(60.0, 60.0, 0, 400, 3);
    assert!(e > 0.005 && e < 0.008, "{e}");
}
