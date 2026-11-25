//! A videoform's documents and the person's words (docs/videoform.feature VF8f). `raea videoform add NAME --group G
//! --brand B --context "…" FILES...` takes everything given with a group of videos: the videos as references, each
//! document (a brief, a brand's guide, notes: PDF, Word, text, a still of one) read once into passages and kept by its
//! content hash (store.rs), and the person's words, kept as theirs, which outrank any document. Every add then learns
//! (`learn`): each passage found where its group's videos carry it out and how; the passages two groups or more both
//! ask, and how they're made, the videoform's (videoform/briefs.json); what only one group asked, its brand's
//! (brand-B/briefs.json); briefs.ts's rules, which the person's AI writes for what a similarity can't tell, measured.
//! `raea videoform docs NAME [GROUP]` lists the passages with their ids for the AI to write briefs.ts from, and
//! `follow --brief FILE...` (`plan`) sets a new job's documents against what was learned.

mod read;
mod store;

use raea_core_brief::Videoform;
use raea_core_brief::videoform::{Briefs, Context};
use raea_sys_videoform::rows::{Rows, Shot};
use raea_sys_videoform_briefs::link::{Ask, Asked, Found, Made, asks, found, nearest};
use raea_sys_videoform_briefs::rules::{Held, held};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
pub use read::is_doc;
use store::{Doc, Gem, sha};

/// A .ts file's default export, as JSON
pub type Eval = fn(&Path) -> Result<String, String>;

/// briefs.ts's name, beside fingerprint.ts
pub const FILE: &str = "briefs.ts";

/// A reference, as learning reads it
struct Ref {
    media: String,
    title: String,
    group: Option<String>,
    shots: Vec<Shot>,
}

/// A brand's fingerprint folder (brand-SLUG), made when there's none
pub fn brand_dir(brand: &str) -> Result<PathBuf, String> {
    let name = format!("brand-{}", raea_sys_videoform_rules::slug(brand));
    let dir = raea_sys_fingerprint::folder::Folder::named(&name);
    if !dir.exists() {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let about = format!("{brand}: the brand's own look, rules and kit, found by raea videoform");
        let out = std::process::Command::new(exe).args(["fingerprint", "new", &name, "--about", &about, "--tags", &name]).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("making {name}: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        // fingerprint new makes the account's first fingerprint its default; a brand's judges only what's tagged with it
        let default = raea_sys_fingerprint::folder::root().join("default");
        if std::fs::read_to_string(&default).is_ok_and(|d| d.trim() == name) {
            let _ = std::fs::remove_file(&default);
        }
    }
    Ok(dir.dir)
}

/// A document given with a group of videos, read and kept (once: a document read before is known by its hash), and
/// told to the section
pub fn take(section: &mut Videoform, dir: &Path, file: &Path, group: Option<&str>, brand: Option<&str>, today: &str) -> Result<String, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let hash = sha(&bytes);
    let title = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let line = match Doc::kept(dir, &hash) {
        true => format!("  (already read: {title})"),
        false => {
            let t = std::time::Instant::now();
            let ps = read::passages(file)?;
            let d = Doc::make(dir, hash.clone(), title.clone(), file.display().to_string(), ps, &mut Gem::default(), today)?;
            format!("  {title}: {} passages, read in {:.1} s", d.passages.len(), t.elapsed().as_secs_f64())
        }
    };
    section.tell(Context { by: "document".into(), text: title, group: group.map(str::to_string), brand: brand.map(str::to_string), at: today.into(), document: Some(hash), path: Some(file.display().to_string()) });
    Ok(line)
}

/// The person's words (--context), kept as a document of theirs, so they're found where they were carried out as a
/// brief's are, and told to the section over any document
pub fn person(section: &mut Videoform, dir: &Path, text: Option<&str>, group: Option<&str>, brand: Option<&str>, today: &str) -> Result<(), String> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(()) };
    let hash = sha(format!("person\n{}\n{text}", group.unwrap_or("")).as_bytes());
    if !Doc::kept(dir, &hash) {
        let mut ps = raea_sys_videoform_briefs::passages::from_text(text);
        if ps.is_empty() {
            ps.push(raea_sys_videoform_briefs::passages::Passage::new("", text, 0));
        }
        Doc::make(dir, hash.clone(), "you".into(), String::new(), ps, &mut Gem::default(), today)?;
    }
    section.tell(Context { by: "person".into(), text: text.into(), group: group.map(str::to_string), brand: brand.map(str::to_string), at: today.into(), document: Some(hash), path: None });
    Ok(())
}

