//! What the references hold, measure by measure (docs/videoform.feature VF4, VF4b): each reference's measures from its
//! kept rows, their range across the references and how strongly it holds (leave one out), the measures that fall in
//! two clear groups, the references far from the rest, and the person's corrections over all of it. The order a
//! measure is decided in: raea's delivery floor (loudness, true peak) first, then the person's word, then the
//! context's (a feel word, "snappy", keeps its measures checked though the references vary), then the references. A cut is critiqued against the ranges this leaves checked.

use crate::rows::Rows;
use raea_core_brief::Correction;
use raea_core_brief::videoform::Feel;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Measures = BTreeMap<String, f64>;

/// Each measure, in words
pub const NAMES: [(&str, &str); 21] = [
    ("seconds", "the video's length (s)"),
    ("cuts_per_min", "cuts a minute"),
    ("median_shot", "a shot's median length (s)"),
    ("long_shot", "a long shot's length, the longest tenth (s)"),
    ("first_shot", "the first shot's length (s)"),
    ("words_per_min", "words a minute while someone speaks"),
    ("pause", "the voice's pause at a cut, median (s)"),
    ("close_up", "shots that are close-ups"),
    ("medium", "shots that are medium"),
    ("wide", "shots that are wide"),
    ("no_people", "shots with no one in them"),
    ("faces", "faces in a shot, on average"),
    ("moving", "shots where the camera moves"),
    ("brightness", "brightness, 0 to 1"),
    ("text", "shots with words on screen"),
    ("captions", "shots captioned with what's said"),
    ("caption_y", "where captions sit, 0 the top to 1 the bottom"),
    ("speech", "shots where speech is heard"),
    ("music", "shots where music is heard"),
    ("on_beat", "cuts within 0.1 s of a beat"),
    ("loudness", "loudness (LUFS)"),
];

/// Measures that are shares of the shots, 0 to 1
const SHARES: [&str; 11] = ["close_up", "medium", "wide", "no_people", "moving", "text", "captions", "speech", "music", "on_beat", "caption_y"];

/// What raea's delivery floor sets for every cut (raea check's sound floor), so the references' range is only reported
pub const FLOOR: [(&str, f64, &str); 2] = [
    ("loudness", -14.0, "every delivery is mastered to -14 LUFS (±1), true peak at most -1 dBTP (raea check's floor)"),
    ("true_peak", -1.0, "every delivery's true peak is at most -1 dBTP (raea check's floor)"),
];

/// Under this many references nothing holds strongly enough to gate on (VF8b)
pub const FEW: usize = 5;

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    match s.len() {
        0 => 0.0,
        n if n % 2 == 1 => s[n / 2],
        n => (s[n / 2 - 1] + s[n / 2]) / 2.0,
    }
}

fn share(n: usize, of: usize) -> f64 {
    if of == 0 { 0.0 } else { n as f64 / of as f64 }
}

/// Lowercase words of two letters or more
fn tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 2).map(str::to_lowercase).collect()
}

/// Words on screen that are a caption of what's said: most of their words are in the shot's
fn caption(text: &str, said: &[String]) -> bool {
    let t = tokens(text);
    !t.is_empty() && t.iter().filter(|w| said.contains(w)).count() * 2 > t.len()
}

/// The cuts within 0.1 s of a beat (the first shot has no cut)
pub fn on_beat(shots: &[crate::Shot]) -> usize {
    shots.iter().skip(1).filter(|x| x.off_beat.is_some_and(|d| d <= 0.1)).count()
}

