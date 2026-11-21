//! The fingerprint's flags asked again (docs/ask.feature AK8). `raea fingerprint score FILE --ask flags` (or `ask:
//! "flags"` in fingerprint.ts) puts each flagged moment to the decision model in its axis's own words, on that moment's
//! window alone, so it costs by the flags and never by the piece; `--ask unsure` also asks each window an axis puts in
//! its plausible band without flagging it. Moments on the same window share a call. Each flag then says "clef agrees
//! (0.71)" or "clef doesn't see it (0.08)", and each answer becomes an `eye:clef` evidence row, which trains the axis's
//! head and never counts as the person's (FP6d). The question carries the axis's words as its label words, so the
//! person's notes calibrate it as they do any question (AK6, AK7).

use std::path::Path;
use std::sync::Arc;

use raea_app_ask::prepare::Piece;
use raea_app_ask::runner::{self, Entry, Event};
pub use raea_app_ask::runner::Opts;
use raea_core_ask::{Question, Sees, Window};
use raea_sys_columns::Columns;
use raea_sys_columns::window::Modality;
use raea_sys_fingerprint::evidence::{Row, features};
use raea_sys_fingerprint::folder::Folder;
use raea_sys_fingerprint::taxonomy::{Axis, Fingerprint};
use raea_sys_fingerprint_use::score::{Scored, clock};
use serde::Serialize;

/// One moment asked again.
#[derive(Debug, Clone, Serialize)]
pub struct Double {
    pub a: f64,
    pub b: f64,
    pub axis: String,
    /// the axis in the person's words
    pub words: String,
    /// whether the fingerprint flagged it (else it was unsure of it)
    pub flagged: bool,
    /// clef's problem score, when it answered
    pub problem: Option<f64>,
    /// clef agrees: past even odds, or past the question's threshold once the person's labels calibrated it
    pub agrees: Option<bool>,
    /// its answer's id in raea ask's record, for `raea ask verdict`
    pub answer: Option<String>,
    /// why it wasn't asked or still waits
    pub not_asked: Option<String>,
}

impl Double {
    /// What clef made of it, as a flag's line says it.
    pub fn said(&self) -> String {
        match (self.agrees, self.problem, &self.not_asked) {
            (Some(true), Some(p), _) => format!("clef agrees ({p:.2})"),
            (Some(false), Some(p), _) => format!("clef doesn't see it ({p:.2})"),
            (_, _, Some(why)) => format!("clef wasn't asked: {why}"),
            _ => "clef gave no answer".into(),
        }
    }
}

/// The moments asked again, a line each under the score's summary, as its flags are written (the app reads them).
pub fn lines(ds: &[Double]) -> String {
    let each = ds.iter().map(|d| format!("  {}-{}  {}: {}{}\n", clock(d.a), clock(d.b), d.axis, d.said(), if d.flagged { "" } else { " (the fingerprint was unsure)" }));
    format!("asked again of clef ({} moment{}):\n{}", ds.len(), if ds.len() == 1 { "" } else { "s" }, each.collect::<String>())
}

/// The moments to ask: every flag, and with `unsure` each run of windows an axis puts in its plausible band unflagged.
pub fn moments(s: &Scored, fp: &Fingerprint, which: &str) -> Vec<Double> {
    let words = |axis: &str| fp.axis(axis).and_then(|a| a.words.first().cloned()).unwrap_or_else(|| axis.to_string());
    let one = |a: f64, b: f64, axis: &str, flagged: bool| Double { a, b, axis: axis.into(), words: words(axis), flagged, problem: None, agrees: None, answer: None, not_asked: None };
    let mut out: Vec<Double> = s.moments.iter().map(|m| one(m.a, m.b, &m.axis, true)).collect();
    if which == "unsure" {
        for w in &s.windows {
            for x in w.axes.iter().filter(|x| !x.flagged && x.band == Some("plausible")) {
                match out.iter_mut().rev().find(|d| d.axis == x.axis && !d.flagged) {
                    Some(d) if w.a <= d.b => d.b = w.b,
                    _ => out.push(one(w.a, w.b, &x.axis, false)),
                }
            }
        }
    }
    out
}

