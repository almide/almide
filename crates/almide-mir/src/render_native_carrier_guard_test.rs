use super::*;
use crate::IntOp;

#[test]
fn a_result_slot_does_not_choose_one_of_two_distinct_heap_payloads() {
    use crate::render_native_rec::{install, RecDecl};
    let _guard = install(vec![
        RecDecl { name: "Error".into(), fields: vec![NTy::Str], ..Default::default() },
        RecDecl { name: "ListResult".into(), result_of: Some((NTy::Vec, NTy::Rec(0))), ..Default::default() },
        RecDecl { name: "StringResult".into(), result_of: Some((NTy::Rec(0), NTy::Str)), ..Default::default() },
        RecDecl { name: "SameResult".into(), result_of: Some((NTy::Rec(0), NTy::Rec(0))), ..Default::default() },
        RecDecl { name: "Optional".into(), option_of: Some(NTy::Rec(0)), ..Default::default() },
    ]);
    assert_eq!(slot_kind(1, 0), None);
    assert_eq!(slot_kind(2, 0), None);
    assert_eq!(slot_kind(3, 0), Some(NTy::Rec(0)));
    assert_eq!(slot_kind(4, 0), Some(NTy::Rec(0)));
    assert_eq!(slot_kind(3, 1), None);
}

fn guarded() -> MirFunction {
    let v = ValueId;
    MirFunction {
        ops: vec![
            Op::Prim {
                kind: PrimKind::Handle,
                dst: Some(v(1)),
                args: vec![v(0)],
            },
            Op::ConstInt {
                dst: v(2),
                value: RESULT_TAG_OFFSET,
            },
            Op::IntBinOp {
                dst: v(3),
                op: IntOp::Add,
                a: v(1),
                b: v(2),
            },
            Op::Prim {
                kind: PrimKind::Load { width: 4 },
                dst: Some(v(4)),
                args: vec![v(3)],
            },
            Op::ConstInt {
                dst: v(5),
                value: i64::from(BLOCK_HEADER),
            },
            Op::IntBinOp {
                dst: v(6),
                op: IntOp::Add,
                a: v(1),
                b: v(5),
            },
            Op::Prim {
                kind: PrimKind::LoadHandle,
                dst: Some(v(7)),
                args: vec![v(6)],
            },
            Op::IfThen {
                cond: v(4),
                dst: None,
            },
            Op::Else { val: None },
            Op::Prim {
                kind: PrimKind::Handle,
                dst: Some(v(8)),
                args: vec![v(7)],
            },
            Op::EndIf { val: None },
        ],
        ..Default::default()
    }
}

#[test]
fn guarded_payload_load_moves_under_its_tag_without_changing_other_ops() {
    let mut f = guarded();
    let old = f.ops.clone();
    sink_guarded_payload_reads(&mut f);
    let load = |op: &Op| matches!(op, Op::Prim { kind: PrimKind::LoadHandle, dst: Some(ValueId(7)), .. });
    let at = f.ops.iter().position(load).expect("the fn has the load op");
    assert!(matches!(f.ops[at - 1], Op::Else { .. }));
    assert_eq!(f.ops.len(), old.len());
    assert_eq!(format!("{:?}", f.ops.iter().filter(|o| !load(o)).collect::<Vec<_>>()),
        format!("{:?}", old.iter().filter(|o| !load(o)).collect::<Vec<_>>()));

    for escaped in [false, true] {
        let mut bad = guarded();
        if escaped { bad.ret = Some(ValueId(7)); }
        else { bad.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ValueId(9)), args: vec![ValueId(7)] }); }
        let before = format!("{:?}", bad.ops);
        sink_guarded_payload_reads(&mut bad);
        assert_eq!(format!("{:?}", bad.ops), before);
    }
}

#[test]
fn hoisted_payload_requires_one_matching_tag_arm_and_an_unchanged_carrier() {
    assert_eq!(hoisted_sides(&guarded()).get(&ValueId(7)), Some(&false));
    let mut outside = guarded();
    outside.ops.push(Op::Prim {
        kind: PrimKind::Handle,
        dst: Some(ValueId(9)),
        args: vec![ValueId(7)],
    });
    assert!(!hoisted_sides(&outside).contains_key(&ValueId(7)));
    let mut both = guarded();
    both.ops.insert(
        8,
        Op::Prim {
            kind: PrimKind::Handle,
            dst: Some(ValueId(9)),
            args: vec![ValueId(7)],
        },
    );
    assert!(!hoisted_sides(&both).contains_key(&ValueId(7)));
    let mut rebound = guarded();
    rebound.ops.insert(
        7,
        Op::SetLocal {
            local: ValueId(0),
            src: ValueId(10),
        },
    );
    assert!(!hoisted_sides(&rebound).contains_key(&ValueId(7)));
    let mut written = guarded();
    written.ops.insert(
        7,
        Op::Prim {
            kind: PrimKind::Store { width: 4 },
            dst: None,
            args: vec![ValueId(3), ValueId(10)],
        },
    );
    assert!(!hoisted_sides(&written).contains_key(&ValueId(7)));
}

