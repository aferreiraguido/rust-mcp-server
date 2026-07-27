//! Record for a finding from the bug-hunt analysis.
//!
//! Mirror of `com.softtek.mcp.analysis.BugPatternAnalyzer.Finding` (cobol-mcp-server).
//! Not used in iteration 1; comes into play when `bug_pattern_analyzer.rs`
//! (iteration 2/3) starts producing findings.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub category: FindingCategory,
    pub severity: FindingSeverity,
    /// Stable code (e.g. "UNSAFE_FFI", "DEAD_CODE", "UNREACHABLE").
    pub code: String,
    /// Human-readable message for the LLM.
    pub message: String,
    /// File path.
    pub file: String,
    /// 1-based line number (0 if it cannot be determined).
    pub line: u32,
    /// Additional evidence (source text, AST path, etc.).
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingCategory {
    /// ALTER-like: unsafe blocks, FFI, transmute.
    Unsafe,
    /// Go-to-without-target: unresolved calls, dead paths.
    ControlFlow,
    /// Write-only: variables never read.
    DataFlow,
    /// Paragraphs/functions never called.
    DeadCode,
    /// `.unwrap()` / `.expect()` method calls.
    UnwrapRisk,
    /// `panic!`, `unreachable!`, `todo!`, `unimplemented!`.
    PanicSite,
    /// Structural bugs (duplicate names, etc.).
    Structural,
}