/// An axis as a question: is its defect there (or, for a quality, is it there), looking or listening as it does.
pub fn question(axis: &Axis) -> Question {
    let words = axis.words.first().cloned().unwrap_or_else(|| axis.name.replace(['-', '_'], " "));
    let good = axis.polarity == "good";
    let ask = if good { format!("Is this {words}?") } else { format!("Is there {words} here?") };
    let mut q = Question::plain(&format!("fp.{}", axis.name), &ask);
    if good {
        q.flags = vec!["no".into()];
    }
    let listens = matches!(axis.modality, Modality::Sound | Modality::Events);
    q.sees = if listens { Sees::Nothing } else { Sees::Video };
    q.hears = listens;
    q.label_words = axis.words.clone();
    q
}

/// Each moment asked of `file`, a call a window, at most `o.jobs` at once; the moments with clef's answers. A moment
/// that couldn't be asked (no route, the pool spent, the file unreadable) says why, and the score goes on without it.
pub fn ask(file: &Path, fp: &Fingerprint, mut ds: Vec<Double>, o: &Opts) -> Vec<Double> {
    let piece = match Piece::read(file, None, None, None, None) {
        Ok(p) => Arc::new(p),
        Err(why) => {
            for d in &mut ds {
                d.not_asked = Some(format!("the file couldn't be read: {why}"));
            }
            return ds;
        }
    };
    let entries = ds
        .iter()
        .filter_map(|d| {
            let q = question(fp.axis(&d.axis)?);
            Some(Entry { piece: piece.clone(), window: Some(Window { from: Some(d.a), to: Some(d.b) }), questions: vec![q], dropped: vec![], with: vec![] })
        })
        .collect();
    runner::run(entries, o, &mut |e| {
        let Event::Answer(r) = e else { return };
        let Some(d) = ds.iter_mut().find(|d| format!("fp.{}", d.axis) == r.id && d.a == r.from && r.to == Some(d.b)) else { return };
        d.problem = r.problem;
        d.agrees = r.says.as_ref().map(|_| r.flagged);
        d.answer = r.answer.clone();
        d.not_asked = r.not_asked.clone().or_else(|| r.waits.clone());
    });
    ds
}

/// The answered moments kept in the fingerprint's ledger as clef's evidence, over the file's columns; a moment asked
/// again replaces clef's last row for it. How many it kept.
pub fn keep(dir: &Folder, c: &Columns, project: &str, ds: &[Double]) -> Result<usize, String> {
    let rows: Vec<Row> = ds.iter().filter_map(|d| row(d, &c.media, project)).collect();
    if rows.is_empty() {
        return Ok(0);
    }
    let mut l = dir.ledger().map_err(|e| e.to_string())?;
    let mut n = 0;
    for r in rows {
        l.retain(|x| !(x.source == r.source && x.media == r.media && x.axis == r.axis && x.a == r.a && x.b == r.b));
        let f = features(c, r.a, r.b);
        l.push(r, &f);
        n += 1;
    }
    dir.write_ledger(&l).map_err(|e| e.to_string())?;
    Ok(n)
}

/// An answered moment as another eye's evidence row.
pub fn row(d: &Double, media: &str, project: &str) -> Option<Row> {
    Some(Row {
        media: media.into(),
        a: d.a,
        b: d.b,
        axis: d.axis.clone(),
        bad: d.agrees?,
        weight: 1.0,
        who: "clef".into(),
        source: "eye:clef".into(),
        project: project.into(),
        kind: None,
        region: None,
        words: None,
        at: 0,
        pin: None,
        band: None,
    })
}

#[cfg(test)]
mod tests;
