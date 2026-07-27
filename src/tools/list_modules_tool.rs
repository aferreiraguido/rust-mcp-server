//! `list_modules` — `mod` declarations in the project.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListModulesTool {
    base: BaseRustTool,
}

impl ListModulesTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListModulesTool {
    fn name(&self) -> &'static str {
        "list_modules"
    }

    fn description(&self) -> &'static str {
        "Lists all `mod` declarations in a loaded project: name, file, \
         whether inline (with body in the same file) or external."
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
        let items = RustAnalyzerUtils::list_modules(&entry)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let mut md = String::new();
        md.push_str(&format!("# Modules ({})\n\n", items.len()));
        if items.is_empty() {
            md.push_str("_No modules._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        for m in &items {
            let kind = if m.is_inline { "inline" } else { "external" };
            md.push_str(&format!(
                "- `{}` (`{}`, file: `{}`)\n",
                m.name, kind, m.file
            ));
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
