//! The shot vocabulary (docs/videoform.feature VF6): the references' shots gathered into the kinds of shot the
//! videoform is made of, each named in the vocabulary's words, with its scale, people, talk and length, and how many
//! references have it. A shot is described by how it stands against the vocabulary's picture terms and the background
//! (each phrase's dot product with its pooled picture vector, standardised over every shot), less half its own
//! reference's mean: what one video shows throughout (its street, its people) is that video's content, and what's left
//! is the kind of shot. Shots are gathered by average linkage on that and on their measures; kinds seen in one
//! reference only are content, and their shots join the nearest recurring kind when they're near enough.

use crate::rows::{Rows, Shot};
use crate::vocab::{Background, Head, Space, common, dot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How much of its reference's own look is taken from a shot
const OWN_LOOK: f32 = 0.5;
/// How much a difference in the measures (scale, people, talk) weighs beside the picture
const MEASURES: f32 = 0.6;
/// Groups merge while they're nearer than this (1 less the cosine, plus the measures')
const LINK: f32 = 0.9;
/// A shot of a one-reference group joins the nearest recurring kind within this
const JOINS: f32 = 1.1;
/// A term names a kind when the kind's shots stand this far out on it (standard deviations, on average)
const NAMES: f32 = 0.5;

/// A kind of shot across the references
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Kind {
    /// what it's called: its vocabulary terms and its scale and people, or what's seen when no term stands out
    pub name: String,
    /// the vocabulary's terms it stands out on, most first
    pub terms: Vec<String>,
    /// its shots: [reference, shot]
    pub shots: Vec<(usize, usize)>,
    /// the references it's in
    pub refs: Vec<usize>,
    /// its scale (close-up, medium, wide, no people) and people (no one, one person, two or more), each a share of its shots
    pub framing: BTreeMap<String, f64>,
    pub people: BTreeMap<String, f64>,
    /// how its shots are layered (VF8e): each layer form's share of its shots, "" for one picture alone
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layers: BTreeMap<String, f64>,
    /// what its main layer shows (VF9d), told as a kit's asset is (testimonial, logo, screenshot...): each kind's share
    /// of its layered shots
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shows: BTreeMap<String, f64>,
    /// the share of its shots with words said, and with the camera moving
    pub talking: f64,
    pub moving: f64,
    /// its shots' length: the shortest tenth, the median, the longest tenth, seconds
    pub seconds: [f64; 3],
    /// where in its reference it falls, on average: 0 the start, 1 the end
    pub at: f64,
}

/// The kinds, and for each reference which kind each shot is (None: that reference's own)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Kinds {
    pub kinds: Vec<Kind>,
    pub of: Vec<Vec<Option<usize>>>,
}

/// A shot's people, in words
pub fn people(s: &Shot) -> &'static str {
    match s.faces {
        0 => "no one",
        1 => "one person",
        _ => "two or more people",
    }
}

/// A shot's main layer (VF8e): a person cut out, a picture inset, a card, a split, a screen, a backdrop, in that order;
/// "" for one picture alone or layers not read
pub fn layered(s: &Shot) -> &'static str {
    ["cutout", "inset", "card", "split", "screen", "backdrop"].into_iter().find(|f| s.has_layer(f)).unwrap_or("")
}

/// What a shot's main layer shows (VF9d): a kit's asset kind, "" when it has none or it wasn't told
pub fn shows(s: &Shot) -> &str {
    let form = layered(s);
    s.layers.iter().flatten().find(|l| l.form == form).and_then(|l| l.kind.as_deref()).unwrap_or("")
}

/// A layer form and what it shows, said: "a card laid over, showing a testimonial"
pub fn said_showing(form: &str, kind: &str) -> String {
    match (said(form), kind) {
        ("", _) => String::new(),
        (f, "") => f.into(),
        (f, k) => format!("{f}, showing a {k}"),
    }
}

/// A layer form, said
pub fn said(form: &str) -> &'static str {
    match form {
        "cutout" => "a person cut out over the picture",
        "inset" => "a picture inset",
        "card" => "a card laid over",
        "split" => "a split frame",
        "screen" => "a screen or page",
        "backdrop" => "a plain backdrop",
        _ => "",
    }
}

