//! Wrapper around the `syn` crate to parse `.rs` files.
//!
//! Mirror of `StatementCollector` in `cobol-mcp-server` (which traverses
//! `ProcedureDivision` with `scanAllStatements`). In Rust the unit is
//! `syn::File`, which parses a complete file into an AST with nested items.
//!
//! In iter 2 we do NOT have semantic analysis (no `ra_ap_*`). Therefore:
//! - `list_functions`, `list_structs`, etc.: ✅ work with `syn` only.
//! - `find_implementations` of a trait, `function_call_graph` with
//!   type resolution: ❌ require `ra_ap_*` (iter 3).
//!
//! For the iter 2 tools that do work, this module exposes:
//! - `ParsedFile` (a parsed file with its typed items).
//! - `parse_file(path)` with per-path cache (future).
//! - specific visitors for listings and inspections.

use std::path::Path;
use syn::{File, Item};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("syn parse error: {0}")]
    Parse(String),
}

/// A parsed `.rs` file, with items extracted in flat form
/// (easier to iterate than the nested tree of `syn::File`).
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub path: std::path::PathBuf,
    pub items: Vec<Item>,
}

impl ParsedFile {
    /// Parses a `.rs` file. If parsing fails, returns the wrapped
    /// error — tools must report it to the LLM, not silence it.
    pub fn parse(path: &Path) -> Result<Self, SyntaxError> {
        let content = std::fs::read_to_string(path)?;
        let file: File = syn::parse_file(&content)
            .map_err(|e| SyntaxError::Parse(format!("{}: {}", path.display(), e)))?;
        Ok(Self {
            path: path.to_path_buf(),
            items: file.items,
        })
    }

    /// Name of the item `fn`, `struct`, `enum`, `trait`, etc.
    pub fn item_name(item: &Item) -> Option<String> {
        use syn::Item::*;
        match item {
            Fn(f) => Some(f.sig.ident.to_string()),
            Struct(s) => Some(s.ident.to_string()),
            Enum(e) => Some(e.ident.to_string()),
            Trait(t) => Some(t.ident.to_string()),
            Impl(i) => i
                .trait_
                .as_ref()
                .map(|(_, t, _)| path_to_string(t))
                .or_else(|| match &*i.self_ty {
                    syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
                    _ => None,
                }),
            Type(t) => Some(t.ident.to_string()),
            Const(c) => Some(c.ident.to_string()),
            Static(s) => Some(s.ident.to_string()),
            Mod(m) => Some(m.ident.to_string()),
            Macro(m) => m
                .ident
                .as_ref()
                .map(|i| i.to_string())
                .or_else(|| Some(macro_path_name(&m.mac.path))),
            Use(_) => Some("<use>".to_string()),
            _ => None,
        }
    }

    /// Filters items by one of the categories.
    pub fn filter<'a>(&'a self, kind: ItemKind) -> impl Iterator<Item = &'a Item> + 'a {
        self.items
            .iter()
            .filter(move |it| matches!(it, i if kind_matches(kind, i)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    Macro,
    Use,
    Const,
    Static,
    TypeAlias,
}

fn kind_matches(kind: ItemKind, item: &Item) -> bool {
    use ItemKind::*;
    matches!(
        (kind, item),
        (Function, Item::Fn(_))
            | (Struct, Item::Struct(_))
            | (Enum, Item::Enum(_))
            | (Trait, Item::Trait(_))
            | (Impl, Item::Impl(_))
            | (Module, Item::Mod(_))
            | (Macro, Item::Macro(_))
            | (Use, Item::Use(_))
            | (Const, Item::Const(_))
            | (Static, Item::Static(_))
            | (TypeAlias, Item::Type(_))
    )
}

fn macro_path_name(path: &syn::Path) -> String {
    path_to_string(path)
}

/// Converts a `syn::Path` to string (e.g. `std::collections::HashMap`).
fn path_to_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LIB: &str = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub enum Color {
    Red,
    Green,
    Blue,
}

pub trait Drawable {
    fn draw(&self);
}

impl Drawable for Point {
    fn draw(&self) {
        println!("({}, {})", self.x, self.y);
    }
}

pub mod inner {
    pub fn helper() {}
}

pub const MAX: u32 = 100;
"#;

    #[test]
    fn parse_lib() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("lib.rs");
        std::fs::write(&p, SAMPLE_LIB).unwrap();
        let pf = ParsedFile::parse(&p).unwrap();

        assert_eq!(pf.filter(ItemKind::Function).count(), 1);
        assert_eq!(pf.filter(ItemKind::Struct).count(), 1);
        assert_eq!(pf.filter(ItemKind::Enum).count(), 1);
        assert_eq!(pf.filter(ItemKind::Trait).count(), 1);
        assert_eq!(pf.filter(ItemKind::Impl).count(), 1);
        assert_eq!(pf.filter(ItemKind::Module).count(), 1);
        assert_eq!(pf.filter(ItemKind::Const).count(), 1);
    }

    #[test]
    fn item_names() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("lib.rs");
        std::fs::write(&p, SAMPLE_LIB).unwrap();
        let pf = ParsedFile::parse(&p).unwrap();

        let names: Vec<String> = pf.items.iter().filter_map(ParsedFile::item_name).collect();
        // The impl `impl Drawable for Point` reports "Drawable" (the trait)
        // because trait_ is Some. See `item_name` in `syntax.rs`.
        assert_eq!(
            names,
            vec!["add", "Point", "Color", "Drawable", "Drawable", "inner", "MAX"]
        );
    }
}
