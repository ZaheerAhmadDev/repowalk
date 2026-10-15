//! Minimal JSON writer for `--json` machine output.
//! OWNER: PROD-3.
//!
//! The CLI crate intentionally takes **no new dependencies** (card PROD-3), so
//! rather than pull in `serde_json` this module emits the small, fixed set of
//! shapes the query commands need. Escaping follows RFC 8259.
//!
//! This is *serialization only* — no aggregation or rendering lives here; the
//! values come from `repowalk_core` (`complexity`, `refs`, the export emitters)
//! exactly as the human-readable output does.

/// Escape `s` for inclusion in a JSON string (without the surrounding quotes).
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// A quoted, escaped JSON string.
pub fn string(s: &str) -> String {
    format!("\"{}\"", escape(s))
}

/// A quoted string, or `null`.
pub fn opt_string(s: Option<&str>) -> String {
    match s {
        Some(s) => string(s),
        None => "null".to_string(),
    }
}

/// A JSON array from already-serialized elements (compact; still valid JSON).
pub fn array(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_covers_quotes_backslashes_and_controls() {
        assert_eq!(escape("a\"b\\c\nd\te"), "a\\\"b\\\\c\\nd\\te");
        assert_eq!(escape("\u{01}"), "\\u0001");
    }

    #[test]
    fn string_and_opt_string() {
        assert_eq!(string("hi"), "\"hi\"");
        assert_eq!(opt_string(Some("x")), "\"x\"");
        assert_eq!(opt_string(None), "null");
    }

    #[test]
    fn array_joins_or_is_empty() {
        assert_eq!(array(&[]), "[]");
        assert_eq!(array(&["1".into(), "2".into()]), "[1,2]");
    }
}
