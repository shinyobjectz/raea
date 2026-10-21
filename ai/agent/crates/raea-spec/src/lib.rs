//! Gherkin for the Agent harness (agent/harness.feature, stage 1): one parser for the evals (agent/evals/features),
//! the skill's playbooks, the product specs (docs/creator-lifecycle) and, later, the prompt rules. It keeps what the
//! harness needs and Cucumber's own tools don't give a Rust build: `Rule:` blocks with their own Background, tags at
//! every level (`@eval`, `@gap`, `@name:<id>`, `@k:K-12`, `@claim:<id>`, `@cite:<lesson>`, `@knob:<id>`), docstrings,
//! data tables and each scenario's source lines (the reviewer reads a scenario's own Gherkin).
//!
//! Std only. Line numbers are 1-based; `start..end` ranges are 0-based line indexes into the source, end exclusive.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword { Given, When, Then }

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub keyword: Keyword,
    /// The step's text after its keyword word (`Given`, `And`, `*`…), trimmed.
    pub text: String,
    /// A docstring under the step, its indentation removed.
    pub doc: Option<String>,
    /// A data table under the step: rows of trimmed cells.
    pub table: Option<Vec<Vec<String>>>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Background {
    pub steps: Vec<Step>,
    /// The Background's source lines (keyword through its last step or docstring).
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scenario {
    pub name: String,
    /// The scenario's own tags (see `Feature::tags_of` for the inherited ones).
    pub tags: Vec<String>,
    /// Free text between the title and the first step.
    pub description: Vec<String>,
    pub steps: Vec<Step>,
    /// A Scenario Outline (or Template), with its Examples tables.
    pub outline: bool,
    pub examples: Vec<Vec<Vec<String>>>,
    pub line: usize,
    /// From its first tag line to the next scenario, rule or the end.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rule {
    pub name: String,
    pub tags: Vec<String>,
    pub description: Vec<String>,
    pub background: Option<Background>,
    pub scenarios: Vec<Scenario>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Feature {
    pub name: String,
    pub tags: Vec<String>,
    pub description: Vec<String>,
    pub background: Option<Background>,
    /// Scenarios before any Rule.
    pub scenarios: Vec<Scenario>,
    pub rules: Vec<Rule>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError { pub line: usize, pub message: String }

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "line {}: {}", self.line, self.message) }
}

/// A parsed file: the Feature (None when the file has none) and what couldn't be read. Parsing goes on past an
/// error, so one bad line doesn't hide the rest of the file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub feature: Option<Feature>,
    pub errors: Vec<ParseError>,
}

impl Feature {
    /// Every scenario with the Rule it sits under, in file order.
    pub fn all_scenarios(&self) -> Vec<(Option<&Rule>, &Scenario)> {
        let mut out: Vec<(Option<&Rule>, &Scenario)> = self.scenarios.iter().map(|s| (None, s)).collect();
        for r in &self.rules { out.extend(r.scenarios.iter().map(|s| (Some(r), s))); }
        out
    }

    /// A scenario's tags with those it inherits: the Feature's, then its Rule's, then its own.
    pub fn tags_of(&self, rule: Option<&Rule>, scenario: &Scenario) -> Vec<String> {
        let mut tags = self.tags.clone();
        if let Some(r) = rule { tags.extend(r.tags.iter().cloned()); }
        tags.extend(scenario.tags.iter().cloned());
        tags
    }

    /// The Background steps that run before a scenario: the Feature's, then its Rule's.
    pub fn background_of<'a>(&'a self, rule: Option<&'a Rule>) -> Vec<&'a Step> {
        let mut steps: Vec<&Step> = self.background.iter().flat_map(|b| b.steps.iter()).collect();
        if let Some(b) = rule.and_then(|r| r.background.as_ref()) { steps.extend(b.steps.iter()); }
        steps
    }
}

/// Whether `tags` has `tag` (with its `@`).
pub fn has_tag(tags: &[String], tag: &str) -> bool { tags.iter().any(|t| t == tag) }

/// The value of a `@key:value` tag (`tag_value(&tags, "name")` for `@name:real-hook-cut-30s`); the first wins.
pub fn tag_value<'a>(tags: &'a [String], key: &str) -> Option<&'a str> {
    let prefix = format!("@{key}:");
    tags.iter().find_map(|t| t.strip_prefix(prefix.as_str()))
}

