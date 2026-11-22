//! Stable facade for static analysis.
//!
//! Mirror of `com.softtek.mcp.ProLeapAnalyzerUtils` (cobol-mcp-server).
//! Tools consume this facade, not the `analysis/*` modules directly.
//!
//! **Status (iter 2)**: implements listings, inspections, and references
//! using `syn` + `toml` (without `ra_ap_*`). Queries that require
//! semantic type resolution (`find_implementations`, `get_type_hierarchy`
//! with generic types) arrive in iter 3 with `ra_ap_*`.
//!
//! All functions:
#![allow(clippy::result_large_err)]

use crate::analysis::cargo_manifest::CargoManifest;
use crate::analysis::path_utils::{display_rel_path, is_ignored};
use crate::analysis::syntax::{ItemKind, ParsedFile};
use crate::model::ProjectEntry;
use serde::Serialize;
use std::path::{Path, PathBuf};
use syn::Item;
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("project not found: {0}")]
    NotFound(String),
    #[error("syntax error in {path}: {message}")]
    Syntax { path: String, message: String },
    #[error("manifest error: {0}")]
    Manifest(String),
}

pub type AnalysisResult<T> = Result<T, AnalysisError>;

// ────────────────────────────────────────────────────────────────────────
// Records that tools return to the LLM in Markdown format.
// Mirror of the `*Record` types in `ProLeapAnalyzerUtils` (cobol-mcp-server).
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct FunctionInfo {
    pub name: String,
    pub signature: String,
    pub file: String,
    pub line: u32,
    pub is_pub: bool,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub is_const: bool,
    pub visibility: String, // "pub", "pub(crate)", ""
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub kind: String, // "struct" | "tuple" | "unit"
    pub fields: Vec<FieldInfo>,
    pub derives: Vec<String>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldInfo {
    pub name: String,
    pub ty: String,
    pub visibility: String,
    pub is_pub: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnumInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub variants: Vec<EnumVariant>,
    pub derives: Vec<String>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<FieldInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraitInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub methods: Vec<String>, // names only
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImplInfo {
    pub self_type: String,
    pub trait_name: Option<String>,
    pub file: String,
    pub line: u32,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub is_inline: bool,
    pub visibility: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MacroInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub kind: String, // "macro_rules" | "proc_macro" (heuristic)
    pub args: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileSummary {
    pub file: String,
    pub function_count: u32,
    pub struct_count: u32,
    pub enum_count: u32,
    pub trait_count: u32,
    pub impl_count: u32,
    pub module_count: u32,
    pub macro_count: u32,
    pub total_lines: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgramSummary {
    pub crate_name: Option<String>,
    pub crate_version: Option<String>,
    pub edition: Option<String>,
    pub file_count: u32,
    pub function_count: u32,
    pub struct_count: u32,
    pub enum_count: u32,
    pub trait_count: u32,
    pub impl_count: u32,
    pub module_count: u32,
    pub macro_count: u32,
    pub total_lines: u32,
    pub direct_dependency_count: u32,
    pub feature_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReferenceHit {
    pub file: String,
    pub line: u32,
    pub context: String, // the full line
}

// ────────────────────────────────────────────────────────────────────────
// Internal utilities.
// ────────────────────────────────────────────────────────────────────────

fn line_of_item(path: &Path, item: &Item) -> u32 {
    use syn::spanned::Spanned;
    let _ = path;
    item.span().start().line as u32
}

fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let mut lines = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    lines.push(s.value().trim().to_string());
                }
            }
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn extract_derives(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut out = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("derive") {
            if let syn::Meta::List(list) = &attr.meta {
                let s = list.tokens.to_string();
                for ident in s.split(',') {
                    let ident = ident.trim();
                    if !ident.is_empty() {
                        out.push(ident.to_string());
                    }
                }
            }
        }
    }
    out
}

// ────────────────────────────────────────────────────────────────────────
// Public facade.
// ────────────────────────────────────────────────────────────────────────

pub struct RustAnalyzerUtils;

impl RustAnalyzerUtils {
    // ─── Cargo manifest ───────────────────────────────────────────────

    /// Reads the project's Cargo.toml, using the cache in `entry.cached_manifest`.
    pub fn cargo_manifest(entry: &ProjectEntry) -> AnalysisResult<&CargoManifest> {
        // First check if it's already cached.
        if let Some(manifest) = entry.cached_manifest.get() {
            return Ok(manifest);
        }
        // If not, load and cache it.
        let path = entry.path.join("Cargo.toml");
        let manifest = CargoManifest::load(&path)
            .map_err(|e| AnalysisError::Manifest(format!("{:?}", e)))?;
        // `set` returns Err if already initialized (race). In that case
        // use the one that won the race.
        let manifest = entry.cached_manifest.get_or_init(|| manifest);
        Ok(manifest)
    }

    // ─── File enumeration ─────────────────────────────────────────────

    /// Lists all `.rs` files in the project.
    pub fn rust_files(entry: &ProjectEntry) -> Vec<PathBuf> {
        WalkDir::new(&entry.path)
            .into_iter()
            .filter_entry(|e| !e.file_type().is_dir() || !is_ignored(&entry.path, e.path()))
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
            .map(|e| e.into_path())
            .collect()
    }

    /// Parses all `.rs` files in the project.
    /// Errors are reported in the result (not silenced).
    pub fn parse_all(entry: &ProjectEntry) -> AnalysisResult<Vec<ParsedFile>> {
        let mut out = Vec::new();
        for path in Self::rust_files(entry) {
            match ParsedFile::parse(&path) {
                Ok(p) => out.push(p),
                Err(syn_err) => {
                    return Err(AnalysisError::Syntax {
                        path: Self::rel_display(entry, &path),
                        message: format!("{:?}", syn_err),
                    })
                }
            }
        }
        Ok(out)
    }

    // ─── Function ─────────────────────────────────────────────────────

    pub fn list_functions(entry: &ProjectEntry) -> AnalysisResult<Vec<FunctionInfo>> {
        let parsed = Self::parse_all(entry)?;
        let mut out = Vec::new();
        for pf in &parsed {
            for item in pf.filter(ItemKind::Function) {
                if let Item::Fn(f) = item {
                    out.push(FunctionInfo {
                        name: f.sig.ident.to_string(),
                        signature: quote_signature(&f.sig),
                        file: Self::rel_display(entry, &pf.path),
                        line: line_of_item(&pf.path, item),
                        is_pub: matches!(f.vis, syn::Visibility::Public(_)),
                        is_async: f.sig.asyncness.is_some(),
                        is_unsafe: f.sig.unsafety.is_some(),
                        is_const: f.sig.constness.is_some(),
                        visibility: format_visibility(&f.vis),
                        doc_comment: extract_doc_comment(&f.attrs),
                    });
                }
            }
        }
        out.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    /// Returns the detail of a function: signature + body.
    pub fn inspect_function(
        entry: &ProjectEntry,
        function_name: &str,
    ) -> AnalysisResult<Option<(FunctionInfo, String)>> {
        let parsed = Self::parse_all(entry)?;
        for pf in &parsed {
            for item in pf.filter(ItemKind::Function) {
                if let Item::Fn(f) = item {
                    if f.sig.ident == function_name {
                        let info = FunctionInfo {
                            name: f.sig.ident.to_string(),
                            signature: quote_signature(&f.sig),
                            file: Self::rel_display(entry, &pf.path),
                            line: line_of_item(&pf.path, item),
                            is_pub: matches!(f.vis, syn::Visibility::Public(_)),
                            is_async: f.sig.asyncness.is_some(),
                            is_unsafe: f.sig.unsafety.is_some(),
                            is_const: f.sig.constness.is_some(),
                            visibility: format_visibility(&f.vis),
                            doc_comment: extract_doc_comment(&f.attrs),
                        };
                        // Re-reads the file to extract the body lines.
                        let body_text = extract_body_text(&pf.path, f);
                        return Ok(Some((info, body_text)));
                    }
                }
            }
        }
        Ok(None)
    }

    // ─── Struct ───────────────────────────────────────────────────────

    pub fn list_structs(entry: &ProjectEntry) -> AnalysisResult<Vec<StructInfo>> {
        let parsed = Self::parse_all(entry)?;
        let mut out = Vec::new();
        for pf in &parsed {
            for item in pf.filter(ItemKind::Struct) {
                if let Item::Struct(s) = item {
                    out.push(StructInfo {
                        name: s.ident.to_string(),
                        file: Self::rel_display(entry, &pf.path),
                        line: line_of_item(&pf.path, item),
                        kind: struct_kind(s),
                        fields: extract_struct_fields(s),
                        derives: extract_derives(&s.attrs),
                        doc_comment: extract_doc_comment(&s.attrs),
                    });
                }
            }
        }
        out.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    pub fn inspect_struct(
        entry: &ProjectEntry,
        struct_name: &str,
    ) -> AnalysisResult<Option<StructInfo>> {
        let parsed = Self::parse_all(entry)?;
        for pf in &parsed {
            for item in pf.filter(ItemKind::Struct) {
                if let Item::Struct(s) = item {
                    if s.ident == struct_name {
                        return Ok(Some(StructInfo {
                            name: s.ident.to_string(),
                            file: Self::rel_display(entry, &pf.path),
                            line: line_of_item(&pf.path, item),
                            kind: struct_kind(s),
                            fields: extract_struct_fields(s),
                            derives: extract_derives(&s.attrs),
                            doc_comment: extract_doc_comment(&s.attrs),
                        }));
                    }
                }
            }
        }
        Ok(None)
    }

    // ─── Enum ─────────────────────────────────────────────────────────

    pub fn list_enums(entry: &ProjectEntry) -> AnalysisResult<Vec<EnumInfo>> {
        let parsed = Self::parse_all(entry)?;
        let mut out = Vec::new();
        for pf in &parsed {
            for item in pf.filter(ItemKind::Enum) {
                if let Item::Enum(e) = item {
                    out.push(EnumInfo {
                        name: e.ident.to_string(),
                        file: Self::rel_display(entry, &pf.path),
                        line: line_of_item(&pf.path, item),
                        variants: e
                            .variants
                            .iter()
                            .map(|v| EnumVariant {
                                name: v.ident.to_string(),
                                fields: extract_variant_fields(v),
                            })
                            .collect(),
                        derives: extract_derives(&e.attrs),
                        doc_comment: extract_doc_comment(&e.attrs),
                    });
                }
            }
        }
        out.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    pub fn inspect_enum(entry: &ProjectEntry, enum_name: &str) -> AnalysisResult<Option<EnumInfo>> {
        let parsed = Self::parse_all(entry)?;
        for pf in &parsed {
            for item in pf.filter(ItemKind::Enum) {
                if let Item::Enum(e) = item {
                    if e.ident == enum_name {
                        return Ok(Some(EnumInfo {
                            name: e.ident.to_string(),
                            file: Self::rel_display(entry, &pf.path),
                            line: line_of_item(&pf.path, item),
                            variants: e
                                .variants
                                .iter()
                                .map(|v| EnumVariant {
                                    name: v.ident.to_string(),
                                    fields: extract_variant_fields(v),
                                })
                                .collect(),
                            derives: extract_derives(&e.attrs),
                            doc_comment: extract_doc_comment(&e.attrs),
                        }));
                    }
                }
            }
        }
        Ok(None)
    }

    // ─── Trait ────────────────────────────────────────────────────────

    pub fn list_traits(entry: &ProjectEntry) -> AnalysisResult<Vec<TraitInfo>> {
        let parsed = Self::parse_all(entry)?;
        let mut out = Vec::new();
        for pf in &parsed {
            for item in pf.filter(ItemKind::Trait) {
                if let Item::Trait(t) = item {
                    let methods: Vec<String> = t
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            syn::TraitItem::Fn(m) => Some(m.sig.ident.to_string()),
                            syn::TraitItem::Type(t) => Some(t.ident.to_string()),
                            syn::TraitItem::Const(c) => Some(c.ident.to_string()),
                            _ => None,
                        })
                        .collect();
                    out.push(TraitInfo {
                        name: t.ident.to_string(),
                        file: Self::rel_display(entry, &pf.path),
                        line: line_of_item(&pf.path, item),
                        methods,
                        doc_comment: extract_doc_comment(&t.attrs),
                    });
                }
            }
        }
        out.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    pub fn inspect_trait(
        entry: &ProjectEntry,
        trait_name: &str,
    ) -> AnalysisResult<Option<TraitInfo>> {
        let parsed = Self::parse_all(entry)?;
        for pf in &parsed {
            for item in pf.filter(ItemKind::Trait) {
                if let Item::Trait(t) = item {
                    if t.ident == trait_name {
                        let methods: Vec<String> = t
                            .items
                            .iter()
                            .filter_map(|it| match it {
                                syn::TraitItem::Fn(m) => Some(m.sig.ident.to_string()),
                                syn::TraitItem::Type(t) => Some(t.ident.to_string()),
                                syn::TraitItem::Const(c) => Some(c.ident.to_string()),
                                _ => None,
                            })
                            .collect();
                        return Ok(Some(TraitInfo {
                            name: t.ident.to_string(),
                            file: Self::rel_display(entry, &pf.path),
                            line: line_of_item(&pf.path, item),
                            methods,
                            doc_comment: extract_doc_comment(&t.attrs),
                        }));
                    }
                }
            }
        }
        Ok(None)
    }

    // ─── Impl ─────────────────────────────────────────────────────────

    pub fn list_impls(entry: &ProjectEntry) -> AnalysisResult<Vec<ImplInfo>> {
        let parsed = Self::parse_all(entry)?;
        let mut out = Vec::new();
        for pf in &parsed {
            for item in pf.filter(ItemKind::Impl) {
                if let Item::Impl(i) = item {
                    let self_ty = match &*i.self_ty {
                        syn::Type::Path(p) => p
                            .path
                            .segments
                            .last()
                            .map(|s| s.ident.to_string())
                            .unwrap_or_default(),
                        _ => "<complex>".to_string(),
                    };
                    let trait_name = i.trait_.as_ref().map(|(_, t, _)| {
                        t.segments
                            .iter()
                            .map(|s| s.ident.to_string())
                            .collect::<Vec<_>>()
                            .join("::")
                    });
                    let methods: Vec<String> = i
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            syn::ImplItem::Fn(m) => Some(m.sig.ident.to_string()),
                            syn::ImplItem::Type(t) => Some(t.ident.to_string()),
                            syn::ImplItem::Const(c) => Some(c.ident.to_string()),
                            _ => None,
                        })
                        .collect();
                    out.push(ImplInfo {
                        self_type: self_ty,
                        trait_name,
                        file: Self::rel_display(entry, &pf.path),
                        line: line_of_item(&pf.path, item),
                        methods,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.file.cmp(&b.file).then(a.self_type.cmp(&b.self_type)));
        Ok(out)
    }

    // ─── Module ───────────────────────────────────────────────────────

    pub fn list_modules(entry: &ProjectEntry) -> AnalysisResult<Vec<ModuleInfo>> {
        let parsed = Self::parse_all(entry)?;
        let mut out = Vec::new();
        for pf in &parsed {
            for item in pf.filter(ItemKind::Module) {
                if let Item::Mod(m) = item {
                    out.push(ModuleInfo {
                        name: m.ident.to_string(),
                        file: Self::rel_display(entry, &pf.path),
                        line: line_of_item(&pf.path, item),
                        is_inline: m.content.is_some(),
                        visibility: format_visibility(&m.vis),
                    });
                }
            }
        }
        out.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    // ─── Macro ────────────────────────────────────────────────────────

    pub fn list_macros(entry: &ProjectEntry) -> AnalysisResult<Vec<MacroInfo>> {
        let parsed = Self::parse_all(entry)?;
        let mut out = Vec::new();
        for pf in &parsed {
            for item in &pf.items {
                if let Item::Macro(m) = item {
                    let name = m.ident.as_ref().map(|i| i.to_string()).unwrap_or_else(|| {
                        m.mac
                            .path
                            .segments
                            .first()
                            .map(|s| s.ident.to_string())
                            .unwrap_or_default()
                    });
                    let kind = if name.is_empty() {
                        "anonymous"
                    } else {
                        "macro_rules"
                    };
                    out.push(MacroInfo {
                        name,
                        file: Self::rel_display(entry, &pf.path),
                        line: line_of_item(&pf.path, item),
                        kind: kind.to_string(),
                        args: Some(m.mac.tokens.to_string()),
                    });
                }
            }
        }
        out.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    // ─── File summary ────────────────────────────────────────────────

    pub fn file_summary(entry: &ProjectEntry) -> AnalysisResult<Vec<FileSummary>> {
        let parsed = Self::parse_all(entry)?;
        let mut out = Vec::new();
        for pf in &parsed {
            let total_lines = std::fs::read_to_string(&pf.path)
                .map(|c| c.lines().count() as u32)
                .unwrap_or(0);
            out.push(FileSummary {
                file: Self::rel_display(entry, &pf.path),
                function_count: pf.filter(ItemKind::Function).count() as u32,
                struct_count: pf.filter(ItemKind::Struct).count() as u32,
                enum_count: pf.filter(ItemKind::Enum).count() as u32,
                trait_count: pf.filter(ItemKind::Trait).count() as u32,
                impl_count: pf.filter(ItemKind::Impl).count() as u32,
                module_count: pf.filter(ItemKind::Module).count() as u32,
                macro_count: pf
                    .items
                    .iter()
                    .filter(|i| matches!(i, Item::Macro(_)))
                    .count() as u32,
                total_lines,
            });
        }
        out.sort_by(|a, b| a.file.cmp(&b.file));
        Ok(out)
    }

    pub fn program_summary(entry: &ProjectEntry) -> AnalysisResult<ProgramSummary> {
        let manifest = Self::cargo_manifest(entry).ok();
        let file_sums = Self::file_summary(entry)?;
        let _parsed = Self::parse_all(entry)?;
        Ok(ProgramSummary {
            crate_name: manifest
                .as_ref()
                .and_then(|m| m.crate_name().map(|s| s.to_string())),
            crate_version: manifest
                .as_ref()
                .and_then(|m| m.crate_version().map(|s| s.to_string())),
            edition: manifest
                .as_ref()
                .and_then(|m| m.package.as_ref().and_then(|p| p.edition.clone())),
            file_count: file_sums.len() as u32,
            function_count: file_sums.iter().map(|f| f.function_count).sum(),
            struct_count: file_sums.iter().map(|f| f.struct_count).sum(),
            enum_count: file_sums.iter().map(|f| f.enum_count).sum(),
            trait_count: file_sums.iter().map(|f| f.trait_count).sum(),
            impl_count: file_sums.iter().map(|f| f.impl_count).sum(),
            module_count: file_sums.iter().map(|f| f.module_count).sum(),
            macro_count: file_sums.iter().map(|f| f.macro_count).sum(),
            total_lines: file_sums.iter().map(|f| f.total_lines).sum(),
            direct_dependency_count: manifest
                .as_ref()
                .map(|m| m.direct_dependencies().len() as u32)
                .unwrap_or(0),
            feature_count: manifest
                .as_ref()
                .map(|m| m.features().len() as u32)
                .unwrap_or(0),
        })
    }

    // ─── Reference search (text-based) ────────────────────────────────

    /// Searches all occurrences of a symbol in the project's `.rs` files.
    /// Implementation: grep with `walkdir` (does not use syn — it's textual search).
    pub fn find_references(
        entry: &ProjectEntry,
        symbol: &str,
        case_sensitive: bool,
    ) -> AnalysisResult<Vec<ReferenceHit>> {
        let mut out = Vec::new();
        for path in Self::rust_files(entry) {
            let content = std::fs::read_to_string(&path)?;
            for (i, line) in content.lines().enumerate() {
                let matches = if case_sensitive {
                    line.contains(symbol)
                } else {
                    line.to_lowercase().contains(&symbol.to_lowercase())
                };
                if matches {
                    out.push(ReferenceHit {
                        file: Self::rel_display(entry, &path),
                        line: (i + 1) as u32,
                        context: line.to_string(),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Generic textual search (not filtered by whole-word match).
    pub fn search_source(
        entry: &ProjectEntry,
        query: &str,
        case_sensitive: bool,
    ) -> AnalysisResult<Vec<ReferenceHit>> {
        Self::find_references(entry, query, case_sensitive)
    }

    // ─── Source content ───────────────────────────────────────────────

    /// Returns the content of a file (any file in the project,
    /// not just `.rs`).
    pub fn file_content(entry: &ProjectEntry, rel_path: &str) -> AnalysisResult<String> {
        let path = entry.path.join(rel_path);
        // Path traversal check: normalize `..` lexically
        // and verify that the cleaned path is within the project root.
        // `Path::starts_with` by itself does NOT normalize `..`, which is why
        // we do the normalization manually.
        let normalized = Self::normalize_path(&path);
        let normalized_root = Self::normalize_path(&entry.path);
        if !normalized.starts_with(&normalized_root) {
            return Err(AnalysisError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "path escapes project root",
            )));
        }
        Ok(std::fs::read_to_string(&normalized)?)
    }

    // ─── Symbol resolution ────────────────────────────────────────────

    /// Normalizes a path lexically: resolves `.` and `..` without touching the
    /// filesystem. Useful to validate that a `rel_path` does not escape the
    /// project root.
    fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
        let mut out = std::path::PathBuf::new();
        for comp in path.components() {
            match comp {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                other => out.push(other.as_os_str()),
            }
        }
        out
    }

    /// Does a symbol with that name exist as a top-level item in
    /// any file? (function, struct, enum, trait, module).
    pub fn symbol_exists(entry: &ProjectEntry, symbol: &str) -> AnalysisResult<bool> {
        let parsed = Self::parse_all(entry)?;
        for pf in &parsed {
            for item in &pf.items {
                if let Some(name) = ParsedFile::item_name(item) {
                    if name == symbol {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    /// Converts an absolute path under the project root to a stable,
    /// POSIX-style relative path (`src/lib.rs`). Falls back to the path
    /// itself if it is not under the root.
    fn rel_display(entry: &ProjectEntry, abs_path: &Path) -> String {
        display_rel_path(abs_path.strip_prefix(&entry.path).unwrap_or(abs_path))
    }
}

// ────────────────────────────────────────────────────────────────────────
// Format helpers.
// ────────────────────────────────────────────────────────────────────────

fn format_visibility(vis: &syn::Visibility) -> String {
    match vis {
        syn::Visibility::Inherited => String::new(),
        v => quote::ToTokens::to_token_stream(v).to_string(),
    }
}

/// Converts a syn AST to its string representation.
/// Uses `quote` (which is already a transitive dep of syn).
fn to_string<T: quote::ToTokens>(t: &T) -> String {
    quote::ToTokens::to_token_stream(t).to_string()
}

fn quote_signature(sig: &syn::Signature) -> String {
    let mut s = String::new();
    if sig.constness.is_some() {
        s.push_str("const ");
    }
    if sig.asyncness.is_some() {
        s.push_str("async ");
    }
    if sig.unsafety.is_some() {
        s.push_str("unsafe ");
    }
    s.push_str("fn ");
    s.push_str(&sig.ident.to_string());
    s.push('(');
    let params: Vec<String> = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Receiver(r) => {
                if r.reference.is_some() {
                    if r.mutability.is_some() {
                        "&mut self".to_string()
                    } else {
                        "&self".to_string()
                    }
                } else if r.mutability.is_some() {
                    "mut self".to_string()
                } else {
                    "self".to_string()
                }
            }
            syn::FnArg::Typed(pat_type) => {
                let name = quote::ToTokens::to_token_stream(&*pat_type.pat).to_string();
                let ty = quote::ToTokens::to_token_stream(&*pat_type.ty).to_string();
                format!("{}: {}", name, ty)
            }
        })
        .collect();
    s.push_str(&params.join(", "));
    s.push(')');
    if let syn::ReturnType::Type(_, t) = &sig.output {
        s.push_str(" -> ");
        s.push_str(&quote::ToTokens::to_token_stream(&**t).to_string());
    }
    s
}

fn struct_kind(s: &syn::ItemStruct) -> String {
    match &s.fields {
        syn::Fields::Named(_) => "struct".to_string(),
        syn::Fields::Unnamed(_) => "tuple".to_string(),
        syn::Fields::Unit => "unit".to_string(),
    }
}

fn extract_struct_fields(s: &syn::ItemStruct) -> Vec<FieldInfo> {
    match &s.fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|f| FieldInfo {
                name: f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default(),
                ty: to_string(&f.ty),
                visibility: format_visibility(&f.vis),
                is_pub: matches!(f.vis, syn::Visibility::Public(_)),
            })
            .collect(),
        syn::Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| FieldInfo {
                name: format!("{}", i),
                ty: to_string(&f.ty),
                visibility: format_visibility(&f.vis),
                is_pub: matches!(f.vis, syn::Visibility::Public(_)),
            })
            .collect(),
        syn::Fields::Unit => Vec::new(),
    }
}

fn extract_variant_fields(v: &syn::Variant) -> Vec<FieldInfo> {
    match &v.fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|f| FieldInfo {
                name: f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default(),
                ty: to_string(&f.ty),
                visibility: format_visibility(&f.vis),
                is_pub: matches!(f.vis, syn::Visibility::Public(_)),
            })
            .collect(),
        syn::Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| FieldInfo {
                name: format!("{}", i),
                ty: to_string(&f.ty),
                visibility: format_visibility(&f.vis),
                is_pub: matches!(f.vis, syn::Visibility::Public(_)),
            })
            .collect(),
        syn::Fields::Unit => Vec::new(),
    }
}

fn extract_body_text(path: &Path, f: &syn::ItemFn) -> String {
    let _ = path;
    let tokens = quote::quote! { #f };
    tokens.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProjectEntry;
    use crate::model::ProjectKind;
    use chrono::Utc;
    use std::path::PathBuf;

    fn sample_entry() -> ProjectEntry {
        ProjectEntry {
            name: "sample_crate".to_string(),
            alias: None,
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_crate"),
            kind: ProjectKind::SingleCrate,
            expiry: Utc::now() + chrono::Duration::minutes(10),
            loaded_at: Utc::now(),
            member_crates: vec![],
            cached_manifest: std::sync::OnceLock::new(),
        }
    }

    #[test]
    fn list_functions_on_sample() {
        let entry = sample_entry();
        let fns = RustAnalyzerUtils::list_functions(&entry).unwrap();
        let names: Vec<&str> = fns.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"add"));
        assert!(names.contains(&"multiply"));
    }

    #[test]
    fn program_summary_on_sample() {
        let entry = sample_entry();
        let s = RustAnalyzerUtils::program_summary(&entry).unwrap();
        assert_eq!(s.crate_name.as_deref(), Some("sample_crate"));
        assert!(s.function_count >= 2);
        assert!(s.file_count >= 1);
    }

    #[test]
    fn symbol_exists_true_and_false() {
        let entry = sample_entry();
        assert!(RustAnalyzerUtils::symbol_exists(&entry, "add").unwrap());
        assert!(!RustAnalyzerUtils::symbol_exists(&entry, "nonexistent_xyz").unwrap());
    }
}
