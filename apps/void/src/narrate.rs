//! The magic terminal's narrator: the agent's tool calls as short plain lines, the way a terminal reports, but for
//! someone who doesn't read code. Procedural on purpose: each line comes from what actually happened (the command the
//! shell ran, `raea`'s own answer, a REFUSED or failed tool), so it's instant and never invents. A shell command is
//! named for what it does to the project (read, wrote, edited, planned), never echoed as code (the founder, 2026-10-17:
//! "these terminal logs ... look awful"); what a command printed stays out of the log, but for `raea check`'s faults.

/// A step as the terminal shows it: (mark, text). Marks: ✓ done, ✗ refused or failed, ⚠ warned, ◉ looked, · other.
pub fn step(tool: &str, what: &str, ok: bool, out: &str) -> (char, String) {
    let first = out.lines().next().unwrap_or("").trim();
    if !ok || first.starts_with("REFUSED") {
        return ('✗', format!("failed, {}: {}", if what.trim().is_empty() && tool == "shell" { "an empty command".to_string() } else { phrase(tool, what, false) }, reason(out)));
    }
    match tool {
        "shell" => match raea(what) {
            // `raea check`: its verdict is the last word on the piece
            Some("check") if out.contains("PASS  no faults") => ('✓', format!("checked: no faults{}", saved(out))),
            Some("check") => match faults(out) {
                0 => ('⚠', "checked: the render failed".into()),
                n => ('⚠', format!("checked: {n} fault{}{}", if n == 1 { "" } else { "s" }, saved(out))),
            },
            Some("render") if out.contains("the render failed") => ('⚠', "the render failed".into()),
            Some("render") => ('✓', format!("rendered {}{}", out.lines().next().and_then(|l| l.split_once(": ")).map(|(_, r)| r.split(" (").next().unwrap_or(r).to_string()).unwrap_or_default(), saved(out))),
            Some("task") if what.contains(" ready") && out.contains("isn't ready") => ('⚠', "not ready yet: the checks say what's missing".into()),
            _ => ('✓', phrase(tool, what, true)),
        },
        "look" => ('◉', format!("looked at {}", short_list(what))),
        "list_objects" | "media" | "craft" | "guide" | "search_assets" => ('·', phrase(tool, what, true)),
        "review" => ('✓', format!("reviewed · {}", first.chars().take(60).collect::<String>())),
        _ => ('✓', phrase(tool, what, true)),
    }
}

/// ", saved v3": the version a render or a check kept (host.rs keep_version's "saved v3 of ..." line).
fn saved(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("saved v").or_else(|| l.strip_prefix("the same as v")).and_then(|r| r.split_whitespace().next()).map(|n| format!(" · v{n}"))).unwrap_or_default()
}

/// A process's verb, the column the terminal names it in: what the step does to the project.
pub fn verb(tool: &str, what: &str) -> &'static str {
    match tool {
        "shell" => match raea(what) {
            Some("render") => "render",
            Some("check") => "check",
            Some("craft") => "study",
            Some("task" | "tasks" | "status" | "purpose") => "plan",
            Some("ask" | "answer" | "question") => "ask",
            Some("save" | "versions" | "diff" | "restore" | "checkout") => "version",
            Some("note" | "notes") => "note",
            Some("cut" | "media" | "frame") => "footage",
            Some(_) => "raea",
            None => match shell_kind(what).0 {
                Kind::Read => "read",
                Kind::Write => "write",
                Kind::Edit => "edit",
                Kind::List => "look",
                Kind::Run => "run",
            },
        },
        "browser" => "browse",
        "look" => "look",
        "media" => "footage",
        "search_assets" => "search",
        "review" => "review",
        "brand" => "brand",
        "craft" | "guide" => "study",
        "list_objects" => "objects",
        "open_object" => "open",
        "new_object" => "new",
        _ => "run",
    }
}

