//! The magic terminal's router: what a line typed (or said) is, before anything runs. Today fast rules; the same
//! `route` is where a local small model (FunctionGemma 270M on ort, or nearest examples in EmbeddingGemma) goes,
//! measured against these rules on the same lines.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Opening, playing, undoing: runs at once, no model (intent.rs).
    Command,
    /// One change to what's there ("make the title red"): one move, then one sentence.
    Quick,
    /// About how it looks ("does the caption cover her face?", "make it look better"): look at frames first.
    LookFirst,
    /// A question about the edit: answered from schema, values and frames; nothing changes.
    Question,
    /// Anything bigger: the agent's whole loop.
    Agent,
}

impl Route {
    /// The terminal's route line.
    pub fn label(self) -> &'static str {
        match self {
            Route::Command => "command",
            Route::Quick => "quick edit",
            Route::LookFirst => "looking first",
            Route::Question => "question",
            Route::Agent => "asking the agent",
        }
    }

    /// What the brain is told about the route, before the person's words.
    pub fn hint(self) -> &'static str {
        match self {
            Route::Command | Route::Agent => "",
            Route::Quick => "[quick edit: make exactly one apply move for this, check it if it moves anything, then reply in one short sentence]\n",
            Route::LookFirst => "[look first: call look on the frames this is about before deciding anything]\n",
            Route::Question => "[question: answer from schema, at and look; don't change the edit]\n",
        }
    }
}

const LOOKS: &[&str] = &[
    "look", "looks", "see", "visible", "cover", "covers", "covering", "readable", "legible", "ugly", "better", "pretty",
    "clean", "cluttered", "busy", "overlap", "overlaps", "face", "on screen", "cut off", "crop", "framing",
];
const TUNES: &[&str] = &[
    "bigger", "smaller", "larger", "louder", "quieter", "faster", "slower", "longer", "shorter", "higher", "lower",
    "red", "blue", "green", "yellow", "white", "black", "bold", "thinner", "darker", "lighter", "brighter", "up", "down",
    "left", "right", "earlier", "later", "center", "centre",
];
const STARTS_TUNE: &[&str] = &["make", "set", "change", "move", "turn", "nudge", "shift", "raise", "drop", "speed", "slow", "color", "colour", "rename", "delete", "remove", "hide", "show"];
const QUESTION_WORDS: &[&str] = &["what", "how", "is", "are", "does", "do", "when", "where", "which", "why", "who", "whats", "what's", "how's"];
const BIG: &[&str] = &[
    "video", "explainer", "edit", "turn this", "into", "create", "build", "make a", "make me", "new", "import", "clip",
    "footage", "music", "soundtrack", "captions", "voiceover", "script", "storyboard", "scene", "app", "game", "document",
];

pub fn route(line: &str) -> Route {
    let l = line.trim().to_lowercase();
    // a slash command is one /word; a pasted path (/Users/...) is not
    if l.starts_with('/') && !l.split_whitespace().next().unwrap_or("")[1..].contains('/') {
        return Route::Command;
    }
    let words: Vec<&str> = l.split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|w| !w.is_empty()).collect();
    let first = words.first().copied().unwrap_or("");
    if words.len() <= 3 && matches!(first, "open" | "play" | "pause" | "stop" | "undo" | "listen" | "mute") {
        return Route::Command;
    }
    let has = |set: &[&str]| set.iter().any(|w| if w.contains(' ') { l.contains(w) } else { words.contains(w) });
    // a path or a file is always the agent's (importing)
    if l.contains('/') || l.contains(".mov") || l.contains(".mp4") {
        return Route::Agent;
    }
    let question = l.ends_with('?') || QUESTION_WORDS.contains(&first);
    if question && !l.starts_with("can you") && !l.starts_with("could you") {
        return if has(LOOKS) { Route::LookFirst } else { Route::Question };
    }
    if has(LOOKS) && !has(BIG) {
        return Route::LookFirst;
    }
    if words.len() <= 12 && (STARTS_TUNE.contains(&first) || has(TUNES)) && !has(BIG) {
        return Route::Quick;
    }
    Route::Agent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes() {
        assert_eq!(route("/open hook"), Route::Command);
        assert_eq!(route("play"), Route::Command);
        assert_eq!(route("make the title red"), Route::Quick);
        assert_eq!(route("move the caption up a bit"), Route::Quick);
        assert_eq!(route("does the caption cover her face?"), Route::LookFirst);
        assert_eq!(route("make it look cleaner"), Route::LookFirst);
        assert_eq!(route("how long is it?"), Route::Question);
        assert_eq!(route("turn this clip into an animated explainer with motion graphics"), Route::Agent);
        assert_eq!(route("/Users/me/Downloads/take.mov make an explainer"), Route::Agent);
        assert_eq!(route("use ~/Downloads/take.mov for an explainer"), Route::Agent);
        assert_eq!(route("can you make me a music bed under the voice"), Route::Agent);
    }
}
