use super::*;
use crate::{CallArg, IntOp};

fn v(n: u32) -> ValueId {
    ValueId(n)
}

/// `c = f(); if c.is_err() { len(err c) } else { ok c + 1 }; release c` —
/// the consumer of a `match` on a Result, with `extra` spliced into the then
/// arm and `tail` after the release.
fn dispatch_fn(extra: Vec<Op>, tail: Vec<Op>) -> MirFunction {
    let mut ops = vec![
        Op::CallFn { dst: Some(v(1)), name: "f".into(), args: vec![], result: None },
        Op::Prim { kind: PrimKind::ResTag, dst: Some(v(2)), args: vec![v(1)] },
        Op::IfThen { cond: v(2), dst: None },
        Op::Prim { kind: PrimKind::ResErrStr, dst: Some(v(3)), args: vec![v(1)] },
        Op::CallFn { dst: Some(v(4)), name: "string.len".into(), args: vec![CallArg::Handle(v(3))], result: None },
    ];
    ops.extend(extra);
    ops.extend([
        Op::Else { val: None },
        Op::Prim { kind: PrimKind::ResOkScalar, dst: Some(v(5)), args: vec![v(1)] },
        Op::IntBinOp { dst: v(6), op: IntOp::Add, a: v(5), b: v(5) },
        Op::EndIf { val: None },
        Op::DropListStr { v: v(1) },
    ]);
    ops.extend(tail);
    MirFunction { ops, ..Default::default() }
}

#[test]
fn a_carrier_whose_tag_dispatch_is_its_last_use_goes_at_each_arms_end() {
    let f = dispatch_fn(vec![], vec![]);
    let got = arm_releases(&f, &BTreeSet::new());
    // The Else (index 5) and the EndIf (index 8) each release the carrier.
    assert_eq!(got, BTreeMap::from([(5, v(1)), (8, v(1))]));
}

#[test]
fn a_carrier_read_after_the_dispatch_keeps_its_release() {
    let tail = vec![Op::Prim { kind: PrimKind::ResTag, dst: Some(v(9)), args: vec![v(1)] }];
    assert!(arm_releases(&dispatch_fn(vec![], tail), &BTreeSet::new()).is_empty());
}

#[test]
fn a_carrier_passed_on_inside_an_arm_keeps_its_release() {
    let extra = vec![Op::CallFn { dst: None, name: "g".into(), args: vec![CallArg::Handle(v(1))], result: None }];
    assert!(arm_releases(&dispatch_fn(extra, vec![]), &BTreeSet::new()).is_empty());
}

#[test]
fn an_arm_that_returns_or_aborts_keeps_the_release_after_the_join() {
    let ret = vec![Op::Return { val: None }];
    assert!(arm_releases(&dispatch_fn(ret, vec![]), &BTreeSet::new()).is_empty());
    let die = vec![Op::Prim { kind: PrimKind::Die, dst: None, args: vec![] }];
    assert!(arm_releases(&dispatch_fn(die, vec![]), &BTreeSet::new()).is_empty());
    // A tail-recursion flag exit inside the dispatch breaks out of it.
    assert!(arm_releases(&dispatch_fn(vec![], vec![]), &BTreeSet::from([4])).is_empty());
}

#[test]
fn an_if_on_another_condition_is_not_a_tag_dispatch() {
    let mut f = dispatch_fn(vec![], vec![]);
    f.ops[2] = Op::IfThen { cond: v(7), dst: None };
    assert!(arm_releases(&f, &BTreeSet::new()).is_empty());
}

#[test]
fn the_release_names_only_an_owned_local_held_by_reference() {
    let held = "let mut v1: Result<i64, AlmideString> = f();\nlet v2: i64 = v1.is_err() as i64;\nlet v3: &str = match &v1 { Err(e) => e.as_str(), Ok(_) => \"\" };\n";
    assert_eq!(release_line(held, "v1").as_deref(), Some("drop(v1);"));
    // The other arm's release is not a use.
    assert_eq!(release_line(&format!("{held}drop(v1);\n"), "v1").as_deref(), Some("drop(v1);"));
    // A by-value move, a param (no local binding), a name prefix.
    assert_eq!(release_line(&format!("{held}let v9 = match v1 {{ Err(e) => e, Ok(_) => unreachable!() }};\n"), "v1"), None);
    assert_eq!(release_line("let v2: i64 = v1.is_err() as i64;\n", "v1"), None);
    assert_eq!(release_line(&format!("{held}let v10: i64 = 0;\n"), "v1").as_deref(), Some("drop(v1);"));
}

