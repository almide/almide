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
