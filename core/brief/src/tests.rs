use super::*;

#[test]
fn declarations_match_the_committed_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../sdk/raea/src/brief.gen.ts");
    let now = declarations();
    if std::env::var_os("RAEA_WRITE").is_some() {
        std::fs::write(path, &now).unwrap();
    }
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert!(committed == now, "sdk/raea/src/brief.gen.ts is out of date: RAEA_WRITE=1 cargo test -p raea-core-brief");
}

#[test]
fn a_brief_reads_and_its_doctor_asks_for_what_is_missing() {
    let b: Brief = read("brief.ts", r#"{"title": "A walkthrough", "mode": ["explain"], "assumed": ["voice"], "voice": "one calm narrator",
        "deliver": [{"name": "Walkthrough", "specs": ["60 s", "16:9"]}]}"#).unwrap();
    let d = brief::doctor(&b);
    assert!(d.iter().any(|m| m.contains("`believe`")), "{d:?}");
    assert!(d.iter().any(|m| m.contains("no script")), "{d:?}");
    assert!(d.iter().any(|m| m.contains("`voice` is assumed")), "{d:?}");
}

#[test]
fn a_misspelt_field_is_an_error_that_names_it() {
    let e = read::<Brief>("brief.ts", r#"{"title": "x", "belive": "y"}"#).unwrap_err();
    assert!(e.contains("belive"), "{e}");
}

#[test]
fn a_board_and_a_screenplay_are_one_script() {
    let board: Script = read("script.ts", r#"{"title": "Walkthrough", "scenes": [
        {"beats": [{"onScreen": "the brief page, a cursor typing one line", "lines": [{"who": "narrator", "says": "It starts with one line."}]}], "payoff": "a brief exists"}]}"#).unwrap();
    let play: Script = read("script.ts", r#"{"title": "The night before", "scenes": [
        {"heading": "INT. SENATE - NIGHT", "beats": [{"onScreen": "Torches gutter.", "camera": "push in, keeps moving", "cut": "match on motion",
          "lines": [{"who": "CASSIUS", "says": "Tomorrow.", "how": "quietly"}], "cues": ["strings drop out"], "onWord": "Tomorrow"}]}]}"#).unwrap();
    assert_eq!(board.lines().next().unwrap().who, "narrator");
    assert_eq!(play.lines().next().unwrap().how.as_deref(), Some("quietly"));
}

