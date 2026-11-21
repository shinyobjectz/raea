//! Labels (docs/ask.feature AK6): whether a question's problem was really there in what one call sent, so the question
//! can be calibrated on the person's own eye (calibrate.rs). A label belongs to a question and to the clips a call sent
//! (by their hashes), never to one answer: the same question reworded, asked again of the same clips, has the same
//! labels, which is what lets `raea ask try` measure new words.
//!
//! Where labels come from:
//! - `raea ask verdict ANSWER WORD`: the person answers the question themselves, in its own terms (yes or no, an option,
//!   a level). Theirs counts first.
//! - The feed: a note of the person's on the render (its copy in the project's feed, known by `record::ident`), pinned
//!   inside the window or not pinned at all, holding one of the question's label words, is a yes; an approved post (done)
//!   with no such note is a no. Read again by every `raea ask calibrate`.
//! - Another eye (`verdict --eye agent`: an agent's own look) is kept as `eye:NAME` and never counted, as the
//!   fingerprint keeps another eye's readings (FP6d).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use raea_core_ask::{Kind, Question};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::record;

pub const FILE: &str = "labels.jsonl";

/// A note pinned at t covers 1.5 s before it to 0.5 s after (the fingerprint's span).
const BEFORE: f64 = 1.5;
const AFTER: f64 = 0.5;

/// Whether a question's problem is there in one call's clips, and who said so.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Label {
    pub question: String,
    /// the clips the call sent, by hash
    pub media: String,
    pub bad: bool,
    /// "verdict" (the person's), "note" or "approval" (read from the feed), or "eye:NAME" (kept, never counted)
    pub source: String,
    pub project: String,
    pub file: String,
    pub from: f64,
    pub to: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub at: String,
}

impl Label {
    /// The person's own: a verdict, a note or an approval.
    pub fn counted(&self) -> bool {
        self.rank() > 0
    }
    fn rank(&self) -> u8 {
        match self.source.as_str() {
            "verdict" => 3,
            "note" => 2,
            "approval" => 1,
            _ => 0,
        }
    }
}

