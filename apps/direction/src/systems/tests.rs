use super::*;
use raea_core_facts::{Level, Shot};
use serde_json::{json, Map, Value};

/// Frames 0..n of a 1080x1920 piece at 30 fps, each holding the texts `on(frame)` gives (words, box).
fn piece(n: i64, on: impl Fn(i64) -> Vec<(&'static str, [f64; 4])>) -> Vec<Facts> {
    (0..n)
        .map(|f| {
            let mut w = Facts::new(f, 1080, 1920, 30.0);
            for (words, b) in on(f) {
                let e = w.spawn(None);
                w.put(e, &Text { words: words.into(), font: None, size: Some(64.0), box_: Some(b), extruded: 0.0, lines: Some(1), needs: None });
            }
            w
        })
        .collect()
}

fn rule(check: &str, v: f64) -> Rule {
    Rule::named(check, "", Map::from_iter([("value".to_string(), Value::from(v))]), Level::Error, "taste taste.ts")
}

#[test]
fn a_text_on_too_briefly_is_found_once_over_its_frames() {
    // "GO" is on frames 10 to 24 (half a second), "STAY" all 90
    let w = piece(90, |f| {
        let mut v = vec![("STAY", [0.1, 0.1, 0.9, 0.2])];
        if (10..25).contains(&f) {
            v.push(("GO", [0.1, 0.5, 0.9, 0.6]));
        }
        v
    });
    let got = text(&rule("text-hold", 2.0), &w);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!((got[0].frames, got[0].about[0].name.as_str()), (Some([10, 25]), "GO"));
    assert!(got[0].says.starts_with("\"GO\" is on for 0.5 s, under 2.0 s"), "{}", got[0].says);
    assert_eq!(got[0].measure.as_ref().map(|m| (m.value, m.limit)), Some((0.5, Some(2.0))));
}

#[test]
fn placement_and_margin_name_the_worst_frame_of_a_run() {
    // a caption whose bottom slides down from 70% to 85% over frames 0 to 30
    let w = piece(31, |f| vec![("SALE", [0.2, 0.6, 0.8, 0.70 + 0.005 * f as f64])]);
    let above = text(&rule("text-above", 80.0), &w);
    assert_eq!(above.len(), 1);
    assert_eq!(above[0].frames, Some([21, 31]), "past 80% from frame 21: {above:?}");
    assert!(above[0].says.contains("reaches 85% down the frame, below 80%"), "{}", above[0].says);
    assert!(text(&rule("text-below", 50.0), &w).is_empty(), "it starts at 60%, below 50%");
    // 0.2 of 1080 is 216 px from the left and right; the bottom is 0.15 of 1920 = 288 px at its lowest
    assert!(text(&rule("text-margin", 200.0), &w).is_empty());
    assert_eq!(text(&rule("text-margin", 250.0), &w).len(), 1);
}

#[test]
fn a_rule_with_no_value_says_so() {
    let r = Rule { level: Some(Level::Warn), ..Rule::named("text-hold", "", Map::new(), Level::Error, "taste taste.ts") };
    let got = text(&r, &piece(3, |_| vec![]));
    assert_eq!((got.len(), got[0].level), (1, Level::Warn));
    assert!(got[0].says.contains("needs a value"));
}

#[test]
fn cuts_are_the_frame_before_and_the_first_frame_of_each_shot_after_the_first() {
    let mut w = piece(60, |_| vec![]);
    for f in &mut w {
        let (from, to) = if f.frame < 20 { (0, Some(20)) } else { (20, None) };
        let e = f.spawn(None);
        f.put(e, &Shot { name: None, from, to });
    }
    assert_eq!(Frames::Cuts.pick(&w), vec![19, 20]);
    assert_eq!(Frames::Once.pick(&w), vec![0]);
    assert_eq!(Frames::of(&json!("lines")).unwrap(), Frames::Lines);
    assert_eq!(Frames::of(&json!([3, 4])).unwrap(), Frames::At(vec![3, 4]));
    assert!(Frames::of(&json!("every")).unwrap_err().contains("not \"every\""));
}
