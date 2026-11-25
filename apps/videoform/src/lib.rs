//! `raea videoform` (docs/videoform.md, docs/videoform.feature VF1, VF2): the kind of video a person makes again and
//! again, read once from its references. A videoform is a fingerprint with a `videoform` section in its fingerprint.ts
//! (verbs.rs), so it's found, inherited, taught and shared as any fingerprint is. `add` reads each reference, from the
//! drive where it is or from a link (a platform page, a page with its own player, a playlist or channel's newest
//! `--last N`, a direct link to a file), and keeps only its readings: the reference's source, title, creator and date,
//! its measures, and its columns by content hash. A link's streams are held in a private folder while they're read
//! and dropped after, so no video is kept. A source read before is known by its link or its content hash.
//!
//!   raea videoform new NAME --context "…" [--extends FP] [--kinds K] [--tags T] [SOURCE...]
//!   raea videoform add NAME SOURCE... [--from LIST] [--last N] [--context "…"] [--sheet DIR] [--brand B]
//!   raea videoform ls [NAME] [--json]
//!   raea videoform report NAME [--json]
//!   raea videoform set NAME MEASURE ignore|free|keep|target [LO HI] [--why "…"] | unset NAME MEASURE
//!   raea videoform set NAME RULE keep|free|ignore [--why "…"]   (a rule's id, as report lists it: VF8)
//!   raea videoform score NAME FILE [--json]
//!   raea videoform vocab NAME
//!   raea videoform follow NAME FOOTAGE... [--seconds S] [--shape W:H] [--brand B] [--plan FILE] [--script FILE] [--say N,N,...]
//!       (VF9: footage matched, what it lacks, the cut planned; VF9b: cards from the brand's kit; VF9c: as a script)
//!   raea videoform kit BRAND [FILE|DIR...] [--json]   (VF9b: a brand's stills and clips, in its fingerprint)
//!   raea videoform rights [--yes]
//!
//! Its files are the fingerprint's folder (~/.raea/fingerprints/NAME, or a project's or workspace's): fingerprint.ts
//! with the section (the context, and each reference in a brief's `{ target, note }` shape), and
//! videoform/references.jsonl beside it, what was read of each reference, one a line. After every `add` it says what
//! the references hold (found.rs): each measure's range and how strongly it holds, for the person to correct. Each
//! reading keeps a small still a shot (videoform/stills/HASH/N.jpg), and once there's a vocabulary each report makes
//! a contact sheet a shot kind from them (videoform/kinds/kind-N.jpg), for the person to confirm the rules on.

mod follow;
mod found;
mod ingest;
mod kit;
mod pass;
mod readers;
mod verbs;
mod vocab;

pub use verbs::context;

use serde_json::Value;
use std::path::Path;

/// Evaluates a .ts file's default export to its JSON (the CLI's spec host): fingerprint.ts is read through it
pub type Eval = fn(&Path) -> Result<String, String>;

