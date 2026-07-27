//! `list_dependencies` — direct dependencies from Cargo.toml.

use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ListDependenciesTool {
    base: BaseRustTool,
}

impl ListDependenciesTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ListDependenciesTool {
    fn name(&self) -> &'static str {
        "list_dependencies"
    }

    fn description(&self) -> &'static str {
        "Lists direct dependencies declared in [dependencies], \
         [dev-dependencies] and [build-dependencies] of the root Cargo.toml. \
         Only the static manifest is read — transitive deps are not computed \
         (use `cargo tree` for that)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "scope": {
                    "type": "string",
                    "enum": ["normal", "dev", "build", "all"],
                    "description": "Filter by scope (default: all)"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let scope = BaseRustTool::opt_arg(&args, "scope").unwrap_or_else(|| "all".to_string());
        let entry = self.base.find_entry(&name).await?;
        let manifest = RustAnalyzerUtils::cargo_manifest(&entry)
            .map_err(|e| ErrorData::internal_error(format!("manifest parse: {:?}", e), None))?;
        let deps = manifest.direct_dependencies();
        let filtered: Vec<_> = if scope == "all" {
            deps
        } else {
            deps.iter()
                .filter(|(_, s, _)| *s == scope)
                .cloned()
                .collect()
        };
        let mut md = String::new();
        md.push_str(&format!(
            "# Dependencies of `{}` ({})\n\n",
            entry.name,
            filtered.len()
        ));
        if filtered.is_empty() {
            md.push_str("_No dependencies._\n");
            return Ok(BaseRustTool::ok_markdown(md));
        }
        let headers = vec!["Name", "Scope", "Version", "Features", "Source"];
        let mut rows: Vec<Vec<String>> = Vec::new();
        for (name, scope, value) in &filtered {
            let mut version = String::new();
            let mut features: Vec<String> = Vec::new();
            let mut source = String::new();
            match value {
                toml::Value::String(s) => version = s.clone(),
                toml::Value::Table(t) => {
                    if let Some(toml::Value::String(v)) = t.get("version") {
                        version = v.clone();
                    }
                    if let Some(toml::Value::String(p)) = t.get("path") {
                        source = format!("path: {}", p);
                    } else if let Some(toml::Value::String(g)) = t.get("git") {
                        source = format!("git: {}", g);
                    }
                    if let Some(toml::Value::Array(arr)) = t.get("features") {
                        features = arr
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                    }
                    if let Some(toml::Value::Boolean(true)) = t.get("optional") {
                        source = format!("{} (optional)", source);
                    }
                }
                _ => {}
            }
            rows.push(vec![
                format!("`{}`", name),
                scope.to_string(),
                if version.is_empty() {
                    "-".to_string()
                } else {
                    format!("`{}`", version)
                },
                if features.is_empty() {
                    "-".to_string()
                } else {
                    features.join(", ")
                },
                if source.is_empty() {
                    "-".to_string()
                } else {
                    source
                },
            ]);
        }
        md.push_str(&crate::tools::render::table(&headers, &rows));
        Ok(BaseRustTool::ok_markdown(md))
    }
}
