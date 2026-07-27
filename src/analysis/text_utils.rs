//! Text-search helpers shared by the bug-hunt sweeps.
//!
//! Centralizes whole-word matching logic so that sweeps
//! (currently `find_unused_dependencies` and future ones) can reuse it.

use std::path::Path;

/// Counts occurrences of `needle` as a whole word in a list of files.
/// Returns the total count across all files.
///
/// A "whole word" match means `needle` surrounded by boundaries (non-alphanumeric
/// and non-`_`). This matches:
/// - `use tokio;` ✓
/// - `tokio::spawn` ✓
/// - `extern crate tokio;` ✓
///
/// And does NOT match:
/// - `tokio_util` ✗ (no match — the `_` after acts as a boundary, but
///   `tokio` is a prefix of `tokio_util` which is considered a whole word)
///   → `tokio_util` WOULD match for needle=`tokio_util`, not for
///   `tokio`. Correct behavior.
/// - `atokios` ✗ (no match — the `a` before is not a boundary)
///
/// **Limitation**: reads files with `std::fs::read_to_string`. If
/// a file cannot be read, it is silently ignored.
pub fn count_whole_word_matches<P: AsRef<Path>>(files: &[P], needle: &str) -> usize {
    let mut total = 0;
    for path in files {
        let Ok(content) = std::fs::read_to_string(path.as_ref()) else {
            continue;
        };
        total += count_whole_word_matches_in_str(&content, needle);
    }
    total
}

/// Variant that operates on a `&str` (useful for unit tests and
/// for callers that already have the content in memory).
pub fn count_whole_word_matches_in_str(content: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let bytes = content.as_bytes();
    let needle_bytes = needle.as_bytes();
    let n = needle_bytes.len();
    if bytes.len() < n {
        return 0;
    }
    let mut total = 0;
    let mut start = 0;
    while let Some(pos) = content[start..].find(needle) {
        let abs = start + pos;
        let before_ok =
            abs == 0 || (!bytes[abs - 1].is_ascii_alphanumeric() && bytes[abs - 1] != b'_');
        let after_pos = abs + n;
        let after_ok = after_pos >= bytes.len()
            || (!bytes[after_pos].is_ascii_alphanumeric() && bytes[after_pos] != b'_');
        if before_ok && after_ok {
            total += 1;
        }
        start = abs + 1;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_needle() {
        assert_eq!(count_whole_word_matches_in_str("hello world", ""), 0);
    }

    #[test]
    fn basic_match() {
        assert_eq!(count_whole_word_matches_in_str("use tokio", "tokio"), 1);
        assert_eq!(count_whole_word_matches_in_str("tokio::spawn", "tokio"), 1);
        assert_eq!(
            count_whole_word_matches_in_str("extern crate tokio;", "tokio"),
            1
        );
    }

    #[test]
    fn word_boundary_rejects_substring() {
        // "tokio" in "tokio_util" is NOT a match for needle="tokio".
        assert_eq!(count_whole_word_matches_in_str("tokio_util", "tokio"), 0);
        // "atokios" neither.
        assert_eq!(count_whole_word_matches_in_str("atokios", "tokio"), 0);
        // "tokio_util" IS a match for needle="tokio_util".
        assert_eq!(
            count_whole_word_matches_in_str("tokio_util", "tokio_util"),
            1
        );
    }

    #[test]
    fn multiple_matches_in_one_str() {
        let s = "use tokio; use tokio::sync; tokio::main(); tokio";
        // 4 occurrences of "tokio" as whole word.
        assert_eq!(count_whole_word_matches_in_str(s, "tokio"), 4);
    }

    #[test]
    fn underscores_in_needle_work() {
        // The needle itself can have `_` and it is treated as part of the word.
        assert_eq!(
            count_whole_word_matches_in_str("use tokio_util", "tokio_util"),
            1
        );
    }
}
