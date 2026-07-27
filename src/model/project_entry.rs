//! Record of a Rust project loaded in memory.
//!
//! Analogous to `com.softtek.mcp.model.ProjectEntry` (java-mcp-server).
//! In iteration 2 it will carry an `AnalysisHost` (ra_ap_hir) when
//! available; in iteration 1 only crate metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// Name derived from the directory (last segment of the path).
    pub name: String,

    /// Optional alias (if the user passes it in `load_rust_project`).
    pub alias: Option<String>,

    /// Path to the root `Cargo.toml` of the crate/workspace.
    pub path: PathBuf,

    /// Project type: single crate or workspace.
    pub kind: ProjectKind,

    /// When this entry expires (10 min by default).
    pub expiry: DateTime<Utc>,

    /// When it was loaded.
    pub loaded_at: DateTime<Utc>,

    /// Member crates (if workspace).
    pub member_crates: Vec<String>,

    /// Cache of the parsed `Cargo.toml` (populated lazily by
    /// `RustAnalyzerUtils::cargo_manifest`). `OnceLock` is not `Clone`,
    /// so we implement `Clone` manually.
    #[serde(skip)]
    pub cached_manifest: OnceLock<crate::analysis::cargo_manifest::CargoManifest>,
}

impl Clone for ProjectEntry {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            alias: self.alias.clone(),
            path: self.path.clone(),
            kind: self.kind,
            expiry: self.expiry,
            loaded_at: self.loaded_at,
            member_crates: self.member_crates.clone(),
            cached_manifest: OnceLock::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectKind {
    SingleCrate,
    Workspace,
}

impl ProjectEntry {
    /// Effective name (alias if set, otherwise the derived name).
    pub fn effective_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }

    /// Has it expired?
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expiry
    }
}
