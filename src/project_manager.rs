//! `ProjectManager` — thread-safe cache of loaded projects.
//!
//! Mirror of `com.softtek.mcp.ProjectManager` (java-mcp-server) and
//! `com.softtek.mcp.CobolFileManager` (cobol-mcp-server). Same idea:
//! `RwLock<HashMap<name, ProjectEntry>>` with per-entry expiry.

use crate::model::{
    ProjectEntry, ProjectKind, ProjectLoadError, ProjectLoadErrorCode, ProjectLoadResult,
};
use crate::project_loader::ProjectLoader;
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Default expiry: 10 minutes (same as java/cobol).
pub const DEFAULT_EXPIRY_MINUTES: i64 = 10;

#[derive(Clone)]
pub struct ProjectManager {
    inner: Arc<RwLock<HashMap<String, ProjectEntry>>>,
    loader: ProjectLoader,
}

impl std::fmt::Debug for ProjectManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectManager")
            .field("inner", &self.inner)
            .finish()
    }
}

impl ProjectManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            loader: ProjectLoader::new(),
        }
    }

    pub fn with_loader(loader: ProjectLoader) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            loader,
        }
    }

    /// Loads a project from a source (local path, Git URL, archive URL).
    pub async fn load(
        &self,
        source: &str,
        alias: Option<&str>,
        expiry_minutes: Option<i64>,
    ) -> ProjectLoadResult<ProjectEntry> {
        let resolved_path = self.loader.resolve(source).await?;
        let (kind, member_crates) = detect_workspace(&resolved_path).await?;
        let name = derive_name(&resolved_path);
        let now = Utc::now();
        let entry = ProjectEntry {
            name: name.clone(),
            alias: alias.map(|s| s.to_string()),
            path: resolved_path,
            kind,
            expiry: now + Duration::minutes(expiry_minutes.unwrap_or(DEFAULT_EXPIRY_MINUTES)),
            loaded_at: now,
            member_crates,
            cached_manifest: std::sync::OnceLock::new(),
        };

        let key = entry.effective_name().to_string();
        let mut guard = self.inner.write().await;
        let now = Utc::now();
        guard.retain(|_, v| !v.is_expired(now));
        guard.insert(key.clone(), entry.clone());

        info!(project = %key, "project loaded");
        Ok(entry)
    }

    pub async fn find(&self, name_or_alias: &str) -> Option<ProjectEntry> {
        let now = Utc::now();
        let mut guard = self.inner.write().await;
        guard.retain(|_, v| !v.is_expired(now));
        if let Some(entry) = guard.get(name_or_alias) {
            return Some(entry.clone());
        }
        guard
            .values()
            .find(|e| e.alias.as_deref() == Some(name_or_alias))
            .cloned()
    }

    pub async fn remove(&self, name_or_alias: &str) -> Option<ProjectEntry> {
        let mut guard = self.inner.write().await;
        let removed = if let Some(entry) = guard.remove(name_or_alias) {
            Some(entry)
        } else {
            let key = guard
                .iter()
                .find(|(_, v)| v.alias.as_deref() == Some(name_or_alias))
                .map(|(k, _)| k.clone());
            key.and_then(|k| guard.remove(&k))
        };
        removed
    }

    pub async fn list(&self) -> Vec<ProjectEntry> {
        let now = Utc::now();
        let mut guard = self.inner.write().await;
        guard.retain(|_, v| !v.is_expired(now));
        let mut out: Vec<ProjectEntry> = guard.values().cloned().collect();
        out.sort_by(|a, b| a.effective_name().cmp(b.effective_name()));
        out
    }

    pub async fn count(&self) -> usize {
        let now = Utc::now();
        let guard = self.inner.read().await;
        guard.values().filter(|v| !v.is_expired(now)).count()
    }
}

impl Default for ProjectManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads `Cargo.toml` to detect if it is a workspace and extract members.
async fn detect_workspace(path: &Path) -> ProjectLoadResult<(ProjectKind, Vec<String>)> {
    let cargo_toml = path.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(ProjectLoadError::new(
            ProjectLoadErrorCode::CargoTomlMissing,
            "Cargo.toml missing after resolution",
        ));
    }
    let content = std::fs::read_to_string(&cargo_toml).map_err(|e| {
        ProjectLoadError::new(
            ProjectLoadErrorCode::CargoTomlMalformed,
            format!("Cannot read Cargo.toml: {}", e),
        )
    })?;

    match crate::analysis::cargo_manifest::CargoManifest::parse(
        &content,
        &cargo_toml.display().to_string(),
    ) {
        Ok(m) => {
            if let Some(ws) = m.workspace {
                debug!(members = ?ws.members, "detected workspace");
                Ok((ProjectKind::Workspace, ws.members.unwrap_or_default()))
            } else {
                Ok((ProjectKind::SingleCrate, vec![]))
            }
        }
        Err(e) => {
            // We don't fail the load for a weird Cargo.toml; we report single crate.
            let _ = e;
            Ok((ProjectKind::SingleCrate, vec![]))
        }
    }
}

fn derive_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn load_then_find() {
        let mgr = ProjectManager::new();
        let entry = mgr
            .load(env!("CARGO_MANIFEST_DIR"), Some("self"), None)
            .await
            .expect("load self");
        assert_eq!(entry.effective_name(), "self");
        assert!(matches!(entry.kind, ProjectKind::SingleCrate));

        let found = mgr.find("self").await;
        assert!(found.is_some());

        let removed = mgr.remove("self").await;
        assert!(removed.is_some());
        assert!(mgr.find("self").await.is_none());
    }

    #[tokio::test]
    async fn list_is_sorted_and_stable() {
        let mgr = ProjectManager::new();
        let _ = mgr
            .load(env!("CARGO_MANIFEST_DIR"), Some("zeta"), None)
            .await;
        let _ = mgr
            .load(env!("CARGO_MANIFEST_DIR"), Some("alpha"), None)
            .await;
        let list = mgr.list().await;
        let names: Vec<&str> = list.iter().map(|e| e.effective_name()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[tokio::test]
    async fn missing_path_errors() {
        let mgr = ProjectManager::new();
        let err = mgr
            .load("/nonexistent/path/12345", None, None)
            .await
            .unwrap_err();
        assert_eq!(err.code, ProjectLoadErrorCode::SourceNotFound);
    }

    #[tokio::test]
    async fn missing_cargo_toml_errors() {
        let mgr = ProjectManager::new();
        let dir = tempfile::tempdir().unwrap();
        let err = mgr
            .load(dir.path().to_str().unwrap(), None, None)
            .await
            .unwrap_err();
        assert_eq!(err.code, ProjectLoadErrorCode::CargoTomlMissing);
    }
}
