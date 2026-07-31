//! The one frontend driver.
//!
//! # Why this crate exists
//!
//! Before it, eight files spelled the post-typecheck stage order by hand, and they did
//! not agree. Unit 0.43's B1 inventory found **two different orders shipping at once**:
//!
//! | Site | Order |
//! |---|---|
//! | `src/cli/build.rs`, `src/cli/commands.rs` (native) | lower → **ir_link** → optimize → mono |
//! | `crates/almide-mir/src/pipeline.rs` (wasm), `almide-interp` (the 3rd oracle) | lower → optimize → mono → **ir_link** |
//!
//! So native shipped one order, wasm shipped the other, and the independent third judge
//! was on wasm's side of the question by construction. Nothing was observably wrong —
//! `spec/wasm_cross` was green — but the cross-target equivalence claim was being carried
//! by an untested assumption ("the position of `ir_link` never matters") rather than by a
//! shared driver. #785 is already a recorded bug from exactly this divergence.
//!
//! # The order, and why this one
//!
//! [`link_ir`] runs **optimize → mono → ir_link**, the order the shipped wasm path and the
//! verified v1 trust spine already use. `ir_link` runs LAST so it sees the monomorphized
//! call graph rather than a pre-mono one; running it first means the linker resolves calls
//! that mono is about to specialize.
//!
//! # What is NOT here
//!
//! Everything downstream of the linked IR is consumer-specific and stays with its
//! consumer: `almide-mir`'s transparent-newtype erasure and record-default fill are
//! lowering prep, not frontend stages. The cut point this crate defines is exactly the one
//! `almide-interp` documents as its own — "after `lower → optimize → mono → ir_link`,
//! before any of `almide-codegen`'s target-lowering passes".
//!
//! # Layering
//!
//! `almide-frontend` does not depend on `almide-optimize` (they are siblings), so neither
//! can host a function that runs both. This crate sits above the pair and below
//! `almide-mir`, which is the only placement that introduces no cycle and no layering
//! inversion.

use almide_ir::IrProgram;

/// Run the post-typecheck stage order on an already-lowered `IrProgram`.
///
/// This is the ONLY place in the workspace that spells the order. A second spelling is a
/// drift surface, and `tests/one_driver_test.rs` fails when one appears.
///
/// The caller owns parse/resolve/typecheck/lower, because those need the checker and the
/// resolved module set, which differ between the project path and the single-file path.
/// What every caller shares — and what kept drifting — is what happens AFTER lowering.
pub fn link_ir(ir: &mut IrProgram) {
    optimize_half(ir);
    link_half(ir);
}

/// The stages BEFORE the CLI's integrity gates: optimize, then the top-level-let
/// reclassification that cross-reference const detection needs.
///
/// Split out because `src/compile_driver.rs` runs `verify_ir_or_err` and the
/// `[permissions]` check BETWEEN optimize and monomorphize, on the post-optimize
/// pre-mono IR. Folding those gates to either side of a single `link_ir` call would
/// change WHICH IR they inspect — a behaviour change, and this Unit is a plumbing
/// change. Exposing the two halves keeps the ORDER owned here (a caller cannot
/// reorder what it cannot spell) while making the gate insertion point explicit
/// rather than implicit in a hand-copied sequence.
pub fn optimize_half(ir: &mut IrProgram) {
    almide_optimize::optimize::optimize_program(ir);
    almide_ir::reclassify_top_lets(ir);
}

/// The stages AFTER those gates: monomorphize, then link. `ir_link` last so the
/// linker sees the monomorphized call graph.
pub fn link_half(ir: &mut IrProgram) {
    almide_optimize::mono::monomorphize(ir);
    almide_frontend::ir_link::ir_link(ir);
}


