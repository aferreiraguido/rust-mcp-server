//! `ProjectLoader` — resolves a `source` (local path, Git URL, archive URL)
//! into a directory containing a `Cargo.toml` ready for analysis.
//!
//! Mirror of `com.softtek.mcp.ProjectLoader` (java-mcp-server) and
//! `src/pymcp/project_loader.py` (python-mcp-server). Same idea: a
//! `resolve_source(source: &str) -> PathBuf` function that accepts all three
//! formats.
//!
//! **Status (iter 2)**:
//! - Local path: ✅ complete.
//! - Git URL: ⏳ classifies correctly, but actual cloning is deferred
//!   to iter 3 (coinciding with `ra_ap_*` integration, which also
//!   requires its own toolchain). Returns a clear `NotImplemented`.
//! - Archive URL: ✅ download+extract working (zip + tar.gz + tar.zst
//!   + tar.xz + tar). Uses `ureq` + `zip` + `tar` + `flate2` + `zstd` +
//!     `xz2` (all pure-Rust except `xz2` which is a liblzma binding, OK
//!     for fat-binary on macOS/Linux/Windows).

use crate::model::{ProjectLoadError, ProjectLoadErrorCode, ProjectLoadResult};
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use url::Url;

#[derive(Debug, Clone)]
pub struct ProjectLoader {
    cache_dir: Option<PathBuf>,
}

impl ProjectLoader {
    pub fn new() -> Self {
        Self { cache_dir: None }
    }

