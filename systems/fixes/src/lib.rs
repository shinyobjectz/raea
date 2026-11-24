//! `raea fix SYMPTOM...`: a problem looked up before it's guessed at. Two sources, searched in this order:
//!
//! - raea's own entries, sdk/guides/fixes/raea.tsv in the checkout or an install's share/raea: problems found and fixed
//!   in raea work, each with where and when. Read whole into an in-memory FTS5 table on each call (a few dozen rows), so
//!   an edit to the file is live with no rebuild. An entry shows when it matches at least half the query's words.
//! - the library, models/fixes/fixes-library.db (RAEA_FIXES names another file): about 46,000 problem -> fix entries
//!   taken from community skills and docs whose licences allow it (MIT, Apache-2.0, BSD, CC0, and every ClawHub skill,
//!   which is MIT-0), merged where several sources say the same. Built outside this repository by skillbase's
//!   `troubles.sh release`, pinned in tools/release/models.tsv, packed by tools/release.sh. Read-only SQLite; its
//!   sources and their licences are in meta 'notice' (`raea fix --notice`).
//!
//! The query: "quoted text" is a phrase searched in problems and error text; other words are searched one by one, ORed,
//! after dropping stopwords and words found in more than 4% of the library's entries (they don't tell entries apart).
//! A word naming a domain (bevy, blender, three, ffmpeg, ...) doesn't match on its own but ranks that domain's entries
//! 2.5x. Ranking is BM25 over problem (x8), error text (x6), identifiers (x3), cause (x1.5), fix (x1) and other sources'
//! fixes (x0.5), nudged by support (how many sources state the fix).
use rusqlite::{Connection, OpenFlags, params};
use std::path::PathBuf;

#[cfg(test)]
mod tests;

pub const USAGE: &str = "raea fix SYMPTOM... [--domain DOMAIN] [--limit N] [--sources] [--notice]";
pub const ABOUT: &str = "a problem looked up before you guess: what you see or the error text, \"quoted\" for an exact phrase; raea's own fixes first, then community skills' and docs'";

const STOP: &[&str] = &[
    "a", "an", "the", "and", "or", "of", "to", "in", "on", "for", "with", "by", "is", "are", "be", "it", "its", "this", "that", "my", "i", "me", "we",
    "you", "how", "why", "what", "when", "does", "doesnt", "dont", "do", "not", "no", "but", "can", "cant", "cannot", "get", "gets", "got", "after",
    "before", "while", "from", "into", "at", "as", "have", "has", "had", "was", "were", "will", "would", "should", "could", "any", "some", "all",
    "just", "also", "so", "then", "than", "there", "their", "them", "they", "which",
];

/// A query: its quoted phrases and its other words, lowercased, stopwords out
#[derive(Debug, Default)]
pub struct Query {
    pub phrases: Vec<String>,
    pub words: Vec<String>,
}

pub fn parse(q: &str) -> Query {
    let mut out = Query::default();
    let mut rest = String::new();
    for (i, part) in q.split('"').enumerate() {
        if i % 2 == 1 && !part.trim().is_empty() {
            out.phrases.push(part.trim().to_string());
        } else {
            rest.push_str(part);
            rest.push(' ');
        }
    }
    for w in rest.to_lowercase().split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | ':'))) {
        let w = w.trim_matches(|c| c == '.' || c == ':');
        if w.chars().count() >= 2 && !STOP.contains(&w) && !out.words.iter().any(|x| x == w) {
            out.words.push(w.to_string());
        }
    }
    out
}

/// The domain tag a query word names, which ranks that domain's entries first
fn domain_of(w: &str) -> Option<&'static str> {
    Some(match w {
        "three" | "three.js" | "threejs" => "threejs",
        "r3f" | "fiber" => "r3f",
        "shader" | "shaders" | "glsl" | "wgsl" => "shaders",
        "bevy" => "bevy",
        "blender" => "blender",
        "godot" => "godot",
        "unity" => "unity",
        "unreal" => "unreal",
        "webgpu" => "webgpu",
        "ffmpeg" => "ffmpeg",
        "video" => "video",
        "audio" => "audio",
        "motion" => "motion",
        "solari" => "solari",
        "raea" => "raea",
        _ => return None,
    })
}

/// The words that match: domain words only rank, unless they're all there is
fn matching(words: &[String]) -> Vec<String> {
    let plain: Vec<String> = words.iter().filter(|w| domain_of(w).is_none()).cloned().collect();
    if plain.is_empty() { words.to_vec() } else { plain }
}

/// One entry found. `support` is None for raea's own (then `sources` says where and when it was found)
#[derive(Debug)]
pub struct Hit {
    pub support: Option<i64>,
    pub problem: String,
    pub cause: String,
    pub fix: String,
    pub also: Vec<String>,
    pub domains: String,
    pub sources: String,
}

/// The library's file: RAEA_FIXES, else the install's (share/raea/models/fixes), else the models folder's (a checkout,
/// where tools/release/models.tsv's download puts it)
pub fn library_path() -> Option<PathBuf> {
    let name = "fixes/fixes-library.db";
    std::env::var_os("RAEA_FIXES")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .chain([raea_io_home::home().join("models").join(name), raea_io_home::models().join(name)])
        .find(|p| p.is_file())
}

