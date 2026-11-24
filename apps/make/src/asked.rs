//! The second tier of the gate (docs/make.feature MK4): the `make` set of `raea ask` questions (core/ask
//! library/make.json), asked of the job's newest sheets. A question gates once it is calibrated on the person's labels
//! (`raea ask calibrate`: 16 problem, 16 clean, an AUC of 0.8); until then what it says is advice in the status. An
//! answer counts for a sheet only when its call was about exactly that sheet's bytes (the ledger's `ident`), so a sheet
//! rendered again must be asked again.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use raea_core_ask::{Calibration, Question};
use serde_json::{Value, json};

/// The ledger as status reads it: every call's ident by id, every answer, and the person's calibrations.
#[derive(Default)]
pub struct Ledger {
    pub idents: HashMap<String, String>,
    pub answers: Vec<Value>,
    pub calibrations: BTreeMap<String, Calibration>,
}

impl Ledger {
    /// `raea ask`'s record (RAEA_ASK_DIR, else ~/.raea/ask), read once.
    pub fn read() -> Ledger {
        let dir = raea_io_home::env_path("RAEA_ASK_DIR", ".raea/ask");
        let rows = |f: &str| -> Vec<Value> { std::fs::read_to_string(dir.join(f)).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect() };
        let idents = rows("calls.jsonl").into_iter().filter_map(|c| Some((c["id"].as_str()?.to_string(), c["ident"].as_str()?.to_string()))).collect();
        let answers = rows("answers.jsonl").into_iter().filter(|a| a["question"].as_str().is_some_and(|q| q.starts_with("make."))).collect();
        let calibrations = std::fs::read_to_string(dir.join("calibration.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Ledger { idents, answers, calibrations }
    }
}

/// The `make` set's questions with the person's calibrations in place of raea's (as `raea ask` applies them).
pub fn questions(ledger: &Ledger) -> Vec<Question> {
    let mut qs = raea_core_ask::library::set("make", None).unwrap_or_default();
    for q in &mut qs {
        if let Some(c) = ledger.calibrations.get(&q.version()) {
            q.calibration = Some(c.clone());
        }
    }
    qs
}

/// The sheet a question is asked of: its id's second word (make.clay.floating is asked of a clay sheet).
pub fn sheet_of(q: &Question) -> &str {
    match q.id.split('.').nth(1) {
        Some(sheet) => sheet,
        None => "",
    }
}

/// The command that asks a sheet its questions.
pub fn command(sheet: &Path, qs: &[&Question]) -> String {
    let std: Vec<String> = qs.iter().map(|q| format!("--std {}", q.id)).collect();
    format!("raea ask {} {}", sheet.display(), std.join(" "))
}

/// Each sheet's gate, and the advice: what uncalibrated questions flagged. `sheets` are (kind, file): clay, colour,
/// refs, framing. `notes` is NOTES.md: a flagged question answered there by its id holds nothing.
pub fn gates(sheets: &[(&str, PathBuf)], ledger: &Ledger, notes: &str) -> (Vec<Value>, Vec<String>) {
    let qs = questions(ledger);
    let mut gates = Vec::new();
    let mut advice = Vec::new();
    for (kind, file) in sheets {
        let mine: Vec<&Question> = qs.iter().filter(|q| sheet_of(q) == *kind).collect();
        if mine.is_empty() {
            continue;
        }
        let rel = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let Some(ident) = raea_core_ask::ident::ident(file) else { continue };
        // the newest answer to each question about these exact bytes
        let mut latest: BTreeMap<&str, &Value> = BTreeMap::new();
        for a in &ledger.answers {
            let about = a["call"].as_str().and_then(|c| ledger.idents.get(c)).is_some_and(|i| *i == ident);
            let Some(q) = a["question"].as_str() else { continue };
            if about && latest.get(q).is_none_or(|b| at(a) >= at(b)) {
                latest.insert(q, a);
            }
        }
        let mut open = Vec::new();
        let mut unasked = Vec::new();
        for q in &mine {
            let a = latest.get(q.id.as_str());
            let flagged = a.is_some_and(|a| a["flagged"] == true);
            let p = a.and_then(|a| a["problem"].as_f64()).map(|p| format!(" (p {p:.2})")).unwrap_or_default();
            if q.calibrated() {
                match a {
                    None => unasked.push(*q),
                    Some(_) if flagged && !notes.contains(&q.id) => open.push(format!("{} flagged{p}: {}", q.id, q.fix.as_deref().unwrap_or("fix it, or answer it in NOTES.md by its id"))),
                    _ => {}
                }
            } else if flagged {
                advice.push(format!("{rel}: {} says yes{p}, advice until calibrated: {}", q.id, q.fix.as_deref().unwrap_or("")));
            }
        }
        if !unasked.is_empty() {
            open.push(format!("not asked of this {kind} sheet yet: {}", command(file, &unasked)));
        }
        let calibrated = mine.iter().filter(|q| q.calibrated()).count();
        let says = match (open.is_empty(), calibrated) {
            (true, 0) => format!("advice only: none of its {} questions is calibrated yet (raea ask calibrate)", mine.len()),
            (true, n) => format!("{n} calibrated question{} clear", if n == 1 { "" } else { "s" }),
            (false, _) => open.join("; "),
        };
        let mut g = json!({"gate": "ask", "of": rel, "clear": open.is_empty(), "says": says});
        g["ask"] = json!(command(file, &mine));
        gates.push(g);
    }
    (gates, advice)
}

fn at(a: &Value) -> u64 {
    a["at"].as_str().and_then(|s| s.parse().ok()).or_else(|| a["at"].as_u64()).unwrap_or(0)
}
