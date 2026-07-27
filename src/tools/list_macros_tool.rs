//! `list_macros` — declared macros (heuristic for `macro_rules!` and proc-macros).

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListMacrosTool {
    base: BaseRustTool,
}

impl ListMacrosTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListMacrosTool {
    fn name(&self) -> &'static str {
        "list_macros"
    }

    fn description(&self) -> &'static str {
        "Lists macros declared in a loaded project. With iter 2 (syn-only) \
         only top-level `macro_rules!` invocations and references are \
         detected. Full proc-macro support arrives with iter 3 (ra_ap_*)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let entry = self.base.find_entry(&name).await?;
        let items = RustAnalyzerUtils::list_macros(&entry)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let mut md = String::new();
        md.push_str(&format!("# Macros ({})\n\n", items.len()));
        if items.is_empty() {
            md.push_str("_No macros._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        for m in &items {
            md.push_str(&format!("### `{}` ({})\n\n", m.name, m.kind));
            md.push_str(&format!("- **File**: `{}`\n", m.file));
            if let Some(args) = &m.args {
                let preview = crate::tools::render::truncate(args, 100);
                if !preview.is_empty() {
                    md.push_str(&format!("- **Preview**: `{}`\n", preview));
                }
            }
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
