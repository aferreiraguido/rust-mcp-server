use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use triomphe::Arc;

use ra_ap_cfg::CfgOptions;
use ra_ap_vfs::AbsPathBuf;
use ra_ap_ide_db::{
    base_db::{
        CrateGraphBuilder, CrateOrigin, CrateWorkspaceData, Env, FileSet, SourceRoot, VfsPath,
        target::TargetLoadError,
    },
    ChangeWithProcMacros,
    FileId,
    RootDatabase,
};
use ra_ap_ide::{Analysis, AnalysisHost, Edition};
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum HostError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ra_ap_ide error: {0}")]
    Analysis(String),
}

/// Wrapper around `ra_ap_ide::AnalysisHost` that loads a Rust project
/// from disk into the rust-analyzer semantic engine.
pub struct ProjectAnalysisHost {
    pub root: std::path::PathBuf,
    /// Virtual path -> FileId mapping (used by SymbolResolver).
    pub files: HashMap<String, FileId>,
    host: AnalysisHost,
}

impl ProjectAnalysisHost {
    /// Loads a Rust project into the ra_ap_* ASG.
    pub fn load(root: &Path) -> Result<Self, HostError> {
        let root = std::fs::canonicalize(root)?;

        // 1. Collect all .rs files
        let mut rs_files: Vec<std::path::PathBuf> = Vec::new();
        for entry in WalkDir::new(&root)
            .into_iter()
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
            let vfs_path = VfsPath::from(AbsPathBuf::assert_utf8(
                std::path::PathBuf::from("/").join(rel_path),
            ));
            let content = std::fs::read_to_string(abs_path)?;

            vfs.set_file_contents(vfs_path.clone(), Some(content.as_bytes().to_vec()));
            let (file_id, _excluded) = vfs
                .file_id(&vfs_path)
                .ok_or_else(|| HostError::Analysis(format!("no file_id for {}", vfs_path)))?;

            file_set.insert(file_id, vfs_path.clone());
            files.insert(format!("{}", vfs_path), file_id);
            contents.push((file_id, content));
        }

        // 3. Create source root
        let source_root = SourceRoot::new_local(file_set);

        // 4. Build the crate graph for the root crate
        let mut crate_graph = CrateGraphBuilder::default();
        let lib_rs = root.join("src").join("lib.rs");
        let main_rs = root.join("src").join("main.rs");
        let root_file_id = if lib_rs.exists() {
            let vp = VfsPath::from(AbsPathBuf::assert_utf8(
                std::path::PathBuf::from("/src/lib.rs"),
            ));
            vfs.file_id(&vp).map(|(fid, _)| fid)
        } else if main_rs.exists() {
            let vp = VfsPath::from(AbsPathBuf::assert_utf8(
                std::path::PathBuf::from("/src/main.rs"),
            ));
            vfs.file_id(&vp).map(|(fid, _)| fid)
        } else if let Some((fid, _)) = rs_files.first().and_then(|p| {
            let rel = p.strip_prefix(&root).unwrap_or(p);
            let vp = VfsPath::from(AbsPathBuf::assert_utf8(
                std::path::PathBuf::from("/").join(rel),
            ));
            vfs.file_id(&vp)
        }) {
            Some(fid)
        } else {
            None
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
                Arc::new(AbsPathBuf::assert_utf8(root.join("dummy"))),
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

        Ok(ProjectAnalysisHost {
            root,
            files,
            host,
        })
    }

    /// Returns a snapshot of the analysis.
    pub fn analysis(&self) -> Analysis {
        self.host.analysis()
    }
}
