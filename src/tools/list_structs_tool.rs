//! `list_structs` — all structs in the project.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListStructsTool {
    base: BaseRustTool,
}

impl ListStructsTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListStructsTool {
    fn name(&self) -> &'static str {
        "list_structs"
    }

    fn description(&self) -> &'static str {
        "Lists all struct declarations in a loaded project: name, file, \
         kind (struct|tuple|unit), derives, field count, doc comment."
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
        let items = RustAnalyzerUtils::list_structs(&entry)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let mut md = String::new();
        md.push_str(&format!("# Structs ({})\n\n", items.len()));
        if items.is_empty() {
            md.push_str("_No structs._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        for s in &items {
            md.push_str(&format!("### `{}` ({})\n\n", s.name, s.kind));
            md.push_str(&format!("- **File**: `{}`\n", s.file));
            md.push_str(&format!("- **Fields**: {}\n", s.fields.len()));
            if !s.derives.is_empty() {
                md.push_str(&format!(
                    "- **Derives**: {}\n",
                    s.derives
                        .iter()
                        .map(|d| format!("`{}`", d))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if let Some(doc) = &s.doc_comment {
                md.push_str(&format!(
                    "- **Doc**: {}\n",
                    crate::tools::render::truncate(doc, 200)
                ));
            }
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
