//! Golden snapshot tests for iter 2 tools.
//!
//! Loads the `tests/fixtures/sample_crate` fixture and runs each tool
//! to verify that the Markdown is deterministic and has the expected
//! shape.
//!
//! Current coverage (iter 2): 8 snapshots
//! - list_loaded_projects_empty
//! - list_loaded_projects_with_one_project
//! - program_summary
//! - project_metadata
//! - list_functions
//! - list_structs
//! - inspect_function
//! - list_dependencies

use rust_mcp_server::tools::mcp_tool::McpTool;
use rust_mcp_server::ProjectManager;
use std::path::PathBuf;
use std::sync::Arc;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample_crate")
}

async fn load_fixture() -> Arc<ProjectManager> {
    let manager = Arc::new(ProjectManager::new());
    manager
        .load(fixture_path().to_str().unwrap(), Some("sample"), None)
        .await
        .expect("load fixture");
    manager
}

// Helper: given an McpTool + args, returns the Markdown.
async fn call_text(tool: &dyn McpTool, args: serde_json::Value) -> String {
    let result = tool.call(args).await.expect("tool call");
    match &result.content[0] {
        rmcp::model::ContentBlock::Text(t) => t.text.clone(),
        other => panic!("expected TextContent, got {:?}", other),
    }
}

// ─── list_loaded_projects (regression) ──────────────────────────────

#[tokio::test]
async fn list_loaded_projects_empty() {
    let manager = Arc::new(ProjectManager::new());
    let tool =
        rust_mcp_server::tools::list_loaded_projects_tool::ListLoadedProjectsTool::new(manager);
    let text = call_text(&tool, serde_json::json!({})).await;
    insta::assert_snapshot!(text);
}

#[tokio::test]
async fn list_loaded_projects_with_one_project() {
    let manager = load_fixture().await;
    let tool =
        rust_mcp_server::tools::list_loaded_projects_tool::ListLoadedProjectsTool::new(manager);
    let text = call_text(&tool, serde_json::json!({})).await;
    insta::assert_snapshot!(text);
}

// ─── program_summary ────────────────────────────────────────────────

#[tokio::test]
async fn program_summary_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::program_summary_tool::ProgramSummaryTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    insta::assert_snapshot!(text);
}

// ─── project_metadata ───────────────────────────────────────────────

#[tokio::test]
async fn project_metadata_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::project_metadata_tool::ProjectMetadataTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    insta::assert_snapshot!(text);
}

// ─── list_functions ─────────────────────────────────────────────────

#[tokio::test]
async fn list_functions_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::list_functions_tool::ListFunctionsTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    insta::assert_snapshot!(text);
}

// ─── list_structs ───────────────────────────────────────────────────

#[tokio::test]
async fn list_structs_on_sample() {
    // The current fixture has no structs — returns "_No structs._".
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::list_structs_tool::ListStructsTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    insta::assert_snapshot!(text);
}

// ─── inspect_function ───────────────────────────────────────────────

#[tokio::test]
async fn inspect_function_add() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::inspect_function_tool::InspectFunctionTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "function_name": "add"}),
    )
    .await;
    insta::assert_snapshot!(text);
}

// ─── list_dependencies ──────────────────────────────────────────────

#[tokio::test]
async fn list_dependencies_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::list_dependencies_tool::ListDependenciesTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    insta::assert_snapshot!(text);
}

// ─── validate_code_reference (negative) ─────────────────────────────

#[tokio::test]
async fn validate_existing_symbol() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::validate_code_reference_tool::ValidateCodeReferenceTool::new(
        manager,
    );
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    insta::assert_snapshot!(text);
}

#[tokio::test]
async fn validate_missing_symbol() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::validate_code_reference_tool::ValidateCodeReferenceTool::new(
        manager,
    );
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "nonexistent_xyz"}),
    )
    .await;
    insta::assert_snapshot!(text);
}

// ─── analysis_summary (ra_ap_*) ─────────────────────────────────────

#[tokio::test]
async fn analysis_summary_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::analysis_summary_tool::AnalysisSummaryTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    insta::assert_snapshot!(text);
}

// ─── resolve_type (ra_ap_* semantic query) ──────────────────────────

