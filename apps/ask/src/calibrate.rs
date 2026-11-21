//! Calibration (docs/ask.feature AK7): a question's answers held to the person's labels (labels.rs), per version of its
//! words. Clef isn't trained on the labels, so its AUC on them is held out already; the one thing fitted is the
//! threshold, the problem score that flags 10% of the clean labels. The gates are the fingerprint's (16 labels of each
//! kind, from 3 projects, an AUC of 0.8). A question that passes is calibrated at its threshold: its answers are
//! findings, and the person's calibration replaces raea's shipped one. One under 0.6 after 32 labels doesn't separate,
//! and says which labels it missed, so its words can be fixed. `raea ask try` asks new words of the labelled clips and
//! sets both AUCs side by side.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use raea_core_ask::{Calibration, Question};
use raea_io_clef::Media;
use raea_sys_fingerprint::head::{auc, quantile};
use raea_sys_fingerprint::train::{GATE_AUC, GATE_LABELS, GATE_PROJECTS};
use serde::Serialize;
use serde_json::Value;

use crate::labels::{self, Label};
use crate::prepare::Piece;
use crate::record;
use crate::runner::{self, Event, Group, Kept, Opts, Summary};

pub const FILE: &str = "calibration.json";
/// An AUC under this after twice the gate's labels: the words don't separate the problem.
const SEPARATES: f64 = 0.6;

