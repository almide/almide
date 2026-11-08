//! Whether a value is read again after an op, on every control path of the
//! MIR (#4006, no-perf-loss). The native render turns a copy (`Dup`) into a
//! move when the source is dead after it; a wrong move would leave a later
//! read looking at an emptied value, so the verdict is a liveness fixpoint
//! over the function's real control flow — branches, loop back-edges,
//! returns — never a guess from the op's shape. Anything this analysis does
//! not model keeps the copy.
//!
//! A read counts for the value itself and for every value that may refer to
//! its storage (its address, an address computed from one, an element or a
//! local bound to one). A rebind of the value (`SetLocal`) or its release
//! ends it; every other op naming it reads it.

use crate::{MirFunction, Op, ValueId};
use std::collections::BTreeSet;

/// The control successors of each op of `func`; `None` when the markers do
/// not nest.
fn successors(func: &MirFunction) -> Option<Vec<Vec<usize>>> {
    let n = func.ops.len();
    let mut succ: Vec<Vec<usize>> = (0..n).map(|i| if i + 1 < n { vec![i + 1] } else { Vec::new() }).collect();
    // (IfThen index, Else index) and LoopStart indices, open.
    let mut ifs: Vec<(usize, Option<usize>)> = Vec::new();
    let mut loops: Vec<(usize, Vec<usize>)> = Vec::new();
    // Each loop's (LoopStart, LoopEnd).
    let mut extents: Vec<(usize, usize)> = Vec::new();
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::IfThen { .. } => ifs.push((i, None)),
            Op::Else { .. } => ifs.last_mut()?.1 = Some(i),
            Op::EndIf { .. } => {
                let (start, els) = ifs.pop()?;
                match els {
                    // The then-arm's end jumps over the else-arm; the
                    // condition's false edge enters the else-arm.
                    Some(e) => {
                        succ[e] = vec![i];
                        succ[start].push(e + 1);
                    }
                    None => succ[start].push(i),
                }
            }
            Op::LoopStart => loops.push((i, Vec::new())),
            Op::LoopBreakUnless { .. } => loops.last_mut()?.1.push(i),
            Op::LoopEnd => {
                let (start, breaks) = loops.pop()?;
                extents.push((start, i));
                succ[i] = vec![start + 1];
                for b in breaks {
                    if i + 1 < n {
                        succ[b].push(i + 1);
                    }
                }
            }
            Op::Return { .. } => succ[i].clear(),
            _ => {}
        }
    }
    if !(ifs.is_empty() && loops.is_empty()) {
        return None;
    }
    // A raised exit flag leaves its loop: from it to the loop's end there is
    // only bookkeeping and the close of the arms it is in, and the loop's
    // first test then breaks (`render_native_addr::flag_exits`, the render's
    // own `break` there).
    for i in crate::render_native_addr::flag_exits(func) {
        let (_, end) = extents.iter().filter(|(s, e)| *s < i && i < *e).min_by_key(|(s, e)| e - s)?;
        succ[i] = if end + 1 < n { vec![end + 1] } else { Vec::new() };
    }
    Some(succ)
}

/// `src` and every value that may refer to its storage.
fn referring(func: &MirFunction, src: ValueId) -> BTreeSet<ValueId> {
    let mut set: BTreeSet<ValueId> = std::iter::once(src).collect();
    loop {
        let before = set.len();
        for op in &func.ops {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(op, &mut reads);
            if !reads.iter().any(|r| set.contains(r)) {
                continue;
            }
            match op {
                // A copy or a call result owns its own value.
                Op::Dup { .. } | Op::CallFn { .. } | Op::Call { .. } => {}
                Op::SetLocal { local, .. } => {
                    set.insert(*local);
                }
                _ => {
                    if let Some(d) = crate::mir_ops::defined_value(op) {
                        set.insert(d);
                    }
                }
            }
        }
        if set.len() == before {
            return set;
        }
    }
}

