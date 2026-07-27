//! `list_crates` — lists workspace members (or the single crate).

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListCratesTool {
    base: BaseRustTool,
}

impl ListCratesTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListCratesTool {
    fn name(&self) -> &'static str {
        "list_crates"
    }

    fn description(&self) -> &'static str {
        "Lists the crates in a loaded workspace. For a single-crate project, \
         returns the crate itself. Workspace members are read from the \
         [workspace.members] entry of the root Cargo.toml."
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

        let mut md = String::new();
        let members: Vec<String> = if entry.member_crates.is_empty() {
            vec![entry.name.clone()]
        } else {
            entry.member_crates.clone()
        };
        md.push_str(&format!(
            "# Crates in `{}` ({})\n\n",
            entry.name,
            members.len()
        ));
        for m in &members {
            let path = entry.path.join(m);
            let has_cargo = path.join("Cargo.toml").exists();
            md.push_str(&format!(
                "- `{}` {}\n",
                m,
                if has_cargo {
                    ""
                } else {
                    "(no Cargo.toml found)"
                }
            ));
        }
        // Also pull the manifest's crate name for the root
        if let Ok(manifest) = RustAnalyzerUtils::cargo_manifest(&entry) {
            if let Some(cn) = manifest.crate_name() {
                md.push_str(&format!("\nRoot crate: `{}`\n", cn));
            }
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
