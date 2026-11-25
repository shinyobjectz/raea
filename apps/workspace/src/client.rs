//! A project's client (docs/projects.feature): `raea project tag`, and what a project is filed under.
use super::workspace::*;
use super::*;

/// What a project is filed under in raea cloud (Shane 11-24: raea cloud's tags, not folders inside folders): its
/// client, `project.<name>.client` in .raea/workspace. raea.app files every card `raea show` makes from the folder under
/// it, with a board showing it; a folder naming none is its own client, named for it. Projects don't nest: an old
/// workspace's `parent` is read as "the same client as the parent" and nothing writes it.
pub fn client_of(ws: &Path, name: &str) -> Option<String> {
    get(&ws.join(".raea/workspace"), &format!("project.{name}.client")).filter(|c| !c.trim().is_empty())
}

/// `raea project tag [NAME] [--client C | --no-client]`: a project's client set, cleared, or said (NAME: the project
/// this folder is in when left out)
pub(crate) fn tag(name: Option<&str>, client: Option<&str>, clear: bool, json_out: bool) -> R<String> {
    let here = std::env::current_dir().map_err(|e| e.to_string())?;
    let ws = up(&here, ".raea/workspace").ok_or("not in a workspace: raea workspace init NAME makes one")?;
    tag_in(&ws, &here, name, client, clear, json_out)
}

/// `tag` in the workspace at `ws`, from the folder `here`
pub(crate) fn tag_in(ws: &Path, here: &Path, name: Option<&str>, client: Option<&str>, clear: bool, json_out: bool) -> R<String> {
    // (both resolved: a folder reached through a link, /var for /private/var, is still the workspace's)
    let real = |p: &Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let ws = real(ws);
    let name = match name {
        Some(n) => {
            let s = slug(n);
            if projects(&ws).iter().any(|p| p.0 == s) { s } else if projects(&ws).iter().any(|p| p.0 == n) { n.to_string() } else { return Err(format!("no project {n} in this workspace: raea workspace lists them")) }
        }
        None => {
            let dir = up(here, ".raea/project").map(|d| real(&d)).filter(|d| *d != ws).ok_or("not in a project: name one (raea project tag NAME --client C)")?;
            let rel = relative(&ws, &dir);
            projects(&ws).into_iter().find(|p| p.1 == rel).map(|p| p.0).ok_or_else(|| format!("{} isn't one of this workspace's projects", dir.display()))?
        }
    };
    let file = ws.join(".raea/workspace");
    let key = format!("project.{name}.client");
    if clear {
        if client_of(&ws, &name).is_some() {
            config(Some(&file), &["--unset", &key])?;
            commit(&ws, &[".raea/workspace"], &format!("project {name}: no client"))?;
        }
    } else if let Some(c) = client {
        let c = c.trim();
        if c.is_empty() {
            return Err("a client has a name: --client NAME (--no-client clears it)".into());
        }
        if client_of(&ws, &name).as_deref() != Some(c) {
            set(&file, &key, c)?;
            commit(&ws, &[".raea/workspace"], &format!("project {name}: client {c}"))?;
        }
    }
    let now = client_of(&ws, &name);
    if json_out {
        return Ok(json!({"project": name, "client": now}).to_string());
    }
    Ok(match now {
        Some(c) => format!("{name}: client {c} (raea show files its work under {c}, on {c}'s board)"),
        None => format!("{name}: no client named, so its work is filed under one named for it (raea project tag {name} --client NAME names one)"),
    })
}