/// A reference's measures from its rows (loudness and true peak come from its reading, not its rows)
pub fn measures(rows: &Rows) -> Measures {
    let s = &rows.shots;
    let mut m = Measures::new();
    let Some(last) = s.last() else { return m };
    let n = s.len();
    let lens: Vec<f64> = s.iter().map(|x| x.seconds()).collect();
    let mut sorted = lens.clone();
    sorted.sort_by(f64::total_cmp);
    let count = |f: &dyn Fn(&crate::Shot) -> bool| share(s.iter().filter(|x| f(x)).count(), n);
    m.insert("seconds".into(), last.end);
    m.insert("cuts_per_min".into(), (n - 1) as f64 / last.end.max(1e-3) * 60.0);
    m.insert("median_shot".into(), median(&lens));
    m.insert("long_shot".into(), sorted[((n - 1) as f64 * 0.9).round() as usize]);
    m.insert("first_shot".into(), lens[0]);
    let said: f64 = s.iter().map(|x| x.words.split_whitespace().count() as f64).sum();
    let spoken: f64 = s.iter().filter(|x| x.words_per_minute > 0.0).map(|x| x.words.split_whitespace().count() as f64 / x.words_per_minute).sum();
    if spoken > 0.0 {
        m.insert("words_per_min".into(), said / spoken);
    }
    let pauses: Vec<f64> = s.iter().skip(1).filter_map(|x| x.pause_before).collect();
    if !pauses.is_empty() {
        m.insert("pause".into(), median(&pauses));
    }
    for (k, f) in [("close_up", "close-up"), ("medium", "medium"), ("wide", "wide"), ("no_people", "no people")] {
        m.insert(k.into(), count(&|x| x.framing == f));
    }
    m.insert("faces".into(), s.iter().map(|x| x.faces as f64).sum::<f64>() / n as f64);
    m.insert("moving".into(), count(&|x| x.motion == "moving"));
    m.insert("brightness".into(), s.iter().map(|x| x.brightness).sum::<f64>() / n as f64);
    m.insert("text".into(), count(&|x| !x.text.is_empty()));
    let captions: Vec<Vec<f64>> = s.iter().map(|x| {
        let said = tokens(&x.words);
        x.text.iter().filter(|(t, _)| caption(t, &said)).map(|(_, b)| b[1] + b[3] / 2.0).collect()
    }).collect();
    m.insert("captions".into(), share(captions.iter().filter(|c| !c.is_empty()).count(), n));
    let ys: Vec<f64> = captions.concat();
    if !ys.is_empty() {
        m.insert("caption_y".into(), median(&ys));
    }
    m.insert("speech".into(), count(&|x| x.heard.contains("speech")));
    m.insert("music".into(), count(&|x| x.heard.contains("music")));
    if n > 1 {
        m.insert("on_beat".into(), share(on_beat(s), n - 1));
    }
    m
}

/// One measure across the references
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub lo: f64,
    pub hi: f64,
    pub median: f64,
    /// each reference's value, in the references' order (None: it has no value for this measure)
    pub values: Vec<Option<f64>>,
    /// holds, loose, free (how widely it varies); floor (raea's delivery floor sets it); or the person's word: ignore,
    /// free, keep, target
    pub state: String,
    /// references, floor, or you
    pub by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// leave one out: the share of references inside the others' range
    pub holds: f64,
    /// what's checked on a cut, when anything is: the range widened a tenth each way, or the person's target
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<[f64; 2]>,
    /// when the references fall in two clear groups on it, each group's references
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Vec<usize>>,
}

/// What the references hold, and what's corrected
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Found {
    /// the references by name, in the order their values are
    pub references: Vec<String>,
    pub measures: BTreeMap<String, Range>,
    /// each reference far from the rest on several measures, and which
    pub outliers: Vec<(usize, Vec<String>)>,
    /// fewer than FEW references: nothing gates
    pub weak: bool,
}

/// lo-hi widened by a tenth of its width each way
fn padded(lo: f64, hi: f64) -> [f64; 2] {
    let pad = (hi - lo) * 0.1;
    [lo - pad, hi + pad]
}

/// How widely a measure varies, from its span: a share by its points, anything else against its median
fn width(k: &str, lo: f64, hi: f64, mid: f64) -> &'static str {
    let w = if SHARES.contains(&k) { (hi - lo) * 2.0 } else { (hi - lo) / mid.abs().max(1e-6) };
    if w > 1.0 { "free" } else if w > 0.5 { "loose" } else { "holds" }
}

/// Two clear groups: the widest gap between sorted values is most of the span and far wider than the rest
fn groups(v: &[(usize, f64)]) -> Vec<Vec<usize>> {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.1.total_cmp(&b.1));
    if s.len() < 4 {
        return Vec::new();
    }
    let gaps: Vec<f64> = s.iter().zip(&s[1..]).map(|(a, b)| b.1 - a.1).collect();
    let (at, big) = gaps.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
    let rest = (gaps.iter().sum::<f64>() - big) / (gaps.len() - 1) as f64;
    let span = s.last().unwrap().1 - s[0].1;
    if at == 0 || at + 2 >= s.len() || big < span * 0.4 || big < rest * 3.0 {
        return Vec::new();
    }
    vec![s[..=at].iter().map(|x| x.0).collect(), s[at + 1..].iter().map(|x| x.0).collect()]
}

