//! `function_call_graph` — uses `Analysis::find_all_refs` to find
//! the callers of a function.
//!
//! For now returns only CALLERS (references that are not the
//! definition). CALLEE analysis requires iter 4 with HIR
//! (walking the function body and finding all call exprs).

use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use crate::tools::ra_helpers;
use async_trait::async_trait;
use ra_ap_ide::{FilePosition, FindAllRefsConfig, RaFixtureConfig};
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct FunctionCallGraphTool {
    base: BaseRustTool,
}

impl FunctionCallGraphTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for FunctionCallGraphTool {
    fn name(&self) -> &'static str {
        "function_call_graph"
    }

    fn description(&self) -> &'static str {
        "Returns the call graph ENTRY POINTS (callers) of a named function, \
         using rust-analyzer's semantic `find_all_refs`. For callees, the \
         HIR-based body analysis arrives in iter 4 — for now this tool \
         gives the inbound side of the call graph only. NOTE: results are \
         filtered to skip the function's own definition site."
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
                    "description": "Function name to look up callers for"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Limit on the number of results (default: 100)"
                }
            },
            "required": ["name", "symbol"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let symbol = BaseRustTool::arg(&args, "symbol")?;
        let max = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(100);
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
        let config = FindAllRefsConfig {
            search_scope: None,
            ra_fixture: RaFixtureConfig::<'_>::default(),
            exclude_imports: true,
            exclude_tests: false,
        };
        let analysis = host.analysis();
        let result = analysis.find_all_refs(position, &config).ok().flatten();
        let results = match result {
            Some(r) => r,
            None => {
                return Ok(BaseRustTool::ok_error(format!(
                    "No references found for `{}`",
                    symbol
                )));
            }
        };

        // `find_all_refs` can return multiple `ReferenceSearchResult`s
        // (one per declaration site of the symbol). Iterate all and
        // extract their refs.
        let mut refs: Vec<(ra_ap_ide::FileId, ra_ap_syntax::TextRange)> = Vec::new();
        for result in &results {
            for (file_id, ranges) in &result.references {
                for (range, _category) in ranges {
                    refs.push((*file_id, *range));
                }
            }
        }
        // Filter out the symbol's own definition.
        refs.retain(|(fid, range)| {
            !(*fid == loc.file_id
                && range.start() >= loc.name_range.start()
                && range.end() <= loc.full_range.end())
        });
        refs.sort_by_key(|(fid, range)| (fid.index(), range.start()));
        refs.truncate(max);

        let md = ra_helpers::format_references(
            &format!("Callers of `{}` (semantic, via ra_ap_*)", symbol),
            &refs,
            &host.files,
        );
        Ok(BaseRustTool::ok_markdown(md))
    }
}
