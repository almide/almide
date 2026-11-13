//! `native_unnest_tail`: the flat spelling of a nested tail branch, what the
//! ownership mirror and the certificate say of it once a param is taken
//! over, and the bodies they refuse. The certificate lines pinned here are
//! the ones `almide-verify` judges (`a_taken_over_param_through_sibling_exits`).

use super::*;
use crate::certificate::ownership_certificate_with_poison;
use crate::{verify_ownership, Init, MirParam, Repr, ViolationKind};

fn heap() -> Repr {
    Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }
}

fn v(n: u32) -> ValueId {
    ValueId(n)
}

const X: ValueId = ValueId(0);
const C1: ValueId = ValueId(1);
const C2: ValueId = ValueId(2);

/// A fresh result (`dst`) built on a path that lets `x` go: `extra` runs
/// between the two.
fn leaf(dst: u32, extra: Vec<Op>) -> Vec<Op> {
    let mut ops = vec![Op::Alloc { dst: v(dst), repr: heap(), init: Init::Opaque }, Op::Drop { v: X }];
    ops.extend(extra);
    ops
}

/// `step(x, c1, c2)` with `x` taken over, over `ops` returning `ret`.
fn step_fn(ops: Vec<Op>, ret: u32) -> MirFunction {
    MirFunction {
        name: "step".into(),
        params: vec![
            MirParam { value: X, repr: heap() },
            MirParam { value: C1, repr: Repr::Scalar { width: crate::ScalarWidth::Double } },
            MirParam { value: C2, repr: Repr::Scalar { width: crate::ScalarWidth::Double } },
        ],
        ops,
        ret: Some(v(ret)),
        owned_params: [X].into_iter().collect(),
        ..Default::default()
    }
}

/// `if c1 { if c2 { a } else { b } } else { c }`, each leaf releasing `x`.
fn nested() -> MirFunction {
    let mut ops = vec![Op::IfThen { cond: C1, dst: Some(v(10)) }, Op::IfThen { cond: C2, dst: Some(v(11)) }];
    ops.extend(leaf(20, vec![Op::Consume { v: v(20) }]));
    ops.push(Op::Else { val: Some(v(20)) });
    ops.extend(leaf(21, vec![Op::Consume { v: v(21) }]));
    ops.push(Op::EndIf { val: Some(v(21)) });
    ops.push(Op::Else { val: Some(v(11)) });
    ops.extend(leaf(22, vec![Op::Consume { v: v(22) }]));
    ops.push(Op::EndIf { val: Some(v(22)) });
    step_fn(ops, 10)
}

/// The flat spelling of [`nested`]: two sibling regions whose `else` arm
/// leaves, then the innermost `then` leaf as the tail. `at_c`, `at_b` and
/// `at_a` run after each leaf's release of `x`.
fn flat(at_c: Vec<Op>, at_b: Vec<Op>, at_a: Vec<Op>) -> MirFunction {
    let mut ops = vec![Op::IfThen { cond: C1, dst: None }, Op::Else { val: None }];
    ops.extend(leaf(22, at_c));
    ops.extend([Op::Return { val: Some(v(22)) }, Op::EndIf { val: None }]);
    ops.extend([Op::IfThen { cond: C2, dst: None }, Op::Else { val: None }]);
    ops.extend(leaf(21, at_b));
    ops.extend([Op::Return { val: Some(v(21)) }, Op::EndIf { val: None }]);
    ops.extend(leaf(20, at_a));
    step_fn(ops, 20)
}

#[test]
fn a_nested_tail_branch_becomes_sibling_early_returns() {
    assert_eq!(unnest(&nested()), Some(flat(vec![], vec![], vec![])));
    // Nothing left to flatten, and a body with no tail branch has none.
    assert_eq!(unnest(&flat(vec![], vec![], vec![])), None);
    assert_eq!(unnest(&step_fn(leaf(20, vec![]), 20)), None);
}

#[test]
fn the_flat_body_certifies_where_the_nested_one_cannot() {
    // Nested: `x` is released in an arm of an arm — the line cannot be
    // written with flat arms, and the certificate is the rejecting poison.
    let (cert, poisoned) = ownership_certificate_with_poison(&nested());
    assert!(poisoned, "{cert}");
    assert_eq!(verify_ownership(&nested()), Ok(()));
    // Flat: `x` enters held (`i`); each level is one bracket whose leaving
    // arm ends at zero, and the tail path releases it.
    let f = flat(vec![], vec![], vec![]);
    let (cert, poisoned) = ownership_certificate_with_poison(&f);
    assert!(!poisoned, "{cert}");
    assert_eq!(cert.lines().next(), Some("i{|dx}{|dx}d"), "{cert}");
    assert_eq!(verify_ownership(&f), Ok(()));
}

