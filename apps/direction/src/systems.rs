//! raea's own checks by name (docs/direction.feature DR4): each a system over a composition's worlds, held to its
//! rule's values, as a project's TS check is (sdk/raea/src/check.ts). Taste's six forms are six of them, with the same
//! names and values: the four text forms run over every frame's Text with no render; loudness and still-max are
//! measured on the newest render itself, in the floor's pass (`floor`: raea check's floor on that render).

use std::collections::BTreeMap;

use raea_core_facts::{Facts, Finding, Rule, Source, Text};

/// Where a check runs: every frame; the frame before each cut and the cut's first frame; every frame a line is
/// said or sung on; once, on the first frame; or these.
#[derive(Debug, Clone, PartialEq)]
pub enum Frames {
    All,
    Cuts,
    Lines,
    Once,
    At(Vec<i64>),
}

impl Frames {
    /// As a check file declares it ("all", "cuts", "lines", "once" or a list of frames).
    pub fn of(v: &serde_json::Value) -> Result<Frames, String> {
        match v {
            serde_json::Value::Null => Ok(Frames::All),
            serde_json::Value::String(s) => match s.as_str() {
                "all" => Ok(Frames::All),
                "cuts" => Ok(Frames::Cuts),
                "lines" => Ok(Frames::Lines),
                "once" => Ok(Frames::Once),
                o => Err(format!("frames is \"all\", \"cuts\", \"lines\", \"once\" or a list of frames, not \"{o}\"")),
            },
            serde_json::Value::Array(a) => a.iter().map(|f| f.as_i64().ok_or("frames lists frame numbers")).collect::<Result<_, _>>().map(Frames::At).map_err(str::to_string),
            o => Err(format!("frames is \"all\", \"cuts\", \"lines\", \"once\" or a list of frames, not {o}")),
        }
    }

    /// Whether running here needs the frames of the whole piece read (cuts and lines are found in them).
    pub fn whole(&self) -> bool {
        matches!(self, Frames::All | Frames::Cuts | Frames::Lines)
    }

    /// The frames of `worlds` (every frame read, in order) it runs on.
    pub fn pick(&self, worlds: &[Facts]) -> Vec<i64> {
        let first = worlds.first().map_or(0, |w| w.frame);
        match self {
            Frames::All => worlds.iter().map(|w| w.frame).collect(),
            Frames::Once => vec![first],
            Frames::At(v) => v.clone(),
            Frames::Lines => worlds.iter().filter(|w| !w.find(&["Line"]).is_empty()).map(|w| w.frame).collect(),
            Frames::Cuts => {
                let mut cuts: Vec<i64> = worlds.iter().flat_map(|w| w.find(&["Shot"]).into_iter().filter_map(|e| e.components["Shot"]["from"].as_i64())).filter(|f| *f > first).collect();
                cuts.sort();
                cuts.dedup();
                cuts.into_iter().flat_map(|c| [c - 1, c]).collect()
            }
        }
    }
}

/// One of raea's checks: its name, whether it needs a render, where it runs, and its line when no rule gives one.
#[derive(Debug, Clone)]
pub struct Named {
    pub name: &'static str,
    pub render: bool,
    pub frames: Frames,
    pub text: &'static str,
}

/// raea's named checks.
pub fn named() -> Vec<Named> {
    let n = |name, render, frames, text| Named { name, render, frames, text };
    vec![
        n("text-hold", false, Frames::All, "text holds long enough to read"),
        n("text-above", false, Frames::All, "text sits above a line down the frame"),
        n("text-below", false, Frames::All, "text sits below a line down the frame"),
        n("text-margin", false, Frames::All, "text keeps a margin from the frame's edges"),
        n("loudness", true, Frames::Once, "the mix is at its loudness"),
        n("still-max", true, Frames::Once, "the picture is never still for too long"),
        n("floor", true, Frames::Once, "raea's floor: what every finished piece must meet"),
        n("text-read", true, Frames::All, "text on screen can be read for as long as it needs, and OCR reads what can be read (read.rs)"),
        // a videoform's rules (docs/videoform.feature VF10), over the composition's cuts with no render
        n("videoform-covers", false, Frames::All, "every video has it"),
        n("videoform-opens", false, Frames::All, "the first shot has it"),
        n("videoform-closes", false, Frames::All, "the last shot has it"),
        n("videoform-slot", false, Frames::All, "the section has it"),
        n("videoform-order", false, Frames::All, "one section comes before another"),
        n("videoform-together", false, Frames::All, "where a place is shown, its thing is shown with it"),
        n("videoform-event", false, Frames::All, "what follows a word said or shown"),
    ]
}

/// A text's identity across frames: its layer key, else its words.
pub(crate) fn ident(f: &Facts, e: u32, t: &Text) -> (String, Option<String>) {
    let key = f.get::<Source>(e).and_then(|s| s.key);
    (key.clone().unwrap_or_else(|| t.words.clone()), key)
}

