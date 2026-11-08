use super::*;
use crate::IntOp;

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
