//! Project load errors.
//!
//! Mirror of `com.softtek.mcp.model.ProjectLoadException` (java-mcp-server).
//! Stable codes so MCP consumers can parse them.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize)]
#[error("[{code:?}] {message}")]
pub struct ProjectLoadError {
    pub code: ProjectLoadErrorCode,
    pub message: String,
    /// Path involved, if applicable (NOT logged with PII in production — see AGENTS.md §10).
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectLoadErrorCode {
    /// Local path does not exist or is not a readable directory.
    SourceNotFound,
    /// `Cargo.toml` not found at the path.
    CargoTomlMissing,
    /// `Cargo.toml` exists but cannot be parsed.
    CargoTomlMalformed,
    /// Git URL cannot be cloned.
    GitCloneFailed,
    /// Archive (zip/tar.gz/tar.zst) cannot be extracted.
    ArchiveExtractionFailed,
    /// Internal error (our bug, not the input's).
    InternalError,
}

impl ProjectLoadError {
    pub fn new(code: ProjectLoadErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

pub type ProjectLoadResult<T> = Result<T, ProjectLoadError>;
