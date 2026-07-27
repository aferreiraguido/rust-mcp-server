//! `inspect_build_config` — shows the project's `Cargo.toml`.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct InspectBuildConfigTool {
    base: BaseRustTool,
}

impl InspectBuildConfigTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for InspectBuildConfigTool {
    fn name(&self) -> &'static str {
        "inspect_build_config"
    }

    fn description(&self) -> &'static str {
        "Returns the raw `Cargo.toml` content of a loaded project. \
         Use this when you need to see the full build configuration."
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
        let cargo_toml = entry.path.join("Cargo.toml");
        let content = std::fs::read_to_string(&cargo_toml).map_err(|e| {
            ErrorData::internal_error(format!("Cannot read Cargo.toml: {}", e), None)
        })?;
        let mut md = String::new();
        md.push_str(&format!("# `Cargo.toml` — `{}`\n\n", entry.name));
        md.push_str(&crate::tools::render::code_block(&content, "toml"));
        // Also show the parsed summary
        if let Ok(manifest) = RustAnalyzerUtils::cargo_manifest(&entry) {
            md.push_str("\n## Parsed\n\n");
            md.push_str(&format!(
                "- Crate: `{}` v{}\n",
                manifest.crate_name().unwrap_or("?"),
                manifest.crate_version().unwrap_or("?")
            ));
            let deps = manifest.direct_dependencies();
            if !deps.is_empty() {
                md.push_str(&format!("- Direct dependencies: {}\n", deps.len()));
            }
            let features = manifest.features();
            if !features.is_empty() {
                md.push_str(&format!("- Features: {}\n", features.len()));
            }
            let members = manifest.workspace_members();
            if !members.is_empty() {
                md.push_str(&format!("- Workspace members: {}\n", members.len()));
            }
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
