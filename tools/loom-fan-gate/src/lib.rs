//! The fan gate protocol under loom (security row 6). The protocol is the
//! runtime's own source, extracted by build.rs; this file supplies what the
//! extracted span calls outside itself, and the shim it is rewritten onto.
//!
//! Every shim object carries a `Live` witness: each access to it is a loom
//! READ of the witness, and dropping it is a WRITE. So a helper that touches a
//! gate (or an offer's block queue, or its borrowed taking loop) without its
//! access happening-before the caller's frame being torn down is a loom
//! causality violation — the use-after-free the gate protocol exists to rule
//! out, found over every interleaving loom enumerates instead of the ones a
//! scheduler seed happens to draw.
#![allow(dead_code, unused_doc_comments, non_snake_case, clippy::all)]

pub mod shim;

/// Output buffering and the fan sinks are per-thread state with no part in
/// the gate protocol.
pub fn almide_stdout_flush() {}
pub type AlmideFanSink = ();
pub fn almide_fan_current() -> Option<AlmideFanSink> {
    None
}
pub fn almide_fan_adopt(_sink: Option<AlmideFanSink>) {}
/// The model spawns its helper threads itself; they enter through
/// `almide_fan_steal` exactly as a pool thread does.
pub fn almide_fan_pool_wake(_helpers: usize) {}
pub fn almide_rt_list_par_workers(_len: usize) -> usize {
    4
}

include!(concat!(env!("OUT_DIR"), "/gate.rs"));

/// A pool thread's turn, as `almide_fan_hand_loop` takes it: enter a
/// published offer with blocks left, help it, leave.
pub fn helper_turn() -> bool {
    match almide_fan_steal(|_| true) {
        Some(gate) => {
            almide_fan_help(gate);
            true
        }
        None => false,
    }
}

/// An offer of `blocks` blocks on `workers` workers whose work counts the
/// blocks it ran; returns that count and whether any taker panicked.
pub fn offer(blocks: usize, workers: usize, nested: Option<usize>) -> (usize, bool) {
    let ran = loom::sync::atomic::AtomicUsize::new(0);
    let outcomes = almide_fan_deal((0..blocks).collect::<Vec<usize>>(), workers, |it| {
        for _ in it {
            if let Some(inner) = nested {
                let (n, panicked) = offer(inner, workers, None);
                assert_eq!(n, inner, "a nested offer ran {n} of its {inner} blocks");
                assert!(!panicked, "a nested offer's taker panicked");
            }
            ran.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    });
    let panicked = outcomes.iter().any(Result::is_err);
    (ran.load(std::sync::atomic::Ordering::SeqCst), panicked)
}