/// The references' measures, the floor, the context's feel words and the person's corrections, into what's found
pub fn found(refs: &[(String, Measures)], corrections: &[Correction], feel: &[Feel]) -> Found {
    let mut out = Found { references: refs.iter().map(|r| r.0.clone()).collect(), weak: refs.len() < FEW, ..Default::default() };
    let names = NAMES.iter().map(|n| n.0).chain(["true_peak"]);
    for k in names {
        let have: Vec<(usize, f64)> = refs.iter().enumerate().filter_map(|(i, r)| Some((i, *r.1.get(k)?))).collect();
        let values: Vec<Option<f64>> = (0..refs.len()).map(|i| have.iter().find(|h| h.0 == i).map(|h| h.1)).collect();
        if have.is_empty() {
            continue;
        }
        let v: Vec<f64> = have.iter().map(|x| x.1).collect();
        let (lo, hi) = (v.iter().copied().fold(f64::MAX, f64::min), v.iter().copied().fold(f64::MIN, f64::max));
        let mid = median(&v);
        let inside = (0..v.len()).filter(|&i| {
            let rest: Vec<f64> = v.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, x)| *x).collect();
            let r = padded(rest.iter().copied().fold(f64::MAX, f64::min), rest.iter().copied().fold(f64::MIN, f64::max));
            v[i] >= r[0] && v[i] <= r[1]
        });
        let holds = if v.len() >= 3 { share(inside.count(), v.len()) } else { 0.0 };
        let mut r = Range { lo, hi, median: mid, values, state: width(k, lo, hi, mid).into(), by: "references".into(), holds, ..Default::default() };
        if let Some(f) = FLOOR.iter().find(|f| f.0 == k) {
            (r.state, r.by, r.why) = ("floor".into(), "floor".into(), Some(f.2.into()));
        }
        // groups only where it varies enough to matter
        if r.state == "loose" || r.state == "free" {
            r.groups = groups(&have);
        }
        if r.by == "references" && let Some(f) = feel.iter().find(|f| f.measures.iter().any(|m| m.as_str() == k)) {
            (r.state, r.by, r.why) = ("keep".into(), "context".into(), Some(format!("the context's \"{}\"", f.word)));
        }
        if let Some(c) = corrections.iter().find(|c| c.measure == k) {
            (r.state, r.by, r.why) = (c.say.clone(), "you".into(), c.why.clone());
        }
        r.checked = match r.state.as_str() {
            "holds" | "loose" | "keep" => Some(padded(lo, hi)),
            "target" => corrections.iter().find(|c| c.measure == k).and_then(|c| c.range),
            _ => None,
        };
        if SHARES.contains(&k) {
            r.checked = r.checked.map(|[a, b]| [a.max(0.0), b.min(1.0)]);
        }
        out.measures.insert(k.to_string(), r);
    }
    out.outliers = outliers(&out);
    out
}

/// The references far from the rest (over three times the spread from the median) on three measures or more, of
/// those the floor and the person haven't set aside
fn outliers(f: &Found) -> Vec<(usize, Vec<String>)> {
    let mut far: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (k, r) in f.measures.iter().filter(|(_, r)| r.by == "references") {
        let v: Vec<f64> = r.values.iter().flatten().copied().collect();
        let spread = median(&v.iter().map(|x| (x - r.median).abs()).collect::<Vec<_>>()).max(if SHARES.contains(&k.as_str()) { 0.05 } else { r.median.abs() * 0.05 });
        for (i, x) in r.values.iter().enumerate() {
            if x.is_some_and(|x| (x - r.median).abs() > spread * 3.0) {
                far.entry(i).or_default().push(k.clone());
            }
        }
    }
    far.into_iter().filter(|(_, ks)| ks.len() >= 3).collect()
}

/// One measure of a cut outside what's checked: its value, the range, and which way to move
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Out {
    pub measure: String,
    pub value: f64,
    pub range: [f64; 2],
    /// raise or lower
    pub move_: String,
}

