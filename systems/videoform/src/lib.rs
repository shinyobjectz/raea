//! A videoform's maths over its kept rows (docs/videoform.md): what's kept of each reference (rows.rs), what the
//! references hold measure by measure with the person's corrections (ranges.rs), what the vocabulary's terms did on
//! them (vocab.rs), the shot kinds and the structure made from them (VF6, VF7); the rules are raea-sys-videoform-rules' (VF8), and following new footage raea-sys-videoform-follow's (VF9). Nothing here opens a reference: it all runs on the rows.

pub mod groups;
pub mod kinds;
pub mod ranges;
pub mod rows;
pub mod structure;
pub mod vocab;

pub use rows::{Rows, Shot};
