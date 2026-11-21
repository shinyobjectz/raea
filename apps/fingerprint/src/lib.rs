//! `raea fingerprint` (docs/fingerprint.feature FP3a): a person's fingerprint made, bootstrapped, taught and used to
//! score a piece. The encoders take seconds to load, so the verbs run in a resident service (`raea fingerprint
//! serve`, serve.rs) that keeps them loaded: the first verb starts it, on localhost, and later verbs reach it in
//! milliseconds (earshot's CLI reloads its models on every call, 20-40 s). It leaves after 30 idle minutes, or on
//! `raea fingerprint stop`. RAEA_FINGERPRINT_HERE=1 runs a verb in this process instead.

mod approve;
mod bootstrap;
mod moving;
mod notes;
mod pairing;
mod pick;
mod postmortem;
mod profiles;
mod reembed;
mod serve;
mod things;
mod verbs;

use std::path::Path;

pub use serve::serve;

pub const USAGE: &str = "raea fingerprint new NAME [--who W] [--about TEXT] [--extends FP] [--kinds K,K] [--tags T,T] | raea fingerprint bootstrap FILE... [--from LIST] [--per N] | raea fingerprint teach FILE --axis A --at T [--bad|--good] [--span S] [--box x0,y0,x1,y1] [--words W] [--project P] | raea fingerprint approve FILE... [--project P] | raea fingerprint pair WORSE BETTER --axis A [--at T] [--then T2] | raea fingerprint versions V1 V2... | raea fingerprint profile REFS... --as NAME [--against LIST] | raea fingerprint score FILE [--was OLD] [--ask flags|unsure] [--json] [--sheet out.png] [--timeline] [--profile NAME] | raea fingerprint report [--json] | raea fingerprint which [PATH] [--json] | raea fingerprint use NAME [--in project|workspace] [PATH] | raea fingerprint diff A B [--json] | raea fingerprint share NAME [--in project|workspace] [PATH] | raea fingerprint reembed PLACE... | raea fingerprint read PROJECT [--by kimi|claude] | raea fingerprint readings [PROJECT] [--ask N] [--tally] | raea fingerprint learn-notes [PROJECT] | raea fingerprint move [--fp CHILD] [--to PARENT] [--axis A] [--source S] [--project P] | raea fingerprint postmortem [PROJECT] [--ask N] [--tally] | raea fingerprint things FILE [--project P] [--json] | raea fingerprint stop [--fp NAME]   (a person's sense of good work as axes with small heads over on-device encoders, docs/fingerprint.md: new makes ~/.raea/fingerprints/NAME with raea's base taxonomy in fingerprint.ts (or FP's axes with --extends), what it's for, the kinds of piece and project tags it judges, and every axis scoring from its words; bootstrap puts known defects into windows of real work and trains the mechanical axes, held out by source, and patch heads that say where in the frame a picture defect is; teach adds a label at a moment; approve makes approved work clean on every axis, away from its notes; pair says one moment is better than another on an axis (a pick, a hand edit); versions makes weak pairs of noted moments the next version changed; score prints the moments it expects a note on first (where in the frame, and the layers there, when its patch head points held out), then every axis per window with --json, a sheet PNG with --sheet, or the piece second by second in words with --timeline, and with --was OLD whether each of OLD's gating flags is fixed, still there, or looks disguised (nearer the axis's disguised defects than clean work), and with --ask flags each flag put to clef again on its own window, whether clef agrees said beside it and kept as clef's evidence (unsure: the moments it's unsure of too); profile takes the ranges of work you admire (loudness, tilt, cut rate, motion, brightness, contrast, saturation, sharpness), checked leave-one-out, and score --profile says what falls outside them, where and which way; report says each head's held-out score, labels, cheats, gate and what it can't judge; which says the one that judges a piece (its project's, its workspace's, then yours) and why, use pins one there, diff lists the axes two fingerprints rank the same evidence most differently on, and share copies one's taxonomy and evidence (no heads, no media) into a project or workspace for its team; reembed moves a fingerprint's evidence to this machine's encoders, re-embedding the rows whose media it finds; read has a fresh kimi or claude session read a project's pinned notes into kinds and axes, readings puts them to you to check and tallies your answers, and learn-notes makes the defects and praise evidence once you've agreed with 45 kinds and 40 axes in 50, each project's in the fingerprint that judges it; move takes a child's own labels (by axis, source or project) up into a fingerprint it extends, which never reads them otherwise; postmortem scores every version in a project's feed and asks you at most ten yes/no questions in the app, each with its moment playing, where the fingerprint disagrees with your labels or is least sure, and --tally makes your answers evidence and says how each head's held-out score moved; things finds a piece's shots, its things (a render's layers, and the marks drawn on its notes) with their depth, and the camera's path, and says where a thing floats, two pass through each other, the camera lurches mid-move or nothing is on screen, as JSON for `raea check --things` with --json)";

