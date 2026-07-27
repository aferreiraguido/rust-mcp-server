//! `unload_rust_project` — quita un proyecto del cache.

use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct UnloadRustProjectTool {
    base: BaseRustTool,
}

impl UnloadRustProjectTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for UnloadRustProjectTool {
    fn name(&self) -> &'static str {
        "unload_rust_project"
    }

    fn description(&self) -> &'static str {
        "Removes a project from the in-memory cache by name or alias. \
         Returns NOT_FOUND if the project isn't loaded."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias to unload"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        match self.base.manager.remove(&name).await {
            Some(entry) => {
                let md = format!(
                    "# Project Unloaded\n\nRemoved `{}` (path: `{}`)\n",
                    entry.effective_name(),
                    entry.path.display()
                );
                Ok(BaseRustTool::ok_markdown(md))
            }
            None => Ok(BaseRustTool::ok_error(format!(
                "Project '{}' not found (or already expired)",
                name
            ))),
        }
    }
}
