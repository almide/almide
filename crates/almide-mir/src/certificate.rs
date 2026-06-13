//! Ownership certificate emission — the seam between the untrusted compiler and
//! the KERNEL-PROVEN checker (proofs/, the v1 flight-grade spine).
//!
//! `ownership_certificate` projects a function's MIR ownership ops to the
//! per-object refcount-event stream (certificate format v0): one line per
//! reference-counted OBJECT, `i` = an ownership +1 (Alloc/Dup), `d` = a −1
//! (Drop/Consume, and the move-out of a heap return). This is the SAME
//! per-object accounting [`crate::verify_ownership`] enforces — but emitted as a
//! portable certificate the proven Coq checker `check_all` re-verifies. So each
//! build's memory-safety is re-checkable by a proven artifact, not just by the
//! (untrusted) compiler's own pass.
//!
//! By construction the proven checker accepts `ownership_certificate(f)` iff
//! `verify_ownership(f)` accepts (same invariant); the unit tests pin that
//! correspondence, and `proofs/gate.sh` runs the actual proven binary on it.

use crate::{MirFunction, Op, ValueId};
use std::collections::BTreeMap;

/// Per-object refcount-event accumulator, preserving object creation order.
struct Streams {
    of: BTreeMap<ValueId, ValueId>, // handle → object representative
    order: Vec<ValueId>,            // objects in first-seen order
    stream: BTreeMap<ValueId, String>,
}

impl Streams {
    fn new() -> Self {
        Streams { of: BTreeMap::new(), order: Vec::new(), stream: BTreeMap::new() }
    }
    /// Record a +1/−1 event (`'i'`/`'d'`) on object `o`.
    fn event(&mut self, o: ValueId, c: char) {
        if !self.stream.contains_key(&o) {
            self.stream.insert(o, String::new());
            self.order.push(o);
        }
        self.stream.get_mut(&o).unwrap().push(c);
    }
    fn object_of(&self, handle: ValueId) -> ValueId {
        // Well-formed MIR always has the handle mapped; fall back to identity so a
        // malformed input yields an unbalanced (rejected) certificate rather than
        // a panic.
        self.of.get(&handle).copied().unwrap_or(handle)
    }
}

