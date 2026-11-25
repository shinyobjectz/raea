//! What was asked, against what was made (VF8f). Each passage of a group's documents and the person's words is set
//! against every shot of every reference by EmbeddingGemma 2: its vector against what each shot says and shows. A
//! shot carries a passage out when the two stand out of each other's backgrounds: the passage is nearer this shot
//! than to most shots, and the shot nearer this passage than to most passages (both z at least FIRES), and the pair
//! stands JOIN out of all pairs (a reference's best of many shots is always near), so a passage near everything, or a
//! shot near every passage, carries nothing out. Each reference of the passage's group is held
//! to its best such shot, and that shot's construction (where it falls, what's said or shown, how it's layered,
//! framed, heard) is how the passage was carried out there.
//!
//! Across groups: a passage and another group's passage that are each other's nearest, both standing out of their
//! backgrounds, say the same thing (two briefs' calls to action). Those joined are one ask. What their carrying out
//! shares, group by group, is a pattern: the ask, and how it's made, each part with its strength (the mean, over the
//! groups, of the share of its references made that way) and how common it is in shots at large. No counts cut it
//! off: one group is a brand's finding, two or more the videoform's, each said with its strength.

use crate::passages::Passage;
use raea_sys_videoform::rows::Shot;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// How far out of its background a pair stands to count (z)
pub const FIRES: f64 = 2.0;
/// A share of the shots or videos counted as the start or the end of one
const EDGE: f64 = 0.2;

/// A passage, from whom and for which group, with its two vectors: `find` (as a query, against what shots say and
/// show) and `same` (for passages saying the same thing)
#[derive(Clone, Debug, Default)]
pub struct Asked {
    pub passage: Passage,
    /// the document's title, or "you" for the person's words
    pub from: String,
    pub person: bool,
    pub group: Option<String>,
    pub at: String,
    pub find: Vec<f32>,
    pub same: Vec<f32>,
}

/// A reference, its group, and what each shot says and shows as vectors (empty where it says or shows nothing)
#[derive(Clone, Debug, Default)]
pub struct Made {
    pub media: String,
    pub title: String,
    pub group: Option<String>,
    pub shots: Vec<Shot>,
    pub said: Vec<Vec<f32>>,
    pub shown: Vec<Vec<f32>>,
}

/// Where a passage was carried out in one reference
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Moment {
    pub media: String,
    pub shot: usize,
    /// said or shown
    pub by: String,
    pub words: String,
    pub z: f64,
    pub how: Vec<String>,
}

/// One passage, and where it was carried out
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub id: String,
    pub said: String,
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// the references of its group, and those it's carried out in
    pub of: usize,
    pub moments: Vec<Moment>,
    /// the share of the other groups' references it fires in as well: what finding it by chance (or because every
    /// video of the kind does it) looks like
    #[serde(default)]
    pub chance: f64,
    /// each other group's references it fires in, and how many it has
    #[serde(default)]
    pub elsewhere: BTreeMap<String, (usize, usize)>,
}

/// A part of how an ask is made, with its strength and how common it is in all shots
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub how: String,
    pub strength: f64,
    pub usual: f64,
}

/// An ask, said in one group or several, and how it's made
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    /// each passage's id and words, with its group
    pub passages: Vec<(String, Option<String>, String)>,
    pub groups: Vec<String>,
    /// the person said it (their words outrank a document's)
    pub person: bool,
    /// the mean, over its groups, of the share of their references it's carried out in
    pub carried: f64,
    /// the mean, over its groups, of its passages' chance: carried out above it is what the ask set apart
    #[serde(default)]
    pub chance: f64,
    pub parts: Vec<Part>,
}

fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x * y) as f64).sum()
}

fn spread(v: &[f64]) -> (f64, f64) {
    let n = v.len().max(1) as f64;
    let m = v.iter().sum::<f64>() / n;
    let mut sq = 0.0;
    for x in v {
        sq += (x - m).powi(2);
    }
    let sd = (sq / n).sqrt().max(1e-6);
    (m, sd)
}

