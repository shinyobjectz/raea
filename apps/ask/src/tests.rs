use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use raea_core_ask::{Facts, Kind, Picks, Question, Sees, Window, library};
use raea_io_clef::media::Probe;
use raea_io_clef::{Answered, Call, Fail};
use serde_json::{Map, Value, json};

use crate::prepare::{self, Piece, VOICE};
use crate::runner::{self, Answer, Entry, Event, Opts};

fn rubric() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../sdk/guides/craft/rubric.md")).expect("the craft rubric")
}

pub(crate) fn setup() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    // SAFETY: set once, before any test reads it, to the same value for every test
    ONCE.call_once(|| unsafe { std::env::set_var("RAEA_ASK_DIR", std::env::temp_dir().join(format!("raea-ask-test-{}", std::process::id()))) });
}

fn ids(qs: &[Question]) -> Vec<&str> {
    qs.iter().map(|q| q.id.as_str()).collect()
}

fn piece(file: &str, voice: Option<bool>) -> Arc<Piece> {
    let probe = Probe { seconds: 10.0, video: true, still: false, sound: true };
    Arc::new(Piece { file: PathBuf::from(file), probe, voice, ..Default::default() })
}

fn entry(p: &Arc<Piece>, window: Option<Window>, questions: Vec<Question>) -> Entry {
    Entry { piece: p.clone(), window, questions, dropped: vec![], with: vec![] }
}

#[test]
fn no_one_speaking_asks_sound_silent_and_a_file_without_sound_isnt_listened_to() {
    let r = rubric();
    let sound = library::find("sound", Some(&r)).unwrap();
    let silent = Facts { voice: Some(false), sound: true, video: true, ..Default::default() };
    let (asked, dropped) = prepare::keep(vec![sound.clone()], &silent, Some(&r));
    assert_eq!(ids(&asked), ["sound-silent"]);
    assert!(dropped[0].1.contains("no one speaks") && dropped[0].1.contains("sound-silent instead"), "{dropped:?}");
    let mut listen = Question::plain("loud", "Is it loud?");
    listen.sees = Sees::Nothing;
    let (asked, dropped) = prepare::keep(vec![listen], &Facts { video: true, ..Default::default() }, None);
    assert!(asked.is_empty() && dropped[0].1.contains("no sound"), "{dropped:?}");
    // not knowing whether anyone speaks, both are kept, and the plan adds the voice question to their call
    let unknown = Facts { sound: true, video: true, ..Default::default() };
    let (asked, _) = prepare::keep(vec![sound], &unknown, Some(&r));
    assert_eq!(ids(&asked), ["sound", "sound-silent"]);
    let (groups, _) = runner::plan(&[entry(&piece("a.mp4", None), None, asked)]);
    assert_eq!(groups.len(), 1);
    assert_eq!(ids(&groups[0].questions), ["sound", "sound-silent", VOICE]);
}

#[test]
fn the_state_says_what_the_piece_is() {
    let mut p = (*piece("a.mp4", Some(false))).clone();
    p.brief = json!({"title": "Spring launch", "mode": ["pitch"], "audience": "busy founders", "believe": "it saves them an hour."});
    p.extra = Some("raea measured -14 LUFS.".into());
    let s = p.state();
    for want in ["A pitch piece: \"Spring launch\".", "It's for: busy founders.", "believing: it saves them an hour.", "No one speaks", "runs 10 s", "-14 LUFS"] {
        assert!(s.contains(want), "{want}: {s}");
    }
    assert_eq!(p.facts().modes, ["pitch"]);
    // no brief and the caller's own state: the caller says what the piece is
    let own = Piece { extra: Some("A game: \"Rover\".".into()), ..(*piece("a.mp4", None)).clone() };
    assert_eq!(own.state(), "It runs 10 s. A game: \"Rover\".");
}

#[test]
fn calls_hold_64_questions_and_split_by_window_and_what_they_look_at() {
    let p = piece("a.mp4", Some(true));
    let many: Vec<Question> = (0..65).map(|i| Question::plain(&format!("q{i}"), "Is it loud?")).collect();
    let (groups, _) = runner::plan(&[entry(&p, None, many)]);
    assert_eq!(groups.iter().map(|g| g.questions.len()).collect::<Vec<_>>(), [63, 2], "one place a call is kept for the voice question");
    let hook = Window { from: Some(0.0), to: Some(3.0) };
    let mut still = Question::plain("s", "Is the text readable?");
    still.sees = Sees::Stills;
    let (groups, _) = runner::plan(&[entry(&p, None, vec![Question::plain("a", "Is it loud?"), still]), entry(&p, Some(hook), vec![Question::plain("b", "Is it loud?")])]);
    assert_eq!(groups.len(), 3);
    let mut pick = Question::plain("best", "Which suits the brief best?");
    pick.picks = Some(Picks::All);
    let (groups, unasked) = runner::plan(&[entry(&p, None, vec![pick])]);
    assert!(groups.is_empty() && unasked[0].not_asked.as_deref().is_some_and(|n| n.contains("--with")), "{unasked:?}");
    let (groups, unasked) = runner::plan(&[entry(&p, None, vec![library::find("decide.changed", None).unwrap()])]);
    assert!(groups.is_empty() && !unasked[0].version.is_empty(), "{unasked:?}");
}