fn measures(s: &Shot) -> [String; 4] {
    [s.framing.clone(), people(s).into(), (!s.words.trim().is_empty()).to_string(), layered(s).into()]
}

/// Each shot's description against the phrases, standardised and less its reference's own look, unit length
fn describe(refs: &[Rows], phrases: &[&[f32]]) -> Vec<Vec<f32>> {
    let mut d: Vec<Vec<f32>> = Vec::new();
    let mut whose: Vec<usize> = Vec::new();
    for (r, rows) in refs.iter().enumerate() {
        let sig = rows.sig.len() / rows.shots.len().max(1);
        for s in 0..rows.shots.len() {
            let w = &rows.sig[s * sig..(s + 1) * sig];
            d.push(phrases.iter().map(|p| dot(p, w)).collect());
            whose.push(r);
        }
    }
    let n = d.len().max(1) as f32;
    for f in 0..phrases.len() {
        let m = d.iter().map(|x| x[f]).sum::<f32>() / n;
        let sd = (d.iter().map(|x| (x[f] - m).powi(2)).sum::<f32>() / n).sqrt().max(1e-6);
        d.iter_mut().for_each(|x| x[f] = (x[f] - m) / sd);
    }
    for r in 0..refs.len() {
        let mine: Vec<usize> = (0..d.len()).filter(|&i| whose[i] == r).collect();
        let mean: Vec<f32> = (0..phrases.len()).map(|f| mine.iter().map(|&i| d[i][f]).sum::<f32>() / mine.len().max(1) as f32).collect();
        mine.iter().for_each(|&i| d[i].iter_mut().zip(&mean).for_each(|(x, m)| *x -= OWN_LOOK * m));
    }
    d
}

fn unit(v: &[f32]) -> Vec<f32> {
    let len = dot(v, v).sqrt().max(1e-6);
    v.iter().map(|x| x / len).collect()
}

/// Average linkage: groups merged nearest first while they're nearer than `link`
fn gather(dist: &[f32], n: usize, link: f32) -> Vec<Vec<usize>> {
    let mut d = dist.to_vec();
    let mut groups: Vec<Option<Vec<usize>>> = (0..n).map(|i| Some(vec![i])).collect();
    loop {
        let mut best: Option<(f32, usize, usize)> = None;
        for a in (0..n).filter(|&a| groups[a].is_some()) {
            for b in (a + 1..n).filter(|&b| groups[b].is_some()) {
                if best.is_none_or(|x| d[a * n + b] < x.0) {
                    best = Some((d[a * n + b], a, b));
                }
            }
        }
        let Some((_, a, b)) = best.filter(|x| x.0 <= link) else { break };
        let (na, nb) = (groups[a].as_ref().unwrap().len() as f32, groups[b].as_ref().unwrap().len() as f32);
        for k in (0..n).filter(|&k| k != a && k != b && groups[k].is_some()) {
            let v = (na * d[a * n + k] + nb * d[b * n + k]) / (na + nb);
            d[a * n + k] = v;
            d[k * n + a] = v;
        }
        let moved = groups[b].take().unwrap();
        groups[a].as_mut().unwrap().extend(moved);
    }
    groups.into_iter().flatten().collect()
}

fn shares(of: impl Iterator<Item = String>) -> BTreeMap<String, f64> {
    let mut m: BTreeMap<String, f64> = BTreeMap::new();
    let mut n = 0.0;
    of.for_each(|k| {
        *m.entry(k).or_default() += 1.0;
        n += 1.0;
    });
    m.values_mut().for_each(|v| *v /= n);
    m
}

/// The most of a share map, when it's most of them
fn most(m: &BTreeMap<String, f64>) -> Option<&str> {
    m.iter().max_by(|(_, a), (_, b)| a.total_cmp(b)).filter(|x| *x.1 >= 0.6).map(|x| x.0.as_str())
}

