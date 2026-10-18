use async_trait::async_trait;
use ignore::WalkBuilder;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;

use crate::types::{AgentTool, AgentToolResult};

pub struct GrepTool;

#[async_trait]
impl AgentTool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn description(&self) -> &str {
        "Search file contents under a directory. The pattern is a regex by default; set fixed_string=true to match it literally. Supports before/after context lines. Honors .gitignore."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Pattern to search for (regex by default)"},
                "path": {"type": "string", "description": "Directory to search (default: cwd)"},
                "fixed_string": {"type": "boolean", "default": false, "description": "If true, treat pattern as a literal string (escaped with regex::escape)"},
                "before": {"type": "integer", "default": 0, "description": "Number of context lines to show before each match"},
                "after": {"type": "integer", "default": 0, "description": "Number of context lines to show after each match"},
                "max_matches": {"type": "integer", "default": 200}
            },
            "required": ["pattern"]
        })
    }
    async fn execute(&self, _id: &str, args: Value) -> Result<AgentToolResult, String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or("missing 'pattern'")?
            .to_string();
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();
        let fixed_string = args
            .get("fixed_string")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let before = args.get("before").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let after = args.get("after").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let max = args
            .get("max_matches")
            .and_then(|v| v.as_u64())
            .unwrap_or(200) as usize;

        let compiled_pattern = if fixed_string {
            regex::escape(&pattern)
        } else {
            pattern.clone()
        };
        let re = Regex::new(&compiled_pattern).map_err(|e| format!("invalid regex: {e}"))?;

        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let mut buf = String::new();
            let mut hits = 0usize;
            let walker = WalkBuilder::new(&path).follow_links(false).build();
            'outer: for entry in walker.flatten() {
                if hits >= max {
                    break;
                }
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let text = match fs::read_to_string(p) {
                    Ok(t) => t,
                    Err(_) => continue, // skip binary or unreadable
                };
                let lines: Vec<&str> = text.lines().collect();
                // Find matching line indices first.
                let mut match_indices: Vec<usize> = Vec::new();
                for (i, line) in lines.iter().enumerate() {
                    if re.is_match(line) {
                        match_indices.push(i);
                    }
                }
                if match_indices.is_empty() {
                    continue;
                }
                // Cap match indices by remaining budget.
                let remaining = max.saturating_sub(hits);
                if match_indices.len() > remaining {
                    match_indices.truncate(remaining);
                }
                // Collect context line indices for each match.
                let match_set: BTreeSet<usize> = match_indices.iter().copied().collect();
                let mut context_set: BTreeSet<usize> = BTreeSet::new();
                for &mi in &match_indices {
                    let start = mi.saturating_sub(before);
                    let end = mi.saturating_add(after).min(lines.len().saturating_sub(1));
                    for k in start..=end {
                        context_set.insert(k);
                    }
                }
                for idx in &context_set {
                    let lineno = idx + 1;
                    let line = lines[*idx];
                    let sep = if match_set.contains(idx) { ':' } else { '-' };
                    buf.push_str(&format!(
                        "{}{}{}{}{}\n",
                        p.display(),
                        sep,
                        lineno,
                        sep,
                        line
                    ));
                }
                hits += match_indices.len();
                if hits >= max {
                    buf.push_str(&format!("... (truncated at {max} matches)\n"));
                    break 'outer;
                }
            }
            Ok(buf)
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(AgentToolResult::text(if result.is_empty() {
            "(no matches)".to_string()
        } else {
            result
        }))
    }
}
