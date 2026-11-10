use super::*;
use crate::{Repr, PLACEHOLDER_LAYOUT};

fn builder() -> MirFunction {
    let v = ValueId;
    MirFunction {
        name: "builder".into(),
        ret: Some(v(1)),
        ops: vec![
            Op::ConstInt {
                dst: v(0),
                value: 3,
            },
            Op::Alloc {
                dst: v(1),
                repr: Repr::Ptr {
                    layout: PLACEHOLDER_LAYOUT,
                },
                init: Init::DynList { len: v(0) },
            },
            Op::Prim {
                kind: PrimKind::Handle,
                dst: Some(v(5)),
                args: vec![v(1)],
            },
            Op::ConstInt {
                dst: v(6),
                value: i64::from(crate::lower::layout::BLOCK_LEN_OFFSET),
            },
            Op::IntBinOp {
                dst: v(7),
                op: IntOp::Add,
                a: v(5),
                b: v(6),
            },
            Op::ConstInt {
                dst: v(2),
                value: 0,
            },
            Op::ConstInt {
                dst: v(3),
                value: 0,
            },
            Op::ConstInt {
                dst: v(4),
                value: 1,
            },
            Op::LoopStart,
            Op::IntBinOp {
                dst: v(8),
                op: IntOp::Lt,
                a: v(3),
                b: v(0),
            },
            Op::LoopBreakUnless { cond: v(8) },
            Op::IfThen {
                cond: v(8),
                dst: None,
            },
            Op::IntBinOp {
                dst: v(9),
                op: IntOp::Add,
                a: v(2),
                b: v(4),
            },
            Op::SetLocal {
                local: v(2),
                src: v(9),
            },
            Op::Else { val: None },
            Op::EndIf { val: None },
            Op::IntBinOp {
                dst: v(10),
                op: IntOp::Add,
                a: v(3),
                b: v(4),
            },
            Op::SetLocal {
                local: v(3),
                src: v(10),
            },
            Op::LoopEnd,
            Op::Prim {
                kind: PrimKind::Store { width: 4 },
                dst: None,
                args: vec![v(7), v(2)],
            },
        ],
        ..Default::default()
    }
}

#[test]
fn a_fresh_prefix_has_a_bounded_final_length() {
    assert_eq!(
        finalizers(&builder()),
        BTreeMap::from([(ValueId(7), ValueId(1))])
    );
}

#[test]
fn exclusive_filter_arms_commit_once_but_successive_arms_cannot() {
    let mut f = builder();
    let increment = [
        Op::IntBinOp { dst: ValueId(11), op: IntOp::Add, a: ValueId(2), b: ValueId(4) },
        Op::SetLocal { local: ValueId(2), src: ValueId(11) },
    ];
    // Both alternatives increment, but no execution takes both alternatives.
    f.ops.splice(15..15, increment.clone());
    assert_eq!(finalizers(&f), BTreeMap::from([(ValueId(7), ValueId(1))]));

    let mut f = builder();
    // A second independent condition can execute after the first increment.
    f.ops.splice(16..16, [
        Op::IfThen { cond: ValueId(8), dst: None },
        increment[0].clone(), increment[1].clone(),
        Op::Else { val: None }, Op::EndIf { val: None },
    ]);
    assert!(finalizers(&f).is_empty());
}

#[test]
fn counter_or_alias_changes_cannot_be_misread_as_prefix_commitment() {
    let v = ValueId;
    let mut cases = Vec::new();
    let mut f = builder();
    f.ops[5] = Op::ConstInt {
        dst: v(2),
        value: -1,
    };
    cases.push(("negative", f));
    let mut f = builder();
    f.ops[7] = Op::ConstInt {
        dst: v(4),
        value: 2,
    };
    cases.push(("stride", f));
    let mut f = builder();
    f.ops.insert(13, f.ops[13].clone());
    cases.push(("two updates", f));
    let mut f = builder();
    f.ops.insert(
        8,
        Op::Dup {
            dst: v(20),
            src: v(1),
        },
    );
    cases.push(("alias", f));
    let mut f = builder();
    f.ops.push(f.ops[19].clone());
    cases.push(("later growth", f));
    let mut f = builder();
    let op = f.ops.remove(9);
    f.ops.insert(8, op);
    cases.push(("stale condition", f));
    let mut f = builder();
    let op = f.ops.remove(12);
    f.ops.insert(8, op);
    cases.push(("stale increment", f));
    let mut f = builder();
    f.ops.insert(
        13,
        Op::SetLocal {
            local: v(4),
            src: v(0),
        },
    );
    cases.push(("mutable stride", f));
    let mut f = builder();
    f.ops.insert(
        13,
        Op::SetLocal {
            local: v(0),
            src: v(4),
        },
    );
    cases.push(("mutable bound", f));
    let mut f = builder();
    let op = f.ops.remove(17);
    f.ops.insert(14, op);
    cases.push(("conditional induction", f));
    let mut f = builder();
    f.ops.insert(
        9,
        Op::IfThen {
            cond: v(4),
            dst: None,
        },
    );
    f.ops.insert(11, Op::EndIf { val: None });
    cases.push(("conditional comparison", f));
    let mut f = builder();
    f.ops.insert(
        10,
        Op::SetLocal {
            local: v(8),
            src: v(4),
        },
    );
    cases.push(("mutable comparison", f));
    for (name, f) in cases {
        assert!(finalizers(&f).is_empty(), "{name}");
    }
}
