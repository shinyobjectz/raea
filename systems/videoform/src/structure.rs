//! The structure (docs/videoform.feature VF7): the references cut into the sections the vocabulary names, in order,
//! each a slot with where it starts, how long it runs, how many shots it has and the shot kinds (VF6) it's made of;
//! and the template, a NAME.script.ts a project made to the videoform starts from (a scene a slot, a beat a kind),
//! checked by `raea spec` as any script is. A reference's section starts at the first shot it's shown on (vocab.rs's
//! hits) after the one before's start; the first section found starts the video. Sections shown by the same terms
//! can't be told apart where they meet, and the report says so.
//! With no sections in the vocabulary, the slots are the open, the middle and the close: the first quarter, the half
//! and the last quarter.

use crate::kinds::Kinds;
use crate::rows::Rows;
use crate::vocab::SectionFound;
use raea_core_brief::Script;
use raea_core_brief::script::{Beat, Scene};
use serde::{Deserialize, Serialize};

/// A kind of shot a slot is made of, in this many references or more of those that have the slot
const MOSTLY: f64 = 0.5;

/// A shot kind inside a slot
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KindIn {
    pub kind: usize,
    pub name: String,
    /// how many of the references with the slot have it there
    pub refs: usize,
    /// its shots in the slot, the median over those references, and their median length, seconds
    pub count: f64,
    pub seconds: f64,
    /// where in the slot it falls, on average: 0 the slot's start, 1 its end
    pub at: f64,
}