/// The clips a call sent, as one key.
pub fn media_key(call: &Value) -> String {
    let mut m: Vec<&str> = call["media"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    m.sort_unstable();
    m.join("+")
}

/// The feed a call's render was posted to: its project's, else the nearest one above its file.
fn feed_of(call: &Value) -> Option<PathBuf> {
    let file = Path::new(call["file"].as_str().unwrap_or_default());
    call["project"].as_str().map(Path::new).into_iter().chain(file.ancestors()).map(|d| d.join(".raea/feed")).find(|d| d.is_dir())
}

/// A call's project: the folder holding its brief, else the folder holding the feed it was posted to, else its file's.
pub fn project_of(call: &Value) -> String {
    if let Some(p) = call["project"].as_str() {
        return p.to_string();
    }
    let home = feed_of(call).and_then(|f| f.parent()?.parent().map(Path::to_path_buf));
    let file = Path::new(call["file"].as_str().unwrap_or_default());
    home.as_deref().or(file.parent()).map(|p| p.display().to_string()).unwrap_or_default()
}

pub fn all() -> Vec<Label> {
    record::rows(FILE).into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect()
}

fn write(labels: &[Label]) -> Result<(), String> {
    record::write_whole(FILE, &labels.iter().filter_map(|l| serde_json::to_string(l).ok()).map(|l| l + "\n").collect::<String>())
}

/// The label that stands for each question and clips: the person's verdict, else a note, else an approval.
pub fn standing(labels: &[Label]) -> HashMap<(String, String), &Label> {
    let mut out: HashMap<(String, String), &Label> = HashMap::new();
    for l in labels.iter().filter(|l| l.counted()) {
        let key = (l.question.clone(), l.media.clone());
        if out.get(&key).is_none_or(|was| l.rank() >= was.rank()) {
            out.insert(key, l);
        }
    }
    out
}

/// A question as its answer kept it (the whole question, else its words).
pub fn question_of(answer: &Value) -> Question {
    serde_json::from_value(answer["q"].clone()).unwrap_or_else(|_| Question::plain(answer["question"].as_str().unwrap_or("q"), answer["ask"].as_str().unwrap_or_default()))
}

/// Whether the answer the person gives means the problem is there: yes's flagged answer (yes, or no for a question
/// flagged on no), a flagged option, or a grade or rate in its lower half.
pub fn means_problem(q: &Question, said: &str) -> Result<bool, String> {
    let said = said.trim().to_lowercase();
    let levels = q.levels();
    if q.kind == Kind::Yes {
        return match said.as_str() {
            "yes" | "no" => Ok((said == "yes") != q.flags.first().is_some_and(|f| f == "no")),
            _ => Err(format!("{}: answer yes or no", q.id)),
        };
    }
    if q.kind == Kind::Choose && q.flags.is_empty() {
        return Err(format!("{} picks without a wrong answer, so it isn't calibrated", q.id));
    }
    let Some(i) = levels.iter().position(|l| l.to_lowercase() == said) else { return Err(format!("{}: answer one of {}", q.id, levels.join(", "))) };
    Ok(match q.kind {
        Kind::Choose => q.flags.iter().any(|f| f.to_lowercase() == said),
        _ => (i as f64) < (levels.len().max(2) - 1) as f64 / 2.0,
    })
}

/// Every call in the record, by id.
pub fn calls() -> HashMap<String, Value> {
    record::rows("calls.jsonl").into_iter().filter_map(|c| Some((c["id"].as_str()?.to_string(), c))).collect()
}

fn label(q: &str, call: &Value, bad: bool, source: &str, note: Option<String>) -> Label {
    Label {
        question: q.to_string(),
        media: media_key(call),
        bad,
        source: source.into(),
        project: project_of(call),
        file: call["file"].as_str().unwrap_or_default().into(),
        from: call["from"].as_f64().unwrap_or(0.0),
        to: call["to"].as_f64(),
        note,
        at: record::now(),
    }
}

/// `raea ask verdict ANSWER WORD [--eye NAME]`: the person's answer to the question on that answer's clips (or another
/// eye's, kept and never counted). A verdict given again replaces the last from the same source.
pub fn verdict(answer: &str, said: &str, eye: Option<&str>) -> Result<String, String> {
    let row = record::rows("answers.jsonl").into_iter().find(|a| a["id"] == answer).ok_or_else(|| format!("ask verdict: no answer {answer} in {}", record::dir().display()))?;
    let call = calls().remove(row["call"].as_str().unwrap_or_default()).ok_or_else(|| format!("ask verdict: answer {answer}'s call isn't in the record"))?;
    let q = question_of(&row);
    let bad = means_problem(&q, said)?;
    let source = eye.map_or_else(|| "verdict".to_string(), |e| format!("eye:{e}"));
    let new = label(&q.id, &call, bad, &source, None);
    let mut labels = all();
    labels.retain(|l| !(l.question == new.question && l.media == new.media && l.source == new.source));
    let line = format!("{}: {} on {} ({}){}", q.id, if bad { "the problem is there" } else { "no problem" }, new.file, span(new.from, new.to), if eye.is_some() { format!(", kept as {source} and not counted") } else { String::new() });
    labels.push(new);
    write(&labels)?;
    Ok(line)
}

pub fn span(from: f64, to: Option<f64>) -> String {
    match to {
        Some(t) => format!("{from:.1}-{t:.1} s"),
        None if from > 0.0 => format!("from {from:.1} s"),
        None => "the whole file".into(),
    }
}

/// One post of a feed: whether it's approved, its person's notes (pinned second or none, words), and its files.
struct Post {
    done: bool,
    notes: Vec<(Option<f64>, String)>,
    files: Vec<PathBuf>,
}

fn posts(feed: &Path) -> Vec<Post> {
    let project = feed.parent().and_then(Path::parent).unwrap_or(feed);
    let lib = raea_io_home::user_home().unwrap_or_default().join(".raea/feed/media");
    let mut out = Vec::new();
    for e in std::fs::read_dir(feed).into_iter().flatten().flatten() {
        let Some(p) = std::fs::read_to_string(e.path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
        let notes = p["notes"].as_array().into_iter().flatten().filter(|n| n["author"]["kind"] == "person").filter_map(|n| Some((n["pin"]["t"].as_f64(), n["text"].as_str()?.to_lowercase()))).collect();
        let files = p["media"].as_array().into_iter().flatten().filter_map(|m| m["path"].as_str()).filter_map(|m| [lib.join(m), project.join(m)].into_iter().find(|f| f.is_file())).collect();
        out.push(Post { done: p["status"] == "done", notes, files });
    }
    out
}

/// Labels read again from the person's notes and approvals, for every recorded answer whose question has label words.
/// The feed's labels replace the last ones read; verdicts and other eyes' stay. Says how many of each it found.
pub fn from_feed() -> Result<(usize, usize), String> {
    let calls = calls();
    let mut feeds: HashMap<PathBuf, Vec<Post>> = HashMap::new();
    let mut idents: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut found: HashMap<(String, String), Label> = HashMap::new();
    for row in record::rows("answers.jsonl") {
        let q = question_of(&row);
        let Some(call) = calls.get(row["call"].as_str().unwrap_or_default()) else { continue };
        let (Some(ident), false) = (call["ident"].as_str(), q.label_words.is_empty()) else { continue };
        let Some(dir) = feed_of(call) else { continue };
        let len = record::ident_len(ident);
        let same = |f: &PathBuf, idents: &mut HashMap<PathBuf, Option<String>>| {
            std::fs::metadata(f).is_ok_and(|m| Some(m.len()) == len) && idents.entry(f.clone()).or_insert_with(|| record::ident(f)).as_deref() == Some(ident)
        };
        let (from, to) = (call["from"].as_f64().unwrap_or(0.0), call["to"].as_f64().unwrap_or(f64::INFINITY));
        let words: Vec<String> = q.label_words.iter().map(|w| w.to_lowercase()).collect();
        for post in feeds.entry(dir.clone()).or_insert_with(|| posts(&dir)).iter().filter(|p| p.files.iter().any(|f| same(f, &mut idents))) {
            let inside = |t: &Option<f64>| t.is_none_or(|t| t - BEFORE < to && t + AFTER > from);
            let seen = post.notes.iter().find(|(t, text)| inside(t) && words.iter().any(|w| text.contains(w.as_str())));
            let l = match (seen, post.done) {
                (Some((_, text)), _) => label(&q.id, call, true, "note", Some(text.clone())),
                (None, true) => label(&q.id, call, false, "approval", None),
                (None, false) => continue,
            };
            let key = (l.question.clone(), l.media.clone());
            if found.get(&key).is_none_or(|was| l.rank() > was.rank()) {
                found.insert(key, l);
            }
        }
    }
    let mut labels = all();
    labels.retain(|l| !matches!(l.source.as_str(), "note" | "approval"));
    let yes = found.values().filter(|l| l.bad).count();
    let no = found.len() - yes;
    labels.extend(found.into_values());
    write(&labels)?;
    Ok((yes, no))
}
