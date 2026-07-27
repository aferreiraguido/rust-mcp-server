//! `validate_code_reference` — does this symbol exist in the project?

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ValidateCodeReferenceTool {
    base: BaseRustTool,
}

impl ValidateCodeReferenceTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ValidateCodeReferenceTool {
    fn name(&self) -> &'static str {
        "validate_code_reference"
    }

    fn description(&self) -> &'static str {
        "Returns EXISTS / NOT_FOUND for a top-level symbol name in a \
         loaded project. Symbols checked: functions, structs, enums, \
         traits, modules, consts, statics, type aliases. Does NOT match \
         methods (those live inside impl blocks)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "symbol": {
                    "type": "string",
                    "description": "Symbol name to look up"
                }
            },
            "required": ["name", "symbol"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let symbol = BaseRustTool::arg(&args, "symbol")?;
        let entry = self.base.find_entry(&name).await?;
        let exists = RustAnalyzerUtils::symbol_exists(&entry, &symbol)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let status = if exists { "EXISTS" } else { "NOT_FOUND" };
        let md = format!("# Validate: `{}`\n\n**Result**: `{}`\n", symbol, status);
        Ok(BaseRustTool::ok_markdown(md))
    }
}
