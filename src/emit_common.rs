/// Shared utilities for code emitters (Rust, TypeScript).

/// Sanitize an identifier for use in generated code.
/// Replaces `?` with `_hdlm_qm_` (e.g., `exists?` → `exists_hdlm_qm_`).
/// The `_hdlm_` prefix prevents collisions with user-defined identifiers.
pub fn sanitize(name: &str) -> String {
    name.replace('?', "_hdlm_qm_")
}
