//! A videoform's rules held on one new video (docs/videoform.feature VF10): the cut is read into rows as a reference
//! is, set last among the references, and each rule is tested on it with the same features and the same test that
//! mined it, so a rule holds on a cut exactly as it held on the references.

use crate::{answered, features, with};
use raea_sys_videoform::rows::Rows;
use raea_sys_videoform::vocab::Report;
use serde_json::{Map, Value};

/// What a rule does on the new video
#[derive(Clone, Debug, PartialEq)]
pub enum Held {
    Holds,
    /// it doesn't: the shots where it doesn't (none when it's missing from the whole video)
    Breaks(Vec<usize>),
    /// it can't be told on this video yet, and what it waits for (how it's layered, read from a render)
    Waits(String),
    /// it doesn't apply to this video, and why (its section isn't found, its trigger never comes)
    Moot(String),
}

/// A value of a rule's, as text ("" when it has none)
fn text<'a>(values: &'a Map<String, Value>, k: &str) -> &'a str {
    values.get(k).and_then(Value::as_str).unwrap_or_default()
}

/// Where a section starts in the new video, when it's found there
fn starts(rep: &Report, name: &str, c: usize) -> Option<f64> {
    let sec = rep.sections.iter().find(|x| x.name == name)?;
    sec.found.get(c).copied().flatten()
}

/// The feature a rule names by its words (a place, a thing)
fn worded(feats: &[crate::Feat], w: &str) -> Option<usize> {
    feats.iter().position(|f| f.words == w)
}

/// The rule written `check` with `values` (a line of NAME.taste.ts) on the last of `refs`, with `fired` and `rep`
/// made over all of them (the new video's kinds set by the caller in `rep.kinds.of`)
pub fn hold(check: &str, values: &Map<String, Value>, refs: &[Rows], fired: &[Vec<Vec<bool>>], rep: &Report) -> Held {
    let Some(c) = refs.len().checked_sub(1) else { return Held::Waits("no video".into()) };
    let (feats, on) = features(refs, fired, rep, &rep.kinds);
    let on = &on[c];
    let n = on.len();
    let s = |k: &str| text(values, k);
    // a feature by what it's held to (a slot's own name left out)
    let by = |want: &Map<String, Value>| feats.iter().position(|f| &f.values == want);
    let without = |k: &str| {
        let mut m = values.clone();
        m.remove(k);
        m
    };
    let unread = values.contains_key("layer") && refs[c].shots.iter().all(|x| x.layers.is_none());
    if unread {
        return Held::Waits("its render, which says how it's layered (raea render, then raea check)".into());
    }
    let missing = |what: &str| Held::Moot(format!("{what} isn't anything the vocabulary finds on the references now"));
    let breaks = |ok: bool, at: Vec<usize>| if ok { Held::Holds } else { Held::Breaks(at) };
    match check {
        "videoform-covers" | "videoform-opens" | "videoform-closes" => {
            let Some(f) = by(values) else { return missing("what it holds to") };
            let at = with(on, f);
            match check {
                "videoform-covers" => breaks(!at.is_empty(), vec![]),
                "videoform-opens" => breaks(at.first() == Some(&0), vec![0]),
                _ => breaks(n > 0 && at.last() == Some(&(n - 1)), vec![n.saturating_sub(1)]),
            }
        }
        "videoform-slot" => {
            let Some(f) = by(&without("slot")) else { return missing("what it holds to") };
            let Some(i) = rep.sections.iter().position(|x| x.name == s("slot")) else { return missing(s("slot")) };
            match raea_sys_videoform::structure::spans(refs, &rep.sections)[c][i] {
                None => Held::Moot(format!("{} isn't found in it: none of what the vocabulary finds it by", s("slot"))),
                Some((a, b)) => breaks((a..b).any(|j| on[j][f]), (a..b).collect()),
            }
        }
        "videoform-order" => {
            match (starts(rep, s("before"), c), starts(rep, s("after"), c)) {
                (Some(a), Some(b)) => breaks(a < b, vec![]),
                _ => Held::Moot(format!("{} or {} isn't found in it", s("before"), s("after"))),
            }
        }
        "videoform-together" => {
            let (Some(p), Some(t)) = (worded(&feats, s("place")), worded(&feats, s("thing"))) else { return missing("the place or the thing") };
            let near = |j: usize| (j.saturating_sub(1)..=(j + 1).min(n.saturating_sub(1))).any(|x| on[x][t]);
            let mut runs: Vec<Vec<usize>> = Vec::new();
            for j in (0..n).filter(|&j| on[j][p]) {
                match runs.last_mut() {
                    Some(run) if j <= run.last().unwrap() + 2 => run.push(j),
                    _ => runs.push(vec![j]),
                }
            }
            if runs.is_empty() {
                return Held::Moot(format!("it never shows {}", s("place")));
            }
            let bad: Vec<usize> = runs.iter().filter(|run| !run.iter().any(|&j| near(j))).flatten().copied().collect();
            breaks(bad.is_empty(), bad)
        }
        "videoform-event" => {
            let mut want = Map::new();
            want.insert("term".into(), values.get("when").cloned().unwrap_or_default());
            let Some(t) = by(&want) else { return missing("its trigger") };
            let then = s("then");
            let f = feats.iter().position(|x| x.id == then);
            if f.is_none() && then != "punch-in" {
                return missing(then);
            }
            let does = |j: usize| answered(&refs[c], on, j, f);
            let at = with(on, t);
            if at.is_empty() {
                return Held::Moot(format!("{} never comes in it", values.get("when").and_then(Value::as_str).unwrap_or("its trigger")));
            }
            let bad: Vec<usize> = at.iter().copied().filter(|&j| !does(j)).collect();
            breaks(bad.len() * 2 <= at.len(), bad)
        }
        o => Held::Moot(format!("{o} isn't one of the videoform's checks")),
    }
}
