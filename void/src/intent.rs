//! What a line typed or said means. A few commands run at once, here, without the model (the composer's slash
//! palette lists them); everything else goes to the brain.

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// Open the spaces menu.
    Spaces,
    Open(String),
    New(String),
    /// The voice panel, or straight to a voice by name.
    Voice(Option<String>),
    Listen,
    Mute,
    Undo(u32),
    /// Clear the transcript and forget the conversation.
    Clear,
    /// The composer hides itself until the pointer reaches the bottom edge (like the Dock), or stays.
    Dock(Option<bool>),
    Help,
    /// What void keeps cached, or clear one ("frames", "voices", "models").
    Cache(Option<String>),
    /// Play or pause the stage.
    Play(bool),
    /// Dock the terminal: bottom, top, left or right (none: the next one round).
    Layout(Option<String>),
    /// The skills and commands on the computer, listed.
    Skills,
    /// Anything else: the brain.
    Ask(String),
}

/// The palette: (command, what it does).
pub const COMMANDS: &[(&str, &str)] = &[
    ("/spaces", "Browse spaces"),
    ("/new", "New space · /new name"),
    ("/open", "Open a space · /open name"),
    ("/voice", "Voice settings · /voice alba"),
    ("/listen", "Start listening"),
    ("/mute", "Stop listening"),
    ("/undo", "Undo the last change · /undo 3"),
    ("/clear", "Clear the conversation"),
    ("/dock", "Hide the composer automatically · /dock off"),
    ("/layout", "Terminal at the bottom, left or right · /layout right"),
    ("/cache", "Caches and memory · /cache clear frames"),
    ("/skills", "Skills and your own commands · then /name"),
    ("/help", "What void can do"),
];

pub fn parse(line: &str) -> Option<Intent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let Some(rest) = line.strip_prefix('/') else {
        return Some(Intent::Ask(line.to_string()));
    };
    let (cmd, arg) = rest.split_once(char::is_whitespace).map(|(c, a)| (c, a.trim())).unwrap_or((rest, ""));
    let arg_opt = (!arg.is_empty()).then(|| arg.to_string());
    Some(match cmd.to_lowercase().as_str() {
        "spaces" | "space" | "s" => Intent::Spaces,
        "open" | "o" => match arg_opt {
            Some(a) => Intent::Open(a),
            None => Intent::Spaces,
        },
        "new" | "n" => Intent::New(arg_opt.unwrap_or_else(|| "Untitled".into())),
        "voice" | "v" => Intent::Voice(arg_opt),
        "listen" | "mic" => Intent::Listen,
        "mute" | "quiet" => Intent::Mute,
        "undo" | "u" => Intent::Undo(arg.parse().unwrap_or(1).max(1)),
        "clear" | "reset" => Intent::Clear,
        "dock" => Intent::Dock(match arg {
            "on" | "hide" | "auto" => Some(true),
            "off" | "show" | "keep" => Some(false),
            _ => None,
        }),
        "help" | "?" => Intent::Help,
        "skills" | "commands" => Intent::Skills,
        "play" => Intent::Play(true),
        "layout" | "dock-terminal" => Intent::Layout(match arg { "bottom" | "left" | "right" => Some(arg.to_string()), _ => None }),
        "pause" | "stop" => Intent::Play(false),
        "cache" => Intent::Cache(arg.strip_prefix("clear").map(|w| w.trim().to_string())),
        // an unknown command is a question for the brain, slash and all
        _ => Intent::Ask(line.to_string()),
    })
}

/// Palette entries matching what's typed so far: void's own commands, then the person's commands and the skills on the
/// computer (skills.rs), each as `/name`.
pub fn suggest(line: &str) -> Vec<(String, String)> {
    let Some(rest) = line.strip_prefix('/') else { return vec![] };
    if rest.contains(char::is_whitespace) {
        return vec![];
    }
    let r = rest.to_lowercase();
    let project = crate::workspace::current().map(|w| w.root);
    let mut out: Vec<(String, String)> = COMMANDS.iter().filter(|(c, _)| c[1..].starts_with(&r)).map(|(c, d)| (c.to_string(), d.to_string())).collect();
    out.extend(crate::skills::commands(project.as_deref()).into_iter().filter(|c| c.name.to_lowercase().starts_with(&r)).map(|c| (format!("/{}", c.name), c.description)));
    out.extend(crate::skills::skills(project.as_deref()).into_iter().filter(|s| s.name.to_lowercase().starts_with(&r)).map(|s| (format!("/{}", s.name), s.description.chars().take(80).collect())));
    out
}

