//! A vocabulary's terms against the kept rows (docs/videoform.feature VF5): the context says what matters, the
//! references show how it's done. Each picture term is a word head (FP2e): the mean of its phrasings' SigLIP 2 text
//! vectors, against each shot's pooled picture vector, as a z over a background of ordinary descriptions; a sound term
//! the same in CLAP's space. A term fires on a shot when it stands out of the background and the shot is nearer it than
//! its near-misses. Patterns are read in the words on screen and the words said. What's reported, for the person's AI
//! to revise from: terms that never fire, fire everywhere, or fire with another on the same shots; what the context
//! says is always there and where it isn't; each section and the references it's found in; and shots that recur
//! across references with no term naming them.

use crate::rows::Rows;
use raea_core_brief::Vocabulary;
use raea_core_brief::videoform::{ATS, HOWS, Section};
use serde::{Deserialize, Serialize};

/// How far out of the background a term stands on a shot to fire there (z)
pub const FIRES: f32 = 2.0;
/// Fires on more of the shots than this, in every reference: it tells nothing apart
const EVERYWHERE: f64 = 0.8;
/// Two terms on this much the same shots (Jaccard) fire together
const TOGETHER: f64 = 0.7;
/// Shots this alike (cosine) are one recurring shot
const ALIKE: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Space {
    Picture,
    Sound,
}

/// A term, embedded: its vector (unit), its near-misses', and what the context says of it
#[derive(Clone, Debug, Default)]
pub struct Head {
    pub name: String,
    pub space: Option<Space>,
    pub v: Vec<f32>,
    pub unlike: Vec<Vec<f32>>,
    pub how: Option<String>,
    /// something to ignore: its shots are left out of what's unnamed
    pub ignore: bool,
}

/// A pattern of words: on screen, or said
pub struct Pat {
    pub name: String,
    pub re: regex::Regex,
    pub said: bool,
    pub how: Option<String>,
}

/// The background each space's z is taken against: ordinary descriptions, embedded
#[derive(Clone, Debug, Default)]
pub struct Background {
    pub picture: Vec<Vec<f32>>,
    pub sound: Vec<Vec<f32>>,
}

pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).fold(0.0, |s, (x, y)| s + x * y)
}

/// A head's z on a shot's vector, and whether it fires there
pub fn z(h: &Head, bg: &[Vec<f32>], w: &[f32]) -> Option<(f32, bool)> {
    if bg.is_empty() || w.iter().all(|x| *x == 0.0) {
        return None;
    }
    let d: Vec<f32> = bg.iter().map(|b| dot(b, w)).collect();
    let m = d.iter().sum::<f32>() / d.len() as f32;
    let sd = (d.iter().map(|x| (x - m).powi(2)).sum::<f32>() / d.len() as f32).sqrt().max(1e-6);
    let own = dot(&h.v, w);
    let z = (own - m) / sd;
    Some((z, z >= FIRES && h.unlike.iter().all(|u| dot(u, w) < own)))
}

/// What fires on each shot of a reference: [shot][head, then pattern]
pub fn fire(rows: &Rows, heads: &[Head], bg: &Background, pats: &[Pat]) -> Vec<Vec<bool>> {
    let n = rows.shots.len().max(1);
    let (sig, clap) = (rows.sig.len() / n, rows.clap.len() / n);
    rows.shots.iter().enumerate().map(|(i, s)| {
        let mut row: Vec<bool> = heads.iter().map(|h| match h.space {
            Some(Space::Picture) => z(h, &bg.picture, &rows.sig[i * sig..(i + 1) * sig]).is_some_and(|x| x.1),
            Some(Space::Sound) => z(h, &bg.sound, &rows.clap[i * clap..(i + 1) * clap]).is_some_and(|x| x.1),
            None => false,
        }).collect();
        let screen = s.text.iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(" / ");
        row.extend(pats.iter().map(|p| p.re.is_match(if p.said { &s.words } else { &screen })));
        row
    }).collect()
}

