//! `search_source` — text search in the project's `.rs` files.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct SearchSourceTool {
    base: BaseRustTool,
}

impl SearchSourceTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for SearchSourceTool {
    fn name(&self) -> &'static str {
        "search_source"
    }

    fn description(&self) -> &'static str {
        "Text search across all `.rs` files in a loaded project. \
         Returns matching lines with file:line:context format. \
         Case-sensitive by default; set `case_sensitive=false` for case-insensitive."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "query": {
                    "type": "string",
                    "description": "Text to search for"
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "If false, search is case-insensitive (default: true)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Limit number of results (default: 200)"
                }
            },
            "required": ["name", "query"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let query = BaseRustTool::arg(&args, "query")?;
        let case_sensitive = BaseRustTool::bool_arg(&args, "case_sensitive");
        let max = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(200);
        let entry = self.base.find_entry(&name).await?;
        let hits = RustAnalyzerUtils::search_source(&entry, &query, case_sensitive)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let total = hits.len();
        let mut md = String::new();
        md.push_str(&format!(
            "# Search: `{}` (case_{}) — {} hit(s)\n\n",
            query,
            if case_sensitive {
                "sensitive"
            } else {
                "insensitive"
            },
            total
        ));
        if total == 0 {
            md.push_str("_No matches._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        let take = hits.iter().take(max);
        for h in take {
            md.push_str(&format!(
                "**`{}:{}`**\n```rust\n{}\n```\n",
                h.file, h.line, h.context
            ));
        }
        if total > max {
            md.push_str(&format!("\n_… {} more results not shown._\n", total - max));
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
