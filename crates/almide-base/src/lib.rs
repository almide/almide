//! Foundation FACADE (greenfield unit 2 boundary adaptation, PORTLOG.md).
//!
//! The incumbent's almide-base owned four modules. In greenfield, span and
//! diagnostic live in `almide-diag` (unit 1); this crate re-exports them
//! under the incumbent's paths so verbatim-ported crates' `use almide_base::…`
//! lines compile UNCHANGED. `intern` and `profile` are ported verbatim from
//! almide@a877d2138 (crates/almide-base/src/{intern,profile}.rs).

pub mod intern;
pub mod profile;

pub use almide_diag::diagnostic;
pub use almide_diag::span;

// Root re-exports, matching the incumbent's lib.rs surface exactly.
pub use almide_diag::diagnostic::Diagnostic;
pub use almide_diag::span::Span;
pub use intern::{resolve, sym, Sym};
