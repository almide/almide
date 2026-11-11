//! An early exit (`Op::Return` inside an arm) carries the whole function-exit
//! obligation at the op: everything the path owns is released exactly once
//! before it, the returned value moves out, and nothing moved earlier is
//! touched again. The ownership verifier accepts the exits the `!` rule emits
//! and refuses each way of getting one wrong.

use almide_mir::{verify_ownership, Init, MirFunction, Op, Repr, ValueId, ViolationKind, PLACEHOLDER_LAYOUT};

fn heap() -> Repr {
    Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
}

fn func(ops: Vec<Op>, ret: ValueId) -> MirFunction {
    MirFunction { name: "f".into(), ops, ret: Some(ret), ..Default::default() }
}

fn alloc(dst: ValueId) -> Op {
    Op::Alloc { dst, repr: heap(), init: Init::Opaque }
}

const X: ValueId = ValueId(0);
const Y: ValueId = ValueId(1);
const C: ValueId = ValueId(2);
const E: ValueId = ValueId(3);

fn kinds(f: &MirFunction) -> Vec<ViolationKind> {
    verify_ownership(f).err().unwrap_or_default().into_iter().map(|v| v.kind).collect()
}

#[test]
fn an_exit_that_releases_every_live_value_once_is_accepted() {
    // Two live values; the exit arm releases both and returns a scalar; the
    // surviving path releases them at the end.
    let f = func(
        vec![
            alloc(X),
            alloc(Y),
            Op::Const { dst: C },
            Op::IfThen { cond: C, dst: None },
            Op::Drop { v: Y },
            Op::Drop { v: X },
            Op::Return { val: Some(C) },
            Op::EndIf { val: None },
            Op::Drop { v: Y },
            Op::Drop { v: X },
        ],
        C,
    );
    assert_eq!(verify_ownership(&f), Ok(()));
}

#[test]
fn an_exit_that_returns_a_fresh_err_built_from_a_copied_piece_is_accepted() {
    // The rebox exit: copy the err piece out of the carrier, release the
    // carrier and the other live value, build the fresh block around the
    // copy, return it.
    let fresh = ValueId(4);
    let f = func(
        vec![
            alloc(X),
            alloc(Y),
            Op::Const { dst: C },
            Op::IfThen { cond: C, dst: None },
            Op::Dup { dst: E, src: Y },
            Op::Drop { v: X },
            Op::Drop { v: Y },
            Op::Alloc { dst: fresh, repr: heap(), init: Init::ResErrStr { piece: E } },
            Op::Consume { v: E },
            Op::Return { val: Some(fresh) },
            Op::EndIf { val: None },
            Op::Drop { v: Y },
            Op::Drop { v: X },
        ],
        C,
    );
    assert_eq!(verify_ownership(&f), Ok(()));
}

#[test]
fn an_exit_that_skips_one_of_two_releases_is_refused() {
    let f = func(
        vec![
            alloc(X),
            alloc(Y),
            Op::Const { dst: C },
            Op::IfThen { cond: C, dst: None },
            Op::Drop { v: Y },
            Op::Return { val: Some(C) },
            Op::EndIf { val: None },
            Op::Drop { v: Y },
            Op::Drop { v: X },
        ],
        C,
    );
    let errs = verify_ownership(&f).expect_err("the exit leaves `x` owned");
    assert!(
        errs.iter().any(|v| v.kind == ViolationKind::Leak && v.value == X && v.op_index == 5),
        "expected a Leak of x at the Return, got {errs:?}"
    );
}

#[test]
fn an_exit_that_releases_a_value_twice_is_refused() {
    let f = func(
        vec![
            alloc(X),
            Op::Const { dst: C },
            Op::IfThen { cond: C, dst: None },
            Op::Drop { v: X },
            Op::Drop { v: X },
            Op::Return { val: Some(C) },
            Op::EndIf { val: None },
            Op::Drop { v: X },
        ],
        C,
    );
    assert!(kinds(&f).contains(&ViolationKind::DoubleFree), "got {:?}", verify_ownership(&f));
}

#[test]
fn an_exit_that_releases_a_value_moved_earlier_is_refused() {
    // `x` moved out before the branch; the exit arm releases it again.
    let f = func(
        vec![
            alloc(X),
            Op::Const { dst: C },
            Op::Consume { v: X },
            Op::IfThen { cond: C, dst: None },
            Op::Drop { v: X },
            Op::Return { val: Some(C) },
            Op::EndIf { val: None },
        ],
        C,
    );
    assert!(!kinds(&f).is_empty(), "a release after the move must be refused");
}

#[test]
fn an_exit_that_returns_a_value_moved_earlier_is_refused() {
    let f = func(
        vec![
            alloc(X),
            Op::Const { dst: C },
            Op::Consume { v: X },
            Op::IfThen { cond: C, dst: None },
            Op::Return { val: Some(X) },
            Op::EndIf { val: None },
        ],
        C,
    );
    assert!(kinds(&f).contains(&ViolationKind::UseAfterMove), "got {:?}", verify_ownership(&f));
}
