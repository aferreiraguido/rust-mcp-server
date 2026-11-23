use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use triomphe::Arc;

use ra_ap_cfg::CfgOptions;
use ra_ap_ide::{Analysis, AnalysisHost, Edition};
use ra_ap_ide_db::{
    base_db::{
        target::TargetLoadError, CrateGraphBuilder, CrateOrigin, CrateWorkspaceData, Env, FileSet,
        SourceRoot, VfsPath,
    },
    ChangeWithProcMacros, FileId, RootDatabase,
};
use ra_ap_vfs::AbsPathBuf;
use walkdir::WalkDir;

use crate::analysis::path_utils::{display_path, is_ignored};

#[derive(Debug, Error)]
pub enum HostError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ra_ap_ide error: {0}")]
    Analysis(String),
}

/// Converts an OS path into an `AbsPathBuf`.
///
/// `AbsPathBuf` requires an **OS-absolute** path. On Windows a rooted path
/// such as `\src\lib.rs` (what `PathBuf::from("/").join("src/lib.rs")`
/// produces) has no drive prefix and is NOT absolute, so the `assert_*`
/// constructors panic. Panics inside `spawn_blocking` surface as an opaque
/// "join error: task panicked", so we always use the fallible constructor and
/// return a readable `HostError` instead.
fn to_abs_path(path: &Path) -> Result<AbsPathBuf, HostError> {
    let Some(utf8) = path.to_str() else {
        return Err(HostError::Analysis(format!(
            "path is not valid UTF-8: {}",
            path.display()
        )));
    };
    AbsPathBuf::try_from(utf8).map_err(|p| {
        HostError::Analysis(format!(
            "path is not OS-absolute: {} (on Windows, rooted paths such as \
             \\src\\lib.rs have no drive prefix; derive VFS paths from the \
             canonicalized filesystem path)",
            p
        ))
    })
}

fn to_vfs_path(path: &Path) -> Result<VfsPath, HostError> {
    Ok(VfsPath::from(to_abs_path(path)?))
}

/// Wrapper around `ra_ap_ide::AnalysisHost` that loads a Rust project
/// from disk into the rust-analyzer semantic engine.
pub struct ProjectAnalysisHost {
    pub root: std::path::PathBuf,
    /// Display path (`/src/lib.rs`) -> FileId mapping (used by SymbolResolver).
    pub files: HashMap<String, FileId>,
    host: AnalysisHost,
}

impl ProjectAnalysisHost {
    /// Loads a Rust project into the ra_ap_* ASG.
    pub fn load(root: &Path) -> Result<Self, HostError> {
        let root = std::fs::canonicalize(root)?;

        // 1. Collect all .rs files, skipping build artifacts (`target/`) and
        //    VCS metadata. Otherwise generated sources are indexed as part of
        //    the project (duplicated symbols, wrong crate root).
        let mut rs_files: Vec<std::path::PathBuf> = Vec::new();
        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|e| !e.file_type().is_dir() || !is_ignored(&root, e.path()))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
        {
            rs_files.push(entry.path().to_path_buf());
        }

        // 2. Build VFS: assign FileIds, build FileSet
        let mut file_set = FileSet::default();
        let mut files: HashMap<String, FileId> = HashMap::new();
        let mut contents: Vec<(FileId, String)> = Vec::new();
        let mut vfs = ra_ap_vfs::Vfs::default();

        for abs_path in &rs_files {
            let rel_path = abs_path.strip_prefix(&root).unwrap_or(abs_path);
            // VFS identity: the real absolute path (valid on every OS).
            let vfs_path = to_vfs_path(abs_path)?;
            let content = std::fs::read_to_string(abs_path)?;

            vfs.set_file_contents(vfs_path.clone(), Some(content.as_bytes().to_vec()));
            let (file_id, _excluded) = vfs
                .file_id(&vfs_path)
                .ok_or_else(|| HostError::Analysis(format!("no file_id for {}", vfs_path)))?;

            file_set.insert(file_id, vfs_path.clone());
            // Display key: project-relative and POSIX-style, so the Markdown
            // output is identical on Windows and Unix.
            files.insert(display_path(rel_path), file_id);
            contents.push((file_id, content));
        }

        // 3. Create source root
        let source_root = SourceRoot::new_local(file_set);

        // 4. Build the crate graph for the root crate.
        //    The crate root must be a real absolute path: `root.join(...)`
        //    (root is already canonicalized), never a synthetic `/src/lib.rs`.
        let mut crate_graph = CrateGraphBuilder::default();
        let root_file = ["src/lib.rs", "src/main.rs"]
            .iter()
            .map(|rel| root.join(rel))
            .find(|p| p.exists())
            .or_else(|| rs_files.first().cloned());

        let root_file_id = match root_file {
            Some(path) => vfs.file_id(&to_vfs_path(&path)?).map(|(fid, _)| fid),
            None => None,
        };

        if let Some(rfid) = root_file_id {
            crate_graph.add_crate_root(
                rfid,
                Edition::Edition2021,
                None,
                None,
                CfgOptions::default(),
                None,
                Env::default(),
                CrateOrigin::Local {
                    repo: None,
                    name: None,
                },
                vec![],
                false,
                Arc::new(to_abs_path(&root.join("dummy"))?),
                Arc::new(CrateWorkspaceData {
                    target: Err(TargetLoadError::from("target not loaded")),
                    toolchain: None,
                }),
            );
        }

        // 5. Create the database and apply changes
        let mut db = RootDatabase::default();
        let mut change = ChangeWithProcMacros::default();
        change.set_roots(vec![source_root]);
        change.set_crate_graph(crate_graph);
        for (file_id, content) in &contents {
            change.change_file(*file_id, Some(content.clone()));
        }
        change.apply(&mut db);

        // 6. Create AnalysisHost
        let host = AnalysisHost::with_database(db);

        Ok(ProjectAnalysisHost { root, files, host })
    }

    /// Returns a snapshot of the analysis.
    pub fn analysis(&self) -> Analysis {
        self.host.analysis()
    }
}
