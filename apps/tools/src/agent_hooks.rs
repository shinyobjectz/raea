//! The hooks `raea skills install` adds an agent (docs/agents.feature A5d, A5e; docs/skills.feature SK10), the same two
//! for every harness, each written where and how that harness reads hooks: before a shell command, `raea hook AGENT`
//! steers media opened in the Mac's viewer to `raea show`; after a file is saved, it runs the quick check on a
//! composition. Claude Code's run after every tool and as a turn ends too, to hand the session what the person sent from
//! the raea app (apps/check/src/inbox.rs). An event that already has a `raea hook` entry keeps it, its matcher widened
//! to this raea's when an older raea wrote a narrower one.
use crate::agent_mcp::user_home;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

type R<T> = Result<T, String>;

/// One hook: the harness's event, the tools it matches, and the seconds it may take.
struct Hook {
    event: &'static str,
    matcher: &'static str,
    secs: u64,
}

/// Each harness's hooks, by its own event and tool names.
fn hooks(agent: &str) -> R<Vec<Hook>> {
    let h = |event, matcher, secs| Hook { event, matcher, secs };
    Ok(match agent {
        "claude" => vec![h("PreToolUse", "Bash", 30), h("PostToolUse", "*", 60), h("Stop", "", 30)],
        "codex" => vec![h("PreToolUse", "^(Bash|shell|shell_command|exec_command)$", 30), h("PostToolUse", "^(apply_patch|Edit|Write)$", 60)],
        "gemini" => vec![h("BeforeTool", "run_shell_command", 30), h("AfterTool", "write_file|replace", 60)],
        "kimi" => vec![h("PreToolUse", "Bash|Shell", 30), h("PostToolUse", "Write|Edit|WriteFile|StrReplaceFile", 60)],
        a => return Err(format!("no agent {a:?}: claude, codex, kimi or gemini")),
    })
}

/// The folder an environment variable names, else the one given.
fn named_dir(var: &str, or: PathBuf) -> PathBuf {
    std::env::var_os(var).filter(|v| !v.is_empty()).map_or(or, PathBuf::from)
}

/// Where the harness keeps its hooks.
fn file(agent: &str) -> R<PathBuf> {
    let home = user_home()?;
    Ok(match agent {
        "claude" => home.join(".claude/settings.json"),
        "codex" => named_dir("CODEX_HOME", home.join(".codex")).join("hooks.json"),
        "gemini" => home.join(".gemini/settings.json"),
        _ => named_dir("KIMI_CODE_HOME", home.join(".kimi-code")).join("config.toml"),
    })
}

