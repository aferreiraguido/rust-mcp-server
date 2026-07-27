//! `resolve_type` — uses `Analysis::goto_definition` to resolve a
//! type name (struct/enum/trait/type-alias) to its definition.
//!
//! **Status (iter 3.5)**: this tool works semantically. Accepts a
//! symbol name (any type that `SymbolResolver` recognizes:
//! functions, structs, enums, traits, consts, etc.) and returns the
//! location(s) of its definition.

use crate::project_manager::ProjectManager;
use crate::tools::base_rust_tool::BaseRustTool;
use crate::tools::mcp_tool::McpTool;
use crate::tools::ra_helpers;
use async_trait::async_trait;
use ra_ap_ide::{FilePosition, GotoDefinitionConfig, RaFixtureConfig};
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ResolveTypeTool {
    base: BaseRustTool,
}

impl ResolveTypeTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self {
            base: BaseRustTool::new(manager),
        }
    }
}

#[async_trait]
impl McpTool for ResolveTypeTool {
    fn name(&self) -> &'static str {
        "resolve_type"
    }

    fn description(&self) -> &'static str {
        "Resolves a symbol name to its definition site using the rust-analyzer \
         semantic engine. Works for functions, structs, enums, traits, consts, \
         statics, type aliases, and modules. Returns the file and range where \
         the symbol is defined. For ambiguous names, picks the highest-priority \
         match (function > struct > enum > trait > type alias > module > const > static)."
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
                    "description": "Symbol name to resolve"
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

        // Load the ASG and build the resolver. (Blocking → spawn_blocking.)
        let entry_path = entry.path.clone();
        let (host, resolver) = tokio::task::spawn_blocking(move || {
            ra_ap_helpers::build_resolver_for_entry_at(&entry_path)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("join: {}", e), None))?
        .map_err(|e| ErrorData::internal_error(format!("load: {}", e), None))?;

        // Resolve the symbol in the index
        let loc = match resolver.resolve(&symbol) {
            Some(l) => l.clone(),
            None => {
                return Ok(BaseRustTool::ok_error(format!(
                    "Symbol `{}` not found in `{}`",
                    symbol, entry.name
                )));
            }
        };

        // Build FilePosition and call goto_definition
        let position = FilePosition {
            file_id: loc.file_id,
            offset: loc.name_range.start(),
        };
        let config = GotoDefinitionConfig::<'_> {
            ra_fixture: RaFixtureConfig::<'_>::default(),
        };
        let analysis = host.analysis();
        let result = analysis.goto_definition(position, &config).ok().flatten();
        let result = match result {
            Some(r) => r,
            None => {
                return Ok(BaseRustTool::ok_error(format!(
                    "Could not resolve `{}` to a definition (maybe a primitive?)",
                    symbol
                )));
            }
        };

        let md = ra_helpers::format_targets(
            &format!("Definition of `{}`", symbol),
            &result.info,
            &host.files,
        );
        Ok(BaseRustTool::ok_markdown(md))
    }
}

// Helper that compiles with just the path — separated so that `spawn_blocking`
// does not have to capture `entry` which contains a non-Send `Arc<ProjectManager>`.
mod ra_ap_helpers {
    use crate::analysis::analysis_host::ProjectAnalysisHost;
    use crate::analysis::symbol_resolver::{ResolverError, SymbolResolver};
    use crate::model::{ProjectEntry, ProjectKind};
    use chrono::{Duration, Utc};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    pub fn build_resolver_for_entry_at(
        path: &Path,
    ) -> Result<(ProjectAnalysisHost, SymbolResolver), ResolverError> {
        let host = ProjectAnalysisHost::load(path)
            .map_err(|e| ResolverError::Analysis(format!("could not load ASG: {}", e)))?;
        let analysis = host.analysis();
        let resolver = SymbolResolver::build(&analysis, &host.files);
        Ok((host, resolver))
    }

    // Helper to build a ProjectEntry from a path (at runtime
    // it comes with one). Only used in tests.
    #[allow(dead_code)]
    pub fn _entry_at(path: PathBuf) -> ProjectEntry {
        ProjectEntry {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string(),
            alias: None,
            path,
            kind: ProjectKind::SingleCrate,
            expiry: Utc::now() + Duration::minutes(10),
            loaded_at: Utc::now(),
            member_crates: vec![],
            cached_manifest: std::sync::OnceLock::new(),
        }
    }

    // To avoid HashMap unused warning.
    #[allow(dead_code)]
    fn _hashmap_marker() -> HashMap<(), ()> {
        HashMap::new()
    }
}