/// The lines kept under a finished process: the faults `raea check` names (four at most). Nothing else a command
/// prints goes into the log.
pub fn detail(tool: &str, what: &str, out: &str) -> Vec<String> {
    if tool != "shell" || raea(what) != Some("check") {
        return vec![];
    }
    let faults: Vec<&str> = out.lines().filter(|l| l.starts_with("- ")).collect();
    let mut d: Vec<String> = faults.iter().take(4).map(|l| format!("✗ {}", l.trim_start_matches("- ").chars().take(160).collect::<String>())).collect();
    if faults.len() > 4 {
        d.push(format!("· and {} more", faults.len() - 4));
    }
    d
}

/// The line shown while a tool runs.
pub fn doing(tool: &str, what: &str) -> String {
    phrase(tool, what, false)
}

/// What a step does, in words: running ("reading edit.tsx") or done ("read edit.tsx").
fn phrase(tool: &str, what: &str, done: bool) -> String {
    let p = |running: &str, past: &str| if done { past.to_string() } else { running.to_string() };
    match tool {
        "shell" => match raea(what) {
            Some(sub) => raea_phrase(sub, what, done),
            None => {
                let (kind, files) = shell_kind(what);
                let f = if files.is_empty() { String::new() } else { format!(" {}", files.join(", ")) };
                match kind {
                    Kind::Read => p(&format!("reading{}", if f.is_empty() { " files".into() } else { f.clone() }), &format!("read{}", if f.is_empty() { " files".into() } else { f })),
                    Kind::Write => p(&format!("writing{f}"), &format!("wrote{f}")),
                    Kind::Edit => p(&format!("editing{f}"), &format!("edited{f}")),
                    Kind::List => p("looking through the files", "looked through the files"),
                    Kind::Run => {
                        let cmd = first_command(what);
                        p(&format!("running {cmd}"), &format!("ran {cmd}"))
                    }
                }
            }
        },
        "browser" => p(&format!("browsing: {what}"), &format!("browsed: {what}")),
        "look" => p(&format!("looking at {}", short_list(what)), &format!("looked at {}", short_list(what))),
        "guide" => p(&format!("reading the {what} guide"), &format!("read the {what} guide")),
        "craft" => p(&format!("reading {what}"), &format!("read {what}")),
        "media" => p("looking through the footage", "looked through the footage"),
        "search_assets" => p(&format!("searching the files for {what}"), &format!("searched the files for {what}")),
        "review" => p("watching it like a creative director", "reviewed it"),
        "brand" => p(&format!("fetching {what}'s brand kit"), &format!("fetched {what}'s brand kit")),
        "list_objects" => p("listing the objects", "listed the objects"),
        "open_object" => p(&format!("opening {what}"), &format!("opened {what}")),
        "new_object" => p(&format!("making {what}"), &format!("new object · {what}")),
        other => other.replace('_', " "),
    }
}

