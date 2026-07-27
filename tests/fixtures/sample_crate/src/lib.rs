//! Fixture principal para tests de rust-mcp-server.
//!
//! Uses real deps (`cfg-if`, `once_cell`, `pretty_assertions`) so that
//! the tools `list_dependencies`, `find_references`, `find_unused_dependencies`,
//! and `function_callers` have real material to work with.

use cfg_if::cfg_if;
use once_cell::sync::OnceCell;

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}

static CACHED_VALUE: OnceCell<i32> = OnceCell::new();

/// Returns `true` si el binario se compiló en release mode.
/// Ejercita el dep `cfg-if`.
pub fn is_release() -> bool {
    cfg_if! {
        if #[cfg(debug_assertions)] {
            false
        } else {
            true
        }
    }
}

/// Returns the cached value, initializing it on first call.
/// Ejercita el dep `once_cell`.
pub fn cached_value() -> i32 {
    *CACHED_VALUE.get_or_init(|| 42)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
    }

    #[test]
    fn test_multiply() {
        assert_eq!(multiply(2, 3), 6);
    }

    #[test]
    fn test_is_release() {
        // En tests siempre es false (debug_assertions activo).
        assert_eq!(is_release(), false);
    }

    #[test]
    fn test_cached_value() {
        assert_eq!(cached_value(), 42);
        assert_eq!(cached_value(), 42); // segunda llamada, mismo valor
    }
}
