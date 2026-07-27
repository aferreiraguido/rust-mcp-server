//! MCP server tools.
//!
//! Mirror of `com.softtek.mcp.tools` (java/cobol-mcp-server).
//! Each tool in its own file, explicitly registered in
//! `RustMcpServer::start()` (server.rs).

pub mod analysis_summary_tool;
pub mod base_rust_tool;
pub mod bug_hunt_helpers;
pub mod find_dead_code_tool;
pub mod find_implementations_tool;
pub mod find_panic_sites_tool;
pub mod find_references_tool;
pub mod find_unchecked_patterns_tool;
pub mod find_unsafe_blocks_tool;
pub mod find_unused_dependencies_tool;
pub mod find_unwrap_patterns_tool;
pub mod function_call_graph_tool;
pub mod function_callers_tool;
pub mod get_file_content_tool;
pub mod get_type_hierarchy_tool;
pub mod inspect_build_config_tool;
pub mod inspect_enum_tool;
pub mod inspect_function_tool;
pub mod inspect_struct_tool;
pub mod inspect_trait_tool;
pub mod list_cargo_features_tool;
pub mod list_crates_tool;
pub mod list_dependencies_tool;
pub mod list_enums_tool;
pub mod list_functions_tool;
pub mod list_impls_tool;
pub mod list_loaded_projects_tool;
pub mod list_macros_tool;
pub mod list_modules_tool;
pub mod list_structs_tool;
pub mod list_traits_tool;
pub mod load_rust_project_tool;
pub mod mcp_tool;
pub mod program_summary_tool;
pub mod project_metadata_tool;
pub mod ra_helpers;
pub mod render;
pub mod resolve_type_tool;
pub mod search_source_tool;
pub mod struct_field_xref_tool;
pub mod unload_rust_project_tool;
pub mod validate_code_reference_tool;
