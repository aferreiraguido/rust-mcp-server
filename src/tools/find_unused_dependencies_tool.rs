//! `find_unused_dependencies` — detects deps declared in `Cargo.toml`
//! that are not referenced in the code.
//!
//! Heuristic: for each dep, searches for whole-word matches in all
//! `.rs` files of the project. If 0 matches, marks it as unused.

use crate::analysis::bug_pattern_analyzer::BugPatternAnalyzer;
use crate::model::FindingSeverity;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::bug_hunt_helpers;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct FindUnusedDependenciesTool {
    base: BaseRustTool,
}

impl FindUnusedDependenciesTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for FindUnusedDependenciesTool {
    fn name(&self) -> &'static str {
        "find_unused_dependencies"
    }

    fn description(&self) -> &'static str {
        "Sweep 6 of bug-hunt: finds dependencies declared in [dependencies], \
         [dev-dependencies] and [build-dependencies] that are never used in \
         the project's `.rs` files. Heuristic whole-word text search — for \
         accurate analysis, use `cargo install cargo-machete`."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Project name or alias" },
                "min_severity": {
                    "type": "string",
                    "enum": ["info", "low", "medium", "high", "critical"],
                    "description": "Minimum severity to report (default: info)"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        // min_severity: default `info` so dev-deps show up too.
        let min_sev = match args
            .get("min_severity")
            .and_then(|v| v.as_str())
            .unwrap_or("info")
        {
            "low" => FindingSeverity::Low,
            "medium" => FindingSeverity::Medium,
            "high" => FindingSeverity::High,
            "critical" => FindingSeverity::Critical,
            _ => FindingSeverity::Info,
        };
        let entry = self.base.find_entry(&name).await?;
        let findings = tokio::task::spawn_blocking(move || {
            BugPatternAnalyzer::find_unused_dependencies(&entry)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("join: {}", e), None))?
        .map_err(|e| ErrorData::internal_error(format!("analyze: {}", e), None))?;
        let md = bug_hunt_helpers::format_findings("Unused dependencies", &findings, min_sev);
        Ok(BaseRustTool::ok_markdown(md))
    }
}