pub fn open_library(path: &std::path::Path) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|e| format!("{}: {e}", path.display()))
}

fn quoted(t: &str) -> String {
    format!("\"{}\"", t.replace('"', ""))
}

/// The library's entries for `q`, best first
pub fn search_library(db: &Connection, q: &Query, domain: Option<&str>, n: usize) -> Result<Vec<Hit>, String> {
    let e = |e: rusqlite::Error| format!("raea fix: the library: {e}");
    let total: i64 = db
        .query_row("SELECT v FROM meta WHERE k = 'fixes'", [], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or_else(|| db.query_row("SELECT count(*) FROM fixes", [], |r| r.get(0)).map_err(e), Ok)?;
    let cap = total / 25 + 1;
    // words too common to tell entries apart go; if every word is that common, keep them all
    let mut count = db.prepare("SELECT count(*) FROM (SELECT 1 FROM fixes_fts WHERE fixes_fts MATCH ?1 LIMIT ?2)").map_err(e)?;
    let mut keep = Vec::new();
    for w in &q.words {
        let c: i64 = count.query_row(params![quoted(w), cap], |r| r.get(0)).map_err(e)?;
        if c > 0 && c < cap {
            keep.push(w.clone());
        }
    }
    if keep.is_empty() && q.phrases.is_empty() {
        keep = q.words.clone();
    }
    let terms: Vec<String> = q
        .phrases
        .iter()
        .map(|p| format!("{{problem errors}}: {}", quoted(p)))
        .chain(matching(&keep).iter().map(|w| quoted(w)))
        .collect();
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    // the boosts name only domains domain_of returns, so they're written into the SQL; the rest are parameters
    let boost: String = q.words.iter().filter_map(|w| domain_of(w)).map(|d| format!(" * (CASE WHEN (' ' || f.domains || ' ') LIKE '% {d} %' THEN 2.5 ELSE 1.0 END)")).collect();
    let sql = format!(
        "SELECT f.support, f.problem, coalesce(f.cause, ''), f.fix, coalesce(f.also, ''), coalesce(f.domains, ''), coalesce(f.sources, '')
         FROM fixes_fts JOIN fixes f ON f.id = fixes_fts.rowid
         WHERE fixes_fts MATCH ?1 AND (?2 = '' OR (' ' || f.domains || ' ') LIKE '% ' || ?2 || ' %')
         ORDER BY bm25(fixes_fts, 8.0, 6.0, 3.0, 1.5, 1.0, 0.5) * (1.0 + 0.06 * min(f.support, 10)){boost} LIMIT ?3"
    );
    let mut st = db.prepare(&sql).map_err(e)?;
    let rows = st
        .query_map(params![terms.join(" OR "), domain.unwrap_or(""), n as i64], |r| {
            Ok(Hit {
                support: Some(r.get(0)?),
                problem: r.get(1)?,
                cause: r.get(2)?,
                fix: r.get(3)?,
                also: r.get::<_, String>(4)?.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect(),
                domains: r.get(5)?,
                sources: r.get(6)?,
            })
        })
        .map_err(e)?;
    rows.collect::<Result<_, _>>().map_err(e)
}

/// raea's own entries, one a line: problem, cause, fix, error text, domains, where found (tab-separated; # comments)
pub fn own_entries(text: &str) -> Vec<[String; 6]> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let p: Vec<&str> = l.split('\t').collect();
            (p.len() == 6).then(|| std::array::from_fn(|i| p[i].trim().to_string()))
        })
        .collect()
}

/// raea's own entries for `q`: those matching at least half its words (or a phrase), most words matched first
pub fn search_own(rows: &[[String; 6]], q: &Query, domain: Option<&str>, n: usize) -> Result<Vec<Hit>, String> {
    let e = |e: rusqlite::Error| format!("raea fix: raea's own entries: {e}");
    let db = Connection::open_in_memory().map_err(e)?;
    db.execute_batch("CREATE VIRTUAL TABLE own USING fts5(problem, errors, cause, fix, domains UNINDEXED, found UNINDEXED, tokenize='porter unicode61 remove_diacritics 2');")
        .map_err(e)?;
    let mut ins = db.prepare("INSERT INTO own(rowid, problem, cause, fix, errors, domains, found) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)").map_err(e)?;
    for (i, r) in rows.iter().enumerate() {
        ins.execute(params![i as i64, r[0], r[1], r[2], r[3], r[4], r[5]]).map_err(e)?;
    }
    let words = matching(&q.words);
    let need = words.len().div_ceil(2).max(1);
    let mut st = db.prepare("SELECT rowid, bm25(own, 8.0, 6.0, 1.5, 1.0) FROM own WHERE own MATCH ?1").map_err(e)?;
    // rowid -> (words matched, best bm25)
    let mut score: std::collections::HashMap<i64, (usize, f64)> = Default::default();
    let mut tally = |m: String, weight: usize| -> Result<(), String> {
        for hit in st.query_map([m], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))).map_err(e)? {
            let (id, b) = hit.map_err(e)?;
            let s = score.entry(id).or_insert((0, 0.0));
            s.0 += weight;
            s.1 = s.1.min(b);
        }
        Ok(())
    };
    for p in &q.phrases {
        tally(format!("{{problem errors}}: {}", quoted(p)), need)?;
    }
    for w in &words {
        tally(quoted(w), 1)?;
    }
    let mut ids: Vec<(i64, (usize, f64))> = score.into_iter().filter(|(_, s)| s.0 >= need).collect();
    ids.retain(|(id, _)| domain.is_none_or(|d| rows[*id as usize][4].split_whitespace().any(|x| x == d)));
    ids.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.1.1.total_cmp(&b.1.1)));
    Ok(ids
        .into_iter()
        .take(n)
        .map(|(id, _)| {
            let r = &rows[id as usize];
            Hit { support: None, problem: r[0].clone(), cause: r[1].clone(), fix: r[2].clone(), also: Vec::new(), domains: r[4].clone(), sources: r[5].clone() }
        })
        .collect())
}

