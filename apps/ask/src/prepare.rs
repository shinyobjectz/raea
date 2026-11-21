//! Ask understands a question before it spends (docs/ask.feature AK2): what the file holds (ffprobe), what the project's
//! brief says it's for, whether anyone speaks in it, and from those which questions can be asked, which are swapped for
//! their variant (sound for sound-silent) and which are dropped, each with why. The state paragraph tells clef what the
//! piece is, as the app's critique writes it. When it isn't known whether anyone speaks, both variants are kept, the
//! voice question goes in the same call (runner.rs), free, and the variant that doesn't apply is set aside.

use std::path::{Path, PathBuf};

use raea_core_ask::{Facts, Question, library};
use raea_io_clef::media::{self, Probe};
use serde_json::{Value, json};

/// The question asked beside the rest when no one knows yet whether anyone speaks in the piece.
pub const VOICE: &str = "voice-heard";

/// What raea knows of the piece.
#[derive(Debug, Clone, Default)]
pub struct Piece {
    pub file: PathBuf,
    pub probe: Probe,
    /// the brief's fields, as `raea spec brief.ts --as brief` reads them ({} without a brief)
    pub brief: Value,
    /// the project's folder (the one holding the brief), when there is one
    pub project: Option<PathBuf>,
    /// the words heard, when a words file was given
    pub words: Option<String>,
    /// someone speaks: from --voice, or from the words file (none heard is no)
    pub voice: Option<bool>,
    /// what the caller adds to the state (what raea measured, the platform)
    pub extra: Option<String>,
}

/// The brief nearest the file: brief.ts in its folder or any folder above it.
pub fn brief_near(file: &Path) -> Option<PathBuf> {
    let abs = dunce::canonicalize(file).ok()?;
    abs.ancestors().skip(1).map(|d| d.join("brief.ts")).find(|b| b.is_file())
}

/// A brief's fields, through `raea spec FILE --as brief` (this raea), or {} when it can't be read.
fn brief_of(path: &Path) -> Value {
    let Ok(exe) = std::env::current_exe() else { return json!({}) };
    let Ok(out) = std::process::Command::new(exe).args(["spec"]).arg(path).args(["--as", "brief"]).output() else { return json!({}) };
    let text = String::from_utf8_lossy(&out.stdout);
    let v: Value = text.find('{').and_then(|i| serde_json::from_str(&text[i..]).ok()).unwrap_or_else(|| json!({}));
    v.get("brief").cloned().unwrap_or(v)
}

/// The words heard, from a words file (a JSON list of words, each a string or {word} or {text}): their text, and
/// whether anyone speaks (an empty list is no one).
pub fn words_of(path: &Path) -> Result<(String, bool), String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?).map_err(|e| format!("{}: {e}", path.display()))?;
    let list = v.as_array().or(v["words"].as_array()).ok_or_else(|| format!("{}: a list of words", path.display()))?;
    let words: Vec<&str> = list.iter().filter_map(|w| w.as_str().or(w["word"].as_str()).or(w["text"].as_str())).collect();
    Ok((words.join(" "), !words.is_empty()))
}

impl Piece {
    /// The piece as raea finds it: the file probed, its brief (given or the nearest), its words.
    pub fn read(file: &Path, brief: Option<&Path>, words: Option<&Path>, voice: Option<bool>, extra: Option<String>) -> Result<Piece, String> {
        let probe = media::probe(file)?;
        // "none": no brief, even with one near (a caller that writes the whole state itself, as the app's critique does)
        let brief_file = match brief {
            Some(p) if p.as_os_str() == "none" => None,
            Some(p) => Some(p.to_path_buf()),
            None => brief_near(file),
        };
        let (words, heard) = match words {
            Some(w) => words_of(w).map(|(t, h)| (Some(t).filter(|t| !t.is_empty()), Some(h)))?,
            None => (None, None),
        };
        Ok(Piece {
            file: file.to_path_buf(),
            probe,
            brief: brief_file.as_deref().map(brief_of).unwrap_or_else(|| json!({})),
            project: brief_file.and_then(|b| b.parent().map(Path::to_path_buf)),
            words,
            voice: voice.or(heard).or((!probe.sound).then_some(false)),
            extra,
        })
    }