/// A section across the references
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub name: String,
    pub at: Option<String>,
    /// the terms and patterns it's shown by
    pub shows: Vec<String>,
    /// the references it's found in
    pub refs: Vec<usize>,
    /// where it starts (a share of the video, earliest and latest), its length in seconds and its shots (the shortest
    /// tenth, the median, the longest tenth)
    pub start: [f64; 2],
    pub seconds: [f64; 3],
    pub shots: [f64; 3],
    /// the kinds it's made of, in the order they come
    pub kinds: Vec<KindIn>,
    /// how it's cut into, most often
    pub cut: String,
    /// what's said in it where it's shown, from the reference nearest its median length: [reference, words]
    pub said: Option<(usize, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Structure {
    /// the references' lengths, seconds, and their shots
    pub seconds: [f64; 3],
    pub shots: [f64; 3],
    pub slots: Vec<Slot>,
}

/// The shortest tenth, the median, the longest tenth
fn spread(mut v: Vec<f64>) -> [f64; 3] {
    if v.is_empty() {
        return [0.0; 3];
    }
    v.sort_by(f64::total_cmp);
    let q = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    [q(0.1), q(0.5), q(0.9)]
}

fn length(r: &Rows) -> f64 {
    r.shots.last().map_or(0.0, |s| s.end)
}

/// Where each slot starts in each reference, a shot, when it's found there
fn starts(refs: &[Rows], sections: &[SectionFound]) -> Vec<Vec<Option<usize>>> {
    refs.iter().enumerate().map(|(r, rows)| {
        if sections.is_empty() {
            let at = |p: f64| rows.shots.iter().position(|s| s.start >= length(rows) * p);
            return vec![Some(0), at(0.25), at(0.75)];
        }
        let mut out: Vec<Option<usize>> = Vec::new();
        let mut prev: Option<usize> = None;
        for sec in sections {
            let s = sec.hits[r].iter().copied().find(|&h| prev.is_none_or(|p| h > p));
            prev = s.or(prev);
            out.push(s);
        }
        if let Some(first) = out.iter_mut().flatten().next() {
            *first = 0;
        }
        out
    }).collect()
}

/// Each reference's slots, in the vocabulary's order: the slot's [first shot, end shot) where it's found there, to
/// the next slot found or the end (VF8's rules are held to these too)
pub fn spans(refs: &[Rows], sections: &[SectionFound]) -> Vec<Vec<Option<(usize, usize)>>> {
    starts(refs, sections).iter().zip(refs).map(|(st, rows)| (0..st.len()).map(|i| {
        let s = st[i]?;
        Some((s, st[i + 1..].iter().flatten().copied().find(|&e| e > s).unwrap_or(rows.shots.len())))
    }).collect()).collect()
}

/// The references' structure, from their rows, their shot kinds and where the vocabulary's sections are shown
pub fn structure(refs: &[Rows], kinds: &Kinds, sections: &[SectionFound], shows: &[Vec<String>]) -> Structure {
    let at = spans(refs, sections);
    let names: Vec<(String, Option<String>)> = if sections.is_empty() {
        ["open", "middle", "close"].map(|n| (n.into(), Some(n.into()))).to_vec()
    } else {
        sections.iter().map(|SectionFound { name, at, .. }| (name.clone(), at.clone())).collect()
    };
    let slots = names.into_iter().enumerate().map(|(i, (name, place))| {
        let spans: Vec<(usize, usize, usize)> = at.iter().enumerate().filter_map(|(r, sp)| sp[i].map(|(s, e)| (r, s, e))).collect();
        let secs = |&(r, s, e): &(usize, usize, usize)| refs[r].shots[e - 1].end - refs[r].shots[s].start;
        let seconds = spread(spans.iter().map(secs).collect());
        let mut kinds_in: Vec<KindIn> = kinds.kinds.iter().enumerate().filter_map(|(k, kind)| {
            let per: Vec<(usize, f64, Vec<f64>)> = spans.iter().filter_map(|&(r, s, e)| {
                let mine: Vec<usize> = (s..e).filter(|&j| kinds.of[r][j] == Some(k)).collect();
                let span = (e - s).max(1) as f64;
                (!mine.is_empty()).then(|| (mine.len(), mine.iter().map(|&j| (j - s) as f64 / span).sum::<f64>() / mine.len() as f64, mine.iter().map(|&j| refs[r].shots[j].seconds()).collect()))
            }).collect();
            if per.is_empty() || (per.len() as f64) < spans.len() as f64 * MOSTLY {
                return None;
            }
            Some(KindIn {
                kind: k,
                name: kind.name.clone(),
                refs: per.len(),
                count: spread(per.iter().map(|p| p.0 as f64).collect())[1],
                seconds: spread(per.iter().flat_map(|p| p.2.clone()).collect())[1],
                at: per.iter().map(|p| p.1).sum::<f64>() / per.len() as f64,
            })
        }).collect();
        kinds_in.sort_by(|a, b| a.at.total_cmp(&b.at));
        let mut cuts: Vec<&str> = spans.iter().map(|&(r, s, _)| refs[r].shots[s].transition.as_str()).filter(|t| !t.is_empty()).collect();
        cuts.sort_unstable();
        let cut = cuts.chunk_by(|a, b| a == b).max_by_key(|c| c.len()).and_then(|c| c.first()).unwrap_or(&"cut").to_string();
        let typical = spans.iter().min_by(|a, b| (secs(a) - seconds[1]).abs().total_cmp(&(secs(b) - seconds[1]).abs()));
        let said = typical.and_then(|&(r, s, e)| {
            let shown = |j: &usize| sections.get(i).is_none_or(|sec| sec.hits[r].contains(j));
            let words: Vec<&str> = (s..e).filter(shown).map(|j| refs[r].shots[j].words.trim()).filter(|w| !w.is_empty()).collect();
            (!words.is_empty()).then(|| (r, words.join(" ")))
        });
        let share = |&(r, s, _): &(usize, usize, usize)| refs[r].shots[s].start / length(&refs[r]).max(1e-6);
        let starts: Vec<f64> = spans.iter().map(share).collect();
        Slot {
            name,
            at: place,
            shows: shows.get(i).cloned().unwrap_or_default(),
            refs: spans.iter().map(|x| x.0).collect(),
            start: [starts.iter().copied().fold(f64::INFINITY, f64::min).min(1.0), starts.iter().copied().fold(0.0, f64::max)],
            seconds,
            shots: spread(spans.iter().map(|&(_, s, e)| (e - s) as f64).collect()),
            kinds: kinds_in,
            cut,
            said,
        }
    }).collect();
    Structure { seconds: spread(refs.iter().map(length).collect()), shots: spread(refs.iter().map(|r| r.shots.len() as f64).collect()), slots }
}

fn words(s: &str, n: usize) -> String {
    let w: Vec<&str> = s.split_whitespace().collect();
    if w.len() <= n { w.join(" ") } else { format!("{} …", w[..n].join(" ")) }
}

/// The template: a scene a slot, a beat a kind it's made of, as long as it runs there
pub fn template(st: &Structure, title: &str, names: &[&str]) -> Script {
    let n = names.len();
    let scenes = st.slots.iter().filter(|s| !s.refs.is_empty()).map(|s| {
        let [lo, mid, hi] = s.seconds;
        let place = s.at.as_deref().map(|a| format!(", the {a}")).unwrap_or_default();
        let from = if s.start[1] == 0.0 { "it starts the video".to_string() } else { format!("from {:.0}% to {:.0}% in", s.start[0] * 100.0, s.start[1] * 100.0) };
        let heading = format!("{}{place}: {mid:.1} s ({lo:.1} to {hi:.1}), {:.0} shots, {from}; in {} of {n}", s.name, s.shots[1], s.refs.len());
        let mut beats: Vec<Beat> = s.kinds.iter().map(|k| Beat {
            on_screen: if k.count >= 2.0 { format!("{}, about {:.0} shots of {:.1} s", k.name, k.count, k.seconds) } else { format!("{}, {:.1} s", k.name, k.seconds) },
            camera: None,
            cut: None,
            lines: Vec::new(),
            cues: Vec::new(),
            seconds: Some((k.count * k.seconds * 10.0).round() / 10.0),
            on_word: None,
        }).collect();
        if beats.is_empty() {
            beats.push(Beat { on_screen: "any shot: no one kind is in most of them".into(), camera: None, cut: None, lines: Vec::new(), cues: Vec::new(), seconds: Some(mid), on_word: None });
        }
        beats[0].cut = Some(s.cut.clone());
        let mut payoff = if s.shows.is_empty() { String::new() } else { format!("shows {}", s.shows.join(", ")) };
        if let Some((r, w)) = &s.said {
            payoff += &format!("{}as said in {}: \"{}\"", if payoff.is_empty() { "" } else { "; " }, names.get(*r).copied().unwrap_or("?"), words(w, 30));
        }
        Scene { heading: Some(heading), beats, payoff: Some(payoff).filter(|p| !p.is_empty()), beat: None }
    }).collect();
    let [lo, mid, hi] = st.seconds;
    let logline = format!("{title}'s template, from {n} references: {mid:.1} s ({lo:.1} to {hi:.1}), {:.0} shots ({:.0} to {:.0}). Fill each beat with the footage's own shots of that kind.", st.shots[1], st.shots[0], st.shots[2]);
    Script { title: format!("{title}: the template"), logline: Some(logline), scenes }
}

/// A value with its nulls and empty lists left out, as a script.ts leaves out what it doesn't say
fn bare(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            m.retain(|_, x| !x.is_null() && x.as_array().is_none_or(|a| !a.is_empty()));
            m.values_mut().for_each(bare);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(bare),
        _ => {}
    }
}

