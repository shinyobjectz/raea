//! A person's taste: standing corrections kept across every project, read by every agent before any work. A rule is
//! words; a rule that's mechanical also carries a check `raea check` holds the work to: one of six typed forms, any of
//! raea's checks by name, or a project's own `.ts` check by its path from the taste file (docs/direction.feature DR4).
//! The person's taste, a workspace's and a project's are read the same way.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Taste {
    pub title: String,
    /// when this file is read ("Read for pitch, explain and story")
    pub note: Option<String>,
    #[serde(rename = "for")]
    #[ts(rename = "for")]
    pub for_: Option<String>,
    /// the kinds of work with their own taste file (motion, games, sound, ui)
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub kinds: Vec<String>,
    pub sections: Vec<TasteSection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TasteSection {
    /// "Always", "Never", "Camera"
    pub heading: String,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Rule {
    pub text: String,
    pub check: Option<RuleCheck>,
    /// what a named or a .ts check is held to (`{ band: 0.8 }`); a form carries its own value
    #[ts(type = "Record<string, number | string | boolean>", optional)]
    pub values: Option<Map<String, Value>>,
    /// how bad what its check finds is: error, warn or note (unset: a form's findings are errors, another check's keep
    /// its own level)
    pub level: Option<RuleLevel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RuleLevel {
    Error,
    Warn,
    Note,
}

/// What holds a line: one of the six forms raea measures on an export (`{ form: "text-above", value: 75 }`), or a
/// check by name (one of raea's, `"loudness"`) or by path (the project's own, `"../checks/cut.ts"`, written with
/// raea/check), held to the line's `values`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum RuleCheck {
    Form(Check),
    Named(String),
}

impl<'de> Deserialize<'de> for RuleCheck {
    // a name is a string; anything else is a form, and a wrong one says what was wrong with it (not "no variant")
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match Value::deserialize(d)? {
            Value::String(s) if !s.trim().is_empty() => Ok(RuleCheck::Named(s)),
            Value::String(_) => Err(serde::de::Error::custom("a check's name or path is empty")),
            v => Check::deserialize(v).map(RuleCheck::Form).map_err(serde::de::Error::custom),
        }
    }
}

impl RuleCheck {
    /// The form, when it is one of the six.
    pub fn form(&self) -> Option<Check> {
        match self {
            RuleCheck::Form(c) => Some(*c),
            RuleCheck::Named(_) => None,
        }
    }
}

/// What a check measures on an export. Values keep the units the checker has always used.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "form", content = "value", rename_all = "kebab-case")]
pub enum Check {
    /// every text holds at least this many seconds
    TextHold(f64),
    /// every text's bottom edge sits above this percent of the frame's height
    TextAbove(f64),
    /// every text's top edge sits below this percent of the frame's height
    TextBelow(f64),
    /// every text keeps this many px (at 1080 on the short side) from each edge
    TextMargin(f64),
    /// integrated loudness within 1 LU of this many LUFS
    Loudness(f64),
    /// nothing holds still longer than this many seconds
    StillMax(f64),
}

impl Check {
    /// The form's name as `raea check` reports it.
    pub fn form(&self) -> &'static str {
        match self {
            Check::TextHold(_) => "text-hold",
            Check::TextAbove(_) => "text-above",
            Check::TextBelow(_) => "text-below",
            Check::TextMargin(_) => "text-margin",
            Check::Loudness(_) => "loudness",
            Check::StillMax(_) => "still-max",
        }
    }
    pub fn value(&self) -> f64 {
        match *self {
            Check::TextHold(v) | Check::TextAbove(v) | Check::TextBelow(v) | Check::TextMargin(v) | Check::Loudness(v) | Check::StillMax(v) => v,
        }
    }
}

impl Taste {
    /// Every rule that carries one of the six forms, with its words (what `raea check VIDEO.mp4` measures).
    pub fn checks(&self) -> impl Iterator<Item = (&str, Check)> {
        self.sections.iter().flat_map(|s| s.rules.iter()).filter_map(|r| r.check.as_ref()?.form().map(|c| (r.text.as_str(), c)))
    }
}
