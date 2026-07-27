//! Data models (records) for the project.
//!
//! Mirror of `com.softtek.mcp.model` in `java-mcp-server` and `cobol-mcp-server`:
//! pure types, no logic, no I/O.

pub mod finding;
pub mod project_entry;
pub mod project_load_exception;

pub use finding::{Finding, FindingCategory, FindingSeverity};
pub use project_entry::{ProjectEntry, ProjectKind};
pub use project_load_exception::{ProjectLoadError, ProjectLoadErrorCode, ProjectLoadResult};
