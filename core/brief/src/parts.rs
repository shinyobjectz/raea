//! An object's parts (NAME.parts.ts, `parts()` from "raea/brief"), written before any geometry and frozen when
//! building starts (docs/handoff/solids.md, "The plan is the contract"; named parts since 11-20, so "plan" is only a
//! project's plan.org, docs/plan.feature PL8a). Generic over what's made: a crown, a feather and a rocking horse are all a concept and a tree of
//! named parts, each with a level, a count, and a material and style it has or inherits.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Parts {
    pub concept: Concept,
    /// by name; the build's parts are typed from these keys, so a planned part the build skips is a tsc error
    pub parts: BTreeMap<String, Part>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Concept {
    /// what it is ("flight feather", "toy rocking horse")
    pub what: String,
    pub style: String,
    /// its largest extent, in metres
    pub size: f64,
    /// how close it's seen, in metres: sets how much detail up close must hold
    pub seen_from: f64,
    pub material: Option<String>,
    #[serde(rename = "use")]
    #[ts(rename = "use")]
    pub use_: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// the silhouette that reads from far away
    Primary,
    /// the structure that reads at mid distance
    Secondary,
    /// the detail that reads up close
    Tertiary,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum Count {
    Exact(u32),
    /// made by a generator, between these
    Range { min: u32, max: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Part {
    /// the label it carries into the world ("seat", "leaf", "gem")
    pub role: String,
    pub level: Level,
    /// the part it sits on or in; none for the root
    pub parent: Option<String>,
    pub count: Option<Count>,
    /// its own, else its parent's, else the concept's
    pub material: Option<String>,
    pub style: Option<String>,
    /// what a near crop of it must show
    pub near: Option<String>,
}

/// A part's material and style after inheritance.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub material: String,
    pub style: String,
}

/// Everything wrong with a plan that would let a build leave something behind: a parent that isn't planned, a loop,
/// no root, a range upside down, or a part that resolves to no material. Empty when the plan can be built against.
pub fn check(plan: &Parts) -> Vec<String> {
    let mut out = Vec::new();
    let roots: Vec<&String> = plan.parts.iter().filter(|(_, p)| p.parent.is_none()).map(|(n, _)| n).collect();
    if roots.is_empty() {
        out.push("no root part: one part has no parent".into());
    }
    for (name, p) in &plan.parts {
        if let Some(parent) = &p.parent
            && !plan.parts.contains_key(parent)
        {
            out.push(format!("`{name}`'s parent `{parent}` isn't planned"));
        }
        if let Some(Count::Range { min, max }) = p.count
            && min > max
        {
            out.push(format!("`{name}`'s count runs {min} to {max}"));
        }
        if chain(plan, name).is_none() {
            out.push(format!("`{name}` is in a loop of parents"));
        } else if resolve(plan, name).is_none() {
            out.push(format!("`{name}` has no material: give it one, or its parent, or the concept"));
        }
    }
    if plan.concept.seen_from < plan.concept.size && !plan.parts.values().any(|p| p.level == Level::Tertiary) {
        out.push(format!("seen from {} m, closer than its size: plan the tertiary detail up close", plan.concept.seen_from));
    }
    out
}

/// The part and its ancestors, nearest first; none on a loop or a missing parent.
fn chain<'a>(plan: &'a Parts, name: &str) -> Option<Vec<&'a Part>> {
    let mut out = Vec::new();
    let mut at = plan.parts.get(name)?;
    loop {
        out.push(at);
        if out.len() > plan.parts.len() {
            return None;
        }
        match &at.parent {
            None => return Some(out),
            Some(p) => at = plan.parts.get(p)?,
        }
    }
}

/// A part's material and style, inherited up its parents to the concept. Style always resolves (the concept has
/// one); material may not.
pub fn resolve(plan: &Parts, name: &str) -> Option<Resolved> {
    let chain = chain(plan, name)?;
    let material = chain.iter().find_map(|p| p.material.clone()).or_else(|| plan.concept.material.clone())?;
    let style = chain.iter().find_map(|p| p.style.clone()).unwrap_or_else(|| plan.concept.style.clone());
    Some(Resolved { material, style })
}
