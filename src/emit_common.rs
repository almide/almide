/// Shared utilities for code emitters (Rust, TypeScript).

/// Sanitize an identifier for use in generated code.
/// Replaces `?` with `_qm_` (e.g., `exists?` → `exists_qm_`).
pub fn sanitize(name: &str) -> String {
    name.replace('?', "_qm_")
}
