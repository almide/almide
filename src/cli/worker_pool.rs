//! The bounded worker pool every parallel test lane runs on.
//!
//! Each lane (`cmd_test`'s compile and run phases, `cmd_test_wasm`, and
//! `cmd_test_fast`'s wasm and native-fallback phases) used to carry its own
//! copy of the same thread + slot-channel + result-channel scaffolding; the
//! copies differed only in the per-item work. This module is that scaffolding,
//! once.

use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};

/// How many workers a lane runs at once: one per available CPU, or 4 when the
/// count cannot be read.
pub(super) fn cpu_slots() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

/// Run `work` over every item, one thread per item, at most `slots` of them
/// working at once. Results come back in COMPLETION order — callers that print
/// them sort first, so a parallel run's transcript stays deterministic.
pub(super) fn bounded_parallel<T, R, F>(items: Vec<T>, slots: usize, work: F) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> R + Send + Sync + 'static,
{
    let work = Arc::new(work);
    let (tx, rx) = mpsc::channel();
    let (slot_tx, slot_rx) = mpsc::sync_channel::<()>(slots);
    for _ in 0..slots {
        let _ = slot_tx.send(());
    }
    let slot_tx = Arc::new(slot_tx);
    let slot_rx = Arc::new(Mutex::new(slot_rx));
    let handles: Vec<_> = items
        .into_iter()
        .map(|item| {
            let (tx, work) = (tx.clone(), Arc::clone(&work));
            let (slot_tx, slot_rx) = (Arc::clone(&slot_tx), Arc::clone(&slot_rx));
            std::thread::spawn(move || {
                // The lock is held only across `recv`, which cannot panic, so a
                // poisoned lock is unreachable; taking the guard either way
                // keeps a waiting worker from dying with its item unreported.
                let _ = slot_rx.lock().unwrap_or_else(PoisonError::into_inner).recv();
                let result = work(item);
                let _ = slot_tx.send(());
                let _ = tx.send(result);
            })
        })
        .collect();
    drop(tx);
    let results: Vec<R> = rx.iter().collect();
    for h in handles {
        let _ = h.join();
    }
    results
}

/// Run one test-harness worker's job, turning a panic inside the compiler
/// into `on_panic(message)` (#2533). A worker thread that panicked used to
/// die without sending its result, so the file dropped out of the tally
/// entirely — the harness then printed "All N test file(s) passed" over a
/// file that never compiled — and the semaphore permit it held was lost.
pub(super) fn guard_worker_panic<T>(job: impl FnOnce() -> T, on_panic: impl FnOnce(String) -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).unwrap_or_else(|payload| {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(non-string panic payload)".to_string());
        on_panic(format!("the compiler panicked (this is an Almide bug): {msg}"))
    })
}

#[cfg(test)]
mod worker_panic_tests {
    use super::guard_worker_panic;

    /// #2533: a worker whose compile panicked must come back as a failure the
    /// tally counts, not vanish (the harness printed "All N passed" over it).
    #[test]
    fn a_panicking_worker_becomes_a_counted_failure() {
        let got: Result<u32, String> =
            guard_worker_panic(|| panic!("Postcondition violation after pass 'X'"), Err);
        let msg = got.expect_err("a panic must surface as the failure value");
        assert!(msg.contains("the compiler panicked"), "{msg}");
        assert!(msg.contains("Postcondition violation after pass 'X'"), "{msg}");
    }

    #[test]
    fn a_normal_worker_passes_through() {
        let got: Result<u32, String> = guard_worker_panic(|| Ok(7), Err);
        assert_eq!(got, Ok(7));
    }
}
