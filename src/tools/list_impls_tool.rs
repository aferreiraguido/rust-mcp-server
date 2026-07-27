//! `list_impls` — `impl` blocks (with or without trait).

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListImplsTool {
    base: BaseRustTool,
}

impl ListImplsTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListImplsTool {
    fn name(&self) -> &'static str {
        "list_impls"
    }

    fn description(&self) -> &'static str {
        "Lists all `impl` blocks in a loaded project: target type, optional \
         trait, file, methods provided. Use this to map out which types \
         implement which traits (note: without `ra_ap_*` we don't resolve \
         trait bounds or associated types — iter 3 will)."
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
        let items = RustAnalyzerUtils::list_impls(&entry)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let mut md = String::new();
        md.push_str(&format!("# Impl blocks ({})\n\n", items.len()));
        if items.is_empty() {
            md.push_str("_No impl blocks._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        let headers = vec!["Self type", "Trait", "File", "Methods"];
        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|i| {
                vec![
                    format!("`{}`", i.self_type),
                    i.trait_name
                        .as_deref()
                        .map(|t| format!("`{}`", t))
                        .unwrap_or_else(|| "-".to_string()),
                    i.file.clone(),
                    i.methods.len().to_string(),
                ]
            })
            .collect();
        md.push_str(&crate::tools::render::table(&headers, &rows));
        Ok(BaseRustTool::ok_markdown(md))
    }
}
