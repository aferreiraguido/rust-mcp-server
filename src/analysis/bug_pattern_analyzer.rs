//! `BugPatternAnalyzer` — 5 bug-hunt sweeps for Rust code.
//!
//! Mirror of `com.softtek.mcp.analysis.BugPatternAnalyzer` (cobol-mcp-server).
//!
//! 4 sweeps are **static** (pure `syn`, no ASG):
//! - `find_unsafe_blocks`     — flag `unsafe { ... }` blocks
//! - `find_unwrap_patterns`   — flag `.unwrap()`, `.expect()` chains
//! - `find_unchecked_patterns` — flag `mem::transmute`, raw pointer ops
//! - `find_panic_sites`       — flag `panic!`, `unreachable!`, `todo!`
//!
//! 1 sweep is **semantic** (needs `ra_ap_*`):
//! - `find_dead_code`         — unreferenced items (via `find_all_refs`)
//!
//! Each sweep returns `Vec<Finding>` with category / severity / code /
//! location / evidence.

use crate::analysis::path_utils::display_rel_path;
use crate::analysis::rust_analyzer_utils::RustAnalyzerUtils;
use crate::model::ProjectEntry;
use crate::model::{Finding, FindingCategory, FindingSeverity};

use ra_ap_syntax::ast::HasName as _;
use ra_ap_syntax::AstNode as _;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BugPatternError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("syntax error: {0}")]
    Syntax(String),
    #[error("analysis error: {0}")]
    Analysis(String),
}

pub struct BugPatternAnalyzer;