    pub fn with_cache_dir(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir: Some(cache_dir),
        }
    }

    fn cache_dir(&self) -> PathBuf {
        self.cache_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("rust-mcp-server-cache"))
    }

    pub fn classify(source: &str) -> SourceKind {
        // Shortcut: SSH-style (git@host:user/repo) — not a standard URL but is Git.
        if source.starts_with("git@") {
            return SourceKind::GitUrl;
        }
        if let Ok(url) = Url::parse(source) {
            match url.scheme() {
                "http" | "https" => {
                    let path = url.path().to_lowercase();
                    if path.ends_with(".zip")
                        || path.ends_with(".tar.gz")
                        || path.ends_with(".tgz")
                        || path.ends_with(".tar.zst")
                        || path.ends_with(".tzst")
                        || path.ends_with(".tar.xz")
                        || path.ends_with(".txz")
                        || path.ends_with(".tar")
                    {
                        SourceKind::ArchiveUrl
                    } else {
                        SourceKind::GitUrl
                    }
                }
                "git" | "ssh" => SourceKind::GitUrl,
                "file" => SourceKind::LocalPath,
                _ => SourceKind::LocalPath,
            }
        } else {
            SourceKind::LocalPath
        }
    }

    pub async fn resolve(&self, source: &str) -> ProjectLoadResult<PathBuf> {
        match Self::classify(source) {
            SourceKind::LocalPath => Self::resolve_local(source),
            SourceKind::GitUrl => self.resolve_git(source).await,
            SourceKind::ArchiveUrl => self.resolve_archive(source).await,
        }
    }

    fn resolve_local(source: &str) -> ProjectLoadResult<PathBuf> {
        let path = PathBuf::from(source);
        if !path.exists() {
            return Err(ProjectLoadError::new(
                ProjectLoadErrorCode::SourceNotFound,
                format!("Path does not exist: {}", source),
            )
            .with_path(source));
        }
        if !path.is_dir() {
            return Err(ProjectLoadError::new(
                ProjectLoadErrorCode::SourceNotFound,
                format!("Path is not a directory: {}", source),
            )
            .with_path(source));
        }
        if !path.join("Cargo.toml").exists() {
            return Err(ProjectLoadError::new(
                ProjectLoadErrorCode::CargoTomlMissing,
                format!("No Cargo.toml found in {}", source),
            )
            .with_path(source));
        }
        // Cache an absolute path: every tool resolves files against
        // `ProjectEntry.path`, so it must not depend on the process CWD.
        // `std::path::absolute` avoids the `\\?\` verbatim prefix that
        // `canonicalize` adds on Windows.
        std::path::absolute(&path).map_err(|e| {
            ProjectLoadError::new(
                ProjectLoadErrorCode::InternalError,
                format!("Cannot make path absolute: {} ({})", source, e),
            )
            .with_path(source)
        })
    }

    async fn resolve_git(&self, url: &str) -> ProjectLoadResult<PathBuf> {
        let cache = self.cache_dir();
        std::fs::create_dir_all(&cache).map_err(|e| {
            ProjectLoadError::new(
                ProjectLoadErrorCode::InternalError,
                format!("Cannot create cache dir: {}", e),
            )
        })?;

        let repo_name = derive_repo_name(url);
        let dest = unique_dir(&cache.join(&repo_name));
        info!(url = url, dest = %dest.display(), "cloning git repo via `git` CLI");

        // Call `git` CLI in a spawn_blocking (process::Command
        // is blocking). Use `git clone --depth 1` for fast
        // (shallow) clones.
        let url_owned = url.to_string();
        let dest_clone = dest.clone();
        let clone_result =
            tokio::task::spawn_blocking(move || clone_via_git_cli(&url_owned, &dest_clone))
                .await
                .map_err(|e| {
                    ProjectLoadError::new(
                        ProjectLoadErrorCode::InternalError,
                        format!("blocking task: {}", e),
                    )
                })?;

        if let Err(msg) = clone_result {
            return Err(ProjectLoadError::new(
                ProjectLoadErrorCode::GitCloneFailed,
                format!("git clone failed for {}: {}", url, msg),
            )
            .with_path(url));
        }

        if !dest.join("Cargo.toml").exists() {
            return Err(ProjectLoadError::new(
                ProjectLoadErrorCode::CargoTomlMissing,
                format!(
                    "Cloned {} but no Cargo.toml at root (not a Rust crate?)",
                    url
                ),
            )
            .with_path(url));
        }

        debug!("cloned ok");
        Ok(dest)
    }

    async fn resolve_archive(&self, url: &str) -> ProjectLoadResult<PathBuf> {
        let cache = self.cache_dir();
        std::fs::create_dir_all(&cache).map_err(|e| {
            ProjectLoadError::new(
                ProjectLoadErrorCode::InternalError,
                format!("Cannot create cache dir: {}", e),
            )
        })?;

        let archive_name = url
            .rsplit('/')
            .next()
            .and_then(|s| s.split('?').next())
            .unwrap_or("archive")
            .to_string();
        let archive_path = cache.join(&archive_name);

        info!(url = url, dest = %archive_path.display(), "downloading archive");

        // Download
        let url_owned = url.to_string();
        let archive_path_clone = archive_path.clone();
        let download_result =
            tokio::task::spawn_blocking(move || download_blocking(&url_owned, &archive_path_clone))
                .await
                .map_err(|e| {
                    ProjectLoadError::new(
                        ProjectLoadErrorCode::InternalError,
                        format!("blocking task: {}", e),
                    )
                })?;

        if let Err(msg) = download_result {
            return Err(ProjectLoadError::new(
                ProjectLoadErrorCode::ArchiveExtractionFailed,
                format!("download failed for {}: {}", url, msg),
            )
            .with_path(url));
        }

        // Extract
        let archive_path_for_extract = archive_path.clone();
        let extract_result =
            tokio::task::spawn_blocking(move || extract_blocking(&archive_path_for_extract))
                .await
                .map_err(|e| {
                    ProjectLoadError::new(
                        ProjectLoadErrorCode::InternalError,
                        format!("blocking task: {}", e),
                    )
                })?;

        let extract_dir = match extract_result {
            Ok(d) => d,
            Err(msg) => {
                return Err(ProjectLoadError::new(
                    ProjectLoadErrorCode::ArchiveExtractionFailed,
                    format!("extraction failed: {}", msg),
                )
                .with_path(url));
            }
        };

        if !extract_dir.join("Cargo.toml").exists() {
            return Err(ProjectLoadError::new(
                ProjectLoadErrorCode::CargoTomlMissing,
                "Extracted archive but no Cargo.toml at root (not a Rust crate?)",
            )
            .with_path(url));
        }

        Ok(extract_dir)
    }

    pub fn cleanup(&self) -> std::io::Result<()> {
        let dir = self.cache_dir();
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
}

