//! A fingerprint's `videoform` section (docs/videoform.feature VF2): the kind of video a person makes again and again,
//! read once from its references. It sits in fingerprint.ts beside the axes, so a videoform is found, inherited,
//! taught and shared as any fingerprint is. Each part names the home that does its job already: references in a
//! brief's shape, the structure as a NAME.script.ts template, the rules as NAME.taste.ts, taste lines; and the vocabulary.ts the
//! person's AI writes from the context (VF5).

use crate::brief::Reference;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Videoform {
    /// What the videos are, in the person's words, as a videoform made before VF8f kept it: one line. It's read into
    /// `contexts` as the person's first, and never written again
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub context: String,
    /// Everything said of the videos, the person's words and the documents given with them (VF8f): each with who
    /// said it, the group of videos it's for, its brand, and when. The person's own words come first, then the
    /// documents, newest first (`ranked`); the person's AI writes the vocabulary and briefs.ts from all of it
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Context>>", optional)]
    pub contexts: Vec<Context>,
    /// Each reference as a brief names one: its link or its path as the target, its title and creator as the note.
    /// What was read of each is kept beside fingerprint.ts in videoform/references.jsonl, by content hash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Reference>>", optional)]
    pub references: Vec<Reference>,
    /// The vocabulary.ts the person's AI wrote from the context: the things to look out for (VF5)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vocabulary: Option<String>,
    /// The NAME.script.ts a project made to the videoform starts from: its slots, their order and lengths (VF7)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// The NAME.taste.ts its rules are written to (VF8): taste lines, each with the check that holds it (a standing rule
    /// is a taste line, docs/projects.feature PJ9), mined from the references with how strongly each holds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<String>,
    /// The person's word on what was found, by measure, over what the references show (VF4b): kept through references
    /// added and read again, each with why
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Correction>>", optional)]
    pub corrections: Vec<Correction>,
}

/// One thing said of a videoform's videos (VF8f): the person's words as they gave videos (`by: "person"`), or a
/// document given with them (`by: "document"`: a brief, a brand's guide, notes), whose passages are kept by its content
/// hash in videoform/documents/HASH.json
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Context {
    /// person or document
    pub by: String,
    /// the person's words; a document's title
    pub text: String,
    /// the videos it's for, as `add --group` named them (a brand and its round, "Dutch R1"); none: all of them
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    /// the day it was given
    pub at: String,
    /// a document's content hash, and where it was read from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// What `by` can say
pub const BYS: [&str; 2] = ["person", "document"];

/// briefs.ts (VF8f): what the person's AI reads in the documents that measuring can't: a constraint, a negation, an
/// order, a choice ("never put text on a face", "Gift Exchange before Wishlist", "captions or a text hook, never
/// both"). Each is checked against its group's videos by measurement, and one that holds in several groups is the
/// videoform's
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Briefs {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<BriefRule>>", optional)]
    pub rules: Vec<BriefRule>,
}

/// One rule read in a document: what it says, the passage it's from, and how it's measured. `kind` is always (each
/// of `what` is in the video), never (none is), before (the first of `what` comes before the second), either (not
/// both), open or close (each of `what` is in the first or last fifth). `what` names measured things: `said:REGEX`,
/// `shown:REGEX` (words on screen), `layer:FORM` or `layer:FORM-KIND` (layer:screen-screenshot), `captions`, `faces:N`,
/// `seconds:LO-HI` (the video's length)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct BriefRule {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// the passage's id (raea videoform docs NAME lists them)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub kind: String,
    pub what: Vec<String>,
}

/// What a brief rule's `kind` can be
pub const RULE_KINDS: [&str; 6] = ["always", "never", "before", "either", "open", "close"];

/// One correction: `ignore` (the references' range isn't the videoform's, nothing's checked), `free` (it varies,
/// reported but not checked), `keep` (checked though it varies) or `target` (checked against `range`, not theirs)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Correction {
    pub measure: String,
    pub say: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// What a correction can say
pub const SAYS: [&str; 4] = ["ignore", "free", "keep", "target"];

/// vocabulary.ts (VF5): the person's AI's reading of the context, beside fingerprint.ts. The context says what
/// matters; the references show how it's done. Each term is looked for in the references' shots, and what it did is
/// reported, so the AI can revise it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Vocabulary {
    /// The context it was written from
    pub context: String,
    /// The places the videos are in, each with the things in it
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Term>>", optional)]
    pub places: Vec<Term>,
    /// Things seen anywhere (the host with a microphone, the product in hand)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Term>>", optional)]
    pub things: Vec<Term>,
    /// Things heard, each with how it sounds (a crowd laughing, a whoosh)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Term>>", optional)]
    pub sounds: Vec<Term>,
    /// Words on screen, as patterns (a price, a store's name)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Pattern>>", optional)]
    pub text: Vec<Pattern>,
    /// Words said, as patterns ("available at", "link in bio")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Pattern>>", optional)]
    pub said: Vec<Pattern>,
    /// The parts the videos move through (the host's opener, the challenge, the call to action), each found by the
    /// terms and patterns it names
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Section>>", optional)]
    pub sections: Vec<Section>,
    /// The context's words for how it feels ("snappy"), each with the measures that say it (raea videoform report
    /// names them): those measures are checked, by the context's word, even where the references vary
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Feel>>", optional)]
    pub feel: Vec<Feel>,
    /// What's in the references but not the videoform's (a watermark, a sponsor's end card), phrased as looks
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub ignore: Vec<String>,
}

