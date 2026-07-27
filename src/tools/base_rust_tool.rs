//! `BaseRustTool` — shared helpers for all tools.
//!
//! Mirror of `com.softtek.mcp.tools.BaseCobolTool` / `BaseJavaTool`.
//! Static methods for argument extraction, Markdown generation,
//! and project lookup. Not a trait — it is a struct with utilities
//! accessible via `self.base.arg(...)` from each tool.

use crate::model::ProjectEntry;
use crate::project_manager::ProjectManager;
use crate::tools::mcp_tool::McpTool;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct BaseRustTool {
    pub manager: Arc<ProjectManager>,
}

impl BaseRustTool {
    pub fn new(manager: Arc<ProjectManager>) -> Self {
        Self { manager }
    }

    /// Extracts a string argument. Returns MCP error if missing.
    pub fn arg(args: &Value, key: &str) -> Result<String, ErrorData> {
        args.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                ErrorData::invalid_params(
                    format!("Missing required argument: '{}'", key),
                    Some(serde_json::json!({ "missing": key })),
                )
            })
    }

    /// Extracts an optional string argument.
    pub fn opt_arg(args: &Value, key: &str) -> Option<String> {
        args.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Extracts a bool argument. Defaults to `false` if missing.
    pub fn bool_arg(args: &Value, key: &str) -> bool {
        args.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    /// Looks up a loaded project. Returns MCP error if not found.
    pub async fn find_entry(&self, name: &str) -> Result<ProjectEntry, ErrorData> {
        self.manager
            .find(name)
            .await
            .ok_or_else(|| self.not_found(name))
    }

    pub fn not_found(&self, name: &str) -> ErrorData {
        ErrorData::invalid_params(
            format!(
                "Project '{}' not found (or expired). Load it first with load_rust_project.",
                name
            ),
            Some(serde_json::json!({ "name": name })),
        )
    }

    /// Packages Markdown as a successful `CallToolResult`.
    pub fn ok_markdown(markdown: String) -> CallToolResult {
        CallToolResult::success(vec![ContentBlock::text(markdown)])
    }

    /// Packages a readable error for the LLM as a successful `CallToolResult`
    /// (semantically "the tool returned an input error", not a crash).
    /// Keeps the API shape predictable.
    pub fn ok_error(message: String) -> CallToolResult {
        let body = format!("# Error\n\n```\n{}\n```\n", message);
        CallToolResult::success(vec![ContentBlock::text(body)])
    }
}

/// Helper for tools that do NOT need a manager (few)
/// to implement McpTool quickly.
#[async_trait]
impl<T: McpTool> McpTool for Arc<T> {
    fn name(&self) -> &'static str {
        (**self).name()
    }
    fn description(&self) -> &'static str {
        (**self).description()
    }
    fn input_schema(&self) -> Value {
        (**self).input_schema()
    }
    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> {
        (**self).call(args).await
    }
}
