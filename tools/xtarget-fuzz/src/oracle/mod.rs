//! The oracle ladder — cheap→expensive checks applied to each generated
//! program. The first rung to *fail* classifies the program; later rungs
//! run only if the earlier ones pass.
//!
//! Rungs:
//!   (a) `almide check` accepts          — else a GENERATOR bug
//!   (b) fmt round-trip is stable        — else a FORMATTER finding
//!   (c) native build succeeds (no ICE)  — else a NATIVE-CODEGEN finding
//!   (d) wasm build succeeds + validates — else a WASM-CODEGEN finding
//!   (e) run both, byte-compare output   — else a DIVERGENCE finding
//!
//! A future third execution rung — a reference interpreter being built
//! in parallel — slots in behind a clean trait ([`ReferenceOracle`]).
//! This crate does NOT depend on it; the hook is `Option<&dyn ...>`.

mod interp;
mod ladder;
mod runner;

pub use interp::InterpOracle;
pub use ladder::{run_ladder, Finding, FindingKind, Outcome, RunEvidence, Rung};
pub use runner::Toolchain;

/// A future reference-interpreter oracle. When supplied, the ladder will
/// additionally compare each target's output against the interpreter's,
/// pinning *which* target diverged (today a divergence only tells us the
/// two targets disagree, not which is correct).
///
/// Implemented by `InterpOracle` (#516) over the `almide::interp`
/// tree-walker; it abstains (returns `None`) on anything it cannot run.
pub trait ReferenceOracle {
    /// Evaluate `source` and return its expected stdout, or `None` if
    /// the interpreter cannot evaluate this program (it then abstains).
    fn evaluate(&self, source: &str) -> Option<String>;
}