/// A run of frames on which one text measures past its rule: the text, its key, the frames (end exclusive) and the
/// worst measure in the run.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub words: String,
    pub key: Option<String>,
    pub from: i64,
    pub to: i64,
    pub value: f64,
}

/// Each text's runs of frames on which `bad` gives a measure, the worst of each run by `worse`.
fn runs(worlds: &[Facts], bad: impl Fn(&Facts, &Text) -> Option<f64>, worse: fn(f64, f64) -> bool) -> Vec<Run> {
    let mut open: BTreeMap<String, Run> = BTreeMap::new();
    let mut done = Vec::new();
    for w in worlds {
        for e in w.find(&["Text"]) {
            let Some(t) = w.get::<Text>(e.id) else { continue };
            let Some(v) = bad(w, &t) else { continue };
            let (id, key) = ident(w, e.id, &t);
            match open.get_mut(&id) {
                Some(r) if r.to == w.frame => {
                    r.to = w.frame + 1;
                    if worse(v, r.value) {
                        r.value = v;
                    }
                }
                _ => {
                    if let Some(r) = open.insert(id, Run { words: t.words.clone(), key, from: w.frame, to: w.frame + 1, value: v }) {
                        done.push(r);
                    }
                }
            }
        }
    }
    done.extend(open.into_values());
    done.sort_by_key(|r| (r.from, r.words.clone()));
    done
}

pub(crate) fn quote(s: &str) -> String {
    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > 40 { format!("\"{}…\"", s.chars().take(39).collect::<String>()) } else { format!("\"{s}\"") }
}

/// A text form's findings over `worlds` (every frame of the composition, in order), held to `rule`'s `value`.
pub fn text(rule: &Rule, worlds: &[Facts]) -> Vec<Finding> {
    let level = rule.level.unwrap_or(raea_core_facts::Level::Error);
    let Some(v) = rule.number("value") else {
        return vec![Finding::new(&rule.id, level, format!("{} needs a value (values: {{ value: N }})", rule.check), &rule.by)];
    };
    let Some(first) = worlds.first() else { return Vec::new() };
    let (w, h, fps) = (first.width as f64, first.height as f64, first.fps);
    // the runs of text past the line across the frame at `v`%: by its edge `k` (1 its top, 3 its bottom), below the
    // line (`down`) or above it
    let past = |k: usize, down: bool| {
        let worse: fn(f64, f64) -> bool = if down { |a, b| a > b } else { |a, b| a < b };
        // a pixel past it, so text that only touches the line holds it
        let line = v + (if down { 100.0 } else { -100.0 }) / h;
        runs(worlds, move |_, t| t.box_.map(|b| b[k] * 100.0).filter(|y| worse(*y, line)), worse)
    };
    // the runs that break the form, each with its measure as its value; what's said of one ({n} its measure, {v} the
    // rule's); and the unit
    let (got, says, unit): (Vec<Run>, &str, &str) = match rule.check.as_str() {
        "text-hold" => {
            let on = runs(worlds, |_, _| Some(0.0), |_, _| false).into_iter().map(|r| Run { value: (r.to - r.from) as f64 / fps, ..r });
            (on.filter(|r| r.value + 0.05 < v).collect(), "is on for {n} s, under {v} s", "s")
        }
        "text-above" => (past(3, true), "reaches {n}% down the frame, below {v}%", "%"),
        "text-below" => (past(1, false), "starts {n}% down the frame, above {v}%", "%"),
        "text-margin" => {
            let s = w.min(h) / 1080.0;
            let margin = move |b: [f64; 4]| (b[0] * w).min(b[1] * h).min(w - b[2] * w).min(h - b[3] * h) / s;
            (runs(worlds, |_, t| t.box_.map(margin).filter(|m| m + 1.0 < v), |a, b| a < b), "is {n} px from the edge, inside {v}", "px")
        }
        other => return vec![Finding::new(&rule.id, level, format!("{other} isn't a text check"), &rule.by)],
    };
    let shown = |x: f64| if unit == "s" { format!("{x:.1}") } else { format!("{x:.0}") };
    got.iter()
        .map(|r| {
            let said = format!("{} {}", quote(&r.words), says.replace("{n}", &shown(r.value)).replace("{v}", &shown(v)));
            let said = if rule.text.is_empty() { said } else { format!("{said} (\"{}\")", rule.text) };
            Finding::new(&rule.id, level, said, &rule.by).about(r.words.clone(), r.key.clone()).frames(r.from, r.to).measure(&rule.check, (r.value * 10.0).round() / 10.0, unit, Some(v))
        })
        .collect()
}

#[cfg(test)]
mod tests;
