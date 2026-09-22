//! E085: `@intrinsic` / `@wasm_intrinsic` outside the stdlib (#2152 item 3).
//!
//! An `@intrinsic("almide_rt_fs_write")` declaration binds a fn name to a
//! runtime symbol directly. The capability story (`--profile critical`,
//! `--allow IO`) is told in terms of stdlib MODULES — a user file that binds
//! the runtime symbol itself never imports `fs`, so nothing in the profile
//! sees the write. `attr_vocab.rs` listed the attribute as an ordinary word
//! and `scripts/check-intrinsic-boundary.sh` proved that intrinsics LINK, not
//! that they are ALLOWED. This is the authority gate: the attribute is legal
//! only in sources that are the runtime boundary — the repository's `stdlib/`
//! and `runtime/` trees, and the bundled stdlib copy compiled into the binary
//! (which has no path; the module driver marks it instead).
//!
//! The judgement is by ORIGIN, never by module name: a user module named
//! `list` is still a user module. When the checker has no origin at all (no
//! source path and not inside a bundled module — the LSP's in-memory analysis)
//! it abstains, as E084 does, rather than guess.
use super::{Checker, err};
use crate::ast::{Attribute, Span};

/// Path components under which the attribute is legal.
const AUTHORITY_TREES: &[&str] = &["stdlib", "runtime"];

impl Checker {
    /// True when the source under inference may declare intrinsics.
    fn intrinsic_authority(&self) -> Option<bool> {
        if self.in_bundled_module {
            return Some(true);
        }
        let file = self.source_file.as_deref()?;
        Some(std::path::Path::new(file).components().any(|c| {
            c.as_os_str().to_str().is_some_and(|s| AUTHORITY_TREES.contains(&s))
        }))
    }

    pub(super) fn reject_user_intrinsic(&mut self, fn_name: &str, attrs: &[Attribute], decl_span: Option<Span>) {
        let Some(attr) = attrs
            .iter()
            .find(|a| matches!(a.name.as_str(), "intrinsic" | "wasm_intrinsic"))
        else {
            return;
        };
        if self.intrinsic_authority() != Some(false) {
            return;
        }
        let mut diagnostic = err(
            format!("@{} on fn '{}' outside the stdlib", attr.name, fn_name),
            "intrinsics live in the stdlib; wrap in an effect fn: call the stdlib module that owns the operation (fs, http, process, ...) from an `effect fn` of your own, so `--profile critical` sees the capability",
            format!("fn {fn_name}"),
        )
        .with_code("E085");
        if let Some(span) = attr.span.or(decl_span) {
            diagnostic.file = self.source_file.clone();
            diagnostic.line = Some(span.line);
            diagnostic.col = Some(span.col);
            diagnostic.end_col = Some(span.end_col);
        }
        self.emit(diagnostic);
    }
}