/// The (index, value) with the largest value
fn top_of(it: impl Iterator<Item = (usize, f64)>) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (k, v) in it {
        if best.is_none_or(|b| v > b.1) {
            best = Some((k, v));
        }
    }
    best
}

/// What a shot's construction is, in words: where it falls, what carries the words, its layers, framing and sound
pub fn how(shots: &[Shot], i: usize, by: &str) -> Vec<String> {
    let s = &shots[i];
    let end = shots.last().map_or(0.0, |l| l.end).max(1e-6);
    let mut out = vec![format!("{by}")];
    out.push(if s.start / end < EDGE { "at the open".into() } else if s.end / end > 1.0 - EDGE { "at the close".into() } else { "in the middle".into() });
    if i == 0 {
        out.push("the first shot".into());
    }
    if i + 1 == shots.len() {
        out.push("the last shot".into());
    }
    for l in s.layers.iter().flatten() {
        out.push(format!("layer {}", l.form));
        if let Some(k) = l.kind.as_ref().filter(|k| !k.is_empty()) {
            out.push(format!("layer {} showing {k}", l.form));
        }
    }
    if !s.text.is_empty() {
        out.push("words on screen".into());
    }
    out.push(format!("framed {}", s.framing));
    out.push(match s.faces {
        0 => "no face".into(),
        1 => "one face".into(),
        _ => "two faces or more".into(),
    });
    out.push(format!("heard: {}", s.heard));
    out.push(if s.seconds() < 1.5 { "a short shot".into() } else if s.seconds() > 4.0 { "a long shot".into() } else { "a mid-length shot".into() });
    out.dedup();
    out
}

/// What a shot shows, as words: its lines on screen
pub fn shown(s: &Shot) -> String {
    s.text.iter().map(|t| t.0.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" / ")
}

/// Each passage against every shot: where its group's references carry it out
pub fn found(asked: &[Asked], made: &[Made]) -> Vec<Found> {
    // every (reference, shot, by) that says or shows something, and its vector
    let mut cells: Vec<(usize, usize, &str, &Vec<f32>)> = Vec::new();
    for (r, m) in made.iter().enumerate() {
        for (i, v) in m.said.iter().enumerate().filter(|(_, v)| !v.is_empty()) {
            cells.push((r, i, "said", v));
        }
        for (i, v) in m.shown.iter().enumerate().filter(|(_, v)| !v.is_empty()) {
            cells.push((r, i, "shown", v));
        }
    }
    if cells.is_empty() || asked.is_empty() {
        return asked.iter().map(|a| Found { id: a.passage.id.clone(), said: a.passage.said(), from: a.from.clone(), group: a.group.clone(), ..Default::default() }).collect();
    }
    let sims: Vec<Vec<f64>> = asked.iter().map(|a| cells.iter().map(|c| dot(&a.find, c.3)).collect()).collect();
    let by_passage: Vec<(f64, f64)> = sims.iter().map(|s| spread(s)).collect();
    let by_cell: Vec<(f64, f64)> = (0..cells.len()).map(|c| spread(&sims.iter().map(|s| s[c]).collect::<Vec<_>>())).collect();
    // and every pair: a reference's best of its many shots is always near, so a moment is held to all of them too
    let all = spread(&sims.concat());
    asked.iter().enumerate().map(|(p, a)| {
        let mine = |r: usize| a.group.is_none() || made[r].group == a.group;
        let of = (0..made.len()).filter(|&r| mine(r)).count();
        let mut best: BTreeMap<usize, (f64, usize)> = BTreeMap::new();
        let mut elsewhere: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let gname = |r: usize| made[r].group.clone().unwrap_or_else(|| "all".into());
        for (c, cell) in cells.iter().enumerate() {
            let z = ((sims[p][c] - by_passage[p].0) / by_passage[p].1).min((sims[p][c] - by_cell[c].0) / by_cell[c].1);
            if z < FIRES || (sims[p][c] - all.0) / all.1 < JOIN {
                continue;
            }
            if !mine(cell.0) {
                elsewhere.entry(gname(cell.0)).or_default().insert(cell.0);
            } else if best.get(&cell.0).is_none_or(|b| z > b.0) {
                best.insert(cell.0, (z, c));
            }
        }
        let mut sizes: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for r in (0..made.len()).filter(|&r| !mine(r)) {
            sizes.entry(gname(r)).or_default().1 += 1;
        }
        for (g, rs) in &elsewhere {
            sizes.entry(g.clone()).or_default().0 = rs.len();
        }
        let n: usize = sizes.values().map(|v| v.0).sum();
        let total: usize = sizes.values().map(|v| v.1).sum();
        let chance = if total == 0 { 0.0 } else { (n as f64 / total as f64 * 100.0).round() / 100.0 };
        let elsewhere = sizes;
        let moments = best.into_iter().map(|(r, (z, c))| {
            let (_, i, by, _) = cells[c];
            let s = &made[r].shots[i];
            let words = if by == "said" { s.words.clone() } else { shown(s) };
            Moment { media: made[r].media.clone(), shot: i, by: by.into(), words, z: (z * 100.0).round() / 100.0, how: how(&made[r].shots, i, by) }
        }).collect();
        Found { id: a.passage.id.clone(), said: a.passage.said(), from: a.from.clone(), group: a.group.clone(), of, moments, chance, elsewhere }
    }).collect()
}

