//! `get_file_content` — contenido raw de un archivo del proyecto.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct GetFileContentTool {
    base: BaseRustTool,
}

impl GetFileContentTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for GetFileContentTool {
    fn name(&self) -> &'static str {
        "get_file_content"
    }

    fn description(&self) -> &'static str {
        "Returns the raw content of a file inside a loaded project. \
         `rel_path` is relative to the project root and must not escape it. \
         Output is wrapped in a code block with the appropriate language."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "rel_path": {
                    "type": "string",
                    "description": "Path relative to the project root"
                },
                "max_lines": {
                    "type": "integer",
                    "description": "Optional limit on number of lines returned (default: 2000)"
                }
            },
            "required": ["name", "rel_path"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let rel_path = BaseRustTool::arg(&args, "rel_path")?;
        let max_lines = args
            .get("max_lines")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(2000);
        let entry = self.base.find_entry(&name).await?;
        let content = RustAnalyzerUtils::file_content(&entry, &rel_path)
            .map_err(|e| ErrorData::internal_error(format!("{:?}", e), None))?;
        let truncated = content
            .lines()
            .take(max_lines)
            .collect::<Vec<_>>()
            .join("\n");
        let more = if content.lines().count() > max_lines {
            format!(
                "\n\n_… truncated at {} lines (file has {})._",
                max_lines,
                content.lines().count()
            )
        } else {
            String::new()
        };
        let lang = std::path::Path::new(&rel_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let lang = match lang {
            "rs" => "rust",
            "toml" => "toml",
            "md" => "markdown",
            "yaml" | "yml" => "yaml",
            "json" => "json",
            "sh" | "bash" => "bash",
            other => other,
        };
        let mut md = String::new();
        md.push_str(&format!("# `{}`\n\n", rel_path));
        md.push_str(&crate::tools::render::code_block(&truncated, lang));
        md.push_str(&more);
        Ok(BaseRustTool::ok_markdown(md))
    }
}
