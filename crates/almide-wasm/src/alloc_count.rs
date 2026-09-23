//! The allocation COUNTER switch of the structural leg (#2407): the twin of
//! native's `ALMIDE_ALLOC_COUNT`.
//!
//! The `__heap` watermark the alloc ledger pins is a PEAK: a bump heap with
//! size-class free lists reaches the same watermark whether it allocated a
//! thousand blocks once or reused ten blocks a hundred times, so allocation
//! churn is invisible in it. Under this switch `$alloc` and `$free` also
//! maintain four i64 globals the host reads after the run:
//!
//! | export            | counts                                            |
//! |-------------------|---------------------------------------------------|
//! | `__alloc_count`   | every `$alloc` call (free-list hit or bump)       |
//! | `__alloc_reused`  | the `$alloc` calls a free-list pop served         |
//! | `__alloc_bytes`   | the sum of the payload lengths requested          |
//! | `__free_count`    | every `$free` call (filed or abandoned)           |
//!
//! OFF (the default) nothing is emitted: the globals, the exports and the
//! increments are all absent, so a shipped module is byte-identical to a
//! build without the switch and neither the size ratchet nor the alloc
//! ledger moves. The counters are appended AFTER the top-let globals, so
//! no existing global index changes and the heap layout (data segment,
//! line buffer, heap floor) is untouched — the watermark of an armed run
//! equals the unarmed one, which the alloc ledger asserts row by row.
//!
//! THREAD-LOCAL with a scoped guard, the `host_exports` discipline: the
//! CLI arms it from the `ALMIDE_WASM_ALLOC_COUNT` env switch, a test arms
//! it on its own thread without touching the process environment.

use std::cell::Cell;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

/// Is the counter being emitted into the module under emission?
pub fn armed() -> bool {
    ARMED.with(|c| c.get())
}

/// The exported counter globals, in the order they are appended after the
/// top-let globals (each an i64 starting at 0).
pub const EXPORTS: [&str; 4] = ["__alloc_count", "__alloc_reused", "__alloc_bytes", "__free_count"];

/// Offsets of each counter from the first counter's global index.
pub(crate) const COUNT: u32 = 0;
pub(crate) const REUSED: u32 = 1;
pub(crate) const BYTES: u32 = 2;
pub(crate) const FREES: u32 = 3;

/// Turn the switch on for a scope and restore the previous state on drop.
#[must_use = "the guard restores the previous state when dropped; binding it to `_` restores immediately"]
pub struct CountGuard(bool);

impl CountGuard {
    pub fn set() -> Self {
        let prev = armed();
        ARMED.with(|c| c.set(true));
        Self(prev)
    }
}

impl Drop for CountGuard {
    fn drop(&mut self) {
        ARMED.with(|c| c.set(self.0));
    }
}
