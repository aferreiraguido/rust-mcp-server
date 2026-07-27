//! `inspect_struct` — details of a struct.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct InspectStructTool {
    base: BaseRustTool,
}

impl InspectStructTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for InspectStructTool {
    fn name(&self) -> &'static str {
        "inspect_struct"
    }

    fn description(&self) -> &'static str {
        "Returns detailed info about a named struct: file, kind, derives, \
         doc, and a table of fields with name, type, visibility."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "struct_name": {
                    "type": "string",
                    "description": "Struct name to inspect"
                }
            },
            "required": ["name", "struct_name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let struct_name = BaseRustTool::arg(&args, "struct_name")?;
        let entry = self.base.find_entry(&name).await?;
        match RustAnalyzerUtils::inspect_struct(&entry, &struct_name)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?
        {
            Some(s) => {
                let mut md = String::new();
                md.push_str(&format!("# `{}` ({})\n\n", s.name, s.kind));
                md.push_str(&format!("- **File**: `{}`\n", s.file));
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
                if !s.fields.is_empty() {
                    md.push_str("\n## Fields\n\n");
                    let headers = vec!["#", "Name", "Type", "Visibility"];
                    let rows: Vec<Vec<String>> = s
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            vec![
                                i.to_string(),
                                format!("`{}`", f.name),
                                format!("`{}`", f.ty),
                                if f.is_pub {
                                    "`pub`".to_string()
                                } else {
                                    "-".to_string()
                                },
                            ]
                        })
                        .collect();
                    md.push_str(&crate::tools::render::table(&headers, &rows));
                }
                Ok(BaseRustTool::ok_markdown(md))
            }
            None => Ok(BaseRustTool::ok_error(format!(
                "Struct `{}` not found in `{}`",
                struct_name, entry.name
            ))),
        }
    }
}
