//! `struct_field_xref` — uses `Analysis::find_all_refs` to find
//! all references to a struct field.
//!
//! The user gives the field name. Returns all refs in the
//! project (may include refs to other structs that have a field
//! with the same name). To filter by specific struct, see
//! iter 4.

use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use crate::tools::ra_helpers;
use async_trait::async_trait;
use ra_ap_ide::{FilePosition, FindAllRefsConfig, RaFixtureConfig};
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct StructFieldXrefTool {
    base: BaseRustTool,
}

impl StructFieldXrefTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for StructFieldXrefTool {
    fn name(&self) -> &'static str {
        "struct_field_xref"
    }

    fn description(&self) -> &'static str {
        "Returns all references to a struct field by name, using \
         rust-analyzer's semantic `find_all_refs`. If multiple structs \
         in the project have a field with the same name, results may \
         include references to any of them. For struct-filtered results, \
         pass both `struct_name` and `field_name` in iter 4."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name or alias"
                },
                "field": {
                    "type": "string",
                    "description": "Field name to look up references for"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Limit on the number of results (default: 100)"
                }
            },
            "required": ["name", "field"],
            "additionalProperties": false
        })
    }

    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        let name = BaseRustTool::arg(&args, "name")?;
        let field = BaseRustTool::arg(&args, "field")?;
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

        let loc = match resolver.resolve(&field) {
            Some(l) => l.clone(),
            None => {
                return Ok(BaseRustTool::ok_error(format!(
                    "Field `{}` not found",
                    field
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
                    field
                )));
            }
        };

        let mut refs: Vec<(ra_ap_ide::FileId, ra_ap_syntax::TextRange)> = Vec::new();
        for result in &results {
            for (file_id, ranges) in &result.references {
                for (range, _cat) in ranges {
                    refs.push((*file_id, *range));
                }
            }
        }
        refs.retain(|(fid, range)| {
            !(*fid == loc.file_id
                && range.start() >= loc.name_range.start()
                && range.end() <= loc.full_range.end())
        });
        refs.sort_by_key(|(fid, range)| (fid.index(), range.start()));
        refs.truncate(max);

        let md = ra_helpers::format_references(
            &format!("References to field `{}` (semantic)", field),
            &refs,
            &host.files,
        );
        Ok(BaseRustTool::ok_markdown(md))
    }
}
