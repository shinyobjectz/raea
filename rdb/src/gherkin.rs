//! Briefs as contracts (docs/relational-plan.feature): a Gherkin .feature brief lowers to checks only. Given and When
//! scope a check, Then states what must hold; nothing in a brief writes to the edit. Checking runs every frame of the
//! edit (pure frame states, no rendering) and reports, per scenario, pass or fail, the failing frames and the rows at
//! fault. That report is the Agent's definition of done.
//!
//!   Background   -> conditions ANDed into every scenario's scope (they hold everywhere)
//!   Given        -> the kind (table) and filters the scenario talks about
//!   When         -> a condition: frames or rows where the check applies
//!   Then / And   -> a predicate that must hold on every row in scope, on every frame in scope
//!   Scenario Outline + Examples -> one check per row
//!
//! The vocabulary is a list of steps (role, phrase, kind, filter, holds, about), written in the retired rrl (a
//! steps.rrl beside the brief, which rrl's steps.rs lowered to `expect`s; nothing reads them since rrl's deletion on
//! 2026-10-17, and this module knows no query language). A phrase's "{name}" slot takes quoted text, {name} a number or a word; slots substitute into the
//! fragments. A step no phrase matches is refused with its line and the library's phrases for that role.

use std::collections::BTreeMap;

use crate::eval::{self, Tup};
use crate::ir::{self, V};

/// One step of a vocabulary, as written: `kind` is the row kind a Given picks, `filter` a fragment that scopes (a
/// Given's or a When's), `holds` a Then's condition, `about` why it matters. Fragments are the step language's text.
#[derive(Debug, Clone, Default)]
pub struct StepSpec {
    pub role: String,
    pub phrase: String,
    pub kind: String,
    pub filter: String,
    pub holds: String,
    pub about: String,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub role: String,
    pub phrase: String,
    pub kind: String,
    pub filter: String,
    pub holds: String,
    pub about: String,
    re: regex::Regex,
    slots: Vec<String>,
}

pub struct Steps(Vec<Step>);

