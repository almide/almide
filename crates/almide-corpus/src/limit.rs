//! A per-fixture wall-clock limit for the corpus sweeps (#3768).
//!
//! A sweep that runs every fixture in one loop with no limit turns one hung
//! program into a hung job: the job's own timeout cancels it and the log names
//! the test, never the fixture. Each fixture here runs under its own limit,
//! and on expiry the sweep fails naming the fixture's path and the limit — a
//! hang is a failure, not a wait (#2567's rule for the output-parity sweep).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

/// The limit every corpus sweep in almide-spine runs one fixture under.
///
/// Chosen from measured walls, not from #2567's 60 s (that sweep times one
/// wasmtime run; these run a whole interpreter leg in-process): the heaviest
/// fixture, `string_position_large_offset`, takes 521 s on the CI runner
/// (`run_parity` column of proofs/corpus-weights.txt) and 47 s in a local
/// release build, and a runner shared with other suites ran the corpus
/// giants up to 1.95× slower (#2381). 1200 s is 2.3× that 521 s. It also
/// stays under the job: a run_parity shard takes 11–16 min, so a hang that
/// starts anywhere in it is cut and named by minute ~36, before the
/// 40-minute cancellation that hid the fixture in #3768.
pub const FIXTURE_LIMIT: Duration = Duration::from_secs(1200);

/// The stop a cut sets: the work polls it (or hands [`Stop::flag`] to a
/// run that polls it, as the interpreter's step meter does) and returns.
#[derive(Clone, Debug, Default)]
pub struct Stop(Arc<AtomicBool>);

impl Stop {
    /// Whether the run was cut and should return now.
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// The shared flag itself, for a run that polls an `AtomicBool`.
    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.0)
    }
}

/// How long a cut run is given to honour its [`Stop`] before the cut is
/// reported as one that did not stop (#3809): the limit itself when that is
/// shorter, so a short test limit is not outwaited by its own grace.
pub const STOP_GRACE: Duration = Duration::from_secs(30);

/// Run one fixture's `work` under `limit`, naming `fixture` if it is cut.
///
/// The work runs on a thread of its own and is handed a [`Stop`]; the caller
/// waits at most `limit` for its result. On expiry the stop is set and the
/// caller waits up to [`STOP_GRACE`] for the thread to finish (#3809), so a
/// cut fixture stops instead of spinning beside the rest of the test process.
/// Then this panics with the fixture's path and the limit, and says whether
/// the thread stopped — a work that never polls its stop is left behind, and
/// the message says so. A panic inside the work is re-raised here with its
/// own message.
pub fn run_limited<R: Send + 'static>(
    fixture: &str,
    limit: Duration,
    work: impl FnOnce(Stop) -> R + Send + 'static,
) -> R {
    let stop = Stop::default();
    let handed = stop.clone();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .name(format!("fixture {fixture}"))
        .spawn(move || {
            let _ = tx.send(work(handed));
        })
        .unwrap_or_else(|e| panic!("{fixture}: cannot start its thread: {e}"));
    match rx.recv_timeout(limit) {
        Ok(r) => r,
        Err(RecvTimeoutError::Timeout) => {
            stop.0.store(true, Ordering::Relaxed);
            let grace = limit.min(STOP_GRACE);
            let after = match rx.recv_timeout(grace) {
                // Finished (with a result or a panic): reap the thread, so a
                // reported stop is a stopped thread.
                Ok(_) | Err(RecvTimeoutError::Disconnected) => {
                    let _ = worker.join();
                    "cut and stopped".to_string()
                }
                Err(RecvTimeoutError::Timeout) => {
                    format!("cut, but it did not stop within {grace:?} of its stop (#3809) and is left running")
                }
            };
            panic!("{fixture}: still running after the per-fixture limit of {limit:?} (#3768) — {after}")
        }
        Err(RecvTimeoutError::Disconnected) => match worker.join() {
            Err(payload) => std::panic::resume_unwind(payload),
            Ok(()) => panic!("{fixture}: its thread ended without a result"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;

    fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
        p.downcast_ref::<String>()
            .cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    /// The helper is driven from a thread of its own with an outer deadline,
    /// so a helper that does not cut the run fails this test instead of
    /// hanging it.
    fn cut_message(fixture: &'static str, limit: Duration) -> String {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let r = std::panic::catch_unwind(|| {
                run_limited(fixture, limit, |_: Stop| -> () {
                    loop {
                        thread::sleep(Duration::from_millis(5));
                    }
                })
            });
            let _ = tx.send(r.err().map(panic_text));
        });
        rx.recv_timeout(Duration::from_secs(10))
            .expect("run_limited did not cut a non-terminating run within 10 s")
            .expect("a cut run must fail, not return")
    }

    #[test]
    fn a_non_terminating_run_is_cut_and_named_by_its_path_and_limit() {
        let msg = cut_message("spec/lang/loops_forever.almd", Duration::from_millis(200));
        assert!(msg.contains("spec/lang/loops_forever.almd"), "the fixture is not named: {msg}");
        assert!(msg.contains("200ms"), "the limit is not named: {msg}");
        // This work never polls its stop: the cut says it is left running.
        assert!(msg.contains("did not stop"), "a run that ignores its stop is reported as stopped: {msg}");
    }

    /// #3809: a cut run must STOP, not keep a core busy for the rest of the
    /// test process. The work spins on a shared counter and honours the stop
    /// it is handed; after the cut the counter must stand still.
    #[test]
    fn a_cut_run_stops_its_thread() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        let ticks = Arc::new(AtomicU64::new(0));
        let seen = Arc::clone(&ticks);
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let r = std::panic::catch_unwind(move || {
                run_limited("spec/lang/spins_forever.almd", Duration::from_millis(200), move |stop: Stop| {
                    while !stop.is_set() {
                        seen.fetch_add(1, Ordering::Relaxed);
                        std::hint::spin_loop();
                    }
                })
            });
            let _ = tx.send(r.err().map(panic_text));
        });
        let msg = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("run_limited did not cut a non-terminating run within 10 s")
            .expect("a cut run must fail, not return");
        assert!(msg.contains("spec/lang/spins_forever.almd") && msg.contains("200ms"), "{msg}");
        assert!(msg.contains("cut and stopped"), "{msg}");
        let before = ticks.load(Ordering::Relaxed);
        thread::sleep(Duration::from_millis(300));
        let after = ticks.load(Ordering::Relaxed);
        assert!(before > 0, "the work never ran");
        assert_eq!(before, after, "the cut run's thread is still running after the cut");
    }

    #[test]
    fn a_finished_run_returns_its_value_and_a_panicking_run_keeps_its_message() {
        assert_eq!(run_limited("spec/lang/quick.almd", Duration::from_secs(5), |_: Stop| 41 + 1), 42);
        let p = std::panic::catch_unwind(|| run_limited("spec/lang/boom.almd", Duration::from_secs(5), |_: Stop| -> () { panic!("boom") }))
            .expect_err("the work's panic reaches the sweep");
        assert_eq!(panic_text(p), "boom");
    }
}
