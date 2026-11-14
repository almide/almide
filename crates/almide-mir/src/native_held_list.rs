//! A list held unchanged while an element read from it is lent (#4661, no
//! perf loss). `let a = list.get(xs, i) ?? value.null()` over a local list
//! `xs` copied the element (a whole Value tree per read) where v0 lends it;
//! the lent form (`Option<&A>`, `render_native_borrow_read::borrowed_options`)
//! needs the list alive and unwritten for as long as anything borrowed from
//! the element is read — what rustc checks of the render, so the conditions
//! here are the conservative side of it: a local qualifies only inside a
//! loop that reads it on every round, so the render never releases it early.

use crate::{CallArg, IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The list `v` the call at `at` reads into the Option `d` stays alive and
/// unwritten until the last read of anything borrowed from `d`: a param the
/// function borrows and never rebinds, or a local that only reads and whose
/// release comes after that last read.
pub(crate) fn held_while_lent(func: &MirFunction, v: ValueId, at: usize, d: ValueId, root: &BTreeMap<ValueId, ValueId>) -> bool {
    let rebinds = |x: ValueId| {
        func.ops.iter().any(|o| matches!(o, Op::SetLocal { local, .. } | Op::Consume { v: local } | Op::MakeUnique { v: local } if *local == x))
    };
    if func.params.iter().any(|p| p.value == v) {
        return !func.owned_params.contains(&v) && !rebinds(v);
    }
    let Some(def_at) = func.ops[..at].iter().position(|o| crate::mir_ops::defined_value(o) == Some(v)) else { return false };
    if rebinds(v) {
        return false;
    }
    let Some(end) = lent_extent(func, at, d) else { return false };
    // The render releases a local after its last read; a loop that reads it
    // on every round (this call) and opened after it keeps it alive to the
    // loop's end, where the lent reads must all have ended.
    let Some((_, loop_end)) = enclosing_loop(func, at).filter(|&(s, _)| s > def_at) else { return false };
    if end >= loop_end {
        return false;
    }
    let mut reads = Vec::new();
    func.ops.iter().enumerate().skip(at + 1).all(|(j, op)| {
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        let touches = reads.contains(&v) || reads.iter().any(|r| root.get(r) == Some(&v));
        if !touches {
            return true;
        }
        if crate::native_move_params::is_release(op, v) {
            return j > end;
        }
        j > end || only_reads(op, v, root)
    })
}

/// The last op index reading anything borrowed from the Option `d` defined at
/// `at`: its payload reads, their copies and addresses, and the joins they
/// feed — stretched to the end of every loop that opened after `at` and
/// reads one of them. `None` when a join feeds a value the scan cannot follow.
fn lent_extent(func: &MirFunction, at: usize, d: ValueId) -> Option<usize> {
    let mut taint: BTreeSet<ValueId> = BTreeSet::from([d]);
    let mut joins: Vec<Option<ValueId>> = Vec::new();
    let mut last = at;
    let mut reads = Vec::new();
    for (j, op) in func.ops.iter().enumerate().skip(at + 1) {
        if let Op::IfThen { dst, .. } = op {
            joins.push(*dst);
            continue;
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        let hit = reads.iter().any(|r| taint.contains(r));
        match op {
            Op::Else { val } | Op::EndIf { val } => {
                let join = if matches!(op, Op::EndIf { .. }) { joins.pop().flatten() } else { joins.last().copied().flatten() };
                if val.is_some_and(|x| taint.contains(&x)) {
                    taint.insert(join?);
                }
                continue;
            }
            _ => {}
        }
        if !hit || crate::render_native_move::bookkeeping(op) {
            continue;
        }
        last = j;
        if let Some(x) = crate::mir_ops::defined_value(op) {
            if matches!(op, Op::Dup { .. } | Op::Prim { kind: PrimKind::Handle | PrimKind::LoadHandle | PrimKind::ElemAddr, .. } | Op::IntBinOp { op: IntOp::Add, .. }) {
                taint.insert(x);
            }
        }
    }
    Some(stretch_to_loops(func, at, last))
}

/// `last`, or the end of the loop that opened after `at` and holds it: a
/// read inside such a loop runs again on the next round.
fn stretch_to_loops(func: &MirFunction, at: usize, last: usize) -> usize {
    let mut open = Vec::new();
    let mut stretched = last;
    for (j, op) in func.ops.iter().enumerate() {
        match op {
            Op::LoopStart => open.push(j),
            Op::LoopEnd => {
                if open.pop().is_some_and(|s| s > at && s < last && last < j) {
                    stretched = stretched.max(j);
                }
            }
            _ => {}
        }
    }
    stretched
}

/// The innermost `(LoopStart, LoopEnd)` around op `at`.
fn enclosing_loop(func: &MirFunction, at: usize) -> Option<(usize, usize)> {
    let mut open = Vec::new();
    let mut best: Option<(usize, usize)> = None;
    for (j, op) in func.ops.iter().enumerate() {
        match op {
            Op::LoopStart => open.push(j),
            Op::LoopEnd => {
                if let Some(s) = open.pop() {
                    if s < at && at < j && best.is_none_or(|(b, _)| s > b) {
                        best = Some((s, j));
                    }
                }
            }
            _ => {}
        }
    }
    best
}

/// `op` only reads the list `v`: lent to a call that does not take or write
/// it, its address, a load through it, a scalar slot read.
fn only_reads(op: &Op, v: ValueId, root: &BTreeMap<ValueId, ValueId>) -> bool {
    match op {
        Op::CallFn { name, args, .. } => args.iter().enumerate().all(|(k, a)| match a {
            CallArg::Handle(x) if *x == v => {
                !crate::native_hof::mutable_argument(name, k) && !crate::native_move_params::by_value(name, k)
            }
            CallArg::Scalar(x) => *x != v,
            _ => true,
        }),
        Op::Prim { kind: PrimKind::Handle, args, .. } => args.as_slice() == [v],
        Op::IntBinOp { op: IntOp::Add, .. } => true,
        Op::Prim { kind: PrimKind::ElemAddr | PrimKind::Load { .. } | PrimKind::LoadHandle, args, .. } => {
            args.first().is_some_and(|a| root.get(a) == Some(&v))
        }
        Op::ListGetScalar { list, idx, .. } => *list == v && *idx != v,
        _ => false,
    }
}