impl BugPatternAnalyzer {
    /// Parses all `.rs` files in the project with `ra_ap_syntax` and returns
    /// them as `(PathBuf, SourceFile, String)` (path, tree, source content).
    /// Reused by the sweeps.
    pub fn parse_all_sources(
        entry: &ProjectEntry,
    ) -> Result<Vec<(std::path::PathBuf, ra_ap_syntax::SourceFile, String)>, BugPatternError> {
        let mut out = Vec::new();
        for path in RustAnalyzerUtils::rust_files(entry) {
            let content = std::fs::read_to_string(&path)?;
            let parse = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::CURRENT);
            out.push((path, parse.tree(), content));
        }
        Ok(out)
    }

    /// Identical to `find_unsafe_blocks` but accepts pre-parsed sources
    /// (useful when running multiple sweeps to avoid re-parsing).
    pub fn find_unsafe_blocks(
        entry: &ProjectEntry,
    ) -> Result<Vec<Finding>, BugPatternError> {
        Self::find_unsafe_blocks_in(entry, &Self::parse_all_sources(entry)?)
    }

    /// Like `find_unsafe_blocks` but with pre-parsed sources.
    pub fn find_unsafe_blocks_in(
        entry: &ProjectEntry,
        sources: &[(std::path::PathBuf, ra_ap_syntax::SourceFile, String)],
    ) -> Result<Vec<Finding>, BugPatternError> {
        use ra_ap_syntax::ast::{BlockExpr, Fn, Impl, Trait};
        let mut findings = Vec::new();
        for (path, source, content) in sources {
            let rel = display_rel_path(path.strip_prefix(&entry.path).unwrap_or(path));

            // BlockExpr with unsafe_token → `unsafe { ... }`
            crate::analysis::ra_walker::walk_typed::<BlockExpr, _>(
                source.syntax(),
                |block| {
                    if block.unsafe_token().is_some() {
                        findings.push(Finding {
                            category: FindingCategory::Unsafe,
                            severity: FindingSeverity::Medium,
                            code: "UNSAFE_BLOCK".to_string(),
                            message: "unsafe block — review memory safety invariants".to_string(),
                            file: rel.clone(),
                            line: line_of_node(block.syntax(), content),
                            evidence: Some("`unsafe { ... }` block".to_string()),
                        });
                    }
                },
            );
            // Fn with unsafe_token → `unsafe fn`
            crate::analysis::ra_walker::walk_typed::<Fn, _>(
                source.syntax(),
                |f| {
                    if f.unsafe_token().is_some() {
                        let name = f
                            .name()
                            .map(|n| n.text().to_string())
                            .unwrap_or_else(|| "<anon>".to_string());
                        findings.push(Finding {
                            category: FindingCategory::Unsafe,
                            severity: FindingSeverity::Medium,
                            code: "UNSAFE_FN".to_string(),
                            message: format!("`unsafe fn {}` — review safety invariants", name),
                            file: rel.clone(),
                            line: line_of_node(f.syntax(), content),
                            evidence: None,
                        });
                    }
                },
            );
            // Impl with unsafe_token → `unsafe impl`
            crate::analysis::ra_walker::walk_typed::<Impl, _>(
                source.syntax(),
                |i| {
                    if i.unsafe_token().is_some() {
                        findings.push(Finding {
                            category: FindingCategory::Unsafe,
                            severity: FindingSeverity::Medium,
                            code: "UNSAFE_IMPL".to_string(),
                            message: "`unsafe impl` — review safety invariants".to_string(),
                            file: rel.clone(),
                            line: line_of_node(i.syntax(), content),
                            evidence: None,
                        });
                    }
                },
            );
            // Trait with unsafe_token → `unsafe trait`
            crate::analysis::ra_walker::walk_typed::<Trait, _>(
                source.syntax(),
                |t| {
                    if t.unsafe_token().is_some() {
                        let name = t
                            .name()
                            .map(|n| n.text().to_string())
                            .unwrap_or_else(|| "<anon>".to_string());
                        findings.push(Finding {
                            category: FindingCategory::Unsafe,
                            severity: FindingSeverity::Medium,
                            code: "UNSAFE_TRAIT".to_string(),
                            message: format!("`unsafe trait {}`", name),
                            file: rel.clone(),
                            line: line_of_node(t.syntax(), content),
                            evidence: None,
                        });
                    }
                },
            );
        }
        Ok(findings)
    }

    // ─── Sweep 2: `.unwrap()` / `.expect()` chains ──────────────────

    pub fn find_unwrap_patterns(entry: &ProjectEntry) -> Result<Vec<Finding>, BugPatternError> {
        Self::find_unwrap_patterns_in(entry, &Self::parse_all_sources(entry)?)
    }

    /// Like `find_unwrap_patterns` but with pre-parsed sources.
    pub fn find_unwrap_patterns_in(
        entry: &ProjectEntry,
        sources: &[(std::path::PathBuf, ra_ap_syntax::SourceFile, String)],
    ) -> Result<Vec<Finding>, BugPatternError> {
        use ra_ap_syntax::ast::MethodCallExpr;
        let mut findings = Vec::new();
        for (path, source, content) in sources {
            let rel = display_rel_path(path.strip_prefix(&entry.path).unwrap_or(path));
            crate::analysis::ra_walker::walk_typed::<MethodCallExpr, _>(
                source.syntax(),
                |call| {
                    let method_name = call
                        .name_ref()
                        .and_then(|m| m.ident_token())
                        .map(|t| t.text().to_string());
                    if let Some(name) = method_name {
                        if name == "unwrap" || name == "expect" {
                            findings.push(Finding {
                                category: FindingCategory::UnwrapRisk,
                                severity: FindingSeverity::Low,
                                code: "UNWRAP_OR_EXPECT".to_string(),
                                message: format!("`.{}()` can panic at runtime", name),
                                file: rel.clone(),
                                line: line_of_node(call.syntax(), content),
                                evidence: None,
                            });
                        }
                    }
                },
            );
        }
        Ok(findings)
    }

    // ─── Sweep 3: `mem::transmute` / raw pointer ops ──────────────

    pub fn find_unchecked_patterns(entry: &ProjectEntry) -> Result<Vec<Finding>, BugPatternError> {
        Self::find_unchecked_patterns_in(entry, &Self::parse_all_sources(entry)?)
    }

    /// Like `find_unchecked_patterns` but with pre-parsed sources.
    pub fn find_unchecked_patterns_in(
        entry: &ProjectEntry,
        sources: &[(std::path::PathBuf, ra_ap_syntax::SourceFile, String)],
    ) -> Result<Vec<Finding>, BugPatternError> {
        use ra_ap_syntax::ast::{CallExpr, MacroCall};
        const UNCHECKED_PATHS: &[&str] = &[
            "mem::transmute",
            "mem::forget",
            "mem::zeroed",
            "std::ptr::read",
            "std::ptr::write",
            "std::ptr::read_unaligned",
            "std::ptr::write_unaligned",
            "std::ptr::copy",
            "std::ptr::copy_nonoverlapping",
        ];
        let mut findings = Vec::new();
        for (path, source, content) in sources {
            let rel = display_rel_path(path.strip_prefix(&entry.path).unwrap_or(path));
            // Macros like `mem::transmute!(...)` (not common but possible)
            crate::analysis::ra_walker::walk_typed::<MacroCall, _>(
                source.syntax(),
                |m| {
                    let path_str = m.path().as_ref().map(path_to_string).unwrap_or_default();
                    if UNCHECKED_PATHS.contains(&path_str.as_str()) {
                        findings.push(Finding {
                            category: FindingCategory::Unsafe,
                            severity: FindingSeverity::High,
                            code: "UNCHECKED_OP".to_string(),
                            message: format!(
                                "`{}` is unsafe — review carefully",
                                path_str
                            ),
                            file: rel.clone(),
                            line: line_of_node(m.syntax(), content),
                            evidence: Some(format!("`{}!`", path_str)),
                        });
                    }
                },
            );
            // Function calls like `mem::transmute(...)` (most common)
            crate::analysis::ra_walker::walk_typed::<CallExpr, _>(
                source.syntax(),
                |c| {
                    let path_str = expr_call_path(c);
                    if UNCHECKED_PATHS.contains(&path_str.as_str()) {
                        findings.push(Finding {
                            category: FindingCategory::Unsafe,
                            severity: FindingSeverity::High,
                            code: "UNCHECKED_FN".to_string(),
                            message: format!("`{}()` is an unsafe fn", path_str),
                            file: rel.clone(),
                            line: line_of_node(c.syntax(), content),
                            evidence: None,
                        });
                    }
                },
            );
        }
        Ok(findings)
    }

    // ─── Sweep 4: `panic!` / `unreachable!` / `todo!` / `unimplemented!` ─

    pub fn find_panic_sites(entry: &ProjectEntry) -> Result<Vec<Finding>, BugPatternError> {
        Self::find_panic_sites_in(entry, &Self::parse_all_sources(entry)?)
    }

    /// Like `find_panic_sites` but with pre-parsed sources.
    pub fn find_panic_sites_in(
        entry: &ProjectEntry,
        sources: &[(std::path::PathBuf, ra_ap_syntax::SourceFile, String)],
    ) -> Result<Vec<Finding>, BugPatternError> {
        use ra_ap_syntax::ast::MacroCall;
        const PANIC_MACROS: &[&str] = &["panic", "unreachable", "todo", "unimplemented"];
        let mut findings = Vec::new();
        for (path, source, content) in sources {
            let rel = display_rel_path(path.strip_prefix(&entry.path).unwrap_or(path));
            crate::analysis::ra_walker::walk_typed::<MacroCall, _>(
                source.syntax(),
                |m| {
                    let macro_name = match m.path() {
                        Some(p) => path_to_string(&p),
                        None => return,
                    };
                    if !PANIC_MACROS.contains(&macro_name.as_str()) {
                        return;
                    }
                    let severity = match macro_name.as_str() {
                        "panic" => FindingSeverity::Medium,
                        "unreachable" | "todo" | "unimplemented" => FindingSeverity::High,
                        _ => FindingSeverity::Low,
                    };
                    let preview = m
                        .syntax()
                        .text()
                        .to_string()
                        .chars()
                        .take(40)
                        .collect::<String>();
                    findings.push(Finding {
                        category: FindingCategory::PanicSite,
                        severity,
                        code: format!("{}_SITE", macro_name.to_uppercase()),
                        message: format!("`{}!` is a panic site", macro_name),
                        file: rel.clone(),
                        line: line_of_node(m.syntax(), content),
                        evidence: Some(format!("`{}!({})`", macro_name, preview)),
                    });
                },
            );
        }
        Ok(findings)
    }

    // ─── Sweep 6: unused dependencies (textual heuristic) ────────────

    /// Detects dependencies declared in `[dependencies]` (and
    /// `[dev-dependencies]`, `[build-dependencies]`) that are never
    /// referenced in the project's `.rs` code.
    ///
    /// **Heuristic** implementation (whole-word text search):
    /// - Iterates over each dependency declared in `Cargo.toml`.
    /// - For each dep, counts occurrences of its name as a
    ///   whole word in all `.rs` files in the project.
    /// - If 0 → marks as unused.
    ///
    /// **Limitations** (vs `cargo tree --no-default-features` or
    /// `cargo machete`):
    /// - False positives: deps referenced only from macros (rare
    ///   in Rust but possible) or from `build.rs` in unscanned
    ///   paths.
    /// - Does not distinguish between normal/dev/build deps; reports all.
    ///
    /// For accurate unused dependency analysis, use
    /// `cargo install cargo-machete` (external).
    pub fn find_unused_dependencies(entry: &ProjectEntry) -> Result<Vec<Finding>, BugPatternError> {
        let manifest = RustAnalyzerUtils::cargo_manifest(entry)
            .map_err(|e| BugPatternError::Analysis(format!("manifest: {}", e)))?;
        let mut findings = Vec::new();
        let dep_files = RustAnalyzerUtils::rust_files(entry);
        for (name, scope, _value) in manifest.direct_dependencies() {
            // Cargo dependency names use `-` (e.g. `async-trait`) while Rust
            // code refers to them with `_` (e.g. `async_trait`). Normalize
            // before the whole-word search to avoid false positives.
            let needle = name.replace('-', "_");
            let count = crate::analysis::text_utils::count_whole_word_matches(&dep_files, &needle);
            if count == 0 {
                let severity = match scope {
                    "normal" => FindingSeverity::Low,
                    "dev" | "build" => FindingSeverity::Info,
                    _ => FindingSeverity::Low,
                };
                findings.push(Finding {
                    category: FindingCategory::Structural,
                    severity,
                    code: "UNUSED_DEPENDENCY".to_string(),
                    message: format!(
                        "Dependency `{}` (scope: {}) is declared but never used",
                        name, scope
                    ),
                    file: "Cargo.toml".to_string(),
                    line: 0,
                    evidence: None,
                });
            }
        }
        Ok(findings)
    }

    // ─── Sweep 5: dead code (semantic, via ra_ap_*) ─────────────────

    /// Detects non-pub items without any semantic references in the
    /// project (no recursion or use from macros, etc.). Uses
    /// `SymbolResolver` + `Analysis::find_all_refs` (iter 4.5 semantic).
    ///
    /// **Dead code criteria**:
    /// - For each symbol indexed by `SymbolResolver`:
    ///   - If `pub` → skipped (may be public API).
    ///   - If it has `#[test]`, `#[derive]`, `#[no_mangle]`, `#[cfg]`
    ///     → skipped.
    ///   - Build a `FilePosition` with the `file_id` and the identifier
    ///     offset (`name_range.start()`).
    ///   - Call `analysis.find_all_refs(pos, &config)`.
    ///   - If the result is `None` or all ref lists are empty
    ///     → 0 references → dead code.
    ///
    /// This handles recursion cases (an item that calls itself
    /// has at least 1 reference) and items used only from
    /// macros (which `ra_ap_*` resolves correctly).
    ///
    /// NOTE: requires a pre-loaded `ProjectAnalysisHost` (via
    /// `ProjectAnalysisHost::load` inside `spawn_blocking`).
    pub fn find_dead_code(
        host: &crate::analysis::analysis_host::ProjectAnalysisHost,
    ) -> Result<Vec<Finding>, BugPatternError> {
        use crate::analysis::symbol_resolver::SymbolResolver;
        use ra_ap_ide::{FilePosition, FindAllRefsConfig, RaFixtureConfig};

        let analysis = host.analysis();
        let resolver = SymbolResolver::build(&analysis, &host.files);

        // 2. For each symbol, query find_all_refs.
        let config = FindAllRefsConfig {
            search_scope: None,
            ra_fixture: RaFixtureConfig::<'_>::default(),
            exclude_imports: true,
            exclude_tests: false,
        };
        let mut findings = Vec::new();
        for (name, locs) in resolver.all_symbols() {
            let Some(loc) = locs.first() else { continue };
            // Filter by visibility and implicit references.
            if is_pub_loc(loc) {
                continue;
            }
            // Methods inside `impl Trait for Type` are invoked via trait
            // objects; `find_all_refs` often does not resolve those callers,
            // so they would be false positives. Skip them.
            if loc.is_trait_method {
                continue;
            }
            // If it has `#[test]`, `#[derive]`, `#[no_mangle]`, or `#[cfg]`,
            // there are implicit references outside the code visible to the
            // engine. It is not considered dead code.
            if loc.implicit_references.is_implicitly_referenced() {
                continue;
            }
            // Build FilePosition and query.
            let pos = FilePosition {
                file_id: loc.file_id,
                offset: loc.name_range.start(),
            };
            let result = analysis.find_all_refs(pos, &config).ok().flatten();
            let total_refs = match result {
                Some(refs) => refs
                    .iter()
                    .map(|r| r.references.values().map(|v| v.len()).sum::<usize>())
                    .sum::<usize>(),
                None => 0,
            };
            // 0 references = not even self-references (recursion) = dead.
            if total_refs == 0 {
                // Find the virtual path to report.
                let virt_path = host
                    .files
                    .iter()
                    .find(|(_, fid)| **fid == loc.file_id)
                    .map(|(p, _)| p.clone())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let evidence = if loc.implicit_references.allow {
                    // Rarely happens, but we document it.
                    Some("#[allow(...)] only — no real ref".to_string())
                } else {
                    None
                };
                findings.push(Finding {
                    category: FindingCategory::DeadCode,
                    severity: FindingSeverity::Low,
                    code: "DEAD_CODE".to_string(),
                    message: format!(
                        "`{}` is never referenced (semantic check via ra_ap_*)",
                        name
                    ),
                    file: virt_path,
                    line: 0,
                    evidence,
                });
            }
        }

        Ok(findings)
    }
}