/// A cut's measures against what's checked
pub fn critique(f: &Found, cut: &Measures) -> Vec<Out> {
    let mut out = Vec::new();
    for (k, r) in &f.measures {
        let (Some(c), Some(&v)) = (r.checked, cut.get(k)) else { continue };
        if v < c[0] || v > c[1] {
            out.push(Out { measure: k.clone(), value: v, range: c, move_: if v < c[0] { "raise" } else { "lower" }.into() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shot;

    fn refs(cuts: &[f64], loud: &[f64]) -> Vec<(String, Measures)> {
        cuts.iter().zip(loud).enumerate().map(|(i, (c, l))| (format!("r{i}"), Measures::from([("cuts_per_min".into(), *c), ("loudness".into(), *l), ("medium".into(), 0.8)]))).collect()
    }

    #[test]
    fn the_floor_then_the_person_then_the_references() {
        let r = refs(&[22.0, 23.0, 24.0, 36.0, 37.0, 38.0, 46.0, 49.0], &[-24.0, -24.0, -23.7, -17.0, -15.3, -15.2, -15.1, -14.7]);
        let f = found(&r, &[], &[]);
        let cuts = &f.measures["cuts_per_min"];
        assert_eq!(cuts.groups, vec![vec![0, 1, 2], vec![3, 4, 5, 6, 7]], "two speeds of cutting are two groups");
        assert_eq!((f.measures["loudness"].state.as_str(), f.measures["loudness"].checked), ("floor", None), "the floor sets loudness");
        assert_eq!(f.measures["medium"].state, "holds");
        assert!(!f.weak);
        let say = |s: &str, range| Correction { measure: "cuts_per_min".into(), say: s.into(), range, why: Some("snappy".into()) };
        let t = found(&r, &[say("target", Some([30.0, 50.0]))], &[]);
        assert_eq!((t.measures["cuts_per_min"].by.as_str(), t.measures["cuts_per_min"].checked), ("you", Some([30.0, 50.0])));
        let cut = Measures::from([("cuts_per_min".into(), 25.0), ("loudness".into(), -30.0), ("medium".into(), 0.8)]);
        assert_eq!(critique(&t, &cut), vec![Out { measure: "cuts_per_min".into(), value: 25.0, range: [30.0, 50.0], move_: "raise".into() }]);
        assert!(critique(&found(&r, &[say("ignore", None)], &[]), &cut).is_empty(), "an ignored measure isn't checked; the floor isn't the videoform's");
        assert!(found(&r[..4], &[], &[]).weak, "under five references nothing gates");
        let snappy = [Feel { word: "snappy".into(), measures: vec!["cuts_per_min".into()] }];
        let k = found(&r, &[], &snappy);
        assert_eq!((k.measures["cuts_per_min"].state.as_str(), k.measures["cuts_per_min"].by.as_str()), ("keep", "context"), "a feel word keeps its measures checked");
        assert_eq!(found(&r, &[say("free", None)], &snappy).measures["cuts_per_min"].by, "you", "the person's word over the context's");
    }

    #[test]
    fn a_reference_measured_from_its_rows() {
        let shot = |start: f64, end: f64, words: &str, text: &str| Shot { start, end, words: words.into(), words_per_minute: if words.is_empty() { 0.0 } else { 180.0 }, framing: "medium".into(), heard: "speech".into(), off_beat: Some(0.05), text: [(text.to_string(), [0.2, 0.6, 0.6, 0.04])].into_iter().filter(|t| !t.0.is_empty()).collect(), ..Default::default() };
        let rows = Rows { shots: vec![shot(0.0, 2.0, "ready for a taste test", "Ready for a taste test?"), shot(2.0, 3.0, "", "BRAND"), shot(3.0, 6.0, "yes", "")], ..Default::default() };
        let m = measures(&rows);
        assert_eq!((m["seconds"], m["cuts_per_min"], m["median_shot"], m["first_shot"]), (6.0, 20.0, 2.0, 2.0));
        assert_eq!((m["text"], m["captions"], m["caption_y"]), (2.0 / 3.0, 1.0 / 3.0, 0.62));
        assert_eq!((m["medium"], m["on_beat"], m["words_per_min"]), (1.0, 1.0, 180.0));
    }
}
