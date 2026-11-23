//! `find_dead_code` — heuristic for unreferenced items.
//!
//! **Iter 4 implementation**: textual (whole-word search) using `syn`.
//! Possible false positives (e.g. items used only from macros).
//! Semantic version with `ra_ap_*` → iter 4.5.

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
use tracing::info;

pub struct FindDeadCodeTool {
    base: BaseRustTool,
}

impl FindDeadCodeTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for FindDeadCodeTool {
    fn name(&self) -> &'static str {
        "find_dead_code"
    }

    fn description(&self) -> &'static str {
        "Sweep 5 of bug-hunt: finds non-`pub` items that are never \
         referenced elsewhere in the project. Iter 4.5 implementation uses \
         the semantic engine (`ra_ap_*` `find_all_refs`) — detects items \
         with zero references including those used only from macros."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Project name or alias" }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let entry = self.base.find_entry(&name).await?;
        let findings = tokio::task::spawn_blocking(move || {
            let host = crate::analysis::analysis_host::ProjectAnalysisHost::load(&entry.path)
                .map_err(|e| format!("analysis load failed: {}", e))?;
            BugPatternAnalyzer::find_dead_code(&host).map_err(|e| format!("analyze: {}", e))
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("join: {}", e), None))?
        .map_err(|e| ErrorData::internal_error(e, None))?;
        info!(count = findings.len(), "find_dead_code completed");
        let md = bug_hunt_helpers::format_findings(
            "Dead code (semantic)",
            &findings,
            FindingSeverity::Low,
        );
        Ok(BaseRustTool::ok_markdown(md))
    }
}
