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

#[test]
fn allocator_prim_feeders_credit_the_loop_slot() {
    let v = ValueId;
    for kind in [PrimKind::ResStrMake, PrimKind::ResMakeOk] {
        let birth = |dst| {
            let mut ops = Vec::new();
            let args = if kind == PrimKind::ResStrMake {
                ops.push(Op::Alloc { dst: v(dst + 10), repr: heap(), init: Init::Str("payload".into()) });
                vec![v(dst + 10)]
            } else {
                ops.push(Op::ConstInt { dst: v(dst + 10), value: 7 });
                vec![v(dst + 10)]
            };
            ops.push(Op::Prim { kind: kind, dst: Some(v(dst)), args });
            if kind == PrimKind::ResStrMake { ops.push(Op::Consume { v: v(dst + 10) }); }
            ops
        };
        let mut ops = birth(1);
        ops.extend([
            Op::LoopStart,
            Op::ConstInt { dst: v(2), value: 1 },
            Op::LoopBreakUnless { cond: v(2) },
        ]);
        ops.extend(birth(3));
        let release = ops.len();
        ops.extend([
            Op::Drop { v: v(1) },
            Op::SetLocal { local: v(1), src: v(3) },
            Op::LoopEnd,
            Op::Drop { v: v(1) },
        ]);
        let f = func(ops);
        assert_eq!(verify_ownership(&f), Ok(()));
        assert!(cert_all_balanced(&ownership_certificate(&f)), "{kind:?}");
        let mut leaking = f.clone();
        leaking.ops.remove(release);
        assert!(verify_ownership(&leaking).is_err());
        assert!(!cert_all_balanced(&ownership_certificate(&leaking)));
    }
}

#[test]
fn derived_views_of_a_retained_child_follow_its_reference() {
    let v = ValueId;
    for held in [false, true] {
        for release_before_read in [false, true] {
            let mut ops = vec![
                Op::Alloc { dst: v(0), repr: heap(), init: Init::Opaque },
                Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(1)), args: vec![v(0)] },
            ];
            if held { ops.push(Op::Dup { dst: v(2), src: v(1) }); }
            ops.extend([
                Op::Alloc { dst: v(3), repr: heap(), init: Init::Opaque },
                Op::Drop { v: v(0) },
                Op::SetLocal { local: v(0), src: v(3) },
                Op::Prim { kind: PrimKind::Handle, dst: Some(v(4)), args: vec![v(1)] },
                Op::ConstInt { dst: v(5), value: 20 },
                Op::IntBinOp { dst: v(6), op: crate::IntOp::Add, a: v(4), b: v(5) },
            ]);
            if held && release_before_read { ops.push(Op::Drop { v: v(2) }); }
            ops.extend([
                Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(7)), args: vec![v(6)] },
                Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(v(7))], result: None },
            ]);
            if held && !release_before_read { ops.push(Op::Drop { v: v(2) }); }
            ops.push(Op::Drop { v: v(0) });
            let f = func(ops);
            let expected = held && !release_before_read;
            assert_eq!(verify_ownership(&f).is_ok(), expected, "held={held}, released={release_before_read}");
            assert_eq!(cert_all_balanced(&ownership_certificate(&f)), expected, "held={held}, released={release_before_read}");
        }
    }
}
