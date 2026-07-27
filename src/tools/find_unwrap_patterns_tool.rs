//! `find_unwrap_patterns` — detects `.unwrap()` / `.expect()` calls.

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

pub struct FindUnwrapPatternsTool {
    base: BaseRustTool,
}

impl FindUnwrapPatternsTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for FindUnwrapPatternsTool {
    fn name(&self) -> &'static str {
        "find_unwrap_patterns"
    }

    fn description(&self) -> &'static str {
        "Sweep 2 of bug-hunt: finds all `.unwrap()` and `.expect()` method \
         calls. Each can panic at runtime. Static analysis via `syn`."
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
            tokio::task::spawn_blocking(move || BugPatternAnalyzer::find_unwrap_patterns(&entry))
                .await
                .map_err(|e| ErrorData::internal_error(format!("join: {}", e), None))?
                .map_err(|e| ErrorData::internal_error(format!("analyze: {}", e), None))?;
        let md = bug_hunt_helpers::format_findings("Unwrap / expect", &findings, min_sev);
        Ok(BaseRustTool::ok_markdown(md))
    }
}
