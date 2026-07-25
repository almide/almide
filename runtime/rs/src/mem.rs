// mem extern — arena scope management.
//
// The bump allocator these checkpoints address exists only on the wasm leg.
// Natively, reference counting already reclaims exactly the allocations a
// restore would, so both are no-ops and a program using `mem` behaves
// identically on both targets — the same shape `bytes.heap_save`/`heap_restore`
// already had. They still need real symbols: without them, codegen emitted
// `almide_rt_mem_save()` against nothing and the generated Rust failed to
// compile (E0425), breaking the check-accepted-must-build invariant.

pub fn almide_rt_mem_save() -> i64 { 0 }
pub fn almide_rt_mem_restore(_mark: i64) {}