/// Helpers for `find_dead_code` (operate on `SymbolLocation.is_pub`
/// and kind, not on `syn::Item`).
fn is_pub_loc(loc: &crate::analysis::symbol_resolver::SymbolLocation) -> bool {
    loc.is_pub
}

// ─── Helpers ───────────────────────────────────────────────────────

/// Resolves the 1-based line number from a `SyntaxNode` and the source
/// content of the file. Uses `TextRange::start()` (byte offset) and
/// counts newlines.
fn line_of_node(node: &ra_ap_syntax::SyntaxNode, content: &str) -> u32 {
    let offset: usize = node.text_range().start().into();
    content[..offset]
        .chars()
        .filter(|&c| c == '\n')
        .count() as u32
        + 1
}

/// Converts an `ast::Path` (from `MacroCall::path()`) to string.
/// E.g.: `mem::transmute` → `"mem::transmute"`.
fn path_to_string(path: &ra_ap_syntax::ast::Path) -> String {
    path.segments()
        .map(|s| {
            s.name_ref()
                .map(|n| n.ident_token().map(|t| t.text().to_string()).unwrap_or_default())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("::")
}

/// Extracts the path from an `ast::CallExpr` (the called function).
/// E.g.: `mem::transmute(x)` → `"mem::transmute"`.
fn expr_call_path(call: &ra_ap_syntax::ast::CallExpr) -> String {
    let path = match call.expr() {
        Some(ra_ap_syntax::ast::Expr::PathExpr(p)) => p.path(),
        _ => None,
    };
    let Some(path) = path else {
        return String::new();
    };
    path.segments()
        .map(|s| {
            s.name_ref()
                .map(|n| n.ident_token().map(|t| t.text().to_string()).unwrap_or_default())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("::")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProjectEntry;
    use crate::model::ProjectKind;
    use chrono::{Duration, Utc};
    use std::path::PathBuf;

    fn sample_entry() -> ProjectEntry {
        ProjectEntry {
            name: "sample".to_string(),
            alias: None,
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_crate"),
            kind: ProjectKind::SingleCrate,
            expiry: Utc::now() + Duration::minutes(10),
            loaded_at: Utc::now(),
            member_crates: vec![],
            cached_manifest: std::sync::OnceLock::new(),
        }
    }

    #[test]
    fn smoke_unsafe() {
        let entry = sample_entry();
        let findings = BugPatternAnalyzer::find_unsafe_blocks(&entry).unwrap();
        // The sample has no unsafe — should return 0.
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn smoke_unwrap() {
        let entry = sample_entry();
        let findings = BugPatternAnalyzer::find_unwrap_patterns(&entry).unwrap();
        // The sample has no unwrap.
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn smoke_panic() {
        let entry = sample_entry();
        let findings = BugPatternAnalyzer::find_panic_sites(&entry).unwrap();
        // The sample has no panic!.
        assert_eq!(findings.len(), 0);
    }
}
