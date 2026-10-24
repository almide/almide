//! One linear-memory budget per guest (#4135).
//!
//! A run is more than one wasmtime store: the run's own instance, the
//! instance-parallel fan's worker instances (`host_fan.rs`, up to the
//! available parallelism) and a `testing.assert_aborts` site's child
//! instance (op 117). Each store used to carry its own
//! `StoreLimits::memory_size(cap)`, so N workers could together hold N × the
//! cap the run was given. Every store of a run now meters its linear-memory
//! growth against ONE shared [`Budget`]: a growth is granted only when the
//! sum over all of the run's live stores stays within the cap.
//!
//! What a refusal means depends on who asked:
//! - a fan worker's refused growth fails its chunk, and a failed offer runs
//!   sequentially in the run's own instance — unobservable, the fan's own
//!   fallback rule;
//! - the run's own instance first takes back the memory its idle pooled fan
//!   workers hold (only pooled workers exist while it runs: an offer runs
//!   inside a host call, with the guest stopped), so a single instance can
//!   still reach the whole cap exactly as before;
//! - a refusal that remains is the guest's `memory.grow` answering -1, which
//!   the emitted allocator turns into C-197's `Error: out of memory`, exit 1.
//!
//! A run that names no cap (the product runner, `almide run --target wasm`)
//! gets the wasm32 address space, 4 GiB, as its budget: no single instance
//! can exceed it anyway, so nothing a single instance does changes, and the
//! run's instances together can no longer reach a multiple of it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The wasm32 address space: the most one instance's memory can reach, and
/// the budget of a run that names none.
pub(crate) const WASM32_SPACE: usize = 1 << 32;

/// A run's linear-memory budget, shared by every store of the run.
pub(crate) struct Budget {
    cap: usize,
    used: AtomicUsize,
}

impl Budget {
    pub(crate) fn new(cap: Option<usize>) -> Arc<Self> {
        Arc::new(Budget { cap: cap.unwrap_or(WASM32_SPACE), used: AtomicUsize::new(0) })
    }

    /// The bytes all of the run's live stores hold now.
    #[cfg(test)]
    pub(crate) fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }
}

/// One store's share of its run's [`Budget`]: what it was granted, given
/// back when the store is dropped.
pub(crate) struct Meter {
    budget: Arc<Budget>,
    mine: usize,
    /// The last grant, given back if wasmtime then fails the growth.
    last: usize,
}

impl Meter {
    pub(crate) fn new(budget: Arc<Budget>) -> Self {
        Meter { budget, mine: 0, last: 0 }
    }

    /// Grant `delta` more bytes if the run's total stays within the cap.
    pub(crate) fn reserve(&mut self, delta: usize) -> bool {
        let cap = self.budget.cap;
        let granted = self
            .budget
            .used
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |u| u.checked_add(delta).filter(|&n| n <= cap))
            .is_ok();
        if granted {
            self.mine += delta;
            self.last = delta;
        }
        granted
    }

    /// Give back the last grant (wasmtime could not perform the growth).
    pub(crate) fn undo_last(&mut self) {
        self.budget.used.fetch_sub(self.last, Ordering::SeqCst);
        self.mine -= self.last;
        self.last = 0;
    }
}

impl Drop for Meter {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.mine, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::{Budget, Meter};

    #[test]
    fn two_meters_share_one_cap_and_give_it_back_when_dropped() {
        let b = Budget::new(Some(100));
        let mut one = Meter::new(b.clone());
        let mut two = Meter::new(b.clone());
        assert!(one.reserve(60));
        assert!(!two.reserve(41), "60 + 41 exceeds the one budget");
        assert!(two.reserve(40));
        assert_eq!(b.used(), 100);
        two.undo_last();
        assert_eq!(b.used(), 60);
        drop(one);
        assert_eq!(b.used(), 0);
        assert!(two.reserve(100));
    }
}
