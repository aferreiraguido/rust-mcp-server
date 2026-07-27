//! `list_loaded_projects` — the first tool. Lists loaded projects.
//!
//! Es el "hola mundo" del patrón: prueba `ProjectManager`, `BaseRustTool`,
//! el trait `McpTool`, y la serialización Markdown determinista.
//! Equivalente directo a `ListLoadedProjectsTool` en java/cobol-mcp-server.

use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListLoadedProjectsTool {
    base: BaseRustTool,
}

impl ListLoadedProjectsTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListLoadedProjectsTool {
    fn name(&self) -> &'static str {
        "list_loaded_projects"
    }

    fn description(&self) -> &'static str {
        "Lists all currently loaded Rust projects. Output is Markdown, sorted \
         by effective name (alias if set, else directory name). Entries expire \
         after 10 minutes by default."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    async fn call(&self, _args: Value) -> Result<CallToolResult, ErrorData> {
        let projects = self.base.manager.list().await;

        if projects.is_empty() {
            return Ok(BaseRustTool::ok_markdown(
                "# Loaded Projects\n\n\
                 _No projects loaded._\n\n\
                 Use `load_rust_project` to load a crate or workspace.\n"
                    .to_string(),
            ));
        }

        let mut md = String::new();
        md.push_str(&format!("# Loaded Projects ({})\n\n", projects.len()));
        md.push_str("| Name | Alias | Kind | Path |\n");
        md.push_str("|------|-------|------|------|\n");
        for p in &projects {
            let alias = p.alias.as_deref().unwrap_or("-");
            let kind = match p.kind {
                crate::model::ProjectKind::SingleCrate => "crate",
                crate::model::ProjectKind::Workspace => "workspace",
            };
            let path = p.path.display();
            md.push_str(&format!(
                "| `{}` | `{}` | {} | `{}` |\n",
                p.name, alias, kind, path
            ));
        }
        if projects.iter().any(|p| !p.member_crates.is_empty()) {
            md.push_str("\n## Workspace Members\n\n");
            for p in &projects {
                if p.member_crates.is_empty() {
                    continue;
                }
                md.push_str(&format!("- `{}`: {}\n", p.name, p.member_crates.join(", ")));
            }
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
