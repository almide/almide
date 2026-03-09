/// Shared utilities for code emitters (Rust, TypeScript).

/// Sanitize an identifier for use in generated code.
/// Replaces `?` with `_hdlm_qm_` (e.g., `exists?` → `exists_hdlm_qm_`).
/// The `_hdlm_` prefix prevents collisions with user-defined identifiers.
pub fn sanitize(name: &str) -> String {
    name.replace('?', "_hdlm_qm_")
}

/// Convert a sanitized or snake_case name to a clean camelCase export name
/// for npm packages. Examples:
///   `is_empty_hdlm_qm_` → `isEmpty`
///   `pangram_hdlm_qm_`  → `pangram`
///   `has_letter`         → `hasLetter`
///   `roman`              → `roman`
pub fn to_clean_export_name(sanitized: &str) -> String {
    // Strip the _hdlm_qm_ suffix (restores original name minus `?`)
    let base = sanitized.strip_suffix("_hdlm_qm_").unwrap_or(sanitized);
    snake_to_camel(base)
}

fn snake_to_camel(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}