fn wrap(t: &str, indent: &str) -> String {
    let (mut out, mut line) = (String::new(), String::new());
    for w in t.split_whitespace() {
        if !line.is_empty() && line.chars().count() + w.chars().count() > 104 {
            out.push_str(&line);
            out.push('\n');
            out.push_str(indent);
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(w);
    }
    out + &line
}

fn cut(t: &str, n: usize) -> String {
    if t.chars().count() <= n { t.to_string() } else { t.chars().take(n).collect::<String>() + " ..." }
}

/// Hits as raea fix prints them
pub fn render(hits: &[Hit], sources: bool) -> String {
    let mut out = String::new();
    for h in hits {
        let head = h.support.map_or_else(|| "raea".to_string(), |s| format!("{s:>4}"));
        out += &format!("{head}  {}\n", wrap(&h.problem, "      "));
        if !h.cause.is_empty() {
            out += &format!("      cause: {}\n", wrap(&h.cause, "             "));
        }
        out += &format!("      fix:   {}\n", wrap(&cut(&h.fix, 600), "             "));
        for a in h.also.iter().take(2) {
            out += &format!("      also:  {}\n", wrap(&cut(a, 240), "             "));
        }
        match (h.support, sources) {
            (None, _) => out += &format!("      [{}]  found in raea work: {}\n\n", h.domains, h.sources),
            (Some(_), true) => out += &format!("      [{}]  from {}\n\n", h.domains, h.sources.replace(',', ", ")),
            (Some(_), false) => out += &format!("      [{}]\n\n", h.domains),
        }
    }
    out
}

/// `raea fix SYMPTOM... [--domain DOMAIN] [--limit N] [--sources] [--notice]`
pub fn cli(args: &[String]) -> Result<String, String> {
    let (mut words, mut domain, mut limit, mut sources, mut notice) = (Vec::new(), None::<String>, 6usize, false, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--domain" => domain = Some(it.next().ok_or("raea fix: --domain takes a domain (bevy, blender, threejs, shaders, ffmpeg, ...)")?.to_lowercase()),
            "--limit" => limit = it.next().and_then(|n| n.parse().ok()).filter(|n| *n > 0).ok_or("raea fix: --limit takes a count")?,
            "--sources" => sources = true,
            "--notice" => notice = true,
            w => words.push(w.to_string()),
        }
    }
    let library = library_path();
    if notice {
        let path = library.ok_or("raea fix: the library isn't installed here (reinstall raea: curl -fsSL https://raea.io/install | sh)")?;
        return open_library(&path)?.query_row("SELECT v FROM meta WHERE k = 'notice'", [], |r| r.get(0)).map_err(|e| format!("raea fix: {e}"));
    }
    let text = words.join(" ");
    let q = parse(&text);
    if q.words.is_empty() && q.phrases.is_empty() {
        return Err(format!("usage: {USAGE}\n       describe what you see (\"shadows cut off\", \"mesh renders black\") or give the error text in quotes"));
    }
    let own_file = raea_io_home::home().join("sdk/guides/fixes/raea.tsv");
    let own_rows = own_entries(&std::fs::read_to_string(&own_file).unwrap_or_default());
    let mut hits = search_own(&own_rows, &q, domain.as_deref(), 3)?;
    let mut note = String::new();
    match &library {
        Some(path) => hits.extend(search_library(&open_library(path)?, &q, domain.as_deref(), limit)?),
        None => note = "raea fix: the library of community fixes isn't installed here; raea's own entries only (reinstall raea for it)\n".into(),
    }
    if hits.is_empty() {
        return Err(format!("{note}raea fix: nothing matches \"{text}\": say what you see in other words, or quote the exact error text"));
    }
    let community = hits.iter().any(|h| h.support.is_some());
    let mut out = render(&hits, sources);
    if community {
        out += "Numbered entries come from community skills and docs (the number: how many sources state it), not checked in raea;\nconfirm a fix on a frame. raea fix --sources names them, --notice their licences.\n";
    }
    Ok(note + &out)
}