/// Every value of a `@key:value` tag (`@k:K-3 @k:K-12`).
pub fn tag_values<'a>(tags: &'a [String], key: &str) -> Vec<&'a str> {
    let prefix = format!("@{key}:");
    tags.iter().filter_map(|t| t.strip_prefix(prefix.as_str())).collect()
}

/// The source lines `start..end` of `text`, joined.
pub fn source(text: &str, start: usize, end: usize) -> String {
    text.lines().skip(start).take(end.saturating_sub(start)).collect::<Vec<_>>().join("\n")
}

/// Where the lines being read belong.
#[derive(Clone, Copy, PartialEq)]
enum At { Nothing, Feature, Rule, Background, Scenario, Examples }

pub fn parse(text: &str) -> Document {
    let lines: Vec<&str> = text.lines().collect();
    let mut doc = Document::default();
    let mut feature: Option<Feature> = None;
    let mut at = At::Nothing;
    let mut pending: Vec<String> = Vec::new();
    let mut pending_from: Option<usize> = None;
    let mut last_keyword: Option<Keyword> = None;
    let err = |doc: &mut Document, line: usize, message: String| doc.errors.push(ParseError { line, message });

    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim();
        let n = i + 1;
        i += 1;

        if line.starts_with("\"\"\"") || line.starts_with("```") {
            let fence = &line[..3];
            let indent = raw.len() - raw.trim_start().len();
            let mut body = Vec::new();
            let mut closed = false;
            while i < lines.len() {
                let l = lines[i];
                i += 1;
                if l.trim() == fence { closed = true; break; }
                let cut = (l.len() - l.trim_start().len()).min(indent);
                body.push(&l[cut..]);
            }
            if !closed { err(&mut doc, n, format!("a docstring opened with {fence} isn't closed")); }
            match last_step(&mut feature, at) {
                Some(step) if step.doc.is_none() => step.doc = Some(body.join("\n")),
                Some(_) => err(&mut doc, n, "a step can have one docstring".into()),
                None => err(&mut doc, n, "a docstring belongs under a step".into()),
            }
            set_end(&mut feature, at, i);
            continue;
        }
        if line.is_empty() || line.starts_with('#') { continue; }
        if line.starts_with('|') {
            let row = cells(line);
            if at == At::Examples {
                if let Some(s) = current_scenario(&mut feature) {
                    if let Some(t) = s.examples.last_mut() { t.push(row); }
                }
            } else {
                match last_step(&mut feature, at) {
                    Some(step) => step.table.get_or_insert_with(Vec::new).push(row),
                    None => err(&mut doc, n, "a table belongs under a step or Examples".into()),
                }
            }
            set_end(&mut feature, at, i);
            continue;
        }
        if line.starts_with('@') {
            pending_from.get_or_insert(n - 1);
            let tags = line.split(" #").next().unwrap_or(line);
            pending.extend(tags.split_whitespace().map(String::from));
            continue;
        }

        if let Some((head, rest)) = line.split_once(':') {
            let rest = rest.trim().to_string();
            match head.trim() {
                "Feature" => {
                    if feature.is_some() { err(&mut doc, n, "a file has one Feature".into()); }
                    feature = Some(Feature { name: rest, tags: std::mem::take(&mut pending), line: n, ..Default::default() });
                    pending_from = None;
                    at = At::Feature;
                    continue;
                }
                "Rule" => {
                    let Some(f) = feature.as_mut() else { err(&mut doc, n, "a Rule needs a Feature above it".into()); continue };
                    close_scenario(f, pending_from.unwrap_or(n - 1));
                    f.rules.push(Rule { name: rest, tags: std::mem::take(&mut pending), line: n, ..Default::default() });
                    pending_from = None;
                    at = At::Rule;
                    continue;
                }
                "Background" => {
                    let Some(f) = feature.as_mut() else { err(&mut doc, n, "a Background needs a Feature above it".into()); continue };
                    close_scenario(f, n - 1);
                    let bg = Some(Background { steps: Vec::new(), start: n - 1, end: n });
                    let slot = match f.rules.last_mut() { Some(r) => &mut r.background, None => &mut f.background };
                    if slot.is_some() { err(&mut doc, n, "one Background per Feature or Rule".into()); }
                    *slot = bg;
                    pending.clear();
                    pending_from = None;
                    at = At::Background;
                    last_keyword = None;
                    continue;
                }
                "Scenario" | "Example" | "Scenario Outline" | "Scenario Template" => {
                    let Some(f) = feature.as_mut() else { err(&mut doc, n, "a Scenario needs a Feature above it".into()); continue };
                    let start = pending_from.take().unwrap_or(n - 1);
                    close_scenario(f, start);
                    let s = Scenario {
                        name: rest, tags: std::mem::take(&mut pending), outline: head.contains("Outline") || head.contains("Template"),
                        line: n, start, end: n, ..Default::default()
                    };
                    match f.rules.last_mut() { Some(r) => r.scenarios.push(s), None => f.scenarios.push(s) }
                    at = At::Scenario;
                    last_keyword = None;
                    continue;
                }
                "Examples" | "Scenarios" => {
                    match current_scenario(&mut feature) {
                        Some(s) if at == At::Scenario || at == At::Examples => {
                            if !s.outline { err(&mut doc, n, "Examples belong to a Scenario Outline".into()); }
                            s.examples.push(Vec::new());
                            s.end = n;
                        }
                        _ => err(&mut doc, n, "Examples belong to a Scenario Outline".into()),
                    }
                    pending.clear();
                    pending_from = None;
                    at = At::Examples;
                    continue;
                }
                _ => {}
            }
        }

        // A step, or free text.
        let (word, text) = line.split_once(' ').map(|(w, t)| (w, t.trim())).unwrap_or((line, ""));
        let keyword = match word {
            "Given" => Some(Keyword::Given),
            "When" => Some(Keyword::When),
            "Then" => Some(Keyword::Then),
            "And" | "But" | "*" => Some(last_keyword.unwrap_or(Keyword::Given)),
            _ => None,
        };
        match (keyword, at) {
            (Some(k), At::Background | At::Scenario) => {
                if matches!(word, "And" | "But") && last_keyword.is_none() {
                    err(&mut doc, n, format!("\"{word}\" needs a Given, When or Then before it"));
                }
                last_keyword = Some(k);
                let step = Step { keyword: k, text: text.to_string(), doc: None, table: None, line: n };
                if let Some(f) = feature.as_mut() {
                    match at {
                        At::Background => if let Some(b) = background_mut(f) { b.steps.push(step); b.end = n; },
                        _ => if let Some(s) = scenario_mut(f) { s.steps.push(step); s.end = n; },
                    }
                }
            }
            // Under a Feature or Rule header, a line that starts like a step ("When a creator is…") is description,
            // as Cucumber's own parser reads it.
            (Some(_), At::Feature) => if let Some(f) = feature.as_mut() { f.description.push(line.to_string()) },
            (Some(_), At::Rule) => if let Some(r) = feature.as_mut().and_then(|f| f.rules.last_mut()) { r.description.push(line.to_string()) },
            (Some(_), _) => err(&mut doc, n, format!("a step outside a Scenario or Background: {line}")),
            (None, _) => {
                // Free text: a description, before the first step.
                let Some(f) = feature.as_mut() else { err(&mut doc, n, format!("text before the Feature: {line}")); continue };
                match at {
                    At::Feature => f.description.push(line.to_string()),
                    At::Rule => if let Some(r) = f.rules.last_mut() { r.description.push(line.to_string()) },
                    At::Scenario => match scenario_mut(f) {
                        Some(s) if s.steps.is_empty() => s.description.push(line.to_string()),
                        _ => err(&mut doc, n, format!("text after a scenario's steps: {line}")),
                    },
                    _ => err(&mut doc, n, format!("unexpected text: {line}")),
                }
            }
        }
    }
    if let Some(f) = feature.as_mut() { close_scenario(f, lines.len()); }
    if feature.is_none() && !text.trim().is_empty() { err(&mut doc, 1, "no Feature in this file".into()); }
    doc.feature = feature;
    doc
}

