//! `find_implementations` — uses `Analysis::goto_implementation`.
//!
//! Given a name (typically a trait), returns all
//! implementations (`impl X for Y`).

use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use crate::tools::ra_helpers;
use async_trait::async_trait;
use ra_ap_ide::{FilePosition, GotoImplementationConfig};
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct FindImplementationsTool {
    base: BaseRustTool,
}

impl FindImplementationsTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for FindImplementationsTool {
    fn name(&self) -> &'static str {
        "find_implementations"
    }

    fn description(&self) -> &'static str {
        "Returns the implementation blocks (`impl X for Y`) for a given \
         trait or type name, using rust-analyzer's semantic analysis. \
         Example: `find_implementations(MyTrait)` returns all `impl MyTrait for ...`."
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
            Some(r) => r,
            None => {
                return Ok(BaseRustTool::ok_error(format!(
                    "No implementations found for `{}`",
                    symbol
                )));
            }
        };

        let md = ra_helpers::format_targets(
            &format!("Implementations of `{}`", symbol),
            &result.info,
            &host.files,
        );
        Ok(BaseRustTool::ok_markdown(md))
    }
}
