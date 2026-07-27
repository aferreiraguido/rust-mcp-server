//! `find_unsafe_blocks` — detects all `unsafe { ... }` blocks.

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

pub struct FindUnsafeBlocksTool {
    base: BaseRustTool,
}

impl FindUnsafeBlocksTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for FindUnsafeBlocksTool {
    fn name(&self) -> &'static str {
        "find_unsafe_blocks"
    }

    fn description(&self) -> &'static str {
        "Sweep 1 of bug-hunt: finds all `unsafe { ... }` blocks in the project. \
         Static analysis via `syn` — no semantic engine needed. Each finding \
         includes the file and a short evidence message."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Project name or alias" },
                "min_severity": {
                    "type": "string",
                    "enum": ["info", "low", "medium", "high", "critical"],
                    "description": "Minimum severity to report (default: low)"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let min_sev = match args
            .get("min_severity")
            .and_then(|v| v.as_str())
            .unwrap_or("low")
        {
            "info" => FindingSeverity::Info,
            "medium" => FindingSeverity::Medium,
            "high" => FindingSeverity::High,
            "critical" => FindingSeverity::Critical,
            _ => FindingSeverity::Low,
        };
        let entry = self.base.find_entry(&name).await?;
        let findings =
            tokio::task::spawn_blocking(move || BugPatternAnalyzer::find_unsafe_blocks(&entry))
                .await
                .map_err(|e| ErrorData::internal_error(format!("join: {}", e), None))?
                .map_err(|e| ErrorData::internal_error(format!("analyze: {}", e), None))?;
        let md = bug_hunt_helpers::format_findings("Unsafe blocks", &findings, min_sev);
        Ok(BaseRustTool::ok_markdown(md))
    }
}
