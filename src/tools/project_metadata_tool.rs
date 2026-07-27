//! `project_metadata` — metadata of the loaded project.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ProjectMetadataTool {
    base: BaseRustTool,
}

impl ProjectMetadataTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ProjectMetadataTool {
    fn name(&self) -> &'static str {
        "project_metadata"
    }

    fn description(&self) -> &'static str {
        "Returns metadata for a loaded project: name, alias, kind, path, \
         package name/version/edition, member crates (if workspace)."
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
        md.push_str(&format!("# Project: `{}`\n\n", entry.name));
        md.push_str("| Field | Value |\n|---|---|\n");
        md.push_str(&format!("| **Name** | `{}` |\n", entry.name));
        md.push_str(&format!(
            "| **Alias** | `{}` |\n",
            entry.alias.as_deref().unwrap_or("-")
        ));
        md.push_str(&format!(
            "| **Kind** | {} |\n",
            match entry.kind {
                crate::model::ProjectKind::SingleCrate => "crate",
                crate::model::ProjectKind::Workspace => "workspace",
            }
        ));
        md.push_str(&format!("| **Path** | `{}` |\n", entry.path.display()));
        // (We don't include `loaded_at` — it is not deterministic for golden snapshots.)

        // Package info from Cargo.toml (optional).
        if let Ok(manifest) = RustAnalyzerUtils::cargo_manifest(&entry) {
            md.push_str("\n## Package\n\n");
            md.push_str("| Field | Value |\n|---|---|\n");
            if let Some(name) = manifest.crate_name() {
                md.push_str(&format!("| **Crate name** | `{}` |\n", name));
            }
            if let Some(version) = manifest.crate_version() {
                md.push_str(&format!("| **Version** | `{}` |\n", version));
            }
            if let Some(edition) = manifest.package.as_ref().and_then(|p| p.edition.as_ref()) {
                md.push_str(&format!("| **Edition** | `{}` |\n", edition));
            }
            if let Some(rust) = manifest
                .package
                .as_ref()
                .and_then(|p| p.rust_version.as_ref())
            {
                md.push_str(&format!("| **MSRV** | `{}` |\n", rust));
            }
            if let Some(license) = manifest.package.as_ref().and_then(|p| p.license.as_ref()) {
                md.push_str(&format!("| **License** | `{}` |\n", license));
            }
            if let Some(repo) = manifest
                .package
                .as_ref()
                .and_then(|p| p.repository.as_ref())
            {
                md.push_str(&format!("| **Repository** | `{}` |\n", repo));
            }
        }

        if !entry.member_crates.is_empty() {
            md.push_str("\n## Workspace Members\n\n");
            for m in &entry.member_crates {
                md.push_str(&format!("- `{}`\n", m));
            }
        }

        Ok(BaseRustTool::ok_markdown(md))
    }
}
