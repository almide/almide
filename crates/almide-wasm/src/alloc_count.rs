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
//! | `__alloc_count`   | every allocation: a `$alloc` call (free-list hit  |
//! |                   | or bump) or a constructor's inlined bump (#2318)  |
//! | `__alloc_reused`  | the `$alloc` calls a free-list pop served         |
//! | `__alloc_bytes`   | the sum of the payload lengths requested          |
//! | `__free_count`    | every `$free` call (filed or abandoned)           |
//! | `__region_reclaimed` | blocks a region window's restore reclaimed     |
//! |                   | wholesale, without a `$free` call (#1961)          |
//!
//! LIVE AT EXIT (the live-heap gate). `allocs − frees − region_reclaimed`
//! read after `main` returns is the number of heap blocks the program never
//! released. An armed `main` also releases every droppable top-let global
//! right before its epilogue (func.rs): a top-let is live BY DESIGN until
//! exit, so the measurement mode drops it the way native's exit would, and
//! what remains is a leak. A shipped module never runs that release.
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
    static KEEP_TOP_LETS: Cell<bool> = const { Cell::new(false) };
}

/// Does the armed `main` release its top-let globals before returning?
/// Always, except under the negative-test hook below.
pub(crate) fn releases_top_lets() -> bool {
    armed() && !KEEP_TOP_LETS.with(|c| c.get())
}

/// Negative-test hook (tests/live_at_exit.rs): drop the armed `main`'s
/// top-let releases — one release the emitter omits — so the live-heap
/// gate has a leak to see. Thread-local; restores on drop.
#[doc(hidden)]
#[must_use = "the guard restores the previous state when dropped"]
pub struct KeepTopLetsGuard(bool);

impl KeepTopLetsGuard {
    pub fn set() -> Self {
        let prev = KEEP_TOP_LETS.with(|c| c.replace(true));
        Self(prev)
    }
}

impl Drop for KeepTopLetsGuard {
    fn drop(&mut self) {
        KEEP_TOP_LETS.with(|c| c.set(self.0));
    }
}

/// Is the counter being emitted into the module under emission?
pub fn armed() -> bool {
    ARMED.with(|c| c.get())
}

/// The exported counter globals, in the order they are appended after the
/// top-let globals (each an i64 starting at 0).
pub const EXPORTS: [&str; 5] = ["__alloc_count", "__alloc_reused", "__alloc_bytes", "__free_count", "__region_reclaimed"];

/// Offsets of each counter from the first counter's global index.
pub(crate) const COUNT: u32 = 0;
pub(crate) const REUSED: u32 = 1;
pub(crate) const BYTES: u32 = 2;
pub(crate) const FREES: u32 = 3;
pub(crate) const RECLAIMED: u32 = 4;

/// Declare the counter globals (i64, mutable, 0) when `counters` names
/// their first index — a no-op for a shipped module.
pub(crate) fn declare_globals(globals: &mut wasm_encoder::GlobalSection, counters: Option<u32>) {
    use wasm_encoder::{ConstExpr, GlobalType, ValType};
    if counters.is_none() {
        return;
    }
    for _ in EXPORTS {
        globals.global(
            GlobalType { val_type: ValType::I64, mutable: true, shared: false },
            &ConstExpr::i64_const(0),
        );
    }
}

/// Export the counters under their names, base index `counters` — a no-op
/// for a shipped module.
pub(crate) fn export_globals(exports: &mut wasm_encoder::ExportSection, counters: Option<u32>) {
    let Some(base) = counters else { return };
    for (k, name) in EXPORTS.iter().enumerate() {
        exports.export(name, wasm_encoder::ExportKind::Global, base + k as u32);
    }
}

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
