//! The heap-cap knob for the STRUCTURAL leg (#1729, the twin of
//! `almide_mir::heap_cap`): a harness-set ceiling on linear memory, baked
//! into the emitted module as the memory's declared MAXIMUM. Growth past it
//! makes `memory.grow` answer -1, which the allocator turns into the DEFINED
//! C-197 "Error: out of memory" + exit 1 — the same observable the incumbent
//! leg's frontier ceiling produces. 0 (the default) means no ceiling and the
//! memory section is byte-identical to a build without the knob.
//!
//! Before this knob existed the structural leg silently IGNORED
//! `--heap-cap`; the static-memory gate's glutton still aborted, but only
//! because the assign leak (#1729) burned the 4 GiB address space — the cap
//! was never what stopped it. THREAD-LOCAL with a scoped guard, for the
//! same cross-test hygiene the mir twin documents.

thread_local! {
    static HEAP_CAP: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The active cap in bytes; 0 = no ceiling.
pub fn heap_cap() -> u32 {
    HEAP_CAP.with(|c| c.get())
}

pub fn set_heap_cap(bytes: u32) {
    HEAP_CAP.with(|c| c.set(bytes));
}

/// Set the cap for a scope and restore the previous value on drop.
#[must_use = "the guard restores the previous cap when dropped; binding it to `_` restores immediately"]
pub struct HeapCapGuard(u32);

impl HeapCapGuard {
    pub fn set(bytes: u32) -> Self {
        let prev = heap_cap();
        set_heap_cap(bytes);
        Self(prev)
    }
}

impl Drop for HeapCapGuard {
    fn drop(&mut self) {
        set_heap_cap(self.0);
    }
}
