//! `raea hook [AGENT]` (docs/agents.feature A5d, A5e; docs/skills.feature SK10): one runner for every agent harness's
//! tool hooks, whichever harness runs the agent (Claude Code, Codex, Gemini CLI, Kimi Code; `raea skills install
//! --agent A` adds them, and they hold inside the raea app too, which leaves each agent its own shell). Before a shell
//! command: media opened in the Mac's viewer or a browser is stopped once while the raea app is installed, the reason
//! naming `raea show`; the same command tried again goes through, so an agent that means it says so twice. After a
//! file is saved: the quick check on a composition. After any tool, and as a turn ends: what the person sent from the raea
//! app (inbox.rs). A stop is exit 2 with the reason on stderr, which every harness
//! hands its agent; the quick check's findings go back in the shape each one reads.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

type R<T> = Result<T, String>;

/// What a still, a video or a sound ends with: what the raea app shows as an item.
const MEDIA: [&str; 25] = ["png", "jpg", "jpeg", "gif", "webp", "heic", "heif", "tif", "tiff", "bmp", "avif", "exr", "mp4", "mov", "m4v", "webm", "mkv", "avi", "wav", "mp3", "m4a", "aac", "flac", "aif", "aiff"];
/// A stopped command tried again within this many seconds goes through.
const AGAIN: u64 = 600;

/// The harness that sent this input, when the hook's command doesn't name it: Gemini CLI names its events BeforeTool and
/// AfterTool, Kimi Code says its client; Claude Code and Codex share one shape.
fn agent_of(v: &Value) -> &'static str {
    match (v["hook_event_name"].as_str().unwrap_or(""), v["client_type"].as_str().unwrap_or("")) {
        ("BeforeTool" | "AfterTool", _) => "gemini",
        (_, c) if c.contains("kimi") => "kimi",
        _ => "claude",
    }
}

/// The shell command a tool call runs, if it's a shell tool's (Bash, shell, shell_command, exec_command,
/// run_shell_command): Codex's apply_patch carries its patch in `command` too, and isn't one.
fn shell_command(v: &Value) -> Option<&str> {
    let cmd = v["tool_input"]["command"].as_str()?;
    (v["tool_name"].as_str() != Some("apply_patch") && !cmd.trim_start().starts_with("*** Begin Patch")).then_some(cmd)
}

/// A shell segment's words, quotes taken off (enough for `open "a b.png"`, not a shell).
fn words(seg: &str) -> Vec<String> {
    let (mut out, mut cur, mut quote, mut any) = (Vec::new(), String::new(), None, false);
    for c in seg.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => (quote, any) = (Some(c), true),
            (None, c) if c.is_whitespace() => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            (None, c) => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The media files a command opens in the Mac's viewer or a browser (`open`, `open -a Preview`, `xdg-open`,
/// `qlmanage -p`); `open -a raea`, `open -R` (Finder), links and other files aren't.
pub fn opened_media(cmd: &str) -> Vec<String> {
    let mut found = Vec::new();
    for seg in cmd.split(['\n', ';', '|', '&']) {
        let w = words(seg);
        let mut it = w.iter().skip_while(|t| t.contains('=') && !t.starts_with('-') || ["command", "exec", "nohup", "env"].contains(&t.as_str()));
        let Some(tool) = it.next() else { continue };
        if !["open", "xdg-open", "qlmanage"].contains(&tool.rsplit_once('/').map_or(tool.as_str(), |(_, name)| name)) {
            continue;
        }
        // the files it names, apart from the app an -a or -b names; one naming raea, or Finder's -R, opens nothing here
        let (mut files, mut app, mut ours) = (Vec::new(), false, false);
        for t in it {
            if app {
                ours |= t.to_lowercase().contains("raea");
                app = false;
            } else if t == "-a" || t == "-b" {
                app = true;
            } else if t == "-R" {
                ours = true;
            } else if !t.starts_with('-') && !t.contains("://") && t.rsplit_once('.').is_some_and(|(_, e)| MEDIA.contains(&e.to_lowercase().as_str())) {
                files.push(t.clone());
            }
        }
        if !ours {
            found.extend(files);
        }
    }
    found
}

/// The raea app on this Mac (RAEA_APP, /Applications or ~/Applications).
fn app_installed(home: &str) -> bool {
    [std::env::var("RAEA_APP").unwrap_or_default(), "/Applications/raea.app".into(), format!("{home}/Applications/raea.app")].iter().any(|p| !p.is_empty() && std::path::Path::new(p).exists())
}

/// True when this command was stopped for this session within AGAIN seconds (the agent's second yes, which clears it);
/// else it's noted and false.
fn tried_before(dir: &Path, key: &str) -> bool {
    let h = key.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
    let at = dir.join(format!("{h:016x}"));
    let fresh = std::fs::metadata(&at).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|e| e.as_secs() < AGAIN);
    if fresh {
        let _ = std::fs::remove_file(&at);
        return true;
    }
    let _ = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&at, key));
    false
}

