//! `inspect_trait` — details of a trait.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct InspectTraitTool {
    base: BaseRustTool,
}

impl InspectTraitTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for InspectTraitTool {
    fn name(&self) -> &'static str {
        "inspect_trait"
    }

    fn description(&self) -> &'static str {
        "Returns detailed info about a named trait: file, required items, doc."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "trait_name": {
                    "type": "string",
                    "description": "Trait name to inspect"
                }
            },
            "required": ["name", "trait_name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let trait_name = BaseRustTool::arg(&args, "trait_name")?;
        let entry = self.base.find_entry(&name).await?;
        match RustAnalyzerUtils::inspect_trait(&entry, &trait_name)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?
        {
            Some(t) => {
                let mut md = String::new();
                md.push_str(&format!("# `{}`\n\n", t.name));
                md.push_str(&format!("- **File**: `{}`\n", t.file));
                if let Some(doc) = &t.doc_comment {
                    md.push_str(&format!(
                        "- **Doc**: {}\n",
                        crate::tools::render::truncate(doc, 200)
                    ));
                }
                md.push_str(&format!("\n## Members ({})\n\n", t.methods.len()));
                for m in &t.methods {
                    md.push_str(&format!("- `{}`\n", m));
                }
                Ok(BaseRustTool::ok_markdown(md))
            }
            None => Ok(BaseRustTool::ok_error(format!(
                "Trait `{}` not found in `{}`",
                trait_name, entry.name
            ))),
        }
    }
}