/// Emit the per-object ownership certificate (format v0) for a function.
pub fn ownership_certificate(func: &MirFunction) -> String {
    let mut s = Streams::new();

    // Heap params arrive OWNED (a +1 the caller transferred).
    for p in &func.params {
        if p.repr.is_heap() {
            s.of.insert(p.value, p.value);
            s.event(p.value, 'i');
        }
    }

    for op in &func.ops {
        match op {
            Op::Alloc { dst, .. } => {
                s.of.insert(*dst, *dst);
                s.event(*dst, 'i');
            }
            Op::Dup { dst, src } => {
                let o = s.object_of(*src);
                s.of.insert(*dst, o);
                s.event(o, 'i');
            }
            Op::Drop { v } | Op::Consume { v } => {
                let o = s.object_of(*v);
                s.event(o, 'd');
            }
            // No refcount change: Borrow/MakeUnique/Const/Pure/Call/CallFn/IntBinOp.
            _ => {}
        }
    }

    // A heap return is MOVED OUT to the caller (a −1).
    if let Some(r) = func.ret {
        if s.of.contains_key(&r) {
            let o = s.object_of(r);
            s.event(o, 'd');
        }
    }

    let mut out = String::new();
    for o in &s.order {
        out.push_str(&s.stream[o]);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify_ownership, Init, MirFunction, MirParam, Op, Repr, ValueId, PLACEHOLDER_LAYOUT};

    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }
    fn func(ops: Vec<Op>) -> MirFunction {
        MirFunction { name: "f".into(), ops, ..Default::default() }
    }

    #[test]
    fn alias_then_drops_is_one_balanced_object() {
        // a = Alloc; b = Dup a; Drop a; Drop b  → ONE object (a), stream "iidd".
        let (a, b) = (ValueId(0), ValueId(1));
        let f = func(vec![
            Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: b, src: a },
            Op::Drop { v: a },
            Op::Drop { v: b },
        ]);
        assert_eq!(ownership_certificate(&f), "iidd\n");
        assert_eq!(verify_ownership(&f), Ok(())); // checker would accept ⟺ this
    }

    #[test]
    fn two_objects_each_balanced() {
        let (a, b) = (ValueId(0), ValueId(1));
        let f = func(vec![
            Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
            Op::Alloc { dst: b, repr: heap(), init: Init::Opaque },
            Op::Drop { v: a },
            Op::Drop { v: b },
        ]);
        // object a: "id", object b: "id" — two balanced lines.
        assert_eq!(ownership_certificate(&f), "id\nid\n");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn leak_shows_as_unbalanced_object() {
        // a allocated, never dropped → stream "i" (rc ends 1 = leak).
        let a = ValueId(0);
        let f = func(vec![Op::Alloc { dst: a, repr: heap(), init: Init::Opaque }]);
        assert_eq!(ownership_certificate(&f), "i\n");
        // verify_ownership flags it too — the certificate faithfully carries it.
        assert!(verify_ownership(&f).is_err());
    }

    // ── faithfulness mechanism ──
    // The certificate must honestly represent the ownership pass: the proven
    // checker's verdict on `ownership_certificate(f)` must equal `verify_ownership(f)`'s.
    // Otherwise the PCC chain certifies the wrong thing. We pin it over many
    // random WELL-FORMED ownership sequences.

    /// Re-run the proven checker's decision in Rust (mirrors the Coq `check_all`):
    /// every line's i/d stream must never dec-below-zero and must end at 0.
    fn cert_all_balanced(cert: &str) -> bool {
        cert.lines().all(|line| {
            let mut rc: i64 = 0;
            for c in line.chars() {
                match c {
                    'i' => rc += 1,
                    'd' => {
                        if rc == 0 {
                            return false; // double-free
                        }
                        rc -= 1;
                    }
                    _ => {}
                }
            }
            rc == 0 // leak iff != 0
        })
    }

    /// A tiny seeded PRNG (no dep), so the random test is deterministic.
    fn next_rand(state: &mut u64) -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *state
    }

    /// Build a random WELL-FORMED ownership op sequence (only dup/drop LIVE
    /// handles). Leftover-undropped handles make it a leak; dropping all makes it
    /// balanced — so the corpus spans accept and reject.
    fn gen_wellformed(seed: u64) -> MirFunction {
        let mut st = seed.wrapping_add(1);
        let mut live: Vec<ValueId> = Vec::new();
        let mut next: u32 = 0;
        let mut ops: Vec<Op> = Vec::new();
        let steps = 3 + (next_rand(&mut st) % 9) as usize;
        for _ in 0..steps {
            let choice = next_rand(&mut st) % 4;
            match choice {
                0 => {
                    // Alloc a fresh object.
                    let v = ValueId(next);
                    next += 1;
                    ops.push(Op::Alloc { dst: v, repr: heap(), init: Init::Opaque });
                    live.push(v);
                }
                1 if !live.is_empty() => {
                    // Dup a live handle → a new handle on the same object.
                    let src = live[(next_rand(&mut st) as usize) % live.len()];
                    let v = ValueId(next);
                    next += 1;
                    ops.push(Op::Dup { dst: v, src });
                    live.push(v);
                }
                2 if !live.is_empty() => {
                    // Drop a live handle.
                    let i = (next_rand(&mut st) as usize) % live.len();
                    let v = live.remove(i);
                    ops.push(Op::Drop { v });
                }
                _ if !live.is_empty() => {
                    // Borrow (no refcount change — must be skipped by the cert).
                    let v = live[(next_rand(&mut st) as usize) % live.len()];
                    ops.push(Op::Borrow { v });
                }
                _ => {}
            }
        }
        func(ops)
    }

    #[test]
    fn certificate_verdict_matches_verify_ownership() {
        for seed in 0u64..500 {
            let f = gen_wellformed(seed);
            let cert_ok = cert_all_balanced(&ownership_certificate(&f));
            let verify_ok = verify_ownership(&f).is_ok();
            assert_eq!(
                cert_ok, verify_ok,
                "seed {seed}: certificate says {cert_ok}, verify_ownership says {verify_ok}\nops: {:?}",
                f.ops
            );
        }
    }

    #[test]
    fn heap_param_owned_and_returned_balances() {
        // fn(p: heap) -> p : param arrives owned (+1), returned = moved out (−1).
        let p = ValueId(0);
        let f = MirFunction {
            name: "id".into(),
            params: vec![MirParam { value: p, repr: heap() }],
            ops: vec![],
            ret: Some(p),
        };
        assert_eq!(ownership_certificate(&f), "id\n");
        assert_eq!(verify_ownership(&f), Ok(()));
    }
}
