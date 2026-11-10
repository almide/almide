//! A branch merge released by the drop of its type. Each arm of the `IfThen`
//! moves one owned value into the merge (`Consume`, then the arm's `val`), the
//! frame reads through the merge and releases it once at scope end. Which op
//! releases it follows the merge's type: a Result of a Value ends in
//! `DropResultValue`, an Option of a record in `DropWrapperRec`. The certificate
//! counted a merge as holding a reference only when a `Consume`, `Drop`,
//! `DropListStr` or `DropVariant` released it, so under any other drop of the
//! family the merge held nothing: `verify_ownership` reported every read
//! through it as a use after free and its drop as a double free, and the
//! certificate wrote the drop as a `d` on a line no `i` opened.
//!
//! Every drop of the family is the same single release. Per op, this pins the
//! accepted shape, and that the verdict still refuses the three ways such a
//! merge can be wrong: released twice, read after its release, never released
//! but read. The first two refusals are checked twice — by `verify_ownership`,
//! and on the certificate by the mirror of the proven check
//! (`common::reject_reason`).

mod common;
use almide_mir::{certificate, verify_ownership, CallArg, Init, MirFunction, Op, PrimKind, Repr, RtFn, ValueId, ViolationKind, PLACEHOLDER_LAYOUT};
use common::reject_reason;

/// Every drop op that releases one reference of its operand, by name.
fn drops(v: ValueId) -> Vec<(&'static str, Op)> {
    vec![
        ("Drop", Op::Drop { v }),
        ("DropListStr", Op::DropListStr { v }),
        ("DropValue", Op::DropValue { v }),
        ("DropListValue", Op::DropListValue { v }),
        ("DropListStrValue", Op::DropListStrValue { v }),
        ("DropListStrStr", Op::DropListStrStr { v }),
        ("DropListIntStr", Op::DropListIntStr { v }),
        ("DropListStrInt", Op::DropListStrInt { v }),
        ("DropResultListValue", Op::DropResultListValue { v }),
        ("DropResultValue", Op::DropResultValue { v }),
        ("DropResultStrInt", Op::DropResultStrInt { v }),
        ("DropResultValueInt", Op::DropResultValueInt { v }),
        ("DropResultListValueInt", Op::DropResultListValueInt { v }),
        ("DropResultListStrInt", Op::DropResultListStrInt { v }),
        ("DropResultListStr", Op::DropResultListStr { v }),
        ("DropListListStr", Op::DropListListStr { v }),
        ("DropVariant", Op::DropVariant { v, ty: "T".into() }),
        ("DropWrapperRec", Op::DropWrapperRec { v, drop_fn: "T".into(), is_result: true, err_rec: false }),
    ]
}

const MERGE: ValueId = ValueId(1);

fn heap(dst: ValueId, text: &str) -> Op {
    Op::Alloc { dst, repr: Repr::Ptr { layout: PLACEHOLDER_LAYOUT }, init: Init::Str(text.into()) }
}

fn read(v: ValueId) -> Op {
    Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(v)], result: None }
}

/// `m = if c then a else b; read(m)`, without the release of `m`.
fn merge_read() -> Vec<Op> {
    let v = ValueId;
    vec![
        Op::ConstInt { dst: v(0), value: 1 },
        Op::IfThen { cond: v(0), dst: Some(MERGE) },
        heap(v(2), "then"),
        Op::Consume { v: v(2) },
        Op::Else { val: Some(v(2)) },
        heap(v(3), "else"),
        Op::Consume { v: v(3) },
        Op::EndIf { val: Some(v(3)) },
        Op::Prim { kind: PrimKind::Handle, dst: Some(v(4)), args: vec![MERGE] },
        read(MERGE),
    ]
}

fn function(name: &str, ops: Vec<Op>) -> MirFunction {
    MirFunction { name: name.into(), ops, ..Default::default() }
}

fn kinds(f: &MirFunction) -> Vec<ViolationKind> {
    verify_ownership(f).err().unwrap_or_default().into_iter().map(|v| v.kind).collect()
}

fn rejected_lines(f: &MirFunction) -> Vec<String> {
    certificate::ownership_certificate(f).lines().filter(|l| reject_reason(l).is_some()).map(str::to_string).collect()
}

#[test]
fn a_merge_released_once_by_any_drop_of_the_family_holds_its_reference() {
    for (name, drop) in drops(MERGE) {
        let mut ops = merge_read();
        ops.push(drop);
        let f = function(name, ops);
        assert_eq!(kinds(&f), [], "{name}: the balanced merge is refused");
        assert_eq!(certificate::merge_dst_i_credits(&f), 1, "{name}: the merge's credit");
        assert!(!certificate::ownership_certificate_with_poison(&f).1, "{name}: poisoned");
        assert_eq!(rejected_lines(&f), Vec::<String>::new(), "{name}: a certificate line is refused");
    }
}

#[test]
fn a_merge_released_twice_is_a_double_free() {
    for ((name, drop), (_, again)) in drops(MERGE).into_iter().zip(drops(MERGE)) {
        let mut ops = merge_read();
        ops.extend([drop, again]);
        let f = function(name, ops);
        assert!(kinds(&f).contains(&ViolationKind::DoubleFree), "{name}: {:?}", kinds(&f));
        assert!(!rejected_lines(&f).is_empty(), "{name}: the certificate carries the second release unrefused");
    }
}

#[test]
fn a_merge_read_after_its_release_is_a_use_after_free() {
    for (name, drop) in drops(MERGE) {
        let mut ops = merge_read();
        ops.extend([drop, read(MERGE)]);
        let f = function(name, ops);
        assert!(kinds(&f).contains(&ViolationKind::UseAfterFree), "{name}: {:?}", kinds(&f));
        assert!(!rejected_lines(&f).is_empty(), "{name}: the certificate carries the late read unrefused");
    }
}

#[test]
fn a_merge_never_released_holds_no_reference_and_an_arm_value_cannot_stand_in() {
    // No release: the merge holds no reference (the rule this change leaves
    // as it was), so the read through it is refused.
    let f = function("unreleased", merge_read());
    assert_eq!(certificate::merge_dst_i_credits(&f), 0);
    assert!(kinds(&f).contains(&ViolationKind::UseAfterFree), "an unreleased merge is read through: {:?}", kinds(&f));
    // Nor can the frame balance by releasing what an arm already moved in.
    for (name, drop) in drops(ValueId(2)) {
        let mut ops = merge_read();
        ops.push(drop);
        let f = function(name, ops);
        assert!(!kinds(&f).is_empty(), "{name}: releasing an arm's moved value instead of the merge is accepted");
    }
}