impl Default for ProjectLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    LocalPath,
    GitUrl,
    ArchiveUrl,
}

#[allow(dead_code)]
fn derive_repo_name(url: &str) -> String {
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    trimmed
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("repo")
        .to_string()
}

fn unique_dir(base: &Path) -> PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }
    for i in 0..1000 {
        let candidate = base.with_extension(format!("{}", i));
        if !candidate.exists() {
            return candidate;
        }
    }
    base.to_path_buf()
}

/// Blocking wrapper around `git clone --depth 1 <url> <dest>`. Uses
/// `git` CLI (not `gix`) because `gix::clone`'s API requires
/// implementing `NestedProgress` and orchestrating the full pipeline.
/// `git` CLI is the pragmatic choice that works on all systems
/// with `git` installed (≥ 2.0).
fn clone_via_git_cli(url: &str, dest: &Path) -> Result<(), String> {
    let output = std::process::Command::new("git")
        .args(["clone", "--depth", "1", url, &dest.to_string_lossy()])
        .output()
        .map_err(|e| format!("failed to execute `git`: {} (is `git` installed?)", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git clone exited with {}: {}",
            output.status, stderr
        ));
    }
    Ok(())
}

fn download_blocking(url: &str, dest: &Path) -> Result<(), String> {
    if let Ok(parsed) = Url::parse(url) {
        if parsed.scheme() == "file" {
            if let Ok(p) = parsed.to_file_path() {
                std::fs::copy(&p, dest).map_err(|e| e.to_string())?;
                return Ok(());
            }
        }
    }
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
    Ok(())
}

fn extract_blocking(archive: &Path) -> Result<PathBuf, String> {
    let parent = archive
        .parent()
        .ok_or_else(|| "no parent dir".to_string())?
        .to_path_buf();
    let stem = archive
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("extracted")
        .to_string();
    // Strip compound extensions like .tar from .tar.gz
    let stem = if let Some(s) = stem.strip_suffix(".tar") {
        s.to_string()
    } else {
        stem
    };
    let dest = unique_dir(&parent.join(&stem));

    let lower = archive.to_string_lossy().to_lowercase();
    if lower.ends_with(".zip") {
        extract_zip(archive, &dest)?;
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        extract_tar_gz(archive, &dest)?;
    } else if lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
        extract_tar_zst(archive, &dest)?;
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        extract_tar_xz(archive, &dest)?;
    } else if lower.ends_with(".tar") {
        extract_tar(archive, &dest)?;
    } else {
        return Err(format!("unsupported archive format: {}", lower));
    }

    if let Ok(entries) = std::fs::read_dir(&dest) {
        let entries: Vec<_> = entries.filter_map(Result::ok).collect();
        if entries.len() == 1 {
            let only = &entries[0];
            if only.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && only.path().join("Cargo.toml").exists()
            {
                return Ok(only.path());
            }
        }
    }

    Ok(dest)
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let out = match entry.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn extract_tar(archive: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(f);
    tar.unpack(dest).map_err(|e| e.to_string())?;
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(f);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(dest).map_err(|e| e.to_string())?;
    Ok(())
}

fn extract_tar_zst(archive: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let zst = zstd::stream::read::Decoder::new(f).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(zst);
    tar.unpack(dest).map_err(|e| e.to_string())?;
    Ok(())
}

