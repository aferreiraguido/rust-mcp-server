//! Common helper for tools that use the ra_ap_* semantic ASG.

use crate::analysis::analysis_host::ProjectAnalysisHost;
use crate::analysis::symbol_resolver::{ResolverError, SymbolResolver};
use ra_ap_ide::NavigationTarget;
use std::collections::HashMap;

pub fn build_resolver_for_entry(
    path: &std::path::Path,
) -> Result<(ProjectAnalysisHost, SymbolResolver), ResolverError> {
    let host = ProjectAnalysisHost::load(path)
        .map_err(|e| ResolverError::Analysis(format!("could not load ASG: {}", e)))?;
    let analysis = host.analysis();
    let resolver = SymbolResolver::build(&analysis, &host.files);
    Ok((host, resolver))
}

pub fn format_targets(
    title: &str,
    targets: &[NavigationTarget],
    files: &HashMap<String, ra_ap_ide::FileId>,
) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {} ({})\n\n", title, targets.len()));
    if targets.is_empty() {
        md.push_str("_No results._\n");
        return md;
    }
    md.push_str("| File | Kind | Name | Range |\n|------|------|------|-------|\n");
    for t in targets {
        let path = file_id_to_path(t.file_id, files);
        let kind = t
            .kind
            .map(|k| format!("{:?}", k))
            .unwrap_or_else(|| "-".to_string());
        let name = t.name.to_string();
        let range = format!(
            "{}..{}",
            u32::from(t.full_range.start()),
            u32::from(t.full_range.end())
        );
        md.push_str(&format!(
            "| `{}` | {} | `{}` | {} |\n",
            path, kind, name, range
        ));
    }
    md
}

/// Formats `Vec<(FileId, TextRange)>` (what `find_all_refs` returns
/// grouped by file).
pub fn format_references(
    title: &str,
    refs: &[(ra_ap_ide::FileId, ra_ap_syntax::TextRange)],
    files: &HashMap<String, ra_ap_ide::FileId>,
) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {} ({})\n\n", title, refs.len()));
    if refs.is_empty() {
        md.push_str("_No references found._\n");
        return md;
    }
    md.push_str("| File | Range |\n|------|-------|\n");
    for (file_id, range) in refs {
        let path = file_id_to_path(*file_id, files);
        md.push_str(&format!(
            "| `{}` | {}..{} |\n",
            path,
            u32::from(range.start()),
            u32::from(range.end())
        ));
    }
    md
}

pub fn file_id_to_path(
    file_id: ra_ap_ide::FileId,
    files: &HashMap<String, ra_ap_ide::FileId>,
) -> String {
    for (path, id) in files {
        if *id == file_id {
            return path.clone();
        }
    }
    format!("<file_id:{}>", file_id.index())
}
