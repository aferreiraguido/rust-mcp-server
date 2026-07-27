//! Shared Markdown rendering helpers for tools.
//!
//! Mirror of the output style of `cobol-mcp-server` (deterministic Markdown,
//! ASCII tables, no timestamps, byte-for-byte reproducible).

pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str("| ");
    out.push_str(&headers.join(" | "));
    out.push_str(" |\n");
    out.push('|');
    out.push_str(
        &headers
            .iter()
            .map(|_| "------")
            .collect::<Vec<_>>()
            .join("|"),
    );
    out.push_str("|\n");
    for row in rows {
        out.push_str("| ");
        out.push_str(&row.join(" | "));
        out.push_str(" |\n");
    }
    out
}

pub fn code_block(content: &str, lang: &str) -> String {
    format!("```{}\n{}\n```\n", lang, content)
}

pub fn list_of(items: &[String]) -> String {
    items
        .iter()
        .map(|i| format!("- {}\n", i))
        .collect::<String>()
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

pub fn section(title: &str) -> String {
    format!("\n## {}\n\n", title)
}

pub fn subsection(title: &str) -> String {
    format!("\n### {}\n\n", title)
}
