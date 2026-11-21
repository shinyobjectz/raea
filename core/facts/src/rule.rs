//! A rule: a line the work is held to, and the check that holds it (docs/direction.feature DR4). The check is one of
//! raea's, by name with its values (apps/direction systems.rs), or a project's own TS check by its path (sdk/raea
//! check.ts). Where a rule was written (raea's floor, the person's, a workspace's or a project's taste file)
//! changes nothing about how it runs: it is only said, as its findings' `by`. A taste file's six forms are six of
//! raea's named checks with the same names and values, so every taste file reads as it did.

use std::path::Path;

use raea_core_brief::taste;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Level;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// what `--only` names: a named check's name, or a check file's name without .ts
    pub id: String,
    /// the line it holds the work to
    pub text: String,
    /// one of raea's checks by name, or a .ts check's path (resolved from the file the rule was written in)
    pub check: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub values: Map<String, Value>,
    /// how bad what it finds is (None: the check's own level, else an error)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
    /// where it was written: "raea", or "taste", "workspace" or "project" and the file; "note" and the ask's id
    pub by: String,
    /// the one composition it runs on, its file from the project (its id after a `#`), as a note's ask names it;
    /// none: every composition
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<String>,
}

impl Rule {
    /// One of raea's checks by `name`, held to `values`.
    pub fn named(name: &str, text: &str, values: Map<String, Value>, level: Level, by: &str) -> Rule {
        Rule { id: name.into(), text: text.into(), check: name.into(), values, level: Some(level), by: by.into(), of: None }
    }

    /// The project's .ts check this rule runs, when it runs one.
    pub fn file(&self) -> Option<&Path> {
        self.check.ends_with(".ts").then(|| Path::new(&self.check))
    }

    /// A value the rule holds its check to: a number.
    pub fn number(&self, name: &str) -> Option<f64> {
        self.values.get(name).and_then(Value::as_f64)
    }

    /// A taste line's rule, when it carries a check. One of the six forms is raea's named check of that form, its
    /// value as `value`, and an error unless the line says otherwise. A check by name, or by path (joined to `dir`, the
    /// taste file's folder; its `..` steps are the caller's to take, and the file's name is the rule's id), is held to
    /// the line's `values` at the line's level (unset: the check's own). `by`: "taste" and the file.
    pub fn of_taste(r: &taste::Rule, dir: &Path, by: &str) -> Option<Rule> {
        let level = r.level.map(|l| match l {
            taste::RuleLevel::Error => Level::Error,
            taste::RuleLevel::Warn => Level::Warn,
            taste::RuleLevel::Note => Level::Note,
        });
        match r.check.as_ref()? {
            taste::RuleCheck::Form(c) => {
                let values = Map::from_iter([("value".to_string(), Value::from(c.value()))]);
                Some(Rule::named(c.form(), &r.text, values, level.unwrap_or(Level::Error), by))
            }
            taste::RuleCheck::Named(check) => {
                let (check, id) = match check.ends_with(".ts") {
                    true => {
                        let path = dir.join(check);
                        let id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        (path.display().to_string(), id)
                    }
                    false => (check.clone(), check.clone()),
                };
                Some(Rule { id, text: r.text.clone(), check, values: r.values.clone().unwrap_or_default(), level, by: by.into(), of: None })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(j: serde_json::Value) -> taste::Rule {
        serde_json::from_value(j).unwrap()
    }

    #[test]
    fn a_taste_form_is_the_named_check_of_the_same_name_and_value() {
        let r = Rule::of_taste(&line(serde_json::json!({"text": "Captions sit clear of the bottom.", "check": {"form": "text-above", "value": 62}})), Path::new("/p"), "taste taste.ts").unwrap();
        assert_eq!((r.id.as_str(), r.check.as_str(), r.number("value"), r.level), ("text-above", "text-above", Some(62.0), Some(Level::Error)));
        assert!(r.file().is_none() && r.by == "taste taste.ts");
        assert!(Rule::of_taste(&line(serde_json::json!({"text": "The look is neutral."})), Path::new("/p"), "taste taste.ts").is_none(), "words alone hold nothing");
    }

    #[test]
    fn a_taste_lines_check_by_path_is_from_its_folder_and_by_name_keeps_its_values() {
        let clear = line(serde_json::json!({"text": "Captions clear the button band", "check": "../checks/clear.ts", "level": "warn"}));
        let r = Rule::of_taste(&clear, Path::new("/p/sub"), "taste sub/taste.ts").unwrap();
        assert_eq!((r.id.as_str(), r.file(), r.level), ("clear", Some(Path::new("/p/sub/../checks/clear.ts")), Some(Level::Warn)));
        let loud = Rule::of_taste(&line(serde_json::json!({"text": "Loud enough", "check": "loudness", "values": {"value": -14}})), Path::new("/p"), "taste taste.ts").unwrap();
        assert_eq!((loud.check.as_str(), loud.number("value"), loud.level), ("loudness", Some(-14.0), None));
    }
}
