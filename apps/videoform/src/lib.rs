//! `raea videoform` (docs/videoform.md, docs/videoform.feature VF1, VF2): the kind of video a person makes again and
//! again, read once from its references. A videoform is a fingerprint with a `videoform` section in its fingerprint.ts
//! (verbs.rs), so it's found, inherited, taught and shared as any fingerprint is. `add` reads each reference, from the
//! drive where it is or from a link (a platform page, a page with its own player, a playlist or channel's newest
//! `--last N`, a direct link to a file), and keeps only its readings: the reference's source, title, creator and date,
//! its measures, and its columns by content hash. A link's streams are held in a private folder while they're read
//! and dropped after, so no video is kept. A source read before is known by its link or its content hash.
//!
//!   raea videoform new NAME --context "…" [--extends FP] [--kinds K] [--tags T] [SOURCE...]
//!   raea videoform add NAME SOURCE... [--from LIST] [--last N] [--context "…"] [--sheet DIR]
//!   raea videoform ls [NAME] [--json]
//!   raea videoform report NAME [--json]
//!   raea videoform set NAME MEASURE ignore|free|keep|target [LO HI] [--why "…"] | unset NAME MEASURE
//!   raea videoform set NAME RULE keep|free|ignore [--why "…"]   (a rule's id, as report lists it: VF8)
//!   raea videoform score NAME FILE [--json]
//!   raea videoform vocab NAME
//!   raea videoform rights [--yes]
//!
//! Its files are the fingerprint's folder (~/.raea/fingerprints/NAME, or a project's or workspace's): fingerprint.ts
//! with the section (the context, and each reference in a brief's `{ target, note }` shape), and
//! videoform/references.jsonl beside it, what was read of each reference, one a line. After every `add` it says what
//! the references hold (found.rs): each measure's range and how strongly it holds, for the person to correct. Each
//! reading keeps a small still a shot (videoform/stills/HASH/N.jpg), and once there's a vocabulary each report makes
//! a contact sheet a shot kind from them (videoform/kinds/kind-N.jpg), for the person to confirm the rules on.

mod found;
mod ingest;
mod pass;
mod readers;
mod rights;
mod verbs;
mod vocab;

pub use verbs::context;

use serde_json::Value;
use std::path::Path;

/// Evaluates a .ts file's default export to its JSON (the CLI's spec host): fingerprint.ts is read through it
pub type Eval = fn(&Path) -> Result<String, String>;

pub const USAGE: &str = "raea videoform new NAME --context \"…\" [--extends FP] [--kinds K] [--tags T] [SOURCE...] | raea videoform add NAME SOURCE... [--from LIST] [--last N] [--context \"…\"] [--sheet DIR] | raea videoform ls [NAME] [--json] | raea videoform report NAME [--json] | raea videoform set NAME MEASURE ignore|free|keep|target [LO HI] [--why \"…\"] | raea videoform unset NAME MEASURE | raea videoform score NAME FILE [--json] | raea videoform vocab NAME | raea videoform rights [--yes]   (a kind of video read once from its references, docs/videoform.md: a fingerprint with a videoform section, found and inherited as any fingerprint is; new makes it, extending your own fingerprint unless --extends names another, judging projects tagged with its name unless --kinds or --tags say which; add reads each file or folder on the drive where it is, and each link (a platform page, a page with its own player, a playlist or channel's newest --last N, a direct link to a file; LIST a file of them, one a line) once, keeping only its readings (source, title, creator, date, shots and pace, loudness, Fingerprint's columns by content hash) and never the video, making the videoform first when it's new; --context is what the videos are, --sheet DIR a contact sheet of each to look at; ls says what each videoform has read; report says what its references hold, measure by measure (shot lengths, cuts a minute, words a minute, pauses, framing, captions, speech and music, cuts on the beat, loudness): each range, how strongly it holds left one out, measures where they fall in two groups, and references far from the rest, as add does after reading; set is your word over theirs, kept with why (ignore: not the videoform's; free: it varies, not checked; keep: checked though it varies; target LO HI: checked against yours), unset takes it back, and raea's delivery floor (-14 LUFS, true peak -1 dBTP) comes before both; score reads a cut as a reference is read and says which checked measures fall outside, and which way to move; vocab starts vocabulary.ts beside fingerprint.ts for your AI to write from the context (the context says what matters, the references show how it's done: places and things to look for, sounds, words on screen and said, the sections the videos move through, and the context's words for how they feel, with the measures that say it), and once it's written, report and vocab say what each term did on the references (never fires, fires everywhere, fires with another, missing where the context says always), where each section was found, shots that recur with no term naming them, the shot kinds the references are made of (named in the vocabulary's words, with their scale, people and length, and how many references have each), and the structure, section by section, written as NAME.script.ts, the template a project made to the videoform starts from; and the rules that hold in most of the references (what every video has, how it opens and closes, what each section is made of and which comes first, a place's things shown with it, what follows a word said or shown), each with how strongly it holds, written as NAME.rules.ts, a controller of rule fields, with a contact sheet a shot kind from the references' stills to confirm them on (set NAME RULE keep|free|ignore is your word on one: kept, a rule broken is an error); rights is the person's once-per-account confirmation that they may use what they add from links)";

pub(crate) fn today() -> String {
    raea_io_resolve::fetch::today()
}

/// A videoform's name: letters, digits, - and _
pub(crate) fn name_ok(n: &str) -> bool {
    !n.is_empty() && n.len() <= 80 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub(crate) fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// The words of `args` that aren't flags or their values
pub(crate) fn plain(args: &[String]) -> Vec<String> {
    let valued = ["--from", "--last", "--context", "--sheet", "--extends", "--kinds", "--tags", "--why"];
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
        } else if valued.contains(&a.as_str()) {
            skip = true;
        } else if !a.starts_with("--") {
            out.push(a.clone());
        }
    }
    out
}

/// What a videoform has read: its folder's videoform/references.jsonl, one reference a line
pub(crate) fn references(dir: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(dir.join("videoform/references.jsonl")).unwrap_or_default();
    text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// `raea videoform ...`, its exit code
pub fn cli(args: &[String], eval: Eval) -> i32 {
    let rest = args.get(1..).unwrap_or_default();
    let (out, code) = match args.first().map(String::as_str) {
        Some("new") => match plain(rest).first() {
            Some(n) if verbs::open(n, None).is_ok() => (format!("raea videoform: {n} is made already (raea videoform add {n} SOURCE... reads more)"), 1),
            _ => ingest::add(rest, eval),
        },
        Some("add") => ingest::add(rest, eval),
        Some("ls") => verbs::ls(rest),
        Some("report") => found::verb_report(rest, eval),
        Some("vocab") => vocab::verb(rest, eval),
        Some("set") => found::verb_set(rest, eval, false),
        Some("unset") => found::verb_set(rest, eval, true),
        Some("score") => found::verb_score(rest, eval),
        Some("rights") => rights::verb(rest),
        _ => (format!("usage: {USAGE}"), 2),
    };
    if !out.is_empty() {
        if code == 0 {
            println!("{out}");
        } else {
            eprintln!("{out}");
        }
    }
    code
}
