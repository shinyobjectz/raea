//! Questions for the decision model (clef-omni), as every caller of `raea ask` writes them (docs/ask.feature AK1): the
//! question and how its answer reads, the sets that come standard, and the model's limits. Its TypeScript declarations
//! are generated from these types (`declarations`): sdk/raea/src/ask.gen.ts, checked by a test.

pub mod ident;
pub mod library;
pub mod limits;
pub mod question;

pub use question::{Calibration, Facts, Kind, LEVELS, Picks, Question, Reading, Sees, When, Window};

use ts_rs::{Config, TS};

macro_rules! decls {
    ($cfg:ident; $($t:ty),* $(,)?) => { vec![$(format!("export {}", <$t as TS>::decl(&$cfg))),*] };
}

/// The TypeScript of every type, one file's worth: sdk/raea/src/ask.gen.ts is this, checked by a test.
pub fn declarations() -> String {
    let cfg = Config::new();
    let mut out = vec!["// Generated from raea-core-ask (core/ask) by its `declarations`; edit the Rust, not this.".to_string()];
    out.extend(decls!(cfg; question::Question, question::Kind, question::Sees, question::Picks, question::When, question::Window, question::Calibration));
    out.push(format!("export const SETS = {:?} as const;", library::SETS));
    out.join("\n\n") + "\n"
}

#[cfg(test)]
mod tests;
