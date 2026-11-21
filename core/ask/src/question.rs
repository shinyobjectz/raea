//! A question for the decision model (docs/ask.feature AK1): clef-omni's own types, what a question needs to be asked,
//! and how its answer reads as a problem score, so every question is calibrated the same way whatever its type.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

/// The five levels a `grade` reads over unless it names its own (the rubric's, and checks.rs's "score").
pub const LEVELS: [&str; 5] = ["poor", "weak", "acceptable", "strong", "excellent"];

/// clef's question types: `yes` is its noul, `choose` and `grade` its choice, `rate` its score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// the probability of yes
    #[default]
    Yes,
    /// one of its options, with a probability on each
    Choose,
    /// ordered levels, lowest first, read as the expected level
    Grade,
    /// clef's own rubric: 2 to 10 levels, its probability-weighted level
    Rate,
}

/// What of the picture a question looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Sees {
    /// the moving picture (2 frames a second, at most a minute)
    #[default]
    Video,
    /// stills (a sheet, a storyboard's panels, frames picked out)
    Stills,
    /// no picture: the sound or the words alone
    Nothing,
}

/// Its options are the files it's given, in order (first, second ...), or all but the first, which is the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Picks {
    All,
    AfterFirst,
}

/// What a piece must be for a question to be asked of it. Unset is any.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
pub struct When {
    /// the brief's modes it's for (pitch, story, explain, play, tool); empty is every mode
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub modes: Vec<String>,
    /// true: only when someone is heard speaking; false: only when no one is
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<bool>,
    /// true: only for a game; false: never for one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<bool>,
}

/// The seconds a question looks at; unset ends are the file's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
pub struct Window {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<f64>,
}

/// How well a question's answers separate work with the problem from work without it, on labels (AK7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
pub struct Calibration {
    pub auc: f64,
    pub n: u32,
    pub positives: u32,
    /// the problem score at which it flags (10% false positives on its labels)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// YYYY-MM-DD
    pub date: String,
    /// whose labels: "raea" (raea's bench, shipped) or "you"
    pub by: String,
}

/// A question, as the standard sets, a taste line, a batch file or the command line write it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Question {
    /// letters, digits, `_`, `.` and `-`
    pub id: String,
    /// the question in words, as clef reads it
    pub ask: String,
    #[serde(rename = "type", default)]
    #[ts(rename = "type", as = "Option<Kind>", optional)]
    pub kind: Kind,
    /// choose: its options; grade and rate: its levels, lowest first (a grade's are LEVELS unless given)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub options: Vec<String>,
    /// what an option (or yes and no) means, when its name alone doesn't say
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<BTreeMap<String, String>>", optional)]
    pub means: BTreeMap<String, String>,
    /// the answers that mean a problem: yes's "yes" (the default) or "no"; choose's options (none: a pick, no problem)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub flags: Vec<String>,
    #[serde(default)]
    #[ts(as = "Option<Sees>", optional)]
    pub sees: Sees,
    /// whether it listens to the sound (true unless set)
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    #[ts(as = "Option<bool>", optional)]
    pub hears: bool,
    /// what the state tells clef beside the media: brief, words (heard), screen-words, measures, brand, platform; and
    /// with: it compares the file with the files set beside it, so it isn't asked without them
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub uses: Vec<String>,
    #[serde(default, skip_serializing_if = "When::is_any")]
    #[ts(as = "Option<When>", optional)]
    pub when: When,
    /// the question asked in its place when `when` doesn't hold (sound -> sound-silent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<Window>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picks: Option<Picks>,
    /// words in a person's note that mean they saw this problem (AK6)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub label_words: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calibration: Option<Calibration>,
    /// what to change when it flags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

fn yes() -> bool {
    true
}
fn is_true(b: &bool) -> bool {
    *b
}

impl When {
    pub fn is_any(&self) -> bool {
        self.modes.is_empty() && self.voice.is_none() && self.game.is_none()
    }
}

/// What raea knows of a piece before asking (apps/ask prepare.rs reads it), for `when` and for what the media can be.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub modes: Vec<String>,
    /// someone is heard speaking: Some(true), no one: Some(false), unknown: None
    pub voice: Option<bool>,
    pub game: bool,
    pub sound: bool,
    pub video: bool,
}

/// An answer read: its value (yes's probability, choose's top probability, a level), its problem score (0 to 1, higher
/// is likelier a problem; none for a pick), and a line in words.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reading {
    pub value: f64,
    pub problem: Option<f64>,
    pub choice: Option<String>,
    pub says: String,
}

impl Question {
    /// A yes or no question in plain words, as the command line and a taste line make one.
    pub fn plain(id: &str, ask: &str) -> Question {
        serde_json::from_value(json!({"id": id, "ask": ask})).expect("a plain question")
    }

    /// A question written in clef's own wire format (`{type: noul | choice | score, instructions, criteria}`), as raea
    /// judge's rubric and the playtest packs write them; None when it isn't one.
    pub fn from_wire(id: &str, w: &Value) -> Option<Question> {
        let mut q = Question::plain(id, w["instructions"].as_str()?);
        match w["type"].as_str()? {
            "noul" => {
                if let (Some(y), Some(n)) = (w["criteria"]["true"].as_str(), w["criteria"]["false"].as_str()) {
                    q.means = BTreeMap::from([("yes".into(), y.into()), ("no".into(), n.into())]);
                }
            }
            "choice" => {
                q.kind = Kind::Choose;
                for (k, v) in w["criteria"].as_object()? {
                    q.options.push(k.clone());
                    if let Some(m) = v.as_str() {
                        q.means.insert(k.clone(), m.into());
                    }
                }
            }
            "score" => {
                q.kind = Kind::Rate;
                q.options = w["criteria"].as_array()?.iter().map(|l| l.as_str().map(str::to_string)).collect::<Option<_>>()?;
            }
            _ => return None,
        }
        Some(q)
    }