fn extract_tar_xz(archive: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let xz = xz2::read::XzDecoder::new(f);
    let mut tar = tar::Archive::new(xz);
    tar.unpack(dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_local() {
        assert_eq!(ProjectLoader::classify("/tmp/foo"), SourceKind::LocalPath);
        assert_eq!(
            ProjectLoader::classify("./Cargo.toml"),
            SourceKind::LocalPath
        );
    }

    #[test]
    fn classify_git() {
        assert_eq!(
            ProjectLoader::classify("https://github.com/foo/bar.git"),
            SourceKind::GitUrl
        );
        assert_eq!(
            ProjectLoader::classify("https://github.com/foo/bar"),
            SourceKind::GitUrl
        );
        assert_eq!(
            ProjectLoader::classify("git@github.com:foo/bar.git"),
            SourceKind::GitUrl
        );
    }

    #[test]
    fn classify_archive() {
        assert_eq!(
            ProjectLoader::classify("https://github.com/foo/bar/archive/refs/heads/main.zip"),
            SourceKind::ArchiveUrl
        );
        assert_eq!(
            ProjectLoader::classify("https://example.com/foo.tar.gz"),
            SourceKind::ArchiveUrl
        );
        assert_eq!(
            ProjectLoader::classify("https://example.com/foo.tgz"),
            SourceKind::ArchiveUrl
        );
    }

    #[test]
    fn resolve_local_ok() {
        let loader = ProjectLoader::new();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_crate");
        let r = futures::executor::block_on(loader.resolve(fixture.to_str().unwrap())).unwrap();
        assert!(r.join("Cargo.toml").exists());
    }

    #[test]
    fn resolve_local_missing() {
        let loader = ProjectLoader::new();
        let err = futures::executor::block_on(loader.resolve("/nonexistent/12345")).unwrap_err();
        assert_eq!(err.code, ProjectLoadErrorCode::SourceNotFound);
    }

    #[test]
    fn resolve_local_no_cargo() {
        let loader = ProjectLoader::new();
        let dir = tempfile::tempdir().unwrap();
        let err =
            futures::executor::block_on(loader.resolve(dir.path().to_str().unwrap())).unwrap_err();
        assert_eq!(err.code, ProjectLoadErrorCode::CargoTomlMissing);
    }

    #[tokio::test]
    #[ignore = "requires network — run manually with 'cargo test -- --ignored'"]
    async fn resolve_git_happy_path() {
        // Clone a small public repo to verify the end-to-end flow.
        let loader = ProjectLoader::new();
        let result = futures::executor::block_on(
            loader.resolve("https://github.com/anomalyco/opencode-sdk-python.git"),
        );
        match result {
            Ok(path) => {
                assert!(
                    path.join("Cargo.toml").exists()
                        || path.join("pyproject.toml").exists()
                        || path.join("setup.py").exists()
                );
            }
            Err(e) => {
                // Network failure is not a test error — ignored.
                panic!("Git clone failed: {:?}", e);
            }
        }
    }

    #[tokio::test]
    async fn resolve_git_invalid_url() {
        // Invalid URL — `git` command fails fast, no network involved.
        let loader = ProjectLoader::new();
        let err = futures::executor::block_on(
            loader.resolve("https://this-domain-does-not-exist-12345.invalid/foo.git"),
        )
        .unwrap_err();
        // The error can be GitCloneFailed or InternalError (if `git` is
        // not installed). Either way, it is not a panic.
        assert!(matches!(
            err.code,
            ProjectLoadErrorCode::GitCloneFailed | ProjectLoadErrorCode::InternalError
        ));
    }

    #[test]
    fn derive_repo_name_works() {
        assert_eq!(derive_repo_name("https://github.com/foo/bar.git"), "bar");
        assert_eq!(derive_repo_name("git@github.com:foo/bar.git"), "bar");
        assert_eq!(derive_repo_name("https://github.com/foo/bar"), "bar");
    }
}
