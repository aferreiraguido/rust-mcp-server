//! `find_references` — all textual occurrences of a symbol.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct FindReferencesTool {
    base: BaseRustTool,
}

impl FindReferencesTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for FindReferencesTool {
    fn name(&self) -> &'static str {
        "find_references"
    }

    fn description(&self) -> &'static str {
        "Text-based reference search for a symbol across all `.rs` files \
         in a loaded project. Returns matching lines with file:line:context. \
         NOTE: this is textual, not semantic — for true cross-references \
         with type resolution, use iter 3 (ra_ap_*)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "symbol": {
                    "type": "string",
                    "description": "Symbol to search for"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Limit number of results (default: 100)"
                }
            },
            "required": ["name", "symbol"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let symbol = BaseRustTool::arg(&args, "symbol")?;
        let max = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(100);
        let entry = self.base.find_entry(&name).await?;
        let hits = RustAnalyzerUtils::find_references(&entry, &symbol, true)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let total = hits.len();
        let mut md = String::new();
        md.push_str(&format!(
            "# References to `{}` — {} hit(s)\n\n",
            symbol, total
        ));
        if total == 0 {
            md.push_str("_No references found._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        for h in hits.iter().take(max) {
            md.push_str(&format!(
                "**`{}:{}`**\n```rust\n{}\n```\n",
                h.file, h.line, h.context
            ));
        }
        if total > max {
            md.push_str(&format!("\n_… {} more results._\n", total - max));
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
