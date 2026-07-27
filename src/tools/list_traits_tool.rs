//! `list_traits` — all traits.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListTraitsTool {
    base: BaseRustTool,
}

impl ListTraitsTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListTraitsTool {
    fn name(&self) -> &'static str {
        "list_traits"
    }

    fn description(&self) -> &'static str {
        "Lists all trait declarations in a loaded project: name, file, \
         required methods, doc comment."
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
        let items = RustAnalyzerUtils::list_traits(&entry)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let mut md = String::new();
        md.push_str(&format!("# Traits ({})\n\n", items.len()));
        if items.is_empty() {
            md.push_str("_No traits._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        for t in &items {
            md.push_str(&format!("### `{}`\n\n", t.name));
            md.push_str(&format!("- **File**: `{}`\n", t.file));
            md.push_str(&format!("- **Items**: {}\n", t.methods.len()));
            if !t.methods.is_empty() {
                md.push_str("- **Members**:\n");
                for m in &t.methods {
                    md.push_str(&format!("  - `{}`\n", m));
                }
            }
            if let Some(doc) = &t.doc_comment {
                md.push_str(&format!(
                    "- **Doc**: {}\n",
                    crate::tools::render::truncate(doc, 200)
                ));
            }
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
