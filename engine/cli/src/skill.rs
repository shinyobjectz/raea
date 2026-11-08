//! `raea skill` and `raea skills install` (docs/skills.feature SK6): the agent skills this raea ships, version-matched.
//! An agent can hold a skill from an older or newer install than its `raea`; the copy's front matter names the version it
//! came with (`version:`, stamped by tools/release.sh and by `skills install`), and when that differs from
//! `raea --version` the agent reads `raea skill` instead.
use super::*;

pub const USAGE: &str = "raea skill [NAME] [--reference cli|feed|files|sdk] | raea skills install --agent claude|codex|kimi|gemini [--pin MOVE]... [--no-mcp] [--no-hooks]   (the agent skill this raea ships, for its version, which raea --version prints: an agent whose copy names another version reads this; install puts raea's skills where that agent reads skills, adds the blender MCP server unless --no-mcp, makes each --pin MOVE a /MOVE of its own, and for claude adds the quick-check hook unless --no-hooks)";

/// Where each agent reads a person's skills, one folder per skill holding SKILL.md. Checked by hand on 11-05:
/// claude: ~/.claude/skills (Claude Code's docs); codex: ~/.agents/skills (developers.openai.com/codex/skills, the user
/// scope; older builds read ~/.codex/skills); kimi: $KIMI_CODE_HOME/skills, else ~/.kimi-code/skills (kimi 2.3.3's own
/// binary; it also reads ~/.agents/skills and ~/.claude/skills); gemini: ~/.gemini/skills (geminicli.com/docs/cli/skills).
fn target(agent: &str) -> R<PathBuf> {
    let home = tools::agent_mcp::user_home()?;
    Ok(match agent {
        "claude" => home.join(".claude/skills"),
        "codex" => home.join(".agents/skills"),
        "kimi" => std::env::var_os("KIMI_CODE_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".kimi-code")).join("skills"),
        "gemini" => home.join(".gemini/skills"),
        a => return Err(format!("no agent {a:?}: claude, codex, kimi or gemini")),
    })
}

/// Each agent and the folder it reads skills from, for `raea doctor` (an agent with no folder is skipped).
pub fn agents() -> Vec<(&'static str, PathBuf)> {
    ["claude", "codex", "kimi", "gemini"].into_iter().filter_map(|a| target(a).ok().map(|t| (a, t))).collect()
}

/// `raea doctor [--fix] [DIR]` (SK10): what doesn't match this raea, an agent's skill reinstalled by --fix.
pub fn doctor(args: &[String]) -> R<String> {
    let again = |agent: &str| install(&["install".into(), "--agent".into(), agent.into(), "--no-mcp".into()]);
    raea_app_workspace::doctor::doctor(args, &agents(), &version(), &again)
}

/// This raea's version: the release's (share/raea/VERSION), or the crate's for a build from a checkout.
pub fn version() -> String {
    std::fs::read_to_string(raea_io_home::home().join("VERSION"))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| format!("{}-checkout", env!("CARGO_PKG_VERSION")))
}

/// The skills raea ships: an install's share/raea/skills, or a checkout's skills/skills (the raea-skills submodule).
fn shipped() -> R<PathBuf> {
    let home = raea_io_home::home();
    ["skills", "skills/skills"].iter().map(|d| home.join(d)).find(|d| d.join("raea/SKILL.md").is_file()).ok_or_else(|| format!("no skills under {}: reinstall raea (curl -fsSL https://raea.io/install | sh), or in a checkout, git submodule update --init skills", home.display()))
}

/// SKILL.md with `version: V` in its front matter (replacing one that's there), so a copy says which raea it came with.
pub fn stamp(text: &str, v: &str) -> String {
    let Some(rest) = text.strip_prefix("---\n") else { return text.to_string() };
    let Some(end) = rest.find("\n---\n") else { return text.to_string() };
    let lines: Vec<&str> = rest[..end].lines().filter(|l| !l.starts_with("version:")).collect();
    format!("---\n{}\nversion: {v}\n---\n{}", lines.join("\n"), &rest[end + 5..])
}

/// `raea skill [NAME] [--reference cli|feed|files|sdk]`: the shipped skill's text, or one of its references.
pub fn skill(args: &[String]) -> R<String> {
    let name = args.first().filter(|a| !a.starts_with("--")).map(String::as_str).unwrap_or("raea");
    let reference = args.iter().position(|a| a == "--reference").map(|i| args.get(i + 1).cloned().ok_or("--reference takes cli, feed, files or sdk"));
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !ok(name) {
        return Err(format!("no skill {name:?}"));
    }
    let dir = shipped()?.join(name);
    if !dir.join("SKILL.md").is_file() {
        return Err(format!("no skill {name:?}{}: {}", if name == "raea-bevy" { " (raea-bevy is the raea skill's 3D guide now: 3d.md beside its SKILL.md)" } else { "" }, installed_names()?.join(", ")));
    }
    match reference {
        Some(r) => {
            let r = r?;
            let path = dir.join("reference").join(format!("{r}.md"));
            if !ok(&r) || !path.is_file() {
                return Err(format!("no reference {r:?} in {name}: cli, feed, files or sdk"));
            }
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
        }
        None => Ok(stamp(&std::fs::read_to_string(dir.join("SKILL.md")).map_err(|e| e.to_string())?, &version())),
    }
}