#[test]
fn option_payload_may_be_read_inside_its_some_guard_but_not_after_rebinding() {
    let mut f = guarded();
    if let Op::ConstInt { value, .. } = &mut f.ops[1] { *value = i64::from(BLOCK_LEN_OFFSET); }
    // Read and use both occur in the Some arm, after its tag test.
    let read = f.ops.remove(6);
    f.ops.insert(7, read);
    let used = f.ops.remove(9);
    f.ops.insert(8, used);
    assert_eq!(hoisted_sides(&f).get(&ValueId(7)), Some(&true));
    f.ops.insert(7, Op::SetLocal { local: ValueId(0), src: ValueId(10) });
    assert!(!hoisted_sides(&f).contains_key(&ValueId(7)));
}

#[test]
fn compared_tags_preserve_branch_polarity_and_reject_reassigned_inputs() {
    for (op, constant, side) in [
        (IntOp::Ne, 0, false), (IntOp::Eq, 1, false),
        (IntOp::Eq, 0, true), (IntOp::Ne, 1, true),
        (IntOp::Ge, 1, false), (IntOp::Lt, 1, true),
        (IntOp::Gt, 0, false), (IntOp::Le, 0, true),
    ] {
        let mut f = guarded();
        f.ops.splice(7..7, [
            Op::ConstInt { dst: ValueId(10), value: constant },
            Op::IntBinOp { dst: ValueId(11), op, a: ValueId(4), b: ValueId(10) },
        ]);
        f.ops[9] = Op::IfThen { cond: ValueId(11), dst: None };
        assert_eq!(hoisted_sides(&f).get(&ValueId(7)), Some(&side));
        for target in [4, 10, 11] {
            let mut bad = f.clone();
            bad.ops.insert(9, Op::SetLocal { local: ValueId(target), src: ValueId(12) });
            assert!(!hoisted_sides(&bad).contains_key(&ValueId(7)));
        }
    }
    let tags = BTreeMap::from([(ValueId(1), (ValueId(0), true))]);
    assert_eq!(tag_comparison(IntOp::Eq, ValueId(1), ValueId(2),
        &BTreeMap::from([(ValueId(2), 2)]), &tags), None);
    for (op, n) in [(IntOp::Ge, 0), (IntOp::Le, 1), (IntOp::Lt, 0), (IntOp::Gt, 1)] {
        assert_eq!(tag_comparison(op, ValueId(1), ValueId(2),
            &BTreeMap::from([(ValueId(2), n)]), &tags), None, "constant predicate is not a guard");
    }
    assert_eq!(tag_comparison(IntOp::Le, ValueId(2), ValueId(1),
        &BTreeMap::from([(ValueId(2), 1)]), &tags), Some((ValueId(0), true)));
}

#[test]
fn combined_tag_guards_prove_only_their_logical_implications() {
    let v = ValueId;
    for (op, then_side, proven) in [
        (IntOp::And, true, Some(true)), (IntOp::And, false, None),
        (IntOp::Or, true, None), (IntOp::Or, false, Some(false)),
    ] {
        let mut f = guarded();
        f.ops.truncate(7);
        f.ops.extend([
            Op::Prim { kind: PrimKind::Handle, dst: Some(v(20)), args: vec![v(19)] },
            Op::IntBinOp { dst: v(21), op: IntOp::Add, a: v(20), b: v(2) },
            Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(v(22)), args: vec![v(21)] },
            Op::IntBinOp { dst: v(23), op, a: v(4), b: v(22) },
            Op::IfThen { cond: v(23), dst: None },
        ]);
        if !then_side { f.ops.push(Op::Else { val: None }); }
        f.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(v(8)), args: vec![v(7)] });
        f.ops.push(Op::EndIf { val: None });
        assert_eq!(hoisted_sides(&f).get(&v(7)).copied(), proven, "{op:?} then={then_side}");
        f.ops.insert(f.ops.len()-2, Op::SetLocal { local: v(0), src: v(30) });
        assert!(!hoisted_sides(&f).contains_key(&v(7)), "a rebound carrier invalidates the combined guard");
    }
}