/// A thing to look or listen for: its phrasings, its near-misses, and how often the context says it's there
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Term {
    pub name: String,
    /// Several ways to say it, as a caption would ("a man holding a microphone on a city street")
    pub looks: Vec<String>,
    /// What it's easily taken for, so a shot nearer those isn't counted
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub unlike: Vec<String>,
    /// A place's things
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Term>>", optional)]
    pub things: Vec<Term>,
    /// What the context says of it: always (in every video) or usually
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how: Option<String>,
}

/// Words on screen or said, as a case-insensitive regular expression
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Pattern {
    pub name: String,
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how: Option<String>,
}

/// A part of the video: where it falls (open, middle, close; anywhere when left out), what shows it (terms and
/// patterns by name), and how often the context says it's there
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Section {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    pub shows: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how: Option<String>,
}

/// A word of the context's for how the videos feel, and the measures that say it
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Feel {
    pub word: String,
    pub measures: Vec<String>,
}

/// What a term's, pattern's or section's `how` can say; where a section can be
pub const HOWS: [&str; 2] = ["always", "usually"];
pub const ATS: [&str; 3] = ["open", "middle", "close"];

impl Videoform {
    /// What's been said, the person's words first (the latest first), then the documents, newest first; a videoform
    /// made before VF8f has its one line as the person's
    pub fn ranked(&self) -> Vec<Context> {
        let mut all = self.contexts.clone();
        if !self.context.is_empty() && !all.iter().any(|c| c.by == "person" && c.text == self.context) {
            all.push(Context { by: "person".into(), text: self.context.clone(), ..Default::default() });
        }
        all.sort_by(|a, b| (a.by != "person").cmp(&(b.by != "person")).then(b.at.cmp(&a.at)));
        all
    }

    /// The person's words, all of them, as one passage the vocabulary is started from (the latest first)
    pub fn said(&self) -> String {
        self.ranked().iter().filter(|c| c.by == "person").map(|c| match &c.group {
            Some(g) => format!("({g}) {}", c.text),
            None => c.text.clone(),
        }).collect::<Vec<_>>().join("\n")
    }

    /// A context kept once: the same words for the same group, or the same document, aren't kept twice
    pub fn tell(&mut self, c: Context) -> bool {
        if !self.context.is_empty() {
            let old = std::mem::take(&mut self.context);
            self.tell(Context { by: "person".into(), text: old, ..Default::default() });
        }
        let same = |x: &Context| match (&x.document, &c.document) {
            (Some(a), Some(b)) => a == b && x.group == c.group,
            _ => x.by == c.by && x.text == c.text && x.group == c.group,
        };
        if self.contexts.iter().any(same) {
            return false;
        }
        self.contexts.push(c);
        true
    }

    /// A reference added once: the same target isn't listed twice
    pub fn add(&mut self, target: &str, note: Option<String>) -> bool {
        if self.references.iter().any(|r| r.target == target) {
            return false;
        }
        self.references.push(Reference { target: target.to_string(), note });
        true
    }

    /// The person's correction of a measure, in place of an earlier one; `None` takes it back
    pub fn correct(&mut self, measure: &str, c: Option<Correction>) {
        self.corrections.retain(|x| x.measure != measure);
        self.corrections.extend(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_from_its_json_and_lists_a_target_once() {
        let mut v: Videoform = crate::read("videoform", r#"{"context": "street interviews"}"#).unwrap();
        assert!(v.add("https://example.com/a", Some("A, by B".into())));
        assert!(!v.add("https://example.com/a", None));
        assert_eq!(v.ranked()[0].text, "street interviews", "a videoform's one line is the person's");
        let doc = Context { by: "document".into(), text: "Brief".into(), group: Some("Dutch R1".into()), at: "2026-11-25".into(), document: Some("ab".into()), ..Default::default() };
        assert!(v.tell(doc.clone()) && !v.tell(doc));
        assert!(v.tell(Context { by: "person".into(), text: "watch the hooks".into(), at: "2026-11-25".into(), ..Default::default() }));
        let r = v.ranked();
        assert_eq!((r[0].text.as_str(), r[2].by.as_str()), ("watch the hooks", "document"), "the person's latest first, documents after");
        let back = serde_json::to_string(&v).unwrap();
        assert!(back.starts_with(r#"{"contexts":[{"by":"person","text":"street interviews","at":""},"#), "{back}");
        assert!(crate::read::<Videoform>("videoform", r#"{"context": "x", "refs": []}"#).is_err());
        let c = |say: &str| Correction { measure: "loudness".into(), say: say.into(), ..Default::default() };
        v.correct("loudness", Some(c("free")));
        v.correct("loudness", Some(c("ignore")));
        assert_eq!(v.corrections, vec![c("ignore")], "a measure has one correction, the latest");
        v.correct("loudness", None);
        assert!(v.corrections.is_empty());
    }
}