/// Two passages of different groups say the same thing when each is the other's nearest in the other's group and the
/// pair stands JOIN out of every pair across those groups: the nearest of many is always near, so a pair is held to
/// all pairs, not to its own passage's others
pub const JOIN: f64 = 3.5;

/// Passages of different groups that say the same thing, joined into asks
fn joined(asked: &[Asked]) -> Vec<Vec<usize>> {
    let mut root: Vec<usize> = (0..asked.len()).collect();
    fn top(root: &mut [usize], i: usize) -> usize {
        if root[i] != i {
            root[i] = top(root, root[i]);
        }
        root[i]
    }
    let groups: BTreeSet<&String> = asked.iter().filter_map(|a| a.group.as_ref()).collect();
    let of = |g: &String| -> Vec<usize> { (0..asked.len()).filter(|&q| asked[q].group.as_ref() == Some(g)).collect() };
    let gs: Vec<&String> = groups.into_iter().collect();
    for (k, ga) in gs.iter().enumerate() {
        for gb in &gs[k + 1..] {
            let (xa, xb) = (of(ga), of(gb));
            let sims: Vec<Vec<f64>> = xa.iter().map(|&p| xb.iter().map(|&q| dot(&asked[p].same, &asked[q].same)).collect()).collect();
            let (m, sd) = spread(&sims.concat());
            for (i, row) in sims.iter().enumerate() {
                let Some((j, s)) = top_of(row.iter().copied().enumerate()) else { continue };
                let back = top_of(sims.iter().map(|r| r[j]).enumerate()).map(|b| b.0);
                if back == Some(i) && (s - m) / sd >= JOIN {
                    let (a, b) = (top(&mut root, xa[i]), top(&mut root, xb[j]));
                    root[a] = b;
                }
            }
        }
    }
    let mut out: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for p in 0..asked.len() {
        let r = top(&mut root, p);
        out.entry(r).or_default().push(p);
    }
    out.into_values().collect()
}

