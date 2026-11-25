//! The rules the person's AI reads in a group's documents (briefs.ts, VF8f), measured on its references: what a
//! similarity can't tell, a "never", an order, a choice, said in measured things and held to each video. A rule that
//! two groups or more say alike (the same kind over the same things) and that holds in them is the videoform's; one
//! only a group says is its brand's. Each is said with how strongly it holds, never cut off by a count.

use raea_core_brief::videoform::{BriefRule, RULE_KINDS};
use crate::link::Made;
use raea_sys_videoform::rows::Shot;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A share of a video counted as its open or its close
const EDGE: f64 = 0.2;

/// A rule measured: the references of its groups it holds in, and those it can be held to
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Held {
    pub text: String,
    pub kind: String,
    pub what: Vec<String>,
    /// each group's (holds, of): of is the references it could be held to there
    pub groups: BTreeMap<String, (usize, usize)>,
    /// the mean, over its groups, of the share it holds in
    pub strength: f64,
    /// the references it's broken in, by title
    pub broken: Vec<String>,
}

/// A measured thing, shot by shot (or the whole video, for its length)
enum Atom {
    Said(Regex),
    Shown(Regex),
    Layer(String, Option<String>),
    Captions,
    Faces(usize),
    Seconds(f64, f64),
}

fn atom(w: &str) -> Result<Atom, String> {
    let (k, v) = w.split_once(':').unwrap_or((w, ""));
    let re = |v: &str| Regex::new(&format!("(?i){v}")).map_err(|e| format!("{w}: {e}"));
    Ok(match k.trim() {
        "said" => Atom::Said(re(v)?),
        "shown" => Atom::Shown(re(v)?),
        "layer" => {
            let (f, kind) = v.split_once('-').map_or((v, None), |(f, k)| (f, Some(k.to_string())));
            Atom::Layer(f.to_string(), kind)
        }
        "captions" => Atom::Captions,
        "faces" => Atom::Faces(v.parse().map_err(|_| format!("{w}: faces:N"))?),
        "seconds" => {
            let (lo, hi) = v.split_once('-').ok_or(format!("{w}: seconds:LO-HI"))?;
            Atom::Seconds(lo.parse().map_err(|_| format!("{w}: seconds:LO-HI"))?, hi.parse().map_err(|_| format!("{w}: seconds:LO-HI"))?)
        }
        _ => return Err(format!("{w}: said:, shown:, layer:, captions, faces: or seconds:")),
    })
}

/// Words on screen while words are said: a caption (a title card's words come with no speech)
fn captioned(s: &Shot) -> bool {
    !s.text.is_empty() && !s.words.trim().is_empty() && s.text.iter().any(|t| t.1[1] > 0.3)
}

/// The shots it's in; for the whole video's length, all or none
fn shots_with(a: &Atom, shots: &[Shot]) -> Vec<bool> {
    let end = shots.last().map_or(0.0, |s| s.end);
    shots.iter().map(|s| match a {
        Atom::Said(r) => r.is_match(&s.words),
        Atom::Shown(r) => s.text.iter().any(|t| r.is_match(&t.0)),
        Atom::Layer(f, k) => s.layers.iter().flatten().any(|l| &l.form == f && k.as_ref().is_none_or(|k| l.kind.as_ref() == Some(k))),
        Atom::Captions => captioned(s),
        Atom::Faces(n) => s.faces >= *n,
        Atom::Seconds(lo, hi) => (*lo..=*hi).contains(&end),
    }).collect()
}

/// Whether a rule holds in one video; None when it can't be held to it (an order whose things aren't both there)
fn holds(kind: &str, atoms: &[Atom], shots: &[Shot]) -> Option<bool> {
    let end = shots.last().map_or(0.0, |s| s.end).max(1e-6);
    let has: Vec<Vec<bool>> = atoms.iter().map(|a| shots_with(a, shots)).collect();
    let any = |v: &Vec<bool>| v.contains(&true);
    let within = |v: &Vec<bool>, f: &dyn Fn(&Shot) -> bool| v.iter().zip(shots).any(|(x, s)| *x && f(s));
    match kind {
        "always" => Some(has.iter().all(any)),
        "never" => Some(!has.iter().any(any)),
        "either" => Some(has.iter().filter(|v| any(v)).count() == 1),
        "before" => {
            let first = |v: &Vec<bool>| Some(v.iter().take_while(|x| !**x).count()).filter(|n| *n < v.len());
            let (a, b) = (first(has.first()?)?, first(has.get(1)?)?);
            Some(a <= b)
        }
        "open" => Some(has.iter().all(|v| within(v, &|s| s.start / end < EDGE))),
        "close" => Some(has.iter().all(|v| within(v, &|s| s.end / end > 1.0 - EDGE))),
        _ => None,
    }
}

