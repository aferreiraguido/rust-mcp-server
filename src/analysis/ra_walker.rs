//! Walk helpers over `ra_ap_syntax::SyntaxNode` for the bug-hunt sweeps.
//!
//! `ra_ap_syntax` does not have an auto-generated `Visit` trait (unlike
//! `syn`). To sweep the AST without re-implementing the
//! infrastructure, we use a recursive walk of `SyntaxNode` +
//! `AstNode::cast` for typed dispatch.

use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode};

/// Recursively walks a `SyntaxNode`. For each node, calls `visitor`
/// with the node and its `SyntaxKind`. The visitor decides whether to act.
pub fn walk<F>(node: &SyntaxNode, visitor: &mut F)
where
    F: FnMut(&SyntaxNode, SyntaxKind),
{
    visitor(node, node.kind());
    for child in node.children() {
        walk(&child, visitor);
    }
}

/// Walk only nodes of a specific type (via `AstNode::cast`).
/// Useful when you only care about, e.g., `ExprUnsafe`.
pub fn walk_typed<N, F>(root: &SyntaxNode, mut visitor: F)
where
    N: AstNode,
    F: FnMut(&N),
{
    walk(root, &mut |node, _kind| {
        if let Some(typed) = N::cast(node.clone()) {
            visitor(&typed);
        }
    });
}

/// Test helper: count nodes of a specific type.
#[cfg(test)]
pub fn count_typed<N: AstNode>(root: &SyntaxNode) -> usize {
    let mut n = 0;
    walk_typed::<N, _>(root, |_| n += 1);
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra_ap_syntax::ast::HasName;

    const SAMPLE: &str = r#"
fn foo() {
    let x = bar();
    unsafe { x }
}

fn bar() -> i32 { 42 }
"#;

    #[test]
    fn walk_finds_all_functions() {
        let parse = ra_ap_syntax::SourceFile::parse(SAMPLE, ra_ap_syntax::Edition::CURRENT);
        let sf = parse.tree();
        let mut fns = 0;
        walk(sf.syntax(), &mut |_, kind| {
            if kind == SyntaxKind::FN {
                fns += 1;
            }
        });
        assert_eq!(fns, 2);
    }

    #[test]
    fn walk_typed_finds_fn() {
        let parse = ra_ap_syntax::SourceFile::parse(SAMPLE, ra_ap_syntax::Edition::CURRENT);
        let sf = parse.tree();
        let mut names = Vec::new();
        walk_typed::<ra_ap_syntax::ast::Fn, _>(sf.syntax(), |f| {
            if let Some(n) = f.name() {
                names.push(n.text().to_string());
            }
        });
        assert_eq!(names, vec!["foo", "bar"]);
    }
}
