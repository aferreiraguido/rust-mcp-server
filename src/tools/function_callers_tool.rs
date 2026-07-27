//! `function_callers` — who calls a function (text-based).
//!
//! ⚠️ **DEPRECATED**: prefer `function_call_graph` (ra_ap_* semantic).

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct FunctionCallersTool {
    base: BaseRustTool,
}

impl FunctionCallersTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for FunctionCallersTool {
    fn name(&self) -> &'static str {
        "function_callers"
    }

    fn description(&self) -> &'static str {
        "⚠️ DEPRECATED — prefer `function_call_graph` (semantic ra_ap_*). \
         Text-based search for callers of a function: finds lines that \
         contain `<symbol>(` (function-call style)."
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
                    "description": "Function name to look up callers for"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Limit (default: 100)"
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
        // Buscar `<symbol>(` (con paréntesis) para reducir falsos positivos.
        let query = format!("{}(", symbol);
        let hits = RustAnalyzerUtils::find_references(&entry, &query, true)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let total = hits.len();
        let mut md = String::new();
        md.push_str(&format!("# Callers of `{}` — {} hit(s)\n\n", symbol, total));
        if total == 0 {
            md.push_str("_No callers found (text search)._\n");
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
