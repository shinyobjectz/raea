//! The taxonomy (FP2b): a fingerprint's axes, each with the words people use for it, the modality its head reads,
//! its scope, polarity and the check it maps to. It's written in fingerprint.ts (`fingerprint({...})` from
//! "raea/brief", sdk/raea/src/fingerprint.ts) and read here as the JSON its default export evaluates to. raea's base
//! taxonomy is drawn from Shane's notes (docs/fingerprint.md): glitch and continuity, sound, pace, camera, cut, light
//! and look, realism and detail, design and type. Creative directions are never axes.

use raea_sys_columns::window::Modality;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Axis {
    pub name: String,
    /// what people say for it, best first; the word head (FP2e) reads these
    pub words: Vec<String>,
    pub modality: Modality,
    /// moment, shot or whole
    #[serde(default = "moment")]
    pub scope: String,
    /// "bad": a high score is the defect (most axes); "good": a high score is the quality
    #[serde(default = "bad")]
    pub polarity: String,
    /// the `raea check` rule that measures it exactly, when one does; the check wins over the head (FP4c)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    /// false where words can't find it (silence matches no description: FP0g)
    #[serde(default = "yes")]
    pub word_head: bool,
    /// the group in the base taxonomy
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
}

fn moment() -> String {
    "moment".into()
}
fn bad() -> String {
    "bad".into()
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Fingerprint {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub who: Option<String>,
    /// one line: what it tracks and when it judges (docs/fingerprint.feature FP14a)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// the fingerprint it builds on, by name: it started from that one's axes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// which pieces it judges when a project or workspace holds more than one (FP14c)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applies: Option<Applies>,
    /// its flags asked again of the decision model as it scores (docs/ask.feature AK8): "flags", or "unsure" for the
    /// moments it's unsure of too
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    pub axes: Vec<Axis>,
    /// each encoder's id and its file's sha256 (FP2a): heads trained on other encoders' columns are refused
    #[serde(default)]
    pub encoders: std::collections::BTreeMap<String, String>,
    /// what makes it a videoform: its context, references, vocabulary, template and rules (docs/videoform.feature VF2)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub videoform: Option<raea_core_brief::Videoform>,
}

/// The kinds of piece (film, ad, game, playable, still, sound) and the project tags a fingerprint is for
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Applies {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// A comma-separated list of kinds or tags, each trimmed and lowercased, the empty ones dropped
pub fn words_of(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in s.split(',').map(str::trim) {
        if !w.is_empty() {
            out.push(w.to_ascii_lowercase());
        }
    }
    out
}

impl Fingerprint {
    pub fn axis(&self, name: &str) -> Option<&Axis> {
        let i = self.axes.iter().position(|a| a.name == name)?;
        self.axes.get(i)
    }
}

fn ax(group: &str, name: &str, m: Modality, words: &[&str], check: Option<&str>, word_head: bool) -> Axis {
    Axis {
        name: name.into(),
        words: words.iter().map(|w| w.to_string()).collect(),
        modality: m,
        scope: "moment".into(),
        polarity: "bad".into(),
        check: check.map(str::to_string),
        word_head,
        group: group.into(),
    }
}

/// raea's base taxonomy: the mechanical axes damage can bootstrap (FP4a), then taste axes that start from words
pub fn base(name: &str) -> Fingerprint {
    use Modality::*;
    let axes = vec![
        ax("light and look", "blur", Picture, &["a blurry, soft, low resolution picture", "an out of focus smeared image"], None, true),
        ax("light and look", "dark", Picture, &["a very dark, underexposed picture", "a murky image where nothing can be seen"], None, true),
        ax("light and look", "glow", Picture, &["a hazy glow over everything", "a washed out picture with heavy bloom"], None, true),
        ax("glitch and continuity", "flash", Change, &["a single black frame flashing"], Some("flash-frame"), false),
        ax("pace", "freeze", Change, &["a frozen picture where nothing moves"], Some("frozen"), false),
        ax("sound", "noise", Sound, &["hiss and static noise over everything", "a noisy recording full of white noise"], None, true),
        ax("sound", "dropout", Sound, &["the sound suddenly cutting out to silence"], Some("dropout"), false),
        ax("sound", "clip", Sound, &["harsh clipped distorted audio", "loud crackling digital distortion"], Some("clipping"), true),
        ax("sound", "muffled voice", Sound, &["a muffled, distant voice that is hard to understand"], None, true),
        ax("sound", "harsh music", Sound, &["harsh, shrill music that hurts to listen to"], None, true),
        ax("glitch and continuity", "glitch", Picture, &["a rendering glitch with broken, torn geometry", "corrupted video with blocky artifacts"], None, true),
        ax("realism and detail", "fake", Picture, &["a fake, plastic looking computer generated scene", "a cheap video game render"], None, true),
        ax("realism and detail", "barren", Picture, &["an empty, barren scene with nothing in it", "a bare grey room with no detail"], None, true),
        ax("design and type", "unreadable text", Picture, &["small, cluttered text that is hard to read"], None, true),
        ax("camera", "shaky camera", Change, &["a shaky handheld camera"], None, false),
    ];
    Fingerprint { name: name.into(), who: None, about: None, extends: None, applies: None, ask: None, axes, encoders: Default::default(), videoform: None }
}

/// The background vocabularies a word head is calibrated against (earshot's method: an axis's similarity as a z
/// against ordinary descriptions), sound and picture
pub const SOUND_VOCAB: [&str; 40] = [
    "a man talking", "a woman talking", "people chatting", "a crowd cheering", "calm electronic music", "a piano",
    "an acoustic guitar", "drums", "an orchestra", "a pop song", "hip hop beats", "ambient synth pads", "birds singing", "wind",
    "rain", "ocean waves", "a car passing", "footsteps", "a door closing", "typing on a keyboard", "a mouse click", "a phone ringing",
    "a dog barking", "water running", "a city street", "a quiet room", "an engine humming", "a whoosh", "a cinematic impact",
    "a bell", "laughter", "applause", "a fire crackling", "a sci-fi beep", "a swelling string section", "a bass drop",
    "a clean studio recording", "a voiceover", "a kitchen", "an airplane",
];

/// Ordinary passages of scripts, briefs and pages, the background a text axis's words are scored against (FP1b)
pub const TEXT_VOCAB: [&str; 24] = [
    "a line of dialogue", "a scene heading", "a description of what the camera sees", "a voiceover line",
    "a call to action", "a product's features", "a brand's tone of voice", "a target audience", "a deadline and deliverables",
    "a shot list", "a song's lyrics", "a joke", "a question to the viewer", "a price and an offer", "a list of steps",
    "a page's headline", "a navigation menu", "a paragraph of an article", "a quote from a customer", "legal small print",
    "a caption", "a title card", "an instruction to the editor", "a mood and references",
];

pub const PICTURE_VOCAB: [&str; 40] = [
    "a person talking to the camera", "a city street at night", "a forest", "an ocean", "a mountain landscape", "an office",
    "a kitchen", "a living room", "a car on a road", "a product on a table", "a phone screen", "a computer screen with an app",
    "a logo on a plain background", "text on a coloured background", "a crowd of people", "a close up of a face", "hands typing",
    "a dog", "a cat", "food on a plate", "a stage with lights", "a sunset sky", "clouds", "a cathedral interior", "a hallway",
    "a beach", "a desert", "snow", "a 3D animated character", "an illustration", "a chart", "a map", "a bedroom", "a garden",
    "a corridor with doors", "a painting", "a sports game", "a concert", "a train", "a bright clean studio shot",
];

/// The sentence a word or description is embedded as: CLAP was trained on captions of this form (perception H5)
pub fn prompt(m: Modality, words: &str) -> String {
    match m {
        Modality::Sound | Modality::Events => format!("This is a sound of {words}."),
        Modality::Text => words.to_string(),
        _ => format!("a photo of {words}"),
    }
}

/// fingerprint.ts for a fingerprint: what a person edits to add, remove or reword axes
pub fn to_ts(f: &Fingerprint) -> String {
    let body = serde_json::to_string_pretty(f).unwrap_or_default();
    format!("// A fingerprint: your sense of good work, as axes (docs/fingerprint.md). Add an axis, reword one, or remove one;\n// the next `raea fingerprint` command embeds the new words and retrains. `raea fingerprint report` says what each head has learned and what it can't judge.\nimport {{ fingerprint }} from \"raea/brief\";\n\nexport default fingerprint({body});\n")
}