fn cells(line: &str) -> Vec<String> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() { Some('|') => cell.push('|'), Some('n') => cell.push('\n'), Some(o) => { cell.push('\\'); cell.push(o) }, None => cell.push('\\') },
            '|' => out.push(std::mem::take(&mut cell).trim().to_string()),
            c => cell.push(c),
        }
    }
    out.push(cell.trim().to_string());
    out
}

fn scenario_mut(f: &mut Feature) -> Option<&mut Scenario> {
    match f.rules.last_mut() { Some(r) => r.scenarios.last_mut(), None => f.scenarios.last_mut() }
}

fn background_mut(f: &mut Feature) -> Option<&mut Background> {
    match f.rules.last_mut() { Some(r) => r.background.as_mut(), None => f.background.as_mut() }
}

fn current_scenario(feature: &mut Option<Feature>) -> Option<&mut Scenario> { feature.as_mut().and_then(scenario_mut) }

fn last_step(feature: &mut Option<Feature>, at: At) -> Option<&mut Step> {
    let f = feature.as_mut()?;
    match at {
        At::Background => background_mut(f)?.steps.last_mut(),
        At::Scenario => scenario_mut(f)?.steps.last_mut(),
        _ => None,
    }
}

/// Moves the end of whatever is being read past a docstring or table that ended at line index `end`.
fn set_end(feature: &mut Option<Feature>, at: At, end: usize) {
    let Some(f) = feature.as_mut() else { return };
    match at {
        At::Background => if let Some(b) = background_mut(f) { b.end = end },
        At::Scenario | At::Examples => if let Some(s) = scenario_mut(f) { s.end = end },
        _ => {}
    }
}

