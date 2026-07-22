
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        verify_ownership, CallArg, Capability, Init, MirFunction, MirParam, Op, PrimKind, Repr, RtFn,
        ValueId, PLACEHOLDER_LAYOUT,
    };

    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }
    fn func(ops: Vec<Op>) -> MirFunction {
        MirFunction { name: "f".into(), ops, ..Default::default() }
    }

    #[test]
    fn value_rc_carrier_balance_is_certified() {
        // 柱C extension: `prim.handle(o)` makes the handle a CARRIER of o's object, so an UNBALANCED
        // rc_inc on it is now a Leak that BOTH verify_ownership and the cert catch — the Value-rc class
        // that used to be invisible in the prim region. `i`(alloc) + `a`(rc_inc on carrier) + `d`(drop)
        // = rc 1 at end → leak.
        let (o, h) = (ValueId(0), ValueId(1));
        let unbalanced = func(vec![
            Op::Alloc { dst: o, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![o] },
            Op::Prim { kind: PrimKind::RcInc, dst: None, args: vec![h] },
            Op::Drop { v: o },
        ]);
        assert_eq!(ownership_certificate(&unbalanced), "iad\n");
        assert!(verify_ownership(&unbalanced).is_err());

        // A BALANCED rc_inc/rc_dec on the carrier → both ACCEPT (rc 0 at end).
        let balanced = func(vec![
            Op::Alloc { dst: o, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![o] },
            Op::Prim { kind: PrimKind::RcInc, dst: None, args: vec![h] },
            Op::Prim { kind: PrimKind::RcDec, dst: None, args: vec![h] },
            Op::Drop { v: o },
        ]);
        assert_eq!(ownership_certificate(&balanced), "iadd\n");
        assert_eq!(verify_ownership(&balanced), Ok(()));

        // A load64-fed rc (NO prim.handle carrier) stays UNMODELED — the differential-test floor: the
        // RcInc on a non-carrier handle emits no `a` and the verifier no-ops it, so the function is
        // just the balanced Alloc+Drop ("id").
        let load_fed = func(vec![
            Op::Alloc { dst: o, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(h), args: vec![o] },
            Op::Prim { kind: PrimKind::RcInc, dst: None, args: vec![h] },
            Op::Drop { v: o },
        ]);
        assert_eq!(ownership_certificate(&load_fed), "id\n");
        assert_eq!(verify_ownership(&load_fed), Ok(()));
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
        // Alloc(i), Dup→alias(a), Drop(d), Drop(d): the alias acquire is `a`.
        assert_eq!(ownership_certificate(&f), "iadd\n");
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
    fn loop_carried_accumulator_folds_to_one_slot_stream() {
        // The heap-loop-carried accumulator (option C): `acc` is alloc'd, then each
        // iteration allocs a NEW object (the `acc + [x]` feeder), drops the OLD acc,
        // and rebinds `acc = new` via SetLocal; finally `acc` is returned (moved out).
        //   acc=Alloc; loop { new=Alloc; Drop acc; SetLocal acc,new }; ret acc
        // The slot folds to ONE stream `i(id)m` — acquire once; loop body acquire-new +
        // drop-old (a rc-preserving body); move out — accepted by the proven check_cert_lc.
        let (acc, new) = (ValueId(0), ValueId(1));
        let mut f = func(vec![
            Op::Alloc { dst: acc, repr: heap(), init: Init::Opaque },
            Op::LoopStart,
            Op::Alloc { dst: new, repr: heap(), init: Init::Opaque },
            Op::Drop { v: acc },
            Op::SetLocal { local: acc, src: new },
            Op::LoopEnd,
        ]);
        f.ret = Some(acc);
        assert_eq!(ownership_certificate(&f), "i(id)m\n");
        // The Rust-side checker accepts it too (SetLocal rebind preserves the slot
        // invariant) — its verdict matches the proven check_cert_lc on the cert.
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn loop_carried_leaky_body_is_rejected() {
        // A loop body that allocs but never drops the old acc → the slot stream is
        // `i(i)m` (loop body NOT rc-preserving: net +1) → REJECT, both here and in Coq.
        let (acc, new) = (ValueId(0), ValueId(1));
        let mut f = func(vec![
            Op::Alloc { dst: acc, repr: heap(), init: Init::Opaque },
            Op::LoopStart,
            Op::Alloc { dst: new, repr: heap(), init: Init::Opaque },
            Op::SetLocal { local: acc, src: new },
            Op::LoopEnd,
        ]);
        f.ret = Some(acc);
        assert_eq!(ownership_certificate(&f), "i(i)m\n");
        // verify_ownership flags the leaked old `acc` object (the dropped Alloc never
        // released before rebind) — the cert faithfully carries the rejection.
        assert!(verify_ownership(&f).is_err());
    }

    #[test]
    fn swap_carried_buffer_folds_dup_feeder_into_the_slot_stream() {
        // The SWAP-CARRY shape (`cur = merged`, loop_buffer_churn / C-131): since the
        // whole-var alias-edge elision the rebind lowers as `Dup tmp = merged;
        // Drop cur; SetLocal cur = tmp` — the slot's feeder is a DUP dst, not an
        // Alloc/call result. The Dup's `a` must route into the slot stream so the
        // per-iteration acquire-new + drop-old reads `(ad)` (rc-preserving); flat, the
        // in-loop drop-old + scope-end drop read `idd` — a FALSE double-free (the
        // corpus-wall REJECT the first develop Trust Spine run caught).
        let (cur, merged, tmp) = (ValueId(0), ValueId(1), ValueId(2));
        let f = func(vec![
            Op::Alloc { dst: cur, repr: heap(), init: Init::Opaque },
            Op::LoopStart,
            Op::Alloc { dst: merged, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: tmp, src: merged },
            Op::Drop { v: cur },
            Op::SetLocal { local: cur, src: tmp },
            Op::Drop { v: merged },
            Op::LoopEnd,
            Op::Drop { v: cur },
        ]);
        assert_eq!(ownership_certificate(&f), "i(ad)d\nid\n");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn swap_carry_without_drop_old_is_rejected() {
        // The same swap-carry but the OLD buffer is never dropped in the body — a
        // real leak: the slot stream reads `i(a)d` (body nets +1, not rc-preserving)
        // → REJECT, and verify_ownership flags the leaked original object.
        let (cur, merged, tmp) = (ValueId(0), ValueId(1), ValueId(2));
        let f = func(vec![
            Op::Alloc { dst: cur, repr: heap(), init: Init::Opaque },
            Op::LoopStart,
            Op::Alloc { dst: merged, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: tmp, src: merged },
            Op::SetLocal { local: cur, src: tmp },
            Op::Drop { v: merged },
            Op::LoopEnd,
            Op::Drop { v: cur },
        ]);
        assert_eq!(ownership_certificate(&f), "i(a)d\nid\n");
        assert!(verify_ownership(&f).is_err());
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

    /// Re-run the proven checker's decision in Rust (mirrors the Coq `check_bc`):
    /// every line's stream must never dec-below-zero and must end at 0, with the
    /// format-v4 branch rule — `{then|else}` arms both execute from the current
    /// count, must not fault, and must AGREE on the leaving count.
    fn cert_all_balanced(cert: &str) -> bool {
        // The flat fold (format v1 alphabet + the 5b `b` guard); None = fault.
        fn fold(seg: &str, mut rc: i64) -> Option<i64> {
            for c in seg.chars() {
                match c {
                    // i/a = +1 (fresh/alias), d/m = −1 (release/move-out).
                    'i' | 'a' => rc += 1,
                    'd' | 'm' => {
                        if rc == 0 {
                            return None; // double-free / use-after-move
                        }
                        rc -= 1;
                    }
                    // b = +0 live use — faults on a dead object (use-after-free),
                    // exactly the Coq `Borrow` guard.
                    'b' => {
                        if rc == 0 {
                            return None;
                        }
                    }
                    _ => {}
                }
            }
            Some(rc)
        }
        cert.lines().all(|line| {
            let mut rc: i64 = 0;
            let mut rest = line;
            while let Some(open) = rest.find('{') {
                rc = match fold(&rest[..open], rc) {
                    Some(r) => r,
                    None => return false,
                };
                let close = match rest[open..].find('}') {
                    Some(c) => open + c,
                    None => return false, // unterminated branch — malformed
                };
                let (t, e) = match rest[open + 1..close].split_once('|') {
                    Some(p) => p,
                    None => return false,
                };
                match (fold(t, rc), fold(e, rc)) {
                    (Some(rt), Some(re)) if rt == re => rc = rt, // arms AGREE
                    _ => return false, // an arm faults or the arms disagree
                }
                rest = &rest[close + 1..];
            }
            match fold(rest, rc) {
                Some(r) => r == 0, // leak iff != 0
                None => false,
            }
        })
    }

    /// A tiny seeded PRNG (no dep), so the random test is deterministic.
    fn next_rand(state: &mut u64) -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *state
    }

    /// Build a random ownership op sequence over LIVE handles, now including
    /// BRANCH regions (format v4): agreeing arms (both alias the same object —
    /// grouped `{a|a}`, net +1), per-arm self-balancing arms (flat flush), and
    /// occasionally DISAGREEING arms (`{a|}` — both the grouped cert and
    /// verify_ownership's branch join must reject). Leftover-undropped handles
    /// make it a leak — so the corpus spans accept and reject across the flat,
    /// borrow and branch machinery, and the test pins that the cert verdict
    /// EQUALS verify_ownership's on every seed.
    fn gen_wellformed(seed: u64) -> MirFunction {
        let mut st = seed.wrapping_add(1);
        let mut live: Vec<ValueId> = Vec::new();
        let mut next: u32 = 0;
        let mut ops: Vec<Op> = Vec::new();
        let steps = 3 + (next_rand(&mut st) % 9) as usize;
        for _ in 0..steps {
            let choice = next_rand(&mut st) % 6;
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
                3 if !live.is_empty() => {
                    // Borrow (a `b` event on the owned stream — liveness-guarded).
                    let v = live[(next_rand(&mut st) as usize) % live.len()];
                    ops.push(Op::Borrow { v });
                }
                4 if !live.is_empty() => {
                    // An AGREEING branch: each arm acquires one alias of the same
                    // live object (net +1 both ways — the heap-result-branch
                    // class, grouped `{a|a}`). The runtime holds ONE new alias
                    // whichever arm ran; hand `y` to the pool (`z` is the other
                    // path's handle — same object, never used again).
                    let x = live[(next_rand(&mut st) as usize) % live.len()];
                    let (c, y, z) = (ValueId(next), ValueId(next + 1), ValueId(next + 2));
                    next += 3;
                    ops.push(Op::Const { dst: c });
                    ops.push(Op::IfThen { cond: c, dst: None });
                    ops.push(Op::Dup { dst: y, src: x });
                    ops.push(Op::Else { val: None });
                    ops.push(Op::Dup { dst: z, src: x });
                    ops.push(Op::EndIf { val: None });
                    live.push(y);
                }
                5 if !live.is_empty() && next_rand(&mut st) % 3 == 0 => {
                    // A DISAGREEING branch (one arm aliases, the other does not —
                    // a path-dependent count): the grouped cert `{a|}` and the
                    // branch join must BOTH reject, and both are sticky, so the
                    // verdicts stay equal however generation continues.
                    let x = live[(next_rand(&mut st) as usize) % live.len()];
                    let (c, y) = (ValueId(next), ValueId(next + 1));
                    next += 2;
                    ops.push(Op::Const { dst: c });
                    ops.push(Op::IfThen { cond: c, dst: None });
                    ops.push(Op::Dup { dst: y, src: x });
                    ops.push(Op::Else { val: None });
                    ops.push(Op::EndIf { val: None });
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

    include!("certificate_p2.rs");
}