/// A rule's things, said alike whatever the case or spaces
fn key(r: &BriefRule) -> String {
    let mut w: Vec<String> = r.what.iter().map(|w| w.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    if r.kind != "before" {
        w.sort();
    }
    format!("{} {}", r.kind, w.join(" | "))
}

/// Each rule, measured on its group's references, rules said alike in several groups joined: the videoform's (said in
/// two groups or more, or for all), then the brands'
pub fn held(rules: &[BriefRule], made: &[Made]) -> Result<(Vec<Held>, Vec<Held>), String> {
    let mut by: BTreeMap<String, Held> = BTreeMap::new();
    for r in rules {
        if !RULE_KINDS.contains(&r.kind.as_str()) {
            return Err(format!("briefs.ts: \"{}\" is {}; a rule is one of {}", r.text, r.kind, RULE_KINDS.join(", ")));
        }
        let atoms: Vec<Atom> = r.what.iter().map(|w| atom(w)).collect::<Result<_, _>>().map_err(|e| format!("briefs.ts: \"{}\": {e}", r.text))?;
        let h = by.entry(key(r)).or_insert_with(|| Held { text: r.text.clone(), kind: r.kind.clone(), what: r.what.clone(), ..Default::default() });
        for m in made.iter().filter(|m| r.group.is_none() || m.group == r.group) {
            let Some(ok) = holds(&r.kind, &atoms, &m.shots) else { continue };
            let (title, g) = (&m.title, &m.group);
            let e = h.groups.entry(g.clone().unwrap_or_else(|| "all".into())).or_default();
            e.1 += 1;
            if ok {
                e.0 += 1;
            } else if !h.broken.contains(title) {
                h.broken.push(title.clone());
            }
        }
        if r.group.is_none() {
            h.groups.entry("all".into()).or_default();
        }
    }
    let mut form = Vec::new();
    let mut brand = Vec::new();
    for (_, mut h) in by {
        let n = h.groups.len().max(1) as f64;
        h.strength = (h.groups.values().map(|(a, b)| *a as f64 / (*b).max(1) as f64).sum::<f64>() / n * 100.0).round() / 100.0;
        if h.groups.len() > 1 || h.groups.contains_key("all") { form.push(h) } else { brand.push(h) }
    }
    for v in [&mut form, &mut brand] {
        v.sort_by(|x, y| y.strength.partial_cmp(&x.strength).unwrap_or(std::cmp::Ordering::Equal));
    }
    Ok((form, brand))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(start: f64, words: &str, on: &str) -> Shot {
        let text = if on.is_empty() { vec![] } else { vec![(on.to_string(), [0.1, 0.7, 0.8, 0.05])] };
        Shot { start, end: start + 2.0, words: words.into(), text, ..Default::default() }
    }

    fn rule(kind: &str, what: &[&str], group: &str) -> BriefRule {
        BriefRule { text: format!("{kind} {what:?}"), kind: kind.into(), what: what.iter().map(|w| w.to_string()).collect(), group: Some(group.into()), from: None }
    }

    #[test]
    fn rules_are_measured_and_those_said_in_two_groups_are_the_videoforms() {
        let a = vec![shot(0.0, "your gift exchange", "Gift Exchange"), shot(2.0, "then your wishlist", ""), shot(8.0, "free on elfster", "")];
        let b = vec![shot(0.0, "the wishlist first", ""), shot(2.0, "gift exchange", ""), shot(8.0, "it's free", "")];
        let made = vec![Made { title: "a".into(), group: Some("A".into()), shots: a, ..Default::default() }, Made { title: "b".into(), group: Some("B".into()), shots: b, ..Default::default() }];
        let rules = vec![
            rule("before", &["said:gift exchange", "said:wishlist"], "A"),
            rule("before", &["said:gift exchange", "said:wishlist"], "B"),
            rule("never", &["said:white elephant"], "A"),
            rule("close", &["said:free"], "B"),
        ];
        let (form, brand) = held(&rules, &made).unwrap();
        assert_eq!(form.len(), 1, "the order is said in both groups");
        assert_eq!((form[0].strength, form[0].broken.clone()), (0.5, vec!["b".to_string()]), "it holds in A's video, not B's");
        assert_eq!(brand.iter().map(|h| (h.kind.as_str(), h.strength)).collect::<Vec<_>>(), [("close", 1.0), ("never", 1.0)]);
        assert!(held(&[rule("sometimes", &["captions"], "A")], &made).is_err());
        assert!(held(&[rule("always", &["colour:red"], "A")], &made).is_err());
    }
}
