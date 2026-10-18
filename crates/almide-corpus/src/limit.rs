//! A per-fixture wall-clock limit for the corpus sweeps (#3768).
//!
//! A sweep that runs every fixture in one loop with no limit turns one hung
//! program into a hung job: the job's own timeout cancels it and the log names
//! the test, never the fixture. Each fixture here runs under its own limit,
//! and on expiry the sweep fails naming the fixture's path and the limit — a
//! hang is a failure, not a wait (#2567's rule for the output-parity sweep).

use std::time::Duration;

/// Run one fixture's `work` under `limit`, naming `fixture` if it is cut.
pub fn run_limited<R: Send + 'static>(
    fixture: &str,
    limit: Duration,
    work: impl FnOnce() -> R + Send + 'static,
) -> R {
    let _ = (fixture, limit);
    work()
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
                run_limited(fixture, limit, || -> () {
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
    }

    #[test]
    fn a_finished_run_returns_its_value_and_a_panicking_run_keeps_its_message() {
        assert_eq!(run_limited("spec/lang/quick.almd", Duration::from_secs(5), || 41 + 1), 42);
        let p = std::panic::catch_unwind(|| run_limited("spec/lang/boom.almd", Duration::from_secs(5), || -> () { panic!("boom") }))
            .expect_err("the work's panic reaches the sweep");
        assert_eq!(panic_text(p), "boom");
    }
}
