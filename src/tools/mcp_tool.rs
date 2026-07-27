//! `McpTool` — the trait that every tool implements.
//!
//! Mirror of `com.softtek.mcp.tools.McpTool` (java/cobol-mcp-server).
//! Verbose by design: one struct per tool, explicit registration in
//! `RustMcpServer::start()`.

use async_trait::async_trait;
use rmcp::model::{CallToolResult, ErrorData};
use serde_json::Value;

/// Stable trait. DO NOT break the shape — it is the contract for consumers.
///
/// Each tool:
/// 1. Implements `name()`, `description()`, `input_schema()` for its descriptor.
/// 2. Implements `call()` to execute the logic.
#[async_trait]
pub trait McpTool: Send + Sync {
    /// Stable name (e.g. "load_rust_project"). Snake_case, no prefixes.
    fn name(&self) -> &'static str;

    /// Description readable by the LLM.
    fn description(&self) -> &'static str;

    /// JSON Schema of the input arguments.
    /// If the tool takes no arguments, return `{"type": "object", "properties": {}}`.
    fn input_schema(&self) -> Value;

    /// Executes the tool. `args` comes from the MCP client as `serde_json::Value`.
    /// Returns `CallToolResult` (Markdown text in our case) or `ErrorData`.
    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData>;
}
