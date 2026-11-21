//! What an agent writes for raea, as Rust types: a project's brief, a script (storyboard or screenplay), a person's
//! taste and an object's plan. The agent writes them as TypeScript modules (docs/handoff/authoring.md); `raea run`
//! evaluates one, and its default export arrives here as JSON. The TypeScript declarations are generated from these
//! types (`declarations`), so what tsc checks is exactly what this crate reads.

pub mod brief;
pub mod controller;
pub mod plan;
pub mod script;
pub mod taste;
pub mod videoform;

pub use brief::Brief;
pub use controller::Controller;
pub use plan::Plan;
pub use script::Script;
pub use taste::Taste;
pub use videoform::{Correction, Videoform, Vocabulary};

use serde::de::DeserializeOwned;
use ts_rs::{Config, TS};

/// Reads one of the schemas from the JSON a module's default export evaluated to. The error names the field and
/// what was wrong with it (an unknown field is an error too, so a misspelt `belive` isn't silently dropped).
pub fn read<T: DeserializeOwned>(what: &str, json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("{what}: {e}"))
}

macro_rules! decls {
    ($cfg:ident; $($t:ty),* $(,)?) => { vec![$(format!("export {}", <$t as TS>::decl(&$cfg))),*] };
}

/// The TypeScript declarations of every schema, one file's worth: sdk/raea/src/brief.gen.ts is this, checked by a test, and
/// `raea/brief` (sdk/raea/src/brief.ts) types its builders with it, so the declarations ship with the SDK.
pub fn declarations() -> String {
    let cfg = Config::new();
    let mut out = vec!["// Generated from raea-core-brief (core/brief) by its `declarations`; edit the Rust, not this.".to_string()];
    out.extend(decls!(cfg;
        brief::Brief, brief::Section, brief::Deliverable, brief::Reference, brief::Stage, brief::Mark, brief::Task, brief::Agent,
        script::Script, script::Scene, script::Beat, script::Line,
        taste::Taste, taste::TasteSection, taste::Rule, taste::Check,
        plan::Plan, plan::Concept, plan::Level, plan::Count, plan::Part,
        controller::Controller, controller::Group, controller::Field, controller::Kind, controller::Step,
        videoform::Videoform,
        videoform::Correction,
        videoform::Vocabulary, videoform::Term, videoform::Pattern, videoform::Section, videoform::Feel,
    ));
    out.join("\n\n") + "\n"
}

#[cfg(test)]
mod tests;