    pub fn modes(&self) -> Vec<String> {
        let m = &self.brief["mode"];
        let list: Vec<&str> = m.as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_else(|| m.as_str().into_iter().collect());
        list.iter().flat_map(|s| s.split(',')).map(|s| s.replace("(assumed)", "").trim().to_string()).filter(|s| !s.is_empty()).collect()
    }

    pub fn facts(&self) -> Facts {
        let modes = self.modes();
        Facts { game: modes.iter().any(|m| m == "play"), modes, voice: self.voice, sound: self.probe.sound, video: self.probe.video }
    }

    /// What clef reads before the media: what the piece is and what's known of it.
    pub fn state(&self) -> String {
        let b = &self.brief;
        let modes = self.modes();
        let what = if modes.is_empty() { "video".to_string() } else { modes.join(" and ") };
        // with no brief and the caller's own state, the caller says what the piece is
        let briefless = b.as_object().is_none_or(|o| o.is_empty());
        let mut s = match (b["title"].as_str(), briefless && self.extra.is_some()) {
            (_, true) => vec![],
            (Some(t), _) => vec![format!("A {what} piece: \"{t}\".")],
            (None, _) => vec![format!("A {what} piece.")],
        };
        for (k, say) in [("audience", "It's for"), ("believe", "It should leave them believing"), ("logline", "Its storyboard"), ("voice", "Its voice")] {
            if let Some(v) = b[k].as_str().filter(|v| !v.trim().is_empty()) {
                s.push(format!("{say}: {}.", v.trim().trim_end_matches('.')));
            }
        }
        if self.voice == Some(false) {
            s.push("No one speaks in it: its sound is the music and the effects, with no voice.".into());
        }
        if let Some(w) = &self.words {
            let cut: String = w.chars().take(1200).collect();
            s.push(format!("The words heard: \"{cut}{}\"", if cut.len() < w.len() { "..." } else { "" }));
        }
        if self.probe.seconds > 0.0 {
            s.push(format!("It runs {:.0} s.", self.probe.seconds));
        }
        if let Some(x) = &self.extra {
            s.push(x.trim().to_string());
        }
        s.join(" ")
    }
}

/// Is anyone heard speaking: asked beside the questions that depend on it, at no cost (clef costs by media, not by
/// questions), when no one knows yet.
pub fn voice_question() -> Question {
    let mut v = Question::plain(VOICE, "Is anyone heard speaking in it (a voice saying words, not singing)?");
    v.sees = raea_core_ask::Sees::Nothing;
    v
}

/// The questions kept for this piece, and those dropped with why. A question that doesn't apply is swapped for its
/// variant when that one does; not knowing whether anyone speaks, both are kept.
pub fn keep(questions: Vec<Question>, f: &Facts, rubric_md: Option<&str>) -> (Vec<Question>, Vec<(String, String)>) {
    let mut asked: Vec<Question> = Vec::new();
    let mut dropped = Vec::new();
    let add = |q: Question, asked: &mut Vec<Question>| {
        if !asked.iter().any(|a| a.id == q.id) {
            asked.push(q);
        }
    };
    for q in questions {
        let Some(why) = q.not_for(f) else {
            // not knowing whether anyone speaks, its variant goes in too: one call answers either way
            let twin = (f.voice.is_none() && q.when.voice.is_some()).then(|| q.swap.as_deref().and_then(|s| library::find(s, rubric_md))).flatten();
            add(q, &mut asked);
            if let Some(t) = twin {
                add(t, &mut asked);
            }
            continue;
        };
        match q.swap.as_deref().and_then(|s| library::find(s, rubric_md)).filter(|s| s.not_for(f).is_none()) {
            Some(s) => {
                dropped.push((q.id.clone(), format!("{why}: asked {} instead", s.id)));
                add(s, &mut asked);
            }
            None => dropped.push((q.id.clone(), why)),
        }
    }
    (asked, dropped)
}
