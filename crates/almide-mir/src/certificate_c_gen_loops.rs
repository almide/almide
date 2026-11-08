// the loop-slot rebind generator of certificate_c_gen.rs, include!-spliced back into
// the same test module (a code move under the 800-line ceiling the codopsy gate holds
// this crate to).

/// #3298: slot rebinds (in a loop, or straight-line) over every root kind — a borrowed param,
/// a payload loaded out of an owned block, an owned `Dup` of either (the
/// lowering's pre-loop copy), or a fresh object — with the drop-old, the
/// feeder kind and the scope-end release drawn at random. The certificate's
/// slot fold and `verify_ownership` must agree on every draw.
fn gen_loop_slots(seed: u64) -> MirFunction {
    let mut st = seed.wrapping_add(13);
    let v = ValueId;
    let (p, parent, child, slot_copy, c, feed, extra) = (v(0), v(1), v(2), v(3), v(4), v(5), v(6));
    let mut ops = vec![Op::Alloc { dst: parent, repr: heap(), init: Init::Opaque }];
    ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(child), args: vec![parent] });
    let root = match next_rand(&mut st) % 4 {
        0 => p,
        1 => child,
        2 => {
            ops.push(Op::Dup { dst: slot_copy, src: p });
            slot_copy
        }
        _ => {
            ops.push(Op::Dup { dst: slot_copy, src: child });
            slot_copy
        }
    };
    // A third of the draws rebind straight-line (the line-slot fold).
    let looped = !next_rand(&mut st).is_multiple_of(3);
    if looped {
        ops.extend([Op::LoopStart, Op::ConstInt { dst: c, value: 1 }, Op::LoopBreakUnless { cond: c }]);
    }
    if next_rand(&mut st).is_multiple_of(2) {
        ops.push(Op::Alloc { dst: feed, repr: heap(), init: Init::Opaque });
    } else {
        ops.push(Op::Dup { dst: feed, src: parent });
    }
    if !next_rand(&mut st).is_multiple_of(5) {
        ops.push(Op::Drop { v: root });
    }
    ops.push(Op::SetLocal { local: root, src: feed });
    if next_rand(&mut st).is_multiple_of(6) {
        ops.push(Op::Alloc { dst: extra, repr: heap(), init: Init::Opaque });
        ops.push(Op::Drop { v: extra });
    }
    if looped {
        ops.push(Op::LoopEnd);
    }
    if !next_rand(&mut st).is_multiple_of(4) {
        ops.push(Op::Drop { v: root });
    }
    ops.push(Op::Drop { v: parent });
    let mut f = func(ops);
    f.params = vec![MirParam { value: p, repr: heap() }];
    f
}

#[test]
fn certificate_verdict_matches_verify_ownership_on_loop_slots() {
    let (mut accepted, mut rejected) = (0, 0);
    for seed in 0u64..4000 {
        let f = gen_loop_slots(seed);
        let cert = ownership_certificate(&f);
        let cert_ok = cert_all_balanced(&cert);
        let verify_ok = verify_ownership(&f).is_ok();
        assert_eq!(
            cert_ok, verify_ok,
            "seed {seed}: certificate says {cert_ok}, verify_ownership says {verify_ok}\ncert: {cert:?}\nops: {:?}",
            f.ops
        );
        if verify_ok {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    assert!(accepted > 300 && rejected > 300, "accepted {accepted}, rejected {rejected}");
}
