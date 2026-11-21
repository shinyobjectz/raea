use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use raea_core_ask::{Kind, Question, Reading};
use raea_io_clef::{Answered, Call, Fail};
use serde_json::json;

use crate::labels::{self, means_problem};
use crate::prepare::Piece;
use crate::runner::{self, Entry, Opts};
use crate::{calibrate, record};

/// The labels and calibration files are rewritten whole: one test at a time.
static LOCK: Mutex<()> = Mutex::new(());

fn reading(p: f64) -> Reading {
    Reading { value: p, problem: Some(p), choice: None, says: format!("{p:.2}") }
}

fn question(id: &str, words: &[&str]) -> Question {
    let mut q = Question::plain(id, &format!("Is there a problem the {id} test names?"));
    q.label_words = words.iter().map(|w| w.to_string()).collect();
    q
}

/// A call and its answer kept, as the runner keeps them; the answer's id.
fn asked(file: &Path, project: &str, clip: &str, q: &Question, p: f64) -> String {
    let call = record::call(&file.display().to_string(), Some(project), 0.0, Some(5.0), "a test", &[clip.to_string()], "a test", 0.0, false);
    record::answer(&call, &file.display().to_string(), q, &reading(p), p >= 0.5)
}

fn write(p: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

#[test]
fn a_persons_note_on_the_posted_render_labels_it_and_an_approval_labels_it_clean() {
    crate::tests::setup();
    let _one = LOCK.lock().unwrap();
    let proj = std::env::temp_dir().join(format!("raea-ask-notes-{}", std::process::id()));
    let (a, b, other) = (proj.join("out/a.mp4"), proj.join("out/b.mp4"), proj.join("old/a.mp4"));
    write(&a, b"render one");
    write(&b, b"render two");
    write(&other, b"another render, the same name");
    let post = |media: &str, status: &str, notes: serde_json::Value| json!({"media": [{"kind": "video", "path": media}], "status": status, "notes": notes});
    let person = |text: &str, t: Option<f64>| json!({"author": {"kind": "person", "name": "me"}, "text": text, "pin": t.map(|t| json!({"t": t}))});
    write(&proj.join(".raea/feed/1.json"), post("out/a.mp4", "seen", json!([person("The caption runs OVER HER FACE here", Some(2.0))])).to_string().as_bytes());
    let agent = json!({"author": {"kind": "agent"}, "text": "over her face"});
    write(&proj.join(".raea/feed/2.json"), post("out/b.mp4", "done", json!([agent, person("lovely", None)])).to_string().as_bytes());
    let q = question("notes.face", &["over her face"]);
    let project = proj.display().to_string();
    asked(&a, &project, "na.mp4", &q, 0.7);
    asked(&b, &project, "nb.mp4", &q, 0.2);
    asked(&other, &project, "nc.mp4", &q, 0.6);
    assert_eq!(labels::from_feed().unwrap(), (1, 1));
    let mine: Vec<_> = labels::all().into_iter().filter(|l| l.question == "notes.face").collect();
    let on = |clip: &str| mine.iter().find(|l| l.media == clip).unwrap_or_else(|| panic!("{clip}: {mine:?}"));
    assert!(on("na.mp4").bad && on("na.mp4").source == "note" && on("na.mp4").note.as_deref().is_some_and(|n| n.contains("over her face")));
    assert!(!on("nb.mp4").bad && on("nb.mp4").source == "approval", "an agent's note isn't the person's");
    assert!(!mine.iter().any(|l| l.media == "nc.mp4"), "another render with the same name isn't the one posted");
}

#[test]
fn a_verdict_reads_in_the_questions_terms_and_another_eyes_isnt_counted() {
    crate::tests::setup();
    let _one = LOCK.lock().unwrap();
    let mut q = question("verdict.logo", &[]);
    assert!(means_problem(&q, "Yes").unwrap() && !means_problem(&q, "no").unwrap());
    q.flags = vec!["no".into()];
    assert!(means_problem(&q, "no").unwrap(), "a question flagged on no: no is the problem");
    let mut g = q.clone();
    g.kind = Kind::Grade;
    assert!(means_problem(&g, "weak").unwrap() && !means_problem(&g, "acceptable").unwrap());
    g.kind = Kind::Choose;
    g.options = vec!["first".into(), "second".into()];
    g.flags = vec![];
    assert!(means_problem(&g, "first").is_err(), "a pick has no wrong answer");
    let file = std::env::temp_dir().join("raea-ask-verdict.mp4");
    let id = asked(&file, "p", "v1.mp4", &q, 0.3);
    labels::verdict(&id, "no", Some("agent")).unwrap();
    let said = labels::verdict(&id, "no", None).unwrap();
    assert!(said.contains("the problem is there"), "{said}");
    let all = labels::all();
    let standing = labels::standing(&all);
    let l = standing.get(&("verdict.logo".to_string(), "v1.mp4".to_string())).unwrap();
    assert!(l.bad && l.source == "verdict");
    assert!(all.iter().any(|l| l.source == "eye:agent" && !l.counted()));
}

/// clef as a fake: yes to anything sent a clip whose bytes start "bad".
fn sees_bad(call: &Call) -> Result<Answered, Fail> {
    let p = if call.media.videos.iter().any(|v| v.starts_with(b"bad")) { 0.9 } else { 0.1 };
    let answers = call.questions.iter().map(|q| (q.id.clone(), json!({"noul": p}))).collect();
    Ok(Answered { answers, cost: 0.001, cached: false, route: "a test".into(), pool_left: None })
}

#[test]
fn labels_from_three_projects_calibrate_a_question_and_new_words_are_measured_on_the_same_clips() {
    crate::tests::setup();
    let _one = LOCK.lock().unwrap();
    let (good, flat) = (question("cal.good", &[]), question("cal.flat", &[]));
    let file = PathBuf::from("/nowhere/cal.mp4");
    for i in 0..36 {
        let bad = i % 2 == 0;
        let clip = record::keep_media(format!("{}{i}", if bad { "bad" } else { "ok" }).as_bytes(), "mp4");
        let project = format!("p{}", i % 3);
        let call = record::call(&file.display().to_string(), Some(&project), 0.0, Some(3.0), "a test", &[clip], "a test", 0.0, false);
        let (p, word) = if bad { (0.6 + i as f64 / 1000.0, "yes") } else { (0.3 + i as f64 / 1000.0, "no") };
        for (q, p) in [(&good, p), (&flat, 0.5)] {
            let id = record::answer(&call, &file.display().to_string(), q, &reading(p), false);
            labels::verdict(&id, word, None).unwrap();
        }
    }
    let (all, _) = calibrate::calibrate(None).unwrap();
    let of = |id: &str| all.iter().find(|s| s.id == id).unwrap_or_else(|| panic!("{id}: {all:?}"));
    let g = of("cal.good");
    assert_eq!((g.state.as_str(), g.problems, g.clean, g.projects, g.auc), ("calibrated", 18, 18, 3, Some(1.0)), "{g:?}");
    let f = of("cal.flat");
    assert_eq!(f.state, "doesn't separate", "{f:?}");
    assert!(!f.misses.is_empty() && f.auc == Some(0.5));
    // the person's calibration goes out with the question from now on
    let piece = Arc::new(Piece { file: file.clone(), ..Default::default() });
    let (groups, _) = runner::plan(&[Entry { piece, window: None, questions: vec![good.clone()], dropped: vec![], with: vec![] }]);
    let sent = &groups[0].questions[0];
    assert!(sent.calibrated() && sent.calibration.as_ref().is_some_and(|c| c.by == "you"), "{sent:?}");
    // new words for the flat question, asked of its labelled clips
    let (sum, both) = calibrate::try_words("cal.flat", "Does anything look broken?", &Opts { jobs: 4, dry: false, ask: sees_bad }, &mut |_| {}).unwrap();
    let (old, new) = both.unwrap();
    assert_eq!(sum.calls, 36);
    assert_eq!((old.auc, new.auc, new.problems + new.clean), (Some(0.5), Some(1.0), 36), "{new:?}");
}
