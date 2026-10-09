//! The per-thread intern caches under concurrent interning (#3512).
//!
//! A key handed out to one thread can be numbered above a key another thread
//! is still inserting. A cache that resolved every smaller key to keep a
//! dense prefix asked the shared interner for that in-flight key and
//! panicked with "Key out of bounds". Many threads intern fresh names and
//! resolve each one at once, so a gap below a fresh key is common.

use almide_base::intern::{resolve, sym};
use std::sync::{Arc, Barrier};

#[test]
fn concurrent_intern_and_resolve_never_reads_an_in_flight_key() {
    const THREADS: usize = 8;
    const PER_THREAD: usize = 20_000;
    let gate = Arc::new(Barrier::new(THREADS));
    let workers: Vec<_> = (0..THREADS)
        .map(|t| {
            let gate = Arc::clone(&gate);
            std::thread::spawn(move || {
                gate.wait();
                for i in 0..PER_THREAD {
                    let name = format!("intern_threads_{t}_{i}");
                    let s = sym(&name);
                    assert_eq!(resolve(s), name);
                    assert_eq!(sym(&name), s);
                }
            })
        })
        .collect();
    for w in workers {
        w.join().expect("a worker panicked");
    }
}