/// The open scenario runs up to line index `next` (where the next scenario, rule or background begins).
fn close_scenario(f: &mut Feature, next: usize) {
    if let Some(s) = scenario_mut(f) { if s.end <= next { s.end = next; } }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"@feature-tag
Feature: Pricing
  A creator sets rates.
  Two lines of description.

  Background:
    Given a project "Glossier holiday" for the brand "glossier"
    And the file "scripts/rate-card.md":
      """
      # Rate card
        indented line
      """

  @eval @name:no-rate-yet @k:K-12 # a comment after tags
  Scenario: The creator has no rate yet
    The Agent offers the course's tiers as guidance.

    When the user says "What should I charge?"
    Then the reply mentions /\$\d+/
    But it doesn't invent a rate

  Rule: Usage is its own line
    The base rate never includes paid usage.

    Background:
      Given the takes:
        | take                 | lines |
        | take-20261008-111940 | 4     |

    @eval @claim:pricing/addons-separate-lines
    Scenario: A rate card with add-ons
      When the user says:
        ```json
        {"rate": 400}
        ```
      Then "scripts/*.md" contains /[Uu]sage/

    Scenario Outline: Durations
      Then the <addon> line says <duration>

      Examples:
        | addon     | duration |
        | usage     | 90 days  |
        | spark ads | a month  |
"#;

    #[test]
    fn features_rules_backgrounds_and_tags() {
        let d = parse(SPEC);
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        let f = d.feature.unwrap();
        assert_eq!((f.name.as_str(), f.tags.clone()), ("Pricing", vec!["@feature-tag".to_string()]));
        assert_eq!(f.description, vec!["A creator sets rates.", "Two lines of description."]);
        let bg = f.background.as_ref().unwrap();
        assert_eq!(bg.steps.len(), 2);
        assert_eq!(bg.steps[1].keyword, Keyword::Given, "And takes the keyword before it");
        assert_eq!(bg.steps[1].doc.as_deref(), Some("# Rate card\n  indented line"));
        assert_eq!(source(SPEC, bg.start, bg.end).lines().last().unwrap().trim(), "\"\"\"", "the background's source ends after its docstring");

        let s = &f.scenarios[0];
        assert_eq!(s.name, "The creator has no rate yet");
        assert_eq!(s.tags, vec!["@eval", "@name:no-rate-yet", "@k:K-12"], "a comment after tags is dropped");
        assert_eq!(s.description, vec!["The Agent offers the course's tiers as guidance."]);
        assert_eq!(s.steps.iter().map(|x| x.keyword).collect::<Vec<_>>(), vec![Keyword::When, Keyword::Then, Keyword::Then]);
        assert!(source(SPEC, s.start, s.end).starts_with("  @eval @name:no-rate-yet"), "a scenario's source starts at its tags");
        assert_eq!(tag_value(&s.tags, "name"), Some("no-rate-yet"));
        assert_eq!(tag_values(&s.tags, "k"), vec!["K-12"]);

        let r = &f.rules[0];
        assert_eq!((r.name.as_str(), r.description.len()), ("Usage is its own line", 1));
        let table = r.background.as_ref().unwrap().steps[0].table.clone().unwrap();
        assert_eq!(table[1], vec!["take-20261008-111940", "4"]);
        let addons = &r.scenarios[0];
        assert_eq!(addons.steps[0].doc.as_deref(), Some("{\"rate\": 400}"), "a ``` fence with a content type");
        assert_eq!(f.tags_of(Some(r), addons), vec!["@feature-tag", "@eval", "@claim:pricing/addons-separate-lines"]);
        assert_eq!(f.background_of(Some(r)).len(), 3, "the Feature's background, then the Rule's");
        let outline = &r.scenarios[1];
        assert!(outline.outline);
        assert_eq!(outline.examples, vec![vec![vec!["addon", "duration"], vec!["usage", "90 days"], vec!["spark ads", "a month"]]
            .into_iter().map(|row| row.into_iter().map(String::from).collect::<Vec<_>>()).collect::<Vec<_>>()]);
        assert_eq!(f.all_scenarios().len(), 3);
        assert!(source(SPEC, outline.start, outline.end).ends_with("| spark ads | a month  |"));
    }

    #[test]
    fn mistakes_are_reported_with_their_line_and_parsing_goes_on() {
        let d = parse("Feature: x\n  Scenario: a\n    And the first step\n    Then it works\n    stray text\n  Given outside\n");
        let lines: Vec<usize> = d.errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, vec![3, 5], "{:?}", d.errors);
        assert_eq!(d.feature.unwrap().scenarios[0].steps.len(), 3, "the Given after stray text still lands in the scenario");
        assert!(!parse("Scenario: no feature\n  Given x\n").errors.is_empty());
        assert!(parse("").errors.is_empty(), "an empty file is fine");
        let open = parse("Feature: x\n  Scenario: a\n    Given y:\n      \"\"\"\n      never closed\n");
        assert!(open.errors.iter().any(|e| e.message.contains("isn't closed")));
    }

    /// Every .feature the harness has parses with no errors, and finds as many scenarios as the file has headers.
    #[test]
    fn every_feature_in_the_repo_parses() {
        // the harness's own Gherkin lives with the host (the host app's evals, prompts and playbooks): RAEA_SPEC_REPO, else
        // ~/host; raea's area docs wrap their steps across lines, which a harness feature doesn't
        let repo = std::env::var_os("RAEA_SPEC_REPO").map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("host"));
        if !repo.join("agent/evals/features").is_dir() {
            eprintln!("no harness repo at {} (RAEA_SPEC_REPO): skipped", repo.display());
            return;
        }
        let mut files = Vec::new();
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if p.is_dir() && !name.starts_with('.') && !matches!(name.as_str(), "node_modules" | "target" | "runs" | "vendor") { walk(&p, out) }
                else if name.ends_with(".feature") { out.push(p) }
            }
        }
        for d in ["agent/evals/features", "agent", "skill/host/references/playbooks", "docs", "knowledge"] { walk(&repo.join(d), &mut files); }
        files.sort();
        files.dedup();
        assert!(files.len() >= 10, "found {}", files.len());
        let mut problems = Vec::new();
        for f in &files {
            let text = std::fs::read_to_string(f).unwrap();
            let d = parse(&text);
            for e in &d.errors { problems.push(format!("{}: {e}", f.display())); }
            // Headers outside doc strings (a quoted example scenario isn't one).
            let mut fence: Option<&str> = None;
            let headers = text.lines().filter(|l| {
                let t = l.trim_start();
                if let Some(f) = fence { if t.trim_end() == f { fence = None; } return false; }
                if t.starts_with("\"\"\"") || t.starts_with("```") { fence = Some(&t[..3]); return false; }
                ["Scenario:", "Example:", "Scenario Outline:", "Scenario Template:"].iter().any(|h| t.starts_with(h))
            }).count();
            let found = d.feature.as_ref().map(|x| x.all_scenarios().len()).unwrap_or(0);
            if found != headers { problems.push(format!("{}: {found} scenarios parsed, {headers} headers", f.display())); }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }
}
