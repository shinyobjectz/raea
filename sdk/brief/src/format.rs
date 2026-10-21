//! The .brief format (format.md): a Fountain profile with one reading for every line, read into the intent graph.
//!
//!   parse     the text into a Doc (each element as written, with where it stands), or every refusal
//!   graph     a Doc, with its `Uses:` applied and its variables filled in, as the JSON the compiler reads
//!   fmt       a Doc back as text, in one layout (parse(fmt(d)) reads the same as d)
//!   convert   a score script (script.fountain, read as ai/score/compiler/front.py reads it) as a .brief: what the
//!             notation defines kept, Fountain action as open beats, @must as checks, and whatever neither reads
//!             kept as `//` comments, so nothing the writer wrote is lost

pub mod ink;

use regex::Regex;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub const KINDS: [&str; 5] = ["to-camera", "b-roll", "product", "screen", "end-card"];
pub const FINDS: [&str; 9] = ["face", "speech", "product", "hands", "screen", "text", "place", "action", "logo"];
/// facts of a moment a find or a check can test, beyond the nine words (performance, motion, light, read-text)
const FACTS: [&str; 6] = ["speaking", "eye-contact", "still", "moving", "bright", "no-text"];
pub const MUSTS: [&str; 7] = ["the hook reads within half a second", "nothing covers the face", "captions read on a phone",
    "everything inside the safe zone", "ends on a word, never mid-word", "no black frames", "the length is within the format"];
