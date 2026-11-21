//! A fingerprint's `videoform` section (docs/videoform.feature VF2): the kind of video a person makes again and again,
//! read once from its references. It sits in fingerprint.ts beside the axes, so a videoform is found, inherited,
//! taught and shared as any fingerprint is. Each part names the home that does its job already: references in a
//! brief's shape, the structure as a NAME.script.ts template, the rules as a NAME.rules.ts controller; and the vocabulary.ts the
//! person's AI writes from the context (VF5).

use crate::brief::Reference;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Videoform {
    /// What the videos are, in the person's words ("street interviews for a brand: the host, a stranger, the product in
    /// hand"); the person's own AI expands it into the vocabulary
    pub context: String,
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
    /// The NAME.rules.ts its rules are written to (VF8): a controller whose fields are rules (docs/direction.feature
    /// DR4), mined from the references, each with how strongly it holds and the check that holds it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<String>,
    /// The person's word on what was found, by measure, over what the references show (VF4b): kept through references
    /// added and read again, each with why
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Correction>>", optional)]
    pub corrections: Vec<Correction>,
}

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
        let back = serde_json::to_string(&v).unwrap();
        assert_eq!(back, r#"{"context":"street interviews","references":[{"target":"https://example.com/a","note":"A, by B"}]}"#);
        assert!(crate::read::<Videoform>("videoform", r#"{"context": "x", "refs": []}"#).is_err());
        let c = |say: &str| Correction { measure: "loudness".into(), say: say.into(), ..Default::default() };
        v.correct("loudness", Some(c("free")));
        v.correct("loudness", Some(c("ignore")));
        assert_eq!(v.corrections, vec![c("ignore")], "a measure has one correction, the latest");
        v.correct("loudness", None);
        assert!(v.corrections.is_empty());
    }
}