impl Steps {
    pub fn new(specs: Vec<StepSpec>) -> Result<Steps, String> {
        let mut out = Vec::new();
        for sp in specs {
            let (re, slots) = pattern(&sp.phrase)?;
            out.push(Step { role: sp.role.to_lowercase(), phrase: sp.phrase, kind: sp.kind, filter: sp.filter, holds: sp.holds, about: sp.about, re, slots });
        }
        Ok(Steps(out))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A phrase to an anchored, case-insensitive regex, and its slot names in order.
fn pattern(phrase: &str) -> Result<(regex::Regex, Vec<String>), String> {
    let slot = regex::Regex::new(r#""\{(\w+)\}"|\{(\w+)\}"#).unwrap();
    let mut re = String::from("(?i)^");
    let mut slots = Vec::new();
    let mut last = 0;
    for m in slot.captures_iter(phrase) {
        let all = m.get(0).unwrap();
        re.push_str(&regex::escape(&phrase[last..all.start()]));
        if let Some(q) = m.get(1) {
            re.push_str(r#""([^"]*)""#);
            slots.push(q.as_str().to_string());
        } else {
            re.push_str(r"([-+]?[0-9]*\.?[0-9]+|[A-Za-z_][\w-]*)");
            slots.push(m.get(2).unwrap().as_str().to_string());
        }
        last = all.end();
    }
    re.push_str(&regex::escape(&phrase[last..]));
    re.push_str(r"\.?$");
    Ok((regex::Regex::new(&re).map_err(|e| format!("phrase {phrase:?}: {e}"))?, slots))
}

/// One Then of one scenario, lowered: over `kind` rows where every `scope` fragment holds, `holds` must be true on
/// every frame. The step language compiles it (rrl: `expect holds if scope...`).
#[derive(Debug, Clone)]
pub struct Check {
    pub label: String,
    pub scenario: String,
    pub then: String,
    pub about: String,
    pub kind: String,
    /// the Givens' and Whens' fragments, in order (Background's first)
    pub scope: Vec<String>,
    pub holds: String,
}

#[derive(Debug, Clone)]
struct StepLine {
    keyword: String,
    text: String,
    line: usize,
}

#[derive(Debug, Default, Clone)]
struct Scenario {
    name: String,
    line: usize,
    steps: Vec<StepLine>,
    examples: Vec<Vec<String>>,
}

#[derive(Default)]
struct Part {
    kind: Option<String>,
    filters: Vec<String>,
    holds: Vec<(String, String, String)>,
}

fn resolve(steps: &Steps, role: &str, s: &StepLine, path: &str, part: &mut Part) -> Result<(), String> {
    let text = s.text.trim();
    let hit = steps.0.iter().filter(|st| st.role == role).find_map(|st| st.re.captures(text).map(|c| (st, c)));
    let Some((st, caps)) = hit else {
        let near: Vec<&str> = steps.0.iter().filter(|st| st.role == role).map(|st| st.phrase.as_str()).collect();
        return Err(format!("{path}:{}: no {role} step matches {text:?} (the library's {role} phrases: {})", s.line, near.join(" | ")));
    };
    let fill = |tpl: &str| -> String {
        let mut out = tpl.to_string();
        for (i, name) in st.slots.iter().enumerate() {
            let v = caps.get(i + 1).map(|m| m.as_str()).unwrap_or("").replace('\'', "''");
            out = out.replace(&format!("{{{name}}}"), &v);
        }
        out
    };
    if !st.kind.is_empty() {
        part.kind = Some(fill(&st.kind));
    }
    if !st.filter.is_empty() {
        part.filters.push(fill(&st.filter));
    }
    if !st.holds.is_empty() {
        part.holds.push((format!("{} {}", s.keyword, text), fill(&st.holds), fill(&st.about)));
    }
    Ok(())
}

fn role_of(keyword: &str, prev: &str) -> String {
    match keyword {
        "Given" => "given",
        "When" => "when",
        "Then" => "then",
        _ => prev,
    }
    .to_string()
}

/// Lower a brief to its checks.
pub fn lower(src: &str, path: &str, steps: &Steps) -> Result<Vec<Check>, String> {
    let mut background: Vec<StepLine> = Vec::new();
    let mut scenarios: Vec<Scenario> = Vec::new();
    let (mut in_bg, mut in_examples) = (false, false);
    for (n, raw) in src.lines().enumerate() {
        let line = n + 1;
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with('@') || t.starts_with("Feature:") {
            continue;
        }
        if t.starts_with("Background:") {
            (in_bg, in_examples) = (true, false);
            continue;
        }
        if let Some(rest) = t.strip_prefix("Scenario Outline:").or_else(|| t.strip_prefix("Scenario:")).or_else(|| t.strip_prefix("Example:")) {
            (in_bg, in_examples) = (false, false);
            scenarios.push(Scenario { name: rest.trim().to_string(), line, ..Default::default() });
            continue;
        }
        if t.starts_with("Examples:") {
            in_examples = true;
            continue;
        }
        if t.starts_with('|') {
            if !in_examples {
                return Err(format!("{path}:{line}: a table outside Examples"));
            }
            let cells: Vec<String> = t.trim_matches('|').split('|').map(|c| c.trim().to_string()).collect();
            scenarios.last_mut().ok_or(format!("{path}:{line}: Examples outside a scenario"))?.examples.push(cells);
            continue;
        }
        let Some(kw) = ["Given", "When", "Then", "And", "But"].into_iter().find(|k| t.starts_with(&format!("{k} "))) else { continue };
        let step = StepLine { keyword: kw.to_string(), text: t[kw.len()..].trim().to_string(), line };
        if in_bg {
            background.push(step);
        } else {
            scenarios.last_mut().ok_or(format!("{path}:{line}: a step outside a scenario"))?.steps.push(step);
        }
    }
    let mut bg = Part::default();
    let mut prev = "given".to_string();
    for s in &background {
        let role = role_of(&s.keyword, &prev);
        prev = role.clone();
        if role == "then" {
            return Err(format!("{path}:{}: Background states context (Given/When); Thens belong in scenarios", s.line));
        }
        let mut p = Part::default();
        resolve(steps, &role, s, path, &mut p)?;
        if p.kind.is_some() {
            return Err(format!("{path}:{}: Background holds for every scenario, so it can't pick a kind; pick it in each scenario", s.line));
        }
        bg.filters.extend(p.filters);
    }
    let mut out = Vec::new();
    for sc in &scenarios {
        let rows: Vec<Vec<(String, String)>> = match sc.examples.split_first() {
            Some((head, body)) => body.iter().map(|r| head.iter().cloned().zip(r.iter().cloned()).collect()).collect(),
            None => vec![vec![]],
        };
        for (ri, row) in rows.iter().enumerate() {
            let mut part = Part { filters: bg.filters.clone(), ..Default::default() };
            let mut prev = "given".to_string();
            for s in &sc.steps {
                let mut s = s.clone();
                for (k, v) in row {
                    s.text = s.text.replace(&format!("<{k}>"), v);
                }
                let role = role_of(&s.keyword, &prev);
                prev = role.clone();
                let mut p = Part::default();
                resolve(steps, &role, &s, path, &mut p)?;
                if let Some(k) = p.kind {
                    match &part.kind {
                        Some(old) if *old != k => return Err(format!("{path}:{}: a scenario talks about one kind ({old}), not {k}; split it", s.line)),
                        _ => part.kind = Some(k),
                    }
                }
                part.filters.extend(p.filters);
                part.holds.extend(p.holds);
            }
            let kind = part.kind.clone().ok_or(format!("{path}:{}: scenario {:?} needs a Given that picks what it's about", sc.line, sc.name))?;
            if part.holds.is_empty() {
                return Err(format!("{path}:{}: scenario {:?} has no Then: a brief states what must hold", sc.line, sc.name));
            }
            let name = if row.is_empty() { sc.name.clone() } else { format!("{} [{}]", sc.name, ri + 1) };
            let scope = part.filters.clone();
            for (then, holds, about) in part.holds {
                out.push(Check { label: format!("{path}:{} {name} / {then}", sc.line), scenario: name.clone(), then, about, kind: kind.clone(), scope: scope.clone(), holds });
            }
        }
    }
    Ok(out)
}

/// One scenario's result.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub scenario: String,
    pub then: String,
    pub about: String,
    pub in_scope: usize,
    /// row id -> failing frames
    pub fails: BTreeMap<String, Vec<i64>>,
    /// one failing row's values at its first failing frame, for the message
    pub example: Option<String>,
}

pub struct Report(pub Vec<Outcome>);

impl Report {
    pub fn ok(&self) -> bool {
        self.0.iter().all(|o| o.fails.is_empty() && o.in_scope > 0)
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        let mut by: BTreeMap<&str, Vec<&Outcome>> = BTreeMap::new();
        for o in &self.0 {
            by.entry(o.scenario.as_str()).or_default().push(o);
        }
        for o in &self.0 {
            let _ = by.get(o.scenario.as_str());
        }
        let mut seen = std::collections::BTreeSet::new();
        for o in &self.0 {
            if !seen.insert(o.scenario.clone()) {
                continue;
            }
            let thens = &by[o.scenario.as_str()];
            let pass = thens.iter().all(|t| t.fails.is_empty() && t.in_scope > 0);
            s.push_str(&format!("{} {}\n", if pass { "PASS" } else { "FAIL" }, o.scenario));
            for t in thens {
                if t.in_scope == 0 {
                    s.push_str(&format!("  {}: nothing in scope on any frame (does the edit have what this talks about?)\n", t.then));
                    continue;
                }
                for (id, fr) in &t.fails {
                    let span = match (fr.first(), fr.last()) {
                        (Some(a), Some(b)) if a != b => format!("frames {a}..={b} ({} frames)", fr.len()),
                        (Some(a), _) => format!("frame {a}"),
                        _ => String::new(),
                    };
                    s.push_str(&format!("  {} -- {id} breaks it at {span}", t.then));
                    if !t.about.is_empty() {
                        s.push_str(&format!(": {}", t.about));
                    }
                    s.push('\n');
                }
                if let Some(ex) = &t.example {
                    s.push_str(&format!("    e.g. {ex}\n"));
                }
            }
        }
        let n = by.len();
        let fails = by.values().filter(|ts| !ts.iter().all(|t| t.fails.is_empty() && t.in_scope > 0)).count();
        s.push_str(&format!("{} of {n} scenarios hold\n", n - fails));
        s
    }
}

/// Check a brief against an edit: every frame's state (inserted rows, keys, rules; no rendering). Each check comes
/// compiled by the step language (an ir::Assert: kind, filter, holds).
pub fn check(compiled: &[(ir::Assert, Check)], prog: &ir::Program, inserted: &[Tup]) -> Result<Report, String> {
    let mut outcomes: Vec<Outcome> = compiled
        .iter()
        .map(|(_, c)| Outcome { scenario: c.scenario.clone(), then: c.then.clone(), about: c.about.clone(), in_scope: 0, fails: BTreeMap::new(), example: None })
        .collect();
    let tl = crate::timeline::Timeline::new(prog, inserted)?;
    for f in 0..prog.frames.max(1) {
        let state = tl.cols(f);
        for (i, (a, _)) in compiled.iter().enumerate() {
            let rows = eval::select_cols(&state, &a.kind, a.filter.as_ref(), f).map_err(|e| format!("{}: {e}", a.label))?;
            outcomes[i].in_scope += rows.len();
            if rows.is_empty() {
                continue;
            }
            let ctes = Default::default();
            let view = eval::SetView { rows: rows.clone(), all: &state, ctes: &ctes, frame: f };
            let h = ir::eval_set(&a.holds, &view).map_err(|e| format!("{}: {e}", a.label))?;
            for (&k, ok) in rows.iter().zip(h) {
                if !ok.truthy() {
                    let t = state.tup(k);
                    outcomes[i].fails.entry(t.id.clone()).or_default().push(f);
                    if outcomes[i].example.is_none() {
                        let vals: Vec<String> = t.vals.iter().filter(|(k, _)| !matches!(k.as_str(), "text" | "font")).map(|(k, v)| format!("{k}={}", match v {
                            V::N(x) => format!("{}", (x * 100.0).round() / 100.0),
                            V::S(s) => s.clone(),
                        })).collect();
                        outcomes[i].example = Some(format!("{} at frame {f}: {}", t.id, vals.join(", ")));
                    }
                }
            }
        }
    }
    Ok(Report(outcomes))
}
