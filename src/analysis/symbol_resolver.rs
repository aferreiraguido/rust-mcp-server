//! `SymbolResolver` — name → location index (file_id + text_range).
//!
//! For each file in the project, parses with `ra_ap_syntax` and
//! registers all top-level items (fn, struct, enum, trait,
//! const, static, type alias, module) and impl methods.
//!
//! Tools use this index to:
//! 1. Given a symbol name, find its `FilePosition` (with offset).
//! 2. Call `Analysis` methods (`goto_definition`, `goto_implementation`,
//!    `find_all_refs`, etc.) at that position.
//! 3. Return the result as deterministic Markdown.

use ra_ap_ide::{Analysis, FileId, FilePosition};
use ra_ap_syntax::{
    ast::{self, HasAttrs, HasModuleItem, HasName, HasVisibility},
    AstNode, Edition, SyntaxKind, SyntaxNode, TextRange,
};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("symbol '{0}' not found")]
    NotFound(String),
    #[error("ambiguous symbol '{0}' (N candidates)")]
    Ambiguous(String, usize),
    #[error("analysis error: {0}")]
    Analysis(String),
}

/// Symbol type in the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Const,
    Static,
    TypeAlias,
    Module,
    ImplMethod,
}

impl SymbolKind {
    #[allow(dead_code)]
    fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "fn",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Const => "const",
            SymbolKind::Static => "static",
            SymbolKind::TypeAlias => "type",
            SymbolKind::Module => "mod",
            SymbolKind::ImplMethod => "impl method",
        }
    }
}

/// A symbol location in the code.
#[derive(Debug, Clone)]
pub struct SymbolLocation {
    pub name: String,
    pub kind: SymbolKind,
    pub file_id: FileId,
    /// Range of the IDENTIFIER (not the complete item). Used to
    /// build a valid `FilePosition`.
    pub name_range: TextRange,
    /// Range of the complete item (includes body, generics, etc.).
    pub full_range: TextRange,
    /// Whether the item is public (`pub fn`, `pub struct`, etc.). Public
    /// items are ignored by the dead-code sweep.
    pub is_pub: bool,
    /// Attributes indicating the item is implicitly referenced
    /// by the toolchain (test runner, derive macros, linker, etc.).
    /// Items with any of these are NOT marked as dead code.
    pub implicit_references: ImplicitReferences,
}

/// Categories of implicit references that avoid false positives in
/// `find_dead_code`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImplicitReferences {
    pub test: bool,      // #[test] — cargo test invokes it
    pub derive: bool,    // #[derive(...)] — the proc-macro generates code that uses it
    pub no_mangle: bool, // #[no_mangle] — the linker can invoke it from C/FFI
    pub cfg: bool,       // #[cfg(...)] — only compiled conditionally
    pub allow: bool,     // #[allow(...)] — lint hint, not a real reference
}

impl ImplicitReferences {
    /// `true` if some attribute implies the item has references
    /// outside the Rust code visible to the engine.
    pub fn is_implicitly_referenced(&self) -> bool {
        self.test || self.derive || self.no_mangle || self.cfg
        // `allow` does NOT count — it is a hint, not a real reference.
    }
}

/// Project symbol index.
pub struct SymbolResolver {
    by_name: HashMap<String, Vec<SymbolLocation>>,
}

impl SymbolResolver {
    /// Builds the index from `Analysis` and a `virtual_path → file_id` mapping.
    pub fn build(analysis: &Analysis, files: &HashMap<String, FileId>) -> Self {
        let mut by_name: HashMap<String, Vec<SymbolLocation>> = HashMap::new();
        for file_id in files.values() {
            // `Analysis::parse` returns `Cancellable<SourceFile>`. In
            // practice it is never cancelled here (we don't use cancellation),
            // but we must handle the `Result`.
            let source = match analysis.parse(*file_id) {
                Ok(s) => s,
                Err(_) => continue, // file could not be parsed — skip
            };
            register_top_level(&source, *file_id, &mut by_name);
        }
        Self { by_name }
    }

    /// Looks up a symbol by name. If there are several, returns the first
    /// (preferentially function/struct/enum/trait, in that order).
    pub fn resolve(&self, name: &str) -> Option<&SymbolLocation> {
        let candidates = self.by_name.get(name)?;
        if candidates.is_empty() {
            return None;
        }
        // Prefer functions/structs/enums/traits over impl methods/consts.
        let priority = |k: SymbolKind| match k {
            SymbolKind::Function => 0,
            SymbolKind::Struct => 1,
            SymbolKind::Enum => 2,
            SymbolKind::Trait => 3,
            SymbolKind::TypeAlias => 4,
            SymbolKind::Module => 5,
            SymbolKind::Const => 6,
            SymbolKind::Static => 7,
            SymbolKind::ImplMethod => 8,
        };
        let mut sorted: Vec<&SymbolLocation> = candidates.iter().collect();
        sorted.sort_by_key(|s| priority(s.kind));
        sorted.first().copied()
    }

