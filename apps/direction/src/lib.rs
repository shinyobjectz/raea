//! How raea takes direction (docs/direction.feature). `raea world` (world.rs): a composition's frame as one world of
//! facts, its 3D, page and timeline as entities with components, printed or queried across media.

mod world;
pub use world::{cli, find, Find, USAGE};

// how `raea render` ends: its findings counted last, and 1 on an error not let through (DR3)
mod rendered;
pub use rendered::rendered;
// frame purity at a render's end: a sample of its frames rendered again and held to the video's (SG9)
mod pure;

// `raea findings`: the project's record read back, fixed, new and carried (DR3)
pub mod findings;

// `raea check PROJECT | EDIT.tsx`: every rule's check, raea's named ones and a project's TS ones, over the worlds (DR4)
pub mod check;
pub mod systems;
pub mod ts;
// text-read: each text's reading time held to when it can be read, and what OCR reads of it (DR28)
pub mod read;