/// The references' shot kinds, named from the vocabulary's picture terms
pub fn kinds(refs: &[Rows], heads: &[Head], bg: &Background) -> Kinds {
    let terms: Vec<&Head> = heads.iter().filter(|h| h.space == Some(Space::Picture) && !h.ignore).collect();
    let phrases: Vec<&[f32]> = terms.iter().map(|h| h.v.as_slice()).chain(terms.iter().flat_map(|h| h.unlike.iter().map(Vec::as_slice))).chain(bg.picture.iter().map(Vec::as_slice)).collect();
    let shots: Vec<(usize, usize)> = refs.iter().enumerate().flat_map(|(r, rows)| (0..rows.shots.len()).map(move |s| (r, s))).collect();
    let shot = |i: usize| &refs[shots[i].0].shots[shots[i].1];
    let desc: Vec<Vec<f32>> = describe(refs, &phrases);
    let u: Vec<Vec<f32>> = desc.iter().map(|d| unit(d)).collect();
    let ms: Vec<[String; 4]> = (0..shots.len()).map(|i| measures(shot(i))).collect();
    let n = shots.len();
    let mut dist = vec![0f32; n * n];
    for a in 0..n {
        for b in a + 1..n {
            let m = ms[a].iter().zip(&ms[b]).filter(|(x, y)| x != y).count() as f32 / 3.0;
            let v = 1.0 - dot(&u[a], &u[b]) + MEASURES * m;
            dist[a * n + b] = v;
            dist[b * n + a] = v;
        }
    }
    let refs_of = |g: &[usize]| {
        let mut r: Vec<usize> = g.iter().map(|&i| shots[i].0).collect();
        r.sort_unstable();
        r.dedup();
        r
    };
    let (mut recurring, own): (Vec<Vec<usize>>, Vec<Vec<usize>>) = gather(&dist, n, LINK).into_iter().partition(|g| refs_of(g).len() >= 2);
    let mut joins: Vec<(usize, usize)> = Vec::new();
    for i in own.into_iter().flatten() {
        let near = recurring.iter().enumerate().map(|(k, g)| (k, g.iter().map(|&j| dist[i * n + j]).sum::<f32>() / g.len() as f32)).min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((k, _)) = near.filter(|x| x.1 <= JOINS) {
            joins.push((k, i));
        }
    }
    joins.into_iter().for_each(|(k, i)| recurring[k].push(i));
    recurring.sort_by_key(|g| std::cmp::Reverse((refs_of(g).len(), g.len())));
    let mut of: Vec<Vec<Option<usize>>> = refs.iter().map(|r| vec![None; r.shots.len()]).collect();
    let kinds = recurring.iter().enumerate().map(|(k, g)| {
        g.iter().for_each(|&i| of[shots[i].0][shots[i].1] = Some(k));
        let mut stand: Vec<(f32, &str)> = (0..terms.len()).map(|t| (g.iter().map(|&i| desc[i][t]).sum::<f32>() / g.len() as f32, terms[t].name.as_str())).filter(|x| x.0 >= NAMES).collect();
        stand.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut lens: Vec<f64> = g.iter().map(|&i| shot(i).seconds()).collect();
        lens.sort_by(f64::total_cmp);
        let q = |p: f64| lens[((lens.len() - 1) as f64 * p).round() as usize];
        let [framing, people, layers] = [|s: &Shot| s.framing.clone(), |s: &Shot| people(s).to_string(), |s: &Shot| layered(s).to_string()].map(|f| shares(g.iter().map(|&i| f(shot(i)))));
        let at = g.iter().map(|&i| shot(i).start / refs[shots[i].0].shots.last().map_or(1.0, |l| l.end).max(1e-6)).sum::<f64>() / g.len() as f64;
        let frac = |f: &dyn Fn(&Shot) -> bool| g.iter().filter(|&&i| f(shot(i))).count() as f64 / g.len() as f64;
        let mut kind = Kind {
            terms: stand.iter().take(2).map(|x| x.1.to_string()).collect(),
            refs: refs_of(g),
            shots: g.iter().map(|&i| shots[i]).collect(),
            talking: frac(&|s| !s.words.trim().is_empty()),
            moving: frac(&|s| s.motion == "moving"),
            seconds: [q(0.1), q(0.5), q(0.9)],
            framing,
            people,
            layers: if layers.keys().all(String::is_empty) { BTreeMap::new() } else { layers },
            shows: shares(g.iter().map(|&i| shows(shot(i)).to_string()).filter(|k| !k.is_empty())),
            at,
            name: String::new(),
        };
        kind.shots.sort_unstable();
        kind.name = name(&kind, g.iter().flat_map(|&i| shot(i).scene.clone()));
        kind
    }).collect();
    Kinds { kinds, of }
}

