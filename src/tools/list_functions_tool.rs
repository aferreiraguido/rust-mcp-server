//! `list_functions` — all functions in the project.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListFunctionsTool {
    base: BaseRustTool,
}

impl ListFunctionsTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListFunctionsTool {
    fn name(&self) -> &'static str {
        "list_functions"
    }

    fn description(&self) -> &'static str {
        "Lists all functions in a loaded project (including associated \
         functions and methods in impl blocks). For each one: signature, \
         file, async/unsafe/const, visibility, doc comment."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "visibility": {
                    "type": "string",
                    "enum": ["all", "pub", "non-pub"],
                    "description": "Filter by visibility (default: all)"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let vis = BaseRustTool::opt_arg(&args, "visibility").unwrap_or_else(|| "all".to_string());
        let entry = self.base.find_entry(&name).await?;
        let fns = RustAnalyzerUtils::list_functions(&entry)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let filtered: Vec<_> = match vis.as_str() {
            "pub" => fns.into_iter().filter(|f| f.is_pub).collect(),
            "non-pub" => fns.into_iter().filter(|f| !f.is_pub).collect(),
            _ => fns,
        };
        let mut md = String::new();
        md.push_str(&format!("# Functions ({})\n\n", filtered.len()));
        if filtered.is_empty() {
            md.push_str("_No functions._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        for f in &filtered {
            let mut flags = Vec::new();
            if f.is_pub {
                flags.push("pub");
            }
            if f.is_async {
                flags.push("async");
            }
            if f.is_unsafe {
                flags.push("unsafe");
            }
            if f.is_const {
                flags.push("const");
            }
            let flags_str = if flags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", flags.join(","))
            };
            md.push_str(&format!(
                "### `{}`{}\n\n- **File**: `{}`\n- **Signature**: `{}`\n",
                f.name, flags_str, f.file, f.signature
            ));
            if let Some(doc) = &f.doc_comment {
                md.push_str(&format!(
                    "- **Doc**: {}\n",
                    crate::tools::render::truncate(doc, 200)
                ));
            }
        }
        Ok(BaseRustTool::ok_markdown(md))
    }
}