fn installed_names() -> R<Vec<String>> {
    // raea-work is internal (work.raea.io's pages); raea-skills' _template starts a skill and isn't one
    let ok = |e: &std::fs::DirEntry, n: &str| n != "raea-work" && !n.starts_with('_') && e.path().join("SKILL.md").is_file();
    let mut v: Vec<String> = std::fs::read_dir(shipped()?).map_err(|e| e.to_string())?.flatten().filter_map(|e| Some(e.file_name().to_string_lossy().into_owned()).filter(|n| ok(&e, n))).collect();
    v.sort();
    Ok(v)
}

/// Copies a folder, following links.
fn copy(from: &Path, to: &Path) -> R<()> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for e in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy(&src, &dst)?;
        } else if src.is_file() {
            std::fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// `raea skills install --agent claude|codex|kimi|gemini`: raea's skills, stamped with this version, where that agent
/// reads them; each replaces the copy that was there.
pub fn install(args: &[String]) -> R<String> {
    if args.first().map(String::as_str) != Some("install") {
        return Err(format!("usage: {USAGE}"));
    }
    let agent = args.iter().position(|a| a == "--agent").and_then(|i| args.get(i + 1)).ok_or("which agent: --agent claude|codex|kimi|gemini")?;
    let to = target(agent)?;
    let (from, v) = (shipped()?, version());
    let mut out = String::new();
    for name in installed_names()? {
        let dst = to.join(&name);
        if dst.exists() {
            std::fs::remove_dir_all(&dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        }
        copy(&from.join(&name), &dst)?;
        let md = dst.join("SKILL.md");
        let text = std::fs::read_to_string(&md).map_err(|e| e.to_string())?;
        std::fs::write(&md, stamp(&text, &v)).map_err(|e| e.to_string())?;
        out += &format!("{}: {name} {v}\n", dst.display());
    }
    let old = to.join("raea-bevy"); // an older raea's copy (stamped `version:`), whose text is the raea skill's 3d.md now
    if std::fs::read_to_string(old.join("SKILL.md")).is_ok_and(|t| t.contains("\nversion: ")) && std::fs::remove_dir_all(&old).is_ok() { out += &format!("{}: removed, now raea's 3d.md\n", old.display()); }
    // pinned moves (SK10): a skill per move, so an agent's /quieter loads the one guide
    for (i, a) in args.iter().enumerate() {
        if a == "--pin" {
            let m = args.get(i + 1).ok_or("--pin takes a move: quieter, bolder, layout, adapt...")?;
            out += &tools::agent_mcp::pin(&to, m, &v)?;
        }
    }
    // Claude Code's hook: the quick check after a composition is saved (SK10); --no-hooks leaves its settings alone
    if agent == "claude" && !args.iter().any(|a| a == "--no-hooks") {
        let settings = target("claude")?.with_file_name("settings.json");
        out += &format!("{}: hook `raea hook claude` {}\n", settings.display(), if tools::agent_mcp::claude_hook(&settings)? { "added" } else { "already there" });
    }
    // the MCP servers raea offers (SK11): blender, for a running Blender; --no-mcp leaves the agent's servers alone
    if !args.iter().any(|a| a == "--no-mcp") {
        out += &tools::agent_mcp::offer(agent)?;
        out += "\n";
    }
    Ok(format!("{out}{agent} reads them in its next session"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_puts_one_version_in_the_front_matter() {
        let md = "---\nname: raea\nversion: 0.0.1\ndescription: d\n---\n\n# raea\n";
        assert_eq!(stamp(md, "0.1.0-canary.3"), "---\nname: raea\ndescription: d\nversion: 0.1.0-canary.3\n---\n\n# raea\n");
        assert_eq!(stamp("# no front matter\n", "1"), "# no front matter\n");
    }

    #[test]
    fn the_skill_and_its_references_come_from_this_raea() {
        let core = skill(&[]).unwrap();
        assert!(core.starts_with("---\nname: raea\n") && core.contains(&format!("\nversion: {}\n---\n", version())), "{}", &core[..200.min(core.len())]);
        assert!(skill(&["--reference".into(), "cli".into()]).unwrap().contains("raea render"));
        assert!(skill(&["raea-bevy".into()]).unwrap_err().contains("3d.md") && installed_names().unwrap() == ["raea"]);
        assert!(skill(&["../x".into()]).is_err());
        assert!(skill(&["--reference".into(), "../../x".into()]).is_err());
    }

    #[test]
    fn install_targets_each_agents_folder() {
        assert!(target("claude").unwrap().ends_with(".claude/skills"));
        assert!(target("codex").unwrap().ends_with(".agents/skills"));
        assert!(target("gemini").unwrap().ends_with(".gemini/skills"));
        assert!(target("cursor").is_err());
    }
}