/// What one term or pattern did on the references
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TermFound {
    pub name: String,
    /// picture, sound, on screen, said
    pub kind: String,
    pub shots: usize,
    pub of: usize,
    /// whether it fires in each reference
    pub refs: Vec<bool>,
    /// fires, never fires, fires everywhere
    pub verdict: String,
    /// the terms it fires with, on much the same shots
    pub with: Vec<String>,
    pub how: Option<String>,
    /// the references the context says it's in that it isn't
    pub missing: Vec<usize>,
}

/// A section across the references: where it's found in each
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SectionFound {
    pub name: String,
    pub at: Option<String>,
    pub how: Option<String>,
    /// the second it starts in each reference, when it's found there
    pub found: Vec<Option<f64>>,
    pub missing: Vec<usize>,
    /// names it shows by that no term or pattern has
    pub unknown: Vec<String>,
    /// in each reference, the shots where it's shown, inside where it falls (VF7 cuts the references at these)
    #[serde(default)]
    pub hits: Vec<Vec<usize>>,
}

/// A shot that recurs across references with no term naming it
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Unnamed {
    pub refs: Vec<usize>,
    pub shots: usize,
    /// one of them: its reference and its start
    pub example: (usize, f64),
    /// what's in them most (Apple Vision's scene labels) and on screen
    pub scene: Vec<String>,
    pub text: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub terms: Vec<TermFound>,
    pub sections: Vec<SectionFound>,
    pub unnamed: Vec<Unnamed>,
    /// the shot kinds (VF6), made beside it from the same heads
    #[serde(default)]
    pub kinds: crate::kinds::Kinds,
    /// the structure (VF7), made from the sections and the kinds
    #[serde(default)]
    pub structure: crate::structure::Structure,
}

/// The references whose firing misses what `how` says is always there
fn missing(how: &Option<String>, refs: &[bool]) -> Vec<usize> {
    if how.as_deref() != Some("always") { Vec::new() } else { refs.iter().enumerate().filter(|(_, f)| !**f).map(|(i, _)| i).collect() }
}

/// Whether a shot falls where a section is: the first quarter, the last, or between
fn inside(at: Option<&str>, start: f64, end: f64, dur: f64) -> bool {
    match at {
        Some("open") => start < dur * 0.25,
        Some("close") => end > dur * 0.75,
        Some("middle") => end > dur * 0.2 && start < dur * 0.8,
        _ => true,
    }
}

/// The most common words of a list, at most `n`
pub(crate) fn common(words: impl Iterator<Item = String>, n: usize) -> Vec<String> {
    let mut count: std::collections::BTreeMap<String, usize> = Default::default();
    words.for_each(|w| *count.entry(w).or_default() += 1);
    let mut v: Vec<(String, usize)> = count.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.into_iter().take(n).map(|x| x.0).collect()
}