/// Adds a hook to a JSON file of `{"hooks": {EVENT: [{matcher, hooks: [...]}]}}` (Claude Code, Codex, Gemini CLI)
/// unless the event has a `raea hook` entry; true when it added.
fn add_json(path: &Path, agent: &str, h: &Hook) -> R<bool> {
    let at = path.display();
    let mut doc: Value = match std::fs::read_to_string(path) {
        Ok(t) if !t.trim().is_empty() => serde_json::from_str(&t).map_err(|e| format!("{at}: {e}"))?,
        _ => json!({}),
    };
    let hooks = doc.as_object_mut().ok_or(format!("{at} isn't a JSON object"))?.entry("hooks").or_insert_with(|| json!({}));
    let list = hooks.as_object_mut().ok_or(format!("{at}: hooks isn't an object"))?.entry(h.event).or_insert_with(|| json!([]));
    let list = list.as_array_mut().ok_or(format!("{at}: hooks.{} isn't a list", h.event))?;
    let ours = |m: &Value| m["hooks"].as_array().into_iter().flatten().any(|x| x["command"].as_str().unwrap_or("").starts_with("raea hook"));
    if let Some(m) = list.iter_mut().find(|m| ours(m)) {
        // an older raea's narrower matcher (Claude's after-save hook, now after every tool) is widened, the rest kept
        if h.matcher == "*" && m["matcher"].as_str() != Some("*") {
            m["matcher"] = h.matcher.into();
            write(path, &(serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"))?;
            return Ok(true);
        }
        return Ok(false);
    }
    // Gemini CLI counts milliseconds and names each hook
    let one = if agent == "gemini" { json!({"name": "raea", "type": "command", "command": "raea hook gemini", "timeout": h.secs * 1000}) } else { json!({"type": "command", "command": format!("raea hook {agent}"), "timeout": h.secs}) };
    list.push(json!({"matcher": h.matcher, "hooks": [one]}));
    write(path, &(serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"))?;
    Ok(true)
}

/// Adds a `[[hooks]]` table to Kimi Code's config.toml (only event, matcher, command and timeout: it refuses other
/// keys) unless one runs `raea hook` on that event; true when it added.
fn add_toml(path: &Path, h: &Hook) -> R<bool> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let ours = text.split("[[hooks]]").skip(1).any(|t| {
        let t = t.split("\n[").next().unwrap_or(t);
        t.contains("command = \"raea hook") && t.contains(&format!("event = \"{}\"", h.event))
    });
    if ours {
        return Ok(false);
    }
    let sep = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
    write(path, &format!("{text}{sep}[[hooks]]\nevent = \"{}\"\nmatcher = \"{}\"\ncommand = \"raea hook kimi\"\ntimeout = {}\n", h.event, h.matcher, h.secs))?;
    Ok(true)
}

/// Writes a config file whole, through a temporary file beside it.
fn write(path: &Path, text: &str) -> R<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("raea-tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Adds raea's hooks for an agent; the line says what happened.
pub fn install(agent: &str) -> R<String> {
    let (path, mut added) = (file(agent)?, Vec::new());
    for h in hooks(agent)? {
        if if agent == "kimi" { add_toml(&path, &h)? } else { add_json(&path, agent, &h)? } {
            added.push(h.event);
        }
    }
    let what = if added.is_empty() { "already there".to_string() } else { format!("added ({})", added.join(", ")) };
    // Codex runs a new or changed hook only once the person trusts it
    let trust = if agent == "codex" && !added.is_empty() { "; trust them in Codex's /hooks" } else { "" };
    Ok(format!("{}: hooks `raea hook {agent}` {what}{trust}\n", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_harness_gets_both_hooks_once_and_keeps_its_own() {
        let d = std::env::temp_dir().join(format!("raea-agent-hooks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let c = d.join("claude/settings.json");
        std::fs::create_dir_all(c.parent().unwrap()).unwrap();
        std::fs::write(&c, r#"{"model":"x","hooks":{"PostToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"other"}]},{"matcher":"Write","hooks":[{"type":"command","command":"raea hook claude"}]}]}}"#).unwrap();
        let [pre, post, stop] = <[Hook; 3]>::try_from(hooks("claude").unwrap()).ok().unwrap();
        assert!(add_json(&c, "claude", &pre).unwrap() && !add_json(&c, "claude", &pre).unwrap());
        assert!(add_json(&c, "claude", &post).unwrap() && !add_json(&c, "claude", &post).unwrap(), "an older raea's after-save hook is widened to every tool, once");
        assert!(add_json(&c, "claude", &stop).unwrap() && !add_json(&c, "claude", &stop).unwrap());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&c).unwrap()).unwrap();
        assert_eq!((v["model"].as_str(), v["hooks"]["PostToolUse"].as_array().unwrap().len()), (Some("x"), 2));
        assert_eq!((v["hooks"]["PostToolUse"][0]["matcher"].as_str(), v["hooks"]["PostToolUse"][1]["matcher"].as_str()), (Some("Bash"), Some("*")));
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "raea hook claude");
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "raea hook claude");
        let g = d.join("gemini/settings.json");
        let [before, _] = <[Hook; 2]>::try_from(hooks("gemini").unwrap()).ok().unwrap();
        assert!(add_json(&g, "gemini", &before).unwrap());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&g).unwrap()).unwrap();
        assert_eq!(v["hooks"]["BeforeTool"][0], json!({"matcher": "run_shell_command", "hooks": [{"name": "raea", "type": "command", "command": "raea hook gemini", "timeout": 30000}]}));
        let k = d.join("kimi/config.toml");
        std::fs::create_dir_all(k.parent().unwrap()).unwrap();
        std::fs::write(&k, "default_model = \"k\"\n\n[[hooks]]\nevent = \"PreToolUse\"\nmatcher = \"Bash\"\ncommand = \"mine\"\n").unwrap();
        let [pre, post] = <[Hook; 2]>::try_from(hooks("kimi").unwrap()).ok().unwrap();
        assert!(add_toml(&k, &pre).unwrap() && !add_toml(&k, &pre).unwrap() && add_toml(&k, &post).unwrap() && !add_toml(&k, &post).unwrap());
        let t = std::fs::read_to_string(&k).unwrap();
        assert!(t.starts_with("default_model = \"k\"\n\n[[hooks]]\nevent = \"PreToolUse\"\nmatcher = \"Bash\"\ncommand = \"mine\"\n\n[[hooks]]\nevent = \"PreToolUse\"\nmatcher = \"Bash|Shell\"\ncommand = \"raea hook kimi\"\ntimeout = 30\n"), "{t}");
        assert_eq!(t.matches("[[hooks]]").count(), 3);
        assert!(hooks("cursor").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
