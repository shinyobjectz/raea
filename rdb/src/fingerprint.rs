//! An edit's fingerprint, in facets, as hypervectors (meaning/k.rs: seeded symbols, bind, permute, majority): what it
//! opens with, which moments of which takes it uses, the rhythm of its cuts, the look of its text, and the shape of its
//! frames. Two variations that share facets are near-copies whatever their ids; `compare` says which facets they share,
//! so an agent making many concepts can be told what to change ("B and C open on the same line, cut to the same beat").

use crate::eval::Tup;
use crate::ir::{Program, V};
use crate::k::{self, Hv};
use crate::previz::{level, row_hv, showing, sym};

pub const FACETS: [&str; 5] = ["hook", "sources", "rhythm", "style", "look"];

/// An edit's facets (in FACETS order) and their bundle.
pub struct Fingerprint {
    pub facets: Vec<Hv>,
    /// facets the edit has nothing for (no clips, no text): two edits without one aren't alike in it
    pub empty: Vec<bool>,
    pub whole: Hv,
}

fn num(t: &Tup, k: &str, d: f64) -> f64 {
    match t.vals.get(k) {
        Some(V::N(x)) => *x,
        _ => d,
    }
}

fn text(t: &Tup, k: &str) -> String {
    match t.vals.get(k) {
        Some(V::S(s)) => s.clone(),
        Some(V::N(n)) => format!("{n}"),
        None => String::new(),
    }
}

/// Order matters in a sequence: each item bound to its place (permuted by its index), then bundled.
fn sequence(items: &[Hv]) -> Hv {
    if items.is_empty() {
        return sym("empty sequence");
    }
    k::majority(&items.iter().enumerate().map(|(i, h)| k::permute(h, i)).collect::<Vec<_>>())
}

fn bag(items: Vec<Hv>, empty: &str) -> Hv {
    if items.is_empty() { sym(empty) } else { k::majority(&items) }
}

