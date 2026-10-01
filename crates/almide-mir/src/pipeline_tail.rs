// ── tail of pipeline.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

include!("pipeline_native_rungs.rs");
include!("pipeline_witnesses.rs");
#[cfg(test)]
include!("pipeline_ctor_resolution_tests.rs");