#[test]
fn taste_checks_are_typed_values() {
    let t: Taste = read("motion.ts", r#"{"title": "Taste: motion", "sections": [{"heading": "Picture", "rules": [
        {"text": "Text sits well clear of the bottom of the frame.", "check": {"form": "text-above", "value": 54}},
        {"text": "The look is neutral and bright."}]}]}"#).unwrap();
    let c: Vec<_> = t.checks().collect();
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].1.form(), c[0].1.value()), ("text-above", 54.0));
    assert!(read::<Taste>("motion.ts", r#"{"title": "t", "sections": [{"heading": "h", "rules": [{"text": "x", "check": {"form": "glow-max", "value": 1}}]}]}"#).is_err());
}

#[test]
fn a_taste_line_holds_any_check_by_name_or_path_with_values_and_a_level() {
    let t: Taste = read("taste.ts", r#"{"title": "Proof", "sections": [{"heading": "Rules", "rules": [
        {"text": "Captions clear the button band", "check": "../checks/caption.ts", "values": {"band": 0.8}, "level": "warn"},
        {"text": "Loud enough", "check": "loudness", "values": {"value": -14}}]}]}"#).unwrap();
    let r = &t.sections[0].rules;
    assert_eq!((r[0].check.as_ref(), r[0].values.as_ref().and_then(|v| v["band"].as_f64()), r[0].level), (Some(&taste::RuleCheck::Named("../checks/caption.ts".into())), Some(0.8), Some(taste::RuleLevel::Warn)));
    assert_eq!(t.checks().count(), 0, "only the six forms are what a video's check measures");
    // it reads back as written, and a wrong level or an empty name says what's wrong
    assert_eq!(read::<Taste>("taste.ts", &serde_json::to_string(&t).unwrap()).unwrap(), t);
    let bad = |rule: &str| read::<Taste>("taste.ts", &format!(r#"{{"title": "t", "sections": [{{"heading": "h", "rules": [{rule}]}}]}}"#)).unwrap_err();
    assert!(bad(r#"{"text": "x", "check": "cut.ts", "level": "warning"}"#).contains("expected one of `error`, `warn`, `note`"));
    assert!(bad(r#"{"text": "x", "check": ""}"#).contains("empty"));
    assert!(bad(r#"{"text": "x", "check": {"form": "text-above", "value": "high"}}"#).contains("invalid type"));
}

#[test]
fn an_objects_parts_leave_nothing_without_a_material() {
    let ok: Parts = read("feather.parts.ts", r#"{"concept": {"what": "flight feather", "style": "realistic", "size": 0.25, "seenFrom": 0.3, "material": "keratin"},
        "parts": {
          "rachis": {"role": "shaft", "level": "primary"},
          "vane": {"role": "vane", "level": "secondary", "parent": "rachis", "count": 2, "style": "barred brown"},
          "barb": {"role": "barb", "level": "tertiary", "parent": "vane", "count": {"min": 300, "max": 500}, "near": "separate barbs, a few split gaps"}}}"#).unwrap();
    assert_eq!(parts::check(&ok), Vec::<String>::new());
    assert_eq!(parts::resolve(&ok, "barb").unwrap(), parts::Resolved { material: "keratin".into(), style: "barred brown".into() });

    let mut bad = ok.clone();
    bad.concept.material = None;
    bad.parts.get_mut("rachis").unwrap().parent = Some("calamus".into());
    let e = parts::check(&bad);
    assert!(e.iter().any(|m| m.contains("`calamus` isn't planned")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("no root")), "{e:?}");
}

#[test]
fn a_close_view_needs_tertiary_detail() {
    let p: Parts = read("horse.parts.ts", r#"{"concept": {"what": "toy rocking horse", "style": "painted wood", "size": 0.9, "seenFrom": 0.5, "material": "beech"},
        "parts": {"body": {"role": "body", "level": "primary"}, "rocker": {"role": "rocker", "level": "secondary", "parent": "body", "count": 2}}}"#).unwrap();
    assert!(parts::check(&p).iter().any(|m| m.contains("tertiary")));
}

#[test]
fn an_older_brief_with_stages_still_reads_and_never_writes_them() {
    let b: Brief = read("brief.ts", r#"{"title": "Old", "stages": [{"name": "", "tasks": [{"text": "Write the plan", "mark": "todo"}]}]}"#).unwrap();
    assert!(b.stages.is_some());
    assert!(!serde_json::to_string(&b).unwrap().contains("stages"));
    assert!(!declarations().contains("stages"), "the schema an agent writes has no stages (docs/plan.feature PL8a)");
}

#[test]
fn a_make_plan_reads_and_its_doctor_names_what_it_lacks() {
    let b: Brief = read("brief.ts", r#"{"title": "Flight desk kit", "make": {"kind": "kit",
        "dials": {"count": 2, "wear": {"value": 0.4, "source": "customer", "means": "scuffed bakelite"}, "time_of_day": "night"},
        "objects": [{"id": "phone", "label": "Rotary phone", "h": 0.12, "prompt": "a rotary desk phone", "source": "invented"},
                    {"id": "ashtray", "label": "Glass ashtray"}]}}"#).unwrap();
    let m = b.make.as_ref().unwrap();
    assert_eq!(m.kind, make::Kind::Kit);
    let d = make::doctor(m);
    assert!(d.iter().any(|x| x.contains("`time_of_day`")), "a scene's dial on a kit: {d:?}");
    assert!(d.iter().any(|x| x.contains("no `style`")), "{d:?}");
    assert!(d.iter().any(|x| x.contains("`ashtray` has no size")), "{d:?}");
    assert!(d.iter().any(|x| x.contains("`ashtray` is generated but has no prompt")), "{d:?}");
    assert!(!d.iter().any(|x| x.contains("`phone`")), "{d:?}");
    assert!(brief::doctor(&b).iter().any(|x| x.starts_with("make: ")));
    let want = make::deliverables(m);
    assert!(want.contains(&"out/kit/phone/phone_lod1.glb".to_string()) && want.contains(&"out/kit/kit.json".to_string()), "{want:?}");
}

#[test]
fn a_shot_plan_wants_a_still_or_a_move_per_shot() {
    let m: make::Make = read("make", r#"{"kind": "shot", "shots": [{"name": "counter_push", "move": "push_in"}, {"name": "Bad Name"}]}"#).unwrap();
    assert_eq!(make::deliverables(&m)[0], "out/shots/counter_push.png|out/shots/counter_push.mp4");
    assert!(make::doctor(&m).iter().any(|x| x.contains("`Bad Name`")));
}