pub fn fingerprint(prog: &Program, rows: &[Tup]) -> Result<Fingerprint, String> {
    let tl = crate::timeline::Timeline::new(prog, rows)?;
    let fps = prog.fps.max(1.0);
    // the opening: the words on screen in the first 3 s, in order of appearance
    let mut opening: Vec<(i64, String)> = Vec::new();
    for f in 0..(3.0 * fps) as i64 {
        for t in tl.tups(f.min(prog.frames.max(1) - 1)) {
            if matches!(t.kind.as_str(), "caption" | "title" | "text") && showing(&t, f) {
                for w in text(&t, "text").split_whitespace() {
                    let w = w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string();
                    if !w.is_empty() && !opening.iter().any(|(_, x)| *x == w) {
                        opening.push((f, w));
                    }
                }
            }
        }
    }
    let hook = sequence(&opening.iter().map(|(_, w)| sym(&format!("word:{w}"))).collect::<Vec<_>>());
    // the takes' moments it uses, and the rhythm of its cuts
    let mut clips: Vec<&Tup> = rows.iter().filter(|t| t.kind == "clip").collect();
    clips.sort_by(|a, b| num(a, "from", 0.0).total_cmp(&num(b, "from", 0.0)));
    let mut used = Vec::new();
    let mut beats = Vec::new();
    for c in &clips {
        let (a, b) = (num(c, "from", 0.0), num(c, "to", prog.frames as f64));
        let (src, at) = (text(c, "src"), num(c, "src_in", 0.0));
        let secs = ((b - a) / fps).max(0.0);
        // each second of source it plays, so overlapping cuts of the same lines overlap here
        for s in at.floor() as i64..(at + secs).ceil() as i64 {
            used.push(sym(&format!("src:{src}:{s}")));
        }
        // cut length on a doubling scale (0.5, 1, 2, 4, 8 s ...)
        beats.push(sym(&format!("beat:{}", (secs.max(0.25) * 2.0).log2().round() as i64)));
    }
    let sources = bag(used, "no clips");
    let rhythm = sequence(&beats);
    // the text's look: each caption or title row's font, weight, size band, colour and third of the frame
    // a highlighted caption's word runs (rows with a parent) aren't text blocks: they count as the caption's lit colour
    let lit = |t: &Tup| {
        rows.iter()
            .filter(|r| matches!(r.vals.get("parent"), Some(V::S(p)) if p == &t.id))
            .map(|r| text(r, "color").to_lowercase())
            .find(|c| *c != text(t, "color").to_lowercase())
    };
    let style = bag(
        rows.iter()
            .filter(|t| matches!(t.kind.as_str(), "caption" | "title" | "text") && !matches!(t.vals.get("parent"), Some(V::S(p)) if !p.is_empty()))
            .map(|t| {
                let third = (num(t, "y", 0.0) / prog.height.max(1) as f64 * 3.0).floor().clamp(0.0, 2.0);
                k::majority(&[
                    k::bind(&sym("style:kind"), &sym(&t.kind)),
                    k::bind(&sym("style:font"), &sym(&text(t, "font").to_lowercase())),
                    k::bind(&sym("style:weight"), &sym(&text(t, "weight"))),
                    k::bind(&sym("style:size"), &level("style:size", num(t, "size", 48.0) / 200.0)),
                    k::bind(&sym("style:color"), &sym(&text(t, "color").to_lowercase())),
                    k::bind(&sym("style:third"), &sym(&format!("{third}"))),
                    k::bind(&sym("style:lit"), &sym(&lit(t).unwrap_or_default())),
                ])
            })
            .collect(),
        "no text",
    );
    // the frames' shape, a second apart
    let look = sequence(&(0..prog.frames.max(1)).step_by(fps as usize).map(|f| row_hv(prog, &tl.tups(f), f)).collect::<Vec<_>>());
    let has_text = rows.iter().any(|t| matches!(t.kind.as_str(), "caption" | "title" | "text"));
    let empty = vec![opening.is_empty(), clips.is_empty(), clips.is_empty(), !has_text, false];
    let facets = vec![hook, sources, rhythm, style, look];
    let whole = k::majority(&FACETS.iter().zip(&facets).map(|(n, h)| k::bind(&sym(&format!("facet:{n}")), h)).collect::<Vec<_>>());
    Ok(Fingerprint { facets, empty, whole })
}

/// Facets this similar or more are "the same" in compare's verdicts (chance is about 0 +- 0.07 at 2048 bits).
pub const SAME: f32 = 0.6;

/// Every pair's similarity, overall and per facet, as JSON: {pairs: [{a, b, whole, facets: {..}, same: [..]}]}.
pub fn compare(names: &[String], fps: &[Fingerprint]) -> serde_json::Value {
    let mut pairs = Vec::new();
    for i in 0..fps.len() {
        for j in i + 1..fps.len() {
            let (a, b) = (&fps[i], &fps[j]);
            let per: Vec<f32> = a.facets.iter().zip(&b.facets).map(|(x, y)| k::hv_sim(x, y)).collect();
            let absent = |i: usize| a.empty[i] && b.empty[i];
            let same: Vec<&str> = (0..FACETS.len()).filter(|&i| per[i] >= SAME && !absent(i)).map(|i| FACETS[i]).collect();
            let neither: Vec<&str> = (0..FACETS.len()).filter(|&i| absent(i)).map(|i| FACETS[i]).collect();
            pairs.push(serde_json::json!({
                "a": names[i], "b": names[j], "whole": (k::hv_sim(&a.whole, &b.whole) * 100.0).round() / 100.0,
                "facets": FACETS.iter().zip(&per).map(|(n, s)| (n.to_string(), serde_json::json!((s * 100.0).round() / 100.0))).collect::<serde_json::Map<_, _>>(),
                "same": same,
                "neither_has": neither,
            }));
        }
    }
    serde_json::json!({ "pairs": pairs, "same_at": SAME })
}