const META: [&str; 7] = ["Mode", "Angle", "Hook", "Refuses", "Credit", "Author", "Draft date"];
const PICTURE: [&str; 5] = [".png", ".jpg", ".jpeg", ".webp", ".heic"];
/// the stylesheet the compiler reads (ai/score/compiler/sheet.py CSS_PROPS, by selector)
const STYLE: [(&str, &[&str]); 2] = [(":root", &["background", "music", "music-gain", "sfx-gain", "platform", "eyes"]), ("captions", &["place", "keys"])];

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}
// a utility the compiler implements (ai/score/compiler/front.py UTIL)
re!(UTIL, r"^(on:'(?:[^']|'\w)+'|until:'(?:[^']|'\w)+'|to-end|place-(top|low|center|full|auto)|punch-in:\d+(\.\d+)?|enter-(fade|rise|slide|pop|bounce)|exit-fade|settle|dur-(sm|md|lg)|ease-(standard|emphasized|decelerate|accelerate)|stagger-after:\w+|sfx:'(?:[^']|'\w)+'|sfx-gain:-?\d+(\.\d+)?|claim:\w+)$");
re!(TOK, r"[\w-]+:'(?:[^']|'\w)*'|\S+");
re!(NOTE_PART, r#"^\s*([A-Za-z_][\w-]*)((?:\s+\w+:(?:"[^"]*"|\[[^\]]*\]|-?\d+(?:\.\d+)?))*)\s*(\|(.*))?$"#);
re!(PROP, r#"(\w+):("[^"]*"|\[[^\]]*\]|-?\d+(?:\.\d+)?)"#);
re!(NOTE, r"\[\[(.*?)\]\]");
re!(VARIANT, r"\s*\{\s*((?:\d+:\d+|[A-Za-z]\w*)(?:\s*,\s*(?:\d+:\d+|[A-Za-z]\w*))*)\s*\}\s*$");
re!(KEY, r"^(\$[A-Za-z_][\w.]*|[A-Z][A-Za-z ]*):\s*(.*)$");
re!(CLAIM, r#"^"([^"]+)"(?:\s+source\s+"([^"]+)")?$"#);
re!(PACE, r"(?i)^(no shot longer than|the hook ends by|a cut at least every)\s+(\d+(?:\.\d+)?)\s*s(?:ec(?:ond)?s?)?$");
re!(PUNCH, r"(?i)^PUNCH IN(?:\s+(\d+(?:\.\d+)?)X?)?[:.]?$");
re!(TRANSITION, r"^(CUT TO|SMASH CUT TO|MATCH CUT TO|PUNCH OUT|FADE OUT|FADE IN|[A-Z ]+ TO)[:.]?$");
re!(CUE, r"^[A-Z][A-Z0-9 .'()-]+$");
re!(BEAT, r"^~\s*([A-Za-z-]+)\s*:\s*(.*?)\s*(?:\[find:\s*([^\]]*)\])?\s*$");
re!(VAR, r"\$([A-Za-z_]\w*)");
re!(SECS, r"(?i)(\d+)(?:\s*(?:to|-)\s*(\d+))?\s*s(?:ec|econds?)?\b");
re!(RATIO, r"(\d+):(\d+)");
re!(NEVER, r#"^never says ((?:"[^"]+"[\s,]*(?:or\s+|and\s+)?)+)$"#);
re!(QUOTED, r#""([^"]+)""#);
re!(LENGTH, r"(?i)^(?:the )?length (?:is )?within (?:the format|(\d+(?:\.\d+)?) ?s(?:ec(?:ond)?s?)?)$");
re!(FENCE, r"(?s)```[\w-]*\n(.*?)```");

#[derive(Debug, Clone)]
pub struct Refusal {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

#[derive(Debug, Clone, Default)]
pub struct Note {
    pub kind: Option<String>,
    pub props: Vec<(String, String)>,
    /// each prop as written: "string" ("..."), "number" or "array" ([...]), for the component's types
    pub types: Vec<(String, &'static str)>,
    pub utils: Vec<String>,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub enum El {
    Section { depth: usize, name: String, variants: Vec<String> },
    Synopsis(String),
    Take { take: String, heading: String },
    Speaker(String),
    Delivery(String),
    Punch(Option<f64>),
    Transition(String),
    Screen { text: String, notes: Vec<Note> },
    Beat { kind: String, phrase: String, find: Vec<String>, variants: Vec<String> },
    Check { rule: String },
    Line { text: String, notes: Vec<Note>, variants: Vec<String> },
    Comment(String),
    Blank,
}

#[derive(Debug, Clone, Default)]
pub struct Doc {
    /// comments above and among the title page's lines, kept for fmt
    pub head: Vec<String>,
    pub title: Vec<(String, String, usize)>,
    pub body: Vec<(El, usize, usize)>,
    pub style: Vec<(String, Vec<(String, String, usize, usize)>, usize)>,
}

fn col_of(raw: &str) -> usize {
    raw.chars().take_while(|c| c.is_whitespace()).count() + 1
}

/// char column (1-based) of byte offset `b` in `s`
fn ccol(s: &str, b: usize) -> usize {
    s[..b.min(s.len())].chars().count() + 1
}

fn toks(s: &str) -> Vec<String> {
    TOK.find_iter(s).map(|m| m.as_str().to_string()).collect()
}

pub fn strip_fence(t: &str) -> &str {
    if t.trim_start().starts_with("```") {
        if let Some(c) = FENCE.captures(t) {
            return c.get(1).unwrap().as_str();
        }
    }
    t
}

/// one `[[ ... ]]` (its inside, starting at char column `col` on line `ln`), as ai/score/compiler/front.py parse_note
/// reads it; every utility the compiler doesn't implement refused at its column
fn note(inner: &str, ln: usize, col: usize, bad: &mut Vec<Refusal>) -> Note {
    let mut n = Note { raw: inner.to_string(), ..Default::default() };
    // a prop's text left open reads everything after it as words: say that once, not each word it swallowed (the note is
    // still read as score's reader read it, so convert keeps what that reader kept)
    let open_quote = inner.matches('"').count() % 2 == 1;
    if open_quote {
        let at = inner.rfind('"').unwrap();
        bad.push(Refusal { line: ln, col: col + ccol(inner, at) - 1, msg: "a prop's \"...\" isn't closed: end its text with \" before the `|`".into() });
    }
    let ts = toks(inner);
    let first = ts.first().cloned().unwrap_or_default();
    let mut util_src = inner.to_string();
    let mut util_off = 0;
    match NOTE_PART.captures(inner) {
        Some(c) if !UTIL.is_match(&first) && first != "|" => {
            let mut kind = c[1].to_string();
            let caps: Vec<_> = PROP.captures_iter(c.get(2).unwrap().as_str()).collect();
            n.types = caps.iter().map(|p| (p[1].to_string(), if p[2].starts_with('"') { "string" } else if p[2].starts_with('[') { "array" } else { "number" })).collect();
            let mut props: Vec<(String, String)> = caps.iter().map(|p| (p[1].to_string(), p[2].trim_matches('"').to_string())).collect();
            if kind == "component" {
                match props.iter().position(|(k, _)| k == "name") {
                    Some(i) => kind = props.remove(i).1,
                    None => bad.push(Refusal { line: ln, col, msg: "`component` without name:\"...\": name the component".into() }),
                }
            }
            n.kind = Some(kind);
            n.props = props;
            match c.get(4) {
                Some(u) => { util_src = u.as_str().to_string(); util_off = u.start(); n.utils = toks(u.as_str()); }
                None => { util_src.clear(); }
            }
        }
        _ => n.utils = ts.into_iter().filter(|x| x != "|").collect(),
    }
    let mut from = 0;
    for u in &n.utils {
        let at = util_src[from..].find(u.as_str()).map(|i| i + from).unwrap_or(0);
        from = at + u.len();
        if !UTIL.is_match(u) && !open_quote {
            let hint = if n.kind.is_none() && !u.contains(':') && n.utils.first() == Some(u) {
                format!("; a part is `[[{u} prop:\"...\" | utilities]]`")
            } else { String::new() };
            bad.push(Refusal { line: ln, col: col + ccol(inner, util_off + at) - 1, msg: format!("`{u}` isn't a utility (timing, place-*, enter-*/exit-fade/settle, dur-*, ease-*, punch-in:N, sfx:'...', sfx-gain:N){hint}") });
        }
    }
    n
}

/// a body line's notes and the text left, the notes refused where a note can't stand
fn notes_of(s: &str, ln: usize, col0: usize, bad: &mut Vec<Refusal>) -> (String, Vec<Note>) {
    let mut ns = Vec::new();
    for c in NOTE.captures_iter(s) {
        let m = c.get(1).unwrap();
        ns.push(note(m.as_str(), ln, col0 + ccol(s, m.start()) - 1, bad));
    }
    if s.contains("[[") && !s.contains("]]") {
        bad.push(Refusal { line: ln, col: col0 + ccol(s, s.find("[[").unwrap()) - 1, msg: "`[[` without `]]`".into() });
    }
    (NOTE.replace_all(s, "").trim().to_string(), ns)
}

fn variants_of(s: &str) -> (String, Vec<String>) {
    match VARIANT.captures(s) {
        Some(c) => (s[..c.get(0).unwrap().start()].trim_end().to_string(), c[1].split(',').map(|x| x.trim().to_string()).collect()),
        None => (s.to_string(), vec![]),
    }
}

fn find_ok(f: &str) -> bool {
    FINDS.contains(&f) || FACTS.contains(&f) || f.starts_with('$')
        || f.strip_prefix("people:").is_some_and(|n| n.parse::<u32>().is_ok() || n.split_once('-').is_some_and(|(a, b)| a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok()))
        || f.strip_prefix("framing:").is_some_and(|x| ["close-up", "medium", "wide"].contains(&x))
}

fn check_form(rule: &str) -> Option<(String, Vec<String>)> {
    let r = rule.trim().trim_end_matches('.');
    if let Some(c) = NEVER.captures(r) {
        return Some(("never-says".into(), QUOTED.captures_iter(&c[1]).map(|q| q[1].to_string()).collect()));
    }
    // a must as listed, with or without its "the"; the length also as `length within 30 s`
    let bare = |x: &str| x.trim_start_matches("the ").to_lowercase();
    if let Some(m) = MUSTS.iter().find(|m| bare(r) == bare(m)) {
        return Some(("must".into(), vec![m.to_string()]));
    }
    if let Some(c) = LENGTH.captures(r) {
        return Some(("must".into(), [MUSTS[6].to_string()].into_iter().chain(c.get(1).map(|x| x.as_str().to_string())).collect()));
    }
    if bare(r) == "claims sourced" || bare(r) == "every claim has a source" {
        return Some(("claims".into(), vec![]));
    }
    if let Some(c) = PACE.captures(r) {
        let form = match c[1].to_lowercase().as_str() { "no shot longer than" => "max-shot", "the hook ends by" => "hook-by", _ => "cut-every" };
        return Some((form.into(), vec![c[2].to_string()]));
    }
    if let Some((a, b)) = r.split_once("->") {
        let (a, b) = (a.trim(), b.trim());
        if find_ok(a) && find_ok(b) && !a.starts_with('$') && !b.starts_with('$') {
            return Some(("implies".into(), vec![a.into(), b.into()]));
        }
    }
    None
}

/// the text as a Doc, or every refusal in it
pub fn parse(text: &str) -> Result<Doc, Vec<Refusal>> {
    let text = strip_fence(text);
    let lines: Vec<&str> = text.lines().collect();
    let split = lines.iter().position(|l| l.trim_end() == "---").unwrap_or(lines.len());
    let mut doc = Doc::default();
    let mut bad = Vec::new();
    // the title page: Key: value lines from the top to the first blank line
    let mut i = 0;
    while i < split && lines[i].trim().is_empty() { i += 1; }
    while i < split {
        let l = lines[i].trim();
        if l.is_empty() && !doc.title.is_empty() { break; }
        if l.is_empty() || l.starts_with("//") {
            if let Some(c) = l.strip_prefix("//") { doc.head.push(c.trim().to_string()); }
            i += 1;
            continue;
        }
        let Some(c) = KEY.captures(l) else { break };
        let (k, v) = (c[1].to_string(), c[2].trim().to_string());
        if !(k.starts_with('$') || ["Title", "Format", "Uses"].contains(&k.as_str()) || META.contains(&k.as_str())) {
            bad.push(Refusal { line: i + 1, col: col_of(lines[i]), msg: format!("`{k}:` isn't a title-page key (Title, Format, Uses, Mode, Angle, Hook, Refuses, Credit, Author, Draft date, or a $variable)") });
        }
        doc.title.push((k, v, i + 1));
        i += 1;
    }
    let mut speaker = false;
    let mut pending_delivery: Option<(El, usize, usize)> = None;
    for (j, raw) in lines.iter().enumerate().take(split).skip(i) {
        let ln = j + 1;
        let col = col_of(raw);
        let l = raw.trim();
        let el = if l.is_empty() {
            speaker = false;
            El::Blank
        } else if let Some(c) = l.strip_prefix("//") {
            El::Comment(c.trim().to_string())
        } else if l.starts_with('#') {
            let depth = l.chars().take_while(|&c| c == '#').count();
            let (name, vs) = variants_of(l[depth..].trim());
            if name.contains("[[") { bad.push(Refusal { line: ln, col, msg: "a note can't stand on a section heading: style goes in the stylesheet".into() }); }
            if name.is_empty() { bad.push(Refusal { line: ln, col, msg: "a section needs a name".into() }); }
            if vs.iter().any(|v| RATIO.is_match(v)) { bad.push(Refusal { line: ln, col, msg: "a deliverable ({1:1}) tags lines and beats; a section heading takes a variant's name ({A})".into() }); }
            speaker = false;
            El::Section { depth, name, variants: vs }
        } else if let Some(s) = l.strip_prefix('=') {
            if s.contains("[[") { bad.push(Refusal { line: ln, col: col + l.find("[[").unwrap(), msg: "a note can't stand on a synopsis: style goes in the stylesheet".into() }); }
            El::Synopsis(NOTE.replace_all(s, "").trim().to_string())
        } else if l.starts_with('.') && l[1..].starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            let head = l[1..].to_string();
            let name: String = head.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
            let take = if name.eq_ignore_ascii_case("BROLL") {
                Regex::new(r"-\s*(\w+)").unwrap().captures(&head).map(|c| c[1].to_string()).unwrap_or_default()
            } else { name };
            if take.is_empty() { bad.push(Refusal { line: ln, col, msg: "a scene heading names a take: `.TAKE02 - TALKING HEAD`".into() }); }
            El::Take { take, heading: head }
        } else if let Some(c) = PUNCH.captures(l) {
            El::Punch(c.get(1).map(|m| m.as_str().parse().unwrap()))
        } else if TRANSITION.is_match(l) {
            El::Transition(l.to_string())
        } else if l.starts_with('@') || (CUE.is_match(l) && l.len() < 40 && !l.ends_with(':')) {
            speaker = true;
            El::Speaker(l.trim_start_matches('@').to_string())
        } else if l.starts_with('(') && l.ends_with(')') {
            El::Delivery(l[1..l.len() - 1].to_string())
        } else if l.starts_with('~') {
            match BEAT.captures(l) {
                Some(c) if KINDS.contains(&c[1].to_lowercase().as_str()) && !c[2].is_empty() => {
                    let (phrase, vs) = variants_of(&c[2]);
                    let find: Vec<String> = c.get(3).map(|f| f.as_str().split([',', ' ']).filter(|x| !x.is_empty()).map(String::from).collect()).unwrap_or_default();
                    for f in &find {
                        if !find_ok(f) {
                            bad.push(Refusal { line: ln, col: col + l.find(f.as_str()).unwrap_or(0), msg: format!("`{f}` isn't a find: face, speech, product, hands, screen, text, place, action, logo, people:N, speaking, eye-contact, still, moving, bright, no-text, framing:close-up|medium|wide, or a $picture") });
                        }
                    }
                    El::Beat { kind: c[1].to_lowercase(), phrase, find, variants: vs }
                }
                Some(c) if !KINDS.contains(&c[1].to_lowercase().as_str()) => {
                    bad.push(Refusal { line: ln, col: col + 1, msg: format!("`{}` isn't a beat's kind: to-camera, b-roll, product, screen or end-card", &c[1]) });
                    El::Blank
                }
                _ => {
                    bad.push(Refusal { line: ln, col, msg: "an open beat is `~ KIND: what happens [find: ...]`".into() });
                    El::Blank
                }
            }
        } else if l.starts_with('>') && NOTE.replace_all(l, "").trim().ends_with('<') {
            let (t, ns) = notes_of(l, ln, col, &mut bad);
            El::Screen { text: t[1..t.len() - 1].trim().to_string(), notes: ns }
        } else if let Some(r) = l.strip_prefix('!') {
            let rule = r.trim();
            if check_form(rule).is_none() {
                let hint = if rule.starts_with("Action") || rule.contains("[[") || rule.split_whitespace().count() > 4 && !rule.contains("->") {
                    format!("; a shot to find is an open beat: `~ b-roll: {}`", NOTE.replace_all(rule.trim_start_matches("Action").trim_start_matches(':').trim(), "").trim())
                } else { String::new() };
                let rule = NOTE.replace_all(rule, "").trim().to_string();
                bad.push(Refusal { line: ln, col: col + 1, msg: format!("`! {rule}` isn't a check: `never says \"...\"`, one of the musts ({}), or FACT -> FACT{hint}", MUSTS.join("; ")) });
            }
            El::Check { rule: rule.to_string() }
        } else if speaker {
            let (t, ns) = notes_of(l, ln, col, &mut bad);
            let (t, vs) = variants_of(&t);
            if t.is_empty() {
                // a note alone on its line: the line before it in this speaker's block takes it
                match doc.body.iter_mut().rev().take_while(|(e, _, _)| !matches!(e, El::Blank | El::Section { .. })).find(|(e, _, _)| matches!(e, El::Line { .. })) {
                    Some((El::Line { notes, .. }, _, _)) => { notes.extend(ns); continue; }
                    _ => { bad.push(Refusal { line: ln, col, msg: "a note stands after the line it's on, and there's no line before it".into() }); }
                }
            }
            El::Line { text: t, notes: ns, variants: vs }
        } else {
            let desc = NOTE.replace_all(l, "");
            bad.push(Refusal { line: ln, col, msg: format!("text that isn't under a speaker: put @SPEAKER before a quoted line, or write a shot as `~ b-roll: {}`", desc.trim()) });
            El::Blank
        };
        // a delivery written just before its speaker (`(measured)` then `@MAYA`) is that speaker's: held until the
        // speaker comes, refused if anything else does
        if let El::Delivery(_) = &el {
            if !speaker {
                if let Some((_, pl, pc)) = pending_delivery.take() { bad.push(Refusal { line: pl, col: pc, msg: "delivery stands under a speaker".into() }); }
                pending_delivery = Some((el, ln, col));
                continue;
            }
        }
        if let Some((d, pl, pc)) = pending_delivery.take() {
            if let El::Speaker(_) = &el {
                doc.body.push((el, ln, col));
                doc.body.push((d, pl, pc));
                continue;
            }
            bad.push(Refusal { line: pl, col: pc, msg: "delivery stands under a speaker (or just before one)".into() });
        }
        if matches!(el, El::Line { .. }) && !speaker {
            bad.push(Refusal { line: ln, col, msg: "delivery stands under a speaker".into() });
        }
        doc.body.push((el, ln, col));
    }
    // the stylesheet: SELECTOR { property: value; ... }, only what the compiler reads
    if split < lines.len() {
        let css: String = lines[split + 1..].join("\n");
        let base = split + 2;
        let rule = Regex::new(r"([^{}]+)\{([^}]*)\}").unwrap();
        let mut last = 0;
        for c in rule.captures_iter(&css) {
            let m0 = c.get(0).unwrap();
            let gap = css[last..m0.start()].trim();
            let pos = |b: usize| (base + css[..b].matches('\n').count(), b - css[..b].rfind('\n').map(|x| x + 1).unwrap_or(0) + 1);
            if !gap.is_empty() { let (l, cc) = pos(last); bad.push(Refusal { line: l, col: cc, msg: format!("`{gap}` isn't a stylesheet rule") }); }
            last = m0.end();
            let sel = c[1].trim().to_string();
            let sm = c.get(1).unwrap();
            let (sl, sc) = pos(sm.start() + (sm.as_str().len() - sm.as_str().trim_start().len()));
            let props = STYLE.iter().find(|(s, _)| *s == sel).map(|(_, p)| *p);
            if sel == "@must" {
                bad.push(Refusal { line: sl, col: sc, msg: "`@must` rules are `!` lines in the body: `! nothing covers the face`".into() });
                continue;
            } else if props.is_none() {
                bad.push(Refusal { line: sl, col: sc, msg: format!("`{sel}` isn't a selector the compiler reads (:root, captions); a part styles itself in its component") });
            }
            let body = c.get(2).unwrap();
            let mut decls = Vec::new();
            let mut off = body.start();
            for d in body.as_str().split(';') {
                let lead = d.len() - d.trim_start().len();
                let (dl, dc) = pos(off + lead);
                off += d.len() + 1;
                let d = d.trim();
                if d.is_empty() { continue; }
                match d.split_once(':') {
                    Some((k, v)) => {
                        let k = k.trim();
                        if props.is_some_and(|p| !p.contains(&k)) {
                            bad.push(Refusal { line: dl, col: dc, msg: format!("`{k}` isn't a property of {sel} the compiler reads ({})", props.unwrap().join(", ")) });
                        }
                        decls.push((k.to_string(), v.trim().to_string(), dl, dc));
                    }
                    None => bad.push(Refusal { line: dl, col: dc, msg: format!("`{d}` isn't `property: value`") }),
                }
            }
            doc.style.push((sel, decls, sl));
        }
        let rest = css[last..].trim();
        if !rest.is_empty() {
            let b = css.rfind(rest).unwrap_or(0);
            bad.push(Refusal { line: base + css[..b].matches('\n').count(), col: 1, msg: format!("`{}` isn't a stylesheet rule", rest.lines().next().unwrap_or("")) });
        }
    }
    if bad.is_empty() { Ok(doc) } else { Err(bad) }
}

pub fn format_secs(f: &str) -> Option<(f64, f64)> {
    let c = SECS.captures(f)?;
    let a: f64 = c[1].parse().ok()?;
    Some(match c.get(2) { Some(b) => (a, b.as_str().parse().ok()?), None => (a * 0.85, a * 1.15) })
}

fn platforms(f: &str) -> Vec<&'static str> {
    let f = f.to_lowercase();
    ["tiktok", "reels", "shorts"].into_iter()
        .filter(|p| f.contains(p) || (*p == "reels" && f.contains("instagram")) || (*p == "shorts" && f.contains("youtube"))).collect()
}

fn title<'a>(d: &'a Doc, k: &str) -> Option<&'a str> {
    d.title.iter().find(|(x, _, _)| x == k).map(|(_, v, _)| v.as_str())
}

pub struct Loaded {
    pub file: PathBuf,
    pub doc: Doc,
}

/// a .brief and every brief its `Uses:` names, depth first, each once; a cycle or a file that won't read refused
pub fn load(file: &Path, seen: &mut Vec<PathBuf>, out: &mut Vec<Loaded>) -> Result<(), Vec<Refusal>> {
    load_text(file, None, seen, out)
}

/// `load`, with the file's text given (an ink story lowered to .brief, ink::lower) instead of read from disk
pub fn load_text(file: &Path, given: Option<&str>, seen: &mut Vec<PathBuf>, out: &mut Vec<Loaded>) -> Result<(), Vec<Refusal>> {
    let file = file.canonicalize().map_err(|e| vec![Refusal { line: 0, col: 0, msg: format!("{}: {e}", file.display()) }])?;
    if seen.contains(&file) {
        return Err(vec![Refusal { line: 0, col: 0, msg: format!("`Uses:` comes back to {}", file.display()) }]);
    }
    if out.iter().any(|l| l.file == file) { return Ok(()); }
    let text = match given { Some(t) => t.to_string(), None => std::fs::read_to_string(&file).map_err(|e| vec![Refusal { line: 0, col: 0, msg: format!("{}: {e}", file.display()) }])? };
    let doc = parse(&text).map_err(|bad| bad.into_iter().map(|r| Refusal { msg: format!("{}:{}:{}: {}", file.display(), r.line, r.col, r.msg), ..r }).collect::<Vec<_>>())?;
    seen.push(file.clone());
    if let Some((_, uses, ln)) = doc.title.iter().find(|(k, _, _)| k == "Uses") {
        for u in uses.split(',').map(str::trim).filter(|u| !u.is_empty()) {
            let p = file.parent().unwrap().join(u);
            if !p.exists() {
                return Err(vec![Refusal { line: *ln, col: 1, msg: format!("{}:{ln}:1: `Uses: {u}`: no such brief", file.display()) }]);
            }
            load(&p, seen, out)?;
            if let Some(l) = out.iter().find(|l| l.file == p.canonicalize().unwrap()) {
                if title(&l.doc, "Format").is_some() {
                    return Err(vec![Refusal { line: *ln, col: 1, msg: format!("{}:{ln}:1: `Uses: {u}` names a piece (it has a Format:); a piece can only use shared briefs", file.display()) }]);
                }
            }
        }
    }
    seen.pop();
    out.push(Loaded { file, doc });
    Ok(())
}

fn note_json(n: &Note, fill: &dyn Fn(&str) -> String) -> Value {
    let anchor = n.utils.iter().find_map(|u| u.strip_prefix("on:'").map(|x| x.trim_end_matches('\'').to_string()));
    let props: Map<String, Value> = n.props.iter().map(|(k, v)| (k.clone(), Value::String(fill(v)))).collect();
    let types: Map<String, Value> = n.types.iter().map(|(k, t)| (k.clone(), json!(t))).collect();
    json!({"kind": n.kind, "props": props, "types": types, "utils": n.utils, "anchor": anchor})
}

/// a graph's parts against the space's components (their props as TypeScript types them, @raea/data/props.ts):
/// an unknown prop, a missing required one, a value of the wrong kind or outside a string union, each refused where its
/// line stands. Props the compiler reads itself (captions' keys) aren't the component's.
pub fn check_props(g: &Value, schema: &Value) -> Vec<Refusal> {
    let mut bad = Vec::new();
    for it in g["items"].as_array().into_iter().flatten() {
        let (ln, col) = (it["at"][0].as_u64().unwrap_or(0) as usize, it["at"][1].as_u64().unwrap_or(1) as usize);
        for n in it["notes"].as_array().into_iter().flatten() {
            let Some(kind) = n["kind"].as_str() else { continue };
            let Some(props) = schema.get(kind).and_then(|p| p.as_object()) else { continue };
            let given = n["props"].as_object().cloned().unwrap_or_default();
            let names: Vec<&str> = props.keys().map(|k| k.as_str()).collect();
            for (k, v) in &given {
                if kind == "captions" && k == "keys" { continue; }
                let Some(p) = props.get(k) else {
                    bad.push(Refusal { line: ln, col, msg: format!("`{k}` isn't a prop of {kind} (its props: {})", names.join(", ")) });
                    continue;
                };
                let want = p["kind"].as_str().unwrap_or("any");
                let got = n["types"][k].as_str().unwrap_or("string");
                let fits = match want { "any" | "object" => true, "boolean" => got == "string" && matches!(v.as_str(), Some("true" | "false")) || got == "number", w => w == got };
                if !fits {
                    let how = match want { "string" => format!("{k}:\"{}\"", v.as_str().unwrap_or("")), "number" => format!("{k}:{}", v.as_str().unwrap_or("0")), "array" => format!("{k}:[...]"), _ => format!("{k}:\"true\"") };
                    bad.push(Refusal { line: ln, col, msg: format!("{kind}'s {k} is a {want}, written as a {got}: {how}") });
                } else if let Some(vals) = p["values"].as_array() {
                    let vs: Vec<&str> = vals.iter().filter_map(|x| x.as_str()).collect();
                    if !vs.contains(&v.as_str().unwrap_or("")) {
                        bad.push(Refusal { line: ln, col, msg: format!("{kind}'s {k} is one of {}, not \"{}\"", vs.join(", "), v.as_str().unwrap_or("")) });
                    }
                }
            }
            for (k, p) in props {
                if p["required"].as_bool() == Some(true) && !given.contains_key(k) {
                    bad.push(Refusal { line: ln, col, msg: format!("{kind} needs {k} (a {}): {kind} {k}:\"...\"", p["kind"].as_str().unwrap_or("value")) });
                }
            }
        }
    }
    bad
}

/// the graph of the last of `docs` (a piece or a shared brief), the ones before it the briefs it uses, in order
pub fn graph(docs: &[Loaded]) -> Result<Value, Vec<Refusal>> {
    let me = docs.last().unwrap();
    let d = &me.doc;
    let mut bad = Vec::new();
    // variables: each used brief's in order, this one's last; a later value wins
    let mut vars: Vec<(String, String)> = Vec::new();
    for l in docs {
        for (k, v, _) in l.doc.title.iter().filter(|(k, _, _)| k.starts_with('$') && !k.starts_with("$claim.")) {
            vars.retain(|(x, _)| x != &k[1..]);
            vars.push((k[1..].to_string(), v.clone()));
        }
    }
    // claims: `$claim.ID: "what it says" source "where it's proven"`, each used brief's in order, this one's last
    let mut claims = Map::new();
    for l in docs {
        for (k, v, ln) in l.doc.title.iter().filter(|(k, _, _)| k.starts_with("$claim.")) {
            match CLAIM.captures(v.trim()) {
                Some(c) => { claims.insert(k["$claim.".len()..].to_string(), json!({"text": &c[1], "source": c.get(2).map(|x| x.as_str()), "from": l.file})); }
                None => bad.push(Refusal { line: *ln, col: 1, msg: format!("a claim is `{k}: \"what it says\" source \"where it's proven\"` (the source may wait, and `! claims sourced` will say so)") }),
            }
        }
    }
    let is_pic = |v: &str| PICTURE.iter().any(|e| v.to_lowercase().ends_with(e));
    let value = |name: &str| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
    let undefined = std::cell::RefCell::new(Vec::<String>::new());
    let fill = |s: &str| -> String {
        VAR.replace_all(s, |c: &regex::Captures| match value(&c[1]) {
            Some(v) => v,
            None => { undefined.borrow_mut().push(c[1].to_string()); c[0].to_string() }
        }).to_string()
    };
    let format = title(d, "Format").map(String::from);
    let deliverables: Vec<Value> = format.as_deref().map(|f| f.split('|').map(|x| {
        let x = x.trim();
        json!({"format": x, "ratio": RATIO.captures(x).map(|c| format!("{}:{}", &c[1], &c[2])), "secs": format_secs(x).map(|(a, b)| vec![a, b]), "platforms": platforms(x)})
    }).collect()).unwrap_or_default();
    let ratios: Vec<String> = deliverables.iter().filter_map(|x| x["ratio"].as_str().map(String::from)).collect();
    let mut sections = Vec::new();
    let mut items = Vec::new();
    let (mut section, mut take, mut speaker, mut delivery): (Option<String>, Option<String>, Option<String>, Option<String>) = (None, None, None, None);
    let mut punch = Value::Null;
    let mut checks = Vec::new();
    for l in &docs[..docs.len() - 1] {
        for (el, ln, _) in &l.doc.body {
            if let El::Check { rule } = el {
                let (form, args) = check_form(&fill(rule)).unwrap();
                checks.push(json!({"rule": fill(rule), "form": form, "args": args, "from": l.file, "at": [ln, 1]}));
            }
        }
    }
    let asks: Vec<Value> = docs[..docs.len() - 1].iter().flat_map(|l| l.doc.body.iter().filter_map(|(el, ln, _)| match el {
        El::Beat { kind, phrase, find, .. } => Some(json!({"kind": kind, "phrase": fill(phrase), "find": find, "from": l.file, "at": [ln, 1]})),
        _ => None,
    })).collect();
    let tag_ok = |vs: &Vec<String>, ln: usize, col: usize, bad: &mut Vec<Refusal>| {
        for v in vs.iter().filter(|v| RATIO.is_match(v)) {
            if !ratios.contains(v) { bad.push(Refusal { line: ln, col, msg: format!("{{{v}}}: no deliverable in Format: is {v} ({})", ratios.join(", ")) }); }
        }
    };
    // a tag is a deliverable (a ratio) or a variant's name; an element is in its own variants, else its section's
    let split = |vs: &Vec<String>| -> (Vec<String>, Vec<String>) { vs.iter().cloned().partition(|v| RATIO.is_match(v)) };
    let mut section_variants: Vec<String> = Vec::new();
    let mut variant_names: Vec<String> = Vec::new();
    let claim_refs = |notes: &Vec<Note>| -> Vec<String> { notes.iter().flat_map(|n| n.utils.iter().filter_map(|u| u.strip_prefix("claim:").map(String::from))).collect() };
    for (el, ln, col) in &d.body {
        let at = json!([ln, col]);
        match el {
            El::Blank => { speaker = None; delivery = None; }
            El::Section { depth, name, variants } => {
                section = Some(name.clone());
                section_variants = variants.clone();
                for v in variants { if !variant_names.contains(v) { variant_names.push(v.clone()); } }
                sections.push(json!({"name": name, "depth": depth, "synopsis": Value::Null, "variant": variants, "at": at}));
            }
            El::Synopsis(s) => { if let Some(x) = sections.last_mut() { x["synopsis"] = json!(s); } }
            El::Take { take: t, .. } => take = Some(t.clone()),
            El::Speaker(s) => speaker = Some(s.clone()),
            El::Delivery(s) => delivery = Some(s.clone()),
            El::Punch(p) => punch = p.map(|x| json!(x)).unwrap_or(json!("unset")),
            El::Transition(t) => items.push(json!({"type": "transition", "text": t, "at": at})),
            El::Comment(_) => {}
            El::Screen { text, notes } => items.push(json!({"type": "screen", "section": section, "text": fill(text), "notes": notes.iter().map(|n| note_json(n, &fill)).collect::<Vec<_>>(), "at": at})),
            El::Beat { kind, phrase, find, variants } => {
                tag_ok(variants, *ln, *col, &mut bad);
                for f in find.iter().filter(|f| f.starts_with('$')) {
                    match value(&f[1..]) {
                        Some(v) if is_pic(&v) => {}
                        Some(_) => bad.push(Refusal { line: *ln, col: *col, msg: format!("{f} is text, not a picture: a find takes a picture variable") }),
                        None => undefined.borrow_mut().push(f[1..].to_string()),
                    }
                }
                let find: Vec<Value> = find.iter().map(|f| if let Some(v) = f.strip_prefix('$') { json!({"picture": value(v)}) } else { json!(f) }).collect();
                let (only, names) = split(variants);
                for v in &names { if !variant_names.contains(v) { variant_names.push(v.clone()); } }
                let variant = if names.is_empty() { section_variants.clone() } else { names };
                items.push(json!({"type": "beat", "section": section, "take": take, "kind": kind, "phrase": fill(phrase), "find": find, "variants": only, "variant": variant, "at": at}));
            }
            El::Check { rule } => {
                let r = fill(rule);
                let (form, args) = check_form(&r).unwrap();
                checks.push(json!({"rule": r, "form": form, "args": args, "from": me.file, "at": at}));
            }
            El::Line { text, notes, variants } => {
                tag_ok(variants, *ln, *col, &mut bad);
                let (only, names) = split(variants);
                for v in &names { if !variant_names.contains(v) { variant_names.push(v.clone()); } }
                let variant = if names.is_empty() { section_variants.clone() } else { names };
                // `first read || second read`: alternatives the compiler picks from; `text` is the first
                let alts: Vec<String> = fill(text).split(" || ").map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
                let refs = claim_refs(notes);
                for r in &refs {
                    if !claims.contains_key(r) { bad.push(Refusal { line: *ln, col: *col, msg: format!("claim:{r} isn't a claim (a `$claim.{r}: \"...\" source \"...\"` line on the title page, here or in a brief this uses)") }); }
                }
                items.push(json!({"type": "line", "section": section, "take": take, "speaker": speaker, "delivery": delivery, "punch": punch,
                    "text": alts.first().cloned().unwrap_or_default(), "alts": if alts.len() > 1 { json!(alts) } else { Value::Null },
                    "notes": notes.iter().map(|n| note_json(n, &fill)).collect::<Vec<_>>(), "claims": refs, "variants": only, "variant": variant, "at": at}));
                punch = Value::Null;
            }
        }
        for u in undefined.borrow_mut().drain(..) {
            bad.push(Refusal { line: *ln, col: *col, msg: format!("${u} isn't defined (a `${u}: value` line on the title page, here or in a brief this uses)") });
        }
    }
    let mut style = Map::new();
    for (sel, decls, _) in &d.style {
        let e = style.entry(sel.clone()).or_insert_with(|| json!({}));
        for (k, v, _, _) in decls { e[k] = json!(fill(v)); }
    }
    for u in undefined.borrow_mut().drain(..) {
        bad.push(Refusal { line: 0, col: 0, msg: format!("${u} isn't defined") });
    }
    if !bad.is_empty() { return Err(bad); }
    let meta: Map<String, Value> = d.title.iter().filter(|(k, _, _)| META.contains(&k.as_str())).map(|(k, v, _)| (k.clone(), json!(fill(v)))).collect();
    Ok(json!({
        "file": me.file, "kind": if format.is_some() { "piece" } else { "shared" },
        "title": title(d, "Title").map(|t| fill(t)), "format": format, "deliverables": deliverables,
        "uses": docs[..docs.len() - 1].iter().map(|l| &l.file).collect::<Vec<_>>(), "meta": meta,
        "vars": vars.iter().map(|(k, v)| (k.clone(), json!({"value": v, "picture": is_pic(v)}))).collect::<Map<_, _>>(),
        "sections": sections, "items": items, "checks": checks, "asks": asks, "style": style, "claims": claims, "variant_names": variant_names,
    }))
}

fn note_text(n: &Note) -> String {
    let mut s = String::new();
    if let Some(k) = &n.kind {
        s.push_str(k);
        for (p, v) in &n.props {
            if v.starts_with('[') || v.parse::<f64>().is_ok() { s.push_str(&format!(" {p}:{v}")); } else { s.push_str(&format!(" {p}:\"{v}\"")); }
        }
        if !n.utils.is_empty() { s.push_str(" | "); }
    }
    s.push_str(&n.utils.join(" "));
    // a list prop last would close as `]]]`, which reads one bracket early
    if s.ends_with(']') { s.push(' '); }
    format!("[[{s}]]")
}

fn tagged(s: String, vs: &[String]) -> String {
    if vs.is_empty() { s } else { format!("{s} {{{}}}", vs.join(", ")) }
}

/// the Doc as text in one layout: the title page, then the body with one blank line before each section and none
/// doubled, then the stylesheet one rule to a line
pub fn fmt(d: &Doc) -> String {
    let mut out: Vec<String> = d.head.iter().map(|c| format!("// {c}")).collect();
    out.extend(d.title.iter().map(|(k, v, _)| format!("{k}: {v}")));
    out.push(String::new());
    for (el, _, _) in &d.body {
        let s = match el {
            El::Blank => String::new(),
            El::Comment(c) => format!("// {c}"),
            El::Section { depth, name, variants } => { if out.last().is_some_and(|l| !l.is_empty()) { out.push(String::new()); } tagged(format!("{} {name}", "#".repeat(*depth)), variants) }
            El::Synopsis(s) => format!("= {s}"),
            El::Take { heading, .. } => format!(".{heading}"),
            El::Speaker(s) => format!("@{s}"),
            El::Delivery(s) => format!("({s})"),
            El::Punch(p) => p.map(|x| format!("PUNCH IN {x}X:")).unwrap_or("PUNCH IN:".into()),
            El::Transition(t) => t.clone(),
            El::Screen { text, notes } => [format!(">{text}<")].into_iter().chain(notes.iter().map(note_text)).collect::<Vec<_>>().join(" "),
            El::Beat { kind, phrase, find, variants } => tagged(if find.is_empty() { format!("~ {kind}: {phrase}") } else { format!("~ {kind}: {phrase} [find: {}]", find.join(" ")) }, variants),
            El::Check { rule } => format!("! {rule}"),
            El::Line { text, notes, variants } => tagged([text.clone()].into_iter().chain(notes.iter().map(note_text)).collect::<Vec<_>>().join(" "), variants),
        };
        if s.is_empty() && out.last().is_some_and(|l| l.is_empty()) { continue; }
        out.push(s);
    }
    while out.last().is_some_and(|l| l.is_empty()) { out.pop(); }
    if !d.style.is_empty() {
        out.push("---".into());
        for (sel, decls, _) in &d.style {
            out.push(format!("{sel} {{ {} }}", decls.iter().map(|(k, v, _, _)| format!("{k}: {v};")).collect::<Vec<_>>().join(" ")));
        }
    }
    out.join("\n") + "\n"
}

/// a score script (script.fountain) as a .brief. What the notation defines is kept as the compiler read it; Fountain
/// action (`!Action ...`, a description with no speaker) becomes an open beat; `@must { ... }` becomes `!` lines;
/// title keys, utilities, props and style the notation doesn't read become `//` comments beside where they stood
pub fn convert(fountain: &str) -> String {
    // as front.py reads a model's answer: the first fenced block anywhere in it, else the text without a fence opened
    // and never closed
    let text = match FENCE.captures(fountain) {
        Some(c) => c.get(1).unwrap().as_str(),
        None => fountain.trim_start().strip_prefix("```").map(|t| t.split_once('\n').map(|x| x.1).unwrap_or("")).unwrap_or(fountain),
    };
    let (body, css) = match text.split_once("\n---\n") { Some((a, b)) => (a, b), None => (text, "") };
    let mut out = Vec::new();
    let mut checks = Vec::new();
    let mut speaker = false;
    let mut in_title = true;
    let mut pending: Vec<String> = Vec::new();
    let mut last_line: Option<usize> = None;
    // what front.py reads from a note: the part and its props, its utilities; those it calls unknown go to a comment
    let keep_note = |inner: &str, dropped: &mut Vec<String>| -> String {
        let mut bad = Vec::new();
        let n = note(inner, 0, 1, &mut bad);
        let mut n2 = n.clone();
        n2.utils.retain(|u| UTIL.is_match(u));
        if n.utils.len() != n2.utils.len() {
            dropped.push(format!("[[{}]] had {}", inner.trim(), n.utils.iter().filter(|u| !UTIL.is_match(u)).cloned().collect::<Vec<_>>().join(" ")));
        }
        if n2.kind.is_none() && n2.utils.is_empty() { String::new() } else { note_text(&n2) }
    };
    for raw in body.lines() {
        let l = raw.trim();
        if in_title {
            if l.is_empty() { if !out.is_empty() { in_title = false; out.push(String::new()); } continue; }
            if let Some(c) = KEY.captures(l) {
                let k = &c[1];
                if k.starts_with('$') || ["Title", "Format", "Uses"].contains(&k) || META.contains(&k) { out.push(l.to_string()); } else { out.push(format!("// {l}")); }
                continue;
            }
            in_title = false;
            out.push(String::new());
        }
        if l.is_empty() {
            speaker = false;
            for p in pending.drain(..) { out.push(format!("// {p}")); }
            out.push(String::new());
            continue;
        }
        let mut dropped = Vec::new();
        // a note left open (`[[... sfx-pop]`): front.py reads it as words of the line; it goes to a comment here
        let l = &match (l.rfind("[["), l.rfind("]]")) {
            (Some(o), c) if c.is_none_or(|c| c < o) => { dropped.push(format!("a note left open: {}", &l[o..])); l[..o].trim_end().to_string() }
            _ => l.to_string(),
        };
        let notes: Vec<String> = NOTE.captures_iter(l).map(|c| keep_note(&c[1], &mut dropped)).filter(|s| !s.is_empty()).collect();
        let bare = NOTE.replace_all(l, "").trim().to_string();
        let with = |s: String, notes: &[String]| if notes.is_empty() { s } else { format!("{s} {}", notes.join(" ")) };
        if let Some(c) = KEY.captures(&bare).filter(|c| &c[1] == "Title" || &c[1] == "Format") {
            out.push(format!("// {} (a title-page key in the body)", &c[0]));
        } else if bare.starts_with('#') {
            out.push(bare.clone());
            if !notes.is_empty() { dropped.push(format!("on the heading: {}", notes.join(" "))); }
            speaker = false;
        } else if bare.starts_with('=') {
            out.push(bare.clone());
            if !notes.is_empty() { dropped.push(format!("on the synopsis: {}", notes.join(" "))); }
        } else if bare.starts_with('.') && bare[1..].starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            out.push(bare.clone());
        } else if PUNCH.is_match(&bare) || TRANSITION.is_match(&bare) || bare.starts_with('@') || (CUE.is_match(&bare) && bare.len() < 40 && !bare.ends_with(':')) {
            if bare.starts_with('@') || (CUE.is_match(&bare) && !PUNCH.is_match(&bare) && !TRANSITION.is_match(&bare)) { speaker = true; }
            out.push(bare.clone());
        } else if bare.starts_with('(') && bare.ends_with(')') {
            if speaker { out.push(bare.clone()); } else { out.push(format!("// {bare} (delivery with no speaker)")); }
        } else if bare.starts_with('~') {
            out.push(format!("// {l} (a lyric)"));
        } else if bare.starts_with('>') && bare.ends_with('<') {
            out.push(with(bare.clone(), &notes));
        } else if speaker && !bare.starts_with('!') {
            out.push(with(bare.clone(), &notes));
            last_line = Some(out.len() - 1);
        } else {
            // Fountain action: a shot the writer wants, which no line says; front.py moved its notes to the line before
            let desc = bare.trim_start_matches('!').trim_start_matches("Action").trim_start_matches(':').trim().replace(['[', ']'], "");
            if !notes.is_empty() {
                match last_line { Some(i) => { out[i].push(' '); out[i].push_str(&notes.join(" ")); } None => dropped.push(format!("before any line: {}", notes.join(" "))) }
            }
            if !desc.is_empty() { out.push(format!("~ b-roll: {desc}")); }
        }
        for x in dropped { pending.push(format!("not in the notation: {x}")); }
        if !speaker { for p in pending.drain(..) { out.push(format!("// {p}")); } }
    }
    for p in pending.drain(..) { out.push(format!("// {p}")); }
    // the stylesheet: the selectors and properties the compiler reads; @must as checks; the rest as comments
    let mut style = Vec::new();
    for c in Regex::new(r"([^{}]+)\{([^}]*)\}").unwrap().captures_iter(css) {
        let sel = c[1].trim();
        let decls: Vec<&str> = c[2].split(';').map(str::trim).filter(|d| !d.is_empty()).collect();
        if sel == "@must" {
            for d in decls {
                match check_form(d) { Some(_) => checks.push(format!("! {d}")), None => checks.push(format!("// must: {d} (not a check the runtime has)")) }
            }
            continue;
        }
        let props = STYLE.iter().find(|(s, _)| *s == sel).map(|(_, p)| *p);
        let (keep, drop): (Vec<&str>, Vec<&str>) = decls.into_iter().partition(|d| props.is_some_and(|p| d.split_once(':').is_some_and(|(k, _)| p.contains(&k.trim()))));
        if !keep.is_empty() { style.push(format!("{sel} {{ {} }}", keep.iter().map(|d| format!("{d};")).collect::<Vec<_>>().join(" "))); }
        if !drop.is_empty() { style.push(format!("// {sel} {{ {} }} (not read by the compiler)", drop.join("; "))); }
    }
    // checks stand after the title page
    let at = out.iter().position(|l| l.is_empty()).map(|i| i + 1).unwrap_or(out.len());
    if !checks.is_empty() { checks.push(String::new()); }
    for (i, c) in checks.into_iter().enumerate() { out.insert(at + i, c); }
    let mut s = out.join("\n");
    while s.contains("\n\n\n") { s = s.replace("\n\n\n", "\n\n"); }
    let mut s = s.trim_end().to_string() + "\n";
    if !style.is_empty() {
        // a comment can't stand in the stylesheet: move them above the ---
        let (cm, rules): (Vec<String>, Vec<String>) = style.into_iter().partition(|x| x.starts_with("//"));
        for c in cm { s.push_str(&c); s.push('\n'); }
        if !rules.is_empty() { s.push_str("---\n"); s.push_str(&rules.join("\n")); s.push('\n'); }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusals(t: &str) -> Vec<String> {
        parse(t).err().unwrap_or_default().into_iter().map(|r| format!("{}:{} {}", r.line, r.col, r.msg)).collect()
    }

    #[test]
    fn reads_the_example() {
        let t = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/format.md")).unwrap();
        let ex = t.split("```\n").nth(1).unwrap();
        let d = parse(ex).unwrap_or_else(|e| panic!("{e:?}"));
        let g = graph(&[Loaded { file: "x.brief".into(), doc: d.clone() }]).unwrap();
        assert_eq!(g["kind"], "piece");
        assert_eq!(g["deliverables"][1]["ratio"], "1:1");
        let lines: Vec<&Value> = g["items"].as_array().unwrap().iter().filter(|i| i["type"] == "line").collect();
        assert_eq!(lines[1]["text"], "Try Milky Jelly.");
        assert_eq!(lines[1]["variants"][0], "9:16");
        assert_eq!(lines[0]["notes"][0]["kind"], "captions");
        let beat = g["items"].as_array().unwrap().iter().find(|i| i["type"] == "beat").unwrap();
        assert_eq!(beat["find"][0]["picture"], "media/milky-jelly.png");
        assert_eq!(g["checks"][0]["form"], "never-says");
        // fmt reads back the same
        let again = parse(&fmt(&d)).unwrap();
        let g2 = graph(&[Loaded { file: "x.brief".into(), doc: again }]).unwrap();
        let strip = |mut g: Value| { for i in g["items"].as_array_mut().unwrap() { i["at"] = Value::Null; } for s in g["sections"].as_array_mut().unwrap() { s["at"] = Value::Null; } for c in g["checks"].as_array_mut().unwrap() { c["at"] = Value::Null; } g };
        assert_eq!(strip(g), strip(g2));
    }

    /// every fenced example in format.md that isn't ink or the graph's shape reads, with no refusal
    #[test]
    fn reads_every_example() {
        let t = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/format.md")).unwrap();
        // fenced blocks: a line starting ``` opens one (its language after it), a line that is ``` closes it
        let (mut blocks, mut cur): (Vec<(String, String)>, Option<(String, String)>) = (vec![], None);
        for l in t.lines() {
            match (&mut cur, l.strip_prefix("```")) {
                (None, Some(lang)) => cur = Some((lang.trim().to_string(), String::new())),
                (Some(_), Some(rest)) if rest.trim().is_empty() => blocks.push(cur.take().unwrap()),
                (Some((_, body)), _) => { body.push_str(l); body.push('\n'); }
                _ => {}
            }
        }
        let mut n = 0;
        for (lang, body) in &blocks {
            if !lang.is_empty() || body.trim_start().starts_with('{') { continue; }
            let d = parse(body).unwrap_or_else(|e| panic!("format.md example {}: {e:?}\n{body}", n + 1));
            graph(&[Loaded { file: "x.brief".into(), doc: d }]).unwrap_or_else(|e| panic!("format.md example {}: {e:?}", n + 1));
            n += 1;
        }
        assert!(n >= 4, "{n} examples");
    }

    #[test]
    fn reads_the_ink_example() {
        let t = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/format.md")).unwrap();
        let ex = t.split("```ink\n").nth(1).unwrap().split("```").next().unwrap();
        for (path, last) in [(vec![], "So this one's getting a thicker foot."), (vec![1], "Every crack tells you where the clay was thin.")] {
            let (text, _) = crate::ink::lower(ex, &path).unwrap_or_else(|e| panic!("{e:?}"));
            let d = parse(&text).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
            let g = graph(&[Loaded { file: "x.ink".into(), doc: d }]).unwrap();
            let lines: Vec<&Value> = g["items"].as_array().unwrap().iter().filter(|i| i["type"] == "line").collect();
            assert_eq!(lines[0]["text"], "That's forty hours, gone.");
            assert_eq!(lines[0]["section"], "the crack");
            assert_eq!(lines.last().unwrap()["text"], last);
            assert_eq!(g["checks"][0]["form"], "never-says");
            assert!(g["items"].as_array().unwrap().iter().any(|i| i["type"] == "beat"));
        }
    }

    #[test]
    fn refuses_with_where() {
        let r = refusals("Title: x\nFormat: 9:16, 20 s\n\n@JUNE\nHi there. [[lockup text:\"hi\" | on:'Hi' pops]]\n");
        assert_eq!(r.len(), 1);
        assert!(r[0].starts_with("5:40 `pops`"), "{r:?}");
        let r = refusals("Title: x\nFormat: 9:16, 20 s\n\n!Action The bracket clicks shut [[follow | place-low]]\n");
        assert!(r[0].contains("~ b-roll: The bracket clicks shut"), "{r:?}");
        let r = refusals("Title: x\nBuild goal: y\n\n# a\n");
        assert!(r[0].starts_with("2:1 `Build goal:`"), "{r:?}");
        let r = refusals("Title: x\n\n# a\nJust words.\n");
        assert!(r[0].starts_with("4:1 text that isn't under a speaker"), "{r:?}");
        let r = refusals("Title: x\n\n# a\n\n---\n:root { music: \"x\"; energy: f; }\n@must { no black frames; }\n");
        assert_eq!(r.len(), 2, "{r:?}");
        assert!(r[0].starts_with("6:21 `energy`"), "{r:?}");
        assert!(r[1].starts_with("7:1 `@must`"), "{r:?}");
        let r = refusals("Title: x\n\n~ drone: over the hills\n~ b-roll: a pour [find: hands sparkle]\n");
        assert!(r[0].starts_with("3:2 `drone`") && r[1].contains("`sparkle` isn't a find"), "{r:?}");
    }

    #[test]
    fn variables_and_variants() {
        let d = parse("Title: x\nFormat: 9:16, 20 s\n\n@A\nTry $thing. {1:1}\n").unwrap();
        let e = graph(&[Loaded { file: "x".into(), doc: d }]).unwrap_err();
        assert!(e.iter().any(|r| r.msg.contains("$thing isn't defined")) && e.iter().any(|r| r.msg.contains("{1:1}")), "{e:?}");
    }
}
