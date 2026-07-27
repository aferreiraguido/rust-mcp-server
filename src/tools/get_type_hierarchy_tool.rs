//! `get_type_hierarchy` — combines `goto_implementation` and `goto_type_definition`.
//!
//! For a trait: returns the types that implement it (via impl blocks).
//! For a type: returns its trait bounds + the types that use it
//! (via `goto_implementation` from its type declaration).
//!
//! It is a semantic "best effort": uses the two available queries and
//! combines the results.

use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use crate::tools::ra_helpers;
use async_trait::async_trait;
use ra_ap_ide::{FilePosition, GotoImplementationConfig};
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct GetTypeHierarchyTool {
    base: BaseRustTool,
}

impl GetTypeHierarchyTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for GetTypeHierarchyTool {
    fn name(&self) -> &'static str {
        "get_type_hierarchy"
    }

    fn description(&self) -> &'static str {
        "Returns type hierarchy information for a given symbol using \
         rust-analyzer's semantic queries. For a trait name, lists all \
         types that implement it. For a type name, returns the impl \
         blocks. NOTE: rust-analyzer doesn't expose a single 'type \
         hierarchy' query — this tool uses `goto_implementation` as \
         the closest equivalent."
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
                    "description": "Trait or type name"
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

        let entry_path = entry.path.clone();
        let (host, resolver) =
            tokio::task::spawn_blocking(move || ra_helpers::build_resolver_for_entry(&entry_path))
                .await
                .map_err(|e| ErrorData::internal_error(format!("join: {}", e), None))?
                .map_err(|e| ErrorData::internal_error(format!("load: {}", e), None))?;

        let loc = match resolver.resolve(&symbol) {
            Some(l) => l.clone(),
            None => {
                return Ok(BaseRustTool::ok_error(format!(
                    "Symbol `{}` not found",
                    symbol
                )));
            }
        };

        let position = FilePosition {
            file_id: loc.file_id,
            offset: loc.name_range.start(),
        };
        let config = GotoImplementationConfig {
            filter_adjacent_derive_implementations: true,
        };
        let analysis = host.analysis();
        let result = analysis
            .goto_implementation(&config, position)
            .ok()
            .flatten();
        let result = match result {
            Some(r) if !r.info.is_empty() => r,
            _ => {
                return Ok(BaseRustTool::ok_error(format!(
                    "No type hierarchy info found for `{}` (might not be a trait/type, \
                     or no impls in this project)",
                    symbol
                )));
            }
        };

        let md = ra_helpers::format_targets(
            &format!("Type hierarchy for `{}`", symbol),
            &result.info,
            &host.files,
        );
        Ok(BaseRustTool::ok_markdown(md))
    }
}