/// Why the command stops, for the agent: what to run instead, and that the same command again goes through.
pub fn steer(files: &[String]) -> String {
    let list = files.iter().map(|f| if f.contains(' ') { format!("{f:?}") } else { f.clone() }).collect::<Vec<_>>().join(" ");
    format!("raea: the raea app is installed here, so show this in it, not the Mac's viewer: `raea show {list}` (several stills make a storyboard; add --ask \"what you want from the person\"; inside the raea app, feed_post does the same). It brings the app's window up on it as an item the person can answer, and their answer comes to the inbox file it prints. If the Mac's viewer really is what's wanted (the person asked for it, or raea show can't), run the same command again and it goes through.")
}

/// The files a tool call saved: `file_path` or `path` (Claude Code, Kimi Code, Gemini CLI), or the files an apply_patch
/// adds or updates (Codex).
fn saved(v: &Value) -> Vec<String> {
    let i = &v["tool_input"];
    if let Some(p) = i["file_path"].as_str().or(i["path"].as_str()).or(i["absolute_path"].as_str()) {
        return vec![p.to_string()];
    }
    let patch = i["command"].as_str().or(i["input"].as_str()).unwrap_or("");
    patch.lines().filter_map(|l| ["*** Add File: ", "*** Update File: "].iter().find_map(|head| l.strip_prefix(head))).map(str::trim).map(String::from).collect()
}

/// Findings for the agent in the shape its harness reads after a tool: Gemini CLI's AfterTool context, Kimi Code's
/// plain stdout, Claude Code's and Codex's PostToolUse context.
fn after(agent: &str, text: &str) -> String {
    match agent {
        "kimi" => text.to_string(),
        "gemini" => json!({"hookSpecificOutput": {"hookEventName": "AfterTool", "additionalContext": text}}).to_string(),
        _ => json!({"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": text}}).to_string(),
    }
}

