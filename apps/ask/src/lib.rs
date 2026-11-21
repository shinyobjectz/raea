//! `raea ask` (docs/ask.feature AK2-AK3): questions about a piece's picture and sound. `prepare` reads what raea knows
//! of the piece and keeps the questions that apply; `runner` plans the calls, cuts each clip once and asks in parallel;
//! `record` keeps every call and answer; `labels` reads the person's verdicts, notes and approvals into labels, and
//! `calibrate` holds each question to them (AK6-AK7); `verbs` is the command. `raea check`, the app's critique and the
//! fingerprint's double-check go through the same runner.

pub mod calibrate;
pub mod labels;
pub mod prepare;
pub mod record;
pub mod runner;
pub mod verbs;

pub use verbs::{USAGE, cli};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod labels_tests;