#[tokio::test]
async fn resolve_type_add_function() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::resolve_type_tool::ResolveTypeTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    // Smoke check: the output must contain the path and the results table.
    assert!(
        text.contains("Definition of `add`"),
        "got: {}",
        &text[..text.len().min(200)]
    );
    assert!(
        text.contains("/lib.rs"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn resolve_type_unknown_symbol() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::resolve_type_tool::ResolveTypeTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "totally_made_up_xyz"}),
    )
    .await;
    // Must report the error in Markdown format
    assert!(
        text.contains("not found"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

// ─── semantic query smoke tests (iter 3.6) ────────────────────────

#[tokio::test]
async fn find_implementations_returns_markdown() {
    let manager = load_fixture().await;
    let tool =
        rust_mcp_server::tools::find_implementations_tool::FindImplementationsTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    // `add` is a fn, not a trait — the tool must respond honestly.
    assert!(
        text.contains("Implementations of `add`") || text.contains("Error"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn get_type_hierarchy_returns_markdown() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::get_type_hierarchy_tool::GetTypeHierarchyTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    assert!(
        text.contains("Type hierarchy") || text.contains("Error"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn function_call_graph_returns_markdown() {
    let manager = load_fixture().await;
    let tool =
        rust_mcp_server::tools::function_call_graph_tool::FunctionCallGraphTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    assert!(
        text.contains("Callers of `add`"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn struct_field_xref_returns_markdown() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::struct_field_xref_tool::StructFieldXrefTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample", "field": "add"})).await;
    assert!(
        text.contains("References to field `add`"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

// ─── text-based reference search (iter 2) ─────────────────────────

#[tokio::test]
async fn find_references_text_search() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::find_references_tool::FindReferencesTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    // The fixture has `add` referenced in test_add.
    assert!(
        text.contains("References to `add`"),
        "got: {}",
        &text[..text.len().min(200)]
    );
    assert!(
        text.contains("References (") || text.contains("hit(s)"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn function_callers_text_search() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::function_callers_tool::FunctionCallersTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    // The fixture has `add(2, 3)` in test_add.
    assert!(
        text.contains("Callers of `add`"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

// ─── file ops (iter 2) ─────────────────────────────────────────────

#[tokio::test]
async fn get_file_content_reads_lib() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::get_file_content_tool::GetFileContentTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "rel_path": "src/lib.rs"}),
    )
    .await;
    assert!(
        text.contains("```rust"),
        "got: {}",
        &text[..text.len().min(200)]
    );
    assert!(
        text.contains("pub fn add"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn get_file_content_blocks_path_traversal() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::get_file_content_tool::GetFileContentTool::new(manager);
    // Path traversal: `..` must fail with an error (no content).
    // The tool propagates ErrorData::internal_error, so `call` returns Err.
    let result = tool
        .call(serde_json::json!({"name": "sample", "rel_path": "../Cargo.toml"}))
        .await;
    assert!(
        result.is_err(),
        "expected Err for path traversal, got: {:?}",
        result
    );
    let err = result.unwrap_err();
    // The message must mention the path traversal.
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("path escapes") || msg.contains("PermissionDenied"),
        "expected path escapes error, got: {}",
        msg
    );
}

#[tokio::test]
async fn inspect_build_config_returns_cargo_toml() {
    let manager = load_fixture().await;
    let tool =
        rust_mcp_server::tools::inspect_build_config_tool::InspectBuildConfigTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    assert!(
        text.contains("Cargo.toml"),
        "got: {}",
        &text[..text.len().min(200)]
    );
    assert!(
        text.contains("sample_crate"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn search_source_finds_test_function() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::search_source_tool::SearchSourceTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "query": "test_add"}),
    )
    .await;
    assert!(
        text.contains("test_add") || text.contains("hit(s)"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

// ─── list_X tools (iter 2) ────────────────────────────────────────

#[tokio::test]
async fn list_enums_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::list_enums_tool::ListEnumsTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    assert!(
        text.contains("# Enums") || text.contains("No enums"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn list_traits_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::list_traits_tool::ListTraitsTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    assert!(
        text.contains("# Traits") || text.contains("No traits"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn list_impls_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::list_impls_tool::ListImplsTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    assert!(
        text.contains("# Impl blocks") || text.contains("No impl blocks"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn list_modules_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::list_modules_tool::ListModulesTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    // The sample has `mod tests`.
    assert!(
        text.contains("# Modules") || text.contains("No modules"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn list_macros_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::list_macros_tool::ListMacrosTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    assert!(
        text.contains("# Macros") || text.contains("No macros"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn list_crates_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::list_crates_tool::ListCratesTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    assert!(
        text.contains("# Crates") || text.contains("Crates in"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn list_cargo_features_on_sample() {
    let manager = load_fixture().await;
    let tool =
        rust_mcp_server::tools::list_cargo_features_tool::ListCargoFeaturesTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    // The sample has no [features] → 0 features.
    assert!(
        text.contains("Features of `sample_crate` (0)"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

// ─── inspect_X tools (iter 2) — the fixture does not have the
//     necessary types for inspect_enum/struct/trait. We verify the
//     "not found" path as a minimum. ─────────────────────────────────

#[tokio::test]
async fn inspect_enum_not_found() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::inspect_enum_tool::InspectEnumTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "enum_name": "MyEnum"}),
    )
    .await;
    // The sample has no MyEnum → honest error.
    assert!(
        text.contains("not found"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn inspect_struct_not_found() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::inspect_struct_tool::InspectStructTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "struct_name": "MyStruct"}),
    )
    .await;
    assert!(
        text.contains("not found"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn inspect_trait_not_found() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::inspect_trait_tool::InspectTraitTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "trait_name": "MyTrait"}),
    )
    .await;
    assert!(
        text.contains("not found"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

// ─── find_implementations (ra_ap_* semantic) ───────────────────────

#[tokio::test]
async fn find_implementations_smoke() {
    let manager = load_fixture().await;
    let tool =
        rust_mcp_server::tools::find_implementations_tool::FindImplementationsTool::new(manager);
    // The fixture has no traits — we use a function (which is a valid symbol).
    // The test verifies that the tool runs without hanging.
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    // No panic, returns something (could be "No implementations" or similar).
    assert!(!text.is_empty());
}

// ─── get_type_hierarchy (ra_ap_* semantic) ──────────────────────────

#[tokio::test]
async fn get_type_hierarchy_smoke() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::get_type_hierarchy_tool::GetTypeHierarchyTool::new(manager);
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    assert!(!text.is_empty());
}

// ─── function_call_graph (ra_ap_* semantic, callers) ────────────────

#[tokio::test]
async fn function_call_graph_add_smoke() {
    let manager = load_fixture().await;
    let tool =
        rust_mcp_server::tools::function_call_graph_tool::FunctionCallGraphTool::new(manager);
    // Smoke test: the tool runs without hanging. The fixture has
    // `assert_eq!(add(2, 3), 5);` inside `#[cfg(test)] mod tests`,
    // but rust-analyzer may or may not consider it a caller (depends
    // on how it processes macro calls). We only verify that the output
    // is valid Markdown.
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "symbol": "add"}),
    )
    .await;
    assert!(
        text.contains("Callers of `add`"),
        "got: {}",
        &text[..text.len().min(300)]
    );
}

// ─── struct_field_xref (ra_ap_* semantic) ──────────────────────────

#[tokio::test]
async fn struct_field_xref_smoke() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::struct_field_xref_tool::StructFieldXrefTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample", "field": "add"})).await;
    // The tool runs without hanging.
    assert!(!text.is_empty());
}

// ─── bug-hunt (iter 4) ─────────────────────────────────────────────

// ─── iter 6+: find_unused_dependencies ────────────────────────────

#[tokio::test]
async fn find_unused_dependencies_on_sample() {
    let manager = load_fixture().await;
    let tool =
        rust_mcp_server::tools::find_unused_dependencies_tool::FindUnusedDependenciesTool::new(
            manager,
        );
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    // The sample has NO deps → 0 findings.
    assert!(
        text.contains("Unused dependencies (0)"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn find_unused_dependencies_on_sample_with_deps() {
    // Load the fixture with unused deps.
    let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sample_crate_with_deps");
    let manager = std::sync::Arc::new(rust_mcp_server::ProjectManager::new());
    manager
        .load(fixture_root.to_str().unwrap(), Some("with_deps"), None)
        .await
        .expect("load sample_crate_with_deps");
    let tool =
        rust_mcp_server::tools::find_unused_dependencies_tool::FindUnusedDependenciesTool::new(
            manager,
        );
    let text = call_text(&tool, serde_json::json!({"name": "with_deps"})).await;
    // The fixture has 3 unused deps: serde, unused_dep, unused_dev_dep.
    // (The default min_severity is "info" so that dev-deps appear.)
    assert!(
        text.contains("Unused dependencies (3)"),
        "got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("serde"),
        "got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("unused_dep"),
        "got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("unused_dev_dep"),
        "got: {}",
        &text[..text.len().min(400)]
    );
}

#[tokio::test]
async fn bug_hunt_unsafe_blocks_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::find_unsafe_blocks_tool::FindUnsafeBlocksTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    // The sample has no unsafe → 0 findings.
    assert!(
        text.contains("Unsafe blocks (0)"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn bug_hunt_unsafe_blocks_with_min_severity() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::find_unsafe_blocks_tool::FindUnsafeBlocksTool::new(manager);
    // min_severity=high should not filter anything because the sample has no
    // unsafe blocks.
    let text = call_text(
        &tool,
        serde_json::json!({"name": "sample", "min_severity": "high"}),
    )
    .await;
    assert!(
        text.contains("Unsafe blocks (0)"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn bug_hunt_unwrap_patterns_on_sample() {
    let manager = load_fixture().await;
    let tool =
        rust_mcp_server::tools::find_unwrap_patterns_tool::FindUnwrapPatternsTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    assert!(
        text.contains("Unwrap / expect (0)"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn bug_hunt_unchecked_patterns_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::find_unchecked_patterns_tool::FindUncheckedPatternsTool::new(
        manager,
    );
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    assert!(
        text.contains("Unchecked memory ops (0)"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn bug_hunt_panic_sites_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::find_panic_sites_tool::FindPanicSitesTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    assert!(
        text.contains("Panic sites (0)"),
        "got: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test]
async fn bug_hunt_dead_code_on_sample() {
    let manager = load_fixture().await;
    let tool = rust_mcp_server::tools::find_dead_code_tool::FindDeadCodeTool::new(manager);
    let text = call_text(&tool, serde_json::json!({"name": "sample"})).await;
    // Iter 4.5: `#[test]` items (test_add, test_multiply) and
    // `#[cfg(test)] mod tests` are NOT marked as dead code.
    // The sample fixture has no dead items → 0 findings.
    assert!(
        text.contains("Dead code (semantic) (0)"),
        "got: {}",
        &text[..text.len().min(300)]
    );
}