/// What the vocabulary did on the references' rows
pub fn report(refs: &[Rows], heads: &[Head], bg: &Background, pats: &[Pat], sections: &[Section]) -> Report {
    let fired: Vec<Vec<Vec<bool>>> = refs.iter().map(|r| fire(r, heads, bg, pats)).collect();
    let names: Vec<&str> = heads.iter().map(|h| h.name.as_str()).chain(pats.iter().map(|p| p.name.as_str())).collect();
    let all: Vec<(usize, usize)> = fired.iter().enumerate().flat_map(|(r, f)| (0..f.len()).map(move |s| (r, s))).collect();
    let on = |t: usize| -> Vec<bool> { all.iter().map(|&(r, s)| fired[r][s][t]).collect() };
    let mut terms = Vec::new();
    for (t, name) in names.iter().enumerate() {
        let hits = on(t);
        let shots = hits.iter().filter(|h| **h).count();
        let in_ref: Vec<bool> = fired.iter().map(|f| f.iter().any(|s| s[t])).collect();
        let (kind, how) = match heads.get(t) {
            Some(h) => (if h.space == Some(Space::Sound) { "sound" } else { "picture" }, h.how.clone()),
            None => {
                let p = &pats[t - heads.len()];
                (if p.said { "said" } else { "on screen" }, p.how.clone())
            }
        };
        let everywhere = shots as f64 > all.len() as f64 * EVERYWHERE && in_ref.iter().all(|x| *x);
        let verdict = if shots == 0 { "never fires" } else if everywhere { "fires everywhere" } else { "fires" };
        let with = (0..names.len()).filter(|&o| o != t).filter(|&o| {
            let other = on(o);
            let (both, either) = hits.iter().zip(&other).fold((0, 0), |(b, e), (x, y)| (b + usize::from(*x && *y), e + usize::from(*x || *y)));
            both >= 3 && both as f64 >= either as f64 * TOGETHER
        }).map(|o| names[o].to_string()).collect();
        terms.push(TermFound { name: name.to_string(), kind: kind.into(), shots, of: all.len(), missing: missing(&how, &in_ref), refs: in_ref, verdict: verdict.into(), with, how });
    }
    let sections = sections.iter().map(|sec| {
        let idx: Vec<usize> = (0..names.len()).filter(|&i| sec.shows.iter().any(|n| n == names[i])).collect();
        let mut unknown = Vec::new();
        for n in &sec.shows {
            if !names.contains(&n.as_str()) {
                unknown.push(n.clone());
            }
        }
        let hits: Vec<Vec<usize>> = refs.iter().zip(&fired).map(|(r, f)| {
            let dur = r.shots.last().map_or(0.0, |s| s.end);
            (0..r.shots.len()).filter(|&i| inside(sec.at.as_deref(), r.shots[i].start, r.shots[i].end, dur) && idx.iter().any(|&t| f[i][t])).collect()
        }).collect();
        let found: Vec<Option<f64>> = refs.iter().zip(&hits).map(|(r, h)| h.first().map(|&i| r.shots[i].start)).collect();
        let present: Vec<bool> = found.iter().map(Option::is_some).collect();
        SectionFound { name: sec.name.clone(), at: sec.at.clone(), how: sec.how.clone(), missing: missing(&sec.how, &present), found, unknown, hits }
    }).collect();
    Report { terms, sections, unnamed: unnamed(refs, &fired, heads), ..Default::default() }
}