/// `raea hook [claude|codex|gemini|kimi]`: the harness's hook input on stdin. Prints what goes back to the agent, or
/// stops the call (exit 2, the reason on stderr).
pub fn hook(args: &[String]) -> R<String> {
    let named = args.first().map(String::as_str);
    if named.is_some_and(|a| !["claude", "codex", "gemini", "kimi"].contains(&a)) {
        return Err("usage: raea hook [claude|codex|gemini|kimi]   (an agent's tool hooks; raea skills install --agent A adds them)".into());
    }
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&input).unwrap_or_default();
    let agent = named.unwrap_or_else(|| agent_of(&v));
    let event = v["hook_event_name"].as_str().unwrap_or("");
    // what the person sent from the raea app since the last look (inbox.rs): handed over after any tool, and a turn about
    // to end goes on to take it up
    let sent = if matches!(event, "PostToolUse" | "AfterTool" | "Stop") { crate::inbox::told(agent, &v) } else { None };
    if event == "Stop" {
        // what the person sent, and a `make` job that isn't done (docs/make.feature MK2): either keeps the turn going
        let reasons: Vec<String> = [sent, raea_app_check_make::stop(&v)].into_iter().flatten().collect();
        return Ok(if reasons.is_empty() { String::new() } else { json!({"decision": "block", "reason": reasons.join("\n\n")}).to_string() });
    }
    let after_tool = matches!(event, "PostToolUse" | "AfterTool");
    if let Some(cmd) = shell_command(&v).filter(|_| !after_tool) {
        let home = raea_io_home::user_home().unwrap_or_default().display().to_string();
        let files = opened_media(cmd);
        let key = format!("{}\n{}\n{cmd}", v["session_id"].as_str().unwrap_or(""), v["cwd"].as_str().unwrap_or(""));
        if !files.is_empty() && app_installed(&home) && !tried_before(&PathBuf::from(&home).join(".raea/hooks/again"), &key) {
            eprintln!("{}", steer(&files));
            std::process::exit(2);
        }
        return Ok(String::new());
    }
    let mut found = Vec::new();
    for path in saved(&v).iter().filter(|p| p.ends_with(".tsx") && std::fs::read_to_string(p).is_ok_and(|t| t.contains("registerRoot"))) {
        let (report, _) = crate::quick::quick(std::slice::from_ref(path), Vec::new())?;
        found.extend(report.lines().skip(1).filter(|l| !l.starts_with("ok ")).map(String::from));
    }
    let quick = (!found.is_empty()).then(|| format!("raea check --quick after this save:\n{}", found.join("\n")));
    let text = [sent, quick].into_iter().flatten().collect::<Vec<_>>().join("\n\n");
    Ok(if text.is_empty() { String::new() } else { after(agent, &text) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_opened_in_a_viewer_is_found_and_the_rest_isnt() {
        assert_eq!(opened_media("open out/a.png out/b.PNG"), ["out/a.png", "out/b.PNG"]);
        assert_eq!(opened_media("cd /tmp && open -a Preview \"sheet 1.jpg\""), ["sheet 1.jpg"]);
        assert_eq!(opened_media("FOO=1 /usr/bin/open cut.mp4; echo done"), ["cut.mp4"]);
        assert_eq!(opened_media("qlmanage -p still.webp >/dev/null 2>&1"), ["still.webp"]);
        assert_eq!(opened_media("xdg-open take.mov"), ["take.mov"]);
        for none in ["open -a raea", "open -a raea out/a.png", "open -R out/a.png", "open https://x.io/a.png", "open notes.pdf", "open .", "cat a.png", "git open a.png", "echo open a.png > x"] {
            assert!(opened_media(none).is_empty(), "{none}: {:?}", opened_media(none));
        }
    }

    #[test]
    fn each_harness_is_told_apart_and_answered_in_its_shape() {
        assert_eq!(agent_of(&json!({"hook_event_name": "BeforeTool", "tool_name": "run_shell_command"})), "gemini");
        assert_eq!(agent_of(&json!({"hook_event_name": "PreToolUse", "client_type": "kimi_code_cli"})), "kimi");
        assert_eq!(agent_of(&json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"})), "claude");
        assert_eq!(shell_command(&json!({"tool_name": "shell_command", "tool_input": {"command": "open a.png"}})), Some("open a.png"));
        assert_eq!(shell_command(&json!({"tool_name": "apply_patch", "tool_input": {"command": "*** Begin Patch\n"}})), None);
        assert_eq!(saved(&json!({"tool_input": {"command": "*** Begin Patch\n*** Update File: a/E.tsx\n@@\n*** Add File: b.ts\n"}})), ["a/E.tsx", "b.ts"]);
        assert_eq!(saved(&json!({"tool_input": {"path": "E.tsx"}})), ["E.tsx"]);
        assert_eq!(after("kimi", "x"), "x");
        assert!(after("gemini", "x").contains("AfterTool") && after("codex", "x").contains("PostToolUse"));
        assert!(steer(&["a b.png".into()]).contains("`raea show \"a b.png\"`"));
    }

    #[test]
    fn the_same_command_again_goes_through_once() {
        let d = std::env::temp_dir().join(format!("raea-hook-again-{}", std::process::id()));
        assert!(!tried_before(&d, "s\n/x\nopen a.png"));
        assert!(tried_before(&d, "s\n/x\nopen a.png"));
        assert!(!tried_before(&d, "s\n/x\nopen a.png"), "a third try is a new first");
        assert!(!tried_before(&d, "other\n/x\nopen a.png"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
