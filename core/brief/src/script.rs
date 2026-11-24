//! A script: storyboards and screenplays are one thing at different detail (Shane, 10-27). A walkthrough's board is
//! one beat a scene with no headings; a film's screenplay fills headings, characters and dialogue. The board table and
//! the screenplay reading are views of the same value.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Script {
    pub title: String,
    pub logline: Option<String>,
    pub scenes: Vec<Scene>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Scene {
    /// where and when ("INT. SENATE - NIGHT", or "The brief page")
    pub heading: Option<String>,
    pub beats: Vec<Beat>,
    /// what the scene must land
    pub payoff: Option<String>,
    /// the story beat it plays, by its id (raea/story's compileScript writes it; docs/story.feature ST0c)
    pub beat: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Beat {
    /// what's seen (a screenplay's action)
    pub on_screen: String,
    /// the move ("push in on the knife", "orbit left, keeps moving through the cut")
    pub camera: Option<String>,
    /// how this beat is cut into ("match on motion", "hard cut")
    pub cut: Option<String>,
    #[serde(default)]
    #[ts(as = "Option<Vec<Line>>", optional)]
    pub lines: Vec<Line>,
    /// sound and music cues ("strings drop out", "the theme's second hit")
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub cues: Vec<String>,
    pub seconds: Option<f64>,
    /// the word the beat cuts on, so a new take keeps the cut in sync with the transcript
    pub on_word: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Line {
    /// a character, or "narrator"
    pub who: String,
    pub says: String,
    /// a screenplay's parenthetical ("quietly")
    pub how: Option<String>,
}

impl Script {
    /// Everything said, in order: what narration, captions and cue timing follow.
    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.scenes.iter().flat_map(|s| s.beats.iter()).flat_map(|b| b.lines.iter())
    }
}
