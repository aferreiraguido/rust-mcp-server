//! Cross-platform path helpers.
//!
//! Two concepts that must never be mixed:
//!
//! - **VFS path** (`ra_ap_vfs::VfsPath` / `AbsPathBuf`): must be a real
//!   **OS-absolute** path. On Windows, a rooted path such as `\src\lib.rs`
//!   has no drive prefix and is *not* absolute — `Path::is_absolute()`
//!   returns `false`. Never synthesize these paths with
//!   `PathBuf::from("/")`; always derive them from the filesystem
//!   (canonicalized root + relative path).
//! - **Display path**: project-relative, POSIX-style (`/src/lib.rs`). Stable
//!   on every platform so the Markdown output (and the golden snapshots) is
//!   byte-for-byte deterministic.

use std::path::{Component, Path};

/// Directory names that are never part of the analyzed source tree.
///
/// `target/` holds build artifacts (including generated `.rs` files from
/// build scripts), which would otherwise be indexed as project sources and
/// produce duplicated symbols. VCS metadata never contains sources either.
const IGNORED_DIRS: &[&str] = &["target", ".git", ".hg", ".svn"];

/// `true` if `path` (which must live under `root`) is inside an ignored
/// directory.
pub fn is_ignored(root: &Path, path: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components().any(|component| match component {
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            IGNORED_DIRS.contains(&name.as_ref())
        }
        _ => false,
    })
}

/// POSIX-style, project-relative display path **without** a leading slash:
/// `src\lib.rs` → `src/lib.rs`. Use it in tables that are already documented
/// as relative to the project root.
pub fn display_rel_path(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

/// Normalizes a project-relative path into a stable, POSIX-style display
/// path: `src\lib.rs` → `/src/lib.rs` on Windows, `/src/lib.rs` on Unix.
pub fn display_path(rel: &Path) -> String {
    let s = display_rel_path(rel);
    if s.starts_with('/') {
        s
    } else {
        format!("/{}", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_path_is_posix_and_rooted() {
        assert_eq!(display_path(Path::new("src/lib.rs")), "/src/lib.rs");
        assert_eq!(display_path(Path::new("src\\lib.rs")), "/src/lib.rs");
        assert_eq!(display_path(Path::new("/src/lib.rs")), "/src/lib.rs");
    }

    #[test]
    fn ignores_target_and_vcs_dirs() {
        let root = Path::new("/proj");
        assert!(is_ignored(root, Path::new("/proj/target/debug/x.rs")));
        assert!(is_ignored(root, Path::new("/proj/.git/hooks/x.rs")));
        assert!(!is_ignored(root, Path::new("/proj/src/lib.rs")));
        assert!(!is_ignored(root, Path::new("/proj")));
    }
}