    /// Returns ALL symbols with that name (useful for reports).
    pub fn resolve_all(&self, name: &str) -> Vec<&SymbolLocation> {
        self.by_name
            .get(name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Iterator over ALL project symbols (name + locations).
    /// Used by the dead-code sweep.
    pub fn all_symbols(&self) -> impl Iterator<Item = (&String, &Vec<SymbolLocation>)> {
        self.by_name.iter()
    }

    /// Converts a SymbolLocation to a FilePosition (offset = start of
    /// the identifier).
    pub fn file_position(loc: &SymbolLocation) -> FilePosition {
        FilePosition {
            file_id: loc.file_id,
            offset: loc.name_range.start(),
        }
    }

    /// Total indexed symbols.
    pub fn len(&self) -> usize {
        self.by_name.values().map(|v| v.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn register_top_level(
    sf: &ra_ap_syntax::SourceFile,
    file_id: FileId,
    out: &mut HashMap<String, Vec<SymbolLocation>>,
) {
    for item in sf.items() {
        register_item(&item, file_id, out);
    }
}

fn register_item(
    item: &ra_ap_syntax::ast::Item,
    file_id: FileId,
    out: &mut HashMap<String, Vec<SymbolLocation>>,
) {
    match item {
        ast::Item::Fn(f) => register_named(f, f.name(), SymbolKind::Function, file_id, out),
        ast::Item::Struct(s) => register_named(s, s.name(), SymbolKind::Struct, file_id, out),
        ast::Item::Enum(e) => register_named(e, e.name(), SymbolKind::Enum, file_id, out),
        ast::Item::Trait(t) => register_named(t, t.name(), SymbolKind::Trait, file_id, out),
        ast::Item::Const(c) => register_named(c, c.name(), SymbolKind::Const, file_id, out),
        ast::Item::Static(s) => register_named(s, s.name(), SymbolKind::Static, file_id, out),
        ast::Item::TypeAlias(ta) => {
            register_named(ta, ta.name(), SymbolKind::TypeAlias, file_id, out)
        }
        ast::Item::Module(m) => {
            register_named(m, m.name(), SymbolKind::Module, file_id, out);
            // Recurse: the module can have nested items (e.g.
            // `mod tests { fn test_add() {...} }`).
            if let Some(items) = m.item_list() {
                for inner in items.items() {
                    register_item(&inner, file_id, out);
                }
            }
        }
        ast::Item::Impl(i) => {
            // Register the impl's methods. Must go through
            // AssocItemList -> AssocItems.
            if let Some(list) = i.assoc_item_list() {
                for assoc in list.assoc_items() {
                    if let ast::AssocItem::Fn(f) = assoc {
                        register_named(&f, f.name(), SymbolKind::ImplMethod, file_id, out);
                    }
                }
            }
        }
        _ => {}
    }
}

fn register_named<N>(
    node: &N,
    name: Option<ast::Name>,
    kind: SymbolKind,
    file_id: FileId,
    out: &mut HashMap<String, Vec<SymbolLocation>>,
) where
    N: AstNode + HasVisibility + HasAttrs,
{
    let Some(name_node) = name else { return };
    let Some(ident) = name_node.ident_token() else {
        return;
    };
    let name_text = ident.text().to_string();
    let name_range = name_node.syntax().text_range();
    let full_range = node.syntax().text_range();
    // `HasVisibility::visibility()` returns `Option<Visibility>`; if it is
    // `Some(v)` and `v.pub_token()` is `Some`, the item is `pub`.
    let is_pub = node.visibility().and_then(|v| v.pub_token()).is_some();
    // Collect attributes that imply implicit references.
    let attrs: Vec<ra_ap_syntax::ast::Attr> = node.attrs().collect();
    let implicit_references = collect_implicit_refs(&attrs);
    out.entry(name_text).or_default().push(SymbolLocation {
        name: ident.text().to_string(),
        kind,
        file_id,
        name_range,
        full_range,
        is_pub,
        implicit_references,
    });
}

/// Extracts attributes that imply implicit references to the item.
///
/// Rust attributes have different forms:
/// - `#[name]` and `#[name(args)]` → `Meta::PathMeta` or `Meta::KeyValueMeta` → they have `path()`.
/// - `#[cfg(...)]` → `Meta::CfgAttrMeta` or `Meta::CfgMeta` → they have `cfg_predicate()`, NO `path()`.
/// - `#[doc = "..."]` → `Meta::NameValueMeta` → has `path()`.
///
/// For those that do have a path (test, derive, no_mangle, allow) we read the
/// first segment of the path. For `#[cfg(...)]` we detect by the
/// Meta enum variant.
fn collect_implicit_refs(attrs: &[ra_ap_syntax::ast::Attr]) -> ImplicitReferences {
    let mut r = ImplicitReferences::default();
    for attr in attrs {
        // First detect `#[cfg(...)]` by the Meta variant.
        // This is necessary because `attr.path()` returns None for
        // `Meta::CfgAttrMeta` and `Meta::CfgMeta` (see ast/node_ext.rs).
        if let Some(
            ra_ap_syntax::ast::Meta::CfgAttrMeta(_) | ra_ap_syntax::ast::Meta::CfgMeta(_),
        ) = attr.meta()
        {
            r.cfg = true;
            continue;
        }
        // For other attrs, read the path.
        let Some(path) = attr.path() else { continue };
        let first_segment = path.segment();
        let path_str = match first_segment.and_then(|seg| seg.name_ref()) {
            Some(name_ref) => name_ref.text().to_string(),
            None => continue,
        };
        match path_str.as_str() {
            "test" => r.test = true,
            "derive" => r.derive = true,
            "no_mangle" => r.no_mangle = true,
            "allow" => r.allow = true,
            _ => {}
        }
    }
    r
}

#[allow(dead_code)]
fn _kind_marker(s: SyntaxKind) -> SyntaxKind {
    s
}

#[allow(dead_code)]
fn _edition_marker() -> Edition {
    Edition::CURRENT
}

#[allow(dead_code)]
fn _syntax_node_marker(n: SyntaxNode) -> SyntaxNode {
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analysis_host::ProjectAnalysisHost;

    #[test]
    fn build_resolver_for_sample() {
        let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_crate");
        let host = ProjectAnalysisHost::load(&fixture_root).expect("load");
        let analysis = host.analysis();
        let resolver = SymbolResolver::build(&analysis, &host.files);
        // The fixture has `add`, `multiply` (lib.rs) and `test_add`,
        // `test_multiply` (mod tests).
        let add = resolver.resolve("add");
        assert!(add.is_some());
        assert_eq!(add.unwrap().kind, SymbolKind::Function);
        // A non-existent symbol.
        assert!(resolver.resolve("nonexistent_xyz").is_none());
        // test_add is a fn inside mod tests, not an impl method.
        let test_add = resolver.resolve("test_add");
        assert!(test_add.is_some());
        assert_eq!(test_add.unwrap().kind, SymbolKind::Function);
        // test_add has `#[test]`, so its implicit_references.test
        // must be true (it should NOT be marked as dead code).
        let test_add_loc = test_add.unwrap();
        assert!(
            test_add_loc.implicit_references.test,
            "test_add must have implicit_references.test = true"
        );
        assert!(
            test_add_loc.implicit_references.is_implicitly_referenced(),
            "test_add must be implicitly_referenced (test or derive)"
        );
        // `add` does not have `#[test]`, so implicit_references.test = false.
        let add_loc = add.unwrap();
        assert!(
            !add_loc.implicit_references.test,
            "add must NOT have implicit_references.test = true"
        );
    }

    #[test]
    fn mod_tests_has_cfg_attribute() {
        let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_crate");
        let host = ProjectAnalysisHost::load(&fixture_root).expect("load");
        let analysis = host.analysis();
        let resolver = SymbolResolver::build(&analysis, &host.files);
        // `mod tests` has `#[cfg(test)]` → must have implicit_references.cfg
        // and therefore be implicitly_referenced (must NOT be dead code).
        let tests = resolver.resolve("tests");
        assert!(tests.is_some(), "mod tests must be indexed");
        let tests_loc = tests.unwrap();
        assert_eq!(tests_loc.kind, SymbolKind::Module);
        assert!(
            tests_loc.implicit_references.cfg,
            "mod tests must have implicit_references.cfg = true (from #[cfg(test)])"
        );
        assert!(
            tests_loc.implicit_references.is_implicitly_referenced(),
            "mod tests must be implicitly_referenced (cfg)"
        );
    }
}
