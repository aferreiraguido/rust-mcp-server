//! `inspect_function` — details of a function by name.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct InspectFunctionTool {
    base: BaseRustTool,
}

impl InspectFunctionTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for InspectFunctionTool {
    fn name(&self) -> &'static str {
        "inspect_function"
    }

    fn description(&self) -> &'static str {
        "Returns detailed info about a named function in a loaded project: \
         signature, file, visibility, doc, and the full body source."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "function_name": {
                    "type": "string",
                    "description": "Function name to inspect"
                }
            },
            "required": ["name", "function_name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let fn_name = BaseRustTool::arg(&args, "function_name")?;
        let entry = self.base.find_entry(&name).await?;
        match RustAnalyzerUtils::inspect_function(&entry, &fn_name)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?
        {
            Some((info, body)) => {
                let mut md = String::new();
                md.push_str(&format!("# `{}`\n\n", info.name));
                md.push_str("| Field | Value |\n|---|---|\n");
                md.push_str(&format!("| **File** | `{}` |\n", info.file));
                md.push_str(&format!("| **Visibility** | `{}` |\n", info.visibility));
                let mut flags = Vec::new();
                if info.is_async {
                    flags.push("async");
                }
                if info.is_const {
                    flags.push("const");
                }
                if info.is_unsafe {
                    flags.push("unsafe");
                }
                if !flags.is_empty() {
                    md.push_str(&format!("| **Modifiers** | {} |\n", flags.join(", ")));
                }
                md.push_str(&format!("| **Signature** | `{}` |\n", info.signature));
                if let Some(doc) = &info.doc_comment {
                    md.push_str(&format!(
                        "| **Doc** | {} |\n",
                        crate::tools::render::truncate(doc, 200)
                    ));
                }
                if !body.is_empty() {
                    md.push_str("\n## Body\n\n");
                    md.push_str(&crate::tools::render::code_block(&body, "rust"));
                }
                Ok(BaseRustTool::ok_markdown(md))
            }
            None => Ok(BaseRustTool::ok_error(format!(
                "Function `{}` not found in `{}`",
                fn_name, entry.name
            ))),
        }
    }
}