/// The person's calibrations that passed the gates, by question version.
pub fn local() -> BTreeMap<String, Calibration> {
    std::fs::read_to_string(record::dir().join(FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// A question with the person's calibration in place of raea's, when it has one.
pub fn apply(q: &mut Question, local: &BTreeMap<String, Calibration>) {
    if let Some(c) = local.get(&q.version()) {
        q.calibration = Some(c.clone());
    }
}

/// A label the answers missed: a problem it scored low, or clean work it scored high.
#[derive(Debug, Clone, Serialize)]
pub struct Miss {
    pub file: String,
    pub span: String,
    pub problem: f64,
    pub bad: bool,
    pub note: Option<String>,
}

/// One version's standing.
#[derive(Debug, Clone, Serialize)]
pub struct Standing {
    pub id: String,
    pub version: String,
    pub ask: String,
    pub problems: usize,
    pub clean: usize,
    pub projects: usize,
    pub auc: Option<f64>,
    pub threshold: Option<f64>,
    /// calibrated, advice or "doesn't separate"
    pub state: String,
    pub why: String,
    pub misses: Vec<Miss>,
    /// other eyes' labels on its clips, kept and not counted
    pub others: usize,
}

/// Each labelled clip's latest problem score for one version, beside its label.
fn scored<'a>(version: &str, standing: &HashMap<(String, String), &'a Label>, calls: &HashMap<String, Value>) -> Vec<(f64, &'a Label)> {
    let mut latest: HashMap<String, (f64, &Label)> = HashMap::new();
    for a in record::rows("answers.jsonl").iter().filter(|a| a["version"] == version) {
        let (Some(p), Some(call)) = (a["problem"].as_f64(), calls.get(a["call"].as_str().unwrap_or_default())) else { continue };
        let key = (a["question"].as_str().unwrap_or_default().to_string(), labels::media_key(call));
        if let Some(l) = standing.get(&key) {
            latest.insert(key.1, (p, l));
        }
    }
    latest.into_values().collect()
}

/// One version held to its labels.
pub fn standing_of(q: &Question, all: &[Label], calls: &HashMap<String, Value>) -> Standing {
    let standing = labels::standing(all);
    let s = scored(&q.version(), &standing, calls);
    let score = |bad: bool| s.iter().filter(|(_, l)| l.bad == bad).map(|(p, _)| *p as f32).collect::<Vec<f32>>();
    let (pos, neg) = (score(true), score(false));
    let projects = s.iter().map(|(_, l)| l.project.as_str()).collect::<BTreeSet<_>>().len();
    let a = Some(auc(&pos, &neg)).filter(|a| a.is_finite());
    let threshold = Some(quantile(&neg, 0.9) as f64).filter(|t| t.is_finite());
    let mut why = Vec::new();
    if pos.len() < GATE_LABELS || neg.len() < GATE_LABELS {
        why.push(format!("{} problem and {} clean labels (needs {GATE_LABELS} of each)", pos.len(), neg.len()));
    }
    if projects < GATE_PROJECTS {
        why.push(format!("labels from {projects} projects (needs {GATE_PROJECTS})"));
    }
    match a {
        Some(v) if v < GATE_AUC => why.push(format!("AUC {v:.2} (needs {GATE_AUC})")),
        None => why.push("no AUC yet: it needs labels of both kinds".into()),
        _ => {}
    }
    let stuck = s.len() >= 2 * GATE_LABELS && a.is_some_and(|v| v < SEPARATES);
    let mut misses: Vec<Miss> = Vec::new();
    if stuck {
        let mut by: Vec<&(f64, &Label)> = s.iter().collect();
        by.sort_by(|x, y| x.0.total_cmp(&y.0));
        let low = by.iter().filter(|(_, l)| l.bad).take(5);
        let high = by.iter().rev().filter(|(_, l)| !l.bad).take(5);
        misses = low.chain(high).map(|(p, l)| Miss { file: l.file.clone(), span: labels::span(l.from, l.to), problem: *p, bad: l.bad, note: l.note.clone() }).collect();
    }
    let others = all.iter().filter(|l| !l.counted() && l.question == q.id).count();
    let state = match (why.is_empty(), stuck) {
        (true, _) => "calibrated",
        (false, true) => "doesn't separate",
        (false, false) => "advice",
    };
    let why = if why.is_empty() { format!("earned: {} labels from {projects} projects, flags past {:.2}", s.len(), threshold.unwrap_or(0.0)) } else { why.join("; ") };
    Standing { id: q.id.clone(), version: q.version(), ask: q.ask.clone(), problems: pos.len(), clean: neg.len(), projects, auc: a, threshold, state: state.into(), why, misses, others }
}

/// Every labelled question's versions as the record kept them (or only `id`'s), newest wording last.
fn versions(only: Option<&str>, all: &[Label]) -> Vec<Question> {
    let labelled: BTreeSet<&str> = all.iter().map(|l| l.question.as_str()).collect();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for a in record::rows("answers.jsonl") {
        let id = a["question"].as_str().unwrap_or_default();
        if only.is_some_and(|o| o != id) || !labelled.contains(id) || !seen.insert(a["version"].as_str().unwrap_or_default().to_string()) {
            continue;
        }
        out.push(labels::question_of(&a));
    }
    out
}

/// `raea ask calibrate [ID]`: the feed's labels read again, then every labelled version held to them. The ones that
/// pass are kept as the person's calibration; one that no longer passes loses it.
pub fn calibrate(only: Option<&str>) -> Result<(Vec<Standing>, (usize, usize)), String> {
    let feed = labels::from_feed()?;
    let (all, calls) = (labels::all(), labels::calls());
    let out: Vec<Standing> = versions(only, &all).iter().map(|q| standing_of(q, &all, &calls)).collect();
    let mut keep = local();
    for s in &out {
        keep.remove(&s.version);
        if s.state == "calibrated" {
            let n = (s.problems + s.clean) as u32;
            let c = Calibration { auc: s.auc.unwrap_or(0.0), n, positives: s.problems as u32, threshold: s.threshold, date: today(), by: "you".into() };
            keep.insert(s.version.clone(), c);
        }
    }
    record::write_whole(FILE, &serde_json::to_string_pretty(&keep).map_err(|e| e.to_string())?)?;
    Ok((out, feed))
}

/// YYYY-MM-DD, UTC.
fn today() -> String {
    raea_io_trace::utc(std::time::SystemTime::now()).0.chars().take(10).collect()
}

/// The groups `raea ask try` sends: the new question on each labelled clip set, with the state it was asked with.
pub fn try_groups(like: &str, words: &str) -> Result<(Question, Question, Vec<Group>), String> {
    let all = labels::all();
    let standing = labels::standing(&all);
    let rows = record::rows("answers.jsonl");
    let old = rows.iter().rev().find(|a| a["question"] == like).map(labels::question_of).ok_or_else(|| format!("ask try: no answers to {like} in the record"))?;
    let new = Question { ask: words.to_string(), calibration: None, ..old.clone() };
    let calls = labels::calls();
    let mut groups: Vec<Group> = Vec::new();
    let mut sent = BTreeSet::new();
    for call in calls.values().filter(|c| standing.contains_key(&(like.to_string(), labels::media_key(c)))) {
        let names: Vec<&str> = call["media"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let Some(kept) = media(&names) else { continue };
        if !sent.insert(labels::media_key(call)) {
            continue;
        }
        let piece = Arc::new(Piece { file: PathBuf::from(call["file"].as_str().unwrap_or_default()), project: call["project"].as_str().map(PathBuf::from), ..Default::default() });
        let (from, to) = (call["from"].as_f64().unwrap_or(0.0), call["to"].as_f64());
        let seconds = to.map_or(60.0, |t| t - from);
        let kept = Kept { video_seconds: if kept.videos.is_empty() { 0.0 } else { seconds }, sound_seconds: if kept.sounds.is_empty() { 0.0 } else { seconds }, media: kept, state: call["state"].as_str().unwrap_or_default().into() };
        groups.push(Group { piece, from, to, sees: new.sees, hears: new.hears, with: vec![], questions: vec![new.clone()], kept: Some(Arc::new(kept)) });
    }
    if groups.is_empty() {
        return Err(format!("ask try: none of {like}'s labelled clips are still kept (raea ask calibrate lists its labels)"));
    }
    Ok((old, new, groups))
}

/// A call's clips from the kept media, in the kinds they were sent as; None when one has been pruned.
fn media(names: &[&str]) -> Option<Media> {
    let mut m = Media::default();
    for n in names {
        let bytes = record::media(n)?;
        match n.rsplit('.').next() {
            Some("mp4") => m.videos.push(bytes),
            Some("mp3") => m.sounds.push(bytes),
            _ => m.stills.push(bytes),
        }
    }
    Some(m)
}

/// `raea ask try "WORDS" --like ID`: the new words asked of the old question's labelled clips, and both versions held to
/// the same labels.
pub fn try_words(like: &str, words: &str, o: &Opts, out: &mut dyn FnMut(Event)) -> Result<(Summary, Option<(Standing, Standing)>), String> {
    let (old, new, groups) = try_groups(like, words)?;
    let sum = runner::run_groups(groups, Vec::new(), o, out);
    if o.dry {
        return Ok((sum, None));
    }
    let (all, calls) = (labels::all(), labels::calls());
    Ok((sum, Some((standing_of(&old, &all, &calls), standing_of(&new, &all, &calls)))))
}
