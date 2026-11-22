//! ASG analysis layer.
//!
//! Iteration 2: only `cargo_manifest` (parses `Cargo.toml`) and `syntax` (parses
//! `.rs` files with `syn`). No `ra_ap_*` yet (iter 3).
//!
//!
//! See AGENTS.md §13 "Iteration plan" for the roadmap.

pub mod analysis_host;
pub mod bug_pattern_analyzer;
pub mod cargo_manifest;
pub mod path_utils;
pub mod ra_walker;
pub mod rust_analyzer_utils;
pub mod symbol_resolver;
pub mod syntax;
pub mod text_utils;