pub const USAGE: &str = "raea videoform new NAME --context \"…\" [--extends FP] [--kinds K] [--tags T] [SOURCE...] | raea videoform add NAME SOURCE... [--from LIST] [--last N] [--context \"…\"] [--sheet DIR] [--brand B] [--group G] | raea videoform docs NAME [GROUP] | raea videoform ls [NAME] [--json] | raea videoform report NAME [--json] | raea videoform set NAME MEASURE ignore|free|keep|target [LO HI] [--why \"…\"] | raea videoform unset NAME MEASURE | raea videoform score NAME FILE [--json] | raea videoform vocab NAME | raea videoform follow NAME FOOTAGE... [--seconds S] [--shape W:H] [--brand B] [--plan FILE] [--script FILE] [--say N,N,...] [--brief FILE,...] | raea videoform kit BRAND [FILE|DIR...] [--json] | raea videoform rights [--yes]   (a kind of video read once from its references, docs/videoform.md: a fingerprint with a videoform section, found and inherited as any fingerprint is; new makes it, extending your own fingerprint unless --extends names another, judging projects tagged with its name unless --kinds or --tags say which; add reads each file or folder on the drive where it is, and each link (a platform page, a page with its own player, a playlist or channel's newest --last N, a direct link to a file; LIST a file of them, one a line) once, keeping only its readings (source, title, creator, date, shots and pace, how each shot is layered, loudness, Fingerprint's columns by content hash) and never the video, and a reference read before layers were gets only its layers read, making the videoform first when it's new; --context is what the videos are, --sheet DIR a contact sheet of each to look at, --brand B the brand the videos read now are for (what only one brand's videos hold is mined from them alone and written to that brand's own fingerprint, brand-B, made when there's none; what holds across brands is the videoform's); --group G the group they're in (a brand and its round, \"Dutch R1\"; the brand when it's left out) with the documents given among the sources (a brief, a brand's guide, notes: PDF, Word, text, or a still named), each read once into passages and kept by content hash, and --context, the person's words, kept as theirs and over any document (VF8f): every add then finds each passage where its group's videos carry it out and how, what two groups or more asked and how it's made is the videoform's, what one group asked its brand's, and briefs.ts, the rules your AI reads in them that a similarity can't (a never, an order, a choice), measured on each group's videos; docs lists each passage with its id and where it was carried out, for your AI to write briefs.ts from; ls says what each videoform has read; report says what its references hold, measure by measure (shot lengths, cuts a minute, words a minute, pauses, framing, captions, how it's layered (a person cut out over another picture, a picture inset, a card laid over, a plain backdrop, a split frame, a screen shown), speech and music, cuts on the beat, loudness): each range, how strongly it holds left one out, measures where they fall in two groups, and references far from the rest, as add does after reading; set is your word over theirs, kept with why (ignore: not the videoform's; free: it varies, not checked; keep: checked though it varies; target LO HI: checked against yours), unset takes it back, and raea's delivery floor (-14 LUFS, true peak -1 dBTP) comes before both; score reads a cut as a reference is read and says which checked measures fall outside, and which way to move; vocab starts vocabulary.ts beside fingerprint.ts for your AI to write from the context (the context says what matters, the references show how it's done: places and things to look for, sounds, words on screen and said, the sections the videos move through, and the context's words for how they feel, with the measures that say it), and once it's written, report and vocab say what each term did on the references (never fires, fires everywhere, fires with another, missing where the context says always), where each section was found, shots that recur with no term naming them, the shot kinds the references are made of (named in the vocabulary's words, with their scale, people and length, and how many references have each), and the structure, section by section, written as NAME.script.ts, the template a project made to the videoform starts from; and the rules that hold in most of the references (what every video has, how it opens and closes, what each section is made of and which comes first, a place's things shown with it, what follows a word said or shown), each with how strongly it holds, written as NAME.taste.ts, taste lines with their checks, with a contact sheet a shot kind from the references' stills to confirm them on (set NAME RULE keep|free|ignore is your word on one: kept, a rule broken is an error); follow reads new raw footage as references are read, once (a sentence said or a few seconds with no talk a piece, kept by content hash), matches each piece to the shot kinds, says what the footage lacks that most references have (a shot list), and plans the cut slot by slot from the structure to its lengths, a layered kind made from two pieces at once in the place the references put it (the speaker cut out over B-roll, B-roll in a box over the speaker) (--seconds S for another length; --shape W:H the frame it's made for; --brand B each card, backdrop and inset shown from B's kit, the asset that fits what's said over it, and what the kit lacks said; --plan FILE writes it as JSON, its pieces numbered; --script FILE writes it as NAME.script.ts, the script a project that approves its script first (the brief's approve: [\"script\"]) shows the person before anything is edited; --say N,N,... the lines your AI chose by what they say, in its order, with the picture cut over them; --brief FILE,... the new job's documents, and the project's brief.ts, each passage set against what the videoform learned of briefs: the ask it's like and how that's made, or new); kit is a brand's kit, kept in its fingerprint (brand-B, made when there's none) so a team shares it: each still in a folder given and each still or clip named, copied in once by content hash and read for what it is (a logo, a product, a person, a testimonial, a screenshot, a graphic, a clip), its words and colours; with no files, what it holds; rights is the person's once-per-account confirmation that they may use what they add from links)";

/// A folder of raea's caches that `raea videoform` keeps its working files in
pub(crate) fn cache(name: &str) -> std::path::PathBuf {
    raea_io_home::caches().unwrap_or_else(std::env::temp_dir).join(name)
}

/// The video files raea videoform reads
const MEDIA: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi", "mxf", "mts"];

/// Whether a file is a video raea videoform reads, by its extension
pub(crate) fn is_media(p: &Path) -> bool {
    ext_in(p, MEDIA)
}

/// Whether a file's extension is one of `list`'s
pub(crate) fn ext_in(p: &Path, list: &[&str]) -> bool {
    p.extension().is_some_and(|x| list.contains(&x.to_string_lossy().to_ascii_lowercase().as_str()))
}

/// The files in a folder that `keep` takes, through its folders (hidden files and Screen Studio projects left out)
pub(crate) fn walk(d: &Path, out: &mut Vec<std::path::PathBuf>, keep: fn(&Path) -> bool) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    let mut all: Vec<std::path::PathBuf> = rd.flatten().map(|e| e.path()).collect();
    all.sort();
    for p in all {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.starts_with('.') || name.ends_with(".screenstudio") {
            continue;
        }
        if p.is_dir() {
            walk(&p, out, keep);
        } else if keep(&p) {
            out.push(p);
        }
    }
}

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
    let valued = ["--from", "--last", "--context", "--sheet", "--extends", "--kinds", "--tags", "--why", "--seconds", "--plan", "--brand", "--say", "--script", "--shape", "--group", "--brief"];
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
        Some("follow") => follow::verb(rest, eval),
        Some("kit") => kit::verb(rest),
        Some("docs") => raea_app_videoform_docs::verb(&plain(rest)).unwrap_or_else(|| (format!("usage: {USAGE}"), 2)),
        Some("rights") => raea_io_resolve::rights::verb(rest),
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
