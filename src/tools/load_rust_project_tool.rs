//! `load_rust_project` — loads a crate/workspace into the manager.
//!
//! Acepta path local, URL Git (`https://...git` o `git@...`), o URL archive
//! (`.zip`, `.tar.gz`, `.tar.zst`, `.tar.xz`).

use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct LoadRustProjectTool {
    base: BaseRustTool,
}

impl LoadRustProjectTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for LoadRustProjectTool {
    fn name(&self) -> &'static str {
        "load_rust_project"
    }

    fn description(&self) -> &'static str {
        "Loads a Rust project into the manager. Source can be a local path, \
         a Git URL (https://github.com/foo/bar or git@github.com:foo/bar), \
         or an archive URL (.zip, .tar.gz, .tar.zst, .tar.xz). The project \
         is cached for 10 minutes (configurable via expiry_minutes)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Path local, Git URL, or archive URL"
                },
                "alias": {
                    "type": "string",
                    "description": "Optional alias to use as the effective name"
                },
                "expiry_minutes": {
                    "type": "integer",
                    "description": "Override the 10-minute default expiration"
                }
            },
            "required": ["source"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let source = BaseRustTool::arg(&args, "source")?;
        let alias = BaseRustTool::opt_arg(&args, "alias");
        let expiry = args.get("expiry_minutes").and_then(|v| v.as_i64());

        match self
            .base
            .manager
            .load(&source, alias.as_deref(), expiry)
            .await
        {
            Ok(entry) => {
                let mut md = String::new();
                md.push_str("# Project Loaded\n\n");
                md.push_str(&format!("- **Name**: `{}`\n", entry.name));
                md.push_str(&format!(
                    "- **Alias**: {}\n",
                    entry.alias.as_deref().unwrap_or("-")
                ));
                md.push_str(&format!("- **Path**: `{}`\n", entry.path.display()));
                md.push_str(&format!(
                    "- **Kind**: {}\n",
                    match entry.kind {
                        crate::model::ProjectKind::SingleCrate => "crate",
                        crate::model::ProjectKind::Workspace => "workspace",
                    }
                ));
                if !entry.member_crates.is_empty() {
                    md.push_str(&format!(
                        "- **Members**: {}\n",
                        entry.member_crates.join(", ")
                    ));
                }
                Ok(BaseRustTool::ok_markdown(md))
            }
            Err(e) => Ok(BaseRustTool::ok_error(format!(
                "[{:?}] {}",
                e.code, e.message
            ))),
        }
    }
}