/// A `raea` command in words.
fn raea_phrase(sub: &str, what: &str, done: bool) -> String {
    let p = |running: String, past: String| if done { past } else { running };
    let q = quoted(what);
    match sub {
        "init" => p("laying out the project".into(), "laid out the project".into()),
        "help" => p("reading raea's commands".into(), "read raea's commands".into()),
        "media" => p("looking through the footage".into(), "looked through the footage".into()),
        "cut" => p("cutting on the words".into(), "cut on the words".into()),
        "frame" => p("finding the face in a take".into(), "found the face in a take".into()),
        "add" => p("adding a component".into(), "added a component".into()),
        "list" => p("listing the components".into(), "listed the components".into()),
        "brand" => p("fetching the brand kit".into(), "fetched the brand kit".into()),
        "render" => p("rendering".into(), "rendered".into()),
        "check" => p("rendering and checking".into(), "checked".into()),
        "still" | "sheet" => p("pulling stills".into(), "pulled stills".into()),
        "script" => p("compiling the brief".into(), "compiled the brief".into()),
        "craft" => {
            // every `raea craft NAME` in the line (`raea craft a; echo; raea craft b`)
            let names: Vec<String> = what.match_indices("raea craft").flat_map(|(i, _)| args_of(&what[i..], "craft").into_iter().take(1)).collect();
            let n = if names.is_empty() { "the craft library".to_string() } else { names.join(", ") };
            p(format!("studying {n}"), format!("studied {n}"))
        }
        "status" => p("reading the project's record".into(), "read the project's record".into()),
        "purpose" => p("writing what the project is for".into(), format!("purpose: {}", q.unwrap_or_default())),
        "tasks" => p("reading the tasks".into(), "read the tasks".into()),
        "task" => match args_of(what, "task").first().map(String::as_str) {
            Some("new") => p("planning the task".into(), format!("task: {}", q.unwrap_or_default())),
            Some("step") if what.contains("--done") => p("ticking off a step".into(), format!("done: {}", q.unwrap_or_default())),
            Some("step") => p("planning a step".into(), format!("step: {}", q.unwrap_or_default())),
            Some("state") => {
                let st = args_of(what, "task").get(2).cloned().unwrap_or_default();
                p(format!("marking it {st}"), format!("marked {st}"))
            }
            _ => p("reading the task".into(), "read the task".into()),
        },
        "ask" => p("asking you".into(), format!("asked: {}", q.unwrap_or_default())),
        "question" => p("reading the question".into(), "read the question".into()),
        "answer" => p("taking your answer".into(), "took your answer".into()),
        "note" => p("noting it".into(), format!("noted: {}", q.unwrap_or_default())),
        "notes" => p("reading the notes".into(), "read the notes".into()),
        "objects" => p("listing the objects".into(), "listed the objects".into()),
        "object" => {
            let a = args_of(what, "object");
            match a.first().map(String::as_str) {
                Some("kind") => p("saying what it is".into(), format!("{} is a {}", a.get(1).cloned().unwrap_or_default(), a.get(2).cloned().unwrap_or_default())),
                _ => p("opening an object".into(), format!("opened {}", a.get(1).cloned().unwrap_or_default())),
            }
        }
        "save" => p("saving a version".into(), "saved a version".into()),
        "versions" => p("listing the versions".into(), "listed the versions".into()),
        "diff" => p("comparing versions".into(), "compared versions".into()),
        "restore" => {
            let n = args_of(what, "restore").first().cloned().unwrap_or_default();
            p(format!("going back to v{}", n.trim_start_matches('v')), format!("back to v{}", n.trim_start_matches('v')))
        }
        "checkout" => p("opening an earlier version".into(), "opened an earlier version".into()),
        other => p(format!("raea {other}"), format!("raea {other}")),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Read,
    Write,
    Edit,
    List,
    Run,
}

/// What a shell command (not `raea`) does, from its commands: writing a file (a heredoc or a redirect), editing one
/// (sed -i), reading files (cat, head, sed -n, grep ...), listing (ls, find), else running something; and the files.
fn shell_kind(cmd: &str) -> (Kind, Vec<String>) {
    let mut kind = Kind::Run;
    let mut files: Vec<String> = Vec::new();
    let file_like = |w: &str| (w.contains('.') || w.contains('/')) && !w.starts_with('-') && !w.contains('*') && !w.starts_with('\'') && !w.starts_with('"') && w.len() < 60;
    let name = |w: &str| w.trim_matches(|c| c == '"' || c == '\'').rsplit('/').next().unwrap_or(w).to_string();
    for part in cmd.split(|c| c == ';' || c == '|' || c == '\n').flat_map(|p| p.split("&&")) {
        let words: Vec<&str> = part.split_whitespace().collect();
        let Some(&first) = words.first() else { continue };
        // a redirect into a file writes it (`cat > edit.tsx <<'EOF'`, `echo x > a.ts`)
        if let Some(i) = words.iter().position(|w| *w == ">" || *w == ">>") {
            if let Some(f) = words.get(i + 1).filter(|f| file_like(f) && !f.starts_with("/dev")) {
                kind = Kind::Write;
                files.push(name(f));
                continue;
            }
        }
        if let Some(f) = words.iter().find_map(|w| w.strip_prefix('>').filter(|f| file_like(f) && !f.starts_with("/dev"))) {
            kind = Kind::Write;
            files.push(name(f));
            continue;
        }
        let args: Vec<String> = words[1..].iter().filter(|w| file_like(w)).map(|w| name(w)).collect();
        match first {
            "sed" if words.contains(&"-i") || words.iter().any(|w| w.starts_with("-i")) => {
                if kind != Kind::Write {
                    kind = Kind::Edit;
                }
                files.extend(args);
            }
            "cat" | "head" | "tail" | "sed" | "grep" | "wc" | "jq" | "less" | "awk" => {
                if kind == Kind::Run || kind == Kind::List {
                    kind = Kind::Read;
                }
                files.extend(args);
            }
            "ls" | "find" | "tree" | "pwd" => {
                if kind == Kind::Run {
                    kind = Kind::List;
                }
            }
            "echo" | "cd" | "printf" | "true" => {}
            _ => {
                if kind != Kind::Run || !files.is_empty() {
                    continue;
                }
                return (Kind::Run, Vec::new());
            }
        }
    }
    files.dedup();
    let mut seen = std::collections::HashSet::new();
    files.retain(|f| seen.insert(f.clone()));
    files.truncate(4);
    (kind, files)
}

/// The first command of a line, short: what's run, without its quoting.
fn first_command(cmd: &str) -> String {
    let c = cmd.split(|c| c == ';' || c == '|' || c == '\n').flat_map(|p| p.split("&&")).map(str::trim).find(|p| !p.is_empty() && !p.starts_with("cd ")).unwrap_or(cmd).trim();
    let short: String = c.chars().take(48).collect();
    if c.chars().count() > 48 { format!("{short}…") } else { short }
}

/// The words after `raea SUB` in a command, unquoted, flags left out.
fn args_of(cmd: &str, sub: &str) -> Vec<String> {
    let Some(at) = cmd.find(&format!("raea {sub}")) else { return vec![] };
    let rest = &cmd[at + 5 + sub.len()..];
    let rest = rest.split(|c| c == ';' || c == '|' || c == '\n').next().unwrap_or("").split("&&").next().unwrap_or("");
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for ch in rest.chars() {
        match (quote, ch) {
            (None, '"' | '\'') => quote = Some(ch),
            (Some(q), c) if c == q => quote = None,
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (_, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    // a flag and its value are left out (--kind "new object")
    let mut keep = Vec::new();
    let mut skip = false;
    for w in out {
        if skip {
            skip = false;
            continue;
        }
        if w.starts_with("--") {
            skip = !matches!(w.as_str(), "--done" | "--multi" | "--full" | "--paths" | "--json" | "--force" | "--append");
            continue;
        }
        keep.push(w);
    }
    keep
}

/// The first quoted text in a command (a task's words, a question, a note), short.
fn quoted(cmd: &str) -> Option<String> {
    let (q, rest) = cmd.char_indices().find(|(_, c)| *c == '"' || *c == '\'').map(|(i, c)| (c, &cmd[i + 1..]))?;
    let t = rest.split(q).next()?;
    let short: String = t.chars().take(90).collect();
    Some(if t.chars().count() > 90 { format!("{short}…") } else { short })
}

/// The `raea` subcommand a shell command runs, if it runs one (`cd x && raea cut take "..."` is "cut").
fn raea(cmd: &str) -> Option<&str> {
    let mut words = cmd.split(|c: char| c.is_whitespace() || c == ';' || c == '&' || c == '|').filter(|w| !w.is_empty());
    words.position(|w| w == "raea")?;
    words.next().or(Some("help"))
}

/// The faults `raea check` lists ("N faults:" then "- ..." lines).
fn faults(out: &str) -> usize {
    out.lines().find_map(|l| l.strip_suffix(" faults:").and_then(|n| n.trim().parse().ok())).unwrap_or_else(|| out.lines().filter(|l| l.starts_with("- ")).count())
}

/// A failure's reason: its first line with something in it, past a REFUSED.
fn reason(out: &str) -> String {
    let first = out.lines().next().unwrap_or("").trim();
    let after = first.split_once("  ").map(|(_, r)| r.trim()).filter(|r| r.len() > 12);
    let line = after.or_else(|| out.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("REFUSED") && !l.starts_with("(exit"))).unwrap_or("");
    line.chars().take(80).collect()
}

/// "a.png, b/c.png" as their file names.
fn short_list(what: &str) -> String {
    let names: Vec<&str> = what.split(',').map(|p| p.trim().rsplit('/').next().unwrap_or("")).filter(|p| !p.is_empty()).collect();
    if names.is_empty() { "the stills".into() } else { names.join(", ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        assert_eq!(step("shell", "raea cut take01 \"hi\"", true, "cuts.ts: 1 clips"), ('✓', "cut on the words".into()));
        assert_eq!(step("shell", "cd . && raea check", true, "final.mp4: 3.0 s\nPASS  no faults found\nsaved v3 of ad \"x\""), ('✓', "checked: no faults · v3".into()));
        assert_eq!(step("shell", "raea check", true, "final.mp4: 3.0 s\n2 faults:\n- a\n- b"), ('⚠', "checked: 2 faults".into()));
        assert_eq!(detail("shell", "raea check", "2 faults:\n- black at 0:01\n- text off the frame"), vec!["✗ black at 0:01", "✗ text off the frame"]);
        assert_eq!(step("shell", "ls media", false, "no such file").0, '✗');
        assert_eq!(step("look", ".stills/3.50.png, .sheets/take.png", true, "showing"), ('◉', "looked at 3.50.png, take.png".into()));
        assert_eq!(step("brand", "arock.com", true, "Arock"), ('✓', "fetched arock.com's brand kit".into()));
    }

    #[test]
    fn a_craft_file_is_not_a_list_of_faults() {
        // the screenshot of 2026-10-17: a craft file's bullets shown as ✗ lines under "run raea craft"
        let out = "# Mode: game\n- **The world leads; the interface recedes.** The play space is the picture.\n- **Feedback within a few frames.**";
        assert!(detail("shell", "raea craft modes/game", out).is_empty());
        assert_eq!(step("shell", "raea craft modes/game; raea craft motion/games", true, out), ('✓', "studied modes/game, motion/games".into()));
        assert_eq!(verb("shell", "raea craft modes/game"), "study");
    }

    #[test]
    fn shell_commands_in_words() {
        assert_eq!(step("shell", "ls -la; echo ---; cat AGENTS.md", true, "..."), ('✓', "read AGENTS.md".into()));
        assert_eq!(step("shell", "cat > edit.tsx <<'EOF'", true, ""), ('✓', "wrote edit.tsx".into()));
        assert_eq!(step("shell", "sed -i 's/a/b/' components/Paddle.tsx", true, ""), ('✓', "edited Paddle.tsx".into()));
        assert_eq!(step("shell", "ls components", true, ""), ('✓', "looked through the files".into()));
        assert_eq!(step("shell", "js-exec sim.ts", true, ""), ('✓', "ran js-exec sim.ts".into()));
        assert_eq!(verb("shell", "cat > edit.tsx <<'EOF'"), "write");
        assert_eq!(step("shell", "raea task new \"create pong\" --kind \"new object\"", true, "t1"), ('✓', "task: create pong".into()));
        assert_eq!(step("shell", "raea task step t1 \"paddles\" --done", true, ""), ('✓', "done: paddles".into()));
        assert_eq!(step("shell", "raea object kind pong game", true, ""), ('✓', "pong is a game".into()));
        assert_eq!(step("shell", "raea restore 2", true, ""), ('✓', "back to v2".into()));
    }
}
