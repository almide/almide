//! The heap-cap knob (#1530, attack-list A1-5): a harness-set ceiling on the
//! wasm bump frontier, baked into the rendered module at render time. A leak
//! that only ever showed as slow bloat — a dropped `rc_dec` starving the
//! free-list so churn bumps fresh blocks forever — meets the ceiling as the
//! DEFINED `$oom` abort ("Error: out of memory", exit 1) at a deterministic
//! iteration instead. 0 (the default) means no ceiling, and the rendered
//! prelude is byte-identical to a build without the knob, so no existing
//! byte-pinned gate sees it.
//!
//! THREAD-LOCAL with a scoped guard, for exactly the reasons
//! [`crate::lower::StrictValuesGuard`] documents: a process-global leaks the
//! knob across `cargo test` threads, and a bare setter leaks it across tests
//! sharing one thread under `--test-threads=1` — a stray cap in an unrelated
//! render would be an ORDER-DEPENDENT wrong-output bug.

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
