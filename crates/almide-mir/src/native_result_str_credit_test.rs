use super::*;

#[test]
fn a_string_result_keeps_its_credit_for_borrowed_payload_calls() {
    let v = ValueId;
    let mut f = MirFunction {
        name: "string_result_credit".into(),
        ops: vec![
            Op::Alloc { dst: v(0), repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }, init: Init::Str("payload".into()) },
            Op::Prim { kind: PrimKind::ResStrMake, dst: Some(v(1)), args: vec![v(0)] },
            Op::Consume { v: v(0) },
            Op::Prim { kind: PrimKind::ResErrStr, dst: Some(v(2)), args: vec![v(1)] },
            Op::Call { dst: None, func: crate::RtFn::PrintStr, args: vec![crate::CallArg::Handle(v(2))], result: None },
            Op::Drop { v: v(1) },
        ],
        ..Default::default()
    };
    assert!(crate::verify_ownership(&f).is_ok());
    let drop = f.ops.pop().unwrap();
    assert!(crate::verify_ownership(&f).is_err(), "missing carrier release");
    f.ops.insert(4, drop);
    assert!(crate::verify_ownership(&f).is_err(), "payload outlives carrier");
}

#[test]
fn carrier_branch_merges_transfer_the_owned_reference() {
    let v = ValueId;
    let mut f = MirFunction {
        name: "carrier_branch_credit".into(),
        ops: vec![
            Op::ConstInt { dst: v(0), value: 1 },
            Op::IfThen { cond: v(0), dst: Some(v(1)) },
            Op::Prim { kind: PrimKind::ResMakeOk, dst: Some(v(2)), args: vec![v(0)] },
            Op::Consume { v: v(2) },
            Op::Else { val: Some(v(2)) },
            Op::Prim { kind: PrimKind::ResMakeOk, dst: Some(v(3)), args: vec![v(0)] },
            Op::Consume { v: v(3) },
            Op::EndIf { val: Some(v(3)) },
            Op::Drop { v: v(1) },
        ],
        ..Default::default()
    };
    assert!(crate::verify_ownership(&f).is_ok());
    assert_eq!(crate::certificate::merge_dst_i_credits(&f), 1);
    assert!(!crate::certificate::ownership_certificate_with_poison(&f).1);
    f.ops.insert(8, Op::Drop { v: v(1) });
    assert!(crate::verify_ownership(&f).is_err(), "merge cannot be released twice");
    f.ops.remove(8);
    f.ops.insert(6, Op::Drop { v: v(3) });
    assert!(crate::verify_ownership(&f).is_err(), "freed arm cannot transfer ownership");
}