/// Every passage said of the videos, the person's first
fn asked(dir: &Path, contexts: &[Context], gem: &mut Gem) -> Vec<Asked> {
    let mut out = Vec::new();
    for c in contexts {
        let Some(d) = c.document.as_deref().and_then(|h| Doc::load(dir, h, gem)) else { continue };
        for (i, p) in d.passages.into_iter().enumerate() {
            let from = if c.by == "person" { "you".to_string() } else { c.text.clone() };
            out.push(Asked { passage: p, from, person: c.by == "person", group: c.group.clone(), at: c.at.clone(), find: d.find[i].clone(), same: d.same[i].clone() });
        }
    }
    // a passage said twice in one group (two documents repeat it) counted once, the person's or newest kept
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|a| seen.insert((a.passage.id.clone(), a.group.clone())));
    out
}

/// The asks only group `g` made, carried out above chance
fn only<'a>(a: &'a [Ask], g: &str) -> Vec<&'a Ask> {
    a.iter().filter(|x| x.groups.len() == 1 && x.groups[0] == g && x.carried > x.chance).collect()
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

fn parts(a: &Ask, n: usize) -> String {
    if a.parts.is_empty() {
        return "nothing set apart from shots at large".into();
    }
    a.parts.iter().take(n).map(|p| format!("{} ({:.0}%, usual {:.0}%)", p.how, p.strength * 100.0, p.usual * 100.0)).collect::<Vec<_>>().join(", ")
}

/// What's learned: written to videoform/briefs.json and each brand's briefs.json, and said
/// (each kept reference: its short title, its rows, its line of references.jsonl, whose source names its group)
pub fn learn(dir: &Path, name: &str, contexts: &[Context], kept: &[(String, Rows, Value)], eval: Eval) -> Result<String, String> {
    let mut gem = Gem::default();
    let asked = asked(dir, contexts, &mut gem);
    if asked.is_empty() {
        return Ok(String::new());
    }
    let group = |v: &Value| v["source"]["group"].as_str().or(v["source"]["brand"].as_str()).map(str::to_string);
    let refs: Vec<Ref> = kept.iter().map(|(t, r, v)| Ref { media: r.media.clone(), title: t.clone(), group: group(v), shots: r.shots.clone() }).collect();
    let refs = &refs[..];
    let mut made = Vec::new();
    for r in refs {
        let (said, shown) = store::words(dir, &r.media, &r.shots, &mut gem)?;
        made.push(Made { media: r.media.clone(), title: r.title.clone(), group: r.group.clone(), shots: r.shots.clone(), said, shown });
    }
    let f = found(&asked, &made);
    let a = asks(&asked, &f, &made);
    let rules = match dir.join(FILE).is_file() {
        true => {
            let json = eval(&dir.join(FILE)).map_err(|e| format!("{FILE}: {e}"))?;
            let b: Briefs = raea_core_brief::read(FILE, &json)?;
            Some(held(&b.rules, &made)?)
        }
        false => None,
    };
    let brand_of: BTreeMap<String, String> = contexts.iter().filter_map(|c| Some((c.group.clone()?, c.brand.clone()?))).collect();
    write(dir, &f, &a, rules.as_ref(), &brand_of)?;
    Ok(said(name, contexts, &f, &a, rules.as_ref(), refs))
}