/// The asks: passages joined across groups, each with how its groups made it
pub fn asks(asked: &[Asked], found: &[Found], made: &[Made]) -> Vec<Ask> {
    // how common each part is in all the shots that say or show something
    let mut usual: BTreeMap<String, f64> = BTreeMap::new();
    let mut total = 0.0f64;
    for m in made {
        for i in 0..m.shots.len() {
            for by in ["said", "shown"] {
                let has = if by == "said" { &m.said } else { &m.shown };
                if has.get(i).is_some_and(|v| !v.is_empty()) {
                    total += 1.0;
                    for h in how(&m.shots, i, by) {
                        *usual.entry(h).or_default() += 1.0;
                    }
                }
            }
        }
    }
    let mut out: Vec<Ask> = joined(asked).into_iter().map(|members| {
        let mut per: BTreeMap<String, (usize, Vec<&Moment>)> = BTreeMap::new();
        for &p in &members {
            let g = asked[p].group.clone().unwrap_or_else(|| "all".into());
            let e = per.entry(g).or_default();
            e.0 = e.0.max(found[p].of);
            e.1.extend(found[p].moments.iter());
        }
        // a reference counted once in a group: its strongest moment
        for (_, ms) in per.values_mut() {
            let mut strongest: BTreeMap<&str, &Moment> = BTreeMap::new();
            for m in ms.iter() {
                if strongest.get(m.media.as_str()).is_none_or(|s| s.z < m.z) {
                    strongest.insert(&m.media, m);
                }
            }
            *ms = strongest.into_values().collect();
        }
        let n = per.len().max(1) as f64;
        let carried = per.values().map(|(of, ms)| ms.len() as f64 / (*of).max(1) as f64).sum::<f64>() / n;
        // chance: how often its passages fire in the groups that didn't ask it (none left: it can't be told)
        let (mut hit, mut of_all) = (0usize, 0usize);
        for &p in &members {
            for (h, of) in found[p].elsewhere.iter().filter(|(g, _)| !per.contains_key(*g)).map(|(_, v)| v) {
                hit += h;
                of_all += of;
            }
        }
        let chance = if of_all == 0 { 0.0 } else { hit as f64 / of_all as f64 };
        let mut hows: BTreeMap<&str, f64> = BTreeMap::new();
        for (of, ms) in per.values() {
            for m in ms {
                for h in &m.how {
                    *hows.entry(h).or_default() += 1.0 / (*of).max(1) as f64 / n;
                }
            }
        }
        let mut parts: Vec<Part> = hows.into_iter().map(|(h, s)| Part { how: h.into(), strength: (s * 100.0).round() / 100.0, usual: (usual.get(h).copied().unwrap_or(0.0) / total.max(1.0) * 100.0).round() / 100.0 }).collect();
        // what sets this ask apart first: made this way more than shots usually are
        parts.retain(|p| p.strength > p.usual);
        parts.sort_by_key(|p| std::cmp::Reverse(((p.strength - p.usual) * 1e4) as i64));
        Ask {
            passages: members.iter().map(|&p| (asked[p].passage.id.clone(), asked[p].group.clone(), asked[p].passage.said())).collect(),
            groups: per.keys().cloned().collect(),
            person: members.iter().any(|&p| asked[p].person),
            carried: (carried * 100.0).round() / 100.0,
            chance: (chance * 100.0).round() / 100.0,
            parts,
        }
    }).collect();
    // the person's first, then the asks carried out furthest above chance
    out.sort_by(|a, b| b.person.cmp(&a.person).then(b.groups.len().cmp(&a.groups.len())).then((b.carried - b.chance).total_cmp(&(a.carried - a.chance))));
    out
}

