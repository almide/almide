//! Ending the process is distinct from returning a value or continuing an arm.
//! The compiler and independent format-v6 checker must agree on both paths.
use almide_mir::{Init, PLACEHOLDER_LAYOUT, MirFunction, Op, PrimKind, Repr, ValueId as V};

fn alloc(v: u32) -> Op {
    Op::Alloc { dst: V(v), repr: Repr::Ptr { layout: PLACEHOLDER_LAYOUT }, init: Init::Str("held".into()) }
}
fn abort() -> Op { Op::Prim { kind: PrimKind::ProcExit, dst: None, args: vec![V(1)] } }
fn verdict(ops: Vec<Op>, accepted: bool) -> String {
    let f = MirFunction { name: "abort_boundary".into(), ops, ..Default::default() };
    let cert = almide_mir::certificate::ownership_certificate(&f);
    assert_eq!(almide_mir::verify_ownership(&f).is_ok(), accepted, "local: {cert}");
    assert_eq!(almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()),
        accepted, "independent: {cert}");
    cert
}
fn outer(then: Vec<Op>, otherwise: Vec<Op>, after: Vec<Op>) -> Vec<Op> {
    let then_value = then.is_empty().then_some(V(1));
    let else_value = otherwise.is_empty().then_some(V(1));
    let mut ops = vec![alloc(0), Op::ConstInt { dst: V(1), value: 1 },
        Op::IfThen { cond: V(1), dst: Some(V(5)) }];
    ops.extend(then);
    ops.push(Op::Else { val: then_value });
    ops.extend(otherwise);
    ops.push(Op::EndIf { val: else_value });
    ops.extend(after);
    ops
}

#[test]
fn aborting_arm_may_hold_references_but_the_surviving_arm_must_release_them() {
    for then_aborts in [true, false] {
        let (a, b) = if then_aborts { (vec![abort()], vec![]) } else { (vec![], vec![abort()]) };
        let cert = verdict(outer(a.clone(), b.clone(), vec![Op::Drop { v: V(0) }]), true);
        assert!(cert.contains('t'), "the process-ending event must be witnessed");
        verdict(outer(a, b, vec![]), false);
    }
}

#[test]
fn abort_does_not_excuse_double_free_or_use_after_free_before_termination() {
    for bad in [
        vec![Op::Drop { v: V(0) }, Op::Drop { v: V(0) }, abort()],
        vec![Op::Drop { v: V(0) }, Op::Borrow { v: V(0) }, abort()],
    ] {
        verdict(outer(bad, vec![], vec![Op::Drop { v: V(0) }]), false);
    }
}

#[test]
fn ordinary_calls_and_nonterminal_primitives_cannot_claim_abort_credit() {
    let ordinary = Op::CallFn { name: "claimed_never".into(), dst: None, args: vec![], result: None };
    for tail in [vec![ordinary], vec![abort(), Op::ConstInt { dst: V(2), value: 0 }]] {
        let mut then = vec![Op::Drop { v: V(0) }];
        then.extend(tail);
        let cert = verdict(outer(then, vec![], vec![Op::Drop { v: V(0) }]), false);
        assert!(!cert.contains('t'), "no immediate terminating primitive at the arm boundary");
    }
}

#[test]
fn a_heap_join_gets_credit_only_from_its_surviving_value() {
    let cert = verdict(vec![Op::ConstInt { dst: V(1), value: 1 },
        Op::IfThen { cond: V(1), dst: Some(V(4)) },
        abort(), Op::Else { val: None },
        alloc(2), Op::Consume { v: V(2) }, Op::EndIf { val: Some(V(2)) },
        Op::Drop { v: V(4) },
    ], true);
    assert!(cert.contains('t'));
}
