//! Escaping user text into the body of a generated Rust `"…"` literal.
//!
//! One rule for every native renderer that splices an Almide string into
//! Rust source. Besides the characters that would end or reshape the literal
//! (`\`, `"`, newline, tab, carriage return), it escapes the Unicode bidi
//! controls U+202A–U+202E and U+2066–U+2069 as `\u{XXXX}`: rustc's
//! deny-by-default `text_direction_codepoint_in_literal` rejects them raw
//! (#3438). The escape denotes the same code point, so the program's string is
//! unchanged; only its spelling in the generated source is.

/// Whether `c` is one of the code points rustc refuses raw in a literal.
pub fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Escape `value` for the inside of a Rust `"…"` literal (quotes not added).
pub fn escape_rust_str(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if is_bidi_control(c) => out.push_str(&format!("\\u{{{:04X}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// [`escape_rust_str`] for a `format!`-family template literal: `{` and `}`
/// are doubled first, so the braces of a `\u{XXXX}` escape stay single.
pub fn escape_rust_fmt_str(value: &str) -> String {
    escape_rust_str(&value.replace('{', "{{").replace('}', "}}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bidi_control_is_escaped_and_nothing_raw_remains() {
        let all: String = ('\u{202A}'..='\u{202E}').chain('\u{2066}'..='\u{2069}').collect();
        let out = escape_rust_str(&all);
        assert_eq!(
            out,
            "\\u{202A}\\u{202B}\\u{202C}\\u{202D}\\u{202E}\\u{2066}\\u{2067}\\u{2068}\\u{2069}"
        );
        assert!(!out.chars().any(is_bidi_control));
    }

    #[test]
    fn the_existing_escapes_are_unchanged() {
        assert_eq!(escape_rust_str("a\\b\"c\nd\te\rf é"), "a\\\\b\\\"c\\nd\\te\\rf é");
    }

    #[test]
    fn a_format_template_keeps_the_escape_braces_single() {
        assert_eq!(escape_rust_fmt_str("{x}\u{2067}"), "{{x}}\\u{2067}");
    }
}
