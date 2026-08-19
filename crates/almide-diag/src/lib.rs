//! Almide diagnostics — the L1–L3 reporting surface (ARCHITECTURE.md §2).
//!
//! Ported verbatim from `almide@a877d2138`:
//! - `src/span.rs`        ← crates/almide-base/src/span.rs
//! - `src/diagnostic.rs`  ← crates/almide-base/src/diagnostic.rs
//! - `src/render.rs`      ← src/diagnostic_render.rs (one import line re-pointed)
//!
//! Provenance, adaptations, and the parity gate: PORTLOG.md (unit 1).

pub mod diagnostic;
pub mod render;
pub mod span;

pub use diagnostic::Diagnostic;
pub use span::Span;
