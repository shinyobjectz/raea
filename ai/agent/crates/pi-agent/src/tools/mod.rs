//! Builtin tools: read, write, edit, ls, grep, glob, todo.
//!
//! Mirrors the core toolset shipped in `packages/coding-agent/src/core/...`. the host app's copy drops
//! `bash` (spawns processes, which the iPhone can't) and `web_fetch` (open web access); the app's
//! own tools in `ugc-agent` replace them.

pub mod edit;
pub mod glob_tool;
pub mod grep;
pub mod ls;
pub mod read;
pub mod todo;
pub mod write;

use std::sync::Arc;

use crate::types::AgentTool;

/// Returns the default suite of builtin tools used by the coding agent.
pub fn default_tools() -> Vec<Arc<dyn AgentTool>> {
    vec![
        Arc::new(read::ReadTool),
        Arc::new(write::WriteTool),
        Arc::new(edit::EditTool),
        Arc::new(ls::LsTool),
        Arc::new(grep::GrepTool),
        Arc::new(glob_tool::GlobTool),
        Arc::new(todo::TodoTool::new()),
    ]
}
