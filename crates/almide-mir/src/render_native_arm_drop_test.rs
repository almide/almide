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