/// Whether `src` (or anything referring to its storage) may be read on some
/// path after `ops[at]`. `true` whenever the control flow is not modelled.
pub(crate) fn live_after(func: &MirFunction, at: usize, src: ValueId) -> bool {
    let Some(succ) = successors(func) else { return true };
    let refs = referring(func, src);
    let n = func.ops.len();
    let mut uses = vec![false; n];
    let mut kills = vec![false; n];
    for (i, op) in func.ops.iter().enumerate() {
        let mut vs = Vec::new();
        crate::mir_ops::op_values(op, &mut vs);
        let names_src = vs.contains(&src);
        let releases = crate::render_native_move::bookkeeping(op)
            && !matches!(op, Op::Consume { .. } | Op::Borrow { .. } | Op::MakeUnique { .. })
            && vs == [src];
        let mut reads = Vec::new();
        crate::mir_ops::op_reads(op, &mut reads);
        // A fresh definition of the value ends the one before it: a value
        // defined in a loop body (an `if`'s merge, a call's result) is a new
        // one each iteration, never read across the back edge.
        let redefines = crate::mir_ops::defined_value(op) == Some(src) && !matches!(op, Op::SetLocal { .. }) && !reads.contains(&src);
        if releases || redefines || matches!(op, Op::SetLocal { local, src: s } if *local == src && *s != src) {
            kills[i] = true;
            continue;
        }
        uses[i] = reads.iter().any(|r| refs.contains(r)) || (names_src && crate::mir_ops::defined_value(op) != Some(src));
    }
    // live_in[i] = uses[i] || (!kills[i] && any live_in[successor]).
    let mut live_in = uses.clone();
    loop {
        let mut changed = false;
        for i in (0..n).rev() {
            if live_in[i] || kills[i] {
                continue;
            }
            if succ[i].iter().any(|&s| live_in[s]) {
                live_in[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    succ[at].iter().any(|&s| live_in[s])
}

/// The copy at `ops[at]` may move `src` out: `src` is this function's own
/// value (not a parameter it borrows, not joined into a borrow, not the
/// return value) and no path after the copy reads it ([`live_after`]).
pub(crate) fn dead_after(
    func: &MirFunction,
    at: usize,
    src: ValueId,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) -> bool {
    if crate::render_native_move::drill_no_moves() || force_moves() {
        return force_moves();
    }
    let borrowed_param = func.params.iter().any(|p| p.value == src) && !func.owned_params.contains(&src);
    let joined = func.ops.iter().any(|op| matches!(op, Op::Dup { dst, src: s } if *s == src && borrow_arms.contains(dst)));
    !borrowed_param && !joined && func.ret != Some(src) && !live_after(func, at, src)
}

/// Negative control (`ALMIDE_NATIVE_DRILL_FORCE_MOVES=1`): every copy whose
/// source is not decided dead elsewhere is taken as a move — the wrong moves
/// the liveness verdict exists to refuse.
fn force_moves() -> bool {
    almide_base::env::flag("ALMIDE_NATIVE_DRILL_FORCE_MOVES")
}

/// The rebound values `ops[at]` reads (params and `SetLocal` targets) whose
/// Rust binding the render may move out as itself rather than through
/// `std::mem::take` (#4006, no-perf-loss): on every path after the op, the
/// binding is rebound before any op names it again, so the moved-from
/// binding is never seen, and the rebind writes into an empty slot instead of
/// dropping a `take`n default. A release only erases to scope end, where
/// Rust's drop flag covers a moved binding; the return value never moves.
/// Rust's own move check rejects any path this misjudges, so a wrong verdict
/// is a build failure, never a read of an emptied value.
pub(crate) fn reinit_reads(func: &MirFunction, at: usize) -> BTreeSet<ValueId> {
    let mut reads = Vec::new();
    crate::mir_ops::op_reads(&func.ops[at], &mut reads);
    let rebound: BTreeSet<ValueId> = func
        .params
        .iter()
        .map(|p| p.value)
        .chain(func.ops.iter().filter_map(|op| match op {
            Op::SetLocal { local, .. } => Some(*local),
            _ => None,
        }))
        .collect();
    let candidates: BTreeSet<ValueId> = reads.into_iter().filter(|v| rebound.contains(v) && func.ret != Some(*v)).collect();
    if candidates.is_empty() {
        return candidates;
    }
    let Some(succ) = successors(func) else { return BTreeSet::new() };
    candidates.into_iter().filter(|v| rebound_before_named(func, &succ, at, *v)).collect()
}

/// Every path from the successors of `ops[at]` rebinds `v` (or defines it
/// afresh) before an op names it or anything referring to its storage.
fn rebound_before_named(func: &MirFunction, succ: &[Vec<usize>], at: usize, v: ValueId) -> bool {
    let refs = referring(func, v);
    let mut seen = vec![false; func.ops.len()];
    let mut work: Vec<usize> = succ[at].clone();
    while let Some(i) = work.pop() {
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        let op = &func.ops[i];
        let mut vs = Vec::new();
        crate::mir_ops::op_values(op, &mut vs);
        // A release of `v` alone erases to scope end: it names nothing.
        if crate::render_native_move::bookkeeping(op)
            && !matches!(op, Op::Consume { .. } | Op::Borrow { .. } | Op::MakeUnique { .. })
            && vs == [v]
        {
            work.extend(succ[i].iter().copied());
            continue;
        }
        // A rebind writes the binding from a value not viewing it.
        match op {
            Op::SetLocal { local, src } if *local == v => {
                if refs.contains(src) {
                    return false;
                }
                continue;
            }
            _ if crate::mir_ops::defined_value(op) == Some(v) && !matches!(op, Op::SetLocal { .. }) => {
                let mut reads = Vec::new();
                crate::mir_ops::op_reads(op, &mut reads);
                if reads.iter().any(|r| refs.contains(r)) {
                    return false;
                }
                continue;
            }
            _ => {}
        }
        let mut reads = Vec::new();
        crate::mir_ops::op_reads(op, &mut reads);
        if vs.contains(&v) || reads.iter().any(|r| refs.contains(r)) {
            return false;
        }
        work.extend(succ[i].iter().copied());
    }
    true
}

/// The ops at `at` are the last uses of `p` on every path through `func`
/// (#4006, class P): each path from the entry to an exit passes exactly one
/// of them, and no path after one reads `p` again. A borrowed parameter
/// copied only there (one copy per branch arm) can then be taken over: the
/// copy on each path is its last use.
pub(crate) fn last_use_on_every_path(func: &MirFunction, p: ValueId, at: &[usize]) -> bool {
    let Some(succ) = successors(func) else { return false };
    if func.ops.is_empty() || at.iter().any(|&i| live_after(func, i, p)) {
        return false;
    }
    // No exit is reachable from the entry without passing one of `at`.
    let mut seen = vec![false; func.ops.len()];
    let mut work = vec![0usize];
    while let Some(i) = work.pop() {
        if std::mem::replace(&mut seen[i], true) || at.contains(&i) {
            continue;
        }
        if succ[i].is_empty() {
            return false;
        }
        work.extend(succ[i].iter().copied());
    }
    true
}