fn write(dir: &Path, f: &[Found], a: &[Ask], rules: Option<&(Vec<Held>, Vec<Held>)>, brand_of: &BTreeMap<String, String>) -> Result<(), String> {
    let (form_rules, brand_rules) = rules.map_or((&[][..], &[][..]), |(x, y)| (&x[..], &y[..]));
    let across: Vec<&Ask> = a.iter().filter(|x| x.groups.len() > 1 || x.groups.iter().any(|g| g == "all")).collect();
    // what a new job is planned from: every ask carried out above chance, those two groups or more asked first
    let mut learned: Vec<&Ask> = a.iter().filter(|x| x.carried > x.chance).collect();
    learned.sort_by_key(|x| std::cmp::Reverse(x.groups.len()));
    let v = json!({"across": across, "asks": learned, "rules": form_rules, "found": f});
    std::fs::write(dir.join("videoform/briefs.json"), serde_json::to_vec_pretty(&v).unwrap()).map_err(|e| e.to_string())?;
    for (g, b) in brand_of {
        let own = only(a, g);
        let rules: Vec<&Held> = brand_rules.iter().filter(|h| h.groups.contains_key(g)).collect();
        if own.is_empty() && rules.is_empty() {
            continue;
        }
        let bd = brand_dir(b)?;
        let file = bd.join("briefs.json");
        let mut all: serde_json::Map<String, Value> = store::json_at(&file).unwrap_or_default();
        all.insert(g.clone(), json!({"asks": own, "rules": rules, "from": dir.display().to_string()}));
        std::fs::write(&file, serde_json::to_vec_pretty(&all).unwrap()).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(())
}

/// What was asked, and how it was made, in a few lines
fn said(name: &str, contexts: &[Context], f: &[Found], a: &[Ask], rules: Option<&(Vec<Held>, Vec<Held>)>, refs: &[Ref]) -> String {
    let docs = contexts.iter().filter(|c| c.by == "document").count();
    let mine = contexts.iter().filter(|c| c.by == "person").count();
    let groups: std::collections::BTreeSet<String> = refs.iter().filter_map(|r| r.group.clone()).collect();
    let carried = f.iter().filter(|x| !x.moments.is_empty()).count();
    let mut out = vec![format!(
        "What was asked, and how it was made (VF8f): {docs} documents and {mine} of your notes, {} passages for {} groups; {carried} carried out where the words said or shown show it",
        f.len(), groups.len()
    )];
    let across: Vec<&Ask> = a.iter().filter(|x| x.groups.len() > 1 && x.carried > x.chance).collect();
    if !across.is_empty() {
        out.push(format!("  asked in two groups or more, and how it's made (the videoform's), {}:", across.len()));
        for x in across.iter().take(12) {
            let words: Vec<String> = x.passages.iter().map(|p| format!("\"{}\" ({})", short(&p.2, 70), p.1.as_deref().unwrap_or("all"))).collect();
            out.push(format!("    {}{}: carried out in {:.0}% (chance {:.0}%); made {}", if x.person { "you: " } else { "" }, words.join(" ~ "), x.carried * 100.0, x.chance * 100.0, parts(x, 4)));
        }
    }
    for g in &groups {
        let own = only(a, g);
        if !own.is_empty() {
            let top: Vec<String> = own.iter().take(3).map(|x| format!("\"{}\" ({:.0}%, chance {:.0}%: {})", short(&x.passages[0].2, 60), x.carried * 100.0, x.chance * 100.0, parts(x, 2))).collect();
            out.push(format!("  only {g} asked, carried out (its brand's): {}; {}", own.len(), top.join("; ")));
        }
    }
    let unseen = f.len() - carried;
    if unseen > 0 {
        out.push(format!("  {unseen} passages not seen carried out: what words don't show (a prop, a place, a deliverable), or what wasn't done; `raea videoform docs {name}` lists them"));
    }
    match rules {
        None => out.push(format!("  {FILE} isn't written: your AI writes the rules a similarity can't tell (a never, an order, a choice) from `raea videoform docs {name}`")),
        Some((form, brand)) => {
            out.push(format!("  {FILE}: {} rules the videoform's, {} a brand's", form.len(), brand.len()));
            for h in form.iter().chain(brand.iter()).take(10) {
                let gs: Vec<String> = h.groups.iter().map(|(g, (n, of))| format!("{g} {n}/{of}")).collect();
                let broken = if h.broken.is_empty() { String::new() } else { format!("; broken in {}", h.broken.iter().take(3).cloned().collect::<Vec<_>>().join(", ")) };
                out.push(format!("    {} ({} {}): holds {:.0}% ({}){broken}", h.text, h.kind, h.what.join(", "), h.strength * 100.0, gs.join(", ")));
            }
        }
    }
    out.join("\n")
}

/// `raea videoform docs NAME [GROUP]` (its plain words), None for its usage
pub fn verb(words: &[String]) -> Option<(String, i32)> {
    let d = raea_sys_fingerprint::folder::Folder::named(words.first()?);
    Some(match d.exists() {
        true => (docs(&d.dir, words.get(1).map(String::as_str)), 0),
        false => (format!("raea videoform: no videoform {}", words[0]), 1),
    })
}

/// Each passage, its id, and where it was carried out, for the AI to write briefs.ts
fn docs(dir: &Path, group: Option<&str>) -> String {
    let Ok(b) = std::fs::read(dir.join("videoform/briefs.json")) else { return "raea videoform: nothing learned of documents yet (add one with its videos: raea videoform add NAME --group G FILE...)".into() };
    let v: Value = serde_json::from_slice(&b).unwrap_or_default();
    let f: Vec<Found> = serde_json::from_value(v["found"].clone()).unwrap_or_default();
    let mut out = Vec::new();
    let mut last = None;
    for x in f.iter().filter(|x| group.is_none() || x.group.as_deref() == group) {
        let head = (x.group.clone(), x.from.clone());
        if last.as_ref() != Some(&head) {
            out.push(format!("{} — {}", x.group.as_deref().unwrap_or("all"), x.from));
            last = Some(head);
        }
        let seen = match x.moments.len() {
            0 => "not seen".to_string(),
            n => format!("seen in {n}/{}: \"{}\" ({})", x.of, short(&x.moments[0].words, 50), x.moments[0].how.iter().take(3).cloned().collect::<Vec<_>>().join(", ")),
        };
        out.push(format!("  {} {} — {seen}", x.id, short(&x.said, 140)));
    }
    out.join("\n")
}

/// A new briefs.ts, for the person's AI to fill
pub fn starter(name: &str) -> String {
    format!(
        r#"// {name}'s brief rules (docs/videoform.feature VF8f), written by your AI from `raea videoform docs {name}`: what a
// similarity can't tell. Each rule is measured on its group's videos, and one two groups say alike is the videoform's.
// kind: always | never | before | either | open | close; what: said:REGEX, shown:REGEX, layer:FORM[-KIND], captions,
// faces:N, seconds:LO-HI. `from` is the passage's id.
import {{ briefs }} from "raea/brief";

export default briefs({{
  rules: [
  ],
}});
"#
    )
}

/// The project's brief.ts (in the folder follow runs in), each field's words a passage: "believe: ..."
fn project(eval: Eval) -> Vec<String> {
    let Some(m) = eval(Path::new("brief.ts")).ok().and_then(|j| serde_json::from_str::<serde_json::Map<String, Value>>(&j).ok()) else { return Vec::new() };
    let mut out = Vec::new();
    for (k, v) in &m {
        match v {
            Value::String(s) => out.push(format!("{k}: {s}")),
            Value::Array(a) if !a.is_empty() && a.iter().all(Value::is_string) => out.push(format!("{k}: {}", a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; "))),
            _ => {}
        }
    }
    out
}

/// `follow --brief FILE,...`: a new job's documents and the project's brief.ts against what was learned, each passage
/// with the ask it's nearest and how that's made, or said to be new; and the plan's "brief" (JSON)
pub fn plan(dir: &Path, brief: Option<&str>, eval: Eval) -> Result<(String, Value), String> {
    let files: Vec<PathBuf> = brief.map(|b| b.split(',').map(|f| PathBuf::from(f.trim())).collect()).unwrap_or_default();
    let words = project(eval);
    let Ok(b) = std::fs::read(dir.join("videoform/briefs.json")) else { return Ok((String::new(), Value::Null)) };
    let v: Value = serde_json::from_slice(&b).map_err(|e| e.to_string())?;
    let learned: Vec<Ask> = serde_json::from_value(v["asks"].clone()).unwrap_or_default();
    // the learned passages' vectors, from the documents kept
    let mut vectors = BTreeMap::new();
    let mut gem = Gem::default();
    for e in std::fs::read_dir(dir.join("videoform/documents")).into_iter().flatten().flatten() {
        let hash = e.path().file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if e.path().extension().is_some_and(|x| x == "json") && let Some(d) = Doc::load(dir, &hash, &mut gem) {
            vectors.extend(d.passages.iter().map(|p| p.id.clone()).zip(d.same));
        }
    }
    let mut ps = Vec::new();
    for f in &files {
        ps.extend(read::passages(f)?.into_iter().map(|p| (f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), p)));
    }
    for w in &words {
        ps.extend(raea_sys_videoform_briefs::passages::from_text(w).into_iter().map(|p| ("you".to_string(), p)));
    }
    let (_, same) = gem.passages(&ps.iter().map(|p| p.1.clone()).collect::<Vec<_>>())?;
    let mut lines = vec![format!("The brief, against what the videoform learned of briefs (VF8f): {} passages", ps.len())];
    let mut rows = Vec::new();
    let mut new = 0;
    for ((from, p), near) in ps.iter().zip(nearest(&same, &learned, &vectors)) {
        match near {
            Some((a, z)) => {
                let by = a.groups.join(" and ");
                lines.push(format!("  \"{}\": as {by}'s \"{}\" was, carried out in {:.0}% (chance {:.0}%): {}", short(&p.said(), 70), short(&a.passages[0].2, 50), a.carried * 100.0, a.chance * 100.0, parts(a, 4)));
                rows.push(json!({"id": p.id, "said": p.said(), "from": from, "like": a.passages, "z": z, "carried": a.carried, "made": a.parts}));
            }
            None => {
                new += 1;
                rows.push(json!({"id": p.id, "said": p.said(), "from": from, "new": true}));
            }
        }
    }
    lines.push(format!("  {new} passages are like nothing the references were asked: your AI plans them from the brief alone"));
    Ok((lines.join("\n"), json!(rows)))
}