#[test]
fn a_read_after_the_release_on_one_path_is_refused() {
    // The tail path reads `x` after letting it go.
    let f = flat(vec![], vec![], vec![Op::Dup { dst: v(30), src: X }, Op::Drop { v: v(30) }]);
    let errs = verify_ownership(&f).unwrap_err();
    assert!(errs.iter().any(|e| e.kind == ViolationKind::UseAfterFree && e.value == X), "{errs:?}");
    assert_eq!(ownership_certificate_with_poison(&f).0.lines().next(), Some("i{|dx}{|dx}dad"));
    // And so does a leaving path.
    let f = flat(vec![], vec![Op::Dup { dst: v(30), src: X }, Op::Drop { v: v(30) }], vec![]);
    let errs = verify_ownership(&f).unwrap_err();
    assert!(errs.iter().any(|e| e.kind == ViolationKind::UseAfterFree && e.value == X), "{errs:?}");
}

#[test]
fn a_second_release_on_one_path_is_refused() {
    let f = flat(vec![Op::Drop { v: X }], vec![], vec![]);
    let errs = verify_ownership(&f).unwrap_err();
    assert!(errs.iter().any(|e| e.kind == ViolationKind::DoubleFree && e.value == X), "{errs:?}");
    let (cert, _) = ownership_certificate_with_poison(&f);
    assert_eq!(cert.lines().next(), Some("i{|ddx}{|dx}d"), "{cert}");
}

#[test]
fn a_release_in_one_arm_only_with_a_later_read_is_refused() {
    // The first region's arm releases `x` and stays; the rest reads it.
    let mut ops = vec![Op::IfThen { cond: C1, dst: None }, Op::Else { val: None }, Op::Drop { v: X }, Op::EndIf { val: None }];
    ops.extend([Op::Dup { dst: v(30), src: X }, Op::Drop { v: v(30) }]);
    ops.extend(leaf(20, vec![]));
    let f = step_fn(ops, 20);
    let errs = verify_ownership(&f).unwrap_err();
    assert!(errs.iter().any(|e| e.kind == ViolationKind::BranchDisagreement && e.value == X), "{errs:?}");
    let (cert, _) = ownership_certificate_with_poison(&f);
    assert_eq!(cert.lines().next(), Some("i{|d}add"), "{cert}");
    // A leaving arm that keeps `x` leaks it at its exit.
    let mut ops = vec![Op::IfThen { cond: C1, dst: None }, Op::Else { val: None }];
    ops.extend([Op::Alloc { dst: v(22), repr: heap(), init: Init::Opaque }, Op::Return { val: Some(v(22)) }, Op::EndIf { val: None }]);
    ops.extend(leaf(20, vec![]));
    let leaky = step_fn(ops, 20);
    let errs = verify_ownership(&leaky).unwrap_err();
    assert!(errs.iter().any(|e| e.kind == ViolationKind::Leak && e.value == X), "{errs:?}");
    assert_eq!(ownership_certificate_with_poison(&leaky).0.lines().next(), Some("i{|x}d"));
}

#[test]
fn a_scalar_tail_and_a_tail_with_more_than_releases_stay() {
    // A scalar leaving value: the return seam may wrap it.
    let mut ops = vec![Op::IfThen { cond: C1, dst: Some(v(10)) }, Op::ConstInt { dst: v(20), value: 1 }, Op::Else { val: Some(v(20)) }];
    ops.extend([Op::ConstInt { dst: v(21), value: 2 }, Op::EndIf { val: Some(v(21)) }]);
    assert_eq!(unnest(&step_fn(ops, 10)), None);
    // An op after the branch that is not a release.
    let mut f = nested();
    f.ops.push(Op::ConstInt { dst: v(40), value: 0 });
    assert_eq!(unnest(&f), None);
    // Both arms hold a region: neither can be the one that leaves.
    let arm = |a: u32, b: u32, j: u32| {
        let mut ops = vec![Op::IfThen { cond: C2, dst: Some(v(j)) }];
        ops.extend(leaf(a, vec![Op::Consume { v: v(a) }]));
        ops.push(Op::Else { val: Some(v(a)) });
        ops.extend(leaf(b, vec![Op::Consume { v: v(b) }]));
        ops.push(Op::EndIf { val: Some(v(b)) });
        ops
    };
    let mut ops = vec![Op::IfThen { cond: C1, dst: Some(v(10)) }];
    ops.extend(arm(20, 21, 11));
    ops.push(Op::Else { val: Some(v(11)) });
    ops.extend(arm(22, 23, 12));
    ops.push(Op::EndIf { val: Some(v(12)) });
    assert_eq!(unnest(&step_fn(ops, 10)), None);
}
