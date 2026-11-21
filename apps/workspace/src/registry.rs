//! A project's compositions and components (docs/projects.feature PJ8): raea-app-registry's reading of its files.
use std::path::Path;

pub use raea_app_registry::{Component, Composition, Fix, How, Level, Lint, Prop, Registry, apply, scan};

/// What `raea context` reads of the project, as (section, text): its compositions and components as raea reads them.
pub fn context(from: &Path) -> Vec<(String, String)> {
    crate::doctor::registry_dir(from).map(|dir| ("PROJECT FILES (raea project scan)".to_string(), scan(&dir).summary())).into_iter().collect()
}