/// What the vocabulary did, as the person's AI reads it: its mistakes, each term, each section with where it's found,
/// the feel words, and the shots that recur with no term naming them
pub fn lines(rep: &Report, rows: &[Rows], names: &[&str], v: &Vocabulary) -> Vec<String> {
    let n = names.len();
    let list = |ix: &[usize]| ix.iter().map(|&i| names[i]).collect::<Vec<_>>().join(", ");
    let mut out = Vec::new();
    out.extend(mistakes(v).into_iter().map(|m| format!("  {m}")));
    for t in rep.terms.iter().filter(|t| !t.name.starts_with("(ignore)")) {
        let how = t.how.as_deref().map(|h| format!(", {h}")).unwrap_or_default();
        let mut l = format!("  {} ({}{how}): {} of {} shots, in {} of {n}", t.name, t.kind, t.shots, t.of, t.refs.iter().filter(|x| **x).count());
        l += match t.verdict.as_str() {
            "never fires" => ": never fires; phrase it as a caption of what a still shows, or drop it",
            "fires everywhere" => ": fires everywhere, so it tells nothing apart; narrow it or drop it",
            _ => "",
        };
        if !t.missing.is_empty() {
            l += &format!("; the context says always, and it isn't in {}", list(&t.missing));
        }
        if !t.with.is_empty() {
            l += &format!("; fires with {} on much the same shots", t.with.join(", "));
        }
        out.push(l);
    }
    if !rep.sections.is_empty() {
        out.push("Sections:".into());
    }
    for s in &rep.sections {
        let at: Vec<f64> = s.found.iter().zip(rows).filter_map(|(f, r)| f.map(|f| if s.at.as_deref() == Some("close") { r.shots.last().map_or(0.0, |l| l.end) - f } else { f })).collect();
        let span = match (at.iter().copied().reduce(f64::min), at.iter().copied().reduce(f64::max)) {
            (Some(a), Some(b)) if s.at.as_deref() == Some("close") => format!(", from {a:.1} to {b:.1} s before the end"),
            (Some(a), Some(b)) => format!(", from {a:.1} to {b:.1} s in"),
            _ => String::new(),
        };
        let what = [s.at.clone(), s.how.clone()].into_iter().flatten().collect::<Vec<_>>().join(", ");
        let mut l = format!("  {}{}: in {} of {n}{span}", s.name, if what.is_empty() { String::new() } else { format!(" ({what})") }, at.len());
        if !s.missing.is_empty() {
            l += &format!("; the context says always, and it isn't found in {}", list(&s.missing));
        }
        if !s.unknown.is_empty() {
            l += &format!("; no term or pattern is named {}", s.unknown.join(", "));
        }
        out.push(l);
    }
    for f in &v.feel {
        out.push(format!("Feel \"{}\": {} checked by the context's word", f.word, f.measures.join(", ")));
    }
    if !rep.unnamed.is_empty() {
        out.push("Shots that recur with no term naming them:".into());
    }
    for u in &rep.unnamed {
        let text = if u.text.is_empty() { String::new() } else { format!("; on screen {}", u.text.iter().map(|t| format!("{t:?}")).collect::<Vec<_>>().join(", ")) };
        out.push(format!("  in {} of {n} references, {} shots (e.g. {} at {:.1} s): {}{text}", u.refs.len(), u.shots, names[u.example.0], u.example.1, u.scene.join(", ")));
    }
    out
}

/// Shots no picture term names, gathered by likeness; those that recur in two references or more, most first
fn unnamed(refs: &[Rows], fired: &[Vec<Vec<bool>>], heads: &[Head]) -> Vec<Unnamed> {
    let picture: Vec<usize> = (0..heads.len()).filter(|&t| heads[t].space == Some(Space::Picture)).collect();
    let mut groups: Vec<(Vec<f32>, Vec<(usize, usize)>)> = Vec::new();
    for (r, rows) in refs.iter().enumerate() {
        let sig = rows.sig.len() / rows.shots.len().max(1);
        for s in 0..rows.shots.len() {
            if picture.iter().any(|&t| fired[r][s][t]) || sig == 0 {
                continue;
            }
            let v = &rows.sig[s * sig..(s + 1) * sig];
            match groups.iter_mut().find(|g| dot(&g.0, v) >= ALIKE) {
                Some(g) => g.1.push((r, s)),
                None => groups.push((v.to_vec(), vec![(r, s)])),
            }
        }
    }
    let mut out: Vec<Unnamed> = groups.into_iter().filter_map(|(_, members)| {
        let mut in_refs: Vec<usize> = members.iter().map(|m| m.0).collect();
        in_refs.dedup();
        if in_refs.len() < 2 {
            return None;
        }
        let shot = |m: &(usize, usize)| &refs[m.0].shots[m.1];
        let scene = common(members.iter().flat_map(|m| shot(m).scene.clone()), 4);
        let text = common(members.iter().flat_map(|m| shot(m).text.iter().map(|t| t.0.clone()).collect::<Vec<_>>()), 3);
        Some(Unnamed { shots: members.len(), example: (members[0].0, shot(&members[0]).start), scene, text, refs: in_refs })
    }).collect();
    out.sort_by_key(|u| std::cmp::Reverse((u.refs.len(), u.shots)));
    out.truncate(6);
    out
}

