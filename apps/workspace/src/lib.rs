//! A workspace's folders and files (docs/projects.feature PJ1-PJ4): `raea workspace` and `raea project` (with split
//! and join). A workspace is a git repo raea changes by commits.
use std::path::{Path, PathBuf};

use serde_json::json;

mod client;
pub mod doctor;
mod repair;
pub mod next;
pub mod registry;
mod storage;
mod workspace;

pub use registry::context;
pub use workspace::{default_root, project, projects, root, slug, up, workspace};

pub type R<T> = Result<T, String>;

#[cfg(test)]
mod tests;
