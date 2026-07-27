//! Common helper for bug-hunt tools: formats `Vec<Finding>` as a Markdown
//! table and applies optional filters.

use crate::model::{Finding, FindingCategory, FindingSeverity};

pub fn format_findings(title: &str, findings: &[Finding], min_severity: FindingSeverity) -> String {
    let mut md = String::new();
    let filtered: Vec<&Finding> = findings
        .iter()
        .filter(|f| severity_rank(f.severity) >= severity_rank(min_severity))
        .collect();
    md.push_str(&format!("# {} ({})\n\n", title, filtered.len()));
    if filtered.is_empty() {
        md.push_str("_No findings._\n");
        return md;
    }
    md.push_str("| Severity | Code | Category | File | Message |\n");
    md.push_str("|----------|------|----------|------|---------|\n");
    for f in &filtered {
        let sev = match f.severity {
            FindingSeverity::Info => "INFO",
            FindingSeverity::Low => "LOW",
            FindingSeverity::Medium => "MEDIUM",
            FindingSeverity::High => "HIGH",
            FindingSeverity::Critical => "CRITICAL",
        };
        let cat = match f.category {
            FindingCategory::Unsafe => "Unsafe",
            FindingCategory::ControlFlow => "ControlFlow",
            FindingCategory::DataFlow => "DataFlow",
            FindingCategory::DeadCode => "DeadCode",
            FindingCategory::UnwrapRisk => "UnwrapRisk",
            FindingCategory::PanicSite => "PanicSite",
            FindingCategory::Structural => "Structural",
        };
        md.push_str(&format!(
            "| {} | `{}` | {} | `{}` | {} |\n",
            sev, f.code, cat, f.file, f.message
        ));
    }
    md.push_str("\n_Use `min_severity` to filter (default LOW)._\n");
    md
}

pub fn severity_rank(s: FindingSeverity) -> u8 {
    match s {
        FindingSeverity::Info => 0,
        FindingSeverity::Low => 1,
        FindingSeverity::Medium => 2,
        FindingSeverity::High => 3,
        FindingSeverity::Critical => 4,
    }
}