    /// Its levels or options as clef is sent them.
    pub fn levels(&self) -> Vec<String> {
        match (self.kind, self.options.is_empty()) {
            (Kind::Grade, true) => LEVELS.iter().map(|s| s.to_string()).collect(),
            _ => self.options.clone(),
        }
    }

    /// A hash of what clef reads (its words, type, options and their meanings): a new wording is a new version, with
    /// its own calibration. FNV-1a, so it's the same on every build.
    pub fn version(&self) -> String {
        let text = format!("{:?}\u{1f}{}\u{1f}{}\u{1f}{:?}", self.kind, self.ask, self.levels().join("\u{1e}"), self.means);
        let h = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
        format!("{h:016x}")
    }

    /// Why it can't be asked of this piece, or None when it can.
    pub fn not_for(&self, f: &Facts) -> Option<String> {
        let w = &self.when;
        if !w.modes.is_empty() && !w.modes.iter().any(|m| m == "all" || f.modes.contains(m)) {
            return Some(format!("it's for {} work, and this is {}", w.modes.join(" or "), if f.modes.is_empty() { "unnamed".into() } else { f.modes.join(" and ") }));
        }
        match (w.voice, f.voice) {
            (Some(true), Some(false)) => return Some("no one speaks in it".into()),
            (Some(false), Some(true)) => return Some("someone speaks in it".into()),
            _ => {}
        }
        if w.game.is_some_and(|g| g != f.game) {
            return Some(if f.game { "it isn't asked of a game".into() } else { "it's asked of games".into() });
        }
        if self.hears && self.sees == Sees::Nothing && !f.sound {
            return Some("it listens, and the file has no sound".into());
        }
        if self.sees == Sees::Video && !f.video && !(self.hears && f.sound) {
            return Some("it watches the picture move, and the file is a still".into());
        }
        None
    }

    /// The question as clef is sent it.
    pub fn wire(&self) -> Value {
        match self.kind {
            Kind::Yes => {
                let mut q = json!({"type": "noul", "instructions": self.ask});
                if let (Some(y), Some(n)) = (self.means.get("yes"), self.means.get("no")) {
                    q["criteria"] = json!({"true": y, "false": n});
                }
                q
            }
            Kind::Choose | Kind::Grade => {
                let criteria: serde_json::Map<String, Value> = self.levels().into_iter().map(|o| {
                    let m = self.means.get(&o).map(|s| Value::from(s.as_str())).unwrap_or(Value::Null);
                    (o, m)
                }).collect();
                json!({"type": "choice", "instructions": self.ask, "criteria": criteria})
            }
            Kind::Rate => json!({"type": "score", "instructions": self.ask, "criteria": self.levels()}),
        }
    }

    /// clef's answer read (the reading checks.rs and the playtest DSL make).
    pub fn read(&self, answer: &Value) -> Reading {
        let levels = self.levels();
        let top = (levels.len().max(2) - 1) as f64;
        let pr = |k: &str| answer["probabilities"][k].as_f64().unwrap_or(0.0);
        match self.kind {
            Kind::Yes => {
                let p = answer["noul"].as_f64().unwrap_or(0.0);
                let bad = if self.flags.first().is_some_and(|f| f == "no") { 1.0 - p } else { p };
                Reading { value: p, problem: Some(bad), choice: None, says: format!("{} ({p:.2} yes)", if p >= 0.5 { "yes" } else { "no" }) }
            }
            Kind::Choose => {
                let choice = answer["choice"].as_str().map(str::to_string);
                let p = choice.as_deref().map(pr).unwrap_or(0.0);
                let problem = (!self.flags.is_empty()).then(|| self.flags.iter().map(|f| pr(f)).sum::<f64>());
                Reading { value: p, problem, says: format!("{} ({p:.2})", choice.as_deref().unwrap_or("no answer")), choice }
            }
            Kind::Grade | Kind::Rate => {
                let v = if self.kind == Kind::Rate {
                    answer["score"].as_f64().unwrap_or(0.0)
                } else {
                    let total: f64 = levels.iter().map(|l| pr(l)).sum();
                    match total > 0.0 {
                        true => levels.iter().enumerate().map(|(i, l)| i as f64 * pr(l)).sum::<f64>() / total,
                        false => levels.iter().position(|l| answer["choice"].as_str() == Some(l)).unwrap_or(0) as f64,
                    }
                };
                let name = levels.get(v.round() as usize).cloned();
                Reading { value: v, problem: Some((1.0 - v / top).clamp(0.0, 1.0)), says: format!("{} ({v:.1} of {top})", name.as_deref().unwrap_or("?")), choice: name }
            }
        }
    }

    /// Calibrated: its labels separate (an AUC of 0.8 or more, the fingerprint's gate) and it has a threshold. Until
    /// then its answers are advice.
    pub fn calibrated(&self) -> bool {
        self.calibration.as_ref().is_some_and(|c| c.auc >= 0.8 && c.threshold.is_some())
    }

    /// Whether a reading flags a problem: past its calibrated threshold, else past even odds (as advice).
    pub fn flagged(&self, r: &Reading) -> bool {
        let at = self.calibration.as_ref().and_then(|c| c.threshold).filter(|_| self.calibrated()).unwrap_or(0.5);
        r.problem.is_some_and(|p| p >= at)
    }
}