/// What the vocabulary says that isn't something it can say: a `how`, an `at` or a feel's measure it doesn't know
pub fn mistakes(v: &Vocabulary) -> Vec<String> {
    let mut out = Vec::new();
    let mut hows: Vec<(&String, &Option<String>)> = Vec::new();
    for t in v.places.iter().flat_map(|p| std::iter::once(p).chain(&p.things)).chain(&v.things).chain(&v.sounds) {
        hows.push((&t.name, &t.how));
    }
    for p in v.text.iter().chain(&v.said) {
        hows.push((&p.name, &p.how));
    }
    for sec in &v.sections {
        hows.push((&sec.name, &sec.how));
    }
    for (n, h) in hows {
        if let Some(h) = h.as_deref().filter(|h| !HOWS.contains(h)) {
            out.push(format!("{n}: how is always or usually, not {h:?}"));
        }
    }
    for s in &v.sections {
        if let Some(a) = s.at.as_deref().filter(|a| !ATS.contains(a)) {
            out.push(format!("{}: at is open, middle or close, not {a:?}", s.name));
        }
    }
    let known = |m: &str| m == "true_peak" || crate::ranges::NAMES.iter().any(|n| n.0 == m);
    for f in &v.feel {
        for m in f.measures.iter().filter(|m| !known(m)) {
            out.push(format!("feel {:?}: no measure {m} (raea videoform report lists them)", f.word));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shot;

    fn unit(v: &[f32]) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / n).collect()
    }

    #[test]
    fn terms_fire_where_they_stand_out_and_sections_where_they_fall() {
        // 3-wide space: the background points along z; "host" along x, its near-miss "crowd" along x and y
        let bg = Background { picture: vec![unit(&[0.1, 0.0, 1.0]), unit(&[0.0, 0.1, 1.0]), unit(&[0.05, 0.05, 1.0])], sound: vec![] };
        let host = Head { name: "host".into(), space: Some(Space::Picture), v: unit(&[1.0, 0.0, 0.0]), unlike: vec![unit(&[0.6, 0.8, 0.0])], how: Some("always".into()), ignore: false };
        let shot = |start: f64, end: f64, words: &str| Shot { start, end, words: words.into(), ..Default::default() };
        let r = |vs: &[[f32; 3]], words: &[&str]| Rows {
            shots: vs.iter().enumerate().map(|(i, _)| shot(i as f64, i as f64 + 1.0, words[i])).collect(),
            sig: vs.iter().flat_map(|v| unit(v)).collect(),
            ..Default::default()
        };
        let a = r(&[[1.0, 0.0, 0.1], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]], &["hi", "", "", "find it at Target"]);
        // b's first shot is nearer the near-miss than the host: not the host
        let b = r(&[[0.5, 0.8, 0.1], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]], &["", "", "", ""]);
        let cta = Pat { name: "store".into(), re: regex::Regex::new("(?i)find it at").unwrap(), said: true, how: None };
        let sec = Section { name: "call to action".into(), at: Some("close".into()), shows: vec!["store".into(), "nope".into()], how: Some("always".into()) };
        let rep = report(&[a, b], &[host], &bg, &[cta], &[sec]);
        assert_eq!((rep.terms[0].shots, rep.terms[0].refs.clone(), rep.terms[0].missing.clone()), (1, vec![true, false], vec![1]));
        assert_eq!((rep.terms[1].kind.as_str(), rep.terms[1].verdict.as_str()), ("said", "fires"));
        assert_eq!(rep.sections[0].found, vec![Some(3.0), None]);
        assert_eq!((rep.sections[0].missing.clone(), rep.sections[0].unknown.clone()), (vec![1], vec!["nope".to_string()]));
        assert_eq!(rep.unnamed[0].refs, vec![0, 1], "the background shots recur in both, and nothing names them");
    }
}