/// Splits streaming reply text into clauses to speak: a clause ends at . ! ? ; : or a newline, or at a comma once it
/// has four words (the first words reach the voice quickly). Returns the finished clauses and keeps the rest in `buf`.
pub fn clauses(buf: &mut String) -> Vec<String> {
    let mut out = Vec::new();
    loop {
        let mut cut = None;
        for (i, c) in buf.char_indices() {
            let end = i + c.len_utf8();
            // a stop at the very end may be a decimal point or an abbreviation still streaming: wait for what follows
            let next_is_space = buf[end..].starts_with(char::is_whitespace);
            if matches!(c, '.' | '!' | '?' | ';' | ':') && next_is_space || c == '\n' {
                cut = Some(end);
                break;
            }
            if c == ',' && next_is_space && buf[..i].split_whitespace().count() >= 4 {
                cut = Some(end);
                break;
            }
        }
        let Some(end) = cut else { break };
        let clause: String = buf.drain(..end).collect();
        if !clause.trim().is_empty() {
            out.push(clause.trim().to_string());
        }
    }
    out
}

/// What was said, without its disfluencies: fillers (um, uh, er, hmm) and a word said twice or more in a row
/// ("a a a direct response ad" -> "a direct response ad"), so the transcript reads as meant.
pub fn clean_speech(text: &str) -> String {
    const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "er", "erm", "ah", "hmm", "mm", "mhm"];
    let bare = |w: &str| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase();
    let mut out: Vec<&str> = Vec::new();
    for w in text.split_whitespace() {
        let b = bare(w);
        if FILLERS.contains(&b.as_str()) {
            continue;
        }
        if out.last().is_some_and(|prev| bare(prev) == b && !b.is_empty()) {
            continue;
        }
        out.push(w);
    }
    let mut t = out.join(" ");
    // a filler dropped from before a comma leaves ", ," behind
    while t.contains(", ,") {
        t = t.replace(", ,", ",");
    }
    t.trim_start_matches([',', ' ']).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_and_questions() {
        assert_eq!(parse("/new Coffee ad"), Some(Intent::New("Coffee ad".into())));
        assert_eq!(parse("/open"), Some(Intent::Spaces));
        assert_eq!(parse("/undo 3"), Some(Intent::Undo(3)));
        assert_eq!(parse("/dock off"), Some(Intent::Dock(Some(false))));
        assert_eq!(parse("make the cube blue"), Some(Intent::Ask("make the cube blue".into())));
        assert_eq!(parse("/frobnicate"), Some(Intent::Ask("/frobnicate".into())));
        assert_eq!(parse("   "), None);
    }

    #[test]
    fn speech_without_disfluencies() {
        assert_eq!(clean_speech("like a a a direct response ad for um like uh adobe or something"), "like a direct response ad for like adobe or something");
        assert_eq!(clean_speech("Um, make it, uh, red"), "make it, red");
    }

    #[test]
    fn palette() {
        assert_eq!(suggest("/n").first().map(|s| s.0.as_str()), Some("/new"));
        assert!(suggest("/new x").is_empty());
        assert!(suggest("hello").is_empty());
    }

    #[test]
    fn clauses_split_as_they_stream() {
        let mut b = String::from("Done. I made the cube blue, and the title");
        assert_eq!(clauses(&mut b), vec!["Done.".to_string(), "I made the cube blue,".to_string()]);
        assert_eq!(b, " and the title");
        b.push_str(" bigger. ");
        assert_eq!(clauses(&mut b), vec!["and the title bigger.".to_string()]);
    }
}