/// A new job's passages against the asks learned: for each, the ask whose passage is nearest, when that pair stands
/// JOIN out of every pair of a new passage and a learned one (the nearest of many is always near)
pub fn nearest<'a>(vs: &[Vec<f32>], asks: &'a [Ask], vectors: &BTreeMap<String, Vec<f32>>) -> Vec<Option<(&'a Ask, f64)>> {
    let learned: Vec<(usize, &Vec<f32>)> = asks.iter().enumerate().flat_map(|(k, a)| a.passages.iter().filter_map(move |p| vectors.get(&p.0).map(|w| (k, w)))).collect();
    let sims: Vec<Vec<f64>> = vs.iter().map(|v| learned.iter().map(|(_, w)| dot(v, w)).collect()).collect();
    let (m, sd) = spread(&sims.concat());
    sims.iter().map(|row| {
        let (i, s) = top_of(row.iter().copied().enumerate())?;
        let z = (s - m) / sd;
        (z >= JOIN).then(|| (&asks[learned[i].0], (z * 100.0).round() / 100.0))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use raea_sys_videoform::rows::Layer;

    fn shot(start: f64, words: &str, layer: Option<&str>) -> Shot {
        let layers = layer.map(|k| vec![Layer { form: "screen".into(), at: [0.0, 0.5, 1.0, 0.5], seen: 1.0, kind: Some(k.into()) }]);
        Shot { start, end: start + 2.0, framing: "medium".into(), faces: 1, heard: "speech".into(), words: words.into(), layers, ..Default::default() }
    }

    fn unit(i: usize, n: usize, w: f32) -> Vec<f32> {
        let mut v = vec![0.05f32; n];
        v[i] = w;
        let s = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / s).collect()
    }

    const N: usize = 24;

    fn asked(id: &str, group: &str, dim: usize) -> Asked {
        Asked { passage: Passage::new("", id, 1), from: "brief".into(), group: Some(group.into()), find: unit(dim, N, 1.0), same: unit(dim, N, 1.0), ..Default::default() }
    }

    /// Two groups' briefs each ask for the app on screen (dimension 0) and something else; their videos carry the app
    /// out on a screen layer: one ask, across both, made on a screen showing a screenshot
    #[test]
    fn an_ask_in_two_briefs_is_found_where_it_was_made_and_how() {
        // eight shots a video, the second saying the app (dimension 0), the rest other things (11 on)
        let made = |g: &str, media: &str, k: usize| {
            let shots: Vec<Shot> = (0..8).map(|i| shot(i as f64 * 2.0, if i == 1 { "it's the app" } else { "hi" }, (i == 1).then_some("screenshot"))).collect();
            let said = (0..8).map(|i| if i == 1 { unit(0, N, 1.0) } else { unit(11 + (i + k) % 13, N, 1.0) }).collect();
            Made { media: media.into(), group: Some(g.into()), said, shown: vec![vec![]; 8], shots, ..Default::default() }
        };
        let videos = vec![made("A", "a1", 0), made("A", "a2", 3), made("B", "b1", 6), made("B", "b2", 9)];
        let mut ask = vec![asked("show the app", "A", 0), asked("show our app", "B", 0)];
        for d in 1..=10 {
            ask.push(asked(&format!("other {d}"), if d % 2 == 0 { "A" } else { "B" }, d));
        }
        let f = found(&ask, &videos);
        assert_eq!(f[0].moments.iter().map(|m| (m.media.as_str(), m.shot)).collect::<Vec<_>>(), [("a1", 1), ("a2", 1)], "found in its own group, in the shot that says it");
        assert!(f[2].moments.is_empty(), "what no shot says isn't found");
        let a = asks(&ask, &f, &videos);
        assert_eq!(a[0].groups, ["A", "B"]);
        assert_eq!(a[0].carried, 1.0);
        assert!(a[0].parts.iter().any(|p| p.how == "layer screen showing screenshot" && p.strength == 1.0 && p.usual == 0.13), "{:?}", a[0].parts);
        let vs: BTreeMap<String, Vec<f32>> = ask.iter().map(|x| (x.passage.id.clone(), x.same.clone())).collect();
        // a new brief: one passage asking for the app, twenty about other things
        let new: Vec<Vec<f32>> = std::iter::once(unit(0, N, 1.0)).chain((0..20).map(|i| unit(13 + i % 11, N, 1.0))).collect();
        let near = nearest(&new, &a, &vs);
        assert_eq!(near[0].map(|n| n.0.groups.len()), Some(2), "a new brief's ask finds the one learned");
        assert!(near[1].is_none(), "one like nothing learned is new");
    }
}
