//! `inspect_enum` — details of an enum.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct InspectEnumTool {
    base: BaseRustTool,
}

impl InspectEnumTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for InspectEnumTool {
    fn name(&self) -> &'static str {
        "inspect_enum"
    }

    fn description(&self) -> &'static str {
        "Returns detailed info about a named enum: file, variants (with \
         their fields and visibility), derives, doc."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "enum_name": {
                    "type": "string",
                    "description": "Enum name to inspect"
                }
            },
            "required": ["name", "enum_name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let enum_name = BaseRustTool::arg(&args, "enum_name")?;
        let entry = self.base.find_entry(&name).await?;
        match RustAnalyzerUtils::inspect_enum(&entry, &enum_name)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?
        {
            Some(e) => {
                let mut md = String::new();
                md.push_str(&format!("# `{}`\n\n", e.name));
                md.push_str(&format!("- **File**: `{}`\n", e.file));
                if !e.derives.is_empty() {
                    md.push_str(&format!(
                        "- **Derives**: {}\n",
                        e.derives
                            .iter()
                            .map(|d| format!("`{}`", d))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if let Some(doc) = &e.doc_comment {
                    md.push_str(&format!(
                        "- **Doc**: {}\n",
                        crate::tools::render::truncate(doc, 200)
                    ));
                }
                md.push_str(&format!("\n## Variants ({})\n\n", e.variants.len()));
                for v in &e.variants {
                    if v.fields.is_empty() {
                        md.push_str(&format!("- `{}`\n", v.name));
                    } else {
                        md.push_str(&format!("- `{}`(", v.name));
                        let field_strs: Vec<String> = v
                            .fields
                            .iter()
                            .map(|f| format!("{}: {}", f.name, f.ty))
                            .collect();
                        md.push_str(&field_strs.join(", "));
                        md.push_str(")\n");
                    }
                }
                Ok(BaseRustTool::ok_markdown(md))
            }
            None => Ok(BaseRustTool::ok_error(format!(
                "Enum `{}` not found in `{}`",
                enum_name, entry.name
            ))),
        }
    }
}
