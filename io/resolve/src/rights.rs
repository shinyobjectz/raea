//! The rights confirmation (docs/videoform.feature VF1): once per account, before the first video is read from a link,
//! the person confirms they have the right to use what they add. The answer is kept by the account it was given for
//! (~/.raea/account.json's user, or this machine's when no one is signed in), in ~/.raea/videoform-rights.json. It
//! lives with the resolver, since it's what a link needs before it's read.

use serde_json::{Value, json};
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;

pub const WORDS: &str = "Before raea reads videos from links: confirm you have the right to use the videos you add \
(your own work, a client's, or licensed). A videoform keeps measurements and vectors from each video, never the \
video itself. Each platform's terms are yours to keep.";

fn file() -> PathBuf {
    raea_io_home::env_path("RAEA_VIDEOFORM_RIGHTS", ".raea/videoform-rights.json")
}

/// The account the answer belongs to: the signed-in user's id, else this machine's
fn account() -> String {
    let path = raea_io_home::env_path("RAEA_ACCOUNT", ".raea/account.json");
    let v = crate::fetch::json_file(&path);
    v["user"]["id"].as_str().map(|id| format!("user:{id}")).unwrap_or_else(|| "this machine".into())
}

fn kept() -> Value {
    crate::fetch::json_file(&file())
}

pub fn confirmed() -> bool {
    kept()[account()]["confirmed"].as_bool() == Some(true)
}

/// The person's yes, kept with the account
pub fn confirm() -> Result<String, String> {
    let mut all = kept();
    let who = account();
    all[&who] = json!({"confirmed": true, "on": crate::fetch::today(), "words": WORDS});
    if let Some(d) = file().parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(file(), serde_json::to_string_pretty(&all).unwrap()).map_err(|e| format!("{}: {e}", file().display()))?;
    Ok(format!("raea videoform: confirmed for {who}; links can be added from now on"))
}

/// Before the first link: asked in the person's terminal, or refused with how they answer it themselves
pub fn ask() -> Result<(), String> {
    if confirmed() {
        return Ok(());
    }
    if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        eprint!("{WORDS}\nDo you have the right to use the videos you add? [y/N] ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        if matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            eprintln!("{}", confirm()?);
            return Ok(());
        }
        return Err("raea videoform: not confirmed, so no links were read (files on your drive can still be added)".into());
    }
    Err(format!("raea videoform: links wait for the person's confirmation, once for the account.\n\n  {WORDS}\n\nThe person answers it themselves, in their terminal: `raea videoform rights` (an agent never answers it for them). Files on the drive can be added meanwhile."))
}

/// `raea videoform rights [--yes]`: the question asked, or the person's yes given as a flag
pub fn verb(args: &[String]) -> (String, i32) {
    if args.iter().any(|a| a == "--yes") {
        return match confirm() {
            Ok(m) => (m, 0),
            Err(e) => (e, 1),
        };
    }
    if confirmed() {
        return (format!("raea videoform: confirmed for {}", account()), 0);
    }
    match ask() {
        Ok(()) => (String::new(), 0),
        Err(_) if !std::io::stdin().is_terminal() => (format!("{WORDS}\n\nIf you do, run `raea videoform rights --yes`."), 1),
        Err(e) => (e, 1),
    }
}
