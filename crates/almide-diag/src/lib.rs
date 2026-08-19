//! Almide diagnostics — the L1–L3 reporting surface (ARCHITECTURE.md §2).
//!
//! Ported verbatim from `almide@a877d2138`:
//! - `src/span.rs`        ← crates/almide-base/src/span.rs
//! - `src/diagnostic.rs`  ← crates/almide-base/src/diagnostic.rs
//! - `src/render.rs`      ← src/diagnostic_render.rs (one import line re-pointed)
//!
//! Provenance, adaptations, and the parity gate: PORTLOG.md (unit 1).

// Scoped allows on the two verbatim-ported modules ONLY (PORTLOG.md unit 1):
// today's clippy flags style in code we deliberately do not edit during a
// port. Scaffolding and future modules stay fully linted. Cleaning these up
// upstream-style is a later, diff-visible change — never a silent porting edit.
#[allow(clippy::collapsible_if, clippy::empty_line_after_doc_comments, clippy::manual_ignore_case_cmp)]
pub mod diagnostic;
#[allow(clippy::collapsible_if, clippy::empty_line_after_doc_comments)]
pub mod render;
pub mod span;

pub use diagnostic::Diagnostic;
pub use span::Span;