/// The template as a NAME.script.ts
pub fn script_ts(s: &Script) -> String {
    let mut v = serde_json::to_value(s).unwrap();
    bare(&mut v);
    format!(
        "// The videoform's template (docs/videoform.feature VF7), made from its references by raea videoform; made again on\n// every add and report, so a project copies it rather than editing it here.\nimport {{ script }} from \"raea/brief\";\n\nexport default script({});\n",
        serde_json::to_string_pretty(&v).unwrap()
    )
}

/// The structure, as the person's AI reads it: each slot, and sections shown by the same terms, which can't be told
/// apart where they meet
pub fn lines(st: &Structure, n: usize) -> Vec<String> {
    let mut out = vec!["Structure, section by section:".to_string()];
    for s in &st.slots {
        let [lo, mid, hi] = s.seconds;
        let kinds = s.kinds.iter().map(|k| k.name.split(':').next().unwrap_or("")).collect::<Vec<_>>().join(" / ");
        out.push(format!("  {}: in {} of {n}, {mid:.1} s ({lo:.1} to {hi:.1}), {:.0} shots{}", s.name, s.refs.len(), s.shots[1], if kinds.is_empty() { String::new() } else { format!(": {kinds}") }));
    }
    for w in st.slots.windows(2) {
        let both: Vec<&str> = w[0].shows.iter().filter(|x| w[1].shows.contains(x)).map(String::as_str).collect();
        if !both.is_empty() {
            out.push(format!("  {} and {} are both shown by {}: where one ends and the other starts can't be told; give each its own", w[0].name, w[1].name, both.join(", ")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::Kind;
    use crate::rows::Shot;

    #[test]
    fn references_cut_at_their_sections_into_slots_of_their_kinds() {
        let shot = |start: f64, words: &str| Shot { start, end: start + 1.0, transition: "cut".into(), words: words.into(), ..Default::default() };
        let r = |n: usize| Rows { shots: (0..n).map(|i| shot(i as f64, if i == n - 1 { "grab it" } else { "" })).collect(), ..Default::default() };
        let refs = [r(4), r(5)];
        // kind 0 the talk, kind 1 the end card
        let kinds = Kinds { kinds: vec![Kind { name: "talk".into(), ..Default::default() }, Kind { name: "card".into(), ..Default::default() }], of: vec![vec![Some(0), Some(0), Some(0), Some(1)], vec![Some(0), Some(0), Some(0), Some(0), Some(1)]] };
        let sec = |name: &str, hits: Vec<Vec<usize>>| SectionFound { name: name.into(), hits, ..Default::default() };
        // the hook is shown on 1 in both (it starts the video); the ask on the last shot, and in b also on 0, before the hook
        let secs = [sec("hook", vec![vec![1], vec![1]]), sec("ask", vec![vec![3], vec![0, 4]])];
        let st = structure(&refs, &kinds, &secs, &[vec![], vec!["the ask".into()]]);
        assert_eq!(st.slots[0].shots, [3.0, 4.0, 4.0], "a 3 shots, b 4");
        assert_eq!(st.slots[1].seconds, [1.0; 3]);
        assert_eq!((st.slots[1].kinds.len(), st.slots[1].kinds[0].name.as_str()), (1, "card"));
        assert_eq!(st.slots[1].said, Some((0, "grab it".into())));
        let s = template(&st, "t", &["a", "b"]);
        assert_eq!(s.scenes[1].payoff.as_deref(), Some("shows the ask; as said in a: \"grab it\""));
        assert_eq!(s.scenes[0].beats[0].cut.as_deref(), Some("cut"));
        let ts = script_ts(&s);
        assert!(ts.contains("\"onScreen\": \"talk, about 4 shots of 1.0 s\"") && !ts.contains("null") && !ts.contains("[]"), "{ts}");
        assert!(lines(&st, 2).len() == 3, "no sections shown by the same terms");
    }
}