/// `c = f(); if <tag word of c> { n += 1 } else { s += ok c }; <tail>`, the
/// tag and the Ok payload read through the carrier's address (a scalar-Ok
/// Result, tagged by its length word), with `between` spliced after the
/// dispatch and `c`'s release after that.
fn word_dispatch_fn(between: Vec<Op>, tail: Vec<Op>) -> MirFunction {
    let mut ops = vec![
        Op::CallFn { dst: Some(v(1)), name: "f".into(), args: vec![], result: None },
        Op::Prim { kind: PrimKind::Handle, dst: Some(v(2)), args: vec![v(1)] },
        Op::ConstInt { dst: v(3), value: i64::from(crate::lower::layout::BLOCK_LEN_OFFSET) },
        Op::IntBinOp { dst: v(4), op: IntOp::Add, a: v(2), b: v(3) },
        Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(v(5)), args: vec![v(4)] },
        Op::IfThen { cond: v(5), dst: None },
        Op::IntBinOp { dst: v(6), op: IntOp::Add, a: v(20), b: v(20) },
        Op::Else { val: None },
        Op::ConstInt { dst: v(7), value: 12 },
        Op::IntBinOp { dst: v(8), op: IntOp::Add, a: v(2), b: v(7) },
        Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v(9)), args: vec![v(8)] },
        Op::EndIf { val: None },
    ];
    ops.extend(between);
    ops.push(Op::DropListStr { v: v(1) });
    ops.extend(tail);
    MirFunction { ops, ..Default::default() }
}

#[test]
fn a_tag_word_dispatch_releases_in_its_arms_past_a_later_unrelated_dispatch() {
    // Right after the join.
    assert_eq!(arm_releases(&word_dispatch_fn(vec![], vec![]), &BTreeSet::new()), BTreeMap::from([(7, v(1)), (11, v(1))]));
    // A second call and its own `if` between the join and the release.
    let between = vec![
        Op::CallFn { dst: Some(v(30)), name: "g".into(), args: vec![], result: None },
        Op::IfThen { cond: v(30), dst: None },
        Op::Else { val: None },
        Op::EndIf { val: None },
    ];
    assert_eq!(arm_releases(&word_dispatch_fn(between, vec![]), &BTreeSet::new()), BTreeMap::from([(7, v(1)), (11, v(1))]));
}

#[test]
fn a_release_outside_the_dispatchs_block_or_after_a_read_stays_put() {
    // The release inside a later `if`: not every path reaches it at this level.
    let mut f = word_dispatch_fn(vec![Op::IfThen { cond: v(30), dst: None }], vec![Op::EndIf { val: None }]);
    assert!(arm_releases(&f, &BTreeSet::new()).is_empty());
    // A loop round ending between the dispatch and the release.
    f = word_dispatch_fn(vec![Op::LoopEnd], vec![]);
    assert!(arm_releases(&f, &BTreeSet::new()).is_empty());
    // The carrier's address read again after the dispatch.
    let again = vec![Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v(31)), args: vec![v(8)] }];
    assert!(arm_releases(&word_dispatch_fn(again, vec![]), &BTreeSet::new()).is_empty());
    // A payload view loaded in an arm and read past the join.
    let mut f = word_dispatch_fn(vec![Op::CallFn { dst: None, name: "h".into(), args: vec![CallArg::Handle(v(32))], result: None }], vec![]);
    f.ops.insert(10, Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(32)), args: vec![v(8)] });
    assert!(arm_releases(&f, &BTreeSet::new()).is_empty());
}

#[test]
fn a_load_off_another_word_is_not_a_tag() {
    let mut f = word_dispatch_fn(vec![], vec![]);
    f.ops[2] = Op::ConstInt { dst: v(3), value: 12 };
    assert!(arm_releases(&f, &BTreeSet::new()).is_empty());
}

#[test]
fn a_comment_naming_the_carrier_is_not_a_use() {
    let held = "let mut v1: Result<i64, AlmdRec1> = f();\n// v2: the address of v1\n// v4: v1 + 4 bytes\nlet v5: i64 = v1.is_err() as i64;\n";
    assert_eq!(release_line(held, "v1").as_deref(), Some("drop(v1);"));
}
