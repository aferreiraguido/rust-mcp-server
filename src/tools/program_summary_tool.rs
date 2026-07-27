//! `program_summary` — high-level counts of a project.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ProgramSummaryTool {
    base: BaseRustTool,
}

impl ProgramSummaryTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ProgramSummaryTool {
    fn name(&self) -> &'static str {
        "program_summary"
    }

    fn description(&self) -> &'static str {
        "Executive summary of a loaded project: crate name+version+edition, \
         file/function/struct/enum/trait/impl/module/macro counts, total LOC, \
         direct dependency count, feature count. Use this for a quick overview."
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
        let s = RustAnalyzerUtils::program_summary(&entry)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;

        let mut md = String::new();
        md.push_str(&format!("# Program Summary: `{}`\n\n", entry.name));

        md.push_str("## Crate\n\n");
        md.push_str(&format!(
            "- **Name**: `{}`\n",
            s.crate_name.as_deref().unwrap_or("?")
        ));
        md.push_str(&format!(
            "- **Version**: `{}`\n",
            s.crate_version.as_deref().unwrap_or("?")
        ));
        md.push_str(&format!(
            "- **Edition**: `{}`\n",
            s.edition.as_deref().unwrap_or("?")
        ));

        md.push_str("\n## Counts\n\n");
        let headers = vec!["Item", "Count"];
        let rows = vec![
            vec!["Files (`.rs`)".to_string(), s.file_count.to_string()],
            vec!["Functions".to_string(), s.function_count.to_string()],
            vec!["Structs".to_string(), s.struct_count.to_string()],
            vec!["Enums".to_string(), s.enum_count.to_string()],
            vec!["Traits".to_string(), s.trait_count.to_string()],
            vec!["Impls".to_string(), s.impl_count.to_string()],
            vec!["Modules".to_string(), s.module_count.to_string()],
            vec!["Macros".to_string(), s.macro_count.to_string()],
            vec!["Total LOC".to_string(), s.total_lines.to_string()],
            vec![
                "Direct deps".to_string(),
                s.direct_dependency_count.to_string(),
            ],
            vec!["Features".to_string(), s.feature_count.to_string()],
        ];
        md.push_str(&crate::tools::render::table(&headers, &rows));
        Ok(BaseRustTool::ok_markdown(md))
    }
}