/// Evaluates a .ts file's default export to its JSON (the CLI's spec host)
pub type Eval = fn(&Path) -> Result<String, String>;

/// The flags each verb takes, and whether each takes a value
pub(crate) const FLAGS: [(&str, bool); 31] = [
    ("--fp", true),
    ("--was", true),
    ("--to", true),
    ("--source", true),
    ("--kinds", true),
    ("--tags", true),
    ("--who", true),
    ("--about", true),
    ("--extends", true),
    ("--from", true),
    ("--per", true),
    ("--axis", true),
    ("--at", true),
    ("--then", true),
    ("--span", true),
    ("--box", true),
    ("--words", true),
    ("--project", true),
    ("--sheet", true),
    ("--in", true),
    ("--as", true),
    ("--against", true),
    ("--profile", true),
    ("--by", true),
    ("--ask", true),
    ("--tally", false),
    ("--bad", false),
    ("--good", false),
    ("--json", false),
    ("--timeline", false),
    ("--here", false),
];

/// A flag's value
pub(crate) fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

pub(crate) fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// A path as given, against the folder the verb ran in (an absolute one stays itself)
pub(crate) fn at(cwd: &Path, p: &str) -> std::path::PathBuf {
    cwd.join(p)
}

/// The files a verb names, against the folder it ran in
pub(crate) fn named(args: &[String], cwd: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    Ok(words(args).map_err(anyhow::Error::msg)?.iter().map(|f| at(cwd, f)).collect())
}

/// The files a list names, one a line against the list's own folder; blank lines and # lines are skipped
pub(crate) fn listed(cwd: &Path, list: &str) -> anyhow::Result<Vec<std::path::PathBuf>> {
    use anyhow::Context;
    let list = at(cwd, list);
    let text = std::fs::read_to_string(&list).with_context(|| list.display().to_string())?;
    let base = list.parent().unwrap_or(cwd);
    Ok(text.lines().map(str::trim).filter(|l| !(l.is_empty() || l.starts_with('#'))).map(|l| at(base, l)).collect())
}

/// The words that aren't flags or their values; an unknown flag is an error
pub(crate) fn words(args: &[String]) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match FLAGS.iter().find(|(f, _)| f == a) {
            Some((_, true)) => {
                it.next().ok_or_else(|| format!("{a} takes a value"))?;
            }
            Some(_) => {}
            None if a.starts_with("--") => return Err(format!("no flag {a}")),
            None => out.push(a.as_str()),
        }
    }
    Ok(out)
}

/// `raea fingerprint VERB ...`, printed; its exit code (1: the verb failed, 2: the command was wrong)
pub fn cli(args: &[String], eval: Eval) -> i32 {
    let verb = match args.first().map(String::as_str) {
        None | Some("--help" | "-h" | "help") => {
            println!("{USAGE}");
            return 0;
        }
        Some(v) => v,
    };
    if let Err(e) = words(&args[1..]) {
        eprintln!("raea fingerprint: {e}\n{USAGE}");
        return 2;
    }
    match verb {
        "serve" => match serve(eval) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("raea fingerprint serve: {e:#}");
                1
            }
        },
        "report" => verbs::report_here(args),
        "which" => pick::which_here(&args[1..], &std::env::current_dir().unwrap_or_default()),
        "diff" => pick::diff_here(&args[1..]),
        "read" => notes::read_here(&args[1..], &std::env::current_dir().unwrap_or_default()),
        "readings" => notes::readings_here(&args[1..], &std::env::current_dir().unwrap_or_default()),
        "share" => pick::share_here(&args[1..], &std::env::current_dir().unwrap_or_default()),
        "use" => pick::use_here(&args[1..], &std::env::current_dir().unwrap_or_default()),
        "new" | "bootstrap" | "teach" | "approve" | "pair" | "versions" | "learn-notes" | "move" | "postmortem" | "things" | "reembed" | "profile" | "score" | "stop" => {
            let cwd = std::env::current_dir().unwrap_or_default();
            let here = has(args, "--here") || std::env::var_os("RAEA_FINGERPRINT_HERE").is_some();
            let (out, code) = if here { serve::here(args, &cwd, eval) } else { serve::call(args, &cwd) };
            if !out.is_empty() {
                println!("{}", out.trim_end());
            }
            code
        }
        v => {
            eprintln!("raea fingerprint: no verb {v}\n{USAGE}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_usage_names_every_flag() {
        let missing: Vec<_> = super::FLAGS.iter().filter(|(f, _)| *f != "--here" && !super::USAGE.contains(f)).collect();
        assert!(missing.is_empty(), "raea fingerprint takes flags its usage doesn't name: {missing:?}");
    }
}
