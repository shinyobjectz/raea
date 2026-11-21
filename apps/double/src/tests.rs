use std::path::PathBuf;

use raea_app_ask::runner::Opts;
use raea_core_ask::Sees;
use raea_io_clef::{Answered, Call, Fail};
use raea_sys_fingerprint::taxonomy::base;
use raea_sys_fingerprint_use::score::{AxisScore, Moment, Scored, Window};
use serde_json::json;

use crate::{ask, lines, moments, question, row};

fn moment(a: f64, b: f64, axis: &str) -> Moment {
    Moment { a, b, axis: axis.into(), words: String::new(), by: "head", state: "gating".into(), band: Some("strong"), gating: true, peak: 1.0, at: None }
}

fn window(a: f64, axis: &str, band: &'static str, flagged: bool) -> Window {
    let s = AxisScore { axis: axis.into(), value: 0.5, by: "head", state: "advice".into(), band: Some(band), flagged, gating: false };
    Window { a, b: a + 2.0, axes: vec![s], seen: None, heard: None, level: -20.0, overall: None }
}

fn scored(moments: Vec<Moment>, windows: Vec<Window>) -> Scored {
    Scored { file: "a.mp4".into(), media: "m".into(), dur: 10.0, fingerprint: "test".into(), moments, windows }
}

#[test]
fn flags_are_asked_and_unsure_adds_the_plausible_windows_run_together() {
    let fp = base("test");
    let s = scored(vec![moment(0.0, 3.0, "blur")], vec![window(4.0, "glitch", "plausible", false), window(5.0, "glitch", "plausible", false), window(8.0, "glitch", "weak", false)]);
    assert_eq!(moments(&s, &fp, "flags").len(), 1);
    let unsure = moments(&s, &fp, "unsure");
    assert_eq!(unsure.iter().map(|d| (d.a, d.b, d.axis.as_str(), d.flagged)).collect::<Vec<_>>(), [(0.0, 3.0, "blur", true), (4.0, 7.0, "glitch", false)]);
    assert!(unsure[0].words.contains("blurry"), "an axis is asked in its own words: {}", unsure[0].words);
}

#[test]
fn an_axis_asks_as_it_senses_and_a_quality_is_flagged_on_no() {
    let fp = base("test");
    let blur = question(fp.axis("blur").unwrap());
    assert!(blur.ask.starts_with("Is there a blurry") && blur.sees == Sees::Video && !blur.hears && blur.flags.is_empty());
    assert_eq!(blur.label_words, fp.axis("blur").unwrap().words, "the person's notes in the axis's words label it");
    let muffled = question(fp.axis("muffled voice").unwrap());
    assert!(muffled.sees == Sees::Nothing && muffled.hears);
    let mut good = fp.axis("blur").unwrap().clone();
    good.polarity = "good".into();
    let q = question(&good);
    assert!(q.ask.starts_with("Is this ") && q.flags == ["no"]);
}

/// clef as a fake: yes to the blur question, no to anything else.
fn blur_only(call: &Call) -> Result<Answered, Fail> {
    let answers = call.questions.iter().map(|q| (q.id.clone(), json!({"noul": if q.id == "fp.blur" { 0.8 } else { 0.1 }}))).collect();
    Ok(Answered { answers, cost: 0.001, cached: false, route: "a test".into(), pool_left: None })
}

/// A 6 s clip with a picture and a tone.
fn clip() -> PathBuf {
    let out = std::env::temp_dir().join(format!("raea-double-{}.mp4", std::process::id()));
    let ff = raea_io_assets::ffmpeg_tool("ffmpeg", false).expect("FFmpeg");
    let ok = std::process::Command::new(ff)
        .args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=10:duration=6", "-f", "lavfi", "-i", "sine=frequency=440:duration=6"])
        .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
        .arg(&out)
        .status()
        .expect("FFmpeg ran");
    assert!(ok.success());
    out
}

#[test]
fn each_flag_is_asked_on_its_own_window_and_said_beside_it_and_kept_as_clefs() {
    // SAFETY: this crate's only test that reads it, set before the runner starts
    unsafe { std::env::set_var("RAEA_ASK_DIR", std::env::temp_dir().join(format!("raea-double-ask-{}", std::process::id()))) };
    let fp = base("test");
    let file = clip();
    let s = scored(vec![moment(0.0, 2.0, "blur"), moment(0.0, 2.0, "glitch"), moment(3.0, 5.0, "muffled voice")], vec![]);
    let ds = ask(&file, &fp, moments(&s, &fp, "flags"), &Opts { jobs: 2, dry: false, ask: blur_only });
    let said: Vec<String> = ds.iter().map(|d| format!("{}: {}", d.axis, d.said())).collect();
    assert_eq!(said, ["blur: clef agrees (0.80)", "glitch: clef doesn't see it (0.10)", "muffled voice: clef doesn't see it (0.10)"]);
    assert!(lines(&ds).starts_with("asked again of clef (3 moments):\n  0:00-0:02  blur: clef agrees (0.80)\n"), "{}", lines(&ds));
    let gone = ask(&std::env::temp_dir().join("raea-double-none.mp4"), &fp, moments(&s, &fp, "flags"), &Opts { jobs: 2, dry: false, ask: blur_only });
    assert!(gone.iter().all(|d| d.agrees.is_none() && d.said().starts_with("clef wasn't asked")), "a file it can't read asks nothing: {gone:?}");
    let r = row(&ds[0], "m", "p").unwrap();
    assert!(r.bad && r.source == "eye:clef" && r.who == "clef" && (r.a, r.b) == (0.0, 2.0));
    assert!(!row(&ds[1], "m", "p").unwrap().bad);
    let mut unanswered = ds[0].clone();
    unanswered.agrees = None;
    assert!(row(&unanswered, "m", "p").is_none(), "a moment clef didn't answer isn't evidence");
}
