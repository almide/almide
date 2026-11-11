//! A tail branch nested in a tail branch, as a chain of early returns.
//!
//! A function whose value is a branch of branches —
//!
//! ```text
//! match list.last(acc) {
//!   some(last) => if last.0 == t.0 then update(acc, …) else acc + [t],
//!   none => [t],
//! }
//! ```
//!
//! — gives `acc` away on two paths of the inner branch and lets it die on the
//! outer one. Taking `acc` over (`native_move_params`) then puts a consuming
//! event in an arm of an arm, and a certificate line holds flat arms only
//! (`CBranch`, proofs/OwnershipChecker.v): the takeover was refused, and the
//! borrowing body copied the whole accumulator on every step.
//!
//! The same control flow has a flat spelling. The code after a tail branch is
//! only the releases of the function's end, so an arm may run them itself and
//! leave (`Op::Return`, law 6 — `CBranchRet`), and the other arm then needs no
//! branch around it:
//!
//! ```text
//! if c { A } else { B }; T; ret join        if c { } else { B; T; return b }
//!                                      ≡    A; T; ret a
//! ```
//!
//! [`unnest`] applies that to the function's tail branch, then to the tail
//! branch the hoisted arm ends in, and so on: each level becomes a sibling
//! region with one leaving arm. Every path runs the ops it ran before, in the
//! same order — the leaving arm its own ops, the releases, the return of its
//! value; the staying arm its own ops and then the same releases — so nothing
//! observable changes, and the ownership mirror and the certificate judge the
//! result like any lowered body.
//!
//! Only a heap-valued tail is rewritten (a scalar tail may be wrapped at the
//! one return seam of `finish_native_fn`), the arm that leaves holds no
//! region of its own, and the tail after the branch is releases alone.

use crate::{MirFunction, Op, ValueId};

fn values(op: &Op) -> Vec<ValueId> {
    let mut out = Vec::new();
    crate::mir_ops::op_values(op, &mut out);
    out
}

/// A release of one value and nothing else.
fn release(op: &Op) -> bool {
    crate::render_native_move::bookkeeping(op)
        && !matches!(op, Op::Consume { .. } | Op::Borrow { .. } | Op::MakeUnique { .. })
        && values(op).len() == 1
}

/// `v` is an owned heap value its own op made: an allocation, a call's heap
/// result, a copy.
fn owned_heap(f: &MirFunction, v: ValueId) -> bool {
    f.ops.iter().any(|op| match op {
        Op::Alloc { dst, repr, .. } => *dst == v && repr.is_heap(),
        Op::CallFn { dst, result, .. } | Op::CallIndirect { dst, result, .. } => {
            *dst == Some(v) && result.is_some_and(|r| r.is_heap())
        }
        Op::Dup { dst, .. } => *dst == v,
        _ => false,
    })
}

/// The `IfThen` and `Else` of the region the `EndIf` at `end` closes, when
/// that region sits at the top level of the body.
fn top_region(ops: &[Op], end: usize) -> Option<(usize, usize)> {
    let mut depth = 0i32;
    let mut els = None;
    for k in (0..=end).rev() {
        match ops[k] {
            Op::EndIf { .. } => depth += 1,
            Op::Else { .. } if depth == 1 => els = Some(k),
            Op::IfThen { .. } => {
                depth -= 1;
                if depth == 0 {
                    let open = ops[..k].iter().fold(0i32, |d, op| match op {
                        Op::IfThen { .. } | Op::LoopStart => d + 1,
                        Op::EndIf { .. } | Op::LoopEnd => d - 1,
                        _ => d,
                    });
                    return (open == 0).then_some(k).zip(els);
                }
            }
            _ => {}
        }
    }
    None
}

/// An arm without the hand-over of its value `v` to the join (`Consume v`
/// as the last op naming it): the value is now returned itself.
fn without_handover(arm: &[Op], v: ValueId) -> Vec<Op> {
    let mut out = arm.to_vec();
    if let Some(at) = out.iter().rposition(|op| values(op).contains(&v)) {
        if matches!(out[at], Op::Consume { v: c } if c == v) {
            out.remove(at);
        }
    }
    out
}

/// One level: the function's tail branch with one arm leaving.
fn step(f: &MirFunction) -> Option<MirFunction> {
    let ret = f.ret?;
    let ops = &f.ops;
    let end = ops.iter().rposition(|op| !release(op))?;
    let Op::EndIf { val: Some(else_val) } = ops[end] else { return None };
    let (start, els) = top_region(ops, end)?;
    let (Op::IfThen { cond, dst: Some(join) }, Op::Else { val: Some(then_val) }) = (&ops[start], &ops[els]) else { return None };
    let tail = &ops[end + 1..];
    if *join != ret || tail.iter().any(|op| values(op).contains(join)) {
        return None;
    }
    let (then_arm, else_arm) = (&ops[start + 1..els], &ops[els + 1..end]);
    let regions = |arm: &[Op]| arm.iter().any(|op| matches!(op, Op::IfThen { .. } | Op::LoopStart | Op::Return { .. }));
    let then_leaves = match (regions(then_arm), regions(else_arm)) {
        (_, false) => false,
        (false, true) => true,
        (true, true) => return None,
    };
    let (leaving, leaving_val, staying, staying_val) =
        if then_leaves { (then_arm, *then_val, else_arm, else_val) } else { (else_arm, else_val, then_arm, *then_val) };
    if !owned_heap(f, leaving_val) {
        return None;
    }
    let mut out = ops[..start].to_vec();
    out.push(Op::IfThen { cond: *cond, dst: None });
    if !then_leaves {
        out.push(Op::Else { val: None });
    }
    out.extend(without_handover(leaving, leaving_val));
    out.extend(tail.iter().cloned());
    out.push(Op::Return { val: Some(leaving_val) });
    if then_leaves {
        out.push(Op::Else { val: None });
    }
    out.push(Op::EndIf { val: None });
    out.extend(without_handover(staying, staying_val));
    out.extend(tail.iter().cloned());
    Some(MirFunction { ops: out, ret: Some(staying_val), ..f.clone() })
}

/// `func` with its nested tail branches as sibling regions of early returns;
/// `None` when it has no tail branch of that shape.
pub(crate) fn unnest(func: &MirFunction) -> Option<MirFunction> {
    // A call-site move is recorded by op index, which this would shift.
    if !func.call_moves.is_empty() {
        return None;
    }
    let mut f = step(func)?;
    while let Some(next) = step(&f) {
        f = next;
    }
    crate::mir_wellformed::check_return_terminal(&f).is_ok().then_some(f)
}