/// clef's answers as a fake gives them: yes 0.8 to every yes question (0.9 to the voice question), the lowest level of
/// a grade.
fn answers(call: &Call) -> Map<String, Value> {
    call.questions
        .iter()
        .map(|q| {
            let a = match q.kind {
                Kind::Yes => json!({"noul": if q.id == VOICE { 0.9 } else { 0.8 }}),
                _ => json!({"choice": q.levels()[0], "probabilities": {q.levels()[0].clone(): 1.0}}),
            };
            (q.id.clone(), a)
        })
        .collect()
}

fn answered(call: &Call) -> Answered {
    Answered { answers: answers(call), cost: 0.001, cached: false, route: "a test".into(), pool_left: Some(1.5) }
}

/// A 2 s clip with a picture and a tone, made with FFmpeg.
fn clip(name: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("raea-ask-{}-{name}.mp4", std::process::id()));
    let ff = raea_io_assets::ffmpeg_tool("ffmpeg", false).expect("FFmpeg");
    let ok = std::process::Command::new(ff)
        .args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=10:duration=2", "-f", "lavfi", "-i", "sine=frequency=440:duration=2"])
        .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
        .arg(&out)
        .status()
        .expect("FFmpeg ran");
    assert!(ok.success());
    out
}

fn run(entries: Vec<Entry>, jobs: usize, ask: fn(&Call) -> Result<Answered, Fail>) -> (runner::Summary, Vec<Answer>) {
    setup();
    let mut got = Vec::new();
    let sum = runner::run(entries, &Opts { jobs, dry: false, ask }, &mut |e| {
        if let Event::Answer(a) = e {
            got.push(a.clone());
        }
    });
    (sum, got)
}

#[test]
fn each_clip_is_cut_once_and_the_voice_answer_sets_a_variant_aside() {
    let file = clip("once");
    let p = Arc::new(Piece::read(&file, None, None, None, None).unwrap());
    assert!(p.probe.video && p.probe.sound && p.voice.is_none());
    let r = rubric();
    let (asked, _) = prepare::keep(vec![library::find("sound", Some(&r)).unwrap()], &p.facts(), Some(&r));
    let mut listen = Question::plain("loud", "Is it too loud?");
    listen.sees = Sees::Nothing;
    let (sum, got) = run(vec![entry(&p, None, [asked, vec![listen]].concat())], 4, |c| Ok(answered(c)));
    assert_eq!((sum.calls, sum.clips), (2, 2), "the picture's call and the sound's call share one cut of the sound");
    let by = |id: &str| got.iter().find(|a| a.id == id).unwrap_or_else(|| panic!("{id}: {got:?}"));
    assert!(by("sound").says.is_some() && by("loud").flagged);
    assert!(by("sound-silent").not_asked.as_deref().is_some_and(|n| n.contains("someone speaks (0.90)")), "{:?}", by("sound-silent"));
    assert!(!got.iter().any(|a| a.id == VOICE), "the voice question isn't told");
    assert_eq!((sum.answered, sum.cost, sum.pool_left), (2, 0.002, Some(1.5)));
}

static CALLS: AtomicUsize = AtomicUsize::new(0);

#[test]
fn a_pool_spent_partway_keeps_what_came_and_the_rest_waits() {
    let file = clip("spent");
    let p = Arc::new(Piece::read(&file, None, None, Some(true), None).unwrap());
    let at = |s: f64| Some(Window { from: Some(s), to: Some(s + 0.5) });
    let entries = (0..4).map(|i| entry(&p, at(i as f64 * 0.4), vec![Question::plain(&format!("w{i}"), "Is it loud?")])).collect();
    let (sum, got) = run(entries, 1, |c| match CALLS.fetch_add(1, Ordering::SeqCst) {
        2 => Err(Fail::Spent("this month's pool is used up".into())),
        _ => Ok(answered(c)),
    });
    assert_eq!((sum.calls, sum.answered, sum.waiting), (2, 2, 2));
    assert_eq!(got.iter().filter(|a| a.waits.as_deref().is_some_and(|w| w.contains("used up"))).count(), 2);
    assert_eq!(CALLS.load(Ordering::SeqCst), 3, "nothing is sent after the pool is spent");
}

#[test]
fn own_words_keep_their_id() {
    assert_eq!(crate::verbs::own_id("Does the music fight the voice?"), "own-does-the-music-fight-the-voice");
    assert_eq!(crate::verbs::own_id("Does the music fight the voice?"), crate::verbs::own_id("does the MUSIC fight the voice"));
}
