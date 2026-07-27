//! `list_cargo_features` — features declared in [features].

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListCargoFeaturesTool {
    base: BaseRustTool,
}

impl ListCargoFeaturesTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListCargoFeaturesTool {
    fn name(&self) -> &'static str {
        "list_cargo_features"
    }

    fn description(&self) -> &'static str {
        "Lists the features declared in [features] of the root Cargo.toml, \
         and for each one, the dependencies or sub-features it activates."
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
        let manifest = RustAnalyzerUtils::cargo_manifest(&entry)
            .map_err(|e| ErrorData::internal_error(format!("manifest parse: {:?}", e), None))?;
        let features = manifest.features();

        let mut md = String::new();
        md.push_str(&format!(
            "# Features of `{}` ({})\n\n",
            entry.name,
            features.len()
        ));
        if features.is_empty() {
            md.push_str("_No features declared._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        let headers = vec!["Feature", "Activates"];
        let rows: Vec<Vec<String>> = features
            .iter()
            .map(|(name, deps)| {
                vec![
                    format!("`{}`", name),
                    if deps.is_empty() {
                        "-".to_string()
                    } else {
                        deps.iter()
                            .map(|d| format!("`{}`", d))
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                ]
            })
            .collect();
        md.push_str(&crate::tools::render::table(&headers, &rows));
        Ok(BaseRustTool::ok_markdown(md))
    }
}