/// A kind's name: its terms, or what Apple Vision sees most in it, then its scale and people
fn name(k: &Kind, scene: impl Iterator<Item = String>) -> String {
    let what = if k.terms.is_empty() {
        format!("unnamed ({})", common(scene, 1).pop().unwrap_or_else(|| "nothing seen".into()))
    } else {
        k.terms.join(", ")
    };
    let layer = most(&k.layers).map(|f| said_showing(f, most(&k.shows).unwrap_or(""))).filter(|l| !l.is_empty());
    let how: Vec<String> = [most(&k.framing).filter(|f| *f != "no people").map(str::to_string), most(&k.people).map(str::to_string), layer].into_iter().flatten().collect();
    if how.is_empty() { what } else { format!("{what}: {}", how.join(", ")) }
}

/// The kinds, as the person's AI reads them, with the references' short names
pub fn lines(k: &Kinds, names: &[&str]) -> Vec<String> {
    let n = names.len();
    let all: usize = k.of.iter().map(Vec::len).sum();
    let own: usize = k.of.iter().flatten().filter(|x| x.is_none()).count();
    let mut out = vec![format!("Shot kinds ({} across {all} shots; {own} shots are their own reference's):", k.kinds.len())];
    for kind in &k.kinds {
        let talk = if kind.talking >= 0.6 { ", talking" } else if kind.talking <= 0.2 { ", no words" } else { "" };
        let place = match kind.at {
            a if a < 0.25 => ", near the start",
            a if a > 0.7 => ", near the end",
            _ => "",
        };
        let s = kind.seconds;
        out.push(format!("  {}: {} shots in {} of {n}{talk}, {:.1} s ({:.1} to {:.1}){place}", kind.name, kind.shots.len(), kind.refs.len(), s[1], s[0], s[2]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit3(v: [f32; 3]) -> Vec<f32> {
        unit(&v)
    }

    #[test]
    fn shots_gather_into_kinds_across_references_and_take_the_terms_they_stand_out_on() {
        // two kinds in each of two references: a talking medium shot along x, a product with no one along y
        let talk = |start: f64| Shot { start, end: start + 1.0, framing: "medium".into(), faces: 1, words: "so".into(), ..Default::default() };
        let product = |start: f64| Shot { start, end: start + 2.0, framing: "no people".into(), ..Default::default() };
        let r = |look: f32| Rows {
            shots: vec![talk(0.0), product(1.0), talk(3.0), product(4.0), talk(6.0)],
            sig: [[1.0, 0.1, look], [0.1, 1.0, look], [1.0, 0.0, look], [0.0, 1.0, look], [0.9, 0.1, look]].into_iter().flat_map(unit3).collect(),
            ..Default::default()
        };
        let head = |name: &str, v: [f32; 3]| Head { name: name.into(), space: Some(Space::Picture), v: unit3(v), ..Default::default() };
        let bg = Background { picture: vec![unit3([0.0, 0.0, 1.0]), unit3([0.5, 0.5, 0.5])], sound: vec![] };
        let k = kinds(&[r(0.2), r(-0.2)], &[head("stranger", [1.0, 0.0, 0.0]), head("product", [0.0, 1.0, 0.0])], &bg);
        assert_eq!(k.kinds.len(), 2, "{:?}", k.kinds.iter().map(|k| &k.name).collect::<Vec<_>>());
        assert_eq!(k.kinds[0].name, "stranger: medium, one person");
        assert_eq!(k.kinds[0].refs, vec![0, 1]);
        assert_eq!((k.kinds[1].name.as_str(), k.kinds[1].seconds[1]), ("product: no one", 2.0));
        assert_eq!(k.of[1], vec![Some(0), Some(1), Some(0), Some(1), Some(0)]);
    }
}
