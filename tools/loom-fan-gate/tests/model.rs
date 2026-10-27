//! The fan gate protocol over the interleavings loom enumerates. Each model
//! checks the protocol's two promises: no helper touches an offer's frame
//! after its caller tore it down (the `Live` witnesses, lib.rs), and every
//! block is taken by exactly one thread (the offer runs `blocks` blocks).
use loom_fan_gate::{helper_turn, offer};

const HELPER_TURNS: usize = 2;

/// `bound`: loom's preemption bound for this model — `None` enumerates every
/// interleaving. `LOOM_MAX_PREEMPTIONS` in the environment overrides it.
///
/// Measured on GCE n2-standard-8 (2026-10-27): the flat model is exhaustive in
/// ~45 s; the nested model took 17 s at bound 2 and 65 s at 3 and did not
/// finish unbounded in 300 s; the two-offer model took 24 s at bound 2 and
/// did not finish at 3 in 20 minutes. Bounded models are loom's standard
/// practice (most concurrency bugs need two or three preemptions) and are
/// recorded as such in this gate's `blind`.
fn builder(bound: Option<usize>) -> loom::model::Builder {
    let mut b = loom::model::Builder::new();
    b.max_branches = 1_000_000;
    if std::env::var_os("LOOM_MAX_PREEMPTIONS").is_none() {
        b.preemption_bound = bound;
    }
    b
}

/// The padded frame below, and nested offers, outgrow loom's default
/// coroutine stack: every model thread, the offering one too, gets this.
const STACK: usize = 1 << 20;

fn spawn<F: FnOnce() + Send + 'static>(f: F) -> loom::thread::JoinHandle<()> {
    loom::thread::Builder::new().stack_size(STACK).spawn(f).unwrap()
}

/// Run `body` on a model thread with a full-size stack.
fn model(bound: Option<usize>, body: fn()) {
    builder(bound).check(move || spawn(body).join().unwrap());
}

fn helper() -> loom::thread::JoinHandle<()> {
    spawn(|| {
        for _ in 0..HELPER_TURNS {
            if !helper_turn() {
                loom::thread::yield_now();
            }
        }
    })
}

/// Run the offer below a padded frame, so the caller's later calls (a join)
/// do not reuse the stack the offer's gate lived in: a helper that reaches a
/// torn-down gate then reads intact witnesses, and loom reports the access
/// instead of the harness faulting on recycled bytes.
#[inline(never)]
fn padded_offer(blocks: usize, workers: usize, nested: Option<usize>) -> (usize, bool) {
    let pad = std::hint::black_box([0u8; 16 * 1024]);
    let r = offer(blocks, workers, nested);
    std::hint::black_box(&pad);
    r
}

fn expect(blocks: usize, got: (usize, bool)) {
    assert!(!got.1, "a taker panicked");
    assert_eq!(got.0, blocks, "the offer ran {} of its {blocks} blocks", got.0);
}

#[test]
fn flat_offer_one_helper() {
    model(None, || {
        let h = helper();
        let r = padded_offer(2, 2, None);
        h.join().unwrap();
        expect(2, r);
    });
}

#[test]
fn nested_offer_one_helper() {
    model(Some(3), || {
        let h = helper();
        let r = padded_offer(2, 2, Some(2));
        h.join().unwrap();
        expect(2, r);
    });
}

#[test]
fn two_arms_one_helper() {
    model(Some(2), || {
        let h = helper();
        let arm = spawn(|| {
            let r = padded_offer(2, 2, None);
            expect(2, r);
        });
        let r = padded_offer(2, 2, None);
        arm.join().unwrap();
        h.join().unwrap();
        expect(2, r);
    });
}
